//! Claude Unleashed - Unified CLI
//!
//! Single binary that handles:
//! - `cu` / `claude-unleashed` - Launch Claude with wrapper features
//! - `cu tui` / `cui` - TUI for profile/version management
//! - `cu tmux` / `cutx` - Headless tmux mode

mod cli;
mod config;
mod input;
mod launcher;
mod patcher;
mod pixel_art;
mod text_input;
mod tmux;
mod tui;
mod version;

use clap::Parser;
use cli::{Cli, Commands};
use std::env;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    // Check how we were invoked (argv[0])
    let invoked_as = env::args()
        .next()
        .and_then(|arg| {
            Path::new(&arg)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_default();

    // Handle symlink invocations
    match invoked_as.as_str() {
        "cui" => return tui::run(),
        "cutx" => {
            // Pass remaining args to tmux module
            let args: Vec<String> = env::args().skip(1).collect();
            return tmux::run(&args);
        }
        _ => {}
    }

    // Parse CLI arguments
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Tui) => tui::run(),
        Some(Commands::Tmux { args }) => tmux::run(&args),
        Some(Commands::Patch { check }) => {
            if check {
                patcher::check_and_patch()
            } else {
                patcher::patch()
            }
        }
        Some(Commands::Version { list, install }) => {
            if list {
                version::list_versions()
            } else if let Some(ver) = install {
                version::install_version(&ver)
            } else {
                version::show_current()
            }
        }
        None => {
            // Default: launch Claude with wrapper features
            launcher::run(cli.auto, cli.prompt, cli.args)
        }
    }
}
