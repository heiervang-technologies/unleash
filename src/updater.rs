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
    let failed = print_summary(&check_results, &outcomes);
    summary_to_result(failed, config.install_only)
}

/// Map the failed-agent count from `print_summary` into the `Result` that
/// `run` returns to its caller. Extracted so the silent-failure-propagation
/// behavior (added in #115 to fix the masked codex regression from #114)
/// is exercisable without spawning real install threads.
fn summary_to_result(failed: u32, install_only: bool) -> io::Result<()> {
    if failed == 0 {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "{} agent{} failed to {}",
        failed,
        if failed == 1 { "" } else { "s" },
        if install_only { "install" } else { "update" },
    )))
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

    let agents: Vec<(usize, AgentType, Option<String>, Option<String>)> = to_update
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                i,
                r.agent_type.clone(),
                r.installed.clone(),
                r.latest.clone(),
            )
        })
        .collect();

    let (tx, rx) = mpsc::channel::<(usize, LineState)>();

    let names: Vec<String> = to_update
        .iter()
        .map(|r| r.agent_type.display_name().to_string())
        .collect();
    let mut renderer = ProgressRenderer::new(&names);

    let mut handles: Vec<(AgentType, thread::JoinHandle<UpdateOutcome>)> = Vec::new();
    for (idx, agent_type, from_version, latest_version) in agents {
        let tx = tx.clone();
        let agent_type_for_handle = agent_type.clone();
        let handle = thread::spawn(move || {
            let start = Instant::now();
            let result = update_agent(agent_type.clone(), &tx, idx, latest_version);
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

/// Print the post-run summary and return the count of failed agents.
/// Caller maps a non-zero count to a non-zero process exit so silent install
/// failures can't cascade (e.g. the Docker image expecting `codex --version`
/// to work after install).
fn print_summary(check_results: &[CheckResult], outcomes: &[UpdateOutcome]) -> u32 {
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

    errors
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
/// Some CLIs (e.g. Pi) print the version to stderr rather than stdout, so
/// we fall back to stderr when stdout has no parsable version.
fn get_installed_version(agent_type: AgentType) -> Option<String> {
    if let AgentType::Custom(_) = &agent_type {
        return None; // custom agents don't support version detection yet
    }
    let def = AgentDefinition::from_type(agent_type.clone());
    let output = Command::new(&def.binary).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Hermes reports two version numbers — the SemVer project version and a
    // CalVer release date. Upstream tags GitHub releases by the CalVer date
    // (e.g. v2026.5.7), so we have to compare against that one or the version
    // check will always think an update is available.
    //   "Hermes Agent v0.13.0 (2026.5.7)"  ->  "2026.5.7"
    if agent_type == AgentType::Hermes {
        if let Some(v) = parse_hermes_calver(&stdout).or_else(|| parse_hermes_calver(&stderr)) {
            return Some(v);
        }
    }

    parse_version(&stdout).or_else(|| parse_version(&stderr))
}

/// Pull the CalVer date out of hermes --version output. The format is
/// "Hermes Agent v<semver> (<calver>)" on the first line.
fn parse_hermes_calver(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    let start = line.rfind('(')?;
    let end = line.rfind(')')?;
    if end <= start + 1 {
        return None;
    }
    let inner = line[start + 1..end].trim();
    if inner.chars().next()?.is_ascii_digit() {
        Some(inner.to_string())
    } else {
        None
    }
}

/// Get the latest available version from GitHub releases API.
fn get_latest_version(agent_type: AgentType) -> io::Result<Option<String>> {
    if let AgentType::Custom(_) = &agent_type {
        return Ok(None); // custom agents don't support version detection yet
    }
    let def = AgentDefinition::from_type(agent_type.clone());

    // Prefer npm registry for agents that have an npm package.
    if let Some(ref package) = def.npm_package {
        if let Some(version) = get_latest_npm_version(package)? {
            return Ok(Some(version));
        }
        // npm not available or returned nothing — fall through to GitHub
    }

    // Try GitHub releases.
    if let Some(ref repo) = def.github_repo {
        if let Some(version) = get_latest_github_version(repo)? {
            return Ok(Some(version));
        }
    }

    // Both live lookups failed (npm down, GitHub rate-limited, no network,
    // etc.) — fall back to the embedded version list compiled into the
    // binary. Without this, the check phase emits "not installed (up to
    // date)" and the install loop silently skips the agent, leaving the
    // user with no install and no error. Embedded versions may be slightly
    // behind upstream but are always installable.
    Ok(latest_embedded_version(&agent_type))
}

/// First (newest) version for `agent_type` from the embedded versions
/// list, or None for agents not represented there (Custom, Unleash).
fn latest_embedded_version(agent_type: &AgentType) -> Option<String> {
    let key = match agent_type {
        AgentType::Claude => "claude",
        AgentType::Codex => "codex",
        AgentType::Antigravity => "antigravity",
        AgentType::Gemini => "gemini",
        AgentType::OpenCode => "opencode",
        AgentType::Pi => "pi",
        AgentType::Hermes => "hermes",
        AgentType::Unleash | AgentType::Custom(_) => return None,
    };
    crate::version::load_embedded_versions()
        .get(key)
        .and_then(|v| v.first())
        .cloned()
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

    // Try with auth first (needed for private repos), then fall back to
    // unauthenticated. A stale `gh auth token` returns 401 "Bad credentials"
    // but curl without -f exits 0, so the auth path can silently shadow a
    // working public request — we have to detect the error in the JSON.
    let token = github_token();
    if let Some(ref t) = token {
        if let Some(version) = fetch_github_release_tag(&url, Some(t))? {
            return Ok(Some(version));
        }
    }
    fetch_github_release_tag(&url, None)
}

/// Make a single GitHub releases-latest API call, optionally authenticated.
/// Returns Some(tag) on success, None if the response was an error
/// (e.g. 401 Bad credentials, 404 not found) or had no `tag_name` field.
fn fetch_github_release_tag(url: &str, token: Option<&str>) -> io::Result<Option<String>> {
    let mut cmd = Command::new("curl");
    cmd.args(["-s", "-H", "Accept: application/vnd.github.v3+json"]);
    if let Some(t) = token {
        cmd.arg("-H").arg(format!("Authorization: token {}", t));
    }

    let output = cmd.arg(url).output()?;
    if !output.status.success() {
        return Ok(None);
    }

    Ok(parse_release_tag(&output.stdout))
}

/// Pull `tag_name` out of a GitHub releases-latest API response body.
/// Returns None for error bodies (e.g. `{"message":"Bad credentials"}`) and
/// for malformed JSON, so the caller can retry without auth.
fn parse_release_tag(body: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    json.get("tag_name")
        .and_then(|t| t.as_str())
        .map(sanitize_version)
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

/// Interactive uninstall flow: prompts the user to uninstall unleash itself,
/// plus optionally any installed agent CLIs. Requires a TTY on stdin.
pub fn uninstall_interactive() -> io::Result<()> {
    let stdin_is_tty = unsafe { libc::isatty(libc::STDIN_FILENO) != 0 };
    if !stdin_is_tty || std::env::var("UNLEASH_TUI").is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Specify agents to uninstall (e.g. 'unleash uninstall gemini') or use --all. \
             Bare 'unleash uninstall' requires an interactive terminal.",
        ));
    }

    println!();
    println!("\x1b[1munleash Uninstaller\x1b[0m");
    println!();

    if !prompt_yes_no("Uninstall unleash?", false)? {
        println!("Cancelled.");
        return Ok(());
    }

    // Probe installed agents
    let mut installed_agents: Vec<(AgentType, String)> = Vec::new();
    for agent_type in AgentType::builtin() {
        if let Some(version) = get_installed_version(agent_type.clone()) {
            installed_agents.push((agent_type.clone(), version));
        }
    }

    let mut agents_to_remove: Vec<AgentType> = Vec::new();
    if !installed_agents.is_empty() {
        println!();
        println!("Installed agent CLIs:");
        for (agent_type, version) in &installed_agents {
            println!("  - {} {}", agent_type.display_name(), version);
        }
        println!();

        if prompt_yes_no("Also uninstall all agent CLIs?", false)? {
            agents_to_remove = installed_agents.iter().map(|(a, _)| a.clone()).collect();
        } else if prompt_yes_no("Select agents individually?", false)? {
            for (agent_type, version) in &installed_agents {
                let q = format!("  Uninstall {} {}?", agent_type.display_name(), version);
                if prompt_yes_no(&q, false)? {
                    agents_to_remove.push(agent_type.clone());
                }
            }
        }
    }

    // Discover what's on disk for unleash itself
    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| std::path::PathBuf::from(".local/bin"));
    let data_dir = dirs::home_dir()
        .map(|h| h.join(".local/share/unleash"))
        .unwrap_or_else(|| std::path::PathBuf::from(".local/share/unleash"));
    let config_dir = dirs::home_dir()
        .map(|h| h.join(".config/unleash"))
        .unwrap_or_else(|| std::path::PathBuf::from(".config/unleash"));
    let native_dir = dirs::home_dir()
        .map(|h| h.join(".local/share/claude/versions"))
        .unwrap_or_else(|| std::path::PathBuf::from(".local/share/claude/versions"));
    let cargo_bin = dirs::home_dir()
        .map(|h| h.join(".cargo/bin/unleash"))
        .unwrap_or_else(|| std::path::PathBuf::from(".cargo/bin/unleash"));

    let bin_names = [
        "unleash",
        "unleash-refresh",
        "unleash-exit",
        "restart-claude",
        "exit-claude",
    ];
    let installed_bins: Vec<std::path::PathBuf> = bin_names
        .iter()
        .map(|n| bin_dir.join(n))
        .filter(|p| p.exists() || p.is_symlink())
        .collect();

    println!();
    println!("The following will be removed:");
    if !installed_bins.is_empty() {
        println!("  Binaries/symlinks in {}:", bin_dir.display());
        for p in &installed_bins {
            println!(
                "    - {}",
                p.file_name().and_then(|n| n.to_str()).unwrap_or("")
            );
        }
    }
    if data_dir.exists() {
        println!("  Data directory: {}", data_dir.display());
    }
    if native_dir.exists() {
        println!("  Native Claude Code binaries: {}", native_dir.display());
    }
    if cargo_bin.exists() {
        println!("  Cargo-installed binary: {}", cargo_bin.display());
    }
    if !agents_to_remove.is_empty() {
        println!("  Agent CLIs:");
        for a in &agents_to_remove {
            println!("    - {}", a.display_name());
        }
    }
    println!();

    if !prompt_yes_no("Proceed?", false)? {
        println!("Cancelled.");
        return Ok(());
    }

    // Uninstall agents first (while unleash is still in place).
    if !agents_to_remove.is_empty() {
        println!();
        println!("Uninstalling agent CLIs...");
        uninstall(agents_to_remove)?;
    }

    // Remove unleash binaries and symlinks
    if !installed_bins.is_empty() {
        println!();
        println!("Removing binaries...");
        for p in &installed_bins {
            match std::fs::remove_file(p) {
                Ok(()) => println!("  removed {}", p.display()),
                Err(e) => eprintln!("  failed to remove {}: {}", p.display(), e),
            }
        }
    }

    if data_dir.exists() {
        match std::fs::remove_dir_all(&data_dir) {
            Ok(()) => println!("  removed {}", data_dir.display()),
            Err(e) => eprintln!("  failed to remove {}: {}", data_dir.display(), e),
        }
    }

    if native_dir.exists() {
        match std::fs::remove_dir_all(&native_dir) {
            Ok(()) => println!("  removed {}", native_dir.display()),
            Err(e) => eprintln!("  failed to remove {}: {}", native_dir.display(), e),
        }
    }

    if cargo_bin.exists() {
        match std::fs::remove_file(&cargo_bin) {
            Ok(()) => println!("  removed {}", cargo_bin.display()),
            Err(e) => eprintln!("  failed to remove {}: {}", cargo_bin.display(), e),
        }
    }

    // Config removal is opt-in (user may want to keep profiles).
    if config_dir.exists() {
        println!();
        let q = format!("Remove configuration directory {}?", config_dir.display());
        if prompt_yes_no(&q, false)? {
            match std::fs::remove_dir_all(&config_dir) {
                Ok(()) => println!("  removed {}", config_dir.display()),
                Err(e) => eprintln!("  failed to remove {}: {}", config_dir.display(), e),
            }
        } else {
            println!("  kept {}", config_dir.display());
        }
    }

    println!();
    println!("unleash has been uninstalled.");
    Ok(())
}

/// Prompt the user with a yes/no question. `default_yes` controls the displayed
/// hint and the behavior when the user just presses Enter.
fn prompt_yes_no(question: &str, default_yes: bool) -> io::Result<bool> {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    use std::io::Write;
    print!("{} {} ", question, hint);
    std::io::stdout().flush().ok();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_lowercase();
    if answer.is_empty() {
        return Ok(default_yes);
    }
    Ok(answer == "y" || answer == "yes")
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
///
/// `latest_version` is the version resolved during the check phase. It's used
/// as the fallback label when post-install `--version` probing comes up empty
/// (e.g. when the freshly installed binary isn't on PATH yet in the current
/// process), so the summary shows `2026.5.7` instead of the literal `latest`.
fn update_agent(
    agent_type: AgentType,
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    latest_version: Option<String>,
) -> io::Result<String> {
    let result = match agent_type {
        AgentType::Unleash => Err(io::Error::other(
            "Use `unleash update` to update unleash itself",
        )),
        AgentType::Claude => update_claude(tx, index),
        AgentType::Codex => update_codex(tx, index, latest_version),
        AgentType::Antigravity => update_antigravity(tx, index, latest_version),
        AgentType::Gemini => update_gemini(tx, index, latest_version),
        AgentType::OpenCode => update_opencode(tx, index),
        AgentType::Pi => update_pi(tx, index, latest_version),
        AgentType::Hermes => update_hermes(tx, index, latest_version),
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
fn update_codex(
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    latest_version: Option<String>,
) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: latest_version.clone().unwrap_or_default(),
            phase: "downloading prebuilt binary...".into(),
        },
    ));

    let mut manager = crate::agents::AgentManager::new()?;
    manager.update_agent(AgentType::Codex)?;

    let version = get_installed_version(AgentType::Codex)
        .or(latest_version)
        .unwrap_or_else(|| "latest".into());
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
fn update_gemini(
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    latest_version: Option<String>,
) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: latest_version.clone().unwrap_or_default(),
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
        let version = get_installed_version(AgentType::Gemini)
            .or(latest_version)
            .unwrap_or_else(|| "latest".into());
        Ok(version)
    } else {
        Err(io::Error::other(format!(
            "npm install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Update Antigravity CLI via an AUR helper.
///
/// Routes through [`crate::version::VersionManager::install_antigravity_version_streaming`]
/// so the install_only/update_only dispatchers share one source of truth. See
/// that function for the full rationale on why this isn't an npm install — the
/// short version is that `@google/antigravity-cli` is not published anywhere
/// and AUR is the only reliable channel.
fn update_antigravity(
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    latest_version: Option<String>,
) -> io::Result<String> {
    let label = latest_version.clone().unwrap_or_else(|| "latest".into());
    let vm = crate::version::VersionManager::new();
    let result = run_install_with_phase_updates(tx, index, label.clone(), |log_tx| {
        vm.install_antigravity_version_streaming("latest", log_tx)
    })?;

    install_result_to_version(result, AgentType::Antigravity, label)
}

/// Run a streaming install, forwarding each log line as a Building phase
/// update on the progress channel. Without this, npm-driven installs
/// (pi, opencode) blocked at "installing via npm..." for the entire 30-90s
/// install with no indication of progress, and the user had no way to know
/// whether the process was alive.
fn run_install_with_phase_updates<F>(
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    version_label: String,
    install: F,
) -> io::Result<crate::version::InstallResult>
where
    F: FnOnce(mpsc::Sender<String>) -> io::Result<crate::version::InstallResult>,
{
    let (log_tx, log_rx) = mpsc::channel::<String>();
    let tx_clone = tx.clone();
    let label = version_label.clone();

    let forwarder = thread::spawn(move || {
        while let Ok(line) = log_rx.recv() {
            // Take the trimmed line, cap length so the progress display
            // doesn't blow out the terminal width. The latest line wins —
            // npm output is verbose and the user just needs to see motion.
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let phase: String = trimmed.chars().take(80).collect();
            let _ = tx_clone.send((
                index,
                LineState::Building {
                    version: label.clone(),
                    phase,
                },
            ));
        }
    });

    let result = install(log_tx)?;
    let _ = forwarder.join();
    Ok(result)
}

/// Convert an InstallResult into the io::Result<String> shape the updater
/// dispatcher expects: Ok(version) on success, Err(reason) on failure.
fn install_result_to_version(
    result: crate::version::InstallResult,
    agent_type: AgentType,
    fallback_version: String,
) -> io::Result<String> {
    if result.success {
        let version = get_installed_version(agent_type).unwrap_or(fallback_version);
        Ok(version)
    } else {
        Err(io::Error::other(result.error.unwrap_or_else(|| {
            if result.stderr.is_empty() {
                "install failed (no details)".into()
            } else {
                result.stderr.clone()
            }
        })))
    }
}

/// Update Pi via npm. Routes through the streaming installer so the user
/// sees live npm progress instead of a frozen "installing via npm..." line.
fn update_pi(
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    latest_version: Option<String>,
) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: latest_version.clone().unwrap_or_default(),
            phase: "installing via npm...".into(),
        },
    ));

    if !ensure_npm()? {
        return Err(io::Error::other("npm required to install Pi"));
    }

    let label = latest_version.clone().unwrap_or_else(|| "latest".into());
    let vm = crate::version::VersionManager::new();
    let result = run_install_with_phase_updates(tx, index, label.clone(), |log_tx| {
        vm.install_pi_version_streaming("latest", log_tx)
    })?;

    install_result_to_version(result, AgentType::Pi, label)
}

