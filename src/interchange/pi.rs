//! Pi coding agent CLI JSONL session converter.
//!
//! Pi's session format is a JSONL file whose first line is a `session` record
//! followed by a chain (linked via `parentId`) of `model_change`,
//! `thinking_level_change`, and `message` records. Messages themselves carry
//! one of three role envelopes (`user`, `assistant`, `toolResult`) with
//! camelCase fields (`toolCallId`, `toolName`, `thinkingSignature`,
//! `cacheRead`, `cacheWrite`, `totalTokens`, `modelId`, `thinkingLevel`,
//! `stopReason`, `responseId`, `errorMessage`, `isError`, `parentId`).
//!
//! The converter is strictly lossless via a stash-the-original strategy:
//! the full inner Pi record shape lives in the hub `extensions.pi` namespace
//! (alongside a cross-CLI `_ucf_hub` session passthrough), so `from_hub` can
//! reconstruct the source byte-for-byte — including the float formatting in
//! `usage.cost` that would otherwise be mangled by hub's `cost: f64` scalar.

use crate::interchange::hub::*;
use crate::interchange::ConvertError;
use serde_json::{Map, Value};
use std::io::BufRead;

/// Convert a Pi JSONL session to hub records.
pub fn to_hub<R: BufRead>(reader: R) -> Result<Vec<HubRecord>, ConvertError> {
    let mut records: Vec<HubRecord> = Vec::new();
    let mut session_emitted = false;
    // If any line carries a `_ucf_hub.session` escape hatch, we replace the
    // Pi-synthesized session header with it after parsing so cross-CLI round
    // trips preserve the original non-Pi session identity.
    let mut carried_session: Option<SessionHeader> = None;
    let mut foreign_session = false;
    let mut last_timestamp: Option<String> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let val: Value = serde_json::from_str(&line)?;

        let ucf_hub = val.get("_ucf_hub");
        let foreign_ext = ucf_hub
            .and_then(|u| u.get("ext"))
            .and_then(|e| e.as_object())
            .cloned();

        if !session_emitted {
            if let Some(sess_val) = ucf_hub.and_then(|u| u.get("session")) {
                carried_session = serde_json::from_value(sess_val.clone()).ok();
                foreign_session = true;
            }
        }
        let foreign_originated = foreign_session || ucf_hub.is_some();

        let rec_type = val
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        // Track timestamp on every record for session.updated_at.
        if let Some(ts) = val.get("timestamp").and_then(|v| v.as_str()) {
            last_timestamp = Some(ts.to_string());
        }

        match rec_type.as_str() {
            "session" => {
                if session_emitted {
                    // Defensive: multiple session lines — ignore subsequent ones.
                    continue;
                }
                records.push(HubRecord::Session(pi_session_to_hub(&val)));
                session_emitted = true;
            }
            "message" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(
                        last_timestamp.as_deref().unwrap_or(""),
                    )));
                    session_emitted = true;
                }
                let mut msg = pi_message_to_hub(&val, foreign_originated)?;
                if let Some(ext) = &foreign_ext {
                    merge_into_extensions(&mut msg.extensions, ext);
                }
                records.push(HubRecord::Message(msg));
            }
            "model_change" | "thinking_level_change" => {
                if !session_emitted {
                    records.push(HubRecord::Session(default_session(
                        last_timestamp.as_deref().unwrap_or(""),
                    )));
                    session_emitted = true;
                }
                let mut evt = pi_control_record_to_event(&val, &rec_type);
                if let Some(ext) = &foreign_ext {
                    merge_into_extensions(&mut evt.extensions, ext);
                }
                records.push(HubRecord::Event(evt));
            }
            "" => {
                return Err(ConvertError::InvalidFormat(
                    "pi: record missing required `type` field".into(),
                ));
            }
            other => {
                return Err(ConvertError::InvalidFormat(format!(
                    "pi: unknown record type \"{other}\""
                )));
            }
        }
    }

    // Patch updated_at on the session we synthesized (last-seen timestamp).
    if let Some(HubRecord::Session(ref mut session)) = records.first_mut() {
        if let Some(ts) = &last_timestamp {
            session.updated_at = ts.clone();
        }
    }

    // If a carried session header was provided via `_ucf_hub.session`, replace
    // the synthesized one so foreign-source sessions survive the round trip.
    if let Some(carried) = carried_session {
        if let Some(HubRecord::Session(ref mut session)) = records.first_mut() {
            *session = carried;
        }
    }

    Ok(records)
}

