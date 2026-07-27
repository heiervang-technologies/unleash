use crate::interchange::hub::{ContentBlock, HubRecord, TokenUsage};
use crate::interchange::inject::source_to_hub;
use crate::interchange::sessions::{discover_native, SessionInfo};
use crate::interchange::CliFormat;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const QUIET_COMPLETION: Duration = Duration::from_millis(900);

#[derive(Debug, Clone)]
pub struct InstanceIdentity {
    pub instance_name: String,
    pub native_session_id: Option<String>,
    pub model_slug: String,
    pub harness: String,
    pub mode: &'static str,
    pub created: u64,
    name_is_configured: bool,
    model_is_configured: bool,
}

impl InstanceIdentity {
    pub fn new(
        instance_name: String,
        model: Option<String>,
        harness: String,
        mode: &'static str,
        name_is_configured: bool,
    ) -> Self {
        Self {
            instance_name,
            native_session_id: None,
            model_slug: model.clone().unwrap_or_else(|| "default".to_string()),
            harness,
            mode,
            created: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            name_is_configured,
            model_is_configured: model.is_some(),
        }
    }

    pub fn model_id(&self) -> String {
        format!(
            "unleash/{}/{}/{}",
            slug_component(&self.instance_name),
            slug_component(&self.model_slug),
            slug_component(&self.harness)
        )
    }

