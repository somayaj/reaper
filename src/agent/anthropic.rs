use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeBackend {
    Api,
    Bedrock,
}

impl ClaudeBackend {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "bedrock" => Self::Bedrock,
            _ => Self::Api,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Bedrock => "bedrock",
        }
    }
}

#[derive(Clone)]
pub struct AnthropicClient {
    backend: ClaudeBackend,
    api_key: Option<String>,
    bedrock_api_key: Option<String>,
    model: String,
    bedrock_region: String,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: Vec<ApiMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    anthropic_version: Option<&'a str>,
}

#[derive(Serialize)]
struct ApiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Option<Vec<ContentBlock>>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    delta: Option<StreamDelta>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
}

impl AnthropicClient {
    pub fn new_api(api_key: String, model: String) -> Self {
        Self {
            backend: ClaudeBackend::Api,
            api_key: Some(api_key),
            bedrock_api_key: None,
            model,
            bedrock_region: "us-east-1".into(),
            http: reqwest::Client::new(),
        }
    }

    pub fn new_bedrock(
        model: String,
        region: String,
        bedrock_api_key: Option<String>,
    ) -> Self {
        Self {
            backend: ClaudeBackend::Bedrock,
            api_key: None,
            bedrock_api_key,
            model,
            bedrock_region: region,
            http: reqwest::Client::new(),
        }
    }

