mod gemini;
mod gemini_chat;

pub use gemini::GeminiClient;
pub use gemini_chat::{ChatTurn, GeminiChatStore};

use std::path::Path;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

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
        merge: workspace::conflict::MergeState {
            active: false,
            kind: None,
        },
        conflict_count: 0,
        ahead: 0,
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

pub async fn suggest_commit_message(
    settings: &SettingsStore,
    ws: &Path,
) -> Result<String> {
    let api_key = settings.gemini_api_key().ok_or_else(|| {
        anyhow::anyhow!(
            "Gemini API key not configured. Open Settings → AI or set REAPER_GEMINI_API_KEY."
        )
    })?;

    let status = workspace::workspace_status(ws)?;
    if status.clean {
        bail!("nothing to commit");
    }

    let diff = workspace::diff_for_commit(ws)?;
    let files_summary = status
        .files
        .iter()
        .map(|f| format!("{} ({})", f.path, f.status))
        .collect::<Vec<_>>()
        .join("\n");

    let context = format!(
        "Branch: {}\n\nChanged files:\n{}\n\nDiff:\n{}",
        status.branch,
        files_summary,
        truncate_for_prompt(&diff, 14_000),
    );

    let client = GeminiClient::new(api_key, settings.gemini_model());
    client.suggest_commit_message(&context).await
}

pub async fn suggest_inline_completion(
    settings: &SettingsStore,
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
) -> Result<String> {
    let prefer_ai =
        workspace::should_prefer_ai_statement_inline(path, line_prefix, content, line);
    let local = if prefer_ai {
        String::new()
    } else {
        apply_inline_fallback(ws, path, line, column, content, line_prefix)
    };
    if !local.is_empty() {
        return Ok(local);
    }

    if let Some(api_key) = settings.gemini_api_key() {
        let context =
            workspace::build_inline_completion_context(ws, path, line, column, content, line_prefix);
        let client = GeminiClient::new(api_key, settings.gemini_model());
        if let Ok(raw) = client.suggest_inline_completion(&context).await {
            let normalized = normalize_inline_suggestion(&raw, line_prefix);
            if !normalized.is_empty() {
                return Ok(normalized);
            }
            if !raw.trim().is_empty() {
                let loose = normalize_inline_suggestion_loose(&raw, line_prefix);
                if !loose.is_empty() {
                    return Ok(loose);
                }
            }
        }
    }

    Ok(String::new())
}

#[derive(Debug, Deserialize)]
struct AiCompletionRaw {
    label: String,
    #[serde(default)]
    insert: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    detail: Option<String>,
}

pub async fn suggest_ai_completions(
    settings: &SettingsStore,
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
    prefix: &str,
) -> Result<Vec<workspace::CompletionItem>> {
    let api_key = settings
        .gemini_api_key()
        .ok_or_else(|| anyhow::anyhow!("gemini not configured"))?;
    let mut context =
        workspace::build_inline_completion_context(ws, path, line, column, content, line_prefix);
    use std::fmt::Write;
    writeln!(context).ok();
    writeln!(
        context,
        "Autocomplete task: user partial token at <CURSOR>: \"{prefix}\""
    )
    .ok();
    writeln!(
        context,
        "Return JSON array of completions for what should be typed next at <CURSOR>."
    )
    .ok();

    let client = GeminiClient::new(api_key, settings.gemini_model());
    let raw = client.suggest_autocomplete_items(&context).await?;
    Ok(parse_ai_completion_items(&raw, line_prefix))
}

pub async fn suggest_run_target(
    settings: &SettingsStore,
    ws: &Path,
    path: &str,
    line: u32,
    content: &str,
    project: &workspace::RunProjectInfo,
    heuristic: &workspace::JavaRunTarget,
) -> Result<workspace::AiRunTargetHint> {
    let api_key = settings
        .gemini_api_key()
        .ok_or_else(|| anyhow::anyhow!("gemini not configured"))?;

    let mut context = String::new();
    use std::fmt::Write;
    writeln!(context, "File: {path}").ok();
    writeln!(context, "Cursor line: {line}").ok();
    writeln!(
        context,
        "Project: build_tool={} spring_boot={} root={}",
        project.build_tool, project.is_spring_boot, project.project_root
    )
    .ok();
    if !project.frameworks.is_empty() {
        writeln!(context, "Project frameworks: {}", project.frameworks.join(", ")).ok();
    }
    writeln!(
        context,
        "Heuristic: class_type={} mode={} runnable={}",
        heuristic.class_type, heuristic.mode, heuristic.runnable
    )
    .ok();
    if let Some(r) = &heuristic.reason {
        writeln!(context, "Heuristic reason: {r}").ok();
    }
    writeln!(context).ok();
    writeln!(context, "--- source ---").ok();
    let preview: String = content
        .lines()
        .take(220)
        .collect::<Vec<_>>()
        .join("\n");
    writeln!(context, "{preview}").ok();

    let client = GeminiClient::new(api_key, settings.gemini_model());
    let raw = client.classify_java_run_target(&context).await?;
    parse_ai_run_target_hint(&raw)
}

