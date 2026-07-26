use super::*;
use super::cache::merge_disk_and_fallback;
use std::process::Command;
use std::time::Instant;
use tempfile::TempDir;
use std::path::PathBuf;

    /// Build a self-contained "origin + clone" pair in a tempdir for the
    /// hermes divergence test. Returns (origin_dir, clone_dir).
    ///
    /// Tests must NEVER touch the real ~/.hermes checkout — every git op
    /// stays inside the tempdir.
    fn make_origin_and_clone(td: &TempDir) -> (PathBuf, PathBuf) {
        use std::fs;
        let origin = td.path().join("origin.git");
        let clone = td.path().join("clone");
        fs::create_dir_all(&origin).unwrap();

        // Bare origin repo with an initial commit on `main`.
        let work = td.path().join("seed");
        fs::create_dir_all(&work).unwrap();
        for args in [
            vec!["init", "--initial-branch=main", "-q"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
            vec!["commit", "--allow-empty", "-m", "initial", "-q"],
        ] {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&work)
                .args(&args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "seed setup failed: {:?}", args);
        }
        let ok = Command::new("git")
            .args(["clone", "--bare", "-q"])
            .arg(&work)
            .arg(&origin)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let ok = Command::new("git")
            .args(["clone", "-q"])
            .arg(&origin)
            .arg(&clone)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        for args in [
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            Command::new("git")
                .arg("-C")
                .arg(&clone)
                .args(&args)
                .status()
                .unwrap();
        }
        (origin, clone)
    }

    fn head_oid(repo: &std::path::Path) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn add_commit(repo: &std::path::Path, message: &str) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["commit", "--allow-empty", "-m", message, "-q"])
            .status()
            .unwrap()
            .success();
        assert!(ok);
    }

    #[test]
    fn reset_diverged_hermes_clone_is_noop_when_in_sync() {
        let td = TempDir::new().unwrap();
        let (_origin, clone) = make_origin_and_clone(&td);
        let before = head_oid(&clone);
        let mut logs: Vec<String> = Vec::new();
        VersionManager::reset_diverged_hermes_clone(&clone, "main", &mut |m| logs.push(m));
        // No divergence → HEAD unchanged.
        assert_eq!(head_oid(&clone), before);
        assert!(logs.is_empty(), "should not log when in sync: {:?}", logs);
    }

    #[test]
    fn reset_diverged_hermes_clone_resets_diverged_head() {
        let td = TempDir::new().unwrap();
        let (origin, clone) = make_origin_and_clone(&td);

        // Advance origin/main beyond what the clone has, by pushing a new
        // commit from a second worktree.
        let advance = td.path().join("advance");
        let ok = Command::new("git")
            .args(["clone", "-q"])
            .arg(&origin)
            .arg(&advance)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        for args in [
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test"],
        ] {
            Command::new("git")
                .arg("-C")
                .arg(&advance)
                .args(&args)
                .status()
                .unwrap();
        }
        add_commit(&advance, "upstream commit");
        let ok = Command::new("git")
            .arg("-C")
            .arg(&advance)
            .args(["push", "-q", "origin", "main"])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let upstream_tip = head_oid(&advance);

        // Now make the original clone diverge with its own local commit
        // (not present upstream) so HEAD is no longer an ancestor of origin/main.
        add_commit(&clone, "diverging local commit");
        let local_before = head_oid(&clone);
        assert_ne!(local_before, upstream_tip);

        let mut logs: Vec<String> = Vec::new();
        VersionManager::reset_diverged_hermes_clone(&clone, "main", &mut |m| logs.push(m));

        // HEAD should now match the upstream tip, not the previous local commit.
        let after = head_oid(&clone);
        assert_eq!(after, upstream_tip, "clone should be reset to origin/main");

        // And we should have emitted a clear log line so the user sees why
        // their local commit went away.
        assert!(
            logs.iter().any(|l| l.contains("Detected divergent")),
            "expected divergence log line, got: {:?}",
            logs
        );
    }

    #[test]
    fn reset_diverged_hermes_clone_skips_when_not_a_git_dir() {
        let td = TempDir::new().unwrap();
        // Pass a path that exists but has no `.git/` — should silently no-op
        // (fresh installs land here and the installer's clone path takes over).
        let mut logs: Vec<String> = Vec::new();
        VersionManager::reset_diverged_hermes_clone(td.path(), "main", &mut |m| logs.push(m));
        assert!(
            logs.is_empty(),
            "should not log when no .git/ present: {:?}",
            logs
        );
    }

    /// Cached `antigravity = ["2.0.1"]` is the bogus value earlier builds
    /// shipped — replace it with the in-binary fallback so the user's check
    /// stops claiming a phantom update is available.
    #[test]
    fn merge_disk_strips_legacy_antigravity_2_0_1_stub() {
        let disk = serde_json::json!({ "antigravity": ["2.0.1"] });
        let fallback = serde_json::json!({ "antigravity": ["1.1.0", "1.0.3"] });
        let merged = merge_disk_and_fallback(&disk, &fallback);
        assert_eq!(
            merged.get("antigravity"),
            Some(&vec!["1.1.0".to_string(), "1.0.3".to_string()])
        );
    }

    /// Refreshed caches also shipped the bogus value alongside a real
    /// Antigravity version. Treat the whole cached list as contaminated.
    #[test]
    fn merge_disk_strips_legacy_antigravity_2_0_1_mixed_cache() {
        let disk = serde_json::json!({ "antigravity": ["2.0.1", "1.0.3"] });
        let fallback = serde_json::json!({ "antigravity": ["1.1.0", "1.0.3"] });
        let merged = merge_disk_and_fallback(&disk, &fallback);
        assert_eq!(
            merged.get("antigravity"),
            Some(&vec!["1.1.0".to_string(), "1.0.3".to_string()])
        );
    }

    /// A real cached version list — e.g. once Google ships a 2.x agy — must
    /// pass through untouched. The migration is intentionally narrow.
    #[test]
    fn merge_disk_preserves_non_stub_antigravity_versions() {
        let disk = serde_json::json!({ "antigravity": ["2.5.0", "2.4.0"] });
        let fallback = serde_json::json!({ "antigravity": ["1.0.3"] });
        let merged = merge_disk_and_fallback(&disk, &fallback);
        assert_eq!(
            merged.get("antigravity"),
            Some(&vec!["2.5.0".to_string(), "2.4.0".to_string()])
        );
    }

    /// No disk cache at all → fall back to the embedded list. Regression
    /// guard for the install_only path that needs a valid `latest` to even
    /// reach the installer.
    #[test]
    fn merge_disk_falls_back_when_antigravity_missing_from_disk() {
        let disk = serde_json::json!({});
        let fallback = serde_json::json!({ "antigravity": ["1.0.3"] });
        let merged = merge_disk_and_fallback(&disk, &fallback);
        assert_eq!(merged.get("antigravity"), Some(&vec!["1.0.3".to_string()]));
    }

    #[test]
    fn test_version_compare() {
        use std::cmp::Ordering;

        // Basic comparisons
        assert_eq!(version_compare("2.1.5", "2.1.4"), Ordering::Greater);
        assert_eq!(version_compare("2.1.5", "2.1.5"), Ordering::Equal);
        assert_eq!(version_compare("2.0.0", "2.1.0"), Ordering::Less);
        assert_eq!(version_compare("2.10.0", "2.9.0"), Ordering::Greater);

        // Equal versions
        assert_eq!(version_compare("1.2.3", "1.2.3"), Ordering::Equal);

        // Zero-padding (the old bug: "1.2" vs "1.2.0")
        assert_eq!(version_compare("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(version_compare("1.2", "1.2.1"), Ordering::Less);
        assert_eq!(version_compare("1.2.1", "1.2"), Ordering::Greater);

        // Less / Greater
        assert_eq!(version_compare("1.2.3", "1.2.4"), Ordering::Less);
        assert_eq!(version_compare("2.0.0", "1.9.9"), Ordering::Greater);

        // Prefix stripping
        assert_eq!(version_compare("v1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(version_compare("rust-v0.116.0", "0.116.0"), Ordering::Equal);

        // Pre-release is less than release (per semver)
        assert_eq!(version_compare("1.2.3-beta", "1.2.3"), Ordering::Less);
        assert_eq!(version_compare("1.2.3-beta.1", "1.2.3"), Ordering::Less);
        assert_eq!(version_compare("1.2.3", "1.2.3-beta"), Ordering::Greater);
        // Pre-release suffixes are compared lexicographically
        assert_eq!(version_compare("1.2.3-alpha", "1.2.3-beta"), Ordering::Less);
        assert_eq!(
            version_compare("1.2.3-beta", "1.2.3-alpha"),
            Ordering::Greater
        );
        // Gemini-style preview versions sort correctly
        assert_eq!(
            version_compare("0.36.0-preview.0", "0.36.0-preview.2"),
            Ordering::Less
        );
        assert_eq!(
            version_compare("0.36.0-preview.5", "0.36.0-preview.6"),
            Ordering::Less
        );
        assert_eq!(
            version_compare("0.36.0-nightly.20260318", "0.36.0-nightly.20260325"),
            Ordering::Less
        );
        // nightly < preview (lexicographic)
        assert_eq!(
            version_compare("0.36.0-nightly.1", "0.36.0-preview.0"),
            Ordering::Less
        );
        // Multi-digit numeric segments (SemVer 11.4 — numeric comparison, not lexicographic)
        assert_eq!(
            version_compare("0.36.0-preview.2", "0.36.0-preview.10"),
            Ordering::Less
        );
        assert_eq!(
            version_compare("0.36.0-preview.10", "0.36.0-preview.2"),
            Ordering::Greater
        );
        assert_eq!(
            version_compare("0.36.0-preview.15", "0.36.0-preview.15"),
            Ordering::Equal
        );

        // Single component
        assert_eq!(version_compare("2", "1"), Ordering::Greater);
        assert_eq!(version_compare("1", "2"), Ordering::Less);

        // Large numbers
        assert_eq!(version_compare("0.116.0", "0.115.9"), Ordering::Greater);
        assert_eq!(version_compare("9.4.0", "9.3.0"), Ordering::Greater);
        assert_eq!(
            version_compare("999999999999999999999999999999.0.0", "1.0.0"),
            Ordering::Greater
        );
        assert_eq!(
            version_compare(
                "1.0.0-preview.999999999999999999999999999999",
                "1.0.0-preview.2"
            ),
            Ordering::Greater
        );
    }

    #[test]
    fn test_version_less_than() {
        assert!(version_less_than("1.2.3", "1.2.4"));
        assert!(!version_less_than("1.2.4", "1.2.3"));
        assert!(!version_less_than("1.2.3", "1.2.3"));
        assert!(version_less_than("v1.0.0", "2.0.0"));
        // Pre-release is less than stable
        assert!(version_less_than("1.2.3-beta", "1.2.3"));
        assert!(!version_less_than("1.2.3", "1.2.3-beta"));
    }

    #[test]
    fn test_version_manager_creation() {
        let _vm = VersionManager::new();
        // Should not panic
    }

    /// Create a mock "claude" binary that sleeps before outputting version.
    /// Returns the temp directory (must be kept alive) and the path to add to PATH.
    fn create_mock_claude(sleep_ms: u32) -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let mock_path = temp.path().to_path_buf();
        let mock_claude = mock_path.join("claude");

        // Create a shell script that sleeps then outputs version
        let script = format!(
            "#!/bin/bash\nsleep {}\necho \"2.1.5 (Claude Code)\"\n",
            sleep_ms as f64 / 1000.0
        );
        std::fs::write(&mock_claude, script).unwrap();

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&mock_claude).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&mock_claude, perms).unwrap();
        }

        (temp, mock_path)
    }

    /// Test that demonstrates the performance problem with calling get_installed_version
    /// on every frame vs using a cached value.
    #[test]
    fn test_cached_version_performance() {
        // Create mock claude that takes 50ms to respond
        let (_temp, mock_path) = create_mock_claude(50);

        // Build a PATH that prepends the mock dir, but inject it per-Command
        // via VersionManager::new_for_test_with_path — no `unsafe set_var` and
        // no race with parallel tests.
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", mock_path.display(), original_path);

        let vm = VersionManager::new_for_test_with_path(new_path);
        const ITERATIONS: u32 = 10;

        // Measure time for subprocess calls (old behavior - calling on every frame)
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let _ = vm.get_installed_version();
        }
        let subprocess_time = start.elapsed();

        // Measure time for cached value access (new behavior)
        let cached_version = vm.get_installed_version(); // Cache once
        let start = Instant::now();
        for _ in 0..ITERATIONS {
            let _ = cached_version.clone();
        }
        let cached_time = start.elapsed();

        // Verify a version was returned (don't check specific version as it may vary)
        assert!(cached_version.is_some(), "Should have a cached version");

        // Assert subprocess calls are slow (should be ~500ms for 10 x 50ms)
        assert!(
            subprocess_time.as_millis() > 100,
            "Subprocess calls should take >100ms, took {}ms",
            subprocess_time.as_millis()
        );

        // Assert cached access is fast (should be <1ms)
        assert!(
            cached_time.as_millis() < 10,
            "Cached access should take <10ms, took {}ms",
            cached_time.as_millis()
        );

        // Assert cached is at least 100x faster
        let speedup = subprocess_time.as_nanos() / cached_time.as_nanos().max(1);
        assert!(
            speedup > 100,
            "Cached should be >100x faster, was only {}x",
            speedup
        );

        println!(
            "Performance test results:\n  Subprocess ({} calls): {:?}\n  Cached ({} accesses): {:?}\n  Speedup: {}x",
            ITERATIONS, subprocess_time, ITERATIONS, cached_time, speedup
        );
    }

    /// Create a mock "npm" binary that captures arguments to a file,
    /// and a mock "curl" that always fails (so native install falls through to npm).
    fn create_mock_npm() -> (TempDir, PathBuf, PathBuf) {
        let temp = TempDir::new().unwrap();
        let mock_path = temp.path().to_path_buf();
        let mock_npm = mock_path.join("npm");
        let args_file = mock_path.join("npm_args.txt");

        let script = format!(
            "#!/bin/bash\necho \"$@\" >> \"{}\"\nexit 0\n",
            args_file.display()
        );
        std::fs::write(&mock_npm, script).unwrap();

        // Mock curl to always fail so native GCS install falls through to npm
        let mock_curl = mock_path.join("curl");
        std::fs::write(&mock_curl, "#!/bin/bash\nexit 1\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for bin in [&mock_npm, &mock_curl] {
                let mut perms = std::fs::metadata(bin).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(bin, perms).unwrap();
            }
        }

        (temp, mock_path, args_file)
    }

    #[test]
    fn test_install_version_uses_force_flag() {
        let (_temp, mock_path, args_file) = create_mock_npm();

        // Build PATH with mock_path prepended and inject it per-Command via
        // the manager — no unsafe env mutation, no parallel-test races.
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", mock_path.display(), original_path);

        let vm = VersionManager::new_for_test_with_path(new_path);
        let result = vm.install_version("2.1.4");

        assert!(result.is_ok(), "install_version should not return an error");
        let install_result = result.unwrap();
        assert!(
            install_result.success,
            "install should succeed with mock npm"
        );

        let captured_args = std::fs::read_to_string(&args_file)
            .expect("Should be able to read captured npm arguments");

        assert!(
            captured_args.contains("--force"),
            "npm install should include --force flag for downgrades. Got: {}",
            captured_args.trim()
        );
        assert!(
            captured_args.contains("install"),
            "Should contain 'install' command. Got: {}",
            captured_args.trim()
        );
        assert!(
            captured_args.contains("-g"),
            "Should contain '-g' for global install. Got: {}",
            captured_args.trim()
        );
        assert!(
            captured_args.contains("@anthropic-ai/claude-code@2.1.4"),
            "Should contain package@version. Got: {}",
            captured_args.trim()
        );
    }

    /// Network-dependent benchmark: measures version fetch latency for all agents
    #[test]
    #[ignore]
    fn bench_parallel_vs_sequential_version_fetch() {
        use std::sync::mpsc as bench_mpsc;

        let vm = VersionManager::new();

        // Sequential fetch
        let start = Instant::now();
        let _ = vm.get_available_versions();
        let _ = vm.get_codex_available_versions();
        let _ = vm.get_gemini_available_versions();
        let _ = vm.get_opencode_available_versions();
        let sequential_time = start.elapsed();

        // Parallel fetch
        let start = Instant::now();
        let (tx, rx) = bench_mpsc::channel::<()>();
        let tx1 = tx.clone();
        let tx2 = tx.clone();
        let tx3 = tx.clone();
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_available_versions();
            let _ = tx.send(());
        });
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_codex_available_versions();
            let _ = tx1.send(());
        });
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_gemini_available_versions();
            let _ = tx2.send(());
        });
        std::thread::spawn(move || {
            let vm = VersionManager::new();
            let _ = vm.get_opencode_available_versions();
            let _ = tx3.send(());
        });
        for _ in 0..4 {
            let _ = rx.recv();
        }
        let parallel_time = start.elapsed();

        println!(
            "Sequential fetch (4 agents): {:?}\nParallel fetch (4 agents): {:?}\nSpeedup: {:.1}x",
            sequential_time,
            parallel_time,
            sequential_time.as_secs_f64() / parallel_time.as_secs_f64().max(0.001)
        );
    }
