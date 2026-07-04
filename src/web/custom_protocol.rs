//! In-process dispatch from wry `reaper://` requests to the same axum router as loopback HTTP.
//!
//! GUI WebView traffic uses the custom protocol so saves and javac POSTs do not compete for
//! WebKit's ~6 HTTP/1.1 connections. Terminal WebSocket and external git stay on loopback TCP.

use std::sync::Arc;

use anyhow::Context;
use axum::body::Body;
use axum::http::{Request, Response, Uri};
use axum::Router;
use http_body_util::BodyExt;
use tokio::runtime::Handle;
use tower::ServiceExt;

pub const SCHEME: &str = "reaper";
pub const WEBVIEW_ENTRY: &str = "reaper://localhost/";

pub struct GuiProtocolBridge {
    router: Router,
    handle: Handle,
}

impl GuiProtocolBridge {
    pub fn new(router: Router, handle: Handle) -> Self {
        Self { router, handle }
    }

    pub fn dispatch_sync(&self, request: wry::http::Request<Vec<u8>>) -> wry::http::Response<Vec<u8>> {
        match self.handle.block_on(self.dispatch(request)) {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("custom protocol dispatch failed: {error:#}");
                wry::http::Response::builder()
                    .status(500)
                    .header(wry::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                    .body(error.to_string().into_bytes())
                    .unwrap_or_else(|_| {
                        wry::http::Response::builder()
                            .status(500)
                            .body(Vec::new())
                            .expect("empty 500 response")
                    })
            }
        }
    }

    async fn dispatch(
        &self,
        request: wry::http::Request<Vec<u8>>,
    ) -> anyhow::Result<wry::http::Response<Vec<u8>>> {
        let axum_request = wry_to_axum_request(request)?;
        let mut router = self.router.clone();
        let axum_response = router
            .oneshot(axum_request)
            .await
            .context("router oneshot")?;
        wry_from_axum_response(axum_response).await
    }
}

fn wry_to_axum_request(request: wry::http::Request<Vec<u8>>) -> anyhow::Result<Request<Body>> {
    let (parts, body) = request.into_parts();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let uri: Uri = format!("http://127.0.0.1{path_and_query}")
        .parse()
        .context("build loopback URI for router")?;

    let mut builder = Request::builder().method(parts.method).uri(uri);
    for (name, value) in parts.headers.iter() {
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from(body))
        .context("build axum request body")
}

async fn wry_from_axum_response(
    response: Response<Body>,
) -> anyhow::Result<wry::http::Response<Vec<u8>>> {
    let (parts, body) = response.into_parts();
    let bytes = body
        .collect()
        .await
        .context("read axum response body")?
        .to_bytes();

    let mut builder = wry::http::Response::builder().status(parts.status);
    for (name, value) in parts.headers.iter() {
        builder = builder.header(name, value);
    }
    builder
        .body(bytes.to_vec())
        .context("build wry response body")
}

pub fn loopback_ws_base(host: &str, port: u16) -> String {
    format!("ws://{host}:{port}")
}

pub fn webview_init_script(loopback_ws: &str) -> String {
    format!(
        "window.__REAPER_LOOPBACK_WS__={loopback_ws:?};",
        loopback_ws = loopback_ws
    )
}

pub type SharedGuiProtocolBridge = Arc<GuiProtocolBridge>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Json;

    #[tokio::test]
    async fn custom_protocol_dispatches_to_router() {
        let router = Router::new().route(
            "/api/version",
            get(|| async { Json(serde_json::json!({ "ok": true })) }),
        );
        let bridge = GuiProtocolBridge::new(router, Handle::current());

        let request = wry::http::Request::builder()
            .uri("reaper://localhost/api/version")
            .body(Vec::new())
            .unwrap();
        let response = bridge.dispatch(request).await.unwrap();
        assert!(response.status().is_success());
        let body = String::from_utf8(response.body().clone()).unwrap();
        assert!(body.contains("\"ok\":true"));
    }
}
