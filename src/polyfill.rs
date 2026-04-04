use crate::agents::AgentPolyfillConfig;
use std::collections::HashMap;

/// The resolved invocation details for a specific agent CLI.
pub struct ResolvedInvocation {
    /// CLI arguments to pass to the agent binary.
    pub args: Vec<String>,
    /// Environment variables to set for the agent process.
    pub env: HashMap<String, String>,
    /// Subcommand prefix inserted before other args, e.g. `["exec"]` for codex headless.
    pub subcommand_prefix: Vec<String>,
}

/// Unified flags that get polyfilled into agent-specific invocations.
pub struct PolyfillFlags {
    /// Bypass permission/approval prompts (default: true).
    pub yolo: bool,
    /// Restore approval prompts — inverse of `yolo`.
    pub safe: bool,
    /// Non-interactive / headless mode with the given prompt string.
    pub headless: Option<String>,
    /// Model selection override.
    pub model: Option<String>,
    /// Resume the most recent session.
    pub continue_session: bool,
    /// Resume a specific session or open picker.
    /// - `None` — flag not set
    /// - `Some(None)` — open session picker
    /// - `Some(Some(id))` — resume a specific session by id
    pub resume: Option<Option<String>>,
    /// Fork the current/resumed session.
    pub fork: bool,
    /// Reasoning effort level (e.g., "high", "low").
    pub effort: Option<String>,
    /// Enable auto-mode (autonomous operation).
    pub auto: bool,
}

impl Default for PolyfillFlags {
    fn default() -> Self {
        Self {
            yolo: true,
            safe: false,
            headless: None,
            model: None,
            continue_session: false,
            resume: None,
            fork: false,
            effort: None,
            auto: false,
        }
    }
}

