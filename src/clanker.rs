//! First-class Clanker Code source installation and revision tracking.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) const REPOSITORY: &str = "heiervang-technologies/clanker-code";
pub(crate) const REPOSITORY_URL: &str =
    "https://github.com/heiervang-technologies/clanker-code.git";
pub(crate) const RELEASE_BRANCH: &str = "clanker";

const RECEIPT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallReceipt {
    schema_version: u32,
    repository: String,
    branch: String,
    revision: String,
    product_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallOutcome {
    pub revision: String,
    pub product_version: String,
    pub install_path: PathBuf,
}

pub(crate) fn revision_label(revision: &str) -> &str {
    revision.get(..12).unwrap_or(revision)
}

pub(crate) fn latest_revision() -> io::Result<String> {
    let output = Command::new("git")
        .args([
            "ls-remote",
            "--exit-code",
            REPOSITORY_URL,
            &format!("refs/heads/{RELEASE_BRANCH}"),
        ])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "failed to resolve {REPOSITORY}/{RELEASE_BRANCH}: {}. \
             Authenticate Git for the private Clanker Code repository and retry",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    parse_ls_remote(&output.stdout)
}

pub(crate) fn installed_revision() -> Option<String> {
    installed_receipt().map(|receipt| receipt.revision)
}

pub(crate) fn installed_product_version() -> Option<String> {
    installed_receipt().map(|receipt| receipt.product_version)
}

pub(crate) fn update_available(latest_revision: &str) -> bool {
    installed_revision().as_deref() != Some(latest_revision)
}

pub(crate) fn install_latest() -> io::Result<InstallOutcome> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Home dir not found"))?;
    install_latest_in(&home)
}

fn install_latest_in(home: &Path) -> io::Result<InstallOutcome> {
    ensure_build_dependencies()?;

    let cache_root = home.join(".cache/unleash");
    let source_dir = cache_root.join("clanker-source");
    let target_dir = cache_root.join("clanker-target");
    fs::create_dir_all(&cache_root)?;

    prepare_source_checkout(&source_dir)?;
    let revision = git_stdout(&source_dir, &["rev-parse", "HEAD"])?;
    validate_revision(&revision)?;

    let codex_rs = source_dir.join("codex-rs");
    if !codex_rs.join("cli/Cargo.toml").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Clanker Code checkout is missing {}",
                codex_rs.join("cli/Cargo.toml").display()
            ),
        ));
    }

    let status = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--release",
            "-p",
            "codex-cli",
            "--bin",
            "clanker",
            "-j",
            "2",
        ])
        .current_dir(&codex_rs)
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("CARGO_PROFILE_RELEASE_LTO", "off")
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "16")
        .status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "failed to build Clanker Code revision {}",
            revision_label(&revision)
        )));
    }

    let built_binary = target_dir.join("release/clanker");
    if !built_binary.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Clanker Code build succeeded but {} was not produced",
                built_binary.display()
            ),
        ));
    }

    strip_binary_if_available(&built_binary)?;

    let product_version = probe_product_version(&built_binary)?;
    let install_path = install_path_for(home);
    crate::agents::atomic_install_binary(&built_binary, &install_path)?;

    let receipt = InstallReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        repository: REPOSITORY.to_string(),
        branch: RELEASE_BRANCH.to_string(),
        revision: revision.clone(),
        product_version: product_version.clone(),
    };
    write_receipt(&receipt_path_for(home), &receipt)?;

    Ok(InstallOutcome {
        revision,
        product_version,
        install_path,
    })
}

fn ensure_build_dependencies() -> io::Result<()> {
    for binary in ["git", "cargo"] {
        if which::which(binary).is_err() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "{binary} is required to install Clanker Code from {REPOSITORY}/{RELEASE_BRANCH}"
                ),
            ));
        }
    }
    Ok(())
}

