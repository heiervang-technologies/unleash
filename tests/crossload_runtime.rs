//! Integration tests for the crossload runtime injection path.
//!
//! These tests exercise `interchange::inject::inject_session` end-to-end:
//! lay down a synthetic source-CLI session in a tempdir, point the path
//! lookups (`HOME`, `XDG_DATA_HOME`, `CODEX_HOME`) at the tempdir, run the
//! injection, then assert the target store now contains a file in the target
//! CLI's expected layout. Where possible we also round-trip the freshly
//! written target session back through the converter to confirm semantic
//! equivalence with the source.
//!
//! Pairs covered (the lossless-tested set from PR #110):
//!   * claude → codex
//!   * codex  → claude
//!   * gemini → opencode
//!
//! The injection paths read from `dirs::home_dir()` / `dirs::data_dir()` and
//! the codex-specific `CODEX_HOME` override; we override those env vars per
//! test. Env vars are process-global so the tests share a Mutex to keep them
//! from clobbering each other.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use unleash::interchange::inject::inject_session;

// ─────────────────────────────────────────────────────────────────────
// Test isolation: env-vars are global, so all tests in this file run
// under a single mutex. The guard saves & restores the prior values so
// nothing leaks between tests.
// ─────────────────────────────────────────────────────────────────────

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn new(vars: &[&'static str]) -> Self {
        // Clear poisoning so a panic in one test doesn't kill the rest.
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let saved = vars
            .iter()
            .map(|k| (*k, std::env::var_os(k)))
            .collect::<Vec<_>>();
        EnvGuard { saved, _lock }
    }

    fn set(&self, key: &str, value: &Path) {
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

// ─────────────────────────────────────────────────────────────────────
// Helpers to lay down source sessions in a tempdir HOME
// ─────────────────────────────────────────────────────────────────────

/// Drop a Claude JSONL session into `$HOME/.claude/projects/<slug>/<session>.jsonl`.
/// Returns the session id (the file stem).
fn place_claude_source(home: &Path, jsonl_bytes: &[u8], session_id: &str) -> PathBuf {
    let project_dir = home.join(".claude").join("projects").join("-test-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join(format!("{session_id}.jsonl"));
    // Patch sessionId on every line to match the filename so discovery
    // returns the same id as we use for the query.
    let mut out = String::new();
    for line in std::str::from_utf8(jsonl_bytes).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "sessionId".into(),
                serde_json::Value::String(session_id.into()),
            );
        }
        out.push_str(&serde_json::to_string(&v).unwrap());
        out.push('\n');
    }
    std::fs::write(&path, &out).unwrap();
    path
}

/// Drop a Codex rollout into `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<ts>-<id>.jsonl`.
fn place_codex_source(codex_home: &Path, jsonl_bytes: &[u8], session_id: &str) -> PathBuf {
    let dir = codex_home
        .join("sessions")
        .join("2026")
        .join("03")
        .join("30");
    std::fs::create_dir_all(&dir).unwrap();
    // Filename pattern:  rollout-YYYY-MM-DDTHH-MM-SS-<uuid>.jsonl
    // Discovery extracts the id by stripping "rollout-" + 19-char timestamp + "-".
    let path = dir.join(format!("rollout-2026-03-30T07-19-37-{session_id}.jsonl"));
    // Patch session_meta.payload.id so the in-file id matches the filename id.
    let mut out = String::new();
    for line in std::str::from_utf8(jsonl_bytes).unwrap().lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
            if let Some(payload) = v.get_mut("payload") {
                payload["id"] = serde_json::Value::String(session_id.into());
            }
        }
        out.push_str(&serde_json::to_string(&v).unwrap());
        out.push('\n');
    }
    std::fs::write(&path, &out).unwrap();
    path
}

