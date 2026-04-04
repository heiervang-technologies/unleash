use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;
use std::io::BufRead;

/// Convert Claude Code JSONL to Hub records.
pub fn to_hub<R: BufRead>(reader: R) -> Result<Vec<HubRecord>, ConvertError> {
    let mut records = Vec::new();
    let mut session_emitted = false;
    let mut last_timestamp = String::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let val: Value = serde_json::from_str(&line)?;

        if !session_emitted {
            records.push(HubRecord::Session(build_session_header(&val)));
            session_emitted = true;
        }

        let ts = str_field(&val, "timestamp");
        if !ts.is_empty() {
            last_timestamp = ts;
        }

        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match msg_type {
            "user" | "assistant" => {
                records.push(HubRecord::Message(message_to_hub(&val, msg_type)?));
            }
            _ => {
                records.push(HubRecord::Event(event_to_hub(&val, msg_type)?));
            }
        }
    }

    // Patch session updated_at
    if let Some(HubRecord::Session(ref mut session)) = records.first_mut() {
        session.updated_at = last_timestamp;
    }

    Ok(records)
}

/// Convert Hub records back to Claude Code JSONL values.
/// Reconstructs from universal fields + minimal extensions. No _original fallback.
pub fn from_hub(records: &[HubRecord]) -> Result<Vec<Value>, ConvertError> {
    let mut lines = Vec::new();
    let mut session_id = String::new();
    let mut version = String::new();

    for record in records {
        match record {
            HubRecord::Session(s) => {
                session_id = s.session_id.clone();
                version = s.source_version.clone();
            }
            HubRecord::Message(msg) => {
                lines.push(hub_message_to_claude(msg, &session_id, &version)?);
            }
            HubRecord::Event(evt) => {
                lines.push(hub_event_to_claude(evt, &session_id, &version)?);
            }
        }
    }

    Ok(lines)
}

// === Helpers ===

use super::helpers::{opt_str, str_field};

fn build_session_header(val: &Value) -> SessionHeader {
    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id: str_field(val, "sessionId"),
        created_at: str_field(val, "timestamp"),
        updated_at: String::new(),
        source_cli: "claude-code".to_string(),
        source_version: str_field(val, "version"),
        project: Some(ProjectInfo {
            directory: str_field(val, "cwd"),
            root: None,
            hash: None,
            vcs: Some("git".to_string()),
            branch: opt_str(val, "gitBranch"),
            sha: None,
            origin_url: None,
        }),
        model: None,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions: serde_json::json!({}),
    }
}

// --- to_hub direction ---

fn message_to_hub(val: &Value, msg_type: &str) -> Result<HubMessage, ConvertError> {
    let message = val.get("message");
    let role = if msg_type == "assistant" {
        "assistant"
    } else {
        message
            .and_then(|m| m.get("role"))
            .and_then(|r| r.as_str())
            .unwrap_or("user")
    };

    let content = extract_content_blocks(val, msg_type)?;
    let metadata = extract_metadata(val, msg_type);

    // Minimal extensions: only Claude-specific fields not in universal schema
    let ext = build_claude_extensions(val, msg_type);

    Ok(HubMessage {
        id: str_field(val, "uuid"),
        api_message_id: message.and_then(|m| opt_str(m, "id")),
        parent_id: opt_str(val, "parentUuid"),
        timestamp: str_field(val, "timestamp"),
        completed_at: None,
        role: role.to_string(),
        content,
        metadata,
        extensions: ext,
    })
}

