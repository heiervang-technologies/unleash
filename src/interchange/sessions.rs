//! Session discovery: find and list conversation sessions across the supported CLIs
//! (claude, codex/clanker, gemini, agy, opencode, pi, hermes, ucf).
//!
//! Discovery is metadata-only and cheap: transcripts are never parsed whole.
//! JSONL stores are sampled with a bounded head + tail read (see
//! [`read_head_tail`]), which is enough for the cwd, the newest
//! `agent-name` / `custom-title` record and the last record timestamp.

use crate::interchange::CliFormat;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub cli: String,
    pub id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub directory: String,
    pub path: PathBuf,
    pub updated_at: String,
    pub message_count: Option<usize>,
    /// The harness that owns this session, when it differs from (or refines)
    /// the transcript *format* in `cli`. `cli` stays the format key used by
    /// crossload / the gateway (`codex` for both upstream codex and the
    /// clanker fork); `harness` says which binary actually wrote it, derived
    /// from the unleash profile that owns the store root (e.g. `clanker` for
    /// `~/.codex`, `codex` for a profile with `CODEX_HOME=~/.codex-openai`).
    /// Additive field: omitted from JSON when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
}

/// Discover all sessions across all CLIs on this machine.
pub fn discover_all() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    sessions.extend(discover_claude());
    sessions.extend(discover_codex());
    sessions.extend(discover_gemini());
    sessions.extend(discover_agy());
    sessions.extend(discover_hermes());
    sessions.extend(discover_opencode());
    sessions.extend(discover_pi());
    sessions.extend(discover_ucf());
    overlay_search_index_names(&mut sessions);
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
}

/// Discover sessions for a specific CLI.
pub fn discover_for(cli: CliFormat) -> Vec<SessionInfo> {
    let mut sessions = discover_format(cli, false);
    overlay_search_index_names(&mut sessions);
    sessions
}

/// Discover sessions for a CLI given by name (as accepted by `--cli`).
///
/// Same as [`discover_for`] except that the Antigravity aliases (`agy`,
/// `antigravity`, `antigravity-cli`) list agy's own conversation store
/// instead of collapsing onto the gemini format's `~/.gemini/tmp` chats.
pub fn discover_for_name(name: &str) -> Result<Vec<SessionInfo>, crate::interchange::ConvertError> {
    if is_agy_alias(name) {
        let mut sessions = discover_agy();
        overlay_search_index_names(&mut sessions);
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        return Ok(sessions);
    }
    let format: CliFormat = name.parse()?;
    Ok(discover_for(format))
}

fn is_agy_alias(name: &str) -> bool {
    matches!(name, "agy" | "antigravity" | "antigravity-cli")
}

/// Discover native sessions without consulting or refreshing the optional
/// semantic-search index.
///
/// The gateway polls this while a headful agent is running. Keeping that hot
/// path filesystem-only avoids spawning background reindex jobs and ensures
/// the native history remains the source of truth for live turn detection.
///
/// When `CODEX_HOME` is set in this process, the Codex scan is limited to that
/// one store root (the running harness's own home), so a live turn is never
/// matched against another harness's concurrently-written sessions. Listing
/// (`discover_for` / `discover_all`) always scans every root.
pub(crate) fn discover_native(cli: CliFormat) -> Vec<SessionInfo> {
    discover_format(cli, true)
}

fn discover_format(cli: CliFormat, active_codex_home_only: bool) -> Vec<SessionInfo> {
    let mut sessions = match cli {
        CliFormat::ClaudeCode => discover_claude(),
        CliFormat::Codex if active_codex_home_only => {
            let mut roots = codex_store_roots();
            if let Some(active) = std::env::var_os("CODEX_HOME").filter(|v| !v.is_empty()) {
                let active = PathBuf::from(active);
                let key = std::fs::canonicalize(&active).unwrap_or(active);
                roots.retain(|r| std::fs::canonicalize(&r.dir).unwrap_or(r.dir.clone()) == key);
            }
            discover_codex_in(&roots)
        }
        CliFormat::Codex => discover_codex(),
        CliFormat::GeminiCli => discover_gemini(),
        CliFormat::Hermes => discover_hermes(),
        CliFormat::OpenCode => discover_opencode(),
        CliFormat::Pi => discover_pi(),
        CliFormat::Ucf => discover_ucf(),
    };
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions
}

/// Overlay friendly names from the search index DB (`unleash sessions name ...`
/// and auto-generated titles) onto the filesystem-derived session list.
///
/// The search index is written by `src/search.rs` to a local SQLite file. If
/// a row exists for `(cli, source_id)`, we copy its preferred title
/// (generated > native) into `SessionInfo.name`, but only when the filesystem
/// scan didn't already pick up a name. This keeps the DB as a *supplementary*
/// source — JSONL-native names (Claude `agent-name`, Codex `session_index`)
/// still win.
///
/// Failures are silent: the index is optional and may not exist on a fresh
/// install or on a machine that's never run `unleash search`.
fn file_mtime_ns(p: &std::path::Path) -> Option<i64> {
    let md = std::fs::metadata(p).ok()?;
    let mtime = md.modified().ok()?;
    let dur = mtime.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_nanos() as i64)
}

fn trigger_background_reindex(db_path: &std::path::Path) {
    let lock_path = db_path.with_file_name("search-index.lock");
    let already_running = if lock_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&lock_path) {
            if let Ok(pid) = content.trim().parse::<i32>() {
                // EPERM means the PID exists but is owned by another user — still
                // a live process for our purposes.
                matches!(
                    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
                    Ok(_) | Err(nix::errno::Errno::EPERM)
                )
            } else {
                false
            }
        } else {
            false
        }
    } else {
        false
    };

    if !already_running {
        let exe = if let Some(override_bin) = std::env::var_os("UNLEASH_REINDEX_BINARY") {
            std::path::PathBuf::from(override_bin)
        } else {
            match std::env::current_exe() {
                Ok(p) => p,
                Err(_) => return,
            }
        };
        if let Some(parent) = lock_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // UNLEASH_REINDEX_LOCK_PATH lets the child clean up its own lock file on
        // exit (see run_sessions_action_async). Without it, the stale lock persists
        // until the next reindex check finds the PID is dead via kill(pid, 0).
        if let Ok(child) = std::process::Command::new(&exe)
            .args(["sessions", "reindex", "--json"])
            .env("UNLEASH_REINDEX_LOCK_PATH", &lock_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            let pid = child.id();
            let _ = std::fs::write(&lock_path, pid.to_string());
        }
    }
}

/// Overlay friendly names from the search index DB (`unleash sessions name ...`
/// and auto-generated titles) onto the filesystem-derived session list.
///
/// The search index is written by `src/search.rs` to a local SQLite file. If
/// a row exists for `(cli, source_id)`, we copy its preferred title
/// (generated > native) into `SessionInfo.name`, but only when the filesystem
/// scan didn't already pick up a name. This keeps the DB as a *supplementary*
/// source — JSONL-native names (Claude `agent-name`, Codex `session_index`)
/// still win.
///
/// Failures are silent: the index is optional and may not exist on a fresh
/// install or on a machine that's never run `unleash search`.
fn overlay_search_index_names(sessions: &mut [SessionInfo]) {
    if sessions.is_empty() {
        return;
    }
    let debug_mode = std::env::var_os("UNLEASH_DEBUG").is_some();

    // Helper to fall back to filesystem titles for exact name matching in `find_session`
    // (e.g. for Claude's agent-name or custom-title) even when the SQLite database is absent or unindexed.
    // This allows exact matching to function immediately on newly-updated or freshly-named files
    // without requiring an explicit reindex.
    let apply_filesystem_fallbacks = |sessions: &mut [SessionInfo]| {
        for s in sessions.iter_mut() {
            if s.name.is_none() && s.title.is_some() {
                s.name = s.title.clone();
            }
        }
    };

    let db_path = match dirs::data_local_dir() {
        Some(d) => d.join("unleash").join("search-index.db"),
        None => {
            apply_filesystem_fallbacks(sessions);
            return;
        }
    };

    let mut needs_reindex = !db_path.exists();
    // (cli, source_id) → (generated_title, native_title, db_mtime_ns)
    type OverlayRow = (Option<String>, Option<String>, i64);
    let mut overlay: std::collections::HashMap<(String, String), OverlayRow> =
        std::collections::HashMap::new();

    if db_path.exists() {
        if let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT cli, source_id, generated_title, native_title, mtime_ns
                 FROM sessions",
            ) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                }) {
                    for row in rows.flatten() {
                        overlay.insert((row.0, row.1), (row.2, row.3, row.4));
                    }
                } else if debug_mode {
                    eprintln!("Warning: Failed to execute search index query");
                }
            } else if debug_mode {
                eprintln!("Warning: Failed to prepare search index query");
            }
        } else if debug_mode {
            eprintln!(
                "Warning: Failed to open search index database {}",
                db_path.display()
            );
        }
    }

    for s in sessions.iter_mut() {
        // Skip background reindexing for Claude's rotated backup/archive sessions
        if s.id.ends_with(".archive") {
            continue;
        }
        let fs_mtime = file_mtime_ns(&s.path).unwrap_or(0);
        if let Some((gen, nat, db_mtime)) = overlay.get(&(s.cli.clone(), s.id.clone())) {
            if *db_mtime != fs_mtime {
                needs_reindex = true;
            }
            if s.name.is_none() {
                if gen.is_some() {
                    // Manual custom title set by the user takes priority
                    s.name = gen.clone();
                } else if s.title.is_some() {
                    // Live filesystem-derived title takes priority over stale/cached native DB title
                    s.name = s.title.clone();
                } else {
                    // Last resort fallback: cached DB native title
                    s.name = nat.clone();
                }
            }
        } else {
            needs_reindex = true;
            if s.name.is_none() && s.title.is_some() {
                // Live filesystem-derived title acts as name fallback if missing from DB
                s.name = s.title.clone();
            }
        }
    }

    if needs_reindex {
        trigger_background_reindex(&db_path);
    }
}