fn parse_ai_run_target_hint(raw: &str) -> Result<workspace::AiRunTargetHint> {
    let unfenced = strip_inline_code_fence(raw).trim();
    if let Ok(v) = serde_json::from_str::<workspace::AiRunTargetHint>(unfenced) {
        return Ok(v);
    }
    if let (Some(start), Some(end)) = (unfenced.find('{'), unfenced.rfind('}')) {
        if let Ok(v) = serde_json::from_str::<workspace::AiRunTargetHint>(&unfenced[start..=end]) {
            return Ok(v);
        }
    }
    bail!("could not parse AI run target response")
}

#[derive(Debug, Deserialize)]
struct AiQuickFixEditRaw {
    #[serde(rename = "startLine", alias = "start_line", default = "default_one")]
    start_line: u32,
    #[serde(rename = "startColumn", alias = "start_column", default = "default_one")]
    start_column: u32,
    #[serde(rename = "endLine", alias = "end_line", default = "default_one")]
    end_line: u32,
    #[serde(rename = "endColumn", alias = "end_column", default = "default_one")]
    end_column: u32,
    #[serde(default)]
    text: String,
}

fn default_one() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
struct AiQuickFixRaw {
    title: String,
    #[serde(default)]
    edits: Vec<AiQuickFixEditRaw>,
}

const QUICK_FIX_CURSOR_TIMEOUT: Duration = Duration::from_secs(12);

pub async fn suggest_quick_fixes(
    settings: &SettingsStore,
    ws: &Path,
    path: &str,
    content: &str,
    diagnostics: &[workspace::QuickFixDiagnostic],
    cursor_bridge: Option<&crate::cursor::CursorBridge>,
) -> Result<Vec<workspace::QuickFix>> {
    if diagnostics.is_empty() {
        return Ok(Vec::new());
    }

    let fixes = workspace::suggest_local_quick_fixes(ws, path, content, diagnostics)?;
    if !fixes.is_empty() {
        return Ok(fixes);
    }

    let context = build_quick_fix_context(path, content, diagnostics);
    let line_count = content.lines().count().max(1) as u32;

    // Gemini: one HTTP call — usually much faster than Cursor session + stream.
    if settings.gemini_api_key().is_some() {
        match suggest_quick_fixes_via_gemini(settings, content, &context, line_count).await {
            Ok(fixes) if !fixes.is_empty() => return Ok(fixes),
            Ok(_) => tracing::debug!("gemini quick fix returned no fixes"),
            Err(e) => tracing::warn!("gemini quick fix failed: {e:#}"),
        }
    }

    if let Some(bridge) = cursor_bridge {
        if settings.cursor_api_key().is_some() && bridge.health().await {
            let cursor = suggest_quick_fixes_via_cursor(
                settings,
                bridge,
                ws,
                content,
                &context,
                line_count,
            );
            match timeout(QUICK_FIX_CURSOR_TIMEOUT, cursor).await {
                Ok(Ok(fixes)) if !fixes.is_empty() => return Ok(fixes),
                Ok(Ok(_)) => tracing::debug!("cursor quick fix returned no fixes"),
                Ok(Err(e)) => tracing::warn!("cursor quick fix failed: {e:#}"),
                Err(_) => tracing::warn!("cursor quick fix timed out after {:?}", QUICK_FIX_CURSOR_TIMEOUT),
            }
        }
    }

    Ok(Vec::new())
}

fn quick_fix_line_len(content: &str, line: u32) -> u32 {
    content
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .map(|l| l.len().max(1) as u32)
        .unwrap_or(1)
}

