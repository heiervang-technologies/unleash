//! Agent Unleashed - Unified CLI for AI Code Agents
//!
//! Single binary that handles:
//! - `au` / `agent-unleashed` - Launch agent with wrapper features
//! - `au tui` / `aui` - TUI for profile/version management
//! - `au tmux` / `autx` - Headless tmux mode
//!
//! Legacy aliases (backwards compatible):
//! - `cu` / `cui` / `cug` / `cutx` / `cutxg` - same as au* variants

mod agents;
mod auth;
mod cli;
mod config;
mod hooks;
#[cfg(feature = "tui")]
mod input;
mod json_output;
mod launcher;
mod patcher;
mod pixel_art;
#[cfg(feature = "tui")]
mod text_input;
#[cfg(feature = "tui")]
mod theme;
mod tmux;
#[cfg(feature = "tui")]
mod tui;
mod version;

use clap::Parser;
use cli::{Cli, Commands};
use std::env;
use std::io;
use std::path::Path;

fn main() -> io::Result<()> {
    // Check for --version or -V flag before clap processing
    // This allows us to show both Claude Unleashed and Claude Code versions
    let args: Vec<String> = env::args().collect();
    let has_json_flag = args.iter().any(|arg| arg == "--json");

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-V") {
        if has_json_flag {
            version::show_current_json();
        } else {
            println!("{}", cli::get_full_version());
        }
        return Ok(());
    }

    // Check how we were invoked (argv[0])
    let invoked_as = env::args()
        .next()
        .and_then(|arg| {
            Path::new(&arg)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_default();

    // Handle symlink invocations (au* = new, cu* = legacy)
    match invoked_as.as_str() {
        #[cfg(feature = "tui")]
        "aui" | "cui" => return tui::run(),
        #[cfg(not(feature = "tui"))]
        "aui" | "cui" => {
            eprintln!("Error: TUI support is not compiled in this build");
            eprintln!("Rebuild with: cargo build --features tui");
            std::process::exit(1);
        }
        "aug" | "cug" => {
            // Shorthand for `au go` - launch agent wrapper
            let args: Vec<String> = env::args().skip(1).collect();
            // Parse args for --auto and -p flags
            let auto = args.iter().any(|a| a == "--auto" || a == "-a");
            let prompt = args
                .iter()
                .position(|a| a == "-p" || a == "--prompt")
                .and_then(|i| args.get(i + 1).cloned());
            // Filter out the flags we consumed
            let pass_args: Vec<String> = args
                .into_iter()
                .filter(|a| a != "--auto" && a != "-a" && a != "-p" && a != "--prompt")
                .collect();
            return launcher::run(auto, prompt, pass_args);
        }
        "autx" | "cutx" => {
            // Pass remaining args to tmux module
            let args: Vec<String> = env::args().skip(1).collect();
            return tmux::run(&args);
        }
        "autxg" | "cutxg" => {
            // Shorthand for 'autx go' - start and attach to tmux session
            let mut args: Vec<String> = vec!["go".to_string()];
            args.extend(env::args().skip(1));
            return tmux::run(&args);
        }
        _ => {}
    }

    // Parse CLI arguments
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Go { auto, prompt, args }) => {
            // Launch Claude with wrapper features
            launcher::run(auto, prompt, args)
        }
        #[cfg(feature = "tui")]
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
                Some(HooksAction::Install) => {
                    manager.install_default_hooks()
                }
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
                Some(HooksAction::Add { event, command, matcher }) => {
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
                        println!("  {} ({}) - {} [{}]", agent.name, agent.binary, agent.description, status);
                    }
                    Ok(())
                }
                Some(AgentsAction::Check { agent }) => {
                    let agents_to_check: Vec<AgentType> = if let Some(name) = agent {
                        vec![AgentType::from_str(&name).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidInput, format!("Unknown agent: {}", name))
                        })?]
                    } else {
                        vec![AgentType::Claude, AgentType::Codex]
                    };

                    for agent_type in agents_to_check {
                        print!("Checking {}... ", agent_type.display_name());
                        match manager.check_update(agent_type) {
                            Ok(true) => {
                                let latest = manager.get_latest_version(agent_type).ok().flatten();
                                println!("update available: {}", latest.as_deref().unwrap_or("unknown"));
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
                        io::Error::new(io::ErrorKind::InvalidInput, format!("Unknown agent: {}", agent))
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
                        io::Error::new(io::ErrorKind::InvalidInput, format!("Unknown agent: {}", agent))
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
            // No subcommand: show help
            use clap::CommandFactory;
            Cli::command().print_help().ok();
            println!(); // Add newline after help
            Ok(())
        }
    }
}