fn prepare_source_checkout(source_dir: &Path) -> io::Result<()> {
    if source_dir.join(".git").is_dir() {
        let origin = git_stdout(source_dir, &["remote", "get-url", "origin"])?;
        if !is_expected_origin(&origin) {
            return Err(io::Error::other(format!(
                "refusing to update unmanaged checkout {} with origin {:?}; expected {}",
                source_dir.display(),
                origin,
                REPOSITORY_URL
            )));
        }

        run_git(
            source_dir,
            &["fetch", "--prune", "--depth", "1", "origin", RELEASE_BRANCH],
        )?;
        run_git(
            source_dir,
            &["checkout", "--detach", "--force", "FETCH_HEAD"],
        )?;
        return Ok(());
    }

    if source_dir.exists() {
        return Err(io::Error::other(format!(
            "refusing to replace non-Git path {}",
            source_dir.display()
        )));
    }

    let parent = source_dir.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Clanker source path has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--branch",
            RELEASE_BRANCH,
            "--single-branch",
            REPOSITORY_URL,
        ])
        .arg(source_dir)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "failed to clone {REPOSITORY}: {}. \
             Authenticate Git for the private Clanker Code repository and retry",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn run_git(current_dir: &Path, args: &[&str]) -> io::Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(current_dir)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            current_dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn git_stdout(current_dir: &Path, args: &[&str]) -> io::Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(current_dir)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            current_dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn strip_binary_if_available(binary: &Path) -> io::Result<()> {
    let Ok(strip) = which::which("strip") else {
        return Ok(());
    };
    let mut command = Command::new(strip);
    #[cfg(target_os = "macos")]
    command.arg("-x");
    #[cfg(not(target_os = "macos"))]
    command.arg("--strip-all");
    let output = command.arg(binary).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "failed to strip Clanker Code binary: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn probe_product_version(binary: &Path) -> io::Result<String> {
    let output = Command::new(binary).arg("--version").output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "new Clanker Code binary failed its version probe: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = line.strip_prefix("Clanker Code ").ok_or_else(|| {
        io::Error::other(format!("unexpected Clanker Code version output: {line}"))
    })?;
    if version.is_empty() {
        return Err(io::Error::other(
            "Clanker Code version output did not contain a version",
        ));
    }
    Ok(version.to_string())
}

fn receipt_path_for(home: &Path) -> PathBuf {
    home.join(".local/share/unleash/clanker-install.json")
}

fn install_path_for(home: &Path) -> PathBuf {
    home.join(".local/bin/clanker")
}

fn installed_receipt() -> Option<InstallReceipt> {
    let home = dirs::home_dir()?;
    installed_receipt_for(&home)
}

/// Return a receipt only when it still describes the binary Unleash owns.
///
/// The receipt is revision authority, not merely a cache: a missing or
/// replaced binary must never make an update check report "up to date".
fn installed_receipt_for(home: &Path) -> Option<InstallReceipt> {
    let receipt = read_receipt(&receipt_path_for(home))?;
    let binary = install_path_for(home);
    if !binary.is_file() {
        return None;
    }
    let product_version = probe_product_version(&binary).ok()?;
    (product_version == receipt.product_version).then_some(receipt)
}

fn write_receipt(path: &Path, receipt: &InstallReceipt) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Clanker install receipt has no parent",
        )
    })?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".clanker-install.{}.tmp", std::process::id()));
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn read_receipt(path: &Path) -> Option<InstallReceipt> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 16 * 1024 {
        return None;
    }
    let receipt: InstallReceipt = serde_json::from_slice(&bytes).ok()?;
    if receipt.schema_version != RECEIPT_SCHEMA_VERSION
        || receipt.repository != REPOSITORY
        || receipt.branch != RELEASE_BRANCH
        || validate_revision(&receipt.revision).is_err()
        || receipt.product_version.is_empty()
    {
        return None;
    }
    Some(receipt)
}

