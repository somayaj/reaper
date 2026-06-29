use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::classpath;
use super::index_jobs::{JavaIndexJobs, JavaIndexStatus};
use super::project_profile::{self, ProjectProfile};
use super::symbols;

#[derive(Clone, Serialize, Default, Debug)]
pub struct ProjectIndexStatus {
    pub state: String,
    pub phase: String,
    pub profile: ProjectProfile,
    pub label: String,
    pub java: JavaIndexStatus,
    pub workspace_symbols: usize,
    pub error: Option<String>,
}

#[derive(Default)]
struct JobEntry {
    building: bool,
    profile: ProjectProfile,
    status: ProjectIndexStatus,
}

pub struct ProjectIndexJobs {
    inner: Arc<Mutex<HashMap<String, JobEntry>>>,
    java: Arc<JavaIndexJobs>,
}

impl ProjectIndexJobs {
    pub fn new(java: Arc<JavaIndexJobs>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            java,
        }
    }

    pub fn status(&self, repo: &str) -> ProjectIndexStatus {
        let mut status = self
            .inner
            .lock()
            .ok()
            .and_then(|g| g.get(repo).map(|e| e.status.clone()))
            .unwrap_or_default();
        status.java = self.java.status(repo);
        status
    }

    /// Scan the repo and start background indexers (called when a workspace opens).
    pub fn on_open(&self, repo: &str, ws: &Path) {
        let profile = project_profile::detect(ws).unwrap_or_default();
        if profile.indexers.iter().any(|i| i == "java") {
            self.java.refresh_status_from_disk(repo, ws);
            self.java.ensure_building(repo, ws);
        }
        self.start(repo, ws, profile, false);
    }

    /// Invalidate caches and rebuild indexes (called after branch checkout).
    pub fn on_checkout(&self, repo: &str, ws: &Path) {
        let _ = classpath::invalidate_caches(ws);
        symbols::invalidate_symbol_cache(ws);
        let profile = project_profile::detect(ws).unwrap_or_default();
        self.start(repo, ws, profile, true);
    }

    fn start(&self, repo: &str, ws: &Path, profile: ProjectProfile, force: bool) {
        if profile.indexers.is_empty() {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            entry.profile = profile.clone();
            entry.status = ProjectIndexStatus {
                state: "idle".into(),
                profile: profile.clone(),
                label: project_profile::indexing_label(&profile),
                java: self.java.status(repo),
                ..Default::default()
            };
            return;
        }

        if !force && self.caches_warm(ws, repo, &profile) {
            let java = self.java.status(repo);
            if java.state != "running" {
                return;
            }
        }

        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            if entry.building {
                return;
            }
            entry.building = true;
            entry.profile = profile.clone();
            entry.status = ProjectIndexStatus {
                state: "running".into(),
                phase: "starting".into(),
                profile: profile.clone(),
                label: project_profile::indexing_label(&profile),
                java: JavaIndexStatus {
                    state: "running".into(),
                    ..Default::default()
                },
                ..Default::default()
            };
            true
        };

        if !should_spawn {
            return;
        }

        if profile.indexers.iter().any(|i| i == "java") {
            self.java.ensure_building(repo, ws);
        }

        let repo_key = repo.to_string();
        let ws_path = ws.to_path_buf();
        let profile_clone = profile.clone();
        let inner = Arc::clone(&self.inner);
        let java_jobs = Arc::clone(&self.java);

        std::thread::spawn(move || {
            let mut workspace_symbols = 0usize;
            let mut error: Option<String> = None;

            if profile_clone.indexers.iter().any(|i| i == "workspace-symbols") {
                touch_project_phase(&inner, &repo_key, "workspace-symbols", 0);
                match symbols::warm_symbol_cache(&ws_path) {
                    Ok(n) => workspace_symbols = n,
                    Err(e) => error = Some(format!("workspace symbols: {e:#}")),
                }
                touch_project_phase(&inner, &repo_key, "workspace-symbols", workspace_symbols);
            }

            if profile_clone.indexers.iter().any(|i| i == "java") {
                touch_project_phase(&inner, &repo_key, "java-index", workspace_symbols);
                for _ in 0..600 {
                    let status = java_jobs.status(&repo_key);
                    if status.state.is_empty()
                        || status.state == "ready"
                        || status.state == "error"
                        || status.state == "idle"
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }

            let java = java_jobs.status(&repo_key);
            let needs_java = profile_clone.indexers.iter().any(|i| i == "java");
            let state = if error.is_some() || java.state == "error" {
                "error".into()
            } else if needs_java && java.state == "running" {
                "running".into()
            } else if java.state == "ready" || workspace_symbols > 0 {
                "ready".into()
            } else if java.state == "running" {
                "running".into()
            } else {
                "idle".into()
            };

            let mut guard = match inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(entry) = guard.get_mut(&repo_key) else {
                return;
            };
            entry.building = false;
            let phase = if state == "ready" {
                "ready".to_string()
            } else if state == "error" {
                "error".to_string()
            } else {
                java.phase.clone()
            };
            entry.status = ProjectIndexStatus {
                state,
                phase,
                profile: profile_clone.clone(),
                label: project_profile::indexing_label(&profile_clone),
                java,
                workspace_symbols,
                error: error.or_else(|| java_jobs.status(&repo_key).error.clone()),
            };
        });
    }

    fn caches_warm(&self, ws: &Path, repo: &str, profile: &ProjectProfile) -> bool {
        let needs_java = profile.indexers.iter().any(|i| i == "java");
        let needs_symbols = profile.indexers.iter().any(|i| i == "workspace-symbols");

        let java_ok = !needs_java || {
            classpath::peek_index_status(ws)
                .map(|p| p.indexed && p.symbol_count > 0)
                .unwrap_or(false)
        };
        let symbols_ok =
            !needs_symbols || super::symbols::workspace_symbol_cache_count(ws) > 0;

        if java_ok && symbols_ok {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return false,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            entry.profile = profile.clone();
            let java = self.java.status(repo);
            let java_running = java.state == "running";
            let java_ready = java.state == "ready" && java.symbol_count > 0;
            entry.status = ProjectIndexStatus {
                state: if java_running {
                    "running".into()
                } else {
                    "ready".into()
                },
                phase: if java_ready {
                    "ready".into()
                } else {
                    String::new()
                },
                profile: profile.clone(),
                label: project_profile::indexing_label(profile),
                java,
                workspace_symbols: self.symbol_cache_count(ws),
                ..Default::default()
            };
            return !java_running;
        }
        false
    }

    fn symbol_cache_count(&self, ws: &Path) -> usize {
        super::symbols::workspace_symbol_cache_count(ws)
    }
}

fn touch_project_phase(
    inner: &Arc<Mutex<HashMap<String, JobEntry>>>,
    repo_key: &str,
    phase: &str,
    workspace_symbols: usize,
) {
    let mut guard = match inner.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let entry = guard.entry(repo_key.to_string()).or_default();
    entry.status.state = "running".into();
    entry.status.phase = phase.into();
    entry.status.workspace_symbols = workspace_symbols;
}