/// Update Hermes Agent via the official curl bash installer.
/// Hermes is not distributed via npm; the installer always pulls latest.
fn update_hermes(
    tx: &mpsc::Sender<(usize, LineState)>,
    index: usize,
    latest_version: Option<String>,
) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: latest_version.clone().unwrap_or_default(),
            phase: "running install.sh...".into(),
        },
    ));

    let label = latest_version.clone().unwrap_or_else(|| "latest".into());
    let vm = crate::version::VersionManager::new();
    let result = run_install_with_phase_updates(tx, index, label.clone(), |log_tx| {
        vm.install_hermes_version_streaming("latest", log_tx)
    })?;

    install_result_to_version(result, AgentType::Hermes, label)
}

/// Update OpenCode via its built-in upgrade command, with npm fallback.
/// Both paths stream output via the InstallResult-returning streaming API.
fn update_opencode(tx: &mpsc::Sender<(usize, LineState)>, index: usize) -> io::Result<String> {
    // Get the target version from npm (source of truth)
    let target = get_latest_npm_version("opencode-ai")?.unwrap_or_else(|| "latest".to_string());

    let _ = tx.send((
        index,
        LineState::Building {
            version: target.clone(),
            phase: format!("upgrading to {}...", target),
        },
    ));

    let vm = crate::version::VersionManager::new();
    let target_for_install = target.clone();
    let result = run_install_with_phase_updates(tx, index, target.clone(), |log_tx| {
        vm.install_opencode_version_streaming(&target_for_install, log_tx)
    })?;

    install_result_to_version(result, AgentType::OpenCode, target)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a successful `UpdateOutcome` for a given agent.
    fn ok_outcome(agent_type: AgentType, from: &str, to: &str) -> UpdateOutcome {
        UpdateOutcome {
            agent_type,
            from_version: Some(from.into()),
            to_version: Some(to.into()),
            duration: Duration::from_millis(10),
            error: None,
        }
    }

    /// Build a failed `UpdateOutcome` (the case that #115 must not let slip
    /// through silently — it had been swallowed before, masking #114's
    /// codex regression in Docker CI for over a day).
    fn err_outcome(agent_type: AgentType, from: &str, err: &str) -> UpdateOutcome {
        UpdateOutcome {
            agent_type,
            from_version: Some(from.into()),
            to_version: None,
            duration: Duration::from_millis(5),
            error: Some(err.into()),
        }
    }

    // ── summary_to_result: the Err mapping added in #115 ──────────────

    #[test]
    fn summary_to_result_zero_failures_is_ok() {
        // Positive control: no failures -> Ok(()), regardless of mode.
        assert!(summary_to_result(0, false).is_ok());
        assert!(summary_to_result(0, true).is_ok());
    }

    #[test]
    fn summary_to_result_single_failure_returns_err_with_failed() {
        let err = summary_to_result(1, false).expect_err("should error on failure");
        let msg = err.to_string();
        assert!(
            msg.contains("failed"),
            "error message must contain 'failed' (was: {msg})"
        );
        assert!(
            msg.contains('1'),
            "error must mention the count (was: {msg})"
        );
        // Singular: "1 agent failed", not "1 agents".
        assert!(
            msg.contains("agent failed"),
            "error must use singular form for 1 agent (was: {msg})"
        );
        // Update mode -> "update", not "install".
        assert!(
            msg.contains("update"),
            "error must mention 'update' for update mode (was: {msg})"
        );
    }

    #[test]
    fn summary_to_result_multiple_failures_uses_plural() {
        let err = summary_to_result(3, false).expect_err("should error on failure");
        let msg = err.to_string();
        assert!(msg.contains("failed"), "msg lacks 'failed': {msg}");
        assert!(
            msg.contains("3 agents failed"),
            "expected plural agents: {msg}"
        );
    }

    #[test]
    fn summary_to_result_install_only_uses_install_verb() {
        let err = summary_to_result(2, true).expect_err("should error on failure");
        let msg = err.to_string();
        assert!(msg.contains("failed"), "msg lacks 'failed': {msg}");
        assert!(
            msg.contains("install"),
            "install_only=true must surface 'install' verb (was: {msg})"
        );
        assert!(
            !msg.contains("update"),
            "install_only=true must not say 'update' (was: {msg})"
        );
    }

    // ── print_summary: count derivation from outcomes ─────────────────
    //
    // Drives the same code path that produced the silent-failure regression:
    // an outcome carrying `error: Some(_)` must contribute to the failed
    // count that `run()` propagates to its `Err` return.

    #[test]
    fn print_summary_counts_outcome_error_as_failed() {
        let check_results = Vec::new();
        let outcomes = vec![err_outcome(
            AgentType::Codex,
            "0.50.0",
            "download failed: 404 Not Found",
        )];
        let failed = print_summary(&check_results, &outcomes);
        assert_eq!(
            failed, 1,
            "outcome with error=Some(_) must be counted as a failure"
        );
        // Round-trip through summary_to_result to confirm it propagates.
        let err = summary_to_result(failed, false).expect_err("should propagate as Err");
        assert!(err.to_string().contains("failed"));
    }

    #[test]
    fn print_summary_counts_unchanged_version_as_failed() {
        // Bonus regression guard: even without an explicit error, an outcome
        // whose from_version == to_version is treated as a silent failure
        // (the install completed but the binary didn't actually upgrade).
        let outcomes = vec![UpdateOutcome {
            agent_type: AgentType::Codex,
            from_version: Some("0.50.0".into()),
            to_version: Some("0.50.0".into()),
            duration: Duration::from_millis(5),
            error: None,
        }];
        let failed = print_summary(&[], &outcomes);
        assert_eq!(failed, 1, "version-unchanged update must count as failure");
    }

    #[test]
    fn print_summary_no_errors_returns_zero() {
        // Negative control: all outcomes succeeded -> failed count is 0.
        let outcomes = vec![
            ok_outcome(AgentType::Claude, "2.1.0", "2.1.77"),
            ok_outcome(AgentType::Codex, "0.50.0", "0.51.0"),
        ];
        let failed = print_summary(&[], &outcomes);
        assert_eq!(failed, 0, "all-ok outcomes must yield zero failures");
        assert!(summary_to_result(failed, false).is_ok());
    }

    #[test]
    fn print_summary_mixed_outcomes_counts_only_failures() {
        let outcomes = vec![
            ok_outcome(AgentType::Claude, "2.1.0", "2.1.77"),
            err_outcome(AgentType::Codex, "0.50.0", "exec format error"),
            err_outcome(AgentType::Gemini, "1.0.0", "npm install failed"),
        ];
        let failed = print_summary(&[], &outcomes);
        assert_eq!(
            failed, 2,
            "exactly two errored outcomes must yield failed=2"
        );

        // And the run() error path must be triggered with the expected message.
        let err = summary_to_result(failed, false).expect_err("must propagate Err");
        let msg = err.to_string();
        assert!(
            msg.contains("2 agents failed"),
            "expected '2 agents failed': {msg}"
        );
        assert!(msg.contains("update"), "expected 'update' verb: {msg}");
    }

    // ── parse_release_tag: stale-gh-token bug from wisp ──────────────
    // A stale `gh auth token` returns 401 "Bad credentials" but curl without
    // -f exits 0, so we'd parse the error body. Treating the error body as
    // "no version available" lets get_latest_github_version retry without
    // auth instead of silently shadowing the public request.

    #[test]
    fn parse_release_tag_extracts_tag_from_success_response() {
        let body = br#"{"tag_name":"v2026.5.7","name":"Release"}"#;
        assert_eq!(parse_release_tag(body), Some("2026.5.7".to_string()));
    }

    #[test]
    fn parse_release_tag_returns_none_for_bad_credentials() {
        let body = br#"{"message":"Bad credentials","documentation_url":"https://docs.github.com/rest","status":"401"}"#;
        assert_eq!(
            parse_release_tag(body),
            None,
            "401 response must miss so caller can fall back to unauthenticated"
        );
    }

    #[test]
    fn parse_release_tag_returns_none_for_not_found() {
        let body = br#"{"message":"Not Found","documentation_url":"https://docs.github.com/rest"}"#;
        assert_eq!(parse_release_tag(body), None);
    }

    #[test]
    fn parse_release_tag_returns_none_for_malformed_json() {
        let body = b"not json at all";
        assert_eq!(parse_release_tag(body), None);
    }

    #[test]
    fn parse_release_tag_returns_none_for_missing_tag_name() {
        let body = br#"{"name":"Release","id":12345}"#;
        assert_eq!(parse_release_tag(body), None);
    }

    #[test]
    fn latest_embedded_version_returns_first_for_bundled_agents() {
        // data/versions.json is compiled into the binary via include_str! —
        // these agents must always have at least one fallback entry or the
        // CUDA Dockerfile / any offline install will silently skip them when
        // the live registry lookup fails (npm down, GitHub rate-limited).
        for agent in [
            AgentType::Claude,
            AgentType::Codex,
            AgentType::Gemini,
            AgentType::OpenCode,
            AgentType::Antigravity,
            AgentType::Pi,
            AgentType::Hermes,
        ] {
            let v = latest_embedded_version(&agent);
            assert!(
                v.is_some(),
                "embedded versions must include {:?} (data/versions.json)",
                agent
            );
            assert!(
                !v.as_deref().unwrap_or("").is_empty(),
                "{:?} embedded version must be non-empty",
                agent
            );
        }
    }

    #[test]
    fn latest_embedded_version_returns_none_for_custom_and_unleash() {
        assert_eq!(latest_embedded_version(&AgentType::Unleash), None);
        assert_eq!(
            latest_embedded_version(&AgentType::Custom("foo".into())),
            None
        );
    }
}
