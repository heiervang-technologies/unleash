//! Claude launcher with wrapper features
//!
//! Implements the wrapper loop that enables:
//! - Self-restart capability (restart-claude command)
//! - Plugin system integration
//! - Auto-patching for unleashed features
//! - Process management

use crate::config::ProfileManager;
use crate::patcher;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitStatus};
use which::which;

/// Environment variable set when running under the wrapper
pub const UNLEASHED_ENV_VAR: &str = "CLAUDE_UNLEASHED";

/// Cache directory for restart triggers
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("claude-unleashed/process-restart")
}

/// Run Claude with wrapper features
pub fn run(auto_mode: bool, prompt: Option<String>, extra_args: Vec<String>) -> io::Result<()> {
    // Check and apply patches if needed
    if let Err(e) = patcher::check_and_patch() {
        eprintln!("Warning: Failed to check/apply patches: {}", e);
    }

    // Find claude command
    let claude_cmd = find_claude_command()?;

    // Setup wrapper environment
    let wrapper_pid = std::process::id();
    let trigger_file = cache_dir().join(format!("restart-trigger-{}", wrapper_pid));
    let message_file = cache_dir().join(format!("restart-message-{}", wrapper_pid));

    // Ensure cache directory exists
    fs::create_dir_all(cache_dir())?;

    // Clean up stale trigger files
    let _ = fs::remove_file(&trigger_file);
    let _ = fs::remove_file(&message_file);

    // Load profile environment variables
    let profile_env = load_profile_env()?;

    // Check authentication on first run
    check_authentication();

    let mut restart_count = 0;

    loop {
        // Clear trigger file before starting
        let _ = fs::remove_file(&trigger_file);

        // Build command arguments
        let mut args = extra_args.clone();

        // If this is a restart, add --continue and message
        if restart_count > 0 {
            if !args.iter().any(|a| a == "--continue" || a == "--resume") {
                args.insert(0, "--dangerously-skip-permissions".to_string());
                args.insert(0, "--continue".to_string());
            }

            // Check for custom restart message
            let restart_msg = if message_file.exists() {
                let msg = fs::read_to_string(&message_file).unwrap_or_else(|_| "RESURRECTED.".to_string());
                let _ = fs::remove_file(&message_file);
                msg
            } else {
                "RESURRECTED.".to_string()
            };

            if !restart_msg.is_empty() {
                args.push(restart_msg);
            }
        }

        // Add prompt if provided
        if let Some(ref p) = prompt {
            args.push("-p".to_string());
            args.push(p.clone());
        }

        // Find plugin directories
        let plugin_args = find_plugins();

        // Build and run command
        let status = run_claude(
            &claude_cmd,
            &args,
            &plugin_args,
            &profile_env,
            wrapper_pid,
            auto_mode,
        )?;

        // Check if restart was requested
        if trigger_file.exists() {
            let _ = fs::remove_file(&trigger_file);
            restart_count += 1;
            std::thread::sleep(std::time::Duration::from_millis(300));
            continue;
        }

        // Check exit status
        let exit_code = status.code().unwrap_or(1);

        // Treat SIGTERM (143 = 128 + 15) as clean exit
        if exit_code == 143 {
            return Ok(());
        }

        // Normal exit
        std::process::exit(exit_code);
    }
}

/// Find the claude command
fn find_claude_command() -> io::Result<PathBuf> {
    // Check CLAUDE_CMD environment variable
    if let Ok(cmd) = env::var("CLAUDE_CMD") {
        return Ok(PathBuf::from(cmd));
    }

    // Try to find 'claude' in PATH
    which("claude").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not find 'claude' command. Make sure Claude Code is installed.",
        )
    })
}

/// Load environment variables from current profile
fn load_profile_env() -> io::Result<HashMap<String, String>> {
    let profile_manager = ProfileManager::new()?;
    let config = profile_manager.load_app_config().unwrap_or_default();
    let profiles = profile_manager.load_all_profiles().unwrap_or_default();

    let profile = profiles
        .iter()
        .find(|p| p.name == config.current_profile)
        .or_else(|| profiles.first());

    Ok(profile.map(|p| p.env.clone()).unwrap_or_default())
}