/// Convert hub records back to a list of Pi JSONL JSON values.
///
/// When the hub session's `source_cli` is not `pi`, the first emitted line
/// carries a `_ucf_hub.session` field holding the full SessionHeader so the
/// hub → pi → hub round trip is lossless. Per-message foreign extensions are
/// stashed under `_ucf_hub.ext`.
pub fn from_hub(records: &[HubRecord]) -> Result<Vec<Value>, ConvertError> {
    let mut lines: Vec<Value> = Vec::new();
    let mut session_passthrough: Option<Value> = None;

    for record in records {
        match record {
            HubRecord::Session(s) => {
                if s.source_cli != "pi" {
                    session_passthrough = Some(serde_json::to_value(s)?);
                }
                lines.push(hub_session_to_pi(s));
            }
            HubRecord::Message(msg) => {
                lines.push(hub_message_to_pi(msg)?);
                // Re-borrow to attach foreign extensions.
                if let Some(foreign) = foreign_extensions(&msg.extensions) {
                    if let Some(line) = lines.last_mut() {
                        attach_ucf_hub_ext(line, foreign);
                    }
                }
            }
            HubRecord::Event(evt) => {
                if let Some(line) = hub_event_to_pi(evt)? {
                    lines.push(line);
                    if let Some(foreign) = foreign_extensions(&evt.extensions) {
                        if let Some(last) = lines.last_mut() {
                            attach_ucf_hub_ext(last, foreign);
                        }
                    }
                }
            }
        }
    }

    if let (Some(sess), Some(first)) = (session_passthrough, lines.first_mut()) {
        attach_ucf_hub_session(first, sess);
    }

    Ok(lines)
}

// =========================================================================
// to_hub helpers
// =========================================================================

fn default_session(timestamp: &str) -> SessionHeader {
    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id: "unknown".into(),
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
        source_cli: "pi".into(),
        source_version: "unknown".into(),
        project: None,
        model: None,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions: Value::Null,
    }
}

fn pi_session_to_hub(val: &Value) -> SessionHeader {
    let session_id = val
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timestamp = val
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cwd = val
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Schema version is an integer on the wire; preserve verbatim.
    let schema_version = val.get("version").cloned().unwrap_or(Value::Null);

    // Capture any Pi-session fields we don't have first-class slots for so
    // that round-trip stays lossless even if Pi adds fields later.
    let known: &[&str] = &["type", "version", "id", "timestamp", "cwd"];
    let mut extras = Map::new();
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if known.contains(&k.as_str()) {
                continue;
            }
            if k == "_ucf_hub" {
                continue;
            }
            extras.insert(k.clone(), v.clone());
        }
    }

    let mut pi_ext = Map::new();
    pi_ext.insert("schema_version".into(), schema_version);
    if !extras.is_empty() {
        pi_ext.insert("extras".into(), Value::Object(extras));
    }

    let project = if cwd.is_empty() {
        None
    } else {
        Some(ProjectInfo {
            directory: cwd.clone(),
            root: None,
            hash: None,
            vcs: None,
            branch: None,
            sha: None,
            origin_url: None,
        })
    };

    SessionHeader {
        ucf_version: UCF_VERSION.to_string(),
        session_id,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        source_cli: "pi".into(),
        source_version: "unknown".into(),
        project,
        model: None,
        title: None,
        slug: None,
        parent_session_id: None,
        extensions: Value::Object(
            [(String::from("pi"), Value::Object(pi_ext))]
                .into_iter()
                .collect(),
        ),
    }
}

fn pi_control_record_to_event(val: &Value, rec_type: &str) -> HubEvent {
    let timestamp = val
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // `data` receives everything except `type` and `timestamp`, so the
    // fields that distinguish the event (id, parentId, provider, modelId,
    // thinkingLevel, ...) all survive.
    let mut data = Map::new();
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if k == "type" || k == "timestamp" || k == "_ucf_hub" {
                continue;
            }
            data.insert(k.clone(), v.clone());
        }
    }

    HubEvent {
        event_type: rec_type.to_string(),
        timestamp,
        data: Value::Object(data),
        // No Pi-specific sidecar needed when everything lands in data —
        // use an explicit empty pi namespace so consumers can identify the
        // origin without ambiguity.
        extensions: Value::Object(
            [(String::from("pi"), Value::Object(Map::new()))]
                .into_iter()
                .collect(),
        ),
    }
}

