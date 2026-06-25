mod gemini;

pub use gemini::GeminiClient;

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::git::{self, GitOutput};
use crate::settings::SettingsStore;
use crate::workspace;

const MAX_COMMANDS: usize = 6;

#[derive(Debug, Deserialize)]
pub struct AgentRequest {
    pub prompt: String,
}

#[derive(Debug, Serialize)]
pub struct AgentStep {
    pub command: Vec<String>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub reply: String,
    pub steps: Vec<AgentStep>,
}

#[derive(Debug, Deserialize)]
struct PlannedAgentAction {
    reply: String,
    commands: Vec<Vec<String>>,
}

pub async fn run_git_agent(
    settings: &SettingsStore,
    ws: &Path,
    prompt: &str,
) -> Result<AgentResponse> {
    let api_key = settings
        .gemini_api_key()
        .ok_or_else(|| anyhow::anyhow!("Gemini API key not configured; set REAPER_GEMINI_API_KEY or add in Settings"))?;

    let status = workspace::workspace_status(ws).unwrap_or_else(|_| workspace::WorkspaceStatus {
        branch: "unknown".into(),
        clean: true,
        files: vec![],
        stdout: String::new(),
    });

    let context = format!(
        "Repository workspace context:\n- branch: {}\n- clean: {}\n- changed files: {}\n\nUser request:\n{}",
        status.branch,
        status.clean,
        if status.files.is_empty() {
            "none".into()
        } else {
            status
                .files
                .iter()
                .map(|f| format!("{} ({})", f.path, f.status))
                .collect::<Vec<_>>()
                .join(", ")
        },
        prompt.trim()
    );

    let client = GeminiClient::new(api_key, settings.gemini_model());
    let raw = client.plan_git_commands(&context).await?;
    let plan = parse_plan(&raw)?;

    if plan.commands.len() > MAX_COMMANDS {
        bail!("agent planned too many commands (max {MAX_COMMANDS})");
    }

    let mut steps = Vec::new();
    for args in plan.commands {
        if args.is_empty() {
            continue;
        }
        let out = git::run_workspace_command(ws, &string_args(&args))?;
        steps.push(AgentStep {
            command: args,
            stdout: out.stdout,
            stderr: out.stderr,
            exit_code: out.exit_code,
        });
    }

    Ok(AgentResponse {
        reply: plan.reply,
        steps,
    })
}

fn string_args(args: &[String]) -> Vec<String> {
    args.to_vec()
}

fn parse_plan(raw: &str) -> Result<PlannedAgentAction> {
    let trimmed = raw.trim();
    if let Ok(plan) = serde_json::from_str::<PlannedAgentAction>(trimmed) {
        return Ok(plan);
    }

    if let Some(json) = extract_json_block(trimmed) {
        if let Ok(plan) = serde_json::from_str::<PlannedAgentAction>(&json) {
            return Ok(plan);
        }
    }

    bail!("agent returned invalid JSON plan")
}

fn extract_json_block(text: &str) -> Option<String> {
    if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        if let Some(end) = rest.find("```") {
            return Some(rest[..end].trim().to_string());
        }
    }
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return Some(text[start..=end].to_string());
        }
    }
    None
}

pub fn allowed_commands_help() -> &'static str {
    "status, log, branch, show, diff, tag, remote, rev-parse, describe, shortlog, blame, ls-tree, cat-file, rev-list, name-rev, for-each-ref, add, commit, checkout, pull, push, fetch, merge, rebase, stash, reset, switch, restore, clean, mv, rm"
}
