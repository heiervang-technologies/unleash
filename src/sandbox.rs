//! Sandbox subcommand: one-command secure container setup and execution.
//!
//! Wraps Docker + gVisor + LAN isolation into a seamless experience:
//!   - `unleash sandbox setup`     — install gVisor, create network, set iptables
//!   - `unleash sandbox run [agent]` — run an agent (or bash shell) in the sandbox
//!   - `unleash sandbox status`    — health check
//!   - `unleash sandbox teardown`  — clean up
//!   - `unleash sandbox allow-ip`  — open a LAN IP for local API access
//!   - `unleash sandbox revoke-ip` — close it

use include_dir::{include_dir, Dir};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DOCKER_IMAGE: &str = "marksverdhei/unleash";
const DOCKER_TAG: &str = "latest";

/// Canonical environment-variable keys the sandbox wizard surfaces by default.
/// These match the API keys the wrapped agent CLIs read at runtime.
pub const CANONICAL_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "OPENROUTER_API_KEY",
    "LOCAL_API_BASE",
    "OPENAI_BASE_URL",
];

/// Embedded copy of the repo's docker/ directory, baked into the binary so that
/// `sandbox setup` works even after `cp unleash ~/.local/bin/` with no repo.
static EMBEDDED_DOCKER: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/docker");

/// Write the embedded docker/ tree to `~/.local/share/unleash/docker/` if it's
/// not already installed there. Returns the path it wrote (or found).
fn ensure_installed_docker_dir() -> io::Result<PathBuf> {
    let data_dir = dirs::data_dir().ok_or_else(|| {
        io::Error::other("Could not locate user data dir (dirs::data_dir() returned None)")
    })?;
    let install_path = data_dir.join("unleash").join("docker");
    if install_path.join("Dockerfile").exists() {
        return Ok(install_path);
    }
    std::fs::create_dir_all(&install_path)?;
    EMBEDDED_DOCKER.extract(&install_path).map_err(|e| {
        io::Error::other(format!(
            "Failed to extract embedded docker/ to {}: {}",
            install_path.display(),
            e
        ))
    })?;
    // include_dir preserves file contents but not modes — re-chmod shell scripts.
    for script in ["sandbox-network.sh", "entrypoint.sh"] {
        let p = install_path.join(script);
        if p.exists() {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms)?;
        }
    }
    println!(
        "\x1b[36m→\x1b[0m Installed docker assets to {}",
        install_path.display()
    );
    Ok(install_path)
}

/// Find the docker/ directory relative to the unleash binary or repo root.
pub fn find_docker_dir() -> Option<PathBuf> {
    // 1. User-local install: ~/.local/share/unleash/docker/
    if let Some(data_dir) = dirs::data_dir() {
        let local_path = data_dir.join("unleash").join("docker");
        if local_path.join("Dockerfile").exists() {
            return Some(local_path);
        }
    }

    // 2. System-wide install: /usr/local/share/unleash/docker/
    if let Ok(exe) = std::env::current_exe() {
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            let share_path = prefix.join("share").join("unleash").join("docker");
            if share_path.join("Dockerfile").exists() {
                return Some(share_path);
            }
        }
    }

    // 3. Repo layout: cwd or parent has docker/
    if let Ok(cwd) = std::env::current_dir() {
        for dir in [&cwd, &cwd.join(".."), &cwd.join("../..")]
            .iter()
            .filter_map(|p| p.canonicalize().ok())
        {
            let docker_dir = dir.join("docker");
            if docker_dir.join("Dockerfile").exists() {
                return Some(docker_dir);
            }
        }
    }

    None
}

/// Like `find_docker_dir` but falls back to extracting the embedded docker/ tree
/// into `~/.local/share/unleash/docker/`. Always succeeds (or returns an error
/// with a clear remediation message).
pub fn find_or_install_docker_dir() -> io::Result<PathBuf> {
    if let Some(dir) = find_docker_dir() {
        return Ok(dir);
    }
    ensure_installed_docker_dir()
}

fn sandbox_network_script(docker_dir: &Path) -> PathBuf {
    docker_dir.join("sandbox-network.sh")
}

/// Convert a Path to a string, lossy but safe (no panics on non-UTF8).
fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
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

