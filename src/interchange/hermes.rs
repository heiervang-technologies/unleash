use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::Value;

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
    if s.len() < 19 {
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
            records.push(HubRecord::Message(HubMessage {
                id: msg_id,
                api_message_id: None,
                parent_id: None,
                timestamp: epoch_to_iso(ts as u64),
                completed_at: None,
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id,
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
                extensions: Value::Object(Default::default()),
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

                    // Collect matching tool result from following tool-role msgs
                    for next in messages.iter().skip(i + 1) {
                        if next["role"].as_str() != Some("tool") {
                            break;
                        }
                        if next["tool_call_id"].as_str() == Some(&call_id) {
                            let res = next["content"].as_str().unwrap_or("").to_string();
                            result_blocks.push(ContentBlock::ToolResult {
                                tool_use_id: call_id.clone(),
                                content: vec![ContentBlock::Text { text: res }],
                                is_error: false,
                                exit_code: None,
                                interrupted: false,
                                status: None,
                                duration_ms: None,
                                title: None,
                                truncated: false,
                            });
                            break;
                        }
                    }
                }
                // Advance past consumed tool-role messages
                let mut j = i + 1;
                while j < messages.len() && messages[j]["role"].as_str() == Some("tool") {
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
                extensions: Value::Object(Default::default()),
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

        let text_content: String = msg
            .content
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

        // Emit primary message
        if !text_content.is_empty() || tool_calls_json.is_some() {
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
                    tool_name: None,
                    timestamp: ts,
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
}
