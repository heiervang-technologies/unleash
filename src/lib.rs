//! Unleash - Unified CLI for AI Code Agents
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
mod json_output;
mod launcher;
mod pixel_art;
mod polyfill;
#[cfg(feature = "tui")]
mod text_input;
#[cfg(feature = "tui")]
mod theme;
#[cfg(feature = "tui")]
mod tui;
mod version;

use clap::Parser;
use crate::agents::AgentType;
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
                    prompt = args.get(i + 1).cloned();
                }
                i += if i + 1 < args.len() { 2 } else { 1 };
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
    println!("      --safe               Restore approval prompts (permissions bypassed by default)");
    println!("  -p, --prompt <PROMPT>    Run non-interactively with the given prompt");
    println!("  -m, --model <MODEL>      Model to use for the session");
    println!("  -c, --continue           Continue the most recent session");
    println!("  -r, --resume [ID]        Resume a session by ID, or open picker");
    println!("      --fork               Fork the session (use with --continue or --resume)");
    println!("  -a, --auto               Enable auto-mode (autonomous operation)");
    println!("  -h, --help               Print this help message");
    println!();
    println!("Passthrough (after --):");
    println!("  Any arguments after '--' are passed directly to the agent CLI unchanged.");
    println!("  Use this for agent-specific flags that unleash doesn't polyfill.");
    println!();
    println!("Examples:");
    println!("  unleash {} -m opus -c              Continue with model override", profile_name);
    println!("  unleash {} -p \"fix the tests\"       Run headless", profile_name);
    println!("  unleash {} --safe -- --verbose      Safe mode + agent-specific flag", profile_name);
}

fn run_agent_with_polyfill(
    profile_name: &str,
    polyfill_args: cli::PolyfillArgs,
    extra_args: Vec<String>,
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
    let agent_def = agents::AgentDefinition::from_type(agent_type);

    // Resolve polyfill flags into agent-specific args
    let flags = polyfill_args.to_polyfill_flags();
    let resolved = polyfill::resolve(&agent_def.polyfill, &flags, &profile.agent_args);

    // Build the launch args: subcommand prefix + polyfill args + profile args + extra args
    let mut launch_args = resolved.subcommand_prefix;
    launch_args.extend(resolved.args);
    launch_args.extend(profile.agent_args.clone());
    launch_args.extend(extra_args);

    // Auto mode from polyfill flag or from legacy args
    let auto = polyfill_args.auto || launch_args.iter().any(|a| a == "--auto" || a == "-a");
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
    launcher::run(auto, None, launch_args)
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

    // If AGENT_CMD is set, enter wrapper mode directly (used by TUI to launch agents)
    // Skip wrapper mode for help/version flags — let clap handle those
    let first_arg = args.get(1).map(String::as_str);
    let is_meta_flag = matches!(first_arg, Some("-h" | "--help" | "-V" | "--version"));
    if env::var("AGENT_CMD").is_ok() && !is_meta_flag {
        let args: Vec<String> = env::args().skip(1).collect();
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

            let (polyfill_args, passthrough) =
                cli::PolyfillArgs::parse_from_raw(&args[1..]);
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
                            format!("Unknown hook event: {}. Valid events: Stop, PreToolUse, PostToolUse, PreCompact, Notification, SessionStart, SubagentStart, SubagentStop, Setup", event),
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
                    println!("Available Agents:\n");
                    for agent in manager.list_agents() {
                        let status = if agent.enabled { "enabled" } else { "disabled" };
                        println!(
                            "  {} ({}) - {} [{}]",
                            agent.name, agent.binary, agent.description, status
                        );
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
                        vec![AgentType::Claude, AgentType::Codex]
                    };

                    for agent_type in agents_to_check {
                        print!("Checking {}... ", agent_type.display_name());
                        match manager.check_update(agent_type) {
                            Ok(true) => {
                                let latest = manager.get_latest_version(agent_type).ok().flatten();
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
                    match manager.update_agent(agent_type) {
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

                    if let Some(def) = manager.get_agent(agent_type) {
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

                        // Get installed version
                        if let Ok(Some(v)) = manager.get_installed_version(agent_type) {
                            println!("Installed: v{}", v);
                        } else {
                            println!("Installed: not found");
                        }
                    }
                    Ok(())
                }
            }
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
    fn test_missing_profile_cli_error() {
        let err = ensure_profile_cli_available(
            "test-profile",
            "__definitely_missing_unleash_test_binary_xyz__",
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains("test-profile"));
    }
}
