//! Claude Code version management
//!
//! Handles detecting installed version, listing available versions,
//! and switching between Claude Code versions.

use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Information about a Claude Code version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub is_installed: bool,
    pub has_patch: bool,
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
}
