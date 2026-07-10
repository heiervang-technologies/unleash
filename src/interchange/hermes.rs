use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;
use std::collections::HashMap;

/// Output from Hub → Hermes conversion: rows ready to INSERT into state.db.
pub struct HermesOutput {
    pub session: HermesSession,
    pub messages: Vec<HermesMessage>,
}

pub struct HermesSession {
    pub id: String,
    pub source: String,
    pub model: Option<String>,
    pub title: Option<String>,
    pub started_at: f64,
    pub ended_at: f64,
    pub message_count: usize,
}

pub struct HermesMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub timestamp: f64,
    /// Plaintext `Thinking` blocks, mapped to Hermes' native `reasoning`
    /// column so reasoning survives hub → Hermes injection instead of being
    /// dropped.
    pub reasoning: Option<String>,
    /// Encrypted/redacted `Thinking` blocks, serialized as a JSON array and
    /// mapped to Hermes' `reasoning_details` column. This is what keeps an
    /// *encrypted* reasoning-only turn from vanishing: its payload lives in
    /// `encrypted_data`, not `text`, so the plaintext `reasoning` column stays
    /// empty and cannot carry it.
    pub reasoning_details: Option<String>,
    /// From a `StepBoundary` finish block, mapped to Hermes' `finish_reason`.
    pub finish_reason: Option<String>,
    /// From a `StepBoundary` finish block's token usage.
    pub token_count: Option<i64>,
}