/// Find a session by name, ID, or slug across all CLIs.
pub fn find_session(query: &str) -> Option<SessionInfo> {
    // Parse "cli:name" format
    let (cli_filter, name) = if let Some(pos) = query.find(':') {
        let cli = &query[..pos];
        let name = &query[pos + 1..];
        (Some(cli.to_string()), name.to_string())
    } else {
        (None, query.to_string())
    };

    let sessions = if let Some(ref cli) = cli_filter {
        discover_for_name(cli).unwrap_or_else(|_| discover_all())
    } else {
        discover_all()
    };

    // Match precedence: exact id > id prefix > exact name > substring of id/title.
    // Two-pass so an exact match in one CLI beats a fuzzy title-contains match
    // in another — important for pi, whose discovered ids are <TIMESTAMP>_<UUID>
    // and need substring matching to be resolvable by the UUID alone.
    let name_lower = name.to_lowercase();

    if let Some(s) = sessions.iter().find(|s| {
        s.id.to_lowercase() == name_lower
            || s.id.to_lowercase().starts_with(&name_lower)
            || s.name
                .as_ref()
                .is_some_and(|n| n.to_lowercase() == name_lower)
    }) {
        return Some(s.clone());
    }

    sessions.into_iter().find(|s| {
        s.id.to_lowercase().contains(&name_lower)
            || s.title
                .as_ref()
                .is_some_and(|t| t.to_lowercase().contains(&name_lower))
    })
}

// === Claude Code discovery ===

/// True if a claude session file's stem identifies a rotated/backup variant
/// rather than a live session. These appear under `.jsonl` extension but the
/// stem retains the suffix, e.g. `<uuid>.archive`. Picking them up dumps stale
/// (often hundreds of MB) history into the picker — and into pi if crossloaded.
fn is_claude_backup_stem(stem: &str) -> bool {
    stem.ends_with(".archive")
        || stem.ends_with(".pre-compact-full")
        || stem.ends_with(".pre-supercompact")
}

fn discover_claude() -> Vec<SessionInfo> {
    match dirs::home_dir() {
        Some(h) => discover_claude_in(&h.join(".claude").join("projects")),
        None => Vec::new(),
    }
}

/// Discover Claude Code transcripts under a `projects/` directory.
fn discover_claude_in(claude_dir: &Path) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let Ok(projects) = std::fs::read_dir(claude_dir) else {
        return sessions;
    };

    for project in projects.flatten() {
        if !project.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }
        let project_path = project.path();
        let project_dir_name = project.file_name().to_string_lossy().to_string();

        let Ok(files) = std::fs::read_dir(&project_path) else {
            continue;
        };

        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if is_claude_backup_stem(&session_id) {
                continue;
            }

            // `std::fs::metadata` follows symlinks: a dangling link (e.g. one
            // left behind by a desk-bootstrap script after its target moved)
            // fails here and is skipped instead of listed as a phantom session.
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }

            let info = read_claude_meta(&path, &project_dir_name);
            let updated_at = info
                .updated_ms
                .or_else(|| mtime_ms(&meta))
                .map(format_epoch_ms)
                .unwrap_or_default();
            let directory = info
                .cwd
                .unwrap_or_else(|| decode_claude_project_dir(&project_dir_name));

            sessions.push(SessionInfo {
                cli: "claude".to_string(),
                id: session_id,
                name: None,
                title: info.title,
                directory,
                path,
                updated_at,
                message_count: None,
                harness: Some("claude".to_string()),
            });
        }
    }

    sessions
}

/// Lossy last-resort decode of a Claude project directory name. Claude
/// encodes the cwd by replacing every non-alphanumeric byte with `-`, so
/// `/home/me/casa/crystal-mom` and `/home/me/casa/crystal/mom` collide. Only
/// used when no transcript record carries a `cwd`.
fn decode_claude_project_dir(name: &str) -> String {
    format!("/{}", name.replace('-', "/").trim_start_matches('/'))
}