fn build_quick_fix_context(
    path: &str,
    content: &str,
    diagnostics: &[workspace::QuickFixDiagnostic],
) -> String {
    let lang = workspace::language_for_path(path).unwrap_or("plaintext");
    let mut out = String::new();
    use std::fmt::Write;
    writeln!(out, "File: {path}").ok();
    writeln!(out, "Language: {lang}").ok();

    if path.ends_with(".java") || path.ends_with(".kt") || path.ends_with(".kts") {
        for line in content.lines().take(40) {
            let trimmed = line.trim();
            if trimmed.starts_with("package ") {
                writeln!(out, "Package: {trimmed}").ok();
                break;
            }
        }
        let imports: Vec<&str> = content
            .lines()
            .filter(|l| l.trim_start().starts_with("import "))
            .take(48)
            .collect();
        if !imports.is_empty() {
            writeln!(out, "Imports:").ok();
            for imp in imports {
                writeln!(out, "  {imp}").ok();
            }
        }
    }

    writeln!(out, "\n--- Code ---").ok();
    out.push_str(&quick_fix_numbered_snippet(content, diagnostics));

    writeln!(out, "\n--- Errors to fix ---").ok();
    for d in diagnostics.iter().take(8) {
        writeln!(
            out,
            "  line {} col {} [{}]: {}",
            d.line,
            d.column.max(1),
            if d.severity.is_empty() {
                "error"
            } else {
                &d.severity
            },
            d.message.trim()
        )
        .ok();
    }
    writeln!(out).ok();
    writeln!(
        out,
        "Return JSON quick fixes that resolve the errors above. Minimal edits only."
    )
    .ok();

    out
}

fn quick_fix_numbered_snippet(
    content: &str,
    diagnostics: &[workspace::QuickFixDiagnostic],
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let focus_idx = diagnostics[0].line.saturating_sub(1) as usize;
    let mut min_idx = focus_idx.min(lines.len() - 1);
    let mut max_idx = min_idx;
    for d in diagnostics.iter().take(8) {
        let i = d.line.saturating_sub(1) as usize;
        if i < lines.len() {
            min_idx = min_idx.min(i);
            max_idx = max_idx.max(i);
        }
    }
    let start = min_idx.saturating_sub(10);
    let end = (max_idx + 10).min(lines.len() - 1);
    let mut snippet = String::new();
    use std::fmt::Write;
    for i in start..=end {
        writeln!(snippet, "{:4}| {}", i + 1, lines[i]).ok();
    }
    snippet
}

const QUICK_FIX_SYSTEM: &str = "You are an IDE quick-fix engine.\n\
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

async fn suggest_quick_fixes_via_cursor(
    settings: &SettingsStore,
    bridge: &crate::cursor::CursorBridge,
    ws: &Path,
    content: &str,
    context: &str,
    line_count: u32,
) -> Result<Vec<workspace::QuickFix>> {
    let api_key = settings
        .cursor_api_key()
        .ok_or_else(|| anyhow::anyhow!("cursor not configured"))?;
    let model = settings.cursor_model();
    let cwd = ws
        .canonicalize()
        .unwrap_or_else(|_| ws.to_path_buf())
        .display()
        .to_string();
    let session_id = bridge
        .create_session(&cwd, &api_key, &model, "ask")
        .await?;
    let prompt = format!("{QUICK_FIX_SYSTEM}\n\n{context}");
    let raw = match bridge
        .chat_collect(&session_id, &prompt, Some(model.as_str()), Some("ask"))
        .await
    {
        Ok(text) => text,
        Err(e) => {
            let _ = bridge.delete_session(&session_id).await;
            return Err(e);
        }
    };
    let _ = bridge.delete_session(&session_id).await;
    Ok(tag_quick_fixes(
        parse_ai_quick_fixes(&raw, line_count, |line| quick_fix_line_len(content, line)),
        "cursor",
    ))
}

async fn suggest_quick_fixes_via_gemini(
    settings: &SettingsStore,
    content: &str,
    context: &str,
    line_count: u32,
) -> Result<Vec<workspace::QuickFix>> {
    let api_key = settings
        .gemini_api_key()
        .ok_or_else(|| anyhow::anyhow!("gemini not configured"))?;

    let client = GeminiClient::new(api_key, settings.gemini_model());
    let raw = client.suggest_quick_fixes(context).await?;
    Ok(tag_quick_fixes(
        parse_ai_quick_fixes(&raw, line_count, |line| quick_fix_line_len(content, line)),
        "gemini",
    ))
}

fn tag_quick_fixes(mut fixes: Vec<workspace::QuickFix>, provider: &str) -> Vec<workspace::QuickFix> {
    for fix in &mut fixes {
        fix.provider = Some(provider.to_string());
    }
    fixes
}