// ── Timestamp helpers (no chrono dep) ───────────────────────────────────────

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn epoch_to_iso(secs: u64) -> String {
    let mut days = (secs / 86400) as i64;
    let time_s = secs % 86400;
    let h = time_s / 3600;
    let m = (time_s % 3600) / 60;
    let s = time_s % 60;

    let mut year = 1970i64;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        year += 1;
    }
    let month_days: [i64; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1i64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Parse a subset of RFC 3339 / ISO 8601 to seconds-since-epoch (truncates sub-seconds).
fn iso_to_epoch(s: &str) -> Option<f64> {
    // Accepts: "YYYY-MM-DDTHH:MM:SS[.fff][Z|+00:00|±HH:MM]"
    let s = s.trim();
    // The fixed-offset byte slices below assume ASCII; a multibyte char in a
    // corrupt/hand-edited timestamp would otherwise panic (not a char boundary)
    // and abort the whole injection. A valid ISO-8601 timestamp is always ASCII.
    if s.len() < 19 || !s.is_ascii() {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let hour: i64 = s[11..13].parse().ok()?;
    let min: i64 = s[14..16].parse().ok()?;
    let sec: i64 = s[17..19].parse().ok()?;

    // Days from 1970-01-01 to year-01-01
    let mut days = 0i64;
    for y in 1970..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    // Days from year-01-01 to month-01
    let month_days: [i64; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for &md in month_days.iter().take((month - 1) as usize) {
        days += md;
    }
    days += day - 1;

    // Timezone offset (we only handle Z and ±HH:MM here)
    let mut offset_secs = 0i64;
    if s.len() > 19 {
        let tz_part = &s[19..];
        let tz_part = tz_part.trim_start_matches('.');
        // Skip sub-seconds digit sequence
        let tz_part = tz_part.trim_start_matches(|c: char| c.is_ascii_digit());
        if tz_part.starts_with('+') || tz_part.starts_with('-') {
            let sign: i64 = if tz_part.starts_with('+') { 1 } else { -1 };
            let tz = &tz_part[1..];
            if tz.len() >= 5 {
                let oh: i64 = tz[0..2].parse().unwrap_or(0);
                let om: i64 = tz[3..5].parse().unwrap_or(0);
                offset_secs = sign * (oh * 3600 + om * 60);
            }
        }
    }

    let epoch = days * 86400 + hour * 3600 + min * 60 + sec - offset_secs;
    Some(epoch as f64)
}

// ── to_hub ───────────────────────────────────────────────────────────────────

/// Convert a Hermes exported session JSON (from `hermes sessions export --session-id <id> -`)
/// into Hub records.
pub fn to_hub(json: &str) -> Result<Vec<HubRecord>, ConvertError> {
    let session: Value = serde_json::from_str(json)?;
    let mut records: Vec<HubRecord> = Vec::new();

    let session_id = session["id"].as_str().unwrap_or("unknown").to_string();
    let model = session["model"].as_str().map(|s| s.to_string());
    let title = session["title"].as_str().map(|s| s.to_string());

    let started_at = session["started_at"].as_f64().unwrap_or(0.0);
    let ended_at = session["ended_at"].as_f64().unwrap_or(started_at);

    records.push(HubRecord::Session(SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id: session_id.clone(),
        created_at: epoch_to_iso(started_at as u64),
        updated_at: epoch_to_iso(ended_at as u64),
        source_cli: "hermes".to_string(),
        source_version: String::new(),
        project: None,
        model: model.clone(),
        title: title.clone(),
        slug: None,
        parent_session_id: session["parent_session_id"].as_str().map(|s| s.to_string()),
        extensions: Value::Object(Default::default()),
    }));

    let messages = session["messages"].as_array().cloned().unwrap_or_default();

    let mut i = 0;
    while i < messages.len() {
        let msg = &messages[i];
        let role = msg["role"].as_str().unwrap_or("user");
        let ts = msg["timestamp"].as_f64().unwrap_or(started_at);
        let msg_id = msg["id"]
            .as_u64()
            .map(|n| n.to_string())
            .unwrap_or_default();

        if role == "tool" {
            // Orphaned tool result — emit as user ToolResult block
            let content = msg["content"].as_str().unwrap_or("").to_string();
            let tool_use_id = msg["tool_call_id"].as_str().unwrap_or(&msg_id).to_string();
            let tool_name = msg["tool_name"].as_str().map(String::from);
            records.push(HubRecord::Message(HubMessage {
                id: msg_id,
                api_message_id: None,
                parent_id: None,
                timestamp: epoch_to_iso(ts as u64),
                completed_at: None,
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: vec![ContentBlock::Text { text: content }],
                    is_error: false,
                    exit_code: None,
                    interrupted: false,
                    status: None,
                    duration_ms: None,
                    title: None,
                    truncated: false,
                }],
                metadata: MessageMetadata::default(),
                extensions: hermes_tool_name_extension(&tool_use_id, tool_name.as_deref()),
            }));
            i += 1;
            continue;
        }

        let content_text = msg["content"].as_str().unwrap_or("").to_string();
        let mut blocks: Vec<ContentBlock> = Vec::new();
        if !content_text.is_empty() {
            blocks.push(ContentBlock::Text { text: content_text });
        }

        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        let mut result_tool_names: HashMap<String, String> = HashMap::new();
        let mut consumed_tool_msgs = 0usize;

        if role == "assistant" {
            if let Some(calls) = msg["tool_calls"].as_array() {
                for call in calls {
                    let call_id = call["id"].as_str().unwrap_or("").to_string();
                    let fn_name = call["function"]["name"]
                        .as_str()
                        .or_else(|| call["name"].as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let raw_args = call["function"]["arguments"]
                        .as_str()
                        .or_else(|| call["arguments"].as_str())
                        .unwrap_or("{}");
                    let input: Value =
                        serde_json::from_str(raw_args).unwrap_or(Value::Object(Default::default()));
                    blocks.push(ContentBlock::ToolUse {
                        id: call_id.clone(),
                        name: fn_name,
                        display_name: None,
                        description: None,
                        input,
                    });
                }

                // Collect every immediately-following tool row. Matched rows
                // become paired ToolResult blocks; extra rows are preserved as
                // orphan ToolResult blocks instead of being consumed and lost.
                let mut j = i + 1;
                while j < messages.len() && messages[j]["role"].as_str() == Some("tool") {
                    let next = &messages[j];
                    let tool_use_id = next["tool_call_id"]
                        .as_str()
                        .map(String::from)
                        .unwrap_or_else(|| {
                            next["id"]
                                .as_u64()
                                .map(|n| n.to_string())
                                .unwrap_or_default()
                        });
                    let res = next["content"].as_str().unwrap_or("").to_string();
                    result_blocks.push(ContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ContentBlock::Text { text: res }],
                        is_error: false,
                        exit_code: None,
                        interrupted: false,
                        status: None,
                        duration_ms: None,
                        title: None,
                        truncated: false,
                    });
                    if let Some(name) = next["tool_name"].as_str() {
                        result_tool_names.insert(tool_use_id.clone(), name.to_string());
                    }
                    consumed_tool_msgs += 1;
                    j += 1;
                }
            }
        }

        if !blocks.is_empty() || role == "user" {
            records.push(HubRecord::Message(HubMessage {
                id: msg_id.clone(),
                api_message_id: None,
                parent_id: None,
                timestamp: epoch_to_iso(ts as u64),
                completed_at: None,
                role: if role == "assistant" {
                    "assistant".to_string()
                } else {
                    "user".to_string()
                },
                content: blocks,
                metadata: MessageMetadata {
                    model: model.clone(),
                    ..MessageMetadata::default()
                },
                extensions: Value::Object(Default::default()),
            }));
        }

        if !result_blocks.is_empty() {
            records.push(HubRecord::Message(HubMessage {
                id: format!("{msg_id}_results"),
                api_message_id: None,
                parent_id: Some(msg_id),
                timestamp: epoch_to_iso(ts as u64),
                completed_at: None,
                role: "user".to_string(),
                content: result_blocks,
                metadata: MessageMetadata::default(),
                extensions: hermes_tool_names_extension(&result_tool_names),
            }));
        }

        i += 1 + consumed_tool_msgs;
    }

    Ok(records)
}