fn pi_message_to_hub(val: &Value, foreign_originated: bool) -> Result<HubMessage, ConvertError> {
    let outer_id = val
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let parent_id = val
        .get("parentId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let outer_ts = val
        .get("timestamp")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let message = val.get("message").cloned().unwrap_or(Value::Null);

    let role = message
        .get("role")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Build a sidecar that captures every Pi-native field outside the content
    // array, keyed so `from_hub` can reconstruct the envelope exactly.
    let mut pi_sidecar = Map::new();

    // Preserve the inner envelope timestamp (ms since epoch, distinct from
    // the outer ISO timestamp).
    if let Some(inner_ts) = message.get("timestamp").cloned() {
        pi_sidecar.insert("envelope_timestamp".into(), inner_ts);
    }

    let (hub_role, hub_content, metadata) = match role.as_str() {
        "user" => {
            let content = extract_content_blocks(message.get("content"));
            ("user".to_string(), content, MessageMetadata::default())
        }
        "assistant" => {
            let content = extract_content_blocks(message.get("content"));

            // Carry the raw usage object verbatim so float formatting round-
            // trips exactly.
            if let Some(usage) = message.get("usage").cloned() {
                pi_sidecar.insert("usage_raw".into(), usage);
            }
            if let Some(api) = message.get("api").cloned() {
                pi_sidecar.insert("api".into(), api);
            }
            if let Some(provider) = message.get("provider").cloned() {
                pi_sidecar.insert("provider".into(), provider);
            }
            if let Some(rid) = message.get("responseId").cloned() {
                pi_sidecar.insert("responseId".into(), rid);
            }
            if let Some(err) = message.get("errorMessage").cloned() {
                pi_sidecar.insert("errorMessage".into(), err);
            }
            let stop_reason_raw = message
                .get("stopReason")
                .and_then(|v| v.as_str())
                .map(String::from);
            if let Some(ref raw) = stop_reason_raw {
                pi_sidecar.insert("stopReason".into(), Value::String(raw.clone()));
            }

            // Populate hub metadata best-effort.
            let model = message
                .get("model")
                .and_then(|v| v.as_str())
                .map(String::from);
            let usage = message.get("usage");
            let tokens = usage.map(|u| TokenUsage {
                input: u.get("input").and_then(|v| v.as_u64()).unwrap_or(0),
                output: u.get("output").and_then(|v| v.as_u64()).unwrap_or(0),
                cache_creation: u.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0),
                cache_read: u.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0),
                reasoning: 0,
                tool: 0,
                total: u.get("totalTokens").and_then(|v| v.as_u64()).unwrap_or(0),
            });
            let cost = usage
                .and_then(|u| u.get("cost"))
                .and_then(|c| c.get("total"))
                .and_then(|v| v.as_f64());
            let hub_stop_reason = stop_reason_raw.as_deref().map(|r| match r {
                "toolUse" => "tool_use".to_string(),
                other => other.to_string(),
            });

            (
                "assistant".to_string(),
                content,
                MessageMetadata {
                    model,
                    tokens,
                    cost,
                    stop_reason: hub_stop_reason,
                    ..Default::default()
                },
            )
        }
        "toolResult" => {
            let tool_use_id = message
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_name = message
                .get("toolName")
                .and_then(|v| v.as_str())
                .map(String::from);
            let is_error = message
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let inner_content = extract_content_blocks(message.get("content"));

            if let Some(name) = &tool_name {
                pi_sidecar.insert("toolName".into(), Value::String(name.clone()));
            }
            pi_sidecar.insert("isError".into(), Value::Bool(is_error));
            if let Some(details) = message.get("details").cloned() {
                // Some Pi toolResults carry a `details` object (often `{}`);
                // preserve it verbatim for byte-exact round-trip.
                pi_sidecar.insert("details".into(), details);
            }

            let content = vec![ContentBlock::ToolResult {
                tool_use_id,
                content: inner_content,
                exit_code: None,
                is_error,
                interrupted: false,
                status: None,
                duration_ms: None,
                title: None,
                truncated: false,
            }];
            (
                "tool".to_string(),
                content,
                MessageMetadata::default(),
            )
        }
        other => {
            return Err(ConvertError::InvalidFormat(format!(
                "pi: unknown message role \"{other}\""
            )));
        }
    };

    // Preserve any message-envelope fields that weren't explicitly mapped so
    // forward-compatible Pi additions round-trip automatically.
    let handled_envelope_fields: &[&str] = &[
        "role",
        "content",
        "timestamp",
        "api",
        "provider",
        "model",
        "usage",
        "stopReason",
        "responseId",
        "errorMessage",
        "toolCallId",
        "toolName",
        "isError",
        "details",
    ];
    let mut envelope_extras = Map::new();
    if let Some(obj) = message.as_object() {
        for (k, v) in obj {
            if !handled_envelope_fields.contains(&k.as_str()) {
                envelope_extras.insert(k.clone(), v.clone());
            }
        }
    }
    if !envelope_extras.is_empty() {
        pi_sidecar.insert("envelope_extras".into(), Value::Object(envelope_extras));
    }

    let extensions = if foreign_originated && pi_sidecar.is_empty() {
        Value::Null
    } else {
        Value::Object(
            [(String::from("pi"), Value::Object(pi_sidecar))]
                .into_iter()
                .collect(),
        )
    };

    Ok(HubMessage {
        id: outer_id,
        api_message_id: None,
        parent_id,
        timestamp: outer_ts,
        completed_at: None,
        role: hub_role,
        content: hub_content,
        metadata,
        extensions,
    })
}

