use std::sync::Arc;

use crate::config::Config;
use crate::cursor::{CursorBridge, SessionStore};
use crate::settings::SettingsStore;
use crate::workspace::{JavaIndexJobs, ProjectIndexJobs};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub settings: SettingsStore,
    pub cursor_bridge: Arc<CursorBridge>,
    pub cursor_sessions: Arc<SessionStore>,
    pub java_index_jobs: Arc<JavaIndexJobs>,
    pub project_index_jobs: Arc<ProjectIndexJobs>,
}

impl AppState {
    pub fn new(config: Config, settings: SettingsStore) -> Self {
        let java_index_jobs = Arc::new(JavaIndexJobs::new());
        Self {
            config: Arc::new(config),
            settings,
            cursor_bridge: Arc::new(CursorBridge::new()),
            cursor_sessions: Arc::new(SessionStore::default()),
            project_index_jobs: Arc::new(ProjectIndexJobs::new(Arc::clone(&java_index_jobs))),
            java_index_jobs,
        }
    }
}
