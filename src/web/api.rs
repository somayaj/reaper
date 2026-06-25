use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::git;
use crate::repos::{
    self, CreateRepoRequest, ImportRepoRequest, LinkRemoteRequest, import_repo, link_remote,
    push_to_remote, sync_from_remote,
};
use crate::state::AppState;
use crate::workspace;

use super::agent;

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/repos", get(list_repos).post(create_repo))
        .route("/api/repos/import", post(import_repo_handler))
        .route("/api/repos/{name}/remote/push", post(push_remote_handler))
        .route("/api/repos/{name}/remote/pull", post(pull_remote_handler))
        .route("/api/repos/{name}/remote", put(link_remote_handler))
        .route("/api/repos/{name}/agent", post(agent::run_agent))
        .route("/api/repos/{name}/branches", get(get_branches))
        .route("/api/repos/{name}/log", get(get_log))
        .route("/api/repos/{name}/git", post(run_bare_git_command))
        .route("/api/repos/{name}/workspace/open", post(open_workspace))
        .route("/api/repos/{name}/workspace/sync", post(sync_workspace))
        .route("/api/repos/{name}/workspace/tree", get(workspace_tree))
        .route(
            "/api/repos/{name}/workspace/file",
            get(read_workspace_file)
                .post(create_workspace_file)
                .put(save_workspace_file)
                .delete(delete_workspace_file),
        )
        .route("/api/repos/{name}/workspace/status", get(workspace_status))
        .route("/api/repos/{name}/workspace/diff", get(workspace_diff))
        .route("/api/repos/{name}/workspace/commit", post(workspace_commit))
        .route("/api/repos/{name}/workspace/checkout", post(workspace_checkout))
        .route("/api/repos/{name}/workspace/git", post(run_workspace_git))
        .route("/api/repos/{name}/workspace/java/info", get(java_main_info))
        .route("/api/repos/{name}/workspace/java/run", post(run_java_main_handler))
        .route("/api/repos/{name}/workspace/gradle/info", get(gradle_project_info_handler))
        .route("/api/repos/{name}/workspace/gradle/run", post(run_gradle_handler))
        .route("/api/repos/{name}/workspace/definition", get(workspace_definition))
        .route("/api/repos/{name}/workspace/format", post(workspace_format))
        .route(
            "/api/repos/{name}",
            get(get_repo).delete(delete_repo_handler),
        )
}

