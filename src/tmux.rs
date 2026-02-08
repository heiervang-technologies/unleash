//! Headless tmux mode for Claude Unleashed
//!
//! Enables programmatic access for automation, scripting, and CI/CD pipelines.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;
use which::which;

/// Default configuration values
const DEFAULT_SESSION_NAME: &str = "agent-unleashed";
const DEFAULT_WAIT_TIMEOUT: u64 = 300;
const DEFAULT_TERM_WIDTH: u32 = 200;
const DEFAULT_TERM_HEIGHT: u32 = 50;
const DEFAULT_STABLE_THRESHOLD: u64 = 3;
const DEFAULT_INIT_WAIT: u64 = 5;

/// Configuration loaded from environment
struct Config {
    session_name: String,
    wait_timeout: u64,
    term_width: u32,
    term_height: u32,
    stable_threshold: u64,
    init_wait: u64,
    cache_dir: PathBuf,
}

impl Config {
    fn from_env() -> Self {
        let session_name = env::var("CUTX_SESSION_NAME").unwrap_or_else(|_| DEFAULT_SESSION_NAME.to_string());
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("agent-unleashed/autx");

        Self {
            session_name: session_name.clone(),
            wait_timeout: env::var("CUTX_WAIT_TIMEOUT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_WAIT_TIMEOUT),
            term_width: env::var("CUTX_TERM_WIDTH")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_TERM_WIDTH),
            term_height: env::var("CUTX_TERM_HEIGHT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_TERM_HEIGHT),
            stable_threshold: env::var("CUTX_STABLE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_STABLE_THRESHOLD),
            init_wait: env::var("CUTX_INIT_WAIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_INIT_WAIT),
            cache_dir,
        }
    }

    fn output_file(&self) -> PathBuf {
        self.cache_dir.join(format!("{}.output", self.session_name))
    }

    fn marker_file(&self) -> PathBuf {
        self.cache_dir.join(format!("{}.marker", self.session_name))
    }

    fn lock_file(&self) -> PathBuf {
        self.cache_dir.join(format!("{}.lock", self.session_name))
    }
}

/// ANSI color codes
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const RED: &str = "\x1b[0;31m";
const NC: &str = "\x1b[0m";

fn log_info(msg: &str) {
    println!("{}[cutx]{} {}", GREEN, NC, msg);
}

fn log_warn(msg: &str) {
    println!("{}[cutx]{} {}", YELLOW, NC, msg);
}

fn log_error(msg: &str) {
    eprintln!("{}[cutx]{} {}", RED, NC, msg);
}

/// Check if tmux is available
fn check_tmux() -> io::Result<()> {
    which("tmux").map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "tmux is required but not installed")
    })?;
    Ok(())
}

/// Check if session exists
fn session_exists(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Find the cu launcher
fn find_launcher() -> io::Result<PathBuf> {
    // Try current exe directory
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cu = dir.join("cu");
            if cu.exists() {
                return Ok(cu);
            }
        }
    }

    // Try PATH
    if let Ok(path) = which("cu") {
        return Ok(path);
    }

    // Fall back to claude
    which("claude").map_err(|_| {
        io::Error::new(io::ErrorKind::NotFound, "Neither cu nor claude command found")
    })
}

/// Run tmux subcommand
pub fn run(args: &[String]) -> io::Result<()> {
    let config = Config::from_env();

    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");

    match cmd {
        "go" => cmd_go(&config, &args[1..]),
        "start" => cmd_start(&config, &args[1..]),
        "send" => cmd_send(&config, &args[1..]),
        "read" => cmd_read(&config),
        "wait" => cmd_wait(&config, args.get(1).and_then(|s| s.parse().ok())),
        "attach" => cmd_attach(&config, args.get(1).map(|s| s.as_str()) == Some("--here")),
        "detach" => {
            log_info("Use Ctrl+B, D to detach from within tmux");
            Ok(())
        }
        "stop" | "kill" => cmd_stop(&config),
        "status" => cmd_status(&config),
        "help" | "--help" | "-h" => {
            cmd_help();
            Ok(())
        }
        "" => {
            cmd_help();
            Ok(())
        }
        _ => {
            // Treat as a message query
            cmd_query(&config, args)
        }
    }
}

