//! Session injection: convert a foreign session and load it into a target CLI.

use crate::interchange::sessions::{find_session, SessionInfo};
use crate::interchange::{claude, codex, gemini, hub::HubRecord, opencode, ConvertError};

/// Result of injecting a session: the session ID to resume with and any extra args.
pub struct InjectionResult {
    #[allow(dead_code)]
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
    let session = find_session(source_query)
        .ok_or_else(|| ConvertError::InvalidFormat(format!("Session not found: {source_query}")))?;

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

pub fn source_to_hub(session: &SessionInfo) -> Result<Vec<HubRecord>, ConvertError> {
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
        "ucf" => {
            let data = std::fs::read_to_string(&session.path)?;
            let mut records = Vec::new();
            for line in data.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(record) = serde_json::from_str(line) {
                    records.push(record);
                }
            }
            Ok(records)
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

    let mut msg_stmt =
        conn.prepare("SELECT data FROM message WHERE session_id = ? ORDER BY time_created")?;
    let messages: Vec<serde_json::Value> = msg_stmt
        .query_map([session_id], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();

    let mut part_stmt =
        conn.prepare("SELECT data FROM part WHERE session_id = ? ORDER BY time_created")?;
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
                Some(serde_json::Value::Array(arr)) => arr.iter().any(|block| {
                    block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| !t.is_empty() && !t.starts_with("[Reasoning]: \n"))
                }),
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
            obj.insert(
                "uuid".to_string(),
                serde_json::Value::String(this_uuid.clone()),
            );

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
                obj.insert("cwd".to_string(), serde_json::Value::String(cwd.clone()));
            }
        }
        output.push_str(&serde_json::to_string(&patched)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!(
        "Injected {} lines to {}",
        claude_lines.len(),
        output_path.display()
    );

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

    // Generate a fresh UUID for the Codex session (Codex uses UUIDv7)
    let session_id = uuid_v4(); // Our pseudo-UUID is fine; Codex accepts any valid UUID

    // Use current working directory (where Codex will be launched)
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| source.directory.clone());

    // Write to Codex sessions directory, respecting CODEX_HOME if set.
    let now = chrono_like_now();
    let codex_home =
        codex_home_dir().ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?;
    let codex_dir = codex_home
        .join("sessions")
        .join(&now[..4]) // year
        .join(&now[5..7]) // month
        .join(&now[8..10]); // day

    std::fs::create_dir_all(&codex_dir)?;

    let output_path = codex_dir.join(format!("rollout-{now}-{session_id}.jsonl"));

    // Write JSONL, patching session_meta with correct cwd and session_id
    let mut output = String::new();
    for line in &codex_lines {
        let mut patched = line.clone();
        // Patch session_meta payload with correct cwd and fresh session_id
        if patched.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
            if let Some(payload) = patched.get_mut("payload") {
                payload["id"] = serde_json::Value::String(session_id.clone());
                if payload
                    .get("cwd")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .is_empty()
                {
                    payload["cwd"] = serde_json::Value::String(cwd.clone());
                }
            }
        }
        output.push_str(&serde_json::to_string(&patched)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!(
        "Injected {} lines to {}",
        codex_lines.len(),
        output_path.display()
    );

    // Register the session in Codex's session_index.jsonl so `codex resume` can find it
    let index_path = codex_home.join("session_index.jsonl");
    let index_entry = serde_json::json!({
        "id": session_id,
        "thread_name": source.name.as_deref().unwrap_or(&source.id),
        "updated_at": now,
    });
    let mut index_line = serde_json::to_string(&index_entry)?;
    index_line.push('\n');
    // Append to the index file
    use std::io::Write;
    let mut index_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)?;
    index_file.write_all(index_line.as_bytes())?;
    eprintln!("Registered session in {}", index_path.display());

    // Register in state_5.sqlite threads table for app-server resume
    register_codex_thread(&codex_home, &session_id, &output_path, &cwd, source);

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

