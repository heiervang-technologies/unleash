//! CLI argument parsing

use clap::{Parser, Subcommand};
use std::process::Command;

/// Get version information. Prints unleash version instantly,
/// then fetches agent CLI version (avoids blocking startup for non-version commands).
pub fn get_full_version() -> String {
    let au_version = env!("CARGO_PKG_VERSION");

    // Check which agent is configured
    let agent_cmd = std::env::var("AGENT_CMD").unwrap_or_else(|_| "claude".to_string());
    let agent_label = match agent_cmd.as_str() {
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "opencode" => "OpenCode",
        _ => "Claude Code",
    };

    // Try to get agent CLI version (only runs when --version is actually passed)
    let agent_version = Command::new(&agent_cmd)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let version_str = String::from_utf8_lossy(&output.stdout);
                version_str
                    .lines()
                    .find(|line| line.contains('.') && line.chars().any(|c| c.is_ascii_digit()))
                    .map(|line| line.trim().replace(" (Claude Code)", ""))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "not installed".to_string());

    if agent_version == "not installed" {
        format!(
            "unleash: v{}\n{}: {}",
            au_version, agent_label, agent_version
        )
    } else {
        format!(
            "unleash: v{}\n{}: v{}",
            au_version, agent_label, agent_version
        )
    }
}

/// Shared polyfill flags for all agent profiles
#[derive(clap::Args, Debug, Clone)]
pub struct PolyfillArgs {
    /// Bypass all permission/approval checks (enabled by default in unleash)
    #[arg(long, hide = true)]
    pub yolo: bool,

    /// Restore approval prompts (disables default permission bypass)
    #[arg(long)]
    pub safe: bool,

    /// Run in non-interactive (headless) mode with the given prompt
    #[arg(short = 'p', long)]
    pub prompt: Option<String>,

    /// Model to use for the session
    #[arg(short, long)]
    pub model: Option<String>,

    /// Continue the most recent session
    #[arg(short, long = "continue")]
    pub continue_session: bool,

    /// Resume a session (by ID or open picker if no ID given)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "")]
    pub resume: Option<String>,

    /// Fork the session (use with --continue or --resume)
    #[arg(long)]
    pub fork: bool,

    /// Enable auto-mode (autonomous operation)
    #[arg(short, long)]
    pub auto: bool,

    /// Keep the conversation natively in UCF (Hub) format
    #[arg(short = 'u', long)]
    pub ucf: Option<String>,

    /// Reasoning effort level (e.g., "high", "low")
    #[arg(short = 'e', long)]
    pub effort: Option<String>,

    /// Load a conversation from another CLI (e.g., --crossload codex:hidden-wolf)
    /// Converts the session history and resumes it in the target agent.
    /// Without a value, opens an interactive session picker.
    #[arg(short = 'x', long, num_args = 0..=1, default_missing_value = "")]
    pub crossload: Option<String>,

    /// Show the resolved command without executing it
    #[arg(long)]
    pub dry_run: bool,
}

impl PolyfillArgs {
    /// Parse polyfill flags from raw args (for external_subcommand path).
    /// Returns (polyfill_args, passthrough_args) where passthrough_args are
    /// everything after `--` plus any unrecognized flags.
    pub fn parse_from_raw(args: &[String]) -> (Self, Vec<String>) {
        let mut polyfill = PolyfillArgs {
            yolo: false,
            safe: false,
            prompt: None,
            model: None,
            continue_session: false,
            resume: None,
            fork: false,
            auto: false,
            ucf: None,
            effort: None,
            crossload: None,
            dry_run: false,
        };
        let mut passthrough = Vec::new();
        let mut hit_separator = false;
        let mut last_value_flag: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];

            // Everything after -- is passthrough
            if arg == "--" {
                hit_separator = true;
                i += 1;
                continue;
            }
            if hit_separator {
                passthrough.push(arg.clone());
                i += 1;
                continue;
            }