    fn messages_from_history<'a>(
        history: &'a [(String, String)],
        prompt: &'a str,
    ) -> Vec<ApiMessage<'a>> {
        let mut messages: Vec<ApiMessage<'a>> = history
            .iter()
            .map(|(role, text)| ApiMessage {
                role: if role == "model" || role == "assistant" {
                    "assistant"
                } else {
                    "user"
                },
                content: text,
            })
            .collect();
        messages.push(ApiMessage {
            role: "user",
            content: prompt,
        });
        messages
    }

    fn extract_text(parsed: &MessagesResponse) -> Option<String> {
        let blocks = parsed.content.as_ref()?;
        let mut out = String::new();
        for block in blocks {
            if block.block_type.as_deref().unwrap_or("text") == "text" {
                if let Some(t) = &block.text {
                    out.push_str(t);
                }
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    pub async fn chat_with_history(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<String> {
        match self.backend {
            ClaudeBackend::Api => self.chat_api(system, history, prompt, false).await,
            ClaudeBackend::Bedrock if self.bedrock_api_key.is_some() => {
                self.chat_mantle(system, history, prompt, false).await
            }
            ClaudeBackend::Bedrock => self.chat_bedrock_runtime(system, history, prompt).await,
        }
    }

    pub async fn chat_stream_with_history(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<reqwest::Response> {
        match self.backend {
            ClaudeBackend::Api => self.stream_api(system, history, prompt).await,
            ClaudeBackend::Bedrock if self.bedrock_api_key.is_some() => {
                self.stream_mantle(system, history, prompt).await
            }
            ClaudeBackend::Bedrock => {
                bail!("bedrock IAM mode uses non-streaming chat; caller should use chat_with_history")
            }
        }
    }

    pub fn uses_streaming(&self) -> bool {
        match self.backend {
            ClaudeBackend::Api => true,
            ClaudeBackend::Bedrock => self.bedrock_api_key.is_some(),
        }
    }

    async fn chat_api(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
        stream: bool,
    ) -> Result<String> {
        let api_key = self
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Anthropic API key not set"))?;
        let messages = Self::messages_from_history(history, prompt);
        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 8192,
            system: Some(system),
            messages,
            stream: if stream { Some(true) } else { None },
            anthropic_version: None,
        };

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("anthropic request failed")?;

        let status = resp.status();
        let parsed: MessagesResponse = resp.json().await.context("parse anthropic response")?;
        if let Some(err) = parsed.error {
            bail!(
                "anthropic error ({}): {}",
                status,
                err.message
                    .or(err.error_type)
                    .unwrap_or_else(|| "unknown".into())
            );
        }
        Self::extract_text(&parsed).ok_or_else(|| anyhow::anyhow!("empty anthropic response"))
    }

    async fn stream_api(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<reqwest::Response> {
        let api_key = self
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Anthropic API key not set"))?;
        let messages = Self::messages_from_history(history, prompt);
        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 8192,
            system: Some(system),
            messages,
            stream: Some(true),
            anthropic_version: None,
        };

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("anthropic stream request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let parsed: MessagesResponse = resp.json().await.unwrap_or(MessagesResponse {
                content: None,
                error: Some(ApiError {
                    message: Some("anthropic stream failed".into()),
                    error_type: None,
                }),
            });
            if let Some(err) = parsed.error {
                bail!(
                    "anthropic error ({}): {}",
                    status,
                    err.message.unwrap_or_else(|| "unknown".into())
                );
            }
            bail!("anthropic stream failed ({status})");
        }
        Ok(resp)
    }

    fn mantle_url(&self) -> String {
        format!(
            "https://bedrock-mantle.{}.api.aws/anthropic/v1/messages",
            self.bedrock_region
        )
    }

    async fn chat_mantle(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
        stream: bool,
    ) -> Result<String> {
        let key = self
            .bedrock_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Bedrock API key not set"))?;
        let messages = Self::messages_from_history(history, prompt);
        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 8192,
            system: Some(system),
            messages,
            stream: if stream { Some(true) } else { None },
            anthropic_version: None,
        };

        let resp = self
            .http
            .post(self.mantle_url())
            .header("Authorization", format!("Bearer {key}"))
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("bedrock mantle request failed")?;

        let status = resp.status();
        let parsed: MessagesResponse = resp.json().await.context("parse bedrock mantle response")?;
        if let Some(err) = parsed.error {
            bail!(
                "bedrock mantle error ({}): {}",
                status,
                err.message.unwrap_or_else(|| "unknown".into())
            );
        }
        Self::extract_text(&parsed).ok_or_else(|| anyhow::anyhow!("empty bedrock mantle response"))
    }

    async fn stream_mantle(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<reqwest::Response> {
        let key = self
            .bedrock_api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Bedrock API key not set"))?;
        let messages = Self::messages_from_history(history, prompt);
        let body = MessagesRequest {
            model: &self.model,
            max_tokens: 8192,
            system: Some(system),
            messages,
            stream: Some(true),
            anthropic_version: None,
        };

        let resp = self
            .http
            .post(self.mantle_url())
            .header("Authorization", format!("Bearer {key}"))
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("bedrock mantle stream failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("bedrock mantle stream failed ({status}): {body}");
        }
        Ok(resp)
    }

    async fn chat_bedrock_runtime(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<String> {
        let region = self.bedrock_region.clone();
        let model_id = self.model.clone();
        let messages = Self::messages_from_history(history, prompt);
        let body = json!({
            "anthropic_version": "bedrock-2023-05-31",
            "max_tokens": 8192,
            "system": system,
            "messages": messages.iter().map(|m| json!({
                "role": m.role,
                "content": m.content,
            })).collect::<Vec<_>>(),
        });

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region))
            .load()
            .await;
        let client = aws_sdk_bedrockruntime::Client::new(&config);
        let resp = client
            .invoke_model()
            .model_id(model_id)
            .content_type("application/json")
            .accept("application/json")
            .body(aws_sdk_bedrockruntime::primitives::Blob::new(
                body.to_string().into_bytes(),
            ))
            .send()
            .await
            .context("bedrock invoke_model failed")?;

        let bytes = resp.body.as_ref();
        let parsed: MessagesResponse =
            serde_json::from_slice(bytes).context("parse bedrock invoke response")?;
        if let Some(err) = parsed.error {
            bail!(
                "bedrock error: {}",
                err.message.unwrap_or_else(|| "unknown".into())
            );
        }
        Self::extract_text(&parsed).ok_or_else(|| anyhow::anyhow!("empty bedrock response"))
    }

    pub async fn suggest_inline_completion(&self, context: &str) -> Result<String> {
        let system = "You are an IDE inline ghost-text completion engine.\n\
            The user sees text they already typed, then <CURSOR>, then your output as gray suggestion text.\n\
            Press Tab to accept. Predict the next character(s), token, expression, statement, or lines.\n\
            RULES:\n\
            - Output ONLY text to insert at <CURSOR>. Never repeat text before <CURSOR> on the current line.\n\
            - Match local naming, types, braces, quotes, and indentation.\n\
            - No markdown, code fences, XML tags, labels, explanations, or reasoning.\n\
            - If nothing sensible fits, output nothing.";
        self.chat_with_history(system, &[], context).await
    }

    pub async fn suggest_quick_fixes(&self, context: &str) -> Result<String> {
        let system = "You are an IDE quick-fix engine.\n\
            Given source code and compiler/linter errors, propose concrete text edits.\n\
            Return a JSON array of up to 5 quick fixes. Each element:\n\
            - title: short action label\n\
            - edits: array of { start_line, start_column, end_line, end_column, text }\n\
            Lines and columns are 1-based. text is the replacement for that range.\n\
            RULES:\n\
            - Prefer the smallest edit that fixes the error.\n\
            - Java: instance methods need a receiver (file.exists(), never bare exists()).\n\
            - Do not repeat unchanged file content — only edit regions.\n\
            - Code only in text fields — no markdown or explanations.\n\
            - If no safe fix, return [].";
        self.chat_with_history(system, &[], context).await
    }

    pub fn parse_stream_payload(payload: &str) -> Result<String, String> {
        let parsed: StreamEvent =
            serde_json::from_str(payload).map_err(|e| format!("parse stream chunk: {e}"))?;
        if let Some(err) = parsed.error {
            return Err(err.message.unwrap_or_else(|| "anthropic error".into()));
        }
        if parsed.event_type.as_deref() == Some("content_block_delta") {
            if let Some(delta) = parsed.delta {
                if delta.delta_type.as_deref().unwrap_or("text_delta") == "text_delta" {
                    return Ok(delta.text.unwrap_or_default());
                }
            }
        }
        Ok(String::new())
    }
}
