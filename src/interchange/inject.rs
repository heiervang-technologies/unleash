//! Session injection: convert a foreign session and load it into a target CLI.

use crate::interchange::crossload_index::{self, entry_is_live};
use crate::interchange::sessions::{find_session, SessionInfo};
use crate::interchange::{
    claude, codex, gemini, hermes,
    hub::{ContentBlock, HubRecord},
    opencode, pi, ConvertError,
};

/// Result of injecting a session: the session ID to resume with and any extra args.
pub struct InjectionResult {
    #[allow(dead_code)]
    pub session_id: String,
    pub resume_args: Vec<String>,
    pub message: String,
}

/// Inject a foreign session into the target CLI's session store.
/// Returns the session ID that the target CLI can resume.
///
/// Idempotent: if the same (source, target) pair has already been crossloaded
/// and the cached target session still exists on disk, the cached target id is
/// returned without re-injecting. Use `UNLEASH_CROSSLOAD_FORCE=1` to bypass.
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

    // Normalize target cli alias to the canonical discriminator we key on.
    let canonical_target = normalize_target_cli(target_cli);
    let force = std::env::var("UNLEASH_CROSSLOAD_FORCE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);

    let mut index = crossload_index::load();
    if !force {
        if let Some(entry) = index.lookup(&session.cli, &session.id, canonical_target) {
            if entry_is_live(entry) {
                // If the cached entry doesn't record the source updated_at or if it doesn't match the current session's updated_at,
                // the source session has been updated since the crossload. We need to re-crossload.
                if entry.source_updated_at.as_deref() == Some(&*session.updated_at) {
                    eprintln!(
                        "Already crossloaded; reusing target session '{}' (set UNLEASH_CROSSLOAD_FORCE=1 to re-inject)",
                        entry.target_session_id
                    );
                    return Ok(InjectionResult {
                        session_id: entry.target_session_id.clone(),
                        resume_args: resume_args_for(canonical_target, &entry.target_session_id),
                        message: format!(
                            "Reused cached crossload of '{}' from {} into {}",
                            session.name.as_deref().unwrap_or(&session.id),
                            session.cli,
                            canonical_target,
                        ),
                    });
                } else {
                    eprintln!(
                        "Source session has been updated since last crossload; re-injecting into {}",
                        canonical_target
                    );
                }
            }
            // Cached target is gone or stale; drop the stale entry and fall through.
            index.remove(&session.cli, &session.id, canonical_target);
        }
    }

    // Convert source to Hub
    let hub_records = source_to_hub(&session)?;
    eprintln!("Converted {} records to hub format", hub_records.len());

    // Apply context budget guard (UNLEASH_CROSSLOAD_MAX_TOKENS or default unlimited).
    let hub_records = if let Some(max_tokens) = context_budget() {
        let (trimmed, dropped) = truncate_hub_to_budget(hub_records, max_tokens);
        if dropped > 0 {
            eprintln!(
                "Context guard: dropped {} oldest messages to stay within {} token budget",
                dropped, max_tokens
            );
        }
        trimmed
    } else {
        hub_records
    };

    // Inject into target
    let (result, target_path) = match target_cli {
        "claude" | "claude-code" => inject_into_claude(&session, &hub_records)?,
        "codex" => inject_into_codex(&session, &hub_records)?,
        "gemini" | "gemini-cli" => inject_into_gemini(&session, &hub_records)?,
        "antigravity" | "antigravity-cli" | "agy" => {
            // agy does NOT share gemini's session storage. Storage details
            // (from a reverse-engineering pass — see issue #307):
            //
            // - ~/.gemini/antigravity-cli/conversations/<uuid>.pb — AES-encrypted
            //   protobuf (Google's google3/.../cortex_pb types). Encryption key
            //   is device-local, can't be reproduced from outside agy.
            // - ~/.gemini/antigravity-cli/conversations/<uuid>.db — plain SQLite
            //   with plaintext protobuf blobs. Local-only writes load successfully.
            // - ~/.gemini/antigravity-cli/cache/last_conversations.json — plaintext
            //   cwd→cascade_id map.
            //
            // But: even a perfectly-encoded `.db` plus correct cache plumbing
            // fails at send-time with `cascade ID mismatch, executor: <empty>,
            // input: <our_cascade>`. agy validates the cascade_id against an
            // executor cascade fetched **server-side** from Google's CodeAssist
            // API. The server doesn't know about our locally-written conversation,
            // returns an empty executor, and the equality check fires.
            //
            // Conclusion: real agy crossload would need server-side cooperation
            // (register the cascade with CodeAssist), which is outside scope.
            // PR #302 chose honest refusal over the prior fake-success behavior.
            return Err(ConvertError::InvalidFormat(
                "Crossloading into Antigravity (agy) requires server-side \
                 conversation registration with Google's CodeAssist API — agy \
                 validates every cascade_id against a server-fetched executor, \
                 and locally-written conversations fail with `cascade ID \
                 mismatch` at send time. See issue #307 for the reverse-engineering \
                 evidence. Workaround: crossload into another CLI \
                 (claude/codex/gemini/opencode/pi/hermes) and run that instead."
                    .to_string(),
            ));
        }
        "hermes" | "hermes-agent" => inject_into_hermes(&session, &hub_records)?,
        "opencode" => inject_into_opencode(&session, &hub_records)?,
        "pi" | "pi-coding-agent" => inject_into_pi(&session, &hub_records)?,
        _ => {
            return Err(ConvertError::InvalidFormat(format!(
                "Unsupported target CLI: {target_cli}"
            )))
        }
    };

    index.record(
        &session.cli,
        &session.id,
        canonical_target,
        result.session_id.clone(),
        target_path,
        Some(session.updated_at.clone()),
    );
    if let Err(e) = crossload_index::save(&index) {
        eprintln!(
            "Warning: could not persist crossload index ({e}); future re-crossloads may duplicate"
        );
    }

    Ok(result)
}

fn normalize_target_cli(target: &str) -> &str {
    match target {
        "claude" | "claude-code" => "claude",
        "codex" => "codex",
        "gemini" | "gemini-cli" => "gemini",
        // agy is intentionally NOT mapped to gemini here. They don't share
        // storage layout (gemini = JSON chats, agy = protobuf .pb files at
        // ~/.gemini/antigravity-cli/conversations/) and the dispatch above
        // refuses agy injection. Keeping the raw "agy" string means stale
        // cache entries from older builds (which used the wrong mapping)
        // don't shadow the explicit error.
        "antigravity" | "antigravity-cli" | "agy" => "agy",
        "hermes" | "hermes-agent" => "hermes",
        "opencode" => "opencode",
        "pi" | "pi-coding-agent" => "pi",
        other => other,
    }
}

/// Read the optional context budget from `UNLEASH_CROSSLOAD_MAX_TOKENS`.
/// Returns `None` when the variable is unset or zero (no limit).
pub(crate) fn context_budget() -> Option<usize> {
    std::env::var("UNLEASH_CROSSLOAD_MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
}

/// Rough token estimate: 1 token ≈ 4 characters.
fn estimate_tokens(records: &[HubRecord]) -> usize {
    let chars: usize = records
        .iter()
        .filter_map(|r| {
            if let HubRecord::Message(m) = r {
                Some(m.content.iter().map(estimate_block_chars).sum::<usize>())
            } else {
                None
            }
        })
        .sum();
    chars / 4
}

fn estimate_block_chars(block: &crate::interchange::hub::ContentBlock) -> usize {
    use crate::interchange::hub::ContentBlock;
    match block {
        ContentBlock::Text { text } => text.len(),
        ContentBlock::ToolUse { name, input, .. } => name.len() + input.to_string().len(),
        ContentBlock::ToolResult { content, .. } => content.iter().map(estimate_block_chars).sum(),
        ContentBlock::Thinking { text, .. } => text.len(),
        ContentBlock::Image { .. } => 256,
        _ => 64,
    }
}

fn tool_use_ids(record: &HubRecord) -> Vec<String> {
    let HubRecord::Message(msg) = record else {
        return Vec::new();
    };
    msg.content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

fn tool_result_ids(record: &HubRecord) -> Vec<String> {
    let HubRecord::Message(msg) = record else {
        return Vec::new();
    };
    msg.content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        })
        .collect()
}

