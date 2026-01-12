//! Claude Code version management
//!
//! Handles detecting installed version, listing available versions,
//! and switching between Claude Code versions.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

// Include the generated blacklist from Cargo.toml
include!(concat!(env!("OUT_DIR"), "/blacklist.rs"));

/// Get the effective blacklist (user override or default from Cargo.toml)
///
/// User can override by creating ~/.config/claude-unleashed/blacklist.txt
/// with one version per line. Empty file means no blacklist.
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

/// Check if a version is blacklisted
pub fn is_blacklisted(version: &str) -> bool {
    get_blacklist().iter().any(|v| v == version)
}

/// Information about a Claude Code version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub is_installed: bool,
    pub has_patch: bool,
    pub is_blacklisted: bool,
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
                    is_blacklisted: is_blacklisted(v),
                });
            }
        }

        // Sort by version (newest first)
        versions.sort_by(|a, b| version_compare(&b.version, &a.version));

        versions
    }

    /// Install a specific version of Claude Code
    pub fn install_version(&self, version: &str) -> io::Result<()> {
        let status = Command::new("npm")
            .args(["install", "-g", &format!("@anthropic-ai/claude-code@{}", version)])
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to install Claude Code {}", version),
            ))
        }
    }

    /// Run the patch script for the installed version
    pub fn run_patch(&self) -> io::Result<()> {
        // Try to find patch script
        let patch_script = self.find_patch_script()?;

        let status = Command::new("bash")
            .arg(&patch_script)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Patch script failed",
            ))
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
pub fn list_versions() -> io::Result<()> {
    let vm = VersionManager::new();
    let versions = vm.get_version_list();
    let current = vm.get_installed_version();

    println!("Claude Code Versions:");
    println!();

    for info in versions {
        let installed = if info.is_installed { " [installed]" } else { "" };
        let patch = if info.has_patch { " *" } else { "" };
        println!("  v{}{}{}", info.version, installed, patch);
    }

    println!();
    println!("  * = has auto-mode patch available");
    if let Some(v) = current {
        println!();
        println!("Currently installed: v{}", v);
    }

    Ok(())
}

/// Install a specific version
pub fn install_version(version: &str) -> io::Result<()> {
    let vm = VersionManager::new();

    println!("Installing Claude Code v{}...", version);
    vm.install_version(version)?;

    println!("Running patch...");
    if vm.run_patch().is_err() {
        println!("Warning: Patch failed (may not be available for this version)");
    }

    println!("Done!");
    Ok(())
}

/// Show current version
pub fn show_current() -> io::Result<()> {
    let vm = VersionManager::new();
    match vm.get_installed_version() {
        Some(v) => println!("Claude Code version: {}", v),
        None => println!("Claude Code is not installed"),
    }
    Ok(())
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
    fn test_default_blacklist() {
        // Verify the default blacklist from Cargo.toml is loaded
        assert!(!DEFAULT_BLACKLIST.is_empty(), "Default blacklist should not be empty");

        // Verify expected versions are in the default blacklist
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.5"), "2.1.5 should be blacklisted");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.1"), "2.1.1 should be blacklisted");
        assert!(DEFAULT_BLACKLIST.contains(&"2.1.0"), "2.1.0 should be blacklisted");
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
        assert!(!is_blacklisted("2.0.0"), "2.0.0 should not be blacklisted");
    }
}
