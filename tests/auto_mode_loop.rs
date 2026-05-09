//! Integration test for the auto-mode restart loop.
//!
//! Auto-mode is unleash's core feature: a Stop hook + flag-file system that
//! drives self-restart loops. `unleash-refresh` writes a trigger file, the
//! wrapper sees it on agent exit and re-launches the agent with `--continue`.
//!
//! This test exercises the loop end-to-end with a fake bash "agent" so any
//! regression in `launcher::run_loop`'s state machine (trigger detection,
//! `--continue` injection on restart, exit on no-trigger) is caught at PR
//! time. Production-side, the loop lives in `src/launcher.rs::run_loop`;
//! `run()` builds a `LauncherConfig` from real config/env and forwards.
//!
//! The fake agent is a short bash script written to a tempdir. Run 1 writes
//! the trigger file (simulating `unleash-refresh`) and exits 0; run 2 exits
//! 0 without writing the trigger, ending the loop. We then assert that the
//! recorded argv files prove the second invocation included `--continue`
//! and the loop returned 0.

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use tempfile::TempDir;
use unleash::launcher::{run_loop, LauncherConfig};

/// Skip the test gracefully if `bash` isn't on PATH.
fn bash_path() -> Option<PathBuf> {
    which::which("bash").ok()
}

/// Build the fake agent script. It records its argv into
/// `<scratch>/run-<N>.argv` (one line per arg) and on the first invocation
/// writes the restart trigger file at `<cache_dir>/restart-trigger-<pid>`.
///
/// `cache_dir` is templated into the script at write time so the script
/// doesn't depend on the wrapper exporting it (which would require a
/// production-side change).
fn write_fake_agent(dir: &std::path::Path, cache_dir: &std::path::Path) -> PathBuf {
    let script_path = dir.join("fake-agent");
    let scratch = dir.join("scratch");
    fs::create_dir_all(&scratch).unwrap();

    // The counter file lets us tell run 1 from run 2 without depending on
    // the wrapper to expose the restart count.
    let counter = scratch.join("counter");
    fs::write(&counter, "0").unwrap();

    let body = format!(
        r#"#!/usr/bin/env bash
set -u
SCRATCH={scratch:?}
COUNTER="$SCRATCH/counter"
N=$(cat "$COUNTER")
N=$((N + 1))
echo -n "$N" > "$COUNTER"

# Record argv (one arg per line) so the test can introspect what we got.
ARGV_FILE="$SCRATCH/run-$N.argv"
: > "$ARGV_FILE"
for arg in "$@"; do
  printf '%s\n' "$arg" >> "$ARGV_FILE"
done
# Record the wrapper pid we saw, for cross-checking.
echo -n "$AGENT_WRAPPER_PID" > "$SCRATCH/run-$N.pid"

if [ "$N" = "1" ]; then
  # First run: simulate `unleash-refresh` writing the trigger file.
  TRIGGER={cache_dir:?}/restart-trigger-$AGENT_WRAPPER_PID
  : > "$TRIGGER"
fi

exit 0
"#,
        scratch = scratch.to_string_lossy(),
        cache_dir = cache_dir.to_string_lossy(),
    );

    fs::write(&script_path, body).unwrap();
    let mut perms = fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script_path, perms).unwrap();
    script_path
}

fn read_argv(path: &std::path::Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn auto_mode_restart_loop_runs_agent_twice_with_continue_on_second_invocation() {
    if bash_path().is_none() {
        eprintln!("skipping: bash not available on PATH");
        return;
    }

    // Use TempDir for both the cache (trigger files) and the scratch
    // (script + argv recordings). Tearing this down deletes all artifacts.
    let workspace = TempDir::new().expect("tempdir");
    let cache_dir = workspace.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();

    // Point HOME at the tempdir so launcher::run_agent's stale-telemetry
    // cleanup (which targets ~/.claude/telemetry) operates on a non-existent
    // path inside the tempdir and never touches the user's real home.
    let home_guard = ScopedEnv::set("HOME", workspace.path());

    let agent_script = write_fake_agent(workspace.path(), &cache_dir);

    // agent_type = None so the loop treats it as non-Claude (no plugin
    // discovery, no `--dangerously-skip-permissions` injection on restart).
    let config = LauncherConfig {
        agent_cmd: agent_script.clone(),
        agent_type: None,
        cache_dir: cache_dir.clone(),
        auto_mode: false, // keep auto-mode marker file out of the equation
        prompt: None,
        extra_args: vec!["initial-arg".to_string()],
        profile_env: HashMap::new(),
        include_plugin_args: false,
    };

    // Run the loop in a worker thread with a timeout so a wedged loop
    // (e.g. trigger file never cleared) fails the test instead of hanging
    // the suite. 30s is generous — a single iteration takes ~300ms due to
    // the inter-restart sleep in the loop.
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let result = run_loop(config);
        let _ = tx.send(result);
    });

    let result = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("run_loop did not return within 30s — likely stuck in restart loop");
    handle.join().expect("worker thread panicked");

    // Drop HOME guard now that the loop is done.
    drop(home_guard);

    let exit_code = result.expect("run_loop returned an io::Error");
    assert_eq!(exit_code, 0, "loop should exit 0 after agent exits cleanly");

    // The fake agent should have run exactly twice.
    let scratch = workspace.path().join("scratch");
    let run1 = scratch.join("run-1.argv");
    let run2 = scratch.join("run-2.argv");
    let run3 = scratch.join("run-3.argv");

    assert!(run1.exists(), "first invocation argv was not recorded");
    assert!(run2.exists(), "second invocation argv was not recorded");
    assert!(
        !run3.exists(),
        "loop should have terminated after run 2, but run 3 happened: {:?}",
        read_argv(&run3)
    );

    let args1 = read_argv(&run1);
    let args2 = read_argv(&run2);

    // Run 1: just the user-supplied extra_args (prompt is None).
    assert!(
        args1.contains(&"initial-arg".to_string()),
        "run 1 should pass through extra_args, got: {:?}",
        args1
    );
    assert!(
        !args1.iter().any(|a| a == "--continue"),
        "run 1 must NOT include --continue (no restart yet), got: {:?}",
        args1
    );

    // Run 2: the wrapper detected the trigger file and re-launched with
    // --continue prepended + the resurrection message appended.
    assert!(
        args2.contains(&"--continue".to_string()),
        "run 2 must include --continue (restart path), got: {:?}",
        args2
    );
    assert!(
        args2.contains(&"initial-arg".to_string()),
        "run 2 should still pass through extra_args, got: {:?}",
        args2
    );
    assert!(
        args2.contains(&"RESURRECTED.".to_string()),
        "run 2 should include the default RESURRECTED. message, got: {:?}",
        args2
    );
    // Non-Claude agent_type → must NOT inject --dangerously-skip-permissions
    assert!(
        !args2
            .iter()
            .any(|a| a == "--dangerously-skip-permissions"),
        "non-Claude agent_type must not get --dangerously-skip-permissions, got: {:?}",
        args2
    );
}