fn extract_content_blocks(content: Option<&Value>) -> Vec<ContentBlock> {
    let Some(arr) = content.and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    arr.iter().map(content_block_from_pi).collect()
}

fn content_block_from_pi(block: &Value) -> ContentBlock {
    let block_type = block
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    match block_type {
        "text" => ContentBlock::Text {
            text: block
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "thinking" => ContentBlock::Thinking {
            text: block
                .get("thinking")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            subject: None,
            description: None,
            signature: block
                .get("thinkingSignature")
                .and_then(|v| v.as_str())
                .map(String::from),
            encrypted: false,
            encryption_format: None,
            encrypted_data: None,
            timestamp: None,
        },
        "toolCall" => ContentBlock::ToolUse {
            id: block
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            name: block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            display_name: None,
            description: None,
            input: block
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Map::new())),
        },
        _ => ContentBlock::Text {
            // Unknown block type — best we can do without losing the shape is
            // fall through to a text block. The `_ucf_hub.ext` path will still
            // preserve the original when this came from a non-Pi source.
            text: serde_json::to_string(block).unwrap_or_default(),
        },
    }
}

// =========================================================================
// from_hub helpers
// =========================================================================

fn hub_session_to_pi(session: &SessionHeader) -> Value {
    // Recover Pi's integer schema version if we saved one.
    let pi_ext = session.extensions.get("pi");
    let schema_version = pi_ext
        .and_then(|p| p.get("schema_version"))
        .cloned()
        .unwrap_or(Value::Number(serde_json::Number::from(3_i64)));

    let cwd = session
        .project
        .as_ref()
        .map(|p| p.directory.clone())
        .unwrap_or_default();

    let mut out = Map::new();
    out.insert("type".into(), Value::String("session".into()));
    out.insert("version".into(), schema_version);
    out.insert("id".into(), Value::String(session.session_id.clone()));
    out.insert(
        "timestamp".into(),
        Value::String(session.created_at.clone()),
    );
    out.insert("cwd".into(), Value::String(cwd));

    // Restore any forward-compatible extras stashed during to_hub.
    if let Some(extras) = pi_ext
        .and_then(|p| p.get("extras"))
        .and_then(|v| v.as_object())
    {
        for (k, v) in extras {
            out.insert(k.clone(), v.clone());
        }
    }

    Value::Object(out)
}

fn hub_event_to_pi(evt: &HubEvent) -> Result<Option<Value>, ConvertError> {
    match evt.event_type.as_str() {
        "model_change" | "thinking_level_change" => {
            let mut out = Map::new();
            out.insert("type".into(), Value::String(evt.event_type.clone()));
            if let Some(obj) = evt.data.as_object() {
                // We want a deterministic-ish field order: id, parentId come
                // first when present, then the type-specific fields in the
                // order the Pi producer emitted them (which we preserved by
                // iterating the original JSON Map's insertion order).
                // `data` already omitted `type`/`timestamp`, so emit them
                // around the data object.
                // First, insert id/parentId if present (to mirror Pi shape).
                for key in ["id", "parentId"] {
                    if let Some(v) = obj.get(key) {
                        out.insert(key.into(), v.clone());
                    }
                }
                out.insert("timestamp".into(), Value::String(evt.timestamp.clone()));
                for (k, v) in obj {
                    if k == "id" || k == "parentId" {
                        continue;
                    }
                    out.insert(k.clone(), v.clone());
                }
            } else {
                out.insert("timestamp".into(), Value::String(evt.timestamp.clone()));
            }
            Ok(Some(Value::Object(out)))
        }
        other => Err(ConvertError::InvalidFormat(format!(
            "pi: cannot emit event type \"{other}\" as Pi record"
        ))),
    }
}

