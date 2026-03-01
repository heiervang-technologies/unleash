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

/// Unleash - Extended CLI for AI Code Agents
#[derive(Parser, Debug)]
#[command(name = "unleash")]
#[command(author = "Heiervang Technologies")]
#[command(version)]
#[command(about = "Unleash - Extended CLI for AI Code Agents")]
#[command(long_about = r#"Unleash - Extended CLI for AI Code Agents

A wrapper for AI code agents (Claude, Codex, Gemini, OpenCode) with extended features:
  - Self-restart capability for MCP server reloading
  - Plugin system integration (loads from plugins/unleashed/)
  - Automatic onboarding bypass
  - TUI for profile and version management

BINARY STRUCTURE:
  unleash     - CLI entrypoint. Opens TUI with no args.
  unleashed   - Direct agent wrapper entrypoint.
  u           - Alias for unleashed.

USAGE NOTES:
  When you run 'unleash', you'll open the TUI by default to manage profiles.
  You can run a profile directly via 'unleash <profile_name> [args...]'
  (e.g., 'unleash claude --auto', 'unleash work').

  Alternatively, use 'unleashed' or 'u' to directly run your active/default
  profile without TUI overhead (e.g., 'u --auto')."#)]
pub struct Cli {
    /// Output results as JSON (supported by: auth, version)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start Claude Code
    Claude {
        /// Additional arguments to pass to Claude
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Start Codex
    Codex {
        /// Additional arguments to pass to Codex
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Start Gemini CLI
    Gemini {
        /// Additional arguments to pass to Gemini
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Start OpenCode
    OpenCode {
        /// Additional arguments to pass to OpenCode
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

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

    /// Manage code agents (Claude, Codex, Aider)
    Agents {
        #[command(subcommand)]
        action: Option<AgentsAction>,
    },

    /// Run a profile by name (e.g. `unleash work`)
    #[command(external_subcommand)]
    Profile(Vec<String>),
}

#[derive(Subcommand, Debug)]
pub enum HooksAction {
    /// Show Claude Code installation info and registered hooks
    Status,

    /// Install default unleashed hooks
    Install,

    /// Sync hooks from unleashed plugins (use sparingly - plugins loaded via --plugin-dir have their hooks loaded automatically)
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
        /// Agent to check (claude, codex, aider). If omitted, checks all.
        agent: Option<String>,
    },

    /// Update an agent to latest version
    Update {
        /// Agent to update (claude, codex, aider)
        agent: String,
    },

    /// Show detailed info about an agent
    Info {
        /// Agent name (claude, codex, aider)
        agent: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_subcommand_still_parses() {
        let cli = Cli::try_parse_from(["unleash", "claude", "--", "--auto"]).unwrap();
        match cli.command {
            Some(Commands::Claude { args }) => assert_eq!(args, vec!["--auto".to_string()]),
            _ => panic!("expected Claude subcommand"),
        }
    }

    #[test]
    fn test_external_profile_subcommand_parses() {
        let cli = Cli::try_parse_from(["unleash", "work", "--auto", "--foo"]).unwrap();
        match cli.command {
            Some(Commands::Profile(args)) => {
                assert_eq!(
                    args,
                    vec![
                        "work".to_string(),
                        "--auto".to_string(),
                        "--foo".to_string()
                    ]
                );
            }
            _ => panic!("expected external profile subcommand"),
        }
    }
}
