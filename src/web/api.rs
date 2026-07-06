use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::{
    Json,
    body::Body,
    extract::{
        Path, Query, State,
        ws::WebSocketUpgrade,
    },
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::git;
use crate::repos::{
    self, CreateRepoRequest, ImportLocalRepoRequest, ImportRepoRequest, LinkRemoteRequest,
    PublishToGitHubRequest, import_local_repo, import_repo, link_remote, metadata, publish_to_github,
    push_preview, push_to_remote, sync_from_remote,
};
use crate::agent as git_agent;
use crate::state::AppState;
use crate::workspace;
use tokio::sync::oneshot;
use tokio::task::AbortHandle;
use tokio::time::timeout;

use super::agent;

static JAVA_FULL_DIAG_SEM: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
const JAVA_DIAG_API_TIMEOUT: Duration = Duration::from_secs(45);

/// When the HTTP client disconnects (save abort, tab close), abort the diag job and kill javac.
struct DiagRequestGuard {
    ws: PathBuf,
    rel_path: String,
    job_abort: Option<AbortHandle>,
    cancel_on_drop: bool,
}

impl DiagRequestGuard {
    fn disable(&mut self) {
        self.cancel_on_drop = false;
        self.job_abort = None;
    }
}

impl Drop for DiagRequestGuard {
    fn drop(&mut self) {
        if self.cancel_on_drop {
            if let Some(abort) = self.job_abort.take() {
                abort.abort();
            }
            workspace::cancel_inflight_diagnostics(&self.ws, &self.rel_path);
        }
    }
}

fn diagnostics_http_response(body: impl IntoResponse) -> Response {
    let mut res = body.into_response();
    res.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    res
}

fn cancelled_diagnostics_response() -> Response {
    diagnostics_http_response(Json(workspace::FileDiagnosticsResult::cancelled()))
}

pub fn routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/api/version", get(app_version))
        .route("/api/repos", get(list_repos).post(create_repo))
        .route("/api/repos/hidden", get(list_hidden_repos_handler))
        .route("/api/repos/import", post(import_repo_handler))
        .route("/api/repos/import/local", post(import_local_repo_handler))
        .route("/api/system/pick-folder", post(pick_folder_handler))
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
        .route("/api/repos/{name}/workspace/fetch", post(fetch_workspace))
        .route("/api/repos/{name}/workspace/tree", get(workspace_tree))
        .route(
            "/api/repos/{name}/workspace/file",
            get(read_workspace_file)
                .post(create_workspace_file)
                .put(save_workspace_file)
                .delete(delete_workspace_file),
        )
        .route("/api/repos/{name}/workspace/mkdir", post(workspace_mkdir))
        .route("/api/repos/{name}/workspace/reveal", post(workspace_reveal))
        .route("/api/repos/{name}/workspace/status", get(workspace_status))
        .route("/api/repos/{name}/workspace/diff", get(workspace_diff))
        .route("/api/repos/{name}/workspace/conflict", get(conflict_stages_handler))
        .route("/api/repos/{name}/workspace/conflict/resolve", post(conflict_resolve_handler))
        .route("/api/repos/{name}/workspace/conflict/continue", post(conflict_continue_handler))
        .route("/api/repos/{name}/workspace/commit/{hash}/diff", get(commit_diff_handler))
        .route("/api/repos/{name}/workspace/commit/suggest", post(suggest_commit_message_handler))
        .route("/api/repos/{name}/workspace/secrets/scan", post(scan_secrets_handler))
        .route("/api/repos/{name}/workspace/commit", post(workspace_commit))
        .route("/api/repos/{name}/workspace/checkout", post(workspace_checkout))
        .route("/api/repos/{name}/workspace/git", post(run_workspace_git))
        .route("/api/repos/{name}/workspace/shell", post(run_workspace_shell))
        .route("/api/repos/{name}/workspace/shell/cd", post(workspace_shell_cd))
        .route("/api/repos/{name}/workspace/exec/cancel", post(cancel_workspace_exec_handler))
        .route("/api/repos/{name}/workspace/terminal", get(workspace_terminal_ws))
        .route("/api/repos/{name}/workspace/java/info", get(java_main_info))
        .route("/api/repos/{name}/workspace/java/run", post(run_java_main_handler))
        .route("/api/repos/{name}/workspace/sql/run", post(run_sql_file_handler))
        .route("/api/repos/{name}/workspace/java/test-methods", get(java_test_methods_handler).post(java_test_methods_post))
        .route("/api/repos/{name}/workspace/gradle/info", get(gradle_project_info_handler))
        .route("/api/repos/{name}/workspace/gradle/run", post(run_gradle_handler))
        .route("/api/repos/{name}/workspace/run/info", get(run_project_info_handler))
        .route("/api/repos/{name}/workspace/run/target", get(run_target_handler).post(run_target_post))
        .route("/api/repos/{name}/workspace/run/task", post(run_project_task_handler))
        .route(
            "/api/repos/{name}/workspace/build/tasks-tree",
            get(build_tasks_tree_handler).post(build_tasks_tree_post),
        )
        .route(
            "/api/repos/{name}/workspace/package/manifest",
            get(package_manifest_handler),
        )
        .route("/api/repos/{name}/workspace/coverage", get(workspace_coverage_handler))
        .route(
            "/api/repos/{name}/workspace/coverage/report",
            get(workspace_coverage_report_handler),
        )
        .route(
            "/api/repos/{name}/workspace/open-external",
            post(workspace_open_external),
        )
        .route(
            "/api/repos/{name}/workspace/db/connection",
            get(workspace_db_connection_get).put(workspace_db_connection_put),
        )
        .route(
            "/api/repos/{name}/workspace/db/schema",
            get(workspace_db_schema_handler),
        )
        .route(
            "/api/repos/{name}/workspace/db/query",
            post(workspace_db_query_handler),
        )
        .route("/api/repos/{name}/workspace/maven/run", post(run_maven_handler))
        .route("/api/repos/{name}/workspace/definition", get(workspace_definition).post(workspace_definition_post))
        .route("/api/repos/{name}/workspace/hover", get(workspace_hover).post(workspace_hover_post))
        .route("/api/repos/{name}/workspace/classes", get(workspace_classes))
        .route("/api/repos/{name}/workspace/search", get(workspace_search))
        .route("/api/repos/{name}/workspace/completions", get(workspace_completions).post(workspace_completions_post))
        .route("/api/repos/{name}/workspace/ai-completions", post(workspace_ai_completions))
        .route("/api/repos/{name}/workspace/inline-complete", post(workspace_inline_complete))
        .route("/api/repos/{name}/workspace/java/index-status", get(java_index_status))
        .route("/api/repos/{name}/workspace/java/ensure-module", post(java_ensure_module))
        .route("/api/repos/{name}/workspace/project/index-status", get(project_index_status))
        .route("/api/repos/{name}/workspace/project/reload", post(reload_project_index))
        .route("/api/repos/{name}/workspace/diagnostics", post(workspace_diagnostics))
        .route("/api/repos/{name}/workspace/quick-fixes", post(workspace_quick_fixes))
        .route("/api/repos/{name}/workspace/java/references", post(workspace_java_references))
        .route("/api/repos/{name}/workspace/references", post(workspace_references))
        .route("/api/repos/{name}/workspace/java/prepare-rename", post(workspace_java_prepare_rename))
        .route("/api/repos/{name}/workspace/prepare-rename", post(workspace_prepare_rename))
        .route("/api/repos/{name}/workspace/java/rename", post(workspace_java_rename))
        .route("/api/repos/{name}/workspace/rename", post(workspace_rename))
        .route("/api/repos/{name}/workspace/java/code-actions", post(workspace_java_code_actions))
        .route("/api/repos/{name}/workspace/java/signature-help", post(workspace_java_signature_help))
        .route("/api/repos/{name}/workspace/signature-help", post(workspace_signature_help))
        .route("/api/repos/{name}/workspace/language-context", get(workspace_language_context))
        .route("/api/repos/{name}/workspace/format", post(workspace_format))
        .route("/api/repos/{name}/unregister", post(unregister_repo_handler))
        .route("/api/repos/{name}/restore", post(restore_repo_handler))
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

async fn list_hidden_repos_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match repos::list_hidden_repos(&state.config, &state.settings) {
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

#[derive(Serialize)]
struct PickFolderResponse {
    path: Option<String>,
}

async fn pick_folder_handler() -> impl IntoResponse {
    match crate::system::pick_folder("Select a git repository folder") {
        Ok(path) => Json(PickFolderResponse { path }).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
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

async fn unregister_repo_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match repos::unregister_repo(&state.config, &state.settings, &name) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::NOT_FOUND, e),
    }
}

async fn restore_repo_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match repos::restore_repo(&state.config, &state.settings, &name) {
        Ok(repo) => Json(repo).into_response(),
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
    if !state.config.repo_exists(&name) {
        return api_error(StatusCode::NOT_FOUND, anyhow::anyhow!("not found"));
    }
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let limit = q.limit.unwrap_or(50).min(200);
    match git::log(&ws, limit) {
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
            if let Err(e) = state.settings.set_last_repo(&name) {
                tracing::warn!("Could not persist last opened repo {name}: {e:#}");
            }
            let profile = workspace::detect_project_profile(&ws).unwrap_or_default();
            state.project_index_jobs.on_open(&name, &ws);
            let index_status = state.project_index_jobs.status(&name);
            let uses_jdtls = workspace::workspace_uses_jdtls(&ws, &profile);
            let jdtls_ready = workspace::jdtls_workspace_ready(&ws);
            Json(serde_json::json!({
                "path": ws.display().to_string(),
                "profile": profile,
                "indexing": index_status.state == "running",
                "jdtls": {
                    "enabled": workspace::jdtls_enabled(),
                    "uses": uses_jdtls,
                    "ready": jdtls_ready,
                    "warming": workspace::jdtls_enabled() && uses_jdtls && !jdtls_ready,
                },
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

async fn fetch_workspace(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::fetch_workspace_remotes(&ws) {
        Ok(out) => git_response(out),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
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
    let path = q.path.clone();
    match tokio::task::spawn_blocking(move || workspace::read_file(&ws, &path)).await {
        Ok(Ok(content)) => {
            Json(serde_json::json!({ "path": q.path, "content": content })).into_response()
        }
        Ok(Err(e)) => api_error(StatusCode::NOT_FOUND, e),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::Error::from(e).context("read file task"),
        ),
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
    let rel_path = body.path.clone();
    match workspace::create_file(&ws, &rel_path, &content) {
        Ok(()) => {
            if rel_path.ends_with(".java") {
                workspace::patch_java_index_after_save(&ws, &rel_path, &content);
            }
            (StatusCode::CREATED, Json(serde_json::json!({ "path": body.path }))).into_response()
        }
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
    let rel_path = body.path.clone();
    let content = body.content.clone();
    let file_path = match workspace::safe_join(&ws, &rel_path) {
        Ok(p) => p,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    if let Some(parent) = file_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                anyhow::Error::from(e).context("create parent dirs"),
            );
        }
    }
    match tokio::fs::write(&file_path, content.as_bytes()).await {
        Ok(()) => {
            if rel_path.ends_with(".java") {
                workspace::patch_java_index_after_save(&ws, &rel_path, &content);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::Error::from(e).context("write file"),
        ),
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

#[derive(Deserialize)]
struct PathBody {
    path: String,
}

async fn workspace_mkdir(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<PathBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::create_dir(&ws, &body.path) {
        Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({ "path": body.path }))).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_reveal(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<PathBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::reveal_in_system(&ws, &body.path) {
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
    match tokio::task::spawn_blocking(move || workspace::workspace_status(&ws)).await {
        Ok(Ok(status)) => Json(status).into_response(),
        Ok(Err(e)) => api_error(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            anyhow::Error::from(e).context("workspace status task"),
        ),
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

#[derive(Deserialize)]
struct SecretScanRequest {
    paths: Vec<String>,
}

#[derive(Serialize)]
struct SecretScanResponse {
    findings: Vec<workspace::secret_scan::SecretFinding>,
}

async fn scan_secrets_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<SecretScanRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let findings = workspace::secret_scan::scan_commit_paths(&ws, &body.paths);
    Json(SecretScanResponse { findings }).into_response()
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
    match workspace::commit_changes(&ws, &body.message, body.paths.as_deref(), false) {
        Ok(commit_out) if !commit_out.success() => git_response(commit_out),
        Ok(commit_out) => {
            if !body.push {
                return git_response(commit_out);
            }
            match push_to_remote(&state.config, &state.settings, &name) {
                Ok(push_out) => {
                    if !push_out.success() {
                        git_response(push_out)
                    } else {
                        git_response(git::GitOutput {
                            stdout: format!(
                                "{}\n{}",
                                commit_out.stdout.trim(),
                                push_out.stdout.trim()
                            ),
                            stderr: format!(
                                "{}\n{}",
                                commit_out.stderr.trim(),
                                push_out.stderr.trim()
                            ),
                            exit_code: 0,
                        })
                    }
                }
                Err(e) => api_error(StatusCode::BAD_REQUEST, e),
            }
        }
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

async fn cancel_workspace_exec_handler() -> impl IntoResponse {
    let cancelled = crate::process_registry::cancel_active_exec();
    Json(serde_json::json!({ "cancelled": cancelled })).into_response()
}

#[derive(Deserialize)]
struct TerminalWsQuery {
    cwd: Option<String>,
}

async fn workspace_terminal_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(query): Query<TerminalWsQuery>,
) -> Response {
    let workspace_path = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let cwd = query.cwd;
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = workspace::terminal::run_terminal_websocket(socket, &workspace_path, cwd.as_deref()).await {
            tracing::warn!("terminal session ended: {e:#}");
        }
    })
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
struct RunSqlRequest {
    path: String,
    #[serde(default)]
    content: Option<String>,
}

async fn run_sql_file_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<RunSqlRequest>,
) -> Response {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };

    let path = body.path.trim().to_string();
    if path.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "path required");
    }
    let content = body.content;
    let database_url = resolve_repo_database_url(&state.config, &name, &ws);
    let db_ssl = metadata::repo_db_ssl(&state.config, &name);
    let (tx, rx) = tokio::sync::mpsc::channel::<workspace::ExecStreamEvent>(256);
    tokio::task::spawn_blocking(move || {
        if let Err(e) = workspace::stream_workspace_sql_file(
            &ws,
            &path,
            content.as_deref(),
            database_url.as_deref(),
            db_ssl.as_ref(),
            tx.clone(),
        ) {
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

async fn run_project_info_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::run_project_info(&ws, &q.path) {
        Ok(info) => Json(info).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct RunTargetQuery {
    path: String,
    #[serde(default = "default_one_u32")]
    line: u32,
    #[serde(default)]
    use_ai: bool,
}

fn default_one_u32() -> u32 {
    1
}

#[derive(Deserialize)]
struct RunTargetRequest {
    path: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default = "default_one_u32")]
    line: u32,
    #[serde(default)]
    use_ai: bool,
}

async fn run_target_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<RunTargetQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = q.path.trim().to_string();
    if path.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "path required");
    }
    let line = q.line.max(1);
    let database_url = resolve_repo_database_url(&state.config, &name, &ws);
    let db_ssl = metadata::repo_db_ssl(&state.config, &name);
    let ws_clone = ws.clone();
    let path_clone = path.clone();
    match tokio::task::spawn_blocking(move || {
        workspace::run_context(
            &ws_clone,
            &path_clone,
            None,
            line,
            database_url.as_deref(),
            db_ssl.as_ref(),
        )
    })
    .await
    {
        Ok(Ok(mut ctx)) => {
            maybe_enhance_run_target_with_ai(
                &state,
                &ws,
                &path,
                line,
                None,
                &mut ctx,
                q.use_ai,
            )
            .await;
            Json(ctx).into_response()
        }
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("run target task failed: {e:#}"),
        ),
    }
}

async fn run_target_post(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<RunTargetRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = body.path.trim().to_string();
    if path.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "path required");
    }
    let line = body.line.max(1);
    let content = body.content.clone();
    let database_url = resolve_repo_database_url(&state.config, &name, &ws);
    let ws_clone = ws.clone();
    let path_clone = path.clone();
    let content_for_task = content.clone();
    let db_ssl = metadata::repo_db_ssl(&state.config, &name);
    match tokio::task::spawn_blocking(move || {
        workspace::run_context(
            &ws_clone,
            &path_clone,
            content_for_task.as_deref(),
            line,
            database_url.as_deref(),
            db_ssl.as_ref(),
        )
    })
    .await
    {
        Ok(Ok(mut ctx)) => {
            maybe_enhance_run_target_with_ai(
                &state,
                &ws,
                &path,
                line,
                content.as_deref(),
                &mut ctx,
                body.use_ai,
            )
            .await;
            Json(ctx).into_response()
        }
        Ok(Err(e)) => api_error(StatusCode::BAD_REQUEST, e),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("run target task failed: {e:#}"),
        ),
    }
}

async fn maybe_enhance_run_target_with_ai(
    state: &AppState,
    ws: &std::path::Path,
    path: &str,
    line: u32,
    content: Option<&str>,
    ctx: &mut workspace::RunContext,
    force: bool,
) {
    let Some(target) = ctx.target.as_mut() else {
        return;
    };
    if state.settings.gemini_api_key().is_none() {
        return;
    }
    let src = match content {
        Some(c) => c.to_string(),
        None => workspace::read_file(ws, path).unwrap_or_default(),
    };
    if !force && !workspace::needs_ai_run_classification(target, &src) {
        return;
    }
    if let Ok(hint) = git_agent::suggest_run_target(
        &state.settings,
        ws,
        path,
        line,
        &src,
        &ctx.project,
        target,
    )
    .await
    {
        workspace::apply_ai_run_target(target, &hint);
    }
}

async fn build_tasks_tree_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::build_tasks_tree(&ws, &q.path, None) {
        Ok(tree) => Json(tree).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct BuildTasksTreeRequest {
    path: String,
    #[serde(default)]
    content: Option<String>,
}

async fn build_tasks_tree_post(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<BuildTasksTreeRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = body.path.trim().to_string();
    if path.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "path required");
    }
    match workspace::build_tasks_tree(&ws, &path, body.content.as_deref()) {
        Ok(tree) => Json(tree).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn package_manifest_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::package_manifest_view(&ws, &q.path) {
        Ok(view) => Json(view).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct RunTaskRequest {
    path: String,
    #[serde(default)]
    task: String,
    #[serde(default)]
    coverage: bool,
}

async fn run_project_task_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<RunTaskRequest>,
) -> Response {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = body.path.trim().to_string();
    let task = body.task.trim().to_string();
    let coverage = body.coverage;
    let (tx, rx) = tokio::sync::mpsc::channel::<workspace::ExecStreamEvent>(256);
    tokio::task::spawn_blocking(move || {
        if let Err(e) = workspace::stream_workspace_run_task(&ws, &path, &task, coverage, tx.clone()) {
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

async fn workspace_coverage_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::coverage_for_file(&ws, &q.path) {
        Ok(cov) => Json(cov).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_coverage_report_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::coverage_report_summary(&ws, &q.path) {
        Ok(summary) => Json(summary).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_open_external(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<PathBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::open_in_system(&ws, &body.path) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

fn repo_database_url(config: &crate::config::Config, name: &str) -> Option<String> {
    metadata::load(config, name)
        .ok()
        .and_then(|meta| meta.database_url)
        .filter(|url| !url.trim().is_empty())
}

fn resolve_repo_database_url(
    config: &crate::config::Config,
    name: &str,
    ws: &std::path::Path,
) -> Option<String> {
    let stored = repo_database_url(config, name);
    workspace::effective_database_url(ws, stored.as_deref())
}

async fn workspace_db_connection_get(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let database_url = repo_database_url(&state.config, &name);
    let db_ssl = metadata::repo_db_ssl(&state.config, &name);
    Json(workspace::db_connection_view(
        &ws,
        database_url.as_deref(),
        db_ssl.as_ref(),
    ))
    .into_response()
}

async fn workspace_db_connection_put(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<workspace::DbConnectionRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match metadata::set_db_connection(
        &state.config,
        &name,
        body.database_url.clone(),
        body.ssl.clone(),
    ) {
        Ok(_) => {
            let database_url = repo_database_url(&state.config, &name);
            let db_ssl = metadata::repo_db_ssl(&state.config, &name);
            Json(workspace::db_connection_view(
                &ws,
                database_url.as_deref(),
                db_ssl.as_ref(),
            ))
            .into_response()
        }
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_db_schema_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let database_url = repo_database_url(&state.config, &name);
    let db_ssl = metadata::repo_db_ssl(&state.config, &name);
    Json(workspace::db_schema(
        &ws,
        database_url.as_deref(),
        db_ssl.as_ref(),
    ))
    .into_response()
}

async fn workspace_db_query_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<workspace::DbQueryRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let database_url = repo_database_url(&state.config, &name);
    let db_ssl = metadata::repo_db_ssl(&state.config, &name);
    let limit = body.limit.clamp(1, 5_000);
    Json(workspace::db_query(
        &ws,
        database_url.as_deref(),
        db_ssl.as_ref(),
        &body.sql,
        limit,
    ))
    .into_response()
}

async fn run_maven_handler(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<RunTaskRequest>,
) -> Response {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let goal = if body.task.trim().is_empty() {
        match workspace::maven_project_info(&ws, body.path.trim()) {
            Ok(info) if info.is_maven => info.default_goal,
            Ok(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    anyhow::anyhow!("not inside a Maven project"),
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
        if let Err(e) = workspace::stream_workspace_maven(&ws, &path, &goal, tx.clone()) {
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
    #[serde(default)]
    member: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
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
    #[serde(default)]
    member: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
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

async fn workspace_hover(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<DefinitionQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let from_path = q.path.trim();
    let result = if let Some(member) = q.member.as_deref().filter(|m| !m.is_empty()) {
        workspace::find_member_hover_with_content(&ws, from_path, q.line, q.column, member, None)
    } else if let Some(symbol) = q.symbol.as_deref().filter(|s| !s.is_empty()) {
        workspace::find_symbol_hover_with_content(&ws, from_path, q.line, q.column, symbol, None)
    } else {
        workspace::find_hover_with_content(&ws, from_path, q.line, q.column, None)
    };
    match result {
        Ok(hit) => Json(hit).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_hover_post(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<DefinitionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let from_path = body.path.trim();
    let result = if let Some(member) = body.member.as_deref().filter(|m| !m.is_empty()) {
        workspace::find_member_hover_with_content(
            &ws,
            from_path,
            body.line,
            body.column,
            member,
            body.content.as_deref(),
        )
    } else if let Some(symbol) = body.symbol.as_deref().filter(|s| !s.is_empty()) {
        workspace::find_symbol_hover_with_content(
            &ws,
            from_path,
            body.line,
            body.column,
            symbol,
            body.content.as_deref(),
        )
    } else {
        workspace::find_hover_with_content(
            &ws,
            from_path,
            body.line,
            body.column,
            body.content.as_deref(),
        )
    };
    match result {
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
    if workspace::is_java_indexable_workspace(&ws) {
        state.java_index_jobs.ensure_for_class_search(&name, &ws);
    }
    let query = q.q.unwrap_or_default();
    let limit = q.limit.unwrap_or(50);
    match workspace::search_classes(&ws, &query, limit) {
        Ok(hits) => Json(hits).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_search(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<ClassSearchQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let query = q.q.unwrap_or_default();
    let limit = q.limit.unwrap_or(50);
    match workspace::search_workspace(&ws, &query, limit) {
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
    let from_path = q.path.trim();
    if workspace::should_ensure_java_index_for_completions(from_path) {
        ensure_java_index_for_file(&state, &name, &ws, from_path);
    }
    match workspace::java_completions(&ws, from_path, q.line, q.column, &prefix, None, &[]) {
        Ok(items) => {
            if items.is_empty() {
                tracing::debug!(
                    "completions empty for {}:{}:{} prefix={:?}",
                    from_path,
                    q.line,
                    q.column,
                    prefix
                );
            } else {
                tracing::debug!(
                    "completions {}:{}:{} → {} items (first: {})",
                    from_path,
                    q.line,
                    q.column,
                    items.len(),
                    items.first().map(|i| i.label.as_str()).unwrap_or("")
                );
            }
            Json(items).into_response()
        },
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct CompletionsOverlay {
    path: String,
    content: String,
}

#[derive(Deserialize)]
struct CompletionsBody {
    path: String,
    line: u32,
    column: u32,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    overlays: Vec<CompletionsOverlay>,
}

async fn workspace_completions_post(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<CompletionsBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let prefix = body.prefix.unwrap_or_default();
    let from_path = body.path.trim();
    if workspace::should_ensure_java_index_for_completions(from_path) {
        ensure_java_index_for_file(&state, &name, &ws, from_path);
    }
    let overlays: Vec<(String, String)> = body
        .overlays
        .into_iter()
        .map(|o| (o.path.trim().to_string(), o.content))
        .filter(|(p, _)| !p.is_empty())
        .collect();
    match workspace::java_completions(
        &ws,
        from_path,
        body.line,
        body.column,
        &prefix,
        body.content.as_deref(),
        &overlays,
    ) {
        Ok(items) => {
            tracing::info!(
                "completions POST {}:{}:{} prefix={:?} → {} items [{}]",
                from_path,
                body.line,
                body.column,
                prefix,
                items.len(),
                items
                    .iter()
                    .take(6)
                    .map(|i| i.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Json(items).into_response()
        },
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct AiCompletionsBody {
    path: String,
    line: u32,
    column: u32,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    line_prefix: Option<String>,
    content: String,
}

async fn workspace_ai_completions(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<AiCompletionsBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let prefix = body.prefix.unwrap_or_default();
    let line_prefix = body.line_prefix.unwrap_or_default();
    match git_agent::suggest_ai_completions(
        &state.settings,
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
        &line_prefix,
        &prefix,
    )
    .await
    {
        Ok(items) => Json(items).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct InlineCompleteRequest {
    path: String,
    line: u32,
    column: u32,
    content: String,
    #[serde(default)]
    line_prefix: String,
    /// Index/symbol fallback only — skip Gemini (fast path for inline ghost).
    #[serde(default)]
    local_only: bool,
}

#[derive(Serialize)]
struct InlineCompleteResponse {
    text: String,
}

async fn workspace_inline_complete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<InlineCompleteRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let from_path = body.path.trim();
    if workspace::should_ensure_java_index_for_completions(from_path) {
        ensure_java_index_for_file(&state, &name, &ws, from_path);
    }
    match git_agent::suggest_inline_completion(
        &state.settings,
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
        &body.line_prefix,
        body.local_only,
    )
    .await
    {
        Ok(text) => Json(InlineCompleteResponse { text }).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize)]
struct DiagnosticsOverlay {
    path: String,
    content: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum DiagnosticsScope {
    #[default]
    Typing,
    Full,
}

#[derive(Deserialize)]
struct DiagnosticsRequest {
    path: String,
    content: String,
    #[serde(default)]
    overlays: Vec<DiagnosticsOverlay>,
    #[serde(default)]
    scope: DiagnosticsScope,
}

fn java_diag_scope(scope: DiagnosticsScope) -> workspace::JavaDiagScope {
    match scope {
        DiagnosticsScope::Typing => workspace::JavaDiagScope::Typing,
        DiagnosticsScope::Full => workspace::JavaDiagScope::Full,
    }
}

async fn java_index_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    Json(state.java_index_jobs.status(&name)).into_response()
}

#[derive(Deserialize)]
struct EnsureModuleQuery {
    path: String,
}

async fn java_ensure_module(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<EnsureModuleQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    ensure_java_index_for_file(&state, &name, &ws, q.path.trim());
    Json(state.java_index_jobs.status(&name)).into_response()
}

async fn project_index_status(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let mut status = state.project_index_jobs.status(&name);
    if status.state != "running" {
        if let Ok(ws) = workspace::ensure_workspace(&state.config, &name) {
            status.needs_refresh = workspace::java_index_needs_refresh(&ws);
        }
    }
    Json(status).into_response()
}

async fn reload_project_index(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    state.project_index_jobs.reload(&name, &ws);
    Json(state.project_index_jobs.status(&name)).into_response()
}

async fn workspace_language_context(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Query(q): Query<PathQuery>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = q.path.trim().to_string();
    if path.is_empty() {
        return api_error(StatusCode::BAD_REQUEST, "path required");
    }
    Json(workspace::language_compiler_context(&ws, &path)).into_response()
}

async fn workspace_diagnostics(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<DiagnosticsRequest>,
) -> Response {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return diagnostics_http_response(api_error(StatusCode::BAD_REQUEST, e)),
    };
    let path = body.path.trim().to_string();
    let rel_path = path.clone();
    let content = body.content;
    let scope = java_diag_scope(body.scope);
    let scope_is_full = scope == workspace::JavaDiagScope::Full;
    let overlays: Vec<(String, String)> = body
        .overlays
        .into_iter()
        .map(|o| (o.path.trim().to_string(), o.content))
        .filter(|(p, _)| !p.is_empty())
        .collect();

    let (tx, rx) = oneshot::channel();
    let ws_job = ws.clone();
    let path_job = path.clone();
    let content_job = content.clone();
    let overlays_job = overlays.clone();
    let job = tokio::spawn(async move {
        let _full_permit = if scope_is_full {
            let sem = JAVA_FULL_DIAG_SEM.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(1)));
            Some(sem.acquire().await.ok())
        } else {
            None
        };
        let result = timeout(
            JAVA_DIAG_API_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                workspace::file_diagnostics(&ws_job, &path_job, &content_job, &overlays_job, scope)
            }),
        )
        .await;
        let _ = tx.send(result);
    });

    let mut guard = DiagRequestGuard {
        ws: ws.clone(),
        rel_path: rel_path.clone(),
        job_abort: Some(job.abort_handle()),
        cancel_on_drop: true,
    };

    // Await the detached job via oneshot — if the client disconnects (fetch abort), hyper drops
    // this handler, the guard aborts javac, and the HTTP connection slot frees immediately.
    match rx.await {
        Ok(Ok(Ok(Ok(result)))) => {
            guard.disable();
            diagnostics_http_response(Json(result))
        }
        Ok(Ok(Ok(Err(e)))) => {
            guard.disable();
            diagnostics_http_response(api_error(StatusCode::BAD_REQUEST, e))
        }
        Ok(Ok(Err(e))) => diagnostics_http_response(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("diagnostics task failed: {e:#}"),
        )),
        Ok(Err(_)) | Err(_) => cancelled_diagnostics_response(),
    }
}

#[derive(Deserialize)]
struct QuickFixesRequest {
    path: String,
    content: String,
    diagnostics: Vec<workspace::QuickFixDiagnostic>,
}

async fn workspace_quick_fixes(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<QuickFixesRequest>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let path = body.path.trim();
    let mut fixes = match workspace::suggest_local_quick_fixes(
        &ws,
        path,
        &body.content,
        &body.diagnostics,
    ) {
        Ok(f) => f,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    if path.ends_with(".java") {
        if let Some(diag) = body.diagnostics.first() {
            if let Ok(jdtls) = workspace::jdtls_code_actions_as_quick_fixes(
                &ws,
                path,
                diag.line,
                diag.column,
                &body.content,
                &["quickfix"],
            ) {
                workspace::merge_quick_fixes(&mut fixes, jdtls);
            }
        }
    }
    match git_agent::suggest_ai_quick_fixes(
        &state.settings,
        &ws,
        path,
        &body.content,
        &body.diagnostics,
        Some(&state.cursor_bridge),
    )
    .await
    {
        Ok(ai) => {
            let ai = workspace::filter_ai_import_fixes(
                &ws,
                path,
                &body.content,
                &fixes,
                ai,
                &body.diagnostics,
            );
            workspace::merge_quick_fixes(&mut fixes, ai);
        }
        Err(e) => {
            if fixes.is_empty() {
                return api_error(StatusCode::BAD_REQUEST, e);
            }
            tracing::warn!("ai quick fixes failed (returning local/jdtls): {e:#}");
        }
    }
    Json(fixes).into_response()
}

#[derive(Deserialize)]
struct JavaPositionBody {
    path: String,
    line: u32,
    column: u32,
    content: String,
}

#[derive(Deserialize)]
struct JavaRenameBody {
    path: String,
    line: u32,
    column: u32,
    content: String,
    new_name: String,
}

#[derive(Deserialize)]
struct JavaCodeActionsBody {
    path: String,
    line: u32,
    column: u32,
    content: String,
    #[serde(default)]
    only: Vec<String>,
}

async fn workspace_java_references(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaPositionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::java_references(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
    ) {
        Ok(items) => Json(items).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_java_prepare_rename(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaPositionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::java_prepare_rename(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
    ) {
        Ok(range) => Json(range).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_java_rename(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaRenameBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::java_rename(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
        &body.new_name,
    ) {
        Ok(edits) => Json(edits).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_java_code_actions(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaCodeActionsBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    let only: Vec<&str> = if body.only.is_empty() {
        vec!["source.organizeImports"]
    } else {
        body.only.iter().map(String::as_str).collect()
    };
    match workspace::java_code_actions(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
        &only,
    ) {
        Ok(actions) => Json(actions).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_java_signature_help(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaPositionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::java_signature_help(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
    ) {
        Ok(help) => Json(help).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_references(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaPositionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::workspace_references(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
    ) {
        Ok(items) => Json(items).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_prepare_rename(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaPositionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::workspace_prepare_rename(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
    ) {
        Ok(range) => Json(range).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_rename(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaRenameBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::workspace_rename(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
        &body.new_name,
    ) {
        Ok(edits) => Json(edits).into_response(),
        Err(e) => api_error(StatusCode::BAD_REQUEST, e),
    }
}

async fn workspace_signature_help(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(body): Json<JavaPositionBody>,
) -> impl IntoResponse {
    let ws = match workspace::ensure_workspace(&state.config, &name) {
        Ok(ws) => ws,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, e),
    };
    match workspace::workspace_signature_help(
        &ws,
        body.path.trim(),
        body.line,
        body.column,
        &body.content,
    ) {
        Ok(help) => Json(help).into_response(),
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

#[derive(Serialize)]
struct AppVersionResponse {
    version: &'static str,
    build: &'static str,
    loopback_ws: String,
}

async fn app_version(State(state): State<Arc<AppState>>) -> Json<AppVersionResponse> {
    Json(AppVersionResponse {
        version: env!("CARGO_PKG_VERSION"),
        build: env!("REAPER_UI_BUILD"),
        loopback_ws: super::loopback_ws_base(&state.config.host, state.config.port),
    })
}

fn ensure_java_index_for_file(
    state: &AppState,
    repo: &str,
    ws: &std::path::Path,
    rel_path: &str,
) {
    state
        .java_index_jobs
        .ensure_module_for_path(repo, ws, rel_path);
}

fn api_error(status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}
