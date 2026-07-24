use std::fs;
use std::io;

/// Detect the current process's (arch, os) pair, normalized to the names
/// most commonly used in GitHub release asset filenames.
///
/// Returns the canonical pair; aliases are handled by the resolver. Linux
/// is `linux`, macOS is `macos`, x86_64 is `x86_64`, aarch64 is `aarch64`.
/// Returns None on platforms we don't support yet (Windows, freebsd, …).
pub(super) fn detect_arch_os() -> Option<(&'static str, &'static str)> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Some(("x86_64", "linux"));
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Some(("aarch64", "linux"));
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Some(("x86_64", "macos"));
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Some(("aarch64", "macos"));
    }
    #[allow(unreachable_code)]
    None
}

/// Pick an asset name from `available` for a custom agent's release.
///
/// Two shapes per the #338 design call:
///
/// - **Shape A (template):** if `asset_template` is Some, substitutes
///   `{name}`, `{arch}`, `{os}`, `{version}`, `{tag}` and uses the result
///   verbatim. Returns `Some(rendered)` if that exact string is in the
///   release's asset list, else `None` so the caller can surface the
///   available-asset list to the user.
/// - **Shape B (convention):** walks a small set of `<name>-<arch>-<os>[.ext]`
///   patterns with sensible architecture/OS aliases (amd64↔x86_64,
///   arm64↔aarch64, darwin↔macos) and the most common archive extensions
///   (`.tar.gz`, `.tgz`, `.zip`, plain). The first pattern that matches an
///   actual asset name in `available` wins.
///
/// Pure function — no I/O, no env reads — so the picker logic is exhaustively
/// unit-testable without spinning up a real release fixture.
pub(crate) fn pick_asset_name(
    name: &str,
    asset_template: Option<&str>,
    arch: &str,
    os: &str,
    version: &str,
    tag: &str,
    available: &[String],
) -> Option<String> {
    if let Some(template) = asset_template {
        let rendered = template
            .replace("{name}", name)
            .replace("{arch}", arch)
            .replace("{os}", os)
            .replace("{version}", version)
            .replace("{tag}", tag);
        return if available.iter().any(|a| a == &rendered) {
            Some(rendered)
        } else {
            None
        };
    }

    let arch_alts: &[&str] = match arch {
        "x86_64" => &["x86_64", "amd64"],
        "aarch64" => &["aarch64", "arm64"],
        _ => return None,
    };
    let os_alts: &[&str] = match os {
        "macos" => &["macos", "darwin"],
        "linux" => &["linux"],
        _ => return None,
    };
    let exts = ["", ".tar.gz", ".tgz", ".zip"];

    for ext in exts {
        for a in arch_alts {
            for o in os_alts {
                for candidate in [
                    format!("{}-{}-{}{}", name, a, o, ext),
                    format!("{}-{}-{}{}", name, o, a, ext),
                    format!("{}_{}_{}{}", name, a, o, ext),
                    format!("{}_{}_{}{}", name, o, a, ext),
                ] {
                    if available.iter().any(|x| x == &candidate) {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    // Universal binary fallback (no arch/os in the name)
    for ext in exts {
        let bare = format!("{}{}", name, ext);
        if available.iter().any(|x| x == &bare) {
            return Some(bare);
        }
    }
    None
}

/// Atomically install a binary from `src` to `dst`.
///
/// Copies to a temp file in the *same directory* as `dst` (so the final
/// `rename` is atomic on the same filesystem), marks it executable, then
/// renames it into place. This avoids two failure modes of a direct
/// `fs::copy(src, dst)`:
///
///   1. A crash/interrupt mid-copy leaves a truncated, executable-looking
///      binary at the canonical path (a subsequent run of a corrupt agent
///      binary, not a clean re-download).
///   2. `ETXTBSY` ("Text file busy") when `dst` is a currently-running
///      executable — `fs::copy` opens `dst` for writing and fails, whereas
///      `rename` swaps the directory entry to a fresh inode. Existing
///      references to the old binary (a running process, a hard link) keep
///      seeing the old inode; new lookups see the new one.
///
/// Returns the number of bytes copied. On any error the temp file is
/// removed so a failed install never leaks a partial file into the
/// install directory.
///
/// Note: unlike a direct `fs::copy`, if `dst` is a symlink this replaces the
/// link with a regular file (rename swaps the directory entry) rather than
/// writing through to the link target — the intended behavior for an install
/// path.
pub(crate) fn atomic_install_binary(
    src: &std::path::Path,
    dst: &std::path::Path,
) -> io::Result<u64> {
    let dir = dst.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "install path has no parent directory",
        )
    })?;
    fs::create_dir_all(dir)?;

    let file_name = dst.file_name().and_then(|n| n.to_str()).unwrap_or("binary");
    let tmp_prefix = format!(".{file_name}.unleash-install.");

    // Self-heal: sweep temps a previous hard-killed install (SIGKILL between
    // copy and rename) leaked here. Each temp carries the writer's pid as its
    // suffix; skip any whose pid is still a live process so we never delete a
    // *concurrent* install's in-flight temp (that racer's rename would then hit
    // ENOENT). On non-Linux the `/proc` probe just returns false and the temp
    // is treated as stale — a benign race, since same-binary concurrent
    // installs are operator error and the atomic rename still protects `dst`.
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(rest) = name.to_str().and_then(|n| n.strip_prefix(&tmp_prefix)) else {
                continue;
            };
            let pid_is_live = rest
                .parse::<u32>()
                .is_ok_and(|pid| std::path::Path::new(&format!("/proc/{pid}")).exists());
            if !pid_is_live {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    // Temp file in the destination directory (guarantees same filesystem, so
    // the rename below is atomic rather than a cross-device copy). The pid
    // suffix keeps concurrent unleash installs from colliding on the same name
    // and lets the sweep above tell a live racer's temp from a stale one.
    let tmp = dir.join(format!("{tmp_prefix}{}", std::process::id()));

    let staged = (|| -> io::Result<u64> {
        let n = fs::copy(src, &tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
        }
        fs::rename(&tmp, dst)?;
        Ok(n)
    })();

    if staged.is_err() {
        // Best-effort cleanup; the real error is the one we return.
        let _ = fs::remove_file(&tmp);
    }
    staged
}

/// After extracting an archive, find the agent binary and copy it to its
/// final install path. Searches breadth-first:
///   1. `<tmp_dir>/<binary>` exact match
///   2. any `<binary>` file in any subdirectory (e.g. archives that wrap
///      the binary in a versioned dir like `aider-0.50.0/aider`)
///
/// Returns the io::Result of the final install. The result is already
/// executable (0o755); callers do not need a subsequent chmod.
pub(super) fn install_extracted_binary(
    tmp_dir: &std::path::Path,
    binary: &str,
    install_path: &std::path::Path,
) -> io::Result<u64> {
    // Direct hit
    let direct = tmp_dir.join(binary);
    if direct.exists() && direct.is_file() {
        return atomic_install_binary(&direct, install_path);
    }
    // Walk one level for archives like `<repo>-<version>/<binary>`
    for entry in fs::read_dir(tmp_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join(binary);
            if candidate.exists() && candidate.is_file() {
                return atomic_install_binary(&candidate, install_path);
            }
        }
    }
    Err(io::Error::other(format!(
        "Could not find binary '{}' in extracted archive at {}. \
         The archive layout may not be supported — try setting an explicit \
         install location or extract manually.",
        binary,
        tmp_dir.display()
    )))
}
