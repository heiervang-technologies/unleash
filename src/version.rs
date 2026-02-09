//! Version management for code agents (Claude Code, Codex)
//!
//! Handles detecting installed version, listing available versions,
//! and switching between versions for multiple agents.
//!
//! Supports two filtering modes per agent:
//! - **Whitelist mode** (default): Only whitelisted versions are allowed
//! - **Blacklist mode**: All versions except blacklisted ones are allowed

use crate::agents::AgentType;
use crate::json_output::{self, VersionListItem, VersionListOutput, VersionOutput};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// GCS bucket base URL for Claude Code native releases
const CLAUDE_GCS_BUCKET: &str = "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

// Include the generated version lists from Cargo.toml
include!(concat!(env!("OUT_DIR"), "/version_lists.rs"));

/// Version filter mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionFilterMode {
    /// Only whitelisted versions are allowed (default)
    Whitelist,
    /// All versions except blacklisted ones are allowed
    Blacklist,
}

impl Default for VersionFilterMode {
    fn default() -> Self {
        match DEFAULT_VERSION_FILTER_MODE {
            "blacklist" => VersionFilterMode::Blacklist,
            _ => VersionFilterMode::Whitelist,
        }
    }
}

impl std::fmt::Display for VersionFilterMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionFilterMode::Whitelist => write!(f, "whitelist"),
            VersionFilterMode::Blacklist => write!(f, "blacklist"),
        }
    }
}

/// Get the version filter mode for an agent from config or default
///
/// User can override in ~/.config/agent-unleashed/config.toml with:
/// ```toml
/// version_filter_mode = "blacklist"        # Claude Code (legacy, still works)
/// codex_version_filter_mode = "blacklist"  # Codex
/// ```
pub fn get_version_filter_mode_for(agent: AgentType) -> VersionFilterMode {
    let config_key = match agent {
        AgentType::Claude => "version_filter_mode",
        AgentType::Codex => "codex_version_filter_mode",
    };

    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".config/agent-unleashed/config.toml");
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with(config_key) {
                        if let Some(eq_pos) = trimmed.find('=') {
                            let value = trimmed[eq_pos + 1..].trim().trim_matches('"').trim_matches('\'');
                            return match value {
                                "blacklist" => VersionFilterMode::Blacklist,
                                _ => VersionFilterMode::Whitelist,
                            };
                        }
                    }
                }
            }
        }
    }

    // Use compiled default for this agent
    let default_mode = match agent {
        AgentType::Claude => DEFAULT_VERSION_FILTER_MODE,
        AgentType::Codex => DEFAULT_CODEX_VERSION_FILTER_MODE,
    };
    match default_mode {
        "blacklist" => VersionFilterMode::Blacklist,
        _ => VersionFilterMode::Whitelist,
    }
}

/// Get the version filter mode (Claude Code, for backward compat)
pub fn get_version_filter_mode() -> VersionFilterMode {
    get_version_filter_mode_for(AgentType::Claude)
}

/// Get the effective whitelist for an agent (user override or default from Cargo.toml)
///
/// User can override by creating:
/// - Claude: ~/.config/agent-unleashed/whitelist.txt
/// - Codex:  ~/.config/agent-unleashed/codex-whitelist.txt
pub fn get_whitelist_for(agent: AgentType) -> Vec<String> {
    let filename = match agent {
        AgentType::Claude => "whitelist.txt",
        AgentType::Codex => "codex-whitelist.txt",
    };

    // Check for user override
    if let Some(home) = dirs::home_dir() {
        let user_whitelist = home.join(".config/agent-unleashed").join(filename);
        if user_whitelist.exists() {
            if let Ok(content) = fs::read_to_string(&user_whitelist) {
                return content
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(|l| l.to_string())
                    .collect();
            }
        }
    }

    // Use default from Cargo.toml
    let defaults: &[&str] = match agent {
        AgentType::Claude => DEFAULT_WHITELIST,
        AgentType::Codex => DEFAULT_CODEX_WHITELIST,
    };
    defaults.iter().map(|s| s.to_string()).collect()
}

/// Get the effective whitelist (Claude Code, for backward compat)
#[allow(dead_code)]
pub fn get_whitelist() -> Vec<String> {
    get_whitelist_for(AgentType::Claude)
}

