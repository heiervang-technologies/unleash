//! Session injection: convert a foreign session and load it into a target CLI.

use crate::interchange::{claude, codex, gemini, opencode, hub::HubRecord, ConvertError};
use crate::interchange::sessions::{SessionInfo, find_session};

/// Result of injecting a session: the session ID to resume with and any extra args.
pub struct InjectionResult {
    pub session_id: String,
    pub resume_args: Vec<String>,
    pub message: String,
}

/// Inject a foreign session into the target CLI's session store.
/// Returns the session ID that the target CLI can resume.
pub fn inject_session(
    source_query: &str,
    target_cli: &str,
) -> Result<InjectionResult, ConvertError> {
    // Find the source session
    let session = find_session(source_query).ok_or_else(|| {
        ConvertError::InvalidFormat(format!("Session not found: {source_query}"))
    })?;

    eprintln!(
        "Found session: {} ({}) from {} at {}",
        session.name.as_deref().unwrap_or(&session.id),
        session.title.as_deref().unwrap_or("untitled"),
        session.cli,
        session.directory,
    );

    // Convert source to Hub
    let hub_records = source_to_hub(&session)?;
    eprintln!("Converted {} records to hub format", hub_records.len());

    // Inject into target
    match target_cli {
        "claude" | "claude-code" => inject_into_claude(&session, &hub_records),
        "codex" => inject_into_codex(&session, &hub_records),
        "gemini" | "gemini-cli" => inject_into_gemini(&session, &hub_records),
        "opencode" => inject_into_opencode(&session, &hub_records),
        _ => Err(ConvertError::InvalidFormat(format!(
            "Unsupported target CLI: {target_cli}"
        ))),
    }
}

fn source_to_hub(session: &SessionInfo) -> Result<Vec<HubRecord>, ConvertError> {
    match session.cli.as_str() {
        "claude" => {
            let data = std::fs::read_to_string(&session.path)?;
            let reader = std::io::BufReader::new(data.as_bytes());
            claude::to_hub(reader)
        }
        "codex" => {
            let data = std::fs::read_to_string(&session.path)?;
            let reader = std::io::BufReader::new(data.as_bytes());
            codex::to_hub(reader)
        }
        "gemini" => {
            let data = std::fs::read(&session.path)?;
            gemini::to_hub(&data)
        }
        "opencode" => {
            // For OpenCode, we need to export from the DB
            let input = export_opencode_session(&session.id)?;
            opencode::to_hub(&input)
        }
        _ => Err(ConvertError::InvalidFormat(format!(
            "Unknown source CLI: {}",
            session.cli
        ))),
    }
}