fn parse_ai_quick_fixes(
    raw: &str,
    line_count: u32,
    line_len: impl Fn(u32) -> u32,
) -> Vec<workspace::QuickFix> {
    let unfenced = strip_inline_code_fence(raw).trim();
    let parsed: Result<Vec<AiQuickFixRaw>, _> = serde_json::from_str(unfenced);
    let items = if let Ok(v) = parsed {
        v
    } else if let (Some(start), Some(end)) = (unfenced.find('['), unfenced.rfind(']')) {
        serde_json::from_str(&unfenced[start..=end]).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut out = Vec::new();
    for item in items {
        let title = item.title.trim();
        if title.is_empty() || title.len() > 160 || ai_completion_looks_like_prose(title) {
            continue;
        }
        let mut edits = Vec::new();
        for e in item.edits {
            if e.text.len() > 8000 {
                continue;
            }
            let mut edit = workspace::QuickFixEdit {
                start_line: e.start_line.max(1),
                start_column: e.start_column.max(1),
                end_line: e.end_line.max(1),
                end_column: e.end_column.max(1),
                text: e.text,
            };
            edit.clamp_to_document(line_count, &line_len);
            edits.push(edit);
        }
        if edits.is_empty() {
            continue;
        }
        out.push(workspace::QuickFix {
            title: title.to_string(),
            edits,
            provider: None,
        });
        if out.len() >= 6 {
            break;
        }
    }
    out
}

fn parse_ai_completion_items(raw: &str, line_prefix: &str) -> Vec<workspace::CompletionItem> {
    let unfenced = strip_inline_code_fence(raw).trim();
    let parsed: Result<Vec<AiCompletionRaw>, _> = serde_json::from_str(unfenced);
    let items = if let Ok(v) = parsed {
        v
    } else if let (Some(start), Some(end)) = (unfenced.find('['), unfenced.rfind(']')) {
        serde_json::from_str(&unfenced[start..=end]).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut out = Vec::new();
    for item in items {
        let label = item.label.trim();
        if label.is_empty() || label.len() > 120 || ai_completion_looks_like_prose(label) {
            continue;
        }
        let insert = item
            .insert
            .filter(|s| !s.trim().is_empty())
            .map(|s| normalize_inline_suggestion(&s, line_prefix))
            .filter(|s| !s.is_empty())
            .or_else(|| Some(label.to_string()));
        let detail = item
            .detail
            .filter(|s| !s.trim().is_empty())
            .map(|s| format!("AI · {}", s.trim()))
            .or_else(|| Some("AI suggestion".to_string()));
        let kind = item
            .kind
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "snippet".to_string());
        out.push(workspace::CompletionItem {
            label: label.to_string(),
            kind,
            detail,
            insert,
            path: None,
            line: None,
            column: None,
            documentation: None,
        });
        if out.len() >= 12 {
            break;
        }
    }
    out
}

fn ai_completion_looks_like_prose(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("Here")
        || t.starts_with("The ")
        || t.starts_with("This ")
        || t.starts_with("You ")
        || (t.len() > 64 && t.contains(' ') && !t.contains('(') && !t.contains(';'))
}

fn apply_inline_fallback(
    ws: &Path,
    path: &str,
    line: u32,
    column: u32,
    content: &str,
    line_prefix: &str,
) -> String {
    if let Some(fallback) =
        workspace::inline_completion_fallback(ws, path, line, column, content, line_prefix)
    {
        let from_index = normalize_inline_suggestion(&fallback, line_prefix);
        if !from_index.is_empty() {
            return from_index;
        }
        if fallback.len() <= MAX_INLINE_CHARS {
            return fallback;
        }
    }
    String::new()
}

fn strip_inline_code_fence(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return text;
    }
    let inner = trimmed.strip_prefix("```").unwrap_or(trimmed).trim_start();
    let lang_end = inner.find('\n');
    let body = if let Some(idx) = lang_end {
        inner[idx + 1..].trim()
    } else {
        inner
    };
    if let Some(end) = body.rfind("```") {
        body[..end].trim()
    } else {
        body
    }
}

fn inline_looks_like_explanation(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("Here")
        || t.starts_with("The ")
        || t.starts_with("This ")
        || t.starts_with("I ")
        || t.starts_with("Note:")
        || t.starts_with("Sure")
        || t.starts_with("Output:")
        || t.starts_with("Completion:")
        || t.starts_with("Suggestion:")
        || t.starts_with("Thinking:")
        || t.starts_with("Thought:")
        || t.to_lowercase().starts_with("thinking")
        || inline_looks_like_prose(t)
}

fn inline_looks_like_prose(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.contains('(')
        || t.contains('{')
        || t.contains('}')
        || t.contains(';')
        || t.contains('=')
        || t.contains('<')
        || t.contains('>')
        || t.starts_with("//")
        || t.starts_with("/*")
        || t.starts_with("#")
    {
        return false;
    }
    // Long sentence-like text without code punctuation.
    if t.len() > 48 && t.contains(' ') {
        return true;
    }
    if t.ends_with('.') && !t.contains(')') {
        return true;
    }
    false
}

