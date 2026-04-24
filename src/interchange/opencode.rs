use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;

/// Input for OpenCode conversion: messages + parts exported from SQLite.
pub struct OpenCodeInput {
    pub session_id: String,
    pub messages: Vec<Value>,
    pub parts: Vec<Value>,
}

/// Output from Hub -> OpenCode conversion.
pub struct OpenCodeOutput {
    pub messages: Vec<Value>,
    pub parts: Vec<Value>,
}

/// Convert OpenCode messages + parts to Hub records.
pub fn to_hub(input: &OpenCodeInput) -> Result<Vec<HubRecord>, ConvertError> {
    let mut records = Vec::new();

    // `messages[0]._ucf_hub.session` is the cross-CLI escape hatch carrying a
    // full SessionHeader for non-OpenCode sources. When present, we replace
    // the synthesized header at the end and treat every message as foreign-
    // originated so we don't stash OpenCode-native fidelity fields
    // (`_original_message`, `_original_parts`) that weren't actually authored
    // by OpenCode.
    let carried_session: Option<SessionHeader> = input
        .messages
        .first()
        .and_then(|m| m.get("_ucf_hub"))
        .and_then(|u| u.get("session"))
        .and_then(|s| serde_json::from_value(s.clone()).ok());
    let foreign_session = carried_session.is_some();

    // Build session header from first message
    if let Some(first_msg) = input.messages.first() {
        records.push(HubRecord::Session(build_session_header(
            &input.session_id,
            first_msg,
            input.messages.last(),
        )));
    }

    // Build an index of parts by message index for association
    // Parts don't have message_id in our exported fixture, so we associate by order:
    // parts are ordered and grouped by the messages they belong to.
    // In practice, we track a part cursor.
    let mut part_idx = 0;
    let part_count = input.parts.len();

    for (msg_i, msg) in input.messages.iter().enumerate() {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");

        let msg_parts = collect_message_parts(
            &input.parts,
            &mut part_idx,
            part_count,
            role,
            msg_i,
            input.messages.len(),
        );

        let content = parts_to_content_blocks(&msg_parts)?;
        let metadata = extract_metadata(msg);

        let foreign_originated = foreign_session || msg.get("_ucf_hub").is_some();

        // When the message came from a foreign source, the "original" opencode
        // payload is a synthesized shape, not a real OpenCode claim — skip the
        // `_original_message`/`_original_parts` stash and the opencode-native
        // extension fields. Preserve only foreign extensions carried through
        // `_ucf_hub.ext`.
        let mut extensions = if foreign_originated {
            Value::Object(serde_json::Map::new())
        } else {
            let mut ext_map = serde_json::Map::new();
            ext_map.insert("_original_message".into(), msg.clone());
            ext_map.insert("_original_parts".into(), Value::Array(msg_parts.clone()));
            if let Value::Object(extra) = build_opencode_extensions(msg) {
                if let Some(Value::Object(oc)) = extra.get("opencode") {
                    for (k, v) in oc {
                        ext_map.insert(k.clone(), v.clone());
                    }
                }
            }
            serde_json::json!({"opencode": ext_map})
        };

        if let Some(foreign) = msg
            .get("_ucf_hub")
            .and_then(|u| u.get("ext"))
            .and_then(|e| e.as_object())
        {
            if let Some(obj) = extensions.as_object_mut() {
                for (k, v) in foreign {
                    obj.insert(k.clone(), v.clone());
                }
            } else {
                extensions = Value::Object(foreign.clone());
            }
        }

        if foreign_originated
            && extensions
                .as_object()
                .is_some_and(serde_json::Map::is_empty)
        {
            extensions = Value::Null;
        }

        let timestamp = unix_ms_to_iso(
            msg.get("time")
                .and_then(|t| t.get("created"))
                .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)))
                .unwrap_or(0.0),
        );
        let completed_at = msg
            .get("time")
            .and_then(|t| t.get("completed"))
            .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)))
            .map(unix_ms_to_iso);

        // For foreign-originated messages, honor the carried hub id so the
        // A → opencode → A round trip preserves message identity. Otherwise
        // synthesize a deterministic id from the message index.
        let id = if foreign_originated {
            msg.get("id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| format!("opencode-msg-{msg_i}"))
        } else {
            format!("opencode-msg-{msg_i}")
        };

        records.push(HubRecord::Message(HubMessage {
            id,
            api_message_id: None,
            parent_id: opt_str(msg, "parentID"),
            timestamp,
            completed_at,
            role: role.to_string(),
            content,
            metadata,
            extensions,
        }));
    }

    // Update session updated_at from last message
    if let (Some(HubRecord::Session(ref mut session)), Some(last_msg)) =
        (records.first_mut(), input.messages.last())
    {
        if let Some(ts) = last_msg
            .get("time")
            .and_then(|t| t.get("completed").or_else(|| t.get("created")))
            .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)))
        {
            session.updated_at = unix_ms_to_iso(ts);
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

/// Convert Hub records back to OpenCode messages + parts.
///
/// When the hub session's `source_cli` is not `opencode`, the first emitted
/// message carries a `_ucf_hub.session` field holding the full SessionHeader
/// so the hub → opencode → hub round trip is lossless. Per-message foreign
/// extensions are stashed under `_ucf_hub.ext`.
pub fn from_hub(records: &[HubRecord]) -> Result<OpenCodeOutput, ConvertError> {
    let mut messages = Vec::new();
    let mut parts = Vec::new();

    // Check if this is a native OpenCode round-trip (has _original_message in extensions)
    let is_native_roundtrip = records.iter().any(|r| {
        if let HubRecord::Message(msg) = r {
            msg.extensions
                .get("opencode")
                .and_then(|g| g.get("_original_message"))
                .is_some()
        } else {
            false
        }
    });

    // If the hub session came from a non-OpenCode source, stash the whole
    // SessionHeader so the hub → opencode → hub round trip is lossless.
    let session_passthrough: Option<Value> = records.iter().find_map(|r| {
        if let HubRecord::Session(s) = r {
            if s.source_cli != "opencode" {
                return serde_json::to_value(s).ok();
            }
        }
        None
    });

    if is_native_roundtrip {
        // Native round-trip: use original messages and parts directly
        for record in records {
            if let HubRecord::Message(msg) = record {
                let oc = msg
                    .extensions
                    .get("opencode")
                    .cloned()
                    .unwrap_or(Value::Null);
                if let Some(orig_msg) = oc.get("_original_message") {
                    messages.push(orig_msg.clone());
                }
                if let Some(orig_parts) = oc.get("_original_parts").and_then(|v| v.as_array()) {
                    parts.extend(orig_parts.iter().cloned());
                }
            }
        }
    } else {
        // Cross-CLI path: reconstruct from hub content
        let mut msg_idx = 0;
        for record in records {
            match record {
                HubRecord::Session(_) => {}
                HubRecord::Message(msg) => {
                    let (mut oc_msg, mut oc_parts) = hub_message_to_opencode(msg)?;
                    for part in &mut oc_parts {
                        if let Some(obj) = part.as_object_mut() {
                            obj.insert("_msg_idx".to_string(), serde_json::json!(msg_idx));
                        }
                    }
                    if let Some(foreign) = foreign_extensions(&msg.extensions) {
                        attach_ucf_hub_ext(&mut oc_msg, foreign);
                    }
                    messages.push(oc_msg);
                    parts.extend(oc_parts);
                    msg_idx += 1;
                }
                HubRecord::Event(_) => {}
            }
        }
    }

    // Attach cross-CLI session passthrough to the first emitted message.
    if let (Some(sess), Some(first)) = (session_passthrough, messages.first_mut()) {
        attach_ucf_hub_session(first, sess);
    }

    Ok(OpenCodeOutput { messages, parts })
}

// === _ucf_hub passthrough helpers ===

/// Extract hub extensions that are NOT `opencode` (foreign to this format).
fn foreign_extensions(ext: &Value) -> Option<Value> {
    let obj = ext.as_object()?;
    let foreign: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "opencode")
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

// === Helpers ===

use super::helpers::opt_str;

fn unix_ms_to_iso(ms: f64) -> String {
    let total_secs = (ms / 1000.0) as i64;
    let millis = (ms % 1000.0) as u32;

    // Calculate date/time components from epoch
    let days = total_secs / 86400;
    let time_of_day = (total_secs % 86400 + 86400) % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from days since epoch (algorithm from Howard Hinnant)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    // Omit the `.000` millisecond suffix when millis is zero so round trips
    // through ISO → ms → ISO don't gain spurious `.000` precision.
    if millis == 0 {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year, m, d, hours, minutes, seconds
        )
    } else {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            year, m, d, hours, minutes, seconds, millis
        )
    }
}

