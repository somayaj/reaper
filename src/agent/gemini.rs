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
    #[serde(rename = "maxOutputTokens", skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(rename = "thinkingConfig", skip_serializing_if = "Option::is_none")]
    thinking_config: Option<ThinkingConfig>,
}

#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "thinkingBudget")]
    thinking_budget: i32,
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
    #[serde(default)]
    thought: Option<bool>,
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

    fn generation_config(
        temperature: f32,
        response_mime_type: &'static str,
        max_output_tokens: Option<u32>,
        disable_thinking: bool,
    ) -> GenerationConfig {
        GenerationConfig {
            temperature,
            response_mime_type,
            max_output_tokens,
            thinking_config: if disable_thinking {
                Some(ThinkingConfig {
                    thinking_budget: 0,
                })
            } else {
                None
            },
        }
    }

    fn extract_response_text(parsed: &GenerateResponse) -> Option<String> {
        let parts = parsed
            .candidates
            .as_ref()?
            .first()?
            .content
            .as_ref()?
            .parts
            .as_ref()?;
        let text = Self::extract_non_thought_text(parts);
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    fn extract_non_thought_text(parts: &[ResponsePart]) -> String {
        let mut chunks: Vec<String> = Vec::new();
        for part in parts {
            if part.thought.unwrap_or(false) {
                continue;
            }
            if let Some(t) = &part.text {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    chunks.push(t.clone());
                }
            }
        }
        strip_model_artifact_tags(&chunks.join(""))
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
            generation_config: Self::generation_config(0.15, "application/json", None, true),
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

        let text = Self::extract_response_text(&parsed)
            .ok_or_else(|| anyhow::anyhow!("empty gemini response"))?;

        Ok(text)
    }

    pub async fn suggest_commit_message(&self, context: &str) -> Result<String> {
        let system = "You write concise git commit messages. Respond with ONLY the commit message text: \
            a subject line (<=72 characters, imperative mood), optionally followed by a blank line and a short body. \
            No markdown, no code fences, no surrounding quotes.";

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents: vec![Content {
                role: "user",
                parts: vec![TextPart { text: context }],
            }],
            generation_config: Self::generation_config(0.2, "text/plain", None, true),
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

        let text = Self::extract_response_text(&parsed)
            .ok_or_else(|| anyhow::anyhow!("empty gemini response"))?;

        Ok(normalize_commit_message(&text))
    }

    pub async fn suggest_inline_completion(&self, context: &str) -> Result<String> {
        let system = "You are an IDE inline ghost-text completion engine.\n\
            The user sees text they already typed, then <CURSOR>, then your output as gray suggestion text.\n\
            Press Tab to accept. Predict the next character(s), token, expression, statement, or lines.\n\
            Infer intent from partial typing plus file context — works for every language and construct.\n\
            RULES:\n\
            - Output ONLY text to insert at <CURSOR>. Never repeat text before <CURSOR> on the current line.\n\
            - Complete ANY valid syntax: keywords, identifiers, operators, punctuation (; ) ] } ), strings, \
              calls, declarations, imports, HTML tags/attributes, CSS properties, SQL clauses, YAML keys, \
              shell commands, regex, comments, closing delimiters, and multi-line blocks.\n\
            - For control flow, offer full blocks when appropriate: while (cond) { body }, for loops, \
              if/else, switch/case, try/catch — indented to match the file.\n\
            - Prefer the smallest useful completion (even one character) when that finishes the token.\n\
            - Offer multiple lines only when clearly continuing a block, method body, or unfinished structure.\n\
            - Match local naming, types, braces, quotes, and indentation.\n\
            - Use newlines between lines; indent continuation lines consistently.\n\
            - No markdown, code fences, XML tags, labels, explanations, or reasoning.\n\
            - Never output thinking, analysis, or commentary — only insertable code/text.\n\
            - If nothing sensible fits, output nothing.";

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents: vec![Content {
                role: "user",
                parts: vec![TextPart { text: context }],
            }],
            generation_config: Self::generation_config(0.15, "text/plain", Some(1024), true),
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

        let text = Self::extract_response_text(&parsed).unwrap_or_default();

        Ok(text)
    }

    pub async fn suggest_autocomplete_items(&self, context: &str) -> Result<String> {
        let system = "You are an IDE code completion engine.\n\
            The user is typing code; <CURSOR> marks the insertion point.\n\
            Return a JSON array of up to 8 completion candidates that fit the partial code and file context.\n\
            Each element must be an object with:\n\
            - label: short display text (what appears in the menu)\n\
            - insert: exact text to insert at <CURSOR> (do not repeat text before <CURSOR> on the line)\n\
            - kind: one of keyword, method, field, variable, class, interface, snippet\n\
            - detail: optional one-line hint (type, purpose)\n\
            RULES:\n\
            - Suggest the next relevant tokens, expressions, statements, or snippets for what is being typed.\n\
            - Prefer completions that continue the current statement, block, call, or declaration.\n\
            - Match Java language level and local naming from context.\n\
            - Code only — no explanations, markdown, or prose.\n\
            - If nothing useful, return [].";

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents: vec![Content {
                role: "user",
                parts: vec![TextPart { text: context }],
            }],
            generation_config: Self::generation_config(0.2, "application/json", Some(512), true),
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

        let text = Self::extract_response_text(&parsed).unwrap_or_default();

        Ok(text)
    }

    pub async fn classify_java_run_target(&self, context: &str) -> Result<String> {
        let system = "You classify a Java source file for IDE Run (F5) behavior.\n\
            Return one JSON object with:\n\
            - class_type: spring-boot-app | spring-boot-test | junit-test | plain-main | quarkus-app | spring-component | library | interface | enum | record\n\
            - mode: none | test | spring-boot | main | project-task\n\
            - runnable: boolean\n\
            - qualified_name: optional fully qualified class name\n\
            - test_filter: optional Gradle/Maven test filter (ClassName or ClassName.method)\n\
            - task: optional build task/goal when mode is spring-boot or project-task (e.g. bootRun, spring-boot:run, quarkusDev)\n\
            - frameworks: array of strings (junit, spring-boot, spring-test, mockito, lombok, slf4j, quarkus, etc.)\n\
            - reason: optional short explanation when runnable is false\n\
            RULES:\n\
            - @SpringBootApplication + Spring Boot project context => spring-boot-app, mode spring-boot, runnable true\n\
            - @Test / @SpringBootTest => junit-test or spring-boot-test, mode test when project has build tool\n\
            - public static void main => plain-main, mode main, runnable true\n\
            - Spring @Component/@Service without main => not runnable\n\
            - Prefer heuristic hints when they match the source.\n\
            - JSON only, no markdown.";

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents: vec![Content {
                role: "user",
                parts: vec![TextPart { text: context }],
            }],
            generation_config: Self::generation_config(0.1, "application/json", Some(512), true),
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

        Ok(Self::extract_response_text(&parsed).unwrap_or_default())
    }

    pub async fn suggest_quick_fixes(&self, context: &str) -> Result<String> {
        let system = "You are an IDE quick-fix engine.\n\
            The user has compiler/linter errors in a source file. Propose concrete fixes they can apply.\n\
            Return a JSON array of up to 5 quick fixes. Each element:\n\
            - title: short menu label (e.g. \"Import java.util.Arrays\", \"Add missing semicolon\")\n\
            - edits: array of text edits to apply together (usually 1 edit; use multiple for import + change)\n\
            Each edit object:\n\
            - startLine, startColumn, endLine, endColumn: 1-based line/column (inclusive start, exclusive end column like the editor)\n\
            - text: exact replacement text for that range (use \\n for newlines)\n\
            RULES:\n\
            - Fix the reported errors using minimal correct edits.\n\
            - For missing imports, insert after package or at top of file.\n\
            - Do not repeat unchanged file content — only edit regions.\n\
            - Code only in text fields — no markdown or explanations.\n\
            - If no safe fix, return [].";

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents: vec![Content {
                role: "user",
                parts: vec![TextPart { text: context }],
            }],
            generation_config: Self::generation_config(0.15, "application/json", Some(2048), true),
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

        let text = Self::extract_response_text(&parsed).unwrap_or_default();

        Ok(text)
    }

    pub async fn chat_with_history(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let mut contents: Vec<Content<'_>> = history
            .iter()
            .map(|(role, text)| Content {
                role: if role == "model" { "model" } else { "user" },
                parts: vec![TextPart { text }],
            })
            .collect();
        contents.push(Content {
            role: "user",
            parts: vec![TextPart { text: prompt }],
        });

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents,
            generation_config: Self::generation_config(0.35, "text/plain", Some(8192), false),
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

        Self::extract_response_text(&parsed)
            .ok_or_else(|| anyhow::anyhow!("empty gemini response"))
    }

    pub async fn chat_stream_with_history(
        &self,
        system: &str,
        history: &[(String, String)],
        prompt: &str,
    ) -> Result<reqwest::Response> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}&alt=sse",
            self.model, self.api_key
        );

        let mut contents: Vec<Content<'_>> = history
            .iter()
            .map(|(role, text)| Content {
                role: if role == "model" { "model" } else { "user" },
                parts: vec![TextPart { text }],
            })
            .collect();
        contents.push(Content {
            role: "user",
            parts: vec![TextPart { text: prompt }],
        });

        let body = GenerateRequest {
            system_instruction: SystemInstruction {
                parts: vec![TextPart { text: system }],
            },
            contents,
            generation_config: Self::generation_config(0.35, "text/plain", Some(8192), false),
        };

        let resp = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("gemini stream request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let parsed: GenerateResponse = resp.json().await.unwrap_or(GenerateResponse {
                candidates: None,
                error: Some(GeminiError {
                    message: Some("gemini stream failed".into()),
                }),
            });
            if let Some(err) = parsed.error {
                bail!(
                    "gemini error ({}): {}",
                    status,
                    err.message.unwrap_or_else(|| "unknown".into())
                );
            }
            bail!("gemini stream failed ({status})");
        }

        Ok(resp)
    }

    pub fn parse_stream_payload(payload: &str) -> Result<String, String> {
        let parsed: GenerateResponse =
            serde_json::from_str(payload).map_err(|e| format!("parse stream chunk: {e}"))?;
        if let Some(err) = parsed.error {
            return Err(err.message.unwrap_or_else(|| "gemini error".into()));
        }
        Ok(Self::extract_stream_chunk_text(&parsed))
    }

    pub fn extract_stream_chunk_text(chunk: &GenerateResponse) -> String {
        let Some(parts) = chunk
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.as_ref())
        else {
            return String::new();
        };
        Self::extract_non_thought_text(parts)
    }
}