            last_value_flag = None;
            match arg.as_str() {
                "--safe" => polyfill.safe = true,
                "--yolo" => polyfill.yolo = true,
                "--fork" => polyfill.fork = true,
                "-c" | "--continue" => polyfill.continue_session = true,
                "-a" | "--auto" => polyfill.auto = true,
                "--dry-run" => polyfill.dry_run = true,
                "-p" | "--prompt" => {
                    if let Some(val) = args.get(i + 1).filter(|v| !v.starts_with('-')) {
                        polyfill.prompt = Some(val.clone());
                        i += 1;
                        last_value_flag = None;
                    } else {
                        last_value_flag = Some(arg.clone());
                    }
                }
                "-m" | "--model" => {
                    if let Some(val) = args.get(i + 1).filter(|v| !v.starts_with('-')) {
                        polyfill.model = Some(val.clone());
                        i += 1;
                        last_value_flag = None;
                    } else {
                        last_value_flag = Some(arg.clone());
                    }
                }
                "-e" | "--effort" => {
                    if let Some(val) = args.get(i + 1).filter(|v| !v.starts_with('-')) {
                        polyfill.effort = Some(val.clone());
                        i += 1;
                        last_value_flag = None;
                    } else {
                        last_value_flag = Some(arg.clone());
                    }
                }
                "-x" | "--crossload" => {
                    if let Some(val) = args.get(i + 1) {
                        if !val.starts_with('-') {
                            polyfill.crossload = Some(val.clone());
                            i += 1;
                        } else {
                            polyfill.crossload = Some(String::new());
                        }
                    } else {
                        polyfill.crossload = Some(String::new());
                    }
                }
                "-r" | "--resume" => {
                    // Check if next arg is a value (not a flag)
                    if let Some(val) = args.get(i + 1) {
                        if !val.starts_with('-') {
                            polyfill.resume = Some(val.clone());
                            i += 1;
                        } else {
                            polyfill.resume = Some(String::new()); // picker mode
                        }
                    } else {
                        polyfill.resume = Some(String::new()); // picker mode
                    }
                }
                _ => {
                    // Check for --prompt=value, --model=value, --resume=value
                    if let Some(val) = arg.strip_prefix("--prompt=") {
                        polyfill.prompt = Some(val.to_string());
                    } else if let Some(val) = arg.strip_prefix("--model=") {
                        polyfill.model = Some(val.to_string());
                    } else if let Some(val) = arg.strip_prefix("--resume=") {
                        polyfill.resume = Some(val.to_string());
                    } else if let Some(val) = arg.strip_prefix("--effort=") {
                        polyfill.effort = Some(val.to_string());
                    } else if let Some(val) = arg.strip_prefix("--ucf=") {
                        polyfill.ucf = Some(val.to_string());
                    } else if let Some(val) = arg.strip_prefix("--crossload=") {
                        polyfill.crossload = Some(val.to_string());
                    } else {
                        // Unrecognized — pass through to agent
                        passthrough.push(arg.clone());
                    }
                }
            }
            i += 1;
        }

        // Validate: warn if a value-taking flag was the last arg without a value
        if let Some(ref flag) = last_value_flag {
            eprintln!("Warning: {} requires a value (e.g. {} <value>)", flag, flag);
        }

        // Validate: --fork has no effect without --continue or --resume
        if polyfill.fork && !polyfill.continue_session && polyfill.resume.is_none() {
            eprintln!("Warning: --fork has no effect without --continue or --resume");
        }

        (polyfill, passthrough)
    }

    /// Convert CLI args into polyfill flags for the resolver, merging with profile defaults.
    /// CLI flags always override profile defaults. An info message is logged when an override occurs.
    pub fn to_polyfill_flags(
        &self,
        profile_defaults: &crate::config::ProfileDefaults,
    ) -> crate::polyfill::PolyfillFlags {
        let resume = if let Some(ref id) = self.resume {
            if id.is_empty() {
                Some(None) // picker mode
            } else {
                Some(Some(id.clone())) // specific session
            }
        } else {
            None
        };

        // Resolve model: CLI > profile default
        let model = if let Some(ref m) = self.model {
            if let Some(ref default_model) = profile_defaults.model {
                if m != default_model {
                    eprintln!(
                        "\x1b[34minfo:\x1b[0m overriding profile model '{}' with CLI flag '{}'",
                        default_model, m
                    );
                }
            }
            Some(m.clone())
        } else {
            profile_defaults.model.clone()
        };

        // Resolve effort: CLI > profile default
        let effort = if let Some(ref e) = self.effort {
            if let Some(ref default_effort) = profile_defaults.effort {
                if e != default_effort {
                    eprintln!(
                        "\x1b[34minfo:\x1b[0m overriding profile effort '{}' with CLI flag '{}'",
                        default_effort, e
                    );
                }
            }
            Some(e.clone())
        } else {
            profile_defaults.effort.clone()
        };

        // Resolve safe/yolo: CLI flags override profile default
        let safe = if self.safe {
            true
        } else if self.yolo && profile_defaults.safe {
            eprintln!("\x1b[34minfo:\x1b[0m overriding profile safe mode with CLI flag '--yolo'");
            false
        } else {
            profile_defaults.safe
        };

        crate::polyfill::PolyfillFlags {
            yolo: !safe,
            safe,
            headless: self.prompt.clone(),
            model,
            continue_session: self.continue_session,
            resume,
            fork: self.fork,
            effort,
        }
    }
}