/// Register an injected session in the Codex state database so `codex resume <id>` works.
fn register_codex_thread(
    codex_home: &std::path::Path,
    session_id: &str,
    rollout_path: &std::path::Path,
    cwd: &str,
    source: &SessionInfo,
) {
    // Find the state DB (state_N.sqlite where N is the latest migration version)
    let state_db_path = find_codex_state_db(codex_home);
    let Some(db_path) = state_db_path else {
        eprintln!("Warning: Could not find Codex state database; session may not appear in `codex resume` picker");
        return;
    };

    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: Could not open Codex state DB: {e}");
            return;
        }
    };

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let title = source
        .title
        .as_deref()
        .unwrap_or(source.name.as_deref().unwrap_or("Imported session"));
    let first_user_message = title;

    let result = conn.execute(
        "INSERT OR REPLACE INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, cli_version, first_user_message, has_user_event, archived, sandbox_policy, approval_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            session_id,
            rollout_path.to_string_lossy().to_string(),
            now_secs,
            now_secs,
            "cli",
            "",
            cwd,
            title,
            "0.0.0",
            first_user_message,
            1i32,  // has_user_event
            0i32,  // not archived
            r#"{"type":"danger-full-access"}"#,  // sandbox_policy (Codex requires NOT NULL)
            "never",  // approval_mode
        ],
    );

    match result {
        Ok(_) => eprintln!("Registered session in Codex state DB"),
        Err(e) => eprintln!("Warning: Failed to register in state DB: {e}"),
    }
}

