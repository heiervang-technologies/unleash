//! Harness-blind routing and the public JSONL normalization runtime.

use super::{parser_for, StreamEvent};
use crate::interchange::hub::ContentBlock;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

/// A runtime event envelope.
///
/// `session_id` is repeated on every serialized event as the stable join key
/// for out-of-band consumers. The flattened `event` remains the canonical
/// discriminated union produced by the adapters.
#[derive(Debug, Clone, Serialize)]
pub struct RoutedStreamEvent {
    pub session_id: String,
    #[serde(flatten)]
    pub event: StreamEvent,
}

/// Harness-independent routing state for attended or headless consumers.
pub struct StreamRuntime {
    ui_bound: bool,
    session_id: String,
    tool_sessions: HashMap<String, String>,
}

impl StreamRuntime {
    /// Create a runtime. When `ui_bound` is false, interaction requests are
    /// not forwarded, but message and tool bookkeeping remain identical.
    pub fn new(ui_bound: bool) -> Self {
        Self {
            ui_bound,
            session_id: String::new(),
            tool_sessions: HashMap::new(),
        }
    }

    pub fn ui_bound(&self) -> bool {
        self.ui_bound
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn session_for_tool(&self, tool_call_id: &str) -> Option<&str> {
        self.tool_sessions.get(tool_call_id).map(String::as_str)
    }

    /// Apply runtime bookkeeping and optionally forward one canonical event.
    pub fn route(&mut self, event: StreamEvent) -> Option<RoutedStreamEvent> {
        match &event {
            StreamEvent::SessionStart(header) => {
                self.session_id.clone_from(&header.session_id);
            }
            StreamEvent::Message(message) => {
                record_tool_sessions(&message.content, &self.session_id, &mut self.tool_sessions);
            }
            StreamEvent::InteractionRequest(request) => {
                if !request.session_id.is_empty() {
                    self.session_id.clone_from(&request.session_id);
                }
                let session_id = if request.session_id.is_empty() {
                    self.session_id.clone()
                } else {
                    request.session_id.clone()
                };
                self.tool_sessions
                    .insert(request.tool_call_id.clone(), session_id);
            }
            _ => {}
        }

        if !self.ui_bound && matches!(event, StreamEvent::InteractionRequest(_)) {
            return None;
        }

        Some(RoutedStreamEvent {
            session_id: self.session_id.clone(),
            event,
        })
    }

    pub fn route_all(
        &mut self,
        events: impl IntoIterator<Item = StreamEvent>,
    ) -> Vec<RoutedStreamEvent> {
        events
            .into_iter()
            .filter_map(|event| self.route(event))
            .collect()
    }
}

impl Default for StreamRuntime {
    fn default() -> Self {
        Self::new(true)
    }
}

fn record_tool_sessions(
    content: &[ContentBlock],
    session_id: &str,
    tool_sessions: &mut HashMap<String, String>,
) {
    for block in content {
        match block {
            ContentBlock::ToolUse { id, .. } => {
                tool_sessions.insert(id.clone(), session_id.to_string());
            }
            ContentBlock::ToolResult { content, .. } => {
                record_tool_sessions(content, session_id, tool_sessions);
            }
            _ => {}
        }
    }
}

/// Normalize harness JSONL from `reader` and write canonical JSONL to `writer`.
pub fn normalize_reader<R: BufRead, W: Write>(
    harness: &str,
    ui_bound: bool,
    reader: R,
    mut writer: W,
) -> io::Result<StreamRuntime> {
    let mut parser = parser_for(harness).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "no stream adapter for harness '{harness}' \
                 (supported: claude, claude-code, codex)"
            ),
        )
    })?;
    let mut runtime = StreamRuntime::new(ui_bound);

    for line in reader.lines() {
        let line = line?;
        parser.set_timestamp(chrono::Utc::now().to_rfc3339());
        write_events(&mut runtime, parser.feed_line(&line), &mut writer)?;
    }
    parser.set_timestamp(chrono::Utc::now().to_rfc3339());
    write_events(&mut runtime, parser.finish(), &mut writer)?;
    writer.flush()?;
    Ok(runtime)
}

