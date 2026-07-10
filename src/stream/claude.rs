//! Claude Code `--output-format stream-json` adapter.
//!
//! Frame reference (headless `claude -p`):
//! - `{"type":"system","subtype":"init",...}` — session identity
//! - `{"type":"assistant","message":{...}}` — completed assistant message
//! - `{"type":"user","message":{...}}` — tool results echoed back
//! - `{"type":"stream_event","event":{...}}` — raw API deltas when
//!   `--include-partial-messages` is set
//! - `{"type":"result",...}` — terminal summary frame

use super::{parse_line, DeltaKind, ParsedLine, StreamEvent, UcfStreamParser};
use crate::interchange::hub::{
    HubEvent, HubMessage, MessageMetadata, SessionHeader, TokenUsage, UCF_VERSION,
};
use serde_json::Value;

pub struct ClaudeStreamParser {
    session_id: String,
    timestamp: String,
    seq: u64,
}

impl ClaudeStreamParser {
    pub fn new() -> Self {
        Self {
            session_id: String::new(),
            timestamp: String::new(),
            seq: 0,
        }
    }

    /// Fixed timestamp applied to emitted messages/events. Live callers pass
    /// wall-clock time per line; tests pass a constant for determinism. When
    /// unset, timestamps are empty and the consumer is expected to stamp.
    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = timestamp.into();
        self
    }

    fn next_id(&mut self) -> String {
        self.seq += 1;
        format!("claude-stream-{}", self.seq)
    }

    /// Claude attaches `session_id` to most frames; grab it wherever it
    /// appears (latest wins) so the run always has an id by the end.
    fn capture_session_id(&mut self, frame: &Value) {
        if let Some(id) = frame.get("session_id").and_then(Value::as_str) {
            if !id.is_empty() {
                self.session_id = id.to_string();
            }
        }
    }

    fn message_frame_to_event(&mut self, frame: &Value, role: &str) -> StreamEvent {
        let message = frame.get("message").cloned().unwrap_or(Value::Null);
        let content = message
            .get("content")
            .map(claude_stream_content_to_hub)
            .unwrap_or_default();

        let mut metadata = MessageMetadata::default();
        if let Some(model) = message.get("model").and_then(Value::as_str) {
            metadata.model = Some(model.to_string());
        }
        if let Some(stop) = message.get("stop_reason").and_then(Value::as_str) {
            metadata.stop_reason = Some(stop.to_string());
        }
        metadata.tokens = message.get("usage").and_then(usage_to_tokens);

        StreamEvent::Message(HubMessage {
            id: self.next_id(),
            api_message_id: message.get("id").and_then(Value::as_str).map(String::from),
            parent_id: None,
            timestamp: self.timestamp.clone(),
            completed_at: None,
            role: role.to_string(),
            content,
            metadata,
            extensions: serde_json::json!({"claude-code": {"_original_frame": frame}}),
        })
    }

    fn stream_event_frame(&self, frame: &Value) -> Vec<StreamEvent> {
        let event = frame.get("event").unwrap_or(&Value::Null);
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or("");
        match event_type {
            "content_block_delta" => {
                let delta = event.get("delta").unwrap_or(&Value::Null);
                let delta_type = delta.get("type").and_then(Value::as_str).unwrap_or("");
                let (kind, text) = match delta_type {
                    "text_delta" => (
                        DeltaKind::Text,
                        delta.get("text").and_then(Value::as_str).unwrap_or(""),
                    ),
                    "thinking_delta" => (
                        DeltaKind::Thinking,
                        delta.get("thinking").and_then(Value::as_str).unwrap_or(""),
                    ),
                    // input_json_delta / signature_delta carry no renderable
                    // text; the completed assistant frame has the full data.
                    _ => return Vec::new(),
                };
                vec![StreamEvent::Delta {
                    kind,
                    text: text.to_string(),
                    cumulative: false,
                }]
            }
            // Pure protocol framing — the completed `assistant` frame
            // repeats everything these delimit, so they carry no data a
            // consumer could lose.
            "message_start"
            | "message_delta"
            | "message_stop"
            | "content_block_start"
            | "content_block_stop"
            | "ping" => Vec::new(),
            _ => vec![StreamEvent::Passthrough {
                harness: "claude-code",
                raw: frame.clone(),
            }],
        }
    }
}

impl Default for ClaudeStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl UcfStreamParser for ClaudeStreamParser {
    fn harness(&self) -> &'static str {
        "claude-code"
    }

    fn feed_line(&mut self, line: &str) -> Vec<StreamEvent> {
        let frame = match parse_line(line) {
            ParsedLine::Frame(frame) => frame,
            ParsedLine::Blank => return Vec::new(),
            ParsedLine::Raw(raw) => {
                return vec![StreamEvent::Passthrough {
                    harness: "claude-code",
                    raw: Value::String(raw),
                }]
            }
        };
        self.capture_session_id(&frame);

        match frame.get("type").and_then(Value::as_str).unwrap_or("") {
            "system" => {
                if frame.get("subtype").and_then(Value::as_str) == Some("init") {
                    vec![StreamEvent::SessionStart(self.init_header(&frame))]
                } else {
                    vec![StreamEvent::Event(HubEvent {
                        event_type: "system".to_string(),
                        timestamp: self.timestamp.clone(),
                        data: frame,
                        extensions: Value::Null,
                    })]
                }
            }
            "assistant" => vec![self.message_frame_to_event(&frame, "assistant")],
            "user" => vec![self.message_frame_to_event(&frame, "user")],
            "stream_event" => self.stream_event_frame(&frame),
            "result" => vec![StreamEvent::Event(HubEvent {
                event_type: "agent_end".to_string(),
                timestamp: self.timestamp.clone(),
                data: frame,
                extensions: Value::Null,
            })],
            _ => vec![StreamEvent::Passthrough {
                harness: "claude-code",
                raw: frame,
            }],
        }
    }
}