// ── from_hub ─────────────────────────────────────────────────────────────────

/// Convert Hub records into Hermes SQLite row structs ready for INSERT.
pub fn from_hub(records: &[HubRecord]) -> Result<HermesOutput, ConvertError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let (session_id, model, title, created_at, updated_at) = records
        .iter()
        .find_map(|r| {
            if let HubRecord::Session(h) = r {
                let ca = iso_to_epoch(&h.created_at).unwrap_or(now);
                let ua = iso_to_epoch(&h.updated_at).unwrap_or(now);
                Some((
                    h.session_id.clone(),
                    h.model.clone(),
                    h.title.clone(),
                    ca,
                    ua,
                ))
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            let id = format!("unleash_import_{}", now as u64);
            (id, None, None, now, now)
        });

    let mut messages: Vec<HermesMessage> = Vec::new();
    let mut tool_names_by_id: HashMap<String, String> = HashMap::new();

    for record in records {
        let HubRecord::Message(msg) = record else {
            continue;
        };

        let ts = iso_to_epoch(&msg.timestamp).unwrap_or(now);

        // ToolUse blocks → tool_calls JSON
        let tool_use_blocks: Vec<&ContentBlock> = msg
            .content
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
            .collect();

        let tool_calls_json: Option<String> = if !tool_use_blocks.is_empty() {
            let calls: Vec<Value> = tool_use_blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::ToolUse {
                        id, name, input, ..
                    } = b
                    {
                        tool_names_by_id.insert(id.clone(), name.clone());
                        Some(serde_json::json!({
                            "id": id,
                            "call_id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input).unwrap_or_default()
                            }
                        }))
                    } else {
                        None
                    }
                })
                .collect();
            Some(serde_json::to_string(&calls).unwrap_or_default())
        } else {
            None
        };

        // Content: Text verbatim, plus textual placeholders for blocks Hermes
        // has no native column for (Image, Patch) so they are not silently
        // dropped on injection.
        let text_content: String = msg
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } if !text.is_empty() => Some(text.clone()),
                ContentBlock::Image {
                    media_type, data, ..
                } => Some(format!("[image: {} ({} bytes)]", media_type, data.len())),
                ContentBlock::Patch { path, .. } => Some(format!("[patch: {path}]")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Thinking → Hermes' reasoning columns. Plaintext goes to `reasoning`;
        // encrypted/redacted reasoning (whose payload is in `encrypted_data`,
        // not `text`) goes to `reasoning_details` as JSON. Capturing BOTH is
        // what keeps a reasoning-only assistant turn from vanishing — including
        // the encrypted case, where `text` is empty so `reasoning` alone would
        // stay None and the emit guard below would drop the turn.
        let mut reasoning_texts: Vec<&str> = Vec::new();
        let mut encrypted_details: Vec<Value> = Vec::new();
        for block in &msg.content {
            if let ContentBlock::Thinking {
                text,
                encrypted,
                encryption_format,
                encrypted_data,
                ..
            } = block
            {
                if !text.is_empty() {
                    reasoning_texts.push(text.as_str());
                }
                if *encrypted {
                    let mut detail = serde_json::Map::new();
                    detail.insert("type".into(), Value::String("reasoning.encrypted".into()));
                    if let Some(data) = encrypted_data {
                        detail.insert("data".into(), Value::String(data.clone()));
                    }
                    if let Some(fmt) = encryption_format {
                        detail.insert("format".into(), Value::String(fmt.clone()));
                    }
                    encrypted_details.push(Value::Object(detail));
                }
            }
        }
        let reasoning: Option<String> =
            (!reasoning_texts.is_empty()).then(|| reasoning_texts.join("\n"));
        let reasoning_details: Option<String> =
            (!encrypted_details.is_empty()).then(|| Value::Array(encrypted_details).to_string());

        // StepBoundary finish → native finish_reason / token_count columns.
        let (finish_reason, token_count) = msg
            .content
            .iter()
            .find_map(|b| match b {
                ContentBlock::StepBoundary {
                    boundary,
                    finish_reason,
                    tokens,
                    ..
                } if boundary == "finish" => Some((
                    finish_reason.clone(),
                    tokens
                        .as_ref()
                        .map(|t| t.total.max(t.output) as i64)
                        .filter(|&n| n > 0),
                )),
                _ => None,
            })
            .unwrap_or((None, None));

        // Emit primary message. Note reasoning_details in the guard: an
        // encrypted reasoning-only turn has empty text, no tool calls, and
        // empty `reasoning`, so without this it would be dropped — the exact
        // headline bug for the encrypted case.
        if !text_content.is_empty()
            || tool_calls_json.is_some()
            || reasoning.is_some()
            || reasoning_details.is_some()
        {
            messages.push(HermesMessage {
                role: msg.role.clone(),
                content: if text_content.is_empty() {
                    None
                } else {
                    Some(text_content)
                },
                tool_calls: tool_calls_json,
                tool_call_id: None,
                tool_name: None,
                timestamp: ts,
                reasoning,
                reasoning_details,
                finish_reason,
                token_count,
            });
        }

        // ToolResult blocks → separate tool-role rows
        for block in &msg.content {
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } = block
            {
                let result_text = content
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
                messages.push(HermesMessage {
                    role: "tool".to_string(),
                    content: Some(result_text),
                    tool_calls: None,
                    tool_call_id: Some(tool_use_id.clone()),
                    tool_name: hermes_tool_name_for_result(&msg.extensions, tool_use_id)
                        .or_else(|| tool_names_by_id.get(tool_use_id).cloned()),
                    timestamp: ts,
                    reasoning: None,
                    reasoning_details: None,
                    finish_reason: None,
                    token_count: None,
                });
            }
        }
    }

    let message_count = messages.len();
    Ok(HermesOutput {
        session: HermesSession {
            id: session_id,
            source: "unleash_import".to_string(),
            model,
            title,
            started_at: created_at,
            ended_at: updated_at,
            message_count,
        },
        messages,
    })
}

