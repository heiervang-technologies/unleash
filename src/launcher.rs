//! Claude launcher with wrapper features
//!
//! Implements the wrapper loop that enables:
//! - Self-restart capability (restart-claude command)
//! - Plugin system integration
//! - Auto-mode via Stop hook + flag file system
//! - Process management

use crate::agents::AgentType;
use crate::config::ProfileManager;
use crate::hooks::HookManager;
use crate::hyprland;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicI32, Ordering};
use which::which;

/// Signal number received while waiting for a child process.
/// 0 = no signal, >0 = signal number (e.g. 2 for SIGINT, 15 for SIGTERM).
static CHILD_SIGNAL: AtomicI32 = AtomicI32::new(0);

extern "C" fn child_signal_handler(sig: libc::c_int) {
    CHILD_SIGNAL.store(sig, Ordering::Relaxed);
}

/// Environment variable set when running under the wrapper
pub const UNLEASHED_ENV_VAR: &str = "AGENT_UNLEASH";

/// Cache directory for restart triggers
fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("unleash/process-restart")
}

/// Detect agent type from the command path
fn detect_agent_type(cmd: &Path) -> Option<AgentType> {
    let name = cmd.file_name()?.to_str()?;
    AgentType::from_str(name)
}

/// Run an agent with wrapper features
pub fn run(auto_mode: bool, prompt: Option<String>, extra_args: Vec<String>) -> io::Result<()> {
    // Sync hooks: install defaults + merge plugin hooks into settings.json
    // Plugin hooks must be in settings.json because Claude Code may not reliably
    // load hooks from --plugin-dir when settings.json already has the event key.
    match HookManager::new() {
        Ok(manager) => {
            // Install default hooks if not already installed
            if let Ok(hooks) = manager.list_hooks() {
                if !hooks.contains_key("PreCompact") {
                    if let Err(e) = manager.install_default_hooks() {
                        eprintln!("Warning: Failed to install default hooks: {}", e);
                    }
                }
            }
            // Always sync plugin hooks into settings.json on launch.
            // Prune first so that plugins toggled off have their hooks removed,
            // then re-register hooks for currently-enabled plugins.
            let plugin_dirs = find_plugin_dirs();
            let all_plugin_dirs = find_all_plugin_dirs();
            if let Err(e) =
                manager.prune_hooks_for_disabled_plugins(&all_plugin_dirs, &plugin_dirs)
            {
                eprintln!("Warning: Failed to prune disabled plugin hooks: {}", e);
            }
            if let Err(e) = manager.sync_plugin_hooks(&plugin_dirs) {
                eprintln!("Warning: Failed to sync plugin hooks: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Warning: Failed to initialize hook manager: {}", e);
        }
    }

    // Find agent command
    let agent_cmd = find_agent_command()?;
    let agent_type = detect_agent_type(&agent_cmd);
    let is_claude = agent_type == Some(AgentType::Claude);

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

    // Hyprland integration: set window rules and notify on start (only if plugin enabled)
    if hyprland::is_focus_enabled() {
        if let Err(e) = hyprland::apply_agent_window_rules() {
            eprintln!("Warning: Failed to apply Hyprland window rules: {}", e);
        }
        let _ = hyprland::notify_info("unleash started");
    }

    let mut restart_count = 0;

    loop {
        // Clear trigger file before starting
        let _ = fs::remove_file(&trigger_file);

        // Build command arguments
        let mut args = extra_args.clone();

        // If this is a restart, add --continue and message
        if restart_count > 0 {
            if !args.iter().any(|a| a == "--continue" || a == "--resume") {
                args.insert(0, "--continue".to_string());
                // Only Claude Code supports --dangerously-skip-permissions
                if is_claude {
                    args.insert(1, "--dangerously-skip-permissions".to_string());
                }
            }

            // Check for custom restart message
            let restart_msg = if message_file.exists() {
                let msg = fs::read_to_string(&message_file)
                    .unwrap_or_else(|_| "RESURRECTED.".to_string());
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

        // Find plugin directories (only used for Claude Code)
        let plugin_args = if is_claude { find_plugins() } else { vec![] };

        // For non-Claude agents, set window transparent before launch
        // (Claude handles this via its own UserPromptSubmit hook)
        if !is_claude && hyprland::is_focus_enabled() {
            let _ = hyprland::focus_set(wrapper_pid);
        }

        // Build and run command
        let status = run_agent(
            &agent_cmd,
            &args,
            &plugin_args,
            &profile_env,
            wrapper_pid,
            auto_mode,
            agent_type,
        )?;

        // If we caught a signal while waiting for the child, exit immediately.
        // The child has been reaped; just clean up and go.
        let sig = CHILD_SIGNAL.load(Ordering::Relaxed);
        if sig != 0 {
            if hyprland::is_focus_enabled() {
                let _ = hyprland::focus_reset(wrapper_pid);
                hyprland::focus_cleanup(wrapper_pid);
                let _ = hyprland::notify_info("unleash stopped");
            }
            std::process::exit(128 + sig);
        }

        // Reset to opaque + play sound after agent exit
        if hyprland::is_focus_enabled() {
            let _ = hyprland::focus_reset(wrapper_pid);
            hyprland::play_idle_sound();
        }

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
            if hyprland::is_focus_enabled() {
                let _ = hyprland::focus_reset(wrapper_pid);
                hyprland::focus_cleanup(wrapper_pid);
                let _ = hyprland::notify_info("unleash stopped");
            }
            return Ok(());
        }

        // Reset focus and clean up on exit (safety net for all agents)
        if hyprland::is_focus_enabled() {
            let _ = hyprland::focus_reset(wrapper_pid);
            hyprland::focus_cleanup(wrapper_pid);

            // Notify on exit
            if exit_code == 0 {
                let _ = hyprland::notify_info("unleash stopped");
            } else {
                let _ =
                    hyprland::notify_warning(&format!("unleash exited with code {}", exit_code));
            }
        }

        // Normal exit
        std::process::exit(exit_code);
    }
}

/// Find the agent command (set via AGENT_CMD env var, or defaults to 'claude')
fn find_agent_command() -> io::Result<PathBuf> {
    // Check AGENT_CMD environment variable
    if let Ok(cmd) = env::var("AGENT_CMD") {
        let cmd_path = PathBuf::from(&cmd);

        // Guard against recursive invocation: if AGENT_CMD resolves to the current
        // executable, a profile has agent_cli_path = "unleash" (the default for
        // Profile::new). Running unleash-as-agent would loop forever. Detect this
        // by comparing canonicalized paths and fail with a helpful error.
        if let Ok(current_exe) = std::env::current_exe() {
            let resolved_cmd = which::which(&cmd).unwrap_or_else(|_| cmd_path.clone());
            let canon_cmd = resolved_cmd.canonicalize().unwrap_or(resolved_cmd);
            let canon_exe = current_exe.canonicalize().unwrap_or(current_exe);
            if canon_cmd == canon_exe {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "AGENT_CMD is set to '{}' which resolves to the unleash binary itself.\n\
                         This would cause infinite recursion.\n\
                         Set agent_cli_path in your profile to the actual agent binary \
                         (e.g. 'claude', 'codex', 'gemini', 'opencode').",
                        cmd
                    ),
                ));
            }
        }

        return Ok(cmd_path);
    }

    // Default to 'claude' in PATH
    which("claude").map_err(|_| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not find agent command. Set AGENT_CMD or install an agent CLI.",
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

/// Find ALL plugin directories regardless of enabled state (for discovery).
/// Searches multiple locations in priority order; first match per plugin name wins.
///
/// Search order:
/// 1. `AGENT_UNLEASH_ROOT` env var → `$ROOT/plugins/bundled/`
/// 2. Relative to executable → `<exe_dir>/../share/unleash/plugins/bundled/`
/// 3. CWD-relative → `plugins/bundled/` (development from repo root)
/// 4. `~/.local/share/unleash/plugins/` (user-installed / non-bundled)
pub fn find_all_plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    // Build candidate roots for bundled plugins (first existing root wins per plugin)
    let mut candidate_roots: Vec<PathBuf> = Vec::new();

    // 1. AGENT_UNLEASH_ROOT env var (explicit override, e.g. for dev/CI)
    if let Ok(root) = std::env::var("AGENT_UNLEASH_ROOT") {
        candidate_roots.push(PathBuf::from(root).join("plugins/bundled"));
    }

    // 2. Relative to executable (works for installed binary at ~/.local/bin/unleash)
    if let Ok(exe) = std::env::current_exe() {
        if let Ok(resolved) = exe.canonicalize() {
            if let Some(exe_dir) = resolved.parent() {
                // Installed layout: exe at ~/.local/bin/, plugins at ~/.local/share/unleash/plugins/bundled/
                candidate_roots.push(exe_dir.join("../share/unleash/plugins/bundled"));
                // Dev build: exe at target/release/ or target/debug/, repo root is ../..
                candidate_roots.push(exe_dir.join("../../plugins/bundled"));
            }
        }
    }

    // 3. CWD-relative (original behavior — works when running from repo root)
    candidate_roots.push(PathBuf::from("plugins/bundled"));

    // Scan candidate roots for plugin directories
    for root in &candidate_roots {
        if !root.exists() {
            continue;
        }
        let root = fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy().to_string();
                        if !seen_names.contains(&name_str) {
                            seen_names.insert(name_str);
                            dirs.push(path);
                        }
                    }
                }
            }
        }
    }

    // 4. ~/.local/share/unleash/plugins/ — user-installed / non-bundled plugins
    if let Some(data_dir) = dirs::data_local_dir() {
        let plugins_dir = data_dir.join("unleash/plugins");
        if plugins_dir.exists() {
            if let Ok(entries) = fs::read_dir(&plugins_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        if let Some(name) = entry.path().file_name() {
                            if seen_names.contains(&name.to_string_lossy().to_string()) {
                                continue;
                            }
                        }
                        dirs.push(entry.path());
                    }
                }
            }
        }
    }

    dirs
}

