//! Version management for code agents (Claude Code, Codex, Gemini CLI, OpenCode)
//!
//! Handles detecting installed version, listing available versions,
//! and switching between versions for multiple agents.

use crate::json_output::{self, VersionListItem, VersionListOutput, VersionOutput};
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

/// Result of a checksum verification attempt.
#[derive(Debug)]
enum ChecksumResult {
    /// Checksum matched.
    Verified,
    /// Checksum did not match.
    Mismatch { expected: String, actual: String },
    /// Verification was skipped (no manifest, no tool, etc.).
    Skipped(String),
}

/// GCS bucket base URL for Claude Code native releases
const CLAUDE_GCS_BUCKET: &str = "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

/// Embedded version lists, compiled into the binary for instant display.
/// Updated periodically and committed to the repo.
pub fn get_versions_file_path() -> PathBuf {
    // 1. Check relative to the executable's directory (works regardless of CWD)
    if let Some(exe_local) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data/versions.json")))
    {
        if exe_local.exists() {
            return exe_local;
        }
    }

    // 2. Fallback to user's config directory
    if let Some(config_dir) = dirs::config_dir() {
        let unleashed_dir = config_dir.join("unleash");
        let _ = std::fs::create_dir_all(&unleashed_dir);
        return unleashed_dir.join("versions.json");
    }

    // 3. Fallback to temp if nothing else works
    std::env::temp_dir().join("unleash-versions.json")
}

/// Load embedded version lists from the dynamically read JSON.
/// Returns a map of agent key -> list of version strings (newest first).
pub fn load_embedded_versions() -> HashMap<String, Vec<String>> {
    let path = get_versions_file_path();
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
    let mut map = HashMap::new();
    for key in &["claude", "codex", "gemini", "opencode"] {
        if let Some(arr) = parsed.get(key).and_then(|v| v.as_array()) {
            let versions: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            map.insert(key.to_string(), versions);
        }
    }
    map
}

pub fn save_embedded_versions(map: &HashMap<crate::agents::AgentType, Vec<VersionInfo>>) {
    let mut out_map = serde_json::Map::new();

    for (agent_type, versions) in map {
        let key = match agent_type {
            crate::agents::AgentType::Claude => "claude",
            crate::agents::AgentType::Codex => "codex",
            crate::agents::AgentType::Gemini => "gemini",
            crate::agents::AgentType::OpenCode => "opencode",
        };
        let arr: Vec<serde_json::Value> = versions
            .iter()
            .map(|v| serde_json::Value::String(v.version.clone()))
            .collect();
        out_map.insert(key.to_string(), serde_json::Value::Array(arr));
    }

    let path = get_versions_file_path();
    if let Ok(json_str) = serde_json::to_string_pretty(&serde_json::Value::Object(out_map)) {
        let _ = std::fs::write(path, json_str);
    }
}

/// Information about an agent version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub is_installed: bool,
}

/// A single conflicting binary installation found on the system
#[derive(Debug, Clone)]
pub struct ConflictEntry {
    /// Filesystem path to the binary
    pub path: PathBuf,
    /// Version string reported by the binary (empty if detection failed)
    pub version: String,
    /// Human-readable install source (e.g. "native", "npm", "PATH")
    pub source: String,
    /// Whether this is the binary that would be invoked (first in PATH)
    pub active: bool,
}

/// Result of an installation attempt
#[derive(Debug, Clone)]
pub struct InstallResult {
    pub success: bool,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

/// Version manager for code agents
pub struct VersionManager;

impl VersionManager {
    pub fn new() -> Self {
        Self
    }

    // ── Claude Code ──────────────────────────────────────────────

    /// Get the currently installed Claude Code version
    pub fn get_installed_version(&self) -> Option<String> {
        let output = Command::new("claude").arg("--version").output().ok()?;

        if output.status.success() {
            let version_str = String::from_utf8_lossy(&output.stdout);
            // Parse "2.1.5 (Claude Code)" -> "2.1.5"
            let version = version_str
                .lines()
                .next()?
                .trim()
                .replace(" (Claude Code)", "");
            Some(version)
        } else {
            None
        }
    }

    /// Check if there are conflicting installations (e.g. native + npm for Claude Code)
    #[allow(dead_code)]
    pub fn has_conflicts(&self, binary_name: &str) -> bool {
        self.detect_conflicts(binary_name).len() > 1
    }

    /// Detect all conflicting installations and return structured details.
    ///
    /// Returns a list of [`ConflictEntry`] describing each distinct installation
    /// found on the system. When the list has more than one entry, the
    /// installations are in conflict. The first entry is marked `active = true`
    /// (it is the one that would win in PATH).
    pub fn detect_conflicts(&self, binary_name: &str) -> Vec<ConflictEntry> {
        if binary_name == "claude" {
            return self.detect_claude_conflicts();
        }
        // For non-Claude agents, multiple PATH entries are normal (symlinks,
        // package managers, system packages). Don't flag as conflicts.
        Vec::new()
    }

    /// Internal: detect conflicting Claude Code installations.
    fn detect_claude_conflicts(&self) -> Vec<ConflictEntry> {
        let mut entries: Vec<ConflictEntry> = Vec::new();

        // Determine the first-in-PATH binary so we can mark it active
        let active_path: Option<PathBuf> = which::which("claude")
            .ok()
            .and_then(|p| p.canonicalize().ok().or(Some(p)));

        // Check native installation
        let native_dir = dirs::home_dir().map(|h| h.join(".local/share/claude/versions"));
        if let Some(ref dir) = native_dir {
            if dir.exists() && dir.read_dir().is_ok_and(|mut d| d.next().is_some()) {
                // Find the actual binary path for native
                let bin_path = dirs::home_dir()
                    .map(|h| h.join(".local/bin/claude"))
                    .unwrap_or_else(|| PathBuf::from("/usr/local/bin/claude"));
                let version = Self::version_at_path(&bin_path);
                let canonical = bin_path.canonicalize().ok().unwrap_or_else(|| bin_path.clone());
                let is_active = active_path.as_ref().is_some_and(|a| *a == canonical);
                entries.push(ConflictEntry {
                    path: bin_path,
                    version,
                    source: "native".to_string(),
                    active: is_active,
                });
            }
        }

        // Check NPM global installation
        if Self::has_npm() {
            if let Ok(out) = Command::new("npm")
                .args(["list", "-g", "@anthropic-ai/claude-code"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if out.status.success()
                    && !stdout.contains("empty")
                    && stdout.contains("@anthropic-ai/claude-code")
                {
                    // Locate the npm global binary
                    let npm_bin = Self::npm_global_bin("claude");
                    let version = npm_bin
                        .as_ref()
                        .map(|p| Self::version_at_path(p))
                        .unwrap_or_default();
                    let path = npm_bin.unwrap_or_else(|| PathBuf::from("npm:@anthropic-ai/claude-code"));
                    let canonical = path.canonicalize().ok().unwrap_or_else(|| path.clone());
                    let is_active = active_path.as_ref().is_some_and(|a| *a == canonical);
                    entries.push(ConflictEntry {
                        path,
                        version,
                        source: "npm".to_string(),
                        active: is_active,
                    });
                }
            }
        }

        // If no entry was marked active but we have entries, mark the first one
        if !entries.is_empty() && !entries.iter().any(|e| e.active) {
            entries[0].active = true;
        }

        entries
    }

    /// Get the version string from a specific binary path.
    fn version_at_path(path: &std::path::Path) -> String {
        use std::time::Duration;

        // Spawn with timeout to avoid hanging on broken binaries
        let mut child = match Command::new(path)
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return String::new(),
        };

        // Wait with 5 second timeout
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if start.elapsed() > Duration::from_secs(5) {
                        let _ = child.kill();
                        return String::new();
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return String::new(),
            }
        }

