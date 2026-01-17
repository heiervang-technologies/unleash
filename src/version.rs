//! Claude Code version management
//!
//! Handles detecting installed version, listing available versions,
//! and switching between Claude Code versions.
//!
//! Supports two filtering modes:
//! - **Whitelist mode** (default): Only whitelisted versions are allowed
//! - **Blacklist mode**: All versions except blacklisted ones are allowed

use crate::json_output::{self, VersionListItem, VersionListOutput, VersionOutput};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

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

/// Get the version filter mode from config or default
///
/// User can override in ~/.config/claude-unleashed/config.toml with:
/// ```toml
/// version_filter_mode = "blacklist"  # or "whitelist"
/// ```
pub fn get_version_filter_mode() -> VersionFilterMode {
    if let Some(home) = dirs::home_dir() {
        let config_path = home.join(".config/claude-unleashed/config.toml");
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("version_filter_mode") {
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

    VersionFilterMode::default()
}

/// Get the effective whitelist (user override or default from Cargo.toml)
///
/// User can override by creating ~/.config/claude-unleashed/whitelist.txt
/// with one version per line. Empty file means no whitelist (all versions blocked).
pub fn get_whitelist() -> Vec<String> {
    // Check for user override
    if let Some(home) = dirs::home_dir() {
        let user_whitelist = home.join(".config/claude-unleashed/whitelist.txt");
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
    DEFAULT_WHITELIST.iter().map(|s| s.to_string()).collect()
}

/// Get the effective blacklist (user override or default from Cargo.toml)
///
/// User can override by creating ~/.config/claude-unleashed/blacklist.txt
/// with one version per line. Empty file means no blacklist (all versions allowed).
pub fn get_blacklist() -> Vec<String> {
    // Check for user override
    if let Some(home) = dirs::home_dir() {
        let user_blacklist = home.join(".config/claude-unleashed/blacklist.txt");
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
    DEFAULT_BLACKLIST.iter().map(|s| s.to_string()).collect()
}

/// Check if a version is whitelisted (verified to work)
pub fn is_whitelisted(version: &str) -> bool {
    get_whitelist().iter().any(|v| v == version)
}

/// Check if a version is blacklisted (known issues)
pub fn is_blacklisted(version: &str) -> bool {
    get_blacklist().iter().any(|v| v == version)
}

/// Check if a version is allowed based on the current filter mode
///
/// - In whitelist mode: version must be in the whitelist
/// - In blacklist mode: version must NOT be in the blacklist
pub fn is_version_allowed(version: &str) -> bool {
    match get_version_filter_mode() {
        VersionFilterMode::Whitelist => is_whitelisted(version),
        VersionFilterMode::Blacklist => !is_blacklisted(version),
    }
}

/// Information about a Claude Code version
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

/// Version manager for Claude Code
pub struct VersionManager {
    /// Path to patches directory (for checking supported versions)
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

    /// Get list of versions that have patch configs
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

    /// Get available versions from npm registry
    pub fn get_available_versions(&self) -> io::Result<Vec<String>> {
        let output = Command::new("npm")
            .args(["view", "@anthropic-ai/claude-code", "versions", "--json"])
            .output()?;

        if output.status.success() {
            let json_str = String::from_utf8_lossy(&output.stdout);
            // Simple JSON array parsing (avoid adding serde_json dependency)
            let versions: Vec<String> = json_str
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Return recent versions (last 20)
            let recent: Vec<String> = versions.into_iter().rev().take(20).collect();
            Ok(recent)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to query npm registry",
            ))
        }
    }

    /// Get combined version list with status
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
                    is_whitelisted: is_whitelisted(v),
                    is_blacklisted: is_blacklisted(v),
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
                    is_whitelisted: is_whitelisted(v),
                    is_blacklisted: is_blacklisted(v),
                });
            }
        }

        // Sort by version (newest first)
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        versions
    }

    /// Install a specific version of Claude Code
    /// Returns (success, stdout, stderr) for TUI to display if needed
    pub fn install_version(&self, version: &str) -> io::Result<InstallResult> {
        // Use --force to allow downgrading to older versions
        let output = Command::new("npm")
            .args(["install", "-g", "--force", &format!("@anthropic-ai/claude-code@{}", version)])
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
                error: Some(format!("npm install exited with status {}", output.status)),
            })
        }
    }

    /// Run the patch script for the installed version
    /// Returns InstallResult with captured output
    pub fn run_patch(&self) -> io::Result<InstallResult> {
        // Try to find patch script
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
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Compare version strings (semver-like)
fn version_compare(a: &str, b: &str) -> std::cmp::Ordering {
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
        return Err(io::Error::new(
            io::ErrorKind::Other,
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
        claude_unleashed_version: cu_version.to_string(),
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
    ///
    /// This test proves that:
    /// 1. Calling subprocess 10 times takes significant time (>100ms with 50ms sleep)
    /// 2. Accessing cached value 10 times is nearly instant (<1ms)
    /// 3. The cached approach is at least 100x faster
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

        // Verify the version was correctly parsed
        assert_eq!(cached_version, Some("2.1.5".to_string()));

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
        // Verify the default whitelist from Cargo.toml is loaded
        assert!(!DEFAULT_WHITELIST.is_empty(), "Default whitelist should not be empty");

        // Verify expected versions are in the default whitelist
        assert!(DEFAULT_WHITELIST.contains(&"2.1.4"), "2.1.4 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.1.3"), "2.1.3 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.1.2"), "2.1.2 should be whitelisted");
        assert!(DEFAULT_WHITELIST.contains(&"2.0.77"), "2.0.77 should be whitelisted");
    }

    #[test]
    fn test_default_blacklist() {
        // Verify the default blacklist from Cargo.toml is loaded
        assert!(!DEFAULT_BLACKLIST.is_empty(), "Default blacklist should not be empty");

        // Verify expected versions are in the default blacklist
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.5"), "2.1.5 should be blacklisted");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.1"), "2.1.1 should be blacklisted");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.0"), "2.1.0 should be blacklisted");
    }

    #[test]
    fn test_is_whitelisted() {
        // Test that whitelisted versions are detected
        assert!(is_whitelisted("2.1.4"), "2.1.4 should be whitelisted");
        assert!(is_whitelisted("2.1.3"), "2.1.3 should be whitelisted");
        assert!(is_whitelisted("2.1.2"), "2.1.2 should be whitelisted");
        assert!(is_whitelisted("2.0.77"), "2.0.77 should be whitelisted");

        // Test that non-whitelisted versions are not detected
        assert!(!is_whitelisted("2.1.5"), "2.1.5 should not be whitelisted");
        assert!(!is_whitelisted("2.1.1"), "2.1.1 should not be whitelisted");
        assert!(!is_whitelisted("2.1.0"), "2.1.0 should not be whitelisted");
        assert!(!is_whitelisted("9.9.9"), "9.9.9 should not be whitelisted");
    }

    #[test]
    fn test_is_blacklisted() {
        // Test that blacklisted versions are detected
        assert!(is_blacklisted("2.1.5"), "2.1.5 should be blacklisted");
        assert!(is_blacklisted("2.1.1"), "2.1.1 should be blacklisted");
        assert!(is_blacklisted("2.1.0"), "2.1.0 should be blacklisted");

        // Test that non-blacklisted versions are not detected
        assert!(!is_blacklisted("2.1.4"), "2.1.4 should not be blacklisted");
        assert!(!is_blacklisted("2.1.3"), "2.1.3 should not be blacklisted");
        assert!(!is_blacklisted("2.0.77"), "2.0.77 should not be blacklisted");
        assert!(!is_blacklisted("9.9.9"), "9.9.9 should not be blacklisted");
    }

    #[test]
    fn test_default_filter_mode() {
        // Verify default mode is whitelist
        assert_eq!(DEFAULT_VERSION_FILTER_MODE, "whitelist");
    }

    /// Create a mock "npm" binary that captures arguments to a file.
    /// Returns the temp directory (must be kept alive), the path to add to PATH,
    /// and the path to the args capture file.
    fn create_mock_npm() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        let mock_path = temp.path().to_path_buf();
        let mock_npm = mock_path.join("npm");
        let args_file = mock_path.join("npm_args.txt");

        // Create a shell script that captures all arguments to a file
        let script = format!(
            "#!/bin/bash\necho \"$@\" > \"{}\"\nexit 0\n",
            args_file.display()
        );
        std::fs::write(&mock_npm, script).unwrap();

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&mock_npm).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&mock_npm, perms).unwrap();
        }

        (temp, mock_path, args_file)
    }

    /// Test that install_version uses --force flag to allow downgrades.
    ///
    /// This is critical because npm won't downgrade a globally installed package
    /// to an older version without --force. Without this flag, users cannot
    /// install older whitelisted versions when a newer version is installed.
    #[test]
    fn test_install_version_uses_force_flag() {
        // Create mock npm that captures arguments
        let (_temp, mock_path, args_file) = create_mock_npm();

        // Prepend mock to PATH
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", mock_path.display(), original_path);

        // SAFETY: This test runs single-threaded and restores PATH before returning
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        let vm = VersionManager::new();
        let result = vm.install_version("2.1.4");

        // Restore PATH before any assertions
        // SAFETY: Restoring original PATH value
        unsafe {
            std::env::set_var("PATH", original_path);
        }

        // Verify the install succeeded (mock returns exit 0)
        assert!(result.is_ok(), "install_version should not return an error");
        let install_result = result.unwrap();
        assert!(install_result.success, "install should succeed with mock npm");

        // Read the captured arguments
        let captured_args = std::fs::read_to_string(&args_file)
            .expect("Should be able to read captured npm arguments");

        // Verify --force flag is present
        assert!(
            captured_args.contains("--force"),
            "npm install should include --force flag for downgrades. Got: {}",
            captured_args.trim()
        );

        // Verify the full expected command structure
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
