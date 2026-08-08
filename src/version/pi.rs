use std::io;
use std::sync::mpsc;

use super::types::{InstallResult, VersionInfo};
use super::VersionManager;
use super::{load_embedded_versions, version_compare};

impl VersionManager {
    // ── Pi version management ───────────────────────

    pub fn get_pi_available_versions(&self) -> io::Result<Vec<String>> {
        let res = Self::query_npm_registry_versions(crate::agents::PI_NPM_PACKAGE, 20);
        match res {
            Ok(versions) if !versions.is_empty() => Ok(versions),
            _ => {
                let embedded = load_embedded_versions();
                if let Some(v_list) = embedded.get("pi") {
                    if !v_list.is_empty() {
                        return Ok(v_list.clone());
                    }
                }
                Err(io::Error::other(
                    "Failed to query available versions for Pi",
                ))
            }
        }
    }

    /// Get combined Pi version list with status
    pub fn get_pi_version_list(&self, installed: Option<&str>) -> Vec<VersionInfo> {
        let available = self.get_pi_available_versions().unwrap_or_default();

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

    pub fn install_pi_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        if !Self::has_npm() {
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: "npm is not available".to_string(),
                error: Some("npm is required to install Pi".to_string()),
            });
        }

        let use_sudo = Self::npm_global_needs_sudo();
        let _ = log_tx.send(format!(
            "Running: {}npm install -g {}@{}",
            if use_sudo { "sudo " } else { "" },
            crate::agents::PI_NPM_PACKAGE,
            version
        ));
        // `--force` is what handles the bin collision on this path: Pi moved
        // scope upstream, and the deprecated package declares the same `pi`
        // bin, so without it npm can refuse or silently keep the old link.
        // The updater in AgentManager uninstalls the old package outright;
        // here we only ever install one explicit version, so forcing the link
        // is enough.
        let (ok, stdout, stderr) = Self::run_streaming(
            Self::npm_global_command().args([
                "install",
                "-g",
                "--force",
                &format!("{}@{}", crate::agents::PI_NPM_PACKAGE, version),
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
                Some(format!("Failed to install Pi v{}: {}", version, stderr))
            },
        })
    }
}