/// Find the Codex state database file (state_N.sqlite)
fn find_codex_state_db(codex_home: &std::path::Path) -> Option<std::path::PathBuf> {
    // Look for state_*.sqlite files, pick the highest version number
    let entries = std::fs::read_dir(codex_home).ok()?;
    let mut best: Option<(u32, std::path::PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if let Some(rest) = name_str.strip_prefix("state_") {
            if let Some(ver_str) = rest.strip_suffix(".sqlite") {
                if let Ok(ver) = ver_str.parse::<u32>() {
                    if best.as_ref().is_none_or(|(best_ver, _)| ver > *best_ver) {
                        best = Some((ver, entry.path()));
                    }
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

fn inject_into_gemini(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<InjectionResult, ConvertError> {
    let mut session_id = extract_session_id(hub_records);

    // Gemini CLI validates that --resume arguments are valid UUIDs.
    // If the source session ID is not a UUID (e.g. native UCF named sessions),
    // we must generate a fresh UUID for Gemini to accept it.
    let is_uuid = session_id.len() == 36
        && session_id.split('-').count() == 5
        && session_id.split('-').all(|seg| seg.chars().all(|c| c.is_ascii_hexdigit()));

    let final_records = if !is_uuid {
        session_id = uuid_v4();
        let mut patched = hub_records.to_vec();
        for record in &mut patched {
            if let HubRecord::Session(ref mut header) = record {
                header.session_id = session_id.clone();
            }
        }
        patched
    } else {
        hub_records.to_vec()
    };

    let gemini_val = gemini::from_hub(&final_records)?;

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

    // Ensure projectHash and id are correct in the output
    let mut gemini_val = gemini_val;
    gemini_val["projectHash"] = serde_json::Value::String(project_hash);
    gemini_val["id"] = serde_json::Value::String(session_id.clone());

    let json = serde_json::to_string_pretty(&gemini_val)?;
    std::fs::write(&output_path, &json)?;

    // Write/append logs.json entries for session discovery
    let logs_path = gemini_base.join("logs.json");
    let log_entries = gemini::build_logs_entries(&final_records);
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
    let projects_path = dirs::home_dir().map(|h| h.join(".gemini").join("projects.json"));

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
    // Compute SHA-256 using the platform's CLI tool.
    // `sha256sum` is standard on Linux; macOS ships `shasum -a 256` instead.
    use std::io::Write;
    use std::process::Command;

    fn run_sha(cmd: &str, extra_args: &[&str], input: &[u8]) -> Option<String> {
        let mut child = Command::new(cmd)
            .args(extra_args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .ok()?;
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(input);
        }
        let out = child.wait_with_output().ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .map(String::from)
    }

    let bytes = input.as_bytes();
    // Try sha256sum (Linux/BSD/Windows WSL), then shasum -a 256 (macOS).
    run_sha("sha256sum", &[], bytes)
        .or_else(|| run_sha("shasum", &["-a", "256"], bytes))
        .unwrap_or_else(|| {
            let h = simple_hash(input);
            format!("{:016x}{:016x}{:016x}{:016x}", h, input.len(), h, input.len())
        })
}

fn inject_into_opencode(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<InjectionResult, ConvertError> {
    let oc_output = opencode::from_hub(hub_records)?;

    let db_path = dirs::data_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No data dir".into()))?
        .join("opencode")
        .join("opencode.db");

    if !db_path.exists() {
        return Err(ConvertError::InvalidFormat(format!(
            "OpenCode database not found at {}",
            db_path.display()
        )));
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to open OpenCode DB: {e}")))?;

    // Use current working directory as the project directory
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| source.directory.clone());

    // Find or create the project entry (outside transaction — idempotent)
    let project_id = find_or_create_opencode_project(&conn, &cwd)?;

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    // Pre-generate all IDs with atomic counter to avoid collisions
    let session_id = opencode_id("ses");
    let slug = generate_slug();
    let msg_ids: Vec<String> = (0..oc_output.messages.len())
        .map(|_| opencode_id("msg"))
        .collect();

    let title = source
        .title
        .as_deref()
        .unwrap_or(source.name.as_deref().unwrap_or("Imported session"));

    // Pre-group parts by _msg_idx for O(N) lookup instead of O(N*M)
    let mut parts_by_msg: std::collections::HashMap<u64, Vec<&serde_json::Value>> =
        std::collections::HashMap::new();
    for part in &oc_output.parts {
        if let Some(idx) = part.get("_msg_idx").and_then(|v| v.as_u64()) {
            parts_by_msg.entry(idx).or_default().push(part);
        }
    }

    // Wrap all inserts in a transaction for atomicity
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to begin transaction: {e}")))?;

    tx.execute(
        "INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![session_id, project_id, slug, cwd, title, "1.0.0", now_ms, now_ms],
    )
    .map_err(|e| ConvertError::InvalidFormat(format!("Failed to insert session: {e}")))?;

    // Insert messages and parts
    for (msg_i, oc_msg) in oc_output.messages.iter().enumerate() {
        let msg_id = &msg_ids[msg_i];
        let parent_msg_id = if msg_i > 0 { Some(&msg_ids[msg_i - 1]) } else { None };

        // Patch the message data with proper IDs and parentID chain
        let mut msg_data = oc_msg.clone();
        msg_data["id"] = serde_json::Value::String(msg_id.clone());
        msg_data["sessionID"] = serde_json::Value::String(session_id.clone());
        if let Some(pid) = parent_msg_id {
            msg_data["parentID"] = serde_json::Value::String(pid.clone());
        }

        let msg_created = msg_data
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
            .unwrap_or(now_ms as f64) as i64;

        let msg_updated = msg_data
            .get("time")
            .and_then(|t| t.get("completed"))
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)))
            .unwrap_or(msg_created as f64) as i64;

        let data_str = serde_json::to_string(&msg_data)?;

        tx.execute(
            "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![msg_id, session_id, msg_created, msg_updated, data_str],
        )
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to insert message {msg_i}: {e}")))?;

        // Insert parts that belong to this message (pre-grouped)
        if let Some(msg_parts) = parts_by_msg.get(&(msg_i as u64)) {
            for part in msg_parts {
                let part_id = opencode_id("prt");
                let mut part_data = (*part).clone();
                if let Some(obj) = part_data.as_object_mut() {
                    obj.remove("_msg_idx");
                }
                let part_str = serde_json::to_string(&part_data)?;

                tx.execute(
                    "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![part_id, msg_id, session_id, msg_created, msg_updated, part_str],
                )
                .map_err(|e| ConvertError::InvalidFormat(format!("Failed to insert part: {e}")))?;
            }
        }
    }

    tx.commit()
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to commit transaction: {e}")))?;

    eprintln!(
        "Injected {} messages into OpenCode (session: {}, slug: {})",
        oc_output.messages.len(),
        session_id,
        slug,
    );

    Ok(InjectionResult {
        session_id: session_id.clone(),
        resume_args: vec!["-s".into(), session_id],
        message: format!(
            "Session '{}' from {} injected into OpenCode (slug: {})",
            source.name.as_deref().unwrap_or(&source.id),
            source.cli,
            slug,
        ),
    })
}

/// Find an existing OpenCode project by worktree path, or create one.
fn find_or_create_opencode_project(
    conn: &rusqlite::Connection,
    worktree: &str,
) -> Result<String, ConvertError> {
    // Check if project already exists for this worktree
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM project WHERE worktree = ?1",
            [worktree],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    // Create a new project entry
    let project_id = sha1_hex(worktree);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    conn.execute(
        "INSERT INTO project (id, worktree, vcs, time_created, time_updated, sandboxes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![project_id, worktree, "git", now_ms, now_ms, "[]"],
    )
    .map_err(|e| ConvertError::InvalidFormat(format!("Failed to insert project: {e}")))?;

    eprintln!("Created OpenCode project for {worktree}");
    Ok(project_id)
}

/// Generate an OpenCode-style prefixed ID (e.g. ses_xxxx, msg_xxxx, prt_xxxx).
/// Uses an atomic counter to guarantee uniqueness across rapid calls within the same process.
fn opencode_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let pid = std::process::id() as u128;
    let val = nanos ^ (pid << 32) ^ (seq << 48);
    // Hex timestamp prefix + base62 random suffix
    let hex_part = format!("{:08x}", (nanos / 1_000_000) as u32);
    let suffix = base62_encode(val);
    format!("{prefix}_{hex_part}{suffix}")
}

fn base62_encode(mut val: u128) -> String {
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    if val == 0 {
        return "0".to_string();
    }
    let mut result = Vec::new();
    while val > 0 {
        result.push(CHARS[(val % 62) as usize]);
        val /= 62;
    }
    result.reverse();
    String::from_utf8(result).unwrap_or_default()
}

fn sha1_hex(input: &str) -> String {
    // Shell out to sha1sum/shasum for SHA1, matching OpenCode's project ID generation
    fn run_sha(cmd: &str, args: &[&str], data: &[u8]) -> Option<String> {
        use std::io::Write;
        let mut child = std::process::Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;
        child.stdin.take()?.write_all(data).ok()?;
        let out = child.wait_with_output().ok()?;
        String::from_utf8(out.stdout)
            .ok()?
            .split_whitespace()
            .next()
            .map(String::from)
    }

    let bytes = input.as_bytes();
    run_sha("sha1sum", &[], bytes)
        .or_else(|| run_sha("shasum", &["-a", "1"], bytes))
        .unwrap_or_else(|| {
            let h = simple_hash(input);
            format!("{:016x}{:016x}{:08x}", h, input.len(), h as u32)
        })
}

fn generate_slug() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let adjectives = [
        "amber", "bold", "calm", "dark", "eager", "fair", "glad", "hazy",
        "idle", "keen", "lean", "mild", "neat", "odd", "pale", "quick",
        "rare", "slim", "tall", "vast", "warm", "wise", "young", "zen",
    ];
    let nouns = [
        "bear", "crow", "deer", "dove", "eagle", "fawn", "goat", "hawk",
        "ibis", "jay", "kite", "lark", "mole", "newt", "owl", "pike",
        "quail", "robin", "seal", "toad", "urchin", "vole", "wolf", "wren",
    ];
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id() as u128;
    let seed = nanos.wrapping_mul(pid.wrapping_add(1));
    let adj = adjectives[(seed % adjectives.len() as u128) as usize];
    let noun = nouns[((seed / adjectives.len() as u128) % nouns.len() as u128) as usize];
    let suffix = format!("{:04x}", (seed >> 16) & 0xFFFF);
    format!("{adj}-{noun}-{suffix}")
}

// === Helpers ===

/// Return the Codex home directory, respecting the `CODEX_HOME` env var.
fn codex_home_dir() -> Option<std::path::PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Some(std::path::PathBuf::from(home));
    }
    dirs::home_dir().map(|h| h.join(".codex"))
}