/// Returns true if the current process is root (uid 0).
///
/// The wizard *should not* be running as root — it shells out to sudo
/// per privileged step. We use this only to *warn* a user who somehow
/// got here as root that they should run as their normal user.
pub fn is_root() -> bool {
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

/// Check if gVisor (runsc) is installed
pub fn gvisor_installed() -> bool {
    check_command_exists("runsc")
}

/// Check if Docker is running
pub fn docker_running() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if the sandbox network exists
pub fn network_exists() -> bool {
    Command::new("docker")
        .args(["network", "inspect", "unleash-sandbox"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if iptables LAN-blocking rules are in place.
///
/// Checks three chains — all must be present for the sandbox to be safe:
///   - raw/PREROUTING (pre-DNAT; catches k8s NodePorts, Docker port-maps)
///   - DOCKER-USER   (container → other LAN hosts via FORWARD)
///   - INPUT         (container → Docker host itself)
pub fn iptables_rules_active() -> bool {
    // Silence stderr — without root, iptables prints noisy lock errors we don't care about.
    let raw_ok = Command::new("iptables")
        .args([
            "-t",
            "raw",
            "-C",
            "PREROUTING",
            "-s",
            "172.30.0.0/16",
            "-d",
            "10.0.0.0/8",
            "-j",
            "DROP",
        ])
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let forward_ok = Command::new("iptables")
        .args(["-L", "DOCKER-USER", "-n"])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("172.30.0.0/16"))
        .unwrap_or(false);
    let input_ok = Command::new("iptables")
        .args(["-C", "INPUT", "-s", "172.30.0.0/16", "-j", "DROP"])
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    raw_ok && forward_ok && input_ok
}

/// Check if the unleash Docker image exists (either local or pulled)
pub fn image_exists() -> bool {
    let full_image = format!("{}:{}", DOCKER_IMAGE, DOCKER_TAG);
    Command::new("docker")
        .args(["image", "inspect", &full_image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get the canonical Docker Hub image name (`marksverdhei/unleash:latest`).
/// Both the pulled image and a `docker compose build` produce this tag.
fn image_name() -> String {
    format!("{}:{}", DOCKER_IMAGE, DOCKER_TAG)
}

/// Check if the .env file exists in the docker directory
pub fn env_file_exists(docker_dir: &Path) -> bool {
    docker_dir.join(".env").exists()
}

// ─── Step functions ─────────────────────────────────────────
//
// Each step is a small function returning a structured outcome.
// The CLI `run_setup` runs them in order and prints colored output.
// The TUI wizard (see tui::app) calls them individually so it can
// render per-step status, retry, or skip.

/// Why a step failed (used by both CLI and TUI to suggest next actions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepFailure {
    /// Docker daemon isn't running. User must start it.
    DockerNotRunning,
    /// Architecture isn't supported for the gVisor binary release.
    UnsupportedArch,
    /// `sudo` not available on this system.
    SudoMissing,
    /// User cancelled the sudo password prompt or auth failed.
    SudoAuth,
    /// No TTY available — sudo can't prompt for a password.
    NoTty,
    /// Network setup script not found at the expected path.
    ScriptMissing(PathBuf),
    /// Docker image pull failed *and* repo source for a local build is absent.
    PullFailedNoSource(String),
    /// Generic recoverable error with a human-readable message.
    Recoverable(String),
}

impl StepFailure {
    /// Human-readable summary suitable for the TUI details pane.
    pub fn message(&self) -> String {
        match self {
            StepFailure::DockerNotRunning => "Docker daemon is not running.".into(),
            StepFailure::UnsupportedArch => {
                "Unsupported CPU architecture — only x86_64 and aarch64 have prebuilt gVisor binaries.".into()
            }
            StepFailure::SudoMissing => {
                "`sudo` is not installed on PATH. Install sudo or run as a user with passwordless privilege escalation.".into()
            }
            StepFailure::SudoAuth => {
                "sudo authentication was cancelled or failed. Retry?".into()
            }
            StepFailure::NoTty => {
                "This step needs an interactive sudo prompt. Run the wizard from a terminal, or pre-authorize with `sudo -v` first.".into()
            }
            StepFailure::ScriptMissing(p) => {
                format!("sandbox-network.sh not found at {}", p.display())
            }
            StepFailure::PullFailedNoSource(img) => format!(
                "`docker pull {}` failed and the repo source isn't available for a local build.",
                img
            ),
            StepFailure::Recoverable(s) => s.clone(),
        }
    }

    /// Concrete next-step suggestion(s) shown alongside the error.
    pub fn next_actions(&self) -> Vec<String> {
        match self {
            StepFailure::DockerNotRunning => vec![
                "Start Docker: sudo systemctl start docker".into(),
                "Then retry this step.".into(),
            ],
            StepFailure::UnsupportedArch => vec![
                "Build gVisor from source (see https://gvisor.dev/docs/user_guide/install/), then retry.".into(),
            ],
            StepFailure::SudoMissing => vec![
                "Install sudo (e.g. `pacman -S sudo` / `apt install sudo`).".into(),
                "Or run the manual setup steps directly (docker/sandbox-network.sh).".into(),
            ],
            StepFailure::SudoAuth => vec![
                "Retry — you'll be prompted for your password again.".into(),
                "Or run `sudo -v` in another terminal first to cache credentials.".into(),
            ],
            StepFailure::NoTty => vec![
                "Open the wizard in an interactive terminal.".into(),
                "Or pre-authorize: `sudo -v` then re-run.".into(),
            ],
            StepFailure::ScriptMissing(_) => vec![
                "Reinstall unleash, or run from inside the unleash repo.".into(),
            ],
            StepFailure::PullFailedNoSource(img) => vec![
                format!("Check network connectivity and retry: docker pull {}", img),
                "Or run the wizard from inside the unleash repo to enable local build fallback.".into(),
            ],
            StepFailure::Recoverable(_) => vec!["Retry the step.".into()],
        }
    }
}

impl std::fmt::Display for StepFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl From<StepFailure> for io::Error {
    fn from(f: StepFailure) -> Self {
        io::Error::other(f.message())
    }
}

impl From<io::Error> for StepFailure {
    fn from(e: io::Error) -> Self {
        StepFailure::Recoverable(e.to_string())
    }
}

/// Outcome of a sudo-fronted command: distinguishes user-facing failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SudoOutcome {
    Ok,
    /// `sudo` itself isn't available.
    SudoMissing,
    /// User cancelled the password prompt, wrong password, or sudoers rejection.
    AuthFailed,
    /// No TTY — sudo can't prompt.
    NoTty,
    /// The privileged command itself failed (with stderr).
    CommandFailed(String),
    /// Other I/O error launching the command.
    #[allow(dead_code)]
    Other(String),
}

/// Run a privileged command via `sudo`, classifying common failure modes.
///
/// Pass `cmd_path` as an *absolute* path so it works under sudo's `secure_path`.
/// `args` are forwarded to the privileged command. Output is inherited so the
/// password prompt is visible to the user — callers running inside a TUI must
/// suspend the alternate screen first.
pub fn run_sudo<P: AsRef<std::ffi::OsStr>>(cmd_path: &str, args: &[P]) -> io::Result<SudoOutcome> {
    if which::which("sudo").is_err() {
        return Ok(SudoOutcome::SudoMissing);
    }
    // Only require a TTY if we don't already have cached creds.
    if !has_tty() && !sudo_has_cached_credentials() {
        return Ok(SudoOutcome::NoTty);
    }

    let mut cmd = Command::new("sudo");
    cmd.arg(cmd_path);
    for a in args {
        cmd.arg(a);
    }
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status()?;
    if status.success() {
        return Ok(SudoOutcome::Ok);
    }
    // sudo returns 1 on auth failure / cancel and on most command failures.
    // Use `sudo -n true` afterward to disambiguate: if `-n` succeeds, creds
    // were valid (so the command itself failed); otherwise it was an auth issue.
    let auth_ok = Command::new("sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !auth_ok {
        return Ok(SudoOutcome::AuthFailed);
    }
    Ok(SudoOutcome::CommandFailed(format!(
        "exited with code {}",
        status.code().unwrap_or(-1)
    )))
}

fn has_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn sudo_has_cached_credentials() -> bool {
    Command::new("sudo")
        .args(["-n", "true"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Step 1: verify Docker daemon is reachable.
pub fn step_check_docker() -> Result<String, StepFailure> {
    if docker_running() {
        Ok("Docker daemon is running.".into())
    } else {
        Err(StepFailure::DockerNotRunning)
    }
}

/// Step 2: detect / install gVisor (`runsc`).
///
/// If `auto_install` is false, this only checks. Otherwise it tries the
/// privileged install path (one sudo invocation downloading the official
/// release tarball and registering the runtime with Docker).
pub fn step_gvisor(auto_install: bool) -> Result<String, StepFailure> {
    if gvisor_installed() {
        return Ok("gVisor (runsc) is installed.".into());
    }
    if !auto_install {
        return Err(StepFailure::Recoverable(
            "gVisor not installed. Auto-install was declined.".into(),
        ));
    }
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        return Err(StepFailure::UnsupportedArch);
    };
    let url = format!(
        "https://storage.googleapis.com/gvisor/releases/release/latest/{}/runsc",
        arch
    );
    // Run the install via sudo bash -c with an absolute /usr/bin/bash where
    // available so secure_path doesn't bite us; otherwise fall back to "bash".
    let bash = which::which("bash")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "bash".to_string());
    let script = format!(
        "curl -fsSL -o /tmp/runsc '{}' && chmod +x /tmp/runsc && mv /tmp/runsc /usr/local/bin/runsc && runsc install && systemctl restart docker",
        url
    );
    let outcome = run_sudo(&bash, &["-c", &script])?;
    match outcome {
        SudoOutcome::Ok => Ok("gVisor installed and Docker restarted.".into()),
        SudoOutcome::SudoMissing => Err(StepFailure::SudoMissing),
        SudoOutcome::AuthFailed => Err(StepFailure::SudoAuth),
        SudoOutcome::NoTty => Err(StepFailure::NoTty),
        SudoOutcome::CommandFailed(s) | SudoOutcome::Other(s) => Err(StepFailure::Recoverable(
            format!("gVisor install failed: {}", s),
        )),
    }
}

/// Step 3: create sandbox network + iptables rules via the bundled script.
///
/// This step shells out to `sudo bash <abs sandbox-network.sh> setup`.
pub fn step_sandbox_network(docker_dir: &Path) -> Result<String, StepFailure> {
    let script = sandbox_network_script(docker_dir);
    if !script.exists() {
        return Err(StepFailure::ScriptMissing(script));
    }
    if iptables_rules_active() && network_exists() {
        return Ok("Sandbox network and iptables rules already active.".into());
    }
    // Use absolute path to bash so sudo's secure_path doesn't strip it.
    let bash = which::which("bash")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/bin/bash".to_string());
    let abs_script = path_str(&script);
    let outcome = run_sudo(&bash, &[abs_script.as_str(), "setup"])?;
    match outcome {
        SudoOutcome::Ok => Ok("Sandbox network and iptables rules configured.".into()),
        SudoOutcome::SudoMissing => Err(StepFailure::SudoMissing),
        SudoOutcome::AuthFailed => Err(StepFailure::SudoAuth),
        SudoOutcome::NoTty => Err(StepFailure::NoTty),
        SudoOutcome::CommandFailed(s) | SudoOutcome::Other(s) => Err(StepFailure::Recoverable(
            format!("sandbox-network.sh setup failed: {}", s),
        )),
    }
}

/// Step 4: ensure the unleash container image is locally available.
///
/// Tries `docker pull` first, then falls back to a local `docker build`
/// when the unleash repo source tree is reachable from `docker_dir`.
pub fn step_container_image(docker_dir: &Path) -> Result<String, StepFailure> {
    if image_exists() {
        return Ok(format!("Image {} present.", image_name()));
    }
    let full_image = format!("{}:{}", DOCKER_IMAGE, DOCKER_TAG);
    if run_command("docker", &["pull", &full_image]).unwrap_or(false) {
        return Ok(format!("Pulled {}.", full_image));
    }
    let dockerfile = docker_dir.join("Dockerfile");
    let context = docker_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let repo_sources_present = dockerfile.exists()
        && context.join("Cargo.toml").exists()
        && context.join("src").is_dir()
        && context.join("scripts/unleash-exit").exists();
    if !repo_sources_present {
        return Err(StepFailure::PullFailedNoSource(full_image));
    }
    let ok = run_command(
        "docker",
        &[
            "build",
            "-f",
            &path_str(&dockerfile),
            "-t",
            &full_image,
            &context.to_string_lossy(),
        ],
    )
    .unwrap_or(false);
    if ok {
        Ok(format!("Built {} locally.", full_image))
    } else {
        Err(StepFailure::Recoverable(
            "docker build failed — see output above.".into(),
        ))
    }
}

/// Step 5 (CLI variant): copy `example.env` → `.env` if missing.
/// The TUI wizard handles env config interactively instead.
pub fn step_env_seed(docker_dir: &Path) -> Result<String, StepFailure> {
    if env_file_exists(docker_dir) {
        return Ok(".env already exists.".into());
    }
    let example = docker_dir.join("example.env");
    let dotenv = docker_dir.join(".env");
    if example.exists() {
        std::fs::copy(&example, &dotenv)
            .map_err(|e| StepFailure::Recoverable(format!("Failed to copy example.env: {}", e)))?;
        Ok(format!(
            "Created {} from example.env. Edit it with your API keys before running agents.",
            dotenv.display()
        ))
    } else {
        Ok("No example.env found. Create docker/.env manually.".into())
    }
}

// ─── Env passthrough config ─────────────────────────────────

/// Path to the JSON file listing keys to passthrough at `docker run` time.
/// Lives in the user's config dir so it survives reinstalls.
pub fn passthrough_config_path() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(base.join("unleash").join("sandbox-passthrough.toml"))
}

/// Persist the list of env-var keys that should be propagated from the host
/// at `docker run` time. The file format is a tiny TOML document:
/// ```toml
/// passthrough = ["ANTHROPIC_API_KEY", "GEMINI_API_KEY"]
/// ```
pub fn save_passthrough_keys(path: &Path, keys: &[String]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut buf = String::from("# Generated by `unleash sandbox` wizard.\n");
    buf.push_str("# Lists env-var keys whose values are propagated from the host\n");
    buf.push_str("# into the sandbox container at runtime via `docker run -e <KEY>`.\n");
    buf.push_str("passthrough = [\n");
    for k in keys {
        buf.push_str(&format!("  \"{}\",\n", k.replace('"', "\\\"")));
    }
    buf.push_str("]\n");
    std::fs::write(path, buf)
}

/// Load the passthrough key list. Returns an empty Vec if the file is missing
/// or malformed (with a debug-only warning) — this is best-effort config.
pub fn load_passthrough_keys(path: &Path) -> Vec<String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    #[derive(serde::Deserialize)]
    struct Doc {
        passthrough: Option<Vec<String>>,
    }
    toml::from_str::<Doc>(&text)
        .ok()
        .and_then(|d| d.passthrough)
        .unwrap_or_default()
}

/// Walk `example.env` and return the canonical key names mentioned (commented
/// or uncommented). Used by the wizard to show *just* the keys this user is
/// likely to care about, alongside `CANONICAL_ENV_KEYS`.
pub fn canonical_keys_from_example(docker_dir: &Path) -> Vec<String> {
    let mut keys: Vec<String> = CANONICAL_ENV_KEYS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let example = docker_dir.join("example.env");
    if let Ok(text) = std::fs::read_to_string(&example) {
        for line in text.lines() {
            let trimmed = line.trim().trim_start_matches('#').trim_start();
            if let Some(eq) = trimmed.find('=') {
                let key = trimmed[..eq].trim();
                if !key.is_empty()
                    && key
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                    && !keys.contains(&key.to_string())
                {
                    keys.push(key.to_string());
                }
            }
        }
    }
    keys
}

/// Inject `-e KEY` flags into a `docker run` (or `docker compose run`) command
/// for every key in the passthrough config that's actually present in the
/// host env. Idempotent and safe to call when no config exists.
pub fn apply_passthrough_env(cmd: &mut Command) {
    let path = match passthrough_config_path() {
        Some(p) => p,
        None => return,
    };
    let keys = load_passthrough_keys(&path);
    for key in keys {
        if std::env::var(&key).is_ok() {
            cmd.args(["-e", &key]);
        }
    }
}

/// Write a `docker/.env` file from explicit-value entries. Existing file is
/// overwritten — callers that want to merge should read first.
pub fn write_dotenv(docker_dir: &Path, entries: &[(String, String)]) -> io::Result<()> {
    let dotenv = docker_dir.join(".env");
    let mut buf = String::from("# Generated by `unleash sandbox` wizard.\n");
    buf.push_str("# Explicit values for keys you chose NOT to passthrough from the host.\n");
    for (k, v) in entries {
        if v.is_empty() {
            continue;
        }
        buf.push_str(&format!("{}={}\n", k, v));
    }
    std::fs::write(dotenv, buf)
}

// ─── Subcommands ────────────────────────────────────────────

pub fn run_setup(docker_dir: &Path) -> io::Result<()> {
    if is_root() {
        eprintln!(
            "\x1b[33mwarning:\x1b[0m unleash should be run as your normal user. \
             The wizard handles privilege escalation per-step internally."
        );
    }

    println!("\x1b[1m=== Sandbox Setup ===\x1b[0m\n");

    // Step 1: Check Docker
    print!("  Docker daemon... ");
    match step_check_docker() {
        Ok(_) => println!("\x1b[32m✓\x1b[0m running"),
        Err(e) => {
            println!("\x1b[31m✗\x1b[0m {}", e.message());
            for hint in e.next_actions() {
                eprintln!("    {}", hint);
            }
            return Err(e.into());
        }
    }

    // Step 2: Check/install gVisor
    print!("  gVisor (runsc)... ");
    if gvisor_installed() {
        println!("\x1b[32m✓\x1b[0m installed");
    } else {
        println!("\x1b[33m!\x1b[0m not found — installing (may prompt for sudo password)...");
        match step_gvisor(true) {
            Ok(msg) => println!("  \x1b[32m✓\x1b[0m {}", msg),
            Err(e) => {
                eprintln!("  \x1b[31m✗\x1b[0m {}", e.message());
                for hint in e.next_actions() {
                    eprintln!("     {}", hint);
                }
                return Err(e.into());
            }
        }
    }

    // Step 3: Create sandbox network + iptables rules
    print!("  Sandbox network... ");
    if iptables_rules_active() && network_exists() {
        println!("\x1b[32m✓\x1b[0m already active");
    } else {
        println!("(may prompt for sudo password)");
        match step_sandbox_network(docker_dir) {
            Ok(msg) => println!("  \x1b[32m✓\x1b[0m {}", msg),
            Err(e) => {
                eprintln!("  \x1b[31m✗\x1b[0m {}", e.message());
                for hint in e.next_actions() {
                    eprintln!("     {}", hint);
                }
                return Err(e.into());
            }
        }
    }

    // Step 4: Pull / build container image
    print!("  Docker image... ");
    if image_exists() {
        println!("\x1b[32m✓\x1b[0m {} exists", image_name());
        println!(
            "    (to update: docker pull {}:{})",
            DOCKER_IMAGE, DOCKER_TAG
        );
    } else {
        println!("\x1b[33m!\x1b[0m not found — pulling from Docker Hub...");
        match step_container_image(docker_dir) {
            Ok(msg) => println!("  \x1b[32m✓\x1b[0m {}", msg),
            Err(e) => {
                eprintln!("  \x1b[31m✗\x1b[0m {}", e.message());
                for hint in e.next_actions() {
                    eprintln!("     {}", hint);
                }
                return Err(e.into());
            }
        }
    }

    // Step 5: Seed .env if absent (CLI mode — TUI wizard does interactive config)
    print!("  API keys (.env)... ");
    if env_file_exists(docker_dir) {
        println!("\x1b[32m✓\x1b[0m found");
    } else {
        match step_env_seed(docker_dir) {
            Ok(msg) => {
                println!("\x1b[33m!\x1b[0m");
                println!("    {}", msg);
            }
            Err(e) => {
                println!("\x1b[31m✗\x1b[0m {}", e.message());
            }
        }
    }

    println!("\n\x1b[32m=== Sandbox ready! ===\x1b[0m");
    println!("  Run an agent:  unleash sandbox run claude");
    println!("  Open a shell:  unleash sandbox run");
    println!("  Check status:  unleash sandbox status");
    Ok(())
}

pub fn run_status(docker_dir: Option<&Path>) -> io::Result<()> {
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
        let name = image_name();
        let age = run_command_output(
            "docker",
            &["image", "inspect", &name, "--format", "{{.Created}}"],
        )
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
        println!(
            "\x1b[32m✓\x1b[0m {} ({})",
            name,
            if age.is_empty() { "unknown age" } else { &age }
        );
    } else {
        println!("\x1b[31m✗\x1b[0m not found (run: unleash sandbox setup)");
    }

    // .env (only check if we know the docker dir)
    print!("  API keys (.env):  ");
    if let Some(dir) = docker_dir {
        if env_file_exists(dir) {
            let count = std::fs::read_to_string(dir.join(".env"))
                .ok()
                .map(|content| {
                    content
                        .lines()
                        .filter(|l| {
                            let l = l.trim();
                            !l.is_empty()
                                && !l.starts_with('#')
                                && l.contains('=')
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
    } else {
        println!("\x1b[33m-\x1b[0m skipped (docker dir not found)");
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

fn validate_sandbox_name(name: &str) -> io::Result<()> {
    // RFC 1123 hostname: alphanumeric + hyphens, max 63 chars, no leading/trailing hyphen
    if name.is_empty() || name.len() > 63 {
        return Err(io::Error::other("sandbox name must be 1-63 characters"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(io::Error::other(
            "sandbox name must contain only alphanumeric characters and hyphens",
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(io::Error::other(
            "sandbox name must not start or end with a hyphen",
        ));
    }
    Ok(())
}

pub fn run_agent(
    docker_dir: Option<&Path>,
    agent: &str,
    name: &str,
    extra_args: &[String],
    unsafe_override: bool,
) -> io::Result<()> {
    // Validate sandbox name (used as Docker hostname, must be RFC 1123 compliant)
    validate_sandbox_name(name)?;

    // Validate agent name — must match a CLI built into the sandbox image
    // (docker/Dockerfile) and a service in docker-compose.yml.
    let valid_agents = [
        "claude", "codex", "gemini", "opencode", "pi", "bash", "unleash",
    ];
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

    let mut network_ok = true;
    if !network_exists() {
        network_ok = false;
        eprintln!("\x1b[31merror:\x1b[0m Sandbox network not found. LAN isolation is not active.");
        eprintln!("  Fix: sudo unleash sandbox setup");
    }

    if !iptables_rules_active() {
        network_ok = false;
        eprintln!("\x1b[31merror:\x1b[0m Cannot verify iptables rules (need root, or rules missing after reboot).");
        eprintln!("  Fix: sudo ./docker/sandbox-network.sh setup");
    }

    if !network_ok {
        if unsafe_override {
            eprintln!("\x1b[33mwarning:\x1b[0m Proceeding without network isolation because --i-know-its-unsafe was provided.");
        } else {
            return Err(io::Error::other("Sandbox network/firewall not fully configured. Use --i-know-its-unsafe to run anyway."));
        }
    }

    if let Some(dir) = docker_dir {
        if !env_file_exists(dir) {
            eprintln!("\x1b[33mwarning:\x1b[0m No .env file found. API keys may not be set.");
            eprintln!("  Fix: cp docker/example.env docker/.env && edit docker/.env");
        }
    }

    let img = image_name();

    // Try compose first (has env_file, service definitions, etc.)
    let compose_file = docker_dir.map(|d| d.join("docker-compose.yml"));
    let use_compose = compose_file.as_ref().map(|f| f.exists()).unwrap_or(false);

    let mut cmd = Command::new("docker");

    if use_compose {
        cmd.args(["compose", "-f", &path_str(compose_file.as_ref().unwrap())]);

        // Check for local-api overlay
        let local_api_compose = docker_dir.unwrap().join("docker-compose.local-api.yml");
        if std::env::var("LOCAL_API_BASE").is_ok() && local_api_compose.exists() {
            cmd.args(["-f", &path_str(&local_api_compose)]);
        }

        cmd.args([
            "run",
            "--rm",
            "-e",
            &format!("SANDBOX_NAME={}", name),
            "-e",
            &format!("HOSTNAME={}", name),
        ]);

        // Inject any wizard-configured passthrough keys.
        apply_passthrough_env(&mut cmd);

        cmd.arg(agent);
    } else {
        // Direct docker run (no compose files available — e.g., installed via binary only)
        let container_name = format!("unleash-{}", name);
        cmd.args([
            "run",
            "--rm",
            "-it",
            "--runtime",
            "runsc",
            "--network",
            "unleash-sandbox",
            "--name",
            &container_name,
            "--hostname",
            name,
            "--dns",
            "8.8.8.8",
            "--dns",
            "8.8.4.4",
            "-e",
            &format!("SANDBOX_NAME={}", name),
            "-v",
            &format!(
                "{}:/workspace",
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            ),
            "-w",
            "/workspace",
        ]);

        // Wizard-configured passthrough keys (replaces the previous hard-coded list).
        apply_passthrough_env(&mut cmd);

        // Pass through .env file if present
        if let Some(dir) = docker_dir {
            let dotenv = dir.join(".env");
            if dotenv.exists() {
                cmd.args(["--env-file", &path_str(&dotenv)]);
            }
        }

        cmd.args([&img, agent]);
    }

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

/// Shared helper: run `<bash> <abs-script-path> <args...>` under sudo.
fn run_sudo_script(script: &Path, args: &[&str]) -> io::Result<()> {
    if !script.exists() {
        eprintln!(
            "\x1b[31merror:\x1b[0m sandbox-network.sh not found at {}",
            script.display()
        );
        return Err(io::Error::other("script not found"));
    }
    let bash = which::which("bash")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/bin/bash".to_string());
    let abs_script = path_str(script);
    let mut full_args: Vec<&str> = vec![abs_script.as_str()];
    full_args.extend_from_slice(args);
    match run_sudo(&bash, &full_args)? {
        SudoOutcome::Ok => Ok(()),
        SudoOutcome::SudoMissing => Err(StepFailure::SudoMissing.into()),
        SudoOutcome::AuthFailed => Err(StepFailure::SudoAuth.into()),
        SudoOutcome::NoTty => Err(StepFailure::NoTty.into()),
        SudoOutcome::CommandFailed(s) | SudoOutcome::Other(s) => Err(io::Error::other(s)),
    }
}

pub fn run_teardown(docker_dir: &Path) -> io::Result<()> {
    println!("\x1b[1m=== Sandbox Teardown ===\x1b[0m\n");
    println!("(may prompt for sudo password)");
    run_sudo_script(&sandbox_network_script(docker_dir), &["teardown"])?;
    println!("\n\x1b[32mTeardown complete.\x1b[0m");
    Ok(())
}

pub fn run_allow_ip(docker_dir: &Path, ip: &str) -> io::Result<()> {
    println!("(may prompt for sudo password)");
    run_sudo_script(&sandbox_network_script(docker_dir), &["allow-ip", ip])
}

pub fn run_revoke_ip(docker_dir: &Path, ip: &str) -> io::Result<()> {
    println!("(may prompt for sudo password)");
    run_sudo_script(&sandbox_network_script(docker_dir), &["revoke-ip", ip])
}

pub fn run_list() -> io::Result<()> {
    if !docker_running() {
        eprintln!("\x1b[31merror:\x1b[0m Docker is not running.");
        return Err(io::Error::other("Docker not running"));
    }

    // List containers on the sandbox network with useful columns
    let output = Command::new("docker")
        .args([
            "ps",
            "--filter",
            "network=unleash-sandbox",
            "--format",
            "table {{.ID}}\t{{.Names}}\t{{.Status}}\t{{.RunningFor}}\t{{.Command}}",
        ])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    if lines.len() <= 1 {
        println!("No running sandboxes.");
        println!("  Start one: unleash sandbox run claude");
        return Ok(());
    }

    println!("\x1b[1mRunning Sandboxes\x1b[0m\n");
    for line in &lines {
        println!("  {}", line);
    }
    println!("\n  Enter a sandbox: unleash sandbox enter <NAME>");
    Ok(())
}

pub fn run_enter(target: &str, shell: &str) -> io::Result<()> {
    // Validate target to prevent argument injection in docker exec
    validate_sandbox_name(target)?;

    if !docker_running() {
        eprintln!("\x1b[31merror:\x1b[0m Docker is not running.");
        return Err(io::Error::other("Docker not running"));
    }

    // Find the container — try target as container name or ID first
    let found = Command::new("docker")
        .args(["inspect", "--format", "{{.ID}}", target])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let container_id = if found {
        target.to_string()
    } else {
        // Search sandbox containers by hostname
        let output = Command::new("docker")
            .args(["ps", "-q", "--filter", "network=unleash-sandbox"])
            .output()?;

        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let container_ids: Vec<&str> = raw
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        // Check each container's hostname
        let mut match_id = None;
        for id in &container_ids {
            let hostname = run_command_output(
                "docker",
                &["inspect", "--format", "{{.Config.Hostname}}", id],
            )?;
            if hostname.trim() == target {
                match_id = Some(id.to_string());
                break;
            }
        }

        match_id.ok_or_else(|| {
            io::Error::other(format!(
                "No sandbox found matching '{}'. Run 'unleash sandbox list' to see running sandboxes.",
                target
            ))
        })?
    };

    // Exec into the container
    let status = Command::new("docker")
        .args(["exec", "-it", &container_id, "--", shell])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "Shell exited with code {}",
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

/// Main dispatch for `unleash sandbox <action>` (or bare `unleash sandbox`).
///
/// `None` action means "launch the interactive wizard" — the wizard lives in
/// the TUI (see `tui::app`). For the TUI-less build we fall back to the linear
/// CLI setup flow.
pub fn handle_sandbox(action: Option<&SandboxAction>) -> io::Result<()> {
    let docker_dir = find_docker_dir();

    // These work without docker dir
    match action {
        Some(SandboxAction::Status) => return run_status(docker_dir.as_deref()),
        Some(SandboxAction::List) => return run_list(),
        Some(SandboxAction::Enter { target, shell }) => return run_enter(target, shell),
        Some(SandboxAction::Run {
            name,
            agent,
            args,
            i_know_its_unsafe,
        }) => {
            return run_agent(
                docker_dir.as_deref(),
                agent,
                name,
                args.as_slice(),
                *i_know_its_unsafe,
            )
        }
        _ => {}
    }

    // For actions that need docker_dir: fall back to extracting embedded assets
    // so `unleash sandbox setup` works from anywhere with just the binary.
    let docker_dir = match docker_dir {
        Some(d) => d,
        None => find_or_install_docker_dir().map_err(|e| {
            io::Error::other(format!(
                "Could not locate or install docker/ assets. Tried:\n  \
                 - ~/.local/share/unleash/docker/\n  \
                 - <install-prefix>/share/unleash/docker/\n  \
                 - ./docker/ (and parents)\n\
                 Underlying error: {}",
                e
            ))
        })?,
    };

    match action {
        Some(SandboxAction::Setup) => run_setup(&docker_dir),
        Some(SandboxAction::Teardown) => run_teardown(&docker_dir),
        Some(SandboxAction::AllowIp { ip }) => run_allow_ip(&docker_dir, ip),
        Some(SandboxAction::RevokeIp { ip }) => run_revoke_ip(&docker_dir, ip),
        // Bare `unleash sandbox` → launch the TUI wizard. If TUI feature is off,
        // fall back to the CLI setup flow.
        None => {
            #[cfg(feature = "tui")]
            return crate::tui::run_sandbox_wizard();
            #[cfg(not(feature = "tui"))]
            return run_setup(&docker_dir);
        }
        Some(SandboxAction::Status)
        | Some(SandboxAction::List)
        | Some(SandboxAction::Enter { .. })
        | Some(SandboxAction::Run { .. }) => unreachable!(),
    }
}

/// Sandbox subcommand actions (parsed by clap in cli.rs)
#[derive(clap::Subcommand, Debug)]
pub enum SandboxAction {
    /// Set up the sandbox: install gVisor, create network, apply firewall rules, build image
    Setup,

    /// Show sandbox health status
    Status,

    /// List running sandbox containers
    List,

    /// Open a shell in a running sandbox container
    ///
    /// TARGET can be a container name, ID, or hostname.
    ///
    /// Examples:
    ///   unleash sandbox enter mybox
    ///   unleash sandbox enter abc123
    ///   unleash sandbox enter mybox --shell /bin/zsh
    Enter {
        /// Container name, ID, or hostname to enter
        target: String,

        /// Shell to use inside the container
        #[arg(long, default_value = "bash")]
        shell: String,
    },

    /// Remove sandbox network and firewall rules
    Teardown,

    /// Open a single LAN IP (optionally port-restricted) through the sandbox firewall
    ///
    /// Examples:
    ///   unleash sandbox allow-ip 192.168.1.100        # all ports
    ///   unleash sandbox allow-ip 192.168.1.100:8080   # port 8080 only (recommended)
    #[command(name = "allow-ip")]
    AllowIp {
        /// The private IP address to allow, optionally with port (e.g., 192.168.1.100:8080)
        ip: String,
    },

    /// Close a previously opened LAN IP
    #[command(name = "revoke-ip")]
    RevokeIp {
        /// The private IP address to revoke (must match the format used in allow-ip)
        ip: String,
    },

    /// Run an agent in the sandbox (e.g., `unleash sandbox run claude`)
    ///
    /// Without an agent argument, opens a bash shell.
    Run {
        /// Sandbox name (used as hostname; allows multiple sandboxes)
        #[arg(long, default_value = "sandbox")]
        name: String,

        /// Bypass the network isolation checks and run the sandbox even if LAN-blocking iptables rules are missing or unverified.
        #[arg(long)]
        i_know_its_unsafe: bool,

        /// Agent to run: claude, codex, gemini, opencode, pi, bash, unleash
        /// Defaults to bash if omitted.
        #[arg(default_value = "bash")]
        agent: String,

        /// Extra arguments passed to the agent
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_passthrough_keys_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("passthrough.toml");
        let keys = vec![
            "ANTHROPIC_API_KEY".to_string(),
            "OPENAI_API_KEY".to_string(),
        ];
        save_passthrough_keys(&path, &keys).unwrap();
        let loaded = load_passthrough_keys(&path);
        assert_eq!(loaded, keys);
    }

    #[test]
    fn test_load_passthrough_missing_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        assert_eq!(load_passthrough_keys(&path), Vec::<String>::new());
    }

    #[test]
    fn test_load_passthrough_malformed_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this isn't = valid toml @@@").unwrap();
        assert_eq!(load_passthrough_keys(&path), Vec::<String>::new());
    }

    #[test]
    fn test_apply_passthrough_env_no_config_is_noop() {
        // We can't easily mock passthrough_config_path() since it reads
        // dirs::config_dir(); but we can prove the function doesn't panic
        // when there's no key in env / no config.
        let mut cmd = Command::new("true");
        apply_passthrough_env(&mut cmd);
        // No assertion — we just want this to not panic.
    }

    #[test]
    fn test_canonical_keys_includes_known() {
        let dir = TempDir::new().unwrap();
        // No example.env — should still return the canonical set.
        let keys = canonical_keys_from_example(dir.path());
        assert!(keys.contains(&"ANTHROPIC_API_KEY".to_string()));
        assert!(keys.contains(&"OPENAI_API_KEY".to_string()));
        assert!(keys.contains(&"GEMINI_API_KEY".to_string()));
    }

    #[test]
    fn test_canonical_keys_includes_extras_from_example_env() {
        let dir = TempDir::new().unwrap();
        let example = dir.path().join("example.env");
        std::fs::write(&example, "WEIRD_CUSTOM_KEY=\nANTHROPIC_API_KEY=\n").unwrap();
        let keys = canonical_keys_from_example(dir.path());
        assert!(keys.contains(&"WEIRD_CUSTOM_KEY".to_string()));
        // Canonical entries should NOT be duplicated.
        let dupes = keys.iter().filter(|k| *k == "ANTHROPIC_API_KEY").count();
        assert_eq!(dupes, 1);
    }

    #[test]
    fn test_write_dotenv_skips_empty_values() {
        let dir = TempDir::new().unwrap();
        let entries = vec![
            ("KEY_A".to_string(), "valueA".to_string()),
            ("KEY_B".to_string(), "".to_string()),
            ("KEY_C".to_string(), "valueC".to_string()),
        ];
        write_dotenv(dir.path(), &entries).unwrap();
        let written = std::fs::read_to_string(dir.path().join(".env")).unwrap();
        assert!(written.contains("KEY_A=valueA"));
        assert!(!written.contains("KEY_B="));
        assert!(written.contains("KEY_C=valueC"));
    }

    #[test]
    fn test_step_failure_messages_are_nonempty() {
        for f in [
            StepFailure::DockerNotRunning,
            StepFailure::UnsupportedArch,
            StepFailure::SudoMissing,
            StepFailure::SudoAuth,
            StepFailure::NoTty,
            StepFailure::ScriptMissing(PathBuf::from("/nope")),
            StepFailure::PullFailedNoSource("img".into()),
            StepFailure::Recoverable("boom".into()),
        ] {
            assert!(!f.message().is_empty());
            assert!(!f.next_actions().is_empty());
        }
    }
}
