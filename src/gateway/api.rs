use super::history::{GatewayUsage, InstanceIdentity, TurnResult, TurnUpdate};
use super::{Driver, TurnStreamEvent};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::{Duration, SystemTime};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
static RESPONSE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct AppState {
    identity: Arc<RwLock<InstanceIdentity>>,
    driver: Driver,
    turn_gate: Arc<Mutex<()>>,
    turns: Arc<StdMutex<TurnRegistry>>,
    api_key: Option<Arc<str>>,
}

#[derive(Clone, PartialEq, Eq)]
struct TurnFingerprint {
    model: String,
    prompt: String,
}

struct TurnRegistry {
    entries: HashMap<String, TurnEntry>,
}

struct TurnEntry {
    fingerprint: TurnFingerprint,
    response_id: String,
    created: u64,
    state: StoredTurn,
}

enum StoredTurn {
    Running {
        sender: broadcast::Sender<TurnStreamEvent>,
        updates: Vec<TurnUpdate>,
    },
    Complete {
        result: TurnResult,
        updates: Vec<TurnUpdate>,
    },
    Failed {
        message: String,
        updates: Vec<TurnUpdate>,
    },
}

enum TurnSource {
    Live {
        receiver: broadcast::Receiver<TurnStreamEvent>,
        prefix: VecDeque<TurnStreamEvent>,
        registry: Arc<StdMutex<TurnRegistry>>,
        key: String,
        cursor: usize,
    },
    Complete {
        result: Option<TurnResult>,
        text_emitted: bool,
    },
    Failed(Option<String>),
}

pub async fn run(
    listener: TcpListener,
    identity: Arc<RwLock<InstanceIdentity>>,
    driver: Driver,
    api_key: Option<String>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let state = AppState {
        identity,
        driver,
        turn_gate: Arc::new(Mutex::new(())),
        turns: Arc::new(StdMutex::new(TurnRegistry {
            entries: HashMap::new(),
        })),
        api_key: api_key.map(Arc::from),
    };
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(chat_completions))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state);

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(std::io::Error::other)
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

async fn models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return error.into_response();
    }
    let identity = state.identity.read().expect("identity poisoned").clone();
    Json(json!({
        "object": "list",
        "data": [{
            "id": identity.model_id(),
            "object": "model",
            "created": identity.created,
            "owned_by": "unleash",
            "x-unleash": {
                "instance_name": identity.instance_name,
                "session_id": identity.native_session_id,
                "model_slug": identity.model_slug,
                "harness": identity.harness,
                "mode": identity.mode
            }
        }]
    }))
    .into_response()
}

async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<ChatCompletionRequest>, JsonRejection>,
) -> Response {
    if let Err(error) = authorize(&state, &headers) {
        return error.into_response();
    }
    let request = match payload {
        Ok(Json(request)) => request,
        Err(error) => {
            return ApiError::bad_request(
                format!("invalid Chat Completions request: {}", error.body_text()),
                Some("invalid_request_error"),
            )
            .into_response()
        }
    };
    if request.n.is_some_and(|n| n != 1) {
        return ApiError::bad_request("only n=1 is supported", Some("unsupported_value"))
            .into_response();
    }
    if request
        .tools
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
    {
        return ApiError::bad_request(
            "client-owned tool calls are not supported; tools remain owned by the attached agent",
            Some("unsupported_tools"),
        )
        .into_response();
    }
    let prompt = match prompt_from_messages(&request.messages) {
        Ok(prompt) => prompt,
        Err(error) => return error.into_response(),
    };
    let idempotency_key = match idempotency_key(&headers) {
        Ok(key) => key,
        Err(error) => return error.into_response(),
    };
    let fingerprint = TurnFingerprint {
        model: request.model.clone(),
        prompt: prompt.clone(),
    };
    let (response_id, created, source) =
        match begin_or_replay_turn(&state, idempotency_key, fingerprint, prompt) {
            Ok(turn) => turn,
            Err(error) => return error.into_response(),
        };

    if request.stream.unwrap_or(false) {
        streaming_response(
            source,
            response_id,
            created,
            request.model.clone(),
            request
                .stream_options
                .as_ref()
                .is_some_and(|options| options.include_usage),
        )
    } else {
        non_streaming_response(source, response_id, created, request.model.clone()).await
    }
}

fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get("idempotency-key")
        .ok_or_else(|| {
            ApiError::bad_request(
                "Idempotency-Key is required for every stateful agent turn",
                Some("idempotency_key_required"),
            )
        })?
        .to_str()
        .map_err(|_| {
            ApiError::bad_request(
                "Idempotency-Key must contain visible ASCII text",
                Some("invalid_idempotency_key"),
            )
        })?;
    if value.is_empty()
        || value.len() > 255
        || value.bytes().any(|byte| !(0x21..=0x7e).contains(&byte))
    {
        return Err(ApiError::bad_request(
            "Idempotency-Key must be 1 to 255 visible ASCII characters",
            Some("invalid_idempotency_key"),
        ));
    }
    Ok(value.to_string())
}

fn begin_or_replay_turn(
    state: &AppState,
    key: String,
    fingerprint: TurnFingerprint,
    prompt: String,
) -> Result<(String, u64, TurnSource), ApiError> {
    let mut turns = state
        .turns
        .lock()
        .map_err(|_| ApiError::internal("idempotency registry lock was poisoned"))?;
    if let Some(entry) = turns.entries.get(&key) {
        if entry.fingerprint != fingerprint {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "Idempotency-Key was already used for a different model or user turn",
                Some("idempotency_key_conflict"),
            ));
        }
        let source = match &entry.state {
            StoredTurn::Running { sender, updates } => TurnSource::Live {
                receiver: sender.subscribe(),
                prefix: updates
                    .iter()
                    .cloned()
                    .map(TurnStreamEvent::Update)
                    .collect(),
                registry: Arc::clone(&state.turns),
                key: key.clone(),
                cursor: updates.len(),
            },
            StoredTurn::Complete { result, .. } => TurnSource::Complete {
                result: Some(result.clone()),
                text_emitted: false,
            },
            StoredTurn::Failed { message, .. } => TurnSource::Failed(Some(message.clone())),
        };
        return Ok((entry.response_id.clone(), entry.created, source));
    }

    let current_model = state.identity.read().expect("identity poisoned").model_id();
    if fingerprint.model != current_model {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!(
                "model '{}' is not a live instance; use GET /v1/models",
                fingerprint.model
            ),
            Some("model_not_found"),
        ));
    }

    let gate = Arc::clone(&state.turn_gate).try_lock_owned().map_err(|_| {
        ApiError::new(
            StatusCode::CONFLICT,
            "this stateful agent instance is already processing a different turn",
            Some("instance_busy"),
        )
    })?;
    let driver_receiver = state.driver.start_turn(prompt, gate).map_err(|error| {
        ApiError::new(
            if error.code == "native_instance_busy" {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_GATEWAY
            },
            error.message,
            Some(error.code),
        )
    })?;
    let response_id = next_response_id();
    let created = unix_timestamp();
    let (sender, _) = broadcast::channel(64);
    let source = TurnSource::Live {
        receiver: sender.subscribe(),
        prefix: VecDeque::new(),
        registry: Arc::clone(&state.turns),
        key: key.clone(),
        cursor: 0,
    };
    turns.entries.insert(
        key.clone(),
        TurnEntry {
            fingerprint,
            response_id: response_id.clone(),
            created,
            state: StoredTurn::Running {
                sender: sender.clone(),
                updates: Vec::new(),
            },
        },
    );
    drop(turns);

    let registry = Arc::clone(&state.turns);
    tokio::spawn(async move {
        monitor_turn(key, driver_receiver, sender, registry).await;
    });
    Ok((response_id, created, source))
}