        match child.wait_with_output() {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout).to_string();
                s.lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .replace(" (Claude Code)", "")
            }
            _ => String::new(),
        }
    }

    /// Locate the npm global binary for a given command name.
    fn npm_global_bin(name: &str) -> Option<PathBuf> {
        let out = Command::new("npm").args(["bin", "-g"]).output().ok()?;
        if out.status.success() {
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let p = PathBuf::from(dir).join(name);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Silently remove npm-installed Claude Code if present.
    /// Called after a successful native install to prevent conflicts.
    fn remove_npm_claude_if_present() {
        if !Self::has_npm() {
            return;
        }
        // Check if npm package is installed
        if let Ok(out) = Command::new("npm")
            .args(["list", "-g", "@anthropic-ai/claude-code"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if out.status.success()
                && !stdout.contains("empty")
                && stdout.contains("@anthropic-ai/claude-code")
            {
                eprintln!("  Removing conflicting npm installation...");
                match Self::npm_global_command()
                    .args(["uninstall", "-g", "@anthropic-ai/claude-code"])
                    .output()
                {
                    Ok(o) if o.status.success() => {
                        eprintln!("  \x1b[32m+\x1b[0m npm package removed");
                    }
                    Ok(o) => {
                        eprintln!(
                            "  \x1b[31mx\x1b[0m npm uninstall failed: {}",
                            String::from_utf8_lossy(&o.stderr).trim()
                        );
                    }
                    Err(e) => {
                        eprintln!("  \x1b[31mx\x1b[0m npm uninstall failed: {}", e);
                    }
                }
            }
        }
    }

    /// Cleanup conflicting installations
    pub fn cleanup_conflicts(&self, binary_name: &str) -> io::Result<()> {
        if binary_name == "claude" {
            // Keep native, uninstall npm
            if Self::has_npm() {
                let _ = Self::npm_global_command()
                    .args(["uninstall", "-g", "@anthropic-ai/claude-code"])
                    .output();
            }
        } else if binary_name == "opencode" {
            // Keep ~/.opencode/bin/opencode (native installer), remove npm global
            if Self::has_npm() {
                let _ = Self::npm_global_command()
                    .args(["uninstall", "-g", "opencode-ai"])
                    .output();
            }
            // Remove /usr/bin/opencode if it's a stale copy
            if let Ok(paths) = which::which_all("opencode") {
                let opencode_home = dirs::home_dir().map(|h| h.join(".opencode/bin/opencode"));
                for path in paths {
                    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                    // Skip the native install path
                    if opencode_home.as_ref().is_some_and(|h| {
                        h.canonicalize().unwrap_or_else(|_| h.clone()) == canonical
                    }) {
                        continue;
                    }
                    // Try to remove other copies (may fail for /usr/bin without sudo, that's ok)
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
        Ok(())
    }

    /// Get the latest Claude Code version from GCS
    pub fn get_latest_gcs_version() -> Option<String> {
        let output = Command::new("curl")
            .args(["-fsSL", &format!("{}/latest", CLAUDE_GCS_BUCKET)])
            .output()
            .ok()?;
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                Some(version)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Detect the current platform for GCS downloads
    fn detect_platform() -> String {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;

        let gcs_arch = match arch {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            _ => "x64",
        };

        let gcs_os = match os {
            "linux" => "linux",
            "macos" => "darwin",
            _ => "linux",
        };

        // Check for musl on Linux
        if gcs_os == "linux"
            && (std::path::Path::new("/lib/libc.musl-x86_64.so.1").exists()
                || std::path::Path::new("/lib/libc.musl-aarch64.so.1").exists())
        {
            return format!("{}-{}-musl", gcs_os, gcs_arch);
        }

        format!("{}-{}", gcs_os, gcs_arch)
    }

    /// Check if npm is available
    pub fn has_npm() -> bool {
        Command::new("npm")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Query the npm registry HTTP API for available versions of a package.
    /// Uses curl — no npm binary required.
    fn query_npm_registry_versions(package: &str, limit: usize) -> io::Result<Vec<String>> {
        // npm registry URL: https://registry.npmjs.org/<package>
        // The response has a "versions" object with version strings as keys.
        // We use the abbreviated metadata endpoint for speed.
        let url = format!("https://registry.npmjs.org/{}", package);
        let output = Command::new("curl")
            .args(["-fsSL", "-H", "Accept: application/vnd.npm.install-v1+json", &url])
            .output()
            .map_err(|e| io::Error::other(format!("curl not found: {}", e)))?;

        if !output.status.success() {
            return Err(io::Error::other(format!(
                "Failed to query npm registry for {}",
                package
            )));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        // Parse the "versions" object and extract keys
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| io::Error::other(format!("Failed to parse registry response: {}", e)))?;

        let mut versions: Vec<String> = parsed
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        versions.sort_by(|a, b| version_compare(b, a));
        versions.truncate(limit);
        Ok(versions)
    }

    /// Check whether `npm install -g` needs `sudo` on this system.
    ///
    /// Returns `true` when the npm global prefix directory (e.g. `/usr/lib`)
    /// is not owned by the current user, which is the default on Arch Linux.
    /// The result is cached for the lifetime of the process since the npm
    /// prefix won't change mid-run.
    pub fn npm_global_needs_sudo() -> bool {
        use std::sync::OnceLock;
        static NEEDS_SUDO: OnceLock<bool> = OnceLock::new();
        *NEEDS_SUDO.get_or_init(|| {
            let prefix = Command::new("npm")
                .args(["config", "get", "prefix"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            match prefix {
                Some(p) => {
                    use std::os::unix::fs::MetadataExt;
                    let path = std::path::Path::new(&p);
                    let uid = nix::unistd::getuid().as_raw();
                    path.metadata()
                        .map(|m| m.uid() != uid)
                        .unwrap_or(false)
                }
                None => false,
            }
        })
    }

    /// Create a `Command` for npm global operations, prepending `sudo -n`
    /// (non-interactive) if the prefix is root-owned. Using `-n` avoids
    /// silent hangs when called from background threads where no TTY is
    /// available for a password prompt.
    pub fn npm_global_command() -> Command {
        if Self::npm_global_needs_sudo() {
            let mut cmd = Command::new("sudo");
            cmd.args(["-n", "npm"]);
            cmd
        } else {
            Command::new("npm")
        }
    }

    /// Extract SHA256 checksum from manifest JSON for a given platform
    fn extract_checksum_from_manifest(manifest: &str, platform: &str) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(manifest).ok()?;
        json.get(platform)?
            .get("checksum")?
            .as_str()
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .map(|s| s.to_string())
    }

    /// Verify SHA-256 checksum of a downloaded file against the manifest.
    fn verify_checksum_for_file(
        file_path: &std::path::Path,
        manifest_url: &str,
        platform: &str,
    ) -> ChecksumResult {
        let manifest_output = match Command::new("curl").args(["-fsSL", manifest_url]).output() {
            Ok(o) if o.status.success() => o,
            _ => return ChecksumResult::Skipped("manifest not available".into()),
        };

        let manifest = String::from_utf8_lossy(&manifest_output.stdout);
        let expected = match Self::extract_checksum_from_manifest(&manifest, platform) {
            Some(e) => e,
            None => return ChecksumResult::Skipped("no checksum in manifest".into()),
        };

        let checksum_cmd = if cfg!(target_os = "macos") {
            "shasum"
        } else {
            "sha256sum"
        };
        let mut cmd = Command::new(checksum_cmd);
        if cfg!(target_os = "macos") {
            cmd.args(["-a", "256"]);
        }
        cmd.arg(file_path.to_str().unwrap_or(""));

        match cmd.output() {
            Ok(o) if o.status.success() => {
                let actual = String::from_utf8_lossy(&o.stdout);
                let actual_checksum = actual.split_whitespace().next().unwrap_or("").to_string();
                if actual_checksum == expected {
                    ChecksumResult::Verified
                } else {
                    ChecksumResult::Mismatch {
                        expected,
                        actual: actual_checksum,
                    }
                }
            }
            _ => ChecksumResult::Skipped("sha256sum failed".into()),
        }
    }

    /// Get available Claude Code versions from GCS + npm registry
    pub fn get_available_versions(&self) -> io::Result<Vec<String>> {
        let mut seen = std::collections::HashSet::new();
        let mut versions = Vec::new();

        // Try GCS first: get latest version
        if let Some(latest) = Self::get_latest_gcs_version() {
            if seen.insert(latest.clone()) {
                versions.push(latest);
            }
        }

        // Query npm registry for additional versions
        if Self::has_npm() {
            if let Ok(output) = Command::new("npm")
                .args(["view", "@anthropic-ai/claude-code", "versions", "--json"])
                .output()
            {
                if output.status.success() {
                    let json_str = String::from_utf8_lossy(&output.stdout);
                    let npm_versions: Vec<String> =
                        serde_json::from_str(json_str.trim()).unwrap_or_default();

                    for v in npm_versions.into_iter().rev().take(20) {
                        if seen.insert(v.clone()) {
                            versions.push(v);
                        }
                    }
                }
            }
        }

        if versions.is_empty() {
            return Err(io::Error::other(
                "Failed to query available versions from GCS and npm",
            ));
        }

        // Sort newest first and take top 20
        versions.sort_by(|a, b| version_compare(b, a));
        versions.truncate(20);
        Ok(versions)
    }

    /// Get combined Claude Code version list with status
    pub fn get_version_list(&self) -> Vec<VersionInfo> {
        let installed = self.get_installed_version();
        let available = self.get_available_versions().unwrap_or_default();

        let mut versions: Vec<VersionInfo> = available
            .into_iter()
            .map(|v| VersionInfo {
                is_installed: installed.as_ref() == Some(&v),
                version: v,
            })
            .collect();

        // Sort by version (newest first)
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        versions
    }

    /// Install a specific version of Claude Code
    /// Tries native binary from GCS first, falls back to npm
    pub fn install_version(&self, version: &str) -> io::Result<InstallResult> {
        // Try native (GCS) first
        let native_result = self.install_version_native(version)?;
        if native_result.success {
            // Clean up npm installation if present to avoid conflicts
            Self::remove_npm_claude_if_present();
            return Ok(native_result);
        }

        // Fallback: try npm
        if Self::has_npm() {
            let output = Command::new("npm")
                .args([
                    "install",
                    "-g",
                    "--force",
                    &format!("@anthropic-ai/claude-code@{}", version),
                ])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                // After install, update symlink to npm-installed cli.js
                if let Ok(npm_output) = Command::new("npm").args(["root", "-g"]).output() {
                    if npm_output.status.success() {
                        let npm_root = String::from_utf8_lossy(&npm_output.stdout)
                            .trim()
                            .to_string();
                        let cli_js =
                            PathBuf::from(&npm_root).join("@anthropic-ai/claude-code/cli.js");
                        if cli_js.exists() {
                            if let Some(home) = dirs::home_dir() {
                                let bin_claude = home.join(".local/bin/claude");
                                let _ = std::fs::remove_file(&bin_claude);
                                #[cfg(unix)]
                                std::os::unix::fs::symlink(&cli_js, &bin_claude).ok();
                            }
                        }
                    }
                }

                return Ok(InstallResult {
                    success: true,
                    stdout,
                    stderr,
                    error: None,
                });
            }
        }

        // Both methods failed - return the native error
        Ok(native_result)
    }

    /// Install Claude Code using the native installer (GCS binary download)
    pub fn install_version_native(&self, version: &str) -> io::Result<InstallResult> {
        let platform = Self::detect_platform();
        let download_url = format!("{}/{}/{}/claude", CLAUDE_GCS_BUCKET, version, platform);
        let manifest_url = format!("{}/{}/manifest.json", CLAUDE_GCS_BUCKET, version);

        // Create version directory
        let version_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/share/claude/versions");
        std::fs::create_dir_all(&version_dir)?;

        let binary_path = version_dir.join(version);
        let temp_path = version_dir.join(format!("{}.tmp", version));

        // Download binary
        let download = Command::new("curl")
            .args([
                "-fsSL",
                "-o",
                temp_path.to_str().unwrap_or("/tmp/claude-download"),
                &download_url,
            ])
            .output()?;

        if !download.status.success() {
            let _ = std::fs::remove_file(&temp_path);
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: String::from_utf8_lossy(&download.stderr).to_string(),
                error: Some(format!(
                    "Failed to download Claude Code {} from GCS",
                    version
                )),
            });
        }

        // Download manifest for checksum verification
        let checksum_status = Self::verify_checksum_for_file(&temp_path, &manifest_url, &platform);
        match checksum_status {
            ChecksumResult::Verified => {
                eprintln!("  \x1b[32m+\x1b[0m Checksum verified (SHA-256)");
            }
            ChecksumResult::Mismatch { expected, actual } => {
                let _ = std::fs::remove_file(&temp_path);
                eprintln!("  \x1b[31mx\x1b[0m Checksum FAILED: expected {}, got {}", expected, actual);
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Checksum mismatch: expected {}, got {}", expected, actual),
                    error: Some("Checksum verification failed".to_string()),
                });
            }
            ChecksumResult::Skipped(reason) => {
                eprintln!("  \x1b[33m-\x1b[0m Checksum skipped ({})", reason);
            }
        }

        // Make executable and move into place
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&temp_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&temp_path, perms)?;
        }

        std::fs::rename(&temp_path, &binary_path)?;

        // Update ~/.local/bin/claude symlink to point to the new binary
        if let Some(home) = dirs::home_dir() {
            let bin_dir = home.join(".local/bin");
            std::fs::create_dir_all(&bin_dir)?;
            let bin_claude = bin_dir.join("claude");
            let _ = std::fs::remove_file(&bin_claude);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&binary_path, &bin_claude).ok();
        }

        Ok(InstallResult {
            success: true,
            stdout: format!(
                "Claude Code v{} installed natively to {}",
                version,
                binary_path.display()
            ),
            stderr: String::new(),
            error: None,
        })
    }

    // ── Codex ────────────────────────────────────────────────────

    /// Get available Codex versions from GitHub releases (tags matching rust-v*)
    pub fn get_codex_available_versions(&self) -> io::Result<Vec<String>> {
        let output = Command::new("gh")
            .args([
                "api",
                "repos/openai/codex/tags",
                "--paginate",
                "--jq",
                ".[].name",
            ])
            .output()?;

        if output.status.success() {
            let tag_output = String::from_utf8_lossy(&output.stdout);
            let mut versions: Vec<String> = tag_output
                .lines()
                .filter(|line| line.starts_with("rust-v"))
                .filter(|line| !line.contains("alpha"))
                .map(|line| line.trim_start_matches("rust-v").to_string())
                .filter(|v| !v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit()))
                .collect();
            // Sort newest first, then take top 20
            versions.sort_by(|a, b| version_compare(b, a));
            versions.truncate(20);
            Ok(versions)
        } else {
            Err(io::Error::other(
                "Failed to query GitHub releases for Codex",
            ))
        }
    }

    /// Get combined Codex version list with status
    pub fn get_codex_version_list(&self, installed: Option<&str>) -> Vec<VersionInfo> {
        let available = self.get_codex_available_versions().unwrap_or_default();

        let mut versions = Vec::new();

        for v in &available {
            versions.push(VersionInfo {
                version: v.clone(),
                is_installed: installed == Some(v.as_str()),
            });
        }

        // Sort by version (newest first)
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        versions
    }

    /// Install a specific Codex version by downloading prebuilt binaries from GitHub releases
    #[allow(dead_code)]
    pub fn install_codex_version(&self, version: &str) -> io::Result<InstallResult> {
        let tag = format!("rust-v{}", version);
        let asset_name = Self::codex_asset_name();

        let install_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/bin");
        fs::create_dir_all(&install_dir)?;

        // Download to a temp directory
        let tmp_dir = std::env::temp_dir().join(format!("codex-install-{}", version));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir)?;

        // Download the main codex binary tarball
        let download = Command::new("gh")
            .args([
                "release",
                "download",
                &tag,
                "--repo",
                "openai/codex",
                "--pattern",
                &format!("{}.tar.gz", asset_name),
                "--dir",
                tmp_dir.to_str().unwrap_or("/tmp"),
            ])
            .output()?;

        if !download.status.success() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false,
                stdout: String::from_utf8_lossy(&download.stdout).to_string(),
                stderr: String::from_utf8_lossy(&download.stderr).to_string(),
                error: Some(format!(
                    "Failed to download {} from release {}",
                    asset_name, tag
                )),
            });
        }

        // Extract the tarball
        let extract = Command::new("tar")
            .args(["xzf", &format!("{}.tar.gz", asset_name)])
            .current_dir(&tmp_dir)
            .output()?;

        if !extract.status.success() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false,
                stdout: String::from_utf8_lossy(&extract.stdout).to_string(),
                stderr: String::from_utf8_lossy(&extract.stderr).to_string(),
                error: Some("Failed to extract tarball".to_string()),
            });
        }

        // Install the binary
        let extracted_binary = tmp_dir.join(&asset_name);
        let install_path = install_dir.join("codex");

        if !extracted_binary.exists() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: format!("Expected binary {} not found in archive", asset_name),
                error: Some(format!("Binary {} not found after extraction", asset_name)),
            });
        }

        fs::copy(&extracted_binary, &install_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&install_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&install_path, perms)?;
        }

        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(InstallResult {
            success: true,
            stdout: format!("Codex v{} installed to {}", version, install_path.display()),
            stderr: String::new(),
            error: None,
        })
    }

    // ── Gemini CLI ────────────────────────────────────────────

    /// Get available Gemini CLI versions from npm registry
    pub fn get_gemini_available_versions(&self) -> io::Result<Vec<String>> {
        Self::query_npm_registry_versions("@google/gemini-cli", 20)
    }

    /// Get combined Gemini CLI version list with status
    pub fn get_gemini_version_list(&self, installed: Option<&str>) -> Vec<VersionInfo> {
        let available = self.get_gemini_available_versions().unwrap_or_default();

        let mut versions: Vec<VersionInfo> = available
            .into_iter()
            .map(|v| VersionInfo {
                is_installed: installed == Some(v.as_str()),
                version: v,
            })
            .collect();

        versions.sort_by(|a, b| version_compare(&b.version, &a.version));
        versions
    }

    /// Install a specific Gemini CLI version via npm
    #[allow(dead_code)]
    pub fn install_gemini_version(&self, version: &str) -> io::Result<InstallResult> {
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "npm is not available".to_string(),
                error: Some("npm is required to install Gemini CLI".to_string()),
            });
        }

        let output = Self::npm_global_command()
            .args([
                "install",
                "-g",
                "--force",
                &format!("@google/gemini-cli@{}", version),
            ])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(InstallResult {
            success: output.status.success(),
            stdout,
            stderr: stderr.clone(),
            error: if output.status.success() {
                None
            } else {
                Some(format!(
                    "Failed to install Gemini CLI v{}: {}",
                    version, stderr
                ))
            },
        })
    }

    // ── OpenCode ────────────────────────────────────────────

    /// Get available OpenCode versions from npm registry.
    /// OpenCode is distributed via npm (`opencode-ai` package). GitHub releases
    /// for `opencode-ai/opencode` use a different versioning scheme (0.0.x) and
    /// should not be mixed with npm versions (1.x.x).
    pub fn get_opencode_available_versions(&self) -> io::Result<Vec<String>> {
        let mut versions = Self::query_npm_registry_versions("opencode-ai", 20)?;
        versions.retain(|s| s.starts_with(|c: char| c.is_ascii_digit()));

        if versions.is_empty() {
            return Err(io::Error::other(
                "Failed to query available versions for OpenCode",
            ));
        }
        Ok(versions)
    }

    /// Get combined OpenCode version list with status
    pub fn get_opencode_version_list(&self, installed: Option<&str>) -> Vec<VersionInfo> {
        let available = self.get_opencode_available_versions().unwrap_or_default();

        let mut versions: Vec<VersionInfo> = available
            .into_iter()
            .map(|v| VersionInfo {
                is_installed: installed == Some(v.as_str()),
                version: v,
            })
            .collect();

        versions.sort_by(|a, b| version_compare(&b.version, &a.version));
        versions
    }

    /// Install a specific OpenCode version.
    /// Uses `opencode upgrade <version>` if opencode is already installed (updates in-place),
    /// otherwise falls back to npm install.
    #[allow(dead_code)]
    pub fn install_opencode_version(&self, version: &str) -> io::Result<InstallResult> {
        // Prefer `opencode upgrade` if already installed (updates the actual binary in-place)
        if which::which("opencode").is_ok() {
            let output = Command::new("opencode")
                .args(["upgrade", version])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            return Ok(InstallResult {
                success: output.status.success(),
                stdout,
                stderr: stderr.clone(),
                error: if output.status.success() {
                    None
                } else {
                    Some(format!("Failed to upgrade OpenCode to v{}: {}", version, stderr))
                },
            });
        }

        // Fresh install via npm
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "Neither opencode nor npm is available".to_string(),
                error: Some("Install opencode first: curl -fsSL https://opencode.ai/install | bash".to_string()),
            });
        }

        let output = Self::npm_global_command()
            .args(["install", "-g", &format!("opencode-ai@{}", version)])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(InstallResult {
            success: output.status.success(),
            stdout,
            stderr: stderr.clone(),
            error: if output.status.success() {
                None
            } else {
                Some(format!("Failed to install OpenCode v{}: {}", version, stderr))
            },
        })
    }

    // ── Streaming install methods ──────────────────────────────

    /// Read stdout/stderr from a child process, sending each line via `log_tx`.
    /// Reads stdout in a spawned thread and stderr in the calling thread to avoid
    /// pipe buffer deadlock. Returns accumulated (stdout, stderr) strings.
    fn stream_child_output(
        child: &mut std::process::Child,
        log_tx: &mpsc::Sender<String>,
    ) -> (String, String) {
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let tx_clone = log_tx.clone();

        let stdout_thread = thread::spawn(move || {
            let mut acc = String::new();
            if let Some(pipe) = stdout_pipe {
                for line in io::BufReader::new(pipe).lines().map_while(Result::ok) {
                    let _ = tx_clone.send(line.clone());
                    acc.push_str(&line);
                    acc.push('\n');
                }
            }
            acc
        });

        let mut stderr_acc = String::new();
        if let Some(pipe) = stderr_pipe {
            for line in io::BufReader::new(pipe).lines().map_while(Result::ok) {
                let _ = log_tx.send(line.clone());
                stderr_acc.push_str(&line);
                stderr_acc.push('\n');
            }
        }

        let stdout_acc = stdout_thread.join().unwrap_or_default();
        (stdout_acc, stderr_acc)
    }

    /// Run a command with streaming output, returning (success, stdout, stderr)
    fn run_streaming(
        cmd: &mut Command,
        log_tx: &mpsc::Sender<String>,
    ) -> io::Result<(bool, String, String)> {
        let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let (stdout, stderr) = Self::stream_child_output(&mut child, log_tx);
        let status = child.wait()?;
        Ok((status.success(), stdout, stderr))
    }

    /// Install Claude Code with streaming log output
    pub fn install_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        // Try native (GCS) first
        let _ = log_tx.send(format!(
            "Attempting native install of Claude Code v{}...",
            version
        ));
        let native_result = self.install_version_native_streaming(version, &log_tx)?;
        if native_result.success {
            // Clean up npm installation if present to avoid conflicts
            Self::remove_npm_claude_if_present();
            return Ok(native_result);
        }

        // Fallback: try npm
        if Self::has_npm() {
            let _ = log_tx.send("Native install failed, trying npm fallback...".to_string());
            let _ = log_tx.send(format!(
                "Running: npm install -g @anthropic-ai/claude-code@{}",
                version
            ));

            let (ok, stdout, stderr) = Self::run_streaming(
                Command::new("npm").args([
                    "install",
                    "-g",
                    "--force",
                    &format!("@anthropic-ai/claude-code@{}", version),
                ]),
                &log_tx,
            )?;

            if ok {
                let _ = log_tx.send("Updating symlink...".to_string());
                if let Ok(npm_output) = Command::new("npm").args(["root", "-g"]).output() {
                    if npm_output.status.success() {
                        let npm_root = String::from_utf8_lossy(&npm_output.stdout)
                            .trim()
                            .to_string();
                        let cli_js =
                            PathBuf::from(&npm_root).join("@anthropic-ai/claude-code/cli.js");
                        if cli_js.exists() {
                            if let Some(home) = dirs::home_dir() {
                                let bin_claude = home.join(".local/bin/claude");
                                let _ = fs::remove_file(&bin_claude);
                                #[cfg(unix)]
                                std::os::unix::fs::symlink(&cli_js, &bin_claude).ok();
                            }
                        }
                    }
                }
                return Ok(InstallResult {
                    success: true,
                    stdout,
                    stderr,
                    error: None,
                });
            }
        }

        Ok(native_result)
    }

    /// Native (GCS) install with streaming log output
    fn install_version_native_streaming(
        &self,
        version: &str,
        log_tx: &mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        let platform = Self::detect_platform();
        let download_url = format!("{}/{}/{}/claude", CLAUDE_GCS_BUCKET, version, platform);
        let manifest_url = format!("{}/{}/manifest.json", CLAUDE_GCS_BUCKET, version);

        let version_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/share/claude/versions");
        fs::create_dir_all(&version_dir)?;

        let binary_path = version_dir.join(version);
        let temp_path = version_dir.join(format!("{}.tmp", version));

        // Download binary
        let _ = log_tx.send(format!("Downloading Claude Code v{} from GCS...", version));
        let (ok, _stdout, stderr) = Self::run_streaming(
            Command::new("curl").args([
                "-fSL",
                "-o",
                temp_path.to_str().unwrap_or("/tmp/claude-download"),
                &download_url,
            ]),
            log_tx,
        )?;

        if !ok {
            let _ = fs::remove_file(&temp_path);
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr,
                error: Some(format!(
                    "Failed to download Claude Code {} from GCS",
                    version
                )),
            });
        }

        // Verify checksum
        let _ = log_tx.send("Verifying checksum...".to_string());
        let checksum_status = Self::verify_checksum_for_file(&temp_path, &manifest_url, &platform);
        match checksum_status {
            ChecksumResult::Verified => {
                let _ = log_tx.send("\x1b[32m+\x1b[0m Checksum verified (SHA-256)".to_string());
            }
            ChecksumResult::Mismatch { expected, actual } => {
                let _ = fs::remove_file(&temp_path);
                let _ = log_tx.send(format!(
                    "\x1b[31mx\x1b[0m Checksum FAILED: expected {}, got {}",
                    expected, actual
                ));
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Checksum mismatch: expected {}, got {}", expected, actual),
                    error: Some("Checksum verification failed".to_string()),
                });
            }
            ChecksumResult::Skipped(reason) => {
                let _ = log_tx.send(format!("\x1b[33m-\x1b[0m Checksum skipped ({})", reason));
            }
        }

        // Make executable and move into place
        let _ = log_tx.send("Setting executable permissions...".to_string());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&temp_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&temp_path, perms)?;
        }

        fs::rename(&temp_path, &binary_path)?;

        let _ = log_tx.send("Updating symlink...".to_string());
        if let Some(home) = dirs::home_dir() {
            let bin_dir = home.join(".local/bin");
            fs::create_dir_all(&bin_dir)?;
            let bin_claude = bin_dir.join("claude");
            let _ = fs::remove_file(&bin_claude);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&binary_path, &bin_claude).ok();
        }

        Ok(InstallResult {
            success: true,
            stdout: format!(
                "Claude Code v{} installed natively to {}",
                version,
                binary_path.display()
            ),
            stderr: String::new(),
            error: None,
        })
    }

    /// Install Codex with streaming log output
    pub fn install_codex_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        let tag = format!("rust-v{}", version);
        let asset_name = Self::codex_asset_name();

        let install_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/bin");
        fs::create_dir_all(&install_dir)?;

        let tmp_dir = std::env::temp_dir().join(format!("codex-install-{}", version));
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir)?;

        // Download
        let _ = log_tx.send(format!(
            "Downloading Codex {} from GitHub release {}...",
            asset_name, tag
        ));
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("gh").args([
                "release",
                "download",
                &tag,
                "--repo",
                "openai/codex",
                "--pattern",
                &format!("{}.tar.gz", asset_name),
                "--dir",
                tmp_dir.to_str().unwrap_or("/tmp"),
            ]),
            &log_tx,
        )?;

        if !ok {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false,
                stdout,
                stderr,
                error: Some(format!(
                    "Failed to download {} from release {}",
                    asset_name, tag
                )),
            });
        }

        // Extract
        let _ = log_tx.send("Extracting tarball...".to_string());
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("tar")
                .args(["xzf", &format!("{}.tar.gz", asset_name)])
                .current_dir(&tmp_dir),
            &log_tx,
        )?;

        if !ok {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false,
                stdout,
                stderr,
                error: Some("Failed to extract tarball".to_string()),
            });
        }

        // Install binary
        let extracted_binary = tmp_dir.join(&asset_name);
        let install_path = install_dir.join("codex");

        if !extracted_binary.exists() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: format!("Expected binary {} not found in archive", asset_name),
                error: Some(format!("Binary {} not found after extraction", asset_name)),
            });
        }

        let _ = log_tx.send(format!(
            "Installing binary to {}...",
            install_path.display()
        ));
        fs::copy(&extracted_binary, &install_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&install_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&install_path, perms)?;
        }

        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(InstallResult {
            success: true,
            stdout: format!("Codex v{} installed to {}", version, install_path.display()),
            stderr: String::new(),
            error: None,
        })
    }

    /// Install Gemini CLI with streaming log output
    pub fn install_gemini_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "npm is not available".to_string(),
                error: Some("npm is required to install Gemini CLI".to_string()),
            });
        }

        let use_sudo = Self::npm_global_needs_sudo();
        let _ = log_tx.send(format!(
            "Running: {}npm install -g @google/gemini-cli@{}",
            if use_sudo { "sudo " } else { "" },
            version
        ));
        let (ok, stdout, stderr) = Self::run_streaming(
            Self::npm_global_command().args([
                "install",
                "-g",
                "--force",
                &format!("@google/gemini-cli@{}", version),
            ]),
            &log_tx,
        )?;

        Ok(InstallResult {
            success: ok,
            stdout,
            stderr: stderr.clone(),
            error: if ok {
                None
            } else {
                Some(format!(
                    "Failed to install Gemini CLI v{}: {}",
                    version, stderr
                ))
            },
        })
    }

    /// Install OpenCode with streaming log output.
    /// Uses `opencode upgrade <version>` if already installed, npm otherwise.
    pub fn install_opencode_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        // Prefer `opencode upgrade` if already installed (updates the actual binary in-place)
        if which::which("opencode").is_ok() {
            let _ = log_tx.send(format!("Running: opencode upgrade {}", version));
            let (ok, stdout, stderr) = Self::run_streaming(
                Command::new("opencode").args(["upgrade", version]),
                &log_tx,
            )?;

            return Ok(InstallResult {
                success: ok,
                stdout,
                stderr: stderr.clone(),
                error: if ok {
                    None
                } else {
                    Some(format!("Failed to upgrade OpenCode to v{}: {}", version, stderr))
                },
            });
        }

        // Fresh install via npm
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "Neither opencode nor npm is available".to_string(),
                error: Some("Install opencode first: curl -fsSL https://opencode.ai/install | bash".to_string()),
            });
        }

        let use_sudo = Self::npm_global_needs_sudo();
        let _ = log_tx.send(format!(
            "Running: {}npm install -g opencode-ai@{}",
            if use_sudo { "sudo " } else { "" },
            version
        ));
        let (ok, stdout, stderr) = Self::run_streaming(
            Self::npm_global_command().args([
                "install",
                "-g",
                &format!("opencode-ai@{}", version),
            ]),
            &log_tx,
        )?;

        Ok(InstallResult {
            success: ok,
            stdout,
            stderr: stderr.clone(),
            error: if ok {
                None
            } else {
                Some(format!("Failed to install OpenCode v{}: {}", version, stderr))
            },
        })
    }

    /// Determine the correct GitHub release asset name for this platform
    fn codex_asset_name() -> String {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;

        let target_arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => "x86_64",
        };

        let target_triple = match os {
            "linux" => format!("{}-unknown-linux-gnu", target_arch),
            "macos" => format!("{}-apple-darwin", target_arch),
            _ => format!("{}-unknown-linux-gnu", target_arch),
        };

        format!("codex-{}", target_triple)
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self
    }
}