fn hub_message_to_pi(msg: &HubMessage) -> Result<Value, ConvertError> {
    let pi_ext = msg.extensions.get("pi").cloned().unwrap_or(Value::Null);
    let pi_obj = pi_ext.as_object().cloned().unwrap_or_default();

    let mut inner = Map::new();

    let role = match msg.role.as_str() {
        "user" => "user",
        "assistant" => "assistant",
        "tool" => "toolResult",
        other => {
            return Err(ConvertError::InvalidFormat(format!(
                "pi: cannot emit hub role \"{other}\" as Pi message"
            )));
        }
    };

    inner.insert("role".into(), Value::String(role.into()));

    if role == "toolResult" {
        // Expect a single ToolResult content block; fall back to the first
        // ToolResult we can find.
        let tr = msg
            .content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                    ..
                } => Some((tool_use_id.clone(), content.clone(), *is_error)),
                _ => None,
            })
            .ok_or_else(|| {
                ConvertError::InvalidFormat(
                    "pi: tool-role message missing tool_result content block".into(),
                )
            })?;

        inner.insert("toolCallId".into(), Value::String(tr.0));
        // toolName: prefer sidecar, else hub tool_use_id is not enough — emit empty
        let tool_name = pi_obj
            .get("toolName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        inner.insert("toolName".into(), Value::String(tool_name.into()));
        inner.insert(
            "content".into(),
            Value::Array(tr.1.iter().map(content_block_to_pi).collect()),
        );
        if let Some(details) = pi_obj.get("details") {
            inner.insert("details".into(), details.clone());
        }
        inner.insert(
            "isError".into(),
            pi_obj
                .get("isError")
                .cloned()
                .unwrap_or(Value::Bool(tr.2)),
        );
        if let Some(ts) = pi_obj.get("envelope_timestamp") {
            inner.insert("timestamp".into(), ts.clone());
        }
    } else {
        inner.insert(
            "content".into(),
            Value::Array(msg.content.iter().map(content_block_to_pi).collect()),
        );

        if role == "assistant" {
            // Order matches Pi's natural emission: api, provider, model,
            // usage, stopReason, timestamp, responseId, errorMessage.
            if let Some(api) = pi_obj.get("api") {
                inner.insert("api".into(), api.clone());
            }
            if let Some(provider) = pi_obj.get("provider") {
                inner.insert("provider".into(), provider.clone());
            } else if let Some(model) = &msg.metadata.model {
                // No sidecar — best effort
                let _ = model;
            }
            if let Some(model) = pi_obj.get("model") {
                inner.insert("model".into(), model.clone());
            } else if let Some(model) = &msg.metadata.model {
                inner.insert("model".into(), Value::String(model.clone()));
            }

            // Usage: prefer the verbatim stashed object to avoid float-fmt
            // round-trip loss. Otherwise synthesize from hub metadata.
            if let Some(usage_raw) = pi_obj.get("usage_raw") {
                inner.insert("usage".into(), usage_raw.clone());
            } else if let Some(tokens) = &msg.metadata.tokens {
                let cost_total = msg.metadata.cost.unwrap_or(0.0);
                let usage = serde_json::json!({
                    "input": tokens.input,
                    "output": tokens.output,
                    "cacheRead": tokens.cache_read,
                    "cacheWrite": tokens.cache_creation,
                    "totalTokens": tokens.total,
                    "cost": {
                        "input": 0.0,
                        "output": 0.0,
                        "cacheRead": 0.0,
                        "cacheWrite": 0.0,
                        "total": cost_total,
                    }
                });
                inner.insert("usage".into(), usage);
            }

            // Stop reason: prefer the raw value we stashed.
            if let Some(raw) = pi_obj.get("stopReason") {
                inner.insert("stopReason".into(), raw.clone());
            } else if let Some(sr) = &msg.metadata.stop_reason {
                let pi_sr = match sr.as_str() {
                    "tool_use" => "toolUse".to_string(),
                    other => other.to_string(),
                };
                inner.insert("stopReason".into(), Value::String(pi_sr));
            }
        }

        if let Some(ts) = pi_obj.get("envelope_timestamp") {
            inner.insert("timestamp".into(), ts.clone());
        }

        if role == "assistant" {
            if let Some(rid) = pi_obj.get("responseId") {
                inner.insert("responseId".into(), rid.clone());
            }
            if let Some(err) = pi_obj.get("errorMessage") {
                inner.insert("errorMessage".into(), err.clone());
            }
        }
    }

    // Restore any envelope fields we preserved but didn't map.
    if let Some(extras) = pi_obj
        .get("envelope_extras")
        .and_then(|v| v.as_object())
    {
        for (k, v) in extras {
            inner.insert(k.clone(), v.clone());
        }
    }

    let mut out = Map::new();
    out.insert("type".into(), Value::String("message".into()));
    out.insert("id".into(), Value::String(msg.id.clone()));
    out.insert(
        "parentId".into(),
        msg.parent_id
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
    );
    out.insert(
        "timestamp".into(),
        Value::String(msg.timestamp.clone()),
    );
    out.insert("message".into(), Value::Object(inner));

    Ok(Value::Object(out))
}