fn hermes_tool_name_extension(tool_use_id: &str, tool_name: Option<&str>) -> Value {
    let mut names = HashMap::new();
    if let Some(name) = tool_name {
        names.insert(tool_use_id.to_string(), name.to_string());
    }
    hermes_tool_names_extension(&names)
}

fn hermes_tool_names_extension(names: &HashMap<String, String>) -> Value {
    if names.is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::json!({
        "hermes": {
            "tool_names": names,
        }
    })
}

fn hermes_tool_name_for_result(ext: &Value, tool_use_id: &str) -> Option<String> {
    ext.get("hermes")
        .and_then(|h| h.get("tool_names"))
        .and_then(|names| names.get(tool_use_id))
        .and_then(|name| name.as_str())
        .map(String::from)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_to_iso_known_date() {
        // 2026-05-24T00:00:00Z = 1779580800 seconds (verified via Python datetime)
        assert_eq!(epoch_to_iso(1779580800), "2026-05-24T00:00:00Z");
    }

    #[test]
    fn iso_to_epoch_roundtrip() {
        let epoch = 1779580800u64;
        let iso = epoch_to_iso(epoch);
        let back = iso_to_epoch(&iso).expect("parse failed");
        assert_eq!(back as u64, epoch);
    }

    #[test]
    fn iso_to_epoch_with_offset() {
        // 2026-05-24T02:00:00+02:00 = 2026-05-24T00:00:00Z = 1779580800
        let epoch = iso_to_epoch("2026-05-24T02:00:00+02:00").unwrap();
        assert_eq!(epoch as u64, 1779580800);
    }

    #[test]
    fn iso_to_epoch_multibyte_is_none_not_panic() {
        // A multibyte char inside the fixed-offset region used to panic the byte
        // slices (not a char boundary), aborting injection. It must now return
        // None instead. The 'é' sits where s[8..10] would split it.
        assert!(iso_to_epoch("2026-06-é7T12:00:00Z").is_none());
        // A multibyte char in the sub-seconds / tz tail must not panic either.
        assert!(iso_to_epoch("2026-06-27T12:00:00.5世Z").is_none());
        // Valid ASCII timestamps still parse.
        assert!(iso_to_epoch("2026-06-27T12:00:00Z").is_some());
    }

    #[test]
    fn to_hub_simple_conversation() {
        let json = serde_json::json!({
            "id": "test_session_001",
            "source": "claude",
            "model": "claude-sonnet",
            "title": "Test session",
            "started_at": 1779321600.0,
            "ended_at": 1779321700.0,
            "parent_session_id": null,
            "messages": [
                {
                    "id": 1,
                    "role": "user",
                    "content": "Hello, world!",
                    "tool_calls": null,
                    "tool_call_id": null,
                    "tool_name": null,
                    "timestamp": 1779321601.0
                },
                {
                    "id": 2,
                    "role": "assistant",
                    "content": "Hi there!",
                    "tool_calls": null,
                    "tool_call_id": null,
                    "tool_name": null,
                    "timestamp": 1779321650.0
                }
            ]
        })
        .to_string();

        let records = to_hub(&json).unwrap();
        assert_eq!(records.len(), 3); // session + 2 messages
        assert!(matches!(&records[0], HubRecord::Session(_)));
        if let HubRecord::Session(s) = &records[0] {
            assert_eq!(s.session_id, "test_session_001");
            assert_eq!(s.source_cli, "hermes");
            assert_eq!(s.title.as_deref(), Some("Test session"));
        }
        if let HubRecord::Message(m) = &records[1] {
            assert_eq!(m.role, "user");
            assert!(
                matches!(&m.content[0], ContentBlock::Text { text } if text == "Hello, world!")
            );
        }
    }

    #[test]
    fn to_hub_tool_call_pairs() {
        let json = serde_json::json!({
            "id": "tool_session",
            "source": "claude",
            "model": "claude-sonnet",
            "title": null,
            "started_at": 1779321600.0,
            "ended_at": 1779321700.0,
            "parent_session_id": null,
            "messages": [
                {
                    "id": 1,
                    "role": "user",
                    "content": "Run ls",
                    "tool_calls": null,
                    "tool_call_id": null,
                    "tool_name": null,
                    "timestamp": 1779321601.0
                },
                {
                    "id": 2,
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "bash", "arguments": "{\"cmd\":\"ls\"}"}}],
                    "tool_call_id": null,
                    "tool_name": null,
                    "timestamp": 1779321620.0
                },
                {
                    "id": 3,
                    "role": "tool",
                    "content": "file1.txt\nfile2.txt",
                    "tool_calls": null,
                    "tool_call_id": "call_1",
                    "tool_name": "bash",
                    "timestamp": 1779321630.0
                }
            ]
        })
        .to_string();

        let records = to_hub(&json).unwrap();
        // Expect: session, user msg, assistant msg (with ToolUse), user msg (with ToolResult)
        assert_eq!(records.len(), 4);
        if let HubRecord::Message(m) = &records[2] {
            assert_eq!(m.role, "assistant");
            assert!(m
                .content
                .iter()
                .any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name == "bash")));
        }
        if let HubRecord::Message(m) = &records[3] {
            assert_eq!(m.role, "user");
            assert!(m.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_1")));
        }
    }

    #[test]
    fn to_hub_preserves_extra_consecutive_tool_rows() {
        let json = serde_json::json!({
            "id": "tool_session",
            "source": "claude",
            "model": "claude-sonnet",
            "title": null,
            "started_at": 1779321600.0,
            "ended_at": 1779321700.0,
            "parent_session_id": null,
            "messages": [
                {
                    "id": 1,
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "bash", "arguments": "{\"cmd\":\"ls\"}"}}],
                    "tool_call_id": null,
                    "tool_name": null,
                    "timestamp": 1779321620.0
                },
                {
                    "id": 2,
                    "role": "tool",
                    "content": "matched",
                    "tool_calls": null,
                    "tool_call_id": "call_1",
                    "tool_name": "bash",
                    "timestamp": 1779321630.0
                },
                {
                    "id": 3,
                    "role": "tool",
                    "content": "extra",
                    "tool_calls": null,
                    "tool_call_id": "call_extra",
                    "tool_name": "grep",
                    "timestamp": 1779321631.0
                }
            ]
        })
        .to_string();

        let records = to_hub(&json).unwrap();
        let result_msg = records
            .iter()
            .filter_map(|r| match r {
                HubRecord::Message(m) if m.role == "user" => Some(m),
                _ => None,
            })
            .find(|m| {
                m.content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolResult { .. }))
            })
            .expect("missing tool result message");
        let result_ids: Vec<_> = result_msg
            .content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(result_ids, vec!["call_1", "call_extra"]);
        assert_eq!(
            hermes_tool_name_for_result(&result_msg.extensions, "call_extra").as_deref(),
            Some("grep")
        );
    }

    #[test]
    fn from_hub_roundtrip_basic() {
        let json = serde_json::json!({
            "id": "rt_session",
            "source": "claude",
            "model": "claude-sonnet",
            "title": "Roundtrip test",
            "started_at": 1779321600.0,
            "ended_at": 1779321700.0,
            "parent_session_id": null,
            "messages": [
                {"id": 1, "role": "user", "content": "Hello", "tool_calls": null, "tool_call_id": null, "tool_name": null, "timestamp": 1779321601.0},
                {"id": 2, "role": "assistant", "content": "Hi", "tool_calls": null, "tool_call_id": null, "tool_name": null, "timestamp": 1779321650.0}
            ]
        })
        .to_string();

        let hub = to_hub(&json).unwrap();
        let out = from_hub(&hub).unwrap();
        assert_eq!(out.session.id, "rt_session");
        assert_eq!(out.session.title.as_deref(), Some("Roundtrip test"));
        assert_eq!(out.messages.len(), 2);
        assert_eq!(out.messages[0].role, "user");
        assert_eq!(out.messages[1].role, "assistant");
    }

    #[test]
    fn from_hub_preserves_tool_result_names() {
        let json = serde_json::json!({
            "id": "tool_session",
            "source": "claude",
            "model": "claude-sonnet",
            "title": null,
            "started_at": 1779321600.0,
            "ended_at": 1779321700.0,
            "parent_session_id": null,
            "messages": [
                {
                    "id": 1,
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "bash", "arguments": "{\"cmd\":\"ls\"}"}}],
                    "tool_call_id": null,
                    "tool_name": null,
                    "timestamp": 1779321620.0
                },
                {
                    "id": 2,
                    "role": "tool",
                    "content": "matched",
                    "tool_calls": null,
                    "tool_call_id": "call_1",
                    "tool_name": "bash",
                    "timestamp": 1779321630.0
                },
                {
                    "id": 3,
                    "role": "tool",
                    "content": "extra",
                    "tool_calls": null,
                    "tool_call_id": "call_extra",
                    "tool_name": "grep",
                    "timestamp": 1779321631.0
                }
            ]
        })
        .to_string();

        let hub = to_hub(&json).unwrap();
        let out = from_hub(&hub).unwrap();
        let tools: Vec<_> = out.messages.iter().filter(|m| m.role == "tool").collect();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(tools[0].tool_name.as_deref(), Some("bash"));
        assert_eq!(tools[1].tool_call_id.as_deref(), Some("call_extra"));
        assert_eq!(tools[1].tool_name.as_deref(), Some("grep"));
    }

    #[test]
    fn from_hub_preserves_reasoning_image_patch_and_step() {
        let session = HubRecord::Session(SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: "s".into(),
            created_at: "2026-06-01T00:00:00Z".into(),
            updated_at: "2026-06-01T00:01:00Z".into(),
            source_cli: "codex".into(),
            source_version: String::new(),
            project: None,
            model: Some("gpt".into()),
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: Value::Object(Default::default()),
        });

        // A reasoning-only assistant turn — no text, no tool calls. Previously
        // dropped entirely; must now be emitted with the reasoning preserved.
        let reasoning_only = HubRecord::Message(HubMessage {
            id: "m1".into(),
            api_message_id: None,
            parent_id: None,
            timestamp: "2026-06-01T00:00:10Z".into(),
            completed_at: None,
            role: "assistant".into(),
            content: vec![ContentBlock::Thinking {
                text: "let me think".into(),
                subject: None,
                description: None,
                signature: None,
                encrypted: false,
                encryption_format: None,
                encrypted_data: None,
                timestamp: None,
            }],
            metadata: MessageMetadata::default(),
            extensions: Value::Object(Default::default()),
        });

        // A mixed turn exercising Image, Patch, and a StepBoundary finish.
        let mixed = HubRecord::Message(HubMessage {
            id: "m2".into(),
            api_message_id: None,
            parent_id: None,
            timestamp: "2026-06-01T00:00:20Z".into(),
            completed_at: None,
            role: "assistant".into(),
            content: vec![
                ContentBlock::Text {
                    text: "done".into(),
                },
                ContentBlock::Image {
                    media_type: "image/png".into(),
                    encoding: "base64".into(),
                    data: "AAAA".into(),
                    source_url: None,
                },
                ContentBlock::Patch {
                    path: "src/x.rs".into(),
                    hash_before: None,
                    hash_after: None,
                },
                ContentBlock::StepBoundary {
                    boundary: "finish".into(),
                    snapshot: None,
                    finish_reason: Some("stop".into()),
                    cost: None,
                    tokens: Some(TokenUsage {
                        input: 10,
                        output: 20,
                        cache_creation: 0,
                        cache_read: 0,
                        reasoning: 0,
                        tool: 0,
                        total: 30,
                    }),
                },
            ],
            metadata: MessageMetadata::default(),
            extensions: Value::Object(Default::default()),
        });

        let out = from_hub(&[session, reasoning_only, mixed]).unwrap();
        let assistants: Vec<_> = out
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .collect();
        assert_eq!(
            assistants.len(),
            2,
            "reasoning-only turn must still be emitted"
        );

        assert_eq!(assistants[0].reasoning.as_deref(), Some("let me think"));
        assert!(
            assistants[0].content.is_none(),
            "reasoning-only turn has no textual content"
        );

        let mixed_out = assistants[1];
        let content = mixed_out.content.as_deref().unwrap();
        assert!(content.contains("done"));
        assert!(
            content.contains("[image:"),
            "image not preserved: {content}"
        );
        assert!(
            content.contains("[patch: src/x.rs]"),
            "patch not preserved: {content}"
        );
        assert_eq!(mixed_out.finish_reason.as_deref(), Some("stop"));
        assert_eq!(mixed_out.token_count, Some(30));
    }

    #[test]
    fn encrypted_reasoning_only_turn_survives() {
        // The headline bug's encrypted variant: an assistant turn whose only
        // content is an ENCRYPTED thinking block. Its payload is in
        // `encrypted_data`, not `text`, so a text-only extractor leaves
        // `reasoning` empty and the turn was being dropped. It must now survive
        // via `reasoning_details`.
        let session = HubRecord::Session(SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: "s".into(),
            created_at: "2026-06-01T00:00:00Z".into(),
            updated_at: "2026-06-01T00:01:00Z".into(),
            source_cli: "codex".into(),
            source_version: String::new(),
            project: None,
            model: None,
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: Value::Object(Default::default()),
        });
        let encrypted_only = HubRecord::Message(HubMessage {
            id: "m1".into(),
            api_message_id: None,
            parent_id: None,
            timestamp: "2026-06-01T00:00:10Z".into(),
            completed_at: None,
            role: "assistant".into(),
            content: vec![ContentBlock::Thinking {
                text: String::new(),
                subject: None,
                description: None,
                signature: None,
                encrypted: true,
                encryption_format: Some("codex-v1".into()),
                encrypted_data: Some("BASE64BLOB".into()),
                timestamp: None,
            }],
            metadata: MessageMetadata::default(),
            extensions: Value::Object(Default::default()),
        });

        let out = from_hub(&[session, encrypted_only]).unwrap();
        let assistants: Vec<_> = out
            .messages
            .iter()
            .filter(|m| m.role == "assistant")
            .collect();
        assert_eq!(
            assistants.len(),
            1,
            "encrypted reasoning-only turn must not vanish"
        );
        let m = assistants[0];
        assert!(
            m.reasoning.is_none(),
            "no plaintext reasoning for an encrypted-only turn"
        );
        let details = m
            .reasoning_details
            .as_deref()
            .expect("encrypted payload preserved in reasoning_details");
        assert!(details.contains("reasoning.encrypted"), "got {details}");
        assert!(details.contains("BASE64BLOB"), "payload lost: {details}");
        assert!(details.contains("codex-v1"), "format lost: {details}");
    }
}
