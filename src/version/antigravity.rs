use std::io;
use std::process::Command;
use std::sync::mpsc;

use super::types::{InstallResult, VersionInfo};
use super::VersionManager;
use super::{load_embedded_versions, version_compare};

impl VersionManager {
    // ── Antigravity version management ──────────────

    pub fn get_antigravity_available_versions(&self) -> io::Result<Vec<String>> {
        let embedded = load_embedded_versions();
        Ok(embedded.get("antigravity").cloned().unwrap_or_default())
    }

    /// Get combined Antigravity CLI version list with status
    pub fn get_antigravity_version_list(&self, installed: Option<&str>) -> Vec<VersionInfo> {
        let available = self
            .get_antigravity_available_versions()
            .unwrap_or_default();

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

    /// Install Antigravity CLI.
    ///
    /// Antigravity (the `agy` binary) has no public npm or GitHub-releases
    /// distribution. The only stable channels are the AUR `antigravity-cli`
    /// package (for Arch users, via yay/paru) and the antigravity.google
    /// download page.
    ///
    /// Previously this returned a hard-coded "managed by the system package
    /// manager (pacman/yay)" error. That was misleading on every non-Arch
    /// system and unhelpful even on Arch (the user has to read the error,
    /// look up the package name, and run yay themselves). Now: if an AUR
    /// helper is on PATH, drive it; otherwise return an honest, actionable
    /// error pointing at antigravity.google.
    ///
    /// `version` is accepted to match the trait but is ignored — AUR ships
    /// "whatever's current".
    pub fn install_antigravity_version_streaming(
        &self,
        _version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        let helper = ["yay", "paru"]
            .iter()
            .find(|h| Command::new(*h).arg("--version").output().is_ok());

        let Some(helper) = helper else {
            let msg = "Antigravity CLI has no npm/GitHub release channel. \
                       Install via your distro's AUR helper (yay/paru — package \
                       `antigravity-cli`) or download from https://antigravity.google";
            let _ = log_tx.send(msg.to_string());
            return Ok(InstallResult {
                success: false,
                stdout: String::new(),
                stderr: msg.to_string(),
                error: Some(msg.to_string()),
            });
        };

        let _ = log_tx.send(format!(
            "Running: {} -S --noconfirm --needed antigravity-cli",
            helper
        ));
        let mut cmd = Command::new(helper);
        cmd.args(["-S", "--noconfirm", "--needed", "antigravity-cli"]);
        let (ok, stdout, stderr) = Self::run_streaming(&mut cmd, &log_tx)?;

        Ok(InstallResult {
            success: ok,
            stdout,
            stderr: stderr.clone(),
            error: if ok {
                None
            } else if stderr.contains("sudo") || stderr.contains("password") {
                Some(format!(
                    "{} -S failed (sudo prompt blocked — run `{} -S antigravity-cli` interactively): {}",
                    helper, helper, stderr
                ))
            } else {
                Some(format!("{} -S antigravity-cli failed: {}", helper, stderr))
            },
        })
    }

}
