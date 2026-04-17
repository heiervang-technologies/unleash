use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;
use std::io::BufRead;

/// Convert Codex JSONL rollout to Hub records.
pub fn to_hub<R: BufRead>(reader: R) -> Result<Vec<HubRecord>, ConvertError> {
    let mut records = Vec::new();
    let mut session_emitted = false;
    // If any line carries a `_ucf_hub.session` escape hatch, we replace the
    // synthesized codex session header with it at the end so cross-CLI round
    // trips preserve the original non-codex session identity.
    let mut carried_session: Option<SessionHeader> = None;
    // True once we've seen `_ucf_hub.session` — every subsequent line is then
    // treated as foreign-originated even without its own `_ucf_hub.ext`, so we
    // don't stash codex-native fidelity fields (`_original_payload`) that
    // weren't actually authored by codex.
    let mut foreign_session = false;

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
        let ucf_hub = val.get("_ucf_hub");
        let foreign_ext = ucf_hub
            .and_then(|u| u.get("ext"))
            .and_then(|e| e.as_object())
            .cloned();

        if !session_emitted {
            if let Some(sess_val) = ucf_hub.and_then(|u| u.get("session")) {
                carried_session = serde_json::from_value(sess_val.clone()).ok();
                foreign_session = true;
            }
        }
        // A line is foreign-originated if the whole session was foreign OR
        // this specific line carries a `_ucf_hub` passthrough.
        let foreign_originated = foreign_session || ucf_hub.is_some();

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
                let mut msg = response_item_to_hub(&payload, &timestamp)?;
                if foreign_originated {
                    // Synthesized codex extensions are not real claims about
                    // codex-origin content. Reset to just the foreign keys.
                    msg.extensions = Value::Object(serde_json::Map::new());
                }
                if let Some(ext) = &foreign_ext {
                    merge_into_extensions(&mut msg.extensions, ext);
                }
                if foreign_originated && ext_is_empty(&msg.extensions) {
                    msg.extensions = Value::Null;
                }
                records.push(HubRecord::Message(msg));
            }
            "event_msg" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                let sub_type = payload
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                let mut ext = serde_json::json!({"codex": {"_outer_type": "event_msg"}});
                if foreign_originated {
                    ext = Value::Object(serde_json::Map::new());
                }
                if let Some(foreign) = &foreign_ext {
                    merge_into_extensions(&mut ext, foreign);
                }
                if foreign_originated && ext_is_empty(&ext) {
                    ext = Value::Null;
                }
                let event_type_str = if sub_type == "token_count" {
                    "token_count".to_string()
                } else {
                    format!("codex_{sub_type}")
                };
                records.push(HubRecord::Event(HubEvent {
                    event_type: event_type_str,
                    timestamp: timestamp.clone(),
                    data: payload.clone(),
                    extensions: ext,
                }));
            }
            "turn_context" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                let mut ext = Value::Null;
                if let Some(foreign) = &foreign_ext {
                    ext = Value::Object(serde_json::Map::new());
                    merge_into_extensions(&mut ext, foreign);
                }
                records.push(HubRecord::Event(HubEvent {
                    event_type: "turn_context".to_string(),
                    timestamp: timestamp.clone(),
                    data: payload.clone(),
                    extensions: ext,
                }));
            }
            _ => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(&timestamp)));
                    session_emitted = true;
                }
                let mut ext = Value::Null;
                if let Some(foreign) = &foreign_ext {
                    ext = Value::Object(serde_json::Map::new());
                    merge_into_extensions(&mut ext, foreign);
                }
                records.push(HubRecord::Event(HubEvent {
                    event_type: format!("codex_{event_type}"),
                    timestamp,
                    data: payload,
                    extensions: ext,
                }));
            }
        }
    }

    // If a carried session header was provided via `_ucf_hub.session`, replace
    // the synthesized one so foreign-source sessions survive the round trip.
    if let Some(carried) = carried_session {
        if let Some(HubRecord::Session(ref mut session)) = records.first_mut() {
            *session = carried;
        }
    }

    Ok(records)
}

