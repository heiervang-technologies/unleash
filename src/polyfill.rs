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
    // Only apply if neither resume nor continue already set the subcommand
    if let Some(ref prompt) = flags.headless {
        if subcommand_prefix.is_empty() {
            let (h_args, h_sub) = config.get_headless_invocation(prompt);
            args.extend(h_args);
            subcommand_prefix.extend(h_sub);
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

    #[test]
    fn test_claude_yolo_deduplication() {
        let config = AgentDefinition::claude().polyfill;
        let existing = vec!["--dangerously-skip-permissions".to_string()];
        let inv = resolve(&config, &default_flags(), &existing);
        // Should not add it again
        assert_eq!(
            inv.args
                .iter()
                .filter(|a| *a == "--dangerously-skip-permissions")
                .count(),
            0
        );
    }

    #[test]
    fn test_gemini_yolo_resolution() {
        let config = AgentDefinition::gemini().polyfill;
        let inv = resolve(&config, &default_flags(), &[]);
        assert!(inv.args.contains(&"--yolo".to_string()));
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
}
