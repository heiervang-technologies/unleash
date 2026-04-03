use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;
use std::io::BufRead;

/// Convert Codex JSONL rollout to Hub records.
pub fn to_hub<R: BufRead>(reader: R) -> Result<Vec<HubRecord>, ConvertError> {
    let mut records = Vec::new();
    let mut session_emitted = false;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let val: Value = serde_json::from_str(&line)?;

        let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let timestamp = val
            .get("timestamp")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let payload = val.get("payload").cloned().unwrap_or(Value::Null);

        match event_type {
            "session_meta" => {
                records.push(HubRecord::Session(session_meta_to_hub(
                    &payload, &timestamp,
                )));
                session_emitted = true;
            }
            "response_item" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                // Skip developer role (system preambles) — not useful for crossload
                let role = payload
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user");
                if role == "developer" {
                    continue;
                }
                let msg = response_item_to_hub(&payload, &timestamp)?;
                // Skip messages with no content or only empty text
                let has_content = msg.content.iter().any(|b| match b {
                    ContentBlock::Text { text } => !text.trim().is_empty(),
                    _ => true,
                });
                if has_content {
                    records.push(HubRecord::Message(msg));
                }
            }
            "event_msg" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                let sub_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match sub_type {
                    // Skip user_message/agent_message — these duplicate response_item
                    // entries with the same content and timestamp. response_item has
                    // richer structured data (content blocks, item IDs, roles).
                    "user_message" | "agent_message" => {}
                    "token_count" => {
                        records.push(HubRecord::Event(HubEvent {
                            event_type: "token_count".to_string(),
                            timestamp: timestamp.clone(),
                            data: payload.clone(),
                            extensions: Value::Null,
                        }));
                    }
                    _ => {
                        records.push(HubRecord::Event(HubEvent {
                            event_type: format!("codex_{sub_type}"),
                            timestamp: timestamp.clone(),
                            data: payload.clone(),
                            extensions: Value::Null,
                        }));
                    }
                }
            }
            "turn_context" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                records.push(HubRecord::Event(HubEvent {
                    event_type: "turn_context".to_string(),
                    timestamp: timestamp.clone(),
                    data: payload.clone(),
                    extensions: Value::Null,
                }));
            }
            _ => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                records.push(HubRecord::Event(HubEvent {
                    event_type: format!("codex_{event_type}"),
                    timestamp,
                    data: payload,
                    extensions: Value::Null,
                }));
            }
        }
    }

    Ok(records)
}

/// Convert Hub records back to Codex JSONL rollout.
pub fn from_hub(records: &[HubRecord]) -> Result<Vec<Value>, ConvertError> {
    let mut lines = Vec::new();

    for record in records {
        let line = match record {
            HubRecord::Session(s) => hub_session_to_codex(s)?,
            HubRecord::Message(msg) => hub_message_to_codex(msg)?,
            HubRecord::Event(evt) => hub_event_to_codex(evt)?,
        };
        // Skip null entries (events/messages that couldn't be converted)
        if !line.is_null() {
            lines.push(line);
        }
    }

    Ok(lines)
}

// === to_hub helpers ===

fn session_meta_to_hub(payload: &Value, timestamp: &str) -> SessionHeader {
    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id: payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        created_at: payload
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or(timestamp)
            .to_string(),
        updated_at: timestamp.to_string(),
        source_cli: "codex".to_string(),
        source_version: payload
            .get("cli_version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        project: Some(ProjectInfo {
            directory: payload
                .get("cwd")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            root: None,
            hash: None,
            vcs: None,
            branch: None,
            sha: None,
            origin_url: None,
        }),
        model: None,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions: {
            let mut ext = serde_json::Map::new();
            if let Some(v) = payload.get("originator") {
                ext.insert("originator".into(), v.clone());
            }
            if let Some(v) = payload.get("source") {
                ext.insert("source".into(), v.clone());
            }
            if let Some(v) = payload.get("model_provider") {
                ext.insert("model_provider".into(), v.clone());
            }
            if let Some(v) = payload.get("base_instructions") {
                ext.insert("base_instructions".into(), v.clone());
            }
            if ext.is_empty() {
                Value::Null
            } else {
                serde_json::json!({"codex": ext})
            }
        },
    }
}