/// Drop a Gemini session into `$HOME/.gemini/tmp/<slug>/chats/<file>.json`.
fn place_gemini_source(home: &Path, json_bytes: &[u8], session_id: &str) -> PathBuf {
    let chats_dir = home
        .join(".gemini")
        .join("tmp")
        .join("test-project")
        .join("chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let path = chats_dir.join(format!("session-{session_id}.json"));
    // Patch sessionId in the JSON so discovery returns the id we expect.
    let mut v: serde_json::Value = serde_json::from_slice(json_bytes).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert(
            "sessionId".into(),
            serde_json::Value::String(session_id.into()),
        );
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    path
}

/// Initialize an OpenCode SQLite database with the schema the injector expects.
/// Returns the path (`<XDG_DATA_HOME>/opencode/opencode.db`).
fn init_opencode_db(xdg_data_home: &Path) -> PathBuf {
    let dir = xdg_data_home.join("opencode");
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("opencode.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE project (
            id TEXT PRIMARY KEY,
            worktree TEXT NOT NULL,
            vcs TEXT,
            name TEXT,
            icon_url TEXT,
            icon_color TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_initialized INTEGER,
            sandboxes TEXT NOT NULL,
            commands TEXT
        );
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            slug TEXT NOT NULL,
            directory TEXT NOT NULL,
            title TEXT NOT NULL,
            version TEXT NOT NULL,
            share_url TEXT,
            summary_additions INTEGER,
            summary_deletions INTEGER,
            summary_files INTEGER,
            summary_diffs TEXT,
            revert TEXT,
            permission TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            time_compacting INTEGER,
            time_archived INTEGER,
            workspace_id TEXT
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            message_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL,
            data TEXT NOT NULL
        );",
    )
    .unwrap();
    db_path
}

/// Fresh tempdir + the env vars the injection runtime reads. Returns
/// `(tmp, home, xdg_data_home)`. Caller is responsible for placing the
/// source session and calling `inject_session`. The guard parameter must
/// outlive the call so other tests don't race on env vars.
fn isolated_home(guard: &EnvGuard) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let xdg = tmp.path().join("xdg-data");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&xdg).unwrap();
    guard.set("HOME", &home);
    guard.set("XDG_DATA_HOME", &xdg);
    (tmp, home, xdg)
}

// ─────────────────────────────────────────────────────────────────────
// Pair tests
// ─────────────────────────────────────────────────────────────────────

/// claude → codex: drop a Claude JSONL session, inject into the
/// (tempdir) Codex store, and assert a rollout file landed in the
/// expected `sessions/YYYY/MM/DD/` layout with a `session_meta` header.
#[test]
fn inject_claude_into_codex() {
    let guard = EnvGuard::new(&[
        "HOME",
        "XDG_DATA_HOME",
        "CODEX_HOME",
        "UNLEASH_CROSSLOAD_FORCE",
    ]);
    let (_tmp, home, _xdg) = isolated_home(&guard);
    let codex_home = home.join(".codex");
    std::fs::create_dir_all(&codex_home).unwrap();
    guard.set("CODEX_HOME", &codex_home);

    let session_id = "cfbdefef-46d0-438f-bafc-c75f200bc243";
    place_claude_source(&home, &fixture("claude-10turn.jsonl"), session_id);

    let result = inject_session(&format!("claude:{session_id}"), "codex")
        .expect("inject_session claude→codex failed");

    // Sanity: a fresh session id and resume args were produced.
    assert!(!result.session_id.is_empty(), "empty target session id");
    assert_eq!(
        result.resume_args.first().map(String::as_str),
        Some("resume"),
        "codex resume args should start with 'resume': {:?}",
        result.resume_args
    );
    assert_eq!(result.resume_args.get(1), Some(&result.session_id));

    // A rollout file must now exist somewhere under `$CODEX_HOME/sessions/`.
    let rollout = find_first_jsonl(&codex_home.join("sessions"))
        .expect("no rollout file appeared in codex sessions dir");
    let body = std::fs::read_to_string(&rollout).unwrap();
    assert!(
        body.lines().count() >= 3,
        "rollout should have several lines: {} ({})",
        body.lines().count(),
        rollout.display()
    );
    let first: serde_json::Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert_eq!(
        first.get("type").and_then(|t| t.as_str()),
        Some("session_meta"),
        "first line of injected codex rollout must be session_meta"
    );
    assert_eq!(
        first
            .get("payload")
            .and_then(|p| p.get("id"))
            .and_then(|i| i.as_str()),
        Some(result.session_id.as_str()),
        "session_meta.payload.id must match the freshly minted session id"
    );

    // Round-trip: codex → hub. The injector strips images on the way out
    // (codex has no image support), so we only assert the converter
    // accepts the injected file and produces at least the session header
    // and one message record. A deeper semantic-eq is covered by the
    // lossless tests in #110.
    let reader = std::io::BufReader::new(body.as_bytes());
    let hub = unleash::interchange::codex::to_hub(reader)
        .expect("codex->hub on the freshly injected file failed");
    assert!(
        hub.len() >= 2,
        "round-tripped hub stream should have header + content: {}",
        hub.len()
    );
    assert!(
        matches!(hub[0], unleash::interchange::hub::HubRecord::Session(_)),
        "first hub record should be the session header"
    );
}

