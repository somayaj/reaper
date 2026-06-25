use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, put},
};
use serde::Deserialize;

use crate::state::AppState;

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/settings/tokens", get(list_tokens).put(set_token))
        .route("/api/settings/tokens/{host}", delete(remove_token))
        .route("/api/settings/gemini", get(get_gemini).put(set_gemini).delete(clear_gemini))
        .route("/api/settings/cursor", get(get_cursor).put(set_cursor).delete(clear_cursor))
}

async fn list_tokens(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.settings.list_tokens()).into_response()
}

#[derive(Deserialize)]
struct SetTokenRequest {
    host: String,
    token: String,
}

async fn set_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetTokenRequest>,
) -> impl IntoResponse {
    if body.host.is_empty() || body.token.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "host and token required");
    }
    match state.settings.set_token(&body.host, body.token) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn remove_token(
    State(state): State<Arc<AppState>>,
    Path(host): Path<String>,
) -> impl IntoResponse {
    match state.settings.remove_token(&host) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => api_error(StatusCode::NOT_FOUND, "token not found"),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn get_gemini(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.settings.gemini_view()).into_response()
}

#[derive(Deserialize)]
struct SetGeminiRequest {
    api_key: String,
    model: Option<String>,
}

async fn set_gemini(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetGeminiRequest>,
) -> impl IntoResponse {
    if body.api_key.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "api_key required");
    }
    if let Err(e) = state.settings.set_gemini_api_key(body.api_key) {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    if let Some(model) = body.model.filter(|m| !m.is_empty()) {
        if let Err(e) = state.settings.set_gemini_model(model) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    Json(state.settings.gemini_view()).into_response()
}

async fn clear_gemini(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let _ = state.settings.clear_gemini_api_key();
    Json(state.settings.gemini_view()).into_response()
}

async fn get_cursor(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let bridge_ok = state.cursor_bridge.health().await;
    let bridge_error = if bridge_ok {
        None
    } else {
        crate::cursor::last_bridge_error().await
    };
    Json(state.settings.cursor_view(bridge_ok, bridge_error)).into_response()
}

#[derive(Deserialize)]
struct SetCursorRequest {
    api_key: String,
    model: Option<String>,
}

async fn reset_cursor_sessions(state: &AppState) {
    for (_, session_id) in state.cursor_sessions.drain_all() {
        let _ = state.cursor_bridge.delete_session(&session_id).await;
    }
}

async fn set_cursor(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetCursorRequest>,
) -> impl IntoResponse {
    let api_key = match crate::settings::normalize_cursor_api_key(&body.api_key) {
        Ok(key) => key,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    if let Err(e) = state.settings.set_cursor_api_key(api_key) {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, e);
    }
    reset_cursor_sessions(&state).await;
    if let Some(model) = body.model.filter(|m| !m.is_empty()) {
        if let Err(e) = state.settings.set_cursor_model(model) {
            return api_error(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    let bridge_ok = state.cursor_bridge.health().await;
    let bridge_error = if bridge_ok {
        None
    } else {
        crate::cursor::last_bridge_error().await
    };
    Json(state.settings.cursor_view(bridge_ok, bridge_error)).into_response()
}

async fn clear_cursor(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let _ = state.settings.clear_cursor_api_key();
    reset_cursor_sessions(&state).await;
    let bridge_ok = state.cursor_bridge.health().await;
    let bridge_error = if bridge_ok {
        None
    } else {
        crate::cursor::last_bridge_error().await
    };
    Json(state.settings.cursor_view(bridge_ok, bridge_error)).into_response()
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
