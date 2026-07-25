//! Claude Code `--output-format stream-json` adapter.
//!
//! Frame reference (headless `claude -p`):
//! - `{"type":"system","subtype":"init",...}` — session identity
//! - `{"type":"assistant","message":{...}}` — completed assistant message
//! - `{"type":"user","message":{...}}` — tool results echoed back
//! - `{"type":"stream_event","event":{...}}` — raw API deltas when
//!   `--include-partial-messages` is set
//! - `{"type":"result",...}` — terminal summary frame

use super::{
    interaction_requests_from_tool, parse_line, DeltaKind, ParsedLine, StreamEvent, UcfStreamParser,
};
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

    fn message_frame_to_events(&mut self, frame: &Value, role: &str) -> Vec<StreamEvent> {
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

        let interaction_requests = if role == "assistant" {
            content
                .iter()
                .flat_map(|block| match block {
                    crate::interchange::hub::ContentBlock::ToolUse {
                        id, name, input, ..
                    } => interaction_requests_from_tool(&self.session_id, id, name, input),
                    _ => Vec::new(),
                })
                .map(StreamEvent::InteractionRequest)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let mut events = vec![StreamEvent::Message(HubMessage {
            id: self.next_id(),
            api_message_id: message.get("id").and_then(Value::as_str).map(String::from),
            parent_id: None,
            timestamp: self.timestamp.clone(),
            completed_at: None,
            role: role.to_string(),
            content,
            metadata,
            extensions: serde_json::json!({"claude-code": {"_original_frame": frame}}),
        })];
        events.extend(interaction_requests);
        events
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
                    // These are not renderable text, but their timing matters
                    // to some live consumers. Preserve the full frame instead
                    // of silently consuming it.
                    "input_json_delta" | "signature_delta" => {
                        return vec![StreamEvent::Event(HubEvent {
                            event_type: format!("content_block_delta:{delta_type}"),
                            timestamp: self.timestamp.clone(),
                            data: frame.clone(),
                            extensions: Value::Null,
                        })]
                    }
                    _ => {
                        return vec![StreamEvent::Passthrough {
                            harness: "claude-code",
                            raw: frame.clone(),
                        }]
                    }
                };
                vec![StreamEvent::Delta {
                    kind,
                    text: text.to_string(),
                    cumulative: false,
                }]
            }
            // These carry per-turn usage / stop reason and early tool
            // identity respectively. Their timing is not repeated by the
            // completed assistant frame, so preserve the raw frame.
            "message_delta" | "content_block_start" => {
                vec![StreamEvent::Event(HubEvent {
                    event_type: event_type.to_string(),
                    timestamp: self.timestamp.clone(),
                    data: frame.clone(),
                    extensions: Value::Null,
                })]
            }
            // Pure delimiters with no unique data.
            "message_start" | "message_stop" | "content_block_stop" | "ping" => Vec::new(),
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

    fn set_timestamp(&mut self, timestamp: String) {
        self.timestamp = timestamp;
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
                    let event_type = frame
                        .get("subtype")
                        .and_then(Value::as_str)
                        .filter(|subtype| !subtype.is_empty())
                        .map(|subtype| format!("system:{subtype}"))
                        .unwrap_or_else(|| "system".to_string());
                    vec![StreamEvent::Event(HubEvent {
                        event_type,
                        timestamp: self.timestamp.clone(),
                        data: frame,
                        extensions: Value::Null,
                    })]
                }
            }
            "assistant" => self.message_frame_to_events(&frame, "assistant"),
            "user" => self.message_frame_to_events(&frame, "user"),
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
            .map(|b| {
                // A block the converter rejects must not silently vanish from
                // `content`; the raw frame survives in `_original_frame`.
                crate::interchange::claude::claude_content_to_hub(b).unwrap_or_else(|_| {
                    ContentBlock::Text {
                        text: format!("[Unparseable content block: {b}]"),
                    }
                })
            })
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

    #[test]
    fn unique_partial_lifecycle_data_is_never_silently_consumed() {
        let events = feed_all(include_str!(
            "tests/fixtures/claude-partial-lifecycle.jsonl"
        ));
        let lifecycle: Vec<&HubEvent> = events
            .iter()
            .filter_map(|event| match event {
                StreamEvent::Event(event) => Some(event),
                _ => None,
            })
            .collect();

        for expected in [
            "content_block_start",
            "content_block_delta:input_json_delta",
            "message_delta",
        ] {
            assert!(
                lifecycle.iter().any(|event| event.event_type == expected),
                "missing {expected}: {lifecycle:?}"
            );
        }
        assert_eq!(
            lifecycle
                .iter()
                .find(|event| event.event_type == "message_delta")
                .and_then(|event| event.data.pointer("/event/usage/output_tokens"))
                .and_then(Value::as_u64),
            Some(12)
        );
        assert_eq!(
            lifecycle
                .iter()
                .find(|event| event.event_type == "content_block_start")
                .and_then(|event| event.data.pointer("/event/content_block/name"))
                .and_then(Value::as_str),
            Some("Bash")
        );
    }

    #[test]
    fn system_subtypes_are_promoted_with_a_missing_subtype_fallback() {
        let events = feed_all(include_str!(
            "tests/fixtures/claude-partial-lifecycle.jsonl"
        ));
        let types: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                StreamEvent::Event(event) => Some(event.event_type.as_str()),
                _ => None,
            })
            .collect();
        assert!(types.contains(&"system:hook_started"));
        assert!(types.contains(&"system:task_started"));
        assert!(types.contains(&"system"));
    }

    #[test]
    fn ui_tools_emit_canonical_requests_and_remain_normal_tool_messages() {
        let events = feed_all(include_str!("tests/fixtures/claude-ui-stream.jsonl"));
        let message = events
            .iter()
            .find_map(|event| match event {
                StreamEvent::Message(message) => Some(message),
                _ => None,
            })
            .expect("ordinary assistant message");
        let tool_names: Vec<&str> = message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tool_names, ["AskUserQuestion", "ExitPlanMode"]);

        let requests: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                StreamEvent::InteractionRequest(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].session_id, "ui-1");
        assert_eq!(requests[0].method, super::super::InteractionMethod::Select);
        assert_eq!(requests[0].options, ["Tokio", "smol"]);
        assert_eq!(
            requests[1].method,
            super::super::InteractionMethod::Approval
        );
        assert_eq!(
            requests[1].message,
            "Implement the canonical stream router and its tests."
        );
    }
}