    fn update_from_session(&mut self, session: &SessionInfo, records: &[HubRecord]) {
        self.native_session_id = Some(session.id.clone());
        if !self.name_is_configured {
            if let Some(name) = session
                .name
                .as_deref()
                .or(session.title.as_deref())
                .filter(|name| !name.trim().is_empty())
            {
                self.instance_name = name.to_string();
            }
        }
        if !self.model_is_configured {
            if let Some(model) = records.iter().rev().find_map(model_from_record) {
                self.model_slug = model;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub cached_prompt_tokens: u64,
    pub reasoning_tokens: u64,
}

impl GatewayUsage {
    fn add_tokens(&mut self, tokens: &TokenUsage) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(tokens.input);
        self.completion_tokens = self.completion_tokens.saturating_add(tokens.output);
        self.cached_prompt_tokens = self.cached_prompt_tokens.saturating_add(tokens.cache_read);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(tokens.reasoning);
        self.total_tokens = self.total_tokens.saturating_add(if tokens.total == 0 {
            tokens.input.saturating_add(tokens.output)
        } else {
            tokens.total
        });
    }

    fn replace_from_codex_event(&mut self, usage: &Value) {
        let read = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
        self.prompt_tokens = read("input_tokens");
        self.completion_tokens = read("output_tokens");
        self.cached_prompt_tokens = read("cached_input_tokens");
        self.reasoning_tokens = read("reasoning_output_tokens");
        self.total_tokens = read("total_tokens");
        if self.total_tokens == 0 {
            self.total_tokens = self.prompt_tokens.saturating_add(self.completion_tokens);
        }
    }
}

#[derive(Debug, Clone)]
pub enum TurnUpdate {
    Text(String),
}

#[derive(Debug, Clone)]
pub struct TurnResult {
    pub text: String,
    pub usage: GatewayUsage,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FileSnapshot {
    records: usize,
    modified: SystemTime,
}

pub struct HistoryTracker {
    format: CliFormat,
    attached: Option<SessionInfo>,
    identity: Arc<RwLock<InstanceIdentity>>,
    local_turn_baseline: Option<HashMap<PathBuf, FileSnapshot>>,
}

impl HistoryTracker {
    pub fn new(
        format: CliFormat,
        session_query: Option<&str>,
        identity: Arc<RwLock<InstanceIdentity>>,
    ) -> Result<Self, String> {
        let attached = match session_query {
            Some(query) => Some(resolve_native_session(format, query)?),
            None => None,
        };

        let mut tracker = Self {
            format,
            attached,
            identity,
            local_turn_baseline: None,
        };
        tracker.refresh_attached_identity();
        Ok(tracker)
    }

    pub fn native_session_id(&self) -> Option<&str> {
        self.attached.as_ref().map(|session| session.id.as_str())
    }

    pub(super) fn capture_baseline(&self) -> HashMap<PathBuf, FileSnapshot> {
        let mut snapshots = HashMap::new();
        for session in self.candidates() {
            let records = read_records(&session).map_or(0, |records| records.len());
            snapshots.insert(
                session.path.clone(),
                FileSnapshot {
                    records,
                    modified: modified_at(&session.path),
                },
            );
        }
        snapshots
    }

    /// Called while the terminal-input gate is held, immediately before a
    /// locally submitted newline is forwarded to the native harness.
    pub(super) fn mark_local_turn_started(&mut self) {
        if self.local_turn_baseline.is_none() {
            self.local_turn_baseline = Some(self.capture_baseline());
        }
    }

    /// Refuse API injection until a terminal-originated turn has crossed a
    /// native completion boundary. This is checked while the same input gate
    /// used by the terminal copier is held, so a local submission cannot race
    /// between this check and the API write.
    pub(super) fn ensure_headful_idle(&mut self) -> Result<(), String> {
        let Some(baseline) = self.local_turn_baseline.as_ref() else {
            return Ok(());
        };
        let Some((session, records)) = self.completed_session_since(baseline) else {
            return Err(
                "the native agent is still processing a terminal-originated turn".to_string(),
            );
        };

        if self.attached.is_none() {
            self.attached = Some(session.clone());
        }
        self.refresh_identity(&session, &records);
        self.local_turn_baseline = None;
        Ok(())
    }

    pub(super) fn collect_completed_turn<F>(
        &mut self,
        prompt: &str,
        baseline: &HashMap<PathBuf, FileSnapshot>,
        timeout: Duration,
        mut emit: F,
    ) -> Result<TurnResult, String>
    where
        F: FnMut(TurnUpdate),
    {
        let started = Instant::now();
        let mut cursor = 0usize;
        let mut selected_path: Option<PathBuf> = self.attached.as_ref().map(|s| s.path.clone());
        let mut text = String::new();
        let mut usage = GatewayUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_prompt_tokens: 0,
            reasoning_tokens: 0,
        };
        let mut saw_assistant_text = false;
        let mut saw_terminal_event = false;
        let mut terminal_error: Option<String> = None;
        let mut last_change = Instant::now();
        let mut last_modified = SystemTime::UNIX_EPOCH;
        let mut fallback_final_message: Option<String> = None;

        loop {
            if started.elapsed() >= timeout {
                return Err(format!(
                    "agent turn did not complete within {} seconds",
                    timeout.as_secs()
                ));
            }

            if self.attached.is_none() {
                if let Some(session) = self.select_session(prompt, baseline) {
                    selected_path = Some(session.path.clone());
                    self.attached = Some(session);
                }
            }

            let Some(session) = self.attached.clone() else {
                std::thread::sleep(POLL_INTERVAL);
                continue;
            };
            let records = match read_records(&session) {
                Ok(records) => records,
                Err(_) => {
                    // A writer may be between bytes of the final JSONL row.
                    std::thread::sleep(POLL_INTERVAL);
                    continue;
                }
            };

            if selected_path.as_ref() != Some(&session.path) {
                selected_path = Some(session.path.clone());
                cursor = 0;
            }
            if cursor == 0 {
                cursor = baseline
                    .get(&session.path)
                    .map(|snapshot| snapshot.records.min(records.len()))
                    .unwrap_or(0);
            }

            let modified = modified_at(&session.path);
            if modified > last_modified {
                last_modified = modified;
                last_change = Instant::now();
            }

            for record in records.iter().skip(cursor) {
                match record {
                    HubRecord::Session(_) => {}
                    HubRecord::Message(message) => {
                        if message.role != "assistant" {
                            continue;
                        }
                        if let Some(tokens) = &message.metadata.tokens {
                            usage.add_tokens(tokens);
                        }
                        for block in &message.content {
                            if let ContentBlock::Text { text: chunk } = block {
                                if !chunk.is_empty() {
                                    saw_assistant_text = true;
                                    text.push_str(chunk);
                                    emit(TurnUpdate::Text(chunk.clone()));
                                }
                            }
                        }
                        if message
                            .metadata
                            .stop_reason
                            .as_deref()
                            .is_some_and(is_terminal_stop_reason)
                        {
                            saw_terminal_event = true;
                        }
                    }
                    HubRecord::Event(event) => {
                        if event.event_type == "token_count" {
                            if let Some(codex_usage) = codex_last_usage(&event.data) {
                                usage.replace_from_codex_event(codex_usage);
                            }
                        }
                        if is_terminal_event(&event.event_type) {
                            saw_terminal_event = true;
                            fallback_final_message = event
                                .data
                                .get("last_agent_message")
                                .and_then(Value::as_str)
                                .map(String::from);
                        }
                        if is_error_event(&event.event_type) {
                            terminal_error = event
                                .data
                                .get("message")
                                .or_else(|| event.data.pointer("/error/message"))
                                .and_then(Value::as_str)
                                .map(String::from);
                        }
                    }
                }
            }
            cursor = records.len();
            self.refresh_identity(&session, &records);

            if saw_terminal_event {
                if !saw_assistant_text {
                    if let Some(final_message) = fallback_final_message.take() {
                        text.push_str(&final_message);
                        emit(TurnUpdate::Text(final_message));
                        saw_assistant_text = true;
                    }
                }
                if let Some(error) = terminal_error {
                    if !saw_assistant_text {
                        return Err(error);
                    }
                }
                return Ok(TurnResult { text, usage });
            }

            // Some harness histories do not persist an explicit turn boundary.
            // A completed assistant text followed by a stable native history is
            // the conservative cross-harness fallback. Codex and Claude use
            // their explicit task_complete/stop_reason markers above.
            if saw_assistant_text
                && !matches!(self.format, CliFormat::ClaudeCode | CliFormat::Codex)
                && last_change.elapsed() >= QUIET_COMPLETION
            {
                return Ok(TurnResult { text, usage });
            }

            std::thread::sleep(POLL_INTERVAL);
        }
    }

    pub(super) fn collect_after_process<F>(
        &mut self,
        prompt: &str,
        baseline: &HashMap<PathBuf, FileSnapshot>,
        mut emit: F,
    ) -> Result<TurnResult, String>
    where
        F: FnMut(TurnUpdate),
    {
        if self.attached.is_none() {
            self.attached = self
                .select_session(prompt, baseline)
                .or_else(|| self.most_recent_changed_session(baseline));
        }
        let session = self.attached.clone().ok_or_else(|| {
            "the harness exited without creating a discoverable session".to_string()
        })?;
        let records = read_records(&session)?;
        let cursor = baseline
            .get(&session.path)
            .map(|snapshot| snapshot.records.min(records.len()))
            .unwrap_or(0);
        let mut text = String::new();
        let mut fallback = None;
        let mut usage = GatewayUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_prompt_tokens: 0,
            reasoning_tokens: 0,
        };

        for record in records.iter().skip(cursor) {
            match record {
                HubRecord::Message(message) if message.role == "assistant" => {
                    if let Some(tokens) = &message.metadata.tokens {
                        usage.add_tokens(tokens);
                    }
                    for block in &message.content {
                        if let ContentBlock::Text { text: chunk } = block {
                            if !chunk.is_empty() {
                                text.push_str(chunk);
                                emit(TurnUpdate::Text(chunk.clone()));
                            }
                        }
                    }
                }
                HubRecord::Event(event) => {
                    if event.event_type == "token_count" {
                        if let Some(codex_usage) = codex_last_usage(&event.data) {
                            usage.replace_from_codex_event(codex_usage);
                        }
                    }
                    if is_terminal_event(&event.event_type) {
                        fallback = event
                            .data
                            .get("last_agent_message")
                            .and_then(Value::as_str)
                            .map(String::from);
                    }
                }
                _ => {}
            }
        }
        if text.is_empty() {
            if let Some(final_message) = fallback {
                text.push_str(&final_message);
                emit(TurnUpdate::Text(final_message));
            }
        }
        if text.is_empty() {
            return Err("the harness completed without an assistant message".to_string());
        }
        self.refresh_identity(&session, &records);
        Ok(TurnResult { text, usage })
    }

    fn candidates(&self) -> Vec<SessionInfo> {
        discover_native(self.format)
    }

    fn select_session(
        &self,
        prompt: &str,
        baseline: &HashMap<PathBuf, FileSnapshot>,
    ) -> Option<SessionInfo> {
        self.candidates().into_iter().find(|session| {
            let modified = modified_at(&session.path);
            let old = baseline.get(&session.path);
            if old.is_some_and(|snapshot| modified <= snapshot.modified) {
                return false;
            }
            let Ok(records) = read_records(session) else {
                return false;
            };
            let cursor = old.map_or(0, |snapshot| snapshot.records.min(records.len()));
            records
                .iter()
                .skip(cursor)
                .filter_map(|record| match record {
                    HubRecord::Message(message) if message.role == "user" => {
                        message_text(&message.content)
                    }
                    HubRecord::Event(event) if event.event_type.ends_with("user_message") => event
                        .data
                        .get("message")
                        .and_then(Value::as_str)
                        .map(String::from),
                    _ => None,
                })
                .any(|candidate| prompts_match(&candidate, prompt))
        })
    }

    fn most_recent_changed_session(
        &self,
        baseline: &HashMap<PathBuf, FileSnapshot>,
    ) -> Option<SessionInfo> {
        self.candidates().into_iter().find(|session| {
            let modified = modified_at(&session.path);
            baseline
                .get(&session.path)
                .is_none_or(|snapshot| modified > snapshot.modified)
        })
    }

    fn completed_session_since(
        &self,
        baseline: &HashMap<PathBuf, FileSnapshot>,
    ) -> Option<(SessionInfo, Vec<HubRecord>)> {
        let candidates = self
            .attached
            .clone()
            .map_or_else(|| self.candidates(), |session| vec![session]);

        candidates.into_iter().find_map(|session| {
            let records = read_records(&session).ok()?;
            let cursor = baseline
                .get(&session.path)
                .map_or(0, |snapshot| snapshot.records.min(records.len()));
            let appended = records.get(cursor..)?;
            if appended.is_empty() {
                return None;
            }

            let mut saw_assistant_text = false;
            let mut saw_terminal = false;
            for record in appended {
                match record {
                    HubRecord::Message(message) if message.role == "assistant" => {
                        saw_assistant_text |= message_text(&message.content).is_some();
                        saw_terminal |= message
                            .metadata
                            .stop_reason
                            .as_deref()
                            .is_some_and(is_terminal_stop_reason);
                    }
                    HubRecord::Event(event) => {
                        saw_terminal |= is_terminal_event(&event.event_type)
                            || is_terminal_failure_event(&event.event_type);
                    }
                    _ => {}
                }
            }

            let quiet_fallback = saw_assistant_text
                && !matches!(self.format, CliFormat::ClaudeCode | CliFormat::Codex)
                && modified_at(&session.path).elapsed().unwrap_or_default() >= QUIET_COMPLETION;
            (saw_terminal || quiet_fallback).then_some((session, records))
        })
    }

    fn refresh_attached_identity(&mut self) {
        let Some(session) = self.attached.clone() else {
            return;
        };
        if let Ok(records) = read_records(&session) {
            self.refresh_identity(&session, &records);
        }
    }

    fn refresh_identity(&self, session: &SessionInfo, records: &[HubRecord]) {
        self.identity
            .write()
            .expect("identity poisoned")
            .update_from_session(session, records);
    }
}

fn session_matches_format(session: &SessionInfo, format: CliFormat) -> bool {
    matches!(
        (format, session.cli.as_str()),
        (CliFormat::ClaudeCode, "claude")
            | (CliFormat::Codex, "codex")
            | (CliFormat::GeminiCli, "gemini" | "antigravity" | "agy")
            | (CliFormat::Hermes, "hermes")
            | (CliFormat::OpenCode, "opencode")
            | (CliFormat::Pi, "pi")
            | (CliFormat::Ucf, "ucf")
    )
}

fn resolve_native_session(format: CliFormat, query: &str) -> Result<SessionInfo, String> {
    let needle = query
        .split_once(':')
        .map_or(query, |(_, session_part)| session_part)
        .to_lowercase();
    let sessions = discover_native(format);
    sessions
        .iter()
        .find(|session| {
            session_matches_format(session, format)
                && (session.id.to_lowercase() == needle
                    || session
                        .name
                        .as_deref()
                        .is_some_and(|name| name.to_lowercase() == needle))
        })
        .or_else(|| {
            sessions.iter().find(|session| {
                session_matches_format(session, format)
                    && session.id.to_lowercase().starts_with(&needle)
            })
        })
        .cloned()
        .ok_or_else(|| format!("session '{query}' was not found for the selected harness"))
}

fn read_records(session: &SessionInfo) -> Result<Vec<HubRecord>, String> {
    source_to_hub(session).map_err(|error| error.to_string())
}

fn modified_at(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn message_text(content: &[ContentBlock]) -> Option<String> {
    let mut result = String::new();
    for block in content {
        if let ContentBlock::Text { text } = block {
            result.push_str(text);
        }
    }
    (!result.is_empty()).then_some(result)
}

fn prompts_match(native: &str, requested: &str) -> bool {
    native.trim().replace("\r\n", "\n") == requested.trim().replace("\r\n", "\n")
}

fn is_terminal_stop_reason(reason: &str) -> bool {
    !matches!(reason, "" | "tool_use" | "pause_turn")
}

fn is_terminal_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "codex_task_complete"
            | "codex_turn_complete"
            | "task_complete"
            | "turn_complete"
            | "agent_end"
    )
}

