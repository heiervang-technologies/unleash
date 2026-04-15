//! Strict lossless + idempotence tests for cross-CLI conversion.
//!
//! Goal: `A → hub₁ → B (native) → hub₂ → A` must satisfy
//! `semantic_eq(hub₁, hub₂)` for every (A, B) pair. This is stronger than the
//! "portable fields preserved" checks in `cross_cli_tests.rs` — it requires
//! that foreign CLI extensions round-trip through intermediate formats.
//!
//! Idempotence: `to_hub(from_hub(to_hub(x))) == to_hub(x)` for every CLI.
//!
//! These tests are expected to fail until foreign-extension passthrough is
//! implemented in each converter. They document the target behavior.
//!
//! Tests are gated behind `#[ignore]` until the passthrough lands. Run with:
//!   cargo test --lib interchange::lossless_tests -- --ignored --nocapture

#[cfg(test)]
mod tests {
    use crate::interchange::{claude, codex, gemini, hub::*, opencode, semantic_eq::semantic_eq};

    // =======================================================================
    // Fixture loading
    // =======================================================================

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!(
            "{}/src/interchange/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read fixture {path}: {e}"))
    }

    fn all_types_hub() -> Vec<HubRecord> {
        let data = fixture("synthetic/all-content-types.ucf.jsonl");
        let text = String::from_utf8(data).unwrap();
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    // =======================================================================
    // Hub record serialization for comparison
    // =======================================================================

    /// Compare two Vec<HubRecord> via semantic_eq on their JSON form.
    /// Returns Ok(()) if the two streams are semantically identical.
    fn hub_eq(a: &[HubRecord], b: &[HubRecord]) -> Result<(), String> {
        let av = serde_json::to_value(a).unwrap();
        let bv = serde_json::to_value(b).unwrap();
        semantic_eq(&av, &bv)
    }

    // =======================================================================
    // Native round-trip helpers (via each CLI native format)
    // =======================================================================

    fn via_claude(hub: &[HubRecord]) -> Vec<HubRecord> {
        let lines = claude::from_hub(hub).expect("from_hub claude");
        let jsonl: String = lines
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        claude::to_hub(std::io::BufReader::new(jsonl.as_bytes())).expect("to_hub claude")
    }

    fn via_codex(hub: &[HubRecord]) -> Vec<HubRecord> {
        let lines = codex::from_hub(hub).expect("from_hub codex");
        let jsonl: String = lines
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        codex::to_hub(std::io::BufReader::new(jsonl.as_bytes())).expect("to_hub codex")
    }

    fn via_gemini(hub: &[HubRecord]) -> Vec<HubRecord> {
        let val = gemini::from_hub(hub).expect("from_hub gemini");
        let bytes = serde_json::to_vec(&val).unwrap();
        gemini::to_hub(&bytes).expect("to_hub gemini")
    }

    fn via_opencode(hub: &[HubRecord]) -> Vec<HubRecord> {
        let out = opencode::from_hub(hub).expect("from_hub opencode");
        let input = opencode::OpenCodeInput {
            session_id: "lossless-test".into(),
            messages: out.messages,
            parts: out.parts,
        };
        opencode::to_hub(&input).expect("to_hub opencode")
    }

    // =======================================================================
    // Idempotence: to_hub(from_hub(to_hub(native))) == to_hub(native)
    //
    // Equivalently, the "via" helpers above must be idempotent:
    //   via_X(hub) == via_X(via_X(hub))
    // =======================================================================

    fn assert_idempotent(name: &str, f: impl Fn(&[HubRecord]) -> Vec<HubRecord>) {
        let hub = all_types_hub();
        let once = f(&hub);
        let twice = f(&once);
        if let Err(diff) = hub_eq(&once, &twice) {
            panic!("{name}: not idempotent — {diff}");
        }
    }

    #[test]
    #[ignore = "lossless target — pending _ucf_extensions passthrough"]
    fn idempotent_via_claude() {
        assert_idempotent("claude", via_claude);
    }

    #[test]
    #[ignore = "lossless target — pending _ucf_extensions passthrough"]
    fn idempotent_via_codex() {
        assert_idempotent("codex", via_codex);
    }

    #[test]
    #[ignore = "lossless target — pending _ucf_extensions passthrough"]
    fn idempotent_via_gemini() {
        assert_idempotent("gemini", via_gemini);
    }

    #[test]
    #[ignore = "lossless target — pending _ucf_extensions passthrough"]
    fn idempotent_via_opencode() {
        assert_idempotent("opencode", via_opencode);
    }

    // =======================================================================
    // Strict lossless: hub → X → hub must be semantically identical
    //
    // Starting hub has empty extensions (synthetic fixture). This tests
    // that core content survives *perfectly* through each format.
    // =======================================================================

    fn assert_strict_lossless(name: &str, f: impl Fn(&[HubRecord]) -> Vec<HubRecord>) {
        let hub = all_types_hub();
        let result = f(&hub);
        if let Err(diff) = hub_eq(&hub, &result) {
            panic!("{name}: not lossless — {diff}");
        }
    }

    #[test]
    #[ignore = "lossless target"]
    fn lossless_through_claude() {
        assert_strict_lossless("claude", via_claude);
    }

    #[test]
    #[ignore = "lossless target"]
    fn lossless_through_codex() {
        assert_strict_lossless("codex", via_codex);
    }

    #[test]
    #[ignore = "lossless target"]
    fn lossless_through_gemini() {
        assert_strict_lossless("gemini", via_gemini);
    }

    #[test]
    #[ignore = "lossless target"]
    fn lossless_through_opencode() {
        assert_strict_lossless("opencode", via_opencode);
    }

    // =======================================================================
    // Cross-CLI lossless: A-flavored hub → B → hub must equal original
    //
    // We simulate A-flavored hub by first running the synthetic through
    // A's converter (which populates extensions.A). Then we pass it through B.
    // The resulting hub must semantically equal the A-flavored one.
    // =======================================================================

    fn flavored_hub(f: impl Fn(&[HubRecord]) -> Vec<HubRecord>) -> Vec<HubRecord> {
        f(&all_types_hub())
    }

    fn assert_cross_lossless(
        src_name: &str,
        dst_name: &str,
        flavor: impl Fn(&[HubRecord]) -> Vec<HubRecord>,
        passthrough: impl Fn(&[HubRecord]) -> Vec<HubRecord>,
    ) {
        let a = flavored_hub(flavor);
        let b = passthrough(&a);
        if let Err(diff) = hub_eq(&a, &b) {
            panic!("{src_name} → {dst_name} → hub is lossy: {diff}");
        }
    }

    // All 12 cross-CLI pairs.

    #[test]
    #[ignore = "lossless target"]
    fn cross_claude_via_codex() {
        assert_cross_lossless("claude", "codex", via_claude, via_codex);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_claude_via_gemini() {
        assert_cross_lossless("claude", "gemini", via_claude, via_gemini);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_claude_via_opencode() {
        assert_cross_lossless("claude", "opencode", via_claude, via_opencode);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_codex_via_claude() {
        assert_cross_lossless("codex", "claude", via_codex, via_claude);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_codex_via_gemini() {
        assert_cross_lossless("codex", "gemini", via_codex, via_gemini);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_codex_via_opencode() {
        assert_cross_lossless("codex", "opencode", via_codex, via_opencode);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_gemini_via_claude() {
        assert_cross_lossless("gemini", "claude", via_gemini, via_claude);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_gemini_via_codex() {
        assert_cross_lossless("gemini", "codex", via_gemini, via_codex);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_gemini_via_opencode() {
        assert_cross_lossless("gemini", "opencode", via_gemini, via_opencode);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_opencode_via_claude() {
        assert_cross_lossless("opencode", "claude", via_opencode, via_claude);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_opencode_via_codex() {
        assert_cross_lossless("opencode", "codex", via_opencode, via_codex);
    }

    #[test]
    #[ignore = "lossless target"]
    fn cross_opencode_via_gemini() {
        assert_cross_lossless("opencode", "gemini", via_opencode, via_gemini);
    }

    // =======================================================================
    // Diagnostic helper: print the diff for every pair on a single run.
    // Useful for triaging remaining gaps once the feature lands.
    // =======================================================================

    #[test]
    #[ignore = "diagnostic — run manually"]
    fn diagnostic_all_pairs() {
        let clis: Vec<(&str, &dyn Fn(&[HubRecord]) -> Vec<HubRecord>)> = vec![
            ("claude", &via_claude),
            ("codex", &via_codex),
            ("gemini", &via_gemini),
            ("opencode", &via_opencode),
        ];

        let mut rows = Vec::new();
        for (a_name, a_fn) in &clis {
            let a_hub = a_fn(&all_types_hub());
            for (b_name, b_fn) in &clis {
                if a_name == b_name {
                    continue;
                }
                let result = b_fn(&a_hub);
                match hub_eq(&a_hub, &result) {
                    Ok(()) => rows.push(format!("  {a_name} → {b_name}: LOSSLESS")),
                    Err(diff) => rows.push(format!("  {a_name} → {b_name}: {diff}")),
                }
            }
        }

        eprintln!("\n=== Cross-CLI lossless diagnostic ===");
        for row in &rows {
            eprintln!("{row}");
        }
    }
}
