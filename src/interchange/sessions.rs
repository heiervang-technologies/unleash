//! Session discovery: find and list conversation sessions across all 4 CLIs.

use crate::interchange::CliFormat;
use serde::Serialize;
use std::path::PathBuf;

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
}

/// Discover all sessions across all CLIs on this machine.
pub fn discover_all() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    sessions.extend(discover_claude());
    sessions.extend(discover_codex());
    sessions.extend(discover_gemini());
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
    let mut sessions = match cli {
        CliFormat::ClaudeCode => discover_claude(),
        CliFormat::Codex => discover_codex(),
        CliFormat::GeminiCli => discover_gemini(),
        CliFormat::Hermes => discover_hermes(),
        CliFormat::OpenCode => discover_opencode(),
        CliFormat::Pi => discover_pi(),
        CliFormat::Ucf => discover_ucf(),
    };
    overlay_search_index_names(&mut sessions);
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
fn overlay_search_index_names(sessions: &mut [SessionInfo]) {
    if sessions.is_empty() {
        return;
    }
    let db_path = match dirs::data_local_dir() {
        Some(d) => d.join("unleash").join("search-index.db"),
        None => return,
    };
    if !db_path.exists() {
        return;
    }
    let conn = match rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return,
    };
    let mut stmt = match conn.prepare(
        "SELECT cli, source_id, coalesce(generated_title, native_title) AS title
         FROM sessions
         WHERE title IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(_) => return,
    };
    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) {
        Ok(r) => r,
        Err(_) => return,
    };
    let mut overlay: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();
    for row in rows.flatten() {
        overlay.insert((row.0, row.1), row.2);
    }
    if overlay.is_empty() {
        return;
    }
    for s in sessions.iter_mut() {
        if s.name.is_some() {
            continue;
        }
        if let Some(title) = overlay.get(&(s.cli.clone(), s.id.clone())) {
            s.name = Some(title.clone());
        }
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
        if let Ok(format) = cli.parse::<CliFormat>() {
            discover_for(format)
        } else {
            discover_all()
        }
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
    let mut sessions = Vec::new();
    let claude_dir = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return sessions,
    };

    if !claude_dir.exists() {
        return sessions;
    }

    let Ok(projects) = std::fs::read_dir(&claude_dir) else {
        return sessions;
    };

    for project in projects.flatten() {
        if !project.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }
        let project_path = project.path();
        let project_name = project
            .file_name()
            .to_string_lossy()
            .replace('-', "/")
            .trim_start_matches('/')
            .to_string();

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

            // Get modified time as updated_at
            let updated_at = file
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                    format_epoch_ms(duration.as_millis() as u64)
                })
                .unwrap_or_default();

            // Try to get title from first few lines
            let title = read_claude_title(&path);

            sessions.push(SessionInfo {
                cli: "claude".to_string(),
                id: session_id,
                name: None,
                title,
                directory: format!("/{project_name}"),
                path,
                updated_at,
                message_count: None,
            });
        }
    }

    sessions
}

fn read_claude_title(path: &PathBuf) -> Option<String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);

    for line in reader.lines().take(50) {
        let line = line.ok()?;
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("custom-title") {
                return val
                    .get("customTitle")
                    .and_then(|t| t.as_str())
                    .map(String::from);
            }
            if val.get("type").and_then(|t| t.as_str()) == Some("agent-name") {
                return val
                    .get("agentName")
                    .and_then(|t| t.as_str())
                    .map(String::from);
            }
        }
    }
    None
}

// === Codex discovery ===

/// Return the Codex home directory, respecting the `CODEX_HOME` env var.
fn codex_home_dir() -> Option<std::path::PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Some(std::path::PathBuf::from(home));
    }
    dirs::home_dir().map(|h| h.join(".codex"))
}

