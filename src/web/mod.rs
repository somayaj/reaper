mod api;
mod agent;
mod cursor;
mod git_http;
mod settings;

use std::sync::Arc;

use axum::Router;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(api::routes())
        .merge(cursor::routes())
        .merge(settings::routes())
        .merge(git_http::routes())
        .route_service("/", ServeFile::new("static/index.html"))
        .fallback_service(ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state))
}
