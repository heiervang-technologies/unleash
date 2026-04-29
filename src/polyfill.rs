use crate::agents::{AgentPolyfillConfig, SandboxStrategy};
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
    /// Enable verbose/debug output.
    pub verbose: bool,
    /// Output format (e.g., "json", "text", "stream-json").
    pub output_format: Option<String>,
    /// System prompt text to inject.
    pub system_prompt: Option<String>,
    /// Allowed tools filter (comma-separated list).
    pub allowed_tools: Option<String>,
    /// Enable sandbox mode.
    pub sandbox: bool,
    /// Session name.
    pub name: Option<String>,
    /// Additional directory to include.
    pub add_dir: Option<String>,
    /// Approval/permission mode.
    pub approval_mode: Option<String>,
    /// Git worktree mode.
    /// - `None` — flag not set
    /// - `Some(None)` — use auto-generated worktree name
    /// - `Some(Some(name))` — use specific worktree name
    pub worktree: Option<Option<String>>,
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
            verbose: false,
            output_format: None,
            system_prompt: None,
            allowed_tools: None,
            sandbox: false,
            name: None,
            add_dir: None,
            approval_mode: None,
            worktree: None,
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

    // --- Resume / Continue ---
    // Subcommand-style agents (codex) put resume/continue into `subcommand_prefix`
    // instead of args, so the resume subcommand replaces the headless `exec`
    // subcommand cleanly. Flag-style agents append into args as before.
    let resume_or_continue_active = flags.resume.is_some() || flags.continue_session;
    if let Some(ref resume_id) = flags.resume {
        let resume_args = config.get_resume_args(resume_id.as_deref());
        if config.session.resume_is_subcommand {
            subcommand_prefix.extend(resume_args);
        } else {
            args.extend(resume_args);
        }
    } else if flags.continue_session {
        let continue_args = config.get_continue_args();
        if config.session.resume_is_subcommand {
            subcommand_prefix.extend(continue_args);
        } else {
            args.extend(continue_args);
        }
    }

    // --- Headless ---
    // When resume/continue is active for a subcommand-style agent, the resume
    // subcommand owns `subcommand_prefix`; just append the prompt as a positional
    // arg (codex `resume <id> [PROMPT]` accepts a positional prompt).
    // For flag-style agents, headless is skipped when a session is active,
    // since the existing crossload/UCF path appends the resume args to extra_args
    // *after* polyfill resolution and we want the headless flag/prompt to land
    // alongside it.
    if let Some(ref prompt) = flags.headless {
        if resume_or_continue_active && config.session.resume_is_subcommand {
            args.push(prompt.clone());
        } else if !resume_or_continue_active {
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

    // --- Verbose ---
    if flags.verbose {
        if let Some(ref verbose_flag) = config.verbose_flag {
            if !is_dup(verbose_flag) {
                args.push(verbose_flag.clone());
            }
        }
    }

    // --- Output Format ---
    if let Some(ref format) = flags.output_format {
        if let Some(ref output_format_flag) = config.output_format_flag {
            if !is_dup(output_format_flag) {
                args.push(output_format_flag.clone());
                args.push(format.clone());
            }
        }
    }

    // --- System Prompt ---
    if let Some(ref prompt) = flags.system_prompt {
        if let Some(ref system_prompt_flag) = config.system_prompt_flag {
            if !is_dup(system_prompt_flag) {
                args.push(system_prompt_flag.clone());
                args.push(prompt.clone());
            }
        }
    }

    // --- Allowed Tools ---
    if let Some(ref tools) = flags.allowed_tools {
        if let Some(ref allowed_tools_flag) = config.allowed_tools_flag {
            if !is_dup(allowed_tools_flag) {
                args.push(allowed_tools_flag.clone());
                args.push(tools.clone());
            }
        }
    }

    // --- Sandbox ---
    if flags.sandbox {
        match &config.sandbox {
            SandboxStrategy::BoolFlag(flag) => {
                if !is_dup(flag) {
                    args.push(flag.clone());
                }
            }
            SandboxStrategy::ValueFlag(flag, value) => {
                if !is_dup(flag) {
                    args.push(flag.clone());
                    args.push(value.clone());
                }
            }
            SandboxStrategy::Unsupported => {}
        }
    }

    // --- Name ---
    if let Some(ref name) = flags.name {
        if let Some(ref name_flag) = config.name_flag {
            if !is_dup(name_flag) {
                args.push(name_flag.clone());
                args.push(name.clone());
            }
        }
    }

    // --- Add Dir ---
    if let Some(ref dir) = flags.add_dir {
        if let Some(ref add_dir_flag) = config.add_dir_flag {
            if !is_dup(add_dir_flag) {
                args.push(add_dir_flag.clone());
                args.push(dir.clone());
            }
        }
    }

    // --- Approval Mode ---
    if let Some(ref mode) = flags.approval_mode {
        if let Some(ref approval_mode_flag) = config.approval_mode_flag {
            if !is_dup(approval_mode_flag) {
                args.push(approval_mode_flag.clone());
                args.push(mode.clone());
            }
        }
    }

    // --- Worktree ---
    if let Some(ref worktree_name) = flags.worktree {
        if let Some(ref worktree_flag) = config.worktree_flag {
            if !is_dup(worktree_flag) {
                args.push(worktree_flag.clone());
                if let Some(ref name) = worktree_name {
                    args.push(name.clone());
                }
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
        // Codex uses subcommand-style resume/continue, so they go into
        // subcommand_prefix instead of args. This is what allows codex to
        // run `codex resume --last` rather than `codex resume --last` as
        // positional args to whatever the leading subcommand happens to be.
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            continue_session: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert_eq!(
            inv.subcommand_prefix,
            vec!["resume".to_string(), "--last".to_string()]
        );
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
    fn test_codex_headless_combined_with_resume() {
        // Codex's resume is subcommand-style, so when both --resume and -p are
        // set the resume subcommand owns the prefix and the prompt becomes a
        // positional arg of `resume`. This produces:
        //     codex resume <id> "PROMPT"
        // Previously this combo was suppressed (headless dropped) because the
        // two subcommands "exec" and "resume" would have collided as
        //     codex exec PROMPT resume <id>
        // which codex parses as positional args to `exec`.
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            resume: Some(Some("sess-1".to_string())),
            headless: Some("do the thing".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert_eq!(
            inv.subcommand_prefix,
            vec!["resume".to_string(), "sess-1".to_string()]
        );
        assert!(
            inv.args.contains(&"do the thing".to_string()),
            "headless prompt should be present as positional arg"
        );
        assert!(
            !inv.args.contains(&"exec".to_string()),
            "the headless `exec` subcommand must not leak into args"
        );
    }

    #[test]
    fn test_codex_headless_combined_with_continue() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            continue_session: true,
            headless: Some("keep going".to_string()),
            yolo: false,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert_eq!(
            inv.subcommand_prefix,
            vec!["resume".to_string(), "--last".to_string()]
        );
        assert!(inv.args.contains(&"keep going".to_string()));
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

    // ── Verbose ─────────────────────────────────────────────

    #[test]
    fn test_claude_verbose() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            verbose: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--verbose".to_string()));
    }

    #[test]
    fn test_gemini_verbose_is_debug() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            verbose: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--debug".to_string()));
    }

    #[test]
    fn test_opencode_verbose_is_print_logs() {
        let config = AgentDefinition::opencode().polyfill;
        let flags = PolyfillFlags {
            verbose: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--print-logs".to_string()));
    }

    #[test]
    fn test_codex_no_verbose_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            verbose: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.iter().any(|a| a.contains("verbose") || a.contains("debug") || a.contains("print-logs")));
    }

    // ── Output Format ────────────────��──────────────────────

    #[test]
    fn test_claude_output_format_json() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            output_format: Some("json".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--output-format".to_string()));
        assert!(inv.args.contains(&"json".to_string()));
    }

    #[test]
    fn test_gemini_output_format() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            output_format: Some("json".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"-o".to_string()));
        assert!(inv.args.contains(&"json".to_string()));
    }

    #[test]
    fn test_codex_no_output_format_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            output_format: Some("json".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.contains(&"json".to_string()));
    }

    // ── System Prompt ───────────────────────────────────────

    #[test]
    fn test_claude_system_prompt() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            system_prompt: Some("You are a code reviewer".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--system-prompt".to_string()));
        assert!(inv.args.contains(&"You are a code reviewer".to_string()));
    }

    #[test]
    fn test_codex_no_system_prompt_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            system_prompt: Some("test".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.contains(&"--system-prompt".to_string()));
    }

    // ── Allowed Tools ───────────────────────────────────────

    #[test]
    fn test_claude_allowed_tools() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            allowed_tools: Some("Bash,Read,Edit".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--allowedTools".to_string()));
        assert!(inv.args.contains(&"Bash,Read,Edit".to_string()));
    }

    #[test]
    fn test_gemini_allowed_tools() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            allowed_tools: Some("Bash".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--allowed-tools".to_string()));
        assert!(inv.args.contains(&"Bash".to_string()));
    }

    #[test]
    fn test_codex_no_allowed_tools_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            allowed_tools: Some("Bash".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.contains(&"Bash".to_string()));
    }

    // ── Sandbox ─────────────────────────────────────────────

    #[test]
    fn test_gemini_sandbox() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            sandbox: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--sandbox".to_string()));
    }

    #[test]
    fn test_codex_sandbox_workspace() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            sandbox: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--sandbox".to_string()));
        assert!(inv.args.contains(&"workspace-write".to_string()));
    }

    #[test]
    fn test_claude_no_sandbox_support() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            sandbox: true,
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.iter().any(|a| a.contains("sandbox")));
    }

    // ── Name ────────────────────────────────────────────────

    #[test]
    fn test_claude_session_name() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            name: Some("my-session".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--name".to_string()));
        assert!(inv.args.contains(&"my-session".to_string()));
    }

    #[test]
    fn test_codex_no_name_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            name: Some("test".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.contains(&"--name".to_string()));
    }

    // ── Add Dir ─────────────────────────────────────────────

    #[test]
    fn test_claude_add_dir() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            add_dir: Some("/tmp/extra".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--add-dir".to_string()));
        assert!(inv.args.contains(&"/tmp/extra".to_string()));
    }

    #[test]
    fn test_codex_add_dir() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            add_dir: Some("/tmp/extra".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--add-dir".to_string()));
        assert!(inv.args.contains(&"/tmp/extra".to_string()));
    }

    #[test]
    fn test_gemini_add_dir_is_include_directories() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            add_dir: Some("/tmp/extra".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--include-directories".to_string()));
        assert!(inv.args.contains(&"/tmp/extra".to_string()));
    }

    // ── Approval Mode ───────────────────────────────────────

    #[test]
    fn test_claude_permission_mode() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            approval_mode: Some("plan".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--permission-mode".to_string()));
        assert!(inv.args.contains(&"plan".to_string()));
    }

    #[test]
    fn test_gemini_approval_mode() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            approval_mode: Some("yolo".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--approval-mode".to_string()));
        assert!(inv.args.contains(&"yolo".to_string()));
    }

    #[test]
    fn test_codex_approval_mode() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            approval_mode: Some("never".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"-a".to_string()));
        assert!(inv.args.contains(&"never".to_string()));
    }

    #[test]
    fn test_opencode_no_approval_mode() {
        let config = AgentDefinition::opencode().polyfill;
        let flags = PolyfillFlags {
            approval_mode: Some("auto".into()),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.contains(&"auto".to_string()));
    }

    // ── Worktree ────────────────────────────────────────────

    #[test]
    fn test_claude_worktree_no_name() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            worktree: Some(None),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--worktree".to_string()));
    }

    #[test]
    fn test_claude_worktree_with_name() {
        let config = AgentDefinition::claude().polyfill;
        let flags = PolyfillFlags {
            worktree: Some(Some("feature-x".into())),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--worktree".to_string()));
        assert!(inv.args.contains(&"feature-x".to_string()));
    }

    #[test]
    fn test_gemini_worktree() {
        let config = AgentDefinition::gemini().polyfill;
        let flags = PolyfillFlags {
            worktree: Some(Some("feature-x".into())),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(inv.args.contains(&"--worktree".to_string()));
        assert!(inv.args.contains(&"feature-x".to_string()));
    }

    #[test]
    fn test_codex_no_worktree_support() {
        let config = AgentDefinition::codex().polyfill;
        let flags = PolyfillFlags {
            worktree: Some(Some("feature-x".into())),
            ..default_flags()
        };
        let inv = resolve(&config, &flags, &[]);
        assert!(!inv.args.iter().any(|a| a.contains("worktree")));
    }
}
