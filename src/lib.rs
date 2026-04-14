//! unleash - Unified CLI for AI Code Agents
//!
//! Single binary that handles:
//! - `unleash` - Entrypoint (TUI by default, or runs agent subcommands / wrapper mode)
//!

mod agents;
mod auth;
mod cli;
mod config;
mod hooks;
mod hyprland;
#[cfg(feature = "tui")]
mod input;
mod interchange;
mod json_output;
mod launcher;
pub mod pixel_art;
mod polyfill;
mod sandbox;
pub mod token_count;
mod progress;
#[cfg(feature = "tui")]
mod text_input;
pub mod theme;
#[cfg(feature = "tui")]
mod tui;
mod updater;
mod version;

use crate::agents::AgentType;
use clap::Parser;
use cli::{Cli, Commands};
use config::ProfileManager;
use std::env;
use std::io;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, SystemTime};

const FOCUS_TURN_COMPLETE_CMD: &str = "__unleash-focus-turn-complete";
const FOCUS_ARM_CMD: &str = "__unleash-focus-arm";

fn is_wrapper_command(cmd_name: &str) -> bool {
    matches!(cmd_name, "unleash")
}

fn parse_wrapper_launch_args(
    args: Vec<String>,
    parse_prompt_flags: bool,
) -> (bool, Option<String>, Vec<String>) {
    let mut auto = false;
    let mut prompt = None;
    let mut pass_args = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        if arg == "--auto" || arg == "-a" {
            auto = true;
            i += 1;
            continue;
        }

        if parse_prompt_flags {
            if arg == "-p" || arg == "--prompt" {
                if prompt.is_none() {
                    prompt = args.get(i + 1).filter(|v| !v.starts_with('-')).cloned();
                }
                let has_value = args.get(i + 1).filter(|v| !v.starts_with('-')).is_some();
                i += if has_value { 2 } else { 1 };
                continue;
            }
            if let Some(value) = arg.strip_prefix("--prompt=") {
                if prompt.is_none() {
                    prompt = Some(value.to_string());
                }
                i += 1;
                continue;
            }
        }

        pass_args.push(arg.clone());
        i += 1;
    }

    (auto, prompt, pass_args)
}

fn detect_agent_type_from_cmd_path(cmd: &str) -> Option<AgentType> {
    let cmd_name = Path::new(cmd).file_name().and_then(|n| n.to_str())?;
    AgentType::from_str(cmd_name)
}