fn default_session(timestamp: &str) -> SessionHeader {
    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id: "unknown".to_string(),
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
        source_cli: "codex".to_string(),
        source_version: String::new(),
        project: None,
        model: None,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions: Value::Null,
    }
}

fn response_item_to_hub(payload: &Value, timestamp: &str) -> Result<HubMessage, ConvertError> {
    let role = payload.get("role").and_then(|r| r.as_str()).unwrap_or("");
    let payload_type = payload
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("message");

    // Determine hub role and content based on payload type
    let (hub_role, content) = match payload_type {
        "reasoning" => {
            // Reasoning items → assistant thinking blocks
            let text = payload
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|b| b.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            (
                "assistant",
                vec![ContentBlock::Thinking {
                    text,
                    subject: None,
                    description: None,
                    signature: None,
                    encrypted: false,
                    encryption_format: None,
                    encrypted_data: None,
                    timestamp: None,
                }],
            )
        }
        "function_call" => {
            // Function calls → assistant tool_use
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("unknown")
                .to_string();
            let call_id = payload
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = payload
                .get("arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("{}")
                .to_string();
            let input: Value =
                serde_json::from_str(&arguments).unwrap_or(Value::Object(Default::default()));
            (
                "assistant",
                vec![ContentBlock::ToolUse {
                    id: call_id,
                    name,
                    display_name: None,
                    description: None,
                    input,
                }],
            )
        }
        "function_call_output" => {
            // Function call output → user tool_result
            let call_id = payload
                .get("call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let output = payload
                .get("output")
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string();
            (
                "user",
                vec![ContentBlock::ToolResult {
                    tool_use_id: call_id,
                    content: vec![ContentBlock::Text { text: output }],
                    is_error: false,
                    exit_code: None,
                    interrupted: false,
                    status: None,
                    duration_ms: None,
                    title: None,
                    truncated: false,
                }],
            )
        }
        _ => {
            // Regular message — use role from payload
            let hub_role = match role {
                "developer" => "system",
                "assistant" => "assistant",
                _ => "user",
            };
            (hub_role, extract_codex_content(payload)?)
        }
    };

    // Codex-specific extensions
    let mut ext = serde_json::Map::new();
    ext.insert("original_role".into(), Value::String(role.to_string()));
    if let Some(v) = payload.get("id") {
        ext.insert("item_id".into(), v.clone());
    }
    ext.insert(
        "payload_type".into(),
        Value::String(payload_type.to_string()),
    );

    Ok(HubMessage {
        id: payload
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        api_message_id: None,
        parent_id: None,
        timestamp: timestamp.to_string(),
        completed_at: None,
        role: hub_role.to_string(),
        content,
        metadata: MessageMetadata {
            cwd: None,
            ..Default::default()
        },
        extensions: serde_json::json!({"codex": ext}),
    })
}

#[allow(dead_code)] // Used by from_hub round-trip path
fn event_msg_to_hub(
    payload: &Value,
    timestamp: &str,
    sub_type: &str,
) -> Result<HubMessage, ConvertError> {
    let role = if sub_type == "agent_message" {
        "assistant"
    } else {
        "user"
    };

    let text = payload
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    let mut ext = serde_json::Map::new();
    ext.insert("event_msg_type".into(), Value::String(sub_type.to_string()));
    if let Some(v) = payload.get("phase") {
        ext.insert("phase".into(), v.clone());
    }
    if let Some(v) = payload.get("memory_citation") {
        ext.insert("memory_citation".into(), v.clone());
    }
    if sub_type == "user_message" {
        if let Some(v) = payload.get("images") {
            ext.insert("images".into(), v.clone());
        }
    }

    Ok(HubMessage {
        id: String::new(),
        api_message_id: None,
        parent_id: None,
        timestamp: timestamp.to_string(),
        completed_at: None,
        role: role.to_string(),
        content: vec![ContentBlock::Text { text }],
        metadata: Default::default(),
        extensions: serde_json::json!({"codex": ext}),
    })
}

fn extract_codex_content(payload: &Value) -> Result<Vec<ContentBlock>, ConvertError> {
    let content_arr = payload.get("content").and_then(|c| c.as_array());

    match content_arr {
        Some(arr) => arr
            .iter()
            .map(|block| {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "input_text" | "output_text" => Ok(ContentBlock::Text {
                        text: block
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }),
                    "input_image" => Ok(ContentBlock::Image {
                        media_type: block
                            .get("media_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("image/png")
                            .to_string(),
                        encoding: "base64".to_string(),
                        data: block
                            .get("data")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        source_url: block.get("url").and_then(|v| v.as_str()).map(String::from),
                    }),
                    _ => Ok(ContentBlock::Text {
                        text: block
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }),
                }
            })
            .collect(),
        None => Ok(vec![]),
    }
}

// === from_hub helpers ===

fn hub_session_to_codex(session: &SessionHeader) -> Result<Value, ConvertError> {
    let cc = session
        .extensions
        .get("codex")
        .cloned()
        .unwrap_or(Value::Null);

    // Use current working directory if session has no project directory
    let cwd = session
        .project
        .as_ref()
        .map(|p| p.directory.as_str())
        .filter(|d| !d.is_empty())
        .unwrap_or({
            // Will be overridden by inject_into_codex with actual cwd
            ""
        });

    let mut payload = serde_json::json!({
        "id": session.session_id,
        "timestamp": if session.created_at.is_empty() {
            &session.updated_at
        } else {
            &session.created_at
        },
        "cwd": cwd,
        "cli_version": if session.source_version.is_empty() {
            "0.0.0"
        } else {
            &session.source_version
        },
        // Required fields: originator and source must always be present
        "originator": "codex_cli_rs",
        "source": "cli",
    });

    // Restore Codex-specific fields (override defaults if present)
    for key in &[
        "originator",
        "source",
        "model_provider",
        "base_instructions",
    ] {
        if let Some(v) = cc.get(*key) {
            payload[*key] = v.clone();
        }
    }

    Ok(serde_json::json!({
        "timestamp": session.updated_at,
        "type": "session_meta",
        "payload": payload,
    }))
}

fn hub_message_to_codex(msg: &HubMessage) -> Result<Value, ConvertError> {
    let cc = msg.extensions.get("codex").cloned().unwrap_or(Value::Null);

    // Check if this was an event_msg or response_item
    let is_event_msg = cc.get("event_msg_type").is_some();

    if is_event_msg {
        let sub_type = cc
            .get("event_msg_type")
            .and_then(|v| v.as_str())
            .unwrap_or("user_message");

        let text = msg
            .content
            .first()
            .and_then(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or("");

        let mut payload = serde_json::json!({
            "type": sub_type,
            "message": text,
        });
        for key in &["phase", "memory_citation", "images"] {
            if let Some(v) = cc.get(*key) {
                payload[*key] = v.clone();
            }
        }

        Ok(serde_json::json!({
            "timestamp": msg.timestamp,
            "type": "event_msg",
            "payload": payload,
        }))
    } else {
        // Check if this message has Codex-specific payload_type for structured items
        let payload_type = cc
            .get("payload_type")
            .and_then(|v| v.as_str())
            .unwrap_or("message");

        // Handle structured types (function_call, function_call_output, reasoning)
        // that were round-tripped through hub format
        match payload_type {
            "function_call" => {
                // Reconstruct function_call from ToolUse content block
                if let Some(ContentBlock::ToolUse {
                    id, name, input, ..
                }) = msg.content.first()
                {
                    let arguments =
                        serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                    let payload = serde_json::json!({
                        "type": "function_call",
                        "name": name,
                        "call_id": id,
                        "arguments": arguments,
                    });
                    return Ok(serde_json::json!({
                        "timestamp": msg.timestamp,
                        "type": "response_item",
                        "payload": payload,
                    }));
                }
            }
            "function_call_output" => {
                // Reconstruct function_call_output from ToolResult content block
                if let Some(ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                }) = msg.content.first()
                {
                    let output = content
                        .first()
                        .and_then(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .unwrap_or("");
                    let payload = serde_json::json!({
                        "type": "function_call_output",
                        "call_id": tool_use_id,
                        "output": output,
                    });
                    return Ok(serde_json::json!({
                        "timestamp": msg.timestamp,
                        "type": "response_item",
                        "payload": payload,
                    }));
                }
            }
            "reasoning" => {
                // Reconstruct reasoning from Thinking content block
                if let Some(ContentBlock::Thinking { text, .. }) = msg.content.first() {
                    let payload = serde_json::json!({
                        "type": "reasoning",
                        "content": [{"type": "text", "text": text}],
                    });
                    return Ok(serde_json::json!({
                        "timestamp": msg.timestamp,
                        "type": "response_item",
                        "payload": payload,
                    }));
                }
            }
            _ => {}
        }

        // Default: response_item with message content
        let role = cc
            .get("original_role")
            .and_then(|v| v.as_str())
            .unwrap_or(&msg.role);

        let content: Vec<Value> = msg
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => {
                    if msg.role == "assistant" {
                        Some(serde_json::json!({"type": "output_text", "text": text}))
                    } else {
                        Some(serde_json::json!({"type": "input_text", "text": text}))
                    }
                }
                ContentBlock::Image {
                    media_type, data, ..
                } => {
                    Some(serde_json::json!({"type": "input_image", "media_type": media_type, "data": data}))
                }
                ContentBlock::Thinking { text, .. } => {
                    // Convert thinking blocks to output_text for non-reasoning payload types
                    if !text.is_empty() {
                        Some(serde_json::json!({"type": "output_text", "text": text}))
                    } else {
                        None
                    }
                }
                ContentBlock::ToolUse { name, input, id, .. } => {
                    // Emit as a separate function_call — but since we're in content array,
                    // convert to descriptive text
                    let args = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                    Some(serde_json::json!({"type": "output_text", "text": format!("[Tool call: {name}({args}) id={id}]")}))
                }
                ContentBlock::ToolResult { tool_use_id, content, .. } => {
                    let output = content.iter().filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    }).collect::<Vec<_>>().join("\n");
                    Some(serde_json::json!({"type": "input_text", "text": format!("[Tool result for {tool_use_id}]: {output}")}))
                }
                _ => None,
            })
            .collect();

        // Skip messages with empty content
        if content.is_empty() {
            return Ok(Value::Null);
        }

        let mut payload = serde_json::json!({
            "role": role,
            "content": content,
            "type": "message",
        });
        if let Some(v) = cc.get("item_id") {
            payload["id"] = v.clone();
        }

        Ok(serde_json::json!({
            "timestamp": msg.timestamp,
            "type": "response_item",
            "payload": payload,
        }))
    }
}

