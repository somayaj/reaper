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

use crate::cursor::{self, ensure_bridge_running, last_bridge_error};
use crate::state::AppState;
use crate::workspace;

#[derive(Deserialize)]
pub struct ChatBody {
    pub prompt: String,
    pub model: Option<String>,
    pub mode: Option<String>,
}

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/cursor/status", axum::routing::get(cursor_status))
        .route("/api/cursor/models", axum::routing::get(cursor_models))
        .route("/api/cursor/bridge/restart", axum::routing::post(restart_bridge))
        .route("/api/repos/{name}/cursor/chat", axum::routing::post(cursor_chat))
        .route(
            "/api/repos/{name}/cursor/session",
            axum::routing::delete(clear_cursor_session),
        )
}

async fn cursor_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(cursor_status_json(&state).await).into_response()
}

async fn restart_bridge(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    cursor::stop_bridge().await;
    cursor::reclaim_bridge_port().await;
    if let Err(e) = ensure_bridge_running().await {
        tracing::warn!("Cursor bridge restart failed: {e:#}");
    }
    Json(cursor_status_json(&state).await).into_response()
}

async fn cursor_status_json(state: &AppState) -> serde_json::Value {
    let bridge_ok = state.cursor_bridge.health().await;
    let bridge_error = if bridge_ok {
        None
    } else {
        last_bridge_error().await
    };
    serde_json::to_value(state.settings.cursor_view(bridge_ok, bridge_error)).unwrap_or_default()
}

async fn cursor_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let api_key = match state.settings.cursor_api_key() {
        Some(k) => k,
        None => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "Cursor API key not configured",
            );
        }
    };

    if !state.cursor_bridge.health().await {
        if let Err(e) = ensure_bridge_running().await {
            tracing::warn!("Cursor bridge auto-retry failed: {e:#}");
        }
    }
    if !state.cursor_bridge.health().await {
        let detail = last_bridge_error().await.unwrap_or_else(|| {
            "Bridge offline — click Retry in Settings or restart Reaper".into()
        });
        return api_error(StatusCode::SERVICE_UNAVAILABLE, detail);
    }

    match state.cursor_bridge.list_models(&api_key).await {
        Ok(models) => Json(models).into_response(),
        Err(e) => {
            let msg = e.to_string();
            if let Some(hint) = crate::settings::cursor_auth_error(&msg) {
                return api_error(StatusCode::UNAUTHORIZED, hint);
            }
            api_error(StatusCode::BAD_REQUEST, msg)
        }
    }
}

async fn cursor_chat(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ChatBody>,
) -> impl IntoResponse {
    let prompt = body.prompt.trim();
    if prompt.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "prompt required");
    }

    let api_key = match state.settings.cursor_api_key() {
        Some(k) => k,
        None => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "Cursor API key not configured; open Settings → Cursor agent or set REAPER_CURSOR_API_KEY",
            );
        }
    };

    if !state.cursor_bridge.health().await {
        if let Err(e) = ensure_bridge_running().await {
            tracing::warn!("Cursor bridge auto-retry failed: {e:#}");
        }
    }
    if !state.cursor_bridge.health().await {
        let detail = last_bridge_error().await.unwrap_or_else(|| {
            "Bridge offline — click Retry in Settings or restart Reaper".into()
        });
        return api_error(StatusCode::SERVICE_UNAVAILABLE, detail);
    }

    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let cwd = match ws.canonicalize() {
        Ok(p) => p,
        Err(e) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    };

    let model = body
        .model
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| state.settings.cursor_model());
    let mode = body
        .mode
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| state.settings.cursor_mode());

    let session_id = match state.cursor_sessions.get(&name) {
        Some(id) => id,
        None => {
            match state
                .cursor_bridge
                .create_session(
                    &cwd.display().to_string(),
                    &api_key,
                    &model,
                    &mode,
                )
                .await
            {
                Ok(id) => {
                    state.cursor_sessions.set(&name, id.clone());
                    id
                }
                Err(e) => {
                    let msg = e.to_string();
                    if let Some(hint) = crate::settings::cursor_auth_error(&msg) {
                        state.cursor_sessions.remove(&name);
                        return api_error(StatusCode::UNAUTHORIZED, hint);
                    }
                    return api_error(StatusCode::BAD_REQUEST, msg);
                }
            }
        }
    };

    let resp = match state
        .cursor_bridge
        .chat_stream(&session_id, prompt, Some(model.as_str()), Some(mode.as_str()))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            state.cursor_sessions.remove(&name);
            let msg = e.to_string();
            if let Some(hint) = crate::settings::cursor_auth_error(&msg) {
                return api_error(StatusCode::UNAUTHORIZED, hint);
            }
            return api_error(StatusCode::BAD_REQUEST, msg);
        }
    };

    let stream = resp.bytes_stream().map(|chunk| {
        chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn clear_cursor_session(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Some(session_id) = state.cursor_sessions.remove(&name) {
        let _ = state.cursor_bridge.delete_session(&session_id).await;
    }
    StatusCode::NO_CONTENT.into_response()
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
