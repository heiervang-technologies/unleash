//! Live harness output → UCF stream normalization.
//!
//! Transcript conversion (`src/interchange/`) works on completed session
//! files. This module normalizes the *live* JSONL output of a running
//! harness (`claude -p --output-format stream-json`, `codex exec --json`)
//! into UCF-shaped [`StreamEvent`]s, so a consumer never branches on which
//! harness produced a token. See issue #374.
//!
//! Design rules:
//! - The canonical message/session shapes are the existing UCF hub types
//!   ([`HubMessage`], [`SessionHeader`]) — no second schema.
//! - Compliant by default: a frame the adapter does not recognize is never
//!   dropped. It is emitted verbatim as [`StreamEvent::Passthrough`], and a
//!   line that fails to parse as JSON is passed through as a raw string.
//! - Deltas are a separate lightweight variant so attended UIs can render
//!   tokens as they arrive while headless consumers can ignore them and act
//!   only on completed [`StreamEvent::Message`]s.

pub mod claude;
pub mod codex;

use crate::interchange::hub::{HubEvent, HubMessage, SessionHeader};
use serde_json::Value;

/// What kind of text a [`StreamEvent::Delta`] carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaKind {
    Text,
    Thinking,
}

/// A canonical event emitted by every harness stream adapter.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// The harness announced its identity (session id, model, cwd, …).
    SessionStart(SessionHeader),
    /// A completed message, already UCF-shaped. The original harness frame
    /// is preserved under `extensions.<harness>._original_frame`.
    Message(HubMessage),
    /// An incremental token update. `cumulative` is true when `text` is the
    /// full text-so-far (Codex `item.updated`) rather than an append-only
    /// fragment (Claude `text_delta`).
    Delta {
        kind: DeltaKind,
        text: String,
        cumulative: bool,
    },
    /// A lifecycle or status event (`turn_start`, `turn_end`, `agent_end`,
    /// `tool_start`, `error`). `data` carries the raw harness payload.
    Event(HubEvent),
    /// A frame the adapter did not recognize, preserved verbatim.
    Passthrough { harness: &'static str, raw: Value },
}

/// Incremental parser over one harness's JSONL output stream.
///
/// Feed lines as they arrive; each call returns zero or more canonical
/// events. Implementations must be infallible: malformed input becomes
/// [`StreamEvent::Passthrough`], never an error or a silent drop.
pub trait UcfStreamParser {
    /// Which harness this adapter understands (UCF `source_cli` value).
    fn harness(&self) -> &'static str;

    /// Consume one line of harness output.
    fn feed_line(&mut self, line: &str) -> Vec<StreamEvent>;

    /// Flush any buffered state at end of stream.
    fn finish(&mut self) -> Vec<StreamEvent> {
        Vec::new()
    }
}

/// Look up the stream adapter for a harness by its UCF `source_cli` name.
pub fn parser_for(harness: &str) -> Option<Box<dyn UcfStreamParser>> {
    match harness {
        "claude" | "claude-code" => Some(Box::new(claude::ClaudeStreamParser::new())),
        "codex" => Some(Box::new(codex::CodexStreamParser::new())),
        _ => None,
    }
}

/// Outcome of tokenizing one line of harness output.
enum ParsedLine {
    /// Blank line — emit nothing.
    Blank,
    /// A JSON frame for the adapter to interpret.
    Frame(Value),
    /// Not JSON — the adapter must pass it through verbatim.
    Raw(String),
}