/// Claude's project-directory encoding of a cwd: every byte that is not
/// ASCII alphanumeric becomes `-`.
fn encode_claude_project_dir(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

#[derive(Debug, Default)]
struct ClaudeMeta {
    cwd: Option<String>,
    title: Option<String>,
    updated_ms: Option<u64>,
}

/// Read cwd, newest name record and last record timestamp from a Claude
/// transcript using a bounded head + tail sample.
fn read_claude_meta(path: &Path, project_dir_name: &str) -> ClaudeMeta {
    let Some(sample) = read_head_tail(path, HEAD_BYTES, TAIL_BYTES) else {
        return ClaudeMeta::default();
    };
    // Name: the LAST `agent-name` / `custom-title` record wins (renames
    // append a new record). Claude re-appends these records as the session
    // goes on, so the newest one sits near the end of the file; the head is
    // only consulted when the tail has none (short / never-resumed files).
    let title = sample
        .tail_lines()
        .rev()
        .find_map(claude_title_from_line)
        .or_else(|| sample.head_lines().rev().find_map(claude_title_from_line));

    // cwd: every turn record carries the real cwd. Prefer a record whose cwd
    // re-encodes to this project directory (it is the cwd Claude resumes
    // from); a transcript ported into another desk's project dir falls back
    // to its newest recorded cwd.
    let mut cwds: Vec<String> = sample
        .head_lines()
        .filter(|l| contains(l, b"\"cwd\""))
        .filter_map(|l| json_str_field(l, "cwd"))
        .take(1)
        .collect();
    if let Some(c) = sample
        .tail_lines()
        .rev()
        .filter(|l| contains(l, b"\"cwd\""))
        .find_map(|l| json_str_field(l, "cwd"))
    {
        cwds.push(c);
    }
    let cwd = cwds
        .iter()
        .rev()
        .find(|c| encode_claude_project_dir(c) == project_dir_name)
        .or_else(|| cwds.last())
        .cloned();

    ClaudeMeta {
        cwd,
        title,
        updated_ms: last_record_timestamp_ms(&sample),
    }
}

fn claude_title_from_line(line: &[u8]) -> Option<String> {
    if !contains(line, b"\"agent-name\"") && !contains(line, b"\"custom-title\"") {
        return None;
    }
    let val: serde_json::Value = serde_json::from_slice(line).ok()?;
    let key = match val.get("type").and_then(|t| t.as_str()) {
        Some("agent-name") => "agentName",
        Some("custom-title") => "customTitle",
        _ => return None,
    };
    val.get(key)
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .map(String::from)
}

// === Bounded transcript sampling ===

/// Bytes read from the start of a transcript.
const HEAD_BYTES: u64 = 64 * 1024;
/// Bytes read from the end of a transcript. On centurion (2026-10-18) the
/// newest `agent-name` record was always within 33 KiB of EOF, even in
/// 280 MB transcripts; 128 KiB leaves headroom for one large tool result.
const TAIL_BYTES: u64 = 128 * 1024;

/// A bounded head + tail sample of a JSONL file. Only complete lines are
/// exposed: a line cut by the sample boundary is dropped.
struct HeadTail {
    head: Vec<u8>,
    /// `None` when the whole file fit in `head`.
    tail: Option<Vec<u8>>,
    head_is_whole_file: bool,
}

impl HeadTail {
    fn head_lines(&self) -> impl DoubleEndedIterator<Item = &[u8]> {
        let mut buf: &[u8] = &self.head;
        if !self.head_is_whole_file {
            // Drop the trailing partial line.
            buf = match buf.iter().rposition(|&b| b == b'\n') {
                Some(i) => &buf[..i],
                None => &[],
            };
        }
        split_lines(buf)
    }

    /// Lines of the tail sample, or of the whole file if it fit in the head.
    fn tail_lines(&self) -> impl DoubleEndedIterator<Item = &[u8]> {
        let buf: &[u8] = match &self.tail {
            Some(t) => match t.iter().position(|&b| b == b'\n') {
                // Drop the leading partial line.
                Some(i) => &t[i + 1..],
                None => &[],
            },
            None => &self.head,
        };
        split_lines(buf)
    }
}

fn split_lines(buf: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
    buf.split(|&b| b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .filter(|l| !l.is_empty())
}

fn read_head_tail(path: &Path, head_bytes: u64, tail_bytes: u64) -> Option<HeadTail> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len <= head_bytes + tail_bytes {
        let mut head = Vec::with_capacity(len as usize);
        file.read_to_end(&mut head).ok()?;
        return Some(HeadTail {
            head,
            tail: None,
            head_is_whole_file: true,
        });
    }
    let mut head = vec![0u8; head_bytes as usize];
    file.read_exact(&mut head).ok()?;
    file.seek(SeekFrom::Start(len - tail_bytes)).ok()?;
    let mut tail = Vec::with_capacity(tail_bytes as usize);
    file.read_to_end(&mut tail).ok()?;
    Some(HeadTail {
        head,
        tail: Some(tail),
        head_is_whole_file: false,
    })
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn json_str_field(line: &[u8], key: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_slice(line).ok()?;
    val.get(key)
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(String::from)
}

/// Epoch-ms of the newest top-level `timestamp` among the last few stamped
/// records (Claude and Codex both stamp every turn record). Records are not
/// strictly ordered on disk — Claude can append an older `user` record after
/// newer `system` ones — so take the max of a small window, not the last line.
fn last_record_timestamp_ms(sample: &HeadTail) -> Option<u64> {
    const WINDOW: usize = 32;
    sample
        .tail_lines()
        .rev()
        .filter(|l| contains(l, b"\"timestamp\""))
        .filter_map(|l| json_str_field(l, "timestamp").and_then(|t| parse_timestamp_ms(&t)))
        .take(WINDOW)
        .max()
}

/// Parse an RFC 3339 timestamp (or the space-separated variant SQLite / Go
/// emit, e.g. `2026-10-17 10:53:44.607295689+00:00`) into epoch-ms.
fn parse_timestamp_ms(s: &str) -> Option<u64> {
    let s = s.trim();
    let dt = chrono::DateTime::parse_from_rfc3339(s)
        .or_else(|_| chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%:z"))
        .or_else(|_| chrono::DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f%z"))
        .ok()?;
    u64::try_from(dt.timestamp_millis()).ok()
}

fn mtime_ms(meta: &std::fs::Metadata) -> Option<u64> {
    let t = meta.modified().ok()?;
    let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(d.as_millis() as u64)
}

// === Codex discovery ===

/// A Codex store root (a `CODEX_HOME`) and the harness that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexRoot {
    dir: PathBuf,
    harness: String,
}

/// Every Codex store root on this machine: the default `~/.codex`, the
/// process `CODEX_HOME`, and every `CODEX_HOME` an unleash profile sets
/// (`[env]` or `[agents.*.env]`). Upstream codex and the clanker fork can
/// live in different homes (e.g. the codex profile pins
/// `CODEX_HOME=~/.codex-openai`), so reading only one root silently drops
/// the other harness's sessions.
fn codex_store_roots() -> Vec<CodexRoot> {
    let home = dirs::home_dir();
    let profiles_dir = dirs::config_dir().map(|c| c.join("unleash").join("profiles"));
    codex_store_roots_from(
        home.as_deref(),
        profiles_dir.as_deref(),
        std::env::var_os("CODEX_HOME").map(PathBuf::from),
    )
}

/// Normalize a profile's agent binary to a codex-family harness name.
fn codex_family_harness(agent_cli_path: &str) -> Option<&'static str> {
    let base = Path::new(agent_cli_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(agent_cli_path);
    match base {
        "codex" => Some("codex"),
        "clanker" | "clanker-code" => Some("clanker"),
        _ => None,
    }
}

fn expand_home(value: &str, home: Option<&Path>) -> PathBuf {
    if let Some(h) = home {
        if value == "~" || value == "$HOME" {
            return h.to_path_buf();
        }
        for prefix in ["~/", "$HOME/", "${HOME}/"] {
            if let Some(rest) = value.strip_prefix(prefix) {
                return h.join(rest);
            }
        }
    }
    PathBuf::from(value)
}

fn codex_store_roots_from(
    home: Option<&Path>,
    profiles_dir: Option<&Path>,
    env_codex_home: Option<PathBuf>,
) -> Vec<CodexRoot> {
    // (root, claiming harness). `None` = a root with no known owner.
    let mut claims: Vec<(PathBuf, Option<String>)> = Vec::new();
    let default_root = home.map(|h| h.join(".codex"));
    if let Some(d) = &default_root {
        claims.push((d.clone(), None));
    }
    if let Some(e) = env_codex_home.filter(|e| !e.as_os_str().is_empty()) {
        claims.push((e, None));
    }

    let mut profile_files: Vec<PathBuf> = profiles_dir
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                .collect()
        })
        .unwrap_or_default();
    profile_files.sort();

    for pf in profile_files {
        let Ok(text) = std::fs::read_to_string(&pf) else {
            continue;
        };
        let Ok(val) = text.parse::<toml::Table>() else {
            continue;
        };
        let agent_cli_path = val
            .get("agent_cli_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let family = codex_family_harness(agent_cli_path);

        let mut overrides: Vec<String> = Vec::new();
        let env_codex = |t: Option<&toml::Value>| {
            t.and_then(|e| e.get("CODEX_HOME"))
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(String::from)
        };
        if let Some(v) = env_codex(val.get("env")) {
            overrides.push(v);
        }
        if let Some(agents) = val.get("agents").and_then(|a| a.as_table()) {
            for (_, a) in agents {
                if let Some(v) = env_codex(a.get("env")) {
                    overrides.push(v);
                }
            }
        }

        if overrides.is_empty() {
            // A codex-family profile without an override uses the default home.
            if let (Some(h), Some(d)) = (family, &default_root) {
                claims.push((d.clone(), Some(h.to_string())));
            }
        } else {
            for o in overrides {
                claims.push((
                    expand_home(&o, home),
                    Some(family.unwrap_or("codex").to_string()),
                ));
            }
        }
    }

    // Merge claims per (canonicalized) root, preserving first-seen order.
    let mut roots: Vec<(PathBuf, PathBuf, Vec<String>)> = Vec::new();
    for (dir, harness) in claims {
        let key = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        let entry = match roots.iter_mut().position(|(k, _, _)| *k == key) {
            Some(i) => &mut roots[i],
            None => {
                roots.push((key, dir, Vec::new()));
                roots.last_mut().unwrap()
            }
        };
        if let Some(h) = harness {
            if !entry.2.contains(&h) {
                entry.2.push(h);
            }
        }
    }

    roots
        .into_iter()
        .map(|(_, dir, owners)| CodexRoot {
            dir,
            // Exactly one owning harness → that harness. None, or several
            // (e.g. codex and clanker sharing the default home) → the format
            // name, since the transcripts carry no reliable fork marker.
            harness: if owners.len() == 1 {
                owners[0].clone()
            } else {
                "codex".to_string()
            },
        })
        .collect()
}

fn discover_codex() -> Vec<SessionInfo> {
    discover_codex_in(&codex_store_roots())
}

fn discover_codex_in(roots: &[CodexRoot]) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    for root in roots {
        let sessions_dir = root.dir.join("sessions");
        if !sessions_dir.is_dir() {
            continue;
        }
        let start = sessions.len();
        walk_codex_dir(&sessions_dir, &root.harness, &mut sessions);

        // session_index.jsonl carries thread names for this root only.
        let index_path = root.dir.join("session_index.jsonl");
        if let Ok(content) = std::fs::read_to_string(&index_path) {
            let mut names: std::collections::HashMap<String, Option<String>> =
                std::collections::HashMap::new();
            for line in content.lines() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = val
                        .get("thread_name")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    names.insert(id.to_string(), name);
                }
            }
            for session in &mut sessions[start..] {
                if let Some(name) = names.get(&session.id) {
                    session.name = name.clone();
                }
            }
        }
    }
    sessions
}

