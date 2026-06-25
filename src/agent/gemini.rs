use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct GeminiClient {
    api_key: String,
    model: String,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    #[serde(rename = "systemInstruction")]
    system_instruction: SystemInstruction<'a>,
    contents: Vec<Content<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GenerationConfig,
}

#[derive(Serialize)]
struct SystemInstruction<'a> {
    parts: Vec<TextPart<'a>>,
}

#[derive(Serialize)]
struct Content<'a> {
    role: &'a str,
    parts: Vec<TextPart<'a>>,
}

#[derive(Serialize)]
struct TextPart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GenerationConfig {
    temperature: f32,
    #[serde(rename = "responseMimeType")]
    response_mime_type: &'static str,
}

#[derive(Deserialize)]
struct GenerateResponse {
    candidates: Option<Vec<Candidate>>,
    error: Option<GeminiError>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<CandidateContent>,
}

#[derive(Deserialize)]
struct CandidateContent {
    parts: Option<Vec<ResponsePart>>,
}

#[derive(Deserialize)]
struct ResponsePart {
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiError {
    message: Option<String>,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            http: reqwest::Client::new(),
        }
    }

    pub async fn plan_git_commands(&self, user_prompt: &str) -> Result<String> {
        let system = format!(
            "You are Reaper, a git operations agent. Given a user request and repo context, respond ONLY with JSON (no markdown) in this shape:\n\
            {{\"reply\":\"short explanation for the user\",\"commands\":[[\"git\",\"subcommand\",\"...\"], ...]}}\n\
            Rules:\n\
            - Each command is an array of git arguments WITHOUT the word git (example: [\"status\", \"--short\"]).\n\
            - Use at most 6 commands.\n\
            - Only use allowed subcommands: {}.\n\
            - Never run destructive commands like reset --hard unless explicitly requested.\n\
            - Prefer read-only commands (status, log, diff, branch) when the user is asking questions.\n\
            - For commits, use add then commit with -m; only push if user asks to push.",
            crate::agent::allowed_commands_help()
        );

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: &system }],
            },
            contents: vec![Content {
                role: "user",
                parts: vec![TextPart { text: user_prompt }],
            }],
            generation_config: GenerationConfig {
                temperature: 0.15,
                response_mime_type: "application/json",
            },
        };

        let resp = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("gemini request failed")?;

        let status = resp.status();
        let parsed: GenerateResponse = resp.json().await.context("parse gemini response")?;

        if let Some(err) = parsed.error {
            bail!(
                "gemini error ({}): {}",
                status,
                err.message.unwrap_or_else(|| "unknown".into())
            );
        }

        let text = parsed
            .candidates
            .and_then(|c| c.into_iter().next())
            .and_then(|c| c.content)
            .and_then(|c| c.parts)
            .and_then(|p| p.into_iter().next())
            .and_then(|p| p.text)
            .ok_or_else(|| anyhow::anyhow!("empty gemini response"))?;

        Ok(text)
    }
}
