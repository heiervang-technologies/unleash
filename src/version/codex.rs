use std::fs;
use std::io;
use std::process::Command;
use std::sync::mpsc;

use super::types::{InstallResult, VersionInfo};
use super::VersionManager;
use super::{load_embedded_versions, version_compare};

impl VersionManager {
    // ── Codex version management ────────────────────

    pub fn get_codex_available_versions(&self) -> io::Result<Vec<String>> {
        let mut versions = Vec::new();
        if let Ok(output) = Command::new("gh")
            .args([
                "api",
                "repos/openai/codex/tags",
                "--paginate",
                "--jq",
                ".[].name",
            ])
            .output()
        {
            if output.status.success() {
                let tag_output = String::from_utf8_lossy(&output.stdout);
                versions = tag_output
                    .lines()
                    .filter(|line| line.starts_with("rust-v"))
                    .filter(|line| !line.contains("alpha"))
                    .map(|line| line.trim_start_matches("rust-v").to_string())
                    .filter(|v| !v.is_empty() && v.starts_with(|c: char| c.is_ascii_digit()))
                    .collect();
            }
        }

        if versions.is_empty() {
            // Fallback to embedded versions
            let embedded = load_embedded_versions();
            if let Some(v_list) = embedded.get("codex") {
                if !v_list.is_empty() {
                    return Ok(v_list.clone());
                }
            }
            return Err(io::Error::other(
                "Failed to query GitHub releases for Codex",
            ));
        }

        // Sort newest first, then take top 20
        versions.sort_by(|a, b| version_compare(b, a));
        versions.truncate(20);
        Ok(versions)
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
        let code_mode_host_asset_name = Self::codex_code_mode_host_asset_name();

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
                "--pattern",
                &format!("{}.tar.gz", code_mode_host_asset_name),
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

        // Code Mode was introduced as a version-matched companion binary.
        // Older releases may not publish it, so install it when present while
        // retaining the ability to select historical Codex versions.
        let code_mode_host_archive = tmp_dir.join(format!("{}.tar.gz", code_mode_host_asset_name));
        let extracted_code_mode_host = tmp_dir.join(&code_mode_host_asset_name);
        if code_mode_host_archive.exists() {
            let extract_host = Command::new("tar")
                .args(["xzf", &format!("{}.tar.gz", code_mode_host_asset_name)])
                .current_dir(&tmp_dir)
                .output()?;
            if !extract_host.status.success() {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Ok(InstallResult {
                    success: false,
                    stdout: String::from_utf8_lossy(&extract_host.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&extract_host.stderr).to_string(),
                    error: Some("Failed to extract Codex Code Mode host tarball".to_string()),
                });
            }
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

        if code_mode_host_archive.exists() {
            if !extracted_code_mode_host.exists() {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!(
                        "Expected binary {} not found in archive",
                        code_mode_host_asset_name
                    ),
                    error: Some("Code Mode host not found after extraction".to_string()),
                });
            }
            crate::agents::atomic_install_binary(
                &extracted_code_mode_host,
                &install_dir.join("codex-code-mode-host"),
            )?;
        }
        crate::agents::atomic_install_binary(&extracted_binary, &install_path)?;

        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(InstallResult {
            success: true,
            stdout: format!("Codex v{} installed to {}", version, install_path.display()),
            stderr: String::new(),
            error: None,
        })
    }

    pub fn install_codex_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        let tag = format!("rust-v{}", version);
        let asset_name = Self::codex_asset_name();
        let code_mode_host_asset_name = Self::codex_code_mode_host_asset_name();

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
                "--pattern",
                &format!("{}.tar.gz", code_mode_host_asset_name),
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

        let code_mode_host_archive = tmp_dir.join(format!("{}.tar.gz", code_mode_host_asset_name));
        let extracted_code_mode_host = tmp_dir.join(&code_mode_host_asset_name);
        if code_mode_host_archive.exists() {
            let _ = log_tx.send("Extracting Codex Code Mode host...".to_string());
            let (ok, stdout, stderr) = Self::run_streaming(
                Command::new("tar")
                    .args(["xzf", &format!("{}.tar.gz", code_mode_host_asset_name)])
                    .current_dir(&tmp_dir),
                &log_tx,
            )?;
            if !ok {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Ok(InstallResult {
                    success: false,
                    stdout,
                    stderr,
                    error: Some("Failed to extract Codex Code Mode host tarball".to_string()),
                });
            }
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
        // Atomic install: stage + chmod + rename so switching codex versions
        // while a codex agent is running can't hit ETXTBSY or leave a partial
        // binary at the canonical path.
        if code_mode_host_archive.exists() {
            if !extracted_code_mode_host.exists() {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Ok(InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: format!(
                        "Expected binary {} not found in archive",
                        code_mode_host_asset_name
                    ),
                    error: Some("Code Mode host not found after extraction".to_string()),
                });
            }
            let _ = log_tx.send("Installing Codex Code Mode host...".to_string());
            crate::agents::atomic_install_binary(
                &extracted_code_mode_host,
                &install_dir.join("codex-code-mode-host"),
            )?;
        }
        crate::agents::atomic_install_binary(&extracted_binary, &install_path)?;

        let _ = fs::remove_dir_all(&tmp_dir);

        Ok(InstallResult {
            success: true,
            stdout: format!("Codex v{} installed to {}", version, install_path.display()),
            stderr: String::new(),
            error: None,
        })
    }

    pub(super) fn codex_asset_name() -> String {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;

        let target_arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => "x86_64",
        };

        let target_triple = match os {
            "linux" => format!("{}-unknown-linux-musl", target_arch),
            "macos" => format!("{}-apple-darwin", target_arch),
            _ => format!("{}-unknown-linux-musl", target_arch),
        };

        format!("codex-{}", target_triple)
    }

    pub(super) fn codex_code_mode_host_asset_name() -> String {
        Self::codex_asset_name().replacen("codex-", "codex-code-mode-host-", 1)
    }
}
