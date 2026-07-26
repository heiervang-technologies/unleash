use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;

use super::types::{ChecksumResult, ConflictEntry, InstallResult, VersionInfo, CLAUDE_GCS_BUCKET};
use super::VersionManager;
use super::{version_compare, load_embedded_versions};

impl VersionManager {
    // ── Claude Code version management ──────────────

    pub fn get_installed_version(&self) -> Option<String> {
        let output = self.command("claude").arg("--version").output().ok()?;

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
                let canonical = bin_path
                    .canonicalize()
                    .ok()
                    .unwrap_or_else(|| bin_path.clone());
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
                    let path =
                        npm_bin.unwrap_or_else(|| PathBuf::from("npm:@anthropic-ai/claude-code"));
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

    /// Check if npm is available (static — uses the inherited PATH).
    /// Equivalent to `VersionManager::default().has_npm_for_self()`.

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
            // Fallback to embedded versions
            let embedded = load_embedded_versions();
            if let Some(v_list) = embedded.get("claude") {
                if !v_list.is_empty() {
                    return Ok(v_list.clone());
                }
            }
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
        // Skip native download in test mode to prevent overwriting real installations
        if self.should_skip_native_download() {
            return self.install_version_npm_only(version);
        }

        // Try native (GCS) first
        let native_result = self.install_version_native(version)?;
        if native_result.success {
            // Clean up npm installation if present to avoid conflicts
            Self::remove_npm_claude_if_present();
            return Ok(native_result);
        }

        // Fallback: try npm
        if Self::has_npm() {
            let output = Self::npm_global_command()
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

    /// Install via npm only (skips native binary download).
    /// Used by tests to avoid overwriting real installations.
    fn install_version_npm_only(&self, version: &str) -> io::Result<InstallResult> {
        if self.has_npm_for_self() {
            let output = self
                .npm_global_command_for_self()
                .args([
                    "install",
                    "-g",
                    "--force",
                    &format!("@anthropic-ai/claude-code@{}", version),
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
                    Some(stderr)
                },
            })
        } else {
            Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "npm not available".into(),
                error: Some(
                    "npm not available (native install skipped by UNLEASH_SKIP_NATIVE_INSTALL)"
                        .into(),
                ),
            })
        }
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
                eprintln!(
                    "  \x1b[31mx\x1b[0m Checksum FAILED: expected {}, got {}",
                    expected, actual
                );
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Checksum mismatch: expected {}, got {}", expected, actual),
                    error: Some("Checksum verification failed".to_string()),
                });
            }
            ChecksumResult::Failed(reason) => {
                let _ = std::fs::remove_file(&temp_path);
                eprintln!("  \x1b[31mx\x1b[0m Checksum FAILED: {}", reason);
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Checksum failure: {}", reason),
                    error: Some("Checksum verification failed".to_string()),
                });
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

    /// Get available unleash versions from GitHub releases (heiervang-technologies/unleash).
    /// Returns a `VersionInfo` list with the current binary version marked as installed.

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
            let use_sudo = Self::npm_global_needs_sudo();
            let _ = log_tx.send(format!(
                "Running: {}npm install -g @anthropic-ai/claude-code@{}",
                if use_sudo { "sudo " } else { "" },
                version
            ));

            let (ok, stdout, stderr) = Self::run_streaming(
                Self::npm_global_command().args([
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
            ChecksumResult::Failed(reason) => {
                let _ = fs::remove_file(&temp_path);
                let _ = log_tx.send(format!("\x1b[31mx\x1b[0m Checksum FAILED: {}", reason));
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!("Checksum failure: {}", reason),
                    error: Some("Checksum verification failed".to_string()),
                });
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

}