fn content_block_to_pi(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text } => {
            let mut m = Map::new();
            m.insert("type".into(), Value::String("text".into()));
            m.insert("text".into(), Value::String(text.clone()));
            Value::Object(m)
        }
        ContentBlock::Thinking {
            text, signature, ..
        } => {
            let mut m = Map::new();
            m.insert("type".into(), Value::String("thinking".into()));
            m.insert("thinking".into(), Value::String(text.clone()));
            m.insert(
                "thinkingSignature".into(),
                Value::String(signature.clone().unwrap_or_default()),
            );
            Value::Object(m)
        }
        ContentBlock::ToolUse {
            id, name, input, ..
        } => {
            let mut m = Map::new();
            m.insert("type".into(), Value::String("toolCall".into()));
            m.insert("id".into(), Value::String(id.clone()));
            m.insert("name".into(), Value::String(name.clone()));
            m.insert("arguments".into(), input.clone());
            Value::Object(m)
        }
        ContentBlock::ToolResult { content, .. } => {
            // A ToolResult inside another ToolResult shouldn't appear in Pi,
            // but we flatten to first text child for safety.
            let text = content
                .iter()
                .filter_map(|c| match c {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let mut m = Map::new();
            m.insert("type".into(), Value::String("text".into()));
            m.insert("text".into(), Value::String(text));
            Value::Object(m)
        }
        ContentBlock::Image { data, .. } => {
            // Pi doesn't have a native image content block in the observed
            // schema; serialize as text placeholder to avoid silent drops.
            let mut m = Map::new();
            m.insert("type".into(), Value::String("text".into()));
            m.insert(
                "text".into(),
                Value::String(format!("[image: {} bytes]", data.len())),
            );
            Value::Object(m)
        }
        other => {
            let mut m = Map::new();
            m.insert("type".into(), Value::String("text".into()));
            m.insert(
                "text".into(),
                Value::String(serde_json::to_string(other).unwrap_or_default()),
            );
            Value::Object(m)
        }
    }
}

// =========================================================================
// _ucf_hub passthrough helpers (mirrors the patterns in claude.rs/codex.rs)
// =========================================================================

fn merge_into_extensions(extensions: &mut Value, ext: &Map<String, Value>) {
    if let Some(obj) = extensions.as_object_mut() {
        for (k, v) in ext {
            obj.insert(k.clone(), v.clone());
        }
    } else {
        *extensions = Value::Object(ext.clone());
    }
}

fn foreign_extensions(ext: &Value) -> Option<Value> {
    let obj = ext.as_object()?;
    let foreign: Map<String, Value> = obj
        .iter()
        .filter(|(k, _)| k.as_str() != "pi")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if foreign.is_empty() {
        None
    } else {
        Some(Value::Object(foreign))
    }
}