fn build_claude_extensions(val: &Value, msg_type: &str) -> Value {
    // Stash ALL top-level fields that don't map to hub message schema
    // into extensions for lossless round-trip
    let hub_fields: &[&str] = &[
        "type",      // → role / message type
        "message",   // → content blocks
        "uuid",      // → id
        "timestamp", // → timestamp
        "parentUuid", // → parent_id
        "cwd",       // → metadata.cwd
        "gitBranch", // → metadata.git_branch
    ];

    let mut ext = serde_json::Map::new();
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if !hub_fields.contains(&k.as_str()) {
                ext.insert(k.clone(), v.clone());
            }
        }
    }

    // Collect per-content-block extra fields not in hub schema
    if let Some(content_arr) = val
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        let mut block_extras = Vec::new();
        for block in content_arr {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let hub_keys: &[&str] = match block_type {
                "tool_use" => &["type", "id", "name", "input"],
                "tool_result" => &["type", "tool_use_id", "content", "is_error"],
                "text" => &["type", "text"],
                "thinking" => &["type", "thinking", "signature"],
                "image" => &["type", "source"],
                _ => &["type"],
            };
            let mut extras = serde_json::Map::new();
            if let Some(obj) = block.as_object() {
                for (k, v) in obj {
                    if !hub_keys.contains(&k.as_str()) {
                        extras.insert(k.clone(), v.clone());
                    }
                }
            }
            if extras.is_empty() {
                block_extras.push(Value::Null);
            } else {
                block_extras.push(Value::Object(extras));
            }
        }
        // Only store if any block had extras
        if block_extras.iter().any(|e| !e.is_null()) {
            ext.insert("content_extras".into(), Value::Array(block_extras));
        }
    }

    if msg_type == "assistant" {
        // Usage details not in universal tokens (service_tier, inference_geo, speed, cache breakdown)
        if let Some(usage) = val.get("message").and_then(|m| m.get("usage")) {
            let mut usage_ext = serde_json::Map::new();
            for key in &[
                "service_tier",
                "inference_geo",
                "speed",
                "cache_creation",
                "server_tool_use",
                "iterations",
            ] {
                if let Some(v) = usage.get(*key) {
                    usage_ext.insert(key.to_string(), v.clone());
                }
            }
            if !usage_ext.is_empty() {
                ext.insert("usage_extra".into(), Value::Object(usage_ext));
            }
        }
        // stop_sequence
        if let Some(v) = val.get("message").and_then(|m| m.get("stop_sequence")) {
            if !v.is_null() {
                ext.insert("stop_sequence".into(), v.clone());
            }
        }
        // message.type (always "message" but preserve)
        if let Some(v) = val.get("message").and_then(|m| m.get("type")) {
            ext.insert("message_type".into(), v.clone());
        }
    }

    if ext.is_empty() {
        Value::Null
    } else {
        serde_json::json!({"claude-code": ext})
    }
}

fn extract_content_blocks(val: &Value, msg_type: &str) -> Result<Vec<ContentBlock>, ConvertError> {
    let message = val.get("message");

    if msg_type == "assistant" {
        let content_arr = message
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array());
        match content_arr {
            Some(arr) => arr.iter().map(claude_content_to_hub).collect(),
            None => Ok(vec![]),
        }
    } else {
        let content = message.and_then(|m| m.get("content"));
        match content {
            Some(Value::String(s)) => Ok(vec![ContentBlock::Text { text: s.clone() }]),
            Some(Value::Array(arr)) => arr.iter().map(claude_content_to_hub).collect(),
            _ => Ok(vec![]),
        }
    }
}