fn is_error_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "codex_error" | "codex_turn_aborted" | "turn_failed" | "error"
    )
}

fn is_terminal_failure_event(event_type: &str) -> bool {
    matches!(event_type, "codex_turn_aborted" | "turn_failed")
}

fn codex_last_usage(data: &Value) -> Option<&Value> {
    data.pointer("/info/last_token_usage")
        .or_else(|| data.pointer("/info/total_token_usage"))
        .or_else(|| data.get("last_token_usage"))
        .or_else(|| data.get("total_token_usage"))
}

fn model_from_record(record: &HubRecord) -> Option<String> {
    match record {
        HubRecord::Session(session) => session.model.clone(),
        HubRecord::Message(message) => message.metadata.model.clone(),
        HubRecord::Event(event) => event
            .data
            .get("model")
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| {
                event
                    .data
                    .pointer("/thread_settings/model")
                    .and_then(Value::as_str)
                    .map(String::from)
            }),
    }
}

pub fn slug_component(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            separator = false;
        } else if !separator && !slug.is_empty() {
            slug.push('-');
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "unnamed".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interchange::hub::{HubEvent, HubMessage, MessageMetadata, SessionHeader};
    use std::io::Write;

    #[test]
    fn model_slug_combines_instance_model_and_harness() {
        let identity = InstanceIdentity::new(
            "Work Auth".into(),
            Some("GPT-5.6 Codex".into()),
            "Codex".into(),
            "headful",
            true,
        );
        assert_eq!(identity.model_id(), "unleash/work-auth/gpt-5-6-codex/codex");
    }

    #[test]
    fn codex_current_token_payload_uses_last_turn_usage() {
        let event = HubRecord::Event(HubEvent {
            event_type: "token_count".into(),
            timestamp: String::new(),
            data: serde_json::json!({
                "type": "token_count",
                "info": {
                    "total_token_usage": {"input_tokens": 900, "output_tokens": 90},
                    "last_token_usage": {
                        "input_tokens": 100,
                        "cached_input_tokens": 60,
                        "output_tokens": 20,
                        "reasoning_output_tokens": 12,
                        "total_tokens": 120
                    }
                }
            }),
            extensions: Value::Null,
        });
        let HubRecord::Event(event) = event else {
            unreachable!()
        };
        let mut usage = GatewayUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cached_prompt_tokens: 0,
            reasoning_tokens: 0,
        };
        usage.replace_from_codex_event(codex_last_usage(&event.data).unwrap());
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.cached_prompt_tokens, 60);
        assert_eq!(usage.reasoning_tokens, 12);
        assert_eq!(usage.total_tokens, 120);
    }

    #[test]
    fn model_discovery_handles_header_message_and_codex_turn_context() {
        let records = [
            HubRecord::Session(SessionHeader {
                ucf_version: "1".into(),
                session_id: "s".into(),
                created_at: String::new(),
                updated_at: String::new(),
                source_cli: "codex".into(),
                source_version: String::new(),
                project: None,
                model: None,
                title: None,
                slug: None,
                parent_session_id: None,
                extensions: Value::Null,
            }),
            HubRecord::Message(HubMessage {
                id: "m".into(),
                api_message_id: None,
                parent_id: None,
                timestamp: String::new(),
                completed_at: None,
                role: "assistant".into(),
                content: vec![],
                metadata: MessageMetadata {
                    model: Some("older".into()),
                    ..Default::default()
                },
                extensions: Value::Null,
            }),
            HubRecord::Event(HubEvent {
                event_type: "turn_context".into(),
                timestamp: String::new(),
                data: serde_json::json!({"model": "gpt-5.6"}),
                extensions: Value::Null,
            }),
        ];
        assert_eq!(
            records.iter().rev().find_map(model_from_record).as_deref(),
            Some("gpt-5.6")
        );
    }

    #[test]
    fn attached_codex_history_projects_one_new_turn() {
        let _environment = crate::test_env::lock();
        let temporary = tempfile::tempdir().unwrap();
        let session_dir = temporary.path().join("sessions/2026/07/26");
        std::fs::create_dir_all(&session_dir).unwrap();
        let history = session_dir.join("rollout-2026-07-26T10-00-00-session-431.jsonl");
        std::fs::write(
            &history,
            concat!(
                "{\"timestamp\":\"2026-07-26T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"session-431\",\"cwd\":\"/tmp/project\",\"cli_version\":\"0.145.0\"}}\n",
                "{\"timestamp\":\"2026-07-26T10:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"old turn\"}]}}\n",
                "{\"timestamp\":\"2026-07-26T10:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"old answer\"}]}}\n"
            ),
        )
        .unwrap();
        let saved_home = std::env::var_os("CODEX_HOME");
        // SAFETY: guarded by the crate-wide environment lock and restored.
        unsafe { std::env::set_var("CODEX_HOME", temporary.path()) };

        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "codex".into(),
            None,
            "codex".into(),
            "headful",
            false,
        )));
        let mut tracker = HistoryTracker::new(
            CliFormat::Codex,
            Some("codex:session-431"),
            Arc::clone(&identity),
        )
        .unwrap();
        let baseline = tracker.capture_baseline();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&history)
            .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "timestamp": "2026-07-26T10:01:00Z",
                "type": "turn_context",
                "payload": {"model": "gpt-5.6"}
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "timestamp": "2026-07-26T10:01:01Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "new turn"}]
                }
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "timestamp": "2026-07-26T10:01:02Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "new answer"}]
                }
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "timestamp": "2026-07-26T10:01:03Z",
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "last_token_usage": {
                            "input_tokens": 50,
                            "cached_input_tokens": 30,
                            "output_tokens": 10,
                            "reasoning_output_tokens": 4,
                            "total_tokens": 60
                        }
                    }
                }
            })
        )
        .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({
                "timestamp": "2026-07-26T10:01:04Z",
                "type": "event_msg",
                "payload": {
                    "type": "task_complete",
                    "last_agent_message": "new answer"
                }
            })
        )
        .unwrap();
        file.flush().unwrap();

        let mut chunks = Vec::new();
        let result = tracker
            .collect_completed_turn(
                "new turn",
                &baseline,
                Duration::from_secs(1),
                |TurnUpdate::Text(text)| chunks.push(text),
            )
            .unwrap();

        match saved_home {
            Some(value) => unsafe { std::env::set_var("CODEX_HOME", value) },
            None => unsafe { std::env::remove_var("CODEX_HOME") },
        }
        assert_eq!(result.text, "new answer");
        assert_eq!(chunks, vec!["new answer"]);
        assert_eq!(result.usage.prompt_tokens, 50);
        assert_eq!(result.usage.completion_tokens, 10);
        assert_eq!(
            identity.read().unwrap().model_id(),
            "unleash/codex/gpt-5-6/codex"
        );
    }
}