/// unleash - Extended CLI for AI Code Agents
#[derive(Parser, Debug)]
#[command(name = "unleash")]
#[command(author = "Heiervang Technologies")]
#[command(version)]
#[command(
    about = "unleash - Extended CLI for AI Code Agents\n\nRun a profile:  unleash <profile> [flags] [-- passthrough]\nDefault profiles: claude, codex, gemini, opencode\n\nRun 'unleash <profile> --help' for unified flag details."
)]
#[command(long_about = r#"unleash - Extended CLI for AI Code Agents

A wrapper for AI code agents (Claude, Codex, Gemini, OpenCode) with extended features:
  - Unified flags that work across all agents (polyfill layer)
  - Self-restart capability for MCP server reloading
  - Plugin system integration (loads from plugins/bundled/)
  - Auto-mode via Stop hook + flag file system
  - TUI for profile and version management

ARGUMENT LAYERS:
  Arguments BEFORE '--' are unified flags handled by unleash.
  Arguments AFTER '--' are passed directly to the agent CLI.

  unleash claude -m opus -- --effort high
         ^^^^^^ ^^^^^^^^    ^^^^^^^^^^^^
         Profile  Unified   Passthrough (agent-specific)

USAGE:
  unleash              Opens TUI for profile and version management
  unleash <profile>    Run a profile (claude, codex, gemini, opencode, or custom)

UNIFIED FLAGS (before --):
  --safe               Restore approval prompts (permissions bypassed by default)
  -p, --prompt       Run non-interactively with a given prompt
  -m, --model          Model selection
  -c, --continue       Continue most recent session
  -r, --resume [id]    Resume session by ID or open picker
  --fork               Fork the session
  -a, --auto           Enable auto-mode
  -e, --effort <LEVEL> Reasoning effort level (e.g., high, low)"#)]
pub struct Cli {
    /// Output results as JSON (supported by: auth, version, sessions, agents info, agents list)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Manage Claude Code versions
    Version {
        /// List available versions
        #[arg(short, long)]
        list: bool,

        /// Install a specific version
        #[arg(short, long)]
        install: Option<String>,
    },

    /// Check Claude Code authentication status
    #[command(alias = "auth-check")]
    Auth {
        /// Show verbose output with debugging information
        #[arg(short, long)]
        verbose: bool,

        /// Quiet mode - only return exit code, no output
        #[arg(short, long)]
        quiet: bool,
    },

    /// Manage Claude Code hooks
    Hooks {
        #[command(subcommand)]
        action: Option<HooksAction>,
    },

    /// Manage code agents (Claude, Codex, Gemini, OpenCode)
    Agents {
        #[command(subcommand)]
        action: Option<AgentsAction>,
    },

    /// Install an agent CLI for the first time
    ///
    /// Examples:
    ///   unleash install gemini       # Install Gemini CLI
    ///   unleash install codex claude # Install Codex and Claude
    ///   unleash install --all        # Install all agent CLIs
    Install {
        /// Agents to install (e.g. claude, codex, gemini, opencode)
        #[arg(conflicts_with = "all")]
        agents: Vec<String>,

        /// Install all agent CLIs
        #[arg(short, long)]
        all: bool,
    },

