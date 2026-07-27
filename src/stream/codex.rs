//! Codex `codex exec --json` adapter.
//!
//! Frame reference (current Codex CLI JSONL event stream):
//! - `{"type":"thread.started","thread_id":"..."}` — session identity
//! - `{"type":"turn.started"}` / `{"type":"turn.completed","usage":{...}}` /
//!   `{"type":"turn.failed","error":{...}}` — turn lifecycle
//! - `{"type":"item.started"|"item.updated"|"item.completed","item":{...}}`
//!   — current items carry `type`: `agent_message`, `reasoning`,
//!   `command_execution`, `file_change`, `mcp_tool_call`, `collab_tool_call`,
//!   `web_search`, `todo_list`, `error`, …; legacy streams used
//!   `item_type` and `assistant_message`
//! - `{"type":"error","message":"..."}` — stream-level error
//!
//! Codex currently rejects its built-in `request_user_input` operation in
//! `exec` mode, so there is no `exec --json` question frame for this adapter
//! to normalize. The supported boundary and reopen condition are documented
//! in `docs/streaming.md`.

use super::{parse_line, DeltaKind, ParsedLine, StreamEvent, UcfStreamParser};
use crate::interchange::hub::{
    ContentBlock, HubEvent, HubMessage, MessageMetadata, SessionHeader, UCF_VERSION,
};
use serde_json::Value;

pub struct CodexStreamParser {
    session_id: String,
    timestamp: String,
    seq: u64,
}

impl CodexStreamParser {
    pub fn new() -> Self {
        Self {
            session_id: String::new(),
            timestamp: String::new(),
            seq: 0,
        }
    }

    /// Fixed timestamp applied to emitted messages/events; see the Claude
    /// adapter for the contract.
    pub fn with_timestamp(mut self, timestamp: impl Into<String>) -> Self {
        self.timestamp = timestamp.into();
        self
    }

    fn next_id(&mut self) -> String {
        self.seq += 1;
        format!("codex-stream-{}", self.seq)
    }

    /// Wrap content in a message, or pass the frame through when conversion
    /// produced nothing — an empty `Message` would surface as a hollow row in
    /// a persisted `.ucf.jsonl`, while passthrough keeps the frame intact.
    fn message(&mut self, role: &str, content: Vec<ContentBlock>, frame: &Value) -> StreamEvent {
        if content.is_empty() {
            return StreamEvent::Passthrough {
                harness: "codex",
                raw: frame.clone(),
            };
        }
        StreamEvent::Message(HubMessage {
            id: self.next_id(),
            api_message_id: None,
            parent_id: None,
            timestamp: self.timestamp.clone(),
            completed_at: None,
            role: role.to_string(),
            content,
            metadata: MessageMetadata::default(),
            extensions: serde_json::json!({"codex": {"_original_frame": frame}}),
        })
    }

    fn event(&self, event_type: &str, data: Value) -> StreamEvent {
        StreamEvent::Event(HubEvent {
            event_type: event_type.to_string(),
            timestamp: self.timestamp.clone(),
            data,
            extensions: Value::Null,
        })
    }