/// Convert Hub records back to Codex JSONL rollout.
///
/// When the hub session's `source_cli` is not `codex`, the first emitted line
/// carries a `_ucf_hub.session` field holding the full SessionHeader so the
/// hub → codex → hub round trip is lossless. Per-message foreign extensions
/// are stashed under `_ucf_hub.ext`.
pub fn from_hub(records: &[HubRecord]) -> Result<Vec<Value>, ConvertError> {
    let mut lines = Vec::new();
    let mut session_passthrough: Option<Value> = None;

    for record in records {
        match record {
            HubRecord::Session(s) => {
                // Stash the full session header whenever the source isn't
                // Codex — Codex JSONL can't represent fields like ucf_version,
                // title, slug, parent_session_id natively.
                if s.source_cli != "codex" {
                    session_passthrough = Some(serde_json::to_value(s)?);
                }
                let line = hub_session_to_codex(s)?;
                if !line.is_null() {
                    lines.push(line);
                }
            }
            HubRecord::Message(msg) => {
                let mut line = hub_message_to_codex(msg)?;
                if !line.is_null() {
                    if let Some(foreign) = foreign_extensions(&msg.extensions) {
                        attach_ucf_hub_ext(&mut line, foreign);
                    }
                    lines.push(line);
                }
            }
            HubRecord::Event(evt) => {
                let mut line = hub_event_to_codex(evt)?;
                if !line.is_null() {
                    if let Some(foreign) = foreign_extensions(&evt.extensions) {
                        attach_ucf_hub_ext(&mut line, foreign);
                    }
                    lines.push(line);
                }
            }
        }
    }

    // Attach the session passthrough to the first emitted line.
    if let (Some(sess), Some(first)) = (session_passthrough, lines.first_mut()) {
        attach_ucf_hub_session(first, sess);
    }

    Ok(lines)
}

// === _ucf_hub passthrough helpers ===

/// Returns true if `ext` is null or an empty object.
fn ext_is_empty(ext: &Value) -> bool {
    match ext {
        Value::Null => true,
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

/// Merge `ext` map into `extensions`, upgrading to an object if needed.
fn merge_into_extensions(extensions: &mut Value, ext: &serde_json::Map<String, Value>) {
    if let Some(obj) = extensions.as_object_mut() {
        for (k, v) in ext {
            obj.insert(k.clone(), v.clone());
        }
    } else {
        *extensions = Value::Object(ext.clone());
    }
}

/// Extract all hub extensions that are NOT codex (foreign to this format).
fn foreign_extensions(ext: &Value) -> Option<Value> {
    let obj = ext.as_object()?;
    let foreign: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "codex")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if foreign.is_empty() {
        None
    } else {
        Some(Value::Object(foreign))
    }
}

/// Merge `ext` into `line._ucf_hub.ext`, creating the nested objects as needed.
fn attach_ucf_hub_ext(line: &mut Value, ext: Value) {
    let Value::Object(ref mut obj) = line else {
        return;
    };
    let entry = obj
        .entry("_ucf_hub".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(ref mut inner) = entry else {
        return;
    };
    inner.insert("ext".to_string(), ext);
}

/// Attach a serialized SessionHeader to `line._ucf_hub.session`.
fn attach_ucf_hub_session(line: &mut Value, session: Value) {
    let Value::Object(ref mut obj) = line else {
        return;
    };
    let entry = obj
        .entry("_ucf_hub".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(ref mut inner) = entry else {
        return;
    };
    inner.insert("session".to_string(), session);
}

// === to_hub helpers ===

fn session_meta_to_hub(payload: &Value, timestamp: &str) -> SessionHeader {
    // Stash ALL payload fields that aren't mapped to hub session fields
    let hub_payload_fields: &[&str] = &["id", "timestamp", "cwd", "cli_version"];
    let mut ext = serde_json::Map::new();
    if let Some(obj) = payload.as_object() {
        for (k, v) in obj {
            if !hub_payload_fields.contains(&k.as_str()) {
                ext.insert(k.clone(), v.clone());
            }
        }
    }
    // Also stash the outer timestamp for exact reconstruction
    ext.insert("_outer_timestamp".into(), Value::String(timestamp.to_string()));

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
        extensions: if ext.is_empty() {
            Value::Null
        } else {
            serde_json::json!({"codex": ext})
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
            let hub_role = match role {
                "developer" => "system",
                "assistant" => "assistant",
                _ => "user",
            };
            (hub_role, extract_codex_content(payload)?)
        }
    };

    // Stash the ENTIRE original payload for lossless round-trip
    let ext = serde_json::json!({"codex": {
        "_original_payload": payload,
    }});

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
        extensions: ext,
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
        .unwrap_or("");

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
    });

    // Restore ALL Codex-specific fields from extensions
    if let Some(obj) = cc.as_object() {
        for (k, v) in obj {
            if k == "_outer_timestamp" {
                continue; // handled below
            }
            payload[k] = v.clone();
        }
    }

    // If no originator/source in extensions, set defaults
    if payload.get("originator").is_none() {
        payload["originator"] = Value::String("codex_cli_rs".into());
    }
    if payload.get("source").is_none() {
        payload["source"] = Value::String("cli".into());
    }

    // Use the original outer timestamp if available
    let outer_ts = cc
        .get("_outer_timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or(&session.updated_at);

    Ok(serde_json::json!({
        "timestamp": outer_ts,
        "type": "session_meta",
        "payload": payload,
    }))
}

fn hub_message_to_codex(msg: &HubMessage) -> Result<Value, ConvertError> {
    let cc = msg.extensions.get("codex").cloned().unwrap_or(Value::Null);

    // If we have the original payload stashed, use it for lossless round-trip
    if let Some(original) = cc.get("_original_payload") {
        return Ok(serde_json::json!({
            "timestamp": msg.timestamp,
            "type": "response_item",
            "payload": original,
        }));
    }

    // Cross-CLI path: reconstruct from hub content
    let cc_payload_type = cc
        .get("payload_type")
        .and_then(|v| v.as_str())
        .unwrap_or("message");

    match cc_payload_type {
        "function_call" => {
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
            } => Some(
                serde_json::json!({"type": "input_image", "media_type": media_type, "data": data}),
            ),
            ContentBlock::Thinking { text, .. } => {
                if !text.is_empty() {
                    Some(serde_json::json!({"type": "output_text", "text": text}))
                } else {
                    None
                }
            }
            ContentBlock::ToolUse {
                name, input, id, ..
            } => {
                let args = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                Some(serde_json::json!({"type": "output_text", "text": format!("[Tool call: {name}({args}) id={id}]")}))
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                let output = content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(serde_json::json!({"type": "input_text", "text": format!("[Tool result for {tool_use_id}]: {output}")}))
            }
            _ => None,
        })
        .collect();

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
    } else if !msg.id.is_empty() {
        // Foreign-originated message: preserve the hub message id so
        // round-tripping back to hub recovers it.
        payload["id"] = Value::String(msg.id.clone());
    }

    Ok(serde_json::json!({
        "timestamp": msg.timestamp,
        "type": "response_item",
        "payload": payload,
    }))
}