/// Find enabled plugin directories only (filtered by AppConfig.enabled_plugins).
/// Empty enabled list = all plugins enabled (backwards compat).
pub fn find_plugin_dirs() -> Vec<PathBuf> {
    let config = ProfileManager::new()
        .and_then(|m| m.load_app_config())
        .unwrap_or_default();

    let all_dirs = find_all_plugin_dirs();

    // Empty list = all enabled (backwards compat)
    if config.enabled_plugins.is_empty() {
        return all_dirs;
    }

    all_dirs
        .into_iter()
        .filter(|dir| {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            config.enabled_plugins.contains(&name.to_string())
        })
        .collect()
}

/// Convert plugin directories to CLI args
fn find_plugins() -> Vec<String> {
    let mut args = Vec::new();
    for dir in find_plugin_dirs() {
        args.push("--plugin-dir".to_string());
        args.push(dir.to_string_lossy().to_string());
    }
    args
}

/// Run agent command with full configuration
fn run_agent(
    agent_cmd: &PathBuf,
    args: &[String],
    plugin_args: &[String],
    profile_env: &HashMap<String, String>,
    wrapper_pid: u32,
    auto_mode: bool,
    agent_type: Option<AgentType>,
) -> io::Result<ExitStatus> {
    let mut cmd = Command::new(agent_cmd);
    let is_claude = agent_type == Some(AgentType::Claude);

    // Set environment variables from profile
    for (key, value) in profile_env {
        cmd.env(key, value);
    }

    // Set wrapper environment variables
    cmd.env(UNLEASHED_ENV_VAR, "1");
    cmd.env("AGENT_WRAPPER_PID", wrapper_pid.to_string());

    // Set auto mode if requested
    if auto_mode {
        cmd.env("AGENT_AUTO_MODE", "1");

        // Create auto-mode marker file (activates Stop hook enforcement)
        let auto_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("unleash/auto-mode");
        let _ = fs::create_dir_all(&auto_dir);
        let _ = fs::write(
            auto_dir.join(format!("active-{}", wrapper_pid)),
            "auto-start",
        );

        println!("Auto mode activated on startup");
    }

    // Block telemetry for all agents — only allow inference API calls
    // Claude Code
    if env::var("DISABLE_TELEMETRY").is_err() {
        cmd.env("DISABLE_TELEMETRY", "1");
    }
    if env::var("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC").is_err() {
        cmd.env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    }
    // Gemini CLI (telemetry off by default, but be explicit)
    if env::var("GEMINI_TELEMETRY_ENABLED").is_err() {
        cmd.env("GEMINI_TELEMETRY_ENABLED", "false");
    }
    // OpenCode
    if env::var("OPENCODE_DISABLE_SHARE").is_err() {
        cmd.env("OPENCODE_DISABLE_SHARE", "1");
    }
    if env::var("OPENCODE_DISABLE_AUTOUPDATE").is_err() {
        cmd.env("OPENCODE_DISABLE_AUTOUPDATE", "1");
    }

    // Clean up stale telemetry retry queue (Claude Code persists failed events here)
    if let Some(home) = dirs::home_dir() {
        let telemetry_dir = home.join(".claude/telemetry");
        if telemetry_dir.exists() {
            let _ = fs::remove_dir_all(&telemetry_dir);
            let _ = fs::create_dir_all(&telemetry_dir);
        }
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

    // If supercompact plugin is active, disable Claude's auto-compaction.
    // Our preemptive Layer 1 (UserPromptSubmit hook) handles compaction instead,
    // eliminating the race condition with Claude's API compaction call.
    if plugin_args.iter().any(|a| a.contains("supercompact")) {
        cmd.env("DISABLE_AUTO_COMPACT", "1");
    }

    // Claude Code-specific flags (other agents don't support these)
    if is_claude {
        // Add plugin arguments
        cmd.args(plugin_args);

        // Bypass permissions — skip if polyfill already handled yolo/safe
        let polyfill_handled = env::var("UNLEASH_POLYFILL_ACTIVE").ok().as_deref() == Some("1");
        if !polyfill_handled {
            // Legacy path (run_profile without polyfill) — always add yolo
            if !args.iter().any(|a| a == "--dangerously-skip-permissions") {
                eprintln!("\x1b[33m[unleash] WARNING: Running with --dangerously-skip-permissions automatically enabled.\x1b[0m");
            }
            cmd.arg("--dangerously-skip-permissions");
        }
    }

    // Codex native notify hook: end-of-turn => reset opaque + idle sound.
    if agent_type == Some(AgentType::Codex) && hyprland::is_focus_enabled() {
        if let Ok(exe) = env::current_exe() {
            let exe = exe
                .to_string_lossy()
                .replace('\\', "\\\\")
                .replace('\"', "\\\"");
            let notify_override = format!(
                "notify=[\"{}\",\"__unleash-focus-turn-complete\",\"{}\"]",
                exe, wrapper_pid
            );
            cmd.arg("-c").arg(notify_override);
        }
    }

    // Add user arguments
    cmd.args(args);

    // Reset signal flag before spawning
    CHILD_SIGNAL.store(0, Ordering::Relaxed);

    let mut child = cmd.spawn()?;

    // While the child runs, catch SIGINT/SIGTERM instead of dying immediately.
    // The child shares our process group and receives terminal signals directly;
    // we just need to stay alive until it exits so we can reap it cleanly.
    unsafe {
        let handler = child_signal_handler as *const () as libc::sighandler_t;
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }

    let status = child.wait()?;

    // Restore default signal handlers now that the child has exited
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }

    Ok(status)
}

/// Check if authentication is configured
fn check_authentication() {
    // Check OAuth token
    if env::var("CLAUDE_CODE_OAUTH_TOKEN").is_ok() {
        eprintln!(
            "\x1b[32m✓\x1b[0m Using OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable"
        );
        return;
    }

    // Check credentials file
    if let Some(home) = dirs::home_dir() {
        let creds_file = home.join(".claude/.credentials.json");
        if creds_file.exists() {
            eprintln!("\x1b[32m✓\x1b[0m Using credentials from ~/.claude/.credentials.json");
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

    kill(Pid::from_raw(wrapper_pid as i32), Signal::SIGTERM).map_err(io::Error::other)?;

    Ok(())
}
