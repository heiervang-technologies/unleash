use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;

/// Convert a Gemini CLI session JSON file to Hub records.
///
/// Gemini stores sessions as a single JSON object with a `messages` array,
/// not JSONL. The reader should provide the entire file content.
pub fn to_hub(data: &[u8]) -> Result<Vec<HubRecord>, ConvertError> {
    let root: Value = serde_json::from_slice(data)?;
    let mut records = Vec::new();

    // `root._ucf_hub.session` is the cross-CLI escape hatch carrying a full
    // SessionHeader for non-Gemini sources. When present, we replace the
    // synthesized header at the end and treat every message as foreign-
    // originated so we don't stash Gemini-native `_original_message` fidelity
    // fields that weren't actually authored by Gemini.
    let carried_session: Option<SessionHeader> = root
        .get("_ucf_hub")
        .and_then(|u| u.get("session"))
        .and_then(|s| serde_json::from_value(s.clone()).ok());
    let foreign_session = carried_session.is_some();

    records.push(HubRecord::Session(build_session_header(&root)));

    let messages = root
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();

    for msg in &messages {
        // Gemini uses "type" for the role field in some versions, "role" in others
        let role_raw = {
            let r = str_field(msg, "role");
            if r.is_empty() {
                str_field(msg, "type")
            } else {
                r
            }
        };
        match role_raw.as_str() {
            "user" | "gemini" => {
                let mut hub_msg = message_to_hub(msg, foreign_session)?;
                let mut results = Vec::new();
                
                // Gemini bundles ToolUse and ToolResult in the same assistant message.
                // The Hub format expects ToolResults in a subsequent user message.
                hub_msg.content.retain(|b| {
                    if matches!(b, ContentBlock::ToolResult { .. }) {
                        results.push(b.clone());
                        false
                    } else {
                        true
                    }
                });
                
                let result_msg = if !results.is_empty() {
                    Some(HubMessage {
                        id: format!("{}_results", hub_msg.id),
                        api_message_id: None,
                        parent_id: Some(hub_msg.id.clone()),
                        timestamp: hub_msg.timestamp.clone(),
                        completed_at: hub_msg.completed_at.clone(),
                        role: "user".to_string(),
                        content: results,
                        metadata: hub_msg.metadata.clone(),
                        extensions: hub_msg.extensions.clone(),
                    })
                } else {
                    None
                };

                records.push(HubRecord::Message(hub_msg));
                if let Some(rm) = result_msg {
                    records.push(HubRecord::Message(rm));
                }
            }
            "info" => {
                records.push(HubRecord::Event(info_to_hub_event(msg, foreign_session)?));
            }
            _ => {
                // Unknown role — treat as event
                records.push(HubRecord::Event(info_to_hub_event(msg, foreign_session)?));
            }
        }
    }

    // Patch session updated_at from last message timestamp (only if not already set)
    if let Some(HubRecord::Session(ref mut session)) = records.first_mut() {
        if session.updated_at.is_empty() {
            if let Some(last_ts) = messages
                .last()
                .and_then(|m| m.get("timestamp"))
                .and_then(|t| t.as_str())
            {
                session.updated_at = last_ts.to_string();
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

/// Convert Hub records back to Gemini CLI session JSON.
/// Returns a single JSON Value representing the entire session file.
///
/// The output matches real Gemini CLI format:
/// - Uses "type" field for message roles (not "role")
/// - Includes startTime, lastUpdated, kind at top level
/// - User content is [{text: "..."}] array format
/// - Gemini messages always have a "content" field (empty string when toolCalls present)
pub fn from_hub(records: &[HubRecord]) -> Result<Value, ConvertError> {
    let mut session_id = String::new();
    let mut session_ext = Value::Null;
    let mut start_time = String::new();
    let mut last_updated = String::new();
    let mut messages = Vec::new();
    let mut session_passthrough: Option<Value> = None;

    // Check if this is a native Gemini round-trip (has _original_message in extensions)
    let is_native_roundtrip = records.iter().any(|r| match r {
        HubRecord::Message(msg) => msg
            .extensions
            .get("gemini-cli")
            .and_then(|g| g.get("_original_message"))
            .is_some(),
        HubRecord::Event(evt) => evt
            .extensions
            .get("gemini-cli")
            .and_then(|g| g.get("_original_message"))
            .is_some(),
        _ => false,
    });

    // If the hub session came from a non-Gemini source, stash the whole
    // SessionHeader so the hub → gemini → hub round trip is lossless.
    if let Some(HubRecord::Session(s)) = records
        .iter()
        .find(|r| matches!(r, HubRecord::Session(_)))
    {
        if s.source_cli != "gemini-cli" {
            session_passthrough = Some(serde_json::to_value(s)?);
        }
    }

    if is_native_roundtrip {
        // Native Gemini round-trip: use original messages directly
        // Skip synthesized tool-result messages (those with _results suffix IDs)
        for record in records {
            match record {
                HubRecord::Session(s) => {
                    session_id = s.session_id.clone();
                    start_time = s.created_at.clone();
                    last_updated = s.updated_at.clone();
                    session_ext = s
                        .extensions
                        .get("gemini-cli")
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                HubRecord::Message(msg) => {
                    // Skip synthesized tool-result messages (IDs ending with _results)
                    if msg.id.ends_with("_results") {
                        continue;
                    }
                    if let Some(orig) = msg
                        .extensions
                        .get("gemini-cli")
                        .and_then(|g| g.get("_original_message"))
                    {
                        messages.push(orig.clone());
                    }
                }
                HubRecord::Event(evt) => {
                    if let Some(orig) = evt
                        .extensions
                        .get("gemini-cli")
                        .and_then(|g| g.get("_original_message"))
                    {
                        messages.push(orig.clone());
                    }
                }
            }
        }
    } else {
        // Cross-CLI path: reconstruct from hub content
        let normalized_records = normalize_hub_records_for_gemini(records);
        for record in &normalized_records {
            match record {
                HubRecord::Session(s) => {
                    session_id = s.session_id.clone();
                    start_time = s.created_at.clone();
                    last_updated = s.updated_at.clone();
                    session_ext = s
                        .extensions
                        .get("gemini-cli")
                        .cloned()
                        .unwrap_or(Value::Null);
                }
                HubRecord::Message(msg) => {
                    let mut gm = hub_message_to_gemini(msg)?;
                    if let Some(foreign) = foreign_extensions(&msg.extensions) {
                        attach_ucf_hub_ext(&mut gm, foreign);
                    }
                    // Gemini has no native `completed_at` — stash it so
                    // cross-CLI round trips preserve dual-timestamp semantics.
                    if let Some(ref ca) = msg.completed_at {
                        attach_ucf_hub_field(&mut gm, "completed_at", Value::String(ca.clone()));
                    }
                    messages.push(gm);
                }
                HubRecord::Event(evt) => {
                    let mut gm = hub_event_to_gemini(evt)?;
                    if let Some(foreign) = foreign_extensions(&evt.extensions) {
                        attach_ucf_hub_ext(&mut gm, foreign);
                    }
                    messages.push(gm);
                }
            }
        }

        // Ensure every message has id and timestamp
        let mut first_valid_ts = String::new();
        let mut last_valid_ts = String::new();
        let msg_count = messages.len();
        for (i, msg) in messages.iter_mut().enumerate().take(msg_count) {
            if let Some(obj) = msg.as_object_mut() {
                if obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .is_none_or(|s| s.is_empty())
                {
                    obj.insert("id".to_string(), Value::String(format!("msg-{i:04}")));
                }
                if let Some(ts) = obj.get("timestamp").and_then(|v| v.as_str()) {
                    if !ts.is_empty() {
                        if first_valid_ts.is_empty() {
                            first_valid_ts = ts.to_string();
                        }
                        last_valid_ts = ts.to_string();
                    }
                }
            }
        }

        if start_time.is_empty() {
            start_time = first_valid_ts;
        }
        if start_time.is_empty() {
            start_time = "1970-01-01T00:00:00.000Z".to_string();
        }
        if last_updated.is_empty() {
            last_updated = if last_valid_ts.is_empty() {
                start_time.clone()
            } else {
                last_valid_ts
            };
        }
    }

    let mut root = serde_json::json!({
        "sessionId": session_id,
        "startTime": start_time,
        "lastUpdated": last_updated,
        "messages": messages,
    });

    // Restore all session-level Gemini-specific fields from extensions
    if let Some(obj) = session_ext.as_object() {
        for (k, v) in obj {
            root[k] = v.clone();
        }
    }

    // Ensure "kind" is present (default if not in extensions)
    if root.get("kind").is_none() {
        root["kind"] = Value::String("main".into());
    }

    // Attach cross-CLI session passthrough (foreign source_cli).
    if let Some(sess) = session_passthrough {
        attach_ucf_hub_session(&mut root, sess);
    }

    Ok(root)
}

/// Extract hub extensions that are NOT `gemini-cli` (foreign to this format).
fn foreign_extensions(ext: &Value) -> Option<Value> {
    let obj = ext.as_object()?;
    let foreign: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "gemini-cli")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if foreign.is_empty() {
        None
    } else {
        Some(Value::Object(foreign))
    }
}

/// Merge `ext` into `node._ucf_hub.ext`, creating the nested objects as needed.
fn attach_ucf_hub_ext(node: &mut Value, ext: Value) {
    let Value::Object(ref mut obj) = node else {
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

/// Attach a serialized SessionHeader to `node._ucf_hub.session`.
fn attach_ucf_hub_session(node: &mut Value, session: Value) {
    let Value::Object(ref mut obj) = node else {
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

/// Set `node._ucf_hub.<key> = value`.
fn attach_ucf_hub_field(node: &mut Value, key: &str, value: Value) {
    let Value::Object(ref mut obj) = node else {
        return;
    };
    let entry = obj
        .entry("_ucf_hub".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let Value::Object(ref mut inner) = entry else {
        return;
    };
    inner.insert(key.to_string(), value);
}

fn normalize_hub_records_for_gemini(records: &[HubRecord]) -> Vec<HubRecord> {
    let mut norm = Vec::new();
    for record in records {
        norm.push(record.clone());
    }

    // Pass 1: move ToolResults to the message that has the matching ToolUse
    for i in 0..norm.len() {
        if let HubRecord::Message(msg) = &norm[i] {
            let mut extracted_results = Vec::new();
            let mut new_content = Vec::new();
            for block in &msg.content {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    extracted_results.push((tool_use_id.clone(), block.clone()));
                } else {
                    new_content.push(block.clone());
                }
            }

            if !extracted_results.is_empty() {
                if let HubRecord::Message(m) = &mut norm[i] {
                    m.content = new_content;
                }

                for (t_id, res_block) in extracted_results {
                    let mut found = false;
                    for j in (0..=i).rev() {
                        if let HubRecord::Message(prev_msg) = &mut norm[j] {
                            let mut insert_idx = None;
                            for (k, prev_block) in prev_msg.content.iter().enumerate() {
                                if let ContentBlock::ToolUse { id, .. } = prev_block {
                                    if id == &t_id {
                                        insert_idx = Some(k + 1);
                                        break;
                                    }
                                }
                            }
                            if let Some(idx) = insert_idx {
                                prev_msg.content.insert(idx, res_block.clone());
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        if let HubRecord::Message(m) = &mut norm[i] {
                            m.content.push(res_block);
                        }
                    }
                }
            }
        }
    }

    // Pass 2: strip empty text blocks and remove messages that become totally empty
    let norm2: Vec<HubRecord> = norm
        .into_iter()
        .filter_map(|mut r| {
            if let HubRecord::Message(m) = &mut r {
                m.content.retain(|b| match b {
                    ContentBlock::Text { text } => !text.trim().is_empty(),
                    _ => true,
                });
                if m.content.is_empty() {
                    return None;
                }
            }
            Some(r)
        })
        .collect();

    // Pass 3: Merge adjacent messages of the same role, and drop leading assistant messages
    let mut final_norm = Vec::new();
    let mut has_seen_user_message = false;

    for r in norm2 {
        match r {
            HubRecord::Message(mut msg) => {
                if !has_seen_user_message && msg.role != "user" {
                    // Drop leading non-user messages
                    continue;
                }
                has_seen_user_message = true;

                // Check if we can merge with the previous message
                let mut merged = false;
                for prev in final_norm.iter_mut().rev() {
                    if let HubRecord::Message(prev_msg) = prev {
                        if prev_msg.role == msg.role {
                            prev_msg.content.append(&mut msg.content);
                            merged = true;
                        }
                        break; // Only look at the immediately preceding message
                    } else if let HubRecord::Session(_) | HubRecord::Event(_) = prev {
                        // Ignore non-message records between messages
                        continue;
                    }
                }

                if !merged {
                    final_norm.push(HubRecord::Message(msg));
                }
            }
            other => {
                final_norm.push(other);
            }
        }
    }

    final_norm
}

/// Build a logs.json entry array for the Gemini session.
/// Each user message gets an entry with sessionId, messageId (index), type, message, timestamp.
pub fn build_logs_entries(records: &[HubRecord]) -> Vec<Value> {
    let mut entries = Vec::new();
    let mut session_id = String::new();
    let mut user_msg_idx = 0u64;

    for record in records {
        match record {
            HubRecord::Session(s) => {
                session_id = s.session_id.clone();
            }
            HubRecord::Message(msg) if msg.role == "user" => {
                // Extract first text content for the message field
                let text = msg
                    .content
                    .iter()
                    .find_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();

                entries.push(serde_json::json!({
                    "sessionId": session_id,
                    "messageId": user_msg_idx,
                    "type": "user",
                    "message": text,
                    "timestamp": msg.timestamp,
                }));
                user_msg_idx += 1;
            }
            _ => {}
        }
    }

    entries
}

// === Helpers ===

use super::helpers::{opt_str, str_field};

fn build_session_header(root: &Value) -> SessionHeader {
    let session_id = str_field(root, "sessionId");

    // Use startTime if present, otherwise fall back to first message timestamp
    let created_at = opt_str(root, "startTime").unwrap_or_else(|| {
        root.get("messages")
            .and_then(|m| m.as_array())
            .and_then(|arr| arr.first())
            .and_then(|m| m.get("timestamp"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    });

    let updated_at = opt_str(root, "lastUpdated").unwrap_or_default();

    // Stash ALL root-level fields (except messages/sessionId which are mapped)
    // `_ucf_hub` carries cross-CLI passthrough and is handled separately in `to_hub`.
    let hub_fields: &[&str] = &["sessionId", "messages", "startTime", "lastUpdated", "_ucf_hub"];
    let mut ext = serde_json::Map::new();
    if let Some(obj) = root.as_object() {
        for (k, v) in obj {
            if !hub_fields.contains(&k.as_str()) {
                ext.insert(k.clone(), v.clone());
            }
        }
    }

    let extensions = if ext.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({"gemini-cli": ext})
    };

    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id,
        created_at,
        updated_at,
        source_cli: "gemini-cli".to_string(),
        source_version: String::new(),
        project: None,
        model: None,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions,
    }
}

// --- to_hub direction ---

fn message_to_hub(msg: &Value, foreign_session: bool) -> Result<HubMessage, ConvertError> {
    let role_raw = {
        let r = str_field(msg, "role");
        if r.is_empty() {
            str_field(msg, "type")
        } else {
            r
        }
    };
    let role = match role_raw.as_str() {
        "gemini" => "assistant",
        other => other,
    };

    let mut content = Vec::new();

    // Extract thinking blocks from thoughts[] array
    if let Some(thoughts) = msg.get("thoughts").and_then(|t| t.as_array()) {
        for thought in thoughts {
            content.push(ContentBlock::Thinking {
                text: str_field(thought, "thought"),
                subject: opt_str(thought, "subject"),
                description: opt_str(thought, "description"),
                signature: None,
                encrypted: false,
                encryption_format: None,
                encrypted_data: None,
                timestamp: opt_str(thought, "timestamp"),
            });
        }
    }

    // Extract text content — Gemini uses:
    // - "content": [{text: "..."}] for user messages (array of objects)
    // - "content": "..." for gemini messages (string, often empty)
    // - "text": "..." legacy format (string)
    if let Some(text) = msg.get("text").and_then(|t| t.as_str()) {
        if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_string(),
            });
        }
    } else if let Some(content_val) = msg.get("content") {
        if let Some(content_arr) = content_val.as_array() {
            for item in content_arr {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        content.push(ContentBlock::Text {
                            text: text.to_string(),
                        });
                    }
                }
            }
        } else if let Some(text) = content_val.as_str() {
            if !text.is_empty() {
                content.push(ContentBlock::Text {
                    text: text.to_string(),
                });
            }
        }
    }

    // Extract tool calls from toolCalls[] array
    if let Some(tool_calls) = msg.get("toolCalls").and_then(|t| t.as_array()) {
        for tc in tool_calls {
            let tool_id = str_field(tc, "id");
            let name = str_field(tc, "name");

            // Tool invocation
            content.push(ContentBlock::ToolUse {
                id: tool_id.clone(),
                name: name.clone(),
                display_name: opt_str(tc, "displayName"),
                description: opt_str(tc, "description"),
                input: tc.get("args").cloned().unwrap_or(Value::Null),
            });

            // Inline tool result if present
            if let Some(result) = tc.get("result") {
                let result_content = if let Some(arr) = result.as_array() {
                    arr.iter()
                        .filter_map(|r| {
                            r.as_str().map(|s| ContentBlock::Text {
                                text: s.to_string(),
                            })
                        })
                        .collect()
                } else if let Some(s) = result.as_str() {
                    vec![ContentBlock::Text {
                        text: s.to_string(),
                    }]
                } else {
                    vec![ContentBlock::Text {
                        text: result.to_string(),
                    }]
                };

                let status = opt_str(tc, "status");
                let is_error =
                    status.as_deref() == Some("ERROR") || status.as_deref() == Some("CANCELLED");

                content.push(ContentBlock::ToolResult {
                    tool_use_id: tool_id,
                    content: result_content,
                    exit_code: tc
                        .get("exitCode")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as i32),
                    is_error,
                    interrupted: status.as_deref() == Some("CANCELLED"),
                    status,
                    duration_ms: tc.get("duration").and_then(|v| v.as_u64()),
                    title: None,
                    truncated: false,
                });
            }
        }
    }

    // Extract inline images from inlineData
    if let Some(inline_data) = msg.get("inlineData").and_then(|d| d.as_array()) {
        for item in inline_data {
            content.push(ContentBlock::Image {
                media_type: str_field(item, "mimeType"),
                encoding: "base64".to_string(),
                data: str_field(item, "data"),
                source_url: None,
            });
        }
    }

    let metadata = extract_metadata(msg);

    // Stash the ENTIRE original message for lossless round-trip — but only for
    // Gemini-authored messages. If the session or this line is foreign-
    // originated, the "original" is a synthesized Gemini shape, not a real
    // Gemini claim, so we skip stashing and just preserve any non-Gemini
    // foreign extensions carried via `_ucf_hub.ext`.
    let foreign_originated = foreign_session || msg.get("_ucf_hub").is_some();
    let mut ext = if foreign_originated {
        Value::Object(serde_json::Map::new())
    } else {
        serde_json::json!({"gemini-cli": {"_original_message": msg}})
    };

    if let Some(foreign) = msg
        .get("_ucf_hub")
        .and_then(|u| u.get("ext"))
        .and_then(|e| e.as_object())
    {
        if let Some(obj) = ext.as_object_mut() {
            for (k, v) in foreign {
                obj.insert(k.clone(), v.clone());
            }
        } else {
            ext = Value::Object(foreign.clone());
        }
    }

    if foreign_originated
        && ext
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        ext = Value::Null;
    }

    // Gemini has no native `completed_at`; recover it from `_ucf_hub` when
    // present so cross-CLI round trips preserve dual-timestamp semantics.
    let completed_at = msg
        .get("_ucf_hub")
        .and_then(|u| u.get("completed_at"))
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(HubMessage {
        id: str_field(msg, "id"),
        api_message_id: None,
        parent_id: None,
        timestamp: str_field(msg, "timestamp"),
        completed_at,
        role: role.to_string(),
        content,
        metadata,
        extensions: ext,
    })
}

fn extract_metadata(msg: &Value) -> MessageMetadata {
    let tokens = msg.get("tokens").map(|t| TokenUsage {
        input: t.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
        output: t.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
        cache_creation: 0,
        cache_read: t.get("cached").and_then(|v| v.as_u64()).unwrap_or(0),
        reasoning: t.get("thoughts").and_then(|v| v.as_u64()).unwrap_or(0),
        tool: t.get("tool").and_then(|v| v.as_u64()).unwrap_or(0),
        total: t.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
    });

    MessageMetadata {
        model: opt_str(msg, "model"),
        tokens,
        ..Default::default()
    }
}

fn info_to_hub_event(msg: &Value, foreign_session: bool) -> Result<HubEvent, ConvertError> {
    let foreign_originated = foreign_session || msg.get("_ucf_hub").is_some();

    let mut ext = if foreign_originated {
        Value::Object(serde_json::Map::new())
    } else {
        serde_json::json!({"gemini-cli": {"_original_message": msg}})
    };

    if let Some(foreign) = msg
        .get("_ucf_hub")
        .and_then(|u| u.get("ext"))
        .and_then(|e| e.as_object())
    {
        if let Some(obj) = ext.as_object_mut() {
            for (k, v) in foreign {
                obj.insert(k.clone(), v.clone());
            }
        } else {
            ext = Value::Object(foreign.clone());
        }
    }

    if foreign_originated
        && ext
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        ext = Value::Null;
    }

    Ok(HubEvent {
        event_type: "info".to_string(),
        timestamp: str_field(msg, "timestamp"),
        data: serde_json::json!({"text": str_field(msg, "text")}),
        extensions: ext,
    })
}

// --- from_hub direction ---

fn hub_message_to_gemini(msg: &HubMessage) -> Result<Value, ConvertError> {
    let gc = msg
        .extensions
        .get("gemini-cli")
        .cloned()
        .unwrap_or(Value::Null);

    let role = match msg.role.as_str() {
        "assistant" => "gemini",
        other => other,
    };

    // Always use "type" field — this is what real Gemini CLI expects
    let mut gemini_msg = serde_json::json!({
        "id": msg.id,
        "type": role,
        "timestamp": msg.timestamp,
    });

    // Reconstruct text from text content blocks
    let text_parts: Vec<&str> = msg
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text } = b {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();

    if role == "user" {
        // User messages use content: [{text: "..."}] array format
        let mut content_arr: Vec<Value> = text_parts
            .iter()
            .map(|t| serde_json::json!({"text": t}))
            .collect();
            
        // Gemini API will fail if a user message has an empty parts array
        if content_arr.is_empty() {
            content_arr.push(serde_json::json!({"text": " "}));
        }

        gemini_msg["content"] = Value::Array(content_arr);
    } else {
        // Gemini messages: "content" is a string (often empty when toolCalls present)
        let text_str = text_parts.join("\n");
        gemini_msg["content"] = Value::String(text_str);
    }

    // Reconstruct thoughts[] from Thinking blocks
    let thoughts: Vec<Value> = msg
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Thinking {
                text,
                subject,
                description,
                timestamp,
                ..
            } = b
            {
                let mut thought = serde_json::json!({"thought": text});
                if let Some(s) = subject {
                    thought["subject"] = Value::String(s.clone());
                }
                if let Some(d) = description {
                    thought["description"] = Value::String(d.clone());
                }
                if let Some(ts) = timestamp {
                    thought["timestamp"] = Value::String(ts.clone());
                }
                Some(thought)
            } else {
                None
            }
        })
        .collect();
    if !thoughts.is_empty() {
        gemini_msg["thoughts"] = Value::Array(thoughts);
    }

    // Reconstruct toolCalls[] from ToolUse + ToolResult pairs
    let tool_calls = reconstruct_tool_calls(&msg.content);
    if !tool_calls.is_empty() {
        gemini_msg["toolCalls"] = Value::Array(tool_calls);
    }

    // Reconstruct inlineData from Image blocks
    let images: Vec<Value> = msg
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Image {
                media_type, data, ..
            } = b
            {
                Some(serde_json::json!({
                    "mimeType": media_type,
                    "data": data,
                }))
            } else {
                None
            }
        })
        .collect();
    if !images.is_empty() {
        gemini_msg["inlineData"] = Value::Array(images);
    }

    // Reconstruct tokens — always emit all fields that were present in original
    if let Some(ref tokens) = msg.metadata.tokens {
        let mut tok = serde_json::json!({
            "input": tokens.input,
            "output": tokens.output,
            "total": tokens.total,
        });
        // Always emit cached/thoughts/tool if they were tracked (even if 0)
        // since Gemini includes them in the original format
        tok["cached"] = Value::Number(tokens.cache_read.into());
        tok["thoughts"] = Value::Number(tokens.reasoning.into());
        tok["tool"] = Value::Number(tokens.tool.into());
        gemini_msg["tokens"] = tok;
    }

    // Reconstruct model
    if let Some(ref model) = msg.metadata.model {
        gemini_msg["model"] = Value::String(model.clone());
    }

    // Restore Gemini-specific extensions (skip internal tracking fields)
    if let Some(obj) = gc.as_object() {
        for (k, v) in obj {
            if !k.starts_with('_') {
                gemini_msg[k] = v.clone();
            }
        }
    }

    Ok(gemini_msg)
}

