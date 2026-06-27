//! Regression test for the orphaned-agent / unforwarded-signal bug
//! (issue #353, finding H1).
//!
//! A directed `SIGTERM` to the wrapper — exactly what `unleash-exit` /
//! `launcher::trigger_exit` and external process managers send — must be
//! forwarded to the agent child so it terminates too, instead of being orphaned
//! while the wrapper blocks forever in `child.wait()`.
//!
//! The fake agent replays the precise `trigger_exit` scenario: it sends SIGTERM
//! to its wrapper (`AGENT_WRAPPER_PID`) and then `exec sleep`s to try to outlive
//! it. With the fix the wrapper forwards the signal and the agent dies promptly,
//! so `run_loop` returns `128 + SIGTERM` (143) within a fraction of a second.
//! Without the fix the signal is swallowed and the wrapper hangs until the
//! agent's own sleep elapses (~8 s) and exits 0.
//!
//! This file holds a single test on purpose: `run_loop` installs process-global
//! signal handlers, so it must not run alongside other signal-sensitive tests.

#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tempfile::TempDir;
use unleash::launcher::{run_loop, LauncherConfig};

const SIGTERM: i32 = 15;

fn bash_path() -> Option<PathBuf> {
    which::which("bash").ok()
}

/// Fake agent: record pid, wait for the wrapper to install its handler, send a
/// directed SIGTERM at the wrapper, then `exec sleep` so this process is a bare
/// `sleep` that dies immediately on the forwarded signal (avoids bash's
/// foreground-signal deferral). If it is *not* killed it simply exits 0 after
/// the sleep — the "orphan survived" outcome.
fn write_fake_agent(dir: &Path, pid_file: &Path) -> PathBuf {
    let script = dir.join("fake-agent");
    let body = format!(
        r#"#!/usr/bin/env bash
set -u
echo $$ > {pid_file:?}
sleep 0.3
kill -TERM "$AGENT_WRAPPER_PID"
exec sleep 8
"#
    );
    fs::write(&script, body).unwrap();
    let mut perms = fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&script, perms).unwrap();
    script
}

#[test]
fn directed_sigterm_to_wrapper_is_forwarded_to_child() {
    if bash_path().is_none() {
        eprintln!("skipping directed_sigterm_to_wrapper_is_forwarded_to_child: bash not on PATH");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let pid_file = tmp.path().join("agent.pid");
    let agent = write_fake_agent(tmp.path(), &pid_file);

    let config = LauncherConfig {
        agent_cmd: agent,
        agent_type: None,
        cache_dir: tmp.path().join("cache"),
        auto_mode: false,
        prompt: None,
        extra_args: vec![],
        profile_env: HashMap::new(),
        include_plugin_args: false,
    };

    let start = Instant::now();
    let code = run_loop(config).expect("run_loop should return, not hang");
    let elapsed = start.elapsed();

    // With the fix, the wrapper forwards the agent's directed SIGTERM back to the
    // agent, which dies almost immediately. A hang (~8 s) means the signal was
    // swallowed and the child was orphaned.
    assert!(
        elapsed < Duration::from_secs(4),
        "run_loop took {elapsed:?} — directed SIGTERM was not forwarded to the child (orphan bug)"
    );
    assert_eq!(
        code,
        128 + SIGTERM,
        "expected a SIGTERM-signalled exit (143); got {code}"
    );
}