impl ClaudeStreamParser {
    fn init_header(&self, frame: &Value) -> SessionHeader {
        SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: self.session_id.clone(),
            created_at: self.timestamp.clone(),
            updated_at: self.timestamp.clone(),
            source_cli: "claude-code".to_string(),
            source_version: frame
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            project: frame.get("cwd").and_then(Value::as_str).map(|cwd| {
                crate::interchange::hub::ProjectInfo {
                    directory: cwd.to_string(),
                    root: None,
                    hash: None,
                    vcs: None,
                    branch: None,
                    sha: None,
                    origin_url: None,
                }
            }),
            model: frame.get("model").and_then(Value::as_str).map(String::from),
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: serde_json::json!({"claude-code": {"_original_frame": frame}}),
        }
    }
}

/// Convert a Claude message `content` value (string or block array) into hub
/// blocks, delegating per-block parsing to the interchange converter so the
/// live and transcript paths cannot drift.
fn claude_stream_content_to_hub(content: &Value) -> Vec<crate::interchange::hub::ContentBlock> {
    use crate::interchange::hub::ContentBlock;
    match content {
        Value::String(s) => vec![ContentBlock::Text { text: s.clone() }],
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| crate::interchange::claude::claude_content_to_hub(b).ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn usage_to_tokens(usage: &Value) -> Option<TokenUsage> {
    let get = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    if !usage.is_object() {
        return None;
    }
    Some(TokenUsage {
        input: get("input_tokens"),
        output: get("output_tokens"),
        cache_creation: get("cache_creation_input_tokens"),
        cache_read: get("cache_read_input_tokens"),
        reasoning: 0,
        tool: 0,
        total: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::hub::ContentBlock;

    fn feed_all(fixture: &str) -> Vec<StreamEvent> {
        let mut parser = ClaudeStreamParser::new().with_timestamp("2026-07-10T12:00:00Z");
        fixture
            .lines()
            .flat_map(|line| parser.feed_line(line))
            .collect()
    }

    #[test]
    fn init_frame_becomes_session_header() {
        let events = feed_all(include_str!("tests/fixtures/claude-stream.jsonl"));
        let header = events
            .iter()
            .find_map(|e| match e {
                StreamEvent::SessionStart(h) => Some(h),
                _ => None,
            })
            .expect("session start");
        assert_eq!(header.session_id, "c1");
        assert_eq!(header.source_cli, "claude-code");
        assert_eq!(header.model.as_deref(), Some("claude-opus-4-6"));
        assert_eq!(
            header.project.as_ref().map(|p| p.directory.as_str()),
            Some("/tmp/proj")
        );
    }

    #[test]
    fn assistant_frame_preserves_original_and_usage() {
        let events = feed_all(include_str!("tests/fixtures/claude-stream.jsonl"));
        let msg = events
            .iter()
            .find_map(|e| match e {
                StreamEvent::Message(m) if m.role == "assistant" => Some(m),
                _ => None,
            })
            .expect("assistant message");
        assert_eq!(msg.api_message_id.as_deref(), Some("msg_01"));
        assert_eq!(msg.metadata.tokens.as_ref().map(|t| t.input), Some(10));
        assert!(msg
            .extensions
            .pointer("/claude-code/_original_frame/message/id")
            .is_some());
        assert!(msg
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { name, .. } if name == "Bash")));
    }

    #[test]
    fn text_deltas_are_incremental() {
        let events = feed_all(include_str!("tests/fixtures/claude-stream.jsonl"));
        let deltas: Vec<&StreamEvent> = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::Delta { .. }))
            .collect();
        assert_eq!(deltas.len(), 2);
        let combined: String = deltas
            .iter()
            .map(|e| match e {
                StreamEvent::Delta {
                    text, cumulative, ..
                } => {
                    assert!(!cumulative);
                    text.as_str()
                }
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(combined, "Hello");
    }

    #[test]
    fn string_content_user_message_is_accepted() {
        let mut parser = ClaudeStreamParser::new();
        let events = parser.feed_line(
            r#"{"type":"user","session_id":"c9","message":{"role":"user","content":"plain text"}}"#,
        );
        match &events[0] {
            StreamEvent::Message(m) => {
                assert!(
                    matches!(&m.content[0], ContentBlock::Text { text } if text == "plain text")
                );
            }
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[test]
    fn unknown_stream_event_subtype_passes_through() {
        let mut parser = ClaudeStreamParser::new();
        let events = parser.feed_line(
            r#"{"type":"stream_event","session_id":"c1","event":{"type":"audio_delta","data":"x"}}"#,
        );
        assert!(matches!(&events[0], StreamEvent::Passthrough { .. }));
    }
}