fn claude_content_to_hub(block: &Value) -> Result<ContentBlock, ConvertError> {
    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match block_type {
        "text" => Ok(ContentBlock::Text {
            text: block
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "tool_use" => Ok(ContentBlock::ToolUse {
            id: str_field(block, "id"),
            name: str_field(block, "name"),
            display_name: None,
            description: None,
            input: block.get("input").cloned().unwrap_or(Value::Null),
        }),
        "tool_result" => {
            let content = match block.get("content") {
                Some(Value::String(s)) => vec![ContentBlock::Text { text: s.clone() }],
                Some(Value::Array(arr)) => {
                    arr.iter()
                        .map(claude_content_to_hub)
                        .collect::<Result<Vec<_>, _>>()?
                }
                _ => vec![],
            };
            Ok(ContentBlock::ToolResult {
                tool_use_id: str_field(block, "tool_use_id"),
                content,
                exit_code: None,
                is_error: block
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                interrupted: false,
                status: None,
                duration_ms: None,
                title: None,
                truncated: false,
            })
        }
        "thinking" => Ok(ContentBlock::Thinking {
            text: block
                .get("thinking")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string(),
            subject: None,
            description: None,
            signature: opt_str(block, "signature"),
            encrypted: false,
            encryption_format: None,
            encrypted_data: None,
            timestamp: None,
        }),
        "image" => Ok(ContentBlock::Image {
            media_type: block
                .get("source")
                .and_then(|s| s.get("media_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("image/png")
                .to_string(),
            encoding: "base64".to_string(),
            data: block
                .get("source")
                .and_then(|s| s.get("data"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source_url: None,
        }),
        _ => Ok(ContentBlock::Text {
            text: format!("[Unknown block type: {block_type}]"),
        }),
    }
}

fn extract_metadata(val: &Value, msg_type: &str) -> MessageMetadata {
    if msg_type == "assistant" {
        let message = val.get("message");
        let usage = message.and_then(|m| m.get("usage"));
        MessageMetadata {
            model: message.and_then(|m| opt_str(m, "model")),
            tokens: usage.map(|u| TokenUsage {
                input: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                output: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                cache_creation: u
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                cache_read: u
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                reasoning: 0,
                tool: 0,
                total: 0,
            }),
            stop_reason: message.and_then(|m| opt_str(m, "stop_reason")),
            cwd: opt_str(val, "cwd"),
            git_branch: opt_str(val, "gitBranch"),
            ..Default::default()
        }
    } else {
        MessageMetadata {
            cwd: opt_str(val, "cwd"),
            git_branch: opt_str(val, "gitBranch"),
            ..Default::default()
        }
    }
}

fn event_to_hub(val: &Value, msg_type: &str) -> Result<HubEvent, ConvertError> {
    // Stash ALL fields except the hub event schema fields (type, timestamp, data)
    // into extensions for lossless round-trip
    let hub_fields: &[&str] = &["type", "timestamp", "data"];
    let mut ext = serde_json::Map::new();
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if !hub_fields.contains(&k.as_str()) && !v.is_null() {
                ext.insert(k.clone(), v.clone());
            }
        }
    }

    let data = val.get("data").cloned().unwrap_or(Value::Null);

    Ok(HubEvent {
        event_type: msg_type.to_string(),
        timestamp: str_field(val, "timestamp"),
        data,
        extensions: if ext.is_empty() {
            Value::Null
        } else {
            serde_json::json!({"claude-code": ext})
        },
    })
}

// --- from_hub direction ---

fn hub_message_to_claude(
    msg: &HubMessage,
    session_id: &str,
    version: &str,
) -> Result<Value, ConvertError> {
    let cc = msg
        .extensions
        .get("claude-code")
        .cloned()
        .unwrap_or(Value::Null);

    // Build content array
    let mut content = hub_content_to_claude(&msg.content);

    // Merge per-block extras back from extensions
    if let Some(extras_arr) = cc.get("content_extras").and_then(|v| v.as_array()) {
        for (i, extras) in extras_arr.iter().enumerate() {
            if let (Some(block), Some(obj)) = (content.get_mut(i), extras.as_object()) {
                for (k, v) in obj {
                    block[k] = v.clone();
                }
            }
        }
    }

    let message = if msg.role == "assistant" {
        let mut m = serde_json::json!({
            "role": "assistant",
            "content": content,
        });
        if let Some(ref api_id) = msg.api_message_id {
            m["id"] = Value::String(api_id.clone());
        }
        if let Some(ref model) = msg.metadata.model {
            m["model"] = Value::String(model.clone());
        }
        // Reconstruct message.type
        if let Some(mt) = cc.get("message_type") {
            m["type"] = mt.clone();
        } else {
            m["type"] = Value::String("message".into());
        }
        if let Some(ref tokens) = msg.metadata.tokens {
            let mut usage = serde_json::json!({
                "input_tokens": tokens.input,
                "output_tokens": tokens.output,
                "cache_creation_input_tokens": tokens.cache_creation,
                "cache_read_input_tokens": tokens.cache_read,
            });
            // Restore extra usage fields
            if let Some(extra) = cc.get("usage_extra") {
                if let Some(obj) = extra.as_object() {
                    for (k, v) in obj {
                        usage[k] = v.clone();
                    }
                }
            }
            m["usage"] = usage;
        }
        if let Some(ref stop) = msg.metadata.stop_reason {
            m["stop_reason"] = Value::String(stop.clone());
        }
        if let Some(ss) = cc.get("stop_sequence") {
            m["stop_sequence"] = ss.clone();
        } else {
            m["stop_sequence"] = Value::Null;
        }
        m
    } else {
        // User message: content is string or array
        let content_val = if content.len() == 1 {
            if let Some(text) = content[0].get("text") {
                if content[0].get("type").and_then(|t| t.as_str()) == Some("text") {
                    // Simple text message
                    text.clone()
                } else {
                    Value::Array(content)
                }
            } else {
                Value::Array(content)
            }
        } else if content.is_empty() {
            Value::String(String::new())
        } else {
            Value::Array(content)
        };
        serde_json::json!({"role": "user", "content": content_val})
    };

    let mut line = serde_json::json!({
        "type": msg.role,
        "message": message,
        "uuid": msg.id,
        "timestamp": msg.timestamp,
    });

    if let Some(ref parent) = msg.parent_id {
        line["parentUuid"] = Value::String(parent.clone());
    } else {
        line["parentUuid"] = Value::Null;
    }

    // Restore all Claude-specific fields from extensions
    if let Some(obj) = cc.as_object() {
        for (k, v) in obj {
            // Skip internal fields that are handled specially
            if k == "usage_extra"
                || k == "stop_sequence"
                || k == "message_type"
                || k == "content_extras"
            {
                continue;
            }
            line[k] = v.clone();
        }
    }

    // Fill in required Claude fields — from extensions or defaults
    // Only add fields that aren't already present (from extensions or original)
    let is_from_claude = cc.is_object() && !cc.as_object().unwrap().is_empty();

    if line.get("sessionId").is_none_or(|v| v.is_null()) && !session_id.is_empty() {
        line["sessionId"] = Value::String(session_id.to_string());
    }
    if line.get("version").is_none_or(|v| v.is_null()) && !version.is_empty() {
        line["version"] = Value::String(version.to_string());
    }

    // Only set cross-CLI defaults when source is NOT Claude
    if !is_from_claude {
        if line.get("isSidechain").is_none_or(|v| v.is_null()) {
            line["isSidechain"] = Value::Bool(false);
        }
        if line.get("userType").is_none_or(|v| v.is_null()) {
            line["userType"] = Value::String("external".into());
        }
        if line.get("permissionMode").is_none() {
            line["permissionMode"] = Value::String("bypassPermissions".into());
        }
        if line.get("promptId").is_none_or(|v| v.is_null()) {
            line["promptId"] = Value::String(msg.id.clone());
        }
    }

    // Restore universal fields
    if let Some(ref cwd) = msg.metadata.cwd {
        line["cwd"] = Value::String(cwd.clone());
    }
    if let Some(ref branch) = msg.metadata.git_branch {
        line["gitBranch"] = Value::String(branch.clone());
    }

    Ok(line)
}

fn hub_content_to_claude(blocks: &[ContentBlock]) -> Vec<Value> {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => {
                serde_json::json!({"type": "text", "text": text})
            }
            ContentBlock::ToolUse {
                id, name, input, ..
            } => {
                serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": input})
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
                ..
            } => {
                let content_val = if content.len() == 1 {
                    if let ContentBlock::Text { text } = &content[0] {
                        Value::String(text.clone())
                    } else {
                        Value::Array(hub_content_to_claude(content))
                    }
                } else {
                    Value::Array(hub_content_to_claude(content))
                };
                let mut obj = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content_val,
                });
                if *is_error {
                    obj["is_error"] = Value::Bool(true);
                } else {
                    obj["is_error"] = Value::Bool(false);
                }
                obj
            }
            ContentBlock::Thinking {
                text, signature, ..
            } => {
                if let Some(sig) = signature {
                    // Claude thinking block with signature — preserve
                    serde_json::json!({"type": "thinking", "thinking": text, "signature": sig})
                } else {
                    // Foreign thinking block (no signature) — convert to text
                    // Claude API requires signature on thinking blocks
                    serde_json::json!({"type": "text", "text": format!("[Reasoning]: {text}")})
                }
            }
            ContentBlock::Image {
                media_type, data, ..
            } => {
                serde_json::json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": media_type, "data": data}
                })
            }
            _ => serde_json::json!({"type": "text", "text": "[unconverted block]"}),
        })
        .collect()
}