fn hub_event_to_codex(evt: &HubEvent) -> Result<Value, ConvertError> {
    // Skip events with null/empty data — these produce unparseable Codex JSONL lines
    if evt.data.is_null() {
        return Ok(Value::Null);
    }

    let codex_type = if evt.event_type.starts_with("codex_") {
        evt.event_type
            .strip_prefix("codex_")
            .unwrap_or(&evt.event_type)
    } else {
        &evt.event_type
    };

    match codex_type {
        "turn_context" => Ok(serde_json::json!({
            "timestamp": evt.timestamp,
            "type": "turn_context",
            "payload": evt.data,
        })),
        "token_count" => Ok(serde_json::json!({
            "timestamp": evt.timestamp,
            "type": "event_msg",
            "payload": evt.data,
        })),
        _ => Ok(serde_json::json!({
            "timestamp": evt.timestamp,
            "type": "event_msg",
            "payload": evt.data,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::semantic_eq::semantic_eq;

    #[test]
    fn test_session_meta_round_trip() {
        let original = r#"{"timestamp":"2026-03-29T16:20:39.619Z","type":"session_meta","payload":{"id":"019d3a5a-7abf","timestamp":"2026-03-29T16:08:21.441Z","cwd":"/home/user/project","originator":"codex_cli_rs","cli_version":"0.117.0","source":"cli","model_provider":"local"}}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();

        assert_eq!(back.len(), 1);
        let orig_val: Value = serde_json::from_str(original).unwrap();
        semantic_eq(&orig_val, &back[0]).unwrap();
    }

    #[test]
    fn test_response_item_round_trip() {
        let original = r#"{"timestamp":"2026-03-29T16:20:39.620Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello world"}]}}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();

        // First line is session_meta, second is our message
        assert!(back.len() >= 2);
        let orig_val: Value = serde_json::from_str(original).unwrap();
        // The response_item is at index 1 (after session)
        semantic_eq(&orig_val, &back[1]).unwrap();
    }

    #[test]
    fn test_event_msg_user_agent_skipped() {
        // event_msg user_message/agent_message should be skipped (duplicates response_item)
        let input = r#"{"timestamp":"2026-03-29T16:21:00Z","type":"event_msg","payload":{"type":"agent_message","message":"I can help.","phase":null}}
{"timestamp":"2026-03-29T16:21:01Z","type":"event_msg","payload":{"type":"user_message","message":"thanks"}}
{"timestamp":"2026-03-29T16:21:02Z","type":"event_msg","payload":{"type":"token_count","input_tokens":100,"output_tokens":50}}"#;

        let reader = std::io::BufReader::new(input.as_bytes());
        let hub = to_hub(reader).unwrap();
        // Should have session + token_count event only (no messages from event_msg)
        let messages: Vec<_> = hub
            .iter()
            .filter(|r| matches!(r, HubRecord::Message(_)))
            .collect();
        assert_eq!(messages.len(), 0, "event_msg user/agent should be skipped");
        let events: Vec<_> = hub
            .iter()
            .filter(|r| matches!(r, HubRecord::Event(_)))
            .collect();
        assert_eq!(events.len(), 1, "token_count event should be kept");
    }

    #[test]
    fn test_developer_role_filtered() {
        let input = r#"{"timestamp":"2026-03-29T16:20:00Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"system preamble stuff"}]}}
{"timestamp":"2026-03-29T16:20:01Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}}
{"timestamp":"2026-03-29T16:20:02Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi there"}]}}"#;

        let reader = std::io::BufReader::new(input.as_bytes());
        let hub = to_hub(reader).unwrap();
        let messages: Vec<_> = hub
            .iter()
            .filter_map(|r| {
                if let HubRecord::Message(m) = r {
                    Some(m)
                } else {
                    None
                }
            })
            .collect();
        // developer message should be filtered out, only user + assistant remain
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn test_turn_context_round_trip() {
        let original = r#"{"timestamp":"2026-03-29T16:20:39.620Z","type":"turn_context","payload":{"turn_id":"019d3a65","cwd":"/home/user","approval_policy":"never","model":"qwen3.5-27b"}}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();

        let orig_val: Value = serde_json::from_str(original).unwrap();
        let ctx_line = back
            .iter()
            .find(|l| l.get("type").and_then(|t| t.as_str()) == Some("turn_context"))
            .unwrap();
        semantic_eq(&orig_val, ctx_line).unwrap();
    }
}