async fn monitor_turn(
    key: String,
    mut receiver: mpsc::Receiver<TurnStreamEvent>,
    sender: broadcast::Sender<TurnStreamEvent>,
    registry: Arc<StdMutex<TurnRegistry>>,
) {
    while let Some(event) = receiver.recv().await {
        match &event {
            TurnStreamEvent::Update(update) => {
                store_and_publish_turn_update(&registry, &key, update.clone(), &sender);
                continue;
            }
            TurnStreamEvent::Complete(result) => {
                store_terminal_turn_complete(&registry, &key, result.clone());
            }
            TurnStreamEvent::Error(message) => {
                store_terminal_turn_failed(&registry, &key, message.clone());
            }
        }
        let _ = sender.send(event);
        return;
    }

    let message = "agent stream ended unexpectedly".to_string();
    store_terminal_turn_failed(&registry, &key, message.clone());
    let _ = sender.send(TurnStreamEvent::Error(message));
}

fn store_terminal_turn_complete(registry: &StdMutex<TurnRegistry>, key: &str, result: TurnResult) {
    let Ok(mut registry) = registry.lock() else {
        return;
    };
    let Some(entry) = registry.entries.get_mut(key) else {
        return;
    };
    if let StoredTurn::Running { updates, .. } = &mut entry.state {
        entry.state = StoredTurn::Complete {
            result,
            updates: std::mem::take(updates),
        };
    }
}

fn store_terminal_turn_failed(registry: &StdMutex<TurnRegistry>, key: &str, message: String) {
    let Ok(mut registry) = registry.lock() else {
        return;
    };
    let Some(entry) = registry.entries.get_mut(key) else {
        return;
    };
    if let StoredTurn::Running { updates, .. } = &mut entry.state {
        entry.state = StoredTurn::Failed {
            message,
            updates: std::mem::take(updates),
        };
    }
}

fn store_and_publish_turn_update(
    registry: &StdMutex<TurnRegistry>,
    key: &str,
    update: TurnUpdate,
    sender: &broadcast::Sender<TurnStreamEvent>,
) {
    let Ok(mut registry) = registry.lock() else {
        return;
    };
    if let Some(TurnEntry {
        state: StoredTurn::Running { updates, .. },
        ..
    }) = registry.entries.get_mut(key)
    {
        updates.push(update.clone());
        // Subscribe and prefix snapshot happen under this same registry lock,
        // so a retry sees this update either in its prefix or on broadcast,
        // never neither (or both).
        let _ = sender.send(TurnStreamEvent::Update(update));
    }
}

