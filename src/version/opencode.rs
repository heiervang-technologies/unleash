use std::io;
use std::process::Command;
use std::sync::mpsc;

use super::types::{InstallResult, VersionInfo};
use super::VersionManager;
use super::{load_embedded_versions, version_compare};

impl VersionManager {
    // ── OpenCode version management ─────────────────

    pub fn get_opencode_available_versions(&self) -> io::Result<Vec<String>> {
        let mut versions = Vec::new();
        if let Ok(mut v) = Self::query_npm_registry_versions("opencode-ai", 20) {
            v.retain(|s| s.starts_with(|c: char| c.is_ascii_digit()));
            versions = v;
        }

        if versions.is_empty() {
            // Fallback to embedded versions
            let embedded = load_embedded_versions();
            if let Some(v_list) = embedded.get("opencode") {
                if !v_list.is_empty() {
                    return Ok(v_list.clone());
                }
            }
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
                    Some(format!(
                        "Failed to upgrade OpenCode to v{}: {}",
                        version, stderr
                    ))
                },
            });
        }

        // Fresh install via npm
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "Neither opencode nor npm is available".to_string(),
                error: Some(
                    "Install opencode first: curl -fsSL https://opencode.ai/install | bash"
                        .to_string(),
                ),
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
                Some(format!(
                    "Failed to install OpenCode v{}: {}",
                    version, stderr
                ))
            },
        })
    }

    // ── Streaming install methods ──────────────────────────────

    /// Read stdout/stderr from a child process, sending each line via `log_tx`.
    /// Reads stdout in a spawned thread and stderr in the calling thread to avoid
    /// pipe buffer deadlock. Returns accumulated (stdout, stderr) strings.

    pub fn install_opencode_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        // Prefer `opencode upgrade` if already installed (updates the actual binary in-place)
        if which::which("opencode").is_ok() {
            let _ = log_tx.send(format!("Running: opencode upgrade {}", version));
            let (ok, stdout, stderr) =
                Self::run_streaming(Command::new("opencode").args(["upgrade", version]), &log_tx)?;

            return Ok(InstallResult {
                success: ok,
                stdout,
                stderr: stderr.clone(),
                error: if ok {
                    None
                } else {
                    Some(format!(
                        "Failed to upgrade OpenCode to v{}: {}",
                        version, stderr
                    ))
                },
            });
        }

        // Fresh install via npm
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "Neither opencode nor npm is available".to_string(),
                error: Some(
                    "Install opencode first: curl -fsSL https://opencode.ai/install | bash"
                        .to_string(),
                ),
            });
        }

        let use_sudo = Self::npm_global_needs_sudo();
        let _ = log_tx.send(format!(
            "Running: {}npm install -g opencode-ai@{}",
            if use_sudo { "sudo " } else { "" },
            version
        ));
        let (ok, stdout, stderr) = Self::run_streaming(
            Self::npm_global_command().args(["install", "-g", &format!("opencode-ai@{}", version)]),
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
                    "Failed to install OpenCode v{}: {}",
                    version, stderr
                ))
            },
        })
    }

}