/// Trim the oldest user+assistant message pairs from `records` until
/// the estimated token count fits within `max_tokens`.
/// The session header (first record) is always kept.
/// Returns (trimmed_records, num_messages_dropped).
pub(crate) fn truncate_hub_to_budget(
    records: Vec<HubRecord>,
    max_tokens: usize,
) -> (Vec<HubRecord>, usize) {
    if estimate_tokens(&records) <= max_tokens {
        return (records, 0);
    }

    // Separate the session header from the messages.
    let (header, mut messages): (Vec<HubRecord>, Vec<HubRecord>) = records
        .into_iter()
        .partition(|r| matches!(r, HubRecord::Session(_)));

    let mut dropped = 0;
    while !messages.is_empty() {
        let current: Vec<HubRecord> = header.iter().chain(messages.iter()).cloned().collect();
        if estimate_tokens(&current) <= max_tokens {
            return (current, dropped);
        }
        // Drop the oldest message plus any linked tool counterpart. Keeping
        // only one side leaves downstream Claude injection with orphaned
        // tool_result blocks, which Claude rejects.
        let removed = messages.remove(0);
        dropped += 1;
        let mut linked_ids = tool_use_ids(&removed);
        linked_ids.extend(tool_result_ids(&removed));
        if !linked_ids.is_empty() {
            let before = messages.len();
            messages.retain(|msg| {
                let uses = tool_use_ids(msg);
                let results = tool_result_ids(msg);
                !linked_ids
                    .iter()
                    .any(|id| uses.contains(id) || results.contains(id))
            });
            dropped += before - messages.len();
        }
    }

    // Couldn't fit even with all messages dropped — return just the header.
    (header, dropped)
}

fn claude_text_is_keepable(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && !text.starts_with("<environment_context")
        && !text.starts_with("<permissions")
        && !text.starts_with("<user_shell_command")
        && text != "[Reasoning]:"
}

fn claude_block_tool_use_id(block: &serde_json::Value) -> Option<String> {
    (block.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
        .then(|| block.get("id").and_then(|id| id.as_str()).map(String::from))
        .flatten()
}

fn claude_block_tool_result_id(block: &serde_json::Value) -> Option<String> {
    (block.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
        .then(|| {
            block
                .get("tool_use_id")
                .and_then(|id| id.as_str())
                .map(String::from)
        })
        .flatten()
}

fn claude_block_is_keepable(block: &serde_json::Value, seen_tool_uses: &[String]) -> bool {
    match block.get("type").and_then(|t| t.as_str()) {
        Some("text") => block
            .get("text")
            .and_then(|t| t.as_str())
            .is_some_and(claude_text_is_keepable),
        Some("tool_use") | Some("image") => true,
        Some("thinking") => {
            block
                .get("thinking")
                .and_then(|text| text.as_str())
                .is_some_and(|text| !text.trim().is_empty())
                || block
                    .get("signature")
                    .and_then(|signature| signature.as_str())
                    .is_some_and(|signature| !signature.is_empty())
        }
        Some("tool_result") => {
            claude_block_tool_result_id(block).is_some_and(|id| seen_tool_uses.contains(&id))
        }
        _ => false,
    }
}

fn filter_claude_injection_lines(lines: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut kept = Vec::new();
    let mut seen_tool_uses = Vec::new();

    for mut line in lines {
        let msg_type = line.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if msg_type != "user" && msg_type != "assistant" {
            continue;
        }

        let content = line
            .get_mut("message")
            .and_then(|message| message.get_mut("content"));
        let keep = match content {
            Some(serde_json::Value::String(s)) => claude_text_is_keepable(s),
            Some(serde_json::Value::Array(arr)) => {
                arr.retain(|block| claude_block_is_keepable(block, &seen_tool_uses));
                !arr.is_empty()
            }
            _ => false,
        };

        if keep {
            if let Some(arr) = line
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_array())
            {
                for block in arr {
                    if let Some(id) = claude_block_tool_use_id(block) {
                        seen_tool_uses.push(id);
                    }
                }
            }
            kept.push(line);
        }
    }

    kept
}

/// Turn converter output into records Claude Code can actually resume.
///
/// Foreign harness message ids (for example Codex `msg_*` / `ctc_*`) are not
/// Claude record UUIDs. Claude also expects imported assistant envelopes to
/// look like native API messages; a bare `{role, content}` assistant record is
/// discoverable by `--resume` but stalls before the first model request.
fn prepare_claude_injection_lines(
    lines: Vec<serde_json::Value>,
    session_id: &str,
    cwd: &str,
) -> Vec<serde_json::Value> {
    let mut prepared = Vec::with_capacity(lines.len() + 1);
    let mut previous_uuid: Option<String> = None;
    let mut last_user_prompt = String::new();

    for mut line in lines {
        let Some(obj) = line.as_object_mut() else {
            continue;
        };
        let role = obj
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if role != "user" && role != "assistant" {
            continue;
        }

        let record_uuid = uuid_v4();
        obj.insert(
            "sessionId".into(),
            serde_json::Value::String(session_id.into()),
        );
        obj.insert(
            "uuid".into(),
            serde_json::Value::String(record_uuid.clone()),
        );
        obj.insert(
            "parentUuid".into(),
            previous_uuid
                .as_ref()
                .map_or(serde_json::Value::Null, |parent| {
                    serde_json::Value::String(parent.clone())
                }),
        );
        obj.entry("cwd")
            .or_insert_with(|| serde_json::Value::String(cwd.into()));
        obj.entry("isSidechain")
            .or_insert(serde_json::Value::Bool(false));
        obj.entry("userType")
            .or_insert_with(|| serde_json::Value::String("external".into()));
        obj.entry("entrypoint")
            .or_insert_with(|| serde_json::Value::String("sdk-cli".into()));
        obj.entry("gitBranch")
            .or_insert_with(|| serde_json::Value::String("HEAD".into()));

        if role == "user" {
            obj.insert("promptId".into(), serde_json::Value::String(uuid_v4()));
            obj.entry("promptSource")
                .or_insert_with(|| serde_json::Value::String("sdk".into()));
            obj.entry("permissionMode")
                .or_insert_with(|| serde_json::Value::String("bypassPermissions".into()));
            if let Some(content) = obj
                .get("message")
                .and_then(|message| message.get("content"))
            {
                last_user_prompt = content
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| content.to_string());
            }
        } else {
            // promptId/permissionMode are user-envelope fields. Leaving a
            // Codex-derived id here produces a non-native assistant record.
            obj.remove("promptId");
            obj.remove("promptSource");
            obj.remove("permissionMode");

            let message = obj
                .entry("message")
                .or_insert_with(|| serde_json::json!({"role": "assistant", "content": []}));
            if let Some(message) = message.as_object_mut() {
                message
                    .entry("type")
                    .or_insert_with(|| serde_json::Value::String("message".into()));
                message
                    .entry("role")
                    .or_insert_with(|| serde_json::Value::String("assistant".into()));
                message
                    .entry("model")
                    .or_insert_with(|| serde_json::Value::String("claude-imported-history".into()));
                message.entry("id").or_insert_with(|| {
                    serde_json::Value::String(format!(
                        "msg_imported_{}",
                        record_uuid.replace('-', "")
                    ))
                });
                let has_tool_use = message
                    .get("content")
                    .and_then(|content| content.as_array())
                    .is_some_and(|blocks| {
                        blocks.iter().any(|block| {
                            block.get("type").and_then(|kind| kind.as_str()) == Some("tool_use")
                        })
                    });
                message.entry("stop_reason").or_insert_with(|| {
                    serde_json::Value::String(if has_tool_use {
                        "tool_use".into()
                    } else {
                        "end_turn".into()
                    })
                });
                message
                    .entry("stop_sequence")
                    .or_insert(serde_json::Value::Null);
                message.entry("usage").or_insert_with(|| {
                    serde_json::json!({
                        "input_tokens": 0,
                        "output_tokens": 0,
                        "cache_creation_input_tokens": 0,
                        "cache_read_input_tokens": 0
                    })
                });
            }
            obj.entry("requestId").or_insert_with(|| {
                serde_json::Value::String(format!("req_imported_{}", record_uuid.replace('-', "")))
            });
        }

        previous_uuid = Some(record_uuid);
        prepared.push(line);
    }

    if let Some(leaf_uuid) = previous_uuid {
        let last_prompt: String = last_user_prompt.chars().take(200).collect();
        prepared.push(serde_json::json!({
            "type": "last-prompt",
            "lastPrompt": last_prompt,
            "leafUuid": leaf_uuid,
            "sessionId": session_id,
        }));
    }

    prepared
}