impl TurnSource {
    async fn recv(&mut self) -> Option<TurnStreamEvent> {
        match self {
            Self::Live {
                receiver,
                prefix,
                registry,
                key,
                cursor,
            } => loop {
                if let Some(event) = prefix.pop_front() {
                    return Some(event);
                }
                match receiver.recv().await {
                    Ok(event) => {
                        if matches!(event, TurnStreamEvent::Update(_)) {
                            *cursor += 1;
                        }
                        return Some(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let Ok(registry_lock) = registry.lock() else {
                            continue;
                        };
                        let Some(entry) = registry_lock.entries.get(key) else {
                            continue;
                        };
                        let (updates_ref, terminal, new_receiver) = match &entry.state {
                            StoredTurn::Running { updates, sender } => {
                                (updates, None, Some(sender.subscribe()))
                            }
                            StoredTurn::Complete { updates, result } => (
                                updates,
                                Some(TurnStreamEvent::Complete(result.clone())),
                                None,
                            ),
                            StoredTurn::Failed { updates, message } => {
                                (updates, Some(TurnStreamEvent::Error(message.clone())), None)
                            }
                        };
                        for update in updates_ref.iter().skip(*cursor) {
                            prefix.push_back(TurnStreamEvent::Update(update.clone()));
                        }
                        *cursor = updates_ref.len();
                        if let Some(event) = terminal {
                            prefix.push_back(event);
                        }
                        if let Some(rx) = new_receiver {
                            *receiver = rx;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            },
            Self::Complete {
                result,
                text_emitted,
            } => {
                if !*text_emitted {
                    *text_emitted = true;
                    return result.as_ref().map(|result| {
                        TurnStreamEvent::Update(TurnUpdate::Text(result.text.clone()))
                    });
                }
                result.take().map(TurnStreamEvent::Complete)
            }
            Self::Failed(message) => message.take().map(TurnStreamEvent::Error),
        }
    }
}

fn streaming_response(
    mut source: TurnSource,
    response_id: String,
    created: u64,
    initial_model: String,
    include_usage: bool,
) -> Response {
    let (sender, output) = mpsc::channel::<Result<Event, Infallible>>(64);
    tokio::spawn(async move {
        let first = stream_chunk(
            &response_id,
            created,
            &initial_model,
            json!({"role": "assistant", "content": ""}),
            None,
            None,
        );
        if send_sse(&sender, &first).await.is_err() {
            return;
        }

        while let Some(event) = source.recv().await {
            match event {
                TurnStreamEvent::Update(TurnUpdate::Text(text)) => {
                    let chunk = stream_chunk(
                        &response_id,
                        created,
                        &initial_model,
                        json!({"content": text}),
                        None,
                        None,
                    );
                    if send_sse(&sender, &chunk).await.is_err() {
                        return;
                    }
                }
                TurnStreamEvent::Complete(result) => {
                    let final_chunk = stream_chunk(
                        &response_id,
                        created,
                        &initial_model,
                        json!({}),
                        Some("stop"),
                        None,
                    );
                    let _ = send_sse(&sender, &final_chunk).await;
                    if include_usage {
                        let usage_chunk = json!({
                            "id": response_id,
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": initial_model,
                            "choices": [],
                            "usage": usage_json(&result.usage)
                        });
                        let _ = send_sse(&sender, &usage_chunk).await;
                    }
                    let _ = sender.send(Ok(Event::default().data("[DONE]"))).await;
                    return;
                }
                TurnStreamEvent::Error(message) => {
                    let error = json!({
                        "error": {
                            "message": message,
                            "type": "agent_error",
                            "param": null,
                            "code": "agent_turn_failed"
                        }
                    });
                    let _ = send_sse(&sender, &error).await;
                    let _ = sender.send(Ok(Event::default().data("[DONE]"))).await;
                    return;
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(output))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

async fn non_streaming_response(
    mut source: TurnSource,
    response_id: String,
    created: u64,
    initial_model: String,
) -> Response {
    while let Some(event) = source.recv().await {
        match event {
            TurnStreamEvent::Update(_) => {}
            TurnStreamEvent::Complete(result) => {
                return Json(json!({
                    "id": response_id,
                    "object": "chat.completion",
                    "created": created,
                    "model": initial_model,
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": result.text,
                            "refusal": null
                        },
                        "logprobs": null,
                        "finish_reason": "stop"
                    }],
                    "usage": usage_json(&result.usage)
                }))
                .into_response()
            }
            TurnStreamEvent::Error(message) => {
                return ApiError::new(StatusCode::BAD_GATEWAY, message, Some("agent_turn_failed"))
                    .into_response()
            }
        }
    }
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        format!("agent stream for model '{initial_model}' ended unexpectedly"),
        Some("agent_stream_closed"),
    )
    .into_response()
}

fn stream_chunk(
    id: &str,
    created: u64,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
    usage: Option<Value>,
) -> Value {
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "logprobs": null,
            "finish_reason": finish_reason
        }],
        "usage": usage
    })
}

fn usage_json(usage: &GatewayUsage) -> Value {
    json!({
        "prompt_tokens": usage.prompt_tokens,
        "completion_tokens": usage.completion_tokens,
        "total_tokens": usage.total_tokens,
        "prompt_tokens_details": {
            "cached_tokens": usage.cached_prompt_tokens
        },
        "completion_tokens_details": {
            "reasoning_tokens": usage.reasoning_tokens
        }
    })
}

async fn send_sse(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    value: &Value,
) -> Result<(), ()> {
    sender
        .send(Ok(Event::default().data(value.to_string())))
        .await
        .map_err(|_| ())
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.api_key.as_deref() else {
        return Ok(());
    };
    let provided = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided.is_some_and(|provided| constant_time_eq(provided.as_bytes(), expected.as_bytes())) {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing or invalid bearer token",
            Some("invalid_api_key"),
        ))
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut different = left.len() ^ right.len();
    let max = left.len().max(right.len());
    for index in 0..max {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        different |= usize::from(a ^ b);
    }
    different == 0
}

