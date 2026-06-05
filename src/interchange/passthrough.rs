//! Render hub records as a human-readable chat transcript.
//!
//! Used by `unleash convert --to passthrough` so the prior conversation can
//! be passed as a single initial prompt to a target CLI (especially useful
//! for CLIs that refuse session-injection — see #313, originally surfaced
//! by agy's server-side cascade validation in #307).
//!
//! The output is plain-text markdown, intentionally lossy: tool calls,
//! tool results, thinking blocks, and images are summarised, not reproduced
//! verbatim, so the result fits in a single prompt without code-block
//! escaping or signature-verification issues.

use crate::interchange::hub::{ContentBlock, HubMessage, HubRecord};

const TOOL_INPUT_SUMMARY_BYTES: usize = 240;
const TOOL_RESULT_SUMMARY_BYTES: usize = 400;
const IMAGE_PLACEHOLDER: &str = "[image elided — passthrough mode does not carry binary content]";

/// Render a slice of `HubRecord`s as a markdown chat transcript.
///
/// Session headers and events are skipped; only `HubMessage` records produce
/// output. Roles render as `## User` / `## Assistant` / `## System` headings
/// per their `role` string (preserved literally so foreign roles still render).
pub fn render_as_transcript(records: &[HubRecord]) -> String {
    let mut out = String::new();
    out.push_str(
        "# Prior conversation (passthrough crossload)\n\n\
         The following is the verbatim history from the source CLI session, \
         rendered as plain text. Tool calls, tool results, thinking blocks, \
         and images are summarised for readability.\n\n\
         ---\n\n",
    );

    for record in records {
        if let HubRecord::Message(msg) = record {
            render_message(msg, &mut out);
        }
    }

    out.push_str("---\n\n*End of prior conversation.*\n");
    out
}

fn render_message(msg: &HubMessage, out: &mut String) {
    if msg.content.is_empty() {
        return;
    }
    let heading = match msg.role.as_str() {
        "user" => "User".to_string(),
        "assistant" => "Assistant".to_string(),
        "system" => "System".to_string(),
        other => capitalise(other),
    };
    out.push_str(&format!("## {heading}\n\n"));

    for block in &msg.content {
        render_block(block, out);
    }
    out.push('\n');
}

