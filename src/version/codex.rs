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

    /// Install a specific release, including its matching Code Mode host.
    #[allow(dead_code)]
    pub fn install_codex_version(&self, version: &str) -> io::Result<InstallResult> {
        self.install_codex_release(version, |_| {})
    }

    pub fn install_codex_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        self.install_codex_release(version, |message| {
            let _ = log_tx.send(message.to_owned());
        })
    }

    fn install_codex_release(
        &self,
        version: &str,
        log: impl FnMut(&str),
    ) -> io::Result<InstallResult> {
        let install_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?
            .join(".local/bin");
        match crate::agents::codex::install_release(Some(version), &install_dir, log) {
            Ok(version) => Ok(InstallResult {
                success: true,
                stdout: format!("Codex v{version} installed to {}", install_dir.display()),
                stderr: String::new(),
                error: None,
            }),
            Err(error) => Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: error.to_string(),
                error: Some(error.to_string()),
            }),
        }
    }
}