/// Get the effective blacklist for an agent (user override or default from Cargo.toml)
///
/// User can override by creating:
/// - Claude: ~/.config/agent-unleashed/blacklist.txt
/// - Codex:  ~/.config/agent-unleashed/codex-blacklist.txt
pub fn get_blacklist_for(agent: AgentType) -> Vec<String> {
    let filename = match agent {
        AgentType::Claude => "blacklist.txt",
        AgentType::Codex => "codex-blacklist.txt",
    };

    // Check for user override
    if let Some(home) = dirs::home_dir() {
        let user_blacklist = home.join(".config/agent-unleashed").join(filename);
        if user_blacklist.exists() {
            if let Ok(content) = fs::read_to_string(&user_blacklist) {
                return content
                    .lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(|l| l.to_string())
                    .collect();
            }
        }
    }

    // Use default from Cargo.toml
    let defaults: &[&str] = match agent {
        AgentType::Claude => DEFAULT_BLACKLIST,
        AgentType::Codex => DEFAULT_CODEX_BLACKLIST,
    };
    defaults.iter().map(|s| s.to_string()).collect()
}

/// Get the effective blacklist (Claude Code, for backward compat)
#[allow(dead_code)]
pub fn get_blacklist() -> Vec<String> {
    get_blacklist_for(AgentType::Claude)
}

/// Check if a version is whitelisted for an agent
pub fn is_whitelisted_for(version: &str, agent: AgentType) -> bool {
    get_whitelist_for(agent).iter().any(|v| v == version)
}

/// Check if a version is whitelisted (Claude Code, for backward compat)
#[allow(dead_code)]
pub fn is_whitelisted(version: &str) -> bool {
    is_whitelisted_for(version, AgentType::Claude)
}

/// Check if a version is blacklisted for an agent
pub fn is_blacklisted_for(version: &str, agent: AgentType) -> bool {
    get_blacklist_for(agent).iter().any(|v| v == version)
}

/// Check if a version is blacklisted (Claude Code, for backward compat)
#[allow(dead_code)]
pub fn is_blacklisted(version: &str) -> bool {
    is_blacklisted_for(version, AgentType::Claude)
}

/// Check if a version is allowed for an agent based on the current filter mode
pub fn is_version_allowed_for(version: &str, agent: AgentType) -> bool {
    match get_version_filter_mode_for(agent) {
        VersionFilterMode::Whitelist => is_whitelisted_for(version, agent),
        VersionFilterMode::Blacklist => !is_blacklisted_for(version, agent),
    }
}

/// Check if a version is allowed (Claude Code, for backward compat)
#[allow(dead_code)]
pub fn is_version_allowed(version: &str) -> bool {
    is_version_allowed_for(version, AgentType::Claude)
}

/// Information about an agent version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub is_installed: bool,
    pub has_patch: bool,
    pub is_whitelisted: bool,
    pub is_blacklisted: bool,
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
pub struct VersionManager {
    /// Path to patches directory (for checking supported Claude versions)
    patches_dir: Option<PathBuf>,
}

impl VersionManager {
    pub fn new() -> Self {
        // Try to find patches directory relative to exe or in common locations
        let patches_dir = Self::find_patches_dir();
        Self { patches_dir }
    }

    fn find_patches_dir() -> Option<PathBuf> {
        // Try relative to executable
        if let Ok(exe_path) = std::env::current_exe() {
            // Check ~/.local/bin/patches
            if let Some(bin_dir) = exe_path.parent() {
                let patches = bin_dir.join("patches/versions");
                if patches.exists() {
                    return Some(patches);
                }
            }
        }

        // Try ~/.local/bin/patches
        if let Some(home) = dirs::home_dir() {
            let patches = home.join(".local/bin/patches/versions");
            if patches.exists() {
                return Some(patches);
            }
        }

        // Try repo location (for development)
        let repo_patches = PathBuf::from("scripts/patches/versions");
        if repo_patches.exists() {
            return Some(repo_patches);
        }

        None
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

    /// Get list of Claude versions that have patch configs
    pub fn get_supported_versions(&self) -> Vec<String> {
        let mut versions = Vec::new();

        if let Some(ref patches_dir) = self.patches_dir {
            if let Ok(entries) = std::fs::read_dir(patches_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "conf") {
                        if let Some(stem) = path.file_stem() {
                            versions.push(stem.to_string_lossy().to_string());
                        }
                    }
                }
            }
        }

        // Sort versions
        versions.sort_by(|a, b| version_compare(a, b));
        versions.reverse(); // Newest first
        versions
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