fn hub_event_to_claude(
    evt: &HubEvent,
    session_id: &str,
    version: &str,
) -> Result<Value, ConvertError> {
    let cc = evt
        .extensions
        .get("claude-code")
        .cloned()
        .unwrap_or(Value::Null);

    let mut line = serde_json::json!({
        "type": evt.event_type,
    });
    if !evt.timestamp.is_empty() {
        line["timestamp"] = Value::String(evt.timestamp.clone());
    }

    if !evt.data.is_null() {
        line["data"] = evt.data.clone();
    }

    // Restore all Claude-specific event fields
    if let Some(obj) = cc.as_object() {
        for (k, v) in obj {
            line[k] = v.clone();
        }
    }

    // Fill session/version if not already present
    if (line.get("sessionId").is_none() || line["sessionId"].is_null()) && !session_id.is_empty() {
        line["sessionId"] = Value::String(session_id.to_string());
    }
    if (line.get("version").is_none() || line["version"].is_null()) && !version.is_empty() {
        line["version"] = Value::String(version.to_string());
    }

    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::semantic_eq::semantic_eq;

    #[test]
    fn test_user_message_round_trip() {
        let original = r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"hello world"},"uuid":"test-uuid-1","timestamp":"2026-03-29T12:00:00Z","userType":"external","cwd":"/home/user/project","sessionId":"session-1","version":"2.1.87","gitBranch":"main"}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();
        assert_eq!(back.len(), 1);

        let orig_val: Value = serde_json::from_str(original).unwrap();
        semantic_eq(&orig_val, &back[0]).unwrap();
    }

    #[test]
    fn test_assistant_with_tool_use_round_trip() {
        let original = r#"{"parentUuid":"p1","isSidechain":false,"type":"assistant","message":{"model":"claude-opus-4-6","id":"msg_01ABC","type":"message","role":"assistant","content":[{"type":"thinking","thinking":"let me check","signature":"sig123"},{"type":"tool_use","id":"toolu_01","name":"Bash","input":{"command":"ls"}}],"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":500,"cache_read_input_tokens":200}},"requestId":"req_01","uuid":"test-uuid-2","timestamp":"2026-03-29T12:01:00Z","userType":"external","cwd":"/home/user/project","sessionId":"session-1","version":"2.1.87","gitBranch":"main"}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();

        let orig_val: Value = serde_json::from_str(original).unwrap();
        semantic_eq(&orig_val, &back[0]).unwrap();
    }

    #[test]
    fn test_progress_event_round_trip() {
        let original = r#"{"parentUuid":null,"isSidechain":false,"type":"progress","data":{"type":"hook_progress","hookEvent":"SessionStart","hookName":"test"},"timestamp":"2026-03-29T12:00:00Z","uuid":"evt-1","userType":"external","cwd":"/home/user","sessionId":"s1","version":"2.1.87","gitBranch":"main"}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();

        let orig_val: Value = serde_json::from_str(original).unwrap();
        semantic_eq(&orig_val, &back[0]).unwrap();
    }

    #[test]
    fn test_cache_split_preserved() {
        let original = r#"{"type":"assistant","message":{"role":"assistant","type":"message","content":[{"type":"text","text":"hi"}],"usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":500,"cache_read_input_tokens":200},"model":"claude-opus-4-6","id":"msg_01","stop_reason":"end_turn","stop_sequence":null},"uuid":"u1","timestamp":"2026-03-29T12:00:00Z","sessionId":"s1"}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();

        // Verify hub has separate cache fields
        if let HubRecord::Message(ref msg) = hub[1] {
            let tokens = msg.metadata.tokens.as_ref().unwrap();
            assert_eq!(tokens.cache_creation, 500);
            assert_eq!(tokens.cache_read, 200);
        }

        let back = from_hub(&hub).unwrap();
        let orig_val: Value = serde_json::from_str(original).unwrap();
        semantic_eq(&orig_val, &back[0]).unwrap();
    }

    #[test]
    fn test_tool_result_round_trip() {
        let original = r#"{"parentUuid":"p1","isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_01","type":"tool_result","content":"file listing output","is_error":false}]},"uuid":"u2","timestamp":"2026-03-29T12:02:00Z","toolUseResult":{"stdout":"listing","stderr":"","interrupted":false},"sourceToolAssistantUUID":"a1","userType":"external","cwd":"/home/user","sessionId":"s1","version":"2.1.87","gitBranch":"main"}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();
        let back = from_hub(&hub).unwrap();

        let orig_val: Value = serde_json::from_str(original).unwrap();
        semantic_eq(&orig_val, &back[0]).unwrap();
    }

    #[test]
    fn test_extensions_are_minimal() {
        let original = r#"{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"hello"},"uuid":"u1","timestamp":"2026-03-29T12:00:00Z","userType":"external","cwd":"/home/user","sessionId":"s1","version":"2.1.87","gitBranch":"main"}"#;

        let reader = std::io::BufReader::new(original.as_bytes());
        let hub = to_hub(reader).unwrap();

        // Check that extensions only has Claude-specific fields, not duplicated universal ones
        if let HubRecord::Message(ref msg) = hub[1] {
            let ext = &msg.extensions;
            let cc = ext.get("claude-code").unwrap();
            // Should have isSidechain, userType, sessionId, version (Claude-specific)
            assert!(cc.get("isSidechain").is_some());
            assert!(cc.get("userType").is_some());
            // Should NOT have cwd, gitBranch, timestamp (universal)
            assert!(cc.get("cwd").is_none());
            assert!(cc.get("gitBranch").is_none());
            assert!(cc.get("timestamp").is_none());
        }
    }

    /// Foreign thinking blocks (no Claude signature) are converted to
    /// `[Reasoning]: text` format. Claude's API requires a signature on
    /// thinking blocks, so unsigned thinking must be preserved as text.
    /// This test documents and locks that behavior.
    #[test]
    fn test_foreign_thinking_preserved_as_text_block_not_lost() {
        let records = vec![
            HubRecord::Session(SessionHeader {
                ucf_version: UCF_VERSION.to_string(),
                session_id: "foreign-think".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:01Z".into(),
                source_cli: "gemini".into(),
                source_version: "1.0".into(),
                project: None,
                model: None,
                title: None,
                slug: None,
                parent_session_id: None,
                extensions: serde_json::json!({}),
            }),
            HubRecord::Message(HubMessage {
                id: "m1".into(),
                api_message_id: None,
                parent_id: None,
                timestamp: "2026-01-01T00:00:00Z".into(),
                completed_at: None,
                role: "assistant".into(),
                content: vec![
                    ContentBlock::Thinking {
                        text: "Let me think about this...".into(),
                        signature: None, // foreign thinking — no Claude signature
                        subject: None,
                        description: None,
                        encrypted: false,
                        encryption_format: None,
                        encrypted_data: None,
                        timestamp: None,
                    },
                    ContentBlock::Text {
                        text: "Here is my answer.".into(),
                    },
                ],
                metadata: MessageMetadata::default(),
                extensions: serde_json::json!({}),
            }),
        ];
        let claude_lines = from_hub(&records).unwrap();

        // The thinking text should appear somewhere in the output as [Reasoning]: ...
        let all_text: String = claude_lines
            .iter()
            .filter_map(|l| l.get("message").and_then(|m| m.get("content")))
            .filter_map(|c| {
                if let Some(s) = c.as_str() {
                    Some(s.to_string())
                } else if let Some(arr) = c.as_array() {
                    Some(
                        arr.iter()
                            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            all_text.contains("think about this"),
            "foreign thinking content must not be lost: {all_text}"
        );
        assert!(
            all_text.contains("[Reasoning]"),
            "foreign thinking should be tagged as [Reasoning]: {all_text}"
        );
        // The original text answer should also survive
        assert!(
            all_text.contains("Here is my answer"),
            "regular text content must survive: {all_text}"
        );
    }

    /// Signed thinking blocks (from Claude itself) should be preserved
    /// as proper thinking blocks, not converted to text.
    #[test]
    fn test_signed_thinking_preserved_as_thinking_block() {
        let records = vec![
            HubRecord::Session(SessionHeader {
                ucf_version: UCF_VERSION.to_string(),
                session_id: "signed-think".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:01Z".into(),
                source_cli: "claude-code".into(),
                source_version: "2.1".into(),
                project: None,
                model: None,
                title: None,
                slug: None,
                parent_session_id: None,
                extensions: serde_json::json!({}),
            }),
            HubRecord::Message(HubMessage {
                id: "m1".into(),
                api_message_id: None,
                parent_id: None,
                timestamp: "2026-01-01T00:00:00Z".into(),
                completed_at: None,
                role: "assistant".into(),
                content: vec![
                    ContentBlock::Thinking {
                        text: "I need to analyze this carefully.".into(),
                        signature: Some("sig_abc123".into()),
                        subject: None,
                        description: None,
                        encrypted: false,
                        encryption_format: None,
                        encrypted_data: None,
                        timestamp: None,
                    },
                    ContentBlock::Text {
                        text: "My analysis shows...".into(),
                    },
                ],
                metadata: MessageMetadata::default(),
                extensions: serde_json::json!({}),
            }),
        ];
        let claude_lines = from_hub(&records).unwrap();

        // Find the assistant message content
        let content = claude_lines
            .iter()
            .find(|l| l.get("type").and_then(|t| t.as_str()) == Some("assistant"))
            .and_then(|l| l.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
            .expect("assistant message should have content array");

        // Should have a thinking block with the signature
        let thinking = content
            .iter()
            .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("thinking"));
        assert!(
            thinking.is_some(),
            "signed thinking should be preserved as thinking block"
        );
        assert_eq!(
            thinking
                .unwrap()
                .get("signature")
                .and_then(|s| s.as_str()),
            Some("sig_abc123"),
            "signature should be preserved"
        );
    }
}