fn codex_history_path() -> Option<std::path::PathBuf> {
    if let Some(codex_home) = env::var_os("CODEX_HOME") {
        return Some(std::path::PathBuf::from(codex_home).join("history.jsonl"));
    }
    dirs::home_dir().map(|home| home.join(".codex/history.jsonl"))
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn focus_arm_wait_for_next_turn(wrapper_pid: u32) {
    let history = match codex_history_path() {
        Some(path) => path,
        None => return,
    };
    let baseline = file_mtime(&history).unwrap_or(SystemTime::UNIX_EPOCH);
    let started = std::time::Instant::now();

    // Best effort: wait for the next history append as a proxy for "new prompt sent".
    while started.elapsed() < Duration::from_secs(2 * 60 * 60) {
        if !std::path::Path::new(&format!("/proc/{}", wrapper_pid)).exists() {
            return;
        }

        if let Some(mtime) = file_mtime(&history) {
            if mtime > baseline {
                let _ = hyprland::focus_set(wrapper_pid);
                return;
            }
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn ensure_profile_cli_available(profile_name: &str, cli_path: &str) -> io::Result<()> {
    let cmd_name = Path::new(cli_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if is_wrapper_command(cmd_name) {
        return Ok(());
    }

    let looks_like_path = cli_path.contains(std::path::MAIN_SEPARATOR);
    let available = if looks_like_path {
        Path::new(cli_path).exists()
    } else {
        which::which(cli_path).is_ok()
    };

    if available {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "Profile '{}' uses '{}', but that CLI is not installed or not in PATH.\nInstall it first, or edit the profile in `unleash` (TUI).",
            profile_name, cli_path
        ),
    ))
}

fn print_profile_help(profile_name: &str) {
    println!("Run the '{}' profile with unified flags\n", profile_name);
    println!("Usage: unleash {} [FLAGS] [-- PASSTHROUGH]\n", profile_name);
    println!("Unified flags (translated to agent-specific syntax):");
    println!(
        "      --safe                       Restore approval prompts (permissions bypassed by default)"
    );
    println!(
        "  -p, --prompt <PROMPT>            Run non-interactively with the given prompt"
    );
    println!(
        "  -m, --model <MODEL>              Model to use for the session"
    );
    println!(
        "  -c, --continue                   Continue the most recent session"
    );
    println!(
        "  -r, --resume [ID]                Resume a session by ID, or open picker"
    );
    println!(
        "      --fork                       Fork the session (use with --continue or --resume)"
    );
    println!(
        "  -a, --auto                       Enable auto-mode (autonomous operation)"
    );
    println!(
        "  -e, --effort <LEVEL>             Reasoning effort level (e.g., high, low)"
    );
    println!(
        "  -v, --verbose                    Enable verbose/debug output"
    );
    println!(
        "      --output-format <FORMAT>     Output format (json, text, stream-json)"
    );
    println!(
        "      --system-prompt <TEXT>        System prompt text to inject"
    );
    println!(
        "      --allowed-tools <TOOLS>      Allowed tools filter (comma-separated)"
    );
    println!(
        "      --sandbox                    Enable sandbox mode"
    );
    println!(
        "      --name <NAME>                Session name"
    );
    println!(
        "      --add-dir <DIR>              Additional directory to include"
    );
    println!(
        "      --approval-mode <MODE>       Approval/permission mode"
    );
    println!(
        "      --worktree [NAME]            Git worktree mode (optional name)"
    );
    println!(
        "      --dry-run                    Show the resolved command without executing it"
    );
    println!(
        "  -h, --help                       Print this help message"
    );
    println!();
    println!("Passthrough (after --):");
    println!("  Any arguments after '--' are passed directly to the agent CLI unchanged.");
    println!("  Use this for agent-specific flags that unleash doesn't polyfill.");
    println!();
    println!("Examples:");
    println!(
        "  unleash {} -m opus -c                Continue with model override",
        profile_name
    );
    println!(
        "  unleash {} -p \"fix the tests\"         Run headless",
        profile_name
    );
    println!(
        "  unleash {} --verbose --sandbox        Verbose + sandbox mode",
        profile_name
    );
    println!(
        "  unleash {} --safe -- --extra-flag     Safe mode + agent-specific flag",
        profile_name
    );
}

fn run_agent_with_polyfill(
    profile_name: &str,
    polyfill_args: cli::PolyfillArgs,
    mut extra_args: Vec<String>,
) -> io::Result<()> {
    let manager = ProfileManager::new()?;
    let profile = manager.load_profile(profile_name).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            let available = manager.list_profiles().unwrap_or_default();
            let hint = if available.is_empty() {
                String::new()
            } else {
                format!("\nAvailable profiles: {}", available.join(", "))
            };
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Profile '{}' not found.{}", profile_name, hint),
            )
        } else {
            e
        }
    })?;

    ensure_profile_cli_available(&profile.name, &profile.agent_cli_path)?;

    let mut app_config = manager.load_app_config().unwrap_or_default();
    if app_config.current_profile != profile.name {
        app_config.current_profile = profile.name.clone();
        manager.save_app_config(&app_config)?;
    }

    // Determine the agent type for polyfill resolution
    let agent_type = profile.agent_type().unwrap_or(AgentType::Claude);
    let agent_def = match &agent_type {
        AgentType::Custom(ref name) => {
            let app_config = manager.load_app_config().unwrap_or_default();
            app_config
                .custom_agents
                .iter()
                .find(|a| a.name == *name)
                .map(agents::AgentDefinition::from_custom_config)
                .unwrap_or_else(|| agents::AgentDefinition::from_type(AgentType::Claude))
        }
        _ => agents::AgentDefinition::from_type(agent_type.clone()),
    };

    // Resolve polyfill flags into agent-specific args (CLI overrides profile defaults)
    let flags = polyfill_args.to_polyfill_flags(&profile.defaults);
    let resolved = polyfill::resolve(&agent_def.polyfill, &flags, &profile.agent_cli_args);

    // --dry-run: print the resolved command and exit without executing
    if polyfill_args.dry_run {
        let binary = &profile.agent_cli_path;
        let mut full_args = resolved.subcommand_prefix.clone();
        full_args.extend(resolved.args.clone());
        full_args.extend(profile.agent_cli_args.clone());
        full_args.extend(extra_args.clone());
        println!("Would execute: {} {}", binary, full_args.join(" "));
        if !resolved.env.is_empty() {
            for (k, v) in &resolved.env {
                println!("  env: {}={}", k, v);
            }
        }
        return Ok(());
    }

    // --crossload: inject a foreign session before launching
    let target_cli_owned;
    let target_cli = match &agent_type {
        AgentType::Claude => "claude",
        AgentType::Codex => "codex",
        AgentType::Gemini => "gemini",
        AgentType::OpenCode => "opencode",
        AgentType::Custom(name) => {
            target_cli_owned = name.clone();
            &target_cli_owned
        }
    };

    let mut ucf_active = None;
    if let Some(ref ucf_name) = polyfill_args.ucf {
        if ucf_name.is_empty() {
            eprintln!("\x1b[31m✗\x1b[0m --ucf requires a session name (e.g. --ucf my-project)");
            return Ok(());
        }
        let (session_id, resume_args) = setup_ucf_session(ucf_name, target_cli)?;
        extra_args.extend(resume_args);
        ucf_active = Some((ucf_name.clone(), target_cli.to_string(), session_id));
    } else if let Some(ref crossload_query) = polyfill_args.crossload {

        let query = if crossload_query.is_empty() {
            // Interactive picker
            #[cfg(feature = "tui")]
            {
                match tui::session_picker::pick_session()? {
                    Some(session) => format!("{}:{}", session.cli, session.id),
                    None => {
                        eprintln!("No session selected.");
                        return Ok(());
                    }
                }
            }
            #[cfg(not(feature = "tui"))]
            {
                eprintln!("Interactive session picker requires TUI feature. Specify a session: --crossload cli:name");
                return Ok(());
            }
        } else {
            crossload_query.clone()
        };

        eprintln!("\x1b[34minfo:\x1b[0m Loading session: {query} into {target_cli}");
        match interchange::inject::inject_session(&query, target_cli) {
            Ok(result) => {
                eprintln!("\x1b[32m✓\x1b[0m {}", result.message);
                // Add resume args to launch the session
                extra_args.extend(result.resume_args);
            }
            Err(e) => {
                eprintln!("\x1b[31m✗\x1b[0m Crossload failed: {e}");
                return Err(io::Error::other(e.to_string()));
            }
        }
    }

    // Build the launch args: subcommand prefix + polyfill args + profile args + extra args
    let mut launch_args = resolved.subcommand_prefix;
    launch_args.extend(resolved.args);
    launch_args.extend(profile.agent_cli_args.clone());
    launch_args.extend(extra_args);

    // Auto mode from polyfill flag, profile default, or legacy args
    let auto = polyfill_args.auto
        || profile.defaults.auto
        || launch_args.iter().any(|a| a == "--auto" || a == "-a");
    if polyfill_args.auto && !profile.defaults.auto {
        // CLI explicitly enabled auto — no need to log, it's additive
    } else if profile.defaults.auto && !polyfill_args.auto {
        eprintln!("\x1b[34minfo:\x1b[0m auto-mode enabled by profile default");
    }
    let launch_args: Vec<String> = launch_args
        .into_iter()
        .filter(|a| a != "--auto" && a != "-a")
        .collect();

    // Set AGENT_CMD so the launcher knows which binary to use
    env::set_var("AGENT_CMD", &profile.agent_cli_path);

    // Signal to launcher that polyfill handled yolo/permissions
    env::set_var("UNLEASH_POLYFILL_ACTIVE", "1");

    // Set polyfill-resolved env vars
    for (key, value) in &resolved.env {
        env::set_var(key, value);
    }

    // Headless prompt is handled by the polyfill (adds -p/exec/run to args)
    // Don't pass prompt separately to launcher since it would double-add for Claude
    let res = launcher::run(auto, None, launch_args);

    if let Some((ucf_name, target_cli, session_id)) = ucf_active {
        sync_ucf_session(&ucf_name, &target_cli, &session_id);
    }

    res
}

