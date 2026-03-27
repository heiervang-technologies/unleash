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
    let to_update: Vec<&CheckResult> = check_results.iter().filter(|r| r.update_available).collect();

    if to_update.is_empty() {
        println!("\nAll agents are up to date.");
        return Ok(());
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

    let names: Vec<String> = agent_list.iter().map(|a| a.display_name().to_string()).collect();
    let mut renderer = ProgressRenderer::new(names.as_slice());

    // Spawn one thread per agent.
    let mut handles = Vec::new();
    for (idx, &agent_type) in agent_list.iter().enumerate() {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            let _ = tx.send((idx, LineState::Checking));
            let result = check_agent(agent_type);
            let state = match &result {
                Ok(r) if r.update_available => LineState::UpdateAvailable {
                    from: r.installed.clone().unwrap_or_default(),
                    to: r.latest.clone().unwrap_or_default(),
                },
                Ok(r) => LineState::UpToDate(
                    r.installed.clone().unwrap_or_else(|| "not installed".into()),
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
        .map(|(i, r)| (i, r.agent_type, r.installed.clone()))
        .collect();

    let (tx, rx) = mpsc::channel::<(usize, LineState)>();

    let names: Vec<String> = to_update
        .iter()
        .map(|r| r.agent_type.display_name().to_string())
        .collect();
    let mut renderer = ProgressRenderer::new(&names);

    let mut handles = Vec::new();
    for (idx, agent_type, from_version) in agents {
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            let start = Instant::now();
            update_agent(agent_type, &tx, idx);
            let duration = start.elapsed();

            // Re-check installed version after update.
            let to_version = get_installed_version(agent_type);

            UpdateOutcome {
                agent_type,
                from_version,
                to_version,
                duration,
                error: None,
            }
        }));
    }

    drop(tx);

    for (idx, state) in &rx {
        renderer.update(idx, state);
        renderer.render();
    }

    let mut outcomes = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok(outcome) => outcomes.push(outcome),
            Err(_) => {
                outcomes.push(UpdateOutcome {
                    agent_type: AgentType::Claude, // placeholder; thread panicked
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
                "  x {}    {} ({})",
                outcome.agent_type.display_name(),
                outcome.from_version.as_deref().unwrap_or("?"),
                err,
            );
            errors += 1;
        } else {
            println!(
                "  + {}    {} -> {} ({:.1}s)",
                outcome.agent_type.display_name(),
                outcome.from_version.as_deref().unwrap_or("?"),
                outcome.to_version.as_deref().unwrap_or("?"),
                outcome.duration.as_secs_f64(),
            );
            updated += 1;
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
        Some(ref ver) if ver != current => {
            if check_only {
                println!("  Unleash          {} -> {} (update available)", current, ver);
            } else {
                println!("  Unleash          updating {} -> {}...", current, ver);
                // Self-update: re-run install script
                let output = Command::new("bash")
                    .args(["-c", "gh repo clone heiervang-technologies/unleash /tmp/unleash-update 2>/dev/null && bash /tmp/unleash-update/scripts/install.sh && rm -rf /tmp/unleash-update"])
                    .output()?;
                if output.status.success() {
                    println!("  Unleash          {} -> {} (updated)", current, ver);
                } else {
                    eprintln!("  Unleash          update failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
        }
        Some(_) => {
            println!("  Unleash          {} (up to date)", current);
        }
        None => {
            println!("  Unleash          {} (could not check latest)", current);
        }
    }

    Ok(())
}

/// Check a single agent's installed vs latest version.
fn check_agent(agent_type: AgentType) -> io::Result<CheckResult> {
    let installed = get_installed_version(agent_type);
    let latest = get_latest_version(agent_type)?;

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
    let def = AgentDefinition::from_type(agent_type);
    let output = Command::new(&def.binary).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_version(&String::from_utf8_lossy(&output.stdout))
}

/// Get the latest available version from GitHub releases API.
fn get_latest_version(agent_type: AgentType) -> io::Result<Option<String>> {
    let def = AgentDefinition::from_type(agent_type);

    // Prefer npm registry for agents that have an npm package.
    if let Some(ref package) = def.npm_package {
        return get_latest_npm_version(package);
    }

    // Fall back to GitHub releases.
    if let Some(ref repo) = def.github_repo {
        return get_latest_github_version(repo);
    }

    Ok(None)
}

fn get_latest_npm_version(package: &str) -> io::Result<Option<String>> {
    let output = Command::new("npm")
        .args(["view", package, "version"])
        .output()?;

    if !output.status.success() {
        return Ok(None);
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        Ok(None)
    } else {
        Ok(Some(version))
    }
}

fn get_latest_github_version(repo: &str) -> io::Result<Option<String>> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let output = Command::new("curl")
        .args(["-s", "-H", "Accept: application/vnd.github.v3+json", &url])
        .output()?;

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
fn update_agent(agent_type: AgentType, tx: &mpsc::Sender<(usize, LineState)>, index: usize) {
    let result = match agent_type {
        AgentType::Claude => update_claude(tx, index),
        AgentType::Codex => update_codex(tx, index),
        AgentType::Gemini => update_gemini(tx, index),
        AgentType::OpenCode => update_opencode(tx, index),
    };

    match result {
        Ok(version) => {
            let _ = tx.send((
                index,
                LineState::Complete {
                    from: String::new(), // filled by caller from CheckResult
                    to: version,
                    duration: Duration::ZERO, // filled by caller
                },
            ));
        }
        Err(e) => {
            let _ = tx.send((index, LineState::Error(e.to_string())));
        }
    }
}

/// Update Claude Code via npm.
fn update_claude(tx: &mpsc::Sender<(usize, LineState)>, index: usize) -> io::Result<String> {
    let _ = tx.send((
        index,
        LineState::Building {
            version: String::new(),
            phase: "installing via npm...".into(),
        },
    ));

    let output = Command::new("npm")
        .args(["install", "-g", "@anthropic-ai/claude-code@latest"])
        .output()?;

    if output.status.success() {
        let version = get_installed_version(AgentType::Claude).unwrap_or_else(|| "latest".into());
        Ok(version)
    } else {
        Err(io::Error::other(format!(
            "npm install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
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
    let result = manager.update_agent(AgentType::Codex)?;

    let version = get_installed_version(AgentType::Codex).unwrap_or_else(|| "latest".into());
    eprintln!("{}", result);
    Ok(version)
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

    let output = Command::new("npm")
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
    let target = get_latest_npm_version("opencode-ai")?
        .unwrap_or_else(|| "latest".to_string());

    let _ = tx.send((
        index,
        LineState::Building {
            version: String::new(),
            phase: format!("upgrading to {}...", target),
        },
    ));

    let output = Command::new("opencode")
        .args(["upgrade", &target])
        .output()?;

    if output.status.success() {
        let version =
            get_installed_version(AgentType::OpenCode).unwrap_or_else(|| target.clone());
        Ok(version)
    } else {
        // Fallback: try npm install if opencode upgrade fails
        eprintln!("opencode upgrade failed, trying npm install...");
        let npm_output = Command::new("npm")
            .args(["install", "-g", &format!("opencode-ai@{}", target)])
            .output()?;

        if npm_output.status.success() {
            Ok(target)
        } else {
            Err(io::Error::other(format!(
                "opencode upgrade failed: {}",
                String::from_utf8_lossy(&output.stderr)
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

/// Return true if version `a` is strictly less than `b` (semver-ish).
fn version_less_than(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    let va = parse(a);
    let vb = parse(b);
    for i in 0..va.len().max(vb.len()) {
        let pa = va.get(i).copied().unwrap_or(0);
        let pb = vb.get(i).copied().unwrap_or(0);
        if pa < pb {
            return true;
        }
        if pa > pb {
            return false;
        }
    }
    false
}
