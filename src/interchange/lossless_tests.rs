//! Strict lossless + idempotence tests for cross-CLI conversion.
//!
//! Goal: `A → hub₁ → B (native) → hub₂ → A` must satisfy
//! `semantic_eq(hub₁, hub₂)` for every (A, B) pair. This is stronger than the
//! "portable fields preserved" checks in `cross_cli_tests.rs` — it requires
//! that foreign CLI extensions round-trip through intermediate formats.
//!
//! Idempotence: `to_hub(from_hub(to_hub(x))) == to_hub(x)` for every CLI.
//!
//! Foreign-extension passthrough is implemented in the codex/claude/gemini/
//! opencode converters, which round-trip the full synthetic fixture strictly.
//! Pi and Hermes are normalizing converters (see the "matrix broadening"
//! section): they are asserted idempotent on the full fixture and strictly
//! lossless on the content surface each models natively. All tests run by
//! default; two diagnostics are `#[ignore]`d and run only with `--ignored`.

#[cfg(test)]
mod tests {
    use crate::interchange::{
        claude, codex, gemini, hermes, hub::*, opencode, pi, semantic_eq::semantic_eq,
    };

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

    /// Collect *every* differing leaf path between two JSON values (unlike
    /// `semantic_eq`, which returns only the first diff). Used by the
    /// characterization tests to pin the full-fixture residual as an exact set,
    /// so a future regression that drops a NEW field — or a fix that recovers a
    /// tracked one — both break CI instead of sliding by while the round-trip
    /// stays idempotent + native-subset-lossless.
    fn json_diff_paths(
        a: &serde_json::Value,
        b: &serde_json::Value,
        path: &str,
        out: &mut Vec<String>,
    ) {
        use serde_json::Value;
        match (a, b) {
            (Value::Object(ma), Value::Object(mb)) => {
                let mut keys: std::collections::BTreeSet<&String> = ma.keys().collect();
                keys.extend(mb.keys());
                for k in keys {
                    let child = format!("{path}.{k}");
                    match (ma.get(k), mb.get(k)) {
                        (Some(va), Some(vb)) => json_diff_paths(va, vb, &child, out),
                        (Some(_), None) => out.push(format!("{child} [removed]")),
                        (None, Some(_)) => out.push(format!("{child} [added]")),
                        (None, None) => {}
                    }
                }
            }
            (Value::Array(aa), Value::Array(ba)) => {
                if aa.len() != ba.len() {
                    out.push(format!("{path} [len {} != {}]", aa.len(), ba.len()));
                }
                for (i, (x, y)) in aa.iter().zip(ba.iter()).enumerate() {
                    json_diff_paths(x, y, &format!("{path}[{i}]"), out);
                }
            }
            _ => {
                if a != b {
                    out.push(path.to_string());
                }
            }
        }
    }

    fn residual_paths(orig: &[HubRecord], round_tripped: &[HubRecord]) -> Vec<String> {
        let a = serde_json::to_value(orig).unwrap();
        let b = serde_json::to_value(round_tripped).unwrap();
        let mut out = Vec::new();
        json_diff_paths(&a, &b, "$", &mut out);
        out.sort();
        out
    }

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

    fn via_pi(hub: &[HubRecord]) -> Vec<HubRecord> {
        let vals = pi::from_hub(hub).expect("from_hub pi");
        let jsonl: String = vals
            .iter()
            .map(|v| serde_json::to_string(v).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        pi::to_hub(std::io::BufReader::new(jsonl.as_bytes())).expect("to_hub pi")
    }

    /// Serialize a `HermesOutput` into the session-JSON shape that
    /// `hermes::to_hub` consumes — i.e. exactly what a Hermes `state.db`
    /// round-trip (INSERT via inject, then SELECT back) would produce. The
    /// sequential `id` mirrors the SQLite rowid assigned on insert.
    fn hermes_session_json(out: &hermes::HermesOutput) -> String {
        let messages: Vec<serde_json::Value> = out
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| {
                serde_json::json!({
                    "id": (i as u64) + 1,
                    "role": m.role,
                    "content": m.content,
                    "tool_calls": m.tool_calls
                        .as_deref()
                        .map(|s| serde_json::from_str::<serde_json::Value>(s)
                            .unwrap_or(serde_json::Value::Null)),
                    "tool_call_id": m.tool_call_id,
                    "tool_name": m.tool_name,
                    "timestamp": m.timestamp,
                    "reasoning": m.reasoning,
                    "reasoning_details": m.reasoning_details,
                    "finish_reason": m.finish_reason,
                    "token_count": m.token_count,
                })
            })
            .collect();
        serde_json::json!({
            "id": out.session.id,
            "model": out.session.model,
            "title": out.session.title,
            "started_at": out.session.started_at,
            "ended_at": out.session.ended_at,
            "messages": messages,
        })
        .to_string()
    }