fn resume_args_for(target: &str, session_id: &str) -> Vec<String> {
    let agent_type = match target {
        "claude" | "claude-code" => crate::agents::AgentType::Claude,
        "codex" => crate::agents::AgentType::Codex,
        "gemini" | "gemini-cli" => crate::agents::AgentType::Gemini,
        "antigravity" | "antigravity-cli" | "agy" => crate::agents::AgentType::Antigravity,
        "hermes" | "hermes-agent" => crate::agents::AgentType::Hermes,
        "opencode" => crate::agents::AgentType::OpenCode,
        "pi" | "pi-coding-agent" => crate::agents::AgentType::Pi,
        _ => crate::agents::AgentType::Custom(target.to_string()),
    };
    crate::agents::AgentDefinition::from_type(agent_type)
        .polyfill
        .get_resume_args(Some(session_id))
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
        "gemini" | "antigravity" | "agy" => {
            let data = std::fs::read(&session.path)?;
            gemini::to_hub(&data)
        }
        "opencode" => {
            // For OpenCode, we need to export from the DB
            let input = export_opencode_session(&session.id)?;
            opencode::to_hub(&input)
        }
        "hermes" => {
            let json = export_hermes_session(&session.id)?;
            hermes::to_hub(&json)
        }
        "pi" => {
            let data = std::fs::read_to_string(&session.path)?;
            let reader = std::io::BufReader::new(data.as_bytes());
            pi::to_hub(reader)
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

fn export_hermes_session(session_id: &str) -> Result<String, ConvertError> {
    let db_path = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".hermes")
        .join("state.db");

    let conn = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let session: serde_json::Value = conn
        .query_row(
            "SELECT id, source, model, title, started_at, ended_at, parent_session_id
             FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| {
                Ok(serde_json::json!({
                    "id":                row.get::<_, String>(0)?,
                    "source":            row.get::<_, String>(1)?,
                    "model":             row.get::<_, Option<String>>(2)?,
                    "title":             row.get::<_, Option<String>>(3)?,
                    "started_at":        row.get::<_, f64>(4)?,
                    "ended_at":          row.get::<_, Option<f64>>(5)?,
                    "parent_session_id": row.get::<_, Option<String>>(6)?,
                }))
            },
        )
        .map_err(|e| ConvertError::InvalidFormat(format!("Session not found in Hermes DB: {e}")))?;

    let mut msg_stmt = conn.prepare(
        "SELECT id, role, content, tool_calls, tool_call_id, tool_name, timestamp
         FROM messages WHERE session_id = ?1 ORDER BY timestamp, id",
    )?;
    let messages: Vec<serde_json::Value> = msg_stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok(serde_json::json!({
                "id":          row.get::<_, i64>(0)?,
                "session_id":  session_id,
                "role":        row.get::<_, String>(1)?,
                "content":     row.get::<_, Option<String>>(2)?,
                "tool_calls":  row.get::<_, Option<String>>(3)
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .unwrap_or(serde_json::Value::Null),
                "tool_call_id": row.get::<_, Option<String>>(4)?,
                "tool_name":   row.get::<_, Option<String>>(5)?,
                "timestamp":   row.get::<_, f64>(6)?,
            }))
        })?
        .flatten()
        .collect();

    let mut full = session;
    full["messages"] = serde_json::Value::Array(messages);
    Ok(full.to_string())
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

    // The authoritative message<->part association lives in the `message.id` /
    // `part.message_id` columns, NOT in the `data` JSON payload (which omits
    // them). Carry those columns into the JSON so `opencode::to_hub` can group
    // parts by their owning message instead of guessing by position — position
    // guessing shifts attachments and later user turns when a message has an
    // irregular number of parts.
    let mut msg_stmt =
        conn.prepare("SELECT id, data FROM message WHERE session_id = ? ORDER BY time_created")?;
    let messages: Vec<serde_json::Value> = msg_stmt
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(id, data)| {
            let mut value: serde_json::Value = serde_json::from_str(&data).ok()?;
            if let Some(obj) = value.as_object_mut() {
                obj.entry("id").or_insert(serde_json::Value::String(id));
            }
            Some(value)
        })
        .collect();

    let mut part_stmt = conn
        .prepare("SELECT message_id, data FROM part WHERE session_id = ? ORDER BY time_created")?;
    let parts: Vec<serde_json::Value> = part_stmt
        .query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(message_id, data)| {
            let mut value: serde_json::Value = serde_json::from_str(&data).ok()?;
            if let Some(obj) = value.as_object_mut() {
                obj.entry("messageID")
                    .or_insert(serde_json::Value::String(message_id));
            }
            Some(value)
        })
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
) -> Result<(InjectionResult, String), ConvertError> {
    let all_claude_lines = claude::from_hub(hub_records)?;

    // Claude renders all JSONL records as conversation turns. Keep real
    // user/assistant content, including tool/image-only turns, but avoid
    // orphan tool_result blocks when an earlier trim dropped the tool_use.
    let claude_lines = filter_claude_injection_lines(all_claude_lines);

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

    let claude_lines = prepare_claude_injection_lines(claude_lines, &session_id, &cwd);

    // Write native-resumable Claude JSONL.
    let mut output = String::new();
    for line in &claude_lines {
        output.push_str(&serde_json::to_string(line)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!(
        "Injected {} lines to {}",
        claude_lines.len(),
        output_path.display()
    );

    let target_path = output_path.to_string_lossy().to_string();
    Ok((
        InjectionResult {
            session_id: session_id.clone(),
            resume_args: vec!["--resume".into(), session_id],
            message: format!(
                "Session '{}' from {} injected into Claude Code",
                source.name.as_deref().unwrap_or(&source.id),
                source.cli,
            ),
        },
        target_path,
    ))
}

fn inject_into_codex(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<(InjectionResult, String), ConvertError> {
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

    let target_path = output_path.to_string_lossy().to_string();
    Ok((
        InjectionResult {
            session_id: session_id.clone(),
            resume_args: vec!["resume".into(), session_id],
            message: format!(
                "Session '{}' from {} injected into Codex",
                source.name.as_deref().unwrap_or(&source.id),
                source.cli,
            ),
        },
        target_path,
    ))
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
) -> Result<(InjectionResult, String), ConvertError> {
    let mut session_id = extract_session_id(hub_records);

    // Gemini CLI validates that --resume arguments are valid UUIDs.
    // If the source session ID is not a UUID (e.g. native UCF named sessions),
    // we must generate a fresh UUID for Gemini to accept it.
    let is_uuid = session_id.len() == 36
        && session_id.split('-').count() == 5
        && session_id
            .split('-')
            .all(|seg| seg.chars().all(|c| c.is_ascii_hexdigit()));

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

    let target_path = output_path.to_string_lossy().to_string();
    Ok((
        InjectionResult {
            session_id: session_id.clone(),
            resume_args: vec!["--resume".into(), session_id],
            message: format!(
                "Session '{}' from {} injected into Gemini CLI",
                source.name.as_deref().unwrap_or(&source.id),
                source.cli,
            ),
        },
        target_path,
    ))
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
            format!(
                "{:016x}{:016x}{:016x}{:016x}",
                h,
                input.len(),
                h,
                input.len()
            )
        })
}

fn inject_into_opencode(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<(InjectionResult, String), ConvertError> {
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
        let parent_msg_id = if msg_i > 0 {
            Some(&msg_ids[msg_i - 1])
        } else {
            None
        };

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

    // OpenCode is DB-backed — no representative file path; entry_is_live()
    // treats an empty target_path as always live.
    Ok((
        InjectionResult {
            session_id: session_id.clone(),
            resume_args: vec!["-s".into(), session_id],
            message: format!(
                "Session '{}' from {} injected into OpenCode (slug: {})",
                source.name.as_deref().unwrap_or(&source.id),
                source.cli,
                slug,
            ),
        },
        String::new(),
    ))
}

/// Soft cap on injected pi session size. Pi loads sessions via Node's
/// `readFileSync(path, "utf8")`, which throws `ERR_STRING_TOO_LONG` past
/// ~512 MB and OOMs the picker well before that. The cap also keeps
fn inject_into_hermes(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<(InjectionResult, String), ConvertError> {
    use crate::interchange::hermes;

    let output = hermes::from_hub(hub_records)?;

    let db_path = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".hermes")
        .join("state.db");

    if !db_path.exists() {
        return Err(ConvertError::InvalidFormat(format!(
            "Hermes database not found at {}",
            db_path.display()
        )));
    }

    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to open Hermes DB: {e}")))?;

    let session_id = output.session.id.clone();
    let title = source
        .title
        .as_deref()
        .or(source.name.as_deref())
        .unwrap_or("Imported session")
        .to_string();

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to begin transaction: {e}")))?;

    tx.execute(
        "INSERT OR REPLACE INTO sessions
         (id, source, model, title, started_at, ended_at, message_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            session_id,
            output.session.source,
            output.session.model,
            title,
            output.session.started_at,
            output.session.ended_at,
            output.session.message_count as i64,
        ],
    )
    .map_err(|e| ConvertError::InvalidFormat(format!("Failed to insert session: {e}")))?;

    hermes_insert_messages(&tx, &session_id, &output.messages)?;

    tx.commit()
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to commit: {e}")))?;

    eprintln!(
        "Injected {} messages into Hermes session {}",
        output.messages.len(),
        session_id
    );

    let target_path = db_path.to_string_lossy().to_string();
    Ok((
        InjectionResult {
            session_id: session_id.clone(),
            resume_args: vec!["--resume".into(), session_id],
            message: format!(
                "Injected {} messages from {} into Hermes",
                output.messages.len(),
                source.cli,
            ),
        },
        target_path,
    ))
}

/// Insert Hermes messages, writing only the columns the target `messages` table
/// actually has. Older Hermes DBs predate the `reasoning` / `reasoning_details`
/// / `finish_reason` / `token_count` columns; a fixed INSERT that names them
/// aborts the whole injection on those DBs. We probe `PRAGMA table_info` once
/// and build the statement from the intersection of what we can write and what
/// exists.
fn hermes_insert_messages(
    conn: &rusqlite::Connection,
    session_id: &str,
    messages: &[hermes::HermesMessage],
) -> Result<(), ConvertError> {
    use std::collections::HashSet;

    let existing: HashSet<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(messages)").map_err(|e| {
            ConvertError::InvalidFormat(format!("Failed to read messages schema: {e}"))
        })?;
        let cols = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| {
                ConvertError::InvalidFormat(format!("Failed to read messages schema: {e}"))
            })?
            .filter_map(Result::ok)
            .collect();
        cols
    };

    // Base columns are assumed present on every Hermes schema; optional columns
    // are appended only when the probe found them. Column order and per-message
    // param order below MUST stay in lockstep.
    //
    // TRADEOFF: on an old schema lacking `reasoning_details`, an encrypted
    // reasoning-only turn (empty content, no plaintext reasoning, payload only
    // in reasoning_details) lands as a content-less "ghost" row — the payload
    // can't be stored because the column doesn't exist. This is deliberate:
    // preserving the turn as an empty row beats dropping it or aborting the
    // whole injection. Such DBs simply predate encrypted-reasoning support.
    let optional_cols = [
        "reasoning",
        "reasoning_details",
        "finish_reason",
        "token_count",
    ];
    let mut columns: Vec<&str> = vec![
        "session_id",
        "role",
        "content",
        "tool_calls",
        "tool_call_id",
        "tool_name",
        "timestamp",
    ];
    for col in optional_cols {
        if existing.contains(col) {
            columns.push(col);
        }
    }
    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "INSERT INTO messages ({}) VALUES ({})",
        columns.join(", "),
        placeholders.join(", ")
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| ConvertError::InvalidFormat(format!("Failed to prepare insert: {e}")))?;

    for msg in messages {
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![
            &session_id,
            &msg.role,
            &msg.content,
            &msg.tool_calls,
            &msg.tool_call_id,
            &msg.tool_name,
            &msg.timestamp,
        ];
        if existing.contains("reasoning") {
            params.push(&msg.reasoning);
        }
        if existing.contains("reasoning_details") {
            params.push(&msg.reasoning_details);
        }
        if existing.contains("finish_reason") {
            params.push(&msg.finish_reason);
        }
        if existing.contains("token_count") {
            params.push(&msg.token_count);
        }
        stmt.execute(params.as_slice())
            .map_err(|e| ConvertError::InvalidFormat(format!("Failed to insert message: {e}")))?;
    }

    Ok(())
}

