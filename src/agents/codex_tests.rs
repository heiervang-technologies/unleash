use super::*;

const TRIPLE: &str = "x86_64-unknown-linux-musl";

struct Fixture {
    root: tempfile::TempDir,
    install: PathBuf,
    release: Release,
}

impl Fixture {
    fn new(host: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let install = root.path().join("bin");
        fs::create_dir(&install).unwrap();
        fs::write(install.join("codex"), "old cli").unwrap();
        fs::write(install.join(HOST), "old host").unwrap();
        let mut assets = Vec::new();
        for binary in std::iter::once("codex").chain(host.then_some(HOST)) {
            let name = format!("{binary}-{TRIPLE}");
            fs::write(
                root.path().join(&name),
                format!("#!/bin/sh\nprintf '{binary} 0.153.4\\n'\n"),
            )
            .unwrap();
            let archive = root.path().join(format!("{name}.tar.gz"));
            run(Command::new("tar")
                .arg("czf")
                .arg(&archive)
                .arg("-C")
                .arg(root.path())
                .arg(&name))
            .unwrap();
            let digest = format!("sha256:{:x}", Sha256::digest(fs::read(&archive).unwrap()));
            assets.push(Asset {
                name: format!("{name}.tar.gz"),
                browser_download_url: archive.to_str().unwrap().to_owned(),
                digest: Some(digest),
            });
        }
        Self {
            root,
            install,
            release: Release {
                tag_name: "rust-v0.153.4".into(),
                assets,
            },
        }
    }

    fn install(&self) -> io::Result<()> {
        install_assets(
            &self.release,
            TRIPLE,
            &self.install,
            &mut |_| {},
            |asset, destination| {
                fs::copy(&asset.browser_download_url, destination)?;
                Ok(())
            },
        )
    }

    fn assert_old_pair(&self) {
        assert_eq!(
            fs::read_to_string(self.install.join("codex")).unwrap(),
            "old cli"
        );
        assert_eq!(
            fs::read_to_string(self.install.join(HOST)).unwrap(),
            "old host"
        );
    }
}

#[test]
fn release_replaces_stale_host_and_cli_with_matching_executables() {
    let fixture = Fixture::new(true);
    fixture.install().unwrap();
    for binary in ["codex", HOST] {
        assert_eq!(
            run(&mut Command::new(fixture.install.join(binary))).unwrap(),
            format!("{binary} 0.153.4\n").as_bytes()
        );
    }
}

#[test]
fn fresh_install_includes_host() {
    let fixture = Fixture::new(true);
    fs::remove_dir_all(&fixture.install).unwrap();
    fixture.install().unwrap();
    assert!(fixture.install.join("codex").is_file());
    assert!(fixture.install.join(HOST).is_file());
}

#[test]
fn legacy_release_removes_stale_host() {
    let mut fixture = Fixture::new(false);
    // Older releases do not always publish GitHub digests.
    fixture.release.assets[0].digest = None;
    fixture.install().unwrap();
    assert!(!fixture.install.join(HOST).exists());
    assert_eq!(
        run(&mut Command::new(fixture.install.join("codex"))).unwrap(),
        b"codex 0.153.4\n"
    );
}

#[test]
fn missing_platform_host_is_not_treated_as_legacy() {
    let mut fixture = Fixture::new(true);
    fixture.release.assets[1].name = "codex-code-mode-host-aarch64-apple-darwin.tar.gz".into();
    assert!(fixture
        .install()
        .unwrap_err()
        .to_string()
        .contains("missing"));
    fixture.assert_old_pair();
}

#[test]
fn failed_host_download_preserves_pair() {
    let fixture = Fixture::new(true);
    let result = install_assets(
        &fixture.release,
        TRIPLE,
        &fixture.install,
        &mut |_| {},
        |asset, destination| {
            if asset.name.starts_with(HOST) {
                return Err(io::Error::other("network failure"));
            }
            fs::copy(&asset.browser_download_url, destination)?;
            Ok(())
        },
    );
    assert!(result.unwrap_err().to_string().contains("network failure"));
    fixture.assert_old_pair();
}