    /// Check if a version exists on GCS (by checking if its manifest exists)
    fn gcs_version_exists(version: &str) -> bool {
        Command::new("curl")
            .args(["-fsSL", "--head", "-o", "/dev/null", "-w", "%{http_code}",
                   &format!("{}/{}/manifest.json", CLAUDE_GCS_BUCKET, version)])
            .output()
            .is_ok_and(|o| {
                o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "200"
            })
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

        // Check known whitelisted versions on GCS
        let whitelist = get_whitelist_for(AgentType::Claude);
        for v in &whitelist {
            if !seen.contains(v) && Self::gcs_version_exists(v) {
                seen.insert(v.clone());
                versions.push(v.clone());
            }
        }

        // Fallback: query npm registry for additional versions
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
        let supported = self.get_supported_versions();
        let available = self.get_available_versions().unwrap_or_default();

        // Combine supported and available, removing duplicates
        let mut seen = std::collections::HashSet::new();
        let mut versions = Vec::new();

        // Add supported versions first (they have patches)
        for v in &supported {
            if seen.insert(v.clone()) {
                versions.push(VersionInfo {
                    version: v.clone(),
                    is_installed: installed.as_ref() == Some(v),
                    has_patch: true,
                    is_whitelisted: is_whitelisted_for(v, AgentType::Claude),
                    is_blacklisted: is_blacklisted_for(v, AgentType::Claude),
                });
            }
        }

        // Add other available versions
        for v in &available {
            if seen.insert(v.clone()) {
                versions.push(VersionInfo {
                    version: v.clone(),
                    is_installed: installed.as_ref() == Some(v),
                    has_patch: supported.contains(v),
                    is_whitelisted: is_whitelisted_for(v, AgentType::Claude),
                    is_blacklisted: is_blacklisted_for(v, AgentType::Claude),
                });
            }
        }

        // Sort by version (newest first)
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        versions
    }

    /// Install a specific version of Claude Code
    /// Tries npm first (produces patchable cli.js), falls back to native binary from GCS
    pub fn install_version(&self, version: &str) -> io::Result<InstallResult> {
        // Try npm first (preserves patchable cli.js for auto-mode support)
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
            // npm install failed, fall through to native
        }

        // Fallback: install via native binary from GCS
        self.install_version_native(version)
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