/// Find plugin directories
fn find_plugins() -> Vec<String> {
    let mut args = Vec::new();

    // Check repo location (for development)
    let repo_plugins = PathBuf::from("plugins/unleashed");
    if repo_plugins.exists() {
        if let Ok(entries) = fs::read_dir(&repo_plugins) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    args.push("--plugin-dir".to_string());
                    args.push(entry.path().to_string_lossy().to_string());
                }
            }
        }
    }

    // Check ~/.local/share/claude-unleashed/plugins
    if let Some(data_dir) = dirs::data_local_dir() {
        let plugins_dir = data_dir.join("claude-unleashed/plugins");
        if plugins_dir.exists() {
            if let Ok(entries) = fs::read_dir(&plugins_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        args.push("--plugin-dir".to_string());
                        args.push(entry.path().to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    args
}

/// Run claude command with full configuration
fn run_claude(
    claude_cmd: &PathBuf,
    args: &[String],
    plugin_args: &[String],
    profile_env: &HashMap<String, String>,
    wrapper_pid: u32,
    auto_mode: bool,
) -> io::Result<ExitStatus> {
    let mut cmd = Command::new(claude_cmd);

    // Set environment variables from profile
    for (key, value) in profile_env {
        cmd.env(key, value);
    }

    // Set wrapper environment variables
    cmd.env(UNLEASHED_ENV_VAR, "1");
    cmd.env("CLAUDE_WRAPPER_PID", wrapper_pid.to_string());

    // Set auto mode if requested
    if auto_mode {
        cmd.env("CLAUDE_AUTO_MODE", "1");

        // Create auto-mode marker file
        let auto_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("claude-unleashed/auto-mode");
        let _ = fs::create_dir_all(&auto_dir);
        let _ = fs::write(
            auto_dir.join(format!("active-{}", wrapper_pid)),
            "auto-start",
        );
        println!("Auto mode activated on startup");
    }

    // Set timeout environment variables (extended timeouts)
    if env::var("BASH_DEFAULT_TIMEOUT_MS").is_err() {
        cmd.env("BASH_DEFAULT_TIMEOUT_MS", "999999999");
    }
    if env::var("BASH_MAX_TIMEOUT_MS").is_err() {
        cmd.env("BASH_MAX_TIMEOUT_MS", "999999999");
    }
    if env::var("MCP_TOOL_TIMEOUT").is_err() {
        cmd.env("MCP_TOOL_TIMEOUT", "999999999");
    }

    // Add plugin arguments
    cmd.args(plugin_args);

    // Add --dangerously-skip-permissions (required for hooks to work)
    cmd.arg("--dangerously-skip-permissions");

    // Add user arguments
    cmd.args(args);

    cmd.status()
}

/// Check if authentication is configured
fn check_authentication() {
    // Check OAuth token
    if env::var("CLAUDE_CODE_OAUTH_TOKEN").is_ok() {
        println!("\x1b[32m✓\x1b[0m Using OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable");
        return;
    }

    // Check credentials file
    if let Some(home) = dirs::home_dir() {
        let creds_file = home.join(".claude/.credentials.json");
        if creds_file.exists() {
            println!("\x1b[32m✓\x1b[0m Using credentials from ~/.claude/.credentials.json");
            return;
        }
    }

    // macOS Keychain check would go here (not easily done in pure Rust)

    // No authentication found
    eprintln!();
    eprintln!("\x1b[33m⚠ WARNING: Claude Code authentication not configured\x1b[0m");
    eprintln!();
    eprintln!("To authenticate, you have two options:");
    eprintln!();
    eprintln!("1. Generate a long-lived OAuth token (recommended for automation):");
    eprintln!("   Run: claude setup-token");
    eprintln!("   Then export: export CLAUDE_CODE_OAUTH_TOKEN=<your-token>");
    eprintln!();
    eprintln!("2. Authenticate interactively:");
    eprintln!("   Run: claude");
    eprintln!("   Follow the browser authentication flow");
    eprintln!();
    eprintln!("For more info, see: https://code.claude.com/docs/en/iam");
    eprintln!();
}

/// Create restart trigger file (called by restart-claude command)
#[allow(dead_code)]
pub fn trigger_restart(wrapper_pid: u32, message: Option<&str>) -> io::Result<()> {
    let trigger_file = cache_dir().join(format!("restart-trigger-{}", wrapper_pid));
    let message_file = cache_dir().join(format!("restart-message-{}", wrapper_pid));

    fs::create_dir_all(cache_dir())?;
    fs::write(&trigger_file, "")?;

    if let Some(msg) = message {
        fs::write(&message_file, msg)?;
    }

    Ok(())
}

/// Exit without restart (called by exit-claude command)
#[allow(dead_code)]
pub fn trigger_exit(wrapper_pid: u32) -> io::Result<()> {
    // Just send SIGTERM to the wrapper process
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    kill(Pid::from_raw(wrapper_pid as i32), Signal::SIGTERM)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}