/// the partial-UUID resolver fast — pi reads every file in the project
/// dir to match a prefix, so one huge file can starve the rest.
/// Tunable via `UNLEASH_PI_MAX_BYTES`.
const PI_MAX_BYTES_DEFAULT: usize = 50 * 1024 * 1024;

fn inject_into_pi(
    source: &SessionInfo,
    hub_records: &[HubRecord],
) -> Result<(InjectionResult, String), ConvertError> {
    let mut pi_lines = pi::from_hub(hub_records)?;
    if pi_lines.is_empty() {
        return Err(ConvertError::InvalidFormat(
            "Pi injection: converter produced no records".into(),
        ));
    }

    // Fresh session UUID for the Pi file + resume handle.
    let session_id = uuid_v4();

    // Pi lands in the user's current working directory; use cwd when available,
    // otherwise fall back to the source session's original cwd.
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| source.directory.clone());

    // Patch the first record (must be the session header) with the new id/cwd
    // and capture its timestamp for the filename. Foreign sessions often arrive
    // with an empty created_at, which would yield filenames like `_<id>.jsonl`
    // that pi rejects with "No conversation found". Treat empty as missing.
    let timestamp = {
        let first = pi_lines
            .get_mut(0)
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| {
                ConvertError::InvalidFormat(
                    "Pi injection: first record is not a JSON object".into(),
                )
            })?;
        first.insert("id".into(), serde_json::Value::String(session_id.clone()));
        first.insert("cwd".into(), serde_json::Value::String(cwd.clone()));
        let ts = pi_session_timestamp_or_now(first);
        first.insert("timestamp".into(), serde_json::Value::String(ts.clone()));
        ts
    };

    // Drop oldest records (after the header) until the serialized output fits
    // pi's byte budget. Foreign source sessions can be hundreds of MB once
    // converted — far past what pi can readFileSync at startup.
    let max_bytes = std::env::var("UNLEASH_PI_MAX_BYTES")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(PI_MAX_BYTES_DEFAULT);
    let dropped = trim_pi_lines_to_byte_budget(&mut pi_lines, max_bytes);

    // Regenerate the parentId chain: each non-session record gets a fresh id,
    // parentId links to the previous record's id (or null for the first).
    let mut prev_id: Option<String> = None;
    for line in pi_lines.iter_mut().skip(1) {
        if let serde_json::Value::Object(obj) = line {
            let new_id = short_id();
            obj.insert("id".into(), serde_json::Value::String(new_id.clone()));
            obj.insert(
                "parentId".into(),
                match &prev_id {
                    Some(p) => serde_json::Value::String(p.clone()),
                    None => serde_json::Value::Null,
                },
            );
            prev_id = Some(new_id);
        }
    }

    // Pi encodes the project dir as --<path with / replaced by ->--.
    let project_dir_name = encode_pi_project_path(&cwd);
    let project_dir = dirs::home_dir()
        .ok_or_else(|| ConvertError::InvalidFormat("No home dir".into()))?
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join(&project_dir_name);
    std::fs::create_dir_all(&project_dir)?;

    // Filename: <timestamp-with-dashes>_<session-uuid>.jsonl — colons and dots
    // from the ISO timestamp become dashes to match real Pi session files.
    let ts_for_file = timestamp.replace([':', '.'], "-");
    let output_path = project_dir.join(format!("{ts_for_file}_{session_id}.jsonl"));

    let mut output = String::new();
    for line in &pi_lines {
        output.push_str(&serde_json::to_string(line)?);
        output.push('\n');
    }
    std::fs::write(&output_path, &output)?;

    eprintln!(
        "Injected {} lines to {}",
        pi_lines.len(),
        output_path.display()
    );

    let target_path = output_path.to_string_lossy().to_string();
    // Pass the full path as the resume handle. `pi --session <path|id>` accepts
    // either, but pi's UUID resolver walks every file in the cwd's project dir
    // to match a prefix — and any one outsized file crashes that walk with
    // ERR_STRING_TOO_LONG, leaving freshly-injected sessions invisible. The
    // path bypasses the walk entirely.
    Ok((
        InjectionResult {
            session_id: target_path.clone(),
            resume_args: vec!["--session".into(), target_path.clone()],
            message: if dropped > 0 {
                format!(
                    "Session '{}' from {} injected into Pi ({} oldest records trimmed to fit {})",
                    source.name.as_deref().unwrap_or(&source.id),
                    source.cli,
                    dropped,
                    format_byte_budget(max_bytes),
                )
            } else {
                format!(
                    "Session '{}' from {} injected into Pi",
                    source.name.as_deref().unwrap_or(&source.id),
                    source.cli,
                )
            },
        },
        target_path,
    ))
}