fn iso_to_unix_ms(iso: &str) -> f64 {
    // Parse ISO 8601 format: YYYY-MM-DDTHH:MM:SS.mmmZ
    let parts: Vec<&str> = iso.split('T').collect();
    if parts.len() != 2 {
        return 0.0;
    }
    let date_parts: Vec<i64> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() != 3 {
        return 0.0;
    }
    let (year, month, day) = (date_parts[0], date_parts[1], date_parts[2]);

    let time_str = parts[1].trim_end_matches('Z').trim_end_matches("+00:00");
    let time_parts: Vec<&str> = time_str.split(':').collect();
    if time_parts.len() < 3 {
        return 0.0;
    }
    let hours: i64 = time_parts[0].parse().unwrap_or(0);
    let minutes: i64 = time_parts[1].parse().unwrap_or(0);
    let sec_parts: Vec<&str> = time_parts[2].split('.').collect();
    let seconds: i64 = sec_parts[0].parse().unwrap_or(0);
    let millis: i64 = if sec_parts.len() > 1 {
        let frac = sec_parts[1];
        let padded = format!("{:0<3}", &frac[..frac.len().min(3)]);
        padded.parse().unwrap_or(0)
    } else {
        0
    };

    // Days from civil date (Howard Hinnant algorithm)
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * m as u64 + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe as i64 - 719468;

    (days * 86400 + hours * 3600 + minutes * 60 + seconds) as f64 * 1000.0 + millis as f64
}