fn write_events<W: Write>(
    runtime: &mut StreamRuntime,
    events: impl IntoIterator<Item = StreamEvent>,
    writer: &mut W,
) -> io::Result<()> {
    for event in runtime.route_all(events) {
        serde_json::to_writer(&mut *writer, &event).map_err(io::Error::other)?;
        writer.write_all(b"\n")?;
    }
    Ok(())
}

/// CLI entry point for `unleash stream`.
pub fn normalize_command(harness: &str, headless: bool, input: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let writer = BufWriter::new(stdout.lock());
    if input == "-" {
        let stdin = io::stdin();
        normalize_reader(harness, !headless, stdin.lock(), writer)?;
    } else {
        let file = File::open(input)?;
        normalize_reader(harness, !headless, BufReader::new(file), writer)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::hub::{HubMessage, MessageMetadata, SessionHeader, UCF_VERSION};

    fn header() -> StreamEvent {
        StreamEvent::SessionStart(SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: "session-1".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            source_cli: "claude-code".to_string(),
            source_version: String::new(),
            project: None,
            model: None,
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: serde_json::Value::Null,
        })
    }

    fn tool_message() -> StreamEvent {
        StreamEvent::Message(HubMessage {
            id: "m1".to_string(),
            api_message_id: None,
            parent_id: None,
            timestamp: String::new(),
            completed_at: None,
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: "call-1".to_string(),
                name: "AskUserQuestion".to_string(),
                display_name: None,
                description: None,
                input: serde_json::json!({"question": "Continue?"}),
            }],
            metadata: MessageMetadata::default(),
            extensions: serde_json::Value::Null,
        })
    }

    fn request() -> StreamEvent {
        StreamEvent::InteractionRequest(super::super::InteractionRequest {
            id: "call-1".to_string(),
            session_id: "session-1".to_string(),
            tool_call_id: "call-1".to_string(),
            method: super::super::InteractionMethod::Input,
            title: "Question".to_string(),
            message: "Continue?".to_string(),
            options: Vec::new(),
            custom: true,
            multiple: false,
            raw: serde_json::json!({"question": "Continue?"}),
        })
    }

    #[test]
    fn attended_and_headless_share_bookkeeping_and_transcript() {
        let events = vec![header(), tool_message(), request()];
        let mut attended = StreamRuntime::new(true);
        let mut headless = StreamRuntime::new(false);

        let attended_output = attended.route_all(events.clone());
        let headless_output = headless.route_all(events);

        assert_eq!(attended_output.len(), 3);
        assert_eq!(headless_output.len(), 2);
        assert!(matches!(
            attended_output[2].event,
            StreamEvent::InteractionRequest(_)
        ));
        assert!(headless_output
            .iter()
            .all(|event| !matches!(event.event, StreamEvent::InteractionRequest(_))));
        assert!(matches!(attended_output[1].event, StreamEvent::Message(_)));
        assert!(matches!(headless_output[1].event, StreamEvent::Message(_)));
        assert_eq!(
            serde_json::to_value(&attended_output[1].event).unwrap(),
            serde_json::to_value(&headless_output[1].event).unwrap(),
            "headless mode must not rewrite the preserved tool message"
        );
        assert_eq!(attended.session_for_tool("call-1"), Some("session-1"));
        assert_eq!(headless.session_for_tool("call-1"), Some("session-1"));
    }

    #[test]
    fn every_serialized_event_has_the_session_join_key() {
        let mut runtime = StreamRuntime::new(true);
        for event in runtime.route_all([header(), tool_message(), request()]) {
            let value = serde_json::to_value(event).unwrap();
            assert_eq!(
                value.get("session_id").and_then(serde_json::Value::as_str),
                Some("session-1")
            );
            assert!(value.get("type").is_some());
        }
    }
}