/// Start a new Claude session
fn cmd_start(config: &Config, args: &[String]) -> io::Result<()> {
    check_tmux()?;

    if session_exists(&config.session_name) {
        log_warn(&format!("Session '{}' already exists", config.session_name));
        log_info("Use 'cutx attach' to connect or 'cutx stop' to restart");
        return Ok(());
    }

    // Parse flags
    let mut auto_mode = false;
    let mut daemon_mode = false;
    let mut claude_args = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--auto" | "-a" => auto_mode = true,
            "-d" | "--daemon" => daemon_mode = true,
            _ => claude_args.push(arg.clone()),
        }
    }

    // Find launcher
    let launcher = find_launcher()?;

    log_info(&format!("Starting Claude session '{}'...", config.session_name));
    if auto_mode {
        log_info("Auto mode enabled");
    }
    if daemon_mode {
        log_info("Daemon mode: session will close when Claude exits");
    }

    // Ensure cache directory exists
    fs::create_dir_all(&config.cache_dir)?;

    // Clear previous output
    let _ = fs::write(config.output_file(), "");

    // Create tmux session
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &config.session_name,
            "-x",
            &config.term_width.to_string(),
            "-y",
            &config.term_height.to_string(),
        ])
        .status()?;

    if !status.success() {
        return Err(io::Error::other("Failed to create tmux session"));
    }

    // Enable logging
    let _ = Command::new("tmux")
        .args([
            "pipe-pane",
            "-t",
            &config.session_name,
            "-o",
            &format!("cat >> {}", config.output_file().display()),
        ])
        .status();

    // Build command
    let mut cmd_parts = Vec::new();
    if auto_mode {
        cmd_parts.push("CLAUDE_AUTO_MODE=1".to_string());
    }
    cmd_parts.push(launcher.to_string_lossy().to_string());
    cmd_parts.extend(claude_args);
    if daemon_mode {
        cmd_parts.push(format!("; tmux kill-session -t {}", config.session_name));
    }

    let cmd_str = cmd_parts.join(" ");

    // Send command to tmux
    Command::new("tmux")
        .args(["send-keys", "-t", &config.session_name, &cmd_str, "Enter"])
        .status()?;

    log_info("Session started. Use 'cutx attach' to connect interactively");
    log_info("Or use 'cutx send \"message\"' to send commands");

    Ok(())
}

/// Start and attach to a Claude session (session closes when Claude exits)
fn cmd_go(config: &Config, args: &[String]) -> io::Result<()> {
    check_tmux()?;

    // If session already exists, just attach
    if session_exists(&config.session_name) {
        log_info(&format!("Session '{}' already running, attaching...", config.session_name));
        return cmd_attach(config, false);
    }

    // Start with daemon mode (session closes when Claude exits)
    let mut start_args = vec!["--daemon".to_string()];
    start_args.extend(args.iter().cloned());
    cmd_start(config, &start_args)?;

    // Small delay to let session initialize
    thread::sleep(Duration::from_millis(100));

    // Attach to the session
    cmd_attach(config, false)
}

/// Send a message to Claude
fn cmd_send(config: &Config, args: &[String]) -> io::Result<()> {
    check_tmux()?;

    if !session_exists(&config.session_name) {
        log_error("No active session. Run 'cutx start' first");
        return Err(io::Error::new(io::ErrorKind::NotFound, "No active session"));
    }

    let message = args.join(" ");
    if message.is_empty() {
        log_error("No message provided");
        println!("Usage: cutx send \"your message\"");
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "No message"));
    }

    // Record current output length as marker
    let marker = fs::read(config.output_file())
        .map(|b| b.len())
        .unwrap_or(0);
    fs::write(config.marker_file(), marker.to_string())?;

    // Send the message
    Command::new("tmux")
        .args(["send-keys", "-t", &config.session_name, &message, "Enter"])
        .status()?;

    log_info("Message sent");
    Ok(())
}

/// Read output from Claude
fn cmd_read(config: &Config) -> io::Result<()> {
    if !config.output_file().exists() {
        log_error("No output file found. Is a session running?");
        return Err(io::Error::new(io::ErrorKind::NotFound, "No output file"));
    }

    let output = fs::read(config.output_file())?;

    // If marker exists, show only new output
    let start = if config.marker_file().exists() {
        fs::read_to_string(config.marker_file())
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0)
    } else {
        0
    };

    if start < output.len() {
        io::stdout().write_all(&output[start..])?;
    }

    Ok(())
}

/// Wait for Claude to finish responding
fn cmd_wait(config: &Config, timeout_override: Option<u64>) -> io::Result<()> {
    check_tmux()?;

    if !session_exists(&config.session_name) {
        log_error("No active session");
        return Err(io::Error::new(io::ErrorKind::NotFound, "No active session"));
    }

    let timeout = timeout_override.unwrap_or(config.wait_timeout);
    log_info(&format!("Waiting for response (timeout: {}s)...", timeout));

    let mut elapsed = 0u64;
    let mut last_size = 0usize;
    let mut stable_count = 0u64;

    while elapsed < timeout {
        thread::sleep(Duration::from_secs(1));
        elapsed += 1;

        // Check if session still exists
        if !session_exists(&config.session_name) {
            log_error("Session terminated unexpectedly");
            return Err(io::Error::other("Session terminated"));
        }

        let current_size = fs::read(config.output_file())
            .map(|b| b.len())
            .unwrap_or(0);

        if current_size == last_size {
            stable_count += 1;
            if stable_count >= config.stable_threshold {
                log_info("Response complete");
                return Ok(());
            }
        } else {
            stable_count = 0;
            last_size = current_size;
        }
    }

    log_warn("Timeout reached");
    Err(io::Error::new(io::ErrorKind::TimedOut, "Timeout"))
}

