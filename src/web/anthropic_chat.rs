use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;

use crate::agent::{AnthropicClient, ClaudeBackend, anthropic_client_from_settings};
use crate::settings::{normalize_anthropic_model, normalize_bedrock_model};
use crate::state::AppState;
use crate::workspace;

#[derive(Deserialize)]
struct ChatBody {
    prompt: String,
    model: Option<String>,
}

struct StreamContext {
    upstream: std::pin::Pin<Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    sse_buf: String,
    assistant: String,
    finished: bool,
    sessions: Arc<crate::agent::AnthropicChatStore>,
    repo: String,
}

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route(
            "/api/repos/{name}/anthropic/chat",
            axum::routing::post(anthropic_chat),
        )
        .route(
            "/api/repos/{name}/anthropic/session",
            axum::routing::delete(clear_anthropic_session),
        )
}

async fn anthropic_chat(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ChatBody>,
) -> impl IntoResponse {
    let prompt = body.prompt.trim().to_string();
    if prompt.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "prompt required");
    }

    let mut client = match anthropic_client_from_settings(&state.settings) {
        Some(c) => c,
        None => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "Claude not configured; open Settings → Claude or set REAPER_ANTHROPIC_API_KEY / Bedrock credentials",
            );
        }
    };

    if workspace::ensure_workspace(&state.config, &name).is_err() {
        return api_error(StatusCode::BAD_REQUEST, "repository not found");
    }

    if let Some(model) = body.model.filter(|m| !m.trim().is_empty()) {
        let backend = ClaudeBackend::parse(&state.settings.anthropic_backend());
        client = match backend {
            ClaudeBackend::Api => {
                let api_key = match state.settings.anthropic_api_key() {
                    Some(k) => k,
                    None => {
                        return api_error(
                            StatusCode::BAD_REQUEST,
                            "Anthropic API key not configured",
                        );
                    }
                };
                AnthropicClient::new_api(api_key, normalize_anthropic_model(&model))
            }
            ClaudeBackend::Bedrock => AnthropicClient::new_bedrock(
                normalize_bedrock_model(&model),
                state.settings.bedrock_region(),
                state.settings.bedrock_api_key(),
            ),
        };
    }

    let history = state
        .anthropic_chat_sessions
        .history(&name)
        .into_iter()
        .map(|turn| (turn.role, turn.text))
        .collect::<Vec<_>>();

    let system = "You are Reaper's Claude coding assistant inside a local git IDE. \
        Help the user understand code, plan changes, debug issues, and write snippets. \
        Be concise and use markdown when helpful. \
        You cannot edit files or run commands in this chat mode — describe changes and show code blocks instead."
        .to_string();

    let sessions = Arc::clone(&state.anthropic_chat_sessions);
    sessions.push(&name, "user", &prompt);

    if !client.uses_streaming() {
        return match client
            .chat_with_history(&system, &history, &prompt)
            .await
        {
            Ok(text) => {
                if !text.trim().is_empty() {
                    sessions.push(&name, "assistant", &text);
                }
                let text_event = sse_event(&format!(
                    r#"{{"type":"text","text":{}}}"#,
                    serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into())
                ));
                let done = sse_event(r#"{"type":"done","status":"finished"}"#);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header(header::CACHE_CONTROL, "no-cache")
                    .body(Body::from(format!("{text_event}{done}")))
                    .unwrap_or_else(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))
            }
            Err(e) => api_error(StatusCode::BAD_REQUEST, e),
        };
    }

    let resp = match client
        .chat_stream_with_history(&system, &history, &prompt)
        .await
    {
        Ok(r) => r,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let repo = name.clone();
    let ctx = StreamContext {
        upstream: Box::pin(resp.bytes_stream()),
        sse_buf: String::new(),
        assistant: String::new(),
        finished: false,
        sessions,
        repo,
    };

    let out_stream = futures_util::stream::unfold(ctx, |mut ctx| async move {
        if ctx.finished {
            return None;
        }

        loop {
            normalize_sse_buf(&mut ctx.sse_buf);
            while let Some(line_end) = ctx.sse_buf.find('\n') {
                let line = ctx.sse_buf[..line_end].trim().to_string();
                ctx.sse_buf = ctx.sse_buf[line_end + 1..].to_string();
                if line.is_empty() {
                    continue;
                }
                if let Some(event) = process_sse_line(&line, &mut ctx.assistant) {
                    if event.is_error {
                        ctx.finished = true;
                        return Some((Ok::<_, std::io::Error>(bytes::Bytes::from(event.payload)), ctx));
                    }
                    if !event.payload.is_empty() {
                        return Some((Ok::<_, std::io::Error>(bytes::Bytes::from(event.payload)), ctx));
                    }
                }
            }

            match ctx.upstream.next().await {
                Some(Ok(chunk)) => {
                    ctx.sse_buf.push_str(&String::from_utf8_lossy(&chunk));
                }
                Some(Err(e)) => {
                    ctx.finished = true;
                    let err = sse_event(&format!(
                        r#"{{"type":"error","error":{}}}"#,
                        serde_json::to_string(&e.to_string())
                            .unwrap_or_else(|_| "\"stream error\"".into())
                    ));
                    return Some((Ok(bytes::Bytes::from(err)), ctx));
                }
                None => {
                    normalize_sse_buf(&mut ctx.sse_buf);
                    for line in ctx.sse_buf.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        if let Some(event) = process_sse_line(line, &mut ctx.assistant) {
                            if !event.is_error && !event.payload.is_empty() {
                                if !ctx.assistant.is_empty() {
                                    ctx.sessions.push(&ctx.repo, "assistant", &ctx.assistant);
                                }
                                ctx.finished = true;
                                let done = sse_event(r#"{"type":"done","status":"finished"}"#);
                                return Some((
                                    Ok(bytes::Bytes::from(format!("{}{}", event.payload, done))),
                                    ctx,
                                ));
                            }
                        }
                    }

                    if !ctx.assistant.is_empty() {
                        ctx.sessions.push(&ctx.repo, "assistant", &ctx.assistant);
                    }
                    ctx.finished = true;
                    let done = sse_event(r#"{"type":"done","status":"finished"}"#);
                    return Some((Ok(bytes::Bytes::from(done)), ctx));
                }
            }
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(out_stream))
        .unwrap_or_else(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))
}

struct SseLineResult {
    payload: String,
    is_error: bool,
}

fn normalize_sse_buf(buf: &mut String) {
    if buf.contains('\r') {
        *buf = buf.replace("\r\n", "\n").replace('\r', "\n");
    }
}

fn process_sse_line(line: &str, assistant: &mut String) -> Option<SseLineResult> {
    let payload = line.strip_prefix("data:").map(str::trim).unwrap_or(line).trim();
    if payload.is_empty() || payload == "[DONE]" {
        return None;
    }
    match AnthropicClient::parse_stream_payload(payload) {
        Ok(chunk) if chunk.is_empty() => None,
        Ok(chunk) => {
            assistant.push_str(&chunk);
            Some(SseLineResult {
                payload: sse_event(&format!(
                    r#"{{"type":"text","text":{}}}"#,
                    serde_json::to_string(&chunk).unwrap_or_else(|_| "\"\"".into())
                )),
                is_error: false,
            })
        }
        Err(msg) => Some(SseLineResult {
            payload: sse_event(&format!(
                r#"{{"type":"error","error":{}}}"#,
                serde_json::to_string(&msg).unwrap_or_else(|_| "\"anthropic error\"".into())
            )),
            is_error: true,
        }),
    }
}

fn sse_event(json: &str) -> String {
    format!("data: {json}\n\n")
}

async fn clear_anthropic_session(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    state.anthropic_chat_sessions.clear(&name);
    StatusCode::NO_CONTENT.into_response()
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