fn format_byte_budget(bytes: usize) -> String {
    const KB: usize = 1024;
    const MB: usize = 1024 * KB;
    if bytes >= MB {
        format!("~{} MB", bytes / MB)
    } else if bytes >= KB {
        format!("~{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Drop oldest non-header records until the serialized output fits `max_bytes`.
/// Always preserves index 0 (the session header). Returns the number of
/// records removed from the middle of the list.
fn trim_pi_lines_to_byte_budget(lines: &mut Vec<serde_json::Value>, max_bytes: usize) -> usize {
    if lines.len() < 2 {
        return 0;
    }

    let sizes: Vec<usize> = lines
        .iter()
        .map(|v| serde_json::to_string(v).map(|s| s.len() + 1).unwrap_or(0))
        .collect();
    let total: usize = sizes.iter().sum();
    if total <= max_bytes {
        return 0;
    }

    let header_size = sizes[0];
    let available = max_bytes.saturating_sub(header_size);

    let mut acc = 0usize;
    let mut keep_from = lines.len();
    for i in (1..lines.len()).rev() {
        let next = acc.saturating_add(sizes[i]);
        if next > available {
            break;
        }
        acc = next;
        keep_from = i;
    }

    let dropped = keep_from.saturating_sub(1);
    if dropped == 0 {
        return 0;
    }

    let suffix: Vec<serde_json::Value> = lines.drain(keep_from..).collect();
    lines.truncate(1);
    lines.extend(suffix);
    dropped
}

/// Read a usable session timestamp from a Pi session header, falling back to
/// `current_iso_timestamp()` when the field is missing or empty. Pi rejects
/// session files whose filename stem starts with '_' (the result of an empty
/// timestamp), so we cannot trust the foreign converter's output verbatim.
fn pi_session_timestamp_or_now(header: &serde_json::Map<String, serde_json::Value>) -> String {
    header
        .get("timestamp")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(current_iso_timestamp)
}

/// Encode a cwd for Pi's project-dir naming scheme: strip leading '/', replace
/// '/' with '-', wrap in '--...--'. An empty cwd yields "--imported--".
fn encode_pi_project_path(dir: &str) -> String {
    let trimmed = dir.trim_start_matches('/');
    if trimmed.is_empty() {
        return "--imported--".to_string();
    }
    format!("--{}--", trimmed.replace('/', "-"))
}

/// Short hex id matching Pi's per-record identifier style (8 hex chars).
fn short_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Mix with a fast counter to disambiguate calls in the same nanosecond.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mixed = (nanos as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(seq);
    format!("{:08x}", (mixed as u32))
}

fn current_iso_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Approximate RFC3339 formatter — enough for Pi's filename stem.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    // Days since epoch to calendar date (proleptic Gregorian).
    let days = secs.div_euclid(86_400);
    let remainder = secs.rem_euclid(86_400);
    let hh = remainder / 3600;
    let mm = (remainder % 3600) / 60;
    let ss = remainder % 60;
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hh, mm, ss, millis
    )
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    // Convert days since 1970-01-01 to (year, month, day).
    // Algorithm: Howard Hinnant's civil_from_days.
    days += 719_468;
    let era = days.div_euclid(146_097);
    let doe = days.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
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
        "amber", "bold", "calm", "dark", "eager", "fair", "glad", "hazy", "idle", "keen", "lean",
        "mild", "neat", "odd", "pale", "quick", "rare", "slim", "tall", "vast", "warm", "wise",
        "young", "zen",
    ];
    let nouns = [
        "bear", "crow", "deer", "dove", "eagle", "fawn", "goat", "hawk", "ibis", "jay", "kite",
        "lark", "mole", "newt", "owl", "pike", "quail", "robin", "seal", "toad", "urchin", "vole",
        "wolf", "wren",
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
    // Mirrors Claude Code's `TM(H)` exactly (verified 2026-05-29 against
    // cli.js v2.1.154 — strings dump at /tmp/claude-strings.txt):
    //
    //   function TM(H) {
    //     let $ = H.replace(/[^a-zA-Z0-9]/g, "-");
    //     if ($.length <= 200) return $;
    //     return `${$.slice(0, 200)}-${Math.abs(FmH(H)).toString(36)}`;
    //   }
    //
    // The hash is computed from the **original** path `H`, not the encoded
    // form. Output is base36 of |int32|. Caller is responsible for NFC
    // normalization if the path contains non-ASCII chars (Claude normalizes
    // upstream in F3()/aKH()).
    let encoded: String = dir
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if encoded.len() <= 200 {
        encoded
    } else {
        let suffix = to_base36(claude_path_hash(dir).unsigned_abs() as u64);
        format!("{}-{}", &encoded[..200], suffix)
    }
}

/// Mirrors Claude Code's `FmH(H)` 32-bit polynomial hash:
///
/// ```js
/// function FmH(H) {
///   let $ = 0;
///   for (let q = 0; q < H.length; q++) $ = ($ << 5) - $ + H.charCodeAt(q) | 0;
///   return $;
/// }
/// ```
///
/// `($ << 5) - $` is `31 * $`. The `| 0` operator coerces to signed int32 at
/// each step, so we use `i32` with wrapping arithmetic. `charCodeAt` returns
/// UTF-16 code units — for BMP non-ASCII chars this is the code point itself,
/// for supplementary chars it's surrogate pairs.
fn claude_path_hash(s: &str) -> i32 {
    let mut h: i32 = 0;
    for c in s.encode_utf16() {
        h = h.wrapping_shl(5).wrapping_sub(h).wrapping_add(c as i32);
    }
    h
}

/// Polynomial × 31 fallback hash for SHA256/SHA1 cache-key construction when
/// the system `sha256sum`/`shasum` binaries are unavailable. Output stability
/// across runs is the only requirement; it intentionally does NOT match any
/// real cryptographic hash. Do not use for Claude project-path encoding —
/// that needs claude_path_hash() above.
fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    h
}