    /// Uninstall an agent CLI
    ///
    /// Examples:
    ///   unleash uninstall gemini       # Uninstall Gemini CLI
    ///   unleash uninstall --all        # Uninstall all agent CLIs
    Uninstall {
        /// Agents to uninstall (e.g. claude, codex, gemini, opencode)
        #[arg(conflicts_with = "all")]
        agents: Vec<String>,

        /// Uninstall all agent CLIs
        #[arg(short, long)]
        all: bool,
    },

    /// Update unleash and/or agent CLIs (only updates already-installed agents)
    ///
    /// No args: update unleash itself
    /// -c/--clis: update all installed agent CLIs
    /// -a/--all: update unleash + all installed agent CLIs
    /// Positional args: update specific agents (e.g. 'unleash update claude codex')
    Update {
        /// Specific agents to update (e.g. claude, codex, gemini, opencode)
        #[arg(conflicts_with_all = ["clis", "all"])]
        agents: Vec<String>,

        /// Update all agent CLIs (not unleash itself)
        #[arg(short, long, conflicts_with = "all")]
        clis: bool,

        /// Update everything (unleash + all agent CLIs)
        #[arg(short, long, conflicts_with = "clis")]
        all: bool,

        /// Only check for updates, don't install
        #[arg(long)]
        check: bool,
    },

    /// List conversation sessions across all agent CLIs
    Sessions {
        /// Filter by CLI (claude, codex, gemini, opencode)
        #[arg(short, long)]
        cli: Option<String>,

        /// Find a specific session by name/ID (supports cli:name format)
        #[arg(short, long)]
        find: Option<String>,
    },

    /// Convert conversation history between CLI formats
    Convert {
        /// Source format (claude, codex, gemini, opencode, hub)
        #[arg(long)]
        from: String,

        /// Target format (claude, codex, gemini, opencode, hub). Defaults to hub.
        #[arg(long, default_value = "hub")]
        to: String,

        /// Input file path
        input: String,

        /// Output file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,

        /// Verify lossless round-trip instead of converting
        #[arg(long)]
        verify: bool,
    },

    /// Run a profile by name (catches any unknown subcommand as a profile name)
    #[command(external_subcommand)]
    Profile(Vec<String>),
}

#[derive(Subcommand, Debug)]
pub enum HooksAction {
    /// Show Claude Code installation info and registered hooks
    Status,

    /// Install default hooks
    Install,

    /// Sync hooks from bundled plugins (use sparingly - plugins loaded via --plugin-dir have their hooks loaded automatically)
    Sync,

    /// List all registered hooks
    List,

    /// Add a custom hook
    Add {
        /// Hook event (Stop, PreToolUse, PostToolUse, PreCompact, etc.)
        event: String,

        /// Command to execute
        command: String,

        /// Optional matcher pattern
        #[arg(short, long)]
        matcher: Option<String>,
    },