/// Shared helper: parse a JSONL line, routing blank lines to nothing and
/// non-JSON lines to passthrough.
fn parse_line(line: &str) -> ParsedLine {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ParsedLine::Blank;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => ParsedLine::Frame(value),
        Err(_) => ParsedLine::Raw(trimmed.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::hub::ContentBlock;

    /// A deliberately harness-blind consumer: everything it learns about the
    /// run comes from canonical events. This is the #374 acceptance check —
    /// if this compiles and passes against both fixtures without inspecting
    /// the harness, adapters have fully normalized their streams.
    #[derive(Debug, Default)]
    struct Transcript {
        session_id: String,
        texts: Vec<String>,
        thinking: Vec<String>,
        tool_calls: Vec<(String, String)>, // (id, name)
        tool_results: Vec<String>,         // tool_use_id
        delta_chars: usize,
        lifecycle: Vec<String>,
        passthrough: usize,
    }

    fn consume(events: Vec<StreamEvent>) -> Transcript {
        let mut t = Transcript::default();
        for event in events {
            match event {
                StreamEvent::SessionStart(header) => t.session_id = header.session_id,
                StreamEvent::Message(msg) => {
                    for block in &msg.content {
                        match block {
                            ContentBlock::Text { text } => t.texts.push(text.clone()),
                            ContentBlock::Thinking { text, .. } => t.thinking.push(text.clone()),
                            ContentBlock::ToolUse { id, name, .. } => {
                                t.tool_calls.push((id.clone(), name.clone()))
                            }
                            ContentBlock::ToolResult { tool_use_id, .. } => {
                                t.tool_results.push(tool_use_id.clone())
                            }
                            _ => {}
                        }
                    }
                }
                StreamEvent::Delta { text, .. } => t.delta_chars += text.len(),
                StreamEvent::Event(evt) => t.lifecycle.push(evt.event_type),
                StreamEvent::Passthrough { .. } => t.passthrough += 1,
            }
        }
        t
    }

    fn run_fixture(harness: &str, fixture: &str) -> Transcript {
        let mut parser = parser_for(harness).expect("adapter registered");
        let mut events = Vec::new();
        for line in fixture.lines() {
            events.extend(parser.feed_line(line));
        }
        events.extend(parser.finish());
        consume(events)
    }

    #[test]
    fn consumer_is_harness_blind_for_claude() {
        let t = run_fixture(
            "claude-code",
            include_str!("tests/fixtures/claude-stream.jsonl"),
        );
        assert_eq!(t.session_id, "c1");
        assert!(t.texts.iter().any(|s| s == "Hello"));
        assert!(t.texts.iter().any(|s| s == "Done."));
        assert_eq!(t.tool_calls, vec![("toolu_01".into(), "Bash".into())]);
        assert_eq!(t.tool_results, vec!["toolu_01".to_string()]);
        assert!(t.delta_chars > 0, "partial-message deltas surface");
        assert!(t.lifecycle.contains(&"agent_end".to_string()));
        assert_eq!(t.passthrough, 1, "unknown frame passes through");
    }

    #[test]
    fn consumer_is_harness_blind_for_codex() {
        let t = run_fixture("codex", include_str!("tests/fixtures/codex-stream.jsonl"));
        assert_eq!(t.session_id, "t1");
        assert!(t.texts.iter().any(|s| s == "Done."));
        assert!(t.thinking.iter().any(|s| s.contains("Rust crate")));
        assert_eq!(t.tool_calls.len(), 1);
        assert_eq!(t.tool_results.len(), 1);
        assert_eq!(
            t.tool_calls[0].0, t.tool_results[0],
            "tool result links to its call"
        );
        assert!(t.delta_chars > 0, "item.updated surfaces as delta");
        assert!(t.lifecycle.contains(&"turn_end".to_string()));
        assert_eq!(t.passthrough, 1, "unknown frame passes through");
    }

    #[test]
    fn non_json_line_passes_through_verbatim() {
        for harness in ["claude-code", "codex"] {
            let mut parser = parser_for(harness).unwrap();
            let events = parser.feed_line("not json at all");
            assert_eq!(events.len(), 1);
            match &events[0] {
                StreamEvent::Passthrough { raw, .. } => {
                    assert_eq!(raw.as_str(), Some("not json at all"))
                }
                other => panic!("expected passthrough, got {other:?}"),
            }
        }
    }

    #[test]
    fn blank_lines_emit_nothing() {
        for harness in ["claude-code", "codex"] {
            let mut parser = parser_for(harness).unwrap();
            assert!(parser.feed_line("").is_empty());
            assert!(parser.feed_line("   ").is_empty());
        }
    }

    #[test]
    fn unknown_harness_has_no_parser() {
        assert!(parser_for("not-a-harness").is_none());
    }
}