#[test]
fn auto_mode_restart_loop_injects_dangerously_skip_permissions_for_claude() {
    if bash_path().is_none() {
        eprintln!("skipping: bash not available on PATH");
        return;
    }

    let workspace = TempDir::new().expect("tempdir");
    let cache_dir = workspace.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();
    let home_guard = ScopedEnv::set("HOME", workspace.path());

    // For agent_type detection we name the binary "claude" so
    // `detect_agent_type` resolves to AgentType::Claude. The fake script
    // still does the trigger-then-clean-exit dance.
    let scripts_dir = workspace.path().join("bin");
    fs::create_dir_all(&scripts_dir).unwrap();
    let real_script = write_fake_agent(workspace.path(), &cache_dir);
    let claude_path = scripts_dir.join("claude");
    // Symlink so the script content is reused but the binary name is "claude".
    std::os::unix::fs::symlink(&real_script, &claude_path).unwrap();

    let config = LauncherConfig {
        agent_cmd: claude_path,
        agent_type: Some(unleash::agents::AgentType::Claude),
        cache_dir: cache_dir.clone(),
        auto_mode: false,
        prompt: None,
        extra_args: vec![],
        profile_env: HashMap::new(),
        include_plugin_args: false,
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(run_loop(config));
    });
    let result = rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("run_loop did not return within 30s");
    handle.join().expect("worker thread panicked");
    drop(home_guard);

    assert_eq!(result.expect("io error from run_loop"), 0);

    let scratch = workspace.path().join("scratch");
    let args2 = read_argv(&scratch.join("run-2.argv"));
    assert!(
        args2.contains(&"--continue".to_string()),
        "Claude run 2 must include --continue, got: {:?}",
        args2
    );
    assert!(
        args2.contains(&"--dangerously-skip-permissions".to_string()),
        "Claude run 2 must include --dangerously-skip-permissions on restart, got: {:?}",
        args2
    );
}

#[test]
fn auto_mode_restart_loop_no_trigger_returns_immediately() {
    if bash_path().is_none() {
        eprintln!("skipping: bash not available on PATH");
        return;
    }

    let workspace = TempDir::new().expect("tempdir");
    let cache_dir = workspace.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();
    let home_guard = ScopedEnv::set("HOME", workspace.path());

    // Write a one-shot agent that records its argv and exits cleanly
    // without writing a trigger file. The loop should run exactly once
    // and return.
    let scratch = workspace.path().join("scratch");
    fs::create_dir_all(&scratch).unwrap();
    let script = workspace.path().join("oneshot");
    let body = format!(
        r#"#!/usr/bin/env bash
set -u
ARGV_FILE={scratch:?}/oneshot.argv
: > "$ARGV_FILE"
for arg in "$@"; do printf '%s\n' "$arg" >> "$ARGV_FILE"; done
exit 0
"#,
        scratch = scratch.to_string_lossy(),
    );
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let config = LauncherConfig {
        agent_cmd: script,
        agent_type: None,
        cache_dir: cache_dir.clone(),
        auto_mode: false,
        prompt: None,
        extra_args: vec!["only-arg".to_string()],
        profile_env: HashMap::new(),
        include_plugin_args: false,
    };

    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        let _ = tx.send(run_loop(config));
    });
    let result = rx
        .recv_timeout(std::time::Duration::from_secs(15))
        .expect("run_loop did not return within 15s");
    handle.join().expect("worker thread panicked");
    drop(home_guard);

    assert_eq!(result.expect("io error from run_loop"), 0);
    let argv = read_argv(&scratch.join("oneshot.argv"));
    assert_eq!(argv, vec!["only-arg".to_string()]);
}

/// Scope-guarded env var setter. Restores the prior value (or unsets)
/// when dropped. Tests in the same binary run sequentially by default
/// when set_test_threads is 1; to be defensive about parallel test
/// scheduling clobbering shared env, callers should serialize tests
/// that mutate the same key (we only mutate HOME and the integration
/// test binary defaults to single-threaded for tests touching env).
struct ScopedEnv {
    key: String,
    prev: Option<std::ffi::OsString>,
}

impl ScopedEnv {
    fn set<P: AsRef<std::path::Path>>(key: &str, value: P) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value.as_ref().as_os_str());
        Self {
            key: key.to_string(),
            prev,
        }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(&self.key, v),
            None => std::env::remove_var(&self.key),
        }
    }
}