    /// Run the patch script for the installed Claude version
    pub fn run_patch(&self) -> io::Result<InstallResult> {
        let patch_script = self.find_patch_script()?;

        let output = Command::new("bash")
            .arg(&patch_script)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(InstallResult {
                success: true,
                stdout,
                stderr,
                error: None,
            })
        } else {
            Ok(InstallResult {
                success: false,
                stdout,
                stderr,
                error: Some("Patch script failed".to_string()),
            })
        }
    }

    fn find_patch_script(&self) -> io::Result<PathBuf> {
        // Try relative to patches dir
        if let Some(ref patches_dir) = self.patches_dir {
            let script = patches_dir.parent().and_then(|p| p.parent()).map(|p| p.join("patch-claude.sh"));
            if let Some(s) = script {
                if s.exists() {
                    return Ok(s);
                }
            }
        }

        // Try ~/.local/bin
        if let Some(home) = dirs::home_dir() {
            let script = home.join(".local/bin/patch-claude.sh");
            if script.exists() {
                return Ok(script);
            }
        }

        // Try current directory (development)
        let script = PathBuf::from("scripts/patch-claude.sh");
        if script.exists() {
            return Ok(script);
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "patch-claude.sh not found",
        ))
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
                has_patch: false,
                is_whitelisted: is_whitelisted_for(v, AgentType::Codex),
                is_blacklisted: is_blacklisted_for(v, AgentType::Codex),
            });
        }

        // Sort by version (newest first)
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        versions
    }

    /// Install a specific Codex version by downloading prebuilt binaries from GitHub releases
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
        Self::new()
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
    let mode = get_version_filter_mode();

    if json {
        let output = VersionListOutput {
            currently_installed: current,
            filter_mode: mode.to_string(),
            versions: versions
                .into_iter()
                .map(|info| VersionListItem {
                    version: info.version,
                    is_installed: info.is_installed,
                    has_patch: info.has_patch,
                    is_whitelisted: info.is_whitelisted,
                    is_blacklisted: info.is_blacklisted,
                })
                .collect(),
        };
        json_output::print_json(&output);
    } else {
        println!("Claude Code Versions (mode: {}):", mode);
        println!();

        for info in versions {
            let installed = if info.is_installed { " [installed]" } else { "" };
            let patch = if info.has_patch { " *" } else { "" };
            let whitelisted = if info.is_whitelisted { " ✓" } else { "" };
            let blacklisted = if info.is_blacklisted { " ⛔" } else { "" };
            println!("  v{}{}{}{}{}", info.version, installed, patch, whitelisted, blacklisted);
        }

        println!();
        println!("  * = has auto-mode patch available");
        println!("  ✓ = whitelisted (verified working)");
        println!("  ⛔ = blacklisted (known issues)");
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
        println!("Running patch...");
    }

    let patch_result = vm.run_patch();
    let patch_warning = patch_result.is_err() || patch_result.as_ref().is_ok_and(|r| !r.success);

    if !json {
        if patch_warning {
            println!("Warning: Patch failed (may not be available for this version)");
        }
        println!("Done!");
    } else {
        let message = if patch_warning {
            format!(
                "Successfully installed Claude Code v{} (patch not available)",
                version
            )
        } else {
            format!("Successfully installed Claude Code v{}", version)
        };
        json_output::print_success_json(&message);
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
        let vm = VersionManager::new();
        // Should not panic
        let _ = vm.get_supported_versions();
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

    #[test]
    fn test_default_whitelist() {
        assert!(!DEFAULT_WHITELIST.is_empty(), "Default whitelist should not be empty");
        assert!(DEFAULT_WHITELIST.contains(&"2.1.12"), "2.1.12 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.1.4"), "2.1.4 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.1.3"), "2.1.3 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.1.2"), "2.1.2 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.0.77"), "2.0.77 should be whitelisted");
    }

    #[test]
    fn test_default_blacklist() {
        assert!(!DEFAULT_BLACKLIST.is_empty(), "Default blacklist should not be empty");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.5"), "2.1.5 should be blacklisted");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.1"), "2.1.1 should be blacklisted");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.0"), "2.1.0 should be blacklisted");
    }

    #[test]
    fn test_default_codex_whitelist() {
        assert!(!DEFAULT_CODEX_WHITELIST.is_empty(), "Codex whitelist should not be empty");
        assert!(DEFAULT_CODEX_WHITELIST.contains(&"0.93.0"), "0.93.0 should be in Codex whitelist");
        assert!(DEFAULT_CODEX_WHITELIST.contains(&"0.92.0"), "0.92.0 should be in Codex whitelist");
    }

    #[test]
    fn test_default_codex_blacklist() {
        // Codex blacklist is currently empty
        assert!(DEFAULT_CODEX_BLACKLIST.is_empty(), "Codex blacklist should be empty initially");
    }

    #[test]
    fn test_is_whitelisted() {
        assert!(is_whitelisted("2.1.12"), "2.1.12 should be whitelisted");
        assert!(is_whitelisted("2.1.4"), "2.1.4 should be whitelisted");
        assert!(is_whitelisted("2.1.3"), "2.1.3 should be whitelisted");
        assert!(is_whitelisted("2.1.2"), "2.1.2 should be whitelisted");
        assert!(is_whitelisted("2.0.77"), "2.0.77 should be whitelisted");

        assert!(!is_whitelisted("2.1.14"), "2.1.14 should not be whitelisted");
        assert!(!is_whitelisted("2.1.5"), "2.1.5 should not be whitelisted");
        assert!(!is_whitelisted("2.1.1"), "2.1.1 should not be whitelisted");
        assert!(!is_whitelisted("2.1.0"), "2.1.0 should not be whitelisted");
        assert!(!is_whitelisted("9.9.9"), "9.9.9 should not be whitelisted");
    }

    #[test]
    fn test_is_blacklisted() {
        assert!(is_blacklisted("2.1.5"), "2.1.5 should be blacklisted");
        assert!(is_blacklisted("2.1.1"), "2.1.1 should be blacklisted");
        assert!(is_blacklisted("2.1.0"), "2.1.0 should be blacklisted");

        assert!(!is_blacklisted("2.1.4"), "2.1.4 should not be blacklisted");
        assert!(!is_blacklisted("2.1.3"), "2.1.3 should not be blacklisted");
        assert!(!is_blacklisted("2.0.77"), "2.0.77 should not be blacklisted");
        assert!(!is_blacklisted("9.9.9"), "9.9.9 should not be blacklisted");
    }

    #[test]
    fn test_is_whitelisted_for_codex() {
        assert!(is_whitelisted_for("0.93.0", AgentType::Codex), "0.93.0 should be Codex whitelisted");
        assert!(is_whitelisted_for("0.92.0", AgentType::Codex), "0.92.0 should be Codex whitelisted");
        assert!(!is_whitelisted_for("0.50.0", AgentType::Codex), "0.50.0 should not be Codex whitelisted");
    }

    #[test]
    fn test_is_version_allowed_for() {
        // Claude uses whitelist mode by default
        assert!(is_version_allowed_for("2.1.12", AgentType::Claude));
        assert!(!is_version_allowed_for("9.9.9", AgentType::Claude));

        // Codex uses whitelist mode by default
        assert!(is_version_allowed_for("0.93.0", AgentType::Codex));
        assert!(!is_version_allowed_for("9.9.9", AgentType::Codex));
    }

    #[test]
    fn test_default_filter_mode() {
        assert_eq!(DEFAULT_VERSION_FILTER_MODE, "whitelist");
        assert_eq!(DEFAULT_CODEX_VERSION_FILTER_MODE, "whitelist");
    }

    /// Create a mock "npm" binary that captures arguments to a file.
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&mock_npm).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&mock_npm, perms).unwrap();
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
}