/// Resolve unified flags into an agent-specific invocation using data-driven config.
pub fn resolve(
    config: &AgentPolyfillConfig,
    flags: &PolyfillFlags,
    existing_args: &[String],
) -> ResolvedInvocation {
    let mut args: Vec<String> = Vec::new();
    let env: HashMap<String, String> = HashMap::new();
    let mut subcommand_prefix: Vec<String> = Vec::new();

    // Helper to check if a flag is already present in existing args
    let is_dup = |flag: &str| -> bool { existing_args.iter().any(|a| a == flag) };

    // --- Yolo / Safe ---
    if flags.yolo && !flags.safe {
        if let Some(yolo_flag) = config.get_yolo_flag() {
            if !is_dup(&yolo_flag) {
                args.push(yolo_flag);
            }
        }
    }

    // --- Model ---
    if let Some(ref model) = flags.model {
        let model_flag = config.get_model_flag();
        if !is_dup(&model_flag) {
            args.push(model_flag);
            args.push(model.clone());
        }
    }

    // --- Resume ---
    // Resume is handled before headless because it might set a subcommand prefix
    // (e.g. Codex) which takes precedence.
    if let Some(ref resume_id) = flags.resume {
        let resume_args = config.get_resume_args(resume_id.as_deref());
        // For data-driven simplicity, we don't deduplicate multi-arg session commands yet
        args.extend(resume_args);
    } else if flags.continue_session {
        // --- Continue ---
        let continue_args = config.get_continue_args();
        args.extend(continue_args);
    }

    // --- Headless ---
    // Only apply if neither resume nor continue is active; those modes already
    // place the agent into a specific session and adding a headless subcommand
    // (e.g. "exec" for Codex) would produce a garbled invocation.
    if let Some(ref prompt) = flags.headless {
        if flags.resume.is_none() && !flags.continue_session {
            let (h_args, h_sub) = config.get_headless_invocation(prompt);
            args.extend(h_args);
            subcommand_prefix.extend(h_sub);
        }
    }

    // --- Effort ---
    if let Some(ref effort) = flags.effort {
        if let Some(effort_flag) = config.get_effort_flag() {
            if !is_dup(&effort_flag) {
                args.push(effort_flag);
                args.push(effort.clone());
            }
        } else {
            eprintln!("Warning: Agent does not support reasoning effort flag");
        }
    }

    // --- Auto ---
    if flags.auto {
        if let Some(ref auto_flag) = config.auto_flag {
            if !is_dup(auto_flag) {
                args.push(auto_flag.clone());
            }
        }
    }

    // --- Fork ---
    if flags.fork {
        let (f_args, f_sub, supported) = config.get_fork_invocation();
        if supported {
            args.extend(f_args);
            subcommand_prefix.extend(f_sub);
        } else {
            eprintln!("Warning: Agent does not support session forking");
        }
    }

    ResolvedInvocation {
        args,
        env,
        subcommand_prefix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentDefinition;

    fn default_flags() -> PolyfillFlags {
        PolyfillFlags::default()
    }

    // ── Yolo / Safe ──────────────────────────────────────────

    #[test]
    fn test_claude_yolo_deduplication() {
        let config = AgentDefinition::claude().polyfill;
        let existing = vec!["--dangerously-skip-permissions".to_string()];
        let inv = resolve(&config, &default_flags(), &existing);
        assert_eq!(
            inv.args
                .iter()
                .filter(|a| *a == "--dangerously-skip-permissions")
                .count(),
            0
        );
    }

    #[test]
    fn test_claude_yolo_added_when_absent() {
        let config = AgentDefinition::claude().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        assert!(inv
            .args
            .contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_safe_suppresses_yolo() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            safe: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv
            .args
            .contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn test_opencode_has_no_yolo_flag() {
        let config = AgentDefinition::opencode().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        // OpenCode has no yolo flag — args should not contain any permission-bypass
        assert!(inv
            .args
            .iter()
            .all(|a| !a.contains("yolo") && !a.contains("skip-permissions")));
    }

    #[test]
    fn test_gemini_yolo_resolution() {
        let config = AgentDefinition::gemini().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        assert!(inv.args.contains(&"--yolo".to_string()));
    }

    #[test]
    fn test_codex_yolo_flag() {
        let config = AgentDefinition::codex().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        assert!(inv
            .args
            .contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
    }

    // ── Model ────────────────────────────────────────────────

    #[test]
    fn test_claude_model_flag() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            model: Some("opus".to_string()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--model".to_string()));
        assert!(inv.args.contains(&"opus".to_string()));
    }

    #[test]
    fn test_model_flag_deduplication() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            model: Some("opus".to_string()),
            ..default_flags()
        };
        let existing = vec!["--model".to_string()];
        let inv = resolve(&config, &flags, &existing);
        // Should not add --model again
        assert!(!inv.args.contains(&"--model".to_string()));
    }

    #[test]
    fn test_model_not_added_when_none() {
        let config = AgentDefinition::claude().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        assert!(!inv.args.contains(&"--model".to_string()));
    }

    // ── Continue / Resume ────────────────────────────────────

    #[test]
    fn test_claude_continue_session() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            continue_session: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--continue".to_string()));
    }

    #[test]
    fn test_codex_continue_session() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            continue_session: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"resume".to_string()));
        assert!(inv.args.contains(&"--last".to_string()));
    }

    #[test]
    fn test_claude_resume_with_id() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            resume: Some(Some("abc-123".to_string())),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--resume".to_string()));
        assert!(inv.args.contains(&"abc-123".to_string()));
    }

    #[test]
    fn test_claude_resume_without_id() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            resume: Some(None),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--resume".to_string()));
        // No session ID appended
        assert_eq!(inv.args.iter().filter(|a| *a == "--resume").count(), 1);
    }

    #[test]
    fn test_resume_takes_precedence_over_continue() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            continue_session: true,
            resume: Some(Some("xyz".to_string())),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--resume".to_string()));
        assert!(!inv.args.contains(&"--continue".to_string()));
    }

    // ── Headless ─────────────────────────────────────────────

    #[test]
    fn test_claude_headless_flag() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            headless: Some("do the thing".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"-p".to_string()));
        assert!(inv.args.contains(&"do the thing".to_string()));
        assert!(inv.subcommand_prefix.is_empty());
    }

    #[test]
    fn test_codex_headless_subcommand() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            headless: Some("test".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert_eq!(inv.subcommand_prefix, vec!["exec".to_string()]);
        assert!(inv.args.contains(&"test".to_string()));
    }

    #[test]
    fn test_opencode_headless_subcommand() {
        let config = AgentDefinition::opencode().polyfill;
        let flags = PolyfillFlags {
            headless: Some("fix it".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert_eq!(inv.subcommand_prefix, vec!["run".to_string()]);
        assert!(inv.args.contains(&"fix it".to_string()));
    }

    #[test]
    fn test_gemini_headless_flag() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            headless: Some("hello".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"-p".to_string()));
        assert!(inv.args.contains(&"hello".to_string()));
        assert!(inv.subcommand_prefix.is_empty());
    }

    // ── Effort ───────────────────────────────────────────────

    #[test]
    fn test_claude_effort_flag() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            effort: Some("high".to_string()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--effort".to_string()));
        assert!(inv.args.contains(&"high".to_string()));
    }

    #[test]
    fn test_codex_no_effort_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            effort: Some("high".to_string()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        // Codex has no effort flag — should not appear
        assert!(!inv.args.contains(&"high".to_string()));
    }

    // ── Fork ─────────────────────────────────────────────────

    #[test]
    fn test_claude_fork_flag() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            fork: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--fork-session".to_string()));
    }

    #[test]
    fn test_codex_fork_subcommand() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            fork: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.subcommand_prefix.contains(&"fork".to_string()));
    }

    #[test]
    fn test_gemini_fork_unsupported() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            fork: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        // Gemini does not support forking — no fork args
        assert!(
            inv.subcommand_prefix.is_empty()
                || !inv.subcommand_prefix.contains(&"fork".to_string())
        );
        assert!(!inv.args.contains(&"--fork".to_string()));
    }

    #[test]
    fn test_opencode_fork_flag() {
        let config = AgentDefinition::opencode().polyfill;
        let flags = PolyfillFlags {
            fork: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--fork".to_string()));
    }

    // ── Combined flags ───────────────────────────────────────

    #[test]
    fn test_claude_model_and_effort_together() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            model: Some("sonnet".to_string()),
            effort: Some("low".to_string()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--model".to_string()));
        assert!(inv.args.contains(&"sonnet".to_string()));
        assert!(inv.args.contains(&"--effort".to_string()));
        assert!(inv.args.contains(&"low".to_string()));
    }

    #[test]
    fn test_claude_resume_and_fork() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            resume: Some(Some("sess-1".to_string())),
            fork: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--resume".to_string()));
        assert!(inv.args.contains(&"sess-1".to_string()));
        assert!(inv.args.contains(&"--fork-session".to_string()));
    }

    // --- Headless suppressed by resume / continue ---

    #[test]
    fn test_codex_headless_suppressed_when_resume_set() {
        // Codex headless uses a subcommand ("exec"). When --resume is also set
        // the two subcommands would conflict. Headless must be suppressed.
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            resume: Some(Some("sess-1".to_string())),
            headless: Some("do the thing".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(
            inv.subcommand_prefix.is_empty(),
            "exec subcommand must not be added when resume is set"
        );
        assert!(
            !inv.args.contains(&"do the thing".to_string()),
            "headless prompt must not appear"
        );
        // resume args should still be present
        assert!(inv.args.contains(&"resume".to_string()));
        assert!(inv.args.contains(&"sess-1".to_string()));
    }

    #[test]
    fn test_codex_headless_suppressed_when_continue_set() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            continue_session: true,
            headless: Some("keep going".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(
            inv.subcommand_prefix.is_empty(),
            "exec subcommand must not be added when continue is set"
        );
        assert!(
            !inv.args.contains(&"keep going".to_string()),
            "headless prompt must not appear"
        );
        assert!(inv.args.contains(&"resume".to_string()));
    }

    #[test]
    fn test_claude_headless_still_applied_standalone() {
        // Sanity: when no resume/continue, headless should still work for Claude
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            headless: Some("do the thing".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"-p".to_string()));
        assert!(inv.args.contains(&"do the thing".to_string()));
    }

    #[test]
    fn test_no_flags_produces_only_yolo() {
        let config = AgentDefinition::claude().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        assert_eq!(inv.args, vec!["--dangerously-skip-permissions".to_string()]);
        assert!(inv.subcommand_prefix.is_empty());
        assert!(inv.env.is_empty());
    }

    // ── Auto ────────────────────────────────────────────────

    #[test]
    fn test_codex_auto_flag() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            auto: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--full-auto".to_string()));
    }

    #[test]
    fn test_claude_auto_is_env_only() {
        // Claude auto-mode is via AGENT_AUTO_MODE env var + Stop hook, not a CLI flag.
        // The polyfill resolver must NOT add any auto flag — lib.rs handles it.
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            auto: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.iter().any(|a| a.contains("auto")));
    }

    #[test]
    fn test_gemini_no_auto_support() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            auto: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.iter().any(|a| a.contains("auto")));
    }
}
