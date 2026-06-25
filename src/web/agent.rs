use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use crate::agent;
use crate::state::AppState;
use crate::workspace;

#[derive(Deserialize)]
pub struct AgentBody {
    pub prompt: String,
}

pub async fn run_agent(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<AgentBody>,
) -> impl IntoResponse {
    if body.prompt.trim().is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "prompt required");
    }

    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    match agent::run_git_agent(&state.settings, &ws, &body.prompt).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