fn parse_ls_remote(stdout: &[u8]) -> io::Result<String> {
    let text = std::str::from_utf8(stdout)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let mut lines = text.lines();
    let line = lines
        .next()
        .ok_or_else(|| io::Error::other("Clanker Code branch did not resolve to a revision"))?;
    if lines.next().is_some() {
        return Err(io::Error::other(
            "Clanker Code branch resolution returned multiple revisions",
        ));
    }
    let mut fields = line.split_whitespace();
    let revision = fields
        .next()
        .ok_or_else(|| io::Error::other("Clanker Code revision was missing"))?;
    let reference = fields
        .next()
        .ok_or_else(|| io::Error::other("Clanker Code branch reference was missing"))?;
    if fields.next().is_some() || reference != format!("refs/heads/{RELEASE_BRANCH}") {
        return Err(io::Error::other(
            "Clanker Code branch resolution returned an unexpected reference",
        ));
    }
    validate_revision(revision)?;
    Ok(revision.to_string())
}

fn validate_revision(revision: &str) -> io::Result<()> {
    if revision.len() == 40
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "invalid Clanker Code Git revision {revision:?}"
        )))
    }
}

fn is_expected_origin(origin: &str) -> bool {
    matches!(
        origin.trim_end_matches('/').trim_end_matches(".git"),
        "https://github.com/heiervang-technologies/clanker-code"
            | "git@github.com:heiervang-technologies/clanker-code"
            | "ssh://git@github.com/heiervang-technologies/clanker-code"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVISION: &str = "40e7d1c0d9b0621d756eb14a5aa7735466aca0a9";

    #[test]
    fn parses_exact_release_branch_revision() {
        let output = format!("{REVISION}\trefs/heads/clanker\n");
        assert_eq!(parse_ls_remote(output.as_bytes()).unwrap(), REVISION);
        assert_eq!(revision_label(REVISION), "40e7d1c0d9b0");
    }

    #[test]
    fn rejects_ambiguous_or_noncanonical_revision_output() {
        assert!(parse_ls_remote(b"BAD\trefs/heads/clanker\n").is_err());
        assert!(parse_ls_remote(format!("{REVISION}\trefs/heads/main\n").as_bytes()).is_err());
        assert!(parse_ls_remote(
            format!("{REVISION}\trefs/heads/clanker\n{REVISION}\trefs/heads/main\n").as_bytes()
        )
        .is_err());
    }

    #[test]
    fn validates_supported_origin_forms() {
        assert!(is_expected_origin(REPOSITORY_URL));
        assert!(is_expected_origin(
            "git@github.com:heiervang-technologies/clanker-code.git"
        ));
        assert!(is_expected_origin(
            "ssh://git@github.com/heiervang-technologies/clanker-code"
        ));
        assert!(!is_expected_origin("https://github.com/openai/codex.git"));
    }

    #[test]
    fn receipt_round_trips_and_rejects_wrong_authority() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("clanker-install.json");
        let receipt = InstallReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            repository: REPOSITORY.to_string(),
            branch: RELEASE_BRANCH.to_string(),
            revision: REVISION.to_string(),
            product_version: "0.1.0+codex.0.143.0".to_string(),
        };
        write_receipt(&path, &receipt).unwrap();
        assert_eq!(read_receipt(&path), Some(receipt.clone()));

        let mut invalid = receipt;
        invalid.repository = "openai/codex".to_string();
        write_receipt(&path, &invalid).unwrap();
        assert_eq!(read_receipt(&path), None);
    }

    #[cfg(unix)]
    #[test]
    fn installed_receipt_requires_the_owned_matching_binary() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let receipt = InstallReceipt {
            schema_version: RECEIPT_SCHEMA_VERSION,
            repository: REPOSITORY.to_string(),
            branch: RELEASE_BRANCH.to_string(),
            revision: REVISION.to_string(),
            product_version: "0.1.0+codex.0.143.0".to_string(),
        };
        write_receipt(&receipt_path_for(temp.path()), &receipt).unwrap();

        assert_eq!(installed_receipt_for(temp.path()), None);

        let binary = install_path_for(temp.path());
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' 'Clanker Code 0.1.0+codex.0.143.0'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary, permissions).unwrap();

        assert_eq!(installed_receipt_for(temp.path()), Some(receipt.clone()));

        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' 'Clanker Code 0.1.0+codex.replaced'\n",
        )
        .unwrap();
        assert_eq!(installed_receipt_for(temp.path()), None);
    }
}