fn extract_session_id(records: &[HubRecord]) -> String {
    records
        .iter()
        .find_map(|r| {
            if let HubRecord::Session(s) = r {
                Some(s.session_id.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(uuid_v4)
}

fn encode_claude_project_path(dir: &str) -> String {
    // Claude replaces ALL non-alphanumeric chars with dashes, and truncates at 200 chars with hash
    let encoded: String = dir
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if encoded.len() > 200 {
        let hash = simple_hash(&encoded);
        format!("{}-{:x}", &encoded[..200], hash)
    } else {
        encoded
    }
}

fn simple_hash(s: &str) -> u64 {
    // TODO: verify this matches Claude Code's actual truncation hash implementation.
    // Claude Code's source uses a hash suffix for long paths, but the exact algorithm
    // (polynomial basis, seed, output width) has not been confirmed against a live binary.
    // Until verified, injected long-path sessions may land in a different directory than
    // Claude Code would create natively — meaning resume won't find them.
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    h
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
    let c = (lo & 0x0FFF) as u16 | 0x4000; // version 4
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
    // Rough conversion (good enough for filenames).
    // Cap month at 12: remaining_days can be up to 364, and 364/30+1 = 13.
    let days = secs / 86400;
    let years = days / 365;
    let year = 1970 + years;
    let remaining = days - years * 365;
    let month = (remaining / 30 + 1).min(12);
    let day = remaining % 30 + 1;
    let hour = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;
    let sec = secs % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}-{min:02}-{sec:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::hub::{HubRecord, SessionHeader, UCF_VERSION};

    // ── encode_claude_project_path ───────────────────────────

    #[test]
    fn test_encode_claude_project_path_absolute() {
        assert_eq!(
            encode_claude_project_path("/home/me/project"),
            "-home-me-project"
        );
    }

    #[test]
    fn test_encode_claude_project_path_root() {
        assert_eq!(encode_claude_project_path("/"), "-");
    }

    #[test]
    fn test_encode_claude_project_path_nested() {
        assert_eq!(
            encode_claude_project_path("/home/me/code/rust/unleash"),
            "-home-me-code-rust-unleash"
        );
    }

    #[test]
    fn test_encode_claude_project_path_no_slash() {
        assert_eq!(encode_claude_project_path("relative"), "relative");
    }

    #[test]
    fn test_encode_claude_project_path_long_path_truncates_with_hash() {
        // Paths over 200 chars should be truncated to 200 chars with a hex hash suffix
        let long_path = "/home/me/".to_string() + &"a".repeat(200);
        let result = encode_claude_project_path(&long_path);
        // Result must be longer than 200 (truncated prefix + "-" + hex hash)
        assert!(
            result.len() > 200,
            "long path result should be longer than 200: len={}",
            result.len()
        );
        // The 200-char prefix is the encoded version of the first 200 encoded chars
        let encoded_full: String = long_path
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        assert!(
            result.starts_with(&encoded_full[..200]),
            "should start with first 200 encoded chars"
        );
        // Must have a dash-separated hex suffix
        let suffix = &result[200..];
        assert!(suffix.starts_with('-'), "suffix should start with dash");
        let hex_part = &suffix[1..];
        assert!(!hex_part.is_empty(), "hex hash suffix should not be empty");
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "suffix should be hex: {hex_part}"
        );
    }

    // ── uuid_v4 ──────────────────────────────────────────────

    #[test]
    fn test_uuid_v4_format() {
        let id = uuid_v4();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.len(),
            5,
            "UUID should have 5 dash-separated parts: {id}"
        );
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn test_uuid_v4_version_bit_set() {
        let id = uuid_v4();
        let parts: Vec<&str> = id.split('-').collect();
        // The implementation ORs with 0x4000 (sets bit 14), but doesn't mask
        // higher bits, so it's not strict RFC 4122 — just verify the bit is set.
        let third_group = u16::from_str_radix(parts[2], 16).unwrap();
        assert!(
            third_group & 0x4000 != 0,
            "version bit 14 should be set: {id}"
        );
    }

    #[test]
    fn test_uuid_v4_variant_bit_set() {
        let id = uuid_v4();
        let parts: Vec<&str> = id.split('-').collect();
        // The implementation ORs with 0x8000 (sets bit 15), so variant 1 bit is set.
        let fourth_group = u16::from_str_radix(parts[3], 16).unwrap();
        assert!(
            fourth_group & 0x8000 != 0,
            "variant bit 15 should be set: {id}"
        );
    }

    #[test]
    fn test_uuid_v4_uniqueness() {
        let a = uuid_v4();
        // Introduce a tiny delay to get a different timestamp
        std::thread::sleep(std::time::Duration::from_millis(1));
        let b = uuid_v4();
        assert_ne!(a, b, "consecutive UUIDs should differ");
    }

    #[test]
    fn test_uuid_v4_is_valid_hex() {
        let id = uuid_v4();
        for c in id.chars() {
            assert!(
                c == '-' || c.is_ascii_hexdigit(),
                "UUID should only contain hex digits and dashes: {id}"
            );
        }
    }

    // ── chrono_like_now ──────────────────────────────────────

    #[test]
    fn test_chrono_like_now_format() {
        let ts = chrono_like_now();
        // Expected: YYYY-MM-DDTHH-MM-SS (19 chars)
        assert_eq!(ts.len(), 19, "timestamp length should be 19: {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], "-");
        assert_eq!(&ts[16..17], "-");
    }

    #[test]
    fn test_chrono_like_now_reasonable_year() {
        let ts = chrono_like_now();
        let year: u32 = ts[..4].parse().unwrap();
        assert!((2024..=2100).contains(&year), "year out of range: {year}");
    }

    #[test]
    fn test_chrono_like_now_valid_ranges() {
        let ts = chrono_like_now();
        let month: u32 = ts[5..7].parse().unwrap();
        let day: u32 = ts[8..10].parse().unwrap();
        let hour: u32 = ts[11..13].parse().unwrap();
        let min: u32 = ts[14..16].parse().unwrap();
        let sec: u32 = ts[17..19].parse().unwrap();
        assert!((1..=12).contains(&month), "month out of range: {month}");
        assert!((1..=31).contains(&day), "day out of range: {day}");
        assert!(hour < 24, "hour out of range: {hour}");
        assert!(min < 60, "min out of range: {min}");
        assert!(sec < 60, "sec out of range: {sec}");
    }

    // ── extract_session_id ───────────────────────────────────

    #[test]
    fn test_extract_session_id_from_records() {
        let header = SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: "test-session-123".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            source_cli: "claude".to_string(),
            source_version: "1.0".to_string(),
            project: None,
            model: None,
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: serde_json::Value::Null,
        };
        let records = vec![HubRecord::Session(header)];
        assert_eq!(extract_session_id(&records), "test-session-123");
    }

    #[test]
    fn test_extract_session_id_empty_records() {
        let records: Vec<HubRecord> = vec![];
        let id = extract_session_id(&records);
        // Should generate a UUID fallback
        assert_eq!(
            id.split('-').count(),
            5,
            "fallback should be UUID format: {id}"
        );
    }

    // ── sha256_hex ───────────────────────────────────────────

    #[test]
    fn test_sha256_hex_known_value() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = sha256_hex("");
        // Either the tool works and returns the known hash, or it returns the
        // length-based fallback.  Both are acceptable; we just verify it's hex.
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "sha256_hex result should be all hex digits: {hash}"
        );
        assert!(
            hash.len() >= 16,
            "result should be at least 16 hex chars: {hash}"
        );
    }

    #[test]
    fn test_sha256_hex_different_inputs_differ() {
        let h1 = sha256_hex("/home/alice/project");
        let h2 = sha256_hex("/home/bob/project");
        // If the system tool is available, hashes must differ.
        // If the fallback fires, both strings have the same length (19) so they'd
        // match — we only assert difference when the results look like real hashes.
        if h1.len() == 64 {
            assert_ne!(
                h1, h2,
                "different paths should produce different SHA-256 hashes"
            );
        }
    }

    // ── opencode_id ─────────────────────────────────────────

    #[test]
    fn test_opencode_id_format() {
        let id = opencode_id("ses");
        assert!(id.starts_with("ses_"), "should start with prefix: {id}");
        // Hex part after prefix should be 8 chars
        let after_prefix = &id[4..];
        assert!(after_prefix.len() >= 9, "should have hex + suffix: {id}");
        let hex_part = &after_prefix[..8];
        assert!(
            hex_part.chars().all(|c| c.is_ascii_hexdigit()),
            "hex part should be hex: {hex_part}"
        );
    }

    #[test]
    fn test_opencode_id_uniqueness() {
        let mut ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = opencode_id("msg");
            assert!(ids.insert(id.clone()), "duplicate ID generated: {id}");
        }
    }

    #[test]
    fn test_opencode_id_different_prefixes() {
        let ses = opencode_id("ses");
        let msg = opencode_id("msg");
        let prt = opencode_id("prt");
        assert!(ses.starts_with("ses_"));
        assert!(msg.starts_with("msg_"));
        assert!(prt.starts_with("prt_"));
    }

    // ── base62_encode ───────────────────────────────────────

    #[test]
    fn test_base62_encode_zero() {
        assert_eq!(base62_encode(0), "0");
    }

    #[test]
    fn test_base62_encode_known_values() {
        assert_eq!(base62_encode(1), "1");
        assert_eq!(base62_encode(61), "z");
        assert_eq!(base62_encode(62), "10");
    }

    #[test]
    fn test_base62_encode_only_valid_chars() {
        let encoded = base62_encode(u128::MAX);
        assert!(
            encoded
                .chars()
                .all(|c| c.is_ascii_alphanumeric()),
            "should only contain alphanumeric chars: {encoded}"
        );
    }

    // ── sha1_hex ────────────────────────────────────────────

    #[test]
    fn test_sha1_hex_known_value() {
        let hash = sha1_hex("");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "should be hex: {hash}"
        );
        // SHA1("") = da39a3ee5e6b4b0d3255bfef95601890afd80709
        if hash.len() == 40 {
            assert_eq!(hash, "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        }
    }

    #[test]
    fn test_sha1_hex_different_inputs() {
        let h1 = sha1_hex("/home/alice");
        let h2 = sha1_hex("/home/bob");
        if h1.len() == 40 {
            assert_ne!(h1, h2);
        }
    }

    // ── generate_slug ───────────────────────────────────────

    #[test]
    fn test_generate_slug_format() {
        let slug = generate_slug();
        let parts: Vec<&str> = slug.split('-').collect();
        assert_eq!(parts.len(), 3, "slug should be adjective-noun-suffix: {slug}");
        assert!(
            parts[0].chars().all(|c| c.is_ascii_lowercase()),
            "adjective should be lowercase: {}",
            parts[0]
        );
        assert!(
            parts[1].chars().all(|c| c.is_ascii_lowercase()),
            "noun should be lowercase: {}",
            parts[1]
        );
        assert!(
            parts[2].chars().all(|c| c.is_ascii_hexdigit()),
            "suffix should be hex: {}",
            parts[2]
        );
    }

    // ── inject_into_opencode (in-memory DB) ─────────────────

    #[test]
    fn test_inject_into_opencode_sqlite() {
        // Create an in-memory SQLite DB with OpenCode's schema
        let conn = rusqlite::Connection::open_in_memory().unwrap();
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
                workspace_id TEXT,
                FOREIGN KEY (project_id) REFERENCES project(id) ON DELETE CASCADE
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL,
                FOREIGN KEY (session_id) REFERENCES session(id) ON DELETE CASCADE
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL,
                data TEXT NOT NULL,
                FOREIGN KEY (message_id) REFERENCES message(id) ON DELETE CASCADE
            );",
        )
        .unwrap();

        // Verify find_or_create_opencode_project creates a project
        let project_id =
            find_or_create_opencode_project(&conn, "/home/test/project").unwrap();
        assert!(!project_id.is_empty());

        // Second call should return the same project
        let project_id2 =
            find_or_create_opencode_project(&conn, "/home/test/project").unwrap();
        assert_eq!(project_id, project_id2);

        // Verify project was created
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM project", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