fn export_opencode_session(session_id: &str) -> Result<opencode::OpenCodeInput, ConvertError> {
    let db_path = dirs::data_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No data dir".into()))?
        .join("opencode")
        .join("opencode.db");

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;

    let mut msg_stmt = conn.prepare(
        "SELECT data FROM message WHERE session_id = ? ORDER BY time_created",
    )?;
    let messages: Vec<serde_json::Value> = msg_stmt
        .query_map([session_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();

    let mut part_stmt = conn.prepare(
        "SELECT data FROM part WHERE session_id = ? ORDER BY time_created",
    )?;
    let parts: Vec<serde_json::Value> = part_stmt
        .query_map([session_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();

    Ok(opencode::OpenCodeInput {
        session_id: session_id.to_string(),
        messages,
        parts,
    })
}

// === Target injection ===

fn inject_into_claude(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<InjectionResult, ConvertError> {
    let all_claude_lines = claude::from_hub(hub_records)?;

    // Filter: only keep user/assistant messages with non-empty, non-system content
    // Claude renders ALL JSONL lines as conversation turns — events show as blanks
    let claude_lines: Vec<_> = all_claude_lines
        .into_iter()
        .filter(|line| {
            let msg_type = line.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if msg_type != "user" && msg_type != "assistant" {
                return false;
            }
            // Check content is non-empty and not system preamble
            let content = line.get("message").and_then(|m| m.get("content"));
            match content {
                Some(serde_json::Value::String(s)) => {
                    !s.is_empty()
                        && !s.starts_with("<environment_context")
                        && !s.starts_with("<permissions")
                        && !s.starts_with("<user_shell_command")
                }
                Some(serde_json::Value::Array(arr)) => {
                    arr.iter().any(|block| {
                        block.get("text").and_then(|t| t.as_str()).map_or(false, |t| {
                            !t.is_empty() && !t.starts_with("[Reasoning]: \n")
                        })
                    })
                }
                _ => false,
            }
        })
        .collect();

    // Generate a fresh UUID for the Claude session
    let session_id = uuid_v4();

    // Use current working directory for the project path (where Claude will be launched)
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| source.directory.clone());

    let project_dir_name = if cwd.is_empty() {
        "imported".to_string()
    } else {
        encode_claude_project_path(&cwd)
    };

    let project_dir = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".claude")
        .join("projects")
        .join(&project_dir_name);

    std::fs::create_dir_all(&project_dir)?;

    let output_path = project_dir.join(format!("{session_id}.jsonl"));

    // Write JSONL, patching sessionId and building parentUuid chain
    let mut output = String::new();
    let mut prev_uuid: Option<String> = None;
    for line in &claude_lines {
        let mut patched = line.clone();
        if let serde_json::Value::Object(ref mut obj) = patched {
            obj.insert(
                "sessionId".to_string(),
                serde_json::Value::String(session_id.clone()),
            );

            // Ensure every line has a unique uuid
            let existing_uuid = obj
                .get("uuid")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);
            let this_uuid = existing_uuid.unwrap_or_else(uuid_v4);
            obj.insert("uuid".to_string(), serde_json::Value::String(this_uuid.clone()));

            // Build parentUuid chain: ALWAYS set, each line points to the previous
            // This overwrites any existing parentUuid to ensure a clean linear chain
            obj.insert(
                "parentUuid".to_string(),
                match &prev_uuid {
                    Some(parent) => serde_json::Value::String(parent.clone()),
                    None => serde_json::Value::Null,
                },
            );
            prev_uuid = Some(this_uuid);

            // Ensure cwd is set
            if !obj.contains_key("cwd") || obj["cwd"].is_null() {
                obj.insert(
                    "cwd".to_string(),
                    serde_json::Value::String(cwd.clone()),
                );
            }
        }
        output.push_str(&serde_json::to_string(&patched)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!("Injected {} lines to {}", claude_lines.len(), output_path.display());

    Ok(InjectionResult {
        session_id: session_id.clone(),
        resume_args: vec!["--resume".into(), session_id],
        message: format!(
            "Session '{}' from {} injected into Claude Code",
            source.name.as_deref().unwrap_or(&source.id),
            source.cli,
        ),
    })
}

fn inject_into_codex(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<InjectionResult, ConvertError> {
    let codex_lines = codex::from_hub(hub_records)?;

    let session_id = extract_session_id(hub_records);

    // Write to Codex sessions directory
    let now = chrono_like_now();
    let codex_dir = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".codex")
        .join("sessions")
        .join(&now[..4])  // year
        .join(&now[5..7]) // month
        .join(&now[8..10]); // day

    std::fs::create_dir_all(&codex_dir)?;

    let output_path = codex_dir.join(format!("rollout-{now}-{session_id}.jsonl"));

    let mut output = String::new();
    for line in &codex_lines {
        output.push_str(&serde_json::to_string(line)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!("Injected {} lines to {}", codex_lines.len(), output_path.display());

    Ok(InjectionResult {
        session_id: session_id.clone(),
        resume_args: vec!["resume".into(), session_id],
        message: format!(
            "Session '{}' from {} injected into Codex",
            source.name.as_deref().unwrap_or(&source.id),
            source.cli,
        ),
    })
}

fn inject_into_gemini(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<InjectionResult, ConvertError> {
    let gemini_val = gemini::from_hub(hub_records)?;
    let session_id = extract_session_id(hub_records);

    // Gemini uses project slugs from ~/.gemini/projects.json for session dirs
    // Falls back to SHA-256 hash if not in projects.json
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| source.directory.clone());

    let project_dir_name = gemini_project_slug(&cwd);
    let project_hash = sha256_hex(&cwd);

    let gemini_base = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".gemini")
        .join("tmp")
        .join(&project_dir_name);
    let chats_dir = gemini_base.join("chats");
    std::fs::create_dir_all(&chats_dir)?;

    // Filename: session-YYYY-MM-DDTHH-MM-<uuid8>.json
    let now = chrono_like_now();
    let date_part = &now[..now.len().min(16)].replace(':', "-");
    let uuid_short = &session_id[..session_id.len().min(8)];
    let output_path = chats_dir.join(format!("session-{}-{}.json", date_part, uuid_short));

    // Ensure projectHash is in the output
    let mut gemini_val = gemini_val;
    gemini_val["projectHash"] = serde_json::Value::String(project_hash);

    let json = serde_json::to_string_pretty(&gemini_val)?;
    std::fs::write(&output_path, &json)?;

    // Write/append logs.json entries for session discovery
    let logs_path = gemini_base.join("logs.json");
    let log_entries = gemini::build_logs_entries(hub_records);
    if !log_entries.is_empty() {
        let mut existing_logs: Vec<serde_json::Value> = if logs_path.exists() {
            std::fs::read_to_string(&logs_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        existing_logs.extend(log_entries);
        let logs_json = serde_json::to_string_pretty(&existing_logs)?;
        std::fs::write(&logs_path, &logs_json)?;
    }

    eprintln!("Injected session to {}", output_path.display());

    Ok(InjectionResult {
        session_id: session_id.clone(),
        resume_args: vec!["--resume".into(), session_id],
        message: format!(
            "Session '{}' from {} injected into Gemini CLI",
            source.name.as_deref().unwrap_or(&source.id),
            source.cli,
        ),
    })
}

fn gemini_project_slug(cwd: &str) -> String {
    // Look up project slug from ~/.gemini/projects.json
    let projects_path = dirs::home_dir()
        .map(|h| h.join(".gemini").join("projects.json"));

    if let Some(path) = projects_path {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(projects) = val.get("projects").and_then(|p| p.as_object()) {
                    // Exact match first
                    if let Some(slug) = projects.get(cwd).and_then(|v| v.as_str()) {
                        return slug.to_string();
                    }
                    // Try without trailing slash
                    let trimmed = cwd.trim_end_matches('/');
                    if let Some(slug) = projects.get(trimmed).and_then(|v| v.as_str()) {
                        return slug.to_string();
                    }
                }
            }
        }
    }

    // Fallback: use last path segment
    cwd.rsplit('/').next().unwrap_or("imported").to_string()
}

fn sha256_hex(input: &str) -> String {
    // Simple SHA-256 without external dependency — use the system's sha256sum
    use std::process::Command;
    Command::new("sha256sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(input.as_bytes());
            }
            child.wait_with_output()
        })
        .ok()
        .and_then(|out| {
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .next()
                .map(String::from)
        })
        .unwrap_or_else(|| format!("{:x}", input.len()))
}

fn inject_into_opencode(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<InjectionResult, ConvertError> {
    // OpenCode uses SQLite — we'd need to INSERT into the database
    // For now, export as Hub format and document manual import
    let session_id = extract_session_id(hub_records);

    let output_dir = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".local")
        .join("share")
        .join("opencode")
        .join("imported");

    std::fs::create_dir_all(&output_dir)?;

    let output_path = output_dir.join(format!("{session_id}.ucf.jsonl"));

    let mut output = String::new();
    for record in hub_records {
        output.push_str(&serde_json::to_string(record)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!("Exported to hub format at {}", output_path.display());
    eprintln!("Note: OpenCode SQLite injection not yet supported. Use hub file for reference.");

    Ok(InjectionResult {
        session_id,
        resume_args: vec![],
        message: format!(
            "Session '{}' from {} exported as hub format (OpenCode SQLite injection pending)",
            source.name.as_deref().unwrap_or(&source.id),
            source.cli,
        ),
    })
}

// === Helpers ===

fn extract_session_id(records: &[HubRecord]) -> String {
    records.iter().find_map(|r| {
        if let HubRecord::Session(s) = r {
            Some(s.session_id.clone())
        } else {
            None
        }
    }).unwrap_or_else(|| uuid_v4())
}

fn encode_claude_project_path(dir: &str) -> String {
    // Claude encodes /home/me/project as -home-me-project (leading dash, slashes to dashes)
    dir.replace('/', "-")
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Generate a UUID-like string from timestamp + pseudo-random bits
    let hi = (nanos >> 64) as u64;
    let lo = nanos as u64;
    let pid = std::process::id() as u64;
    let a = (lo >> 32) as u32;
    let b = (lo >> 16) as u16;
    let c = (lo & 0xFFFF) as u16 | 0x4000; // version 4
    let d = ((hi >> 48) as u16 & 0x3FFF) | 0x8000; // variant 1
    let e = hi ^ pid;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

fn chrono_like_now() -> String {
    // Simple ISO-ish timestamp without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Rough conversion (good enough for filenames)
    let days = secs / 86400;
    let years = days / 365;
    let year = 1970 + years;
    let remaining = days - years * 365;
    let month = remaining / 30 + 1;
    let day = remaining % 30 + 1;
    let hour = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;
    let sec = secs % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{min:02}-{sec:02}")
}
