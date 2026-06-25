use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::any,
};
use serde::Deserialize;

use crate::git::{self, GitService};
use crate::state::AppState;

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new().route("/git/{*git_path}", any(git_http))
}

#[derive(Deserialize, Default)]
struct GitQuery {
    #[serde(default)]
    service: Option<String>,
}

enum GitEndpoint {
    InfoRefs,
    UploadPack,
    ReceivePack,
}

fn parse_git_path(git_path: &str) -> Option<(String, GitEndpoint)> {
    let (prefix, endpoint) = if let Some(p) = git_path.strip_suffix("/info/refs") {
        (p, GitEndpoint::InfoRefs)
    } else if let Some(p) = git_path.strip_suffix("/git-upload-pack") {
        (p, GitEndpoint::UploadPack)
    } else if let Some(p) = git_path.strip_suffix("/git-receive-pack") {
        (p, GitEndpoint::ReceivePack)
    } else {
        return None;
    };
    let name = prefix.strip_suffix(".git")?.to_string();
    Some((name, endpoint))
}

async fn git_http(
    State(state): State<Arc<AppState>>,
    Path(git_path): Path<String>,
    Query(q): Query<GitQuery>,
    method: axum::http::Method,
    body: Bytes,
) -> impl IntoResponse {
    let Some((name, endpoint)) = parse_git_path(&git_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let repo = state.config.repo_path(&name);
    if !state.config.repo_exists(&name) {
        return StatusCode::NOT_FOUND.into_response();
    }

    match endpoint {
        GitEndpoint::InfoRefs => {
            if method != axum::http::Method::GET {
                return StatusCode::METHOD_NOT_ALLOWED.into_response();
            }
            let Some(service) = q.service.as_deref().and_then(GitService::from_query) else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            match git::advertise_refs(&repo, service).await {
                Ok(resp) => resp.into_response(),
                Err(e) => {
                    tracing::error!("info/refs failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        GitEndpoint::UploadPack => {
            if method != axum::http::Method::POST {
                return StatusCode::METHOD_NOT_ALLOWED.into_response();
            }
            git_rpc(&repo, GitService::UploadPack, body).await
        }
        GitEndpoint::ReceivePack => {
            if method != axum::http::Method::POST {
                return StatusCode::METHOD_NOT_ALLOWED.into_response();
            }
            git_rpc(&repo, GitService::ReceivePack, body).await
        }
    }
}

async fn git_rpc(repo: &std::path::Path, service: GitService, body: Bytes) -> axum::response::Response {
    match git::rpc(repo, service, body).await {
        Ok(resp) => resp.into_response(),
        Err(e) => {
            tracing::error!("git rpc failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