fn walk_codex_dir(dir: &std::path::Path, harness: &str, sessions: &mut Vec<SessionInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_codex_dir(&path, harness, sessions);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            // Skip dangling symlinks (metadata follows links).
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };

            // Extract session ID from filename: rollout-YYYY-MM-DDTHH-mm-ss-UUID.jsonl
            // The timestamp is exactly 19 chars: YYYY-MM-DDTHH-mm-ss
            // After "rollout-" (8 chars) + timestamp (19 chars) + "-" (1 char) = UUID starts at 28
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let after_prefix = filename.strip_prefix("rollout-").unwrap_or(filename);
            let session_id = if after_prefix.len() > 20 {
                // Skip timestamp (19 chars) + separator (1 char)
                after_prefix[20..].to_string()
            } else {
                after_prefix.to_string()
            };

            // Also try reading session_meta from first line for CWD
            let cwd = read_codex_cwd(&path);

            // Real timestamp: the newest record's `timestamp`, else mtime.
            let updated_at = read_head_tail(&path, 0, CODEX_TAIL_BYTES)
                .as_ref()
                .and_then(last_record_timestamp_ms)
                .or_else(|| mtime_ms(&meta))
                .map(format_epoch_ms)
                .unwrap_or_default();

            sessions.push(SessionInfo {
                cli: "codex".to_string(),
                id: session_id,
                name: None,
                title: None,
                directory: cwd.unwrap_or_default(),
                path,
                updated_at,
                message_count: None,
                harness: Some(harness.to_string()),
            });
        }
    }
}

/// Tail bytes sampled from a Codex rollout to find the last record timestamp.
const CODEX_TAIL_BYTES: u64 = 64 * 1024;

fn read_codex_cwd(path: &std::path::Path) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().take(5) {
        let line = line.ok()?;
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
                return val
                    .get("payload")
                    .and_then(|p| p.get("cwd"))
                    .and_then(|c| c.as_str())
                    .map(String::from);
            }
        }
    }
    None
}

// === Antigravity (agy) discovery ===

fn discover_agy() -> Vec<SessionInfo> {
    match dirs::home_dir() {
        Some(h) => discover_agy_in(&h.join(".gemini").join("antigravity-cli")),
        None => Vec::new(),
    }
}

/// Discover agy conversations: one SQLite DB per conversation under
/// `conversations/<id>.db`, enriched from `conversation_summaries.db`
/// (title, workspace, agent name, last-modified time, step count). agy does
/// NOT use gemini's `~/.gemini/tmp` chats store.
fn discover_agy_in(root: &Path) -> Vec<SessionInfo> {
    struct Summary {
        title: String,
        workspace: Option<String>,
        agent_name: String,
        updated_ms: Option<u64>,
        steps: Option<i64>,
    }
    let mut summaries: std::collections::HashMap<String, Summary> =
        std::collections::HashMap::new();
    let summaries_db = root.join("conversation_summaries.db");
    if summaries_db.is_file() {
        if let Ok(conn) = rusqlite::Connection::open_with_flags(
            &summaries_db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT conversation_id, title, workspace_uris, agent_name,
                        CAST(last_modified_time AS TEXT), step_count
                 FROM conversation_summaries",
            ) {
                if let Ok(rows) = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                    ))
                }) {
                    for (id, title, uris, agent, modified, steps) in rows.flatten() {
                        let Some(id) = id else { continue };
                        summaries.insert(
                            id,
                            Summary {
                                title: title.unwrap_or_default(),
                                workspace: uris.as_deref().and_then(first_workspace_path),
                                agent_name: agent.unwrap_or_default(),
                                updated_ms: modified.as_deref().and_then(parse_timestamp_ms),
                                steps,
                            },
                        );
                    }
                }
            }
        }
    }

    let mut sessions = Vec::new();
    let Ok(entries) = std::fs::read_dir(root.join("conversations")) else {
        return sessions;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `<id>.db` only — the `-wal` / `-shm` sidecars have other extensions.
        if path.extension().and_then(|e| e.to_str()) != Some("db") {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };
        let summary = summaries.get(&id);
        let updated_at = summary
            .and_then(|s| s.updated_ms)
            .or_else(|| mtime_ms(&meta))
            .map(format_epoch_ms)
            .unwrap_or_default();
        sessions.push(SessionInfo {
            cli: "agy".to_string(),
            name: summary
                .map(|s| s.agent_name.clone())
                .filter(|n| !n.is_empty()),
            title: summary.map(|s| s.title.clone()).filter(|t| !t.is_empty()),
            directory: summary
                .and_then(|s| s.workspace.clone())
                .unwrap_or_default(),
            id,
            path,
            updated_at,
            message_count: summary
                .and_then(|s| s.steps)
                .and_then(|n| usize::try_from(n).ok()),
            harness: Some("agy".to_string()),
        });
    }
    sessions
}

/// First `file://` URI of agy's JSON `workspace_uris` array, as a path.
fn first_workspace_path(uris_json: &str) -> Option<String> {
    let uris: Vec<String> = serde_json::from_str(uris_json).ok()?;
    let uri = uris.into_iter().next()?;
    let path = uri.strip_prefix("file://").unwrap_or(&uri);
    Some(percent_decode(path))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// === Gemini discovery ===

fn discover_gemini() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let gemini_dir = match dirs::home_dir() {
        Some(h) => h.join(".gemini").join("tmp"),
        None => return sessions,
    };

    if !gemini_dir.exists() {
        return sessions;
    }

    let Ok(projects) = std::fs::read_dir(&gemini_dir) else {
        return sessions;
    };

    for project in projects.flatten() {
        let chats_dir = project.path().join("chats");
        if !chats_dir.exists() {
            continue;
        }

        let Ok(files) = std::fs::read_dir(&chats_dir) else {
            continue;
        };

        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            // Read session JSON to get metadata
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let session_id = val
                        .get("sessionId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let updated_at = val
                        .get("lastUpdated")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let msg_count = val
                        .get("messages")
                        .and_then(|m| m.as_array())
                        .map(|a| a.len());

                    sessions.push(SessionInfo {
                        cli: "gemini".to_string(),
                        id: session_id,
                        name: None,
                        title: None,
                        directory: String::new(),
                        path,
                        updated_at,
                        message_count: msg_count,
                        harness: Some("gemini".to_string()),
                    });
                }
            }
        }
    }

    sessions
}

// === Hermes discovery ===

fn discover_hermes() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let db_path = match dirs::home_dir() {
        Some(h) => h.join(".hermes").join("state.db"),
        None => return sessions,
    };
    if !db_path.exists() {
        return sessions;
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return sessions,
    };
    let mut stmt = match conn.prepare(
        "SELECT id, title, started_at, ended_at, message_count, model
         FROM sessions
         WHERE source != 'cron'
         ORDER BY ended_at DESC
         LIMIT 500",
    ) {
        Ok(s) => s,
        Err(_) => return sessions,
    };
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let title: Option<String> = row.get(1)?;
        let started_at: f64 = row.get(2)?;
        let ended_at: f64 = row.get::<_, Option<f64>>(3)?.unwrap_or(started_at);
        let message_count: Option<i64> = row.get(4)?;
        Ok((id, title, started_at, ended_at, message_count))
    });
    let rows = match rows {
        Ok(r) => r,
        Err(_) => return sessions,
    };
    for row in rows.flatten() {
        let (id, title, _started, ended_at, msg_count) = row;
        let updated_at = format_epoch_ms((ended_at.max(0.0) * 1000.0) as u64);
        sessions.push(SessionInfo {
            cli: "hermes".to_string(),
            id: id.clone(),
            name: None,
            title,
            directory: dirs::home_dir()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: db_path.clone(),
            updated_at,
            message_count: msg_count.map(|n| n as usize),
            harness: Some("hermes".to_string()),
        });
    }
    sessions
}