async fn list_repos(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match repos::list_repos(&state.config, &state.settings) {
        Ok(repos) => Json(repos).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn create_repo(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateRepoRequest>,
) -> impl IntoResponse {
    match repos::create_repo(&state.config, &state.settings, body) {
        Ok(repo) => (StatusCode::CREATED, Json(repo)).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn import_repo_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ImportRepoRequest>,
) -> impl IntoResponse {
    match import_repo(&state.config, &state.settings, body) {
        Ok(repo) => (StatusCode::CREATED, Json(repo)).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn get_repo(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match repos::get_repo(&state.config, &state.settings, &name) {
        Ok(repo) => Json(repo).into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, e),
    }
}

async fn delete_repo_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match repos::delete_repo(&state.config, &name) {
        Ok(()) => {
            let ws = state.config.workspace_path(&name);
            let _ = std::fs::remove_dir_all(ws);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => api_error(StatusCode::NOT_FOUND, e),
    }
}

async fn link_remote_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<LinkRemoteRequest>,
) -> impl IntoResponse {
    match link_remote(&state.config, &state.settings, &name, body) {
        Ok(repo) => Json(repo).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn pull_remote_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match sync_from_remote(&state.config, &state.settings, &name) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn push_remote_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match push_to_remote(&state.config, &state.settings, &name) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn get_branches(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let path = state.config.repo_path(&name);
    if !state.config.repo_exists(&name) {
        return api_error(StatusCode::NOT_FOUND, anyhow::anyhow!("not found"));
    }
    match git::list_branches(&path) {
        Ok(branches) => Json(branches).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct LogQuery {
    limit: Option<usize>,
}

async fn get_log(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<LogQuery>,
) -> impl IntoResponse {
    let path = state.config.repo_path(&name);
    if !state.config.repo_exists(&name) {
        return api_error(StatusCode::NOT_FOUND, anyhow::anyhow!("not found"));
    }
    let limit = q.limit.unwrap_or(50).min(200);
    match git::log(&path, limit) {
        Ok(commits) => Json(commits).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct GitCommandRequest {
    args: Vec<String>,
}

#[derive(Serialize)]
struct GitCommandResponse {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

async fn run_bare_git_command(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<GitCommandRequest>,
) -> impl IntoResponse {
    let path = state.config.repo_path(&name);
    if !state.config.repo_exists(&name) {
        return api_error(StatusCode::NOT_FOUND, anyhow::anyhow!("not found"));
    }
    match git::run_allowed_command(&path, &body.args) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn open_workspace(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => Json(serde_json::json!({ "path": ws.display().to_string() })).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn sync_workspace(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::sync_workspace(&ws) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn workspace_tree(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::build_tree(&ws) {
        Ok(tree) => Json(tree).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

async fn read_workspace_file(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::read_file(&ws, &q.path) {
        Ok(content) => {
            Json(serde_json::json!({ "path": q.path, "content": content })).into_response()
        }
        Err(e) => api_error(StatusCode::NOT_FOUND, e),
    }
}

#[derive(Deserialize)]
struct SaveFileRequest {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct CreateFileRequest {
    path: String,
    content: Option<String>,
}

async fn create_workspace_file(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<CreateFileRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let content = body.content.unwrap_or_default();
    match workspace::create_file(&ws, &body.path, &content) {
        Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "path": body.path }))).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn save_workspace_file(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<SaveFileRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::write_file(&ws, &body.path, &body.content) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn delete_workspace_file(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::delete_path(&ws, &q.path) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::workspace_status(&ws) {
        Ok(status) => Json(status).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct DiffQuery {
    path: Option<String>,
    staged: Option<bool>,
}

async fn workspace_diff(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DiffQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::workspace_diff(&ws, q.path.as_deref(), q.staged.unwrap_or(false)) {
        Ok(diff) => Json(serde_json::json!({ "diff": diff })).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct CommitRequest {
    message: String,
    paths: Option<Vec<String>>,
}

async fn workspace_commit(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<CommitRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::commit_and_push(&ws, &body.message, body.paths.as_deref()) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct CheckoutRequest {
    branch: String,
}

async fn workspace_checkout(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<CheckoutRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::checkout_branch(&ws, &body.branch) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn run_workspace_git(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<GitCommandRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::run_workspace_git(&ws, &body.args) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn java_main_info(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::java_main_info(&ws, &q.path) {
        Ok(info) => Json(info).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct JavaRunRequest {
    path: String,
}

async fn run_java_main_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaRunRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::run_java_main(&ws, body.path.trim()) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn gradle_project_info_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::gradle_project_info(&ws, &q.path) {
        Ok(info) => Json(info).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct GradleRunRequest {
    path: String,
    #[serde(default)]
    task: String,
}

async fn run_gradle_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<GradleRunRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let task = if body.task.trim().is_empty() {
        match workspace::gradle_project_info(&ws, body.path.trim()) {
            Ok(info) if info.is_gradle => info.default_task,
            Ok(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    anyhow::anyhow!("not inside a Gradle project"),
                )
            }
            Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
        }
    } else {
        body.task.trim().to_string()
    };
    match workspace::run_gradle(&ws, body.path.trim(), &task) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct DefinitionQuery {
    path: String,
    line: u32,
    column: u32,
}

async fn workspace_definition(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DefinitionQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::find_definition(&ws, q.path.trim(), q.line, q.column) {
        Ok(hit) => Json(hit).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct FormatRequest {
    path: String,
    content: String,
}

#[derive(Serialize)]
struct FormatResponse {
    content: String,
}

async fn workspace_format(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<FormatRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::format_file(&ws, body.path.trim(), &body.content) {
        Ok(content) => Json(FormatResponse { content }).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

fn git_response(out: git::GitOutput) -> axum::response::Response {
    Json(GitCommandResponse {
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.exit_code,
    })
    .into_response()
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
