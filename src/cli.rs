//! CLI argument parsing

use clap::{Parser, Subcommand};
use std::process::Command;

/// Get the full version information (both Unleash and Claude Code)
pub fn get_full_version() -> String {
    let au_version = env!("CARGO_PKG_VERSION");

    // Try to get Claude Code version
    let claude_version = Command::new("claude")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let version_str = String::from_utf8_lossy(&output.stdout);
                // Parse "2.1.5 (Claude Code)" -> "2.1.5"
                version_str
                    .lines()
                    .next()
                    .map(|line| line.trim().replace(" (Claude Code)", ""))
            } else {
                None
            }
        })
        .unwrap_or_else(|| "not installed".to_string());

    if claude_version == "not installed" {
        format!("Unleash: v{}\nClaude Code: {}", au_version, claude_version)
    } else {
        format!("Unleash: v{}\nClaude Code: v{}", au_version, claude_version)
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
        };
        let mut passthrough = Vec::new();
        let mut hit_separator = false;

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

            match arg.as_str() {
                "--safe" => polyfill.safe = true,
                "--yolo" => polyfill.yolo = true,
                "--fork" => polyfill.fork = true,
                "-c" | "--continue" => polyfill.continue_session = true,
                "-a" | "--auto" => polyfill.auto = true,
                "-p" | "--prompt" => {
                    if let Some(val) = args.get(i + 1) {
                        polyfill.prompt = Some(val.clone());
                        i += 1;
                    }
                }
                "-m" | "--model" => {
                    if let Some(val) = args.get(i + 1) {
                        polyfill.model = Some(val.clone());
                        i += 1;
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
                    } else {
                        // Unrecognized — pass through to agent
                        passthrough.push(arg.clone());
                    }
                }
            }
            i += 1;
        }

        (polyfill, passthrough)
    }

    /// Convert CLI args into polyfill flags for the resolver
    pub fn to_polyfill_flags(&self) -> crate::polyfill::PolyfillFlags {
        let resume = if let Some(ref id) = self.resume {
            if id.is_empty() {
                Some(None) // picker mode
            } else {
                Some(Some(id.clone())) // specific session
            }
        } else {
            None
        };

        crate::polyfill::PolyfillFlags {
            // yolo is true by default unless --safe is passed
            yolo: !self.safe,
            safe: self.safe,
            headless: self.prompt.clone(),
            model: self.model.clone(),
            continue_session: self.continue_session,
            resume,
            fork: self.fork,
        }
    }
}

/// Unleash - Extended CLI for AI Code Agents
#[derive(Parser, Debug)]
#[command(name = "unleash")]
#[command(author = "Heiervang Technologies")]
#[command(version)]
#[command(about = "Unleash - Extended CLI for AI Code Agents\n\nRun a profile:  unleash <profile> [flags] [-- passthrough]\nDefault profiles: claude, codex, gemini, opencode\n\nRun 'unleash <profile> --help' for unified flag details.")]
#[command(long_about = r#"Unleash - Extended CLI for AI Code Agents

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
  -a, --auto           Enable auto-mode"#)]
pub struct Cli {
    /// Output results as JSON (supported by: auth, version)
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

    /// Update agent CLIs to latest versions with parallel progress
    Update {
        /// Specific agents to update (omit for all installed agents)
        agents: Vec<String>,

        /// Only check for updates, don't install
        #[arg(long)]
        check: bool,

        /// Also update unleash itself
        #[arg(long = "self")]
        update_self: bool,
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
        let cli = Cli::try_parse_from([
            "unleash", "claude", "-m", "opus", "--", "--effort", "high",
        ]).unwrap();
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
    fn test_parse_passthrough_separator() {
        let args: Vec<String> = vec![
            "-m".into(), "opus".into(), "--".into(), "--effort".into(), "high".into(),
        ];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert_eq!(passthrough, vec!["--effort".to_string(), "high".to_string()]);
    }

    #[test]
    fn test_parse_unrecognized_flags_pass_through() {
        let args: Vec<String> = vec![
            "-m".into(), "opus".into(), "--verbose".into(), "--debug".into(),
        ];
        let (polyfill, passthrough) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert_eq!(passthrough, vec!["--verbose".to_string(), "--debug".to_string()]);
    }

    #[test]
    fn test_parse_equals_syntax() {
        let args: Vec<String> = vec!["--model=opus".into(), "--prompt=fix bug".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        assert_eq!(polyfill.model, Some("opus".to_string()));
        assert_eq!(polyfill.prompt, Some("fix bug".to_string()));
    }

    #[test]
    fn test_safe_to_polyfill_flags() {
        let args: Vec<String> = vec!["--safe".into()];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let flags = polyfill.to_polyfill_flags();
        assert!(!flags.yolo);
        assert!(flags.safe);
    }

    #[test]
    fn test_default_is_yolo() {
        let args: Vec<String> = vec![];
        let (polyfill, _) = PolyfillArgs::parse_from_raw(&args);
        let flags = polyfill.to_polyfill_flags();
        assert!(flags.yolo);
        assert!(!flags.safe);
    }
}
