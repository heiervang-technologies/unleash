//! Sandbox subcommand: one-command secure container setup and execution.
//!
//! Wraps Docker + gVisor + LAN isolation into a seamless experience:
//!   - `unleash sandbox setup`     — install gVisor, create network, set iptables
//!   - `unleash sandbox <agent>`   — run an agent in the sandbox
//!   - `unleash sandbox status`    — health check
//!   - `unleash sandbox teardown`  — clean up
//!   - `unleash sandbox allow-ip`  — open a LAN IP for local API access
//!   - `unleash sandbox revoke-ip` — close it

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Find the docker/ directory relative to the unleash binary or repo root.
fn find_docker_dir() -> Option<PathBuf> {
    // Try relative to current exe
    if let Ok(exe) = std::env::current_exe() {
        // Installed: /usr/local/bin/unleash -> look for /usr/local/share/unleash/docker/
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            let share_path = prefix.join("share").join("unleash").join("docker");
            if share_path.join("Dockerfile").exists() {
                return Some(share_path);
            }
        }
    }

    // Try repo layout: cwd or parent has docker/
    let cwd = std::env::current_dir().ok()?;
    for dir in [&cwd, &cwd.join(".."), &cwd.join("../..")]
        .iter()
        .filter_map(|p| p.canonicalize().ok())
    {
        let docker_dir = dir.join("docker");
        if docker_dir.join("Dockerfile").exists() {
            return Some(docker_dir);
        }
    }

    None
}

fn sandbox_network_script(docker_dir: &Path) -> PathBuf {
    docker_dir.join("sandbox-network.sh")
}

fn check_command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_command(cmd: &str, args: &[&str]) -> io::Result<bool> {
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(status.success())
}

fn run_command_output(cmd: &str, args: &[&str]) -> io::Result<String> {
    let output = Command::new(cmd).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn is_root() -> bool {
    // Check UID via /proc or id command
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|uid| uid.parse::<u32>().ok())
                .map(|uid| uid == 0)
        })
        .unwrap_or(false)
}

fn needs_sudo(action: &str) -> io::Result<()> {
    if !is_root() {
        eprintln!(
            "\x1b[33mwarning:\x1b[0m '{}' requires root privileges. Re-run with sudo:",
            action
        );
        eprintln!("  sudo unleash sandbox {}", action);
        return Err(io::Error::other("requires root"));
    }
    Ok(())
}

/// Check if gVisor (runsc) is installed
fn gvisor_installed() -> bool {
    check_command_exists("runsc")
}