fn strip_between_tags(text: &str, open: &str, close: &str) -> String {
    let mut out = text.to_string();
    loop {
        let lower = out.to_lowercase();
        let start = match lower.find(open) {
            Some(s) => s,
            None => break,
        };
        let after = start + open.len();
        if let Some(rel) = lower[after..].find(close) {
            let end = after + rel + close.len();
            out.replace_range(start..end, "");
        } else {
            out.truncate(start);
            break;
        }
    }
    out
}

fn strip_model_artifact_tags(text: &str) -> String {
    let open_think = concat!("<", "think", ">");
    let close_think = concat!("<", "/", "think", ">");
    let open_rr = concat!("<", "redacted_reasoning", ">");
    let close_rr = concat!("<", "/", "redacted_reasoning", ">");
    let stripped = strip_between_tags(text, open_think, close_think);
    strip_between_tags(&stripped, open_rr, close_rr).trim().to_string()
}

fn normalize_commit_message(text: &str) -> String {
    let mut msg = text.trim().to_string();
    if msg.starts_with("```") {
        msg = msg
            .trim_start_matches('`')
            .trim_start_matches("text")
            .trim_start_matches('\n')
            .to_string();
        if let Some(end) = msg.rfind("```") {
            msg.truncate(end);
        }
        msg = msg.trim().to_string();
    }
    msg.trim_matches('"').trim().to_string()
}