fn discover_codex() -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let codex_dir = match codex_home_dir() {
        Some(h) => h.join("sessions"),
        None => return sessions,
    };

    if !codex_dir.exists() {
        return sessions;
    }

    // Walk year/month/day structure
    walk_codex_dir(&codex_dir, &mut sessions);

    // Also check session_index.jsonl for names
    let index_path = match codex_home_dir() {
        Some(h) => h.join("session_index.jsonl"),
        None => return sessions,
    };
    if index_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&index_path) {
            for line in content.lines() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = val
                        .get("thread_name")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    // Update session name if we found it
                    if let Some(session) = sessions.iter_mut().find(|s| s.id == id) {
                        session.name = name;
                    }
                }
            }
        }
    }

    sessions
}

fn walk_codex_dir(dir: &std::path::Path, sessions: &mut Vec<SessionInfo>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_codex_dir(&path, sessions);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
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

            let updated_at = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                    format_epoch_ms(duration.as_millis() as u64)
                })
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
            });
        }
    }
}

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
        // Format epoch to rough ISO timestamp (sortable)
        let secs = ended_at as u64;
        let updated_at = {
            let days = secs / 86400;
            let years = days / 365;
            let year = 1970 + years;
            let rem = days - years * 365;
            let month = (rem / 30 + 1).min(12);
            let day = rem % 30 + 1;
            let h = (secs % 86400) / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
        };
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
        })
    }) else {
        return sessions;
    };

    for row in rows.flatten() {
        sessions.push(row);
    }

    sessions
}

fn format_epoch_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let hours = secs / 3600;
    let days = hours / 24;
    let years = days / 365;
    let year = 1970 + years;
    // Simple ISO approximation (good enough for sorting).
    // Clamp month to 1-12: remaining_days can reach 364, giving month=13 without the clamp.
    let remaining_days = days - years * 365;
    let month = (remaining_days / 30 + 1).min(12);
    let day = remaining_days % 30 + 1;
    format!("{year:04}-{month:02}-{day:02}T00:00:00Z")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Should not panic even if CLI dirs don't exist
        let sessions = discover_all();
        // We can't assert count since it depends on the machine; just verify no panic.
        let _ = sessions.len();
    }

    #[test]
    fn test_overlay_search_index_fills_missing_name() {
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
                UNIQUE(cli, source_id));
             INSERT INTO sessions (cli, source_id, generated_title) VALUES
               ('claude', 'test-id-xyz', 'overlay-name'),
               ('claude', 'preserved-id', 'should-not-overwrite');",
        )
        .unwrap();
        drop(conn);

        // Point dirs::data_local_dir() at our temp by hijacking XDG_DATA_HOME.
        // The unleash subdir must exist below it.
        let unleash_dir = tmp.path().join("unleash");
        std::fs::create_dir_all(&unleash_dir).unwrap();
        std::fs::rename(&db_path, unleash_dir.join("search-index.db")).unwrap();
        let saved_xdg = std::env::var_os("XDG_DATA_HOME");
        // SAFETY: tests are single-threaded for env mutation; restored below.
        unsafe { std::env::set_var("XDG_DATA_HOME", tmp.path()); }

        overlay_search_index_names(&mut sessions);

        // Restore env before any panic surfaces.
        match saved_xdg {
            Some(v) => unsafe { std::env::set_var("XDG_DATA_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_DATA_HOME") },
        }

        assert_eq!(sessions[0].name.as_deref(), Some("overlay-name"));
        assert_eq!(
            sessions[1].name.as_deref(),
            Some("preexisting"),
            "overlay must not overwrite an existing name"
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
    fn test_format_epoch_ms_no_month_13() {
        // Regression: remaining_days >= 360 used to produce month=13.
        // Test timestamps near Dec 26-31 of several years.
        // Exact day 360 of 1970 = 1970-12-27 ~ 31104000000 ms
        let ts = format_epoch_ms(31_104_000_000);
        let month: u32 = ts[5..7].parse().unwrap();
        assert!(
            month >= 1 && month <= 12,
            "month={} out of range in '{}'",
            month,
            ts
        );

        // Also verify a timestamp at end of a later year
        let ts2 = format_epoch_ms(1_735_603_200_000); // ~2024-12-31
        let month2: u32 = ts2[5..7].parse().unwrap();
        assert!(
            month2 >= 1 && month2 <= 12,
            "month={} out of range in '{}'",
            month2,
            ts2
        );
    }
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
                    });
                }
            }
        }
    }

    sessions
}
