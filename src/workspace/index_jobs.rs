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
    /// JARs indexed so far (staged indexing).
    #[serde(default)]
    pub jars_indexed: usize,
    #[serde(default)]
    pub jars_total: usize,
    #[serde(default = "default_index_complete")]
    pub index_complete: bool,
}

fn default_index_complete() -> bool {
    true
}

#[derive(Default)]
struct JobEntry {
    building: bool,
    background_running: bool,
    tooling_running: bool,
    /// Module roots currently indexing (lazy on-demand).
    root_building: std::collections::HashSet<String>,
    status: JavaIndexStatus,
}

#[derive(Clone)]
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

    /// Clear in-memory status so a forced reload can restart indexing.
    pub fn clear_repo(&self, repo: &str) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.remove(repo);
        }
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
            if entry.building || entry.background_running {
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

        if classpath::java_index_is_lazy() {
            self.refresh_status_from_disk(repo, ws);
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            if entry.building || entry.status.state == "running" {
                return;
            }
            if entry.status.symbol_count == 0 {
                entry.status = JavaIndexStatus {
                    state: "ready".into(),
                    phase: "on-demand".into(),
                    index_complete: true,
                    ..Default::default()
                };
            }
            return;
        }

        let repo_key = repo.to_string();
        let ws_path = ws.to_path_buf();
        let peek = classpath::peek_index_status(&ws_path).ok();
        let has_symbols = peek.as_ref().is_some_and(|p| p.symbol_count > 0);
        let needs_tooling = classpath::needs_any_tooling_classpath_resolve(&ws_path);
        let background_pending = peek
            .as_ref()
            .is_some_and(|p| p.indexed && !p.index_complete && p.symbol_count > 0);

        if let Some(ref peek) = peek {
            if peek.symbol_count > 0 {
                let (should_spawn_bg, should_return_ready, should_spawn_tooling) = {
                    let mut guard = match self.inner.lock() {
                        Ok(g) => g,
                        Err(_) => return,
                    };
                    let entry = guard.entry(repo_key.clone()).or_default();
                    if !entry.building {
                        entry.status = status_from_warm(peek, None);
                    }
                    let spawn_bg = background_pending && !entry.background_running;
                    let return_ready = !needs_tooling
                        && peek.cached
                        && peek.index_complete
                        && entry.status.state == "ready";
                    let spawn_tooling = has_symbols && needs_tooling && !entry.tooling_running;
                    (spawn_bg, return_ready, spawn_tooling)
                };
                if should_spawn_bg {
                    self.spawn_background_index(&repo_key, &ws_path);
                }
                if should_return_ready || background_pending {
                    return;
                }
                if should_spawn_tooling {
                    self.spawn_tooling_resolve(&repo_key, &ws_path);
                    return;
                }
            }
        }

        let partial_count = peek
            .as_ref()
            .filter(|p| p.indexed && p.symbol_count > 0)
            .map(|p| p.symbol_count)
            .unwrap_or(0);

        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo_key.clone()).or_default();
            if entry.building || entry.tooling_running || entry.background_running {
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
            if crate::process_registry::is_shutdown_requested() {
                return;
            }
            let progress_inner = Arc::clone(&inner);
            let progress_repo = repo_key.clone();
            let progress: Box<dyn Fn(&str, usize) + Send> = Box::new(move |phase: &str, count: usize| {
                if let Ok(mut guard) = progress_inner.lock() {
                    if let Some(entry) = guard.get_mut(&progress_repo) {
                        entry.status.state = "running".into();
                        entry.status.phase = phase.to_string();
                        entry.status.symbol_count = count;
                    }
                }
            });
            if let Err(e) = classpath::resolve_classpaths_for_index(&ws_path, Some(&progress)) {
                tracing::warn!(
                    "Classpath resolve before index failed for {}: {e:#}",
                    ws_path.display()
                );
            }
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
                    if !warm.index_complete {
                        drop(guard);
                        JavaIndexJobs {
                            inner: Arc::clone(&inner),
                        }
                        .spawn_background_index(&repo_key, &ws_path);
                    }
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

    fn spawn_background_index(&self, repo_key: &str, ws: &Path) {
        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo_key.to_string()).or_default();
            if entry.building || entry.background_running {
                return;
            }
            entry.background_running = true;
            if entry.status.state != "running" {
                entry.status.state = "ready".into();
                entry.status.phase = "jar-index-background".into();
            }
            true
        };
        if !should_spawn {
            return;
        }

        let inner = Arc::clone(&self.inner);
        let repo_key = repo_key.to_string();
        let ws_path = ws.to_path_buf();
        std::thread::spawn(move || {
            if crate::process_registry::is_shutdown_requested() {
                return;
            }
            tracing::info!(
                "Continuing Java JAR index in background for {}",
                ws_path.display()
            );
            let progress_inner = Arc::clone(&inner);
            let progress_repo = repo_key.clone();
            let progress: Box<dyn Fn(&str, usize) + Send> = Box::new(move |phase: &str, count: usize| {
                if let Ok(mut guard) = progress_inner.lock() {
                    if let Some(entry) = guard.get_mut(&progress_repo) {
                        entry.status.phase = phase.to_string();
                        entry.status.symbol_count = count;
                        if phase == "jar-index-background" {
                            entry.status.state = "ready".into();
                        }
                    }
                }
            });
            let jar_progress_inner = Arc::clone(&inner);
            let jar_progress_repo = repo_key.clone();
            let jar_progress: Box<dyn Fn(usize, usize) + Send> =
                Box::new(move |jars_indexed: usize, jars_total: usize| {
                    if let Ok(mut guard) = jar_progress_inner.lock() {
                        if let Some(entry) = guard.get_mut(&jar_progress_repo) {
                            entry.status.jars_indexed = jars_indexed;
                            entry.status.jars_total = jars_total;
                            entry.status.phase = "jar-index-background".into();
                            entry.status.state = "ready".into();
                        }
                    }
                });
            if let Err(e) = classpath::continue_background_index(
                &ws_path,
                Some(&progress),
                Some(&jar_progress),
            ) {
                tracing::warn!(
                    "Background Java index failed for {}: {e:#}",
                    ws_path.display()
                );
            }
            let mut guard = match inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(entry) = guard.get_mut(&repo_key) else {
                return;
            };
            entry.background_running = false;
            if entry.building {
                return;
            }
            if let Ok(peek) = classpath::peek_index_status(&ws_path) {
                if peek.symbol_count > 0 {
                    entry.status = status_from_warm(&peek, None);
                }
            }
        });
    }

    /// Lazy mode: index the module containing `rel_path` when a file is opened or completions run.
    pub fn ensure_module_for_path(&self, repo: &str, ws: &Path, rel_path: &str) {
        if !classpath::java_index_is_lazy() {
            self.ensure_building(repo, ws);
            return;
        }
        if !classpath::is_java_indexable_workspace(ws) {
            return;
        }
        let Ok(Some(root)) = classpath::index_root_for_path(ws, rel_path) else {
            return;
        };
        let root_for_index = root.canonicalize().unwrap_or(root);
        let root_key = root_for_index.display().to_string();

        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo.to_string()).or_default();
            if entry.root_building.contains(&root_key) {
                return;
            }
            entry.root_building.insert(root_key.clone());
            if entry.status.state.is_empty() || entry.status.state == "idle" {
                entry.status.state = "ready".into();
                entry.status.phase = "on-demand".into();
            }
            true
        };
        if !should_spawn {
            return;
        }

        let inner = Arc::clone(&self.inner);
        let repo_key = repo.to_string();
        let ws_path = ws.to_path_buf();
        std::thread::spawn(move || {
            if crate::process_registry::is_shutdown_requested() {
                return;
            }
            let progress_inner = Arc::clone(&inner);
            let progress_repo = repo_key.clone();
            let progress: Box<dyn Fn(&str, usize) + Send> = Box::new(move |phase: &str, count: usize| {
                if let Ok(mut guard) = progress_inner.lock() {
                    if let Some(entry) = guard.get_mut(&progress_repo) {
                        entry.status.state = "running".into();
                        entry.status.phase = phase.to_string();
                        entry.status.symbol_count = count;
                    }
                }
            });
            let warm = classpath::warm_single_root_index(&ws_path, &root_for_index, Some(&progress));
            let mut guard = match inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(entry) = guard.get_mut(&repo_key) else {
                return;
            };
            entry.root_building.remove(&root_key);
            match warm {
                Ok(w) if w.symbol_count > 0 => {
                    entry.status = status_from_warm(&w, None);
                    if !w.index_complete {
                        drop(guard);
                        JavaIndexJobs {
                            inner: Arc::clone(&inner),
                        }
                        .spawn_background_index_for_root(&repo_key, &ws_path, &root_for_index);
                    }
                }
                Ok(_) => {
                    entry.status.phase = "on-demand".into();
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

    fn spawn_background_index_for_root(&self, repo_key: &str, ws: &Path, root: &Path) {
        let root_key = root
            .canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .display()
            .to_string();
        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo_key.to_string()).or_default();
            if entry.background_running {
                return;
            }
            entry.background_running = true;
            if entry.status.state != "running" {
                entry.status.state = "ready".into();
                entry.status.phase = "jar-index-background".into();
            }
            true
        };
        if !should_spawn {
            return;
        }

        let inner = Arc::clone(&self.inner);
        let repo_key = repo_key.to_string();
        let ws_path = ws.to_path_buf();
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            if crate::process_registry::is_shutdown_requested() {
                return;
            }
            let progress_inner = Arc::clone(&inner);
            let progress_repo = repo_key.clone();
            let progress: Box<dyn Fn(&str, usize) + Send> = Box::new(move |phase: &str, count: usize| {
                if let Ok(mut guard) = progress_inner.lock() {
                    if let Some(entry) = guard.get_mut(&progress_repo) {
                        entry.status.phase = phase.to_string();
                        entry.status.symbol_count = count;
                        entry.status.state = "ready".into();
                    }
                }
            });
            let jar_progress_inner = Arc::clone(&inner);
            let jar_progress_repo = repo_key.clone();
            let jar_progress: Box<dyn Fn(usize, usize) + Send> =
                Box::new(move |jars_indexed: usize, jars_total: usize| {
                    if let Ok(mut guard) = jar_progress_inner.lock() {
                        if let Some(entry) = guard.get_mut(&jar_progress_repo) {
                            entry.status.jars_indexed = jars_indexed;
                            entry.status.jars_total = jars_total;
                            entry.status.phase = "jar-index-background".into();
                            entry.status.state = "ready".into();
                        }
                    }
                });
            let _ = classpath::continue_background_index_root_pub(
                &ws_path,
                &root,
                Some(&progress),
                Some(&jar_progress),
            );
            let mut guard = match inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(entry) = guard.get_mut(&repo_key) else {
                return;
            };
            entry.background_running = false;
            entry.root_building.remove(&root_key);
            if let Ok(peek) = classpath::peek_index_status(&ws_path) {
                if peek.symbol_count > 0 {
                    entry.status = status_from_warm(&peek, None);
                }
            }
        });
    }

    /// Resolve Gradle/Maven compile classpath in the background without rebuilding the Java index.
    fn spawn_tooling_resolve(&self, repo_key: &str, ws: &Path) {
        let should_spawn = {
            let mut guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let entry = guard.entry(repo_key.to_string()).or_default();
            if entry.building || entry.tooling_running {
                return;
            }
            entry.tooling_running = true;
            true
        };
        if !should_spawn {
            return;
        }

        let inner = Arc::clone(&self.inner);
        let repo_key = repo_key.to_string();
        let ws_path = ws.to_path_buf();
        std::thread::spawn(move || {
            if crate::process_registry::is_shutdown_requested() {
                return;
            }
            tracing::info!(
                "Resolving compile classpath in background for {} (index kept)",
                ws_path.display()
            );
            let progress_inner = Arc::clone(&inner);
            let progress_repo = repo_key.clone();
            let progress: Box<dyn Fn(&str, usize) + Send> = Box::new(move |phase: &str, count: usize| {
                if let Ok(mut guard) = progress_inner.lock() {
                    if let Some(entry) = guard.get_mut(&progress_repo) {
                        if !entry.building {
                            entry.status.phase = phase.to_string();
                            entry.status.symbol_count = count;
                        }
                    }
                }
            });
            if let Err(e) = classpath::resolve_classpaths_for_index(&ws_path, Some(&progress)) {
                tracing::warn!(
                    "Background classpath resolve failed for {}: {e:#}",
                    ws_path.display()
                );
            }
            let mut guard = match inner.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(entry) = guard.get_mut(&repo_key) else {
                return;
            };
            entry.tooling_running = false;
            if entry.building {
                return;
            }
            if let Ok(peek) = classpath::peek_index_status(&ws_path) {
                if peek.symbol_count > 0 {
                    entry.status = status_from_warm(&peek, None);
                }
            }
        });
    }
}

fn status_from_warm(warm: &WarmIndexStatus, error: Option<String>) -> JavaIndexStatus {
    let ready = warm.symbol_count > 0;
    let phase = if ready && !warm.index_complete {
        "jar-index-background".into()
    } else if ready {
        "ready".into()
    } else {
        String::new()
    };
    JavaIndexStatus {
        state: if ready {
            "ready".into()
        } else if warm.indexed {
            "idle".into()
        } else {
            "idle".into()
        },
        phase,
        symbol_count: warm.symbol_count,
        dependency_jars: warm.dependency_jars,
        source_jars: warm.source_jars,
        jdk_sources: warm.jdk_sources,
        spring_symbols: warm.spring_symbols,
        jdk_symbols: warm.jdk_symbols,
        cached: warm.cached,
        error,
        jars_indexed: warm.jars_indexed,
        jars_total: warm.jars_total,
        index_complete: warm.index_complete,
    }
}