/// Returns true if `first_arg` is a known unleash subcommand that must be
/// routed through clap even when running inside the wrapper (AGENT_UNLEASH=1).
///
/// This list must stay in sync with the `Commands` enum in `cli.rs`.
/// Any subcommand missing here will be silently swallowed by the wrapper's
/// reentry path and forwarded to the underlying agent CLI instead.
pub(crate) fn is_known_subcommand(first_arg: &str) -> bool {
    matches!(
        first_arg,
        "version"
            | "auth"
            | "auth-check"
            | "hooks"
            | "agents"
            | "update"
            | "install"
            | "uninstall"
            | "sessions"
            | "convert"
            | "sandbox"
            | "token-count"
            | "help"
    )
}

pub fn run() -> io::Result<()> {
    // Check for --version or -V flag before clap processing
    // This allows us to show both Claude Unleashed and Claude Code versions
    let args: Vec<String> = env::args().collect();

    if args.get(1).map(String::as_str) == Some(FOCUS_TURN_COMPLETE_CMD) {
        let wrapper_pid = args
            .get(2)
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or_else(std::process::id);
        let _ = hyprland::focus_reset(wrapper_pid);
        hyprland::play_idle_sound();

        if let Ok(exe) = env::current_exe() {
            let _ = Command::new(exe)
                .arg(FOCUS_ARM_CMD)
                .arg(wrapper_pid.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some(FOCUS_ARM_CMD) {
        if let Some(wrapper_pid) = args.get(2).and_then(|s| s.parse::<u32>().ok()) {
            focus_arm_wait_for_next_turn(wrapper_pid);
        }
        return Ok(());
    }

    let has_json_flag = args.iter().any(|arg| arg == "--json");

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-V") {
        if has_json_flag {
            version::show_current_json();
        } else {
            println!("{}", cli::get_full_version());
        }
        return Ok(());
    }

    // If AGENT_CMD is set AND we're running under the wrapper (AGENT_UNLEASH=1),
    // enter wrapper mode directly. This prevents stale AGENT_CMD from a previous
    // session from hijacking a fresh `unleash codex` invocation.
    // Skip wrapper mode for help/version flags — let clap handle those
    let has_meta_flag = args
        .iter()
        .skip(1)
        .any(|a| matches!(a.as_str(), "-h" | "--help" | "-V" | "--version"));
    let first_arg_is_subcommand = args
        .get(1)
        .map(|a| is_known_subcommand(a.as_str()))
        .unwrap_or(false);
    let is_wrapper_reentry = env::var("AGENT_CMD").is_ok()
        && env::var(launcher::UNLEASHED_ENV_VAR).ok().as_deref() == Some("1");
    if is_wrapper_reentry && !has_meta_flag && !first_arg_is_subcommand {
        let args: Vec<String> = env::args().skip(1).collect();

        // Check for --crossload/-x or --ucf/-u in wrapper mode — handle before launch
        let has_crossload = args
            .iter()
            .any(|a| a == "-x" || a == "--crossload" || a.starts_with("--crossload="));
        let has_ucf = args
            .iter()
            .any(|a| a == "-u" || a == "--ucf" || a.starts_with("--ucf="));

        if has_crossload || has_ucf {
            // Extract crossload query and strip args
            let mut crossload_query = String::new();
            let mut ucf_name = String::new();
            let mut filtered_args = Vec::new();
            let mut skip_next = false;
            for (i, arg) in args.iter().enumerate() {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                if arg == "-x" || arg == "--crossload" {
                    if let Some(next) = args.get(i + 1) {
                        if !next.starts_with('-') {
                            crossload_query = next.clone();
                            skip_next = true;
                        }
                    }
                } else if arg == "-u" || arg == "--ucf" {
                    if let Some(next) = args.get(i + 1) {
                        if !next.starts_with('-') {
                            ucf_name = next.clone();
                            skip_next = true;
                        }
                    }
                } else if let Some(val) = arg.strip_prefix("--crossload=") {
                    crossload_query = val.to_string();
                } else if let Some(val) = arg.strip_prefix("--ucf=") {
                    ucf_name = val.to_string();
                } else if i == 0 && !arg.starts_with('-') {
                    // First positional arg is the profile name — skip it.
                    continue;
                } else {
                    filtered_args.push(arg.clone());
                }
            }

            // Detect target CLI from the first arg (profile name), not AGENT_CMD
            // When user runs `unleash gemini -x`, the first arg is "gemini"
            let first_arg = args.first().map(String::as_str).unwrap_or("");
            let target_cli = match first_arg {
                "claude" | "claude-code" => "claude",
                "codex" => "codex",
                "gemini" | "gemini-cli" => "gemini",
                "opencode" => "opencode",
                _ => {
                    // Fall back to AGENT_CMD
                    env::var("AGENT_CMD")
                        .ok()
                        .and_then(|cmd| detect_agent_type_from_cmd_path(&cmd))
                        .map(|agent| match agent {
                            AgentType::Claude => "claude",
                            AgentType::Codex => "codex",
                            AgentType::Gemini => "gemini",
                            AgentType::OpenCode => "opencode",
                            AgentType::Custom(_) => "claude", // custom agents fall back to claude for crossload
                        })
                        .unwrap_or("claude")
                }
            };

            let mut ucf_session_id = None;

            if !ucf_name.is_empty() {
                let (session_id, resume_args) = setup_ucf_session(&ucf_name, target_cli)?;
                filtered_args.extend(resume_args);
                ucf_session_id = Some(session_id);
            } else {
                let query = if crossload_query.is_empty() {
                    #[cfg(feature = "tui")]
                    {
                        match tui::session_picker::pick_session() {
                            Ok(Some(session)) => format!("{}:{}", session.cli, session.id),
                            Ok(None) => {
                                eprintln!("No session selected.");
                                return Ok(());
                            }
                            Err(e) => {
                                eprintln!("Picker error: {e}");
                                return Ok(());
                            }
                        }
                    }
                    #[cfg(not(feature = "tui"))]
                    {
                        eprintln!("Interactive picker requires TUI. Use: --crossload cli:name");
                        return Ok(());
                    }
                } else {
                    crossload_query
                };

                eprintln!("\x1b[34minfo:\x1b[0m Loading session: {query} into {target_cli}");
                match interchange::inject::inject_session(&query, target_cli) {
                    Ok(result) => {
                        eprintln!("\x1b[32m✓\x1b[0m {}", result.message);
                        filtered_args.extend(result.resume_args);
                    }
                    Err(e) => {
                        eprintln!("\x1b[31m✗\x1b[0m Crossload failed: {e}");
                        return Err(io::Error::other(e.to_string()));
                    }
                }
            }

            let parse_prompt_flags =
                detect_agent_type_from_cmd_path(&env::var("AGENT_CMD").unwrap_or_default())
                    .map(|a| a == AgentType::Claude)
                    .unwrap_or(true);

            let (auto, prompt, pass_args) =
                parse_wrapper_launch_args(filtered_args, parse_prompt_flags);
            let res = launcher::run(auto, prompt, pass_args);
            if let Some(session_id) = ucf_session_id {
                sync_ucf_session(&ucf_name, target_cli, &session_id);
            }
            return res;
        }

        let parse_prompt_flags = env::var("AGENT_CMD")
            .ok()
            .and_then(|cmd| detect_agent_type_from_cmd_path(&cmd))
            .map(|agent| agent == AgentType::Claude)
            .unwrap_or(true);

        let (auto, prompt, pass_args) = parse_wrapper_launch_args(args, parse_prompt_flags);
        return launcher::run(auto, prompt, pass_args);
    }

    // Parse CLI arguments
    let cli = Cli::parse();

    match cli.command {
        // `unleash <profile> [polyfill flags] [-- passthrough]`
        Some(Commands::Profile(args)) => {
            if args.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Profile name is required",
                ));
            }
            let profile_name = &args[0];

            // Intercept --help / -h since clap can't render help for external_subcommand
            if args[1..].iter().any(|a| a == "--help" || a == "-h") {
                print_profile_help(profile_name);
                return Ok(());
            }

            let (polyfill_args, passthrough) = cli::PolyfillArgs::parse_from_raw(&args[1..]);
            run_agent_with_polyfill(profile_name, polyfill_args, passthrough)
        }
        Some(Commands::Version { list, install }) => {
            if list {
                version::list_versions(cli.json)
            } else if let Some(ver) = install {
                version::install_version(&ver, cli.json)
            } else if cli.json {
                version::show_current_json();
                Ok(())
            } else {
                version::show_current()
            }
        }
        Some(Commands::Auth { verbose, quiet }) => {
            let exit_code = auth::run(verbose, cli.json, quiet)?;
            std::process::exit(if exit_code == std::process::ExitCode::SUCCESS {
                0
            } else {
                1
            });
        }
        Some(Commands::Hooks { action }) => {
            use cli::HooksAction;
            use hooks::{HookEvent, HookManager};

            let manager = match HookManager::new() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            match action {
                Some(HooksAction::Status) | None => {
                    println!("{}", manager.summary());
                    println!();
                    println!("Registered hooks:");
                    match manager.list_hooks() {
                        Ok(hooks) => {
                            if hooks.is_empty() {
                                println!("  (none)");
                            } else {
                                for (event, commands) in &hooks {
                                    println!("  {}:", event);
                                    for cmd in commands {
                                        println!("    - {}", cmd);
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("  Error listing hooks: {}", e),
                    }
                    Ok(())
                }
                Some(HooksAction::Install) => manager.install_default_hooks(),
                Some(HooksAction::Sync) => {
                    let plugin_dirs = launcher::find_plugin_dirs();
                    manager.sync_plugin_hooks(&plugin_dirs)?;
                    println!("Synced hooks from {} plugin(s)", plugin_dirs.len());
                    Ok(())
                }
                Some(HooksAction::List) => {
                    match manager.list_hooks() {
                        Ok(hooks) => {
                            if hooks.is_empty() {
                                println!("No hooks registered");
                            } else {
                                for (event, commands) in &hooks {
                                    println!("{}:", event);
                                    for cmd in commands {
                                        println!("  {}", cmd);
                                    }
                                }
                            }
                        }
                        Err(e) => eprintln!("Error listing hooks: {}", e),
                    }
                    Ok(())
                }
                Some(HooksAction::Add {
                    event,
                    command,
                    matcher,
                }) => {
                    let hook_event = HookEvent::from_str(&event).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Unknown hook event: {}. Valid events: Stop, PreToolUse, PostToolUse, PreCompact, Notification, SessionStart, SubagentStart, SubagentStop, Setup, UserPromptSubmit, SessionEnd", event),
                        )
                    })?;
                    manager.register_hook(hook_event, &command, matcher.as_deref())?;
                    println!("Added hook for {}: {}", event, command);
                    Ok(())
                }
                Some(HooksAction::Remove { event, command }) => {
                    let hook_event = HookEvent::from_str(&event).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Unknown hook event: {}", event),
                        )
                    })?;
                    if manager.unregister_hook(hook_event, &command)? {
                        println!("Removed hook for {}: {}", event, command);
                    } else {
                        println!("Hook not found");
                    }
                    Ok(())
                }
            }
        }
        Some(Commands::Agents { action }) => {
            use agents::{AgentManager, AgentType};
            use cli::AgentsAction;
            let json = cli.json;

            let mut manager = match AgentManager::new() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            match action {
                Some(AgentsAction::Status) | None => {
                    println!("Code Agents Status:\n");
                    let status = manager.status_summary();
                    for (agent_type, installed, latest, update_available) in status {
                        let name = agent_type.display_name();
                        let installed_str = installed.as_deref().unwrap_or("not installed");
                        let update_str = if update_available {
                            format!(" (update available: {})", latest.as_deref().unwrap_or("?"))
                        } else {
                            String::new()
                        };
                        println!("  {}: v{}{}", name, installed_str, update_str);
                    }
                    manager.save_version_cache()?;
                    Ok(())
                }
                Some(AgentsAction::List) => {
                    // Clone agent definitions first to avoid borrow conflicts when
                    // calling get_installed_version (which takes &mut self).
                    let defs: Vec<_> = manager.list_agents().into_iter().cloned().collect();
                    if json {
                        let items: Vec<json_output::AgentInfoOutput> = defs
                            .iter()
                            .map(|def| {
                                let installed =
                                    manager.get_installed_version(def.agent_type.clone()).ok().flatten();
                                json_output::AgentInfoOutput {
                                    agent_type: format!("{:?}", def.agent_type).to_lowercase(),
                                    name: def.name.clone(),
                                    binary: def.binary.clone(),
                                    description: def.description.clone(),
                                    github_repo: def.github_repo.clone(),
                                    npm_package: def.npm_package.clone(),
                                    enabled: def.enabled,
                                    installed_version: installed,
                                }
                            })
                            .collect();
                        json_output::print_json(&items);
                    } else {
                        println!("Available Agents:\n");
                        for agent in &defs {
                            let status = if agent.enabled { "enabled" } else { "disabled" };
                            println!(
                                "  {} ({}) - {} [{}]",
                                agent.name, agent.binary, agent.description, status
                            );
                        }
                    }
                    Ok(())
                }
                Some(AgentsAction::Check { agent }) => {
                    let agents_to_check: Vec<AgentType> = if let Some(name) = agent {
                        vec![AgentType::from_str(&name).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!("Unknown agent: {}", name),
                            )
                        })?]
                    } else {
                        AgentType::builtin().to_vec()
                    };

                    for agent_type in agents_to_check {
                        print!("Checking {}... ", agent_type.display_name());
                        match manager.check_update(agent_type.clone()) {
                            Ok(true) => {
                                let latest = manager.get_latest_version(agent_type.clone()).ok().flatten();
                                println!(
                                    "update available: {}",
                                    latest.as_deref().unwrap_or("unknown")
                                );
                            }
                            Ok(false) => println!("up to date"),
                            Err(e) => println!("error: {}", e),
                        }
                    }
                    manager.save_version_cache()?;
                    Ok(())
                }
                Some(AgentsAction::Update { agent }) => {
                    let agent_type = AgentType::from_str(&agent).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Unknown agent: {}", agent),
                        )
                    })?;

                    println!("Updating {}...", agent_type.display_name());
                    match manager.update_agent(agent_type.clone()) {
                        Ok(msg) => {
                            println!("{}", msg);
                            // Refresh version cache
                            manager.get_installed_version(agent_type)?;
                            manager.save_version_cache()?;
                        }
                        Err(e) => eprintln!("Error: {}", e),
                    }
                    Ok(())
                }
                Some(AgentsAction::Info { agent }) => {
                    let agent_type = AgentType::from_str(&agent).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Unknown agent: {}", agent),
                        )
                    })?;

                    // Clone to release the immutable borrow before calling
                    // get_installed_version which takes &mut self.
                    if let Some(def) = manager.get_agent(agent_type.clone()).cloned() {
                        let installed = manager.get_installed_version(agent_type).ok().flatten();
                        if json {
                            let info = json_output::AgentInfoOutput {
                                agent_type: format!("{:?}", def.agent_type).to_lowercase(),
                                name: def.name.clone(),
                                binary: def.binary.clone(),
                                description: def.description.clone(),
                                github_repo: def.github_repo.clone(),
                                npm_package: def.npm_package.clone(),
                                enabled: def.enabled,
                                installed_version: installed,
                            };
                            json_output::print_json(&info);
                        } else {
                            println!("Agent: {}", def.name);
                            println!("Binary: {}", def.binary);
                            println!("Description: {}", def.description);
                            if let Some(repo) = &def.github_repo {
                                println!("GitHub: https://github.com/{}", repo);
                            }
                            if let Some(npm) = &def.npm_package {
                                println!("NPM: {}", npm);
                            }
                            println!("Enabled: {}", def.enabled);
                            if let Some(v) = installed {
                                println!("Installed: v{}", v);
                            } else {
                                println!("Installed: not found");
                            }
                        }
                    }
                    Ok(())
                }
            }
        }
        Some(Commands::Install { agents, all }) => {
            let agent_types = if all {
                AgentType::builtin().to_vec()
            } else if !agents.is_empty() {
                agents
                    .iter()
                    .map(|name| {
                        AgentType::from_str(name).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!(
                                    "Unknown agent: {}. Valid: claude, codex, gemini, opencode",
                                    name
                                ),
                            )
                        })
                    })
                    .collect::<io::Result<Vec<_>>>()?
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Specify agents to install (e.g. 'unleash install gemini') or use --all",
                ));
            };

            updater::run(updater::UpdateConfig {
                agents: agent_types,
                check_only: false,
                include_self: false,
                json: cli.json,
                update_only: false, // install mode: install even if not present
                install_only: true, // install mode: skip already-installed agents
            })
        }
        Some(Commands::Uninstall { agents, all }) => {
            let agent_types = if all {
                AgentType::builtin().to_vec()
            } else if !agents.is_empty() {
                agents
                    .iter()
                    .map(|name| {
                        AgentType::from_str(name).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!(
                                    "Unknown agent: {}. Valid: claude, codex, gemini, opencode",
                                    name
                                ),
                            )
                        })
                    })
                    .collect::<io::Result<Vec<_>>>()?
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Specify agents to uninstall (e.g. 'unleash uninstall gemini') or use --all",
                ));
            };

            updater::uninstall(agent_types)
        }
        Some(Commands::Update {
            agents,
            clis,
            all,
            check,
        }) => {
            // Determine what to update:
            // - no args, no flags: update unleash itself
            // - -c/--clis: update all installed agent CLIs
            // - -a/--all: update unleash + all installed agent CLIs
            // - positional args: update specific agents
            let agent_types = if all || clis {
                AgentType::builtin().to_vec()
            } else if !agents.is_empty() {
                agents
                    .iter()
                    .map(|name| {
                        AgentType::from_str(name).ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                format!(
                                    "Unknown agent: {}. Valid: claude, codex, gemini, opencode",
                                    name
                                ),
                            )
                        })
                    })
                    .collect::<io::Result<Vec<_>>>()?
            } else {
                // No args: self-update only
                vec![]
            };

            let include_self = all || (!clis && agents.is_empty());

            updater::run(updater::UpdateConfig {
                agents: agent_types,
                check_only: check,
                include_self,
                json: cli.json,
                update_only: true, // update mode: skip agents not already installed
                install_only: false, // update mode: update existing agents
            })
        }
        Some(Commands::Sessions {
            cli: cli_filter,
            find,
        }) => {
            let json = cli.json;
            if let Some(query) = find {
                match interchange::sessions::find_session(&query) {
                    Some(session) => {
                        if json {
                            json_output::print_json(&session);
                        } else {
                            println!(
                                "{:<10} {:<40} {:<20} {}",
                                session.cli,
                                session.id,
                                session.name.unwrap_or_default(),
                                session.directory
                            );
                        }
                    }
                    None => {
                        eprintln!("No session found matching: {query}");
                        std::process::exit(1);
                    }
                }
            } else {
                let sessions = if let Some(ref cli_name) = cli_filter {
                    let format: interchange::CliFormat = cli_name
                        .parse()
                        .map_err(|e: interchange::ConvertError| io::Error::other(e.to_string()))?;
                    interchange::sessions::discover_for(format)
                } else {
                    interchange::sessions::discover_all()
                };

                if json {
                    json_output::print_json(&sessions);
                } else if sessions.is_empty() {
                    println!("No sessions found.");
                } else {
                    println!(
                        "{:<10} {:<20} {:<30} {:<20} DIRECTORY",
                        "CLI", "NAME", "TITLE", "UPDATED"
                    );
                    println!("{}", "-".repeat(100));
                    for s in sessions.iter().take(50) {
                        println!(
                            "{:<10} {:<20} {:<30} {:<20} {}",
                            s.cli,
                            s.name.as_deref().unwrap_or(&s.id[..s.id.len().min(18)]),
                            s.title
                                .as_deref()
                                .unwrap_or("")
                                .chars()
                                .take(28)
                                .collect::<String>(),
                            &s.updated_at[..s.updated_at.len().min(10)],
                            s.directory,
                        );
                    }
                    if sessions.len() > 50 {
                        println!("... and {} more", sessions.len() - 50);
                    }
                }
            }
            Ok(())
        }
        Some(Commands::Convert {
            from,
            to,
            input,
            output,
            verify,
        }) => {
            interchange::convert_command(&from, &to, &input, output.as_deref(), verify)
                .map_err(|e| io::Error::other(e.to_string()))?;
            Ok(())
        }
        Some(Commands::Sandbox { action }) => {
            sandbox::handle_sandbox(&action)
        }
        Some(Commands::TokenCount { file, tokenizer }) => {
            token_count::handle_token_count(&file, tokenizer.as_deref())
        }
        None => {
            #[cfg(feature = "tui")]
            return tui::run();

            #[cfg(not(feature = "tui"))]
            {
                use clap::CommandFactory;
                Cli::command().print_help().ok();
                println!();
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wrapper_launch_args() {
        let args = vec![
            "--auto".to_string(),
            "-p".to_string(),
            "hello".to_string(),
            "--foo".to_string(),
        ];
        let (auto, prompt, pass_args) = parse_wrapper_launch_args(args, true);
        assert!(auto);
        assert_eq!(prompt.as_deref(), Some("hello"));
        assert_eq!(pass_args, vec!["--foo".to_string()]);
    }

    #[test]
    fn test_parse_wrapper_launch_args_non_claude_keeps_profile_flag() {
        let args = vec![
            "-p".to_string(),
            "minimax".to_string(),
            "--yolo".to_string(),
        ];
        let (auto, prompt, pass_args) = parse_wrapper_launch_args(args, false);
        assert!(!auto);
        assert_eq!(prompt, None);
        assert_eq!(
            pass_args,
            vec![
                "-p".to_string(),
                "minimax".to_string(),
                "--yolo".to_string()
            ]
        );
    }

    #[test]
    fn test_parse_wrapper_launch_args_supports_prompt_equals() {
        let args = vec!["--prompt=hello".to_string(), "--foo".to_string()];
        let (auto, prompt, pass_args) = parse_wrapper_launch_args(args, true);
        assert!(!auto);
        assert_eq!(prompt.as_deref(), Some("hello"));
        assert_eq!(pass_args, vec!["--foo".to_string()]);
    }

    #[test]
    fn test_wrapper_command_detection() {
        assert!(is_wrapper_command("unleash"));
        assert!(!is_wrapper_command("unleashed"));
        assert!(!is_wrapper_command("u"));
        assert!(!is_wrapper_command("claude"));
    }

    #[test]
    fn test_parse_wrapper_prompt_does_not_consume_flag_as_value() {
        // Regression: `-p --continue` must not treat `--continue` as the prompt value.
        // `-p` is consumed (it's a value-flag pair with no valid value), `--continue` passes through.
        let args = vec!["-p".to_string(), "--continue".to_string()];
        let (_, prompt, pass_args) = parse_wrapper_launch_args(args, true);
        assert_eq!(prompt, None);
        assert_eq!(pass_args, vec!["--continue".to_string()]);
    }

    #[test]
    fn test_parse_wrapper_prompt_equals_with_flag_value() {
        // `--prompt=fix it --continue` must preserve --continue as a pass arg
        let args = vec!["--prompt=fix it".to_string(), "--continue".to_string()];
        let (_, prompt, pass_args) = parse_wrapper_launch_args(args, true);
        assert_eq!(prompt.as_deref(), Some("fix it"));
        assert_eq!(pass_args, vec!["--continue".to_string()]);
    }

    #[test]
    fn test_missing_profile_cli_error() {
        let err = ensure_profile_cli_available(
            "test-profile",
            "__definitely_missing_unleash_test_binary_xyz__",
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("test-profile"));
    }

    #[test]
    fn test_is_known_subcommand_all_present() {
        // Every entry in the Commands enum must appear here so the wrapper
        // reentry path does not swallow them.
        for cmd in &[
            "version",
            "auth",
            "auth-check",
            "hooks",
            "agents",
            "update",
            "install",
            "uninstall",
            "sessions",
            "convert",
            "sandbox",
            "token-count",
            "help",
        ] {
            assert!(
                is_known_subcommand(cmd),
                "'{cmd}' should be recognised as a known subcommand"
            );
        }
    }

    #[test]
    fn test_is_known_subcommand_rejects_profile_names() {
        // Profile names must NOT be recognised as subcommands so they fall
        // through to `run_agent_with_polyfill`.
        for name in &["claude", "codex", "gemini", "opencode", "my-profile"] {
            assert!(
                !is_known_subcommand(name),
                "'{name}' should NOT be recognised as a known subcommand"
            );
        }
    }

    #[test]
    fn test_is_known_subcommand_rejects_arbitrary_strings() {
        assert!(!is_known_subcommand("invalid-subcommand"));
        assert!(!is_known_subcommand(""));
        assert!(!is_known_subcommand("VERSION")); // case sensitive
    }
}

fn setup_ucf_session(ucf_name: &str, target_cli: &str) -> io::Result<(String, Vec<String>)> {
    let ucf_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(env::var("HOME").unwrap_or_default()).join(".local/share"))
        .join("unleash")
        .join("sessions")
        .join(format!("{}.ucf.jsonl", ucf_name));
        
    let ucf_query = format!("ucf:{}", ucf_name);

    if !ucf_path.exists() {
        eprintln!("\x1b[34minfo:\x1b[0m Starting new native UCF session: {ucf_name}");
        std::fs::create_dir_all(ucf_path.parent().unwrap())?;
        let now_iso = std::process::Command::new("date")
            .arg("-u")
            .arg("+%Y-%m-%dT%H:%M:%SZ")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

        let header = interchange::hub::SessionHeader {
            ucf_version: interchange::hub::UCF_VERSION.to_string(),
            session_id: ucf_name.to_string(),
            created_at: now_iso.clone(),
            updated_at: now_iso.clone(),
            source_cli: "ucf".to_string(),
            source_version: "1.0.0".to_string(),
            project: None,
            model: None,
            title: Some(ucf_name.to_string()),
            slug: Some(ucf_name.to_string()),
            parent_session_id: None,
            extensions: serde_json::json!({}),
        };
        let msg = interchange::hub::HubMessage {
            id: format!("init-{ucf_name}"),
            api_message_id: None,
            parent_id: None,
            timestamp: now_iso.clone(),
            completed_at: Some(now_iso),
            role: "user".to_string(),
            content: vec![interchange::hub::ContentBlock::Text {
                text: "[UCF Session Initialized]".to_string(),
            }],
            metadata: Default::default(),
            extensions: serde_json::json!({}),
        };

        let record_session = interchange::hub::HubRecord::Session(header);
        let record_msg = interchange::hub::HubRecord::Message(msg);
        let data = serde_json::to_string(&record_session).unwrap()
            + "\n"
            + &serde_json::to_string(&record_msg).unwrap()
            + "\n";
        std::fs::write(&ucf_path, data)?;
    } else {
        eprintln!("\x1b[34minfo:\x1b[0m Loading native UCF session: {ucf_name} into {target_cli}");
    }

    match interchange::inject::inject_session(&ucf_query, target_cli) {
        Ok(result) => {
            eprintln!("\x1b[32m✓\x1b[0m {}", result.message);
            Ok((result.session_id, result.resume_args))
        }
        Err(e) => {
            eprintln!("\x1b[31m✗\x1b[0m UCF initialization failed: {e}");
            Err(io::Error::other(e.to_string()))
        }
    }
}

fn sync_ucf_session(ucf_name: &str, target_cli: &str, session_id: &str) {
    let query = format!("{}:{}", target_cli, session_id);
    if let Some(session) = interchange::sessions::find_session(&query) {
        if let Ok(records) = interchange::inject::source_to_hub(&session) {
            let ucf_path = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from(env::var("HOME").unwrap_or_default()).join(".local/share"))
                .join("unleash")
                .join("sessions")
                .join(format!("{}.ucf.jsonl", ucf_name));
            
            let mut out = String::new();
            for r in records {
                out.push_str(&serde_json::to_string(&r).unwrap());
                out.push('\n');
            }
            if let Err(e) = std::fs::write(&ucf_path, out) {
                eprintln!("Warning: Failed to save UCF session back to {}: {}", ucf_path.display(), e);
            } else {
                eprintln!("\x1b[32m✓\x1b[0m Saved native UCF session: {}", ucf_name);
            }
        }
    }
}