const MAX_INLINE_LINES: usize = 15;
const MAX_INLINE_CHARS: usize = 2500;

fn line_indent(prefix: &str) -> String {
    prefix
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn cap_inline_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let capped = if lines.len() > MAX_INLINE_LINES {
        lines[..MAX_INLINE_LINES].join("\n")
    } else {
        lines.join("\n")
    };
    if capped.len() > MAX_INLINE_CHARS {
        capped[..MAX_INLINE_CHARS].trim_end().to_string()
    } else {
        capped.trim_end().to_string()
    }
}

fn apply_indent_to_continuation_lines(lines: &mut [String], base_indent: &str) {
    for line in lines.iter_mut().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            *line = format!("{base_indent}{line}");
        }
    }
}

fn normalize_inline_suggestion(suggestion: &str, line_prefix: &str) -> String {
    if suggestion.trim().is_empty() {
        return String::new();
    }

    let lower = suggestion.to_lowercase();
    if lower.contains("<cursor>") || suggestion.contains(">>>") {
        return String::new();
    }

    let unfenced = strip_inline_code_fence(suggestion);
    let capped = cap_inline_lines(unfenced.trim());
    if capped.trim().is_empty() {
        return String::new();
    }

    let mut lines: Vec<String> = capped.lines().map(|l| l.to_string()).collect();
    if lines.is_empty() {
        return String::new();
    }

    if (lines[0].starts_with('"') && lines[0].ends_with('"') && lines[0].len() > 1)
        || (lines[0].starts_with('\'') && lines[0].ends_with('\'') && lines[0].len() > 1)
    {
        lines[0] = lines[0][1..lines[0].len() - 1].to_string();
    }

    if inline_looks_like_explanation(lines[0].trim_start()) {
        return String::new();
    }

    lines[0] = strip_overlap_prefix(line_prefix, &lines[0]);
    if lines[0].trim().is_empty() && lines.len() == 1 {
        return String::new();
    }

    apply_indent_to_continuation_lines(&mut lines, &line_indent(line_prefix));

    let out = lines.join("\n");
    if out.trim().is_empty() {
        return String::new();
    }
    if out == line_prefix || out.trim() == line_prefix.trim() {
        return String::new();
    }
    out
}

fn normalize_inline_suggestion_loose(suggestion: &str, line_prefix: &str) -> String {
    if suggestion.trim().is_empty() {
        return String::new();
    }
    let unfenced = strip_inline_code_fence(suggestion);
    let capped = cap_inline_lines(unfenced.trim());
    if capped.trim().is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = capped.lines().map(|l| l.to_string()).collect();
    lines[0] = strip_overlap_prefix(line_prefix, &lines[0]);
    if lines[0].trim().is_empty() {
        return String::new();
    }
    apply_indent_to_continuation_lines(&mut lines, &line_indent(line_prefix));
    lines.join("\n").trim_end().to_string()
}

fn strip_overlap_prefix(line_prefix: &str, suggestion: &str) -> String {
    if suggestion.is_empty() {
        return String::new();
    }
    if line_prefix.ends_with(suggestion) || suggestion == line_prefix {
        return String::new();
    }
    for len in (1..=line_prefix.len().min(suggestion.len())).rev() {
        let tail = &line_prefix[line_prefix.len() - len..];
        if suggestion.starts_with(tail) {
            return suggestion[tail.len()..].to_string();
        }
    }
    suggestion.to_string()
}

fn truncate_for_prompt(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    format!(
        "{}…\n\n[diff truncated — {} bytes total]",
        &text[..max],
        text.len()
    )
}

#[cfg(test)]
mod inline_tests {
    use super::*;

    #[test]
    fn rejects_explanations_and_markers() {
        assert_eq!(
            normalize_inline_suggestion("Here is the completion: foo", "    "),
            ""
        );
        assert_eq!(
            normalize_inline_suggestion(">>> foo<CURSOR>", "    "),
            ""
        );
        assert_eq!(normalize_inline_suggestion("```java\nfoo()\n```", "foo("), ")");
    }

    #[test]
    fn strips_overlap_and_multiline() {
        assert_eq!(
            normalize_inline_suggestion("name)\nextra line", "foo("),
            "name)\nextra line"
        );
        assert_eq!(
            normalize_inline_suggestion("foo(bar", "    foo("),
            "bar"
        );
        let multi = normalize_inline_suggestion("urn null;\n        log.info(\"ok\");", "        ret");
        assert!(multi.contains("urn null"));
        assert!(multi.contains("log.info"));
    }
}
