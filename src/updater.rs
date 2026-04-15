//! Parallel update orchestrator for agent CLIs.
//!
//! Checks and updates multiple agents concurrently using one thread per agent,
//! with real-time progress reporting via mpsc channels.

use std::io;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::agents::{AgentDefinition, AgentType};
use crate::progress::{LineState, ProgressRenderer};
use crate::version::version_less_than;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Configuration for a parallel update run.
pub struct UpdateConfig {
    /// Which agents to update.
    pub agents: Vec<AgentType>,
    /// Just check for available updates; do not install.
    pub check_only: bool,
    /// Also update unleash itself.
    pub include_self: bool,
    /// Produce JSON output instead of progress bars.
    pub json: bool,
    /// Only update agents that are already installed (skip uninstalled).
    /// When false (install mode), all listed agents are installed.
    pub update_only: bool,
    /// Only install agents that are NOT already installed (skip installed).
    /// When true, already-installed agents are left untouched.
    #[allow(dead_code)]
    pub install_only: bool,
}

/// Result of a version check for a single agent.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub agent_type: AgentType,
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub update_available: bool,
}

/// Outcome of a completed update for the summary phase.
#[derive(Debug, Clone)]
struct UpdateOutcome {
    agent_type: AgentType,
    from_version: Option<String>,
    to_version: Option<String>,
    duration: Duration,
    error: Option<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the parallel update workflow.
///
/// 1. Check all requested agents for available updates (parallel).
/// 2. Unless `check_only`, update agents that have updates (parallel).
/// 3. Print a summary.
pub fn run(config: UpdateConfig) -> io::Result<()> {
    // Handle unleash self-update
    if config.include_self {
        check_or_update_self(config.check_only)?;
    }

    // If no agents specified, we're done (self-update only)
    if config.agents.is_empty() {
        return Ok(());
    }

    let agents = config.agents.clone();

    // ------------------------------------------------------------------
    // Phase 1 – parallel version checks
    // ------------------------------------------------------------------
    let check_results = phase_check(&agents, config.json)?;

    if config.check_only || config.json {
        if config.json {
            print_check_json(&check_results);
        }
        return Ok(());
    }

    // Collect agents that need updating.
    // In update_only mode, skip agents that aren't installed.
    // In install_only mode, skip agents that ARE already installed.
    let to_update: Vec<&CheckResult> = check_results
        .iter()
        .filter(|r| {
            if !r.update_available {
                return false;
            }
            if config.update_only && r.installed.is_none() {
                return false;
            }
            if config.install_only && r.installed.is_some() {
                return false;
            }
            true
        })
        .collect();

    if to_update.is_empty() {
        println!("\nAll agents are up to date.");
        return Ok(());
    }

    // Warn if npm is missing and agents that need it are queued
    if !crate::version::VersionManager::has_npm() {
        let npm_agents: Vec<String> = to_update
            .iter()
            .filter_map(|r| {
                if let AgentType::Custom(_) = &r.agent_type {
                    return None; // custom agents don't need npm warning
                }
                let def = AgentDefinition::from_type(r.agent_type.clone());
                if def.npm_package.is_some() && r.agent_type != AgentType::Claude {
                    Some(def.name)
                } else {
                    None
                }
            })
            .collect();
        if !npm_agents.is_empty() {
            eprintln!(
                "\n\x1b[33mwarning:\x1b[0m npm not found. Required for: {}",
                npm_agents.join(", ")
            );
            eprintln!(
                "  Install Node.js: https://nodejs.org  (or: curl -fsSL https://fnm.vercel.app/install | bash)\n"
            );
        }
    }

    // ------------------------------------------------------------------
    // Phase 2 – parallel updates
    // ------------------------------------------------------------------
    let outcomes = phase_update(&to_update, &check_results)?;

    // ------------------------------------------------------------------
    // Phase 3 – summary
    // ------------------------------------------------------------------
    print_summary(&check_results, &outcomes);

    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 1: Check
// ---------------------------------------------------------------------------

fn phase_check(agents: &[AgentType], json: bool) -> io::Result<Vec<CheckResult>> {
    if !json {
        println!("Checking agents...");
    }

    let agent_list: Vec<AgentType> = agents.to_vec();
    let (tx, rx) = mpsc::channel::<(usize, LineState)>();

    let names: Vec<String> = agent_list
        .iter()
        .map(|a| a.display_name().to_string())
        .collect();
    let mut renderer = ProgressRenderer::new(names.as_slice());

    // Spawn one thread per agent.
    let mut handles = Vec::new();
    for (idx, agent_type) in agent_list.iter().enumerate() {
        let tx = tx.clone();
        let agent_type = agent_type.clone();
        handles.push(thread::spawn(move || {
            let _ = tx.send((idx, LineState::Checking));
            let result = check_agent(agent_type);
            let state = match &result {
                Ok(r) if r.update_available => LineState::UpdateAvailable {
                    from: r.installed.clone().unwrap_or_default(),
                    to: r.latest.clone().unwrap_or_default(),
                },
                Ok(r) => LineState::UpToDate(
                    r.installed
                        .clone()
                        .unwrap_or_else(|| "not installed".into()),
                ),
                Err(e) => LineState::Error(e.to_string()),
            };
            let _ = tx.send((idx, state));
            result
        }));
    }

    // Drop our copy so the channel closes when all threads finish.
    drop(tx);

    // Drain progress events.
    if !json {
        for (idx, state) in &rx {
            renderer.update(idx, state);
            renderer.render();
        }
    } else {
        // Still drain so threads can finish.
        for _ in &rx {}
    }

    // Collect results.
    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.join() {
            Ok(Ok(r)) => results.push(r),
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {
                return Err(io::Error::other("Thread panicked"));
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Phase 2: Update
// ---------------------------------------------------------------------------

fn phase_update(
    to_update: &[&CheckResult],
    _all_results: &[CheckResult],
) -> io::Result<Vec<UpdateOutcome>> {
    let count = to_update.len();
    println!(
        "\nUpdating {} agent{}...",
        count,
        if count == 1 { "" } else { "s" }
    );

    let agents: Vec<(usize, AgentType, Option<String>)> = to_update
        .iter()
        .enumerate()
        .map(|(i, r)| (i, r.agent_type.clone(), r.installed.clone()))
        .collect();

    let (tx, rx) = mpsc::channel::<(usize, LineState)>();

    let names: Vec<String> = to_update
        .iter()
        .map(|r| r.agent_type.display_name().to_string())
        .collect();
    let mut renderer = ProgressRenderer::new(&names);

    let mut handles: Vec<(AgentType, thread::JoinHandle<UpdateOutcome>)> = Vec::new();
    for (idx, agent_type, from_version) in agents {
        let tx = tx.clone();
        let agent_type_for_handle = agent_type.clone();
        let handle = thread::spawn(move || {
            let start = Instant::now();
            let result = update_agent(agent_type.clone(), &tx, idx);
            let duration = start.elapsed();

            match result {
                Ok(_) => {
                    let to_version = get_installed_version(agent_type.clone());
                    UpdateOutcome {
                        agent_type,
                        from_version,
                        to_version,
                        duration,
                        error: None,
                    }
                }
                Err(e) => UpdateOutcome {
                    agent_type,
                    from_version,
                    to_version: None,
                    duration,
                    error: Some(e.to_string()),
                },
            }
        });
        handles.push((agent_type_for_handle, handle));
    }

    drop(tx);

    for (idx, state) in &rx {
        renderer.update(idx, state);
        renderer.render();
    }

    let mut outcomes = Vec::new();
    for (agent_type, handle) in handles {
        match handle.join() {
            Ok(outcome) => outcomes.push(outcome),
            Err(_) => {
                outcomes.push(UpdateOutcome {
                    agent_type,
                    from_version: None,
                    to_version: None,
                    duration: Duration::ZERO,
                    error: Some("Thread panicked".into()),
                });
            }
        }
    }

    Ok(outcomes)
}

// ---------------------------------------------------------------------------
// Phase 3: Summary
// ---------------------------------------------------------------------------

fn print_summary(check_results: &[CheckResult], outcomes: &[UpdateOutcome]) {
    println!();

    let mut updated = 0u32;
    let mut up_to_date = 0u32;
    let mut errors = 0u32;

    // Print updated agents first.
    for outcome in outcomes {
        if let Some(ref err) = outcome.error {
            println!(
                "  x {}    {} (FAILED: {})",
                outcome.agent_type.display_name(),
                outcome.from_version.as_deref().unwrap_or("?"),
                err,
            );
            errors += 1;
        } else {
            // Check if version actually changed
            let from = outcome.from_version.as_deref().unwrap_or("?");
            let to = outcome.to_version.as_deref().unwrap_or("?");
            if from != "?" && to != "?" && from == to {
                println!(
                    "  x {}    {} (FAILED: version unchanged after update attempt)",
                    outcome.agent_type.display_name(),
                    from,
                );
                errors += 1;
            } else {
                println!(
                    "  + {}    {} -> {} ({:.1}s)",
                    outcome.agent_type.display_name(),
                    from,
                    to,
                    outcome.duration.as_secs_f64(),
                );
                updated += 1;
            }
        }
    }

    // Print agents that were already up-to-date.
    for result in check_results {
        if !result.update_available {
            println!(
                "  . {}    {} (up to date)",
                result.agent_type.display_name(),
                result.installed.as_deref().unwrap_or("not installed"),
            );
            up_to_date += 1;
        }
    }

    println!();
    let mut parts = Vec::new();
    if updated > 0 {
        parts.push(format!("{} updated", updated));
    }
    if up_to_date > 0 {
        parts.push(format!("{} up to date", up_to_date));
    }
    if errors > 0 {
        parts.push(format!("{} failed", errors));
    }
    println!("{}", parts.join(", "));
}

// ---------------------------------------------------------------------------
// Per-agent check logic (runs in worker thread)
// ---------------------------------------------------------------------------

/// Check or update unleash itself.
fn check_or_update_self(check_only: bool) -> io::Result<()> {
    let current = env!("CARGO_PKG_VERSION");

    // Check latest release from GitHub
    eprintln!("Checking unleash...");
    let latest = get_latest_github_version("heiervang-technologies/unleash")?;

    match latest {
        Some(ref ver) if version_less_than(current, ver) => {
            if check_only {
                println!(
                    "  unleash          {} -> {} (update available)",
                    current, ver
                );
            } else {
                println!("  unleash          updating {} -> {}...", current, ver);
                // Self-update: re-run install script
                let output = Command::new("bash")
                    .args(["-c", "gh repo clone heiervang-technologies/unleash /tmp/unleash-update 2>/dev/null && bash /tmp/unleash-update/scripts/install.sh && rm -rf /tmp/unleash-update"])
                    .output()?;
                if output.status.success() {
                    println!("  unleash          {} -> {} (updated)", current, ver);
                } else {
                    eprintln!(
                        "  unleash          update failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
        Some(_) => {
            println!("  unleash          {} (up to date)", current);
        }
        None => {
            println!("  unleash          {} (could not check latest)", current);
        }
    }

    Ok(())
}

/// Check a single agent's installed vs latest version.
fn check_agent(agent_type: AgentType) -> io::Result<CheckResult> {
    let installed = get_installed_version(agent_type.clone());
    let latest = get_latest_version(agent_type.clone())?;

    let update_available = match (&installed, &latest) {
        (Some(i), Some(l)) => version_less_than(i, l),
        (None, Some(_)) => true, // not installed, latest exists
        _ => false,
    };

    Ok(CheckResult {
        agent_type,
        installed,
        latest,
        update_available,
    })
}

/// Get the currently installed version by running `<binary> --version`.
fn get_installed_version(agent_type: AgentType) -> Option<String> {
    if let AgentType::Custom(_) = &agent_type {
        return None; // custom agents don't support version detection yet
    }
    let def = AgentDefinition::from_type(agent_type);
    let output = Command::new(&def.binary).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&output.stdout))
}

/// Get the latest available version from GitHub releases API.
fn get_latest_version(agent_type: AgentType) -> io::Result<Option<String>> {
    if let AgentType::Custom(_) = &agent_type {
        return Ok(None); // custom agents don't support version detection yet
    }
    let def = AgentDefinition::from_type(agent_type);

    // Prefer npm registry for agents that have an npm package.
    if let Some(ref package) = def.npm_package {
        if let Some(version) = get_latest_npm_version(package)? {
            return Ok(Some(version));
        }
        // npm not available or returned nothing — fall through to GitHub
    }

    // Fall back to GitHub releases.
    if let Some(ref repo) = def.github_repo {
        return get_latest_github_version(repo);
    }

    Ok(None)
}

fn get_latest_npm_version(package: &str) -> io::Result<Option<String>> {
    // Try npm first
    if let Ok(output) = Command::new("npm")
        .args(["view", package, "version"])
        .output()
    {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return Ok(Some(version));
            }
        }
    }

    // Fallback: curl the npm registry API (works without npm installed)
    if let Ok(output) = Command::new("curl")
        .args([
            "-fsSL",
            &format!("https://registry.npmjs.org/{}/latest", package),
        ])
        .output()
    {
        if output.status.success() {
            let json_str = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                if let Some(version) = parsed.get("version").and_then(|v| v.as_str()) {
                    return Ok(Some(version.to_string()));
                }
            }
        }
    }

    Ok(None)
}

fn get_latest_github_version(repo: &str) -> io::Result<Option<String>> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-H", "Accept: application/vnd.github.v3+json"]);

    // Add auth for private repos — try GH_TOKEN, GITHUB_TOKEN, or `gh auth token`
    if let Some(token) = github_token() {
        cmd.arg("-H").arg(format!("Authorization: token {}", token));
    }

    let output = cmd.arg(&url).output()?;

    if !output.status.success() {
        return Ok(None);
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    Ok(json
        .get("tag_name")
        .and_then(|t| t.as_str())
        .map(sanitize_version))
}

/// Get a GitHub token for API auth (needed for private repos).
/// Tries GH_TOKEN, GITHUB_TOKEN env vars, then `gh auth token`.
fn github_token() -> Option<String> {
    if let Ok(token) = std::env::var("GH_TOKEN") {
        return Some(token);
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        return Some(token);
    }
    // Try gh CLI
    Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Uninstall one or more agent CLIs.
pub fn uninstall(agents: Vec<AgentType>) -> io::Result<()> {
    for agent_type in &agents {
        let installed = get_installed_version(agent_type.clone());
        if installed.is_none() {
            println!(
                "  . {}    not installed, skipping",
                agent_type.display_name()
            );
            continue;
        }

        print!("  - {}    uninstalling...", agent_type.display_name());

        let result = uninstall_agent(agent_type.clone());
        match result {
            Ok(()) => {
                println!(
                    "\r  - {}    {} (removed)",
                    agent_type.display_name(),
                    installed.unwrap_or_default(),
                );
            }
            Err(e) => {
                println!("\r  x {}    FAILED: {}", agent_type.display_name(), e,);
            }
        }
    }
    Ok(())
}

fn uninstall_agent(agent_type: AgentType) -> io::Result<()> {
    if let AgentType::Custom(_) = &agent_type {
        return Err(io::Error::other(
            "Uninstall is not supported for custom agents",
        ));
    }
    let def = AgentDefinition::from_type(agent_type);

    // Try npm uninstall first for agents with npm packages
    if let Some(ref package) = def.npm_package {
        if let Ok(output) = crate::version::VersionManager::npm_global_command()
            .args(["uninstall", "-g", package])
            .output()
        {
            if output.status.success() {
                return Ok(());
            }
        }
    }

    // For binary-installed agents, remove the binary directly
    let binary_path = which_binary(&def.binary);
    if let Some(path) = binary_path {
        std::fs::remove_file(&path)
            .map_err(|e| io::Error::other(format!("Failed to remove {}: {}", path.display(), e)))?;
        return Ok(());
    }

    Err(io::Error::other(format!(
        "Could not find {} to uninstall",
        def.binary,
    )))
}

fn which_binary(name: &str) -> Option<std::path::PathBuf> {
    Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string()))
}

/// Strip version prefixes like "v", "rust-v" to get a clean semver string.
fn sanitize_version(s: &str) -> String {
    s.trim_start_matches("rust-v")
        .trim_start_matches('v')
        .to_string()
}

// ---------------------------------------------------------------------------
// Per-agent update logic (runs in worker thread)
// ---------------------------------------------------------------------------

/// Run the actual update for an agent, sending progress events via the channel.
/// Returns Ok(version) on success or Err on failure.
fn update_agent(
    agent_type: AgentType,
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
) -> io::Result<String> {
    let result = match agent_type {
        AgentType::Claude => update_claude(tx, index),
        AgentType::Codex => update_codex(tx, index),
        AgentType::Gemini => update_gemini(tx, index),
        AgentType::OpenCode => update_opencode(tx, index),
        AgentType::Custom(_) => Err(io::Error::other(
            "Version management is not yet supported for custom agents",
        )),
    };

    match &result {
        Ok(version) => {
            let _ = tx.send((
                index,
                LineState::Complete {
                    from: String::new(), // filled by caller from CheckResult
                    to: version.clone(),
                    duration: Duration::ZERO, // filled by caller
                },
            ));
        }
        Err(e) => {
            let _ = tx.send((index, LineState::Error(e.to_string())));
        }
    }

    result
}

/// Update Claude Code — native GCS binary first, npm fallback.
fn update_claude(tx: &mpsc::Sender<(usize, LineState)>, index: usize) -> io::Result<String> {
    // Get the target version — try npm registry first, fall back to GCS version list
    let target = get_latest_npm_version("@anthropic-ai/claude-code")?
        .or_else(|| {
            crate::version::VersionManager::new()
                .get_available_versions()
                .ok()
                .and_then(|v| v.into_iter().next())
        })
        .ok_or_else(|| io::Error::other("Could not determine latest Claude Code version"))?;

    let _ = tx.send((
        index,
        LineState::Building {
            version: target.clone(),
            phase: "installing native binary...".into(),
        },
    ));

    let vm = crate::version::VersionManager::new();
    let result = vm.install_version(&target)?;

    if result.success {
        let version = get_installed_version(AgentType::Claude).unwrap_or(target);
        Ok(version)
    } else {
        let err_msg = result
            .error
            .or_else(|| {
                if !result.stderr.is_empty() {
                    Some(result.stderr.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "install failed (no details)".into());
        Err(io::Error::other(err_msg))
    }
}

/// Update Codex — prebuilt binary preferred, source build fallback.
/// Delegates to AgentManager which handles the download/build logic.
fn update_codex(tx: &mpsc::Sender<(usize, LineState)>, index: usize) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: String::new(),
            phase: "downloading prebuilt binary...".into(),
        },
    ));

    let mut manager = crate::agents::AgentManager::new()?;
    manager.update_agent(AgentType::Codex)?;

    let version = get_installed_version(AgentType::Codex).unwrap_or_else(|| "latest".into());
    Ok(version)
}

/// Ensure npm is available, offering to install Node.js if missing.
/// Returns Ok(true) if npm is available, Ok(false) if user declined.
fn ensure_npm() -> io::Result<bool> {
    if crate::version::VersionManager::has_npm() {
        return Ok(true);
    }

    // Check if nvm is already installed but not sourced
    if try_source_nvm() {
        return Ok(true);
    }

    // In TUI/non-interactive mode, can't prompt — return error
    let stdin_is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) != 0 };
    if !stdin_is_tty || std::env::var("UNLEASH_TUI").is_ok() {
        return Err(io::Error::other(
            "npm not found. Install Node.js: https://nodejs.org (or: curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash)"
        ));
    }