fn prompt_from_messages(messages: &[ChatMessage]) -> Result<String, ApiError> {
    if messages.is_empty() {
        return Err(ApiError::bad_request(
            "messages must contain at least one user message",
            Some("invalid_messages"),
        ));
    }
    if messages.iter().any(|message| message.role == "tool") {
        return Err(ApiError::bad_request(
            "tool result messages cannot be injected into an agent-owned tool loop",
            Some("unsupported_tools"),
        ));
    }
    let message = messages.last().ok_or_else(|| {
        ApiError::bad_request(
            "messages must contain a user message",
            Some("invalid_messages"),
        )
    })?;
    if message.role != "user" {
        return Err(ApiError::bad_request(
            "the final message must be a new user turn",
            Some("invalid_messages"),
        ));
    }
    let prompt = content_text(&message.content)?;
    if prompt.trim().is_empty() {
        return Err(ApiError::bad_request(
            "the final user message must contain text",
            Some("invalid_messages"),
        ));
    }
    Ok(prompt)
}

fn content_text(content: &Value) -> Result<String, ApiError> {
    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text" | "input_text") => {
                        let value = part.get("text").and_then(Value::as_str).ok_or_else(|| {
                            ApiError::bad_request(
                                "text content parts require a string 'text' field",
                                Some("invalid_messages"),
                            )
                        })?;
                        text.push_str(value);
                    }
                    Some(other) => {
                        return Err(ApiError::bad_request(
                            format!("message content part '{other}' is not supported"),
                            Some("unsupported_content"),
                        ))
                    }
                    None => {
                        return Err(ApiError::bad_request(
                            "message content parts require a 'type' field",
                            Some("invalid_messages"),
                        ))
                    }
                }
            }
            Ok(text)
        }
        _ => Err(ApiError::bad_request(
            "message content must be a string or an array of text parts",
            Some("invalid_messages"),
        )),
    }
}