/// Attach to the session
fn cmd_attach(config: &Config, join_here: bool) -> io::Result<()> {
    check_tmux()?;

    if !session_exists(&config.session_name) {
        log_error("No active session. Run 'cutx start' first");
        return Err(io::Error::new(io::ErrorKind::NotFound, "No active session"));
    }

    let in_tmux = env::var("TMUX").is_ok();

    if in_tmux {
        if join_here {
            log_info("Joining Claude pane into current window...");
            Command::new("tmux")
                .args(["join-pane", "-s", &format!("{}:0.0", config.session_name), "-h"])
                .status()?;
        } else {
            log_info("Switching to session (prefix + ( or ) to switch back)...");
            Command::new("tmux")
                .args(["switch-client", "-t", &config.session_name])
                .status()?;
        }
    } else {
        log_info("Attaching to session (Ctrl+B, D to detach)...");
        Command::new("tmux")
            .args(["attach-session", "-t", &config.session_name])
            .status()?;
    }

    Ok(())
}

/// Stop the session
fn cmd_stop(config: &Config) -> io::Result<()> {
    check_tmux()?;

    if !session_exists(&config.session_name) {
        log_info("No active session");
        return Ok(());
    }

    log_info(&format!("Stopping session '{}'...", config.session_name));
    Command::new("tmux")
        .args(["kill-session", "-t", &config.session_name])
        .status()?;

    // Cleanup files
    let _ = fs::remove_file(config.output_file());
    let _ = fs::remove_file(config.marker_file());
    let _ = fs::remove_file(config.lock_file());

    log_info("Session stopped");
    Ok(())
}

/// Check session status
fn cmd_status(config: &Config) -> io::Result<()> {
    check_tmux()?;

    if session_exists(&config.session_name) {
        log_info(&format!("Session '{}' is {}running{}", config.session_name, GREEN, NC));

        println!();
        let _ = Command::new("tmux")
            .args([
                "list-sessions",
                "-F",
                "#{session_name}: #{session_windows} windows, created #{session_created_string}",
            ])
            .status();

        // Show recent output
        if config.output_file().exists() {
            if let Ok(output) = fs::read_to_string(config.output_file()) {
                if !output.is_empty() {
                    println!();
                    println!("Recent output (last 10 lines):");
                    println!("─────────────────────────────");
                    for line in output.lines().rev().take(10).collect::<Vec<_>>().into_iter().rev() {
                        println!("{}", line);
                    }
                }
            }
        }
    } else {
        log_info(&format!("Session '{}' is {}not running{}", config.session_name, RED, NC));
    }

    Ok(())
}

/// Send message and wait for response (shorthand)
fn cmd_query(config: &Config, args: &[String]) -> io::Result<()> {
    // Start session if not running
    if !session_exists(&config.session_name) {
        cmd_start(config, &[])?;
        thread::sleep(Duration::from_secs(config.init_wait));
    }

    cmd_send(config, args)?;
    cmd_wait(config, None)?;
    println!();
    println!("─────────────────────────────");
    cmd_read(config)?;

    Ok(())
}

/// Show help
fn cmd_help() {
    println!(
        r#"cutx - Claude Unleashed Wrapper (Headless tmux mode)

USAGE:
    cutx <command> [args]
    cutx "message"           Send message and wait for response
    cutxg                    Shorthand for 'cutx go'

COMMANDS:
    go [args]       Start session and attach (closes when Claude exits)
                    This is the recommended way to use cutx interactively.
                    Shorthand: cutxg

    start [--auto] [-d] [args]
                    Start a new Claude session in tmux
                    --auto: enable auto mode (Claude won't stop)
                    -d/--daemon: kill session when Claude exits
                    Additional args are passed to agent-unleashed

    send "msg"      Send a message to the running Claude session

    read            Read current output from Claude

    wait [secs]     Wait for Claude to finish responding
                    Default timeout: 300 seconds

    attach [--here] Attach to the Claude tmux session
                    --here: join pane into current window
                    Without --here: switch to session

    stop            Stop the Claude session

    status          Check if session is running and show info

    help            Show this help message

ENVIRONMENT:
    CUTX_SESSION_NAME      tmux session name (default: agent-unleashed)
    CUTX_WAIT_TIMEOUT      Default wait timeout in seconds (default: 300)
    CUTX_TERM_WIDTH        Terminal width for tmux session (default: 200)
    CUTX_TERM_HEIGHT       Terminal height for tmux session (default: 50)
    CUTX_STABLE_THRESHOLD  Seconds of stable output to consider complete (default: 3)
    CUTX_INIT_WAIT         Seconds to wait for Claude to initialize (default: 5)

EXAMPLES:
    # Start and attach interactively (recommended)
    cutx go
    # Or use the shorthand:
    cutxg

    # Start a background session and send messages
    cutx start
    cutx send "Hello Claude, how are you?"
    cutx wait
    cutx read

    # Or use the query shorthand
    cutx "Hello Claude, how are you?"

    # Attach to existing session
    cutx attach

    # Start with auto mode
    cutx start --auto

    # Check status
    cutx status
"#
    );
}