fn build_session_header(
    session_id: &str,
    first_msg: &Value,
    last_msg: Option<&Value>,
) -> SessionHeader {
    let model_id = first_msg
        .get("model")
        .and_then(|m| m.get("modelID"))
        .or_else(|| first_msg.get("modelID"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let created_ts = first_msg
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)))
        .unwrap_or(0.0);

    let updated_ts = last_msg
        .and_then(|m| {
            m.get("time")
                .and_then(|t| t.get("completed").or_else(|| t.get("created")))
                .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)))
        })
        .unwrap_or(created_ts);

    let cwd = first_msg
        .get("path")
        .and_then(|p| p.get("cwd"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let root = first_msg
        .get("path")
        .and_then(|p| p.get("root"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let provider = first_msg
        .get("model")
        .and_then(|m| m.get("providerID"))
        .or_else(|| first_msg.get("providerID"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id: session_id.to_string(),
        created_at: unix_ms_to_iso(created_ts),
        updated_at: unix_ms_to_iso(updated_ts),
        source_cli: "opencode".to_string(),
        source_version: String::new(),
        project: Some(ProjectInfo {
            directory: cwd.to_string(),
            root,
            hash: None,
            vcs: Some("git".to_string()),
            branch: None,
            sha: None,
            origin_url: None,
        }),
        model: model_id,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions: serde_json::json!({
            "opencode": {
                "providerID": provider
            }
        }),
    }
}

/// Collect parts that belong to a given message.
/// Strategy: user messages get parts until we hit a step-start (or non-text for user).
/// Assistant messages get parts from step-start through step-finish, possibly multiple steps.
fn collect_message_parts(
    all_parts: &[Value],
    idx: &mut usize,
    total: usize,
    role: &str,
    _msg_i: usize,
    _msg_count: usize,
) -> Vec<Value> {
    let mut collected = Vec::new();

    // If parts have _msg_idx (injected by from_hub for reliable round-tripping), use it
    if *idx < total {
        if all_parts[*idx].get("_msg_idx").is_some() {
            while *idx < total {
                let part = &all_parts[*idx];
                if part.get("_msg_idx").and_then(|v| v.as_u64()) == Some(_msg_i as u64) {
                    let mut cleaned_part = part.clone();
                    if let Some(obj) = cleaned_part.as_object_mut() {
                        obj.remove("_msg_idx");
                    }
                    collected.push(cleaned_part);
                    *idx += 1;
                } else {
                    break;
                }
            }
            return collected;
        }
    }

    if role == "user" {
        // User messages typically have one text part, maybe tool results
        while *idx < total {
            let part = &all_parts[*idx];
            let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if ptype == "step-start" || ptype == "reasoning" {
                break;
            }
            collected.push(part.clone());
            *idx += 1;

            // If it's a native opencode export without _msg_idx, we conservatively break after one text part
            // to avoid consuming the next user message's text part.
            if ptype == "text" {
                break;
            }
        }
    } else {
        // Assistant: collect step-start -> ... -> step-finish sequences
        // May have multiple steps per message
        while *idx < total {
            let part = &all_parts[*idx];
            let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");

            // If we hit a text part that looks like the next user message's text, stop
            if ptype == "text" && !collected.is_empty() {
                let last_type = collected
                    .last()
                    .and_then(|p: &Value| p.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if last_type == "step-finish" {
                    // Check if next part after this text is step-start (still this message)
                    // or text (next user message)
                    let next_idx = *idx + 1;
                    if next_idx < total {
                        let next_type = all_parts[next_idx]
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if next_type != "step-start" {
                            // This text belongs to next user message
                            break;
                        }
                    } else {
                        // Last text after a step-finish is likely next user msg
                        break;
                    }
                }
            }

            collected.push(part.clone());
            *idx += 1;

            // After a step-finish with no following step-start for this message, check
            if ptype == "step-finish" {
                if *idx < total {
                    let next_type = all_parts[*idx]
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if next_type == "step-start" {
                        // Another step in same message, continue
                        continue;
                    } else if next_type == "text" || next_type == "tool" {
                        // Could be next user message text/tool, break
                        break;
                    }
                }
                // If no more parts or next is something else, this message is done
                if *idx >= total {
                    break;
                }
            }
        }
    }

    collected
}

fn parts_to_content_blocks(parts: &[Value]) -> Result<Vec<ContentBlock>, ConvertError> {
    let mut blocks = Vec::new();

    for part in parts {
        let ptype = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match ptype {
            "text" => {
                let text = part
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if !text.is_empty() {
                    blocks.push(ContentBlock::Text { text });
                }
                // Recover images stored in _hub_images extension
                if let Some(images) = part.get("_hub_images").and_then(|v| v.as_array()) {
                    for img in images {
                        blocks.push(ContentBlock::Image {
                            media_type: img
                                .get("media_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("image/png")
                                .to_string(),
                            encoding: img
                                .get("encoding")
                                .and_then(|v| v.as_str())
                                .unwrap_or("base64")
                                .to_string(),
                            data: img
                                .get("data")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            source_url: img
                                .get("source_url")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                        });
                    }
                }
            }
            "step-start" => {
                blocks.push(ContentBlock::StepBoundary {
                    boundary: "start".to_string(),
                    snapshot: opt_str(part, "snapshot"),
                    finish_reason: None,
                    cost: None,
                    tokens: None,
                });
            }
            "step-finish" => {
                let tokens = part.get("tokens").map(|t| TokenUsage {
                    input: t.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
                    output: t.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
                    cache_creation: 0,
                    cache_read: t
                        .get("cache")
                        .and_then(|c| c.get("read"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                    reasoning: t.get("reasoning").and_then(|v| v.as_u64()).unwrap_or(0),
                    tool: 0,
                    total: 0,
                });
                blocks.push(ContentBlock::StepBoundary {
                    boundary: "finish".to_string(),
                    snapshot: None,
                    finish_reason: opt_str(part, "reason"),
                    cost: part.get("cost").and_then(|v| v.as_f64()),
                    tokens,
                });
            }
            "reasoning" => {
                let text = part
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Check for encrypted reasoning
                let metadata = part.get("metadata");
                let reasoning_details = metadata
                    .and_then(|m| m.get("openrouter"))
                    .and_then(|or| or.get("reasoning_details"))
                    .and_then(|rd| rd.as_array());

                let mut encrypted = false;
                let mut encryption_format = None;
                let mut encrypted_data = None;

                if let Some(details) = reasoning_details {
                    for detail in details {
                        if detail.get("type").and_then(|v| v.as_str())
                            == Some("reasoning.encrypted")
                        {
                            encrypted = true;
                            encryption_format = detail
                                .get("format")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            encrypted_data = detail
                                .get("data")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            break;
                        }
                    }
                }

                blocks.push(ContentBlock::Thinking {
                    text,
                    subject: None,
                    description: None,
                    signature: None,
                    encrypted,
                    encryption_format,
                    encrypted_data,
                    timestamp: None,
                });

                // Store full metadata in a companion extension if needed
                // (handled at message level via extensions)
            }
            "tool" => {
                let tool_name = part
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let state = part.get("state");
                let call_id = part
                    .get("callID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let input_val = state
                    .and_then(|s| s.get("input"))
                    .cloned()
                    .unwrap_or(Value::Null);

                blocks.push(ContentBlock::ToolUse {
                    id: call_id.clone(),
                    name: tool_name,
                    display_name: None,
                    description: state.and_then(|s| opt_str(s, "title")),
                    input: input_val,
                });

                // Tool result in same part
                let output_text = state
                    .and_then(|s| s.get("output"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let status = state
                    .and_then(|s| s.get("status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed")
                    .to_string();
                let is_error = status == "error";
                let exit_code = state
                    .and_then(|s| s.get("metadata"))
                    .and_then(|m| m.get("exit"))
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32);
                let truncated = state
                    .and_then(|s| s.get("metadata"))
                    .and_then(|m| m.get("truncated"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let start_ms = state
                    .and_then(|s| s.get("time"))
                    .and_then(|t| t.get("start"))
                    .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)));
                let end_ms = state
                    .and_then(|s| s.get("time"))
                    .and_then(|t| t.get("end"))
                    .and_then(|v| v.as_f64().or_else(|| v.as_u64().map(|u| u as f64)));
                let duration = match (start_ms, end_ms) {
                    (Some(s), Some(e)) => Some((e - s) as u64),
                    _ => None,
                };

                blocks.push(ContentBlock::ToolResult {
                    tool_use_id: call_id,
                    content: vec![ContentBlock::Text { text: output_text }],
                    exit_code,
                    is_error,
                    interrupted: false,
                    status: Some(status),
                    duration_ms: duration,
                    title: state.and_then(|s| opt_str(s, "title")),
                    truncated,
                });
            }
            "patch" => {
                let path = part
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let hash_before = part
                    .get("hash")
                    .and_then(|h| h.get("before"))
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let hash_after = part
                    .get("hash")
                    .and_then(|h| h.get("after"))
                    .and_then(|v| v.as_str())
                    .map(String::from);

                blocks.push(ContentBlock::Patch {
                    path,
                    hash_before,
                    hash_after,
                });
            }
            _ => {
                // Unknown part type, preserve as text annotation
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        blocks.push(ContentBlock::Text {
                            text: text.to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(blocks)
}

fn extract_metadata(msg: &Value) -> MessageMetadata {
    let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");

    if role == "assistant" {
        let tokens = msg.get("tokens").map(|t| TokenUsage {
            input: t.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
            output: t.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
            cache_creation: t
                .get("cache")
                .and_then(|c| c.get("write"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            cache_read: t
                .get("cache")
                .and_then(|c| c.get("read"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            reasoning: t.get("reasoning").and_then(|v| v.as_u64()).unwrap_or(0),
            tool: 0,
            total: 0,
        });

        let model = msg
            .get("modelID")
            .and_then(|v| v.as_str())
            .map(String::from);
        let cwd = msg
            .get("path")
            .and_then(|p| p.get("cwd"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let root = msg
            .get("path")
            .and_then(|p| p.get("root"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let cost = msg.get("cost").and_then(|v| v.as_f64());
        let stop_reason = opt_str(msg, "finish");
        let mode = opt_str(msg, "mode");
        let agent = opt_str(msg, "agent");

        MessageMetadata {
            model,
            tokens,
            tokens_cumulative: false,
            cost,
            stop_reason,
            cwd,
            root,
            git_branch: None,
            mode,
            agent,
        }
    } else {
        MessageMetadata {
            agent: opt_str(msg, "agent"),
            ..Default::default()
        }
    }
}

fn build_opencode_extensions(msg: &Value) -> Value {
    let mut ext = serde_json::Map::new();

    // Preserve providerID (not in universal schema)
    if let Some(v) = msg.get("providerID").and_then(|v| v.as_str()) {
        ext.insert("providerID".into(), Value::String(v.to_string()));
    }

    // Preserve model info on user messages (nested model object)
    if let Some(model) = msg.get("model") {
        if model.is_object() {
            ext.insert("model".into(), model.clone());
        }
    }

    // Preserve summary on user messages
    if let Some(summary) = msg.get("summary") {
        ext.insert("summary".into(), summary.clone());
    }

    // Preserve error data
    if let Some(error) = msg.get("error") {
        ext.insert("error".into(), error.clone());
    }

    if ext.is_empty() {
        Value::Null
    } else {
        serde_json::json!({"opencode": ext})
    }
}

// === from_hub direction ===

fn hub_message_to_opencode(msg: &HubMessage) -> Result<(Value, Vec<Value>), ConvertError> {
    let oc = msg
        .extensions
        .get("opencode")
        .cloned()
        .unwrap_or(Value::Null);

    let created_ms = iso_to_unix_ms(&msg.timestamp);
    let completed_ms = msg.completed_at.as_deref().map(iso_to_unix_ms);

    // OpenCode has no native "tool" role: tool results are carried as tool
    // parts on user-role messages (mirroring Anthropic's convention). Any
    // incoming tool-role hub message (e.g. from Pi) is coerced to user so
    // the output matches OpenCode's on-the-wire shape and the cross-CLI
    // round-trip produces portable roles.
    let out_role = if msg.role == "tool" { "user" } else { msg.role.as_str() };

    let mut message = serde_json::json!({
        "role": out_role,
        "time": {
            "created": created_ms,
        },
    });

    // Preserve the hub message id so cross-CLI round trips can restore it.
    if !msg.id.is_empty() {
        message["id"] = Value::String(msg.id.clone());
    }

    if let Some(completed) = completed_ms {
        message["time"]["completed"] = serde_json::json!(completed);
    }

    if msg.role == "assistant" {
        if let Some(ref model) = msg.metadata.model {
            message["modelID"] = Value::String(model.clone());
        }
        if let Some(provider) = oc.get("providerID") {
            message["providerID"] = provider.clone();
        }
        if let Some(ref mode) = msg.metadata.mode {
            message["mode"] = Value::String(mode.clone());
        }
        if let Some(ref agent) = msg.metadata.agent {
            message["agent"] = Value::String(agent.clone());
        }
        if let Some(ref cwd) = msg.metadata.cwd {
            message["path"] = serde_json::json!({
                "cwd": cwd,
            });
            if let Some(ref root) = msg.metadata.root {
                message["path"]["root"] = Value::String(root.clone());
            }
        }
        if let Some(cost) = msg.metadata.cost {
            message["cost"] = serde_json::json!(cost);
        }
        if let Some(ref tokens) = msg.metadata.tokens {
            message["tokens"] = serde_json::json!({
                "input": tokens.input,
                "output": tokens.output,
                "reasoning": tokens.reasoning,
                "cache": {
                    "read": tokens.cache_read,
                    "write": tokens.cache_creation,
                }
            });
        }
        if let Some(ref stop) = msg.metadata.stop_reason {
            message["finish"] = Value::String(stop.clone());
        }
    }

    if let Some(ref parent) = msg.parent_id {
        message["parentID"] = Value::String(parent.clone());
    }

    // Restore OpenCode-specific fields from extensions
    if let Some(summary) = oc.get("summary") {
        message["summary"] = summary.clone();
    }
    if let Some(error) = oc.get("error") {
        message["error"] = error.clone();
    }
    if msg.role == "user" {
        if let Some(ref agent) = msg.metadata.agent {
            message["agent"] = Value::String(agent.clone());
        }
        if let Some(model) = oc.get("model") {
            message["model"] = model.clone();
        }
    }

    // Convert content blocks back to OpenCode parts
    let parts = hub_content_to_opencode_parts(&msg.content);

    Ok((message, parts))
}

fn hub_content_to_opencode_parts(blocks: &[ContentBlock]) -> Vec<Value> {
    let mut parts = Vec::new();

    for block in blocks {
        match block {
            ContentBlock::Text { text } => {
                parts.push(serde_json::json!({
                    "type": "text",
                    "text": text,
                }));
            }
            ContentBlock::StepBoundary {
                boundary,
                snapshot,
                finish_reason,
                cost,
                tokens,
            } => {
                if boundary == "start" {
                    let mut part = serde_json::json!({
                        "type": "step-start",
                        "text": "",
                    });
                    if let Some(snap) = snapshot {
                        part["snapshot"] = Value::String(snap.clone());
                    }
                    parts.push(part);
                } else {
                    let mut part = serde_json::json!({
                        "type": "step-finish",
                        "text": "",
                    });
                    if let Some(reason) = finish_reason {
                        part["reason"] = Value::String(reason.clone());
                    }
                    if let Some(c) = cost {
                        part["cost"] = serde_json::json!(c);
                    }
                    if let Some(t) = tokens {
                        part["tokens"] = serde_json::json!({
                            "input": t.input,
                            "output": t.output,
                            "reasoning": t.reasoning,
                            "cache": {
                                "read": t.cache_read,
                                "write": t.cache_creation,
                            }
                        });
                    }
                    parts.push(part);
                }
            }
            ContentBlock::Thinking {
                text,
                encrypted,
                encryption_format,
                encrypted_data,
                ..
            } => {
                let mut part = serde_json::json!({
                    "type": "reasoning",
                    "text": text,
                });
                if *encrypted {
                    let mut details = Vec::new();
                    let mut detail = serde_json::json!({
                        "type": "reasoning.encrypted",
                    });
                    if let Some(fmt) = encryption_format {
                        detail["format"] = Value::String(fmt.clone());
                    }
                    if let Some(data) = encrypted_data {
                        detail["data"] = Value::String(data.clone());
                    }
                    details.push(detail);
                    part["metadata"] = serde_json::json!({
                        "openrouter": {
                            "reasoning_details": details,
                        }
                    });
                }
                parts.push(part);
            }
            ContentBlock::ToolUse {
                id,
                name,
                description,
                input,
                ..
            } => {
                // In OpenCode, tool use and result are combined in one part.
                // We emit the tool use here; the ToolResult will follow and complete it.
                // But since Hub separates them, we need to look ahead.
                // For simplicity in from_hub, we emit a complete tool part when we see ToolUse,
                // and skip the subsequent ToolResult (which has the output).
                // Actually, we need both. Let's emit a placeholder that gets merged.
                let mut part = serde_json::json!({
                    "type": "tool",
                    "tool": name,
                    "callID": id,
                    "state": {
                        "status": "pending",
                        "input": input,
                    }
                });
                if let Some(title) = description {
                    part["state"]["title"] = Value::String(title.clone());
                }
                parts.push(part);
            }
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                exit_code,
                is_error,
                status,
                duration_ms,
                title,
                truncated,
                ..
            } => {
                // Merge with the preceding ToolUse part if possible
                let output_text = content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let oc_status =
                    status
                        .as_deref()
                        .unwrap_or(if *is_error { "error" } else { "completed" });

                // Find and update the matching ToolUse part
                let mut merged = false;
                for existing in parts.iter_mut().rev() {
                    if existing.get("type").and_then(|v| v.as_str()) == Some("tool") {
                        let existing_call = existing
                            .get("callID")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if existing_call == tool_use_id {
                            existing["state"]["status"] = Value::String(oc_status.to_string());
                            existing["state"]["output"] = Value::String(output_text.clone());
                            if let Some(ec) = exit_code {
                                existing["state"]["metadata"] = serde_json::json!({
                                    "exit": ec,
                                    "truncated": truncated,
                                });
                            }
                            if let Some(t) = title {
                                existing["state"]["title"] = Value::String(t.clone());
                            }
                            if let (Some(dur), Some(start)) = (
                                duration_ms,
                                existing["state"]
                                    .get("time")
                                    .and_then(|t| t.get("start"))
                                    .and_then(|v| v.as_f64()),
                            ) {
                                existing["state"]["time"]["end"] =
                                    serde_json::json!(start + *dur as f64);
                            }
                            merged = true;
                            break;
                        }
                    }
                }

                if !merged {
                    // Standalone tool result without matching use
                    parts.push(serde_json::json!({
                        "type": "tool",
                        "tool": "unknown",
                        "callID": tool_use_id,
                        "state": {
                            "status": oc_status,
                            "output": output_text,
                        }
                    }));
                }
            }
            ContentBlock::Patch {
                path,
                hash_before,
                hash_after,
            } => {
                let mut part = serde_json::json!({
                    "type": "patch",
                    "path": path,
                });
                let mut hash = serde_json::Map::new();
                if let Some(before) = hash_before {
                    hash.insert("before".into(), Value::String(before.clone()));
                }
                if let Some(after) = hash_after {
                    hash.insert("after".into(), Value::String(after.clone()));
                }
                if !hash.is_empty() {
                    part["hash"] = Value::Object(hash);
                }
                parts.push(part);
            }
            ContentBlock::Image {
                media_type,
                encoding,
                data,
                source_url,
            } => {
                // OpenCode doesn't have native image part support.
                // Store images in _hub_images on the preceding text part for round-trip recovery.
                let img = serde_json::json!({
                    "media_type": media_type,
                    "encoding": encoding,
                    "data": data,
                    "source_url": source_url,
                });
                // Find the last text part and attach the image to it
                let mut attached = false;
                for existing in parts.iter_mut().rev() {
                    if existing.get("type").and_then(|v| v.as_str()) == Some("text") {
                        let images = existing
                            .as_object_mut()
                            .unwrap()
                            .entry("_hub_images")
                            .or_insert_with(|| serde_json::json!([]));
                        images.as_array_mut().unwrap().push(img.clone());
                        attached = true;
                        break;
                    }
                }
                if !attached {
                    // No preceding text part — create a placeholder text part with the image
                    parts.push(serde_json::json!({
                        "type": "text",
                        "text": "[image]",
                        "_hub_images": [img],
                    }));
                }
            }
        }
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(messages: &str, parts: &str) -> OpenCodeInput {
        OpenCodeInput {
            session_id: "ses_test_001".to_string(),
            messages: serde_json::from_str(messages).unwrap(),
            parts: serde_json::from_str(parts).unwrap(),
        }
    }

    #[test]
    fn test_basic_round_trip() {
        let messages = r#"[
            {
                "role": "user",
                "time": {"created": 1769002722116},
                "agent": "build",
                "model": {"providerID": "openrouter", "modelID": "google/gemini-3-flash-preview"}
            },
            {
                "role": "assistant",
                "time": {"created": 1769002722121, "completed": 1769002727109},
                "parentID": "msg_001",
                "modelID": "google/gemini-3-flash-preview",
                "providerID": "openrouter",
                "mode": "build",
                "agent": "build",
                "path": {"cwd": "/home/user/project", "root": "/"},
                "cost": 0.008043,
                "tokens": {"input": 12228, "output": 368, "reasoning": 275, "cache": {"read": 0, "write": 0}},
                "finish": "tool-calls"
            }
        ]"#;

        let parts = r#"[
            {"type": "text", "text": "Fix the bug"},
            {"type": "step-start", "text": ""},
            {"type": "reasoning", "text": "Let me analyze this..."},
            {"type": "text", "text": "I will check the code."},
            {"type": "tool", "tool": "bash", "callID": "call_001", "state": {"status": "completed", "input": {"command": "ls"}, "output": "/usr/bin\n/home", "time": {"start": 1769002727085, "end": 1769002727103}}},
            {"type": "step-finish", "reason": "tool-calls", "cost": 0.008043, "tokens": {"input": 12228, "output": 368, "reasoning": 275, "cache": {"read": 0, "write": 0}}, "text": ""}
        ]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        // Verify session header
        match &hub[0] {
            HubRecord::Session(s) => {
                assert_eq!(s.source_cli, "opencode");
                assert_eq!(s.session_id, "ses_test_001");
            }
            _ => panic!("Expected Session"),
        }

        // Verify user message
        match &hub[1] {
            HubRecord::Message(m) => {
                assert_eq!(m.role, "user");
                assert_eq!(m.content.len(), 1); // text part
            }
            _ => panic!("Expected Message"),
        }

        // Verify assistant message
        match &hub[2] {
            HubRecord::Message(m) => {
                assert_eq!(m.role, "assistant");
                assert!(m.completed_at.is_some());
                assert_eq!(m.metadata.cost, Some(0.008043));
                assert!(m.content.len() >= 4); // step-start, reasoning, text, tool_use, tool_result, step-finish
            }
            _ => panic!("Expected Message"),
        }

        // Convert back
        let output = from_hub(&hub).unwrap();
        assert_eq!(output.messages.len(), 2);
        assert!(output.parts.len() >= 4);

        // Verify round-trip of key fields
        // User message agent preserved
        assert_eq!(
            output.messages[0].get("agent").and_then(|v| v.as_str()),
            Some("build")
        );

        // Assistant cost preserved
        let back_cost = output.messages[1]
            .get("cost")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        assert!((back_cost - 0.008043).abs() < 0.000001);

        // Assistant tokens preserved
        let back_tokens = output.messages[1].get("tokens").unwrap();
        assert_eq!(
            back_tokens.get("input").and_then(|v| v.as_u64()),
            Some(12228)
        );

        // Dual timestamps preserved
        assert!(output.messages[1]["time"]["created"].as_f64().is_some());
        assert!(output.messages[1]["time"]["completed"].as_f64().is_some());
    }

    #[test]
    fn test_dual_timestamps_preserved() {
        let messages = r#"[{
            "role": "assistant",
            "time": {"created": 1769002722121, "completed": 1769002727109},
            "modelID": "test-model",
            "providerID": "test",
            "mode": "normal",
            "agent": "default",
            "path": {"cwd": "/tmp", "root": "/"},
            "cost": 0.0,
            "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "finish": "end_turn"
        }]"#;
        let parts = r#"[{"type": "step-start", "text": ""}, {"type": "text", "text": "Hello"}, {"type": "step-finish", "reason": "end_turn", "text": ""}]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        if let HubRecord::Message(ref m) = hub[1] {
            assert!(m.completed_at.is_some());
            // Verify created != completed
            assert_ne!(&m.timestamp, m.completed_at.as_ref().unwrap());
        }

        let output = from_hub(&hub).unwrap();
        let created = output.messages[0]["time"]["created"].as_f64().unwrap();
        let completed = output.messages[0]["time"]["completed"].as_f64().unwrap();
        assert!((created - 1769002722121.0).abs() < 1.0);
        assert!((completed - 1769002727109.0).abs() < 1.0);
    }

    #[test]
    fn test_step_boundaries_round_trip() {
        let messages = r#"[{
            "role": "assistant",
            "time": {"created": 1000000, "completed": 2000000},
            "modelID": "model",
            "providerID": "p",
            "mode": "build",
            "agent": "build",
            "path": {"cwd": "/tmp", "root": "/"},
            "cost": 0.01,
            "tokens": {"input": 100, "output": 50, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "finish": "end_turn"
        }]"#;
        let parts = r#"[
            {"type": "step-start", "text": ""},
            {"type": "text", "text": "Hello"},
            {"type": "step-finish", "reason": "end_turn", "cost": 0.01, "tokens": {"input": 100, "output": 50, "reasoning": 0, "cache": {"read": 0, "write": 0}}, "text": ""}
        ]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        if let HubRecord::Message(ref m) = hub[1] {
            // Should have step-start, text, step-finish
            let step_starts: Vec<_> = m
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::StepBoundary { boundary, .. } if boundary == "start"))
                .collect();
            let step_finishes: Vec<_> = m
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::StepBoundary { boundary, .. } if boundary == "finish"))
                .collect();
            assert_eq!(step_starts.len(), 1);
            assert_eq!(step_finishes.len(), 1);

            // Verify finish has reason
            if let ContentBlock::StepBoundary {
                finish_reason,
                cost,
                ..
            } = &step_finishes[0]
            {
                assert_eq!(finish_reason.as_deref(), Some("end_turn"));
                assert_eq!(*cost, Some(0.01));
            }
        }

        let output = from_hub(&hub).unwrap();
        let step_finish = output
            .parts
            .iter()
            .find(|p| p.get("type").and_then(|v| v.as_str()) == Some("step-finish"))
            .unwrap();
        assert_eq!(
            step_finish.get("reason").and_then(|v| v.as_str()),
            Some("end_turn")
        );
    }

    #[test]
    fn test_tool_round_trip() {
        let messages = r#"[{
            "role": "assistant",
            "time": {"created": 1000000, "completed": 2000000},
            "modelID": "model",
            "providerID": "p",
            "mode": "build",
            "agent": "build",
            "path": {"cwd": "/tmp", "root": "/"},
            "cost": 0.0,
            "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "finish": "tool-calls"
        }]"#;
        let parts = r#"[
            {"type": "step-start", "text": ""},
            {"type": "tool", "tool": "bash", "callID": "call_123", "state": {"status": "completed", "input": {"command": "echo hello"}, "output": "hello", "title": "Run echo", "metadata": {"exit": 0, "truncated": false}, "time": {"start": 1000, "end": 1500}}, "text": ""},
            {"type": "step-finish", "reason": "tool-calls", "text": ""}
        ]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        if let HubRecord::Message(ref m) = hub[1] {
            let tool_uses: Vec<_> = m
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                .collect();
            assert_eq!(tool_uses.len(), 1);
            if let ContentBlock::ToolUse { name, id, .. } = tool_uses[0] {
                assert_eq!(name, "bash");
                assert_eq!(id, "call_123");
            }
        }

        let output = from_hub(&hub).unwrap();
        let tool_part = output
            .parts
            .iter()
            .find(|p| p.get("type").and_then(|v| v.as_str()) == Some("tool"))
            .unwrap();
        assert_eq!(tool_part.get("tool").and_then(|v| v.as_str()), Some("bash"));
        assert_eq!(
            tool_part["state"].get("status").and_then(|v| v.as_str()),
            Some("completed")
        );
        assert_eq!(
            tool_part["state"].get("output").and_then(|v| v.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn test_encrypted_reasoning_round_trip() {
        let messages = r#"[{
            "role": "assistant",
            "time": {"created": 1000000, "completed": 2000000},
            "modelID": "model",
            "providerID": "p",
            "mode": "build",
            "agent": "build",
            "path": {"cwd": "/tmp", "root": "/"},
            "cost": 0.0,
            "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "finish": "end_turn"
        }]"#;
        let parts = r#"[
            {"type": "step-start", "text": ""},
            {"type": "reasoning", "text": "[REDACTED]", "metadata": {"openrouter": {"reasoning_details": [{"type": "reasoning.encrypted", "data": "encrypted_blob_here", "format": "google-gemini-v1", "index": 0}]}}},
            {"type": "text", "text": "Done"},
            {"type": "step-finish", "reason": "end_turn", "text": ""}
        ]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        if let HubRecord::Message(ref m) = hub[1] {
            let thinking: Vec<_> = m
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::Thinking { .. }))
                .collect();
            assert_eq!(thinking.len(), 1);
            if let ContentBlock::Thinking {
                encrypted,
                encryption_format,
                encrypted_data,
                ..
            } = thinking[0]
            {
                assert!(encrypted);
                assert_eq!(encryption_format.as_deref(), Some("google-gemini-v1"));
                assert_eq!(encrypted_data.as_deref(), Some("encrypted_blob_here"));
            }
        }

        let output = from_hub(&hub).unwrap();
        let reasoning_part = output
            .parts
            .iter()
            .find(|p| p.get("type").and_then(|v| v.as_str()) == Some("reasoning"))
            .unwrap();
        let details = &reasoning_part["metadata"]["openrouter"]["reasoning_details"];
        assert_eq!(
            details[0].get("type").and_then(|v| v.as_str()),
            Some("reasoning.encrypted")
        );
        assert_eq!(
            details[0].get("data").and_then(|v| v.as_str()),
            Some("encrypted_blob_here")
        );
    }

    #[test]
    fn test_patch_round_trip() {
        let messages = r#"[{
            "role": "assistant",
            "time": {"created": 1000000, "completed": 2000000},
            "modelID": "model",
            "providerID": "p",
            "mode": "build",
            "agent": "build",
            "path": {"cwd": "/tmp", "root": "/"},
            "cost": 0.0,
            "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "finish": "end_turn"
        }]"#;
        let parts = r#"[
            {"type": "step-start", "text": ""},
            {"type": "patch", "path": "/home/user/project/src/main.rs", "hash": {"before": "aaa111", "after": "bbb222"}},
            {"type": "step-finish", "reason": "end_turn", "text": ""}
        ]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        if let HubRecord::Message(ref m) = hub[1] {
            let patches: Vec<_> = m
                .content
                .iter()
                .filter(|b| matches!(b, ContentBlock::Patch { .. }))
                .collect();
            assert_eq!(patches.len(), 1);
            if let ContentBlock::Patch {
                path,
                hash_before,
                hash_after,
            } = patches[0]
            {
                assert_eq!(path, "/home/user/project/src/main.rs");
                assert_eq!(hash_before.as_deref(), Some("aaa111"));
                assert_eq!(hash_after.as_deref(), Some("bbb222"));
            }
        }

        let output = from_hub(&hub).unwrap();
        let patch_part = output
            .parts
            .iter()
            .find(|p| p.get("type").and_then(|v| v.as_str()) == Some("patch"))
            .unwrap();
        assert_eq!(patch_part["hash"]["before"].as_str(), Some("aaa111"));
        assert_eq!(patch_part["hash"]["after"].as_str(), Some("bbb222"));
    }

    #[test]
    fn test_cost_preserved() {
        let messages = r#"[{
            "role": "assistant",
            "time": {"created": 1000000, "completed": 2000000},
            "modelID": "model",
            "providerID": "p",
            "mode": "normal",
            "agent": "default",
            "path": {"cwd": "/tmp", "root": "/"},
            "cost": 0.003456,
            "tokens": {"input": 1500, "output": 800, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "finish": "end_turn"
        }]"#;
        let parts = r#"[{"type": "step-start", "text": ""}, {"type": "text", "text": "Hello"}, {"type": "step-finish", "reason": "end_turn", "text": ""}]"#;

        let input = make_input(messages, parts);
        let hub = to_hub(&input).unwrap();

        if let HubRecord::Message(ref m) = hub[1] {
            assert_eq!(m.metadata.cost, Some(0.003456));
        }

        let output = from_hub(&hub).unwrap();
        let cost = output.messages[0]
            .get("cost")
            .and_then(|v| v.as_f64())
            .unwrap();
        assert!((cost - 0.003456).abs() < 0.000001);
    }

    #[test]
    fn test_image_preserved_in_extensions() {
        let records = vec![
            HubRecord::Session(SessionHeader {
                ucf_version: UCF_VERSION.to_string(),
                session_id: "img-test".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:01Z".into(),
                source_cli: "claude".into(),
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
                    ContentBlock::Text {
                        text: "Here's the image:".into(),
                    },
                    ContentBlock::Image {
                        media_type: "image/png".into(),
                        encoding: "base64".into(),
                        data: "iVBORw0KGgoAAAANSUhEUg==".into(),
                        source_url: None,
                    },
                ],
                metadata: MessageMetadata::default(),
                extensions: serde_json::json!({}),
            }),
        ];

        let output = from_hub(&records).unwrap();
        // Text should still be there (OpenCode parts use "text" not "content")
        let has_text = output.parts.iter().any(|p| {
            p.get("type").and_then(|t| t.as_str()) == Some("text")
                && p.get("text")
                    .and_then(|c| c.as_str())
                    .is_some_and(|s| s.contains("image"))
        });
        assert!(has_text, "text content should be preserved");

        // Image should be stored in _hub_images on the nearest text part
        let has_image = output.parts.iter().any(|p| p.get("_hub_images").is_some());
        assert!(
            has_image,
            "image should be preserved in _hub_images, not silently dropped"
        );
    }

    #[test]
    fn test_image_round_trip_via_opencode() {
        // hub -> opencode -> hub: images should survive via _hub_images extensions
        let records = vec![
            HubRecord::Session(SessionHeader {
                ucf_version: UCF_VERSION.to_string(),
                session_id: "img-rt".into(),
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-01T00:00:01Z".into(),
                source_cli: "claude".into(),
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
                    ContentBlock::Text {
                        text: "Here's the logo:".into(),
                    },
                    ContentBlock::Image {
                        media_type: "image/png".into(),
                        encoding: "base64".into(),
                        data: "iVBORw0KGgoAAAANSUhEUg==".into(),
                        source_url: None,
                    },
                ],
                metadata: MessageMetadata::default(),
                extensions: serde_json::json!({}),
            }),
        ];
        let oc = from_hub(&records).unwrap();
        let input = OpenCodeInput {
            session_id: "img-rt".into(),
            messages: oc.messages,
            parts: oc.parts,
        };
        let back = to_hub(&input).unwrap();
        let has_image = back.iter().any(|r| {
            if let HubRecord::Message(m) = r {
                m.content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Image { .. }))
            } else {
                false
            }
        });
        assert!(
            has_image,
            "image should survive opencode round-trip via _hub_images"
        );
    }
}
