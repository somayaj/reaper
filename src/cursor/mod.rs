use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};

mod spawn;

pub use spawn::{bridge_dir, ensure_bridge_running, last_bridge_error, reclaim_bridge_port, stop_bridge};

const DEFAULT_BRIDGE: &str = "http://127.0.0.1:8091";

pub fn bridge_url() -> String {
    std::env::var("REAPER_CURSOR_BRIDGE_URL").unwrap_or_else(|_| DEFAULT_BRIDGE.into())
}

#[derive(Default)]
pub struct SessionStore {
    inner: Mutex<HashMap<String, String>>,
}

impl SessionStore {
    pub fn get(&self, repo: &str) -> Option<String> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.get(repo).cloned())
    }

    pub fn set(&self, repo: &str, session_id: String) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert(repo.to_string(), session_id);
        }
    }

    pub fn remove(&self, repo: &str) -> Option<String> {
        self.inner.lock().ok()?.remove(repo)
    }

    pub fn drain_all(&self) -> Vec<(String, String)> {
        self.inner
            .lock()
            .ok()
            .map(|mut g| g.drain().collect())
            .unwrap_or_default()
    }
}

#[derive(Serialize)]
struct CreateSessionRequest<'a> {
    cwd: &'a str,
    #[serde(rename = "apiKey")]
    api_key: &'a str,
    model: &'a str,
    mode: &'a str,
}

#[derive(Deserialize)]
struct CreateSessionResponse {
    #[serde(rename = "sessionId")]
    session_id: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    prompt: &'a str,
    model: Option<&'a str>,
    mode: Option<&'a str>,
}

#[derive(Serialize)]
struct ListModelsRequest<'a> {
    #[serde(rename = "apiKey")]
    api_key: &'a str,
}

pub struct CursorBridge {
    client: Client,
    base: String,
}

impl CursorBridge {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base: bridge_url(),
        }
    }

    pub async fn health(&self) -> bool {
        self.client
            .get(format!("{}/health", self.base))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub async fn create_session(
        &self,
        cwd: &str,
        api_key: &str,
        model: &str,
        mode: &str,
    ) -> Result<String> {
        let resp = self
            .client
            .post(format!("{}/sessions", self.base))
            .json(&CreateSessionRequest {
                cwd,
                api_key,
                model,
                mode,
            })
            .send()
            .await
            .context("cursor bridge unreachable; ensure Node.js is installed and cursor-bridge starts with Reaper")?;

        if !resp.status().is_success() {
            let err: serde_json::Value = resp.json().await.unwrap_or_default();
            bail!(
                "{}",
                err.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("failed to create cursor session")
            );
        }

        let body: CreateSessionResponse = resp.json().await?;
        Ok(body.session_id)
    }

    pub async fn chat_stream(
        &self,
        session_id: &str,
        prompt: &str,
        model: Option<&str>,
        mode: Option<&str>,
    ) -> Result<reqwest::Response> {
        let resp = self
            .client
            .post(format!("{}/sessions/{session_id}/chat", self.base))
            .json(&ChatRequest {
                prompt,
                model,
                mode,
            })
            .send()
            .await
            .context("cursor bridge chat request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err: serde_json::Value = resp.json().await.unwrap_or_default();
            bail!(
                "{}",
                err.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&format!("cursor chat failed ({status})"))
            );
        }

        Ok(resp)
    }

    pub async fn list_models(&self, api_key: &str) -> Result<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}/models", self.base))
            .json(&ListModelsRequest { api_key })
            .send()
            .await
            .context("cursor bridge models request failed")?;

        if !resp.status().is_success() {
            let err: serde_json::Value = resp.json().await.unwrap_or_default();
            bail!(
                "{}",
                err.get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("failed to list cursor models")
            );
        }

        resp.json().await.context("invalid models response")
    }

    pub async fn stop_chat(&self, session_id: &str) -> Result<()> {
        let resp = self
            .client
            .post(format!("{}/sessions/{session_id}/stop", self.base))
            .send()
            .await
            .context("cursor bridge stop request failed")?;

        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }

        let err: serde_json::Value = resp.json().await.unwrap_or_default();
        bail!(
            "{}",
            err.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("failed to stop cursor agent")
        );
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let _ = self
            .client
            .delete(format!("{}/sessions/{session_id}", self.base))
            .send()
            .await;
        Ok(())
    }
}