    fn item_completed(&mut self, frame: &Value) -> Vec<StreamEvent> {
        let item = frame.get("item").unwrap_or(&Value::Null);
        let item_type = codex_item_type(item);
        let item_id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        match item_type {
            "agent_message" | "assistant_message" => {
                let content = if text.is_empty() {
                    Vec::new()
                } else {
                    vec![ContentBlock::Text { text }]
                };
                vec![self.message("assistant", content, frame)]
            }
            "reasoning" => {
                let content = if text.is_empty() {
                    Vec::new()
                } else {
                    vec![ContentBlock::Thinking {
                        text,
                        subject: None,
                        description: None,
                        signature: None,
                        encrypted: false,
                        encryption_format: None,
                        encrypted_data: None,
                        timestamp: None,
                    }]
                };
                vec![self.message("assistant", content, frame)]
            }
            "command_execution" | "mcp_tool_call" | "collab_tool_call" => {
                let name = if item_type == "command_execution" {
                    "shell".to_string()
                } else if item_type == "collab_tool_call" {
                    item.get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("collaboration")
                        .to_string()
                } else {
                    item.get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("mcp")
                        .to_string()
                };
                let input = if item_type == "command_execution" {
                    serde_json::json!({
                        "command": item.get("command").cloned().unwrap_or(Value::Null)
                    })
                } else if item_type == "collab_tool_call" {
                    serde_json::json!({
                        "sender_thread_id": item.get("sender_thread_id"),
                        "receiver_thread_ids": item.get("receiver_thread_ids"),
                        "prompt": item.get("prompt"),
                    })
                } else {
                    item.get("arguments")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Default::default()))
                };
                let exit_code = item
                    .get("exit_code")
                    .and_then(Value::as_i64)
                    .map(|c| c as i32);
                // MCP tool calls carry no exit code, so a failure there is
                // only visible through `status`.
                let status = item.get("status").and_then(Value::as_str);
                let output = codex_item_output(item);
                vec![
                    self.message(
                        "assistant",
                        vec![ContentBlock::ToolUse {
                            id: item_id.clone(),
                            name,
                            display_name: None,
                            description: None,
                            input,
                        }],
                        frame,
                    ),
                    self.message(
                        "user",
                        vec![ContentBlock::ToolResult {
                            tool_use_id: item_id,
                            content: vec![ContentBlock::Text { text: output }],
                            exit_code,
                            is_error: exit_code.is_some_and(|code| code != 0)
                                || matches!(
                                    status,
                                    Some("failed" | "error" | "declined" | "interrupted")
                                ),
                            interrupted: false,
                            status: status.map(String::from),
                            duration_ms: None,
                            title: None,
                            truncated: false,
                        }],
                        frame,
                    ),
                ]
            }
            "file_change" => {
                let patches: Vec<ContentBlock> = item
                    .get("changes")
                    .and_then(Value::as_array)
                    .map(|changes| {
                        changes
                            .iter()
                            .filter_map(|change| {
                                change.get("path").and_then(Value::as_str).map(|path| {
                                    ContentBlock::Patch {
                                        path: path.to_string(),
                                        hash_before: None,
                                        hash_after: None,
                                    }
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                vec![self.message("assistant", patches, frame)]
            }
            "web_search" => vec![self.event("tool_end", frame.clone())],
            "todo_list" => vec![self.event("todo_update", frame.clone())],
            "error" => vec![self.event("error", frame.clone())],
            _ => vec![StreamEvent::Passthrough {
                harness: "codex",
                raw: frame.clone(),
            }],
        }
    }

    fn item_updated(&self, frame: &Value) -> Vec<StreamEvent> {
        let item = frame.get("item").unwrap_or(&Value::Null);
        let text = item
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        match codex_item_type(item) {
            // Codex updates carry the full text-so-far, not an append-only
            // fragment — hence `cumulative: true`.
            "agent_message" | "assistant_message" => vec![StreamEvent::Delta {
                kind: DeltaKind::Text,
                text,
                cumulative: true,
            }],
            "reasoning" => vec![StreamEvent::Delta {
                kind: DeltaKind::Thinking,
                text,
                cumulative: true,
            }],
            // Tool-ish updates are repeated in full by item.completed.
            "command_execution" | "mcp_tool_call" | "collab_tool_call" | "file_change"
            | "web_search" => Vec::new(),
            "todo_list" => vec![self.event("todo_update", frame.clone())],
            _ => vec![StreamEvent::Passthrough {
                harness: "codex",
                raw: frame.clone(),
            }],
        }
    }

    fn item_started(&self, frame: &Value) -> Vec<StreamEvent> {
        let item = frame.get("item").unwrap_or(&Value::Null);
        match codex_item_type(item) {
            "command_execution" | "mcp_tool_call" | "collab_tool_call" | "web_search" => {
                vec![self.event("tool_start", frame.clone())]
            }
            // Message-ish starts carry no data their updates/completion
            // don't repeat.
            "agent_message" | "assistant_message" | "reasoning" | "file_change" => Vec::new(),
            "todo_list" => vec![self.event("todo_update", frame.clone())],
            _ => vec![StreamEvent::Passthrough {
                harness: "codex",
                raw: frame.clone(),
            }],
        }
    }
}

/// Codex exec used `item_type`/`assistant_message` in early JSONL builds and
/// now publishes `type`/`agent_message` in its SDK contract. Accept both wire
/// generations; unknown values still become passthrough events.
fn codex_item_type(item: &Value) -> &str {
    item.get("type")
        .or_else(|| item.get("item_type"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn codex_item_output(item: &Value) -> String {
    if let Some(output) = item
        .get("aggregated_output")
        .or_else(|| item.get("output"))
        .and_then(Value::as_str)
    {
        return output.to_string();
    }
    if let Some(message) = item.pointer("/error/message").and_then(Value::as_str) {
        return message.to_string();
    }
    let rendered_content = item
        .pointer("/result/content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .map(|block| {
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .or_else(|| block.as_str())
                        .map_or_else(|| block.to_string(), String::from)
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if !rendered_content.is_empty() {
        return rendered_content;
    }
    item.pointer("/result/structured_content")
        .filter(|value| !value.is_null())
        .or_else(|| item.get("agents_states"))
        .filter(|value| !value.is_null())
        .map(Value::to_string)
        .unwrap_or_default()
}

impl Default for CodexStreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl UcfStreamParser for CodexStreamParser {
    fn harness(&self) -> &'static str {
        "codex"
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
                    harness: "codex",
                    raw: Value::String(raw),
                }]
            }
        };

        match frame.get("type").and_then(Value::as_str).unwrap_or("") {
            "thread.started" => {
                if let Some(id) = frame.get("thread_id").and_then(Value::as_str) {
                    self.session_id = id.to_string();
                }
                vec![StreamEvent::SessionStart(SessionHeader {
                    ucf_version: UCF_VERSION.to_string(),
                    session_id: self.session_id.clone(),
                    created_at: self.timestamp.clone(),
                    updated_at: self.timestamp.clone(),
                    source_cli: "codex".to_string(),
                    source_version: String::new(),
                    project: None,
                    model: None,
                    title: None,
                    slug: None,
                    parent_session_id: None,
                    extensions: serde_json::json!({"codex": {"_original_frame": frame}}),
                })]
            }
            "turn.started" => vec![self.event("turn_start", frame)],
            "turn.completed" => vec![self.event("turn_end", frame)],
            "turn.failed" | "error" => vec![self.event("error", frame)],
            "item.started" => self.item_started(&frame),
            "item.updated" => self.item_updated(&frame),
            "item.completed" => self.item_completed(&frame),
            _ => vec![StreamEvent::Passthrough {
                harness: "codex",
                raw: frame,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(fixture: &str) -> Vec<StreamEvent> {
        let mut parser = CodexStreamParser::new().with_timestamp("2026-07-10T12:00:00Z");
        fixture
            .lines()
            .flat_map(|line| parser.feed_line(line))
            .collect()
    }

    #[test]
    fn command_execution_becomes_linked_tool_pair() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream.jsonl"));
        let tool_use = events
            .iter()
            .find_map(|e| match e {
                StreamEvent::Message(m) => m.content.iter().find_map(|b| match b {
                    ContentBlock::ToolUse {
                        id, name, input, ..
                    } => Some((id, name, input)),
                    _ => None,
                }),
                _ => None,
            })
            .expect("tool use");
        assert_eq!(tool_use.0, "item_0");
        assert_eq!(tool_use.1, "shell");
        assert_eq!(
            tool_use.2.get("command").and_then(Value::as_str),
            Some("ls")
        );

        let tool_result = events
            .iter()
            .find_map(|e| match e {
                StreamEvent::Message(m) => m.content.iter().find_map(|b| match b {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        exit_code,
                        is_error,
                        ..
                    } => Some((tool_use_id, exit_code, is_error)),
                    _ => None,
                }),
                _ => None,
            })
            .expect("tool result");
        assert_eq!(tool_result.0, "item_0");
        assert_eq!(*tool_result.1, Some(0));
        assert!(!tool_result.2);
    }

    #[test]
    fn reasoning_item_becomes_thinking_block() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream.jsonl"));
        assert!(events.iter().any(|e| matches!(
            e,
            StreamEvent::Message(m) if m.content.iter().any(
                |b| matches!(b, ContentBlock::Thinking { text, .. } if text.contains("Rust crate"))
            )
        )));
    }

    #[test]
    fn item_updated_is_cumulative_delta() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream.jsonl"));
        let delta = events
            .iter()
            .find_map(|e| match e {
                StreamEvent::Delta {
                    kind,
                    text,
                    cumulative,
                } => Some((kind, text, cumulative)),
                _ => None,
            })
            .expect("delta");
        assert_eq!(*delta.0, DeltaKind::Text);
        assert_eq!(delta.1, "Do");
        assert!(*delta.2);
    }

    #[test]
    fn failed_command_is_error_result() {
        let mut parser = CodexStreamParser::new();
        let events = parser.feed_line(
            r#"{"type":"item.completed","item":{"id":"item_9","item_type":"command_execution","command":"false","aggregated_output":"","exit_code":1,"status":"failed"}}"#,
        );
        let is_error = events.iter().find_map(|e| match e {
            StreamEvent::Message(m) => m.content.iter().find_map(|b| match b {
                ContentBlock::ToolResult { is_error, .. } => Some(*is_error),
                _ => None,
            }),
            _ => None,
        });
        assert_eq!(is_error, Some(true));
    }

    #[test]
    fn failed_mcp_call_without_exit_code_is_error_result() {
        let mut parser = CodexStreamParser::new();
        let events = parser.feed_line(
            r#"{"type":"item.completed","item":{"id":"item_9","item_type":"mcp_tool_call","tool":"search","arguments":{"q":"x"},"output":"connection refused","status":"failed"}}"#,
        );
        let result = events.iter().find_map(|e| match e {
            StreamEvent::Message(m) => m.content.iter().find_map(|b| match b {
                ContentBlock::ToolResult {
                    is_error,
                    exit_code,
                    ..
                } => Some((*is_error, *exit_code)),
                _ => None,
            }),
            _ => None,
        });
        assert_eq!(result, Some((true, None)));
    }

    #[test]
    fn empty_items_pass_through_instead_of_hollow_messages() {
        let mut parser = CodexStreamParser::new();
        for line in [
            // file_change whose changes all lack a path
            r#"{"type":"item.completed","item":{"id":"i1","item_type":"file_change","changes":[{"kind":"edit"}]}}"#,
            // assistant_message / reasoning with empty text
            r#"{"type":"item.completed","item":{"id":"i2","item_type":"assistant_message","text":""}}"#,
            r#"{"type":"item.completed","item":{"id":"i3","item_type":"reasoning","text":""}}"#,
        ] {
            let events = parser.feed_line(line);
            assert_eq!(events.len(), 1, "{line}");
            assert!(
                matches!(&events[0], StreamEvent::Passthrough { .. }),
                "expected passthrough for {line}, got {:?}",
                events[0]
            );
        }
    }

    #[test]
    fn unknown_item_type_passes_through() {
        let mut parser = CodexStreamParser::new();
        let events = parser.feed_line(
            r#"{"type":"item.completed","item":{"id":"item_9","item_type":"hologram","text":"?"}}"#,
        );
        assert!(matches!(&events[0], StreamEvent::Passthrough { .. }));
    }

    #[test]
    fn current_exec_contract_uses_type_and_agent_message() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream-current.jsonl"));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Message(message)
                if message.role == "assistant"
                    && message.content.iter().any(|block| matches!(
                        block,
                        ContentBlock::Text { text } if text == "The crate is ready."
                    ))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Event(event) if event.event_type == "turn_end"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Passthrough { raw, .. }
                if raw.get("type").and_then(Value::as_str) == Some("future.event")
        )));
    }

    #[test]
    fn current_mcp_result_content_is_preserved_as_tool_output() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream-current.jsonl"));
        let output = events.iter().find_map(|event| match event {
            StreamEvent::Message(message) => message.content.iter().find_map(|block| match block {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } if tool_use_id == "item_1" => content.iter().find_map(|content| match content {
                    ContentBlock::Text { text } => Some(text),
                    _ => None,
                }),
                _ => None,
            }),
            _ => None,
        });
        assert_eq!(output.map(String::as_str), Some("result one\nresult two"));
    }

    #[test]
    fn current_turn_usage_payload_is_preserved() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream-current.jsonl"));
        let turn_end = events.iter().find_map(|event| match event {
            StreamEvent::Event(event) if event.event_type == "turn_end" => Some(event),
            _ => None,
        });
        let turn_end = turn_end.expect("turn_end");
        assert_eq!(
            turn_end
                .data
                .pointer("/usage/reasoning_output_tokens")
                .and_then(Value::as_u64),
            Some(10)
        );
        assert_eq!(
            turn_end
                .data
                .pointer("/usage/cache_write_input_tokens")
                .and_then(Value::as_u64),
            Some(0)
        );
    }

    #[test]
    fn declined_current_command_is_an_error_result() {
        let mut parser = CodexStreamParser::new();
        let events = parser.feed_line(
            r#"{"type":"item.completed","item":{"id":"item_9","type":"command_execution","command":"rm x","aggregated_output":"declined","exit_code":null,"status":"declined"}}"#,
        );
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Message(message)
                if message.content.iter().any(|block| matches!(
                    block,
                    ContentBlock::ToolResult { is_error: true, .. }
                ))
        )));
    }

    #[test]
    fn current_exec_item_variants_keep_lifecycle_and_payloads() {
        let events = feed_all(include_str!("tests/fixtures/codex-stream-current.jsonl"));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Message(message)
                if message.content.iter().any(|block| matches!(
                    block,
                    ContentBlock::ToolUse { id, name, .. }
                        if id == "item_5" && name == "spawn_agent"
                ))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Message(message)
                if message.content.iter().any(|block| matches!(
                    block,
                    ContentBlock::ToolResult { tool_use_id, content, .. }
                        if tool_use_id == "item_5"
                            && content.iter().any(|part| matches!(
                                part,
                                ContentBlock::Text { text } if text.contains("completed")
                            ))
                ))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Message(message)
                if message.content.iter().any(|block| matches!(
                    block,
                    ContentBlock::Patch { path, .. } if path == "src/lib.rs"
                ))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Event(event)
                if event.event_type == "tool_end"
                    && event.data.pointer("/item/id").and_then(Value::as_str) == Some("item_6")
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::Event(event)
                if event.event_type == "error"
                    && event.data.pointer("/item/message").and_then(Value::as_str)
                        == Some("non-fatal warning")
        )));
    }
}