    eprintln!("\n\x1b[33mnpm not found.\x1b[0m Node.js is required to install this agent.");
    eprint!("Install Node.js via nvm? [Y/n] ");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_lowercase();
    if !answer.is_empty() && answer != "y" && answer != "yes" {
        return Ok(false);
    }

    eprintln!("Installing nvm and Node.js LTS...");

    // Install nvm
    let nvm_install = Command::new("bash")
        .args([
            "-c",
            "curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash",
        ])
        .status()?;
    if !nvm_install.success() {
        return Err(io::Error::other("Failed to install nvm"));
    }

    // Source nvm and install LTS Node
    let node_install = Command::new("bash")
        .args(["-c", "export NVM_DIR=\"$HOME/.nvm\" && . \"$NVM_DIR/nvm.sh\" && nvm install --lts && nvm use --lts"])
        .status()?;
    if !node_install.success() {
        return Err(io::Error::other("Failed to install Node.js via nvm"));
    }

    // Add nvm node to PATH for this process
    if try_source_nvm() {
        eprintln!("\x1b[32m✓\x1b[0m Node.js installed successfully\n");
        Ok(true)
    } else {
        eprintln!("\x1b[33m!\x1b[0m nvm installed but npm not found in current session.");
        eprintln!("  Restart your shell and try again.\n");
        Ok(false)
    }
}