    /// Remove a hook by command
    Remove {
        /// Hook event
        event: String,

        /// Command to remove
        command: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum AgentsAction {
    /// Show status of all agents (installed versions, updates available)
    Status,

    /// List available agents
    List,

    /// Check for updates
    Check {
        /// Agent to check (claude, codex, gemini, opencode). If omitted, checks all.
        agent: Option<String>,
    },

    /// Update an agent to latest version
    Update {
        /// Agent to update (claude, codex, gemini, opencode)
        agent: String,
    },

    /// Show detailed info about an agent
    Info {
        /// Agent name (claude, codex, gemini, opencode)
        agent: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Clap routing: profiles go through external_subcommand ---

    #[test]
    fn test_profile_captures_all_args() {
        let cli = Cli::try_parse_from(["unleash", "claude", "-m", "opus", "--safe"]).unwrap();
        match cli.command {
            Some(Commands::Profile(args)) => {
                assert_eq!(args[0], "claude");
                assert!(args.contains(&"-m".to_string()));
                assert!(args.contains(&"opus".to_string()));
                assert!(args.contains(&"--safe".to_string()));
            }
            _ => panic!("expected Profile subcommand"),
        }
    }

    #[test]
    fn test_profile_with_passthrough() {
        let cli =
            Cli::try_parse_from(["unleash", "claude", "-m", "opus", "--", "--effort", "high"])
                .unwrap();
        match cli.command {
            Some(Commands::Profile(args)) => {
                assert_eq!(args[0], "claude");
                // external_subcommand captures -- and everything after
                assert!(args.contains(&"--".to_string()));
            }
            _ => panic!("expected Profile subcommand"),
        }
    }

    // --- parse_from_raw: manual polyfill flag extraction ---

    #[test]
    fn test_parse_safe_flag() {
        let args: Vec<String> = vec!["--safe".into()];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.safe);
        assert!(passthrough.is_empty());
    }

    #[test]
    fn test_parse_prompt() {
        let args: Vec<String> = vec!["-p".into(), "fix tests".into()];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.prompt, Some("fix tests".to_string()));
        assert!(passthrough.is_empty());
    }

    #[test]
    fn test_parse_model() {
        let args: Vec<String> = vec!["-m".into(), "opus".into()];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert!(passthrough.is_empty());
    }

    #[test]
    fn test_parse_continue() {
        let args: Vec<String> = vec!["-c".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.continue_session);
    }

    #[test]
    fn test_parse_resume_with_id() {
        let args: Vec<String> = vec!["-r".into(), "abc123".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.resume, Some("abc123".to_string()));
    }

    #[test]
    fn test_parse_resume_without_id() {
        let args: Vec<String> = vec!["--resume".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.resume, Some("".to_string()));
    }

    #[test]
    fn test_parse_fork_with_continue() {
        let args: Vec<String> = vec!["-c".into(), "--fork".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.continue_session);
        assert!(polyfill.fork);
    }

    #[test]
    fn test_parse_auto() {
        let args: Vec<String> = vec!["--auto".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.auto);
    }

    #[test]
    fn test_parse_dry_run() {
        let args: Vec<String> = vec!["--dry-run".into(), "-m".into(), "opus".into()];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.dry_run);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert!(passthrough.is_empty());
    }

    #[test]
    fn test_parse_passthrough_separator() {
        let args: Vec<String> = vec![
            "-m".into(),
            "opus".into(),
            "--".into(),
            "--effort".into(),
            "high".into(),
        ];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert_eq!(
            passthrough,
            vec!["--effort".to_string(), "high".to_string()]
        );
    }

    #[test]
    fn test_parse_unrecognized_flags_pass_through() {
        let args: Vec<String> = vec![
            "-m".into(),
            "opus".into(),
            "--verbose".into(),
            "--debug".into(),
        ];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert_eq!(
            passthrough,
            vec!["--verbose".to_string(), "--debug".to_string()]
        );
    }

