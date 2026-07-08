use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;

use crate::cursor::{self, ensure_bridge_running, invalidate_health_cache, last_bridge_error};
use crate::state::AppState;
use crate::workspace;

const BRIDGE_HEALTH_TTL: Duration = Duration::from_secs(8);

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
            "/api/repos/{name}/cursor/session/warm",
            axum::routing::post(warm_cursor_session),
        )
        .route("/api/repos/{name}/cursor/stop", axum::routing::post(cursor_stop))
        .route(
            "/api/repos/{name}/cursor/session",
            axum::routing::delete(clear_cursor_session),
        )
}

async fn cursor_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(cursor_status_json(&state).await).into_response()
}

async fn restart_bridge(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    for (_, session_id) in state.cursor_sessions.drain_all() {
        let _ = state.cursor_bridge.delete_session(&session_id).await;
    }
    cursor::stop_bridge().await;
    cursor::reclaim_bridge_port().await;
    invalidate_health_cache();
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

async fn ensure_bridge_ready(state: &AppState) -> Result<(), Response> {
    let was_healthy = state.cursor_bridge.health_cached(BRIDGE_HEALTH_TTL).await;
    if !was_healthy {
        if let Err(e) = ensure_bridge_running().await {
            tracing::warn!("Cursor bridge auto-retry failed: {e:#}");
        }
        if !was_healthy && state.cursor_bridge.health_cached(Duration::from_secs(1)).await {
            // Bridge process restarted — in-memory sessions are gone.
            let _ = state.cursor_sessions.drain_all();
        }
    }
    if !state.cursor_bridge.health_cached(BRIDGE_HEALTH_TTL).await {
        let detail = last_bridge_error().await.unwrap_or_else(|| {
            "Bridge offline — click Retry in Settings or restart Reaper".into()
        });
        return Err(api_error(StatusCode::SERVICE_UNAVAILABLE, detail));
    }
    Ok(())
}

fn cursor_api_key(state: &AppState) -> Result<String, Response> {
    state.settings.cursor_api_key().ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "Cursor API key not configured; open Settings → Cursor agent or set REAPER_CURSOR_API_KEY",
        )
    })
}

async fn workspace_cwd(state: &AppState, name: &str) -> Result<PathBuf, Response> {
    let ws = workspace::ensure_workspace(&state.config, name).map_err(|e| {
        api_error(StatusCode::BAD_REQUEST, e)
    })?;
    ws.canonicalize()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn ensure_cursor_session(
    state: &AppState,
    name: &str,
    cwd: &PathBuf,
    api_key: &str,
    model: &str,
    mode: &str,
) -> Result<String, Response> {
    if let Some(id) = state.cursor_sessions.get(name) {
        return Ok(id);
    }

    match state
        .cursor_bridge
        .create_session(
            &cwd.display().to_string(),
            api_key,
            model,
            mode,
        )
        .await
    {
        Ok(id) => {
            state.cursor_sessions.set(name, id.clone());
            Ok(id)
        }
        Err(e) => {
            let msg = e.to_string();
            if let Some(hint) = crate::settings::cursor_agent_error(&msg) {
                state.cursor_sessions.remove(name);
                let status = if crate::settings::cursor_auth_error(&msg).is_some() {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::BAD_REQUEST
                };
                Err(api_error(status, hint))
            } else {
                Err(api_error(StatusCode::BAD_REQUEST, msg))
            }
        }
    }
}

async fn validate_cursor_model(
    state: &AppState,
    api_key: &str,
    model: &str,
) -> Result<(), Response> {
    state
        .cursor_bridge
        .validate_model(api_key, model)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            api_error(
                StatusCode::BAD_REQUEST,
                crate::settings::cursor_model_error(&msg).unwrap_or(msg),
            )
        })
}

