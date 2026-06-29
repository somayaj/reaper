use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::classpath::{self, WarmIndexStatus};

#[derive(Clone, Serialize, Default, Debug)]
pub struct JavaIndexStatus {
    pub state: String,
    pub phase: String,
    pub symbol_count: usize,
    pub dependency_jars: usize,
    pub source_jars: usize,
    pub jdk_sources: bool,
    pub spring_symbols: usize,
    pub jdk_symbols: usize,
    pub cached: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct JobEntry {
    building: bool,
    status: JavaIndexStatus,
}

pub struct JavaIndexJobs {
    inner: Arc<Mutex<HashMap<String, JobEntry>>>,
}

impl Default for JavaIndexJobs {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaIndexJobs {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn status(&self, repo: &str) -> JavaIndexStatus {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.get(repo).map(|e| e.status.clone()))
            .unwrap_or_default()
    }

    /// Load Java index counts from disk into memory (no rebuild).
    pub fn refresh_status_from_disk(&self, repo: &str, ws: &Path) {
        if !classpath::is_java_indexable_workspace(ws) {
            return;
        }
        if let Ok(peek) = classpath::peek_index_status(ws) {
            if peek.symbol_count == 0 && !peek.indexed {
                return;
            }
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            if entry.building {
                return;
            }
            entry.status = status_from_warm(&peek, None);
        }
    }

    /// Start a background index build if one is not already running.
    pub fn ensure_building(&self, repo: &str, ws: &Path) {
        if !classpath::is_java_indexable_workspace(ws) {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            if !entry.building {
                entry.status = JavaIndexStatus {
                    state: "idle".into(),
                    ..Default::default()
                };
            }
            return;
        }

        let repo_key = repo.to_string();
        let ws_path = ws.to_path_buf();

        if let Ok(peek) = classpath::peek_index_status(&ws_path) {
            if peek.symbol_count > 0 {
                let mut guard = match self.inner.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let entry = guard.entry(repo_key.clone()).or_default();
                if !entry.building {
                    entry.status = status_from_warm(&peek, None);
                }
                if peek.cached && entry.status.state == "ready" {
                    return;
                }
            }
        }

        let partial_count = classpath::peek_index_status(&ws_path)
            .ok()
            .filter(|p| p.indexed && p.symbol_count > 0)
            .map(|p| p.symbol_count)
            .unwrap_or(0);

        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo_key.clone()).or_default();
            if entry.building {
                return;
            }
            entry.building = true;
            entry.status = JavaIndexStatus {
                state: "running".into(),
                symbol_count: partial_count,
                ..Default::default()
            };
            true
        };

        if !should_spawn {
            return;
        }

        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let progress_inner = Arc::clone(&inner);
            let progress_repo = repo_key.clone();
            let progress = Box::new(move |phase: &str, count: usize| {
                if let Ok(mut guard) = progress_inner.lock() {
                    if let Some(entry) = guard.get_mut(&progress_repo) {
                        entry.status.state = "running".into();
                        entry.status.phase = phase.to_string();
                        entry.status.symbol_count = count;
                    }
                }
            });
            let result = classpath::warm_index_with_progress(&ws_path, Some(progress));
            let mut guard = match inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(entry) = guard.get_mut(&repo_key) else {
                return;
            };
            entry.building = false;
            match result {
                Ok(warm) if warm.indexed => {
                    entry.status = status_from_warm(&warm, None);
                }
                Ok(_) => {
                    entry.status.state = "idle".into();
                }
                Err(e) => {
                    entry.status = JavaIndexStatus {
                        state: "error".into(),
                        error: Some(e.to_string()),
                        ..Default::default()
                    };
                }
            }
        });
    }
}

fn status_from_warm(warm: &WarmIndexStatus, error: Option<String>) -> JavaIndexStatus {
    JavaIndexStatus {
        state: if warm.symbol_count > 0 {
            "ready".into()
        } else if warm.indexed {
            "idle".into()
        } else {
            "idle".into()
        },
        phase: if warm.symbol_count > 0 {
            "ready".into()
        } else {
            String::new()
        },
        symbol_count: warm.symbol_count,
        dependency_jars: warm.dependency_jars,
        source_jars: warm.source_jars,
        jdk_sources: warm.jdk_sources,
        spring_symbols: warm.spring_symbols,
        jdk_symbols: warm.jdk_symbols,
        cached: warm.cached,
        error,
    }
}
