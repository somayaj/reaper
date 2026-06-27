use std::sync::Arc;

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use bytes::Bytes;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::git;
use crate::repos::{
    self, CreateRepoRequest, ImportLocalRepoRequest, ImportRepoRequest, LinkRemoteRequest,
    PublishToGitHubRequest, import_local_repo, import_repo, link_remote, publish_to_github,
    push_preview, push_to_remote, sync_from_remote,
};
use crate::agent as git_agent;
use crate::state::AppState;
use crate::workspace;

use super::agent;

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/repos", get(list_repos).post(create_repo))
        .route("/api/repos/import", post(import_repo_handler))
        .route("/api/repos/import/local", post(import_local_repo_handler))
        .route("/api/repos/{name}/remote/push/preview", get(push_preview_handler))
        .route("/api/repos/{name}/remote/push", post(push_remote_handler))
        .route("/api/repos/{name}/remote/pull", post(pull_remote_handler))
        .route("/api/repos/{name}/remote/publish", post(publish_github_handler))
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
        .route("/api/repos/{name}/workspace/conflict", get(conflict_stages_handler))
        .route("/api/repos/{name}/workspace/conflict/resolve", post(conflict_resolve_handler))
        .route("/api/repos/{name}/workspace/conflict/continue", post(conflict_continue_handler))
        .route("/api/repos/{name}/workspace/commit/{hash}/diff", get(commit_diff_handler))
        .route("/api/repos/{name}/workspace/commit/suggest", post(suggest_commit_message_handler))
        .route("/api/repos/{name}/workspace/commit", post(workspace_commit))
        .route("/api/repos/{name}/workspace/checkout", post(workspace_checkout))
        .route("/api/repos/{name}/workspace/git", post(run_workspace_git))
        .route("/api/repos/{name}/workspace/shell", post(run_workspace_shell))
        .route("/api/repos/{name}/workspace/shell/cd", post(workspace_shell_cd))
        .route("/api/repos/{name}/workspace/java/info", get(java_main_info))
        .route("/api/repos/{name}/workspace/java/run", post(run_java_main_handler))
        .route("/api/repos/{name}/workspace/java/test-methods", get(java_test_methods_handler).post(java_test_methods_post))
        .route("/api/repos/{name}/workspace/gradle/info", get(gradle_project_info_handler))
        .route("/api/repos/{name}/workspace/gradle/run", post(run_gradle_handler))
        .route("/api/repos/{name}/workspace/definition", get(workspace_definition).post(workspace_definition_post))
        .route("/api/repos/{name}/workspace/classes", get(workspace_classes))
        .route("/api/repos/{name}/workspace/completions", get(workspace_completions))
        .route("/api/repos/{name}/workspace/java/index-status", get(java_index_status))
        .route("/api/repos/{name}/workspace/project/index-status", get(project_index_status))
        .route("/api/repos/{name}/workspace/diagnostics", post(workspace_diagnostics))
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