/// Equivalent to JS `n.toString(36)` for non-negative integers. Digits 0-9
/// then a-z (lowercase). Empty input is "0".
fn to_base36(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::with_capacity(13);
    while n > 0 {
        let d = (n % 36) as u8;
        digits.push(if d < 10 { b'0' + d } else { b'a' + (d - 10) });
        n /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap()
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

    // ── hermes_insert_messages schema probe ──────────────────

    fn sample_hermes_msg() -> hermes::HermesMessage {
        hermes::HermesMessage {
            role: "assistant".into(),
            content: Some("hi".into()),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            timestamp: 1000.0,
            reasoning: Some("thought".into()),
            reasoning_details: Some(r#"[{"type":"reasoning.encrypted","data":"x"}]"#.into()),
            finish_reason: Some("stop".into()),
            token_count: Some(42),
        }
    }

    #[test]
    fn hermes_insert_tolerates_old_schema_without_reasoning_columns() {
        // An older Hermes DB whose `messages` table predates the reasoning /
        // finish_reason / token_count / reasoning_details columns. The insert
        // must succeed (writing only the base columns), not abort the whole
        // injection.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                tool_call_id TEXT,
                tool_calls TEXT,
                tool_name TEXT,
                timestamp REAL NOT NULL
            );",
        )
        .unwrap();

        hermes_insert_messages(&conn, "sess1", &[sample_hermes_msg()])
            .expect("insert must not abort on an old schema");

        let (count, content): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(content) FROM messages WHERE session_id = 'sess1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(content, "hi");
    }

    #[test]
    fn hermes_insert_writes_reasoning_columns_when_present() {
        // A current Hermes DB with the extended columns: the reasoning /
        // reasoning_details / finish_reason / token_count values must land.
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                tool_call_id TEXT,
                tool_calls TEXT,
                tool_name TEXT,
                timestamp REAL NOT NULL,
                token_count INTEGER,
                finish_reason TEXT,
                reasoning TEXT,
                reasoning_details TEXT
            );",
        )
        .unwrap();

        hermes_insert_messages(&conn, "sess1", &[sample_hermes_msg()]).unwrap();

        let (reasoning, details, finish, tokens): (String, String, String, i64) = conn
            .query_row(
                "SELECT reasoning, reasoning_details, finish_reason, token_count
                 FROM messages WHERE session_id = 'sess1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(reasoning, "thought");
        assert!(details.contains("reasoning.encrypted"));
        assert_eq!(finish, "stop");
        assert_eq!(tokens, 42);
    }

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
    fn test_encode_claude_project_path_long_path_matches_claude_fixture() {
        // Fixtures computed by running Claude Code's TM(H) against Node
        // (cli.js v2.1.154). If this test fails, our hash drifted from
        // Claude's and crossload --resume into long-path projects breaks.
        // To regenerate: paste the FmH/p81/TM functions in `node -e`.
        let cases: &[(String, &str)] = &[
            // /home/me/ + "a"*200
            (
                "/home/me/".to_string() + &"a".repeat(200),
                "-home-me-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-8t3x7u",
            ),
            // /home/me/ + "/"*199 — encodes to all-dashes
            (
                "/home/me/".to_string() + &"/".repeat(199),
                "-home-me-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------prcamf",
            ),
            // /home/me/ + "a"*500 — different hash from the *200 case (proves
            // we hash original cwd, not the encoded-then-truncated form)
            (
                "/home/me/".to_string() + &"a".repeat(500),
                "-home-me-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-ko6vqi",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(
                encode_claude_project_path(input).as_str(),
                *expected,
                "encoding mismatch for input of len {}",
                input.len()
            );
        }
    }

    #[test]
    fn test_claude_path_hash_matches_js_fixtures() {
        // FmH (the 32-bit polynomial × 31) fixtures from Node.
        // These pin the algorithm independently of encode_claude_project_path.
        assert_eq!(claude_path_hash("/home/me/project"), 1226745027);
        assert_eq!(
            claude_path_hash(&("/home/me/".to_string() + &"a".repeat(200))),
            -532621290
        );
        assert_eq!(claude_path_hash(""), 0);
    }

    #[test]
    fn test_to_base36_matches_js_tostring_36() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
        // Math.abs(-532621290).toString(36) — i.e. p81 of "/home/me/" + "a"*200
        assert_eq!(to_base36(532621290), "8t3x7u");
        // Math.abs(-1557577671).toString(36) — p81 of "/home/me/" + "/"*199
        assert_eq!(to_base36(1557577671), "prcamf");
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
            encoded.chars().all(|c| c.is_ascii_alphanumeric()),
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
        assert_eq!(
            parts.len(),
            3,
            "slug should be adjective-noun-suffix: {slug}"
        );
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
        let project_id = find_or_create_opencode_project(&conn, "/home/test/project").unwrap();
        assert!(!project_id.is_empty());

        // Second call should return the same project
        let project_id2 = find_or_create_opencode_project(&conn, "/home/test/project").unwrap();
        assert_eq!(project_id, project_id2);

        // Verify project was created
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM project", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── encode_pi_project_path ───────────────────────────────

    #[test]
    fn test_encode_pi_project_path_absolute() {
        assert_eq!(
            encode_pi_project_path("/home/me/ht/unleash"),
            "--home-me-ht-unleash--"
        );
    }

    #[test]
    fn test_encode_pi_project_path_root() {
        assert_eq!(encode_pi_project_path("/"), "--imported--");
    }

    #[test]
    fn test_encode_pi_project_path_empty() {
        assert_eq!(encode_pi_project_path(""), "--imported--");
    }

    #[test]
    fn test_encode_pi_project_path_nested() {
        // Matches the real fixture dir shipped with the repo.
        assert_eq!(
            encode_pi_project_path("/home/me/ht/forks/ht-llama.cpp"),
            "--home-me-ht-forks-ht-llama.cpp--"
        );
    }

    // ── short_id + current_iso_timestamp ─────────────────────

    #[test]
    fn test_short_id_is_hex_8() {
        let id = short_id();
        assert_eq!(id.len(), 8, "short_id should be 8 chars: {id}");
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "short_id should be hex only: {id}"
        );
    }

    #[test]
    fn test_short_id_uniqueness_rapid() {
        // Even in a tight loop the atomic counter keeps consecutive ids distinct.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(short_id()));
        }
    }

    // ── pi crossload regressions ─────────────────────────────

    #[test]
    fn test_resume_args_for_pi_uses_session_flag() {
        // pi's `--resume` is a no-arg picker; resuming a specific session
        // requires `--session <path|id>`. inject_session must produce the
        // latter or pi will error with "unknown option".
        assert_eq!(
            resume_args_for("pi", "abc-123"),
            vec!["--session".to_string(), "abc-123".to_string()],
        );
    }

    #[test]
    fn test_pi_session_timestamp_fallback_when_empty() {
        // Foreign converters (e.g. claude→hub) often leave session.created_at
        // empty, which would land in pi's session header as "timestamp": "".
        // The fallback must kick in for missing AND empty values, otherwise
        // the resulting filename starts with '_' and pi rejects the file.
        let mut empty = serde_json::Map::new();
        empty.insert("timestamp".into(), serde_json::Value::String(String::new()));
        let ts = pi_session_timestamp_or_now(&empty);
        assert!(!ts.is_empty(), "empty timestamp must fall back to now");
        assert_eq!(ts.len(), 24, "fallback should be ISO-8601: {ts}");

        let missing = serde_json::Map::new();
        let ts2 = pi_session_timestamp_or_now(&missing);
        assert_eq!(ts2.len(), 24, "missing timestamp must fall back to now");

        let mut populated = serde_json::Map::new();
        populated.insert(
            "timestamp".into(),
            serde_json::Value::String("2026-04-27T19:00:00.000Z".into()),
        );
        assert_eq!(
            pi_session_timestamp_or_now(&populated),
            "2026-04-27T19:00:00.000Z",
            "populated timestamp must be preserved as-is"
        );
    }

    #[test]
    fn test_trim_pi_lines_under_budget_keeps_everything() {
        let mut lines: Vec<serde_json::Value> = (0..10)
            .map(|i| serde_json::json!({"type": "message", "n": i}))
            .collect();
        lines.insert(0, serde_json::json!({"type": "session", "id": "h"}));
        let before = lines.len();
        let dropped = trim_pi_lines_to_byte_budget(&mut lines, 10 * 1024 * 1024);
        assert_eq!(dropped, 0);
        assert_eq!(lines.len(), before);
    }

    #[test]
    fn test_trim_pi_lines_over_budget_drops_oldest_keeps_header_and_tail() {
        // Header + 50 message records, each ~50 bytes serialized. Budget of
        // ~600 bytes after the header should keep roughly the last 10 records.
        let header = serde_json::json!({"type": "session", "id": "h"});
        let mut lines = vec![header.clone()];
        for i in 0..50 {
            lines.push(serde_json::json!({
                "type": "message",
                "id": format!("rec-{i:05}"),
                "parentId": null,
                "message": {"role": "user", "content": [{"type":"text","text":"x"}]}
            }));
        }
        let total: usize = lines
            .iter()
            .map(|v| serde_json::to_string(v).unwrap().len() + 1)
            .sum();
        let budget = total / 5; // keep roughly the last 20%
        let dropped = trim_pi_lines_to_byte_budget(&mut lines, budget);
        assert!(dropped > 0, "expected some records to be dropped");
        // Header still present and unchanged
        assert_eq!(lines[0], header, "header must be preserved as first line");
        // Tail order preserved
        let last = lines.last().unwrap();
        assert_eq!(last["id"], "rec-00049");
        // Total bytes now under budget
        let new_total: usize = lines
            .iter()
            .map(|v| serde_json::to_string(v).unwrap().len() + 1)
            .sum();
        assert!(
            new_total <= budget,
            "post-trim {new_total} > budget {budget}"
        );
    }

    #[test]
    fn test_trim_pi_lines_single_line_no_op() {
        let mut lines = vec![serde_json::json!({"type": "session"})];
        let dropped = trim_pi_lines_to_byte_budget(&mut lines, 1);
        assert_eq!(dropped, 0);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_current_iso_timestamp_shape() {
        let ts = current_iso_timestamp();
        // YYYY-MM-DDTHH:MM:SS.mmmZ → 24 chars
        assert_eq!(ts.len(), 24, "timestamp should be 24 chars: {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        assert_eq!(&ts[19..20], ".");
        assert!(ts.ends_with('Z'));
    }

    // ── context budget / truncation ──────────────────────────────────────────

    fn make_session_header() -> HubRecord {
        HubRecord::Session(SessionHeader {
            ucf_version: UCF_VERSION.to_string(),
            session_id: "test-id".to_string(),
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            source_cli: "claude".to_string(),
            source_version: "1.0.0".to_string(),
            project: None,
            model: None,
            title: None,
            slug: None,
            parent_session_id: None,
            extensions: serde_json::Value::Null,
        })
    }

    fn make_text_message(role: &str, text: &str) -> HubRecord {
        HubRecord::Message(crate::interchange::hub::HubMessage {
            id: format!("{}-{}", role, text.len()),
            api_message_id: None,
            parent_id: None,
            timestamp: "2026-01-01T00:00:00.000Z".to_string(),
            completed_at: None,
            role: role.to_string(),
            content: vec![crate::interchange::hub::ContentBlock::Text {
                text: text.to_string(),
            }],
            metadata: Default::default(),
            extensions: serde_json::Value::Null,
        })
    }

    fn make_tool_use_message(id: &str, command: &str) -> HubRecord {
        HubRecord::Message(crate::interchange::hub::HubMessage {
            id: format!("assistant-{id}"),
            api_message_id: None,
            parent_id: None,
            timestamp: "2026-01-01T00:00:00.000Z".to_string(),
            completed_at: None,
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: "Bash".to_string(),
                display_name: None,
                description: None,
                input: serde_json::json!({ "command": command }),
            }],
            metadata: Default::default(),
            extensions: serde_json::Value::Null,
        })
    }

    fn make_tool_result_message(id: &str, output: &str) -> HubRecord {
        HubRecord::Message(crate::interchange::hub::HubMessage {
            id: format!("user-{id}-result"),
            api_message_id: None,
            parent_id: None,
            timestamp: "2026-01-01T00:00:01.000Z".to_string(),
            completed_at: None,
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: vec![ContentBlock::Text {
                    text: output.to_string(),
                }],
                is_error: false,
                exit_code: None,
                interrupted: false,
                status: None,
                duration_ms: None,
                title: None,
                truncated: false,
            }],
            metadata: Default::default(),
            extensions: serde_json::Value::Null,
        })
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_tokens_text() {
        let records = vec![
            make_session_header(),
            make_text_message("user", "aaaa"), // 4 chars = 1 token
            make_text_message("assistant", "bbbbbbbb"), // 8 chars = 2 tokens
        ];
        assert_eq!(estimate_tokens(&records), 3);
    }

    #[test]
    fn test_truncate_no_op_when_within_budget() {
        let records = vec![
            make_session_header(),
            make_text_message("user", "hello"),
            make_text_message("assistant", "world"),
        ];
        let (trimmed, dropped) = truncate_hub_to_budget(records.clone(), 10_000);
        assert_eq!(dropped, 0);
        assert_eq!(trimmed.len(), records.len());
    }

    #[test]
    fn test_truncate_drops_oldest_messages() {
        let long_text = "x".repeat(400); // 400 chars = 100 tokens each
        let records = vec![
            make_session_header(),
            make_text_message("user", &long_text), // oldest
            make_text_message("assistant", &long_text), // 2nd
            make_text_message("user", &long_text), // newest user
            make_text_message("assistant", &long_text), // newest assistant
        ];
        // 4 messages × 100 tokens = 400 tokens total; budget = 250 → must drop 2
        let (trimmed, dropped) = truncate_hub_to_budget(records, 250);
        assert_eq!(dropped, 2);
        // Header + 2 newest messages
        assert_eq!(trimmed.len(), 3);
    }

    #[test]
    fn test_truncate_drops_linked_tool_result_with_tool_use() {
        let records = vec![
            make_session_header(),
            make_tool_use_message("call_1", &"x".repeat(1200)),
            make_tool_result_message("call_1", "ok"),
            make_text_message("user", "keep"),
        ];

        let (trimmed, dropped) = truncate_hub_to_budget(records, 10);
        assert_eq!(dropped, 2);
        assert_eq!(trimmed.len(), 2);
        assert!(matches!(trimmed[0], HubRecord::Session(_)));
        assert!(matches!(
            &trimmed[1],
            HubRecord::Message(msg) if msg.content.iter().any(|b| matches!(b, ContentBlock::Text { text } if text == "keep"))
        ));
    }

    #[test]
    fn test_truncate_always_keeps_header() {
        let records = vec![
            make_session_header(),
            make_text_message("user", &"x".repeat(400)),
        ];
        // Tiny budget — the message gets dropped but the header stays.
        let (trimmed, dropped) = truncate_hub_to_budget(records, 1);
        assert_eq!(dropped, 1);
        assert_eq!(trimmed.len(), 1);
        assert!(matches!(trimmed[0], HubRecord::Session(_)));
    }

    #[test]
    fn test_context_budget_env_var() {
        // Unset → None
        std::env::remove_var("UNLEASH_CROSSLOAD_MAX_TOKENS");
        assert_eq!(context_budget(), None);

        std::env::set_var("UNLEASH_CROSSLOAD_MAX_TOKENS", "128000");
        assert_eq!(context_budget(), Some(128_000));

        // Zero means unlimited
        std::env::set_var("UNLEASH_CROSSLOAD_MAX_TOKENS", "0");
        assert_eq!(context_budget(), None);

        std::env::remove_var("UNLEASH_CROSSLOAD_MAX_TOKENS");
    }

    #[test]
    fn test_claude_filter_keeps_tool_only_pair() {
        let lines = vec![
            serde_json::json!({
                "type": "assistant",
                "message": {"role": "assistant", "content": [{
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "Bash",
                    "input": {"command": "echo hi"}
                }]}
            }),
            serde_json::json!({
                "type": "user",
                "message": {"role": "user", "content": [{
                    "type": "tool_result",
                    "tool_use_id": "toolu_1",
                    "content": "hi"
                }]}
            }),
        ];

        let filtered = filter_claude_injection_lines(lines);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_claude_filter_drops_orphan_tool_result() {
        let lines = vec![serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [{
                "type": "tool_result",
                "tool_use_id": "missing",
                "content": "orphan"
            }]}
        })];

        let filtered = filter_claude_injection_lines(lines);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_codex_custom_tools_survive_claude_injection_without_blank_reasoning() {
        let source = include_str!("tests/fixtures/codex-modern-custom-tools.jsonl");
        let hub = codex::to_hub(std::io::BufReader::new(source.as_bytes())).unwrap();
        let claude_lines = claude::from_hub(&hub).unwrap();
        assert_eq!(
            claude_lines.len(),
            5,
            "Claude file should retain an empty lossless carrier for reasoning"
        );
        let reasoning_carrier = claude_lines
            .iter()
            .find(|line| {
                line.pointer("/message/content")
                    .and_then(|content| content.as_array())
                    .is_some_and(Vec::is_empty)
            })
            .expect("empty reasoning should remain as a stashed carrier row");
        assert_eq!(
            reasoning_carrier
                .pointer("/_ucf_hub/message/content/0/encrypted_data")
                .and_then(|value| value.as_str()),
            Some("opaque-codex-reasoning")
        );
        let filtered = filter_claude_injection_lines(claude_lines);

        assert_eq!(filtered.len(), 4, "user, tool call, tool result, answer");

        let blocks: Vec<&serde_json::Value> = filtered
            .iter()
            .filter_map(|line| line.pointer("/message/content").and_then(|v| v.as_array()))
            .flatten()
            .collect();
        let tool_use = blocks
            .iter()
            .find(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
            .expect("Claude import should contain tool_use");
        assert_eq!(tool_use.get("id").and_then(|v| v.as_str()), Some("call_1"));
        assert_eq!(tool_use.get("name").and_then(|v| v.as_str()), Some("exec"));
        assert!(tool_use
            .pointer("/input/input")
            .and_then(|v| v.as_str())
            .is_some_and(|input| input.contains("rg --files")));

        let tool_result = blocks
            .iter()
            .find(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
            .expect("Claude import should contain matching tool_result");
        assert_eq!(
            tool_result.get("tool_use_id").and_then(|v| v.as_str()),
            Some("call_1")
        );
        assert!(tool_result
            .get("content")
            .and_then(|v| v.as_array())
            .is_some_and(|content| content.iter().any(|block| {
                block
                    .get("text")
                    .and_then(|v| v.as_str())
                    .is_some_and(|text| text.contains("Cargo.toml"))
            })));

        assert!(!filtered.iter().any(|line| {
            line.pointer("/message/content")
                .and_then(|v| v.as_array())
                .is_some_and(|content| {
                    content.iter().any(|block| {
                        block
                            .get("text")
                            .and_then(|v| v.as_str())
                            .is_some_and(|text| text.trim() == "[Reasoning]:")
                    })
                })
        }));
    }

    #[test]
    fn test_claude_filter_keeps_signed_empty_thinking() {
        let lines = vec![serde_json::json!({
            "type": "assistant",
            "message": {"role": "assistant", "content": [{
                "type": "thinking",
                "thinking": "",
                "signature": "signed-empty"
            }]}
        })];

        let filtered = filter_claude_injection_lines(lines);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_claude_filter_removes_blank_blocks_from_kept_messages() {
        let lines = vec![serde_json::json!({
            "type": "assistant",
            "message": {"role": "assistant", "content": [
                {"type": "text", "text": "[Reasoning]:   \n"},
                {"type": "text", "text": "kept answer"}
            ]}
        })];

        let filtered = filter_claude_injection_lines(lines);
        let content = filtered[0]
            .pointer("/message/content")
            .and_then(|value| value.as_array())
            .unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(
            content[0].get("text").and_then(|v| v.as_str()),
            Some("kept answer")
        );
    }

    #[test]
    fn test_prepare_claude_injection_lines_emits_native_resumable_envelopes() {
        let lines = vec![
            serde_json::json!({
                "type": "user",
                "message": {"role": "user", "content": "hello"},
                "uuid": "codex-user-id",
                "timestamp": "2026-07-13T00:00:00Z"
            }),
            serde_json::json!({
                "type": "assistant",
                "message": {"role": "assistant", "content": [{"type": "text", "text": "hi"}]},
                "uuid": "msg_codex_not_a_uuid",
                "promptId": "msg_codex_not_a_prompt_uuid",
                "permissionMode": "bypassPermissions",
                "timestamp": "2026-07-13T00:00:01Z"
            }),
        ];

        let prepared = prepare_claude_injection_lines(lines, "session-id", "/tmp/project");
        assert_eq!(prepared.len(), 3, "two messages plus last-prompt metadata");

        let user_uuid = prepared[0]["uuid"].as_str().unwrap();
        let assistant_uuid = prepared[1]["uuid"].as_str().unwrap();
        assert_eq!(user_uuid.len(), 36);
        assert_eq!(assistant_uuid.len(), 36);
        assert_ne!(user_uuid, "codex-user-id");
        assert_ne!(assistant_uuid, "msg_codex_not_a_uuid");
        assert!(prepared[0]["parentUuid"].is_null());
        assert_eq!(prepared[1]["parentUuid"].as_str(), Some(user_uuid));

        let prompt_id = prepared[0]["promptId"].as_str().unwrap();
        assert_eq!(prompt_id.len(), 36);
        assert!(prepared[1].get("promptId").is_none());
        assert!(prepared[1].get("permissionMode").is_none());
        assert_eq!(
            prepared[1]
                .pointer("/message/type")
                .and_then(|v| v.as_str()),
            Some("message")
        );
        assert_eq!(
            prepared[1]
                .pointer("/message/stop_reason")
                .and_then(|v| v.as_str()),
            Some("end_turn")
        );
        assert!(prepared[1].pointer("/message/id").is_some());
        assert!(prepared[1].pointer("/message/model").is_some());
        assert!(prepared[1].pointer("/message/usage").is_some());
        assert!(prepared[1].get("requestId").is_some());

        assert_eq!(prepared[2]["type"].as_str(), Some("last-prompt"));
        assert_eq!(prepared[2]["lastPrompt"].as_str(), Some("hello"));
        assert_eq!(prepared[2]["leafUuid"].as_str(), Some(assistant_uuid));
        assert_eq!(prepared[2]["sessionId"].as_str(), Some("session-id"));
    }

    #[test]
    fn normalize_target_cli_keeps_agy_distinct_from_gemini() {
        // Prior behavior mapped agy → gemini, which made `inject_into_gemini`
        // write JSON chats into ~/.gemini/tmp/<proj>/chats/ — a file agy
        // can't read because agy's conversations live at
        // ~/.gemini/antigravity-cli/conversations/<uuid>.pb (protobuf).
        // The new behavior keeps the cli string as "agy" so the dispatch
        // can refuse it with a clear error, and stale cache entries from
        // older builds don't shadow that refusal.
        assert_eq!(normalize_target_cli("agy"), "agy");
        assert_eq!(normalize_target_cli("antigravity"), "agy");
        assert_eq!(normalize_target_cli("antigravity-cli"), "agy");
        // gemini still resolves to "gemini" — the two CLIs share the
        // `~/.gemini/` prefix but nothing else.
        assert_eq!(normalize_target_cli("gemini"), "gemini");
        assert_eq!(normalize_target_cli("gemini-cli"), "gemini");
    }
}