#[test]
fn failed_host_checksum_preserves_pair() {
    let mut fixture = Fixture::new(true);
    fixture.release.assets[1].digest = Some(format!("sha256:{}", "0".repeat(64)));
    assert!(fixture
        .install()
        .unwrap_err()
        .to_string()
        .contains("Checksum mismatch"));
    fixture.assert_old_pair();
}

#[test]
fn malformed_host_archive_preserves_pair() {
    let mut fixture = Fixture::new(true);
    // A valid CLI tarball is still not a valid helper tarball.
    fixture.release.assets[1].browser_download_url =
        fixture.release.assets[0].browser_download_url.clone();
    fixture.release.assets[1].digest = fixture.release.assets[0].digest.clone();
    assert!(fixture
        .install()
        .unwrap_err()
        .to_string()
        .contains("contains no"));
    fixture.assert_old_pair();
}

#[test]
fn missing_source_host_binary_preserves_pair() {
    let fixture = Fixture::new(true);
    let cli = fixture.root.path().join(format!("codex-{TRIPLE}"));
    assert!(install_bundle(
        &cli,
        Some(&fixture.root.path().join("missing")),
        &fixture.install
    )
    .is_err());
    fixture.assert_old_pair();
}

#[test]
fn failed_cli_replacement_rolls_back_updated_or_removed_host() {
    for new_host in [true, false] {
        let fixture = Fixture::new(true);
        let cli = fixture.root.path().join(format!("codex-{TRIPLE}"));
        let host = fixture.root.path().join(format!("{HOST}-{TRIPLE}"));
        let result = install_bundle_with(
            &cli,
            new_host.then_some(host.as_path()),
            &fixture.install,
            |from, to| {
                if to.file_name().unwrap() == "codex" {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "injected rename failure",
                    ));
                }
                fs::rename(from, to)
            },
        );
        assert!(result.is_err());
        fixture.assert_old_pair();
    }
}

#[test]
fn failed_fresh_install_removes_new_host_on_rollback() {
    let fixture = Fixture::new(true);
    fs::remove_file(fixture.install.join(HOST)).unwrap();
    let cli = fixture.root.path().join(format!("codex-{TRIPLE}"));
    let host = fixture.root.path().join(format!("{HOST}-{TRIPLE}"));
    assert!(
        install_bundle_with(&cli, Some(&host), &fixture.install, |from, to| {
            if to.file_name().unwrap() == "codex" {
                return Err(io::Error::other("rename failed"));
            }
            fs::rename(from, to)
        })
        .is_err()
    );
    assert!(!fixture.install.join(HOST).exists());
    assert_eq!(
        fs::read_to_string(fixture.install.join("codex")).unwrap(),
        "old cli"
    );
}

#[cfg(unix)]
#[test]
fn failed_install_restores_host_symlink_without_modifying_target() {
    let fixture = Fixture::new(true);
    let external_host = fixture.root.path().join("external-host");
    fs::write(&external_host, "old host").unwrap();
    fs::remove_file(fixture.install.join(HOST)).unwrap();
    std::os::unix::fs::symlink(&external_host, fixture.install.join(HOST)).unwrap();
    let cli = fixture.root.path().join(format!("codex-{TRIPLE}"));
    let host = fixture.root.path().join(format!("{HOST}-{TRIPLE}"));
    assert!(
        install_bundle_with(&cli, Some(&host), &fixture.install, |from, to| {
            if to.file_name().unwrap() == "codex" {
                return Err(io::Error::other("rename failed"));
            }
            fs::rename(from, to)
        })
        .is_err()
    );
    assert_eq!(
        fs::read_link(fixture.install.join(HOST)).unwrap(),
        external_host
    );
    fixture.assert_old_pair();
}

#[test]
fn source_build_detects_host_package_and_legacy_trees() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(source_host_package(temp.path()).unwrap(), None);
    fs::create_dir(temp.path().join("code-mode-host")).unwrap();
    let manifest = temp.path().join("code-mode-host/Cargo.toml");
    fs::write(&manifest, "[package]\nname = 'codex-code-mode-host'\n").unwrap();
    assert_eq!(
        source_host_package(temp.path()).unwrap().as_deref(),
        Some(HOST)
    );
    fs::write(&manifest, "[package]\n").unwrap();
    assert!(source_host_package(temp.path()).is_err());
}
