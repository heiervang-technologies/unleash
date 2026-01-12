//! CLI argument parsing

use clap::{Parser, Subcommand};

/// Claude Unleashed - Extended CLI for Claude Code
#[derive(Parser, Debug)]
#[command(name = "cu")]
#[command(author = "Heiervang Technologies")]
#[command(version)]
#[command(about = "Claude Unleashed - Extended CLI for Claude Code", long_about = None)]
pub struct Cli {
    /// Enable auto mode (Claude won't wait for confirmations)
    #[arg(short, long)]
    pub auto: bool,

    /// Run in headless mode with this prompt
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Additional arguments to pass to Claude
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Launch the TUI for profile and version management
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
}