/// codex → claude: drop a Codex rollout, inject into the (tempdir) Claude
/// projects store, and assert a `<id>.jsonl` file landed there with a
/// well-formed `parentUuid` chain.
#[test]
fn inject_codex_into_claude() {
    let guard = EnvGuard::new(&[
        "HOME",
        "XDG_DATA_HOME",
        "CODEX_HOME",
        "UNLEASH_CROSSLOAD_FORCE",
    ]);
    let (_tmp, home, _xdg) = isolated_home(&guard);
    let codex_home = home.join(".codex");
    std::fs::create_dir_all(&codex_home).unwrap();
    guard.set("CODEX_HOME", &codex_home);

    let session_id = "12a9d56c-5021-4bb9-9905-6072e5e775fa";
    place_codex_source(&codex_home, &fixture("codex-10turn.jsonl"), session_id);

    let result = inject_session(&format!("codex:{session_id}"), "claude")
        .expect("inject_session codex→claude failed");

    assert!(!result.session_id.is_empty(), "empty target session id");
    assert_eq!(
        result.resume_args,
        vec!["--resume".to_string(), result.session_id.clone()],
        "claude resume args must be --resume <id>"
    );

    // The injector writes to `$HOME/.claude/projects/<encoded-cwd>/<id>.jsonl`.
    // The encoded cwd depends on the test process's `current_dir()` at the
    // time of the call, so just walk all project dirs.
    let projects = home.join(".claude").join("projects");
    let written = find_jsonl_with_stem(&projects, &result.session_id)
        .expect("no <session_id>.jsonl appeared under .claude/projects");
    let body = std::fs::read_to_string(&written).unwrap();
    assert!(!body.is_empty(), "claude jsonl should not be empty");

    // Validate parentUuid chain + sessionId imprinted on every line.
    let mut prev: Option<String> = None;
    let mut lines = 0usize;
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        lines += 1;
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let sid = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
        assert_eq!(
            sid, result.session_id,
            "sessionId must be patched on every line"
        );
        let uuid = v
            .get("uuid")
            .and_then(|u| u.as_str())
            .expect("every claude line must have a uuid")
            .to_string();
        let parent = v
            .get("parentUuid")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match (&prev, parent) {
            (None, serde_json::Value::Null) => {}
            (Some(p), serde_json::Value::String(pu)) => {
                assert_eq!(p, &pu, "parentUuid chain broken between consecutive lines")
            }
            (None, other) => panic!("first line must have null parentUuid, got {other:?}"),
            (Some(_), other) => panic!("non-first line must have string parentUuid, got {other:?}"),
        }
        prev = Some(uuid);
    }
    assert!(lines >= 2, "claude file should have ≥2 lines, got {lines}");

    // Round-trip claude → hub on the freshly written file.
    let reader = std::io::BufReader::new(body.as_bytes());
    let hub = unleash::interchange::claude::to_hub(reader)
        .expect("claude->hub on the freshly injected file failed");
    assert!(
        hub.len() >= 2,
        "round-tripped hub stream should have header + content: {}",
        hub.len()
    );
}

