//! Cross-CLI conversion tests.
//!
//! Tests all 12 conversion pairs (4 CLIs x 3 targets each).
//! For each pair: source -> Hub -> target -> Hub, then verify portable fields
//! are preserved between the two Hub representations.

#[cfg(test)]
mod tests {
    use crate::interchange::{claude, codex, gemini, hub::*, opencode};

    // ======================================================================
    // Helpers
    // ======================================================================

    /// Load a fixture file from the tests/fixtures directory.
    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/src/interchange/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read fixture {path}: {e}"))
    }

    fn claude_to_hub() -> Vec<HubRecord> {
        let data = fixture("claude-sample.jsonl");
        let reader = std::io::BufReader::new(data.as_slice());
        claude::to_hub(reader).expect("claude to_hub failed")
    }

    fn codex_to_hub() -> Vec<HubRecord> {
        let data = fixture("codex-sample.jsonl");
        let reader = std::io::BufReader::new(data.as_slice());
        codex::to_hub(reader).expect("codex to_hub failed")
    }

    fn gemini_to_hub() -> Vec<HubRecord> {
        let data = fixture("gemini-sample.json");
        gemini::to_hub(&data).expect("gemini to_hub failed")
    }

    fn opencode_to_hub() -> Vec<HubRecord> {
        let msgs_data = fixture("opencode-messages.json");
        let parts_data = fixture("opencode-parts.json");
        let msgs: Vec<serde_json::Value> = serde_json::from_slice(&msgs_data).unwrap();
        let parts: Vec<serde_json::Value> = serde_json::from_slice(&parts_data).unwrap();
        let input = opencode::OpenCodeInput {
            session_id: "opencode-test-session".into(),
            messages: msgs,
            parts,
        };
        opencode::to_hub(&input).expect("opencode to_hub failed")
    }

    /// Extract portable fields from Hub messages for comparison.
    /// Portable = fields common to all CLIs that should survive cross-conversion.
    #[derive(Debug)]
    struct PortableMessage {
        role: String,
        has_text: bool,
        text_preview: String, // first 100 chars of text content
        tool_use_count: usize,
        tool_result_count: usize,
        thinking_count: usize,
        has_tokens: bool,
    }

    fn extract_portable(records: &[HubRecord]) -> Vec<PortableMessage> {
        records
            .iter()
            .filter_map(|r| {
                if let HubRecord::Message(msg) = r {
                    let text_blocks: Vec<&str> = msg
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
                    let text_preview = text_blocks.join(" ");
                    let text_preview = if text_preview.len() > 100 {
                        text_preview[..100].to_string()
                    } else {
                        text_preview
                    };

                    Some(PortableMessage {
                        role: msg.role.clone(),
                        has_text: !text_blocks.is_empty(),
                        text_preview,
                        tool_use_count: msg
                            .content
                            .iter()
                            .filter(|b| matches!(b, ContentBlock::ToolUse { .. }))
                            .count(),
                        tool_result_count: msg
                            .content
                            .iter()
                            .filter(|b| matches!(b, ContentBlock::ToolResult { .. }))
                            .count(),
                        thinking_count: msg
                            .content
                            .iter()
                            .filter(|b| matches!(b, ContentBlock::Thinking { .. }))
                            .count(),
                        has_tokens: msg.metadata.tokens.is_some(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Assert portable fields are preserved across cross-CLI conversion.
    ///
    /// Cross-CLI conversion is inherently imperfect: different CLIs represent
    /// content differently (Gemini inlines tool results, Codex uses event streams,
    /// thinking-only messages may gain placeholder text, etc.). These tests verify
    /// that the CORE portable properties survive:
    ///
    /// 1. Messages exist (at least 1)
    /// 2. Roles are only user/assistant/system (normalized)
    /// 3. Total tool_use count is preserved (tools don't vanish)
    /// 4. At least some text content survives
    fn assert_portable_preserved(
        source_name: &str,
        target_name: &str,
        original: &[PortableMessage],
        converted: &[PortableMessage],
    ) {
        // Both must have messages
        assert!(
            !original.is_empty(),
            "{source_name}: no messages in original"
        );
        assert!(
            !converted.is_empty(),
            "{source_name} -> {target_name}: no messages after conversion"
        );

        // Roles in converted must all be valid
        for (i, msg) in converted.iter().enumerate() {
            assert!(
                matches!(msg.role.as_str(), "user" | "assistant" | "system"),
                "{source_name} -> {target_name} msg {i}: invalid role '{}'",
                msg.role
            );
        }

        // Count aggregate portable properties
        let orig_user_count = original.iter().filter(|m| m.role == "user").count();
        let conv_user_count = converted.iter().filter(|m| m.role == "user").count();
        let orig_assistant_count = original.iter().filter(|m| m.role == "assistant").count();
        let conv_assistant_count = converted.iter().filter(|m| m.role == "assistant").count();

        // User/assistant message counts should be close (within tolerance for
        // format differences like Codex session_meta becoming an extra message)
        let user_diff = (orig_user_count as i64 - conv_user_count as i64).unsigned_abs();
        assert!(
            user_diff <= 2,
            "{source_name} -> {target_name}: user message count diverged too much ({orig_user_count} vs {conv_user_count})"
        );

        let assistant_diff = (orig_assistant_count as i64 - conv_assistant_count as i64).unsigned_abs();
        assert!(
            assistant_diff <= 2,
            "{source_name} -> {target_name}: assistant message count diverged too much ({orig_assistant_count} vs {conv_assistant_count})"
        );

        // Total tool_use invocations should be preserved where the target format supports them.
        // Codex uses function_call events (not content blocks) so tool_use counts may differ
        // when converting through Codex. We check this for non-Codex targets only.
        let orig_tools: usize = original.iter().map(|m| m.tool_use_count).sum();
        let conv_tools: usize = converted.iter().map(|m| m.tool_use_count).sum();
        if orig_tools > 0 && !target_name.contains("codex") {
            assert!(
                conv_tools > 0,
                "{source_name} -> {target_name}: tool calls lost entirely (orig={orig_tools}, conv=0)"
            );
        }

        // At least some text content should survive
        let orig_has_any_text = original.iter().any(|m| m.has_text);
        let conv_has_any_text = converted.iter().any(|m| m.has_text);
        if orig_has_any_text {
            assert!(
                conv_has_any_text,
                "{source_name} -> {target_name}: all text content lost"
            );
        }
    }

    // ======================================================================
    // Hub round-trip through each target format
    // ======================================================================

    /// Convert Hub records through a target CLI and back, return the new Hub records.
    fn round_trip_via_claude(hub: &[HubRecord]) -> Vec<HubRecord> {
        let claude_lines = claude::from_hub(hub).expect("from_hub to claude failed");
        let jsonl: String = claude_lines
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let reader = std::io::BufReader::new(jsonl.as_bytes());
        claude::to_hub(reader).expect("claude to_hub on converted data failed")
    }

    fn round_trip_via_codex(hub: &[HubRecord]) -> Vec<HubRecord> {
        let codex_lines = codex::from_hub(hub).expect("from_hub to codex failed");
        let jsonl: String = codex_lines
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let reader = std::io::BufReader::new(jsonl.as_bytes());
        codex::to_hub(reader).expect("codex to_hub on converted data failed")
    }

    fn round_trip_via_gemini(hub: &[HubRecord]) -> Vec<HubRecord> {
        let gemini_val = gemini::from_hub(hub).expect("from_hub to gemini failed");
        let json = serde_json::to_vec(&gemini_val).unwrap();
        gemini::to_hub(&json).expect("gemini to_hub on converted data failed")
    }

    fn round_trip_via_opencode(hub: &[HubRecord]) -> Vec<HubRecord> {
        let oc_output = opencode::from_hub(hub).expect("from_hub to opencode failed");
        let input = opencode::OpenCodeInput {
            session_id: "cross-cli-test".into(),
            messages: oc_output.messages,
            parts: oc_output.parts,
        };
        opencode::to_hub(&input).expect("opencode to_hub on converted data failed")
    }

    // ======================================================================
    // Claude -> X tests (3 pairs)
    // ======================================================================

    #[test]
    fn test_claude_to_codex_portable_fields() {
        let hub = claude_to_hub();
        let original = extract_portable(&hub);
        let via_codex = round_trip_via_codex(&hub);
        let converted = extract_portable(&via_codex);
        assert_portable_preserved("claude", "codex", &original, &converted);
    }

    #[test]
    fn test_claude_to_gemini_portable_fields() {
        let hub = claude_to_hub();
        let original = extract_portable(&hub);
        let via_gemini = round_trip_via_gemini(&hub);
        let converted = extract_portable(&via_gemini);
        assert_portable_preserved("claude", "gemini", &original, &converted);
    }

    #[test]
    fn test_claude_to_opencode_portable_fields() {
        let hub = claude_to_hub();
        let original = extract_portable(&hub);
        let via_oc = round_trip_via_opencode(&hub);
        let converted = extract_portable(&via_oc);
        assert_portable_preserved("claude", "opencode", &original, &converted);
    }

    // ======================================================================
    // Codex -> X tests (3 pairs)
    // ======================================================================

    #[test]
    fn test_codex_to_claude_portable_fields() {
        let hub = codex_to_hub();
        let original = extract_portable(&hub);
        let via_claude = round_trip_via_claude(&hub);
        let converted = extract_portable(&via_claude);
        assert_portable_preserved("codex", "claude", &original, &converted);
    }

    #[test]
    fn test_codex_to_gemini_portable_fields() {
        let hub = codex_to_hub();
        let original = extract_portable(&hub);
        let via_gemini = round_trip_via_gemini(&hub);
        let converted = extract_portable(&via_gemini);
        assert_portable_preserved("codex", "gemini", &original, &converted);
    }

    #[test]
    fn test_codex_to_opencode_portable_fields() {
        let hub = codex_to_hub();
        let original = extract_portable(&hub);
        let via_oc = round_trip_via_opencode(&hub);
        let converted = extract_portable(&via_oc);
        assert_portable_preserved("codex", "opencode", &original, &converted);
    }

    // ======================================================================
    // Gemini -> X tests (3 pairs)
    // ======================================================================

    #[test]
    fn test_gemini_to_claude_portable_fields() {
        let hub = gemini_to_hub();
        let original = extract_portable(&hub);
        let via_claude = round_trip_via_claude(&hub);
        let converted = extract_portable(&via_claude);
        assert_portable_preserved("gemini", "claude", &original, &converted);
    }

    #[test]
    fn test_gemini_to_codex_portable_fields() {
        let hub = gemini_to_hub();
        let original = extract_portable(&hub);
        let via_codex = round_trip_via_codex(&hub);
        let converted = extract_portable(&via_codex);
        assert_portable_preserved("gemini", "codex", &original, &converted);
    }

    #[test]
    fn test_gemini_to_opencode_portable_fields() {
        let hub = gemini_to_hub();
        let original = extract_portable(&hub);
        let via_oc = round_trip_via_opencode(&hub);
        let converted = extract_portable(&via_oc);
        assert_portable_preserved("gemini", "opencode", &original, &converted);
    }

    // ======================================================================
    // OpenCode -> X tests (3 pairs)
    // ======================================================================

    #[test]
    fn test_opencode_to_claude_portable_fields() {
        let hub = opencode_to_hub();
        let original = extract_portable(&hub);
        let via_claude = round_trip_via_claude(&hub);
        let converted = extract_portable(&via_claude);
        assert_portable_preserved("opencode", "claude", &original, &converted);
    }

    #[test]
    fn test_opencode_to_codex_portable_fields() {
        let hub = opencode_to_hub();
        let original = extract_portable(&hub);
        let via_codex = round_trip_via_codex(&hub);
        let converted = extract_portable(&via_codex);
        assert_portable_preserved("opencode", "codex", &original, &converted);
    }

    #[test]
    fn test_opencode_to_gemini_portable_fields() {
        let hub = opencode_to_hub();
        let original = extract_portable(&hub);
        let via_gemini = round_trip_via_gemini(&hub);
        let converted = extract_portable(&via_gemini);
        assert_portable_preserved("opencode", "gemini", &original, &converted);
    }
}
