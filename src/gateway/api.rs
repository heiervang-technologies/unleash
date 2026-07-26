use super::history::{GatewayUsage, InstanceIdentity, TurnUpdate};
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
use std::convert::Infallible;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;

const MAX_REQUEST_BYTES: usize = 1024 * 1024;
static RESPONSE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct AppState {
    identity: Arc<RwLock<InstanceIdentity>>,
    driver: Driver,
    turn_gate: Arc<Mutex<()>>,
    api_key: Option<Arc<str>>,
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
    let current_model = state.identity.read().expect("identity poisoned").model_id();
    if request.model != current_model {
        return ApiError::new(
            StatusCode::NOT_FOUND,
            format!(
                "model '{}' is not a live instance; use GET /v1/models",
                request.model
            ),
            Some("model_not_found"),
        )
        .into_response();
    }
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

    let gate = match Arc::clone(&state.turn_gate).try_lock_owned() {
        Ok(gate) => gate,
        Err(_) => {
            return ApiError::new(
                StatusCode::CONFLICT,
                "this stateful agent instance is already processing a turn",
                Some("instance_busy"),
            )
            .into_response()
        }
    };
    let response_id = next_response_id();
    let receiver = state.driver.start_turn(prompt, gate);

    if request.stream.unwrap_or(false) {
        streaming_response(
            receiver,
            response_id,
            current_model,
            request
                .stream_options
                .as_ref()
                .is_some_and(|options| options.include_usage),
        )
    } else {
        non_streaming_response(receiver, response_id, current_model).await
    }
}

fn streaming_response(
    mut receiver: mpsc::Receiver<TurnStreamEvent>,
    response_id: String,
    initial_model: String,
    include_usage: bool,
) -> Response {
    let (sender, output) = mpsc::channel::<Result<Event, Infallible>>(64);
    tokio::spawn(async move {
        let created = unix_timestamp();
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

        while let Some(event) = receiver.recv().await {
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
    mut receiver: mpsc::Receiver<TurnStreamEvent>,
    response_id: String,
    initial_model: String,
) -> Response {
    while let Some(event) = receiver.recv().await {
        match event {
            TurnStreamEvent::Update(_) => {}
            TurnStreamEvent::Complete(result) => {
                return Json(json!({
                    "id": response_id,
                    "object": "chat.completion",
                    "created": unix_timestamp(),
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
    let message = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .ok_or_else(|| {
            ApiError::bad_request(
                "messages must contain a user message",
                Some("invalid_messages"),
            )
        })?;
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
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::history::{GatewayUsage, TurnResult};

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

        let _ = shutdown_tx.send(());
        server.await.unwrap().unwrap();
    }
}