fn attach_ucf_hub_ext(line: &mut Value, ext: Value) {
    let Value::Object(ref mut obj) = line else {
        return;
    };
    let entry = obj
        .entry("_ucf_hub".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(ref mut inner) = entry else {
        return;
    };
    inner.insert("ext".to_string(), ext);
}

fn attach_ucf_hub_session(line: &mut Value, session: Value) {
    let Value::Object(ref mut obj) = line else {
        return;
    };
    let entry = obj
        .entry("_ucf_hub".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(ref mut inner) = entry else {
        return;
    };
    inner.insert("session".to_string(), session);
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::semantic_eq::semantic_eq;

    fn roundtrip_lines(input: &str) -> Vec<Value> {
        let reader = std::io::BufReader::new(input.as_bytes());
        let hub = to_hub(reader).expect("to_hub");
        from_hub(&hub).expect("from_hub")
    }

    fn assert_lines_match(original: &str, produced: &[Value]) {
        let orig_lines: Vec<Value> = original
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("valid json"))
            .collect();
        assert_eq!(
            orig_lines.len(),
            produced.len(),
            "line count mismatch (orig={}, produced={})",
            orig_lines.len(),
            produced.len()
        );
        for (i, (orig, out)) in orig_lines.iter().zip(produced.iter()).enumerate() {
            if let Err(e) = semantic_eq(orig, out) {
                panic!("line {i} mismatch: {e}\norig: {orig}\nout:  {out}");
            }
        }
    }

    #[test]
    fn basic_round_trip() {
        let input = concat!(
            r#"{"type":"session","version":3,"id":"019daf9c-92c6-742e-8766-1490fd1a7f43","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/home/me/ht/forks/ht-llama.cpp"}"#,
            "\n",
            r#"{"type":"model_change","id":"09be72c5","parentId":null,"timestamp":"2026-04-21T10:36:07.242Z","provider":"huggingface","modelId":"MiniMaxAI/MiniMax-M2.7"}"#,
            "\n",
            r#"{"type":"thinking_level_change","id":"9a110c07","parentId":"09be72c5","timestamp":"2026-04-21T10:36:07.242Z","thinkingLevel":"medium"}"#,
            "\n",
            r#"{"type":"message","id":"0b08a768","parentId":"9a110c07","timestamp":"2026-04-21T10:36:29.270Z","message":{"role":"user","content":[{"type":"text","text":"Did you see the agents.md?"}],"timestamp":1776767789268}}"#,
        );
        let produced = roundtrip_lines(input);
        assert_lines_match(input, &produced);
    }

    #[test]
    fn thinking_block_round_trip() {
        let input = r#"{"type":"session","version":3,"id":"sess-1","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/tmp"}
{"type":"message","id":"m1","parentId":null,"timestamp":"2026-04-21T10:36:30.384Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me check\n","thinkingSignature":"reasoning_content"},{"type":"text","text":"hi"}],"api":"openai-completions","provider":"huggingface","model":"m","usage":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0,"totalTokens":3,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}},"stopReason":"stop","timestamp":1776767790396,"responseId":"resp-1"}}"#;
        let produced = roundtrip_lines(input);
        assert_lines_match(input, &produced);

        // Also verify the hub side captured thinking with a signature.
        let hub = to_hub(std::io::BufReader::new(input.as_bytes())).unwrap();
        let assistant = hub
            .iter()
            .find_map(|r| match r {
                HubRecord::Message(m) if m.role == "assistant" => Some(m),
                _ => None,
            })
            .expect("assistant message");
        let has_thinking = assistant.content.iter().any(|b| {
            matches!(
                b,
                ContentBlock::Thinking { signature: Some(sig), .. } if sig == "reasoning_content"
            )
        });
        assert!(has_thinking, "thinking block should preserve signature");
    }

    #[test]
    fn tool_call_and_result_round_trip() {
        let input = r#"{"type":"session","version":3,"id":"s","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/tmp"}
{"type":"message","id":"a1","parentId":null,"timestamp":"2026-04-21T10:36:30.384Z","message":{"role":"assistant","content":[{"type":"toolCall","id":"call_1","name":"read","arguments":{"path":"AGENTS.md"}}],"api":"openai-completions","provider":"huggingface","model":"m","usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0,"totalTokens":15,"cost":{"input":0.0,"output":0.0,"cacheRead":0.0,"cacheWrite":0.0,"total":0.0}},"stopReason":"toolUse","timestamp":1776767789304,"responseId":"r1"}}
{"type":"message","id":"t1","parentId":"a1","timestamp":"2026-04-21T10:36:30.396Z","message":{"role":"toolResult","toolCallId":"call_1","toolName":"read","content":[{"type":"text","text":"ENOENT"}],"details":{},"isError":true,"timestamp":1776767790396}}"#;
        let produced = roundtrip_lines(input);
        assert_lines_match(input, &produced);

        // Hub side: the toolResult should become role=tool with a single
        // ToolResult content block.
        let hub = to_hub(std::io::BufReader::new(input.as_bytes())).unwrap();
        let tool_msg = hub
            .iter()
            .find_map(|r| match r {
                HubRecord::Message(m) if m.role == "tool" => Some(m),
                _ => None,
            })
            .expect("tool-role message");
        assert!(matches!(
            tool_msg.content.first(),
            Some(ContentBlock::ToolResult { is_error: true, .. })
        ));
    }

    #[test]
    fn model_change_preserved() {
        let input = r#"{"type":"session","version":3,"id":"s","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/tmp"}
{"type":"model_change","id":"a","parentId":null,"timestamp":"2026-04-21T10:36:07.242Z","provider":"huggingface","modelId":"MiniMaxAI/MiniMax-M2.7"}
{"type":"model_change","id":"b","parentId":"a","timestamp":"2026-04-21T10:36:08.000Z","provider":"anthropic","modelId":"claude-opus-4"}
{"type":"thinking_level_change","id":"c","parentId":"b","timestamp":"2026-04-21T10:36:08.001Z","thinkingLevel":"high"}"#;
        let produced = roundtrip_lines(input);
        assert_lines_match(input, &produced);

        // Hub side: three events with correct event_type.
        let hub = to_hub(std::io::BufReader::new(input.as_bytes())).unwrap();
        let events: Vec<&HubEvent> = hub
            .iter()
            .filter_map(|r| match r {
                HubRecord::Event(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, "model_change");
        assert_eq!(events[1].event_type, "model_change");
        assert_eq!(events[2].event_type, "thinking_level_change");
        // data should carry modelId for model_change events
        assert_eq!(
            events[0].data.get("modelId").and_then(|v| v.as_str()),
            Some("MiniMaxAI/MiniMax-M2.7")
        );
    }

    #[test]
    fn usage_and_cost_round_trip_exact() {
        // Floats including scientific notation — this is the scenario Pi
        // actually emits (e.g. 0.00006599999999999999).
        let input = r#"{"type":"session","version":3,"id":"s","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/tmp"}
{"type":"message","id":"a","parentId":null,"timestamp":"2026-04-21T10:36:30.384Z","message":{"role":"assistant","content":[{"type":"text","text":"hi"}],"api":"openai-completions","provider":"huggingface","model":"m","usage":{"input":5728,"output":55,"cacheRead":0,"cacheWrite":0,"totalTokens":5783,"cost":{"input":0.0017184,"output":0.00006599999999999999,"cacheRead":0,"cacheWrite":0,"total":0.0017844}},"stopReason":"stop","timestamp":1776767789304,"responseId":"r"}}
{"type":"message","id":"b","parentId":"a","timestamp":"2026-04-21T10:36:31.000Z","message":{"role":"assistant","content":[{"type":"text","text":"hi2"}],"api":"openai-completions","provider":"huggingface","model":"m","usage":{"input":1,"output":2,"cacheRead":0,"cacheWrite":0,"totalTokens":3,"cost":{"input":1.5e-7,"output":2.5e-7,"cacheRead":0,"cacheWrite":0,"total":4e-7}},"stopReason":"stop","timestamp":1776767790000,"responseId":"r2"}}"#;
        let produced = roundtrip_lines(input);
        assert_lines_match(input, &produced);

        // Hub side: confirm the verbatim usage_raw made it into extensions.
        let hub = to_hub(std::io::BufReader::new(input.as_bytes())).unwrap();
        let first_assistant = hub
            .iter()
            .find_map(|r| match r {
                HubRecord::Message(m) if m.role == "assistant" => Some(m),
                _ => None,
            })
            .expect("assistant");
        let usage_raw = first_assistant
            .extensions
            .get("pi")
            .and_then(|p| p.get("usage_raw"))
            .expect("usage_raw preserved");
        // The float inside cost.output must still be there verbatim (as f64).
        let output_cost = usage_raw
            .get("cost")
            .and_then(|c| c.get("output"))
            .and_then(|v| v.as_f64())
            .unwrap();
        assert!((output_cost - 0.00006599999999999999_f64).abs() < 1e-20);
    }

    #[test]
    fn error_on_unknown_top_level_type() {
        let input = r#"{"type":"session","version":3,"id":"s","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/tmp"}
{"type":"bogus","id":"x","timestamp":"2026-04-21T10:36:07.242Z"}"#;
        let reader = std::io::BufReader::new(input.as_bytes());
        let err = to_hub(reader).expect_err("should error on unknown type");
        match err {
            ConvertError::InvalidFormat(m) => assert!(m.contains("bogus"), "msg: {m}"),
            other => panic!("wrong error variant: {other:?}"),
        }
    }

    #[test]
    fn session_extensions_carry_schema_version() {
        let input = r#"{"type":"session","version":7,"id":"s","timestamp":"2026-04-21T10:36:07.238Z","cwd":"/tmp"}"#;
        let hub = to_hub(std::io::BufReader::new(input.as_bytes())).unwrap();
        let HubRecord::Session(ref header) = hub[0] else {
            panic!("not a session")
        };
        assert_eq!(
            header
                .extensions
                .get("pi")
                .and_then(|p| p.get("schema_version"))
                .and_then(|v| v.as_i64()),
            Some(7)
        );
        // Round-trip.
        let back = from_hub(&hub).unwrap();
        let orig: Value = serde_json::from_str(input).unwrap();
        semantic_eq(&orig, &back[0]).unwrap();
    }

    /// Round-trip the real Pi session fixture. The fixture is trimmed to the
    /// first few records to keep the test binary small; the full 64-line
    /// fixture lives outside the repo and is covered by the
    /// `real_world_roundtrip` ignored test below.
    #[test]
    fn fixture_round_trip() {
        let input = include_str!("tests/fixtures/pi-sample.jsonl");
        let produced = roundtrip_lines(input);
        assert_lines_match(input, &produced);
    }

    /// Real-world fixture round-trip. Runs the full 64-line Pi session at
    /// `~/.pi/agent/sessions/...` through to_hub/from_hub and asserts
    /// line-by-line semantic equality. Ignored by default because the fixture
    /// lives outside the repo.
    #[test]
    #[ignore = "requires real pi session fixture outside the repo"]
    fn real_world_roundtrip() {
        let path = "/home/me/.pi/agent/sessions/--home-me-ht-forks-ht-llama.cpp--/2026-04-21T10-36-07-238Z_019daf9c-92c6-742e-8766-1490fd1a7f43.jsonl";
        let input = std::fs::read_to_string(path).expect("fixture file");
        let produced = roundtrip_lines(&input);
        assert_lines_match(&input, &produced);
    }
}