// === OpenCode discovery ===

fn discover_opencode() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let db_path = match dirs::data_dir() {
        Some(d) => d.join("opencode").join("opencode.db"),
        None => return sessions,
    };

    if !db_path.exists() {
        return sessions;
    }

    let Ok(conn) =
        rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        return sessions;
    };

    let Ok(mut stmt) = conn.prepare(
        "SELECT id, slug, title, directory, time_updated FROM session ORDER BY time_updated DESC",
    ) else {
        return sessions;
    };

    let Ok(rows) = stmt.query_map([], |row| {
        Ok(SessionInfo {
            cli: "opencode".to_string(),
            id: row.get::<_, String>(0)?,
            name: row.get::<_, Option<String>>(1)?,
            title: row.get::<_, Option<String>>(2)?,
            directory: row.get::<_, String>(3).unwrap_or_default(),
            path: db_path.clone(),
            updated_at: row
                .get::<_, Option<i64>>(4)?
                .map(|ms| format_epoch_ms(ms as u64))
                .unwrap_or_default(),
            message_count: None,
            harness: Some("opencode".to_string()),
        })
    }) else {
        return sessions;
    };

    for row in rows.flatten() {
        sessions.push(row);
    }

    sessions
}

/// Format epoch-ms as an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// This string doubles as the crossload cache's freshness key
/// (`Entry::source_updated_at`, compared in `inject::inject_session` and
/// `crossload_index::run_doctor`), so it MUST keep second granularity: a
/// same-day `/compact` has to change it. See the
/// `format_epoch_ms_distinguishes_same_day_updates` regression test.
///
/// It used to approximate the calendar with 365-day years and 30-day months
/// (a 2026-10-18 mtime rendered as `2026-11-05`), which sorted correctly but
/// was wrong to display or filter on.
fn format_epoch_ms(ms: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ms as i64)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

// === Pi discovery ===

fn discover_pi() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let pi_dir = match dirs::home_dir() {
        Some(h) => h.join(".pi").join("agent").join("sessions"),
        None => return sessions,
    };

    if !pi_dir.exists() {
        return sessions;
    }

    let Ok(project_dirs) = std::fs::read_dir(&pi_dir) else {
        return sessions;
    };

    for project in project_dirs.flatten() {
        if !project.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }
        let project_path = project.path();
        // Pi encodes the project dir as --<path-with-slashes-as-hyphens>--
        let raw = project.file_name().to_string_lossy().to_string();
        let decoded = raw
            .strip_prefix("--")
            .and_then(|s| s.strip_suffix("--"))
            .map(|s| format!("/{}", s.replace('-', "/")))
            .unwrap_or(raw.clone());

        let Ok(files) = std::fs::read_dir(&project_path) else {
            continue;
        };

        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            let updated_at = file
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                    format_epoch_ms(duration.as_millis() as u64)
                })
                .unwrap_or_default();

            sessions.push(SessionInfo {
                cli: "pi".to_string(),
                id: session_id,
                name: None,
                title: None,
                directory: decoded.clone(),
                path,
                updated_at,
                message_count: None,
                harness: Some("pi".to_string()),
            });
        }
    }

    sessions
}

// === UCF (Native Hub) discovery ===

