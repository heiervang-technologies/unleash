use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;

use super::types::{InstallResult, VersionInfo};
use super::VersionManager;

impl VersionManager {
    // ── Hermes version management ───────────────────

    // ── Hermes ──────────────────────────────────────────────

    /// Get the Hermes version list. Hermes is installed via a curl bash
    /// script that always installs latest, so we expose a single "latest"
    /// entry. The installed flag reflects whether the binary is present.
    pub fn get_hermes_version_list(&self, installed: Option<&str>) -> Vec<VersionInfo> {
        vec![VersionInfo {
            version: "latest".to_string(),
            is_installed: installed.is_some(),
        }]
    }

    // ── OpenCode ────────────────────────────────────────────

    /// Get available OpenCode versions from npm registry.
    /// OpenCode is distributed via npm (`opencode-ai` package). GitHub releases
    /// for `opencode-ai/opencode` use a different versioning scheme (0.0.x) and
    /// should not be mixed with npm versions (1.x.x).

    pub fn install_hermes_version_streaming(
        &self,
        version: &str,
        log_tx: mpsc::Sender<String>,
    ) -> io::Result<InstallResult> {
        if version != "latest" {
            let _ = log_tx.send(format!(
                "Note: Hermes installer always installs the latest version; the requested version '{}' is ignored.",
                version
            ));
        }

        // The installer's update path runs `git pull --ff-only` and aborts
        // when the local clone has diverged from origin/main (e.g. after an
        // upstream squash/rebase). We hit this exact failure mode in the
        // wild: the local checkout had 1 stale commit on top of an older
        // main, so install.sh died with "Not possible to fast-forward".
        // Reset the clone to origin/<branch> before invoking the installer
        // so the ff-only pull always succeeds. Uncommitted local edits are
        // still preserved — the installer's own stash logic handles those.
        if let Some(dir) = Self::hermes_install_dir() {
            let branch = std::env::var("HERMES_BRANCH").unwrap_or_else(|_| "main".to_string());
            let tx = log_tx.clone();
            Self::reset_diverged_hermes_clone(&dir, &branch, &mut |msg| {
                let _ = tx.send(msg);
            });
        }

        let _ = log_tx.send(
            "Running: curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash -s -- --skip-setup".to_string(),
        );

        // --skip-setup bypasses the interactive wizard; null stdin prevents
        // the installer from reading /dev/tty for prompts.
        let (ok, stdout, stderr) = Self::run_streaming(
            Command::new("bash")
                .args([
                    "-c",
                    "curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash -s -- --skip-setup",
                ])
                .stdin(Stdio::null()),
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
                    "Failed to install Hermes Agent v{}: {}",
                    version, stderr
                ))
            },
        })
    }

    /// Resolve the hermes install directory the same way install.sh does:
    /// `HERMES_INSTALL_DIR` env override → `$HOME/.hermes/hermes-agent`.
    pub(crate) fn hermes_install_dir() -> Option<PathBuf> {
        std::env::var("HERMES_INSTALL_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".hermes/hermes-agent")))
    }

    /// Reset the hermes clone to `origin/<branch>` if HEAD has diverged.
    ///
    /// install.sh's `git pull --ff-only` aborts on divergence; we paper over
    /// that by hard-resetting to the upstream branch tip first. The
    /// installer then sees a clean fast-forward (no-op or a normal pull).
    /// No-op when the directory is missing or not a git checkout — fresh
    /// installs fall through to the normal clone path.
    ///
    /// Progress messages are routed through `log` so callers can wire them
    /// into a TUI channel (`unleash` TUI install panel) or stderr (`unleash
    /// agents update` CLI path).
    pub(crate) fn reset_diverged_hermes_clone(
        dir: &std::path::Path,
        branch: &str,
        log: &mut dyn FnMut(String),
    ) {
        if !dir.join(".git").exists() {
            return;
        }
        let remote_ref = format!("origin/{}", branch);

        let fetch_ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["fetch", "origin", branch])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !fetch_ok {
            // Network failure or no remote yet — let the installer handle it.
            return;
        }

        // `git merge-base --is-ancestor HEAD origin/<branch>` exits 0 when
        // HEAD is reachable from upstream (clean ff possible), nonzero
        // otherwise. We only reset on the nonzero case.
        let is_ancestor = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["merge-base", "--is-ancestor", "HEAD", &remote_ref])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(true);
        if is_ancestor {
            return;
        }

        log(format!(
            "Detected divergent hermes checkout at {} — resetting to {} so install can fast-forward",
            dir.display(),
            remote_ref
        ));
        let reset = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["reset", "--hard", &remote_ref])
            .stdin(Stdio::null())
            .output();
        match reset {
            Ok(out) if out.status.success() => {
                log(format!("Reset clone to {}", remote_ref));
            }
            Ok(out) => {
                log(format!(
                    "git reset failed (status {}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
            Err(e) => {
                log(format!("git reset could not run: {}", e));
            }
        }
    }

}