fn next_response_id() -> String {
    format!(
        "chatcmpl-unleash-{}-{}",
        unix_timestamp(),
        RESPONSE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Deserialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    stream_options: Option<StreamOptions>,
    #[serde(default)]
    n: Option<u32>,
    #[serde(default)]
    tools: Option<Value>,
    #[serde(flatten)]
    _extra: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    role: String,
    content: Value,
    #[serde(flatten)]
    _extra: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct StreamOptions {
    #[serde(default)]
    include_usage: bool,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    message: String,
    #[serde(rename = "type")]
    error_type: &'static str,
    param: Option<&'static str>,
    code: Option<&'static str>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    body: ErrorEnvelope,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>, code: Option<&'static str>) -> Self {
        Self {
            status,
            body: ErrorEnvelope {
                error: ErrorBody {
                    message: message.into(),
                    error_type: "invalid_request_error",
                    param: None,
                    code,
                },
            },
        }
    }

    fn bad_request(message: impl Into<String>, code: Option<&'static str>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message, code)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            message,
            Some("internal_error"),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn extracts_only_the_latest_user_turn_from_client_history() {
        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: json!("be terse"),
                _extra: Default::default(),
            },
            ChatMessage {
                role: "user".into(),
                content: json!("first"),
                _extra: Default::default(),
            },
            ChatMessage {
                role: "assistant".into(),
                content: json!("answer"),
                _extra: Default::default(),
            },
            ChatMessage {
                role: "user".into(),
                content: json!([
                    {"type": "text", "text": "second"},
                    {"type": "text", "text": " turn"}
                ]),
                _extra: Default::default(),
            },
        ];
        assert_eq!(prompt_from_messages(&messages).unwrap(), "second turn");
    }

    #[test]
    fn rejects_client_owned_tool_results() {
        let messages = vec![ChatMessage {
            role: "tool".into(),
            content: json!("result"),
            _extra: Default::default(),
        }];
        assert!(prompt_from_messages(&messages).is_err());
    }

    #[test]
    fn rejects_transcript_that_does_not_end_in_a_new_user_turn() {
        let messages = vec![
            ChatMessage {
                role: "user".into(),
                content: json!("already submitted"),
                _extra: Default::default(),
            },
            ChatMessage {
                role: "assistant".into(),
                content: json!("already answered"),
                _extra: Default::default(),
            },
        ];
        let error = prompt_from_messages(&messages).unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.body.error.code, Some("invalid_messages"));
    }

    #[test]
    fn bearer_comparison_handles_different_lengths() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secre"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
    }

    #[test]
    fn streaming_chunk_uses_openai_shape() {
        let chunk = stream_chunk(
            "chatcmpl-1",
            1,
            "unleash/work/gpt/codex",
            json!({"content": "hello"}),
            None,
            None,
        );
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert_eq!(chunk["choices"][0]["delta"]["content"], "hello");
        assert!(chunk["choices"][0]["finish_reason"].is_null());
    }

    #[tokio::test]
    async fn http_surface_lists_instance_and_returns_chat_completion() {
        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "review-agent".into(),
            Some("gpt-5.6".into()),
            "codex".into(),
            "headful",
            true,
        )));
        let model_id = identity.read().unwrap().model_id();
        let starts = Arc::new(AtomicUsize::new(0));
        let driver = Driver::Test {
            result: TurnResult {
                text: "Ready for review.".into(),
                usage: GatewayUsage {
                    prompt_tokens: 100,
                    completion_tokens: 20,
                    total_tokens: 120,
                    cached_prompt_tokens: 60,
                    reasoning_tokens: 10,
                },
            },
            updates: vec![],
            delay: Duration::ZERO,
            starts: Arc::clone(&starts),
            pause: None,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(run(
            listener,
            identity,
            driver,
            Some("test-key".into()),
            async move {
                let _ = shutdown_rx.await;
            },
        ));
        let client = reqwest::Client::new();

        let unauthorized = client
            .get(format!("http://{address}/v1/models"))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let models: Value = client
            .get(format!("http://{address}/v1/models"))
            .bearer_auth("test-key")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(models["data"][0]["id"], model_id);
        assert_eq!(models["data"][0]["x-unleash"]["harness"], "codex");

        let completion: Value = client
            .post(format!("http://{address}/v1/chat/completions"))
            .bearer_auth("test-key")
            .header("Idempotency-Key", "status-turn")
            .json(&json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "Status?"}]
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            completion["choices"][0]["message"]["content"],
            "Ready for review."
        );
        assert_eq!(completion["usage"]["prompt_tokens"], 100);
        assert_eq!(
            completion["usage"]["completion_tokens_details"]["reasoning_tokens"],
            10
        );

        let stream_body = client
            .post(format!("http://{address}/v1/chat/completions"))
            .bearer_auth("test-key")
            .header("Idempotency-Key", "stream-status-turn")
            .json(&json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "Stream status?"}],
                "stream": true,
                "stream_options": {"include_usage": true}
            }))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(stream_body.contains("\"object\":\"chat.completion.chunk\""));
        assert!(stream_body.contains("\"content\":\"Ready for review.\""));
        assert!(stream_body.contains("\"choices\":[]"));
        assert!(stream_body.contains("data: [DONE]"));
        assert_eq!(starts.load(Ordering::SeqCst), 2);

        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn disconnected_retry_attaches_or_replays_without_a_second_native_turn() {
        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "retry-agent".into(),
            Some("gpt-5.6".into()),
            "codex".into(),
            "headful",
            true,
        )));
        let model_id = identity.read().unwrap().model_id();
        let starts = Arc::new(AtomicUsize::new(0));
        let driver = Driver::Test {
            result: TurnResult {
                text: "Executed once.".into(),
                usage: GatewayUsage {
                    prompt_tokens: 10,
                    completion_tokens: 2,
                    total_tokens: 12,
                    cached_prompt_tokens: 0,
                    reasoning_tokens: 0,
                },
            },
            updates: vec![],
            delay: Duration::from_millis(200),
            starts: Arc::clone(&starts),
            pause: None,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(run(listener, identity, driver, None, async move {
            let _ = shutdown_rx.await;
        }));
        let client = reqwest::Client::new();
        let endpoint = format!("http://{address}/v1/chat/completions");
        let body = json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "Do the stateful thing"}],
            "stream": true
        });

        let disconnected = client
            .post(&endpoint)
            .header("Idempotency-Key", "retry-turn-1")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(disconnected.status(), StatusCode::OK);
        drop(disconnected);

        let replay_body = json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "Do the stateful thing"}]
        });
        let retry: Value = client
            .post(&endpoint)
            .header("Idempotency-Key", "retry-turn-1")
            .json(&replay_body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let cached: Value = client
            .post(&endpoint)
            .header("Idempotency-Key", "retry-turn-1")
            .json(&replay_body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(retry["choices"][0]["message"]["content"], "Executed once.");
        assert_eq!(cached["id"], retry["id"]);
        assert_eq!(cached["created"], retry["created"]);
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        let cached_stream = client
            .post(&endpoint)
            .header("Idempotency-Key", "retry-turn-1")
            .json(&body)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(cached_stream.contains("\"content\":\"Executed once.\""));
        assert!(cached_stream.contains("data: [DONE]"));
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        let conflict = client
            .post(&endpoint)
            .header("Idempotency-Key", "retry-turn-1")
            .json(&json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "Different turn"}]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);

        let missing_key: Value = client
            .post(&endpoint)
            .json(&replay_body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            missing_key["error"]["code"],
            Value::String("idempotency_key_required".into())
        );

        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn changing_identity_replay_regression() {
        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "changing-agent".into(),
            Some("gpt-4".into()),
            "harness".into(),
            "headful",
            true,
        )));
        let model_id_1 = identity.read().unwrap().model_id();
        let starts = Arc::new(AtomicUsize::new(0));
        let driver = Driver::Test {
            result: TurnResult {
                text: "Original turn.".into(),
                usage: GatewayUsage {
                    prompt_tokens: 10,
                    completion_tokens: 2,
                    total_tokens: 12,
                    cached_prompt_tokens: 0,
                    reasoning_tokens: 0,
                },
            },
            updates: vec![],
            delay: Duration::ZERO,
            starts: Arc::clone(&starts),
            pause: None,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(run(
            listener,
            Arc::clone(&identity),
            driver,
            None,
            async move {
                let _ = shutdown_rx.await;
            },
        ));
        let client = reqwest::Client::new();
        let endpoint = format!("http://{address}/v1/chat/completions");
        let body = json!({
            "model": model_id_1,
            "messages": [{"role": "user", "content": "Hello"}]
        });

        let first: Value = client
            .post(&endpoint)
            .header("Idempotency-Key", "id-key")
            .json(&body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(first["choices"][0]["message"]["content"], "Original turn.");

        *identity.write().unwrap() = InstanceIdentity::new(
            "changing-agent".into(),
            Some("gpt-5".into()),
            "harness".into(),
            "headful",
            true,
        );

        let replay: Value = client
            .post(&endpoint)
            .header("Idempotency-Key", "id-key")
            .json(&body)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(replay["choices"][0]["message"]["content"], "Original turn.");

        let new_turn = client
            .post(&endpoint)
            .header("Idempotency-Key", "new-key")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(new_turn.status(), StatusCode::NOT_FOUND);

        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn slow_sse_client_receives_lossless_delivery() {
        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "lossless-agent".into(),
            Some("gpt-lossless".into()),
            "codex".into(),
            "headful",
            true,
        )));
        let model_id = identity.read().unwrap().model_id();
        let starts = Arc::new(AtomicUsize::new(0));

        let mut updates = Vec::new();
        for i in 0..100 {
            updates.push(TurnUpdate::Text(format!("chunk{} ", i)));
        }

        let driver = Driver::Test {
            result: TurnResult {
                text: updates
                    .iter()
                    .map(|u| {
                        let TurnUpdate::Text(t) = u;
                        t.clone()
                    })
                    .collect::<String>(),
                usage: GatewayUsage {
                    prompt_tokens: 10,
                    completion_tokens: 100,
                    total_tokens: 110,
                    cached_prompt_tokens: 0,
                    reasoning_tokens: 0,
                },
            },
            updates,
            delay: Duration::ZERO,
            starts: Arc::clone(&starts),
            pause: None,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(run(listener, identity, driver, None, async move {
            let _ = shutdown_rx.await;
        }));

        let client = reqwest::Client::new();
        let endpoint = format!("http://{address}/v1/chat/completions");

        let mut response = client
            .post(&endpoint)
            .header("Idempotency-Key", "lossless-key")
            .json(&json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "Stream it"}],
                "stream": true
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let mut text = String::new();
        while let Some(chunk) = response.chunk().await.unwrap() {
            let chunk_str = String::from_utf8(chunk.to_vec()).unwrap();
            text.push_str(&chunk_str);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        for i in 0..100 {
            assert!(
                text.contains(&format!("\"content\":\"chunk{} \"", i)),
                "Missing chunk {}",
                i
            );
        }

        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn lagging_sse_client_receives_in_order_without_duplicates_during_running_turn() {
        let identity = Arc::new(RwLock::new(InstanceIdentity::new(
            "lagging-agent".into(),
            Some("gpt-lagging".into()),
            "codex".into(),
            "headful",
            true,
        )));
        let model_id = identity.read().unwrap().model_id();
        let starts = Arc::new(AtomicUsize::new(0));

        let mut updates = Vec::new();
        for i in 0..100 {
            updates.push(TurnUpdate::Text(format!("chunk{} ", i)));
        }

        let driver = Driver::Test {
            result: TurnResult {
                text: updates
                    .iter()
                    .map(|u| {
                        let TurnUpdate::Text(t) = u;
                        t.clone()
                    })
                    .collect::<String>(),
                usage: GatewayUsage {
                    prompt_tokens: 10,
                    completion_tokens: 100,
                    total_tokens: 110,
                    cached_prompt_tokens: 0,
                    reasoning_tokens: 0,
                },
            },
            updates,
            delay: Duration::ZERO,
            starts: Arc::clone(&starts),
            pause: Some((80, Duration::from_millis(200))),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(run(listener, identity, driver, None, async move {
            let _ = shutdown_rx.await;
        }));

        let client = reqwest::Client::new();
        let endpoint = format!("http://{address}/v1/chat/completions");

        let mut response = client
            .post(&endpoint)
            .header("Idempotency-Key", "lagging-key")
            .json(&json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "Stream it"}],
                "stream": true
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut text = String::new();
        while let Some(chunk) = response.chunk().await.unwrap() {
            let chunk_str = String::from_utf8(chunk.to_vec()).unwrap();
            text.push_str(&chunk_str);
        }

        let mut content_pieces = Vec::new();
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" || data == "keep-alive" {
                    continue;
                }
                let parsed: Value = serde_json::from_str(data).unwrap();
                if let Some(choices) = parsed.get("choices").and_then(Value::as_array) {
                    if let Some(choice) = choices.first() {
                        if let Some(content) = choice
                            .get("delta")
                            .and_then(|d| d.get("content"))
                            .and_then(Value::as_str)
                        {
                            if !content.is_empty() {
                                content_pieces.push(content.to_string());
                            }
                        }
                    }
                }
            }
        }

        let expected_pieces: Vec<String> = (0..100).map(|i| format!("chunk{} ", i)).collect();
        assert_eq!(
            content_pieces, expected_pieces,
            "Chunks were duplicated or out of order"
        );

        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
    }
}
