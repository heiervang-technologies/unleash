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
    let claude_lines = claude::from_hub(hub_records)?;

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

    // Write JSONL, patching sessionId in every line to match our new session ID
    let mut output = String::new();
    for line in &claude_lines {
        let mut patched = line.clone();
        if let serde_json::Value::Object(ref mut obj) = patched {
            obj.insert(
                "sessionId".to_string(),
                serde_json::Value::String(session_id.clone()),
            );
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

    // Write to Gemini tmp directory
    let gemini_dir = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".gemini")
        .join("tmp")
        .join("imported")
        .join("chats");

    std::fs::create_dir_all(&gemini_dir)?;

    let now = chrono_like_now();
    let output_path = gemini_dir.join(format!("session-{}-{}.json", &now[..16].replace(':', "-"), &session_id[..6.min(session_id.len())]));

    let json = serde_json::to_string_pretty(&gemini_val)?;
    std::fs::write(&output_path, &json)?;

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
    dir.replace('/', "-").trim_start_matches('-').to_string()
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{t:032x}")
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