fn discover_ucf() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let ucf_dir = match dirs::data_dir() {
        Some(d) => d.join("unleash").join("sessions"),
        None => return sessions,
    };

    if !ucf_dir.exists() {
        return sessions;
    }

    let Ok(entries) = std::fs::read_dir(&ucf_dir) else {
        return sessions;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }

        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        if !file_name.ends_with(".ucf.jsonl") {
            continue;
        }

        let id = file_name
            .strip_suffix(".ucf.jsonl")
            .unwrap_or(&file_name)
            .to_string();

        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };

        use std::io::BufRead;
        let reader = std::io::BufReader::new(file);
        let mut lines = reader.lines();

        if let Some(Ok(first_line)) = lines.next() {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&first_line) {
                if val.get("type").and_then(|t| t.as_str()) == Some("session") {
                    let updated_at = val
                        .get("updated_at")
                        .and_then(|t| t.as_str())
                        .unwrap_or("1970-01-01T00:00:00.000Z")
                        .to_string();
                    let title = val
                        .get("title")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());

                    // remaining lines after header (messages + events)
                    let message_count = lines.filter(|l| l.is_ok()).count();

                    sessions.push(SessionInfo {
                        cli: "ucf".to_string(),
                        id: id.clone(),
                        name: Some(id),
                        title,
                        directory: ucf_dir.to_string_lossy().to_string(),
                        path,
                        updated_at,
                        message_count: Some(message_count),
                        harness: Some("ucf".to_string()),
                    });
                }
            }
        }
    }

    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    // Multiple tests in this module mutate UNLEASH_REINDEX_BINARY and
    // XDG_DATA_HOME around a `trigger_background_reindex` fork+exec. Cargo
    // runs tests on a thread pool by default, so two such tests racing can
    // leave the spawned child reading the OTHER test's env. We saw this as
    // intermittent flake-check failures on
    // `test_trigger_background_reindex_sets_lock_path_env_var`, and later
    // cross-MODULE: this module swapping XDG_DATA_HOME mid-skillsync-sync
    // (flake-check run 29565535983). Hence the CRATE-wide `test_env::lock()`
    // — a module-local mutex cannot exclude other modules.

    #[test]
    fn test_find_session_with_cli_prefix() {
        // This test just verifies the parsing, not actual discovery
        let query = "codex:hidden-wolf";
        let pos = query.find(':').unwrap();
        assert_eq!(&query[..pos], "codex");
        assert_eq!(&query[pos + 1..], "hidden-wolf");
    }

    #[test]
    fn test_discover_all_doesnt_crash() {
        // Discovery can spawn a background reindex process. Serialize it with
        // tests that temporarily override that process's environment/binary.
        let _env_lock = crate::test_env::lock();
        // Should not panic even if CLI dirs don't exist
        let sessions = discover_all();
        // We can't assert count since it depends on the machine; just verify no panic.
        let _ = sessions.len();
    }

    #[test]
    fn test_overlay_search_index_fills_missing_name() {
        let _env_lock = crate::test_env::lock();
        let mut sessions = vec![
            SessionInfo {
                cli: "claude".into(),
                id: "test-id-xyz".into(),
                name: None,
                title: None,
                directory: "/tmp".into(),
                path: PathBuf::from("/tmp/nope.jsonl"),
                updated_at: "0".into(),
                message_count: None,
                harness: None,
            },
            SessionInfo {
                cli: "claude".into(),
                id: "preserved-id".into(),
                name: Some("preexisting".into()),
                title: None,
                directory: "/tmp".into(),
                path: PathBuf::from("/tmp/nope2.jsonl"),
                updated_at: "0".into(),
                message_count: None,
                harness: None,
            },
            SessionInfo {
                cli: "claude".into(),
                id: "no-db-entry-id".into(),
                name: None,
                title: Some("fs-title".into()),
                directory: "/tmp".into(),
                path: PathBuf::from("/tmp/nope3.jsonl"),
                updated_at: "0".into(),
                message_count: None,
                harness: None,
            },
        ];

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("search-index.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                pk INTEGER PRIMARY KEY,
                cli TEXT NOT NULL,
                source_id TEXT NOT NULL,
                native_title TEXT,
                generated_title TEXT,
                mtime_ns INTEGER NOT NULL DEFAULT 0,
                UNIQUE(cli, source_id));
             INSERT INTO sessions (cli, source_id, generated_title, mtime_ns) VALUES
               ('claude', 'test-id-xyz', 'overlay-name', 0),
               ('claude', 'preserved-id', 'should-not-overwrite', 0);",
        )
        .unwrap();
        drop(conn);

        // Point dirs::data_local_dir() at our temp by hijacking XDG_DATA_HOME.
        // The unleash subdir must exist below it.
        let unleash_dir = tmp.path().join("unleash");
        std::fs::create_dir_all(&unleash_dir).unwrap();
        std::fs::rename(&db_path, unleash_dir.join("search-index.db")).unwrap();
        let saved_xdg = std::env::var_os("XDG_DATA_HOME");
        let saved_reindex_binary = std::env::var_os("UNLEASH_REINDEX_BINARY");
        let true_path = if std::path::Path::new("/usr/bin/true").exists() {
            "/usr/bin/true"
        } else {
            "/bin/true"
        };
        // SAFETY: tests are single-threaded for env mutation; restored below.
        unsafe {
            std::env::set_var("XDG_DATA_HOME", tmp.path());
            std::env::set_var("UNLEASH_REINDEX_BINARY", true_path);
        }

        overlay_search_index_names(&mut sessions);

        // Restore env before any panic surfaces.
        match saved_xdg {
            Some(v) => unsafe { std::env::set_var("XDG_DATA_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_DATA_HOME") },
        }
        match saved_reindex_binary {
            Some(v) => unsafe { std::env::set_var("UNLEASH_REINDEX_BINARY", v) },
            None => unsafe { std::env::remove_var("UNLEASH_REINDEX_BINARY") },
        }

        assert_eq!(sessions[0].name.as_deref(), Some("overlay-name"));
        assert_eq!(
            sessions[1].name.as_deref(),
            Some("preexisting"),
            "overlay must not overwrite an existing name"
        );
        assert_eq!(
            sessions[2].name.as_deref(),
            Some("fs-title"),
            "filesystem title should fall back to name if DB is missing entry"
        );

        // Verify that the lock file is created as a result of detecting that the
        // third session is missing from the DB (triggering background reindexing).
        let lock_path = unleash_dir.join("search-index.lock");
        assert!(
            lock_path.exists(),
            "search-index.lock should be created when reindexing is needed"
        );
        let content = std::fs::read_to_string(&lock_path).unwrap();
        let pid = content.trim().parse::<i32>().unwrap();
        assert!(pid > 0, "lock file should contain a valid PID");
        // Verify that the spawned process PID is live (kill(pid, 0) returns Ok)
        assert!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok(),
            "the spawned process PID should be active"
        );
    }

    #[test]
    fn test_trigger_background_reindex_sets_lock_path_env_var() {
        let _env_lock = crate::test_env::lock();
        // The child process needs UNLEASH_REINDEX_LOCK_PATH to clean up the lock
        // file on exit. We stub the child with a tiny shell script that asserts
        // the env var is set and points at the expected file. If the env var is
        // missing or wrong, the script exits non-zero and we can detect it via
        // the absence of the marker file it writes.
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("search-index.db");
        let unleash_dir = tmp.path();
        let lock_path = unleash_dir.join("search-index.lock");
        let marker = tmp.path().join("env-was-set.marker");

        // Wait at the end of script body so trigger_background_reindex writes
        // the lock file before we check the marker contents.
        let script_path = tmp.path().join("fake-unleash.sh");
        std::fs::write(
            &script_path,
            format!(
                "#!/bin/sh\necho \"$UNLEASH_REINDEX_LOCK_PATH\" > {:?}\n",
                marker
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&script_path).unwrap().permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&script_path, perm).unwrap();

        let saved_reindex_binary = std::env::var_os("UNLEASH_REINDEX_BINARY");
        // SAFETY: env mutation in tests; restored below.
        unsafe {
            std::env::set_var("UNLEASH_REINDEX_BINARY", &script_path);
        }

        trigger_background_reindex(&db_path);

        // Restore env before assertions
        match saved_reindex_binary {
            Some(v) => unsafe { std::env::set_var("UNLEASH_REINDEX_BINARY", v) },
            None => unsafe { std::env::remove_var("UNLEASH_REINDEX_BINARY") },
        }

        // Give the spawned child a moment to flush its echo. The script is
        // trivial; even on slow runners it completes in milliseconds.
        //
        // Wait for NON-EMPTY marker content, not just existence: under
        // llvm-cov instrumentation on the GHA runners the `echo "$VAR" >
        // file` redirection can leave the file visible-but-empty for a
        // few ms (file-create races ahead of the write-flush), causing
        // an empty `content.trim()` and an inscrutable string-mismatch
        // assertion failure. Observed once on #295. Polling for content
        // size > 0 removes that race without slowing the happy path.
        for _ in 0..40 {
            if let Ok(meta) = std::fs::metadata(&marker) {
                if meta.len() > 0 {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        assert!(marker.exists(), "fake child should have written the marker");
        let content = std::fs::read_to_string(&marker).unwrap();
        assert!(
            !content.trim().is_empty(),
            "marker exists but is empty — child likely raced before flushing the echo redirection"
        );
        let received_path = content.trim();
        assert_eq!(
            received_path,
            lock_path.to_string_lossy(),
            "child must receive UNLEASH_REINDEX_LOCK_PATH pointing at the parent's lock file"
        );
    }

    #[test]
    fn test_is_claude_backup_stem() {
        // Live session UUIDs should pass through.
        assert!(!is_claude_backup_stem(
            "8de7c62b-e0ba-4485-8af0-15b6912613a7"
        ));
        assert!(!is_claude_backup_stem(""));
        // Rotated archive ID — must be skipped or pi crossload eats a 700MB file.
        assert!(is_claude_backup_stem(
            "8de7c62b-e0ba-4485-8af0-15b6912613a7.archive"
        ));
        // Pre-compact snapshots claude writes alongside the live file.
        assert!(is_claude_backup_stem("abc.pre-compact-full"));
        assert!(is_claude_backup_stem("abc.pre-supercompact"));
        // Substring match isn't enough — must be a true suffix.
        assert!(!is_claude_backup_stem("archive-but-not-suffix"));
    }

    #[test]
    fn test_format_epoch_ms() {
        let ts = format_epoch_ms(1774800000000);
        assert!(ts.starts_with("2026-"));
    }

    #[test]
    fn format_epoch_ms_distinguishes_same_day_updates() {
        // Regression for the crossload staleness bug: `updated_at` doubles as the
        // crossload cache's freshness key (Entry::source_updated_at). When this
        // formatter hardcoded `T00:00:00Z`, two mtimes on the SAME UTC day
        // collapsed to identical strings, so re-crossloading a source that was
        // edited/compacted earlier the same day reused the stale target session.
        //
        // Same day (2026-03-29), one hour apart. Pre-fix these were byte-equal;
        // they must now differ so the cache invalidates.
        let base_ms: u64 = 1_774_800_000_000; // 2026-03-29T...
        let one_hour_later = base_ms + 3_600_000;
        let a = format_epoch_ms(base_ms);
        let b = format_epoch_ms(one_hour_later);
        assert_ne!(
            a, b,
            "same-day timestamps 1h apart must produce different strings \
             (else the crossload cache never sees a same-day source edit): {a} == {b}"
        );
        // The date portion must still match — only the time-of-day moved.
        assert_eq!(&a[..10], &b[..10], "date portion should be identical");
        // And a one-second delta must also register (finest granularity we key on).
        assert_ne!(format_epoch_ms(base_ms), format_epoch_ms(base_ms + 1000));
    }

    #[test]
    fn test_format_epoch_ms_no_month_13() {
        // Regression: remaining_days >= 360 used to produce month=13.
        // Test timestamps near Dec 26-31 of several years.
        // Exact day 360 of 1970 = 1970-12-27 ~ 31104000000 ms
        let ts = format_epoch_ms(31_104_000_000);
        let month: u32 = ts[5..7].parse().unwrap();
        assert!(
            (1..=12).contains(&month),
            "month={} out of range in '{}'",
            month,
            ts
        );

        // Also verify a timestamp at end of a later year
        let ts2 = format_epoch_ms(1_735_603_200_000); // ~2024-12-31
        let month2: u32 = ts2[5..7].parse().unwrap();
        assert!(
            (1..=12).contains(&month2),
            "month={} out of range in '{}'",
            month2,
            ts2
        );
    }

    // ── RFC-830 slice U1: discovery correctness fixtures ────────────────

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn claude_turn(cwd: &str, ts: &str) -> String {
        format!(
            r#"{{"type":"user","cwd":"{cwd}","sessionId":"s","timestamp":"{ts}","message":{{"role":"user","content":"hi"}}}}"#
        )
    }

    fn codex_rollout(cwd: &str, last_ts: &str) -> String {
        format!(
            "{}\n{}\n",
            format_args!(
                r#"{{"timestamp":"2026-10-01T08:00:00.000Z","type":"session_meta","payload":{{"id":"x","cwd":"{cwd}","originator":"codex-tui"}}}}"#
            ),
            format_args!(
                r#"{{"timestamp":"{last_ts}","type":"event_msg","payload":{{"type":"task_complete"}}}}"#
            )
        )
    }

    #[test]
    fn format_epoch_ms_is_a_real_calendar_date() {
        // Bug 5: the 365-day-year / 30-day-month approximation rendered a
        // 2026-10-18 mtime as 2026-11-05.
        assert_eq!(format_epoch_ms(1_792_330_604_995), "2026-10-18T13:36:44Z");
        assert_eq!(format_epoch_ms(31_104_000_000), "1970-12-27T00:00:00Z");
        assert_eq!(format_epoch_ms(1_735_603_200_000), "2024-12-31T00:00:00Z");
        assert_eq!(format_epoch_ms(1_709_164_800_000), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn parse_timestamp_ms_accepts_rfc3339_and_sqlite_forms() {
        assert_eq!(
            parse_timestamp_ms("2026-10-18T13:36:44.995Z"),
            Some(1_792_330_604_995)
        );
        assert_eq!(
            parse_timestamp_ms("2026-10-18 13:36:44.995000000+00:00"),
            Some(1_792_330_604_995)
        );
        assert_eq!(parse_timestamp_ms("not a date"), None);
    }

    #[test]
    fn claude_cwd_comes_from_records_not_the_lossy_dir_name() {
        // Bug 4: `/home/me/casa/crystal-mom` encodes to
        // `-home-me-casa-crystal-mom`, which used to decode as
        // `/home/me/casa/crystal/mom`. Dots are encoded the same way.
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path();
        write(
            &projects.join("-home-me-casa-crystal-mom/a.jsonl"),
            &format!(
                "{}\n",
                claude_turn("/home/me/casa/crystal-mom", "2026-10-18T09:00:00Z")
            ),
        );
        write(
            &projects.join("-home-me-ht-uiogaming-no/b.jsonl"),
            &format!(
                "{}\n",
                claude_turn("/home/me/ht/uiogaming.no", "2026-10-18T09:00:00Z")
            ),
        );
        // Ported transcript: head records carry the original cwd, newer
        // records the desk it now lives in → the matching (resumable) cwd wins.
        write(
            &projects.join("-home-me-casa-penny/c.jsonl"),
            &format!(
                "{}\n{}\n",
                claude_turn("/home/me/ht/admin", "2026-10-01T09:00:00Z"),
                claude_turn("/home/me/casa/penny", "2026-10-18T09:00:00Z")
            ),
        );
        // No cwd anywhere → lossy decode is the only option left.
        write(
            &projects.join("-home-me-x/d.jsonl"),
            "{\"type\":\"summary\",\"summary\":\"s\"}\n",
        );

        let mut got: Vec<(String, String)> = discover_claude_in(projects)
            .into_iter()
            .map(|s| (s.id, s.directory))
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                ("a".into(), "/home/me/casa/crystal-mom".into()),
                ("b".into(), "/home/me/ht/uiogaming.no".into()),
                ("c".into(), "/home/me/casa/penny".into()),
                ("d".into(), "/home/me/x".into()),
            ]
        );
    }

    #[test]
    fn claude_updated_at_uses_last_record_timestamp() {
        // Bug 5: the record timestamp is authoritative (copies / touches move
        // mtime); mtime is only the fallback.
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("-p/a.jsonl"),
            &format!(
                "{}\n{}\n{}\n{{\"type\":\"agent-name\",\"agentName\":\"n\"}}\n",
                claude_turn("/p", "2026-10-01T09:00:00Z"),
                claude_turn("/p", "2026-10-18T13:36:44.995Z"),
                // Out-of-order older record appended last: max wins, not last.
                claude_turn("/p", "2026-10-18T10:00:00Z")
            ),
        );
        write(&tmp.path().join("-p/b.jsonl"), "{\"type\":\"summary\"}\n");
        let sessions = discover_claude_in(tmp.path());
        let a = sessions.iter().find(|s| s.id == "a").unwrap();
        assert_eq!(a.updated_at, "2026-10-18T13:36:44Z");
        let b = sessions.iter().find(|s| s.id == "b").unwrap();
        let mtime = std::fs::metadata(&b.path).unwrap();
        assert_eq!(b.updated_at, format_epoch_ms(mtime_ms(&mtime).unwrap()));
    }

    #[test]
    fn claude_agent_name_found_past_line_50_and_in_large_files() {
        // Bug 6: only the first 50 lines were scanned.
        let tmp = tempfile::tempdir().unwrap();
        let turn = claude_turn("/p", "2026-10-18T09:00:00Z");
        let mut late = String::new();
        for _ in 0..200 {
            late.push_str(&turn);
            late.push('\n');
        }
        late.push_str(
            "{\"type\":\"agent-name\",\"agentName\":\"late-name\",\"sessionId\":\"s\"}\n",
        );
        write(&tmp.path().join("-p/late.jsonl"), &late);

        // Larger than head + tail: the name lives only near EOF, and a
        // rename appends a newer record which must win.
        let mut big = String::new();
        big.push_str("{\"type\":\"agent-name\",\"agentName\":\"old-name\"}\n");
        while (big.len() as u64) < HEAD_BYTES + TAIL_BYTES + 64 * 1024 {
            big.push_str(&turn);
            big.push('\n');
        }
        big.push_str("{\"type\":\"custom-title\",\"customTitle\":\"new-name\"}\n");
        big.push_str(&turn);
        big.push('\n');
        write(&tmp.path().join("-p/big.jsonl"), &big);

        // Name only in the head of a large file → still found.
        let mut head_only = String::new();
        head_only.push_str("{\"type\":\"agent-name\",\"agentName\":\"head-name\"}\n");
        while (head_only.len() as u64) < HEAD_BYTES + TAIL_BYTES + 64 * 1024 {
            head_only.push_str(&turn);
            head_only.push('\n');
        }
        write(&tmp.path().join("-p/headonly.jsonl"), &head_only);

        // A user message that merely mentions the record type is not a name.
        write(
            &tmp.path().join("-p/mention.jsonl"),
            "{\"type\":\"user\",\"message\":{\"content\":\"the \\\"agent-name\\\" record\"}}\n",
        );

        let sessions = discover_claude_in(tmp.path());
        let title = |id: &str| {
            sessions
                .iter()
                .find(|s| s.id == id)
                .and_then(|s| s.title.clone())
        };
        assert_eq!(title("late").as_deref(), Some("late-name"));
        assert_eq!(title("big").as_deref(), Some("new-name"));
        assert_eq!(title("headonly").as_deref(), Some("head-name"));
        assert_eq!(title("mention"), None);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_symlinks_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("-p/real.jsonl");
        write(
            &real,
            &format!("{}\n", claude_turn("/p", "2026-10-18T09:00:00Z")),
        );
        std::os::unix::fs::symlink(&real, tmp.path().join("-p/live.jsonl")).unwrap();
        std::os::unix::fs::symlink(
            tmp.path().join("gone.jsonl"),
            tmp.path().join("-p/dangling.jsonl"),
        )
        .unwrap();
        let mut ids: Vec<String> = discover_claude_in(tmp.path())
            .into_iter()
            .map(|s| s.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["live".to_string(), "real".to_string()]);

        // Same for codex rollouts.
        let root = tmp.path().join("codexhome");
        let dangling = root.join("sessions/2026/10/18/rollout-2026-10-18T09-00-00-dead.jsonl");
        std::fs::create_dir_all(dangling.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("nope"), &dangling).unwrap();
        let sessions = discover_codex_in(&[CodexRoot {
            dir: root,
            harness: "codex".into(),
        }]);
        assert!(sessions.is_empty(), "{sessions:?}");
    }

    fn codex_fixture(tmp: &Path) -> (PathBuf, PathBuf) {
        let home = tmp.join("home");
        let profiles = tmp.join("profiles");
        write(
            &profiles.join("codex.toml"),
            "name = \"codex\"\nagent_cli_path = \"codex\"\n[env]\nCODEX_HOME = \"~/.codex-openai\"\n",
        );
        write(
            &profiles.join("clanker.toml"),
            "name = \"clanker\"\nagent_cli_path = \"clanker\"\n[env]\nAU_HYPRLAND_FOCUS = \"1\"\n",
        );
        write(
            &profiles.join("claude.toml"),
            "name = \"claude\"\nagent_cli_path = \"claude\"\n",
        );
        write(
            &home.join(".codex/sessions/2026/10/18/rollout-2026-10-18T09-00-00-aaaa.jsonl"),
            &codex_rollout("/home/me/clanker-work", "2026-10-18T13:36:44.995Z"),
        );
        write(
            &home.join(".codex/session_index.jsonl"),
            "{\"id\":\"aaaa\",\"thread_name\":\"clanker-thread\"}\n{\"id\":\"bbbb\",\"thread_name\":\"wrong-root\"}\n",
        );
        write(
            &home.join(".codex-openai/sessions/2026/10/17/rollout-2026-10-17T09-00-00-bbbb.jsonl"),
            &codex_rollout("/home/me/codex-work", "2026-10-17T10:00:00Z"),
        );
        write(
            &home.join(".codex-openai/session_index.jsonl"),
            "{\"id\":\"bbbb\",\"thread_name\":\"codex-thread\"}\n",
        );
        (home, profiles)
    }

    #[test]
    fn codex_roots_include_profile_codex_home_and_tag_the_owning_harness() {
        // Bugs 1 + 3: a profile-pinned CODEX_HOME (~/.codex-openai) was never
        // scanned, and clanker's ~/.codex sessions were labeled codex.
        let tmp = tempfile::tempdir().unwrap();
        let (home, profiles) = codex_fixture(tmp.path());
        let roots = codex_store_roots_from(Some(&home), Some(&profiles), None);
        assert_eq!(
            roots,
            vec![
                CodexRoot {
                    dir: home.join(".codex"),
                    harness: "clanker".into()
                },
                CodexRoot {
                    dir: home.join(".codex-openai"),
                    harness: "codex".into()
                },
            ]
        );

        let mut got: Vec<(String, String, String, Option<String>, String, String)> =
            discover_codex_in(&roots)
                .into_iter()
                .map(|s| {
                    (
                        s.id,
                        s.cli,
                        s.harness.unwrap(),
                        s.name,
                        s.directory,
                        s.updated_at,
                    )
                })
                .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                (
                    "aaaa".into(),
                    "codex".into(),
                    "clanker".into(),
                    Some("clanker-thread".into()),
                    "/home/me/clanker-work".into(),
                    "2026-10-18T13:36:44Z".into()
                ),
                (
                    "bbbb".into(),
                    "codex".into(),
                    "codex".into(),
                    Some("codex-thread".into()),
                    "/home/me/codex-work".into(),
                    "2026-10-17T10:00:00Z".into()
                ),
            ]
        );
    }

    #[test]
    fn codex_roots_env_override_agent_env_and_shared_default() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let profiles = tmp.path().join("profiles");
        // Per-agent env override on a non-codex-family wrapper profile.
        write(
            &profiles.join("wrap.toml"),
            "name = \"wrap\"\nagent_cli_path = \"unleash\"\n[agents.codex.env]\nCODEX_HOME = \"/srv/codex-alt\"\n",
        );
        // codex and clanker both on the default home → ambiguous → "codex".
        write(
            &profiles.join("codex.toml"),
            "name = \"codex\"\nagent_cli_path = \"/usr/bin/codex\"\n",
        );
        write(
            &profiles.join("clanker.toml"),
            "name = \"clanker\"\nagent_cli_path = \"clanker-code\"\n",
        );
        let roots = codex_store_roots_from(
            Some(&home),
            Some(&profiles),
            Some(PathBuf::from("/opt/env-codex")),
        );
        assert_eq!(
            roots,
            vec![
                CodexRoot {
                    dir: home.join(".codex"),
                    harness: "codex".into()
                },
                CodexRoot {
                    dir: PathBuf::from("/opt/env-codex"),
                    harness: "codex".into()
                },
                CodexRoot {
                    dir: PathBuf::from("/srv/codex-alt"),
                    harness: "codex".into()
                },
            ]
        );
        // No profiles dir at all → just the default home.
        assert_eq!(
            codex_store_roots_from(Some(&home), None, None),
            vec![CodexRoot {
                dir: home.join(".codex"),
                harness: "codex".into()
            }]
        );
    }

    #[test]
    fn agy_conversations_are_discovered_from_summaries_db() {
        // Bug 2: agy keeps conversations under ~/.gemini/antigravity-cli,
        // not gemini's ~/.gemini/tmp, so none were listed.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let conv = root.join("conversations");
        std::fs::create_dir_all(&conv).unwrap();
        for f in ["c1.db", "c1.db-wal", "c1.db-shm", "c2.db"] {
            std::fs::write(conv.join(f), b"").unwrap();
        }
        let db = rusqlite::Connection::open(root.join("conversation_summaries.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id text, title text NOT NULL DEFAULT '',
               preview text NOT NULL DEFAULT '', step_count integer NOT NULL DEFAULT 0,
               last_modified_time datetime NOT NULL, workspace_uris text NOT NULL,
               agent_name text NOT NULL DEFAULT '', last_user_input_time datetime NOT NULL,
               PRIMARY KEY (conversation_id));
             INSERT INTO conversation_summaries VALUES
               ('c1','Voice Embedding','p',269,'2026-10-17 10:53:44.607295689+00:00',
                '[\"file:///home/me/ht/my%20proj\"]','vox','2026-10-17 10:52:25+00:00'),
               ('gone','No file','p',1,'2026-10-01 00:00:00+00:00','[]','','2026-10-01 00:00:00+00:00');",
        )
        .unwrap();
        drop(db);

        let mut sessions = discover_agy_in(root);
        sessions.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(sessions.len(), 2, "{sessions:?}");
        let c1 = &sessions[0];
        assert_eq!(c1.id, "c1");
        assert_eq!(c1.cli, "agy");
        assert_eq!(c1.harness.as_deref(), Some("agy"));
        assert_eq!(c1.name.as_deref(), Some("vox"));
        assert_eq!(c1.title.as_deref(), Some("Voice Embedding"));
        assert_eq!(c1.directory, "/home/me/ht/my proj");
        assert_eq!(c1.updated_at, "2026-10-17T10:53:44Z");
        assert_eq!(c1.message_count, Some(269));
        assert_eq!(c1.path, conv.join("c1.db"));
        // A conversation without a summary row is still listed (mtime date).
        let c2 = &sessions[1];
        assert_eq!(c2.id, "c2");
        assert!(c2.title.is_none());
        assert_eq!(c2.updated_at.len(), "2026-10-17T10:53:44Z".len());
    }

    #[test]
    fn agy_is_a_named_filter_but_not_part_of_the_gemini_format() {
        assert!(is_agy_alias("agy"));
        assert!(is_agy_alias("antigravity-cli"));
        assert!(!is_agy_alias("gemini"));
        // The gateway's GeminiCli hot path must not start returning agy
        // SQLite stores it cannot parse.
        assert!(discover_native(CliFormat::GeminiCli)
            .iter()
            .all(|s| s.cli != "agy"));
    }

    #[test]
    fn session_json_shape_is_additive() {
        let s = SessionInfo {
            cli: "codex".into(),
            id: "i".into(),
            name: None,
            title: None,
            directory: "/d".into(),
            path: PathBuf::from("/p"),
            updated_at: "2026-10-18T00:00:00Z".into(),
            message_count: None,
            harness: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        for k in [
            "cli",
            "id",
            "name",
            "title",
            "directory",
            "path",
            "updated_at",
            "message_count",
        ] {
            assert!(keys.contains(&k), "missing v1 key {k}");
        }
        assert!(!keys.contains(&"harness"), "unknown harness is omitted");
        let s = SessionInfo {
            harness: Some("clanker".into()),
            ..s
        };
        assert_eq!(serde_json::to_value(&s).unwrap()["harness"], "clanker");
    }

    #[test]
    fn percent_decode_handles_escapes_and_garbage() {
        assert_eq!(percent_decode("/a%20b"), "/a b");
        assert_eq!(percent_decode("/100%"), "/100%");
        assert_eq!(percent_decode("/x%zz"), "/x%zz");
        assert_eq!(percent_decode("/%C3%A6"), "/æ");
    }
}