/// Check if Docker is running
fn docker_running() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if the sandbox network exists
fn network_exists() -> bool {
    Command::new("docker")
        .args(["network", "inspect", "unleash-sandbox"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if iptables LAN-blocking rules are in place
fn iptables_rules_active() -> bool {
    let output = Command::new("iptables")
        .args(["-L", "DOCKER-USER", "-n"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains("172.30.0.0/16")
        }
        Err(_) => false,
    }
}

/// Check if the unleash Docker image exists
fn image_exists() -> bool {
    Command::new("docker")
        .args(["image", "inspect", "unleash:latest"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if the .env file exists in the docker directory
fn env_file_exists(docker_dir: &Path) -> bool {
    docker_dir.join(".env").exists()
}

// ─── Subcommands ────────────────────────────────────────────

pub fn run_setup(docker_dir: &Path) -> io::Result<()> {
    needs_sudo("setup")?;

    println!("\x1b[1m=== Sandbox Setup ===\x1b[0m\n");

    // Step 1: Check Docker
    print!("  Docker daemon... ");
    if docker_running() {
        println!("\x1b[32m✓\x1b[0m running");
    } else {
        println!("\x1b[31m✗\x1b[0m not running");
        eprintln!("\nPlease start Docker first: sudo systemctl start docker");
        return Err(io::Error::other("Docker not running"));
    }

    // Step 2: Check/install gVisor
    print!("  gVisor (runsc)... ");
    if gvisor_installed() {
        println!("\x1b[32m✓\x1b[0m installed");
    } else {
        println!("\x1b[33m!\x1b[0m not found — installing...");
        let arch = if cfg!(target_arch = "x86_64") {
            "amd64"
        } else if cfg!(target_arch = "aarch64") {
            "arm64"
        } else {
            return Err(io::Error::other("Unsupported architecture for gVisor"));
        };

        // Download and install gVisor
        let url = format!(
            "https://storage.googleapis.com/gvisor/releases/release/latest/{}/runsc",
            arch
        );
        let ok = run_command("bash", &[
            "-c",
            &format!(
                "curl -fsSL -o /tmp/runsc '{}' && chmod +x /tmp/runsc && mv /tmp/runsc /usr/local/bin/runsc && runsc install && systemctl restart docker",
                url
            ),
        ])?;
        if ok {
            println!("  \x1b[32m✓\x1b[0m gVisor installed and Docker restarted");
        } else {
            eprintln!("  \x1b[31m✗\x1b[0m gVisor installation failed");
            eprintln!("    See https://gvisor.dev/docs/user_guide/install/");
            return Err(io::Error::other("gVisor install failed"));
        }
    }

    // Step 3: Create sandbox network + iptables rules
    print!("  Sandbox network... ");
    let script = sandbox_network_script(docker_dir);
    if script.exists() {
        let ok = run_command("bash", &[script.to_str().unwrap(), "setup"])?;
        if ok {
            println!("\x1b[32m✓\x1b[0m");
        } else {
            println!("\x1b[31m✗\x1b[0m sandbox-network.sh setup failed");
            return Err(io::Error::other("network setup failed"));
        }
    } else {
        println!("\x1b[31m✗\x1b[0m sandbox-network.sh not found at {}", script.display());
        return Err(io::Error::other("script not found"));
    }

    // Step 4: Build Docker image
    print!("  Docker image... ");
    if image_exists() {
        println!("\x1b[32m✓\x1b[0m unleash:latest exists");
        println!("    (to rebuild: docker build -f {}/Dockerfile -t unleash {})",
            docker_dir.display(),
            docker_dir.parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".to_string()),
        );
    } else {
        println!("\x1b[33m!\x1b[0m not found — building...");
        let context = docker_dir
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let dockerfile = docker_dir.join("Dockerfile");
        let ok = run_command("docker", &[
            "build",
            "-f",
            dockerfile.to_str().unwrap(),
            "-t",
            "unleash:latest",
            &context,
        ])?;
        if ok {
            println!("  \x1b[32m✓\x1b[0m Image built");
        } else {
            eprintln!("  \x1b[31m✗\x1b[0m Image build failed");
            return Err(io::Error::other("docker build failed"));
        }
    }

    // Step 5: Check .env
    print!("  API keys (.env)... ");
    if env_file_exists(docker_dir) {
        println!("\x1b[32m✓\x1b[0m found");
    } else {
        println!("\x1b[33m!\x1b[0m not found");
        let example = docker_dir.join("example.env");
        let dotenv = docker_dir.join(".env");
        if example.exists() {
            std::fs::copy(&example, &dotenv)?;
            println!("    Created {} from example.env", dotenv.display());
            println!("    \x1b[33mEdit it with your API keys before running agents.\x1b[0m");
        } else {
            println!("    Create docker/.env with your API keys (see docker/example.env)");
        }
    }

    println!("\n\x1b[32m=== Sandbox ready! ===\x1b[0m");
    println!("  Run an agent:  unleash sandbox claude");
    println!("  Check status:  unleash sandbox status");
    Ok(())
}

pub fn run_status(docker_dir: &Path) -> io::Result<()> {
    println!("\x1b[1mSandbox Status\x1b[0m\n");

    // Docker
    print!("  Docker daemon:    ");
    if docker_running() {
        println!("\x1b[32m✓\x1b[0m running");
    } else {
        println!("\x1b[31m✗\x1b[0m not running");
    }

    // gVisor
    print!("  gVisor (runsc):   ");
    if gvisor_installed() {
        let ver = run_command_output("runsc", &["--version"])
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            .unwrap_or_else(|| "installed".to_string());
        println!("\x1b[32m✓\x1b[0m {}", ver);
    } else {
        println!("\x1b[31m✗\x1b[0m not installed");
    }

    // Network
    print!("  Docker network:   ");
    if network_exists() {
        println!("\x1b[32m✓\x1b[0m unleash-sandbox (172.30.0.0/16)");
    } else {
        println!("\x1b[31m✗\x1b[0m not created");
    }

    // iptables
    print!("  iptables rules:   ");
    if iptables_rules_active() {
        println!("\x1b[32m✓\x1b[0m LAN-blocking rules active");
    } else {
        println!("\x1b[33m?\x1b[0m cannot verify (need root, or rules missing)");
    }

    // Image
    print!("  Container image:  ");
    if image_exists() {
        let age = run_command_output("docker", &[
            "image", "inspect", "unleash:latest",
            "--format", "{{.Created}}",
        ])
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
        println!("\x1b[32m✓\x1b[0m unleash:latest ({})", if age.is_empty() { "unknown age" } else { &age });
    } else {
        println!("\x1b[31m✗\x1b[0m not built");
    }

    // .env
    print!("  API keys (.env):  ");
    if env_file_exists(docker_dir) {
        // Count non-empty, non-comment lines
        let count = std::fs::read_to_string(docker_dir.join(".env"))
            .ok()
            .map(|content| {
                content
                    .lines()
                    .filter(|l| {
                        let l = l.trim();
                        !l.is_empty() && !l.starts_with('#') && l.contains('=')
                            && l.split('=').nth(1).map(|v| !v.is_empty()).unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);
        if count > 0 {
            println!("\x1b[32m✓\x1b[0m {} key(s) configured", count);
        } else {
            println!("\x1b[33m!\x1b[0m file exists but no keys set");
        }
    } else {
        println!("\x1b[31m✗\x1b[0m not found (cp docker/example.env docker/.env)");
    }

    // LAN exceptions
    let exceptions = run_command_output("iptables", &["-L", "DOCKER-USER", "-n"])
        .ok()
        .map(|output| {
            output
                .lines()
                .filter(|l| l.contains("ACCEPT") && l.contains("172.30.0.0/16"))
                .count()
        })
        .unwrap_or(0);
    if exceptions > 0 {
        println!("  LAN exceptions:   {} active", exceptions);
    }

    println!();
    Ok(())
}

pub fn run_agent(docker_dir: &Path, agent: &str, extra_args: &[String]) -> io::Result<()> {
    // Validate agent name
    let valid_agents = ["claude", "codex", "gemini", "opencode", "bash", "unleash"];
    if !valid_agents.contains(&agent) {
        eprintln!(
            "\x1b[31merror:\x1b[0m Unknown agent '{}'. Valid agents: {}",
            agent,
            valid_agents.join(", ")
        );
        return Err(io::Error::other("unknown agent"));
    }

    // Preflight checks
    if !docker_running() {
        eprintln!("\x1b[31merror:\x1b[0m Docker is not running. Start it first.");
        return Err(io::Error::other("Docker not running"));
    }

    if !image_exists() {
        eprintln!("\x1b[31merror:\x1b[0m Docker image not built. Run: unleash sandbox setup");
        return Err(io::Error::other("image not built"));
    }

    if !network_exists() {
        eprintln!("\x1b[33mwarning:\x1b[0m Sandbox network not found. LAN isolation may not be active.");
        eprintln!("  Fix: sudo unleash sandbox setup");
    }

    if !iptables_rules_active() {
        eprintln!("\x1b[33mwarning:\x1b[0m Cannot verify iptables rules (need root, or rules missing after reboot).");
        eprintln!("  Fix: sudo ./docker/sandbox-network.sh setup");
    }

    if !env_file_exists(docker_dir) {
        eprintln!("\x1b[33mwarning:\x1b[0m No .env file found. API keys may not be set.");
        eprintln!("  Fix: cp docker/example.env docker/.env && edit docker/.env");
    }

    // Build the docker compose command
    let compose_file = docker_dir.join("docker-compose.yml");

    let mut cmd = Command::new("docker");
    cmd.args(["compose", "-f", compose_file.to_str().unwrap()]);

    // Check for local-api overlay
    let local_api_compose = docker_dir.join("docker-compose.local-api.yml");
    if std::env::var("LOCAL_API_BASE").is_ok() && local_api_compose.exists() {
        cmd.args(["-f", local_api_compose.to_str().unwrap()]);
    }

    cmd.args(["run", "--rm"]);

    // Pass through extra args (e.g., -p "prompt")
    // These go BEFORE the service name for docker compose run
    // Actually, agent-specific args should go after the service name
    cmd.arg(agent);

    // Pass extra args to the container entrypoint
    for arg in extra_args {
        cmd.arg(arg);
    }

    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "Container exited with code {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

pub fn run_teardown(docker_dir: &Path) -> io::Result<()> {
    needs_sudo("teardown")?;

    println!("\x1b[1m=== Sandbox Teardown ===\x1b[0m\n");

    let script = sandbox_network_script(docker_dir);
    if script.exists() {
        run_command("bash", &[script.to_str().unwrap(), "teardown"])?;
    }

    println!("\n\x1b[32mTeardown complete.\x1b[0m");
    Ok(())
}

pub fn run_allow_ip(docker_dir: &Path, ip: &str) -> io::Result<()> {
    needs_sudo("allow-ip")?;

    let script = sandbox_network_script(docker_dir);
    if !script.exists() {
        eprintln!("\x1b[31merror:\x1b[0m sandbox-network.sh not found");
        return Err(io::Error::other("script not found"));
    }

    let ok = run_command("bash", &[script.to_str().unwrap(), "allow-ip", ip])?;
    if !ok {
        return Err(io::Error::other("allow-ip failed"));
    }
    Ok(())
}

pub fn run_revoke_ip(docker_dir: &Path, ip: &str) -> io::Result<()> {
    needs_sudo("revoke-ip")?;

    let script = sandbox_network_script(docker_dir);
    if !script.exists() {
        eprintln!("\x1b[31merror:\x1b[0m sandbox-network.sh not found");
        return Err(io::Error::other("script not found"));
    }

    let ok = run_command("bash", &[script.to_str().unwrap(), "revoke-ip", ip])?;
    if !ok {
        return Err(io::Error::other("revoke-ip failed"));
    }
    Ok(())
}

/// Main dispatch for `unleash sandbox <action>`
pub fn handle_sandbox(action: &SandboxAction) -> io::Result<()> {
    let docker_dir = find_docker_dir().ok_or_else(|| {
        io::Error::other(
            "Cannot find docker/ directory. Run from the unleash repo root, \
             or ensure docker files are installed at /usr/local/share/unleash/docker/",
        )
    })?;

    match action {
        SandboxAction::Setup => run_setup(&docker_dir),
        SandboxAction::Status => run_status(&docker_dir),
        SandboxAction::Teardown => run_teardown(&docker_dir),
        SandboxAction::AllowIp { ip } => run_allow_ip(&docker_dir, ip),
        SandboxAction::RevokeIp { ip } => run_revoke_ip(&docker_dir, ip),
        SandboxAction::Run(args) => {
            let agent = args.first().ok_or_else(|| {
                io::Error::other("Usage: unleash sandbox <agent> [args...]\n  Agents: claude, codex, gemini, opencode, bash")
            })?;
            run_agent(&docker_dir, agent, &args[1..])
        }
    }
}

/// Sandbox subcommand actions (parsed by clap in cli.rs)
#[derive(clap::Subcommand, Debug)]
pub enum SandboxAction {
    /// Set up the sandbox: install gVisor, create network, apply firewall rules, build image
    Setup,

    /// Show sandbox health status
    Status,

    /// Remove sandbox network and firewall rules
    Teardown,

    /// Open a single LAN IP through the sandbox firewall (for local API servers)
    #[command(name = "allow-ip")]
    AllowIp {
        /// The private IP address to allow (e.g., 192.168.1.100)
        ip: String,
    },

    /// Close a previously opened LAN IP
    #[command(name = "revoke-ip")]
    RevokeIp {
        /// The private IP address to revoke
        ip: String,
    },

    /// Run an agent in the sandbox (e.g., `unleash sandbox claude`)
    #[command(external_subcommand)]
    Run(Vec<String>),
}
