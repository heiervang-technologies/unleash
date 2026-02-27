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

/// GCS bucket base URL for Claude Code native releases
const CLAUDE_GCS_BUCKET: &str = "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

/// Embedded version lists, compiled into the binary for instant display.
/// Updated periodically and committed to the repo.
const EMBEDDED_VERSIONS_JSON: &str = include_str!("embedded_versions.json");

/// Load embedded version lists from the compiled-in JSON.
/// Returns a map of agent key -> list of version strings (newest first).
pub fn load_embedded_versions() -> HashMap<String, Vec<String>> {
    let parsed: serde_json::Value =
        serde_json::from_str(EMBEDDED_VERSIONS_JSON).unwrap_or_default();
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

/// Information about an agent version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub is_installed: bool,
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
        let output = Command::new("claude")
            .arg("--version")
            .output()
            .ok()?;

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
        if gcs_os == "linux" {
            if std::path::Path::new("/lib/libc.musl-x86_64.so.1").exists()
                || std::path::Path::new("/lib/libc.musl-aarch64.so.1").exists()
            {
                return format!("{}-{}-musl", gcs_os, gcs_arch);
            }
        }

        format!("{}-{}", gcs_os, gcs_arch)
    }

    /// Check if npm is available
    fn has_npm() -> bool {
        Command::new("npm")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Extract SHA256 checksum from manifest JSON for a given platform
    fn extract_checksum_from_manifest(manifest: &str, platform: &str) -> Option<String> {
        // Simple parsing without serde_json
        let platform_key = format!("\"{}\"", platform);
        if let Some(platform_pos) = manifest.find(&platform_key) {
            let rest = &manifest[platform_pos..];
            if let Some(checksum_pos) = rest.find("\"checksum\"") {
                let after_key = &rest[checksum_pos + 10..];
                if let Some(start) = after_key.find('"') {
                    let value_start = start + 1;
                    if let Some(end) = after_key[value_start..].find('"') {
                        let checksum = &after_key[value_start..value_start + end];
                        if checksum.len() == 64
                            && checksum.chars().all(|c| c.is_ascii_hexdigit())
                        {
                            return Some(checksum.to_string());
                        }
                    }
                }
            }
        }
        None
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
                    let npm_versions: Vec<String> = json_str
                        .trim()
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

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
            return Ok(native_result);
        }

        // Fallback: try npm
        if Self::has_npm() {
            let output = Command::new("npm")
                .args(["install", "-g", "--force", &format!("@anthropic-ai/claude-code@{}", version)])
                .output()?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if output.status.success() {
                // After install, update symlink to npm-installed cli.js
                if let Ok(npm_output) = Command::new("npm").args(["root", "-g"]).output() {
                    if npm_output.status.success() {
                        let npm_root = String::from_utf8_lossy(&npm_output.stdout).trim().to_string();
                        let cli_js = PathBuf::from(&npm_root).join("@anthropic-ai/claude-code/cli.js");
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
            .args(["-fsSL", "-o", temp_path.to_str().unwrap_or("/tmp/claude-download"), &download_url])
            .output()?;

        if !download.status.success() {
            let _ = std::fs::remove_file(&temp_path);
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: String::from_utf8_lossy(&download.stderr).to_string(),
                error: Some(format!("Failed to download Claude Code {} from GCS", version)),
            });
        }

        // Download manifest for checksum verification
        if let Ok(manifest_output) = Command::new("curl")
            .args(["-fsSL", &manifest_url])
            .output()
        {
            if manifest_output.status.success() {
                let manifest = String::from_utf8_lossy(&manifest_output.stdout);
                if let Some(expected) = Self::extract_checksum_from_manifest(&manifest, &platform) {
                    // Verify checksum
                    let checksum_cmd = if cfg!(target_os = "macos") { "shasum" } else { "sha256sum" };
                    let mut cmd = Command::new(checksum_cmd);
                    if cfg!(target_os = "macos") {
                        cmd.args(["-a", "256"]);
                    }
                    cmd.arg(temp_path.to_str().unwrap_or(""));

                    if let Ok(checksum_output) = cmd.output() {
                        if checksum_output.status.success() {
                            let actual = String::from_utf8_lossy(&checksum_output.stdout);
                            let actual_checksum = actual.split_whitespace().next().unwrap_or("");
                            if actual_checksum != expected {
                                let _ = std::fs::remove_file(&temp_path);
                                return Ok(InstallResult {
                                    success: false,
                                    stdout: String::new(),
                                    stderr: format!("Checksum mismatch: expected {}, got {}", expected, actual_checksum),
                                    error: Some("Checksum verification failed".to_string()),
                                });
                            }
                        }
                    }
                }
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
            stdout: format!("Claude Code v{} installed natively to {}", version, binary_path.display()),
            stderr: String::new(),
            error: None,
        })
    }

    // ── Codex ────────────────────────────────────────────────────

    /// Get available Codex versions from GitHub releases (tags matching rust-v*)
    pub fn get_codex_available_versions(&self) -> io::Result<Vec<String>> {
        let output = Command::new("gh")
            .args([
                "api", "repos/openai/codex/tags",
                "--paginate",
                "--jq", ".[].name",
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
                "release", "download", &tag,
                "--repo", "openai/codex",
                "--pattern", &format!("{}.tar.gz", asset_name),
                "--dir", tmp_dir.to_str().unwrap_or("/tmp"),
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
        if !Self::has_npm() {
            return Err(io::Error::other("npm is not available"));
        }

        let output = Command::new("npm")
            .args(["view", "@google/gemini-cli", "versions", "--json"])
            .output()?;

        if output.status.success() {
            let json_str = String::from_utf8_lossy(&output.stdout);
            let mut versions: Vec<String> = json_str
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();

            versions.sort_by(|a, b| version_compare(b, a));
            versions.truncate(20);
            Ok(versions)
        } else {
            Err(io::Error::other(
                "Failed to query npm registry for Gemini CLI",
            ))
        }
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

        let output = Command::new("npm")
            .args(["install", "-g", "--force", &format!("@google/gemini-cli@{}", version)])
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
                Some(format!("Failed to install Gemini CLI v{}: {}", version, stderr))
            },
        })
    }

    // ── OpenCode ────────────────────────────────────────────

    /// Get available OpenCode versions from GitHub releases + npm
    pub fn get_opencode_available_versions(&self) -> io::Result<Vec<String>> {
        let mut seen = std::collections::HashSet::new();
        let mut versions = Vec::new();

        // Try GitHub releases first
        if let Ok(output) = Command::new("gh")
            .args([
                "api", "repos/opencode-ai/opencode/releases",
                "--jq", ".[].tag_name",
            ])
            .output()
        {
            if output.status.success() {
                let tag_output = String::from_utf8_lossy(&output.stdout);
                for line in tag_output.lines() {
                    let v = line.trim().trim_start_matches('v').to_string();
                    if !v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit()) && seen.insert(v.clone()) {
                        versions.push(v);
                    }
                }
            }
        }

        // Also query npm for additional versions
        if Self::has_npm() {
            if let Ok(output) = Command::new("npm")
                .args(["view", "opencode-ai", "versions", "--json"])
                .output()
            {
                if output.status.success() {
                    let json_str = String::from_utf8_lossy(&output.stdout);
                    let npm_versions: Vec<String> = json_str
                        .trim()
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

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
                "Failed to query available versions for OpenCode",
            ));
        }

        versions.sort_by(|a, b| version_compare(b, a));
        versions.truncate(20);
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

    /// Install a specific OpenCode version via npm
    #[allow(dead_code)]
    pub fn install_opencode_version(&self, version: &str) -> io::Result<InstallResult> {
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "npm is not available".to_string(),
                error: Some("npm is required to install OpenCode".to_string()),
            });
        }

        let output = Command::new("npm")
            .args(["install", "-g", "--force", &format!("opencode-ai@{}", version)])
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
                for line in io::BufReader::new(pipe).lines().flatten() {
                    let _ = tx_clone.send(line.clone());
                    acc.push_str(&line);
                    acc.push('\n');
                }
            }
            acc
        });

        let mut stderr_acc = String::new();
        if let Some(pipe) = stderr_pipe {
            for line in io::BufReader::new(pipe).lines().flatten() {
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
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
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
        let _ = log_tx.send(format!("Attempting native install of Claude Code v{}...", version));
        let native_result = self.install_version_native_streaming(version, &log_tx)?;
        if native_result.success {
            return Ok(native_result);
        }

        // Fallback: try npm
        if Self::has_npm() {
            let _ = log_tx.send(format!("Native install failed, trying npm fallback..."));
            let _ = log_tx.send(format!("Running: npm install -g @anthropic-ai/claude-code@{}", version));

            let (ok, stdout, stderr) = Self::run_streaming(
                Command::new("npm").args(["install", "-g", "--force", &format!("@anthropic-ai/claude-code@{}", version)]),
                &log_tx,
            )?;

            if ok {
                let _ = log_tx.send("Updating symlink...".to_string());
                if let Ok(npm_output) = Command::new("npm").args(["root", "-g"]).output() {
                    if npm_output.status.success() {
                        let npm_root = String::from_utf8_lossy(&npm_output.stdout).trim().to_string();
                        let cli_js = PathBuf::from(&npm_root).join("@anthropic-ai/claude-code/cli.js");
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
                return Ok(InstallResult { success: true, stdout, stderr, error: None });
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
            Command::new("curl").args(["-fSL", "-o", temp_path.to_str().unwrap_or("/tmp/claude-download"), &download_url]),
            log_tx,
        )?;

        if !ok {
            let _ = fs::remove_file(&temp_path);
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr,
                error: Some(format!("Failed to download Claude Code {} from GCS", version)),
            });
        }

        // Verify checksum
        let _ = log_tx.send("Downloading manifest for checksum verification...".to_string());
        if let Ok(manifest_output) = Command::new("curl").args(["-fsSL", &manifest_url]).output() {
            if manifest_output.status.success() {
                let manifest = String::from_utf8_lossy(&manifest_output.stdout);
                if let Some(expected) = Self::extract_checksum_from_manifest(&manifest, &platform) {
                    let _ = log_tx.send("Verifying checksum...".to_string());
                    let checksum_cmd = if cfg!(target_os = "macos") { "shasum" } else { "sha256sum" };
                    let mut cmd = Command::new(checksum_cmd);
                    if cfg!(target_os = "macos") {
                        cmd.args(["-a", "256"]);
                    }
                    cmd.arg(temp_path.to_str().unwrap_or(""));

                    if let Ok(checksum_output) = cmd.output() {
                        if checksum_output.status.success() {
                            let actual = String::from_utf8_lossy(&checksum_output.stdout);
                            let actual_checksum = actual.split_whitespace().next().unwrap_or("");
                            if actual_checksum != expected {
                                let _ = fs::remove_file(&temp_path);
                                let _ = log_tx.send(format!("Checksum mismatch: expected {}, got {}", expected, actual_checksum));
                                return Ok(InstallResult {
                                    success: false,
                                    stdout: String::new(),
                                    stderr: format!("Checksum mismatch: expected {}, got {}", expected, actual_checksum),
                                    error: Some("Checksum verification failed".to_string()),
                                });
                            }
                            let _ = log_tx.send("Checksum verified.".to_string());
                        }
                    }
                }
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
            stdout: format!("Claude Code v{} installed natively to {}", version, binary_path.display()),
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
        let _ = log_tx.send(format!("Downloading Codex {} from GitHub release {}...", asset_name, tag));
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("gh").args([
                "release", "download", &tag,
                "--repo", "openai/codex",
                "--pattern", &format!("{}.tar.gz", asset_name),
                "--dir", tmp_dir.to_str().unwrap_or("/tmp"),
            ]),
            &log_tx,
        )?;

        if !ok {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false, stdout, stderr,
                error: Some(format!("Failed to download {} from release {}", asset_name, tag)),
            });
        }

        // Extract
        let _ = log_tx.send("Extracting tarball...".to_string());
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("tar").args(["xzf", &format!("{}.tar.gz", asset_name)]).current_dir(&tmp_dir),
            &log_tx,
        )?;

        if !ok {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Ok(InstallResult {
                success: false, stdout, stderr,
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

        let _ = log_tx.send(format!("Installing binary to {}...", install_path.display()));
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

        let _ = log_tx.send(format!("Running: npm install -g @google/gemini-cli@{}", version));
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("npm").args(["install", "-g", "--force", &format!("@google/gemini-cli@{}", version)]),
            &log_tx,
        )?;

        Ok(InstallResult {
            success: ok,
            stdout,
            stderr: stderr.clone(),
            error: if ok { None } else { Some(format!("Failed to install Gemini CLI v{}: {}", version, stderr)) },
        })
    }

    /// Install OpenCode with streaming log output
    pub fn install_opencode_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "npm is not available".to_string(),
                error: Some("npm is required to install OpenCode".to_string()),
            });
        }

        let _ = log_tx.send(format!("Running: npm install -g opencode-ai@{}", version));
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("npm").args(["install", "-g", "--force", &format!("opencode-ai@{}", version)]),
            &log_tx,
        )?;

        Ok(InstallResult {
            success: ok,
            stdout,
            stderr: stderr.clone(),
            error: if ok { None } else { Some(format!("Failed to install OpenCode v{}: {}", version, stderr)) },
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

/// Compare version strings (semver-like)
pub(crate) fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u32> {
        s.split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };

    let a_parts = parse(a);
    let b_parts = parse(b);

    for (a_part, b_part) in a_parts.iter().zip(b_parts.iter()) {
        match a_part.cmp(b_part) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }

    a_parts.len().cmp(&b_parts.len())
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
            let installed = if info.is_installed { " [installed]" } else { "" };
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
            eprintln!("Install failed: {}", install_result.error.unwrap_or_default());
            if !install_result.stderr.is_empty() {
                eprintln!("{}", install_result.stderr);
            }
        }
        return Err(io::Error::other(
            format!("Failed to install Claude Code {}", version),
        ));
    }

    if !json {
        println!("Done!");
    } else {
        json_output::print_success_json(&format!("Successfully installed Claude Code v{}", version));
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
    let claude_code_version = vm.get_installed_version().unwrap_or_else(|| "not installed".to_string());
    let is_installed = vm.get_installed_version().is_some();

    let output = VersionOutput {
        agent_unleashed_version: cu_version.to_string(),
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
        assert_eq!(version_compare("2.1.5", "2.1.4"), std::cmp::Ordering::Greater);
        assert_eq!(version_compare("2.1.5", "2.1.5"), std::cmp::Ordering::Equal);
        assert_eq!(version_compare("2.0.0", "2.1.0"), std::cmp::Ordering::Less);
        assert_eq!(version_compare("2.10.0", "2.9.0"), std::cmp::Ordering::Greater);
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
        assert!(install_result.success, "install should succeed with mock npm");

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
        for _ in 0..4 { let _ = rx.recv(); }
        let parallel_time = start.elapsed();

        println!(
            "Sequential fetch (4 agents): {:?}\nParallel fetch (4 agents): {:?}\nSpeedup: {:.1}x",
            sequential_time,
            parallel_time,
            sequential_time.as_secs_f64() / parallel_time.as_secs_f64().max(0.001)
        );
    }
}
