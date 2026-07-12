use std::sync::Arc;

use crate::config::Config;
use crate::cursor::{CursorBridge, SessionStore};
use crate::agent::{AnthropicChatStore, GeminiChatStore};
use crate::settings::SettingsStore;
use crate::ui_preferences::UiPreferencesStore;
use crate::workspace::{JavaIndexJobs, ProjectIndexJobs};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub settings: SettingsStore,
    pub ui_preferences: UiPreferencesStore,
    pub cursor_bridge: Arc<CursorBridge>,
    pub cursor_sessions: Arc<SessionStore>,
    pub gemini_chat_sessions: Arc<GeminiChatStore>,
    pub anthropic_chat_sessions: Arc<AnthropicChatStore>,
    pub java_index_jobs: Arc<JavaIndexJobs>,
    pub project_index_jobs: Arc<ProjectIndexJobs>,
}

impl AppState {
    pub fn new(config: Config, settings: SettingsStore, ui_preferences: UiPreferencesStore) -> Self {
        let java_index_jobs = Arc::new(JavaIndexJobs::new());
        Self {
            config: Arc::new(config),
            settings,
            ui_preferences,
            cursor_bridge: Arc::new(CursorBridge::new()),
            cursor_sessions: Arc::new(SessionStore::default()),
            gemini_chat_sessions: Arc::new(GeminiChatStore::default()),
            anthropic_chat_sessions: Arc::new(AnthropicChatStore::default()),
            project_index_jobs: Arc::new(ProjectIndexJobs::new(Arc::clone(&java_index_jobs))),
            java_index_jobs,
        }
    }
}
