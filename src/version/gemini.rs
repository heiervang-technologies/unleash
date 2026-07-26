use std::io;
use std::sync::mpsc;

use super::types::{InstallResult, VersionInfo};
use super::VersionManager;
use super::{version_compare, load_embedded_versions};

impl VersionManager {
    // ── Gemini CLI version management ───────────────

    pub fn get_gemini_available_versions(&self) -> io::Result<Vec<String>> {
        let res = Self::query_npm_registry_versions("@google/gemini-cli", 20);
        match res {
            Ok(versions) if !versions.is_empty() => Ok(versions),
            _ => {
                let embedded = load_embedded_versions();
                if let Some(v_list) = embedded.get("gemini") {
                    if !v_list.is_empty() {
                        return Ok(v_list.clone());
                    }
                }
                Err(io::Error::other(
                    "Failed to query available versions for Gemini CLI",
                ))
            }
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

    // ── Pi ──────────────────────────────────────────────────

    /// Get available Pi versions from npm registry

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

}