    fn via_hermes(hub: &[HubRecord]) -> Vec<HubRecord> {
        let out = hermes::from_hub(hub).expect("from_hub hermes");
        let json = hermes_session_json(&out);
        hermes::to_hub(&json).expect("to_hub hermes")
    }

    fn hub_from_jsonl(s: &str) -> Vec<HubRecord> {
        s.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Content shapes Pi models natively (text / thinking / tool_use), plus a
    /// `completed_at` to lock the hub-level completion-timestamp round-trip.
    /// Deliberately carries no token/cost metadata — Pi's `usage` is
    /// synthesize-and-reconstruct, a separate strict-round-trip concern from
    /// content preservation (see the residual-boundary diagnostic below).
    fn pi_native_subset() -> Vec<HubRecord> {
        hub_from_jsonl(
            r#"{"type":"session","ucf_version":"1.0.0","session_id":"pi-subset","created_at":"2026-04-04T10:00:00Z","updated_at":"2026-04-04T10:05:00Z","source_cli":"ucf","source_version":"1.0.0","model":"m","title":"Pi subset"}
{"type":"message","id":"m1","timestamp":"2026-04-04T10:00:01Z","role":"user","content":[{"type":"text","text":"hi"}],"metadata":{},"extensions":{}}
{"type":"message","id":"m2","parent_id":"m1","timestamp":"2026-04-04T10:00:02Z","completed_at":"2026-04-04T10:00:05Z","role":"assistant","content":[{"type":"thinking","text":"let me think","signature":"sig1"},{"type":"text","text":"ok"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}],"metadata":{},"extensions":{}}"#,
        )
    }

    /// Content shapes Hermes models natively (text / tool_use → tool_calls +
    /// tool rows). No thinking/image (tracked by #406).
    fn hermes_native_subset() -> Vec<HubRecord> {
        hub_from_jsonl(
            r#"{"type":"session","ucf_version":"1.0.0","session_id":"hermes-subset","created_at":"2026-04-04T10:00:00Z","updated_at":"2026-04-04T10:05:00Z","source_cli":"hermes","source_version":"","model":"m","title":"Hermes subset"}
{"type":"message","id":"1","timestamp":"2026-04-04T10:00:01Z","role":"user","content":[{"type":"text","text":"hi"}],"metadata":{"model":"m"},"extensions":{}}
{"type":"message","id":"2","timestamp":"2026-04-04T10:00:02Z","role":"assistant","content":[{"type":"text","text":"running"},{"type":"tool_use","id":"call_1","name":"Bash","input":{"command":"ls"}}],"metadata":{"model":"m"},"extensions":{}}"#,
        )
    }

    #[test]
    #[ignore = "diagnostic — run manually with --ignored --nocapture"]
    fn diagnostic_pi_hermes() {
        let probe = |name: &str, f: &dyn Fn(&[HubRecord]) -> Vec<HubRecord>, hub: &[HubRecord]| {
            let once = f(hub);
            eprintln!("\n=== {name} strict-lossless ===");
            match hub_eq(hub, &once) {
                Ok(()) => eprintln!("  LOSSLESS"),
                Err(d) => eprintln!("  LOSSY: {d}"),
            }
            let twice = f(&once);
            eprintln!("=== {name} idempotence ===");
            match hub_eq(&once, &twice) {
                Ok(()) => eprintln!("  IDEMPOTENT"),
                Err(d) => eprintln!("  NOT IDEMPOTENT: {d}"),
            }
        };
        let all = all_types_hub();
        probe("pi (all-content-types)", &via_pi, &all);
        probe("hermes (all-content-types)", &via_hermes, &all);
        probe("pi (native subset)", &via_pi, &pi_native_subset());
        probe(
            "hermes (native subset)",
            &via_hermes,
            &hermes_native_subset(),
        );
    }

    // =======================================================================
    // Pi / Hermes matrix broadening (#353)
    //
    // Pi and Hermes cannot be *strictly* byte-lossless for the full
    // all-content-types fixture on `main`. Every residual below has a tracked
    // home:
    //   - Pi has no native home for reasoning-token counts or image blocks
    //     (images degrade to a text placeholder), and its usage/cost is
    //     synthesize-and-reconstruct.  → #412
    //   - Hermes preserves thinking/reasoning-only turns via its native
    //     reasoning columns (#406 write side + the to_hub restore here), but
    //     still degrades image/patch to text placeholders (content), and
    //     normalizes session identity + propagates the session model onto
    //     every message  → #414 (normalization, not content).
    // Run `diagnostic_pi_hermes` (above, --ignored) to see the residuals, and
    // `pi_full_fixture_residual_is_pinned` for the exact Pi set.
    //
    // So the matrix asserts three invariants that ARE achievable today:
    //   1. Idempotence on the full fixture — the normalization is a fixpoint,
    //      so no information erodes across repeated round-trips.
    //   2. Strict losslessness on the content surface each format models
    //      natively (the "native subset" fixtures above).
    //   3. A pinned UPPER BOUND on full-fixture loss (characterization tests
    //      below), so a regression that grows loss can't hide behind (1)+(2).
    // =======================================================================

    /// Characterization guardrail (wiseau #413 review): idempotence + native-
    /// subset losslessness prove the *shape* is right but set no UPPER BOUND on
    /// full-fixture loss — a regression dropping a new out-of-subset field would
    /// stay idempotent and pass the subset test while loss silently grows. This
    /// pins the EXACT residual of `all-content-types → pi → hub`, so any change
    /// (loss grows OR a tracked gap in #412 gets fixed) fails here and forces a
    /// deliberate update. The residual is Pi's normalization surface:
    ///   - reasoning-token counts + cost None→0.0 + redundant usage_raw (#412),
    ///   - image + tool_result content degraded to text placeholders,
    ///   - encrypted-thinking field reshaping.
    #[test]
    fn pi_full_fixture_residual_is_pinned() {
        let all = all_types_hub();
        let residual = residual_paths(&all, &via_pi(&all));
        let expected: Vec<&str> = vec![
            "$[1].extensions",
            "$[2].extensions.pi [added]",
            "$[2].metadata.cost [added]",
            "$[2].metadata.tokens.reasoning",
            "$[3].content[0].encrypted",
            "$[3].content[0].encrypted_data [removed]",
            "$[3].content[0].encryption_format [removed]",
            "$[3].content[0].signature [added]",
            "$[4].content[0].content [removed]",
            "$[4].content[0].duration_ms [removed]",
            "$[4].content[0].exit_code [removed]",
            "$[4].content[0].interrupted [removed]",
            "$[4].content[0].is_error [removed]",
            "$[4].content[0].status [removed]",
            "$[4].content[0].text [added]",
            "$[4].content[0].tool_use_id [removed]",
            "$[4].content[0].truncated [removed]",
            "$[4].content[0].type",
            "$[4].extensions",
            "$[5].content[1].data [removed]",
            "$[5].content[1].encoding [removed]",
            "$[5].content[1].media_type [removed]",
            "$[5].content[1].text [added]",
            "$[5].content[1].type",
            "$[5].extensions",
            "$[6].extensions",
        ];
        assert_eq!(
            residual, expected,
            "Pi full-fixture residual changed. If you FIXED a gap, remove its \
             path(s) here and update #412. If loss GREW, that is a regression."
        );
    }

    /// Upper bound on Hermes record-level loss. The full fixture is 7 records
    /// (1 session + 6 messages); Hermes drops exactly ONE — the reasoning-only
    /// assistant turn (thinking, no text/tools), which it cannot represent
    /// (tracked by #406). A regression dropping MORE fails here rather than
    /// sliding past the idempotent + subset-lossless checks.
    #[test]
    fn hermes_full_fixture_drops_no_turns() {
        // Since Hermes gained reasoning/finish columns on both the write side
        // (from_hub, #406) and the read side (to_hub restore in this PR), no
        // turn of the full fixture is dropped anymore — including the
        // reasoning-only assistant turn that used to vanish.
        let all = all_types_hub();
        assert_eq!(all.len(), 7, "fixture shape assumption");
        let out = via_hermes(&all);
        assert_eq!(out.len(), 7, "Hermes must not drop any turn");
        // Content of surviving turns is still present (not just the count).
        let has_text = |needle: &str| {
            out.iter().any(|r| match r {
                HubRecord::Message(m) => m
                    .content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Text { text } if text.contains(needle))),
                _ => false,
            })
        };
        assert!(
            has_text("I'll run ls"),
            "assistant tool-use turn text survived"
        );
        assert!(has_text("Hello"), "user turn text survived");
        // The formerly-dropped reasoning-only turn survives with its thinking
        // content intact (restored from the native `reasoning` column).
        let has_thinking = out.iter().any(|r| match r {
            HubRecord::Message(m) => m.content.iter().any(
                |b| matches!(b, ContentBlock::Thinking { text, .. } if text.contains("I should run ls")),
            ),
            _ => false,
        });
        assert!(
            has_thinking,
            "reasoning-only turn must survive with content"
        );
    }

    #[test]
    fn idempotent_via_pi() {
        assert_idempotent("pi", via_pi);
    }

    #[test]
    fn idempotent_via_hermes() {
        assert_idempotent("hermes", via_hermes);
    }

    #[test]
    fn lossless_through_pi_native_subset() {
        let hub = pi_native_subset();
        if let Err(diff) = hub_eq(&hub, &via_pi(&hub)) {
            panic!("pi native subset not lossless — {diff}");
        }
    }

    #[test]
    fn lossless_through_hermes_native_subset() {
        let hub = hermes_native_subset();
        if let Err(diff) = hub_eq(&hub, &via_hermes(&hub)) {
            panic!("hermes native subset not lossless — {diff}");
        }
    }

    /// Regression: Pi's `to_hub` used to hardcode `completed_at: None` and
    /// `from_hub` never stashed the hub-level completion timestamp, so it
    /// vanished on every hub → Pi → hub round trip. Proven to fail before the
    /// paired `completedAt` stash/restore in pi.rs.
    #[test]
    fn pi_preserves_completed_at() {
        let hub = pi_native_subset();
        let out = via_pi(&hub);
        let completed: Vec<_> = out
            .iter()
            .filter_map(|r| match r {
                HubRecord::Message(m) => m.completed_at.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(
            completed,
            vec!["2026-04-04T10:00:05Z".to_string()],
            "Pi round-trip must preserve message completed_at"
        );
    }

    /// Native-plus-injected: a session that mixes native records with an
    /// injected (foreign-flavored) message must not drop either on round-trip
    /// — a matrix-level guard for the #393-class "mixed native+injected turns
    /// globally dropped" regression.
    #[test]
    fn native_plus_injected_survives_roundtrip() {
        let native = via_gemini(&all_types_hub()); // gemini-native session
        let injected = via_claude(&all_types_hub()); // claude-flavored records

        let injected_msg = injected
            .iter()
            .find(|r| matches!(r, HubRecord::Message(_)))
            .expect("an injected message")
            .clone();

        let mut mixed = native.clone();
        mixed.push(injected_msg.clone());

        let out = via_gemini(&mixed);

        let msg_count = |v: &[HubRecord]| {
            v.iter()
                .filter(|r| matches!(r, HubRecord::Message(_)))
                .count()
        };
        assert_eq!(
            msg_count(&out),
            msg_count(&mixed),
            "mixed native+injected round-trip dropped messages"
        );

        // The injected message's text content must still be present.
        let injected_text = match &injected_msg {
            HubRecord::Message(m) => m.content.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            }),
            _ => None,
        };
        if let Some(text) = injected_text {
            let survives = out.iter().any(|r| match r {
                HubRecord::Message(m) => m
                    .content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Text { text: t } if *t == text)),
                _ => false,
            });
            assert!(
                survives,
                "injected message content was dropped on round-trip"
            );
        }
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
    fn idempotent_via_claude() {
        assert_idempotent("claude", via_claude);
    }

    #[test]
    fn idempotent_via_codex() {
        assert_idempotent("codex", via_codex);
    }

    #[test]
    fn idempotent_via_gemini() {
        assert_idempotent("gemini", via_gemini);
    }

    #[test]
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
    fn lossless_through_claude() {
        assert_strict_lossless("claude", via_claude);
    }

    #[test]
    fn lossless_through_codex() {
        assert_strict_lossless("codex", via_codex);
    }

    #[test]
    fn lossless_through_gemini() {
        assert_strict_lossless("gemini", via_gemini);
    }

    #[test]
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
    fn cross_claude_via_codex() {
        assert_cross_lossless("claude", "codex", via_claude, via_codex);
    }

    #[test]
    fn cross_claude_via_gemini() {
        assert_cross_lossless("claude", "gemini", via_claude, via_gemini);
    }

    #[test]
    fn cross_claude_via_opencode() {
        assert_cross_lossless("claude", "opencode", via_claude, via_opencode);
    }

    #[test]
    fn cross_codex_via_claude() {
        assert_cross_lossless("codex", "claude", via_codex, via_claude);
    }

    #[test]
    fn cross_codex_via_gemini() {
        assert_cross_lossless("codex", "gemini", via_codex, via_gemini);
    }

    #[test]
    fn cross_codex_via_opencode() {
        assert_cross_lossless("codex", "opencode", via_codex, via_opencode);
    }

    #[test]
    fn cross_gemini_via_claude() {
        assert_cross_lossless("gemini", "claude", via_gemini, via_claude);
    }

    #[test]
    fn cross_gemini_via_codex() {
        assert_cross_lossless("gemini", "codex", via_gemini, via_codex);
    }

    #[test]
    fn cross_gemini_via_opencode() {
        assert_cross_lossless("gemini", "opencode", via_gemini, via_opencode);
    }

    #[test]
    fn cross_opencode_via_claude() {
        assert_cross_lossless("opencode", "claude", via_opencode, via_claude);
    }

    #[test]
    fn cross_opencode_via_codex() {
        assert_cross_lossless("opencode", "codex", via_opencode, via_codex);
    }

    #[test]
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
        type RoundTrip = dyn Fn(&[HubRecord]) -> Vec<HubRecord>;
        let clis: Vec<(&str, &RoundTrip)> = vec![
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
