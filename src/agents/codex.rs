//! Install Codex and its protocol-coupled Code Mode host as one release.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const HOST: &str = "codex-code-mode-host";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

fn platform() -> io::Result<String> {
    let arch = std::env::consts::ARCH;
    let suffix = match (arch, std::env::consts::OS) {
        ("x86_64" | "aarch64", "linux") => "unknown-linux-musl",
        ("x86_64" | "aarch64", "macos") => "apple-darwin",
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "No Codex prebuilt for this platform",
            ))
        }
    };
    Ok(format!("{arch}-{suffix}"))
}

fn run(command: &mut Command) -> io::Result<Vec<u8>> {
    let output = command.output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{} failed: {}",
            command.get_program().to_string_lossy(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(output.stdout)
}

pub(crate) fn install_release(
    version: Option<&str>,
    install_dir: &Path,
    mut log: impl FnMut(&str),
) -> io::Result<String> {
    let triple = platform()?;
    let release_path = match version {
        Some(version)
            if !version.is_empty()
                && version
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c)) =>
        {
            format!("tags/rust-v{version}")
        }
        Some(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid Codex version",
            ))
        }
        None => "latest".to_owned(),
    };
    log("Fetching Codex release metadata...");
    let metadata = run(Command::new("curl").args([
        "-fsSL",
        "-H",
        "Accept: application/vnd.github+json",
        &format!("https://api.github.com/repos/openai/codex/releases/{release_path}"),
    ]))?;
    let release: Release = serde_json::from_slice(&metadata)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if let Some(version) = version {
        if release.tag_name != format!("rust-v{version}") {
            return Err(io::Error::other(
                "Codex release tag does not match requested version",
            ));
        }
    }
    install_assets(
        &release,
        &triple,
        install_dir,
        &mut log,
        |asset, archive| {
            run(Command::new("curl")
                .args(["-fsSL", "-o"])
                .arg(archive)
                .arg(&asset.browser_download_url))?;
            Ok(())
        },
    )?;
    Ok(release.tag_name.trim_start_matches("rust-v").to_owned())
}

fn install_assets(
    release: &Release,
    triple: &str,
    install_dir: &Path,
    log: &mut impl FnMut(&str),
    mut download: impl FnMut(&Asset, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let cli_name = format!("codex-{triple}.tar.gz");
    let host_name = format!("{HOST}-{triple}.tar.gz");
    let cli = release
        .assets
        .iter()
        .find(|a| a.name == cli_name)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                format!("No {cli_name} in {}", release.tag_name),
            )
        })?;
    let host = release.assets.iter().find(|a| a.name == host_name);
    // Absence is legacy only when the release publishes no host at all. A
    // missing platform asset or failed download must not silently keep an old host.
    if host.is_none()
        && release
            .assets
            .iter()
            .any(|a| a.name.starts_with(&format!("{HOST}-")))
    {
        return Err(io::Error::other(format!(
            "Release {} is missing {host_name}",
            release.tag_name
        )));
    }
    let temp = tempfile::Builder::new()
        .prefix("unleash-codex-download-")
        .tempdir()?;
    let mut binaries = Vec::new();
    for (asset, binary) in std::iter::once((cli, "codex")).chain(host.map(|a| (a, HOST))) {
        log(&format!(
            "Downloading {} from {}...",
            asset.name, release.tag_name
        ));
        let archive = temp.path().join(&asset.name);
        download(asset, &archive)?;
        if let Some(digest) = &asset.digest {
            let expected = digest
                .strip_prefix("sha256:")
                .ok_or_else(|| io::Error::other("Unsupported Codex asset digest"))?;
            let mut hasher = Sha256::new();
            io::copy(&mut fs::File::open(&archive)?, &mut hasher)?;
            if format!("{:x}", hasher.finalize()) != expected {
                return Err(io::Error::other(format!(
                    "Checksum mismatch for {}",
                    asset.name
                )));
            }
        }
        // Extract just the expected binary into its own directory, not every
        // archive member. Older archives used the unsuffixed name.
        let entries = run(Command::new("tar").arg("tzf").arg(&archive))?;
        let suffixed = format!("{binary}-{triple}");
        let member = String::from_utf8_lossy(&entries)
            .lines()
            .find(|entry| [binary, &suffixed].contains(&entry.trim_start_matches("./")))
            .map(str::to_owned)
            .ok_or_else(|| {
                io::Error::other(format!("{} contains no {binary} binary", asset.name))
            })?;
        let extracted = temp.path().join(binary);
        fs::create_dir(&extracted)?;
        run(Command::new("tar")
            .arg("xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&extracted)
            .arg("--")
            .arg(&member))?;
        let path = extracted.join(member);
        if !fs::symlink_metadata(&path)?.file_type().is_file() || fs::metadata(&path)?.len() == 0 {
            return Err(io::Error::other(format!(
                "Invalid {binary} binary in {}",
                asset.name
            )));
        }
        binaries.push(path);
    }
    if host.is_none() {
        log("This release has no Code Mode host; removing any previously installed host.");
    }
    log("Installing Codex release binaries...");
    install_bundle(
        &binaries[0],
        binaries.get(1).map(PathBuf::as_path),
        install_dir,
    )
}

