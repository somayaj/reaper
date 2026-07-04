use std::sync::Arc;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch},
};
use serde::Deserialize;

use crate::state::AppState;

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route(
        "/api/ui-preferences",
        get(get_ui_preferences).patch(set_ui_preferences),
    )
}

async fn get_ui_preferences(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.ui_preferences.view()).into_response()
}

#[derive(Deserialize)]
struct SetUiPreferencesRequest {
    coverage_inline_enabled: Option<bool>,
}

async fn set_ui_preferences(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetUiPreferencesRequest>,
) -> impl IntoResponse {
    if let Some(enabled) = body.coverage_inline_enabled {
        match state.ui_preferences.set_coverage_inline_enabled(enabled) {
            Ok(prefs) => return Json(prefs).into_response(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        }
    }
    Json(state.ui_preferences.view()).into_response()
}
