use std::ffi::OsString;
use std::io::{self, BufRead};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use super::compare::version_compare;
use super::types::ChecksumResult;

#[derive(Default, Clone)]
pub struct VersionManager {
    /// When true, `install_version()` and related paths skip native binary
    /// downloads (and fall back to npm-only when applicable). Used exclusively
    /// by tests to avoid overwriting the user's real installation. The
    /// UNLEASH_SKIP_NATIVE_INSTALL env var flips the same switch; this field
    /// lets tests opt in without mutating process-global state (which is
    /// `unsafe` in modern Rust and racy under parallel test execution).
    skip_native_download: bool,

    /// Optional `PATH` override applied to every subprocess this manager
    /// spawns. Used by tests to inject mock `npm` / `claude` binaries from a
    /// `tempdir` without mutating the parent process's environment (which
    /// would race with other tests under `cargo test`'s parallel scheduler).
    /// Production paths leave this `None`, preserving the previous behavior of
    /// inheriting the parent's `PATH` verbatim.
    command_path_override: Option<OsString>,
}

impl VersionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an explicit `PATH` override applied to every subprocess this manager
    /// spawns. Used to inject `npm` paths dynamically without mutating the
    /// parent process's environment (which is racy under multithreaded updates).
    pub fn with_command_path_override(mut self, path: impl Into<std::ffi::OsString>) -> Self {
        self.command_path_override = Some(path.into());
        self
    }

    /// Constructor for tests: disables real native-binary downloads so tests
    /// don't overwrite the developer's installed agent CLIs. Prefer this over
    /// setting UNLEASH_SKIP_NATIVE_INSTALL from a test body.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_for_test() -> Self {
        Self {
            skip_native_download: true,
            command_path_override: None,
        }
    }

    /// Test constructor that ALSO injects a custom `PATH` for every subprocess
    /// this manager spawns. Use to point `npm` / `claude` lookups at a mock
    /// directory inside a `tempdir`. The override is per-`Command` (via
    /// `Command::env`), so it never mutates the parent process's environment
    /// and never races other parallel tests.
    #[cfg(test)]
    pub fn new_for_test_with_path(path: impl Into<OsString>) -> Self {
        Self {
            skip_native_download: true,
            command_path_override: Some(path.into()),
        }
    }

    pub(super) fn should_skip_native_download(&self) -> bool {
        self.skip_native_download || std::env::var("UNLEASH_SKIP_NATIVE_INSTALL").is_ok()
    }

    /// Build a `Command` for the given program, applying the test-only `PATH`
    /// override (if any). In production builds the override is always `None`
    /// and this is functionally identical to `Command::new(program)`.
    pub(super) fn command(&self, program: &str) -> Command {
        let mut cmd = Command::new(program);
        if let Some(path) = self.command_path_override.as_ref() {
            cmd.env("PATH", path);
        }
        cmd
    }

    // ── NPM utilities ─────────────────────────────

    pub fn has_npm() -> bool {
        Self::default().has_npm_for_self()
    }

    /// Instance variant of [`has_npm`] that respects the manager's optional
    /// `PATH` override. Production callers see identical behavior to the
    /// static method; tests can point this at a mock-`npm` `tempdir` without
    /// mutating the process environment.
    pub fn has_npm_for_self(&self) -> bool {
        self.command("npm")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Query the npm registry HTTP API for available versions of a package.
    /// Uses curl — no npm binary required.
    pub(super) fn query_npm_registry_versions(
        package: &str,
        limit: usize,
    ) -> io::Result<Vec<String>> {
        // npm registry URL: https://registry.npmjs.org/<package>
        // The response has a "versions" object with version strings as keys.
        // We use the abbreviated metadata endpoint for speed.
        let url = format!("https://registry.npmjs.org/{}", package);
        let output = Command::new("curl")
            .args([
                "-fsSL",
                "-H",
                "Accept: application/vnd.npm.install-v1+json",
                &url,
            ])
            .output()
            .map_err(|e| io::Error::other(format!("curl not found: {}", e)))?;

        if !output.status.success() {
            return Err(io::Error::other(format!(
                "Failed to query npm registry for {}",
                package
            )));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        // Parse the "versions" object and extract keys
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| io::Error::other(format!("Failed to parse registry response: {}", e)))?;

        let mut versions: Vec<String> = parsed
            .get("versions")
            .and_then(|v| v.as_object())
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        versions.sort_by(|a, b| version_compare(b, a));
        versions.truncate(limit);
        Ok(versions)
    }

    /// Check whether `npm install -g` needs `sudo` on this system.
    ///
    /// Returns `true` when the npm global prefix directory (e.g. `/usr/lib`)
    /// is not owned by the current user, which is the default on Arch Linux.
    /// The result is cached for the lifetime of the process since the npm
    /// prefix won't change mid-run.
    pub fn npm_global_needs_sudo() -> bool {
        Self::default().npm_global_needs_sudo_for_self()
    }

    /// Instance variant. When the manager has a test-only `PATH` override,
    /// the sudo probe is skipped entirely (mock `npm` shims in tempdirs are
    /// always owned by the test user). Otherwise behaves identically to the
    /// static [`npm_global_needs_sudo`] (including the process-wide cache).
    pub fn npm_global_needs_sudo_for_self(&self) -> bool {
        if self.command_path_override.is_some() {
            return false;
        }
        use std::sync::OnceLock;
        static NEEDS_SUDO: OnceLock<bool> = OnceLock::new();
        *NEEDS_SUDO.get_or_init(|| {
            let prefix = Command::new("npm")
                .args(["config", "get", "prefix"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                });

            match prefix {
                Some(p) => {
                    use std::os::unix::fs::MetadataExt;
                    let path = std::path::Path::new(&p);
                    let uid = nix::unistd::getuid().as_raw();
                    path.metadata().map(|m| m.uid() != uid).unwrap_or(false)
                }
                None => false,
            }
        })
    }

    /// Create a `Command` for npm global operations, prepending `sudo -n`
    /// (non-interactive) if the prefix is root-owned. Using `-n` avoids
    /// silent hangs when called from background threads where no TTY is
    /// available for a password prompt.
    pub fn npm_global_command() -> Command {
        Self::default().npm_global_command_for_self()
    }

    /// Instance variant of [`npm_global_command`]. When the manager carries a
    /// test-only `PATH` override, the returned `Command` always points at
    /// plain `npm` (resolved against that override) and skips sudo.
    pub fn npm_global_command_for_self(&self) -> Command {
        if self.npm_global_needs_sudo_for_self() {
            let mut cmd = self.command("sudo");
            cmd.args(["-n", "npm"]);
            cmd
        } else {
            self.command("npm")
        }
    }

    // ── Checksum verification ─────────────────────

    pub(super) fn extract_checksum_from_manifest(manifest: &str, platform: &str) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(manifest).ok()?;
        json.get(platform)?
            .get("checksum")?
            .as_str()
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
            .map(|s| s.to_string())
    }

    /// Verify SHA-256 checksum of a downloaded file against the manifest.
    pub(super) fn verify_checksum_for_file(
        file_path: &std::path::Path,
        manifest_url: &str,
        platform: &str,
    ) -> ChecksumResult {
        let manifest_output = match Command::new("curl").args(["-fsSL", manifest_url]).output() {
            Ok(o) if o.status.success() => o,
            _ => return ChecksumResult::Failed("manifest download failed".into()),
        };

        let manifest = String::from_utf8_lossy(&manifest_output.stdout);
        let expected = match Self::extract_checksum_from_manifest(&manifest, platform) {
            Some(e) => e,
            None => return ChecksumResult::Failed("no checksum in manifest".into()),
        };

        let checksum_cmd = if cfg!(target_os = "macos") {
            "shasum"
        } else {
            "sha256sum"
        };
        let mut cmd = Command::new(checksum_cmd);
        if cfg!(target_os = "macos") {
            cmd.args(["-a", "256"]);
        }
        cmd.arg(file_path.to_str().unwrap_or(""));

        match cmd.output() {
            Ok(o) if o.status.success() => {
                let actual = String::from_utf8_lossy(&o.stdout);
                let actual_checksum = actual.split_whitespace().next().unwrap_or("").to_string();
                if actual_checksum == expected {
                    ChecksumResult::Verified
                } else {
                    ChecksumResult::Mismatch {
                        expected,
                        actual: actual_checksum,
                    }
                }
            }
            _ => ChecksumResult::Failed("sha256sum/shasum execution failed".into()),
        }
    }

    // ── Streaming helpers ─────────────────────────

    pub(super) fn stream_child_output(
        child: &mut std::process::Child,
        log_tx: &mpsc::Sender<String>,
    ) -> (String, String) {
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let tx_clone = log_tx.clone();

        let stdout_thread = thread::spawn(move || {
            let mut acc = String::new();
            if let Some(pipe) = stdout_pipe {
                for line in io::BufReader::new(pipe).lines().map_while(Result::ok) {
                    let _ = tx_clone.send(line.clone());
                    acc.push_str(&line);
                    acc.push('\n');
                }
            }
            acc
        });

        let mut stderr_acc = String::new();
        if let Some(pipe) = stderr_pipe {
            for line in io::BufReader::new(pipe).lines().map_while(Result::ok) {
                let _ = log_tx.send(line.clone());
                stderr_acc.push_str(&line);
                stderr_acc.push('\n');
            }
        }

        let stdout_acc = stdout_thread.join().unwrap_or_default();
        (stdout_acc, stderr_acc)
    }

    /// Run a command with streaming output, returning (success, stdout, stderr).
    /// stdin is forced to /dev/null so installers (npm, post-install scripts,
    /// node-gyp, etc.) never block on inherited TTY input. Users were
    /// reporting having to spam Enter to get pi/opencode installs to finish.
    pub(super) fn run_streaming(
        cmd: &mut Command,
        log_tx: &mpsc::Sender<String>,
    ) -> io::Result<(bool, String, String)> {
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (stdout, stderr) = Self::stream_child_output(&mut child, log_tx);
        let status = child.wait()?;
        Ok((status.success(), stdout, stderr))
    }
}