/// Stage both executables and backups before replacing either destination.
/// Each rename is atomic (also safe for running binaries); the two renames
/// are not a system-wide atomic switch for already-running Codex sessions.
pub(crate) fn install_bundle(
    cli: &Path,
    host: Option<&Path>,
    install_dir: &Path,
) -> io::Result<()> {
    install_bundle_with(cli, host, install_dir, |from, to| fs::rename(from, to))
}

fn install_bundle_with(
    cli: &Path,
    host: Option<&Path>,
    install_dir: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    fs::create_dir_all(install_dir)?;
    // Serialize all unleash Codex installers, including the source fallback.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(install_dir.join(".unleash-codex-install.lock"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: lock owns a valid descriptor for the duration of this install.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let _lock = lock;
    let stage = tempfile::Builder::new()
        .prefix(".unleash-codex-")
        .tempdir_in(install_dir)?;
    let entries = [(HOST, host), ("codex", Some(cli))];
    let mut backups = Vec::new();
    for (name, source) in &entries {
        if let Some(source) = source {
            if !fs::metadata(source)?.is_file() || fs::metadata(source)?.len() == 0 {
                return Err(io::Error::other(format!("Invalid {name} executable")));
            }
            let target = stage.path().join(name);
            fs::copy(source, &target)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
            }
        }
        let destination = install_dir.join(name);
        let backup = stage.path().join(format!("{name}.backup"));
        match fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.is_dir() => {
                return Err(io::Error::other(format!(
                    "{} is a directory",
                    destination.display()
                )))
            }
            Ok(_) => {
                // Preserve symlinks and the inode of any running executable.
                fs::hard_link(&destination, &backup)?;
                backups.push(Some(backup));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => backups.push(None),
            Err(e) => return Err(e),
        }
    }
    for (index, (name, source)) in entries.iter().enumerate() {
        let destination = install_dir.join(name);
        let result = if source.is_some() {
            rename(&stage.path().join(name), &destination)
        } else {
            remove_if_present(&destination)
        };
        if let Err(error) = result {
            for previous in (0..index).rev() {
                let destination = install_dir.join(entries[previous].0);
                let rollback = match &backups[previous] {
                    Some(backup) => fs::rename(backup, destination),
                    None => remove_if_present(&destination),
                };
                if let Err(rollback_error) = rollback {
                    let recovery = stage.keep();
                    return Err(io::Error::other(format!(
                        "{error}; rollback failed: {rollback_error}; backups retained at {}",
                        recovery.display()
                    )));
                }
            }
            return Err(error);
        }
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// Older source trees predate the host package; do not reuse a stale build
/// output in that case. Read the package name instead of assuming it.
pub(crate) fn source_host_package(source: &Path) -> io::Result<Option<String>> {
    let manifest = source.join("code-mode-host/Cargo.toml");
    let contents = match fs::read_to_string(manifest) {
        Ok(contents) => contents,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let parsed: toml::Value = toml::from_str(&contents).map_err(io::Error::other)?;
    parsed
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(|name| Some(name.to_owned()))
        .ok_or_else(|| io::Error::other("Code Mode host manifest has no package name"))
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