async fn import_local_repo_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ImportLocalRepoRequest>,
) -> impl IntoResponse {
    match import_local_repo(&state.config, &state.settings, body) {
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

async fn push_preview_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match push_preview(&state.config, &state.settings, &name) {
        Ok(preview) => Json(preview).into_response(),
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

async fn publish_github_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<PublishToGitHubRequest>,
) -> impl IntoResponse {
    match publish_to_github(&state.config, &state.settings, &name, body).await {
        Ok(result) => Json(result).into_response(),
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
        Ok(ws) => {
            let profile = workspace::detect_project_profile(&ws).unwrap_or_default();
            state.project_index_jobs.on_open(&name, &ws);
            Json(serde_json::json!({
                "path": ws.display().to_string(),
                "profile": profile,
                "indexing": !profile.indexers.is_empty(),
            }))
            .into_response()
        }
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
    Query(q): Query<TreeQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let result = if q.recursive.unwrap_or(false) {
        workspace::build_tree(&ws)
    } else {
        workspace::build_tree_level(&ws, q.dir.as_deref())
    };
    match result {
        Ok(tree) => Json(tree).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct TreeQuery {
    dir: Option<String>,
    recursive: Option<bool>,
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

async fn commit_diff_handler(
    State(state): State<Arc<AppState>>,
    Path((name, hash)): Path<(String, String)>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::commit_diff(&ws, hash.trim()) {
        Ok(diff) => Json(serde_json::json!({ "diff": diff })).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[derive(Deserialize)]
struct ConflictPathQuery {
    path: String,
}

async fn conflict_stages_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<ConflictPathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::conflict_stages(&ws, q.path.trim()) {
        Ok(stages) => Json(stages).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct ConflictResolveRequest {
    path: String,
}

async fn conflict_resolve_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ConflictResolveRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::mark_conflict_resolved(&ws, body.path.trim()) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn conflict_continue_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let merge = workspace::conflict::merge_state(&ws);
    let result = if merge.kind.as_deref() == Some("rebase") {
        git::run_git(Some(&ws), &["rebase", "--continue"])
    } else if merge.kind.as_deref() == Some("cherry-pick") {
        git::run_git(Some(&ws), &["cherry-pick", "--continue"])
    } else {
        git::run_git(Some(&ws), &["commit", "--no-edit"])
    };
    match result {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct CommitRequest {
    message: String,
    paths: Option<Vec<String>>,
    #[serde(default = "default_true")]
    push: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
struct SuggestCommitResponse {
    message: String,
}

async fn suggest_commit_message_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match git_agent::suggest_commit_message(&state.settings, &ws).await {
        Ok(message) => Json(SuggestCommitResponse { message }).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
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
    match workspace::commit_changes(&ws, &body.message, body.paths.as_deref(), body.push) {
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
        Ok(out) => {
            if out.success() {
                state.project_index_jobs.on_checkout(&name, &ws);
            }
            git_response(out)
        }
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

#[derive(Deserialize)]
struct ShellRequest {
    command: String,
    cwd: Option<String>,
}

#[derive(Deserialize)]
struct ShellCdRequest {
    target: String,
    cwd: Option<String>,
}

#[derive(Serialize)]
struct ShellCdResponse {
    cwd: String,
}

async fn run_workspace_shell(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ShellRequest>,
) -> Response {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let command = body.command.trim().to_string();
    if command.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "command required");
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<workspace::ExecStreamEvent>(256);
    let cwd = body.cwd.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = workspace::stream_workspace_shell(&ws, cwd.as_deref(), &command, tx.clone()) {
            let _ = tx.blocking_send(workspace::ExecStreamEvent {
                t: "error".into(),
                text: Some(format!("{e:#}\n")),
                code: Some(-1),
                step: None,
            });
        }
    });

    exec_stream_response(rx)
}

async fn workspace_shell_cd(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<ShellCdRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::change_workspace_directory(&ws, body.cwd.as_deref(), &body.target) {
        Ok(cwd) => Json(ShellCdResponse { cwd }).into_response(),
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
) -> Response {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let path = body.path.trim().to_string();
    let (tx, rx) = tokio::sync::mpsc::channel::<workspace::ExecStreamEvent>(256);
    tokio::task::spawn_blocking(move || {
        if let Err(e) = workspace::stream_workspace_java_main(&ws, &path, tx.clone()) {
            let _ = tx.blocking_send(workspace::ExecStreamEvent {
                t: "error".into(),
                text: Some(format!("{e:#}\n")),
                code: Some(-1),
                step: None,
            });
        }
    });

    exec_stream_response(rx)
}

#[derive(Deserialize)]
struct JavaContentBody {
    path: String,
    #[serde(default)]
    content: Option<String>,
}

async fn java_test_methods_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = q.path.trim();
    let content = match workspace::read_file(&ws, path) {
        Ok(c) => c,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::java_test_methods(&ws, path, &content) {
        Ok(methods) => Json(methods).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn java_test_methods_post(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaContentBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = body.path.trim();
    let content = if let Some(c) = body.content {
        c
    } else {
        match workspace::read_file(&ws, path) {
            Ok(c) => c,
            Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
        }
    };
    match workspace::java_test_methods(&ws, path, &content) {
        Ok(methods) => Json(methods).into_response(),
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
) -> Response {
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

    let path = body.path.trim().to_string();
    let (tx, rx) = tokio::sync::mpsc::channel::<workspace::ExecStreamEvent>(256);
    tokio::task::spawn_blocking(move || {
        if let Err(e) = workspace::stream_workspace_gradle(&ws, &path, &task, tx.clone()) {
            let _ = tx.blocking_send(workspace::ExecStreamEvent {
                t: "error".into(),
                text: Some(format!("{e:#}\n")),
                code: Some(-1),
                step: None,
            });
        }
    });

    exec_stream_response(rx)
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
    let from_path = q.path.trim();
    if workspace::definition_uses_java_index(from_path) {
        state.java_index_jobs.ensure_building(&name, &ws);
    }
    match workspace::find_definition_with_content(&ws, from_path, q.line, q.column, None) {
        Ok(hit) => Json(hit).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct DefinitionBody {
    path: String,
    line: u32,
    column: u32,
    #[serde(default)]
    content: Option<String>,
}

async fn workspace_definition_post(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<DefinitionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let from_path = body.path.trim();
    if workspace::definition_uses_java_index(from_path) {
        state.java_index_jobs.ensure_building(&name, &ws);
    }
    match workspace::find_definition_with_content(
        &ws,
        from_path,
        body.line,
        body.column,
        body.content.as_deref(),
    ) {
        Ok(hit) => Json(hit).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct ClassSearchQuery {
    q: Option<String>,
    limit: Option<usize>,
}

async fn workspace_classes(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<ClassSearchQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    if workspace::is_gradle_workspace(&ws) {
        state.java_index_jobs.ensure_building(&name, &ws);
    }
    let query = q.q.unwrap_or_default();
    let limit = q.limit.unwrap_or(50);
    match workspace::search_classes(&ws, &query, limit) {
        Ok(hits) => Json(hits).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct CompletionQuery {
    path: String,
    line: u32,
    column: u32,
    prefix: Option<String>,
}

async fn workspace_completions(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<CompletionQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let prefix = q.prefix.unwrap_or_default();
    state.java_index_jobs.ensure_building(&name, &ws);
    match workspace::java_completions(&ws, q.path.trim(), q.line, q.column, &prefix) {
        Ok(items) => Json(items).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct DiagnosticsRequest {
    path: String,
    content: String,
}

async fn java_index_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(state.java_index_jobs.status(&name)).into_response()
}

async fn project_index_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(state.project_index_jobs.status(&name)).into_response()
}

async fn workspace_diagnostics(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<DiagnosticsRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = body.path.trim().to_string();
    let content = body.content;
    match tokio::task::spawn_blocking(move || workspace::file_diagnostics(&ws, &path, &content)).await {
        Ok(Ok(items)) => Json(items).into_response(),
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("diagnostics task failed: {e:#}")),
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

fn exec_stream_response(rx: tokio::sync::mpsc::Receiver<workspace::ExecStreamEvent>) -> Response {
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Some(event) => {
                let payload = match serde_json::to_string(&event) {
                    Ok(json) => format!("data: {json}\n\n"),
                    Err(e) => format!(
                        "data: {{\"t\":\"error\",\"text\":{},\"code\":-1}}\n\n",
                        serde_json::to_string(&format!("stream encode failed: {e}"))
                            .unwrap_or_else(|_| "\"error\"".into())
                    ),
                };
                Some((Ok::<Bytes, std::io::Error>(Bytes::from(payload)), rx))
            }
            None => None,
        }
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(Body::from_stream(stream))
        .unwrap_or_else(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