fn hub_event_to_codex(evt: &HubEvent) -> Result<Value, ConvertError> {
    // Skip events with null/empty data — these produce unparseable Codex JSONL lines
    if evt.data.is_null() {
        return Ok(Value::Null);
    }

    let cc = evt.extensions.get("codex").cloned().unwrap_or(Value::Null);

    // Check if we stored the original outer type
    let outer_type = cc
        .get("_outer_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let codex_type = if !outer_type.is_empty() {
        outer_type.to_string()
    } else if evt.event_type == "turn_context" {
        "turn_context".to_string()
    } else if evt.event_type == "token_count" {
        "event_msg".to_string()
    } else if evt.event_type.starts_with("codex_") {
        "event_msg".to_string()
    } else {
        "event_msg".to_string()
    };

    Ok(serde_json::json!({
        "timestamp": evt.timestamp,
        "type": codex_type,
        "payload": evt.data,
    }))
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
    fn test_event_msg_all_preserved() {
        // All event_msg types are preserved for lossless round-trip
        let input = r#"{"timestamp":"2026-03-29T16:21:00Z","type":"event_msg","payload":{"type":"agent_message","message":"I can help.","phase":null}}
{"timestamp":"2026-03-29T16:21:01Z","type":"event_msg","payload":{"type":"user_message","message":"thanks"}}
{"timestamp":"2026-03-29T16:21:02Z","type":"event_msg","payload":{"type":"token_count","input_tokens":100,"output_tokens":50}}"#;

        let reader = std::io::BufReader::new(input.as_bytes());
        let hub = to_hub(reader).unwrap();
        // All 3 event_msg lines should be preserved as events
        let events: Vec<_> = hub
            .iter()
            .filter(|r| matches!(r, HubRecord::Event(_)))
            .collect();
        assert_eq!(events.len(), 3, "all event_msg types should be preserved");
    }

    #[test]
    fn test_developer_role_preserved() {
        // Developer messages are preserved for lossless round-trip
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
        // All 3 messages preserved (developer mapped to system role)
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[2].role, "assistant");
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