/// Try to find nvm's npm and add it to PATH. Returns true if npm is now available.
fn try_source_nvm() -> bool {
    if let Ok(output) = Command::new("bash")
        .args([
            "-c",
            "export NVM_DIR=\"$HOME/.nvm\" && . \"$NVM_DIR/nvm.sh\" 2>/dev/null && which npm",
        ])
        .output()
    {
        if output.status.success() {
            let npm_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Some(bin_dir) = std::path::Path::new(&npm_path).parent() {
                let current_path = std::env::var("PATH").unwrap_or_default();
                std::env::set_var("PATH", format!("{}:{}", bin_dir.display(), current_path));
                return crate::version::VersionManager::has_npm();
            }
        }
    }
    false
}

/// Update Gemini CLI via npm.
fn update_gemini(tx: &mpsc::Sender<(usize, LineState)>, index: usize) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: String::new(),
            phase: "installing via npm...".into(),
        },
    ));

    if !ensure_npm()? {
        return Err(io::Error::other("npm required to install Gemini CLI"));
    }

    let output = crate::version::VersionManager::npm_global_command()
        .args(["install", "-g", "@google/gemini-cli@latest"])
        .output()?;

    if output.status.success() {
        let version = get_installed_version(AgentType::Gemini).unwrap_or_else(|| "latest".into());
        Ok(version)
    } else {
        Err(io::Error::other(format!(
            "npm install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Update OpenCode via its built-in upgrade command.
/// Gets the target version from npm first, then passes it explicitly to `opencode upgrade`.
fn update_opencode(tx: &mpsc::Sender<(usize, LineState)>, index: usize) -> io::Result<String> {
    // Get the target version from npm (source of truth)
    let target = get_latest_npm_version("opencode-ai")?.unwrap_or_else(|| "latest".to_string());

    let _ = tx.send((
        index,
        LineState::Building {
            version: String::new(),
            phase: format!("upgrading to {}...", target),
        },
    ));

    // Try native opencode upgrade first, fall through to npm on any failure (including ENOENT)
    let upgrade_ok = Command::new("opencode")
        .args(["upgrade", &target])
        .output()
        .ok()
        .is_some_and(|o| o.status.success());

    if upgrade_ok {
        let version = get_installed_version(AgentType::OpenCode).unwrap_or_else(|| target.clone());
        Ok(version)
    } else {
        // Fallback: npm install (handles both upgrade failure and binary not found)
        if !ensure_npm()? {
            return Err(io::Error::other("npm required to install OpenCode"));
        }

        let npm_output = crate::version::VersionManager::npm_global_command()
            .args(["install", "-g", &format!("opencode-ai@{}", target)])
            .output()?;

        if npm_output.status.success() {
            Ok(target)
        } else {
            Err(io::Error::other(format!(
                "opencode install failed: {}",
                String::from_utf8_lossy(&npm_output.stderr)
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// JSON output
// ---------------------------------------------------------------------------

fn print_check_json(results: &[CheckResult]) {
    #[derive(serde::Serialize)]
    struct JsonAgent {
        agent: String,
        installed: Option<String>,
        latest: Option<String>,
        update_available: bool,
    }

    let items: Vec<JsonAgent> = results
        .iter()
        .map(|r| JsonAgent {
            agent: r.agent_type.display_name().to_string(),
            installed: r.installed.clone(),
            latest: r.latest.clone(),
            update_available: r.update_available,
        })
        .collect();

    if let Ok(json) = serde_json::to_string_pretty(&items) {
        println!("{}", json);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a version string from command output such as "claude 2.1.77" or "v1.2.3".
fn parse_version(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    for part in line.split_whitespace() {
        let cleaned = sanitize_version(part).trim_end_matches(')').to_string();
        if cleaned
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return Some(cleaned);
        }
    }
    None
}

// version_less_than and version_compare are imported from crate::version
