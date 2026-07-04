mod api;
mod agent;
mod cursor;
mod gemini_chat;
mod git_http;
mod settings;
mod ui_preferences;

use std::sync::Arc;

use axum::Router;
use axum::http::header::{CACHE_CONTROL, HeaderValue};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(api::routes())
        .merge(cursor::routes())
        .merge(gemini_chat::routes())
        .merge(settings::routes())
        .merge(ui_preferences::routes())
        .merge(git_http::routes())
        .route_service(
            "/",
            ServeFile::new(state.config.static_dir.join("index.html")),
        )
        .fallback_service(ServeDir::new(state.config.static_dir.clone()))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state))
}