    #[test]
    fn test_parse_equals_syntax() {
        let args: Vec<String> = vec!["--model=opus".into(), "--prompt=fix bug".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert_eq!(polyfill.prompt, Some("fix bug".to_string()));
    }

    fn no_defaults() -> crate::config::ProfileDefaults {
        crate::config::ProfileDefaults::default()
    }

    #[test]
    fn test_safe_to_polyfill_flags() {
        let args: Vec<String> = vec!["--safe".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let flags = polyfill.to_polyfill_flags(&no_defaults());
        assert!(!flags.yolo);
        assert!(flags.safe);
    }

    #[test]
    fn test_default_is_yolo() {
        let args: Vec<String> = vec![];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let flags = polyfill.to_polyfill_flags(&no_defaults());
        assert!(flags.yolo);
        assert!(!flags.safe);
    }

    #[test]
    fn test_effort_flag() {
        let args: Vec<String> = vec!["-e".into(), "high".into()];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.effort, Some("high".to_string()));
        assert!(passthrough.is_empty());
    }

    #[test]
    fn test_effort_equals_syntax() {
        let args: Vec<String> = vec!["--effort=low".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.effort, Some("low".to_string()));
    }

    #[test]
    fn test_profile_defaults_model_used_when_no_cli_flag() {
        let args: Vec<String> = vec![];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let defaults = crate::config::ProfileDefaults {
            model: Some("opus".to_string()),
            ..Default::default()
        };
        let flags = polyfill.to_polyfill_flags(&defaults);
        assert_eq!(flags.model, Some("opus".to_string()));
    }

    #[test]
    fn test_cli_model_overrides_profile_default() {
        let args: Vec<String> = vec!["-m".into(), "sonnet".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let defaults = crate::config::ProfileDefaults {
            model: Some("opus".to_string()),
            ..Default::default()
        };
        let flags = polyfill.to_polyfill_flags(&defaults);
        assert_eq!(flags.model, Some("sonnet".to_string()));
    }

    #[test]
    fn test_profile_defaults_effort_used_when_no_cli_flag() {
        let args: Vec<String> = vec![];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let defaults = crate::config::ProfileDefaults {
            effort: Some("high".to_string()),
            ..Default::default()
        };
        let flags = polyfill.to_polyfill_flags(&defaults);
        assert_eq!(flags.effort, Some("high".to_string()));
    }

    #[test]
    fn test_profile_safe_default() {
        let args: Vec<String> = vec![];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let defaults = crate::config::ProfileDefaults {
            safe: true,
            ..Default::default()
        };
        let flags = polyfill.to_polyfill_flags(&defaults);
        assert!(flags.safe);
        assert!(!flags.yolo);
    }

    #[test]
    fn test_yolo_overrides_profile_safe() {
        let args: Vec<String> = vec!["--yolo".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let defaults = crate::config::ProfileDefaults {
            safe: true,
            ..Default::default()
        };
        let flags = polyfill.to_polyfill_flags(&defaults);
        assert!(!flags.safe);
        assert!(flags.yolo);
    }

    // --- Validation warning tests ---

    #[test]
    fn test_fork_without_continue_or_resume_is_noop() {
        // --fork alone should still parse but the flag has no effect
        let args: Vec<String> = vec!["--fork".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.fork);
        assert!(!polyfill.continue_session);
        assert!(polyfill.resume.is_none());
    }

    #[test]
    fn test_fork_with_continue_is_valid() {
        let args: Vec<String> = vec!["--fork".into(), "-c".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.fork);
        assert!(polyfill.continue_session);
    }

    #[test]
    fn test_fork_with_resume_is_valid() {
        let args: Vec<String> = vec!["--fork".into(), "--resume".into(), "abc".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.fork);
        assert!(polyfill.resume.is_some());
    }

    #[test]
    fn test_model_without_value_leaves_none() {
        // -m at end of args with no value
        let args: Vec<String> = vec!["-m".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.model.is_none());
    }

    #[test]
    fn test_prompt_without_value_leaves_none() {
        // -p at end of args with no value
        let args: Vec<String> = vec!["-p".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(polyfill.prompt.is_none());
    }

    #[test]
    fn test_model_followed_by_flag_not_consumed_as_value() {
        // -m --safe should NOT treat --safe as the model name
        let args: Vec<String> = vec!["-m".into(), "--safe".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(
            polyfill.model.is_none(),
            "model should not consume --safe as its value"
        );
        assert!(polyfill.safe, "--safe should still be parsed");
    }

    #[test]
    fn test_prompt_followed_by_flag_not_consumed_as_value() {
        // -p --continue should NOT treat --continue as the prompt
        let args: Vec<String> = vec!["-p".into(), "--continue".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(
            polyfill.prompt.is_none(),
            "prompt should not consume --continue as its value"
        );
        assert!(
            polyfill.continue_session,
            "--continue should still be parsed"
        );
    }

    #[test]
    fn test_effort_followed_by_flag_not_consumed_as_value() {
        // -e --auto should NOT treat --auto as the effort level
        let args: Vec<String> = vec!["-e".into(), "--auto".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert!(
            polyfill.effort.is_none(),
            "effort should not consume --auto as its value"
        );
        assert!(polyfill.auto, "--auto should still be parsed");
    }
}