fn cursor_model_ids(value: &serde_json::Value) -> Vec<String> {
    value
        .get("models")
        .and_then(|m| m.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| entry.get("id").and_then(|id| id.as_str()))
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn reconcile_cursor_model_setting(state: &AppState, model_ids: &[String]) {
    if model_ids.is_empty() {
        return;
    }
    let saved = state.settings.cursor_model();
    if model_ids.iter().any(|id| id == &saved) {
        return;
    }
    if let Err(e) = state.settings.set_cursor_model(model_ids[0].clone()) {
        tracing::warn!("failed to reconcile cursor model: {e:#}");
    }
}

async fn cursor_models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let api_key = match cursor_api_key(&state) {
        Ok(k) => k,
        Err(resp) => return resp,
    };

    if let Err(resp) = ensure_bridge_ready(&state).await {
        return resp;
    }

    match state.cursor_bridge.list_models(&api_key).await {
        Ok(mut models) => {
            let ids = cursor_model_ids(&models);
            reconcile_cursor_model_setting(&state, &ids);
            if let Some(obj) = models.as_object_mut() {
                obj.insert(
                    "current_model".into(),
                    serde_json::Value::String(state.settings.cursor_model()),
                );
            }
            Json(models).into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            if let Some(hint) = crate::settings::cursor_agent_error(&msg) {
                let status = if crate::settings::cursor_auth_error(&msg).is_some() {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::BAD_REQUEST
                };
                return api_error(status, hint);
            }
            api_error(StatusCode::BAD_REQUEST, msg)
        }
    }
}

async fn warm_cursor_session(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let api_key = match cursor_api_key(&state) {
        Ok(k) => k,
        Err(resp) => return resp,
    };

    if let Err(resp) = ensure_bridge_ready(&state).await {
        return resp;
    }

    let cwd = match workspace_cwd(&state, &name).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let model = state.settings.cursor_model();
    let mode = state.settings.cursor_mode();

    if let Err(resp) = validate_cursor_model(&state, &api_key, &model).await {
        return resp;
    }

    match ensure_cursor_session(&state, &name, &cwd, &api_key, &model, &mode).await {
        Ok(session_id) => Json(serde_json::json!({
            "ok": true,
            "warmed": true,
            "session_id": session_id,
        }))
        .into_response(),
        Err(resp) => resp,
    }
}

fn cursor_session_stale(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("session not found") || lower.contains("no active run")
}

async fn cursor_chat_stream_with_retry(
    state: &AppState,
    name: &str,
    cwd: &PathBuf,
    api_key: &str,
    model: &str,
    mode: &str,
    prompt: &str,
) -> Result<reqwest::Response, Response> {
    for attempt in 0..2 {
        if attempt > 0 {
            if let Some(stale_id) = state.cursor_sessions.remove(name) {
                let _ = state.cursor_bridge.delete_session(&stale_id).await;
            }
        }

        let session_id =
            match ensure_cursor_session(state, name, cwd, api_key, model, mode).await {
                Ok(id) => id,
                Err(resp) => return Err(resp),
            };

        match state
            .cursor_bridge
            .chat_stream(&session_id, prompt, Some(model), Some(mode))
            .await
        {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                let msg = e.to_string();
                if attempt == 0 && cursor_session_stale(&msg) {
                    tracing::debug!("cursor chat stale session for {name}, recreating");
                    continue;
                }
                state.cursor_sessions.remove(name);
                if let Some(hint) = crate::settings::cursor_agent_error(&msg) {
                    let status = if crate::settings::cursor_auth_error(&msg).is_some() {
                        StatusCode::UNAUTHORIZED
                    } else {
                        StatusCode::BAD_REQUEST
                    };
                    return Err(api_error(status, hint));
                }
                return Err(api_error(StatusCode::BAD_REQUEST, msg));
            }
        }
    }

    Err(api_error(
        StatusCode::BAD_GATEWAY,
        "Cursor session could not be established — try again",
    ))
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

    let api_key = match cursor_api_key(&state) {
        Ok(k) => k,
        Err(resp) => return resp,
    };

    if let Err(resp) = ensure_bridge_ready(&state).await {
        return resp;
    }

    let cwd = match workspace_cwd(&state, &name).await {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    let model = body
        .model
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| state.settings.cursor_model());
    let mode = body
        .mode
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| state.settings.cursor_mode());

    if let Err(resp) = validate_cursor_model(&state, &api_key, &model).await {
        return resp;
    }

    let resp = match cursor_chat_stream_with_retry(
        &state,
        &name,
        &cwd,
        &api_key,
        model.as_str(),
        mode.as_str(),
        prompt,
    )
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
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

async fn cursor_stop(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(session_id) = state.cursor_sessions.get(&name) else {
        return StatusCode::NO_CONTENT.into_response();
    };

    match state.cursor_bridge.stop_chat(&session_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
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
