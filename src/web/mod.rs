mod api;
mod agent;
mod anthropic_chat;
mod cursor;
mod custom_protocol;
mod gemini_chat;
mod git_http;
pub mod serve;
pub use custom_protocol::{
    loopback_ws_base, webview_init_script, GuiProtocolBridge, SharedGuiProtocolBridge, SCHEME,
    WEBVIEW_ENTRY,
};
mod settings;
mod ui_preferences;

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header::{CACHE_CONTROL, HeaderValue}};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::state::AppState;

async fn serve_index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let path = state.config.static_dir.join("index.html");
    let html = match tokio::fs::read_to_string(&path).await {
        Ok(html) => html,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("index.html: {error}"),
            )
                .into_response();
        }
    };
    Html(inject_loopback_ws_script(&html, &state.config.host, state.config.port)).into_response()
}

fn inject_loopback_ws_script(html: &str, host: &str, port: u16) -> String {
    let loopback_ws = loopback_ws_base(host, port);
    let script = format!(
        "<script>window.__REAPER_LOOPBACK_WS__={};</script>",
        serde_json::to_string(&loopback_ws).unwrap_or_else(|_| "\"\"".into())
    );
    if let Some(pos) = html.find("</head>") {
        let mut out = String::with_capacity(html.len() + script.len() + 4);
        out.push_str(&html[..pos]);
        out.push_str("  ");
        out.push_str(&script);
        out.push('\n');
        out.push_str(&html[pos..]);
        out
    } else {
        format!("{script}{html}")
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(api::routes())
        .merge(cursor::routes())
        .merge(gemini_chat::routes())
        .merge(anthropic_chat::routes())
        .merge(settings::routes())
        .merge(ui_preferences::routes())
        .merge(git_http::routes())
        .route("/", get(serve_index))
        .route("/index.html", get(serve_index))
        .fallback_service(ServeDir::new(state.config.static_dir.clone()))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate"),
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(state))
}