/// Pair ToolUse and ToolResult blocks into Gemini toolCalls[] objects.
fn reconstruct_tool_calls(content: &[ContentBlock]) -> Vec<Value> {
    let mut calls = Vec::new();
    let mut i = 0;

    while i < content.len() {
        if let ContentBlock::ToolUse {
            id,
            name,
            display_name,
            description,
            input,
        } = &content[i]
        {
            let mut tc = serde_json::json!({
                "id": id,
                "name": name,
                "args": input,
            });
            if let Some(dn) = display_name {
                tc["displayName"] = Value::String(dn.clone());
            }
            if let Some(desc) = description {
                tc["description"] = Value::String(desc.clone());
            }

            // Check if next block is the matching ToolResult
            if i + 1 < content.len() {
                if let ContentBlock::ToolResult {
                    tool_use_id,
                    content: result_content,
                    exit_code,
                    status,
                    duration_ms,
                    ..
                } = &content[i + 1]
                {
                    if tool_use_id == id {
                        // Reconstruct result as string array
                        let result: Vec<Value> = result_content
                            .iter()
                            .filter_map(|b| {
                                if let ContentBlock::Text { text } = b {
                                    Some(Value::String(text.clone()))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        tc["result"] = Value::Array(result);

                        if let Some(code) = exit_code {
                            tc["exitCode"] = Value::Number((*code).into());
                        }
                        if let Some(s) = status {
                            tc["status"] = Value::String(s.clone());
                        }
                        if let Some(d) = duration_ms {
                            tc["duration"] = Value::Number((*d).into());
                        }

                        i += 1; // skip the ToolResult
                    }
                }
            }

            calls.push(tc);
        }
        i += 1;
    }

    calls
}

fn hub_event_to_gemini(evt: &HubEvent) -> Result<Value, ConvertError> {
    let gc = evt
        .extensions
        .get("gemini-cli")
        .cloned()
        .unwrap_or(Value::Null);

    // Always use "type" field
    let mut gemini_msg = serde_json::json!({
        "type": "info",
        "timestamp": evt.timestamp,
    });

    // Restore text from event data
    if let Some(text) = evt.data.get("text").and_then(|t| t.as_str()) {
        gemini_msg["text"] = Value::String(text.to_string());
    }

    // Restore Gemini-specific fields (skip internal tracking fields)
    if let Some(obj) = gc.as_object() {
        for (k, v) in obj {
            if !k.starts_with('_') {
                gemini_msg[k] = v.clone();
            }
        }
    }

    Ok(gemini_msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::semantic_eq::semantic_eq;

    fn gemini_session_json(messages: &[Value]) -> Vec<u8> {
        let session = serde_json::json!({
            "sessionId": "gem-session-1",
            "projectHash": "abc123hash",
            "installationId": "inst-uuid-1",
            "messages": messages,
        });
        serde_json::to_vec(&session).unwrap()
    }

    #[test]
    fn test_user_message_round_trip() {
        // Real Gemini uses "type" for role and "content" array for user messages
        let msg = serde_json::json!({
            "id": "msg-1",
            "type": "user",
            "content": [{"text": "What files are in this directory?"}],
            "timestamp": "2026-03-29T12:00:00.000Z"
        });

        let data = gemini_session_json(&[msg.clone()]);
        let hub = to_hub(&data).unwrap();
        let back = from_hub(&hub).unwrap();
        let back_messages = back.get("messages").unwrap().as_array().unwrap();

        assert_eq!(back_messages.len(), 1);
        semantic_eq(&msg, &back_messages[0]).unwrap();
    }

    #[test]
    fn test_gemini_message_with_thoughts_round_trip() {
        let msg = serde_json::json!({
            "id": "msg-2",
            "type": "gemini",
            "content": "Here are the files in this directory.",
            "thoughts": [
                {
                    "thought": "Let me check the directory listing.",
                    "subject": "Directory analysis",
                    "description": "Examining file structure",
                    "timestamp": "2026-03-29T12:00:01.000Z"
                }
            ],
            "tokens": {
                "input": 100,
                "output": 50,
                "cached": 20,
                "thoughts": 30,
                "tool": 0,
                "total": 200
            },
            "model": "gemini-2.5-pro",
            "timestamp": "2026-03-29T12:00:02.000Z"
        });

        let data = gemini_session_json(&[
            serde_json::json!({
                "id": "msg-user",
                "type": "user",
                "content": [{"text": "hello"}],
                "timestamp": "2026-03-29T12:00:00.000Z"
            }),
            msg.clone(),
        ]);
        let hub = to_hub(&data).unwrap();

        // Verify hub has correct structure
        if let HubRecord::Message(ref hub_msg) = hub[2] {
            assert_eq!(hub_msg.role, "assistant");
            // Should have thinking + text content blocks
            assert!(hub_msg
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::Thinking { .. })));
            assert!(hub_msg
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::Text { .. })));
            // Token mapping
            let tokens = hub_msg.metadata.tokens.as_ref().unwrap();
            assert_eq!(tokens.cache_read, 20);
            assert_eq!(tokens.reasoning, 30);
        }

        let back = from_hub(&hub).unwrap();
        let back_messages = back.get("messages").unwrap().as_array().unwrap();
        semantic_eq(&msg, &back_messages[1]).unwrap();
    }

    #[test]
    fn test_tool_call_with_result_round_trip() {
        let msg = serde_json::json!({
            "id": "msg-3",
            "type": "gemini",
            "content": "I'll list the files.",
            "toolCalls": [
                {
                    "id": "tc-1",
                    "name": "shell",
                    "displayName": "Shell Command",
                    "description": "Execute a shell command",
                    "args": {"command": "ls -la"},
                    "result": ["total 42\ndrwxr-xr-x 5 user user 4096 Mar 29 12:00 ."],
                    "exitCode": 0,
                    "status": "COMPLETED",
                    "duration": 150
                }
            ],
            "timestamp": "2026-03-29T12:01:00.000Z"
        });

        let data = gemini_session_json(&[
            serde_json::json!({
                "id": "msg-user",
                "type": "user",
                "content": [{"text": "list"}],
                "timestamp": "2026-03-29T12:00:00.000Z"
            }),
            msg.clone(),
        ]);
        let hub = to_hub(&data).unwrap();

        // Verify tool call is on the assistant message (hub[2])
        // and tool result is split into a separate user message (hub[3])
        if let HubRecord::Message(ref hub_msg) = hub[2] {
            let tool_uses: Vec<_> = hub_msg
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                .collect();
            assert_eq!(tool_uses.len(), 1);

            if let ContentBlock::ToolUse {
                display_name,
                description,
                ..
            } = &tool_uses[0]
            {
                assert_eq!(display_name.as_deref(), Some("Shell Command"));
                assert_eq!(description.as_deref(), Some("Execute a shell command"));
            }
        }
        // Tool results are extracted into a separate user message
        if let HubRecord::Message(ref result_msg) = hub[3] {
            assert_eq!(result_msg.role, "user");
            let tool_results: Vec<_> = result_msg
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::ToolResult { .. }))
                .collect();
            assert_eq!(tool_results.len(), 1);
        }

        let back = from_hub(&hub).unwrap();
        let back_messages = back.get("messages").unwrap().as_array().unwrap();
        semantic_eq(&msg, &back_messages[1]).unwrap();
    }

    #[test]
    fn test_info_message_round_trip() {
        let messages = vec![
            serde_json::json!({
                "id": "msg-0",
                "type": "info",
                "text": "Session started",
                "timestamp": "2026-03-29T12:00:00.000Z"
            }),
            serde_json::json!({
                "id": "msg-1",
                "type": "user",
                "content": [{"text": "hello"}],
                "timestamp": "2026-03-29T12:00:01.000Z"
            }),
        ];

        let data = gemini_session_json(&messages);
        let hub = to_hub(&data).unwrap();
        let back = from_hub(&hub).unwrap();
        let back_messages = back.get("messages").unwrap().as_array().unwrap();

        assert_eq!(back_messages.len(), 2);
        semantic_eq(&messages[0], &back_messages[0]).unwrap();
        semantic_eq(&messages[1], &back_messages[1]).unwrap();
    }

    #[test]
    fn test_session_header_round_trip() {
        let data = gemini_session_json(&[serde_json::json!({
            "id": "msg-1",
            "type": "user",
            "content": [{"text": "hi"}],
            "timestamp": "2026-03-29T12:00:00.000Z"
        })]);

        let hub = to_hub(&data).unwrap();
        let back = from_hub(&hub).unwrap();

        assert_eq!(
            back.get("sessionId").unwrap().as_str().unwrap(),
            "gem-session-1"
        );
        assert_eq!(
            back.get("projectHash").unwrap().as_str().unwrap(),
            "abc123hash"
        );
        // Verify new required fields
        assert!(back.get("startTime").is_some());
        assert!(back.get("lastUpdated").is_some());
        assert_eq!(back.get("kind").unwrap().as_str().unwrap(), "main");
    }

    #[test]
    fn test_empty_session_round_trip() {
        let data = gemini_session_json(&[]);
        let hub = to_hub(&data).unwrap();
        let back = from_hub(&hub).unwrap();

        assert_eq!(
            back.get("sessionId").unwrap().as_str().unwrap(),
            "gem-session-1"
        );
        assert!(back.get("messages").unwrap().as_array().unwrap().is_empty());
    }

    #[test]
    fn test_multiple_tool_calls_round_trip() {
        let user_msg = serde_json::json!({
            "id": "msg-user",
            "type": "user",
            "content": [{"text": "run command"}],
            "timestamp": "2026-03-29T12:01:00.000Z"
        });
        let msg = serde_json::json!({
            "id": "msg-4",
            "type": "gemini",
            "content": "Running commands.",
            "toolCalls": [
                {
                    "id": "tc-1",
                    "name": "shell",
                    "args": {"command": "pwd"},
                    "result": ["/home/user"],
                    "exitCode": 0,
                    "status": "COMPLETED"
                },
                {
                    "id": "tc-2",
                    "name": "readFile",
                    "args": {"path": "README.md"},
                    "result": ["# My Project"],
                    "status": "COMPLETED"
                }
            ],
            "timestamp": "2026-03-29T12:02:00.000Z"
        });

        let data = gemini_session_json(&[user_msg.clone(), msg.clone()]);
        let hub = to_hub(&data).unwrap();
        let back = from_hub(&hub).unwrap();
        let back_messages = back.get("messages").unwrap().as_array().unwrap();

        semantic_eq(&msg, &back_messages[1]).unwrap();
    }

    #[test]
    fn test_gemini_extensions_store_original_message() {
        let msg = serde_json::json!({
            "id": "msg-5",
            "type": "gemini",
            "content": "hello",
            "renderOutputAsMarkdown": true,
            "tokens": {"input": 10, "output": 5, "total": 15},
            "model": "gemini-2.5-pro",
            "timestamp": "2026-03-29T12:00:00.000Z"
        });

        let data = gemini_session_json(&[msg.clone()]);
        let hub = to_hub(&data).unwrap();

        if let HubRecord::Message(ref hub_msg) = hub[1] {
            let ext = &hub_msg.extensions;
            let gc = ext.get("gemini-cli").unwrap();
            // Should have the original message stashed for lossless round-trip
            let orig = gc.get("_original_message").unwrap();
            assert_eq!(orig.get("id").unwrap().as_str().unwrap(), "msg-5");
            assert_eq!(
                orig.get("renderOutputAsMarkdown").unwrap().as_bool().unwrap(),
                true
            );
        }
    }
}