fn render_block(block: &ContentBlock, out: &mut String) {
    match block {
        ContentBlock::Text { text } => {
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        ContentBlock::ToolUse {
            name,
            display_name,
            input,
            ..
        } => {
            let label = display_name.as_deref().unwrap_or(name);
            let input_summary =
                serde_json::to_string(input).unwrap_or_else(|_| "<unserializable>".to_string());
            let truncated = truncate(&input_summary, TOOL_INPUT_SUMMARY_BYTES);
            out.push_str(&format!("> **[tool: {label}]** `{truncated}`\n\n"));
        }
        ContentBlock::ToolResult {
            content, is_error, ..
        } => {
            let mut joined = String::new();
            for inner in content {
                if let ContentBlock::Text { text } = inner {
                    joined.push_str(text);
                    joined.push('\n');
                }
            }
            let truncated = truncate(joined.trim(), TOOL_RESULT_SUMMARY_BYTES);
            let marker = if *is_error {
                "tool result (error)"
            } else {
                "tool result"
            };
            out.push_str(&format!("> **[{marker}]** `{truncated}`\n\n"));
        }
        ContentBlock::Thinking { text, .. } => {
            // Thinking blocks usually carry provider-signed payloads that
            // can't be repeated verbatim across providers. Summarise the
            // visible text and drop the signature.
            if !text.trim().is_empty() {
                let truncated = truncate(text.trim(), TOOL_RESULT_SUMMARY_BYTES);
                out.push_str(&format!("> *[thinking]* {truncated}\n\n"));
            }
        }
        ContentBlock::Image { .. } => {
            out.push_str(IMAGE_PLACEHOLDER);
            out.push_str("\n\n");
        }
        ContentBlock::StepBoundary { .. } | ContentBlock::Patch { .. } => {
            // Internal step boundaries / patch annotations aren't useful to a
            // downstream CLI reading this as a prompt; skip silently.
        }
    }
}

fn truncate(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.replace('`', "'").replace('\n', " ");
    }
    // Cut at a char boundary <= max_bytes.
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    let cut = &s[..end];
    format!(
        "{} …[truncated {} bytes]",
        cut.replace('`', "'").replace('\n', " "),
        s.len() - end
    )
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::hub::{HubMessage, MessageMetadata};
    use serde_json::json;

    fn msg(role: &str, blocks: Vec<ContentBlock>) -> HubRecord {
        HubRecord::Message(HubMessage {
            id: "test".into(),
            api_message_id: None,
            parent_id: None,
            timestamp: String::new(),
            completed_at: None,
            role: role.into(),
            content: blocks,
            metadata: MessageMetadata::default(),
            extensions: serde_json::Value::Null,
        })
    }

    #[test]
    fn renders_simple_user_assistant_exchange() {
        let records = vec![
            msg(
                "user",
                vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            ),
            msg(
                "assistant",
                vec![ContentBlock::Text {
                    text: "hi back".into(),
                }],
            ),
        ];
        let out = render_as_transcript(&records);
        assert!(out.contains("## User"));
        assert!(out.contains("hello"));
        assert!(out.contains("## Assistant"));
        assert!(out.contains("hi back"));
        assert!(out.contains("End of prior conversation"));
    }

    #[test]
    fn renders_tool_use_and_result_as_summaries() {
        let records = vec![msg(
            "assistant",
            vec![
                ContentBlock::ToolUse {
                    id: "x".into(),
                    name: "bash".into(),
                    display_name: None,
                    description: None,
                    input: json!({"command": "ls -la"}),
                },
                ContentBlock::ToolResult {
                    tool_use_id: "x".into(),
                    content: vec![ContentBlock::Text {
                        text: "total 0\ndrwxr-xr-x".into(),
                    }],
                    exit_code: Some(0),
                    is_error: false,
                    interrupted: false,
                    status: None,
                    duration_ms: None,
                    title: None,
                    truncated: false,
                },
            ],
        )];
        let out = render_as_transcript(&records);
        assert!(out.contains("[tool: bash]"));
        assert!(out.contains("ls -la"));
        assert!(out.contains("[tool result]"));
        assert!(out.contains("total 0"));
    }

    #[test]
    fn error_tool_result_marker_set() {
        let records = vec![msg(
            "assistant",
            vec![ContentBlock::ToolResult {
                tool_use_id: "x".into(),
                content: vec![ContentBlock::Text {
                    text: "boom".into(),
                }],
                exit_code: Some(1),
                is_error: true,
                interrupted: false,
                status: None,
                duration_ms: None,
                title: None,
                truncated: false,
            }],
        )];
        let out = render_as_transcript(&records);
        assert!(out.contains("[tool result (error)]"));
    }

    #[test]
    fn images_become_placeholder() {
        let records = vec![msg(
            "user",
            vec![ContentBlock::Image {
                media_type: "image/png".into(),
                encoding: "base64".into(),
                data: "AAA".into(),
                source_url: None,
            }],
        )];
        let out = render_as_transcript(&records);
        assert!(out.contains("[image elided"));
        // Image bytes themselves don't leak into output.
        assert!(!out.contains("AAA"));
    }

    #[test]
    fn thinking_blocks_summarised_signature_dropped() {
        let records = vec![msg(
            "assistant",
            vec![ContentBlock::Thinking {
                text: "let me reason through this".into(),
                subject: None,
                description: None,
                signature: Some("SECRET-SIG-DO-NOT-LEAK".into()),
                encrypted: false,
                encryption_format: None,
                encrypted_data: None,
                timestamp: None,
            }],
        )];
        let out = render_as_transcript(&records);
        assert!(out.contains("let me reason through this"));
        assert!(!out.contains("SECRET-SIG-DO-NOT-LEAK"));
    }

    #[test]
    fn empty_message_skipped_no_empty_heading() {
        let records = vec![msg("user", vec![])];
        let out = render_as_transcript(&records);
        assert!(!out.contains("## User"));
    }

    #[test]
    fn truncate_at_char_boundary_with_marker() {
        let long = "x".repeat(1000);
        let t = truncate(&long, 100);
        assert!(t.len() < 200);
        assert!(t.contains("[truncated"));
    }

    #[test]
    fn session_headers_skipped() {
        let header = crate::interchange::hub::SessionHeader {
            ucf_version: "1.0".into(),
            session_id: "sess".into(),
            created_at: String::new(),
            updated_at: String::new(),
            source_cli: "claude".into(),
            source_version: String::new(),
            project: None,
            model: None,
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: serde_json::Value::Null,
        };
        let records = vec![
            HubRecord::Session(header),
            msg(
                "user",
                vec![ContentBlock::Text {
                    text: "first".into(),
                }],
            ),
        ];
        let out = render_as_transcript(&records);
        assert!(out.contains("first"));
        assert!(!out.contains("ucf_version"));
    }
}
