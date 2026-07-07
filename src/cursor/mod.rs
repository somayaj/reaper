use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};

mod spawn;

pub use spawn::{ensure_bridge_running, last_bridge_error, reclaim_bridge_port, stop_bridge};

static BRIDGE_URL: RwLock<Option<String>> = RwLock::new(None);

pub fn set_bridge_url(url: String) {
    if let Ok(mut guard) = BRIDGE_URL.write() {
        *guard = Some(url);
    }
}

pub fn bridge_url() -> String {
    std::env::var("REAPER_CURSOR_BRIDGE_URL")
        .ok()
        .or_else(|| BRIDGE_URL.read().ok().and_then(|guard| guard.clone()))
        .unwrap_or_else(|| load_saved_bridge_url().unwrap_or_else(|| "http://127.0.0.1:8091".into()))
}

fn bridge_port_file() -> std::path::PathBuf {
    crate::config::Config::resolve_data_dir().join("cursor-bridge.port")
}

pub fn load_saved_bridge_url() -> Option<String> {
    let port = std::fs::read_to_string(bridge_port_file()).ok()?;
    let port = port.trim().parse::<u16>().ok()?;
    let host = std::env::var("REAPER_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    Some(format!("http://{host}:{port}"))
}

pub fn save_bridge_port(port: u16) {
    let path = bridge_port_file();
    if let Err(e) = std::fs::write(&path, port.to_string()) {
        tracing::warn!("Could not write {}: {e}", path.display());
    }
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
}

struct HealthCache {
    ok: bool,
    checked_at: Instant,
}

static HEALTH_CACHE: Mutex<Option<HealthCache>> = Mutex::new(None);

pub fn invalidate_health_cache() {
    if let Ok(mut guard) = HEALTH_CACHE.lock() {
        *guard = None;
    }
}

impl CursorBridge {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn base(&self) -> String {
        bridge_url()
    }

    pub async fn health(&self) -> bool {
        let ok = self
            .client
            .get(format!("{}/health", self.base()))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        if let Ok(mut guard) = HEALTH_CACHE.lock() {
            *guard = Some(HealthCache {
                ok,
                checked_at: Instant::now(),
            });
        }
        ok
    }

    pub async fn health_cached(&self, max_age: Duration) -> bool {
        if let Ok(guard) = HEALTH_CACHE.lock() {
            if let Some(ref cache) = *guard {
                if cache.checked_at.elapsed() < max_age {
                    return cache.ok;
                }
            }
        }
        self.health().await
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
            .post(format!("{}/sessions", self.base()))
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
            .post(format!("{}/sessions/{session_id}/chat", self.base()))
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
            .post(format!("{}/models", self.base()))
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

    pub async fn validate_model(&self, api_key: &str, model: &str) -> Result<()> {
        let value = self.list_models(api_key).await?;
        let models = value
            .get("models")
            .and_then(|m| m.as_array())
            .context("failed to list cursor models")?;
        let supported = models.iter().any(|entry| {
            entry.get("id").and_then(|v| v.as_str()) == Some(model)
        });
        if !supported {
            bail!(
                "Model \"{model}\" isn't available for your Cursor API key. Choose a supported model in the agent panel."
            );
        }
        Ok(())
    }

    pub async fn stop_chat(&self, session_id: &str) -> Result<()> {
        let resp = self
            .client
            .post(format!("{}/sessions/{session_id}/stop", self.base()))
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

    pub async fn chat_collect(
        &self,
        session_id: &str,
        prompt: &str,
        model: Option<&str>,
        mode: Option<&str>,
    ) -> Result<String> {
        use futures_util::StreamExt;

        let resp = self
            .chat_stream(session_id, prompt, model, mode)
            .await?;
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut text = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("cursor chat stream read failed")?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buf.find("\n\n") {
                let block = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();
                for line in block.lines() {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let payload = line["data:".len()..].trim();
                    if payload.is_empty() || payload == "[DONE]" {
                        continue;
                    }
                    let v: serde_json::Value =
                        serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
                    match v.get("type").and_then(|t| t.as_str()) {
                        Some("text") => {
                            if let Some(t) = v.get("text").and_then(|t| t.as_str()) {
                                text.push_str(t);
                            }
                        }
                        Some("error") => {
                            let msg = v
                                .get("error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("cursor chat error");
                            bail!("{msg}");
                        }
                        _ => {}
                    }
                }
            }
        }

        if text.trim().is_empty() {
            bail!("empty cursor response");
        }
        Ok(text)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let _ = self
            .client
            .delete(format!("{}/sessions/{session_id}", self.base()))
            .send()
            .await;
        Ok(())
    }
}