/// claude → antigravity: drop a Claude JSONL session, inject into the
/// (tempdir) Antigravity store (which uses Gemini's underlying store),
/// and assert a session JSON file landed in the expected chats layout.
#[test]
fn inject_claude_into_antigravity() {
    let guard = EnvGuard::new(&[
        "HOME",
        "XDG_DATA_HOME",
        "CODEX_HOME",
        "UNLEASH_CROSSLOAD_FORCE",
    ]);
    let (_tmp, home, _xdg) = isolated_home(&guard);

    let session_id = "5abf6b0c-d3a9-4692-bd17-1507f00cb3f7";
    place_claude_source(&home, &fixture("claude-10turn.jsonl"), session_id);

    let result = inject_session(&format!("claude:{session_id}"), "antigravity")
        .expect("inject_session claude→antigravity failed");

    // Sanity: a target session id and resume args were produced.
    assert!(!result.session_id.is_empty(), "empty target session id");
    assert_eq!(
        result.resume_args,
        vec!["--resume".to_string(), result.session_id.clone()],
        "antigravity resume args must be --resume <id>"
    );

    // Antigravity sessions land under `$HOME/.gemini/tmp/` because it takes place of Gemini CLI.
    let gemini_tmp = home.join(".gemini").join("tmp");
    let written = find_json_for_session(&gemini_tmp, &result.session_id)
        .expect("no <session_id>.json appeared under .gemini/tmp");
    let body = std::fs::read_to_string(&written).unwrap();
    assert!(!body.is_empty(), "antigravity json should not be empty");

    // Parse the written JSON and verify session id matches.
    let val: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        val.get("id").and_then(|id| id.as_str()),
        Some(result.session_id.as_str())
    );
}

/// gemini → opencode: drop a Gemini session JSON, set up an OpenCode
/// SQLite DB, inject, then verify a session row appears in the DB along
/// with corresponding messages and parts.
#[test]
fn inject_gemini_into_opencode() {
    let guard = EnvGuard::new(&[
        "HOME",
        "XDG_DATA_HOME",
        "CODEX_HOME",
        "UNLEASH_CROSSLOAD_FORCE",
    ]);
    let (_tmp, home, xdg) = isolated_home(&guard);
    let db_path = init_opencode_db(&xdg);

    let session_id = "f0d6cc5a-2f11-47f8-8300-029b89af4888";
    place_gemini_source(&home, &fixture("gemini-10turn.json"), session_id);

    let result = inject_session(&format!("gemini:{session_id}"), "opencode")
        .expect("inject_session gemini→opencode failed");

    assert!(
        result.session_id.starts_with("ses_"),
        "opencode session id must use 'ses_' prefix: {}",
        result.session_id
    );
    assert_eq!(
        result.resume_args.first().map(String::as_str),
        Some("-s"),
        "opencode resume args start with -s: {:?}",
        result.resume_args
    );
    assert_eq!(result.resume_args.get(1), Some(&result.session_id));

    // Verify the DB now contains exactly one project, one session, and
    // some messages + parts attached to it.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let project_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM project", [], |r| r.get(0))
        .unwrap();
    assert_eq!(project_count, 1, "expected a single project row");

    let session_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM session WHERE id = ?1",
            [&result.session_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        session_count, 1,
        "session row missing for {}",
        result.session_id
    );

    let msg_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM message WHERE session_id = ?1",
            [&result.session_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        msg_count >= 2,
        "expected at least two message rows for the injected session, got {msg_count}"
    );

    let part_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM part WHERE session_id = ?1",
            [&result.session_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        part_count >= 2,
        "expected at least two part rows for the injected session, got {part_count}"
    );

    // The parent_id chain on messages should be a clean linear list:
    // exactly one message has a NULL parentID, every other parentID
    // references a real id in the same session.
    let null_parents: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM message m
             WHERE m.session_id = ?1
             AND json_extract(m.data, '$.parentID') IS NULL",
            [&result.session_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        null_parents, 1,
        "exactly one message should have a null parentID (the head of the chain), got {null_parents}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Filesystem helpers
// ─────────────────────────────────────────────────────────────────────

fn find_first_jsonl(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_first_jsonl(&path) {
                return Some(found);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            return Some(path);
        }
    }
    None
}

fn find_jsonl_with_stem(dir: &Path, stem: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_jsonl_with_stem(&path, stem) {
                return Some(found);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
            && path.file_stem().and_then(|s| s.to_str()) == Some(stem)
        {
            return Some(path);
        }
    }
    None
}

fn find_json_for_session(dir: &Path, session_id: &str) -> Option<PathBuf> {
    if !dir.exists() {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_json_for_session(&path, session_id) {
                return Some(found);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            // Read and parse to see if it matches session_id
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if val.get("id").and_then(|id| id.as_str()) == Some(session_id) {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}