/// Canonically compare two version strings (semver-like).
///
/// - Strips known prefixes ("v", "rust-v") from both inputs.
/// - Pre-release versions (with `-` suffix) are less than the same base version.
/// - Splits on `.` and compares each segment as `u32`.
/// - Zero-pads shorter versions so "1.2" == "1.2.0".
pub(crate) fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    fn strip_prefix(s: &str) -> &str {
        s.trim_start_matches("rust-v").trim_start_matches('v')
    }

    /// Split a version string into (base numeric parts, optional pre-release suffix).
    fn parse_parts(s: &str) -> (Vec<u32>, Option<&str>) {
        let pre = s.split_once('-').map(|(_, rest)| rest);
        let base = s.split('-').next().unwrap_or(s);
        let parts = base
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect();
        (parts, pre)
    }

    let a_stripped = strip_prefix(a);
    let b_stripped = strip_prefix(b);
    let (a_parts, a_pre) = parse_parts(a_stripped);
    let (b_parts, b_pre) = parse_parts(b_stripped);

    // Compare base numeric parts
    for i in 0..a_parts.len().max(b_parts.len()) {
        let pa = a_parts.get(i).copied().unwrap_or(0);
        let pb = b_parts.get(i).copied().unwrap_or(0);
        match pa.cmp(&pb) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    // Same base version: pre-release < release (per semver)
    match (a_pre, b_pre) {
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(a_suffix), Some(b_suffix)) => compare_prerelease(a_suffix, b_suffix),
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Compare pre-release suffixes per SemVer 11.4:
/// split on `.`, numeric segments compare as integers, otherwise lexicographic.
fn compare_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    for i in 0..a_parts.len().max(b_parts.len()) {
        let ap = a_parts.get(i);
        let bp = b_parts.get(i);
        match (ap, bp) {
            (None, Some(_)) => return std::cmp::Ordering::Less,  // fewer fields = lower precedence
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(a_seg), Some(b_seg)) => {
                let ord = match (a_seg.parse::<u64>(), b_seg.parse::<u64>()) {
                    (Ok(an), Ok(bn)) => an.cmp(&bn),
                    (Ok(_), Err(_)) => std::cmp::Ordering::Less,   // numeric < alphanumeric
                    (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                    (Err(_), Err(_)) => a_seg.cmp(b_seg),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (None, None) => break,
        }
    }
    std::cmp::Ordering::Equal
}

/// Convenience wrapper: returns `true` if version `a` is strictly less than `b`.
pub(crate) fn version_less_than(a: &str, b: &str) -> bool {
    version_compare(a, b) == std::cmp::Ordering::Less
}

// CLI commands for version management

/// List available versions
pub fn list_versions(json: bool) -> io::Result<()> {
    let vm = VersionManager::new();
    let versions = vm.get_version_list();
    let current = vm.get_installed_version();

    if json {
        let output = VersionListOutput {
            currently_installed: current,
            versions: versions
                .into_iter()
                .map(|info| VersionListItem {
                    version: info.version,
                    is_installed: info.is_installed,
                })
                .collect(),
        };
        json_output::print_json(&output);
    } else {
        println!("Claude Code Versions:");
        println!();

        for info in versions {
            let installed = if info.is_installed {
                " [installed]"
            } else {
                ""
            };
            println!("  v{}{}", info.version, installed);
        }

        if let Some(v) = current {
            println!();
            println!("Currently installed: v{}", v);
        }
    }

    Ok(())
}

/// Install a specific version
pub fn install_version(version: &str, json: bool) -> io::Result<()> {
    let vm = VersionManager::new();

    if !json {
        println!("Installing Claude Code v{}...", version);
    }

    let install_result = vm.install_version(version)?;
    if !install_result.success {
        if !json {
            eprintln!(
                "Install failed: {}",
                install_result.error.unwrap_or_default()
            );
            if !install_result.stderr.is_empty() {
                eprintln!("{}", install_result.stderr);
            }
        }
        return Err(io::Error::other(format!(
            "Failed to install Claude Code {}",
            version
        )));
    }

    if !json {
        println!("Done!");
    } else {
        json_output::print_success_json(&format!(
            "Successfully installed Claude Code v{}",
            version
        ));
    }

    Ok(())
}

/// Show current version
#[allow(dead_code)]
pub fn show_current() -> io::Result<()> {
    let vm = VersionManager::new();
    match vm.get_installed_version() {
        Some(v) => println!("Claude Code version: {}", v),
        None => println!("Claude Code is not installed"),
    }
    Ok(())
}

/// Show current version as JSON
pub fn show_current_json() {
    let cu_version = env!("CARGO_PKG_VERSION");
    let vm = VersionManager::new();
    let claude_code_version = vm
        .get_installed_version()
        .unwrap_or_else(|| "not installed".to_string());
    let is_installed = vm.get_installed_version().is_some();

    let output = VersionOutput {
        unleash_version: cu_version.to_string(),
        claude_code_version,
        claude_code_installed: is_installed,
    };

    json_output::print_json(&output);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tempfile::TempDir;

    #[test]
    fn test_version_compare() {
        use std::cmp::Ordering;

        // Basic comparisons
        assert_eq!(version_compare("2.1.5", "2.1.4"), Ordering::Greater);
        assert_eq!(version_compare("2.1.5", "2.1.5"), Ordering::Equal);
        assert_eq!(version_compare("2.0.0", "2.1.0"), Ordering::Less);
        assert_eq!(version_compare("2.10.0", "2.9.0"), Ordering::Greater);

        // Equal versions
        assert_eq!(version_compare("1.2.3", "1.2.3"), Ordering::Equal);

        // Zero-padding (the old bug: "1.2" vs "1.2.0")
        assert_eq!(version_compare("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(version_compare("1.2", "1.2.1"), Ordering::Less);
        assert_eq!(version_compare("1.2.1", "1.2"), Ordering::Greater);

        // Less / Greater
        assert_eq!(version_compare("1.2.3", "1.2.4"), Ordering::Less);
        assert_eq!(version_compare("2.0.0", "1.9.9"), Ordering::Greater);

        // Prefix stripping
        assert_eq!(version_compare("v1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(version_compare("rust-v0.116.0", "0.116.0"), Ordering::Equal);

        // Pre-release is less than release (per semver)
        assert_eq!(version_compare("1.2.3-beta", "1.2.3"), Ordering::Less);
        assert_eq!(version_compare("1.2.3-beta.1", "1.2.3"), Ordering::Less);
        assert_eq!(version_compare("1.2.3", "1.2.3-beta"), Ordering::Greater);
        // Pre-release suffixes are compared lexicographically
        assert_eq!(version_compare("1.2.3-alpha", "1.2.3-beta"), Ordering::Less);
        assert_eq!(version_compare("1.2.3-beta", "1.2.3-alpha"), Ordering::Greater);
        // Gemini-style preview versions sort correctly
        assert_eq!(version_compare("0.36.0-preview.0", "0.36.0-preview.2"), Ordering::Less);
        assert_eq!(version_compare("0.36.0-preview.5", "0.36.0-preview.6"), Ordering::Less);
        assert_eq!(version_compare("0.36.0-nightly.20260318", "0.36.0-nightly.20260325"), Ordering::Less);
        // nightly < preview (lexicographic)
        assert_eq!(version_compare("0.36.0-nightly.1", "0.36.0-preview.0"), Ordering::Less);
        // Multi-digit numeric segments (SemVer 11.4 — numeric comparison, not lexicographic)
        assert_eq!(version_compare("0.36.0-preview.2", "0.36.0-preview.10"), Ordering::Less);
        assert_eq!(version_compare("0.36.0-preview.10", "0.36.0-preview.2"), Ordering::Greater);
        assert_eq!(version_compare("0.36.0-preview.15", "0.36.0-preview.15"), Ordering::Equal);

        // Single component
        assert_eq!(version_compare("2", "1"), Ordering::Greater);
        assert_eq!(version_compare("1", "2"), Ordering::Less);

        // Large numbers
        assert_eq!(version_compare("0.116.0", "0.115.9"), Ordering::Greater);
        assert_eq!(version_compare("9.4.0", "9.3.0"), Ordering::Greater);
    }

    #[test]
    fn test_version_less_than() {
        assert!(version_less_than("1.2.3", "1.2.4"));
        assert!(!version_less_than("1.2.4", "1.2.3"));
        assert!(!version_less_than("1.2.3", "1.2.3"));
        assert!(version_less_than("v1.0.0", "2.0.0"));
        // Pre-release is less than stable
        assert!(version_less_than("1.2.3-beta", "1.2.3"));
        assert!(!version_less_than("1.2.3", "1.2.3-beta"));
    }

    #[test]
    fn test_version_manager_creation() {
        let _vm = VersionManager::new();
        // Should not panic
    }

    /// Create a mock "claude" binary that sleeps before outputting version.
    /// Returns the temp directory (must be kept alive) and the path to add to PATH.
    fn create_mock_claude(sleep_ms: u32) -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let mock_path = temp.path().to_path_buf();
        let mock_claude = mock_path.join("claude");

        // Create a shell script that sleeps then outputs version
        let script = format!(
            "#!/bin/bash\nsleep {}\necho \"2.1.5 (Claude Code)\"\n",
            sleep_ms as f64 / 1000.0
        );
        std::fs::write(&mock_claude, script).unwrap();

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&mock_claude).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&mock_claude, perms).unwrap();
        }

        (temp, mock_path)
    }

    /// Test that demonstrates the performance problem with calling get_installed_version
    /// on every frame vs using a cached value.
    #[test]
    fn test_cached_version_performance() {
        // Create mock claude that takes 50ms to respond
        let (_temp, mock_path) = create_mock_claude(50);

        // Prepend mock to PATH
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", mock_path.display(), original_path);

        // SAFETY: This test runs single-threaded and restores PATH before returning
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        let vm = VersionManager::new();
        const ITERATIONS: u32 = 10;

        // Measure time for subprocess calls (old behavior - calling on every frame)
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let _ = vm.get_installed_version();
        }
        let subprocess_time = start.elapsed();

        // Measure time for cached value access (new behavior)
        let cached_version = vm.get_installed_version(); // Cache once
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let _ = cached_version.clone();
        }
        let cached_time = start.elapsed();

        // Restore PATH
        // SAFETY: Restoring original PATH value
        unsafe {
            std::env::set_var("PATH", original_path);
        }

        // Verify a version was returned (don't check specific version as it may vary)
        assert!(cached_version.is_some(), "Should have a cached version");

        // Assert subprocess calls are slow (should be ~500ms for 10 x 50ms)
        assert!(
            subprocess_time.as_millis() > 100,
            "Subprocess calls should take >100ms, took {}ms",
            subprocess_time.as_millis()
        );

        // Assert cached access is fast (should be <1ms)
        assert!(
            cached_time.as_millis() < 10,
            "Cached access should take <10ms, took {}ms",
            cached_time.as_millis()
        );

        // Assert cached is at least 100x faster
        let speedup = subprocess_time.as_nanos() / cached_time.as_nanos().max(1);
        assert!(
            speedup > 100,
            "Cached should be >100x faster, was only {}x",
            speedup
        );

        println!(
            "Performance test results:\n  Subprocess ({} calls): {:?}\n  Cached ({} accesses): {:?}\n  Speedup: {}x",
            ITERATIONS, subprocess_time, ITERATIONS, cached_time, speedup
        );
    }

    /// Create a mock "npm" binary that captures arguments to a file,
    /// and a mock "curl" that always fails (so native install falls through to npm).
    fn create_mock_npm() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        let mock_path = temp.path().to_path_buf();
        let mock_npm = mock_path.join("npm");
        let args_file = mock_path.join("npm_args.txt");

        let script = format!(
            "#!/bin/bash\necho \"$@\" >> \"{}\"\nexit 0\n",
            args_file.display()
        );
        std::fs::write(&mock_npm, script).unwrap();

        // Mock curl to always fail so native GCS install falls through to npm
        let mock_curl = mock_path.join("curl");
        std::fs::write(&mock_curl, "#!/bin/bash\nexit 1\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for bin in [&mock_npm, &mock_curl] {
                let mut perms = std::fs::metadata(bin).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(bin, perms).unwrap();
            }
        }

        (temp, mock_path, args_file)
    }

    #[test]
    fn test_install_version_uses_force_flag() {
        let (_temp, mock_path, args_file) = create_mock_npm();

        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", mock_path.display(), original_path);

        // SAFETY: This test runs single-threaded and restores PATH before returning
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        let vm = VersionManager::new();
        let result = vm.install_version("2.1.4");

        // SAFETY: Restoring original PATH value
        unsafe {
            std::env::set_var("PATH", original_path);
        }

        assert!(result.is_ok(), "install_version should not return an error");
        let install_result = result.unwrap();
        assert!(
            install_result.success,
            "install should succeed with mock npm"
        );

        let captured_args = std::fs::read_to_string(&args_file)
            .expect("Should be able to read captured npm arguments");

        assert!(
            captured_args.contains("--force"),
            "npm install should include --force flag for downgrades. Got: {}",
            captured_args.trim()
        );
        assert!(
            captured_args.contains("install"),
            "Should contain 'install' command. Got: {}",
            captured_args.trim()
        );
        assert!(
            captured_args.contains("-g"),
            "Should contain '-g' for global install. Got: {}",
            captured_args.trim()
        );
        assert!(
            captured_args.contains("@anthropic-ai/claude-code@2.1.4"),
            "Should contain package@version. Got: {}",
            captured_args.trim()
        );
    }

    /// Network-dependent benchmark: measures version fetch latency for all agents
    #[test]
    #[ignore]
    fn bench_parallel_vs_sequential_version_fetch() {
        use std::sync::mpsc as bench_mpsc;

        let vm = VersionManager::new();

        // Sequential fetch
        let start = Instant::now();
        let _ = vm.get_available_versions();
        let _ = vm.get_codex_available_versions();
        let _ = vm.get_gemini_available_versions();
        let _ = vm.get_opencode_available_versions();
        let sequential_time = start.elapsed();

        // Parallel fetch
        let start = Instant::now();
        let (tx, rx) = bench_mpsc::channel::<()>();
        let tx1 = tx.clone();
        let tx2 = tx.clone();
        let tx3 = tx.clone();
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_available_versions();
            let _ = tx.send(());
        });
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_codex_available_versions();
            let _ = tx1.send(());
        });
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_gemini_available_versions();
            let _ = tx2.send(());
        });
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_opencode_available_versions();
            let _ = tx3.send(());
        });
        for _ in 0..4 {
            let _ = rx.recv();
        }
        let parallel_time = start.elapsed();

        println!(
            "Sequential fetch (4 agents): {:?}\nParallel fetch (4 agents): {:?}\nSpeedup: {:.1}x",
            sequential_time,
            parallel_time,
            sequential_time.as_secs_f64() / parallel_time.as_secs_f64().max(0.001)
        );
    }
}
