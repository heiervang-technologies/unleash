//! CLI argument parsing

use clap::{Parser, Subcommand};
use std::process::Command;

/// Get the full version information (both Claude Unleashed and Claude Code)
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
        format!(
            "Agent Unleashed: v{}\nClaude Code: {}",
            au_version, claude_version
        )
    } else {
        format!(
            "Agent Unleashed: v{}\nClaude Code: v{}",
            au_version, claude_version
        )
    }
}

/// Agent Unleashed - Extended CLI for AI Code Agents
#[derive(Parser, Debug)]
#[command(name = "au")]
#[command(author = "Heiervang Technologies")]
#[command(version)]
#[command(about = "Agent Unleashed - Extended CLI for AI Code Agents")]
#[command(long_about = r#"Agent Unleashed - Extended CLI for AI Code Agents

A wrapper for AI code agents (Claude, Codex, etc.) with extended features:
  - Self-restart capability for MCP server reloading
  - Plugin system integration (loads from plugins/unleashed/)
  - Auto-patching for unleashed features
  - Automatic onboarding bypass
  - TUI for profile and version management
  - Headless tmux mode for automation

BINARY STRUCTURE:
  au     - Wrapper script that runs agent with plugins and features
  aui    - TUI for profile and version management
  autx   - Headless tmux automation mode

USAGE NOTES:
  When you run 'au', you're using a wrapper script that adds functionality
  to the underlying agent. The wrapper intercepts some flags like --auto and --help.

  For Claude Code's native help: claude --help
  For wrapper-specific help: au --help
  For TUI help: aui --help (or this command)"#)]
pub struct Cli {
    /// Output results as JSON (supported by: auth, version)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start Claude with unleashed features
    Go {
        /// Enable auto mode (Claude won't wait for confirmations)
        #[arg(short, long)]
        auto: bool,

        /// Run in headless mode with this prompt
        #[arg(short, long)]
        prompt: Option<String>,

        /// Additional arguments to pass to Claude
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Launch the TUI for profile and version management
    #[cfg(feature = "tui")]
    #[command(alias = "ui")]
    Tui,

    /// Headless tmux mode for automation
    #[command(alias = "tx")]
    Tmux {
        /// Arguments for tmux subcommand (start, send, read, wait, attach, stop, status)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Patch Claude Code for auto mode
    Patch {
        /// Just check if patching is needed (don't apply)
        #[arg(short, long)]
        check: bool,
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
