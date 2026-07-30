const AGENT_DOCK_KEY = 'reaper-agent-dock';
const AGENT_RIGHT_WIDTH_KEY = 'reaper-agent-right-w';
const AGENT_BOTTOM_HEIGHT_KEY = 'reaper-agent-bottom-h';
const AGENT_PROVIDER_KEY = 'reaper-agent-provider';
const TERMINAL_DOCK_KEY = 'reaper-terminal-dock';
const TERMINAL_BOTTOM_HEIGHT_KEY = 'reaper-terminal-bottom-height';
const DOCKER_LOGS_DOCK_KEY = 'reaper-docker-logs-dock';
const DOCKER_LOGS_RIGHT_WIDTH_KEY = 'reaper-docker-logs-right-w';
const DOCKER_LOGS_BOTTOM_HEIGHT_KEY = 'reaper-docker-logs-bottom-h';
const BUILD_TASKS_DOCK_KEY = 'reaper-build-tasks-dock';
const BUILD_TASKS_WIDTH_KEY = 'reaper-build-tasks-w';
const PACKAGE_MANIFEST_DOCK_KEY = 'reaper-package-manifest-dock';
const PACKAGE_MANIFEST_WIDTH_KEY = 'reaper-package-manifest-w';
const STRUCTURE_MODE_KEY = 'reaper-structure-mode';
const STRUCTURE_REFRESH_DELAY_MS = 350;
const AST_LANG_EXTS = new Set([
  'java', 'py', 'pyw', 'js', 'mjs', 'cjs', 'jsx', 'ts', 'tsx',
  'go', 'rs', 'c', 'h', 'cpp', 'cc', 'cxx', 'hpp', 'hh',
  'json', 'jsonc', 'yml', 'yaml',
]);
const DB_VIEWER_RIGHT_WIDTH_KEY = 'reaper-db-viewer-right-w';
const DB_VIEWER_SCHEMA_RAIL_WIDTH_KEY = 'reaper-db-schema-rail-w';
const GIT_VIEWER_RIGHT_WIDTH_KEY = 'reaper-git-viewer-right-w';
const EDITOR_FONT_SIZE_KEY = 'reaper-editor-font-size';
const EDITOR_FONT_FAMILY_KEY = 'reaper-editor-font-family';
const AGENT_FONT_SIZE_KEY = 'reaper-agent-font-size';
const AGENT_FONT_FAMILY_KEY = 'reaper-agent-font-family';
const AGENT_FONT_MATCH_EDITOR_KEY = 'reaper-agent-font-match-editor';
const AUTO_SAVE_KEY = 'reaper-auto-save';
const AI_INLINE_COMPLETE_KEY = 'reaper-ai-inline-complete';
const SHOW_DOTFILES_KEY = 'reaper-show-dotfiles';
const NEW_WINDOW_ON_REPO_KEY = 'reaper-new-window-on-repo';
const AUTO_SAVE_DELAY_MS = 2000;
/** Java edits often pause mid-statement; wait longer before persisting. */
const JAVA_AUTO_SAVE_DELAY_MS = 3500;
/** Re-check soon when auto-save is deferred for incomplete syntax. */
const AUTO_SAVE_INCOMPLETE_RETRY_MS = 600;
const AUTO_SAVE_CODE_EXTS = new Set([
  'java', 'kt', 'kts', 'scala', 'js', 'jsx', 'ts', 'tsx', 'mjs', 'cjs',
]);
const SAVE_GATE_DRAIN_MS = 100;
/** Coalesce queued javac before starting (typing + tab switch). */
const JAVA_QUEUE_DIAG_DELAY_MS = 50;
const JAVA_FULL_DIAG_MAX_RETRIES = 3;
const DIAG_DELAY_MS = 200;
const JAVA_DIAG_DELAY_MS = 700;
const ALL_JAVA_DIAG_DELAY_MS = 3000;
const JAVA_DIAG_FULL_STAGGER_MS = 500;
const CONFIG_DIAG_DELAY_MS = 900;
const BUILD_DIAG_DELAY_MS = 1400;
const TAB_RENDER_DELAY_MS = 200;
const EDITOR_CONTENT_SYNC_DELAY_MS = 120;
const TEST_DECOR_DELAY_MS = 200;
const COVERAGE_CLEAR_DELAY_MS = 250;
const PROJECT_RELOAD_DELAY_MS = 2000;
const PROJECT_BUILD_RELOAD_DELAY_MS = 3000;
const PROJECT_INDEX_POLL_MS = 750;
const PROJECT_INDEX_POLL_BACKGROUND_MS = 5000;
const PROJECT_AUTO_REFRESH_MAX = 3;

/** xterm ANSI styling for streamed command output */
const TERM_ESC = {
  reset: '\x1b[0m',
  dim: '\x1b[90m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  brightRed: '\x1b[91m',
  brightGreen: '\x1b[92m',
  brightYellow: '\x1b[93m',
  brightBlue: '\x1b[94m',
  brightCyan: '\x1b[96m',
};

const GRADLE_NOOP_TASK_RE = /^> Task :?\S+.*\b(UP-TO-DATE|FROM-CACHE|SKIPPED|NO-SOURCE)\b/i;

function isGradleNoopTaskLine(line) {
  return GRADLE_NOOP_TASK_RE.test(String(line || '').trimEnd());
}

function flushNoopTaskSummary(term, { into = null } = {}) {
  const count = term.streamNoopTaskCount || 0;
  if (count <= 0) return '';
  term.streamNoopTaskCount = 0;
  const sample = term.streamNoopTaskSample || '';
  term.streamNoopTaskSample = '';
  let line = '';
  if (count === 1) {
    line = `${colorizeStreamLine(sample)}\n`;
  } else {
    line = `\n${TERM_ESC.dim}  … ${count} tasks up-to-date${TERM_ESC.reset}\n`;
  }
  if (into != null) return into + line;
  if (term?.xterm) term.xterm.write(line.replace(/\n/g, '\r\n'));
  return '';
}

function resetStreamCollapseState(term) {
  if (!term) return;
  term.streamNoopTaskCount = 0;
  term.streamNoopTaskSample = '';
}

function termRule(width = 52) {
  return `${TERM_ESC.dim}${'─'.repeat(width)}${TERM_ESC.reset}`;
}

function formatGradleTaskLine(line) {
  const m = line.match(/^(\s*> Task )(:?\S+)(.*)$/);
  if (!m) return null;
  const [, prefix, taskPath, rest] = m;
  const status = rest.trim();
  let statusStyle = TERM_ESC.dim;
  if (/\bFAILED\b/i.test(status)) statusStyle = `${TERM_ESC.brightRed}${TERM_ESC.bold}`;
  else if (/\bUP-TO-DATE\b|\bFROM-CACHE\b|\bSKIPPED\b|\bNO-SOURCE\b/i.test(status)) {
    statusStyle = TERM_ESC.dim;
  } else if (!status) {
    return `${TERM_ESC.dim}${prefix}${TERM_ESC.cyan}${taskPath}${TERM_ESC.reset} ${TERM_ESC.brightCyan}…${TERM_ESC.reset}`;
  } else if (/\bEXECUTED\b|\bSUCCESS\b/i.test(status)) {
    statusStyle = TERM_ESC.green;
  }
  return `${TERM_ESC.dim}${prefix}${TERM_ESC.cyan}${taskPath}${TERM_ESC.reset}${statusStyle}${rest}${TERM_ESC.reset}`;
}

function formatMavenLogLine(line) {
  const m = line.trimStart().match(/^\[(INFO|WARNING|ERROR|DEBUG|WARN)\]\s*(.*)$/);
  if (!m) return null;
  const level = m[1];
  const body = m[2];
  let levelStyle = TERM_ESC.dim;
  if (level === 'ERROR') levelStyle = `${TERM_ESC.brightRed}${TERM_ESC.bold}`;
  else if (level === 'WARNING' || level === 'WARN') levelStyle = TERM_ESC.brightYellow;
  else if (level === 'DEBUG') levelStyle = TERM_ESC.dim;
  else levelStyle = TERM_ESC.blue;
  if (/^--- .+ ---$/.test(body)) {
    return `${levelStyle}${TERM_ESC.dim}── ${body.slice(4, -4)} ──${TERM_ESC.reset}`;
  }
  if (/BUILD SUCCESS/i.test(body)) {
    return `${TERM_ESC.brightGreen}${TERM_ESC.bold}${line.trimEnd()}${TERM_ESC.reset}`;
  }
  if (/BUILD FAILURE/i.test(body)) {
    return `${TERM_ESC.brightRed}${TERM_ESC.bold}${line.trimEnd()}${TERM_ESC.reset}`;
  }
  return `${levelStyle}[${level}]${TERM_ESC.reset} ${body}`;
}

function formatJunitTestLine(line) {
  const trimmed = line.trimEnd();
  let m = trimmed.match(/^([\w.$]+)\s+>\s+([\w.$]+(?:\(\))?)\s+(PASSED|FAILED|SKIPPED|STARTED|PENDING)\b(.*)$/i);
  if (m) {
    const [, cls, method, status, rest] = m;
    let statusStyle = TERM_ESC.brightGreen;
    if (/FAILED/i.test(status)) statusStyle = `${TERM_ESC.brightRed}${TERM_ESC.bold}`;
    else if (/SKIPPED|PENDING/i.test(status)) statusStyle = TERM_ESC.dim;
    else if (/STARTED/i.test(status)) statusStyle = TERM_ESC.brightCyan;
    return `${TERM_ESC.brightBlue}${cls}${TERM_ESC.reset} ${TERM_ESC.dim}>${TERM_ESC.reset} ${TERM_ESC.cyan}${method}${TERM_ESC.reset} ${statusStyle}${status}${TERM_ESC.reset}${rest}`;
  }
  m = trimmed.match(/^([\w.$]+)\s+>\s+([\w.$]+(?:\(\))?)\s+(STANDARD_OUT|STANDARD_ERROR)\b(.*)$/i);
  if (m) {
    const [, cls, method, kind, rest] = m;
    return `${TERM_ESC.brightBlue}${cls}${TERM_ESC.reset} ${TERM_ESC.dim}>${TERM_ESC.reset} ${TERM_ESC.cyan}${method}${TERM_ESC.reset} ${TERM_ESC.dim}${kind}${TERM_ESC.reset}${rest}`;
  }
  m = trimmed.match(/^(\[[^\]]+\]\s+)?Running\s+([\w.$]+)\s*$/i);
  if (m) {
    const tag = m[1] ? `${TERM_ESC.blue}${m[1].trim()}${TERM_ESC.reset} ` : '';
    return `${tag}${TERM_ESC.dim}Running${TERM_ESC.reset} ${TERM_ESC.brightBlue}${m[2]}${TERM_ESC.reset}`;
  }
  m = trimmed.match(/^(Tests run:.*\s-\s+in\s+)([\w.$]+)\s*$/i);
  if (m) {
    let summaryStyle = TERM_ESC.brightGreen;
    if (/\bFailures:\s*[1-9]/i.test(m[1]) || /\bErrors:\s*[1-9]/i.test(m[1])) {
      summaryStyle = TERM_ESC.brightRed;
    }
    return `${summaryStyle}${m[1]}${TERM_ESC.reset}${TERM_ESC.brightBlue}${m[2]}${TERM_ESC.reset}`;
  }
  m = trimmed.match(/^(Test\s+)([\w.$]+)(.*)$/);
  if (m) {
    return `${TERM_ESC.dim}${m[1]}${TERM_ESC.reset}${TERM_ESC.brightBlue}${m[2]}${TERM_ESC.reset}${m[3]}`;
  }
  return null;
}

function colorizeStreamLine(line) {
  const trimmed = line.trimEnd();
  if (!trimmed) return line;

  const gradleTask = formatGradleTaskLine(trimmed);
  if (gradleTask) return gradleTask;

  const junitLine = formatJunitTestLine(line);
  if (junitLine) return junitLine;

  const mavenLine = formatMavenLogLine(line);
  if (mavenLine) return mavenLine;

  if (/^BUILD SUCCESSFUL/i.test(trimmed)) {
    return `${TERM_ESC.brightGreen}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^BUILD FAILED/i.test(trimmed)) {
    return `${TERM_ESC.brightRed}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^Tests run:/i.test(trimmed)) {
    if (/\bFailures:\s*[1-9]/i.test(trimmed) || /\bErrors:\s*[1-9]/i.test(trimmed)) {
      return `${TERM_ESC.brightRed}${line}${TERM_ESC.reset}`;
    }
    return `${TERM_ESC.brightGreen}${line}${TERM_ESC.reset}`;
  }
  if (/\bPASSED\b/i.test(trimmed)) {
    return `${TERM_ESC.green}${line}${TERM_ESC.reset}`;
  }
  if (/^FAILURE:/i.test(trimmed) || /^error:/i.test(trimmed) || /^ERROR\b/i.test(trimmed)) {
    return `${TERM_ESC.brightRed}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/:\s*error:\s/i.test(trimmed)) {
    return `${TERM_ESC.brightRed}${line}${TERM_ESC.reset}`;
  }
  if (/:\s*warning:\s/i.test(trimmed)) {
    return `${TERM_ESC.brightYellow}${line}${TERM_ESC.reset}`;
  }
  if (/^\* What went wrong:/i.test(trimmed) || /^Caused by:/i.test(trimmed)) {
    return `${TERM_ESC.brightYellow}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^warning:/i.test(trimmed) || /^WARNING:/i.test(trimmed) || /\[WARN\]/i.test(trimmed)) {
    return `${TERM_ESC.brightYellow}${line}${TERM_ESC.reset}`;
  }
  if (/^> Configure project /i.test(trimmed) || /^> IDLE/i.test(trimmed)) {
    return `${TERM_ESC.dim}${line}${TERM_ESC.reset}`;
  }
  if (/^Downloading |^Downloaded /i.test(trimmed)) {
    return `${TERM_ESC.dim}${TERM_ESC.blue}${line}${TERM_ESC.reset}`;
  }
  if (/^Daemon will be stopped|^To honour the JVM settings|^A single-use Daemon|^Starting a Gradle Daemon/i.test(trimmed)) {
    return `${TERM_ESC.dim}${line}${TERM_ESC.reset}`;
  }
  if (/^Started .+ in [\d.]+ seconds/i.test(trimmed) || /^Application .+ is running/i.test(trimmed)) {
    return `${TERM_ESC.brightGreen}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^\tat .+\(.+\)$/i.test(trimmed) || /^\.\.\. \d+ more$/i.test(trimmed)) {
    return `${TERM_ESC.dim}${line}${TERM_ESC.reset}`;
  }
  if (/^> .+/.test(trimmed) && /\bcompile\b|\btest\b|\brun\b/i.test(trimmed)) {
    return `${TERM_ESC.magenta}${line}${TERM_ESC.reset}`;
  }
  if (/^\$ /.test(trimmed)) return `${TERM_ESC.cyan}${line}${TERM_ESC.reset}`;
  if (/^FAILURE\b/i.test(trimmed) && /\bFAILED\b/i.test(trimmed)) {
    return `${TERM_ESC.brightRed}${line}${TERM_ESC.reset}`;
  }
  if (/\bFAILED\b/i.test(trimmed) && !/\bUP-TO-DATE\b/i.test(trimmed)) {
    return `${TERM_ESC.red}${line}${TERM_ESC.reset}`;
  }
  return line;
}

const DEFAULT_EDITOR_FONT_SIZE = 11;
const MIN_EDITOR_FONT_SIZE = 10;
const MAX_EDITOR_FONT_SIZE = 28;

/** Editor typefaces — Reaper defaults, not an IDE font clone list. */
const DEFAULT_EDITOR_FONT_ID = 'system-mono';
const EDITOR_FONTS = [
  {
    id: 'system-mono',
    label: 'System mono',
    family: "ui-monospace, 'SF Mono', Menlo, Monaco, Consolas, 'Liberation Mono', monospace",
  },
  {
    id: 'plex-mono',
    label: 'IBM Plex Mono',
    family: "'IBM Plex Mono', Consolas, monospace",
    google: 'IBM+Plex+Mono:wght@400;500',
  },
  {
    id: 'source-code-pro',
    label: 'Source Code Pro',
    family: "'Source Code Pro', Consolas, monospace",
    google: 'Source+Code+Pro:wght@400;500',
  },
  {
    id: 'fira-code',
    label: 'Fira Code',
    family: "'Fira Code', Consolas, monospace",
    google: 'Fira+Code:wght@400;500',
  },
  {
    id: 'roboto-mono',
    label: 'Roboto Mono',
    family: "'Roboto Mono', Consolas, monospace",
    google: 'Roboto+Mono:wght@400;500',
  },
  {
    id: 'inconsolata',
    label: 'Inconsolata',
    family: "'Inconsolata', Consolas, monospace",
    google: 'Inconsolata:wght@400;500',
  },
  {
    id: 'courier',
    label: 'Courier',
    family: "'Courier New', Courier, monospace",
  },
];

const GEMINI_MODELS = [
  { id: 'gemini-3.5-flash', label: 'Gemini 3.5 Flash (default)' },
  { id: 'gemini-2.5-flash', label: 'Gemini 2.5 Flash' },
  { id: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro' },
  { id: 'gemini-3.1-flash-lite', label: 'Gemini 3.1 Flash Lite' },
  { id: 'gemini-3-flash-preview', label: 'Gemini 3 Flash (preview)' },
];

const ANTHROPIC_MODELS = [
  { id: 'claude-sonnet-4-5', label: 'Claude Sonnet 4.5 (default)' },
  { id: 'claude-opus-4-5', label: 'Claude Opus 4.5' },
  { id: 'claude-haiku-4-5', label: 'Claude Haiku 4.5' },
  { id: 'claude-sonnet-4-20250514', label: 'Claude Sonnet 4' },
  { id: 'claude-3-5-haiku-20241022', label: 'Claude 3.5 Haiku' },
];

const BEDROCK_MODELS_FALLBACK = [
  { id: 'anthropic.claude-3-5-sonnet-20241022-v2:0', label: 'Claude 3.5 Sonnet v2 (Bedrock)' },
  { id: 'anthropic.claude-3-5-haiku-20241022-v1:0', label: 'Claude 3.5 Haiku (Bedrock)' },
  { id: 'anthropic.claude-3-opus-20240229-v1:0', label: 'Claude 3 Opus (Bedrock)' },
  { id: 'us.anthropic.claude-sonnet-4-5-20250929-v1:0', label: 'Claude Sonnet 4.5 (US profile)' },
];

function bedrockModelsForSelect() {
  return state.bedrockModels?.length ? state.bedrockModels : BEDROCK_MODELS_FALLBACK;
}

const CURSOR_MODEL_DEFAULT = 'composer-2.5';

function cursorModelsForSelect() {
  if (!state.cursorConfigured) return [];
  if (!state.cursorModelsLoaded) return [];
  return state.cursorModels;
}

function cursorModelIsSupported(modelId) {
  return !!modelId && state.cursorModels.some((m) => m.id === modelId);
}

function normalizeCursorModel(modelId) {
  const list = cursorModelsForSelect();
  if (modelId && list.some((m) => m.id === modelId)) return modelId;
  if (list.length) return list[0].id;
  if (!state.cursorModelsLoaded) return modelId || CURSOR_MODEL_DEFAULT;
  return '';
}

function cursorModelLabel(modelId) {
  const id = normalizeCursorModel(modelId);
  if (!id) return 'no model';
  return state.cursorModels.find((m) => m.id === id)?.label || id;
}

function cursorModelStatusError() {
  if (state.agentProvider !== 'cursor' || !state.cursorConfigured || !state.cursorBridgeOk) {
    return null;
  }
  if (!state.cursorModelsLoaded) return 'Loading Cursor models…';
  if (state.cursorModelsError) return `Could not load Cursor models: ${state.cursorModelsError}`;
  if (!state.cursorModels.length) return 'No Cursor models available for this API key.';
  const model = normalizeCursorModel(state.cursorModel);
  if (!cursorModelIsSupported(model)) {
    return `Model "${model}" isn't available for your Cursor API key. Choose a supported model.`;
  }
  return null;
}

async function reconcileCursorModelSelection() {
  const list = state.cursorModels;
  if (!list.length) return;
  const previous = state.cursorModel || CURSOR_MODEL_DEFAULT;
  if (cursorModelIsSupported(previous)) return;
  const next = list[0].id;
  state.cursorModel = next;
  try {
    const cfg = await api('/api/settings/cursor/model', {
      method: 'PATCH',
      body: JSON.stringify({ model: next }),
    });
    state.cursorModel = cfg.model || next;
  } catch {
    /* keep local selection */
  }
  toast(
    `Model "${previous}" isn't available for your Cursor API key — using ${list[0].label || next}.`,
    'warn',
  );
}

/** Registered chat agents — add providers here as backends land. */
const AGENT_PROVIDER_ORDER = ['cursor', 'gemini', 'anthropic', 'bedrock'];

const REAPER_RELEASES_BASE = 'https://github.com/reaper-org/releases/releases';

function reaperAppVersion() {
  return document.querySelector('meta[name="reaper-app-version"]')?.content?.trim() || '0.1.2';
}

function reaperReleaseTag() {
  return `v${reaperAppVersion()}`;
}

function reaperReleasePageUrl() {
  return `${REAPER_RELEASES_BASE}/tag/${reaperReleaseTag()}`;
}

const AGENT_PROVIDERS = {
  cursor: {
    id: 'cursor',
    label: 'Cursor',
    settingsTab: 'cursor',
    capabilities: { modes: true, tools: true, revert: true, liveFollow: true },
    isConfigured: () => state.cursorConfigured,
    isReady: () => state.cursorConfigured && state.cursorBridgeOk,
    welcome: () => 'Ask the Cursor agent to edit files, run git commands, or explain the codebase.',
    placeholder: 'Ask Cursor… (Enter to send)',
    emptyReply: () => 'Done — check Source Control for changes.',
    hintWhenReady: 'Enter to send · Shift+Enter for newline',
    messageLabel: 'Cursor agent',
    labelClass: 'agent-msg-label-cursor',
    models: () => cursorModelsForSelect(),
    currentModel: () => normalizeCursorModel(state.cursorModel),
    setModel: (id) => setCursorAgentModel(id),
    statusText: () => {
      const modeLabel = state.cursorMode === 'plan' ? 'Plan' : state.cursorMode === 'ask' ? 'Ask' : 'Agent';
      return `Cursor ${modeLabel} · ${cursorModelLabel(state.cursorModel)}`;
    },
    chatPath: '/cursor/chat',
    stopPath: '/cursor/stop',
    chatBody: (prompt) => ({ prompt, model: normalizeCursorModel(state.cursorModel), mode: state.cursorMode }),
    notConfiguredHint: 'Configure Cursor in Settings (⌘,)',
    notReadyHint: () => state.cursorBridgeError || 'Bridge starting… click Retry or restart Reaper',
  },
  gemini: {
    id: 'gemini',
    label: 'Gemini',
    settingsTab: 'ai',
    capabilities: { readOnly: true },
    isConfigured: () => state.geminiConfigured,
    isReady: () => state.geminiConfigured,
    welcome: () => 'Ask the Gemini agent to explain code, brainstorm designs, or draft snippets. It does not edit files or run commands.',
    placeholder: 'Ask Gemini… (Enter to send)',
    emptyReply: () => 'No response from Gemini. Check your API key and model in Settings → AI.',
    hintWhenReady: 'Gemini agent — read-only Q&A · Enter to send',
    messageLabel: 'Gemini agent',
    labelClass: 'agent-msg-label-gemini',
    models: () => GEMINI_MODELS,
    currentModel: () => state.geminiModel,
    setModel: (id) => setGeminiAgentModel(id),
    statusText: () => `Gemini agent · ${state.geminiModel}`,
    chatPath: '/gemini/chat',
    stopPath: null,
    chatBody: (prompt) => ({ prompt, model: state.geminiModel }),
    notConfiguredHint: 'Configure Gemini in Settings → AI (⌘,)',
  },
  anthropic: {
    id: 'anthropic',
    label: 'Claude',
    settingsTab: 'ai',
    capabilities: { readOnly: true },
    isConfigured: () => state.anthropicConfigured,
    isReady: () => state.anthropicConfigured,
    welcome: () => 'Ask Claude (Anthropic API) to explain code, brainstorm designs, or draft snippets. It does not edit files or run commands.',
    placeholder: 'Ask Claude… (Enter to send)',
    emptyReply: () => 'No response from Claude. Check your API key in Settings → AI → Claude.',
    hintWhenReady: 'Claude agent — read-only Q&A · Enter to send',
    messageLabel: 'Claude agent',
    labelClass: 'agent-msg-label-anthropic',
    models: () => ANTHROPIC_MODELS,
    currentModel: () => state.anthropicModel,
    setModel: (id) => setAnthropicAgentModel(id),
    statusText: () => `Claude · ${state.anthropicModel}`,
    chatPath: '/anthropic/chat',
    stopPath: null,
    chatBody: (prompt) => ({
      prompt,
      model: state.anthropicModel,
      backend: 'api',
    }),
    notConfiguredHint: 'Configure Claude in Settings → AI (⌘,)',
  },
  bedrock: {
    id: 'bedrock',
    label: 'Bedrock',
    settingsTab: 'ai',
    capabilities: { readOnly: true },
    isConfigured: () => state.bedrockConfigured,
    isReady: () => state.bedrockConfigured,
    welcome: () => 'Ask Claude on Amazon Bedrock to explain code, brainstorm designs, or draft snippets. It does not edit files or run commands.',
    placeholder: 'Ask Bedrock… (Enter to send)',
    emptyReply: () => 'No response from Bedrock. Check Mantle key / AWS credentials in Settings → AI → Bedrock.',
    hintWhenReady: 'Bedrock agent — read-only Q&A · Enter to send',
    messageLabel: 'Bedrock agent',
    labelClass: 'agent-msg-label-bedrock',
    models: () => bedrockModelsForSelect(),
    currentModel: () => state.bedrockModelId,
    setModel: (id) => setBedrockAgentModel(id),
    statusText: () => `Bedrock · ${state.bedrockModelId}`,
    chatPath: '/anthropic/chat',
    stopPath: null,
    chatBody: (prompt) => ({
      prompt,
      model: state.bedrockModelId,
      backend: 'bedrock',
    }),
    notConfiguredHint: 'Configure Bedrock in Settings → AI (⌘,)',
  },
};

const state = {
  repo: null,
  repos: [],
  hiddenRepos: [],
  branches: [],
  tabs: [],
  tabContents: new Map(),
  activeTab: null,
  editor: null,
  dirty: new Set(),
  activePanel: 'explorer',
  structureMode: localStorage.getItem(STRUCTURE_MODE_KEY) === 'full' ? 'full' : 'structure',
  structureFilter: '',
  structureAst: null,
  structurePath: null,
  structureSeq: 0,
  agentDock: localStorage.getItem(AGENT_DOCK_KEY) || 'left',
  terminalDock: localStorage.getItem(TERMINAL_DOCK_KEY) || 'bottom',
  dockerLogsOpen: false,
  dockerLogsDock: localStorage.getItem(DOCKER_LOGS_DOCK_KEY) || 'right',
  buildTasksDock: localStorage.getItem(BUILD_TASKS_DOCK_KEY) || 'right',
  packageManifestDock: localStorage.getItem(PACKAGE_MANIFEST_DOCK_KEY) || 'right',
  dockerLogsStreaming: false,
  dockerLogsAbortController: null,
  dockerLogsXhr: null,
  dockerLogsModulePath: null,
  dockerLogsLabel: '',
  dockerLogsAutoScroll: true,
  dockerContainers: [],
  dockerSelectedId: null,
  terminalOpen: false,
  terminalMountSync: false,
  terminals: [],
  activeTerminalId: null,
  agentOpen: true,
  cursorConfigured: false,
  cursorBridgeOk: false,
  cursorBridgeError: null,
  cursorKeyMasked: null,
  cursorKeySource: null,
  cursorModel: CURSOR_MODEL_DEFAULT,
  agentProvider: localStorage.getItem(AGENT_PROVIDER_KEY) || 'cursor',
  cursorMode: 'agent',
  cursorModels: [],
  cursorModelsLoaded: false,
  cursorModelsError: null,
  agentBusy: false,
  agentMessageQueue: [],
  /** Live one-line activity while the agent runs (tool name / path). */
  agentActivity: null,
  agentAbortController: null,
  agentStopRequested: false,
  agentLastRevertibleTurn: null,
  agentLiveFollow: false,
  agentLiveDiffPath: null,
  agentLastToolPath: null,
  agentSeenPaths: new Set(),
  agentHadFileChanges: false,
  cloneBusy: false,
  publishBusy: false,
  pushBusy: false,
  pushPreview: null,
  commitBusy: false,
  gitBackgroundFetch: false,
  lastRepo: null,
  recentGitRemotes: [],
  recentGitLocalPaths: [],
  cloneSource: 'remote',
  currentBranch: '',
  defaultBranch: '',
  editorReady: false,
  suppressEditorChange: false,
  autoSaveTimer: null,
  saveInFlight: false,
  projectReloadTimer: null,
  projectReloadPending: false,
  projectReloadBackground: false,
  projectAutoRefreshAttempts: 0,
  gradleInfo: null,
  runInfo: null,
  serverRunTarget: null,
  runTarget: { mode: 'none' },
  repoDetail: null,
  projectIndexPoll: null,
  projectIndexNotified: false,
  projectIndexRunning: false,
  projectIndexReady: false,
  projectIndexStartedAt: 0,
  editorIndexFrozen: false,
  editorIndexFrozenPrevReadOnly: undefined,
  indexFreezeActive: false,
  projectProfile: null,
  repoPickerUnregisterName: null,
  repoPickerUnregisterBusy: false,
  patTokenPendingRemove: null,
  treeNavAnchor: null,
  mergeState: null,
  conflictDecorationIds: [],
  diagDecorationIds: [],
  testRunWidgets: [],
  testCovWidgets: [],
  testMethodsByLine: new Map(),
  fileCoverage: new Map(),
  coverageDecorationIds: [],
  coverageInlineEnabled: true,
  blameEnabled: false,
  blameDecorationIds: [],
  blameByPath: new Map(),
  rebaseMode: false,
  rebaseSteps: [],
  coveragePanelOpen: false,
  coverageReport: null,
  debugPanelOpen: false,
  debugActive: false,
  debugState: { status: 'idle', frames: [], variables: [], breakpoints: [] },
  debugBreakpoints: new Map(),
  debugWatch: [],
  debugCapabilities: null,
  debugWs: null,
  debugDecorationIds: [],
  debugCurrentLineId: null,
  dbViewerPanelOpen: false,
  gitViewerPanelOpen: false,
  gitViewerLastResult: null,
  dbConnection: null,
  dbSchema: null,
  dbSchemaFilter: '',
  dbSchemaOpenSchemas: new Set(),
  dbSchemaOpenTables: new Set(),
  dbSchemaOpenFolders: new Set(),
  dbTreeSelection: null,
  dbQueryResult: null,
  dbGridColumnWidths: {},
  buildTasksPanelOpen: false,
  buildTasksFilter: '',
  buildTasksSelected: null,
  buildTasksFocusKey: null,
  packageManifestPanelOpen: false,
  packageManifestView: null,
  packageManifestFilter: '',
  conflictFiles: new Set(),
  selectedCommitHash: null,
  mainView: 'editor',
  conflictPanelHidden: false,
  geminiConfigured: false,
  anthropicConfigured: false,
  bedrockConfigured: false,
  bedrockModels: [],
  bedrockModelsSource: null,
  bedrockModelsError: null,
  anthropicBackend: 'api',
  anthropicModel: 'claude-sonnet-4-5',
  bedrockModelId: 'anthropic.claude-3-5-sonnet-20241022-v2:0',
  bedrockRegion: 'us-east-1',
  javaLanguageLevel: 17,
  languageContext: null,
  jdtlsReady: false,
  geminiModel: 'gemini-3.5-flash',
  pendingCommitSuggest: false,
  commitSuggestInFlight: false,
  commitSuggestSkipOnce: false,
  commitSelectedPaths: new Set(),
  commitKnownPaths: new Set(),
  lastGitStatusFiles: [],
  mergeBlockedCommit: false,
};

let diagTimer = null;
let allJavaDiagTimer = null;
let tabRenderTimer = null;
let editorContentSyncTimer = null;
let testDecorTimer = null;
let coverageClearTimer = null;
let diagSeq = 0;
let diagRetryTimer = null;
let diagRetryDelayMs = 2500;
let allJavaDiagRefreshGen = 0;
let javaFullDiagTimer = null;
let javaFullDiagSeq = 0;
let javaFullCompileRunning = false;
/** @type {{ path: string, content: string, force: boolean } | null} */
let javaFullCompilePending = null;
/** Coalesced disk writes per path — one PUT at a time, latest buffer wins. */
/** @type {Map<string, { pending: string | null, running: boolean, waiters: Array<{ resolve: (v: boolean) => void, reject: (e: Error) => void }> }>} */
const saveWriteCoalesceByPath = new Map();
let javaCompileFooterGen = 0;
let compileFooterSafetyTimer = null;
const COMPILE_FOOTER_SAFETY_MS = 20000;
/** Last buffer snapshot we started full javac for (per path). */
const javaFullDiagSnapshotByPath = new Map();
/** @type {Map<string, { controller: AbortController, promise: Promise<unknown>, content: string }>} */
const diagFetchByPath = new Map();
let fileDiags = [];
let diagJumpIndex = 0;

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

function mountReaperIcons(root = document) {
  const icons = window.ReaperIcons;
  if (!icons) return;
  root.querySelectorAll('[data-icon]').forEach((el) => {
    const name = el.dataset.icon;
    const svg = icons[name];
    if (!svg) return;
    const badge = el.querySelector('.ij-activity-badge, .ij-activity-badge-dot');
    if (badge) {
      el.insertAdjacentHTML('afterbegin', svg);
    } else {
      el.innerHTML = svg;
    }
  });
}

let saveGateActive = false;
const saveGateWaiters = [];

function enterSaveGate() {
  saveGateActive = true;
}

function leaveSaveGate() {
  saveGateActive = false;
  for (const resolve of saveGateWaiters) resolve();
  saveGateWaiters.length = 0;
}

function waitForSaveGate() {
  if (!saveGateActive) return Promise.resolve();
  return new Promise((resolve) => saveGateWaiters.push(resolve));
}

function usesInProcessApi() {
  return typeof location !== 'undefined' && location.protocol === 'reaper:';
}

/** Free WebKit HTTP slots before save PUT; skip on reaper:// (in-process, no pool limit). */
async function prepareForSave() {
  const wasPolling = !!state.projectIndexPoll;
  stopProjectIndexPolling({ keepUi: true });
  if (usesInProcessApi()) {
    return { wasPolling, lightweight: true };
  }
  abortAllDiagnosticFetches();
  clearTimeout(javaFullDiagTimer);
  javaFullDiagTimer = null;
  javaFullCompilePending = null;
  ++javaFullDiagSeq;
  enterSaveGate();
  await new Promise((r) => setTimeout(r, SAVE_GATE_DRAIN_MS));
  return { wasPolling, lightweight: false };
}

async function api(path, opts = {}) {
  if (!opts.allowDuringSave) await waitForSaveGate();
  const { allowDuringSave: _allowDuringSave, timeoutMs = 120_000, ...fetchOpts } = opts;
  const controller = new AbortController();
  const timer = timeoutMs > 0
    ? setTimeout(() => controller.abort(), timeoutMs)
    : null;
  try {
    const res = await fetch(path, {
      headers: { 'Content-Type': 'application/json', ...fetchOpts.headers },
      signal: controller.signal,
      ...fetchOpts,
    });
    const text = await res.text();
    let data = null;
    try { data = text ? JSON.parse(text) : null; } catch { data = text; }
    if (!res.ok) throw new Error(data?.error || res.statusText);
    return data;
  } catch (err) {
    if (err?.name === 'AbortError') {
      throw new Error(`Request timed out (${Math.round(timeoutMs / 1000)}s): ${path}`);
    }
    throw err;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

function repoApi(name, suffix = '') {
  return `/api/repos/${encodeURIComponent(name)}${suffix}`;
}

const TOAST_ICONS = {
  error: 'toastError',
  warning: 'toastWarning',
  success: 'toastSuccess',
  info: 'toastInfo',
};

const TOAST_KINDS = {
  error: 'Error',
  warning: 'Notice',
  success: 'Done',
  info: '',
};

function dismissToast() {
  const el = $('#toast');
  const slot = $('#toast-slot');
  if (!el || !slot || slot.classList.contains('hidden') || slot.classList.contains('is-closing')) return;
  clearTimeout(toast._timer);
  const progress = el.querySelector('.ij-toast-progress-fill');
  if (progress) progress.style.animationPlayState = 'paused';
  slot.classList.remove('is-open');
  slot.classList.add('is-closing');
  const finish = () => {
    slot.classList.add('hidden');
    slot.classList.remove('is-open', 'is-closing');
    if (progress) progress.style.animation = '';
  };
  const onEnd = (e) => {
    if (e.target !== slot || e.animationName !== 'ij-toast-roll-up') return;
    slot.removeEventListener('animationend', onEnd);
    finish();
  };
  slot.addEventListener('animationend', onEnd);
  setTimeout(finish, 520);
}

function toast(msg, type = 'info', { duration } = {}) {
  const el = $('#toast');
  const slot = $('#toast-slot');
  if (!el || !slot) {
    console.error('[toast missing]', msg);
    return;
  }
  const msgEl = el.querySelector('.ij-toast-message');
  const iconEl = el.querySelector('.ij-toast-icon');
  const kindEl = el.querySelector('.ij-toast-kind');
  if (msgEl) msgEl.textContent = msg;
  if (iconEl) {
    iconEl.dataset.icon = TOAST_ICONS[type] || TOAST_ICONS.info;
    mountReaperIcons(el);
  }
  if (kindEl) {
    const kind = TOAST_KINDS[type] || '';
    kindEl.textContent = kind;
    kindEl.classList.toggle('hidden', !kind);
  }
  el.className = `ij-toast ${type}`;
  slot.classList.remove('hidden', 'is-closing', 'is-open');
  void slot.offsetWidth;
  slot.classList.add('is-open');
  clearTimeout(toast._timer);
  const ms = duration ?? (type === 'error' ? 9000 : 3500);
  const progress = el.querySelector('.ij-toast-progress-fill');
  if (progress) {
    progress.style.animation = 'none';
    void progress.offsetWidth;
    progress.style.animation = `ij-toast-progress ${ms}ms linear forwards`;
  }
  toast._timer = setTimeout(dismissToast, ms);
}

function gradleClassfileVersionToast(output) {
  if (!/Unsupported class file major version/i.test(String(output || ''))) return false;
  const m = String(output).match(/major version\s+(\d+)/i);
  const classMajor = m ? Number(m[1]) : 0;
  const javaMajor = classMajor >= 45 ? classMajor - 44 : classMajor;
  let msg = 'Gradle needs a compatible JDK — open Settings → Java and pick Java 21 or 17.';
  if (javaMajor >= 25) {
    msg = `Java ${javaMajor} is too new for this Gradle version (class file ${classMajor}). Set Settings → Java to Java 21 or 17 for Gradle builds.`;
  } else if (javaMajor > 0 && javaMajor < 17) {
    msg = `Java ${javaMajor} is too old for this Gradle version. Use Java 17 or 21 in Settings → Java.`;
  }
  toast(msg, 'error', { duration: 14000 });
  return true;
}

function completionDebugEnabled() {
  try {
    return localStorage.getItem('reaper-complete-debug') === '1';
  } catch {
    return false;
  }
}

function setCompleteDebugStatus(msg) {
  if (!completionDebugEnabled()) return;
  const line = String(msg || '');
  const dbg = $('#status-complete-debug');
  if (dbg) dbg.textContent = line;
  console.log('[Reaper complete]', line);
}

let globalLoadingSafetyTimer = null;

function setGlobalLoading(on, text = 'Loading…') {
  const overlay = $('#loading-overlay');
  const label = $('#loading-text');
  const spinner = $('#loading-spinner');
  const logo = overlay?.querySelector('[data-reaper-logo]');
  if (label) label.textContent = text;
  spinner?.classList.toggle('hidden', !on);
  logo?.classList.toggle('hidden', on);
  overlay?.classList.toggle('hidden', !on);
  overlay?.classList.toggle('flex', on);
  if (globalLoadingSafetyTimer) {
    clearTimeout(globalLoadingSafetyTimer);
    globalLoadingSafetyTimer = null;
  }
  if (on) {
    globalLoadingSafetyTimer = setTimeout(() => {
      globalLoadingSafetyTimer = null;
      setGlobalLoading(false);
      toast('Opening project is taking longer than expected — showing partial UI', 'warning');
    }, 90_000);
  }
}

/** Apply theme backdrop once the IDE shell is ready to show. */
function markUiReady() {
  document.documentElement?.classList.add('reaper-ui-ready');
  document.body?.classList.add('reaper-ui-ready');
  const bg = getComputedStyle(document.documentElement).getPropertyValue('--ij-bg').trim() || '#2b2b2b';
  document.documentElement.style.backgroundColor = bg;
  document.body.style.backgroundColor = bg;
}

/** Remove launch splash once the IDE is ready. */
function dismissLaunchSplashNow() {
  const splash = $('#launch-splash');
  if (!splash) return;
  splash.dataset.dismissing = '1';
  splash.classList.add('is-dismissing');
  setTimeout(() => {
    markUiReady();
    splash.remove();
  }, 360);
}

function hideLaunchSplash(options = {}) {
  const { immediate = false } = options;
  if (immediate) {
    dismissLaunchSplashNow();
    return;
  }
  const splash = $('#launch-splash');
  if (!splash || splash.dataset.dismissing) return;
  splash.dataset.dismissing = '1';
  dismissLaunchSplashNow();
}

function waitForLaunchSplashSequence() {
  if (typeof window.waitForLaunchSplashHarvest === 'function') {
    return window.waitForLaunchSplashHarvest();
  }
  const started = window.__reaperSplashAt || Date.now();
  const total = window.__reaperSplashTiming?.totalMs || 0;
  return new Promise((resolve) => {
    const remain = Math.max(0, total - (Date.now() - started));
    setTimeout(resolve, remain);
  });
}

function busyIndicatorHtml({ large = false } = {}) {
  const sizeClass = large ? ' ij-busy-indicator--lg' : '';
  return `<span class="ij-busy-indicator${sizeClass}" aria-hidden="true"><span class="ij-busy-indicator-ring"></span><span class="ij-busy-indicator-glow"></span></span>`;
}

function navigationBusyHtml(message) {
  return `<span class="ij-nav-busy">${busyIndicatorHtml()}<span class="ij-nav-busy-label">${escapeHtml(message)}</span></span>`;
}

function busyStatusHtml(message) {
  return `<span class="ij-busy-status">${busyIndicatorHtml()}<span>${escapeHtml(message)}</span></span>`;
}

function setCloneModalState({ busy = false, status = '', error = '' } = {}) {
  const errEl = $('#clone-error');
  const statusEl = $('#clone-status');
  const statusText = $('#clone-status-text');
  const cancelBtn = $('#clone-modal-cancel');
  const submitBtn = $('#btn-clone-submit');
  const overlay = $('#clone-modal-overlay');
  const isLocal = state.cloneSource === 'local';
  const busyLabel = isLocal ? 'Importing…' : 'Cloning…';
  const idleLabel = isLocal ? 'Import & open' : 'Clone & open';

  if (errEl) {
    errEl.textContent = error;
    errEl.classList.toggle('hidden', !error);
    if (error) errEl.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }
  if (statusEl) {
    if (statusText) statusText.textContent = status || busyLabel;
    statusEl.classList.toggle('hidden', !status);
  }
  if (overlay) overlay.classList.toggle('ij-clone-modal--busy', busy);
  if (cancelBtn) cancelBtn.disabled = busy;
  const closeBtn = $('#clone-modal-close');
  if (closeBtn) closeBtn.disabled = busy;
  ['#clone-tab-remote', '#clone-tab-local'].forEach((sel) => {
    const tab = $(sel);
    if (tab) tab.disabled = busy;
  });
  if (submitBtn) {
    submitBtn.disabled = busy;
    submitBtn.textContent = busy ? busyLabel : idleLabel;
  }
  ['#clone-remote-url', '#clone-local-name', '#clone-local-path', '#clone-local-browse', '#clone-recent-remote', '#clone-recent-local'].forEach((sel) => {
    const input = $(sel);
    if (input) input.disabled = busy;
  });
  state.cloneBusy = busy;
}

function setPublishModalState({ busy = false, status = '', error = '' } = {}) {
  const errEl = $('#publish-error');
  const statusEl = $('#publish-status');
  const statusText = $('#publish-status-text');
  const cancelBtn = $('#publish-modal-cancel');
  const submitBtn = $('#btn-publish-submit');
  const overlay = $('#publish-modal-overlay');
  const busyLabel = 'Publishing to remote…';

  if (errEl) {
    errEl.textContent = error;
    errEl.classList.toggle('hidden', !error);
    if (error) errEl.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }
  if (statusEl) {
    if (statusText) statusText.textContent = status || busyLabel;
    statusEl.classList.toggle('hidden', !status);
  }
  if (overlay) overlay.classList.toggle('ij-clone-modal--busy', busy);
  if (cancelBtn) cancelBtn.disabled = busy;
  if (submitBtn) {
    submitBtn.disabled = busy;
    submitBtn.textContent = busy ? busyLabel : 'Publish';
  }
  ['#publish-remote-url', '#publish-host', 'input[name="create"]', 'input[name="private"]'].forEach((sel) => {
    const input = $(sel);
    if (input) input.disabled = busy;
  });
  state.publishBusy = busy;
}

function setPushModalBusy({ busy = false, text = 'Pushing…' } = {}) {
  const overlay = $('#push-modal-overlay');
  const modal = overlay?.querySelector('.ij-push-modal');
  const busyEl = $('#push-modal-busy');
  const busyText = $('#push-modal-busy-text');
  const cancelBtn = $('#push-modal-cancel');
  const confirmBtn = $('#push-modal-confirm');
  if (busyText) busyText.textContent = text;
  busyEl?.classList.toggle('hidden', !busy);
  modal?.classList.toggle('ij-push-modal--busy', busy);
  if (cancelBtn) cancelBtn.disabled = busy;
  if (confirmBtn && busy) confirmBtn.disabled = true;
  state.pushBusy = busy;
}

function setCloneModalTab(source) {
  state.cloneSource = source === 'local' ? 'local' : 'remote';
  const remoteTab = $('#clone-tab-remote');
  const localTab = $('#clone-tab-local');
  const remotePanel = $('#clone-remote-panel');
  const localPanel = $('#clone-local-panel');
  const remoteHint = $('#clone-remote-hint');
  const isLocal = state.cloneSource === 'local';

  remoteTab?.classList.toggle('active', !isLocal);
  localTab?.classList.toggle('active', isLocal);
  remoteTab?.setAttribute('aria-selected', String(!isLocal));
  localTab?.setAttribute('aria-selected', String(isLocal));
  remotePanel?.classList.toggle('hidden', isLocal);
  localPanel?.classList.toggle('hidden', !isLocal);
  remoteHint?.classList.toggle('hidden', isLocal);

  if (isLocal) {
    $('#clone-local-path')?.focus();
  } else {
    $('#clone-remote-url')?.focus();
  }
  setCloneModalState();
}

function deriveNameFromLocalPath(raw) {
  const trimmed = String(raw || '').trim();
  if (!trimmed) return '';
  const base = trimmed.replace(/\/+$/, '').split('/').pop() || '';
  const name = base.replace(/\.git$/i, '');
  const sanitized = name.replace(/[^a-zA-Z0-9._-]/g, '-').replace(/^[-.]+|[-.]+$/g, '');
  if (!sanitized || !/^[a-zA-Z0-9._-]+(\/[a-zA-Z0-9._-]+)?$/.test(sanitized)) return '';
  return sanitized;
}

function normalizeRemoteUrl(raw) {
  const trimmed = String(raw || '').trim();
  if (!trimmed) return '';
  if (/^git@/i.test(trimmed)) return trimmed;
  if (!trimmed.includes('://')) return `https://${trimmed}`;
  return trimmed;
}

function deriveNameFromUrl(raw) {
  if (!raw?.trim()) return '';
  try {
    const u = new URL(raw.includes('://') ? raw.trim() : `https://${raw.trim()}`);
    let path = u.pathname.replace(/^\//, '').replace(/\.git$/i, '');
    if (!path || !/^[a-zA-Z0-9._-]+(\/[a-zA-Z0-9._-]+)?$/.test(path)) return '';
    return path;
  } catch {
    return '';
  }
}

function hostFromUrl(raw) {
  try {
    const u = new URL(raw.includes('://') ? raw.trim() : `https://${raw.trim()}`);
    return u.hostname.toLowerCase();
  } catch {
    return '';
  }
}

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

let settingsTab = 'git';

function editorLineHeightFor(size) {
  return Math.round(size * 1.538);
}

function getEditorFontSize() {
  const n = parseInt(localStorage.getItem(EDITOR_FONT_SIZE_KEY), 10);
  if (Number.isFinite(n) && n >= MIN_EDITOR_FONT_SIZE && n <= MAX_EDITOR_FONT_SIZE) return n;
  return DEFAULT_EDITOR_FONT_SIZE;
}

const LEGACY_EDITOR_FONT_IDS = new Set(['jetbrains-mono', 'jetbrains', 'intellij']);

function normalizeEditorFontId(id) {
  if (!id || LEGACY_EDITOR_FONT_IDS.has(id)) return DEFAULT_EDITOR_FONT_ID;
  return id;
}

function getEditorFontSpec() {
  const stored = localStorage.getItem(EDITOR_FONT_FAMILY_KEY);
  const id = normalizeEditorFontId(stored || DEFAULT_EDITOR_FONT_ID);
  if (stored && id !== stored) {
    localStorage.setItem(EDITOR_FONT_FAMILY_KEY, id);
    document.getElementById('reaper-font-jetbrains-mono')?.remove();
  }
  return EDITOR_FONTS.find((f) => f.id === id) || EDITOR_FONTS[0];
}

function getAgentFontMatchEditor() {
  const stored = localStorage.getItem(AGENT_FONT_MATCH_EDITOR_KEY);
  if (stored === null) return true;
  return stored === '1' || stored === 'true';
}

function getAgentFontSize() {
  if (getAgentFontMatchEditor()) return getEditorFontSize();
  const n = parseInt(localStorage.getItem(AGENT_FONT_SIZE_KEY), 10);
  if (Number.isFinite(n) && n >= MIN_EDITOR_FONT_SIZE && n <= MAX_EDITOR_FONT_SIZE) return n;
  return DEFAULT_EDITOR_FONT_SIZE;
}

function getAgentFontSpec() {
  if (getAgentFontMatchEditor()) return getEditorFontSpec();
  const stored = localStorage.getItem(AGENT_FONT_FAMILY_KEY);
  const id = normalizeEditorFontId(stored || DEFAULT_EDITOR_FONT_ID);
  if (stored && id !== stored) {
    localStorage.setItem(AGENT_FONT_FAMILY_KEY, id);
  }
  return EDITOR_FONTS.find((f) => f.id === id) || EDITOR_FONTS[0];
}

function ensureEditorFontLoaded(spec) {
  if (!spec?.google || document.getElementById(`reaper-font-${spec.id}`)) return;
  const link = document.createElement('link');
  link.id = `reaper-font-${spec.id}`;
  link.rel = 'stylesheet';
  link.href = `https://fonts.googleapis.com/css2?family=${spec.google}&display=swap`;
  document.head.appendChild(link);
}

function getAutoSaveEnabled() {
  const stored = localStorage.getItem(AUTO_SAVE_KEY);
  if (stored === null) return true;
  return stored === '1' || stored === 'true';
}

function setAutoSaveEnabled(enabled) {
  localStorage.setItem(AUTO_SAVE_KEY, enabled ? '1' : '0');
  const checkbox = $('#settings-auto-save');
  if (checkbox) checkbox.checked = enabled;
  if (!enabled && state.autoSaveTimer) {
    clearTimeout(state.autoSaveTimer);
    state.autoSaveTimer = null;
  }
}

async function refreshJavaLanguageLevel() {
  try {
    const jdk = await api('/api/settings/jdk');
    if (jdk?.java_release) {
      state.javaLanguageLevel = jdk.java_release;
      return;
    }
    const ver = jdk?.effective_version || jdk?.version || '';
    state.javaLanguageLevel = parseJavaMajorVersion(ver);
  } catch {
    state.javaLanguageLevel = 17;
  }
}

const JAVA_RELEASE_LEVELS = [8, 11, 17, 21, 22, 23, 24, 25, 26];

/** Parse `21.0.2`, `1.8.0_392`, `openjdk version "21"` → major version. */
function parseJavaMajorVersion(ver) {
  const s = String(ver || '').trim();
  if (!s) return 17;
  const legacy = s.match(/^1\.(\d+)/);
  if (legacy) return parseInt(legacy[1], 10) || 17;
  const m = s.match(/(\d+)/);
  return m ? parseInt(m[1], 10) || 17 : 17;
}

async function refreshLanguageContextForPath(path) {
  if (!state.repo || !path) {
    state.languageContext = null;
    await refreshJavaLanguageLevel();
    updateStatusLanguage(path);
    return;
  }
  try {
    const q = new URLSearchParams({ path });
    const res = await api(repoApi(state.repo, `/workspace/language-context?${q}`));
    if (res) {
      state.languageContext = res;
      if (res.java_level) state.javaLanguageLevel = res.java_level;
      else if (res.jdk_level) state.javaLanguageLevel = res.jdk_level;
      else if (path.endsWith('.java')) await refreshJavaLanguageLevel();
      updateStatusLanguage(path);
      return;
    }
  } catch {
    /* fall back */
  }
  state.languageContext = null;
  await refreshJavaLanguageLevel();
  updateStatusLanguage(path);
}

function completionLevelForPath(path, ctx) {
  if (!path?.endsWith('.java')) return null;
  return ctx?.java_level ?? ctx?.jdk_level ?? state.javaLanguageLevel ?? null;
}

function shortCompilerVersion(raw) {
  const line = String(raw || '').split('\n')[0].trim();
  if (!line) return '';
  const semver = line.match(/(\d+(?:\.\d+){0,2})/);
  if (semver) return semver[1];
  return line.slice(0, 40);
}

function updateStatusLanguage(path) {
  const el = $('#status-language');
  if (!el) return;
  const lang = window.ReaperLang?.langLabelForPath?.(path)
    || window.ReaperLang?.langLabel?.(langForPath(path))
    || 'Plain Text';
  const ctx = state.languageContext;
  const primary = ctx?.compilers?.find((c) => c.version) || ctx?.compilers?.[0];
  const tool = ctx?.completion_tool || primary?.id;
  const verRaw = ctx?.completion_version || primary?.version || '';
  const ver = shortCompilerVersion(verRaw);
  const level = completionLevelForPath(path, ctx);
  if (tool && ver) {
    if (level != null && path?.endsWith('.java')) {
      el.textContent = `${lang} · ${level} · ${tool} ${ver}`;
    } else {
      el.textContent = `${lang} · ${tool} ${ver}`;
    }
    const parts = [`Completions use ${tool} ${ver}`];
    if (level != null && path?.endsWith('.java')) {
      parts.push(`Java language level ${level}`);
      if (ctx?.jdk_level && ctx.jdk_level !== level) {
        parts.push(`configured JDK ${ctx.jdk_level}`);
      }
      if (ctx?.project_java_level) {
        parts.push(`project declares ${ctx.project_java_level}`);
      }
      if (ctx?.configured_java_release) {
        parts.push(`settings level ${ctx.configured_java_release}`);
      }
    } else if (ctx?.dialect) {
      parts.push(`project declares ${ctx.dialect}`);
    }
    el.title = parts.join(' · ');
    return;
  }
  if (level != null && path?.endsWith('.java')) {
    el.textContent = `${lang} · Java ${level}`;
    const parts = [`Java completions use language level ${level}`];
    if (ctx?.project_java_level) {
      parts.push(`project declares ${ctx.project_java_level}`);
    }
    el.title = parts.join(' · ');
    return;
  }
  const compilers = window.ReaperLang?.compilerLabelsForPath?.(path);
  el.textContent = compilers ? `${lang} · ${compilers}` : lang;
  el.title = compilers ? `Language: ${lang}. Compiler(s): ${compilers}` : `Language: ${lang}`;
}

async function refreshJavaLanguageLevelForPath(path) {
  return refreshLanguageContextForPath(path);
}

async function initStatusFooter() {
  const el = $('#status-version');
  if (!el) return;
  const metaBuild = document.querySelector('meta[name="reaper-ui-build"]')?.content;
  let version = document.querySelector('meta[name="reaper-app-version"]')?.content;
  const apply = (v, b) => {
    const parts = [];
    if (v) parts.push(`v${v}`);
    if (b) parts.push(`build-${b}`);
    el.textContent = parts.join(' · ');
    el.title = `Reaper ${parts.join(' · ')} — click for ${reaperReleaseTag()} downloads (Apple Silicon + Intel DMG)`;
  };
  apply(version, metaBuild);
  el.classList.add('ij-status-version-link');
  if (!el.dataset.releaseBound) {
    el.dataset.releaseBound = '1';
    el.addEventListener('click', () => {
      window.open(reaperReleasePageUrl(), '_blank', 'noopener,noreferrer');
    });
  }
  try {
    const info = await api('/api/version');
    if (info) {
      cacheLoopbackWs(info.loopback_ws);
      // Prefer meta tag (loaded static bundle) over compile-time API build.
      apply(info.version || version, metaBuild || info.build);
    }
  } catch {
    /* server still starting */
  }
}

function getAiInlineCompleteEnabled() {
  const stored = localStorage.getItem(AI_INLINE_COMPLETE_KEY);
  if (stored === null) return true;
  return stored === '1';
}

/** Cursor → Gemini → Claude — whichever is configured first. */
function getAiInlineProviderAvailable() {
  if (state.cursorConfigured && state.cursorBridgeOk) return true;
  if (state.geminiConfigured) return true;
  if (state.anthropicConfigured) return true;
  if (state.bedrockConfigured) return true;
  return false;
}

function ensureAiInlineCompleteDefault(enabled) {
  if (localStorage.getItem(AI_INLINE_COMPLETE_KEY) === null && enabled) {
    setAiInlineCompleteEnabled(true);
  }
}

function setAiInlineCompleteEnabled(enabled) {
  localStorage.setItem(AI_INLINE_COMPLETE_KEY, enabled ? '1' : '0');
  const checkbox = $('#settings-ai-inline-complete');
  if (checkbox) checkbox.checked = enabled;
}

function getShowDotfiles() {
  const stored = localStorage.getItem(SHOW_DOTFILES_KEY);
  if (stored === null) return false;
  return stored === '1' || stored === 'true';
}

function setShowDotfiles(show) {
  localStorage.setItem(SHOW_DOTFILES_KEY, show ? '1' : '0');
  syncDotfilesControls(show);
  if (state.repo) {
    void refreshTree().then(() => refreshGitStatus());
  }
}

function getNewWindowOnRepoChange() {
  const stored = localStorage.getItem(NEW_WINDOW_ON_REPO_KEY);
  if (stored === null) return true;
  return stored === '1' || stored === 'true';
}

function setNewWindowOnRepoChange(enabled) {
  localStorage.setItem(NEW_WINDOW_ON_REPO_KEY, enabled ? '1' : '0');
  const checkbox = $('#settings-new-window-on-repo');
  if (checkbox) checkbox.checked = enabled;
}

function buildRepoWindowUrl(repoName) {
  const url = new URL(window.location.href);
  url.searchParams.set('repo', repoName);
  return url.toString();
}

function openRepoInNewWindow(repoName) {
  const url = buildRepoWindowUrl(repoName);
  if (window.ipc?.postMessage) {
    window.ipc.postMessage(JSON.stringify({ type: 'open-repo-window', url }));
    return true;
  }
  const popup = window.open(url, '_blank', 'noopener');
  if (!popup) {
    toast('Could not open a new window. Allow popups or turn off the setting in Settings → Appearance.', 'error');
    return false;
  }
  return true;
}

function isWelcomeVisible() {
  const empty = $('#empty-state');
  return !!(empty && !empty.classList.contains('hidden'));
}

function shouldOpenRepoInNewWindow(repoName) {
  // From the welcome screen (no file open), always load in this window.
  if (isWelcomeVisible() || !state.activeTab) return false;
  return !!(
    repoName
    && state.repo
    && repoName !== state.repo
    && getNewWindowOnRepoChange()
  );
}

function requestRepoSelection(repoName, { revertSelect = true } = {}) {
  if (!repoName) return;
  hideRepoPicker();
  if (shouldOpenRepoInNewWindow(repoName)) {
    setRepoPickerLabel(state.repo || '');
    if (!openRepoInNewWindow(repoName)) {
      setRepoPickerLabel(repoName);
      void selectRepo(repoName);
    }
    return;
  }
  setRepoPickerLabel(repoName);
  void selectRepo(repoName);
}

function updateWindowTitle() {
  const project = state.repo || '';
  const el = $('#window-title-project');
  if (el) el.textContent = project;
  // Live screenshot capture uses the window title as a ready signal.
  if (window.__reaperCapturePhase === 'busy' || window.__reaperCapturePhase === 'ready') {
    const id = window.__reaperDemoNavId || '';
    document.title = `Reaper · ${window.__reaperCapturePhase}:${id}`;
    return;
  }
  document.title = project ? `Reaper — ${project}` : 'Reaper';
}

function getInitialRepoFromUrl() {
  const repo = new URLSearchParams(window.location.search).get('repo')?.trim();
  return repo || null;
}

function shouldSkipAutoRepoOpen() {
  return new URLSearchParams(window.location.search).has('norepo');
}

function shouldShowWelcomeShowcase() {
  return new URLSearchParams(window.location.search).get('showcase') !== '0';
}

async function applyCaptureDemoFromUrl() {
  const params = new URLSearchParams(window.location.search);
  if (!params.has('capture')) return;
  window.__reaperCaptureDone = false;
  dismissLaunchSplashNow();
  // Product-demo default: Deep Navy (override with ?theme=…).
  window.ReaperThemes?.applyTheme?.(params.get('theme') || 'navy');
  await new Promise((r) => setTimeout(r, 800));
  const mode = params.get('capture');
  const hold = async (ms) => { await new Promise((r) => setTimeout(r, ms)); };

  const wantRepo = params.get('repo')?.trim();
  if (wantRepo) {
    for (let i = 0; i < 60 && state.repo !== wantRepo; i++) await hold(250);
    await hold(600);
  }

  const seedDebugStopped = (filePath) => {
    const file = filePath || state.activeTab || 'OrderController.java';
    const short = file.split('/').pop() || file;
    state.debugActive = true;
    state.debugPanelOpen = true;
    state.debugCapabilities = { supported: true, language: 'Java', adapter: 'java' };
    state.debugState = {
      status: 'stopped',
      stop_reason: 'breakpoint',
      language: 'Java',
      adapter: 'Java Debug',
      message: null,
      frames: [
        { id: 1, name: 'create', path: file, line: 22, column: 5 },
        { id: 2, name: 'invoke0', path: 'jdk.internal.reflect.NativeMethodAccessorImpl', line: 77, column: 1 },
        { id: 3, name: 'doFilter', path: 'org.apache.catalina.core.ApplicationFilterChain', line: 168, column: 1 },
      ],
      variables: [
        { name: 'this', value: `${short.replace('.java', '')}@4a3f2c1` },
        { name: 'request', value: 'CreateOrderRequest(sku="SKU-1", qty=2)' },
        { name: 'orderService', value: 'OrderService@1b2c3d4' },
        { name: 'id', value: '"ord-1001"' },
      ],
    };
    state.debugWatch = [
      { expr: 'request.qty()', value: '2' },
      { expr: 'orderService', value: 'OrderService@1b2c3d4' },
    ];
    const key = normalizeDebugBreakpointPath(file);
    state.debugBreakpoints = new Map([[key, new Set([18, 22, 31])]]);
    applyDebugPanelLayout();
    renderDebugPanel();
    renderBreakpointGlyphs();
    $('#tb-debug')?.classList.remove('hidden');
    syncDebugToolbar();
  };

  try {
  if (mode === 'file') {
    const path = params.get('path')?.trim();
    if (path && state.repo) await openFile(path, { silent: true });
    await hold(Number(params.get('hold') || 3500));
    return;
  }

  if (mode === 'panel') {
    const panel = params.get('panel');
    if (panel === 'agent') {
      if (state.agentDock !== 'left') toggleAgent();
      else switchPanel('agent');
    } else if (panel === 'terminal') {
      showTerminal();
    } else if (panel === 'coverage') {
      showCoveragePanel();
      await hold(800);
      await refreshCoveragePanel(state.activeTab);
    } else if (panel === 'debug') {
      showDebugPanel();
    } else if (panel === 'db' || panel === 'db-viewer') {
      showDbViewerPanel();
    } else if (panel === 'git-viewer' || panel === 'git-console') {
      showGitViewerPanel();
    } else if (panel === 'docker' || panel === 'docker-logs') {
      showDockerLogsPanel();
    } else if (panel === 'build-tasks') {
      showBuildTasksPanel();
    } else if (panel === 'package-manifest') {
      showPackageManifestPanel();
    } else if (panel) {
      switchPanel(panel);
    }
    await hold(Number(params.get('hold') || (panel === 'history' || panel === 'build-tasks' ? 3500 : 2200)));
    return;
  }

  if (mode === 'feature') {
    const feature = params.get('feature');
    const path = params.get('path')?.trim();
    if (path && state.repo) {
      await openFile(path, { silent: true });
      await hold(1500);
    }

    if (feature === 'build-tasks') {
      showBuildTasksPanel();
      await hold(5000);
    } else if (feature === 'package-manifest') {
      showPackageManifestPanel();
      await hold(3000);
    } else if (feature === 'search') {
      showSearchEverywhere(params.get('q') || 'Order');
      await hold(2500);
    } else if (feature === 'palette') {
      showPalette();
      await hold(2000);
    } else if (feature === 'go-to-class') {
      showGoToClass(params.get('q') || 'Order');
      await hold(2500);
    } else if (feature === 'go-to-line') {
      showGoToLine();
      await hold(2000);
    } else if (feature === 'branch') {
      showBranchPicker();
      await hold(2500);
    } else if (feature === 'repo-info') {
      await showRepoInfoModal();
      await hold(2500);
    } else if (feature === 'publish') {
      showPublishModal();
      await hold(2200);
    } else if (feature === 'clone' || feature === 'import') {
      showCloneModal();
      await hold(2200);
    } else if (feature === 'new-repo') {
      showModal();
      await hold(2000);
    } else if (feature === 'new-file') {
      showFileModal();
      await hold(2000);
    } else if (feature === 'settings') {
      await showSettingsModal(params.get('tab') || 'git');
      await hold(2500);
    } else if (feature === 'theme' || feature === 'theme-select') {
      const themeId = params.get('theme') || 'navy';
      window.ReaperThemes?.applyTheme?.(themeId);
      await hold(400);
      document.getElementById('reaper-capture-theme-panel')?.remove();
      const themes = window.ReaperThemes?.listThemes?.() || [
        { id: 'darcula', label: 'Darcula (Classic)' },
        { id: 'charcoal', label: 'Charcoal Dark' },
        { id: 'navy', label: 'Deep Navy' },
        { id: 'blueblack', label: 'Blue & Black' },
        { id: 'offwhite', label: 'Off-White Light' },
        { id: 'mono', label: 'Black & White' },
      ];
      const active = window.ReaperThemes?.getStoredTheme?.() || themeId;
      const panel = document.createElement('div');
      panel.id = 'reaper-capture-theme-panel';
      panel.setAttribute('role', 'listbox');
      panel.setAttribute('aria-label', 'Color theme');
      panel.style.cssText = [
        'position:fixed', 'right:24px', 'bottom:48px', 'z-index:2147483646',
        'min-width:260px', 'padding:10px 0',
        'background:var(--ij-panel,#2b2d30)', 'color:var(--ij-text,#bcbec4)',
        'border:1px solid var(--ij-border,#3a3f4b)', 'border-radius:10px',
        'box-shadow:0 12px 32px rgba(0,0,0,.45)',
        'font:13px -apple-system,BlinkMacSystemFont,Segoe UI,sans-serif',
      ].join(';');
      panel.innerHTML = `<div style="padding:4px 14px 10px;font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;opacity:.7">Color theme</div>${
        themes.map((t) => {
          const on = t.id === active;
          return `<div role="option" aria-selected="${on ? 'true' : 'false'}" style="padding:8px 14px;display:flex;justify-content:space-between;gap:16px;${on ? 'background:rgba(110,181,255,.16);color:#fff' : ''}">
            <span>${String(t.label || t.id).replace(/</g, '&lt;')}</span>
            ${on ? '<span style="opacity:.85">✓</span>' : ''}
          </div>`;
        }).join('')
      }`;
      document.body.appendChild(panel);
      $('#theme-select')?.focus();
      await hold(2800);
    } else if (feature === 'theme-cycle') {
      const ids = (params.get('themes') || 'navy,darcula,charcoal,blueblack,offwhite,mono').split(',').map((s) => s.trim()).filter(Boolean);
      for (const id of ids) {
        window.ReaperThemes?.applyTheme?.(id);
        await hold(900);
      }
      window.ReaperThemes?.applyTheme?.('navy');
    } else if (feature === 'coverage') {
      showCoveragePanel();
      await hold(500);
      const covPath = state.activeTab;
      const summary = await refreshCoveragePanel(covPath);
      if (covPath && state.repo) {
        try {
          const cov = await api(
            `${repoApi(state.repo, '/workspace/coverage')}?path=${encodeURIComponent(stripJavaDiagOverlayPath(covPath))}`,
          );
          if (cov?.lines?.length) {
            applyCoverageDecorations(covPath, cov);
            updateCoverageStatus(cov);
          }
        } catch { /* panel still shows report */ }
      }
      if (!summary?.files?.length && !summary?.totals) {
        renderCoveragePanel({
          project_root: state.repo,
          query_path: covPath,
          totals: { lines: { covered: 42, missed: 18, total: 60, rate: 0.7 }, branches: { covered: 8, missed: 4, total: 12, rate: 0.67 } },
          current_file: { path: covPath, name: (covPath || '').split('/').pop(), lines: { covered: 18, missed: 6, total: 24, rate: 0.75 } },
          files: [
            { path: covPath, name: (covPath || 'File.java').split('/').pop(), lines: { covered: 18, missed: 6, total: 24, rate: 0.75 } },
          ],
          report_path: 'target/site/jacoco/jacoco.xml',
        });
      }
      await hold(3200);
    } else if (feature === 'debug' || feature === 'debug-stopped') {
      seedDebugStopped(path || state.activeTab);
      if (state.editor && state.debugState.frames?.[0]?.line) {
        state.editor.setPosition({ lineNumber: state.debugState.frames[0].line, column: 1 });
        highlightDebugCurrentLine();
      }
      await hold(3200);
    } else if (feature === 'debug-panel') {
      state.debugActive = false;
      state.debugState = { status: 'idle', frames: [], variables: [], breakpoints: [] };
      showDebugPanel();
      $('#tb-debug')?.classList.remove('hidden');
      syncDebugToolbar();
      await hold(2500);
    } else if (feature === 'debug-controls' || feature === 'debug-toolbar') {
      seedDebugStopped(path || state.activeTab);
      await hold(2800);
    } else if (feature === 'debug-breakpoints') {
      const file = path || state.activeTab;
      const key = normalizeDebugBreakpointPath(file);
      state.debugBreakpoints = new Map([[key, new Set([12, 18, 22, 31, 40])]]);
      state.debugPanelOpen = true;
      applyDebugPanelLayout();
      renderDebugBreakpointsList();
      renderBreakpointGlyphs();
      showDebugPanel();
      $('#tb-debug')?.classList.remove('hidden');
      await hold(2800);
    } else if (feature === 'debug-watch') {
      seedDebugStopped(path || state.activeTab);
      state.debugWatch = [
        { expr: 'request.qty()', value: '2' },
        { expr: 'id', value: '"ord-1001"' },
        { expr: 'orderService.getClass().getSimpleName()', value: '"OrderService"' },
      ];
      renderDebugWatchList();
      await hold(2800);
    } else if (feature === 'db' || feature === 'db-viewer') {
      showDbViewerPanel();
      await hold(2800);
    } else if (feature === 'git-viewer' || feature === 'git-console') {
      showGitViewerPanel();
      await hold(2800);
    } else if (feature === 'docker' || feature === 'docker-logs') {
      showDockerLogsPanel();
      await hold(2800);
    } else if (feature === 'blame') {
      if (!state.blameEnabled) $('#tb-blame')?.click();
      await hold(2500);
    } else if (feature === 'ai-fix' || feature === 'quickfix') {
      const pop = $('#ai-quickfix-popover');
      if (pop) {
        pop.classList.remove('hidden', 'ij-cascade-popover');
        pop.innerHTML = [
          '<button type="button" class="ij-quickfix-item">AI: Suggest fix</button>',
          '<button type="button" class="ij-quickfix-item">Local: Organize imports</button>',
          '<button type="button" class="ij-quickfix-item">Local: Create method</button>',
        ].join('');
        positionPopoverNearAnchor(pop, aiFixMenuAnchor());
      }
      await hold(2500);
    } else if (feature === 'refactor') {
      let menuItems = [];
      try {
        const pos = state.editor?.getPosition?.();
        if (state.repo && pos && state.activeTab) {
          const body = {
            path: stripJavaDiagOverlayPath(state.activeTab),
            line: pos.lineNumber,
            column: pos.column,
            only: ['refactor', 'refactor.extract', 'refactor.inline', 'refactor.rewrite', 'source'],
          };
          const actions = await api(repoApi(state.repo, '/workspace/java/code-actions'), {
            method: 'POST',
            body: JSON.stringify(body),
            timeoutMs: 20_000,
          });
          menuItems = (actions || [])
            .filter((a) => a?.title)
            .slice(0, 12)
            .map((a) => ({
              title: a.title || 'Refactor',
              provider: 'local',
              edits: a.edits?.length ? a.edits : [{ path: 'demo', range: {}, newText: '' }],
              kind: a.kind,
            }));
        }
      } catch { /* fall through */ }
      if (!menuItems.length) {
        menuItems = [
          { title: 'Extract Method', kind: 'refactor.extract.function', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
          { title: 'Extract Variable', kind: 'refactor.extract.variable', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
          { title: 'Extract Constant', kind: 'refactor.extract.constant', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
          { title: 'Inline Variable', kind: 'refactor.inline', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
          { title: 'Inline Method', kind: 'refactor.inline', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
          { title: 'Convert anonymous to nested', kind: 'refactor.rewrite', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
          { title: 'Organize Imports', kind: 'source.organizeImports', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
        ];
      }
      showRefactorStaircaseMenu(menuItems, () => {}, aiFixMenuAnchor());
      await hold(3200);
    } else if (feature === 'rename') {
      void showRenamePrompt({ title: 'Rename', subtitle: 'Symbol', value: 'orderService' });
      await hold(2200);
    } else if (feature === 'find-usages') {
      showJavaReferences([
        { path: path || 'OrderController.java', line: 12, preview: 'orderService.create(order)' },
        { path: path || 'OrderService.java', line: 40, preview: 'public Order create(Order order)' },
      ], 'Find Usages');
      await hold(2500);
    } else if (feature === 'rebase') {
      switchPanel('history');
      showRebasePanel();
      await hold(2500);
    } else if (feature === 'font') {
      await showSettingsModal('appearance');
      $('#settings-editor-font-size')?.focus();
      await hold(2500);
    } else if (feature === 'toolbar') {
      $('#tb-debug')?.classList.remove('hidden');
      $('#tb-blame')?.classList.remove('hidden');
      await hold(2000);
    } else if (feature === 'push') {
      try {
        await showPushModal();
      } catch { /* preview may fail without remote */ }
      // Seed a visible push preview if the live preview is empty/failed.
      const body = $('#push-modal-body');
      if (body && (!body.innerHTML || body.innerHTML.includes('Loading') || body.innerHTML.length < 40)) {
        body.innerHTML = `
          <div class="ij-push-preview">
            <p class="ij-push-summary"><strong>main</strong> → <code>origin/main</code> · 2 commits ahead</p>
            <ul class="ij-push-file-list panel-scroll">
              <li>M  services/order-service/.../OrderController.java</li>
              <li>A  services/order-service/.../OrderService.java</li>
            </ul>
          </div>`;
        $('#push-modal-overlay')?.classList.remove('hidden');
        $('#push-modal-overlay')?.classList.add('flex');
        const confirm = $('#push-modal-confirm');
        if (confirm) { confirm.disabled = false; confirm.textContent = 'Push'; }
      }
      await hold(2800);
    } else if (feature === 'diff') {
      showDiffInMainArea('OrderController.java', [
        '--- a/OrderController.java',
        '+++ b/OrderController.java',
        '@@ -20,7 +20,10 @@',
        ' @RequiredArgsConstructor',
        ' public class OrderController {',
        '-    private final OrderService orderService;',
        '+    private final OrderService orderService;',
        '+',
        '+    /** Create a new order. */',
        '     @PostMapping',
        '     public ResponseEntity<ApiResponse<OrderResponse>> create(',
      ].join('\n'));
      await hold(2800);
    } else if (feature === 'conflict') {
      const sample = [
        'package com.example.order.web;',
        '',
        'public class OrderController {',
        '<<<<<<< HEAD',
        '    private final OrderService orderService;',
        '=======',
        '    private final OrderService orders;',
        '>>>>>>> feature/rename',
        '}',
      ].join('\n');
      const conflictPath = path || state.activeTab || 'OrderController.java';
      if (state.editor) {
        state.editor.setValue(sample);
        state.conflictFiles = new Set([conflictPath]);
        state.conflictPanelHidden = false;
        updateConflictUi();
      }
      await hold(3000);
    } else if (feature === 'secrets') {
      const findings = [
        { path: '.env', line: 2, reason: 'AWS access key pattern' },
        { path: 'config/secrets.yml', line: 8, reason: 'Private key block' },
      ];
      await showPushModal().catch(() => {});
      const body = $('#push-modal-body');
      if (body) {
        body.innerHTML = `${renderSecretWarningsHtml(findings)}
          <div class="ij-push-preview" style="margin-top:12px">
            <p class="ij-push-summary">Review secrets before push</p>
          </div>`;
        $('#push-modal-overlay')?.classList.remove('hidden');
        $('#push-modal-overlay')?.classList.add('flex');
      }
      await hold(2800);
    } else if (feature === 'import-local' || feature === 'clone-local') {
      showCloneModal('local');
      await hold(2500);
    } else if (feature === 'repo-picker' || feature === 'open-repo') {
      showRepoPicker();
      await hold(2200);
    } else if (feature === 'inline-complete' || feature === 'ghost-text') {
      document.getElementById('reaper-capture-ghost')?.remove();
      const ghost = document.createElement('div');
      ghost.id = 'reaper-capture-ghost';
      ghost.setAttribute('aria-label', 'Inline AI completion');
      ghost.style.cssText = [
        'position:fixed', 'left:42%', 'top:38%', 'z-index:2147483645',
        'font:13px/1.45 ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace',
        'color:rgba(180,190,200,.55)', 'pointer-events:none',
        'background:rgba(20,28,40,.72)', 'padding:10px 14px', 'border-radius:8px',
        'border:1px solid rgba(110,181,255,.25)', 'max-width:420px',
      ].join(';');
      ghost.innerHTML = '<div style="font-size:10px;letter-spacing:.06em;text-transform:uppercase;opacity:.7;margin-bottom:6px">Inline completion</div>'
        + '<span style="color:#c8cdd3">return orderService.create(request);</span>';
      document.body.appendChild(ghost);
      await hold(2800);
    } else if (feature === 'format') {
      toast('Reformatted OrderController.java', 'success');
      await hold(2200);
    } else if (feature === 'organize-imports') {
      showRefactorStaircaseMenu([
        { title: 'Organize Imports', kind: 'source.organizeImports', edits: [{ path: 'demo', range: {}, newText: '' }], provider: 'local' },
      ], () => {}, aiFixMenuAnchor());
      await hold(2500);
    } else if (feature === 'run' || feature === 'gutter-run') {
      $('#tb-run')?.classList.remove('hidden');
      $('#tb-debug')?.classList.remove('hidden');
      if ($('#tb-run')) $('#tb-run').disabled = false;
      showTerminal();
      try {
        terminalLog('> ./gradlew :order-service:bootRun\n');
        terminalLog('BUILD SUCCESSFUL in 4s\n');
        terminalLog('Started OrderServiceApplication in 2.1 seconds\n');
      } catch { /* ignore */ }
      await hold(2800);
    } else if (feature === 'cherry-pick') {
      switchPanel('history');
      await hold(800);
      // Ensure cherry-pick actions are visible in the log list.
      const list = $('#commit-history');
      if (list && !list.innerHTML.includes('cherry-pick')) {
        list.innerHTML = `
          <div class="ij-commit-row">
            <div class="ij-commit-meta"><code>a1b2c3d</code> · main · 2 hours ago</div>
            <div class="ij-commit-msg">Add order create endpoint</div>
            <button type="button" class="ij-commit-action" data-action="cherry-pick">Cherry-pick</button>
          </div>
          <div class="ij-commit-row">
            <div class="ij-commit-meta"><code>d4e5f6a</code> · main · yesterday</div>
            <div class="ij-commit-msg">Wire OrderService</div>
            <button type="button" class="ij-commit-action" data-action="cherry-pick">Cherry-pick</button>
          </div>`;
      }
      await hold(2800);
    } else if (feature === 'agent-providers' || feature === 'multi-provider') {
      if (state.agentDock !== 'left') toggleAgent();
      else switchPanel('agent');
      await hold(400);
      document.getElementById('reaper-capture-providers')?.remove();
      const panel = document.createElement('div');
      panel.id = 'reaper-capture-providers';
      panel.style.cssText = [
        'position:fixed', 'right:28px', 'top:120px', 'z-index:2147483646',
        'min-width:240px', 'padding:10px 0',
        'background:var(--ij-panel,#2b2d30)', 'color:var(--ij-text,#bcbec4)',
        'border:1px solid var(--ij-border,#3a3f4b)', 'border-radius:10px',
        'box-shadow:0 12px 32px rgba(0,0,0,.45)',
        'font:13px -apple-system,BlinkMacSystemFont,Segoe UI,sans-serif',
      ].join(';');
      panel.innerHTML = `<div style="padding:4px 14px 10px;font-size:11px;font-weight:600;letter-spacing:.04em;text-transform:uppercase;opacity:.7">Agent provider</div>
        <div style="padding:8px 14px;background:rgba(110,181,255,.16);color:#fff;display:flex;justify-content:space-between"><span>Cursor agent</span><span>✓</span></div>
        <div style="padding:8px 14px">Gemini agent</div>
        <div style="padding:8px 14px">Claude (Anthropic)</div>
        <div style="padding:8px 14px">Bedrock</div>`;
      document.body.appendChild(panel);
      await hold(2800);
    } else if (feature === 'terminal-bottom') {
      setTerminalDock('bottom');
      showTerminal();
      await hold(2500);
    } else if (feature === 'jdk' || feature === 'toolchain') {
      await showSettingsModal('compilers');
      await hold(2500);
    }
  }
  } finally {
    window.__reaperCaptureDone = true;
  }
}

function syncDockMenuControls() {
  ['left', 'right', 'bottom'].forEach((dock) => {
    $$(`.ij-menu-item[data-action="terminal-dock-${dock}"]`).forEach((el) => {
      const on = state.terminalDock === dock;
      el.classList.toggle('checked', on);
      el.setAttribute('aria-pressed', on ? 'true' : 'false');
    });
    $$(`.ij-menu-item[data-action="agent-dock-${dock}"]`).forEach((el) => {
      const on = state.agentDock === dock;
      el.classList.toggle('checked', on);
      el.setAttribute('aria-pressed', on ? 'true' : 'false');
    });
    $$(`.ij-menu-item[data-action="docker-logs-dock-${dock}"]`).forEach((el) => {
      const on = state.dockerLogsDock === dock;
      el.classList.toggle('checked', on);
      el.setAttribute('aria-pressed', on ? 'true' : 'false');
    });
  });
  ['left', 'right'].forEach((dock) => {
    $$(`.ij-menu-item[data-action="build-tasks-dock-${dock}"]`).forEach((el) => {
      const on = state.buildTasksDock === dock;
      el.classList.toggle('checked', on);
      el.setAttribute('aria-pressed', on ? 'true' : 'false');
    });
    $$(`.ij-menu-item[data-action="package-manifest-dock-${dock}"]`).forEach((el) => {
      const on = state.packageManifestDock === dock;
      el.classList.toggle('checked', on);
      el.setAttribute('aria-pressed', on ? 'true' : 'false');
    });
  });
}

function syncDotfilesControls(show) {
  const on = !!show;
  $$('[data-action="toggle-dotfiles"]').forEach((el) => {
    el.classList.toggle('checked', on);
    el.setAttribute('aria-pressed', on ? 'true' : 'false');
  });
  const sidebarBtn = $('#btn-toggle-dotfiles');
  if (sidebarBtn) {
    sidebarBtn.classList.toggle('active', on);
    sidebarBtn.title = on
      ? 'Hide dotfiles & Gradle wrapper'
      : 'Show dotfiles & Gradle wrapper (gradlew, gradle/wrapper, …)';
  }
  const settings = $('#settings-show-dotfiles');
  if (settings) settings.checked = on;
}

function populateFontSizeSelects() {
  const ids = ['settings-editor-font-size', 'editor-font-size-menu', 'editor-font-size-status'];
  ids.forEach((id) => {
    const select = document.getElementById(id);
    if (!select || select.dataset.populated) return;
    select.dataset.populated = '1';
    for (let n = MIN_EDITOR_FONT_SIZE; n <= MAX_EDITOR_FONT_SIZE; n += 1) {
      const opt = document.createElement('option');
      opt.value = String(n);
      opt.textContent = `${n}px`;
      select.appendChild(opt);
    }
    select.value = String(getEditorFontSize());
    select.addEventListener('change', onEditorFontSizeChange);
    select.addEventListener('mousedown', (e) => e.stopPropagation());
  });
}

function populateFontFamilySelects() {
  const select = document.getElementById('settings-editor-font-family');
  if (!select || select.dataset.populated) return;
  select.dataset.populated = '1';
  EDITOR_FONTS.forEach((font) => {
    const opt = document.createElement('option');
    opt.value = font.id;
    opt.textContent = font.label;
    select.appendChild(opt);
  });
  select.value = getEditorFontSpec().id;
  select.addEventListener('change', onEditorFontFamilyChange);
}

function syncFontFamilyControls(fontId) {
  const el = document.getElementById('settings-editor-font-family');
  if (el && el.value !== fontId) el.value = fontId;
}

function syncFontSizeControls(size) {
  const value = String(size);
  ['settings-editor-font-size', 'editor-font-size-menu', 'editor-font-size-status'].forEach((id) => {
    const el = document.getElementById(id);
    if (el && el.value !== value) el.value = value;
  });
}

function applyAgentTypography() {
  const size = getAgentFontSize();
  const spec = getAgentFontSpec();
  ensureEditorFontLoaded(spec);
  const panel = $('#panel-agent');
  if (!panel) return;
  panel.style.setProperty('--ij-ui-font-size', `${size}px`);
  panel.style.setProperty('--ij-ui-font-family', spec.family);
  panel.style.setProperty('--ij-ui-line-height', String(20 / 11));
}

function applyAgentFontSize(size) {
  if (getAgentFontMatchEditor()) return getEditorFontSize();
  const clamped = Math.min(MAX_EDITOR_FONT_SIZE, Math.max(MIN_EDITOR_FONT_SIZE, Math.round(size)));
  localStorage.setItem(AGENT_FONT_SIZE_KEY, String(clamped));
  applyAgentTypography();
  syncAgentFontControls();
  return clamped;
}

function applyAgentFontFamily(fontId) {
  if (getAgentFontMatchEditor()) return getEditorFontSpec();
  const spec = EDITOR_FONTS.find((f) => f.id === fontId) || EDITOR_FONTS[0];
  localStorage.setItem(AGENT_FONT_FAMILY_KEY, spec.id);
  ensureEditorFontLoaded(spec);
  applyAgentTypography();
  syncAgentFontControls();
  updateAgentFontPreview(spec);
  return spec;
}

function setAgentFontMatchEditor(match) {
  if (!match) {
    if (!localStorage.getItem(AGENT_FONT_SIZE_KEY)) {
      localStorage.setItem(AGENT_FONT_SIZE_KEY, String(getEditorFontSize()));
    }
    if (!localStorage.getItem(AGENT_FONT_FAMILY_KEY)) {
      localStorage.setItem(AGENT_FONT_FAMILY_KEY, getEditorFontSpec().id);
    }
  }
  localStorage.setItem(AGENT_FONT_MATCH_EDITOR_KEY, match ? '1' : '0');
  syncAgentFontControls();
  applyAgentTypography();
}

function applyUiTypography() {
  const size = getEditorFontSize();
  const spec = getEditorFontSpec();
  ensureEditorFontLoaded(spec);
  document.documentElement.style.setProperty('--ij-ui-font-size', `${size}px`);
  document.documentElement.style.setProperty('--ij-ui-font-family', spec.family);
}

function syncTerminalFontSize() {
  syncTerminalTypography();
}

function terminalCssVar(name, fallback) {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

function terminalThemeFromApp() {
  const fg = terminalCssVar('--ij-terminal-fg', terminalCssVar('--ij-text-bright', '#a9b7c6'));
  const bg = terminalCssVar('--ij-terminal-bg', terminalCssVar('--ij-bg', '#2b2b2b'));
  const accent = terminalCssVar('--ij-accent', '#589df6');
  const selection = terminalCssVar('--ij-terminal-selection', terminalCssVar('--ij-selection', '#214283'));
  return {
    background: bg,
    foreground: fg,
    cursor: accent,
    cursorAccent: terminalCssVar('--ij-text-heading', '#ffffff'),
    selectionBackground: selection,
    selectionForeground: terminalCssVar('--ij-text-heading', '#ffffff'),
    black: '#484848',
    red: terminalCssVar('--ij-deleted', '#bc3f3c'),
    green: terminalCssVar('--ij-added', '#629755'),
    yellow: terminalCssVar('--ij-modified', '#ffc66d'),
    blue: accent,
    magenta: '#b589d6',
    cyan: '#56b6c2',
    white: fg,
    brightBlack: terminalCssVar('--ij-text-dim', '#808080'),
    brightRed: '#f44747',
    brightGreen: terminalCssVar('--ij-run-hover', '#73c04d'),
    brightYellow: '#ffcc66',
    brightBlue: terminalCssVar('--ij-accent-hover', '#6ba6f7'),
    brightMagenta: '#d8a0ff',
    brightCyan: '#7eb8da',
    brightWhite: terminalCssVar('--ij-text-heading', '#ffffff'),
  };
}

function applyTerminalTheme(term) {
  const theme = terminalThemeFromApp();
  if (term?.xterm) {
    term.xterm.options.theme = theme;
  }
}

function syncTerminalTypography() {
  const size = getEditorFontSize();
  const family = getEditorFontSpec().family;
  for (const term of state.terminals || []) {
    if (!term?.xterm) continue;
    term.xterm.options.fontSize = size;
    term.xterm.options.fontFamily = family;
    term.xterm.options.lineHeight = 1.12;
    applyTerminalTheme(term);
    fitTerminal(term);
  }
}

window.syncTerminalTheme = syncTerminalTypography;

function applyEditorFontSize(size) {
  const clamped = Math.min(MAX_EDITOR_FONT_SIZE, Math.max(MIN_EDITOR_FONT_SIZE, Math.round(size)));
  localStorage.setItem(EDITOR_FONT_SIZE_KEY, String(clamped));
  applyUiTypography();
  if (state.editor) {
    state.editor.updateOptions({
      fontSize: clamped,
      lineHeight: editorLineHeightFor(clamped),
    });
    if (state.activeTab?.endsWith('.java') || isNativeSourcePath(state.activeTab)) {
      applyTestRunDecorations();
    }
  }
  if (getAgentFontMatchEditor()) {
    applyAgentTypography();
  }
  syncTerminalTypography();
  syncFontSizeControls(clamped);
  syncAgentFontControls();
  updateEditorFontPreview();
  return clamped;
}

function onEditorFontSizeChange(e) {
  const size = parseInt(e.target.value, 10);
  if (!Number.isFinite(size)) return;
  applyEditorFontSize(size);
  document.querySelectorAll('.ij-menu-root.open').forEach((m) => m.classList.remove('open'));
}

function applyEditorFontFamily(fontId) {
  const spec = EDITOR_FONTS.find((f) => f.id === fontId) || EDITOR_FONTS[0];
  localStorage.setItem(EDITOR_FONT_FAMILY_KEY, spec.id);
  ensureEditorFontLoaded(spec);
  applyUiTypography();
  if (state.editor) {
    state.editor.updateOptions({ fontFamily: spec.family });
  }
  if (getAgentFontMatchEditor()) {
    applyAgentTypography();
  }
  syncFontFamilyControls(spec.id);
  syncAgentFontControls();
  syncTerminalTypography();
  updateEditorFontPreview(spec);
  return spec;
}

function updateEditorFontPreview(spec = getEditorFontSpec()) {
  const preview = $('#settings-editor-font-preview');
  if (!preview) return;
  ensureEditorFontLoaded(spec);
  preview.style.fontFamily = spec.family;
  preview.style.fontSize = `${getEditorFontSize()}px`;
  preview.textContent = 'fn harvest() {\n  return "reaper";\n}';
}

function updateAgentFontPreview(spec = getAgentFontSpec()) {
  const preview = $('#settings-agent-font-preview');
  if (!preview) return;
  ensureEditorFontLoaded(spec);
  preview.style.fontFamily = spec.family;
  preview.style.fontSize = `${getAgentFontSize()}px`;
  preview.textContent = 'Explain what this function does…';
}

function onEditorFontFamilyChange(e) {
  applyEditorFontFamily(e.target.value);
  document.querySelectorAll('.ij-menu-root.open').forEach((m) => m.classList.remove('open'));
}

function populateAgentFontSelects() {
  const sizeSelect = document.getElementById('settings-agent-font-size');
  if (sizeSelect && !sizeSelect.dataset.populated) {
    sizeSelect.dataset.populated = '1';
    for (let n = MIN_EDITOR_FONT_SIZE; n <= MAX_EDITOR_FONT_SIZE; n += 1) {
      const opt = document.createElement('option');
      opt.value = String(n);
      opt.textContent = `${n}px`;
      sizeSelect.appendChild(opt);
    }
    sizeSelect.addEventListener('change', onAgentFontSizeChange);
  }

  const familySelect = document.getElementById('settings-agent-font-family');
  if (familySelect && !familySelect.dataset.populated) {
    familySelect.dataset.populated = '1';
    EDITOR_FONTS.forEach((font) => {
      const opt = document.createElement('option');
      opt.value = font.id;
      opt.textContent = font.label;
      familySelect.appendChild(opt);
    });
    familySelect.addEventListener('change', onAgentFontFamilyChange);
  }
}

function syncAgentFontControls() {
  const match = getAgentFontMatchEditor();
  const matchCb = $('#settings-agent-font-match');
  if (matchCb) matchCb.checked = match;

  const sizeEl = $('#settings-agent-font-size');
  if (sizeEl) {
    sizeEl.value = String(getAgentFontSize());
    sizeEl.disabled = match;
  }

  const familyEl = $('#settings-agent-font-family');
  const fontId = getAgentFontSpec().id;
  if (familyEl) {
    if (familyEl.value !== fontId) familyEl.value = fontId;
    familyEl.disabled = match;
  }

  $('#settings-agent-font-custom')?.classList.toggle('hidden', match);

  const summary = $('#settings-agent-font-summary');
  if (summary) {
    if (match) {
      const spec = getEditorFontSpec();
      summary.textContent = `Using editor font: ${getEditorFontSize()}px · ${spec.label}`;
      summary.classList.remove('hidden');
    } else {
      summary.textContent = '';
      summary.classList.add('hidden');
    }
  }

  updateAgentFontPreview();
}

function onAgentFontSizeChange(e) {
  if (getAgentFontMatchEditor()) return;
  const size = parseInt(e.target.value, 10);
  if (!Number.isFinite(size)) return;
  applyAgentFontSize(size);
}

function onAgentFontFamilyChange(e) {
  if (getAgentFontMatchEditor()) return;
  applyAgentFontFamily(e.target.value);
}

function onAgentFontMatchChange(e) {
  setAgentFontMatchEditor(e.target.checked);
}

function loadAgentFontSettingsSection() {
  populateAgentFontSelects();
  syncAgentFontControls();
}

function loadAppearanceSettingsSection() {
  populateFontSizeSelects();
  populateFontFamilySelects();
  loadAgentFontSettingsSection();
  applyAgentTypography();
  syncFontSizeControls(getEditorFontSize());
  syncFontFamilyControls(getEditorFontSpec().id);
  updateEditorFontPreview();
  const autoSave = $('#settings-auto-save');
  if (autoSave) {
    autoSave.checked = getAutoSaveEnabled();
    if (!autoSave.dataset.bound) {
      autoSave.dataset.bound = '1';
      autoSave.addEventListener('change', (e) => setAutoSaveEnabled(e.target.checked));
    }
  }
  const dotfiles = $('#settings-show-dotfiles');
  if (dotfiles) {
    dotfiles.checked = getShowDotfiles();
    if (!dotfiles.dataset.bound) {
      dotfiles.dataset.bound = '1';
      dotfiles.addEventListener('change', (e) => setShowDotfiles(e.target.checked));
    }
  }
  syncDotfilesControls(getShowDotfiles());
  void populateDefaultRepoSelect();
  void syncGitBackgroundFetchSetting();
  const newWindowOnRepo = $('#settings-new-window-on-repo');
  if (newWindowOnRepo) {
    newWindowOnRepo.checked = getNewWindowOnRepoChange();
    if (!newWindowOnRepo.dataset.bound) {
      newWindowOnRepo.dataset.bound = '1';
      newWindowOnRepo.addEventListener('change', (e) => setNewWindowOnRepoChange(e.target.checked));
    }
  }
}

async function syncGitBackgroundFetchSetting() {
  const checkbox = $('#settings-git-background-fetch');
  try {
    const general = await api('/api/settings/general');
    state.gitBackgroundFetch = !!general?.git_background_fetch;
  } catch {
    state.gitBackgroundFetch = false;
  }
  if (checkbox) checkbox.checked = state.gitBackgroundFetch;
  if (checkbox && !checkbox.dataset.bound) {
    checkbox.dataset.bound = '1';
    checkbox.addEventListener('change', async (e) => {
      const enabled = !!e.target.checked;
      try {
        await api('/api/settings/general', {
          method: 'PATCH',
          body: JSON.stringify({ git_background_fetch: enabled }),
        });
        state.gitBackgroundFetch = enabled;
        if (enabled) lastRemoteFetchMs = 0;
        toast(enabled ? 'Background fetch enabled' : 'Background fetch disabled', 'success');
      } catch (err) {
        toast(err.message, 'error');
        checkbox.checked = state.gitBackgroundFetch;
      }
    });
  }
}

async function populateDefaultRepoSelect() {
  const select = $('#settings-default-repo');
  if (!select) return;
  try {
    const [general, repos] = await Promise.all([
      api('/api/settings/general'),
      state.repos.length ? Promise.resolve(state.repos) : api('/api/repos'),
    ]);
    const current = general?.default_repo || '';
    const names = (repos || []).map((r) => r.name);
    select.innerHTML = '<option value="">None — show welcome screen</option>'
      + names.map((name) => `<option value="${escapeHtml(name)}"${name === current ? ' selected' : ''}>${escapeHtml(name)}</option>`).join('');
    await updateDefaultRepoFolderDisplay(current, repos || []);
    if (!select.dataset.bound) {
      select.dataset.bound = '1';
      select.addEventListener('change', async (e) => {
        const value = e.target.value.trim();
        try {
          await api('/api/settings/general', {
            method: 'PATCH',
            body: JSON.stringify({ default_repo: value || null }),
          });
          toast(value ? `Default repo set to ${value}` : 'Default repo cleared', 'success');
          await updateDefaultRepoFolderDisplay(value, state.repos);
        } catch (err) {
          toast(err.message, 'error');
          populateDefaultRepoSelect();
        }
      });
    }
  } catch (err) {
    select.innerHTML = `<option value="">Could not load: ${escapeHtml(err.message)}</option>`;
  }
}

async function updateDefaultRepoFolderDisplay(repoName, repos) {
  const wrap = $('#settings-default-repo-folder');
  const pathEl = $('#settings-default-repo-folder-path');
  if (!wrap || !pathEl) return;
  if (!repoName) {
    wrap.classList.add('hidden');
    pathEl.textContent = '';
    return;
  }
  let folder = repos.find((r) => r.name === repoName)?.project_folder || '';
  if (!folder) {
    try {
      const detail = await api(repoApi(repoName));
      folder = detail.summary?.project_folder || detail.project_folder || '';
    } catch {
      folder = '';
    }
  }
  if (folder) {
    pathEl.textContent = folder;
    wrap.classList.remove('hidden');
  } else {
    wrap.classList.add('hidden');
    pathEl.textContent = '';
  }
}

function switchSettingsTab(tab) {
  settingsTab = tab;
  $$('.ij-settings-tab').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.settingsTab === tab);
  });
  $$('.ij-settings-panel').forEach((panel) => {
    panel.classList.toggle('hidden', panel.id !== `settings-panel-${tab}`);
  });
  $('.ij-settings-modal')?.classList.toggle('ij-settings-modal--compiler', tab === 'compilers');
  if (tab === 'compilers') {
    void loadCompilersSettingsSection();
    setTimeout(() => $('#settings-compiler-search')?.focus(), 0);
  }
}

async function loadPatTokensList() {
  const list = $('#settings-pat-list');
  if (!list) return;
  try {
    const tokens = await api('/api/settings/tokens');
    if (!tokens.length) {
      list.innerHTML = '<p class="ij-settings-empty">No tokens saved yet. Add one below for private HTTPS remotes.</p>';
      return;
    }
    list.innerHTML = tokens.map((t) => {
      const removable = t.source === 'settings';
      if (removable && state.patTokenPendingRemove === t.host) {
        return `<div class="ij-settings-token-row ij-settings-token-row--confirm">
          <span class="ij-settings-token-confirm-text">Remove token for <strong>${escapeHtml(t.host)}</strong>?</span>
          <div class="ij-settings-token-confirm-actions">
            <button type="button" class="ij-settings-remove" data-pat-remove-confirm="${escapeHtml(t.host)}">Remove</button>
            <button type="button" class="ij-settings-remove ghost" data-pat-remove-cancel>Cancel</button>
          </div>
        </div>`;
      }
      const removeBtn = removable
        ? `<button type="button" class="ij-settings-remove" data-pat-remove="${escapeHtml(t.host)}">Remove</button>`
        : '<span class="text-[10px] text-gray-600 shrink-0">read-only</span>';
      return `<div class="ij-settings-token-row">
        <div class="min-w-0">
          <div class="ij-settings-token-host">${escapeHtml(t.host)}</div>
          <div class="ij-settings-token-meta">${escapeHtml(t.masked)} · ${escapeHtml(t.source)}</div>
        </div>
        ${removeBtn}
      </div>`;
    }).join('');
    list.querySelectorAll('[data-pat-remove]').forEach((btn) => {
      btn.addEventListener('click', () => {
        state.patTokenPendingRemove = btn.dataset.patRemove;
        loadPatTokensList();
      });
    });
    list.querySelectorAll('[data-pat-remove-confirm]').forEach((btn) => {
      btn.addEventListener('click', () => removePatToken(btn.dataset.patRemoveConfirm));
    });
    list.querySelectorAll('[data-pat-remove-cancel]').forEach((btn) => {
      btn.addEventListener('click', () => {
        state.patTokenPendingRemove = null;
        loadPatTokensList();
      });
    });
  } catch (err) {
    list.innerHTML = `<p class="ij-settings-empty">${escapeHtml(err.message)}</p>`;
  }
}

async function removePatToken(host) {
  if (!host) return;
  try {
    await api(`/api/settings/tokens/${encodeURIComponent(host)}`, { method: 'DELETE' });
    state.patTokenPendingRemove = null;
    toast(`Removed token for ${host}`, 'success');
    await loadPatTokensList();
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function addPatToken(e) {
  e.preventDefault();
  const host = $('#settings-pat-host')?.value.trim().toLowerCase();
  const token = $('#settings-pat-token')?.value.trim();
  if (!host || !token) {
    toast('Host and token are required', 'error');
    return;
  }
  try {
    await api('/api/settings/tokens', {
      method: 'PUT',
      body: JSON.stringify({ host, token }),
    });
    toast(`Saved token for ${host}`, 'success');
    e.target.reset();
    await loadPatTokensList();
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function loadCursorSettingsSection() {
  const statusEl = $('#settings-cursor-status');
  if (!statusEl) return;
  try {
    const cfg = await api('/api/cursor/status');
    state.cursorConfigured = cfg.configured;
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    state.cursorKeyMasked = cfg.masked || null;
    state.cursorKeySource = cfg.source || null;
    const keyLine = cfg.configured
      ? `<div class="mt-1 font-mono text-xs text-gray-400">${escapeHtml(cfg.masked || '••••')}</div>`
      : '';
    const bridgeClass = cfg.bridge_ok ? 'ok' : (cfg.configured ? 'warn' : 'err');
    const bridgeText = cfg.bridge_ok
      ? 'Bridge connected'
      : (cfg.bridge_error || 'Bridge offline');
    statusEl.innerHTML = `
      <div><strong>API key:</strong> ${cfg.configured ? 'Configured' : 'Not set'}${keyLine}</div>
      <div class="mt-2"><strong>Bridge:</strong> <span class="${bridgeClass}">${escapeHtml(bridgeText)}</span></div>
      ${cfg.source ? `<div class="mt-1 text-[11px] text-gray-600">Source: ${escapeHtml(cfg.source)}</div>` : ''}`;
    const removable = cfg.source === 'settings';
    const clearBtn = $('#settings-cursor-clear');
    clearBtn?.toggleAttribute('disabled', !cfg.configured || !removable);
    if (clearBtn) {
      clearBtn.title = cfg.configured && !removable
        ? `Key is set via ${cfg.source}; unset the environment variable to remove`
        : '';
    }
  } catch (err) {
    statusEl.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
  }
  loadAgentFontSettingsSection();
  updateAgentUi();
}

async function loadCompilersSettingsSection() {
  const list = $('#settings-compilers-list');
  if (!list) return;

  const COMPILER_ORDER = [
    'java', 'kotlin', 'groovy', 'gradle', 'maven',
    'python', 'ruby', 'bundle', 'rails',
    'rustc', 'cargo', 'go',
    'node', 'tsc',
    'jsonlint', 'ajv',
    'yamllint',
    'clangd', 'clang', 'gcc',
    'swiftc', 'dart', 'php', 'luac', 'csc', 'bash',
  ];

  function compilerStatus(tool) {
    if (tool.path_error) {
      return { cls: 'invalid', label: 'Not found' };
    }
    if (tool.configured) {
      return { cls: 'custom', label: 'Custom' };
    }
    if (tool.effective) {
      return { cls: 'ready', label: 'PATH' };
    }
    return { cls: 'missing', label: 'Missing' };
  }

  function renderCompilerRow(tool, { javaInstalled, gradleInstalled, mavenInstalled, jdkSettings }) {
    const isJava = tool.id === 'java';
    const isGradle = tool.id === 'gradle';
    const isMaven = tool.id === 'maven';
    const status = compilerStatus(tool);
    const placeholder = tool.kind === 'home'
      ? '/Library/Java/JavaVirtualMachines/…/Contents/Home'
      : isGradle
        ? '/opt/homebrew/bin/gradle or GRADLE_HOME'
        : isMaven
          ? '/opt/homebrew/bin/mvn or MAVEN_HOME'
          : `/opt/homebrew/bin/${tool.id === 'python' ? 'python3' : tool.id}`;
    const version = tool.version ? `<span class="ij-compiler-version" title="${escapeHtml(tool.version)}">${escapeHtml(tool.version.split('\n')[0].slice(0, 48))}</span>` : '';
    const exts = (tool.extensions || []).length
      ? `<span class="ij-compiler-exts" title="File extensions">${escapeHtml(tool.extensions.join(' '))}</span>`
      : '';
    function installSelected(configured, installPath) {
      if (!configured || !installPath) return false;
      if (configured === installPath) return true;
      const base = configured.replace(/\/$/, '');
      return installPath === base || installPath.startsWith(`${base}/`);
    }
    const jdkSelect = isJava && javaInstalled.length
      ? `<div class="ij-compiler-extra">
          <label class="ij-compiler-extra-label">Installed JDKs</label>
          <select class="ij-settings-select settings-compiler-jdk-select" data-tool-id="java" title="Pick a JDK">
            <option value="">— pick installed JDK —</option>
            ${javaInstalled.map((j) => `<option value="${escapeHtml(j.path)}"${installSelected(tool.path, j.path) ? ' selected' : ''}>${escapeHtml(j.label || j.path)}</option>`).join('')}
          </select>
        </div>`
      : '';
    const javaReleaseSelect = isJava
      ? `<div class="ij-compiler-extra">
          <label class="ij-compiler-extra-label">Language level</label>
          <select class="ij-settings-select settings-java-release-select" data-tool-id="java" title="Java language level for editor squiggles when the project does not declare one">
            <option value="">Auto (from project, else JDK)</option>
            ${JAVA_RELEASE_LEVELS.map((v) => {
              const selected = jdkSettings?.java_release === v ? ' selected' : '';
              return `<option value="${v}"${selected}>Java ${v}</option>`;
            }).join('')}
          </select>
        </div>`
      : '';
    const gradleSelect = isGradle && gradleInstalled.length
      ? `<div class="ij-compiler-extra">
          <label class="ij-compiler-extra-label">Installed Gradle</label>
          <select class="ij-settings-select settings-compiler-gradle-select" data-tool-id="gradle" title="Pick a Gradle version">
            <option value="">— pick installed Gradle —</option>
            ${gradleInstalled.map((g) => `<option value="${escapeHtml(g.path)}"${installSelected(tool.path || tool.effective, g.path) ? ' selected' : ''}>${escapeHtml(g.label || g.path)}</option>`).join('')}
          </select>
        </div>`
      : '';
    const mavenSelect = isMaven && mavenInstalled.length
      ? `<div class="ij-compiler-extra">
          <label class="ij-compiler-extra-label">Installed Maven</label>
          <select class="ij-settings-select settings-compiler-maven-select" data-tool-id="maven" title="Pick a Maven version">
            <option value="">— pick installed Maven —</option>
            ${mavenInstalled.map((m) => `<option value="${escapeHtml(m.path)}"${installSelected(tool.path || tool.effective, m.path) ? ' selected' : ''}>${escapeHtml(m.label || m.path)}</option>`).join('')}
          </select>
        </div>`
      : '';
    const using = tool.effective
      ? `<div class="ij-compiler-using" title="${escapeHtml(tool.effective)}">Using ${escapeHtml(tool.effective)}</div>`
      : '';
    const pathError = tool.path_error
      ? `<div class="ij-compiler-error" title="${escapeHtml(tool.path_error)}">${escapeHtml(tool.path_error)}</div>`
      : '';
    const wrapperNote = (isGradle || isMaven)
      ? `<div class="ij-compiler-note">Project ${isGradle ? 'gradlew' : 'mvnw'} takes precedence when present.</div>`
      : '';
    const javaNote = isJava
      ? '<div class="ij-compiler-note">JDK runs javac. Language level applies when Gradle/Maven do not declare a version.</div>'
      : '';
    return `<article class="ij-compiler-row" data-compiler-row="${escapeHtml(tool.id)}" data-compiler-label="${escapeHtml(tool.label.toLowerCase())} ${escapeHtml(tool.id)}">
      <div class="ij-compiler-row-main">
        <div class="ij-compiler-name">
          <span class="ij-compiler-label">${escapeHtml(tool.label)}</span>
          ${version}
          ${exts}
        </div>
        <span class="ij-compiler-badge ij-compiler-badge--${status.cls}">${status.label}</span>
        <input id="settings-compiler-${escapeHtml(tool.id)}" type="text" spellcheck="false" value="${escapeHtml(tool.path || '')}" placeholder="${escapeHtml(placeholder)}" class="settings-compiler-input ij-compiler-path" data-tool-id="${escapeHtml(tool.id)}" aria-label="${escapeHtml(tool.label)} path" />
        <div class="ij-compiler-actions">
          <button type="button" class="ij-compiler-btn ij-compiler-btn--save settings-compiler-save" data-tool-id="${escapeHtml(tool.id)}" title="Save path">Save</button>
          <button type="button" class="ij-compiler-btn settings-compiler-clear" data-tool-id="${escapeHtml(tool.id)}" title="Use system default"${tool.configured && !tool.source?.startsWith('env:') ? '' : ' disabled'}>Reset</button>
        </div>
      </div>
      ${using}
      ${pathError}
      ${javaNote}
      ${wrapperNote}
      ${jdkSelect}
      ${javaReleaseSelect}
      ${gradleSelect}
      ${mavenSelect}
    </article>`;
  }

  function bindCompilerRows(root) {
    root.querySelectorAll('.settings-compiler-jdk-select, .settings-compiler-gradle-select, .settings-compiler-maven-select').forEach((sel) => {
      sel.addEventListener('change', () => {
        const input = root.querySelector(`.settings-compiler-input[data-tool-id="${sel.dataset.toolId}"]`);
        if (input && sel.value) input.value = sel.value;
      });
    });
    const releaseSel = root.querySelector('.settings-java-release-select');
    if (releaseSel && !releaseSel.dataset.bound) {
      releaseSel.dataset.bound = '1';
      releaseSel.addEventListener('change', async () => {
        const raw = releaseSel.value.trim();
        const release = raw ? parseInt(raw, 10) : null;
        try {
          await api('/api/settings/jdk', {
            method: 'PATCH',
            body: JSON.stringify({ java_release: release }),
          });
          await refreshJavaLanguageLevel();
          if (state.activeTab) await refreshLanguageContextForPath(state.activeTab);
          toast(release ? `Java language level set to ${release}` : 'Java language level: auto', 'success');
        } catch (err) {
          toast(err.message || 'Failed to save Java language level', 'error');
        }
      });
    }
    root.querySelectorAll('.settings-compiler-save').forEach((btn) => {
      btn.addEventListener('click', () => saveCompilerFromSettings(btn.dataset.toolId));
    });
    root.querySelectorAll('.settings-compiler-clear').forEach((btn) => {
      btn.addEventListener('click', () => clearCompilerFromSettings(btn.dataset.toolId));
    });
    root.querySelectorAll('.settings-compiler-input').forEach((input) => {
      input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          saveCompilerFromSettings(input.dataset.toolId);
        }
      });
    });
  }

  function filterCompilerRows(query) {
    const q = query.trim().toLowerCase();
    list.querySelectorAll('.ij-compiler-row').forEach((row) => {
      const hay = row.dataset.compilerLabel || '';
      row.classList.toggle('ij-compiler-row--hidden', q && !hay.includes(q));
    });
  }

  try {
    const [cfg, jdkSettings] = await Promise.all([
      api('/api/settings/compilers'),
      api('/api/settings/jdk').catch(() => null),
    ]);
    const tools = cfg.compilers || cfg.tools || [];
    const javaInstalled = cfg.java_installed || [];
    const gradleInstalled = cfg.gradle_installed || [];
    const mavenInstalled = cfg.maven_installed || [];
    const byId = Object.fromEntries(tools.map((t) => [t.id, t]));
    const ordered = [
      ...COMPILER_ORDER.map((id) => byId[id]).filter(Boolean),
      ...tools.filter((t) => !COMPILER_ORDER.includes(t.id)),
    ];
    if (!ordered.length) {
      list.innerHTML = '<p class="ij-settings-empty">No language compilers loaded. Try reopening Settings → Compiler.</p>';
    } else {
    list.innerHTML = `<div class="ij-compiler-table">
      <div class="ij-compiler-head" aria-hidden="true">
        <span>Language</span>
        <span>Status</span>
        <span>Path override</span>
        <span></span>
      </div>
      <div class="ij-compiler-body">${ordered.map((tool) => renderCompilerRow(tool, { javaInstalled, gradleInstalled, mavenInstalled, jdkSettings })).join('')}</div>
    </div>`;
    bindCompilerRows(list);
    }

    const search = $('#settings-compiler-search');
    if (search && !search.dataset.bound) {
      search.dataset.bound = '1';
      search.addEventListener('input', (e) => filterCompilerRows(e.target.value));
    }
    filterCompilerRows(search?.value || '');
    await populateJavaIndexModeSelect();
  } catch (err) {
    list.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
  }
}

async function populateJavaIndexModeSelect() {
  const select = $('#settings-java-index-mode');
  if (!select) return;
  try {
    const general = await api('/api/settings/general');
    const raw = general?.java_index_mode;
    const mode = raw === 'light' || raw === 'lazy' ? raw : 'standard';
    select.value = mode;
    if (!select.dataset.bound) {
      select.dataset.bound = '1';
      select.addEventListener('change', async (e) => {
        const value = e.target.value;
        if (value !== 'standard' && value !== 'light' && value !== 'lazy') return;
        try {
          await api('/api/settings/general', {
            method: 'PATCH',
            body: JSON.stringify({ java_index_mode: value }),
          });
          const labels = {
            light: 'Light Java index — reload project to apply',
            lazy: 'On-demand Java index — reload project to apply',
            standard: 'Standard Java index — reload project to apply',
          };
          toast(labels[value] || labels.standard, 'success');
        } catch (err) {
          toast(err.message, 'error');
          populateJavaIndexModeSelect();
        }
      });
    }
  } catch (err) {
    /* compilers panel may load before general API is ready */
  }
}

async function saveCompilerFromSettings(id) {
  const input = document.querySelector(`.settings-compiler-input[data-tool-id="${id}"]`);
  const path = input?.value.trim();
  if (!path) {
    toast('Enter a path or use system default', 'error');
    input?.focus();
    return;
  }
  try {
    if (id === 'java') {
      const releaseSel = document.querySelector('.settings-java-release-select');
      const raw = releaseSel?.value?.trim() || '';
      const release = raw ? parseInt(raw, 10) : null;
      await api('/api/settings/jdk', {
        method: 'PATCH',
        body: JSON.stringify({
          java_home: path,
          ...(releaseSel ? { java_release: release } : {}),
        }),
      });
    } else {
      await api('/api/settings/compilers', {
        method: 'PATCH',
        body: JSON.stringify({ id, path }),
      });
    }
    await loadCompilersSettingsSection();
    await refreshJavaLanguageLevel();
    if (state.activeTab) await refreshLanguageContextForPath(state.activeTab);
    toast(`${id} compiler saved`, 'success');
  } catch (err) {
    toast(err.message || `Failed to save ${id}`, 'error');
  }
}

async function clearCompilerFromSettings(id) {
  try {
    await api(`/api/settings/compilers/${encodeURIComponent(id)}`, { method: 'DELETE' });
    await loadCompilersSettingsSection();
    await refreshJavaLanguageLevel();
    if (state.activeTab) await refreshLanguageContextForPath(state.activeTab);
    toast(`${id} using system default`, 'success');
  } catch (err) {
    toast(err.message || `Failed to clear ${id}`, 'error');
  }
}

async function loadSettingsModal() {
  await Promise.all([loadPatTokensList(), loadCursorSettingsSection(), loadGeminiSettingsSection(), loadAnthropicSettingsSection(), loadCompilersSettingsSection()]);
  loadAppearanceSettingsSection();
  loadAgentFontSettingsSection();
  switchSettingsTab(settingsTab);
}

async function showSettingsModal(tab = 'git') {
  settingsTab = tab;
  closeAllMenus();
  hidePalette();
  const overlay = $('#settings-modal-overlay');
  overlay?.classList.remove('hidden');
  overlay?.classList.add('flex');
  $('.ij-settings-modal')?.classList.toggle('ij-settings-modal--compiler', tab === 'compilers');
  void loadSettingsModal();
  setTimeout(async () => {
    let field;
    if (tab === 'cursor') field = $('#settings-cursor-key');
    else if (tab === 'ai') field = $('#settings-gemini-key');
    else if (tab === 'appearance') field = $('#settings-editor-font-size');
    else if (tab === 'compilers') field = $('#settings-compiler-search');
    else field = $('#settings-pat-host');
    field?.focus();
  }, 50);
}

function openAgentTypographySettings() {
  showSettingsModal('appearance');
  setTimeout(() => {
    $('#settings-agent-typography')?.scrollIntoView({ block: 'start', behavior: 'smooth' });
    $('#settings-agent-font-match')?.focus({ preventScroll: true });
  }, 120);
}

function hideSettingsModal() {
  const overlay = $('#settings-modal-overlay');
  overlay?.classList.add('hidden');
  overlay?.classList.remove('flex');
  const cursorKey = $('#settings-cursor-key');
  const patToken = $('#settings-pat-token');
  const geminiKey = $('#settings-gemini-key');
  if (cursorKey) cursorKey.value = '';
  if (patToken) patToken.value = '';
  if (geminiKey) geminiKey.value = '';
}

function isSettingsOpen() {
  return $('#settings-modal-overlay')?.classList.contains('flex');
}

async function saveCursorKeyFromSettings(e) {
  e?.preventDefault();
  const key = $('#settings-cursor-key')?.value.trim();
  if (!key) {
    toast('Enter your Cursor API key', 'error');
    $('#settings-cursor-key')?.focus();
    return;
  }
  try {
    const cfg = await api('/api/settings/cursor', {
      method: 'PUT',
      body: JSON.stringify({ api_key: key }),
    });
    state.cursorConfigured = cfg.configured;
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    state.cursorKeyMasked = cfg.masked || null;
    state.cursorKeySource = cfg.source || null;
    state.cursorModel = cfg.model || state.cursorModel;
    state.cursorMode = cfg.mode || state.cursorMode;
    $('#settings-cursor-key').value = '';
    await loadCursorSettingsSection();
    await loadCursorModels();
    updateAgentUi();
    toast('Cursor API key saved', 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function clearCursorKeyFromSettings() {
  if (!state.cursorConfigured) return;
  if (state.cursorKeySource && state.cursorKeySource !== 'settings') {
    toast(`Key is managed via ${state.cursorKeySource}`, 'info');
    return;
  }
  if (!confirm('Remove the saved Cursor API key?')) return;
  try {
    const cfg = await api('/api/settings/cursor', { method: 'DELETE' });
    state.cursorConfigured = cfg.configured;
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    state.cursorKeyMasked = cfg.masked || null;
    state.cursorKeySource = cfg.source || null;
    await loadCursorSettingsSection();
    await loadCursorModels();
    updateAgentUi();
    toast(
      cfg.configured ? 'Saved key removed; environment key still active' : 'Cursor API key removed',
      cfg.configured ? 'info' : 'success',
    );
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function loadGeminiSettingsSection() {
  const statusEl = $('#settings-gemini-status');
  const form = $('#settings-gemini-form');
  const changeBtn = $('#settings-gemini-change-key');
  if (!statusEl) return;
  try {
    const cfg = await api('/api/settings/gemini');
    state.geminiConfigured = !!cfg.configured;
    ensureAiInlineCompleteDefault(cfg.configured);
    syncGeminiModelSelect(cfg.model);
    refreshAgentProviderUi();
    if (cfg.configured) {
      statusEl.innerHTML = '<span class="ok">AI commit messages are enabled</span>';
      form?.classList.add('hidden');
      changeBtn?.classList.remove('hidden');
    } else {
      statusEl.innerHTML = '<span class="warn">Add a Gemini API key to generate commit messages</span>';
      form?.classList.remove('hidden');
      changeBtn?.classList.add('hidden');
    }
    const clearBtn = $('#settings-gemini-clear');
    const removable = cfg.source === 'settings';
    clearBtn?.toggleAttribute('disabled', !cfg.configured || !removable);
    if (clearBtn) {
      clearBtn.title = cfg.configured && !removable
        ? `Key is set via ${cfg.source}; unset the environment variable to remove`
        : '';
    }
    updateSuggestCommitButton();
    const aiInline = $('#settings-ai-inline-complete');
    if (aiInline) {
      aiInline.checked = getAiInlineCompleteEnabled();
      if (!aiInline.dataset.bound) {
        aiInline.dataset.bound = '1';
        aiInline.addEventListener('change', (e) => setAiInlineCompleteEnabled(e.target.checked));
      }
    }
  } catch (err) {
    statusEl.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
    state.geminiConfigured = false;
    form?.classList.remove('hidden');
    changeBtn?.classList.add('hidden');
    refreshAgentProviderUi();
  }
}

function populateGeminiModelSelect() {
  const sel = $('#settings-gemini-model');
  if (!sel || sel.options.length) return;
  const known = new Set(GEMINI_MODELS.map((m) => m.id));
  sel.innerHTML = GEMINI_MODELS.map(
    (m) => `<option value="${escapeHtml(m.id)}">${escapeHtml(m.label)}</option>`,
  ).join('');
  if (!known.has('gemini-2.0-flash')) {
    /* keep default option from list */
  }
}

function syncGeminiModelSelect(model) {
  populateGeminiModelSelect();
  const sel = $('#settings-gemini-model');
  if (!sel) return;
  const current = model || 'gemini-3.5-flash';
  if (![...sel.options].some((o) => o.value === current)) {
    const opt = document.createElement('option');
    opt.value = current;
    opt.textContent = `${current} (custom)`;
    sel.appendChild(opt);
  }
  sel.value = current;
  sel.dataset.lastValue = current;
}

async function saveGeminiModelFromSettings() {
  const sel = $('#settings-gemini-model');
  if (!sel) return;
  const model = sel.value;
  if (!model || model === sel.dataset.lastValue) return;
  try {
    const cfg = await api('/api/settings/gemini/model', {
      method: 'PATCH',
      body: JSON.stringify({ model }),
    });
    syncGeminiModelSelect(cfg.model);
    toast(`Gemini model set to ${cfg.model}`, 'success');
    if (state.activePanel === 'git' && state.repo) {
      await maybeAutoSuggestCommit();
    }
  } catch (err) {
    toast(err.message, 'error');
    syncGeminiModelSelect(sel.dataset.lastValue);
  }
}

function showGeminiKeyForm() {
  $('#settings-gemini-form')?.classList.remove('hidden');
  $('#settings-gemini-change-key')?.classList.add('hidden');
  $('#settings-gemini-key')?.focus();
}

function updateSuggestCommitButton() {
  const btn = $('#btn-suggest-commit');
  if (!btn) return;
  btn.title = state.geminiConfigured
    ? 'Generate commit message from your changes'
    : 'Set up AI in Settings → AI';
}

async function ensureGeminiReady() {
  try {
    const cfg = await api('/api/settings/gemini');
    state.geminiConfigured = !!cfg.configured;
    state.geminiModel = cfg.model || 'gemini-3.5-flash';
    updateSuggestCommitButton();
    return !!(cfg.configured && cfg.model);
  } catch {
    state.geminiConfigured = false;
    return false;
  }
}

async function saveGeminiKeyFromSettings(e) {
  e?.preventDefault();
  const key = $('#settings-gemini-key')?.value.trim();
  if (!key) {
    toast('Enter a Gemini API key', 'error');
    $('#settings-gemini-key')?.focus();
    return;
  }
  try {
    const model = $('#settings-gemini-model')?.value;
    await api('/api/settings/gemini', {
      method: 'PUT',
      body: JSON.stringify({ api_key: key, model: model || undefined }),
    });
    $('#settings-gemini-key').value = '';
    await loadGeminiSettingsSection();
    toast('Gemini API key saved', 'success');
    const shouldSuggest = state.pendingCommitSuggest
      || (state.activePanel === 'git' && !$('#commit-message')?.value.trim());
    state.pendingCommitSuggest = false;
    if (shouldSuggest && state.repo) {
      hideSettingsModal();
      await maybeAutoSuggestCommit();
    }
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function clearGeminiKeyFromSettings() {
  try {
    const cfg = await api('/api/settings/gemini');
    if (!cfg.configured) return;
    if (cfg.source && cfg.source !== 'settings') {
      toast(`Key is managed via ${cfg.source}`, 'info');
      return;
    }
    if (!confirm('Remove the saved Gemini API key?')) return;
    const out = await api('/api/settings/gemini', { method: 'DELETE' });
    await loadGeminiSettingsSection();
    toast(
      out.configured ? 'Saved key removed; environment key still active' : 'Gemini API key removed',
      out.configured ? 'info' : 'success',
    );
  } catch (err) {
    toast(err.message, 'error');
  }
}

function populateAnthropicModelSelects(cfg = null) {
  const apiSel = $('#settings-anthropic-model');
  if (apiSel && !apiSel.options.length) {
    apiSel.innerHTML = ANTHROPIC_MODELS.map(
      (m) => `<option value="${escapeHtml(m.id)}">${escapeHtml(m.label)}</option>`,
    ).join('');
  }
  const bedSel = $('#settings-bedrock-model');
  if (bedSel) {
    const models = bedrockModelsForSelect();
    const current = cfg?.bedrock_model_id || state.bedrockModelId || models[0]?.id;
    bedSel.innerHTML = models.map(
      (m) => `<option value="${escapeHtml(m.id)}">${escapeHtml(m.label || m.id)}</option>`,
    ).join('');
    if (current && ![...bedSel.options].some((o) => o.value === current)) {
      const opt = document.createElement('option');
      opt.value = current;
      opt.textContent = `${current} (saved)`;
      bedSel.appendChild(opt);
    }
    if (current) bedSel.value = current;
  }
}

function syncAnthropicModelSelects(cfg) {
  populateAnthropicModelSelects(cfg);

  const apiSel = $('#settings-anthropic-model');
  if (apiSel) {
    const current = cfg.model || 'claude-sonnet-4-5';
    if (![...apiSel.options].some((o) => o.value === current)) {
      const opt = document.createElement('option');
      opt.value = current;
      opt.textContent = `${current} (custom)`;
      apiSel.appendChild(opt);
    }
    apiSel.value = current;
  }

  const regionEl = $('#settings-bedrock-region');
  if (regionEl) regionEl.value = cfg.bedrock_region || 'us-east-1';
}

async function refreshBedrockModels({ silent = false } = {}) {
  const statusHint = $('#settings-bedrock-models-hint');
  try {
    const data = await api('/api/settings/bedrock/models');
    const list = Array.isArray(data.models) ? data.models : [];
    state.bedrockModels = list.map((m) => ({
      id: m.id,
      label: m.label || m.id,
      provider: m.provider,
      kind: m.kind,
    }));
    state.bedrockModelsSource = data.source || null;
    state.bedrockModelsError = null;
    if (statusHint) {
      const n = state.bedrockModels.length;
      const src = data.source === 'mantle'
        ? 'Mantle key — Claude models only. Add AWS credentials for the full Bedrock catalog.'
        : `Loaded ${n} text/chat model${n === 1 ? '' : 's'} from AWS (${data.region || state.bedrockRegion}).`;
      statusHint.textContent = src;
      statusHint.className = 'text-[11px] text-gray-500';
    }
    populateAnthropicModelSelects({
      bedrock_model_id: state.bedrockModelId,
      bedrock_region: state.bedrockRegion,
    });
    refreshAgentProviderUi();
    if (!silent) toast(`Loaded ${state.bedrockModels.length} Bedrock models`, 'success');
  } catch (err) {
    state.bedrockModelsError = err.message || String(err);
    if (statusHint) {
      statusHint.textContent = state.bedrockModelsError;
      statusHint.className = 'text-[11px] text-red-400';
    }
    if (!silent) toast(state.bedrockModelsError, 'error');
  }
}

async function loadAnthropicSettingsSection() {
  const statusEl = $('#settings-anthropic-status');
  const form = $('#settings-anthropic-form');
  const changeBtn = $('#settings-anthropic-change-key');
  const bedStatus = $('#settings-bedrock-status');
  const bedForm = $('#settings-bedrock-form');
  const bedChange = $('#settings-bedrock-change-key');
  if (!statusEl && !bedStatus) return;
  try {
    const cfg = await api('/api/settings/anthropic');
    state.anthropicConfigured = cfg.api_configured != null
      ? !!cfg.api_configured
      : !!cfg.masked;
    state.bedrockConfigured = cfg.bedrock_configured != null
      ? !!cfg.bedrock_configured
      : !!(cfg.bedrock_masked);
    state.anthropicBackend = cfg.backend === 'bedrock' ? 'bedrock' : 'api';
    state.anthropicModel = cfg.model || 'claude-sonnet-4-5';
    state.bedrockModelId = cfg.bedrock_model_id || BEDROCK_MODELS_FALLBACK[0].id;
    state.bedrockRegion = cfg.bedrock_region || 'us-east-1';
    ensureAiInlineCompleteDefault(
      state.anthropicConfigured || state.bedrockConfigured || state.geminiConfigured || state.cursorConfigured,
    );
    syncAnthropicModelSelects(cfg);
    refreshAgentProviderUi();
    if (state.bedrockConfigured) {
      void refreshBedrockModels({ silent: true });
    }

    if (statusEl) {
      if (state.anthropicConfigured) {
        statusEl.innerHTML = '<span class="ok">Claude (Anthropic API) is enabled</span>';
        form?.classList.add('hidden');
        changeBtn?.classList.remove('hidden');
      } else {
        statusEl.innerHTML = '<span class="warn">Add an Anthropic API key for the Claude agent</span>';
        form?.classList.remove('hidden');
        changeBtn?.classList.add('hidden');
      }
      const clearBtn = $('#settings-anthropic-clear');
      clearBtn?.toggleAttribute('disabled', !state.anthropicConfigured || (cfg.source && cfg.source !== 'settings'));
      if (clearBtn) {
        clearBtn.title = cfg.configured && cfg.source && cfg.source !== 'settings'
          ? `Key may be set via ${cfg.source}`
          : '';
      }
    }

    if (bedStatus) {
      if (state.bedrockConfigured) {
        const via = cfg.bedrock_masked ? 'Mantle API key' : 'AWS credentials';
        bedStatus.innerHTML = `<span class="ok">Bedrock is enabled (${via})</span>`;
        if (cfg.bedrock_masked) {
          bedForm?.classList.add('hidden');
          bedChange?.classList.remove('hidden');
        } else {
          bedForm?.classList.remove('hidden');
          bedChange?.classList.add('hidden');
        }
      } else {
        bedStatus.innerHTML = '<span class="warn">Add a Mantle key or configure AWS credentials for Bedrock</span>';
        bedForm?.classList.remove('hidden');
        bedChange?.classList.add('hidden');
      }
      const bedClear = $('#settings-bedrock-clear');
      bedClear?.toggleAttribute('disabled', !cfg.bedrock_masked);
    }
  } catch (err) {
    if (statusEl) statusEl.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
    if (bedStatus) bedStatus.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
    state.anthropicConfigured = false;
    state.bedrockConfigured = false;
    form?.classList.remove('hidden');
    changeBtn?.classList.add('hidden');
    bedForm?.classList.remove('hidden');
    bedChange?.classList.add('hidden');
    refreshAgentProviderUi();
  }
}

function showAnthropicKeyForm() {
  $('#settings-anthropic-form')?.classList.remove('hidden');
  $('#settings-anthropic-change-key')?.classList.add('hidden');
  $('#settings-anthropic-key')?.focus();
}

function showBedrockKeyForm() {
  $('#settings-bedrock-form')?.classList.remove('hidden');
  $('#settings-bedrock-change-key')?.classList.add('hidden');
  $('#settings-bedrock-key')?.focus();
}

async function saveAnthropicFromSettings(e) {
  e?.preventDefault();
  const apiKey = $('#settings-anthropic-key')?.value.trim();
  const body = {
    backend: 'api',
    model: $('#settings-anthropic-model')?.value || undefined,
  };
  if (apiKey) body.api_key = apiKey;
  if (!apiKey && !state.anthropicConfigured) {
    toast('Enter an Anthropic API key', 'error');
    $('#settings-anthropic-key')?.focus();
    return;
  }
  try {
    await api('/api/settings/anthropic', {
      method: 'PUT',
      body: JSON.stringify(body),
    });
    if ($('#settings-anthropic-key')) $('#settings-anthropic-key').value = '';
    await loadAnthropicSettingsSection();
    toast('Claude settings saved', 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function saveBedrockFromSettings(e) {
  e?.preventDefault();
  const bedrockKey = $('#settings-bedrock-key')?.value.trim();
  const body = {
    backend: 'bedrock',
    bedrock_model_id: $('#settings-bedrock-model')?.value || undefined,
    bedrock_region: $('#settings-bedrock-region')?.value.trim() || undefined,
  };
  if (bedrockKey) body.bedrock_api_key = bedrockKey;
  try {
    await api('/api/settings/anthropic', {
      method: 'PUT',
      body: JSON.stringify(body),
    });
    if ($('#settings-bedrock-key')) $('#settings-bedrock-key').value = '';
    await loadAnthropicSettingsSection();
    toast('Bedrock settings saved', 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function clearAnthropicFromSettings() {
  try {
    if (!confirm('Remove saved Anthropic API key?')) return;
    await api('/api/settings/anthropic?target=api', { method: 'DELETE' });
    await loadAnthropicSettingsSection();
    toast('Claude API key removed', 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function clearBedrockFromSettings() {
  try {
    if (!confirm('Remove saved Bedrock Mantle API key?')) return;
    await api('/api/settings/anthropic?target=bedrock', { method: 'DELETE' });
    await loadAnthropicSettingsSection();
    toast('Bedrock Mantle key removed', 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function saveAnthropicModelFromSettings() {
  try {
    const model = $('#settings-anthropic-model')?.value;
    const cfg = await api('/api/settings/anthropic/model', {
      method: 'PATCH',
      body: JSON.stringify({ model }),
    });
    state.anthropicModel = cfg.model || model;
    toast(`Claude model set to ${state.anthropicModel}`, 'success');
    refreshAgentProviderUi();
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function saveBedrockModelFromSettings() {
  try {
    const model = $('#settings-bedrock-model')?.value;
    const region = $('#settings-bedrock-region')?.value.trim();
    const cfg = await api('/api/settings/anthropic', {
      method: 'PUT',
      body: JSON.stringify({
        backend: 'bedrock',
        bedrock_model_id: model || undefined,
        bedrock_region: region || undefined,
      }),
    });
    state.bedrockModelId = cfg.bedrock_model_id || model;
    state.bedrockRegion = cfg.bedrock_region || region || 'us-east-1';
    state.bedrockConfigured = cfg.bedrock_configured != null
      ? !!cfg.bedrock_configured
      : state.bedrockConfigured;
    toast(`Bedrock model set to ${state.bedrockModelId}`, 'success');
    refreshAgentProviderUi();
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function setAnthropicAgentModel(modelId) {
  if (!modelId || modelId === state.anthropicModel) return;
  state.anthropicModel = modelId;
  updateAgentUi();
  try {
    const cfg = await api('/api/settings/anthropic/model', {
      method: 'PATCH',
      body: JSON.stringify({ model: modelId }),
    });
    state.anthropicModel = cfg.model || modelId;
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function setBedrockAgentModel(modelId) {
  if (!modelId || modelId === state.bedrockModelId) return;
  state.bedrockModelId = modelId;
  updateAgentUi();
  try {
    const cfg = await api('/api/settings/anthropic', {
      method: 'PUT',
      body: JSON.stringify({ backend: 'bedrock', bedrock_model_id: modelId }),
    });
    state.bedrockModelId = cfg.bedrock_model_id || modelId;
  } catch (err) {
    toast(err.message, 'error');
  }
}

function patHostAliases(host) {
  const base = String(host || '').trim().toLowerCase().split(':')[0];
  if (!base) return [];
  const keys = [base];
  if (base === 'www.github.com' && !keys.includes('github.com')) keys.push('github.com');
  return keys;
}

async function loadPatTokenHosts() {
  try {
    const tokens = await api('/api/settings/tokens');
    return tokens.map((t) => String(t.host || '').toLowerCase());
  } catch {
    return [];
  }
}

async function hasPatForHost(host) {
  const aliases = patHostAliases(host);
  if (!aliases.length) return false;
  const saved = await loadPatTokenHosts();
  if (saved.includes('*')) return true;
  return aliases.some((k) => saved.includes(k));
}

async function hasGitHubPat() {
  return hasPatForHost('github.com');
}

function langForPath(path) {
  return window.ReaperLang?.langForPath(path) || 'plaintext';
}

function fileIcon(name) {
  const lower = name.toLowerCase();
  if (lower.endsWith('.java')) return 'java';
  if (lower.endsWith('.gradle') || lower.endsWith('.gradle.kts') || lower === 'gradlew') return 'gradle';
  if (lower.endsWith('.kt') || lower.endsWith('.kts')) return 'kotlin';
  if (lower.endsWith('.rs')) return 'rust';
  if (lower.endsWith('.js') || lower.endsWith('.mjs') || lower.endsWith('.cjs')) return 'js';
  if (lower.endsWith('.ts') || lower.endsWith('.tsx')) return 'ts';
  if (lower.endsWith('.jsx')) return 'jsx';
  if (lower.endsWith('.json')) return 'json';
  if (lower.endsWith('.md')) return 'markdown';
  if (lower.endsWith('.yml') || lower.endsWith('.yaml')) return 'yaml';
  if (lower.endsWith('.properties')) return 'properties';
  if (lower.endsWith('.xml')) return 'xml';
  if (lower.endsWith('.html') || lower.endsWith('.htm')) return 'html';
  if (lower.endsWith('.css') || lower.endsWith('.scss')) return 'css';
  if (lower.endsWith('.py')) return 'python';
  if (lower.endsWith('.go')) return 'go';
  if (lower.endsWith('.sql')) return 'sql';
  if (lower === 'dockerfile' || lower.startsWith('dockerfile.')) return 'docker';
  if (lower === 'elide.pkl' || lower.endsWith('.pkl')) return 'elide';
  if (lower === '.gitignore' || lower === '.gitattributes') return 'git';
  return 'file';
}

function treeIconSvg(kind) {
  const icons = {
    folder: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3.172a1.5 1.5 0 0 1 1.06.44L9.085 4.5H12.5A1.5 1.5 0 0 1 14 6v6.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12.5V4.5Z" fill="currentColor" fill-opacity=".9"/></svg>',
    folderOpen: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M1.5 6.5A1.5 1.5 0 0 1 3 5h2.55a1 1 0 0 1 .707.293L6.414 7H12.5A1.5 1.5 0 0 1 14 8.5V12a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 1 12V6.5Z" fill="currentColor" fill-opacity=".92"/></svg>',
    java: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#9876aa" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#9876aa" font-family="Consolas,Menlo,monospace">J</text></svg>',
    gradle: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6a8759" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#6a8759" font-family="Consolas,Menlo,monospace">G</text></svg>',
    kotlin: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#7f52ff" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#7f52ff" font-family="Consolas,Menlo,monospace">K</text></svg>',
    rust: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#a17358" fill-opacity=".2"/><text x="8" y="11" text-anchor="middle" font-size="7" font-weight="700" fill="#a17358" font-family="Inter,sans-serif">R</text></svg>',
    js: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#cbcb41" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#cbcb41" font-family="Inter,sans-serif">JS</text></svg>',
    ts: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".2"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">TS</text></svg>',
    jsx: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#61dafb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#61dafb" font-family="Inter,sans-serif">JX</text></svg>',
    json: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">{ }</text></svg>',
    markdown: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="7" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">M</text></svg>',
    yaml: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#cc7832" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#cc7832" font-family="Inter,sans-serif">Y</text></svg>',
    properties: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6a8759" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6a8759" font-family="Inter,sans-serif">P</text></svg>',
    xml: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">&lt;&gt;</text></svg>',
    html: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#e44d26" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#e44d26" font-family="Inter,sans-serif">H</text></svg>',
    css: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">#</text></svg>',
    python: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="7" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">Py</text></svg>',
    go: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="7" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">Go</text></svg>',
    sql: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">SQL</text></svg>',
    docker: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6897bb" fill-opacity=".15"/><text x="8" y="11" text-anchor="middle" font-size="6" font-weight="700" fill="#6897bb" font-family="Inter,sans-serif">D</text></svg>',
    elide: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#2E8B9A" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="7" font-weight="700" fill="#2E8B9A" font-family="Consolas,Menlo,monospace">E</text></svg>',
    git: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#f14c28" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#f14c28" font-family="Consolas,Menlo,monospace">G</text></svg>',
    file: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M4 2.5h5.5L12 5v8.5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1Z" fill="currentColor" fill-opacity=".28"/><path d="M9.5 2.5V5H12" stroke="currentColor" stroke-opacity=".55" stroke-width=".8"/></svg>',
  };
  return icons[kind] || icons.file;
}

function updateBreadcrumbs(path) {
  const el = $('#editor-breadcrumbs');
  if (!el) return;
  if (!path) {
    el.classList.add('hidden');
    el.innerHTML = '';
    return;
  }
  el.classList.remove('hidden');
  const displayPath = workspaceExplorerPath(path);
  const external = isExternalEditorPath(displayPath);
  let parts;
  let segForIndex;
  if (external) {
    parts = displayPath.split('/').filter(Boolean);
    segForIndex = (i) => `/${parts.slice(0, i + 1).join('/')}`;
  } else {
    parts = displayPath.split('/').filter(Boolean);
    segForIndex = (i) => parts.slice(0, i + 1).join('/');
  }
  const crumbHtml = parts.map((part, i) => {
    const seg = segForIndex(i);
    const sep = i < parts.length - 1 ? '<span class="ij-crumb-sep"> › </span>' : '';
    return `<button type="button" class="ij-crumb" data-crumb="${escapeHtml(seg)}"${external ? ' disabled' : ''}>${escapeHtml(part)}</button>${sep}`;
  }).join('');
  el.innerHTML = external
    ? `<span class="ij-crumb-external-label">External</span><span class="ij-crumb-sep"> › </span>${crumbHtml}`
    : crumbHtml;
  $$('.ij-crumb').forEach((btn) => {
    if (btn.disabled) return;
    btn.addEventListener('click', () => {
      const target = btn.dataset.crumb;
      if (target === displayPath) return;
      void revealFileInExplorer(target);
    });
  });
}

function updateEditorStatus(pos) {
  const el = $('#status-cursor');
  if (el && pos) el.textContent = `${pos.lineNumber}:${pos.column}`;
  updateDiagnosticHintAtCursor(pos);
}

function diagnosticFriendlyHint(msg) {
  const lower = String(msg || '').toLowerCase();
  if (lower.includes('reached end of file while parsing')) {
    return ' — check for a missing closing brace }';
  }
  if (lower.includes("'}' expected") || lower.includes('\'}\' expected')) {
    return ' — add a closing brace }';
  }
  if (lower.includes('illegal start of type') && lower.includes('}')) {
    return ' — a method or block may be missing }';
  }
  return '';
}

function formatDiagnosticDisplay(d) {
  const msg = String(d?.message || '').trim();
  if (!msg) return '';
  const prefix = d.severity === 'warning' ? 'Warning: ' : 'Error: ';
  return `${prefix}${msg}${diagnosticFriendlyHint(msg)}`;
}

function truncateDiagnosticText(text, max = 72) {
  const s = String(text || '').trim();
  if (s.length <= max) return s;
  return `${s.slice(0, max - 1)}…`;
}

function primaryDiagnosticForUi(model, diags) {
  if (!diags.length) return null;
  const pos = state.editor?.getPosition();
  if (model && pos) {
    const atCursor = findDiagnosticNearLine(model, pos.lineNumber);
    if (atCursor) return atCursor;
  }
  const errors = diags.filter((d) => d.severity !== 'warning');
  return errors[0] || diags[0];
}

function updateDiagnosticHintAtCursor(pos) {
  const hint = $('#status-diag-hint');
  if (!hint) return;
  const model = state.editor?.getModel();
  if (!model || !pos || !fileDiags.length) {
    hint.classList.add('hidden');
    hint.textContent = '';
    hint.title = '';
    return;
  }
  const d = findDiagnosticNearLine(model, pos.lineNumber);
  if (!d) {
    hint.classList.add('hidden');
    hint.textContent = '';
    hint.title = '';
    return;
  }
  const formatted = formatDiagnosticDisplay(d);
  hint.classList.remove('hidden');
  hint.classList.toggle('is-error', d.severity !== 'warning');
  hint.classList.toggle('is-warning', d.severity === 'warning');
  hint.textContent = truncateDiagnosticText(formatted, 96);
  hint.title = formatted;
}

function setStatusMessage(msg) {
  const el = $('#status-message');
  if (el) el.textContent = msg;
}

function defaultStatusLabel() {
  return state.activeTab?.split('/').pop() || 'Ready';
}

function showCompileFooter({ resetSafety = true } = {}) {
  const gen = ++javaCompileFooterGen;
  if (resetSafety) {
    clearTimeout(compileFooterSafetyTimer);
    compileFooterSafetyTimer = setTimeout(() => finishCompileFooter(gen), COMPILE_FOOTER_SAFETY_MS);
  }
  const el = $('#status-message');
  if (!el) return gen;
  clearSaveFooterStatus();
  el.textContent = 'Analyzing…';
  return gen;
}

function finishCompileFooter(gen, msg) {
  if (gen != null && gen !== javaCompileFooterGen) return;
  clearTimeout(compileFooterSafetyTimer);
  compileFooterSafetyTimer = null;
  clearSaveFooterStatus();
  setStatusMessage(msg ?? defaultStatusLabel());
}

const SAVE_FOOTER_MS = 2000;
const AUTO_SAVE_FOOTER_MS = 1200;
const SAVE_FAIL_FOOTER_MS = 3500;
let saveFooterTimer = null;

function clearSaveFooterStatus() {
  if (saveFooterTimer) {
    clearTimeout(saveFooterTimer);
    saveFooterTimer = null;
  }
  const el = $('#status-message');
  el?.classList.remove('is-save-hint', 'is-save-hint-auto', 'is-save-hint-error', 'is-save-hint-pending');
}

function showSavingFooterStatus() {
  clearSaveFooterStatus();
  const el = $('#status-message');
  if (!el) return;
  el.textContent = 'Saving…';
  el.classList.add('is-save-hint', 'is-save-hint-pending');
}

/** Brief footer confirmation after a successful save (status-left #status-message). */
function showSaveFooterStatus(message = 'Saved', { auto = false, error = false, ms } = {}) {
  clearSaveFooterStatus();
  const el = $('#status-message');
  if (!el) return;
  el.textContent = message;
  el.classList.add('is-save-hint');
  if (auto) el.classList.add('is-save-hint-auto');
  if (error) el.classList.add('is-save-hint-error');
  const duration = ms ?? (error ? SAVE_FAIL_FOOTER_MS : (auto ? AUTO_SAVE_FOOTER_MS : SAVE_FOOTER_MS));
  saveFooterTimer = setTimeout(() => {
    el.classList.remove('is-save-hint', 'is-save-hint-auto', 'is-save-hint-error', 'is-save-hint-pending');
    saveFooterTimer = null;
    if (el.textContent !== message) return;
    setStatusMessage(defaultStatusLabel());
  }, duration);
}

function javaFullDiagRetryDelayMs(attempt) {
  return Math.min(Math.round(diagRetryDelayMs * (attempt + 1)), 8000);
}

/** Java javac for one file (typing while editing, full on save / classpath refresh). */
async function runJavaDiagnosticsForPath(path, content, {
  scope = 'full',
  attempt = 0,
  force = false,
  footerGen: existingFooterGen = null,
} = {}) {
  if (!state.repo || !path?.endsWith('.java')) return;
  const seq = ++javaFullDiagSeq;
  const snapshot = state.editor?.getValue() ?? content;
  javaFullDiagSnapshotByPath.set(path, snapshot);
  let footerGen = existingFooterGen;
  if (path === state.activeTab && footerGen == null) {
    footerGen = showCompileFooter({ resetSafety: true });
  }
  try {
    const result = normalizeDiagnosticsResponse(
      await fetchDiagnosticsForPath(path, snapshot, { scope, force }),
    );
    if (seq !== javaFullDiagSeq || path !== state.activeTab) return;

    const latest = state.editor?.getValue();
    if (latest != null && latest !== snapshot) {
      javaFullDiagSnapshotByPath.delete(path);
      finishCompileFooter(footerGen);
      queueJavaDiagnostics(path, latest, { scope });
      return;
    }

    if (result.cancelled && !result.diagnostics.length) {
      if (attempt < JAVA_FULL_DIAG_MAX_RETRIES) {
        await new Promise((r) => setTimeout(r, javaFullDiagRetryDelayMs(attempt)));
        if (seq !== javaFullDiagSeq || path !== state.activeTab) return;
        const retryContent = state.editor?.getValue() ?? snapshot;
        return runJavaDiagnosticsForPath(path, retryContent, {
          scope,
          attempt: attempt + 1,
          force: false,
          footerGen,
        });
      }
      if (path === state.activeTab) {
        finishCompileFooter(footerGen, 'Compile did not finish');
        toast('Compile did not finish — try saving again', 'warning');
      }
      return;
    }
    applyDiagnostics(path, result.diagnostics);
    if (seq === javaFullDiagSeq && path === state.activeTab) {
      finishCompileFooter(footerGen);
    }
  } catch (err) {
    if (seq !== javaFullDiagSeq || path !== state.activeTab) return;
    if (attempt < JAVA_FULL_DIAG_MAX_RETRIES) {
      await new Promise((r) => setTimeout(r, javaFullDiagRetryDelayMs(attempt)));
      if (seq !== javaFullDiagSeq || path !== state.activeTab) return;
      const retryContent = state.editor?.getValue() ?? content;
      return runJavaDiagnosticsForPath(path, retryContent, {
        scope,
        attempt: attempt + 1,
        force: false,
        footerGen,
      });
    }
    if (path === state.activeTab) {
      const msg = isDiagFetchAbort(err) ? 'Compile timed out' : (err.message || 'Compile failed');
      finishCompileFooter(footerGen, msg);
      toast(`${msg} — try saving again`, 'warning');
    }
  }
}

/** @deprecated use runJavaDiagnosticsForPath */
async function runFullDiagnosticsForPath(path, content, opts = {}) {
  return runJavaDiagnosticsForPath(path, content, { scope: 'full', ...opts });
}
const NAV_BUSY_DELAY_MS = 250;
const NAV_RESULT_MS = 4500;
let navBusyDepth = 0;
let navBusyTimer = null;
let navResultTimer = null;
let navBusyPrevMessage = 'Ready';

function showNavigationBusy(label) {
  const slot = $('#status-nav-indicator');
  const left = $('#status-left');
  if (slot) {
    slot.innerHTML = navigationBusyHtml(label);
    slot.classList.remove('hidden');
  }
  left?.setAttribute('aria-busy', 'true');
}

function hideNavigationBusy() {
  const slot = $('#status-nav-indicator');
  const left = $('#status-left');
  if (slot) {
    slot.innerHTML = '';
    slot.classList.add('hidden');
  }
  left?.removeAttribute('aria-busy');
}

function stopNavigationBusyIndicator() {
  clearTimeout(navBusyTimer);
  navBusyTimer = null;
  hideNavigationBusy();
}

function clearNavigationResult() {
  if (navResultTimer) {
    clearTimeout(navResultTimer);
    navResultTimer = null;
  }
  const el = $('#status-message');
  el?.classList.remove('is-nav-hint', 'is-nav-warn');
}

/** Brief status-bar note after navigation (e.g. no definition found). */
function showNavigationResult(message, kind = 'info') {
  if (navBusyDepth <= 1) {
    stopNavigationBusyIndicator();
  }
  clearNavigationResult();
  const el = $('#status-message');
  if (!el) return;
  el.textContent = message;
  el.classList.toggle('is-nav-hint', kind === 'info');
  el.classList.toggle('is-nav-warn', kind === 'warn');
  navResultTimer = setTimeout(() => {
    el.classList.remove('is-nav-hint', 'is-nav-warn');
    el.textContent = navBusyPrevMessage || 'Ready';
    navResultTimer = null;
  }, NAV_RESULT_MS);
}

/** Status-bar spinner when navigation (F12, usages, rename) exceeds NAV_BUSY_DELAY_MS. */
function runWithNavigationBusy(label, fn) {
  navBusyDepth += 1;
  if (navBusyDepth === 1) {
    navBusyPrevMessage = $('#status-message')?.textContent || 'Ready';
    clearTimeout(navBusyTimer);
    navBusyTimer = setTimeout(() => {
      if (navBusyDepth > 0) showNavigationBusy(label);
    }, NAV_BUSY_DELAY_MS);
  }
  return Promise.resolve().then(fn).finally(() => {
    navBusyDepth = Math.max(0, navBusyDepth - 1);
    if (navBusyDepth > 0) return;
    stopNavigationBusyIndicator();
  });
}

function startCommandStatus(label, terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term) return;
  stopCommandStatus(term);
  term.commandStatusPrev = $('#status-message')?.textContent || 'Ready';
  term.commandStatusLabel = String(label || 'command').trim();
  term.commandStatusStarted = Date.now();
  const tick = () => {
    if (term.streamLine == null) {
      stopCommandStatus(term);
      return;
    }
    const sec = Math.floor((Date.now() - term.commandStatusStarted) / 1000);
    const elapsed = sec >= 60
      ? `${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, '0')}`
      : `${sec}s`;
    setStatusMessage(`${term.commandStatusLabel} · ${elapsed}`);
  };
  tick();
  term.commandStatusTimer = setInterval(tick, 1000);
}

function stopCommandStatus(term) {
  if (!term) return;
  if (term.commandStatusTimer) {
    clearInterval(term.commandStatusTimer);
    term.commandStatusTimer = null;
  }
  if (term.commandStatusPrev != null) {
    setStatusMessage(term.commandStatusPrev);
    term.commandStatusPrev = null;
  }
}

function stopProjectIndexPolling(options = {}) {
  const { keepUi = false } = options;
  if (state.projectIndexPoll) {
    clearInterval(state.projectIndexPoll);
    state.projectIndexPoll = null;
    state.projectIndexPollMs = null;
  }
  if (!keepUi) clearIndexingProgressUi();
}

function ensureProjectIndexPollInterval(ms) {
  if (state.projectIndexPollMs === ms && state.projectIndexPoll) return;
  if (state.projectIndexPoll) clearInterval(state.projectIndexPoll);
  state.projectIndexPollMs = ms;
  state.projectIndexPoll = setInterval(pollProjectIndexStatus, ms);
}

function projectIndexNeedsFreeze(status) {
  if (!status) return false;
  const java = status.java || {};
  // Per-module indexing in on-demand mode — keep the editor usable.
  if (java.phase === 'on-demand' && java.state === 'running') return false;
  const indexers = status.profile?.indexers || [];
  const needsJava = indexers.includes('java');
  const javaRunning = java.state === 'running';
  return status.state === 'running' || (needsJava && javaRunning);
}

function indexingPhaseLabel(phase) {
  switch (phase) {
    case 'workspace-symbols': return 'Scanning workspace symbols';
    case 'java-index': return 'Building Java index';
    case 'classpath':
    case 'classpath-resolve': return 'Resolving dependencies';
    case 'running-gradle-compile': return 'Running Gradle (compiling…)';
    case 'running-gradle-classpath': return 'Running Gradle (classpath…)';
    case 'running-gradle-test-classpath': return 'Running Gradle (test classpath…)';
    case 'running-maven-sources': return 'Running Maven (downloading sources…)';
    case 'running-maven-classpath': return 'Running Maven (classpath…)';
    case 'running-maven-test-classpath': return 'Running Maven (test classpath…)';
    case 'sources':
    case 'extracting-sources': return 'Extracting library sources';
    case 'indexing': return 'Indexing Java symbols';
    case 'jar-index': return 'Indexing dependency classes';
    case 'jar-index-background': return 'Indexing remaining JARs (background)';
    case 'on-demand': return 'On-demand — open files to index modules';
    case 'writing': return 'Saving index';
    case 'starting': return 'Starting';
    case 'ready': return 'Ready';
    default: return phase ? phase.replace(/-/g, ' ') : 'Indexing';
  }
}

function isBackgroundJarIndexPhase(phase, java) {
  if (phase === 'jar-index-background') return true;
  return java?.state === 'ready'
    && java?.index_complete === false
    && (java?.jars_total || 0) > (java?.jars_indexed || 0);
}

function isBackgroundToolingPhase(phase) {
  return phase === 'running-gradle-compile'
    || phase === 'running-gradle-classpath'
    || phase === 'running-gradle-test-classpath'
    || phase === 'running-maven-sources'
    || phase === 'running-maven-classpath'
    || phase === 'running-maven-test-classpath'
    || phase === 'classpath-resolve';
}

function indexingProgressPercent(status) {
  if (!status) return 0;
  const java = status.java || {};
  const phase = java.phase || status.phase || '';
  if (status.state === 'ready' && !isBackgroundJarIndexPhase(phase, java)) return 100;
  const wsN = status.workspace_symbols || 0;
  const rawJavaN = java.symbol_count || 0;

  if (phase === 'jar-index-background' || isBackgroundJarIndexPhase(phase, java)) {
    const indexed = java.jars_indexed || 0;
    const total = java.jars_total || 1;
    return Math.min(94, 72 + Math.round((indexed / total) * 22));
  }
  if (phase === 'writing') return 96;
  if (phase === 'jar-index') {
    const base = Math.floor(rawJavaN / 1000) * 1000;
    const jarFrac = rawJavaN - base;
    const jarPct = Math.min(999, Math.max(0, jarFrac));
    return Math.min(94, 72 + Math.round((jarPct / 999) * 22));
  }
  if (phase === 'extracting-sources') {
    return Math.min(32, 18 + Math.round((rawJavaN / 1000) * 14));
  }
  if (phase === 'indexing') {
    const symPct = Math.min(1, rawJavaN / 250000);
    return Math.min(71, 32 + Math.round(symPct * 39));
  }
  if (rawJavaN > 0 && phase !== 'classpath' && phase !== 'classpath-resolve' && phase !== 'sources') {
    const symPct = Math.min(1, rawJavaN / 250000);
    return Math.min(71, 32 + Math.round(symPct * 39));
  }
  if (phase === 'sources') return 22;
  if (phase === 'running-gradle-compile') return 11;
  if (phase === 'running-gradle-classpath') return 15;
  if (phase === 'running-maven-sources') return 11;
  if (phase === 'running-maven-classpath') return 15;
  if (phase === 'classpath' || phase === 'classpath-resolve') return 14;
  if (phase === 'java-index') return 26;
  if (phase === 'workspace-symbols') return wsN > 0 ? 22 : 12;
  if (wsN > 0) return 25;
  return 8;
}

function formatIndexingProgress(status) {
  const label = status?.label || indexingLabelFromProfile(status?.profile) || 'project';
  const java = status?.java || {};
  const phase = java.phase || status?.phase || 'starting';
  const phaseLabel = indexingPhaseLabel(phase);
  const wsN = status?.workspace_symbols || 0;
  const rawJavaN = java.symbol_count || 0;
  const javaN = phase === 'jar-index'
    ? Math.floor(rawJavaN / 1000) * 1000
    : rawJavaN;
  const jarNote = isBackgroundJarIndexPhase(phase, java) && (java.jars_total || 0) > 0
    ? `${(java.jars_indexed || 0).toLocaleString()}/${java.jars_total.toLocaleString()} JARs`
    : '';
  const parts = [];
  if (wsN > 0) parts.push(`${wsN.toLocaleString()} workspace`);
  if (javaN > 0) parts.push(`${javaN.toLocaleString()} Java`);
  if (java.spring_symbols > 0) parts.push(`${java.spring_symbols.toLocaleString()} Spring`);
  if (jarNote) parts.push(jarNote);
  const detail = parts.length ? parts.join(' · ') : phaseLabel;
  const pct = indexingProgressPercent(status);
  return {
    title: `Indexing ${label}…`,
    phase: phaseLabel,
    stats: parts.join(' · '),
    detail,
    percent: pct,
  };
}

function applyIndexingProgressUi({ title, phase, stats, percent, show, label }) {
  const pct = Math.max(0, Math.min(100, percent || 0));
  const pctText = `${pct}%`;
  const fillBanner = $('#banner-index-progress-fill');
  const fillCard = $('#editor-index-progress-fill');
  const pctBanner = $('#java-index-banner-pct');
  const pctCard = $('#editor-index-overlay-pct');
  const overlay = $('#editor-index-overlay');
  const overlayTitle = $('#editor-index-overlay-title');
  const overlayPhase = $('#editor-index-overlay-text');
  const overlayStats = $('#editor-index-progress-detail');
  const bannerLabel = $('#java-index-banner-label');
  const bannerText = $('#java-index-banner-text');

  if (fillBanner) fillBanner.style.width = `${pct}%`;
  if (fillCard) fillCard.style.width = `${pct}%`;
  if (pctBanner) pctBanner.textContent = show ? pctText : '';
  if (pctCard) pctCard.textContent = show ? pctText : '';
  if (overlayTitle && title) overlayTitle.textContent = title;
  if (overlayPhase) overlayPhase.textContent = phase || '';
  if (overlayStats) overlayStats.textContent = stats || '';
  if (bannerLabel) bannerLabel.textContent = show && label ? label : 'Indexing';
  if (bannerText) {
    bannerText.textContent = stats
      ? `${phase || ''}${phase && stats ? ' — ' : ''}${stats}`
      : (phase || 'Preparing workspace…');
  }
  const banner = $('#java-index-banner');
  if (banner) banner.classList.toggle('hidden', !show);
  // Banner only — never cover the editor.
  overlay?.classList.add('hidden');
  overlay?.setAttribute('aria-hidden', 'true');
}

function clearIndexingProgressUi() {
  $('#java-index-banner')?.classList.add('hidden');
  applyIndexingProgressUi({ show: false });
  state.editorIndexFrozen = false;
  state.indexFreezeActive = false;
  if (state.editor && window.monaco) {
    applyEditorReadOnlyForPath(state.activeTab);
    state.editorIndexFrozenPrevReadOnly = undefined;
  }
}

function indexingLabelFromProfile(profile) {
  const langs = (profile?.languages || []).join(', ');
  const frameworks = (profile?.frameworks || []).filter((f) => f !== 'gradle' && f !== 'maven');
  if (frameworks.length) return frameworks.join(', ');
  if (langs) return langs;
  return 'project';
}

function headerStackLabel(profile) {
  if (!profile) return '';
  const fw = profile.frameworks || [];
  if (fw.includes('spring-boot')) return 'Spring Boot';
  if (fw.includes('maven') && fw.includes('gradle')) return 'Maven/Gradle';
  if (fw.includes('maven')) return 'Maven';
  if (fw.includes('gradle')) return 'Gradle';
  if (fw.includes('rails')) return 'Rails';
  if (fw.includes('django')) return 'Django';
  if (fw.includes('nextjs')) return 'Next.js';
  if (fw.includes('flutter')) return 'Flutter';
  const langs = profile.languages || [];
  if (langs.includes('java')) return 'Java';
  if (langs.includes('kotlin')) return 'Kotlin';
  if (langs.includes('python')) return 'Python';
  if (langs.includes('go')) return 'Go';
  if (langs.includes('ruby')) return 'Ruby';
  if (langs.includes('rust')) return 'Rust';
  if (langs.includes('cpp')) return 'C++';
  if (langs.includes('javascript')) return langs.includes('typescript') ? 'TypeScript' : 'JavaScript';
  return indexingLabelFromProfile(profile);
}

function syncHeaderMenuState() {
  const searchItem = document.querySelector('[data-action="header-search"]');
  if (searchItem) searchItem.disabled = !state.repo;
  const headerSearch = $('#header-search-input');
  if (headerSearch) {
    headerSearch.disabled = !state.repo;
    headerSearch.placeholder = state.repo ? 'Search…' : 'Open a repo…';
  }
}

function updateHeaderBrand() {
  syncHeaderMenuState();
}

function toggleWindowFullscreen() {
  if (window.ipc?.postMessage) {
    window.ipc.postMessage(JSON.stringify({ type: 'toggle-fullscreen' }));
    return;
  }
  if (!document.fullscreenElement) {
    document.documentElement.requestFullscreen?.().catch(() => {});
  } else {
    document.exitFullscreen?.().catch(() => {});
  }
}

function bindHeaderBrand() {
  $('#window-titlebar')?.addEventListener('dblclick', (e) => {
    e.preventDefault();
    toggleWindowFullscreen();
  });
  $('#header-logo-btn')?.addEventListener('click', (e) => {
    e.stopPropagation();
    const root = e.currentTarget.closest('.ij-menu-root');
    const wasOpen = root?.classList.contains('open');
    closeAllMenus();
    if (!wasOpen) root?.classList.add('open');
  });
  $$('.ij-header-logo-dropdown .ij-menu-item[data-action]').forEach((item) => {
    item.addEventListener('click', (e) => {
      e.stopPropagation();
      if (item.disabled) return;
      closeAllMenus();
      runMenuAction(item.dataset.action);
    });
  });
  const headerSearch = $('#header-search-input');
  headerSearch?.addEventListener('focus', () => {
    if (!state.repo) return;
    showSearchEverywhere(headerSearch.value.trim());
  });
  headerSearch?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (state.repo) showSearchEverywhere(headerSearch.value.trim());
      else showRepoPicker();
    }
    if (e.key === 'Escape') headerSearch.blur();
  });
}

function startProjectIndexPolling() {
  stopProjectIndexPolling({ keepUi: true });
  state.projectIndexNotified = false;
  state.projectIndexStartedAt = Date.now();
  pollProjectIndexStatus();
  ensureProjectIndexPollInterval(PROJECT_INDEX_POLL_BACKGROUND_MS);
}

function ensureJavaModuleForPath(path) {
  if (!state.repo || !path) return;
  const p = stripJavaDiagOverlayPath(path).trim();
  if (!p.endsWith('.java')) return;
  const q = new URLSearchParams({ path: p });
  void api(repoApi(state.repo, `/workspace/java/ensure-module?${q}`), { method: 'POST' }).catch(() => {});
}

function startupRepoFromSettings(general) {
  return getInitialRepoFromUrl()
    || general?.default_repo
    || general?.last_repo
    || null;
}

async function ensureStartupIndexPolling() {
  let target;
  try {
    const general = await api('/api/settings/general');
    target = startupRepoFromSettings(general);
  } catch {
    return;
  }
  if (!target) return;
  const repos = state.repos.length ? state.repos : await api('/api/repos').catch(() => []);
  if (Array.isArray(repos) && repos.some((r) => r.name === target)) {
    startProjectIndexPolling();
  }
}

function updateProjectIndexUi(status) {
  const banner = $('#java-index-banner');

  state.projectIndexRunning = projectIndexNeedsFreeze(status);
  const javaReady = status?.java?.state === 'ready'
    && ((status?.java?.symbol_count || 0) > 0 || status?.java?.phase === 'on-demand');
  const javaHasSymbols = (status?.java?.symbol_count || 0) > 0;
  state.projectIndexReady = status?.state === 'ready'
    && (javaReady || (status?.workspace_symbols || 0) > 0)
    || javaHasSymbols;
  state.projectProfile = status?.profile || null;

  updateProjectReloadButton();

  maybeAutoRefreshProjectIndex(status);

  if (projectIndexNeedsFreeze(status)) {
    const progress = formatIndexingProgress(status);
    setStatusMessage(`${progress.title} — ${progress.detail}`);
    applyIndexingProgressUi({
      title: progress.title,
      label: status?.label || indexingLabelFromProfile(status?.profile),
      phase: progress.phase,
      stats: progress.stats,
      percent: progress.percent ?? 5,
      show: true,
    });
    updateHeaderBrand();
    return;
  }

  const bgPhase = status?.java?.phase;
  const java = status?.java || {};
  if (isBackgroundJarIndexPhase(bgPhase, java)) {
    const jarsIndexed = java.jars_indexed || 0;
    const jarsTotal = java.jars_total || 0;
    const phaseLabel = indexingPhaseLabel('jar-index-background');
    const jarLine = jarsTotal > 0
      ? `Core ready · background ${jarsIndexed.toLocaleString()}/${jarsTotal.toLocaleString()} JARs`
      : phaseLabel;
    setStatusMessage(jarLine);
    applyIndexingProgressUi({
      title: jarLine,
      label: status?.label || indexingLabelFromProfile(status?.profile),
      phase: phaseLabel,
      stats: jarLine,
      percent: indexingProgressPercent(status),
      show: true,
    });
    updateHeaderBrand();
    return;
  }

  if (isBackgroundToolingPhase(bgPhase)) {
    const phaseLabel = indexingPhaseLabel(bgPhase);
    setStatusMessage(phaseLabel);
    applyIndexingProgressUi({
      title: `Updating ${status?.label || indexingLabelFromProfile(status?.profile) || 'project'} classpath…`,
      label: status?.label || indexingLabelFromProfile(status?.profile),
      phase: phaseLabel,
      stats: '',
      percent: indexingProgressPercent(status),
      show: true,
    });
    updateHeaderBrand();
    return;
  }

  clearIndexingProgressUi();

  if (status?.state === 'ready' && !state.projectIndexNotified) {
    const java = status.java || {};
    const partial = isBackgroundJarIndexPhase(java.phase, java);
    if (partial) {
      if (!state.projectIndexCoreNotified) {
        state.projectIndexCoreNotified = true;
        const label = status.label || 'Project';
        toast(`${label} core index ready — remaining JARs indexing in background`, 'success');
      }
    } else {
      state.projectIndexNotified = true;
      state.projectIndexCoreNotified = false;
      const background = state.projectReloadBackground;
    state.projectReloadBackground = false;
    if (!background) {
      const label = status.label || 'Project';
      const javaN = status.java?.symbol_count ?? 0;
      const wsN = status.workspace_symbols ?? 0;
      const springN = status.java?.spring_symbols ?? 0;
      const jdkN = status.java?.jdk_symbols ?? 0;
      const total = javaN + wsN;
      const langs = (status.profile?.languages || []).join(', ') || label.toLowerCase();
      const detail = [
        javaN ? `${javaN.toLocaleString()} Java` : '',
        wsN ? `${wsN.toLocaleString()} workspace` : '',
        springN ? `${springN.toLocaleString()} Spring` : '',
        jdkN ? `${jdkN.toLocaleString()} JDK` : '',
      ].filter(Boolean).join(', ');
      toast(
        `${label} index ready — ${total.toLocaleString()} symbols${detail ? ` (${detail})` : ''} [${langs}]`,
        springN > 0 || !status.profile?.frameworks?.includes('spring-boot') ? 'success' : 'warning',
      );
      terminalLog(`${label} index ready: ${total.toLocaleString()} symbols`);
    }
    refreshProjectClasspathUi();
    }
  } else if (status?.state === 'ready' && state.projectIndexCoreNotified && !state.projectIndexNotified) {
    const java = status.java || {};
    if (java.index_complete !== false) {
      state.projectIndexNotified = true;
      state.projectIndexCoreNotified = false;
      const label = status.label || 'Project';
      toast(`${label} index complete`, 'success');
      refreshProjectClasspathUi();
    }
  } else if (status?.state === 'error' && !state.projectIndexNotified) {
    state.projectIndexNotified = true;
    state.projectReloadBackground = false;
    if (!state.projectReloadPending) {
      const errMsg = status.error || status.java?.error || 'unknown error';
      toast(`Project indexing failed: ${errMsg}`, 'error');
      terminalLogError(`Project indexing failed: ${errMsg}`);
    }
  }

  if (!projectIndexNeedsFreeze(status) && state.projectReloadPending) {
    state.projectReloadPending = false;
    scheduleProjectReload(0);
  }
}

function isNativeSourcePath(path) {
  if (!path) return false;
  const lower = path.toLowerCase();
  return lower.endsWith('.c')
    || lower.endsWith('.cpp')
    || lower.endsWith('.cc')
    || lower.endsWith('.cxx')
    || lower.endsWith('.c++');
}

function isDockerBuildFile(path) {
  if (!path) return false;
  const base = path.replace(/\\/g, '/').split('/').pop()?.toLowerCase() || '';
  return base === 'dockerfile' || base.startsWith('dockerfile.')
    || base === 'docker-compose.yml' || base === 'docker-compose.yaml'
    || base === 'compose.yml' || base === 'compose.yaml';
}

function isDockerComposeFile(path) {
  if (!path) return false;
  const base = path.replace(/\\/g, '/').split('/').pop()?.toLowerCase() || '';
  return base === 'docker-compose.yml' || base === 'docker-compose.yaml'
    || base === 'compose.yml' || base === 'compose.yaml';
}

function isNativeTestPath(path) {
  if (!isNativeSourcePath(path)) return false;
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  const lower = base.toLowerCase();
  return (lower.startsWith('test_') && (lower.endsWith('.c') || lower.endsWith('.cpp')))
    || lower.endsWith('_test.c')
    || lower.endsWith('_test.cpp')
    || normalized.includes('/tests/')
    || (normalized.includes('/test/') && (lower.endsWith('.c') || lower.endsWith('.cpp')));
}

function isProjectBuildFile(path) {
  if (!path) return false;
  const normalized = path.replace(/\\/g, '/').toLowerCase();
  const base = normalized.split('/').pop() || '';
  // Java / Groovy / Grails (Gradle & Maven)
  if (base === 'pom.xml') return true;
  if (base === 'build.gradle' || base === 'build.gradle.kts') return true;
  if (base === 'settings.gradle' || base === 'settings.gradle.kts') return true;
  if (base === 'gradle.properties') return true;
  if (normalized.endsWith('/gradle/libs.versions.toml')) return true;
  // Node.js
  if (base === 'package.json') return true;
  // Python (pyproject.toml scripts / tasks)
  if (base === 'pyproject.toml') return true;
  if (base === 'manage.py') return true;
  // Ruby / Rails (tasks — not Gemfile)
  if (base === 'rakefile') return true;
  // Go
  if (base === 'go.mod') return true;
  // C / C++
  if (base === 'cmakelists.txt') return true;
  if (base === 'meson.build') return true;
  if (base === 'makefile' || base === 'gnumakefile') return true;
  if (base === 'vcpkg.json') return true;
  if (base === 'conanfile.txt' || base === 'conanfile.py') return true;
  if (base === 'docker-compose.yml' || base === 'docker-compose.yaml') return true;
  if (base === 'compose.yml' || base === 'compose.yaml') return true;
  if (base === 'dockerfile' || base.startsWith('dockerfile.')) return true;
  // PHP / Composer
  if (base === 'composer.json') return true;
  // Rust / Cargo
  if (base === 'cargo.toml') return true;
  // Dart / Flutter
  if (base === 'pubspec.yaml') return true;
  // Elide (scripts + native :targets)
  if (base === 'elide.pkl') return true;
  // Generic Gradle/Kotlin files
  if (base.endsWith('.gradle') || base.endsWith('.gradle.kts')) return true;
  // Generic TOML (project manifests)
  if (normalized.includes('/gradle/') && base.endsWith('.toml')) return true;
  return false;
}

function buildTasksPanelTitle(buildTool) {
  const labels = {
    maven: 'Maven tasks',
    gradle: 'Gradle tasks',
    npm: 'npm scripts',
    yarn: 'Yarn scripts',
    pnpm: 'pnpm scripts',
    bun: 'Bun scripts',
    cargo: 'Cargo tasks',
    dart: 'Dart tasks',
    flutter: 'Flutter tasks',
    rake: 'Rake tasks',
    rails: 'Rails tasks',
    ruby: 'Ruby tasks',
    go: 'Go tasks',
    cmake: 'CMake tasks',
    make: 'Make targets',
    meson: 'Meson tasks',
    django: 'Django tasks',
    pip: 'Python tasks',
    poetry: 'Poetry tasks',
    uv: 'uv tasks',
    pdm: 'PDM tasks',
    pipenv: 'Pipenv tasks',
    docker: 'Docker tasks',
    elide: 'Elide tasks',
  };
  return labels[buildTool] || 'Build tasks';
}

function buildTaskUsesShellRunner(buildTool) {
  return buildTool !== 'maven' && buildTool !== 'gradle';
}

function buildTaskWorkdir(modulePath) {
  const p = String(modulePath || '').replace(/\\/g, '/').trim();
  if (!p) return undefined;
  const base = p.split('/').pop() || '';
  const lowerBase = base.toLowerCase();
  const manifestNames = new Set([
    'pom.xml', 'build.gradle', 'build.gradle.kts', 'settings.gradle', 'settings.gradle.kts',
    'package.json', 'pyproject.toml', 'manage.py', 'go.mod', 'rakefile', 'cmakelists.txt', 'meson.build', 'makefile', 'gnumakefile',
    'vcpkg.json', 'conanfile.txt', 'conanfile.py', 'pubspec.yaml', 'elide.pkl', 'cargo.toml',
    'docker-compose.yml', 'docker-compose.yaml', 'compose.yml', 'compose.yaml', 'dockerfile',
  ]);
  if (!manifestNames.has(lowerBase) && !lowerBase.startsWith('dockerfile.')) return undefined;
  const dir = p.slice(0, p.length - base.length).replace(/\/$/, '');
  return dir || undefined;
}

function packageManifestKindForPath(path) {
  if (!path) return null;
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  const lower = base.toLowerCase();
  // elide.pkl is build-tasks only (not Package Manifest) — like pom/gradle.
  if (lower === 'cargo.toml') return 'cargo';
  if (lower === 'pubspec.yaml') return 'dart';
  if (lower === 'pyproject.toml') return 'python';
  if (lower === 'requirements.txt') return 'python-reqs';
  if (lower === 'pipfile') return 'pipfile';
  if (lower === 'gemfile' || lower.endsWith('.gemspec')) return 'ruby';
  if (lower === 'go.mod') return 'go';
  if (lower === 'vcpkg.json' || lower === 'conanfile.txt') return 'cpp';
  if (lower === 'cmakelists.txt') return 'cmake';
  if (lower === 'meson.build') return 'meson';
  if (lower === 'makefile' || lower === 'gnumakefile') return 'make';
  return null;
}

function isPackageManifestFile(path) {
  return packageManifestKindForPath(path) !== null;
}

function projectProfileSupportsManifest(kind) {
  const profile = state.projectProfile;
  if (!profile || !kind) return true;
  const langs = new Set(profile.languages || []);
  const frameworks = new Set(profile.frameworks || []);
  switch (kind) {
    case 'cargo':
      return langs.has('rust');
    case 'dart':
      return langs.has('dart') || frameworks.has('flutter');
    case 'ruby':
      return langs.has('ruby') || frameworks.has('rails');
    case 'go':
      return langs.has('go');
    case 'cpp':
      return langs.has('cpp') || langs.has('c')
        || frameworks.has('cmake') || frameworks.has('meson') || frameworks.has('make')
        || frameworks.has('vcpkg') || frameworks.has('conan');
    case 'python':
    case 'python-reqs':
    case 'pipfile':
      return langs.has('python');
    default:
      return true;
  }
}

function projectProfileSupportsBuildTasks(path) {
  const profile = state.projectProfile;
  if (!profile) return true;
  const base = String(path || '').replace(/\\/g, '/').split('/').pop()?.toLowerCase() || '';
  const normalized = String(path || '').replace(/\\/g, '/').toLowerCase();
  
  // Build files always support build tasks
  if (base === 'pom.xml') return true;
  if (base === 'build.gradle' || base === 'build.gradle.kts' || base === 'settings.gradle' || base === 'settings.gradle.kts' || base === 'gradle.properties') return true;
  if (base === 'package.json') return true;
  if (base === 'rakefile') return true;
  if (base === 'pyproject.toml' || base === 'manage.py') return true;
  if (base === 'go.mod') return true;
  if (base === 'cmakelists.txt' || base === 'meson.build' || base === 'makefile' || base === 'gnumakefile') return true;
  if (base === 'vcpkg.json') return true;
  if (base === 'conanfile.txt' || base === 'conanfile.py') return true;
  if (base === 'docker-compose.yml' || base === 'docker-compose.yaml' || base === 'compose.yml' || base === 'compose.yaml' || base === 'dockerfile' || base.startsWith('dockerfile.')) return true;
  if (normalized.endsWith('/gradle/libs.versions.toml')) return true;
  if (base.endsWith('.gradle') || base.endsWith('.gradle.kts')) return true;
  if (base === 'cargo.toml') return true;
  if (base === 'pubspec.yaml') return true;
  if (base === 'elide.pkl') return true;
  
  return true;
}

function shouldAutoOpenPackageManifest(path) {
  const kind = packageManifestKindForPath(path);
  if (!kind || !projectProfileSupportsManifest(kind)) return false;
  // pyproject.toml / go.mod / Makefile / CMake / vcpkg open build tasks; manifest for deps on demand.
  if ((kind === 'python' || kind === 'go' || kind === 'cpp' || kind === 'make' || kind === 'cmake' || kind === 'meson')
    && isProjectBuildFile(path)) return false;
  return true;
}

function projectProfileSupportsNativeBuildTasks() {
  const profile = state.projectProfile;
  if (!profile) return true;
  const langs = new Set(profile.languages || []);
  const frameworks = new Set(profile.frameworks || []);
  return langs.has('cpp') || langs.has('c')
    || frameworks.has('cmake') || frameworks.has('meson') || frameworks.has('make');
}

function shouldAutoOpenBuildTasks(path) {
  if (!path) return false;
  if (isDockerBuildFile(path)) return true;
  if (isProjectBuildFile(path) && projectProfileSupportsBuildTasks(path)) return true;
  if (isNativeSourcePath(path) && projectProfileSupportsNativeBuildTasks()) return true;
  return false;
}

function shouldAutoOpenBuildTasksPanelImmediately(path) {
  return isDockerBuildFile(path)
    || (isProjectBuildFile(path) && projectProfileSupportsBuildTasks(path));
}

let packageManifestTimer = null;
let packageManifestRequestId = 0;

function applyPackageManifestDock() {
  const panel = $('#panel-package-manifest');
  const leftDock = $('#package-manifest-dock-left');
  const rightDock = $('#package-manifest-dock-right');
  const leftResizer = $('#package-manifest-left-resizer');
  const rightResizer = $('#package-manifest-right-resizer');
  if (!panel || !leftDock || !rightDock) return;

  const dock = state.packageManifestDock;
  const open = state.packageManifestPanelOpen;

  if (dock === 'left') {
    leftDock.appendChild(panel);
    leftDock.classList.toggle('hidden', !open);
    leftDock.classList.toggle('flex', open);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
    leftResizer?.classList.toggle('hidden', !open);
    rightResizer?.classList.toggle('hidden', true);
  } else {
    rightDock.appendChild(panel);
    rightDock.classList.toggle('hidden', !open);
    rightDock.classList.toggle('flex', open);
    leftDock.classList.add('hidden');
    leftDock.classList.remove('flex');
    rightResizer?.classList.toggle('hidden', !open);
    leftResizer?.classList.toggle('hidden', true);
  }

  panel.classList.toggle('hidden', !open);
  panel.classList.toggle('flex', open);

  $$('[data-package-manifest-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.packageManifestDock === dock);
  });
  syncDockMenuControls();
}

function showPackageManifestPanel() {
  state.packageManifestPanelOpen = true;
  applyPackageManifestDock();
  void loadPackageManifest(state.activeTab);
}

function hidePackageManifestPanel() {
  state.packageManifestPanelOpen = false;
  applyPackageManifestDock();
}

function togglePackageManifestPanel() {
  if (state.packageManifestPanelOpen) hidePackageManifestPanel();
  else showPackageManifestPanel();
}

function schedulePackageManifestRefresh() {
  if (packageManifestTimer) clearTimeout(packageManifestTimer);
  packageManifestTimer = setTimeout(() => {
    packageManifestTimer = null;
    const tab = stripJavaDiagOverlayPath(state.activeTab || '').trim();
    if (!tab || !state.repo) return;
    if (shouldAutoOpenPackageManifest(tab)) {
      if (state.buildTasksPanelOpen) hideBuildTasksPanel();
      void updatePackageManifestPanel(tab);
    } else if (shouldAutoOpenBuildTasks(tab) && state.packageManifestPanelOpen) {
      hidePackageManifestPanel();
    } else if (state.packageManifestPanelOpen && isPackageManifestFile(tab)) {
      void loadPackageManifest(tab);
    }
  }, 180);
}

function updatePackageManifestPanel(path) {
  if (!state.repo) return;
  const tab = stripJavaDiagOverlayPath(path || state.activeTab || '').trim();
  if (!tab) return;
  if (shouldAutoOpenPackageManifest(tab)) {
    if (state.buildTasksPanelOpen) hideBuildTasksPanel();
    if (!state.packageManifestPanelOpen) {
      state.packageManifestPanelOpen = true;
      applyPackageManifestDock();
    }
    void loadPackageManifest(tab);
  } else if (state.packageManifestPanelOpen) {
    void loadPackageManifest(tab);
  }
}

function packageManifestEcosystemLabel(ecosystem) {
  const labels = {
    cargo: 'Rust · Cargo',
    dart: 'Dart · pub',
    flutter: 'Flutter · pub',
    python: 'Python',
    ruby: 'Ruby · Bundler',
    rake: 'Ruby · Rake',
    go: 'Go modules',
    cmake: 'C/C++ · CMake',
    meson: 'C/C++ · Meson',
    make: 'C/C++ · Make',
    cpp: 'C/C++',
  };
  return labels[ecosystem] || ecosystem;
}

function renderPackageManifest(view, container) {
  if (!container) return;
  const filter = (state.packageManifestFilter || '').trim().toLowerCase();
  if (!view) {
    container.innerHTML = '<div class="ij-package-manifest-empty">No manifest data.</div>';
    return;
  }

  const actionsHtml = (view.actions || []).map((a) =>
    `<button type="button" class="ij-package-manifest-action" data-manifest-cmd="${escapeHtml(a.command)}" title="${escapeHtml(a.command)}">${escapeHtml(a.label)}</button>`,
  ).join('');

  const fieldsHtml = (view.fields || []).map((f) =>
    `<span class="ij-package-manifest-field-label">${escapeHtml(f.label)}</span><span class="ij-package-manifest-field-value">${escapeHtml(f.value)}</span>`,
  ).join('');

  const sectionsHtml = (view.sections || []).map((sec) => {
    const items = (sec.items || []).filter((it) => {
      if (!filter) return true;
      return `${it.name} ${it.detail || ''}`.toLowerCase().includes(filter);
    });
    if (!items.length && filter) return '';
    const rows = items.map((it) =>
      `<div class="ij-package-manifest-item"><span class="ij-package-manifest-item-name">${escapeHtml(it.name)}</span>${it.detail ? `<span class="ij-package-manifest-item-detail">${escapeHtml(it.detail)}</span>` : ''}</div>`,
    ).join('');
    return `<details class="ij-package-manifest-section" open><summary>${escapeHtml(sec.title)} (${items.length})</summary>${rows || '<div class="ij-package-manifest-empty">Empty</div>'}</details>`;
  }).filter(Boolean).join('');

  container.innerHTML = `
    ${actionsHtml ? `<div class="ij-package-manifest-actions">${actionsHtml}</div>` : ''}
    ${fieldsHtml ? `<div class="ij-package-manifest-fields">${fieldsHtml}</div>` : ''}
    ${sectionsHtml || (!filter ? '<div class="ij-package-manifest-empty">No dependencies listed.</div>' : '<div class="ij-package-manifest-empty">No matches for filter.</div>')}
  `;

  container.querySelectorAll('[data-manifest-cmd]').forEach((btn) => {
    btn.addEventListener('click', () => {
      void runManifestCommand(btn.dataset.manifestCmd, view.package_root);
    });
  });
}

async function loadPackageManifest(path) {
  if (!state.repo) return;
  const sourcePath = stripJavaDiagOverlayPath(path || state.activeTab || '').trim();
  if (!sourcePath) return;
  const requestId = ++packageManifestRequestId;
  const bodyEl = $('#package-manifest-body');
  const titleEl = $('#package-manifest-title');
  const subtitleEl = $('#package-manifest-subtitle');
  const ecoEl = $('#package-manifest-ecosystem');
  const filterEl = $('#package-manifest-filter');
  if (filterEl && filterEl.value !== (state.packageManifestFilter || '')) {
    filterEl.value = state.packageManifestFilter || '';
  }
  if (bodyEl) bodyEl.innerHTML = '<div class="ij-package-manifest-empty">Loading manifest…</div>';
  try {
    const q = `?path=${encodeURIComponent(sourcePath)}`;
    const view = await api(repoApi(state.repo, `/workspace/package/manifest${q}`));
    if (requestId !== packageManifestRequestId) return;
    if (stripJavaDiagOverlayPath(state.activeTab || '').trim() !== sourcePath
      && !isPackageManifestFile(stripJavaDiagOverlayPath(state.activeTab || ''))) {
      return;
    }
    state.packageManifestView = view;
    if (ecoEl) ecoEl.textContent = packageManifestEcosystemLabel(view.ecosystem);
    if (titleEl) titleEl.textContent = view.title || 'Package';
    if (subtitleEl) subtitleEl.textContent = view.subtitle || view.manifest_path || '';
    renderPackageManifest(view, bodyEl);
  } catch (err) {
    if (requestId !== packageManifestRequestId) return;
    if (bodyEl) {
      bodyEl.innerHTML = `<div class="ij-package-manifest-empty">${escapeHtml(err.message || 'Failed to load manifest')}</div>`;
    }
  }
}

async function runManifestCommand(command, packageRoot) {
  if (!state.repo || !command) return;
  if (state.dirty.has(state.activeTab)) await saveFile({ silent: true, skipProjectReload: true });
  showTerminal();
  const term = getActiveTerminal();
  if (!term) {
    toast('Terminal not ready — try again', 'error');
    return;
  }
  const cwd = packageRoot || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

let buildTasksTimer = null;
let buildTasksRequestId = 0;
let structureRefreshTimer = null;
let structureCaretTimer = null;

function isAstSupportedPath(path) {
  if (!path || isExternalEditorPath(path)) return false;
  const base = String(path).replace(/\\/g, '/').split('/').pop() || '';
  const lower = base.toLowerCase();
  if (lower === 'dockerfile' || lower.startsWith('dockerfile.')) return false;
  const ext = lower.includes('.') ? lower.split('.').pop() : '';
  return AST_LANG_EXTS.has(ext);
}

function syncStructureModeButtons() {
  const mode = state.structureMode === 'full' ? 'full' : 'structure';
  $$('[data-ast-mode]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.astMode === mode);
  });
}

function setStructureMode(mode) {
  const next = mode === 'full' ? 'full' : 'structure';
  if (state.structureMode === next) return;
  state.structureMode = next;
  localStorage.setItem(STRUCTURE_MODE_KEY, next);
  syncStructureModeButtons();
  void refreshStructurePanel({ force: true });
}

function scheduleStructureRefresh() {
  if (state.activePanel !== 'structure') return;
  clearTimeout(structureRefreshTimer);
  structureRefreshTimer = setTimeout(() => {
    structureRefreshTimer = null;
    void refreshStructurePanel();
  }, STRUCTURE_REFRESH_DELAY_MS);
}

function scheduleStructureCaretHighlight() {
  if (state.activePanel !== 'structure') return;
  clearTimeout(structureCaretTimer);
  structureCaretTimer = setTimeout(() => {
    structureCaretTimer = null;
    highlightStructureUnderCaret();
  }, 80);
}

function structureNodeMatchesFilter(node, filter) {
  if (!filter) return true;
  const mods = Array.isArray(node.modifiers) ? node.modifiers.join(' ') : '';
  const hay = `${node.kind || ''} ${node.name || ''} ${node.label || ''} ${node.detail || ''} ${mods}`.toLowerCase();
  if (hay.includes(filter)) return true;
  return (node.children || []).some((c) => structureNodeMatchesFilter(c, filter));
}

/** Map tree-sitter / outline node kinds → Structure icon buckets. */
function structureIconKind(kind) {
  const k = String(kind || '').toLowerCase();
  // Exact outline kinds first (avoid "constructor".includes("struct")).
  if (k === 'constructor') return 'constructor';
  if (k === 'method' || k === 'function') return 'method';
  if (k === 'field' || k === 'property') return 'field';
  if (k === 'class' || k === 'record') return 'class';
  if (k === 'interface') return 'interface';
  if (k === 'enum') return 'enum';
  if (k === 'annotation') return 'annotation';
  if (k === 'struct') return 'struct';
  if (k === 'trait') return 'trait';
  if (!k || k === 'error' || k.includes('missing')) return 'error';
  if (k.includes('interface')) return 'interface';
  if (k.includes('enum')) return 'enum';
  if (k.includes('annotation')) return 'annotation';
  if (k.includes('constructor')) return 'constructor';
  if (k.includes('record') || k.includes('class')) return 'class';
  if (k.includes('struct')) return 'struct';
  if (k.includes('trait') || k.includes('protocol')) return 'trait';
  if (k.includes('impl')) return 'impl';
  if (
    k.includes('method')
    || k.includes('function')
    || k === 'fn_item'
    || k.includes('arrow_function')
    || k.includes('function_item')
  ) {
    return 'method';
  }
  if (
    k.includes('field')
    || k.includes('property')
    || k.includes('variable_declarator')
    || k.includes('variable_declaration')
    || k.includes('lexical_declaration')
  ) {
    return 'field';
  }
  if (k.includes('const') || k.includes('static_item') || k.includes('constant')) return 'const';
  if (
    k.includes('type_alias')
    || k.includes('type_item')
    || k.includes('type_definition')
    || k.includes('type_declaration')
  ) {
    return 'type';
  }
  if (
    k.includes('package')
    || k.includes('module')
    || k.includes('namespace')
    || k === 'mod_item'
    || k === 'program'
    || k === 'source_file'
    || k === 'compilation_unit'
    || k === 'document'
  ) {
    return 'module';
  }
  if (k.includes('import') || k.includes('export') || k.includes('use_declaration')) return 'import';
  if (k === 'pair' || k.includes('mapping_pair')) return 'key';
  return 'node';
}

function structureIconSvg(iconKind) {
  const badges = {
    class: ['C', '#9876aa'],
    interface: ['I', '#6a8759'],
    enum: ['E', '#cc7832'],
    struct: ['S', '#9876aa'],
    trait: ['T', '#6a8759'],
    impl: ['i', '#6897bb'],
    method: ['m', '#ffc66d'],
    constructor: ['+', '#ffc66d'],
    field: ['f', '#6897bb'],
    const: ['=', '#6897bb'],
    type: ['t', '#a9b7c6'],
    module: ['P', '#bbb529'],
    import: ['→', '#a9b7c6'],
    key: ['k', '#cc7832'],
    annotation: ['@', '#bbb529'],
    error: ['!', '#bc3f3c'],
    node: ['·', '#808080'],
  };
  const [letter, color] = badges[iconKind] || badges.node;
  const fontSize = 8;
  return `<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="${color}" fill-opacity=".2"/><text x="8" y="11.2" text-anchor="middle" font-size="${fontSize}" font-weight="700" fill="${color}" font-family="Consolas,Menlo,monospace">${letter}</text></svg>`;
}

function shortAstKind(kind) {
  const k = String(kind || '');
  if (!k) return 'node';
  const mapped = {
    method_declaration: 'method',
    method_definition: 'method',
    function_declaration: 'function',
    function_definition: 'function',
    function_item: 'function',
    arrow_function: 'function',
    class_declaration: 'class',
    class_definition: 'class',
    class_specifier: 'class',
    interface_declaration: 'interface',
    enum_declaration: 'enum',
    enum_item: 'enum',
    struct_item: 'struct',
    trait_item: 'trait',
    impl_item: 'impl',
    field_declaration: 'field',
    property_identifier: 'property',
    package_declaration: 'package',
    import_declaration: 'import',
    import_statement: 'import',
    export_statement: 'export',
    use_declaration: 'use',
    type_alias_declaration: 'type',
    type_item: 'type',
    lexical_declaration: 'var',
    variable_declaration: 'var',
    variable_declarator: 'var',
    const_item: 'const',
    pair: 'key',
    block_mapping_pair: 'key',
    program: 'file',
    source_file: 'file',
    compilation_unit: 'file',
  };
  if (mapped[k]) return mapped[k];
  return k
    .replace(/_declaration$/i, '')
    .replace(/_definition$/i, '')
    .replace(/_statement$/i, '')
    .replace(/_item$/i, '')
    .replace(/_/g, ' ');
}

function prettyAstKind(kind) {
  return String(kind || '')
    .split('_')
    .filter(Boolean)
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
    .join(' ');
}

function structureKindTag(kind) {
  const k = String(kind || '').toLowerCase();
  if (['class', 'interface', 'enum', 'record', 'annotation', 'method', 'field', 'constructor', 'struct', 'trait', 'function'].includes(k)) {
    return k;
  }
  return shortAstKind(kind);
}

function structureModifierTagsHtml(modifiers) {
  if (!Array.isArray(modifiers) || !modifiers.length) return '';
  return `<span class="ij-structure-tags">${modifiers.map((m) => (
    `<span class="ij-structure-tag ij-structure-tag-mod">${escapeHtml(m)}</span>`
  )).join('')}</span>`;
}

function structureNodeLabelHtml(node) {
  const iconKind = structureIconKind(node.kind);
  const icon = `<span class="ij-tree-icon ij-structure-icon ij-structure-icon-${escapeHtml(iconKind)}" aria-hidden="true">${structureIconSvg(iconKind)}</span>`;
  const fullMode = state.structureMode === 'full';

  if (fullMode) {
    const text = node.label || node.name || prettyAstKind(node.kind);
    return `${icon}<span class="ij-structure-node-anon">${escapeHtml(text)}</span>`;
  }

  // Structure: icon · name · modifier tags · return/field type
  // Kind is conveyed by the icon (class/method/field); avoid redundant kind chips.
  const displayName = node.name || node.label || shortAstKind(node.kind);
  const modsHtml = structureModifierTagsHtml(node.modifiers);
  const detailHtml = node.detail
    ? `<span class="ij-structure-node-detail">${escapeHtml(node.detail)}</span>`
    : '';
  return `${icon}<span class="ij-structure-node-name">${escapeHtml(displayName)}</span>${modsHtml}${detailHtml}`;
}

function renderStructureNode(node, depth, filter) {
  // Skip invisible file wrapper — show class roots directly.
  if (node.kind === 'file' || node.kind === 'package') {
    return (node.children || [])
      .filter((c) => structureNodeMatchesFilter(c, filter))
      .map((c) => renderStructureNode(c, depth, filter))
      .join('');
  }
  if (!structureNodeMatchesFilter(node, filter)) return '';
  const kids = (node.children || []).filter((c) => structureNodeMatchesFilter(c, filter));
  const hasKids = kids.length > 0;
  const label = structureNodeLabelHtml(node);
  const titleParts = [
    node.kind,
    node.name,
    ...(node.modifiers || []),
    node.detail,
  ].filter(Boolean);
  const title = escapeHtml(titleParts.join(' · '));
  const data = `data-sl="${node.start_line || 1}" data-sc="${node.start_column || 1}" data-el="${node.end_line || 1}" data-ec="${node.end_column || 1}" data-kind="${escapeHtml(node.kind || '')}"`;
  const open = depth < 2 || !!filter;
  if (!hasKids) {
    return `<div class="ij-tree-row ij-structure-node-row" style="--depth:${depth}" ${data} title="${title}">${label}</div>`;
  }
  const childHtml = kids.map((c) => renderStructureNode(c, depth + 1, filter)).join('');
  return `<details class="ij-structure-branch" ${open ? 'open' : ''}>
    <summary class="ij-tree-row ij-tree-dir-row ij-structure-node-row" style="--depth:${depth}" ${data} title="${title}" aria-expanded="${open ? 'true' : 'false'}">
      <span class="ij-tree-chevron" aria-hidden="true"></span>
      ${label}
    </summary>
    <div class="ij-structure-children">${childHtml}</div>
  </details>`;
}

function renderStructureTree(ast) {
  const tree = $('#structure-tree');
  const subtitle = $('#structure-subtitle');
  if (!tree) return;
  if (!ast?.root) {
    tree.innerHTML = `<div class="ij-structure-empty">Open a supported source file to view its structure.</div>`;
    if (subtitle) subtitle.textContent = '';
    return;
  }
  const filter = (state.structureFilter || '').trim().toLowerCase();
  if (subtitle) {
    const modeLabel = ast.mode === 'full' ? 'AST' : 'Structure';
    subtitle.textContent = `${ast.language || '?'} · ${modeLabel} · ${ast.path || ''}`;
  }
  const html = renderStructureNode(ast.root, 0, filter);
  tree.innerHTML = html
    ? `<div class="ij-structure-tree">${html}</div>`
    : `<div class="ij-structure-empty">No nodes match this filter.</div>`;
  highlightStructureUnderCaret();
}

async function refreshStructurePanel({ force = false } = {}) {
  const tree = $('#structure-tree');
  const subtitle = $('#structure-subtitle');
  if (!tree) return;
  if (!state.repo) {
    state.structureAst = null;
    state.structurePath = null;
    tree.innerHTML = `<div class="ij-structure-empty">Select a repo to browse structure.</div>`;
    if (subtitle) subtitle.textContent = '';
    return;
  }
  const path = state.activeTab;
  if (!path || !isAstSupportedPath(path)) {
    state.structureAst = null;
    state.structurePath = path || null;
    tree.innerHTML = `<div class="ij-structure-empty">Structure is available for Java, Python, JS/TS, Go, Rust, C/C++, JSON, and YAML.</div>`;
    if (subtitle) subtitle.textContent = path ? path : '';
    return;
  }
  if (!force && state.structureAst && state.structurePath === path && state.activePanel !== 'structure') {
    return;
  }
  const seq = ++state.structureSeq;
  const mode = state.structureMode === 'full' ? 'full' : 'structure';
  const content = state.editor && state.activeTab === path
    ? state.editor.getValue()
    : (state.tabContents.get(path) ?? null);
  try {
    const body = { path, mode };
    if (content != null) body.content = content;
    const ast = await api(repoApi(state.repo, '/workspace/ast'), {
      method: 'POST',
      body: JSON.stringify(body),
      timeoutMs: 30_000,
    });
    if (seq !== state.structureSeq) return;
    state.structureAst = ast;
    state.structurePath = path;
    renderStructureTree(ast);
  } catch (err) {
    if (seq !== state.structureSeq) return;
    state.structureAst = null;
    state.structurePath = path;
    tree.innerHTML = `<div class="ij-structure-empty">${escapeHtml(err.message || 'Failed to parse AST')}</div>`;
    if (subtitle) subtitle.textContent = path;
  }
}

function positionInAstRange(line, column, node) {
  const sl = node.start_line || 1;
  const sc = node.start_column || 1;
  const el = node.end_line || sl;
  const ec = node.end_column || sc;
  if (line < sl || line > el) return false;
  if (line === sl && column < sc) return false;
  if (line === el && column > ec) return false;
  return true;
}

function findDeepestAstNode(node, line, column, best = null) {
  if (!node || !positionInAstRange(line, column, node)) return best;
  let next = node;
  for (const child of node.children || []) {
    const hit = findDeepestAstNode(child, line, column, null);
    if (hit) next = hit;
  }
  return next;
}

function highlightStructureUnderCaret() {
  const tree = $('#structure-tree');
  if (!tree || !state.structureAst?.root || !state.editor) return;
  const pos = state.editor.getPosition();
  if (!pos) return;
  const hit = findDeepestAstNode(state.structureAst.root, pos.lineNumber, pos.column);
  tree.querySelectorAll('.ij-structure-node-row.caret-active').forEach((el) => {
    el.classList.remove('caret-active');
  });
  if (!hit) return;
  const selector = `.ij-structure-node-row[data-sl="${hit.start_line}"][data-sc="${hit.start_column}"][data-el="${hit.end_line}"][data-ec="${hit.end_column}"]`;
  const row = tree.querySelector(selector);
  if (!row) return;
  row.classList.add('caret-active');
  let details = row.closest('details');
  while (details) {
    details.open = true;
    details = details.parentElement?.closest('details') || null;
  }
}

function onStructureTreeClick(e) {
  const row = e.target.closest('.ij-structure-node-row');
  if (!row) return;
  if (e.target.closest('.ij-tree-chevron')) return;
  // Keep Structure open — don't route through openFileAt/activateTabShell
  // (those call revealFileInExplorer and switch to Project).
  e.preventDefault();
  e.stopPropagation();
  const line = Number(row.dataset.sl || 1);
  const column = Number(row.dataset.sc || 1);
  $('#structure-tree')?.querySelectorAll('.ij-structure-node-row.active').forEach((el) => {
    el.classList.remove('active');
  });
  row.classList.add('active');
  if (!state.editor || !state.activeTab) return;
  state.editor.revealLineInCenter(line);
  state.editor.setPosition({ lineNumber: line, column });
  state.editor.focus();
  highlightStructureUnderCaret();
}

function scheduleBuildTasksRefresh(options = {}) {
  const { fromDisk = false } = options;
  if (buildTasksTimer) clearTimeout(buildTasksTimer);
  if (fromDisk) {
    buildTasksTimer = null;
    const tab = stripJavaDiagOverlayPath(state.activeTab || '').trim();
    if (!tab || !state.repo) return;
    if (shouldAutoOpenBuildTasks(tab)) {
      if (state.packageManifestPanelOpen) hidePackageManifestPanel();
      void updateBuildTasksPanel(tab, { fromDisk: true });
    } else if (state.buildTasksPanelOpen) {
      void loadBuildTasksTree(tab, { fromDisk: true });
    }
    return;
  }
  buildTasksTimer = setTimeout(() => {
    buildTasksTimer = null;
    const tab = stripJavaDiagOverlayPath(state.activeTab || '').trim();
    if (!tab || !state.repo) return;
    if (shouldAutoOpenBuildTasks(tab)) {
      if (state.packageManifestPanelOpen) hidePackageManifestPanel();
      void updateBuildTasksPanel(tab);
    } else if (shouldAutoOpenPackageManifest(tab) && state.buildTasksPanelOpen) {
      hideBuildTasksPanel();
    } else if (state.buildTasksPanelOpen) {
      void loadBuildTasksTree(tab);
    }
  }, 400);
}

function applyBuildTasksDock() {
  const panel = $('#panel-build-tasks');
  const leftDock = $('#build-tasks-dock-left');
  const rightDock = $('#build-tasks-dock-right');
  const leftResizer = $('#build-tasks-left-resizer');
  const rightResizer = $('#build-tasks-right-resizer');
  if (!panel || !leftDock || !rightDock) return;

  const dock = state.buildTasksDock;
  const open = state.buildTasksPanelOpen;

  if (dock === 'left') {
    leftDock.appendChild(panel);
    leftDock.classList.toggle('hidden', !open);
    leftDock.classList.toggle('flex', open);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
    leftResizer?.classList.toggle('hidden', !open);
    rightResizer?.classList.toggle('hidden', true);
  } else {
    rightDock.appendChild(panel);
    rightDock.classList.toggle('hidden', !open);
    rightDock.classList.toggle('flex', open);
    leftDock.classList.add('hidden');
    leftDock.classList.remove('flex');
    rightResizer?.classList.toggle('hidden', !open);
    leftResizer?.classList.toggle('hidden', true);
  }

  panel.classList.toggle('hidden', !open);
  panel.classList.toggle('flex', open);

  $$('[data-build-tasks-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.buildTasksDock === dock);
  });
  $('#btn-build-tasks')?.classList.toggle('active', open);
  syncDockMenuControls();
}

function setBuildTasksDock(dock) {
  if (!['left', 'right'].includes(dock)) return;
  state.buildTasksDock = dock;
  localStorage.setItem(BUILD_TASKS_DOCK_KEY, dock);
  applyBuildTasksDock();
}

function setPackageManifestDock(dock) {
  if (!['left', 'right'].includes(dock)) return;
  state.packageManifestDock = dock;
  localStorage.setItem(PACKAGE_MANIFEST_DOCK_KEY, dock);
  applyPackageManifestDock();
}

function resolveBuildTasksPath(path) {
  const p = stripJavaDiagOverlayPath(path || state.activeTab || '').trim();
  if (p) return p;
  return 'settings.gradle';
}

function showBuildTasksPanel() {
  state.buildTasksPanelOpen = true;
  applyBuildTasksDock();
  void loadBuildTasksTree(resolveBuildTasksPath(state.activeTab));
}

function hideBuildTasksPanel() {
  state.buildTasksPanelOpen = false;
  applyBuildTasksDock();
}

function toggleBuildTasksPanel() {
  if (state.buildTasksPanelOpen) hideBuildTasksPanel();
  else showBuildTasksPanel();
}

function updateBuildTasksPanel(path, options = {}) {
  if (!state.repo) return;
  const tab = stripJavaDiagOverlayPath(path || state.activeTab || '').trim();
  if (!tab) return;
  if (shouldAutoOpenBuildTasks(tab)) {
    if (state.packageManifestPanelOpen) hidePackageManifestPanel();
    if (shouldAutoOpenBuildTasksPanelImmediately(tab)) {
      if (!state.buildTasksPanelOpen) {
        state.buildTasksPanelOpen = true;
        applyBuildTasksDock();
      }
    }
    void loadBuildTasksTree(tab, options);
    return;
  }
  if (state.buildTasksPanelOpen) {
    void loadBuildTasksTree(tab, options);
  }
}

async function loadBuildTasksTree(path, options = {}) {
  const { fromDisk = false } = options;
  if (!state.repo) return;
  const sourcePath = stripJavaDiagOverlayPath(path || state.activeTab || '').trim();
  const apiPath = resolveBuildTasksPath(sourcePath);
  if (!apiPath) return;
  const requestId = ++buildTasksRequestId;
  const treeEl = $('#build-tasks-tree');
  const titleEl = $('#build-tasks-title');
  const subtitleEl = $('#build-tasks-subtitle');
  const filterEl = $('#build-tasks-filter');
  if (filterEl && filterEl.value !== (state.buildTasksFilter || '')) {
    filterEl.value = state.buildTasksFilter || '';
  }
  state.buildTasksSelected = null;
  state.buildTasksFocusKey = buildTaskModuleKeyFromPath(sourcePath);
  if (treeEl) treeEl.innerHTML = '<div class="ij-build-tasks-empty">Loading tasks…</div>';
  try {
    const q = `?path=${encodeURIComponent(apiPath)}`;
    const editingCompose = !fromDisk
      && isDockerComposeFile(apiPath)
      && stripJavaDiagOverlayPath(state.activeTab || '') === sourcePath;
    const composeContent = editingCompose
      ? (state.tabContents.get(sourcePath) ?? state.editor?.getValue?.() ?? '')
      : null;
    const tree = editingCompose
      ? await api(repoApi(state.repo, '/workspace/build/tasks-tree'), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: apiPath, content: composeContent }),
      })
      : await api(repoApi(state.repo, `/workspace/build/tasks-tree${q}`));
    if (requestId !== buildTasksRequestId) return;
    if (resolveBuildTasksPath(state.activeTab) !== apiPath
      && stripJavaDiagOverlayPath(state.activeTab || '').trim() !== sourcePath) {
      return;
    }
    state.buildTasksTree = tree;
    if (titleEl) {
      titleEl.textContent = buildTasksPanelTitle(tree.build_tool);
    }
    if (subtitleEl) {
      subtitleEl.textContent = tree.root_name
        ? `${tree.root_name}${tree.root_path ? ` · ${tree.root_path}` : ''}`
        : '';
    }
    if (treeEl) {
      if (tree.build_tool) {
        if (isNativeSourcePath(sourcePath) && projectProfileSupportsNativeBuildTasks()) {
          if (state.packageManifestPanelOpen) hidePackageManifestPanel();
          if (!state.buildTasksPanelOpen) {
            state.buildTasksPanelOpen = true;
            applyBuildTasksDock();
          }
        }
        state.buildTasksFocusKey = tree.focus_module ?? buildTaskModuleKeyFromPath(sourcePath);
        renderBuildTasksExplorer(tree.tree, treeEl, state.buildTasksFocusKey);
      } else {
        treeEl.innerHTML = '<div class="ij-build-tasks-empty">No build tasks found for this file.</div>';
      }
    }
  } catch (err) {
    if (requestId !== buildTasksRequestId) return;
    if (treeEl) treeEl.innerHTML = `<div class="ij-build-tasks-empty">${escapeHtml(err.message || 'Failed to load tasks')}</div>`;
  }
}

function buildTaskRunIcon() {
  return '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><circle cx="8" cy="8" r="6.5" stroke="currentColor" stroke-opacity=".45"/><path d="M6.5 5.2v5.6l5-2.8-5-2.8Z" fill="currentColor" fill-opacity=".85"/></svg>';
}

function buildTaskGroupIcon() {
  return '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3 4.5h10M3 8h7M3 11.5h9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-opacity=".55"/></svg>';
}

function buildTaskModuleKeyFromPath(path) {
  const p = String(stripJavaDiagOverlayPath(path) || '').replace(/\\/g, '/').trim();
  const base = p.split('/').pop() || '';
  const lower = base.toLowerCase();
  const buildManifestNames = new Set([
    'pom.xml', 'build.gradle', 'build.gradle.kts', 'package.json', 'rakefile',
    'cmakelists.txt', 'meson.build', 'makefile', 'gnumakefile',
    'vcpkg.json', 'conanfile.txt', 'conanfile.py', 'go.mod',
    'elide.pkl', 'cargo.toml', 'pubspec.yaml',
  ]);
  if (buildManifestNames.has(lower)) {
    return p.slice(0, p.length - base.length).replace(/\/$/, '');
  }
  if (base === 'settings.gradle' || base === 'settings.gradle.kts' || base === 'gradle.properties') {
    return '';
  }
  return null;
}

function buildTaskNodeModuleKey(node) {
  const p = String(node?.path || '').replace(/\\/g, '/');
  const base = p.split('/').pop() || '';
  const lower = base.toLowerCase();
  const buildManifestNames = new Set([
    'pom.xml', 'build.gradle', 'build.gradle.kts', 'package.json', 'rakefile',
    'cmakelists.txt', 'meson.build', 'makefile', 'gnumakefile',
    'vcpkg.json', 'conanfile.txt', 'conanfile.py', 'go.mod',
    'elide.pkl', 'cargo.toml', 'pubspec.yaml',
  ]);
  if (buildManifestNames.has(lower)) {
    return p.slice(0, p.length - base.length).replace(/\/$/, '');
  }
  if (node?.kind === 'gradle-group') return p;
  if (node?.kind?.endsWith('-root')) return p.slice(0, p.length - base.length).replace(/\/$/, '');
  return p.replace(/\/$/, '');
}

function computeBuildTaskFocusContext(root, focusKey) {
  const expandNodePaths = new Set();
  let focusNodePath = null;
  if (focusKey === null || focusKey === undefined) {
    return { focusKey, focusNodePath, expandNodePaths };
  }

  function walk(node) {
    if (buildTaskNodeModuleKey(node) === focusKey) {
      focusNodePath = node.path;
      return true;
    }
    for (const child of node.children || []) {
      if (walk(child)) {
        expandNodePaths.add(node.path);
        return true;
      }
    }
    return false;
  }

  walk(root);
  return { focusKey, focusNodePath, expandNodePaths };
}

function buildTaskFilter() {
  return (state.buildTasksFilter || '').trim().toLowerCase();
}

function groupBuildTasks(tasks) {
  const groups = {};
  for (const task of tasks) {
    const group = task.group || 'Tasks';
    if (!groups[group]) groups[group] = [];
    groups[group].push(task);
  }
  return groups;
}

function buildTaskMatchesText(text, query) {
  return !query || String(text || '').toLowerCase().includes(query);
}

function renderBuildTasksExplorer(rootNode, container, focusKey) {
  const filter = buildTaskFilter();
  const focusCtx = computeBuildTaskFocusContext(rootNode, focusKey);
  const html = renderBuildTaskModule(rootNode, 0, filter, focusCtx);
  container.innerHTML = html
    ? `<div class="ij-tree ij-build-task-tree" tabindex="0" role="tree" aria-label="Build tasks">${html}</div>`
    : `<div class="ij-build-tasks-empty">${filter ? 'No tasks match your filter.' : 'No tasks found.'}</div>`;
  bindBuildTaskActions(container);
  scrollBuildTaskModuleIntoView(container);
}

function renderBuildTaskModule(node, depth, filter, focusCtx) {
  if (!node) return '';
  const query = filter;
  const visibleTasks = (node.tasks || []).filter((task) =>
    buildTaskMatchesText(task.label, query)
    || buildTaskMatchesText(task.command, query),
  );
  const childParts = (node.children || [])
    .map((child) => renderBuildTaskModule(child, depth + 1, query, focusCtx))
    .filter(Boolean);
  const nodeMatches = buildTaskMatchesText(node.name, query) || buildTaskMatchesText(node.path, query);
  const hasVisible = visibleTasks.length > 0 || childParts.length > 0;
  if (query && !nodeMatches && !hasVisible) return '';

  const isFocused = !!focusCtx.focusNodePath && node.path === focusCtx.focusNodePath;
  const onFocusPath = isFocused || focusCtx.expandNodePaths.has(node.path);
  const open = query ? true : (depth === 0 || onFocusPath);
  const title = node.path ? `${node.name} (${node.path})` : node.name;
  const focusClass = isFocused ? ' ij-build-task-module-focus' : '';
  const groups = groupBuildTasks(visibleTasks);
  const tasksHtml = Object.entries(groups).map(([group, tasks]) => {
    const groupDepth = depth + 1;
    const groupLabel = group.replace(/-/g, ' ');
    return `
      <div class="ij-build-task-group" role="group" aria-label="${escapeHtml(groupLabel)}">
        <div class="ij-tree-row ij-build-task-group-header" style="--depth:${groupDepth}">
          <span class="ij-tree-icon ij-build-task-group-icon">${buildTaskGroupIcon()}</span>
          <span class="ij-tree-label">${escapeHtml(groupLabel)}</span>
        </div>
        <div class="ij-tree-children">
          ${tasks.map((task) => renderBuildTaskLeaf(node.path, task, groupDepth + 1)).join('')}
        </div>
      </div>`;
  }).join('');

  const childrenHtml = childParts.join('');
  const bodyHtml = `${tasksHtml}${childrenHtml}`;
  if (!bodyHtml && !nodeMatches) {
    return `
      <details class="ij-tree-dir ij-build-task-module${focusClass}" data-module-path="${escapeHtml(node.path)}" style="--depth:${depth}" open>
        <summary class="ij-tree-row ij-tree-dir-row" style="--depth:${depth}" title="${escapeHtml(title)}">
          <span class="ij-tree-chevron" aria-hidden="true"></span>
          <span class="ij-tree-icon ij-tree-icon-folder">${treeIconSvg('folder')}${treeIconSvg('folderOpen')}</span>
          <span class="ij-tree-label">${escapeHtml(node.name)}</span>
        </summary>
        <div class="ij-tree-children"><div class="ij-build-tasks-empty ij-build-tasks-empty-inline">No tasks</div></div>
      </details>`;
  }

  return `
    <details class="ij-tree-dir ij-build-task-module${focusClass}" data-module-path="${escapeHtml(node.path)}" ${open ? 'open' : ''}>
      <summary class="ij-tree-row ij-tree-dir-row" style="--depth:${depth}" title="${escapeHtml(title)}" aria-expanded="${open ? 'true' : 'false'}">
        <span class="ij-tree-chevron" aria-hidden="true"></span>
        <span class="ij-tree-icon ij-tree-icon-folder">${treeIconSvg('folder')}${treeIconSvg('folderOpen')}</span>
        <span class="ij-tree-label">${escapeHtml(node.name)}</span>
      </summary>
      <div class="ij-tree-children">${bodyHtml || '<div class="ij-build-tasks-empty ij-build-tasks-empty-inline">No tasks</div>'}</div>
    </details>`;
}

function buildTaskDisplayLabel(task, buildTool) {
  if (buildTool === 'docker' && task?.label) return task.label;
  return task?.command || '';
}

function findBuildTask(modulePath, taskCommand) {
  const tree = state.buildTasksTree?.tree;
  if (!tree) return null;
  const walk = (node) => {
    if (node.path === modulePath) {
      return (node.tasks || []).find((t) => t.command === taskCommand) || null;
    }
    for (const child of node.children || []) {
      const found = walk(child);
      if (found) return found;
    }
    return null;
  };
  return walk(tree);
}

function renderBuildTaskLeaf(modulePath, task, depth) {
  const selected = state.buildTasksSelected?.modulePath === modulePath
    && state.buildTasksSelected?.task === task.command;
  const active = selected ? ' active' : '';
  const displayLabel = buildTaskDisplayLabel(task, state.buildTasksTree?.build_tool);
  const runTitle = displayLabel === task.command
    ? `Run ${task.command} in ${modulePath}`
    : `${displayLabel} — ${task.command}`;
  const liveBadge = task.group === 'logs' && task.command.includes(' logs -f')
    ? '<span class="ij-build-task-live">live</span>'
    : '';
  return `
    <button type="button"
      class="ij-tree-row ij-tree-file-row ij-build-task-leaf${active}"
      style="--depth:${depth}"
      data-module-path="${escapeHtml(modulePath)}"
      data-task="${escapeHtml(task.command)}"
      data-label="${escapeHtml(displayLabel)}"
      role="treeitem"
      title="${escapeHtml(runTitle)}">
      <span class="ij-tree-icon ij-build-task-run-icon">${buildTaskRunIcon()}</span>
      <span class="ij-tree-label">${escapeHtml(displayLabel)}</span>${liveBadge}
    </button>`;
}

function rerenderBuildTasksExplorer() {
  const treeEl = $('#build-tasks-tree');
  if (!treeEl || !state.buildTasksTree?.tree) return;
  const focusKey = state.buildTasksFocusKey ?? buildTaskModuleKeyFromPath(state.activeTab);
  renderBuildTasksExplorer(state.buildTasksTree.tree, treeEl, focusKey);
}

function scrollBuildTaskModuleIntoView(container) {
  requestAnimationFrame(() => {
    const row = container.querySelector('.ij-build-task-module-focus > summary');
    row?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  });
}

function selectBuildTask(modulePath, task, { updateDom = true } = {}) {
  state.buildTasksSelected = modulePath && task ? { modulePath, task } : null;
  if (!updateDom) return;
  const tree = $('#build-tasks-tree')?.querySelector('.ij-build-task-tree');
  if (!tree) return;
  tree.querySelectorAll('.ij-build-task-leaf').forEach((btn) => {
    const active = btn.dataset.modulePath === modulePath && btn.dataset.task === task;
    btn.classList.toggle('active', active);
  });
}

function bindBuildTaskActions(root) {
  const tree = root.querySelector('.ij-build-task-tree');
  tree?.querySelectorAll('.ij-build-task-leaf').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      selectBuildTask(btn.dataset.modulePath, btn.dataset.task);
      void runBuildTask(btn.dataset.modulePath, btn.dataset.task, btn.dataset.label);
    });
    btn.addEventListener('dblclick', (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
  });
  tree?.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' || !state.buildTasksSelected) return;
    e.preventDefault();
    const { modulePath, task } = state.buildTasksSelected;
    const match = findBuildTask(modulePath, task);
    void runBuildTask(modulePath, task, match?.label);
  });
}

async function runBuildTask(modulePath, taskCommand, taskLabel) {
  if (!state.repo || !modulePath || !taskCommand) return;
  const buildTool = state.buildTasksTree?.build_tool || '';
  if (isDockerLogsBuildTask(buildTool, taskCommand)) {
    await runDockerLogsTask(modulePath, taskCommand, taskLabel);
    return;
  }
  if (state.dirty.has(state.activeTab)) await saveFile({ silent: true, skipProjectReload: true });
  const term = getActiveTerminal();
  if (!term) {
    toast('Terminal not ready — try again', 'error');
    return;
  }
  const matched = findBuildTask(modulePath, taskCommand);
  const shortLabel = taskLabel || matched?.label;
  let label;
  if (buildTool === 'docker') {
    label = shortLabel || taskCommand;
  } else if (buildTaskUsesShellRunner(buildTool)) {
    label = taskCommand;
  } else {
    label = `${buildTool === 'maven' ? 'mvn' : 'gradle'} ${taskCommand}`;
  }
  try {
    let exitCode = 0;
    if (buildTaskUsesShellRunner(buildTool)) {
      ({ exitCode } = await runWorkspaceCommandStream(
        '/workspace/shell',
        { command: taskCommand, cwd: buildTaskWorkdir(modulePath) },
        { label: buildTool === 'docker' ? (shortLabel || taskCommand) : label, terminalId: term.id },
      ));
    } else {
      ({ exitCode } = await runWorkspaceCommandStream(
        '/workspace/run/task',
        { path: modulePath, task: taskCommand },
        { label, terminalId: term.id, kind: 'gradle' },
      ));
    }
    await maybeRefreshDbSchemaAfterShell(taskCommand, exitCode);
  } catch (e) {
    toast(e.message || 'Task failed to start', 'error');
    terminalLog(`error: ${e.message}`);
  }
}

function isProjectSourceFile(path) {
  if (!path || path.startsWith('.reaper/')) return false;
  return path.replace(/\\/g, '/').endsWith('.java');
}

/** Java sources or Maven/Gradle files that change the classpath when edited. */
function isProjectClasspathFile(path) {
  return isProjectSourceFile(path) || isProjectBuildFile(path);
}

function hasAutoReloadProject() {
  return hasJavaBuildToolProject() || Boolean(state.runInfo?.has_project);
}

function projectStatusNeedsAutoRefresh(status) {
  if (!status) return false;
  const indexers = status.profile?.indexers || [];
  if (!indexers.includes('java')) return false;
  const java = status.java || {};
  if (java.state === 'running') return false;
  if (status.needs_refresh) return true;
  const frameworks = status.profile?.frameworks || [];
  const isSpring = frameworks.includes('spring-boot') || frameworks.includes('spring-test');
  if (isSpring && java.state === 'ready' && (java.dependency_jars || 0) === 0) return true;
  if (java.state === 'idle' && (java.symbol_count || 0) === 0 && indexers.includes('java')) return true;
  return false;
}

function maybeAutoRefreshProjectIndex(status) {
  if (!state.repo || state.projectIndexRunning) return;
  if (state.projectAutoRefreshAttempts >= PROJECT_AUTO_REFRESH_MAX) return;
  if (!projectStatusNeedsAutoRefresh(status)) return;
  state.projectAutoRefreshAttempts += 1;
  state.projectReloadBackground = true;
  reloadProjectIndex({ silent: true });
}

function scheduleProjectReload(delayMs) {
  if (!state.repo || !hasAutoReloadProject()) return;
  if (!state.activeTab || !isProjectClasspathFile(state.activeTab)) return;
  const delay = delayMs ?? (
    isProjectBuildFile(state.activeTab) ? PROJECT_BUILD_RELOAD_DELAY_MS : PROJECT_RELOAD_DELAY_MS
  );
  clearTimeout(state.projectReloadTimer);
  state.projectReloadTimer = setTimeout(() => runBackgroundProjectReload(), delay);
}

async function runBackgroundProjectReload() {
  state.projectReloadTimer = null;
  if (!state.repo || !hasAutoReloadProject()) return;
  if (state.projectIndexRunning) {
    state.projectReloadPending = true;
    return;
  }
  const path = state.activeTab;
  if (!path || !isProjectClasspathFile(path)) return;
  try {
    if (state.dirty.has(path)) {
      await saveFile({ silent: true, skipProjectReload: true });
    }
    state.projectReloadBackground = true;
    await reloadProjectIndex({ silent: true });
  } catch {
    state.projectReloadBackground = false;
  }
}

function hasJavaBuildToolProject() {
  const frameworks = state.projectProfile?.frameworks || [];
  return frameworks.includes('maven') || frameworks.includes('gradle');
}

function projectBuildToolName() {
  const frameworks = state.projectProfile?.frameworks || [];
  if (frameworks.includes('maven')) return 'Maven';
  if (frameworks.includes('gradle')) return 'Gradle';
  return 'Maven/Gradle';
}

function updateProjectReloadButton() {
  const show = Boolean(state.repo && hasJavaBuildToolProject());
  const tool = projectBuildToolName();
  const title = state.projectIndexRunning
    ? `Re-indexing ${tool} project…`
    : `Reload ${tool} project`;
  const disabled = !show || state.projectIndexRunning;
  for (const sel of ['#tb-reload-project', '#btn-reload-project']) {
    const btn = $(sel);
    if (!btn) continue;
    btn.classList.toggle('hidden', !show);
    btn.disabled = disabled;
    if (show) {
      btn.title = title;
      btn.setAttribute('aria-label', title);
    }
  }
  setMenuDisabled('reload-project', disabled);
}

async function reloadProjectIndex(options = {}) {
  const { silent = false } = options;
  if (!state.repo) return;
  try {
    if (!silent) toast('Reloading Maven/Gradle project…', 'info');
    state.projectIndexNotified = false;
    state.projectIndexStartedAt = Date.now();
    state.projectIndexRunning = true;
    updateProjectReloadButton();
    await api(repoApi(state.repo, '/workspace/project/reload'), { method: 'POST' });
    startProjectIndexPolling();
  } catch (err) {
    state.projectIndexRunning = false;
    updateProjectReloadButton();
    if (!silent) {
      toast(err.message || 'Failed to reload project', 'error');
      terminalLogError(err.message || 'Failed to reload project');
    }
  }
}

async function pollProjectIndexStatus() {
  if (!state.repo) return;
  try {
    const status = await api(repoApi(state.repo, '/workspace/project/index-status'));
    updateProjectIndexUi(status);
    const elapsed = Date.now() - (state.projectIndexStartedAt || Date.now());
    const timedOut = projectIndexNeedsFreeze(status) && elapsed > 5 * 60 * 1000;
    if (timedOut && !state.projectIndexNotified) {
      state.projectIndexNotified = true;
      clearIndexingProgressUi();
      toast('Indexing is taking longer than expected — you can keep working; check Settings → Java if Gradle is stuck', 'warning', { duration: 12000 });
      stopProjectIndexPolling();
      return;
    }
    const backgroundActive = isBackgroundJarIndexPhase(status?.java?.phase, status?.java)
      || isBackgroundToolingPhase(status?.java?.phase);
    if (backgroundActive) {
      ensureProjectIndexPollInterval(PROJECT_INDEX_POLL_BACKGROUND_MS);
    } else if (projectIndexNeedsFreeze(status)) {
      ensureProjectIndexPollInterval(PROJECT_INDEX_POLL_MS);
    } else {
      ensureProjectIndexPollInterval(PROJECT_INDEX_POLL_BACKGROUND_MS);
    }
    const javaStillRunning = status?.profile?.indexers?.includes('java') && status?.java?.state === 'running';
    const onDemandReady = status?.java?.phase === 'on-demand'
      && status?.java?.state !== 'running'
      && !backgroundActive;
    if ((status?.state === 'ready' || status?.state === 'error' || status?.state === 'idle') && !javaStillRunning && (onDemandReady || !backgroundActive)) {
      stopProjectIndexPolling();
    }
  } catch {
    /* ignore transient poll errors */
  }
}

function welcomeShowcaseHtml() {
  if (!shouldShowWelcomeShowcase()) return '';
  const shots = [
    { file: 'welcome-home', label: 'Home', alt: 'Reaper welcome screen with quick actions' },
    { file: 'editor-java', label: 'Editor', alt: 'Monaco editor with Java syntax highlighting' },
    { file: 'git-commit', label: 'Commit', alt: 'Git commit panel with staged changes' },
    { file: 'git-history', label: 'Git log', alt: 'Git history and commit log' },
    { file: 'terminal', label: 'Terminal', alt: 'Integrated terminal for Gradle and shell output' },
    { file: 'agent', label: 'Agent', alt: 'Cursor agent chat for AI-assisted editing' },
    { file: 'build-tasks', label: 'Build', alt: 'Gradle and Maven build tasks tree' },
    { file: 'search', label: 'Search', alt: 'Search classes, files, and text across the project' },
    { file: 'go-to-class', label: 'Navigate', alt: 'Go to Class for Java symbol navigation' },
  ];
  const slides = shots.map(({ file, label, alt }, i) => `
      <figure class="ij-welcome-slide${i === 0 ? ' is-active' : ''}" data-slide="${i}">
        <img src="/screenshots/${file}.png" alt="${escapeHtml(alt)}" loading="${i === 0 ? 'eager' : 'lazy'}" decoding="async" data-shot="${file}" />
        <figcaption>${escapeHtml(label)}</figcaption>
      </figure>`).join('');
  const dots = shots.map(({ label }, i) => `
      <button type="button" class="ij-welcome-carousel-dot${i === 0 ? ' is-active' : ''}" data-carousel-dot="${i}" role="tab" aria-label="${escapeHtml(label)}" aria-selected="${i === 0 ? 'true' : 'false'}"></button>`).join('');
  return `<aside class="ij-welcome-showcase" aria-label="Reaper in action">
    <div class="ij-welcome-carousel" data-welcome-carousel>
      <div class="ij-welcome-carousel-viewport">
        <div class="ij-welcome-carousel-track">${slides}</div>
      </div>
      <div class="ij-welcome-carousel-controls">
        <button type="button" class="ij-welcome-carousel-btn" data-carousel-prev aria-label="Previous screenshot">‹</button>
        <div class="ij-welcome-carousel-dots" role="tablist">${dots}</div>
        <button type="button" class="ij-welcome-carousel-btn" data-carousel-next aria-label="Next screenshot">›</button>
      </div>
    </div>
  </aside>`;
}

function bindWelcomeCarousel(root = document) {
  const carousel = root.querySelector('[data-welcome-carousel]');
  if (!carousel) return;

  const track = carousel.querySelector('.ij-welcome-carousel-track');
  let slides = [].slice.call(carousel.querySelectorAll('.ij-welcome-slide'));
  const dots = [].slice.call(carousel.querySelectorAll('[data-carousel-dot]'));
  if (!track || !slides.length) return;

  function visibleSlides() {
    return slides.filter((slide) => !slide.classList.contains('is-missing'));
  }

  slides.forEach((slide) => {
    const img = slide.querySelector('img');
    if (!img) return;
    img.addEventListener('error', () => {
      slide.classList.add('is-missing');
      slide.remove();
      const dot = dots.find((d) => Number(d.dataset.carouselDot) === Number(slide.dataset.slide));
      dot?.remove();
      slides = visibleSlides();
      if (!slides.length) {
        carousel.closest('.ij-welcome-showcase')?.remove();
        return;
      }
      if (index >= slides.length) index = 0;
      syncTrack();
    }, { once: true });
  });

  let index = 0;
  let timer = null;

  function syncTrack() {
    const visible = visibleSlides();
    if (!visible.length) return;
    const active = visible[index] || visible[0];
    if (!active) return;
    index = visible.indexOf(active);
    track.style.transform = `translate3d(${-index * 100}%, 0, 0)`;
    visible.forEach((slide) => slide.classList.toggle('is-active', slide === active));
    dots.forEach((dot) => {
      const on = Number(active.dataset.slide) === Number(dot.dataset.carouselDot);
      dot.classList.toggle('is-active', on);
      dot.setAttribute('aria-selected', on ? 'true' : 'false');
    });
  }

  function goTo(nextIndex) {
    const visible = visibleSlides();
    if (!visible.length) return;
    index = ((nextIndex % visible.length) + visible.length) % visible.length;
    syncTrack();
  }

  function stopAuto() {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
  }

  function startAuto() {
    stopAuto();
    if (visibleSlides().length < 2) return;
    timer = setInterval(() => goTo(index + 1), 5500);
  }

  carousel.querySelector('[data-carousel-prev]')?.addEventListener('click', () => {
    goTo(index - 1);
    startAuto();
  });
  carousel.querySelector('[data-carousel-next]')?.addEventListener('click', () => {
    goTo(index + 1);
    startAuto();
  });
  dots.forEach((dot) => {
    dot.addEventListener('click', () => {
      const visible = visibleSlides();
      const target = visible.findIndex((s) => Number(s.dataset.slide) === Number(dot.dataset.carouselDot));
      if (target >= 0) goTo(target);
      startAuto();
    });
  });
  carousel.addEventListener('mouseenter', stopAuto);
  carousel.addEventListener('mouseleave', startAuto);

  syncTrack();
  startAuto();
}

function welcomeScreenHtml() {
  const last = state.lastRepo;
  const ordered = [...state.repos].sort((a, b) => {
    if (last && a.name === last) return -1;
    if (last && b.name === last) return 1;
    return String(a.name).localeCompare(String(b.name));
  });
  const recent = ordered.slice(0, 5);
  const recentHtml = recent.length
    ? `<div class="ij-recent">
        <div class="ij-recent-title">Recent repositories</div>
        <div class="ij-recent-list">
          ${recent.map((r) => `<button type="button" class="ij-recent-item" data-recent="${r.name}">${r.name}</button>`).join('')}
        </div>
      </div>`
    : '';
  const icons = window.ReaperIcons || {};
  return `<div class="ij-welcome-layout">
    <div class="ij-welcome-main">
      ${(window.ReaperLogo && window.ReaperLogo.reaperLogoHtml('welcome', { extraClass: 'ij-welcome-logo logo-mark' })) || ''}
      <h2>Welcome to Reaper</h2>
      <p class="ij-welcome-tagline">A local git host and developer studio — edit with syntax highlighting, commit, run Gradle & Java, and sync with GitHub.</p>
      <div class="ij-welcome-actions">
        <button type="button" class="ij-action-card" data-welcome="new">
          <span class="ij-action-icon ij-action-icon--new">${icons.newRepo || ''}</span>
          <strong>New repository</strong>
          <span>Create a repo hosted on this machine</span>
        </button>
        <button type="button" class="ij-action-card" data-welcome="import-repo">
          <span class="ij-action-icon ij-action-icon--clone">${icons.clone || ''}</span>
          <strong>Import repository</strong>
          <span>From a remote URL or a local git folder on this Mac</span>
        </button>
        <button type="button" class="ij-action-card" data-welcome="agent">
          <span class="ij-action-icon ij-action-icon--agent">${icons.agent || ''}</span>
          <strong>Open Agent</strong>
          <span>Chat with Cursor to edit your code</span>
        </button>
      </div>
      ${recentHtml}
      <dl class="ij-shortcuts">
        <div class="ij-shortcut"><dt>⌘⇧N</dt><dd>New repository</dd></div>
        <div class="ij-shortcut"><dt>File</dt><dd>Import repository…</dd></div>
        <div class="ij-shortcut"><dt>⌘P</dt><dd>Search class, file, or text</dd></div>
        <div class="ij-shortcut"><dt>⌘O</dt><dd>Go to Class</dd></div>
        <div class="ij-shortcut"><dt>⌘G</dt><dd>Go to Line</dd></div>
        <div class="ij-shortcut"><dt>⌘K</dt><dd>Command palette</dd></div>
        <div class="ij-shortcut"><dt>⌘S</dt><dd>Save file</dd></div>
        <div class="ij-shortcut"><dt>F5</dt><dd>Run / Gradle</dd></div>
        <div class="ij-shortcut"><dt>⌘N</dt><dd>New file</dd></div>
        <div class="ij-shortcut"><dt>⌘W</dt><dd>Close tab</dd></div>
        <div class="ij-shortcut"><dt>Alt+1</dt><dd>Project tool window</dd></div>
        <div class="ij-shortcut"><dt>Alt+7</dt><dd>Structure tool window</dd></div>
      </dl>
    </div>
    ${welcomeShowcaseHtml()}
  </div>`;
}

function bindWelcomeActions(root = document) {
  root.querySelector('[data-welcome="new"]')?.addEventListener('click', showModal);
  root.querySelector('[data-welcome="import-repo"]')?.addEventListener('click', () => showCloneModal());
  root.querySelector('[data-welcome="agent"]')?.addEventListener('click', toggleAgent);
  root.querySelectorAll('.ij-recent-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      requestRepoSelection(btn.dataset.recent);
    });
  });
}

function renderWelcome() {
  const el = $('#empty-state');
  if (!el) return;
  el.className = 'ij-welcome';
  el.innerHTML = welcomeScreenHtml();
  bindWelcomeActions(el);
  bindWelcomeCarousel(el);
  syncWelcomeLayout();
}

function syncWelcomeLayout() {
  const empty = $('#empty-state');
  const welcomeVisible = empty && !empty.classList.contains('hidden');
  const editor = $('#editor-container');
  const toolbar = $('#editor-toolbar');
  if (welcomeVisible || !state.activeTab) {
    editor?.classList.add('hidden');
    toolbar?.classList.add('hidden');
    toolbar?.classList.remove('flex');
  } else {
    editor?.classList.remove('hidden');
  }
}

function updateStatusBar(status = null) {
  const branchEl = $('#status-branch');
  const changesEl = $('#status-changes');
  const branch = status?.branch || state.currentBranch || '';
  if (branchEl) {
    let label = branch ? `⎇ ${withoutMasterBranch(branch)}` : '';
    const ahead = status?.ahead || 0;
    const behind = status?.behind || 0;
    if (ahead > 0) label += ` ↑${ahead}`;
    if (behind > 0) label += ` ↓${behind}`;
    branchEl.textContent = label;
    const tracking = status?.tracking || '';
    branchEl.title = tracking
      ? `${tracking}${ahead || behind ? ` · ${ahead} ahead, ${behind} behind` : ''}`
      : '';
  }
  if (changesEl) {
    if (!status || status.clean) {
      changesEl.textContent = '';
      changesEl.classList.remove('warn');
    } else {
      const conflicts = status.conflict_count || 0;
      const n = status.files?.length || 0;
      if (conflicts) {
        changesEl.textContent = `${conflicts} conflict${conflicts === 1 ? '' : 's'}`;
      } else {
        changesEl.textContent = n ? `${n} change${n === 1 ? '' : 's'}` : '';
      }
      changesEl.classList.toggle('warn', n > 0);
    }
  }
}

function setMenuDisabled(action, disabled) {
  $$(`.ij-menu-item[data-action="${action}"]`).forEach((el) => {
    el.disabled = disabled;
  });
}

function updateMenuState() {
  const hasRepo = !!state.repo;
  const hasTab = !!state.activeTab;
  const dirty = !!(hasTab && state.dirty.has(state.activeTab) && !isExternalEditorPath(state.activeTab));
  const canRun = !($('#tb-run')?.disabled ?? true);
  setMenuDisabled('save', !dirty);
  setMenuDisabled('format', !hasTab);
  setMenuDisabled('close-tab', !hasTab);
  setMenuDisabled('pull', !hasRepo);
  setMenuDisabled('push', !hasRepo);
  setMenuDisabled('switch-branch', !hasRepo);
  setMenuDisabled('publish', !hasRepo);
  setMenuDisabled('repo-info', !hasRepo);
  setMenuDisabled('new-file', !hasRepo);
  setMenuDisabled('run', !canRun);
  const navPush = $('#btn-nav-push');
  if (navPush) navPush.disabled = !hasRepo;
}

function updateGitNavUi(status = {}) {
  const ahead = status.ahead || 0;
  const behind = status.behind || 0;
  const tracking = status.tracking || '';
  const navPush = $('#btn-nav-push');
  if (navPush) {
    navPush.classList.toggle('ij-header-btn-pending', ahead > 0);
    navPush.title = ahead > 0
      ? `Push ${ahead} commit${ahead === 1 ? '' : 's'}${tracking ? ` (${tracking})` : ''}`
      : 'Push to remote';
  }
  const navPull = $('#btn-sync');
  if (navPull) {
    navPull.classList.toggle('ij-header-btn-pending', behind > 0);
    navPull.title = behind > 0
      ? `Pull ${behind} commit${behind === 1 ? '' : 's'}${tracking ? ` from ${tracking}` : ''}`
      : 'Pull latest';
  }
}

function closeAllMenus() {
  $$('.ij-menu-root.open').forEach((m) => m.classList.remove('open'));
  hideTreeContextMenu();
}

function runMenuAction(action) {
  const map = {
    'new-repo': showModal,
    'import-repo': () => showCloneModal(),
    'new-file': showFileModal,
    save: saveFile,
    format: formatDocument,
    'close-tab': () => state.activeTab && closeTab(state.activeTab),
    'commit-panel': () => switchPanel('git'),
    pull: syncPull,
    push: pushRemote,
    publish: showPublishModal,
    'repo-info': showRepoInfoModal,
    'panel-explorer': () => switchPanel('explorer'),
    'panel-structure': () => switchPanel('structure'),
    'panel-git': () => switchPanel('git'),
    'panel-history': () => switchPanel('history'),
    'panel-terminal': () => showTerminal(),
    'terminal-new': () => newTerminal(),
    'panel-agent': () => { switchPanel('agent'); if (state.agentDock !== 'left') toggleAgent(); },
    'panel-coverage': () => showCoveragePanel(),
    'panel-db-viewer': () => showDbViewerPanel(),
    'panel-git-viewer': () => showGitViewerPanel(),
    'panel-docker-logs': () => showDockerLogsPanel(),
    'panel-build-tasks': () => showBuildTasksPanel(),
    'panel-package-manifest': () => showPackageManifestPanel(),
    'terminal-dock-left': () => setTerminalDock('left'),
    'terminal-dock-right': () => setTerminalDock('right'),
    'terminal-dock-bottom': () => setTerminalDock('bottom'),
    'agent-dock-left': () => setAgentDock('left'),
    'agent-dock-right': () => setAgentDock('right'),
    'agent-dock-bottom': () => setAgentDock('bottom'),
    'docker-logs-dock-left': () => setDockerLogsDock('left'),
    'docker-logs-dock-right': () => setDockerLogsDock('right'),
    'docker-logs-dock-bottom': () => setDockerLogsDock('bottom'),
    'build-tasks-dock-left': () => setBuildTasksDock('left'),
    'build-tasks-dock-right': () => setBuildTasksDock('right'),
    'package-manifest-dock-left': () => setPackageManifestDock('left'),
    'package-manifest-dock-right': () => setPackageManifestDock('right'),
    'toggle-sidebar': () => toggleSidebar(),
    'toggle-dotfiles': () => setShowDotfiles(!getShowDotfiles()),
    'command-palette': showPalette,
    'search-everywhere': () => showSearchEverywhere(),
    'goto-class': showGoToClass,
    'goto-line': showGoToLine,
    'switch-branch': showBranchPicker,
    'header-open-repo': () => showRepoPicker(),
    'header-search': () => showSearchEverywhere(),
    'header-palette': () => showPalette(),
    settings: () => showSettingsModal('git'),
    'settings-git': () => showSettingsModal('git'),
    'settings-cursor': () => showSettingsModal('cursor'),
    'settings-appearance': () => showSettingsModal('appearance'),
    'settings-compiler': () => showSettingsModal('compilers'),
    'settings-compilers': () => showSettingsModal('compilers'),
    'settings-toolchains': () => showSettingsModal('compilers'),
    'settings-java': () => showSettingsModal('compilers'),
    run: runActive,
    'reload-project': () => reloadProjectIndex(),
  };
  map[action]?.();
}

const PALETTE_COMMANDS = [
  { id: 'new-repo', label: 'New repository', kbd: '⌘⇧N', run: showModal },
  { id: 'import-repo', label: 'Import repository', run: () => showCloneModal() },
  { id: 'settings', label: 'Settings', kbd: '⌘,', run: () => showSettingsModal('git') },
  { id: 'settings-git', label: 'Git hosts (PAT)', run: () => showSettingsModal('git') },
  { id: 'settings-cursor', label: 'Cursor agent key', run: () => showSettingsModal('cursor') },
  { id: 'settings-appearance', label: 'Editor appearance', run: () => showSettingsModal('appearance') },
  { id: 'settings-compiler', label: 'Compiler', run: () => showSettingsModal('compilers') },
  { id: 'palette', label: 'Command palette', kbd: '⌘K', run: showPalette },
  { id: 'search', label: 'Search class, file, or text', kbd: '⌘P', run: () => showSearchEverywhere(), needsRepo: true },
  { id: 'goto-class', label: 'Go to Class', kbd: '⌘O', run: showGoToClass, needsRepo: true },
  { id: 'goto-line', label: 'Go to Line', kbd: '⌘G', run: showGoToLine, needsTab: true },
  { id: 'switch-branch', label: 'Switch branch…', kbd: '⌘⇧B', run: showBranchPicker, needsRepo: true },
  { id: 'reload-project', label: 'Reload Maven/Gradle project', run: () => reloadProjectIndex(), needsRepo: true, needsBuildTool: true },
  { id: 'new-file', label: 'New file', kbd: '⌘N', run: showFileModal, needsRepo: true },
  { id: 'save', label: 'Save', kbd: '⌘S', run: saveFile, needsTab: true, needsDirty: true },
  { id: 'format', label: 'Reformat code', kbd: '⇧⌥F', run: formatDocument, needsTab: true },
  { id: 'find-usages', label: 'Find Usages', kbd: 'Alt+F7', run: () => runEditorMonacoAction('reaper.findUsages'), needsTab: true },
  { id: 'rename-symbol', label: 'Rename Symbol', kbd: 'F6', run: () => runEditorMonacoAction('reaper.renameSymbol'), needsTab: true },
  { id: 'java-refactor', label: 'Refactor…', kbd: '⇧⌥R', run: () => runEditorMonacoAction('reaper.javaRefactor'), needsTab: true },
  { id: 'change-all', label: 'Change All Occurrences', kbd: '⌘⌃G', run: () => runEditorMonacoAction('reaper.changeAllOccurrences'), needsTab: true },
  { id: 'run', label: 'Run', kbd: 'F5', run: runActive, needsRun: true },
  { id: 'debug', label: 'Debug', kbd: 'F6', run: () => void startDebugSession(), needsRun: true },
  { id: 'commit', label: 'Commit…', run: () => switchPanel('git'), needsRepo: true },
  { id: 'pull', label: 'Pull', run: syncPull, needsRepo: true },
  { id: 'push', label: 'Push to remote', run: pushRemote, needsRepo: true },
  { id: 'publish', label: 'Publish to remote…', run: showPublishModal, needsRepo: true },
  { id: 'repo-info', label: 'Repository details', run: showRepoInfoModal, needsRepo: true },
  { id: 'explorer', label: 'Show Project', kbd: 'Alt+1', run: () => switchPanel('explorer') },
  { id: 'structure', label: 'Show Structure', kbd: 'Alt+7', run: () => switchPanel('structure'), needsRepo: true },
  { id: 'git-panel', label: 'Show Commit', kbd: 'Alt+9', run: () => switchPanel('git') },
  { id: 'terminal', label: 'Show Terminal', run: () => showTerminal() },
  { id: 'terminal-new', label: 'New Terminal', kbd: '⌘⇧`', run: () => newTerminal(), needsRepo: true },
  { id: 'coverage', label: 'Coverage report', run: () => showCoveragePanel(), needsRepo: true },
  { id: 'debug-panel', label: 'Debug panel', run: () => toggleDebugPanel(), needsRepo: true },
  { id: 'db-viewer', label: 'Database', run: () => showDbViewerPanel(), needsRepo: true },
  { id: 'git-viewer', label: 'Git Console', run: () => showGitViewerPanel(), needsRepo: true },
  { id: 'docker-logs', label: 'Docker', run: () => showDockerLogsPanel(), needsRepo: true },
  { id: 'build-tasks', label: 'Build tasks', run: () => showBuildTasksPanel(), needsRepo: true },
  { id: 'package-manifest', label: 'Package manifest', run: () => showPackageManifestPanel(), needsRepo: true },
  { id: 'sidebar', label: 'Toggle sidebar', run: () => toggleSidebar() },
];

let paletteIndex = 0;

function showPalette() {
  closeAllMenus();
  paletteIndex = 0;
  $('#palette-overlay')?.classList.add('open');
  const input = $('#palette-input');
  if (input) {
    input.value = '';
    renderPaletteResults('');
    setTimeout(() => input.focus(), 30);
  }
}

function hidePalette() {
  $('#palette-overlay')?.classList.remove('open');
}

function paletteMatches(cmd, query) {
  if (!query) return true;
  const q = query.toLowerCase();
  return cmd.label.toLowerCase().includes(q) || cmd.id.includes(q);
}

function paletteEnabled(cmd) {
  if (cmd.needsRepo && !state.repo) return false;
  if (cmd.needsBuildTool && !hasJavaBuildToolProject()) return false;
  if (cmd.needsTab && !state.activeTab) return false;
  if (cmd.needsDirty && !(state.activeTab && state.dirty.has(state.activeTab) && !isExternalEditorPath(state.activeTab))) return false;
  if (cmd.needsRun && ($('#tb-run')?.disabled ?? true)) return false;
  return true;
}

function renderPaletteResults(query) {
  const results = $('#palette-results');
  if (!results) return;
  const items = PALETTE_COMMANDS.filter((c) => paletteMatches(c, query));
  paletteIndex = Math.min(paletteIndex, Math.max(items.length - 1, 0));
  results.innerHTML = items.map((cmd, i) => {
    const disabled = !paletteEnabled(cmd);
    return `<button type="button" class="ij-palette-item${i === paletteIndex ? ' active' : ''}" data-palette-id="${cmd.id}" ${disabled ? 'disabled' : ''}>
      <span>${cmd.label}</span>
      ${cmd.kbd ? `<span class="ij-kbd">${cmd.kbd}</span>` : ''}
    </button>`;
  }).join('') || '<p class="px-4 py-3 text-xs text-gray-600">No matching commands</p>';
  results.querySelectorAll('.ij-palette-item:not([disabled])').forEach((btn) => {
    btn.addEventListener('click', () => {
      const cmd = PALETTE_COMMANDS.find((c) => c.id === btn.dataset.paletteId);
      if (cmd && paletteEnabled(cmd)) {
        hidePalette();
        cmd.run();
      }
    });
  });
}

function runPaletteSelection() {
  const query = $('#palette-input')?.value || '';
  const items = PALETTE_COMMANDS.filter((c) => paletteMatches(c, query));
  const cmd = items[paletteIndex];
  if (cmd && paletteEnabled(cmd)) {
    hidePalette();
    cmd.run();
  }
}

let gotoClassIndex = 0;
let gotoClassHits = [];
let gotoClassTimer = null;

function showGoToClass(initialQuery = '') {
  if (!state.repo) {
    toast('Open a repository first', 'info');
    return;
  }
  closeAllMenus();
  hidePalette();
  gotoClassIndex = 0;
  gotoClassHits = [];
  $('#goto-class-overlay')?.classList.add('open');
  const input = $('#goto-class-input');
  const query = String(initialQuery || '');
  if (input) {
    input.value = query;
    const results = $('#goto-class-results');
    if (results) results.innerHTML = '<p class="px-4 py-3 text-xs text-gray-500">Loading…</p>';
    scheduleGoToClassSearch(query);
    setTimeout(() => input.focus(), 30);
  }
}

function hideGoToClass() {
  $('#goto-class-overlay')?.classList.remove('open');
}

let javaReferencesHits = [];
let javaReferencesIndex = 0;
let javaReferencesLoading = false;

function hideJavaReferences() {
  $('#java-references-overlay')?.classList.remove('open');
  javaReferencesHits = [];
  javaReferencesIndex = 0;
  javaReferencesLoading = false;
}

function formatReferenceHit(hit) {
  const path = String(hit?.path || '');
  const file = path.split('/').pop() || path || 'file';
  const dir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
  const line = Math.max(1, Number(hit?.line) || 1);
  const column = Math.max(1, Number(hit?.column) || 1);
  return { file, pathLabel: dir || path, loc: `${line}:${column}` };
}

function renderJavaReferencesHits() {
  const el = $('#java-references-results');
  if (!el) return;
  el.scrollTop = 0;
  if (javaReferencesLoading) {
    el.innerHTML = '<div class="ij-palette-empty px-3 py-2 text-xs text-gray-500">Searching…</div>';
    return;
  }
  if (!javaReferencesHits.length) {
    el.innerHTML = '<div class="ij-palette-empty px-3 py-2 text-xs text-gray-500">No usages found</div>';
    return;
  }
  el.innerHTML = javaReferencesHits.map((hit, i) => {
    const active = i === javaReferencesIndex ? ' active' : '';
    const { file, pathLabel, loc } = formatReferenceHit(hit);
    return `<button type="button" class="ij-references-item ij-palette-item${active}" data-ref-idx="${i}">
      <span class="ij-references-main">
        <span class="ij-references-file">${escapeHtml(file)}</span>
        <span class="ij-references-path">${escapeHtml(pathLabel)}</span>
      </span>
      <span class="ij-references-loc">${escapeHtml(loc)}</span>
    </button>`;
  }).join('');
  if (javaReferencesIndex > 0) {
    el.querySelector('.active')?.scrollIntoView({ block: 'nearest' });
  }
}

async function openJavaReferenceHit(hit) {
  if (!hit?.path) return;
  hideJavaReferences();
  await openFileAt(hit.path, hit.line || 1, hit.column || 1);
}

function showJavaReferences(refs, title, { loading = false } = {}) {
  closeAllMenus();
  hidePalette();
  hideSearchEverywhere();
  hideGoToClass();
  javaReferencesLoading = loading;
  javaReferencesHits = Array.isArray(refs) ? refs : [];
  javaReferencesIndex = 0;
  const heading = $('#java-references-title');
  if (heading) heading.textContent = title || 'Find Usages';
  renderJavaReferencesHits();
  $('#java-references-overlay')?.classList.add('open');
}

function applyTextEditsToLines(lines, edits) {
  const sorted = [...edits].sort((a, b) => {
    if (a.start_line !== b.start_line) return b.start_line - a.start_line;
    return b.start_column - a.start_column;
  });
  for (const edit of sorted) {
    const startIdx = Math.max(0, edit.start_line - 1);
    const endIdx = Math.max(0, edit.end_line - 1);
    if (startIdx >= lines.length) continue;
    const startCol = Math.max(0, edit.start_column - 1);
    const endCol = Math.max(0, edit.end_column - 1);
    if (startIdx === endIdx) {
      const line = lines[startIdx] ?? '';
      lines[startIdx] = line.slice(0, startCol) + (edit.text ?? '') + line.slice(endCol);
    } else {
      const first = lines[startIdx] ?? '';
      const last = lines[endIdx] ?? '';
      const merged = first.slice(0, startCol) + (edit.text ?? '') + last.slice(endCol);
      lines.splice(startIdx, endIdx - startIdx + 1, merged);
    }
  }
  return lines.join('\n');
}

function resolveWorkspaceEditPath(editPath) {
  const norm = workspaceExplorerPath(editPath);
  if (!norm) return editPath;
  if (state.activeTab && workspaceExplorerPath(state.activeTab) === norm) return state.activeTab;
  const openTab = state.tabs.find((t) => workspaceExplorerPath(t) === norm);
  return openTab || norm;
}

async function applyJavaWorkspaceEdits(fileEdits) {
  if (!state.repo || !Array.isArray(fileEdits) || !fileEdits.length) return 0;
  let changed = 0;
  for (const batch of fileEdits) {
    const edits = batch.edits || [];
    if (!batch.path || !edits.length) continue;
    const tabPath = resolveWorkspaceEditPath(batch.path);
    const apiPath = workspaceExplorerPath(batch.path) || batch.path;
    if (tabPath === state.activeTab && state.editor) {
      const lines = state.editor.getValue().split('\n');
      const next = applyTextEditsToLines(lines, edits);
      state.suppressEditorChange = true;
      state.editor.setValue(next);
      state.suppressEditorChange = false;
      state.tabContents.set(tabPath, next);
      state.dirty.add(tabPath);
      updateSaveButton();
      renderTabs();
      await saveFile({ silent: true, skipProjectReload: true });
      changed += 1;
      continue;
    }
    let content = state.tabContents.get(tabPath);
    if (content == null) {
      try {
        const res = await api(repoApi(state.repo, `/workspace/file?${new URLSearchParams({ path: apiPath })}`));
        content = res?.content ?? '';
      } catch {
        continue;
      }
    }
    const next = applyTextEditsToLines(content.split('\n'), edits);
    try {
      await api(repoApi(state.repo, '/workspace/file'), {
        method: 'PUT',
        body: JSON.stringify({ path: apiPath, content: next }),
      });
      state.tabContents.set(tabPath, next);
      state.dirty.delete(tabPath);
      changed += 1;
    } catch { /* skip */ }
  }
  if (changed > 0) {
    scheduleAllJavaDiagnostics();
    if (hasAutoReloadProject()) scheduleProjectReload(400);
  }
  return changed;
}

function bindJavaReferences() {
  $('#java-references-results')?.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-ref-idx]');
    if (!btn) return;
    const idx = Number(btn.dataset.refIdx);
    if (Number.isFinite(idx) && javaReferencesHits[idx]) {
      void openJavaReferenceHit(javaReferencesHits[idx]);
    }
  });
  document.addEventListener('keydown', (e) => {
    if (!$('#java-references-overlay')?.classList.contains('open')) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      hideJavaReferences();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      javaReferencesIndex = Math.min(javaReferencesIndex + 1, Math.max(javaReferencesHits.length - 1, 0));
      renderJavaReferencesHits();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      javaReferencesIndex = Math.max(javaReferencesIndex - 1, 0);
      renderJavaReferencesHits();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      void openJavaReferenceHit(javaReferencesHits[javaReferencesIndex]);
    }
  });
  $('#java-references-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#java-references-overlay')) hideJavaReferences();
  });
}

function showGoToLine() {
  if (!state.activeTab) {
    toast('Open a file first', 'info');
    return;
  }
  if (!state.editor) {
    toast('Editor not ready', 'info');
    return;
  }
  closeAllMenus();
  hidePalette();
  hideGoToClass();
  hideSearchEverywhere();
  const overlay = $('#goto-line-overlay');
  const input = $('#goto-line-input');
  const subtitle = $('#goto-line-subtitle');
  const preview = $('#goto-line-preview');
  overlay?.classList.add('open');
  const model = state.editor.getModel();
  const file = state.activeTab.split('/').pop() || state.activeTab;
  const lineCount = model?.getLineCount() || 0;
  if (subtitle) {
    subtitle.textContent = lineCount ? `${file} · ${lineCount.toLocaleString()} lines` : file;
  }
  if (preview) {
    preview.textContent = '';
    preview.classList.remove('is-target');
  }
  const pos = state.editor.getPosition();
  const current = pos ? `${pos.lineNumber}${pos.column > 1 ? `:${pos.column}` : ''}` : '';
  if (input) {
    input.value = current;
    updateGoToLinePreview();
    setTimeout(() => {
      input.focus();
      input.select();
    }, 30);
  }
}

function updateGoToLinePreview() {
  const input = $('#goto-line-input');
  const preview = $('#goto-line-preview');
  const model = state.editor?.getModel();
  if (!input || !preview || !model) return;
  const parsed = parseGoToLineInput(input.value, model.getLineCount());
  if (!parsed) {
    preview.textContent = '';
    preview.classList.remove('is-target');
    return;
  }
  const text = model.getLineContent(parsed.line).trimEnd() || '(empty line)';
  preview.textContent = `${parsed.line}: ${text}`;
  preview.classList.add('is-target');
}

function hideGoToLine() {
  $('#goto-line-overlay')?.classList.remove('open');
}

let renamePromptResolver = null;

function renamePromptIsOpen() {
  const overlay = $('#rename-prompt-overlay');
  return overlay && !overlay.classList.contains('hidden');
}

function showRenamePrompt({ title = 'Rename', subtitle = '', value = '' } = {}) {
  return new Promise((resolve) => {
    if (renamePromptResolver) {
      renamePromptResolver(null);
      renamePromptResolver = null;
    }
    renamePromptResolver = resolve;
    closeAllMenus();
    hidePalette();
    const overlay = $('#rename-prompt-overlay');
    const input = $('#rename-prompt-input');
    const titleEl = $('#rename-prompt-title');
    const subtitleEl = $('#rename-prompt-subtitle');
    const submitBtn = $('#rename-prompt-submit');
    if (titleEl) titleEl.textContent = title;
    if (subtitleEl) {
      subtitleEl.textContent = subtitle;
      subtitleEl.classList.toggle('hidden', !subtitle);
    }
    if (submitBtn) {
      submitBtn.textContent = title.toLowerCase().includes('symbol') ? 'Rename Symbol' : 'Rename';
    }
    if (input) {
      input.value = value;
      overlay?.classList.remove('hidden');
      overlay?.classList.add('flex');
      setTimeout(() => {
        input.focus();
        input.select();
      }, 30);
    }
  });
}

function hideRenamePrompt(result = null) {
  const overlay = $('#rename-prompt-overlay');
  overlay?.classList.add('hidden');
  overlay?.classList.remove('flex');
  const resolve = renamePromptResolver;
  renamePromptResolver = null;
  if (resolve) resolve(result);
}

function submitRenamePrompt() {
  const input = $('#rename-prompt-input');
  const value = input?.value?.trim() ?? '';
  hideRenamePrompt(value || null);
}

function parseGoToLineInput(raw, maxLine) {
  const t = String(raw || '').trim();
  if (!t) return null;
  const m = t.match(/^(\d+)(?::(\d+))?$/);
  if (!m) return null;
  const line = Math.min(Math.max(1, parseInt(m[1], 10)), maxLine);
  const column = m[2] ? Math.max(1, parseInt(m[2], 10)) : 1;
  return { line, column };
}

function submitGoToLine() {
  const input = $('#goto-line-input');
  const model = state.editor?.getModel();
  if (!input || !model) return;
  const parsed = parseGoToLineInput(input.value, model.getLineCount());
  if (!parsed) {
    toast('Enter a line number (e.g. 42 or 42:10)', 'info');
    return;
  }
  hideGoToLine();
  state.editor.revealLineInCenter(parsed.line);
  state.editor.setPosition({ lineNumber: parsed.line, column: parsed.column });
  state.editor.focus();
}

function goToLine() {
  showGoToLine();
}

let branchPickerIndex = 0;
let branchPickerBranches = [];

function withoutMasterBranch(name) {
  return name === 'master' ? '' : (name || '');
}

function normalizeBranchList(branches) {
  return (branches || []).filter((b) => b !== 'master');
}

function resolveDefaultBranch(defaultBranch, branches) {
  const def = withoutMasterBranch(defaultBranch);
  if (def && branches.includes(def)) return def;
  if (branches.includes('main')) return 'main';
  return branches[0] || '';
}

function setBranchLabel(branch) {
  const normalized = withoutMasterBranch(branch);
  state.currentBranch = normalized || '';
  const el = $('#branch-picker-label');
  if (el) el.textContent = normalized || 'branch';
  updateDefaultBranchUi();
  updateHeaderBrand();
}

function updateDefaultBranchUi() {
  const el = $('#repo-default-branch');
  if (!el) return;
  const def = withoutMasterBranch(state.defaultBranch);
  if (!state.repo || !def || def === state.currentBranch) {
    el.textContent = '';
    el.classList.add('hidden');
    el.title = '';
    return;
  }
  el.textContent = `default ${def}`;
  el.title = `Repository default branch: ${def}`;
  el.classList.remove('hidden');
}

function filterBranchList(query) {
  const q = query.trim();
  if (!q) return [...state.branches].sort((a, b) => a.localeCompare(b));
  return state.branches
    .map((branch) => ({ branch, score: scoreBranchMatch(branch, q) }))
    .filter((x) => x.score >= 0)
    .sort((a, b) => b.score - a.score || a.branch.localeCompare(b.branch))
    .map((x) => x.branch);
}

function scoreBranchMatch(branch, query) {
  const b = branch.toLowerCase();
  const q = query.trim().toLowerCase();
  if (!q) return 0;
  if (b === q) return 1000;
  if (b.startsWith(q)) return 900 - b.length * 0.01;
  const segment = b.split('/').find((part) => part.startsWith(q));
  if (segment) return 700 - b.indexOf(segment);
  const idx = b.indexOf(q);
  if (idx >= 0) return 500 - idx;
  let bi = 0;
  let spread = 0;
  for (const ch of q) {
    const fi = b.indexOf(ch, bi);
    if (fi < 0) return -1;
    spread += fi;
    bi = fi + 1;
  }
  return 300 - spread;
}

function highlightBranchName(name, query) {
  const q = query.trim();
  if (!q) return escapeHtml(name);
  const nl = name.toLowerCase();
  const ql = q.toLowerCase();
  let idx = nl.indexOf(ql);
  if (idx < 0) {
    idx = nl.split('/').reduce((found, part, i, parts) => {
      if (found >= 0) return found;
      const off = parts.slice(0, i).join('/').length + (i > 0 ? 1 : 0);
      const local = part.toLowerCase().indexOf(ql);
      return local >= 0 ? off + local : -1;
    }, -1);
  }
  if (idx < 0) return escapeHtml(name);
  const before = escapeHtml(name.slice(0, idx));
  const match = escapeHtml(name.slice(idx, idx + q.length));
  const after = escapeHtml(name.slice(idx + q.length));
  return `${before}<mark class="ij-branch-match">${match}</mark>${after}`;
}

function branchAutocompleteTarget(query, branches = branchPickerBranches) {
  const q = query;
  if (!q.trim() || !branches.length) return null;
  const pick = branches[branchPickerIndex] ?? branches[0];
  if (!pick) return null;
  const ql = q.toLowerCase();
  const pl = pick.toLowerCase();
  if (pl.startsWith(ql)) return pick;
  return null;
}

function updateBranchAutocompleteGhost(query) {
  const ghost = $('#branch-picker-ghost');
  const input = $('#branch-picker-input');
  if (!ghost || !input) return;
  const target = branchAutocompleteTarget(query);
  if (!target || !query.trim()) {
    ghost.innerHTML = '';
    input.removeAttribute('aria-activedescendant');
    return;
  }
  const ql = query.toLowerCase();
  if (!target.toLowerCase().startsWith(ql)) {
    ghost.innerHTML = '';
    return;
  }
  ghost.innerHTML = `<span class="ij-branch-ghost-typed">${escapeHtml(query)}</span><span class="ij-branch-ghost-hint">${escapeHtml(target.slice(query.length))}</span>`;
  const active = $(`#branch-picker-option-${branchPickerIndex}`);
  input.setAttribute('aria-activedescendant', active?.id || '');
}

function acceptBranchAutocomplete() {
  const input = $('#branch-picker-input');
  if (!input) return false;
  const target = branchAutocompleteTarget(input.value);
  if (!target || target.toLowerCase() === input.value.trim().toLowerCase()) return false;
  input.value = target;
  branchPickerIndex = 0;
  renderBranchPickerResults(target);
  input.setSelectionRange(target.length, target.length);
  return true;
}

function showBranchPicker() {
  if (!state.repo) {
    toast('Open a repository first', 'info');
    return;
  }
  if (!state.branches.length) {
    toast('No branches in this repository', 'info');
    return;
  }
  closeAllMenus();
  hidePalette();
  branchPickerIndex = 0;
  $('#branch-picker-overlay')?.classList.add('open');
  const input = $('#branch-picker-input');
  if (input) {
    input.value = '';
    renderBranchPickerResults('');
    setTimeout(() => input.focus(), 30);
  }
}

function hideBranchPicker() {
  $('#branch-picker-overlay')?.classList.remove('open');
  const ghost = $('#branch-picker-ghost');
  if (ghost) ghost.innerHTML = '';
}

function setRepoPickerLabel(name) {
  const el = $('#repo-picker-label');
  if (el) el.textContent = name || 'Select repo…';
}

function showRepoPicker() {
  closeAllMenus();
  hidePalette();
  hideBranchPicker();
  state.repoPickerUnregisterName = null;
  $('#repo-picker-overlay')?.classList.add('open');
  renderRepoPickerResults();
}

function hideRepoPicker() {
  if (state.repoPickerUnregisterBusy) return;
  state.repoPickerUnregisterName = null;
  $('#repo-picker-overlay')?.classList.remove('open');
}

function renderRepoPickerResults() {
  const results = $('#repo-picker-results');
  if (!results) return;
  const hidden = state.hiddenRepos || [];
  if (!state.repos.length && !hidden.length) {
    results.innerHTML = '<p class="ij-repo-picker-empty">No repositories yet — import or create one from the welcome screen.</p>';
    return;
  }
  const current = state.repo;
  const pending = state.repoPickerUnregisterName;
  const activeRows = state.repos.map((r) => {
    const label = r.name;
    const isCurrent = r.name === current;
    if (pending === r.name) {
      if (state.repoPickerUnregisterBusy) {
        return `
      <div class="ij-repo-picker-row ij-repo-picker-row--confirm ij-repo-picker-row--busy" role="option" aria-busy="true">
        <span class="ij-repo-picker-confirm-status">
          <span class="ij-clone-spinner" aria-hidden="true"></span>
          <span>Removing <strong>${escapeHtml(r.name)}</strong>…</span>
        </span>
      </div>`;
      }
      return `
      <div class="ij-repo-picker-row ij-repo-picker-row--confirm" role="option">
        <span class="ij-repo-picker-confirm-text">Remove <strong>${escapeHtml(r.name)}</strong> from Reaper?</span>
        <div class="ij-repo-picker-confirm-actions">
          <button type="button" class="ij-repo-picker-confirm-btn" data-repo-unregister-confirm="${escapeHtml(r.name)}">Remove</button>
          <button type="button" class="ij-repo-picker-confirm-btn ghost" data-repo-unregister-cancel>Cancel</button>
        </div>
      </div>`;
    }
    return `
    <div class="ij-repo-picker-row${isCurrent ? ' current' : ''}" role="option" aria-selected="${isCurrent ? 'true' : 'false'}">
      <button type="button" class="ij-repo-picker-item ij-palette-item" data-repo-open="${escapeHtml(r.name)}">
        <span class="ij-repo-picker-name">${escapeHtml(label)}</span>
        ${isCurrent ? '<span class="ij-repo-picker-tag">open</span>' : ''}
      </button>
      <button type="button" class="ij-repo-picker-remove" data-repo-remove="${escapeHtml(r.name)}" title="Remove from Reaper" aria-label="Remove ${escapeHtml(r.name)} from Reaper">×</button>
    </div>`;
  }).join('');

  const hiddenSection = hidden.length
    ? `<div class="ij-repo-picker-section">
        <div class="ij-repo-picker-section-title">Removed from Reaper</div>
        ${hidden.map((r) => `
        <div class="ij-repo-picker-row ij-repo-picker-row--hidden" role="option">
          <span class="ij-repo-picker-name ij-repo-picker-name--muted">${escapeHtml(r.name)}</span>
          <button type="button" class="ij-repo-picker-restore" data-repo-restore="${escapeHtml(r.name)}">Add back</button>
        </div>`).join('')}
      </div>`
    : '';

  results.innerHTML = activeRows + hiddenSection;
}

async function restoreRepo(name) {
  if (!name) return;
  try {
    await api(repoApi(name, '/restore'), { method: 'POST' });
    toast(`Added ${name} back to Reaper`, 'success');
    hideRepoPicker();
    await loadRepos();
    await selectRepo(name);
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function unregisterRepo(name) {
  const repoName = name || state.repo;
  if (!repoName || state.repoPickerUnregisterBusy) return;
  state.repoPickerUnregisterBusy = true;
  if ($('#repo-picker-overlay')?.classList.contains('open')) renderRepoPickerResults();
  try {
    await api(repoApi(repoName, '/unregister'), { method: 'POST' });
    toast(`Removed ${repoName} from Reaper`, 'success');
    hideRepoInfoModal();
    state.repoPickerUnregisterName = null;
    if (state.repo === repoName) {
      state.repo = null;
      resetUI();
      updateWindowTitle();
    }
    state.repoPickerUnregisterBusy = false;
    hideRepoPicker();
    await loadRepos();
  } catch (err) {
    toast(err.message, 'error');
    if ($('#repo-picker-overlay')?.classList.contains('open')) renderRepoPickerResults();
  } finally {
    state.repoPickerUnregisterBusy = false;
  }
}

function renderBranchPickerResults(query) {
  const results = $('#branch-picker-results');
  if (!results) return;
  branchPickerBranches = filterBranchList(query);
  branchPickerIndex = Math.min(branchPickerIndex, Math.max(branchPickerBranches.length - 1, 0));
  if (!branchPickerBranches.length) {
    results.innerHTML = '<p class="ij-branch-picker-empty">No matching branches</p>';
    updateBranchAutocompleteGhost(query);
    return;
  }
  const current = state.currentBranch;
  const defaultBranch = state.defaultBranch;
  results.innerHTML = branchPickerBranches.map((b, i) => `
    <button type="button" id="branch-picker-option-${i}" class="ij-branch-picker-item ij-palette-item${i === branchPickerIndex ? ' active' : ''}${b === current ? ' current' : ''}" data-branch-idx="${i}" role="option" aria-selected="${i === branchPickerIndex ? 'true' : 'false'}">
      <span class="ij-branch-picker-name">⎇ ${highlightBranchName(b, query)}</span>
      ${b === current ? '<span class="ij-branch-picker-tag">current</span>' : ''}
      ${b === defaultBranch && b !== current ? '<span class="ij-branch-picker-tag">default</span>' : ''}
    </button>`).join('');
  results.querySelectorAll('[data-branch-idx]').forEach((btn) => {
    btn.addEventListener('click', () => {
      branchPickerIndex = Number(btn.dataset.branchIdx);
      confirmBranchPickerSelection();
    });
  });
  results.querySelector('.ij-branch-picker-item.active')?.scrollIntoView({ block: 'nearest' });
  updateBranchAutocompleteGhost(query);
}

function confirmBranchPickerSelection() {
  const branch = branchPickerBranches[branchPickerIndex];
  if (!branch) return;
  hideBranchPicker();
  if (branch !== state.currentBranch) checkoutBranch(branch);
}

function scheduleGoToClassSearch(query) {
  if (gotoClassTimer) clearTimeout(gotoClassTimer);
  gotoClassTimer = setTimeout(() => runGoToClassSearch(query), 120);
}

async function runGoToClassSearch(query) {
  const results = $('#goto-class-results');
  if (!results || !state.repo) return;
  try {
    const q = new URLSearchParams({ q: query.trim(), limit: '50' });
    gotoClassHits = await api(`${repoApi(state.repo, '/workspace/classes')}?${q}`);
    if (!Array.isArray(gotoClassHits)) gotoClassHits = [];
    gotoClassHits = gotoClassHits.filter((hit) => !String(hit.path || '').toLowerCase().endsWith('.class'));
    gotoClassIndex = 0;
    renderGoToClassHits();
  } catch (e) {
    results.innerHTML = `<p class="px-4 py-3 text-xs text-red-400">${escapeHtml(e.message || 'Search failed')}</p>`;
  }
}

function renderGoToClassHits() {
  const results = $('#goto-class-results');
  if (!results) return;
  if (!gotoClassHits.length) {
    results.innerHTML = '<p class="px-4 py-3 text-xs text-gray-600">No matching classes</p>';
    return;
  }
  gotoClassIndex = Math.min(gotoClassIndex, Math.max(gotoClassHits.length - 1, 0));
  results.innerHTML = gotoClassHits.map((hit, i) => {
    const qual = hit.qualified && hit.qualified !== hit.name ? hit.qualified : hit.path;
    const kind = hit.kind || 'class';
    return `<button type="button" class="ij-goto-class-item ij-palette-item${i === gotoClassIndex ? ' active' : ''}" data-goto-class-idx="${i}">
      <span class="ij-goto-class-main">
        <span class="ij-goto-class-name">${escapeHtml(hit.name)}</span>
        <span class="ij-goto-class-qual">${escapeHtml(qual)}</span>
      </span>
      <span class="ij-goto-class-kind">${escapeHtml(kind)}</span>
    </button>`;
  }).join('');
  results.querySelectorAll('[data-goto-class-idx]').forEach((btn) => {
    btn.addEventListener('click', () => {
      gotoClassIndex = Number(btn.dataset.gotoClassIdx);
      openGoToClassSelection();
    });
  });
  results.querySelector('.ij-goto-class-item.active')?.scrollIntoView({ block: 'nearest' });
}

function openGoToClassSelection() {
  const hit = gotoClassHits[gotoClassIndex];
  if (!hit?.path) return;
  hideGoToClass();
  openFileAt(hit.path, hit.line || 1, hit.column || 1);
}

let searchIndex = 0;
let searchHits = [];
let searchTimer = null;

function showSearchEverywhere(initialQuery = '') {
  if (!state.repo) {
    toast('Open a repository first', 'info');
    return;
  }
  closeAllMenus();
  hidePalette();
  hideGoToClass();
  searchIndex = 0;
  searchHits = [];
  $('#search-overlay')?.classList.add('open');
  const input = $('#search-input');
  const headerSearch = $('#header-search-input');
  if (headerSearch && initialQuery) headerSearch.value = initialQuery;
  if (input) {
    input.value = initialQuery;
    const results = $('#search-results');
    if (results) {
      results.innerHTML = initialQuery.trim()
        ? '<p class="px-4 py-3 text-xs text-gray-500">Searching…</p>'
        : '<p class="px-4 py-3 text-xs text-gray-600">Type to search classes, files, or text</p>';
    }
    scheduleSearchEverywhere(initialQuery);
    setTimeout(() => {
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
    }, 30);
  }
}

function hideSearchEverywhere() {
  $('#search-overlay')?.classList.remove('open');
  const headerSearch = $('#header-search-input');
  if (headerSearch && document.activeElement === headerSearch) headerSearch.blur();
  if (searchTimer) {
    clearTimeout(searchTimer);
    searchTimer = null;
  }
}

function searchKindLabel(kind) {
  if (kind === 'class') return 'Class';
  if (kind === 'file') return 'File';
  if (kind === 'text') return 'Text';
  return kind;
}

function scheduleSearchEverywhere(query) {
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => runSearchEverywhere(query), 140);
}

async function runSearchEverywhere(query) {
  const results = $('#search-results');
  if (!results || !state.repo) return;
  const q = query.trim();
  if (!q) {
    results.innerHTML = '<p class="px-4 py-3 text-xs text-gray-600">Type to search classes, files, or text</p>';
    searchHits = [];
    return;
  }
  try {
    const params = new URLSearchParams({ q: query, limit: '50' });
    searchHits = await api(`${repoApi(state.repo, '/workspace/search')}?${params}`);
    if (!Array.isArray(searchHits)) searchHits = [];
    searchHits = searchHits.filter((hit) => !String(hit.path || '').toLowerCase().endsWith('.class'));
    searchIndex = 0;
    renderSearchHits();
  } catch (e) {
    results.innerHTML = `<p class="px-4 py-3 text-xs text-red-400">${escapeHtml(e.message || 'Search failed')}</p>`;
  }
}

function renderSearchHits() {
  const results = $('#search-results');
  if (!results) return;
  if (!searchHits.length) {
    results.innerHTML = '<p class="px-4 py-3 text-xs text-gray-600">No matches</p>';
    return;
  }
  searchIndex = Math.min(searchIndex, Math.max(searchHits.length - 1, 0));
  results.innerHTML = searchHits.map((hit, i) => {
    const kind = searchKindLabel(hit.kind);
    return `<button type="button" class="ij-search-item ij-palette-item${i === searchIndex ? ' active' : ''}" data-search-idx="${i}">
      <span class="ij-goto-class-main">
        <span class="ij-goto-class-name">${escapeHtml(hit.label || hit.path)}</span>
        <span class="ij-goto-class-qual">${escapeHtml(hit.detail || hit.path)}</span>
      </span>
      <span class="ij-goto-class-kind ij-search-kind-${escapeHtml(hit.kind || 'file')}">${escapeHtml(kind)}</span>
    </button>`;
  }).join('');
  results.querySelectorAll('[data-search-idx]').forEach((btn) => {
    btn.addEventListener('click', () => {
      searchIndex = Number(btn.dataset.searchIdx);
      openSearchSelection();
    });
  });
  results.querySelector('.ij-search-item.active')?.scrollIntoView({ block: 'nearest' });
}

function openSearchSelection() {
  const hit = searchHits[searchIndex];
  if (!hit?.path) return;
  hideSearchEverywhere();
  void openFileAt(hit.path, hit.line || 1, hit.column || 1);
}

function bindSearchEverywhere() {
  $('#search-input')?.addEventListener('input', (e) => {
    searchIndex = 0;
    scheduleSearchEverywhere(e.target.value);
  });
  $('#search-input')?.addEventListener('keydown', (e) => {
    if (!$('#search-overlay')?.classList.contains('open')) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      hideSearchEverywhere();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      searchIndex = Math.min(searchIndex + 1, Math.max(searchHits.length - 1, 0));
      renderSearchHits();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      searchIndex = Math.max(searchIndex - 1, 0);
      renderSearchHits();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      openSearchSelection();
    }
  });
  $('#search-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#search-overlay')) hideSearchEverywhere();
  });
}

function toggleSidebar(forceCollapse) {
  const sidebar = $('#sidebar');
  if (!sidebar) return;
  const collapsed = typeof forceCollapse === 'boolean'
    ? forceCollapse
    : !sidebar.classList.contains('collapsed');
  sidebar.classList.toggle('collapsed', collapsed);
  document.body.classList.toggle('sidebar-collapsed', collapsed);
  localStorage.setItem('reaper-sidebar-collapsed', collapsed ? '1' : '0');
  const btn = $('#btn-toggle-sidebar');
  if (btn) {
    btn.title = collapsed ? 'Show sidebar' : 'Hide sidebar';
    btn.setAttribute('aria-label', btn.title);
    btn.querySelector('.ij-icon-collapse')?.classList.toggle('hidden', collapsed);
    btn.querySelector('.ij-icon-expand')?.classList.toggle('hidden', !collapsed);
  }
  const expandBtn = $('#btn-sidebar-expand');
  if (expandBtn) {
    expandBtn.classList.toggle('active', collapsed);
  }
}

function initSidebarResize() {
  const saved = localStorage.getItem('reaper-sidebar-w');
  if (saved) document.documentElement.style.setProperty('--ij-sidebar-w', saved);
  if (localStorage.getItem('reaper-sidebar-collapsed') === '1') toggleSidebar(true);

  const resizer = $('#sidebar-resizer');
  if (!resizer) return;
  let dragging = false;
  const onMove = (e) => {
    if (!dragging) return;
    const sidebar = $('#sidebar');
    const strip = $('.ij-toolstrip');
    const min = 180;
    const max = Math.min(window.innerWidth * 0.45, 480);
    const x = e.clientX - (strip?.offsetWidth || 38);
    const w = Math.max(min, Math.min(max, x));
    document.documentElement.style.setProperty('--ij-sidebar-w', `${w}px`);
  };
  const onUp = () => {
    if (!dragging) return;
    dragging = false;
    resizer.classList.remove('dragging');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    const w = getComputedStyle(document.documentElement).getPropertyValue('--ij-sidebar-w').trim();
    if (w) localStorage.setItem('reaper-sidebar-w', w);
  };
  resizer.addEventListener('mousedown', (e) => {
    dragging = true;
    resizer.classList.add('dragging');
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  });
  window.addEventListener('mousemove', onMove);
  window.addEventListener('mouseup', onUp);
}

function applyAgentRightWidth(px) {
  const min = 240;
  const max = Math.min(window.innerWidth * 0.5, 560);
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  document.documentElement.style.setProperty('--ij-agent-right-w', `${clamped}px`);
  return clamped;
}

function applyAgentBottomHeight(px) {
  const dock = $('#agent-dock-bottom');
  if (!dock) return null;
  const min = 180;
  const max = Math.min(window.innerHeight * 0.7, 720);
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  dock.style.setProperty('--ij-agent-bottom-h', `${clamped}px`);
  return clamped;
}

function initAgentDockResize() {
  const savedW = localStorage.getItem(AGENT_RIGHT_WIDTH_KEY);
  if (savedW) document.documentElement.style.setProperty('--ij-agent-right-w', savedW);

  const savedH = parseInt(localStorage.getItem(AGENT_BOTTOM_HEIGHT_KEY), 10);
  if (Number.isFinite(savedH)) applyAgentBottomHeight(savedH);

  const resizer = $('#agent-right-resizer');
  if (resizer) {
    let dragging = false;
    const onMove = (e) => {
      if (!dragging) return;
      applyAgentRightWidth(window.innerWidth - e.clientX);
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      resizer.classList.remove('dragging');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const w = getComputedStyle(document.documentElement).getPropertyValue('--ij-agent-right-w').trim();
      if (w) localStorage.setItem(AGENT_RIGHT_WIDTH_KEY, w);
    };
    resizer.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      dragging = true;
      resizer.classList.add('dragging');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('blur', onUp);
  }

  const handle = $('#agent-bottom-resize');
  if (!handle) return;

  let draggingBottom = false;
  let startY = 0;
  let startH = 0;

  const stopBottomDrag = () => {
    if (!draggingBottom) return;
    draggingBottom = false;
    handle.classList.remove('active');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    const h = applyAgentBottomHeight($('#agent-dock-bottom')?.getBoundingClientRect().height);
    if (h) localStorage.setItem(AGENT_BOTTOM_HEIGHT_KEY, String(h));
  };

  handle.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    draggingBottom = true;
    startY = e.clientY;
    startH = $('#agent-dock-bottom')?.getBoundingClientRect().height || 0;
    handle.classList.add('active');
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  });

  window.addEventListener('mousemove', (e) => {
    if (!draggingBottom) return;
    applyAgentBottomHeight(startH + (startY - e.clientY));
  });

  window.addEventListener('mouseup', stopBottomDrag);
  window.addEventListener('blur', stopBottomDrag);
}

function bindPalette() {
  $('#palette-input')?.addEventListener('input', (e) => {
    paletteIndex = 0;
    renderPaletteResults(e.target.value);
  });
  $('#palette-input')?.addEventListener('keydown', (e) => {
    if (!$('#palette-overlay')?.classList.contains('open')) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      hidePalette();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const query = e.target.value;
      const items = PALETTE_COMMANDS.filter((c) => paletteMatches(c, query));
      paletteIndex = Math.min(paletteIndex + 1, Math.max(items.length - 1, 0));
      renderPaletteResults(query);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      paletteIndex = Math.max(paletteIndex - 1, 0);
      renderPaletteResults(e.target.value);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      runPaletteSelection();
    }
  });
  $('#palette-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#palette-overlay')) hidePalette();
  });
}

function bindGoToClass() {
  $('#goto-class-input')?.addEventListener('input', (e) => {
    gotoClassIndex = 0;
    scheduleGoToClassSearch(e.target.value);
  });
  $('#goto-class-input')?.addEventListener('keydown', (e) => {
    if (!$('#goto-class-overlay')?.classList.contains('open')) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      hideGoToClass();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      gotoClassIndex = Math.min(gotoClassIndex + 1, Math.max(gotoClassHits.length - 1, 0));
      renderGoToClassHits();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      gotoClassIndex = Math.max(gotoClassIndex - 1, 0);
      renderGoToClassHits();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      openGoToClassSelection();
    }
  });
  $('#goto-class-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#goto-class-overlay')) hideGoToClass();
  });
}

function bindGoToLine() {
  $('#goto-line-input')?.addEventListener('input', () => updateGoToLinePreview());
  $('#goto-line-input')?.addEventListener('keydown', (e) => {
    if (!$('#goto-line-overlay')?.classList.contains('open')) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      hideGoToLine();
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      submitGoToLine();
    }
  });
  $('#goto-line-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#goto-line-overlay')) hideGoToLine();
  });
}

function bindRenamePrompt() {
  $('#rename-prompt-cancel')?.addEventListener('click', () => hideRenamePrompt(null));
  $('#rename-prompt-submit')?.addEventListener('click', () => submitRenamePrompt());
  $('#rename-prompt-input')?.addEventListener('keydown', (e) => {
    if (!renamePromptIsOpen()) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      hideRenamePrompt(null);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      submitRenamePrompt();
    }
  });
  $('#rename-prompt-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#rename-prompt-overlay')) hideRenamePrompt(null);
  });
  $('#rename-prompt-overlay .glass')?.addEventListener('click', (e) => e.stopPropagation());
}

function bindBranchPicker() {
  $('#branch-picker-input')?.addEventListener('input', (e) => {
    branchPickerIndex = 0;
    renderBranchPickerResults(e.target.value);
  });
  $('#branch-picker-input')?.addEventListener('keydown', (e) => {
    if (!$('#branch-picker-overlay')?.classList.contains('open')) return;
    const input = e.target;
    if (e.key === 'Escape') {
      e.preventDefault();
      hideBranchPicker();
      return;
    }
    if (e.key === 'Tab') {
      e.preventDefault();
      if (!acceptBranchAutocomplete()) confirmBranchPickerSelection();
      return;
    }
    if (e.key === 'ArrowRight' && input.selectionStart === input.value.length && input.selectionEnd === input.value.length) {
      if (acceptBranchAutocomplete()) {
        e.preventDefault();
        return;
      }
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      branchPickerIndex = Math.min(branchPickerIndex + 1, Math.max(branchPickerBranches.length - 1, 0));
      renderBranchPickerResults(input.value || '');
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      branchPickerIndex = Math.max(branchPickerIndex - 1, 0);
      renderBranchPickerResults(input.value || '');
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      confirmBranchPickerSelection();
    }
  });
  $('#branch-picker-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#branch-picker-overlay')) hideBranchPicker();
  });
}

function bindRepoPicker() {
  $('#repo-picker-btn')?.addEventListener('click', () => showRepoPicker());
  $('#repo-picker-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#repo-picker-overlay')) hideRepoPicker();
  });
  $('#repo-picker-results')?.addEventListener('click', (e) => {
    if (state.repoPickerUnregisterBusy) return;
    const removeBtn = e.target.closest('[data-repo-remove]');
    if (removeBtn) {
      e.preventDefault();
      e.stopPropagation();
      state.repoPickerUnregisterName = removeBtn.dataset.repoRemove || null;
      renderRepoPickerResults();
      return;
    }
    const confirmBtn = e.target.closest('[data-repo-unregister-confirm]');
    if (confirmBtn) {
      e.preventDefault();
      e.stopPropagation();
      void unregisterRepo(confirmBtn.dataset.repoUnregisterConfirm);
      return;
    }
    if (e.target.closest('[data-repo-unregister-cancel]')) {
      e.preventDefault();
      e.stopPropagation();
      state.repoPickerUnregisterName = null;
      renderRepoPickerResults();
      return;
    }
    const openBtn = e.target.closest('[data-repo-open]');
    if (openBtn) {
      e.preventDefault();
      e.stopPropagation();
      requestRepoSelection(openBtn.dataset.repoOpen);
      return;
    }
    const restoreBtn = e.target.closest('[data-repo-restore]');
    if (restoreBtn) {
      e.preventDefault();
      e.stopPropagation();
      void restoreRepo(restoreBtn.dataset.repoRestore);
    }
  });
}

function bindMenus() {
  $$('.ij-menu-trigger').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const root = btn.closest('.ij-menu-root');
      const wasOpen = root?.classList.contains('open');
      closeAllMenus();
      if (!wasOpen) root?.classList.add('open');
    });
  });
  $$('.ij-menu-item[data-action]').forEach((item) => {
    item.addEventListener('click', (e) => {
      e.stopPropagation();
      if (item.disabled) return;
      closeAllMenus();
      runMenuAction(item.dataset.action);
    });
  });
  document.addEventListener('click', (e) => {
    if (e.target.closest('.ij-menu-root')) return;
    $$('.ij-menu-root.open').forEach((m) => m.classList.remove('open'));
    dismissTreeContextMenuIfOutside(e);
  });
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    dismissTreeContextMenuIfOutside(e);
  });
  ['#theme-select', '#theme-select-menu', '#settings-editor-font-size', '#settings-editor-font-family', '#editor-font-size-menu', '#editor-font-size-status'].forEach((sel) => {
    $(sel)?.addEventListener('mousedown', (e) => e.stopPropagation());
  });
}

function statusColor(status) {
  if (status === 'added' || status === 'modified') return 'text-git-modified';
  if (status === 'deleted') return 'text-git-deleted';
  if (status === 'untracked') return 'text-git-untracked';
  return 'text-gray-400';
}

function statusIcon(status) {
  if (status === 'added') return 'A';
  if (status === 'modified') return 'M';
  if (status === 'deleted') return 'D';
  if (status === 'untracked') return '?';
  if (status === 'conflict') return '!';
  return '•';
}

function statusLabel(status) {
  if (status === 'added') return 'New file';
  if (status === 'modified') return 'Modified';
  if (status === 'deleted') return 'Deleted';
  if (status === 'untracked') return 'Untracked';
  if (status === 'conflict') return 'Merge conflict';
  return 'Changed';
}

// --- Diff rendering (side-by-side) ---

function parseUnifiedDiff(text) {
  if (!text?.trim()) return [];
  const raw = text.startsWith('diff --git') ? text : text;
  const chunks = raw.split(/^diff --git /m).filter(Boolean);
  return chunks.map((chunk) => {
    const lines = chunk.split('\n');
    const head = lines[0] || '';
    let path = head.split(/\s+/).pop()?.replace(/^b\//, '') || head;
    if (head.includes(' a/') && head.includes(' b/')) {
      const m = head.match(/\sb\/(\S+)/);
      if (m) path = m[1];
    }
    const hunks = [];
    let current = null;
    for (const line of lines.slice(1)) {
      if (line.startsWith('@@')) {
        current = { header: line, lines: [] };
        hunks.push(current);
      } else if (current && (line.startsWith('+') || line.startsWith('-') || line.startsWith(' ') || line.startsWith('\\'))) {
        if (line.startsWith('\\')) continue;
        current.lines.push(line);
      }
    }
    return { path, hunks };
  });
}

function hunkToSideBySideRows(lines) {
  const rows = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line[0] === ' ') {
      rows.push({ left: line.slice(1), right: line.slice(1), type: 'ctx' });
      i += 1;
    } else if (line[0] === '-') {
      const dels = [];
      while (i < lines.length && lines[i][0] === '-') {
        dels.push(lines[i].slice(1));
        i += 1;
      }
      const adds = [];
      while (i < lines.length && lines[i][0] === '+') {
        adds.push(lines[i].slice(1));
        i += 1;
      }
      const n = Math.max(dels.length, adds.length);
      for (let k = 0; k < n; k += 1) {
        const left = dels[k] ?? '';
        const right = adds[k] ?? '';
        let type = 'change';
        if (left && !right) type = 'del';
        else if (!left && right) type = 'add';
        rows.push({ left, right, type });
      }
    } else if (line[0] === '+') {
      rows.push({ left: '', right: line.slice(1), type: 'add' });
      i += 1;
    } else {
      i += 1;
    }
  }
  return rows;
}

function renderSideBySideHtml(diffText) {
  if (!diffText?.trim()) {
    return '<div class="ij-sbs-empty">No changes</div>';
  }
  if (diffText.startsWith('(no diff')) {
    return `<div class="ij-sbs-empty">${escapeHtml(diffText)}</div>`;
  }
  const files = parseUnifiedDiff(diffText);
  if (!files.length) {
    return `<pre class="ij-sbs-empty">${escapeHtml(diffText)}</pre>`;
  }
  return files.map((file) => {
    const hunksHtml = file.hunks.map((hunk) => {
      const rows = hunkToSideBySideRows(hunk.lines);
      const body = rows.map((row) => `
        <div class="ij-sbs-row ij-sbs-row--${row.type}">
          <span class="ij-sbs-ln"></span>
          <span class="ij-sbs-cell">${escapeHtml(row.left)}</span>
          <span class="ij-sbs-ln"></span>
          <span class="ij-sbs-cell">${escapeHtml(row.right)}</span>
        </div>
      `).join('');
      return `
        <div class="ij-sbs-hunk-header">${escapeHtml(hunk.header)}</div>
        <div class="ij-sbs-grid">
          <div class="ij-sbs-grid-head"><span>Before</span><span></span><span>After</span><span></span></div>
          ${body}
        </div>
      `;
    }).join('');
    let emptyMsg = '<div class="ij-sbs-empty">No changes</div>';
    if (/Binary files .* differ/i.test(diffText)) {
      emptyMsg = '<div class="ij-sbs-empty">Binary file — preview not available</div>';
    }
    return `
      <section class="ij-sbs-file">
        <div class="ij-sbs-file-header">${escapeHtml(file.path)}</div>
        ${hunksHtml || emptyMsg}
      </section>
    `;
  }).join('');
}

function renderDiffInto(el, diffText) {
  if (!el) return;
  el.innerHTML = renderSideBySideHtml(diffText);
}

// --- Merge conflicts ---

function parseConflictMarkers(text) {
  const lines = text.split('\n');
  const conflicts = [];
  let i = 0;
  while (i < lines.length) {
    if (!lines[i].startsWith('<<<<<<<')) {
      i += 1;
      continue;
    }
    const start = i;
    const marker = lines[i].slice(7).trim();
    i += 1;
    const ours = [];
    while (i < lines.length && !lines[i].startsWith('=======')) {
      ours.push(lines[i]);
      i += 1;
    }
    if (i >= lines.length) break;
    i += 1;
    const theirs = [];
    while (i < lines.length && !lines[i].startsWith('>>>>>>>')) {
      theirs.push(lines[i]);
      i += 1;
    }
    let theirsMarker = '';
    let end = i;
    if (i < lines.length && lines[i].startsWith('>>>>>>>')) {
      theirsMarker = lines[i].slice(7).trim();
      end = i;
      i += 1;
    }
    conflicts.push({
      start,
      end,
      marker,
      theirsMarker,
      ours: ours.join('\n'),
      theirs: theirs.join('\n'),
    });
  }
  return conflicts;
}

function isJavaTestFilePath(path) {
  if (!path?.endsWith('.java')) return false;
  const normalized = path.replace(/\\/g, '/').toLowerCase();
  return normalized.includes('/src/test/java/')
    || normalized.includes('/src/integrationtest/java/')
    || normalized.includes('/src/inttest/java/')
    || normalized.includes('/test/java/')
    || normalized.endsWith('test.java')
    || normalized.endsWith('tests.java')
    || normalized.endsWith('it.java');
}

function javaFqcnFromSource(path, content) {
  const simple = path.split('/').pop()?.replace(/\.java$/i, '');
  if (!simple) return null;
  const pkg = content.match(/^\s*package\s+([\w.]+)\s*;/m)?.[1];
  if (pkg) return `${pkg}.${simple}`;
  const normalized = path.replace(/\\/g, '/');
  for (const marker of ['/src/test/java/', '/src/main/java/', '/test/java/', '/main/java/']) {
    const idx = normalized.indexOf(marker);
    if (idx >= 0) {
      const suffix = normalized.slice(idx + marker.length);
      if (suffix.endsWith('.java')) {
        return suffix.slice(0, -5).replace(/\//g, '.');
      }
    }
  }
  return simple;
}

function isTestAnnotationLine(line) {
  const trimmed = (line || '').trim();
  if (/^\s*(import|package)\b/.test(trimmed)) return false;
  return /@Test|@ParameterizedTest|@RepeatedTest|@TestFactory|@TestTemplate/.test(trimmed);
}

/** Name from a method declaration (`void foo(`), not a call (`assertNotNull(`). */
function testMethodDeclarationName(code) {
  const trimmed = (code || '').split('//')[0].trim();
  if (!trimmed || !trimmed.includes('(')) return null;
  if (trimmed.includes('=') || trimmed.includes('new ')) return null;
  const paren = trimmed.indexOf('(');
  const before = trimmed.slice(0, paren).trim();
  if (!before || !/\s/.test(before)) return null;
  const token = before.split(/\s+/).filter((t) => !['public', 'protected', 'private', 'static', 'final', 'synchronized', 'abstract'].includes(t)).pop();
  if (!token || ['void', 'class', 'int', 'long', 'boolean', 'char', 'byte', 'short', 'float', 'double'].includes(token)) return null;
  if (!/^[A-Za-z_]\w*$/.test(token)) return null;
  return token;
}

function testMethodAfterAnnotation(lines, annoIdx) {
  const end = Math.min(annoIdx + 6, lines.length);
  for (let j = annoIdx; j < end; j += 1) {
    if (j > annoIdx && isTestAnnotationLine(lines[j])) break;
    const trimmed = lines[j].trim();
    if (!trimmed || trimmed.startsWith('//')) continue;
    const name = testMethodDeclarationName(stripLeadingJavaAnnotations(trimmed));
    if (name) return { sigIdx: j, name };
  }
  return null;
}

function stripLeadingJavaAnnotations(line) {
  let rest = line.trim();
  while (rest.startsWith('@')) {
    const open = rest.indexOf('(');
    if (open < 0) {
      const sp = rest.search(/\s/);
      rest = (sp >= 0 ? rest.slice(sp) : '').trimStart();
      continue;
    }
    let depth = 0;
    let end = open;
    for (let j = open; j < rest.length; j += 1) {
      if (rest[j] === '(') depth += 1;
      else if (rest[j] === ')') {
        depth -= 1;
        if (depth === 0) {
          end = j + 1;
          break;
        }
      }
    }
    if (depth !== 0) break;
    rest = rest.slice(end).trimStart();
  }
  return rest;
}

function findJavaMethodSignatureIndex(lines, start) {
  for (let i = start; i < lines.length; i += 1) {
    const trimmed = lines[i].trim();
    if (!trimmed || trimmed.startsWith('//')) continue;
    const code = stripLeadingJavaAnnotations(trimmed);
    if (code.startsWith('@')) continue;
    if (parseJavaMethodName(code)) return i;
  }
  return -1;
}

function parseJavaMethodName(line) {
  const trimmed = line.split('//')[0].trim();
  if (!trimmed || !trimmed.includes('(')) return null;
  if (/^\s*(import|package)\b/.test(trimmed) || trimmed.startsWith('@')) return null;
  for (const pat of ['if (', 'while (', 'for (', 'catch (', 'switch (', 'return ', 'throw ', 'new ']) {
    if (trimmed.includes(pat)) return null;
  }
  if (trimmed.includes('=')) return null;
  const paren = trimmed.indexOf('(');
  const before = trimmed.slice(0, paren).trim();
  if (!before || before.includes('.')) return null;
  // Declarations have a return type or modifiers before the name; calls do not (assertNotNull(...)).
  if (!/\s/.test(before)) return null;
  const token = before.split(/\s+/).filter((t) => !['public', 'protected', 'private', 'static', 'final', 'synchronized'].includes(t) && !t.endsWith('<')).pop();
  if (!token || token === 'void' || token === 'class') return null;
  if (!/^[A-Za-z_]\w*$/.test(token)) return null;
  return token;
}

function javaMethodBlockEnd(lines, signatureIdx) {
  let depth = 0;
  let started = false;
  for (let i = signatureIdx; i < lines.length; i += 1) {
    for (const ch of lines[i]) {
      if (ch === '{') {
        depth += 1;
        started = true;
      } else if (ch === '}' && started) {
        depth -= 1;
        if (depth === 0) return i;
      }
    }
  }
  return lines.length - 1;
}

function findTestAnnotationLineIndex(lines, start, sigIdx) {
  for (let i = sigIdx; i >= 0; i -= 1) {
    const t = lines[i].trim();
    if (!t || t.startsWith('//')) continue;
    if (/@Test|@ParameterizedTest|@RepeatedTest|@TestFactory|@TestTemplate/.test(t)) return i;
    if (t.startsWith('@')) continue;
    if (t.includes('(') && !t.endsWith(';') && !t.startsWith('class ')) continue;
    break;
  }
  return start;
}

function findJavaTypeDeclarationLine(lines) {
  for (let i = 0; i < lines.length; i += 1) {
    const t = lines[i].trim();
    if (/^(public\s+|protected\s+|private\s+)?(abstract\s+|static\s+)?(class|record|enum)\s+[A-Za-z_]\w*/.test(t) && !t.includes('(')) {
      return i;
    }
  }
  return -1;
}

function findJavaTestClassLine(lines) {
  return findJavaTypeDeclarationLine(lines);
}

function listJavaTestMethods(path, content) {
  if (!path?.endsWith('.java')) return [];
  const hasTestAnno = /@Test|@ParameterizedTest|@RepeatedTest|@TestFactory|@TestTemplate/.test(content);
  if (!isJavaTestFilePath(path) && !hasTestAnno) return [];
  const className = javaFqcnFromSource(path, content);
  if (!className) return [];
  const lines = content.split('\n');
  const out = [];
  let i = 0;
  while (i < lines.length) {
    if (!isTestAnnotationLine(lines[i])) {
      i += 1;
      continue;
    }
    const annoIdx = i;
    const found = testMethodAfterAnnotation(lines, annoIdx);
    if (!found) {
      i += 1;
      continue;
    }
    const end = javaMethodBlockEnd(lines, found.sigIdx);
    out.push({
      name: found.name,
      line: found.sigIdx + 1,
      glyphLine: annoIdx + 1,
      end_line: end + 1,
      filter: `${className}.${found.name}`,
      isClass: false,
    });
    i = end + 1;
  }

  const classLine = findJavaTestClassLine(lines);
  if (classLine >= 0 && (out.length > 0 || isJavaTestFilePath(path))) {
    out.unshift({
      name: className.split('.').pop(),
      line: classLine + 1,
      glyphLine: classLine + 1,
      end_line: classLine + 1,
      filter: className,
      isClass: true,
    });
  }

  return out;
}

function clearTestRunWidgets() {
  if (!state.editor) return;
  for (const widget of state.testRunWidgets || []) {
    try {
      state.editor.removeGlyphMarginWidget(widget);
    } catch {
      /* ignore stale widgets */
    }
  }
  state.testRunWidgets = [];
  for (const widget of state.testCovWidgets || []) {
    try {
      state.editor.removeGlyphMarginWidget(widget);
    } catch {
      /* ignore stale widgets */
    }
  }
  state.testCovWidgets = [];
}

function projectSupportsCoverage() {
  return Boolean(state.repo);
}

function classLevelTestMethods(path, content) {
  if (!isJavaTestFilePath(path)) return [];
  const fqcn = javaFqcnFromSource(path, content);
  const classLine = findJavaTypeDeclarationLine(content.split('\n'));
  if (!fqcn || classLine < 0) return [];
  return [{
    name: fqcn.split('.').pop(),
    line: classLine + 1,
    glyphLine: classLine + 1,
    end_line: classLine + 1,
    filter: fqcn,
    isClass: true,
  }];
}

function listSpringBootAppGutterTargets(path, content) {
  if (!isJavaMainSourceFile(path)) return [];
  const spring = detectSpringBootApp(content);
  if (!spring?.qualifiedName) return [];
  const classLine = findJavaTypeDeclarationLine(content.split('\n'));
  if (classLine < 0) return [];
  return [{
    name: spring.className,
    glyphLine: classLine + 1,
    qualifiedName: spring.qualifiedName,
  }];
}

function coverageHasLineData(cov) {
  return Boolean(cov?.lines?.length);
}

function coverageHasUsableData(cov) {
  return Boolean(
    cov && (
      cov.lines?.length
      || cov.total_lines > 0
      || cov.report_path
      || cov.summary
    ),
  );
}

function gutterRunPlayIconHtml() {
  return '<svg class="ij-gutter-run-icon" viewBox="0 0 10 10" aria-hidden="true"><path d="M2.1 1.2 8.4 5 2.1 8.8Z" fill="currentColor"/></svg>';
}

function gutterCoverageIconHtml() {
  return '<svg class="ij-gutter-cov-icon" viewBox="0 0 10 10" aria-hidden="true"><text x="5" y="7.1" text-anchor="middle" font-size="8.8" font-family="system-ui,sans-serif" fill="currentColor">©</text></svg>';
}

function findNativeMainLine(content) {
  const lines = String(content || '').split('\n');
  for (let i = 0; i < lines.length; i += 1) {
    const trimmed = lines[i].trim();
    if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('/*') || trimmed.startsWith('*')) continue;
    if (/\bmain\s*\(/.test(lines[i])) return i + 1;
  }
  return -1;
}

function createNativeRunWidget(glyphLine, label) {
  const domNode = document.createElement('button');
  domNode.type = 'button';
  domNode.className = 'ij-native-run-widget ij-gutter-run-btn';
  domNode.innerHTML = gutterRunPlayIconHtml();
  domNode.title = label || 'Run';
  domNode.setAttribute('aria-label', label || 'Run');
  domNode.addEventListener('mousedown', (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  domNode.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    void runActive();
  });
  const lane = monaco.editor.GlyphMarginLane?.Right ?? 2;
  return {
    getId: () => `ij-native-run-${glyphLine}`,
    getDomNode: () => domNode,
    getPosition: () => ({
      range: new monaco.Range(glyphLine, 1, glyphLine, 1),
      lane,
    }),
  };
}

function createTestRunWidget(method) {
  const domNode = document.createElement('button');
  domNode.type = 'button';
  domNode.className = 'ij-test-run-widget ij-gutter-run-btn';
  domNode.innerHTML = gutterRunPlayIconHtml();
  const label = method.isClass ? `Run all tests in ${method.filter}` : `Run ${method.filter}`;
  domNode.title = label;
  domNode.setAttribute('aria-label', label);
  domNode.addEventListener('mousedown', (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  domNode.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    runGradleTest(method.filter);
  });
  const lane = monaco.editor.GlyphMarginLane?.Right ?? 2;
  return {
    getId: () => (method.isClass ? `ij-test-run-class-${method.filter}` : `ij-test-run-${method.filter}`),
    getDomNode: () => domNode,
    getPosition: () => ({
      range: new monaco.Range(method.glyphLine, 1, method.glyphLine, 1),
      lane,
    }),
  };
}

function createSpringBootRunWidget(target) {
  const domNode = document.createElement('button');
  domNode.type = 'button';
  domNode.className = 'ij-spring-run-widget ij-gutter-run-btn';
  domNode.innerHTML = gutterRunPlayIconHtml();
  const label = `Run Spring Boot · ${target.name}`;
  domNode.title = label;
  domNode.setAttribute('aria-label', label);
  domNode.addEventListener('mousedown', (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  domNode.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    void runSpringBootApplication(target.qualifiedName);
  });
  const lane = monaco.editor.GlyphMarginLane?.Right ?? 2;
  return {
    getId: () => `ij-spring-run-${target.qualifiedName}`,
    getDomNode: () => domNode,
    getPosition: () => ({
      range: new monaco.Range(target.glyphLine, 1, target.glyphLine, 1),
      lane,
    }),
  };
}

function createTestCoverageWidget(method) {
  const domNode = document.createElement('button');
  domNode.type = 'button';
  domNode.className = 'ij-test-cov-widget ij-gutter-cov-btn';
  domNode.innerHTML = gutterCoverageIconHtml();
  const label = method.isClass
    ? `Run all tests with coverage in ${method.filter}`
    : `Run ${method.filter} with coverage`;
  domNode.title = label;
  domNode.setAttribute('aria-label', label);
  domNode.addEventListener('mousedown', (e) => {
    e.preventDefault();
    e.stopPropagation();
  });
  domNode.addEventListener('click', (e) => {
    e.preventDefault();
    e.stopPropagation();
    void runProjectTestWithCoverage(method.filter);
  });
  const lane = monaco.editor.GlyphMarginLane?.Left ?? 1;
  return {
    getId: () => (method.isClass ? `ij-test-cov-class-${method.filter}` : `ij-test-cov-${method.filter}`),
    getDomNode: () => domNode,
    getPosition: () => ({
      range: new monaco.Range(method.glyphLine, 1, method.glyphLine, 1),
      lane,
    }),
  };
}

function applyTestRunDecorations() {
  if (!state.editor || !window.monaco) return;
  clearTestRunWidgets();
  state.testMethodsByLine = new Map();
  if (typeof state.editor.addGlyphMarginWidget !== 'function') return;
  const paint = () => {
    const path = state.activeTab;
    const content = state.editor.getModel()?.getValue() ?? '';
    if (path?.endsWith('.java')) {
      let methods = listJavaTestMethods(path, content);
      if (!methods.length) {
        methods = classLevelTestMethods(path, content);
      }
      if (!methods.length) {
        const springApps = listSpringBootAppGutterTargets(path, content);
        if (springApps.length) {
          state.testRunWidgets = [];
          for (const app of springApps) {
            const runWidget = createSpringBootRunWidget(app);
            state.editor.addGlyphMarginWidget(runWidget);
            state.testRunWidgets.push(runWidget);
          }
        }
        return;
      }
      clearTestRunWidgets();
      state.testRunWidgets = [];
      state.testCovWidgets = [];
      const showCoverage = projectSupportsCoverage();
      for (const method of methods) {
        const runWidget = createTestRunWidget(method);
        state.editor.addGlyphMarginWidget(runWidget);
        state.testRunWidgets.push(runWidget);
        state.testMethodsByLine.set(method.glyphLine, method);
        if (showCoverage) {
          const covWidget = createTestCoverageWidget(method);
          state.editor.addGlyphMarginWidget(covWidget);
          state.testCovWidgets.push(covWidget);
        }
      }
      return;
    }
    if (isNativeSourcePath(path)) {
      const mainLine = findNativeMainLine(content);
      const target = resolveRunTarget(path, content, state.runInfo);
      if (mainLine > 0 && target?.runnable && (target.mode === 'native' || target.mode === 'native-test')) {
        const label = runTargetLabel(target, content, path);
        const widget = createNativeRunWidget(mainLine, label);
        state.editor.addGlyphMarginWidget(widget);
        state.testRunWidgets = [widget];
      }
    }
  };
  requestAnimationFrame(() => requestAnimationFrame(paint));
}

function clearCoverageDecorations() {
  if (!state.editor || !window.monaco) return;
  state.coverageDecorationIds = state.editor.deltaDecorations(state.coverageDecorationIds ?? [], []);
}

const COVERAGE_INLINE_KEY = 'reaper-coverage-inline';

async function persistCoverageInlinePref(enabled) {
  try {
    await api('/api/ui-preferences', {
      method: 'PATCH',
      body: JSON.stringify({ coverage_inline_enabled: enabled }),
    });
  } catch (err) {
    console.warn('[Reaper] Failed to save ui-preferences.json:', err);
  }
}

async function loadCoverageInlinePref() {
  try {
    const prefs = await api('/api/ui-preferences');
    state.coverageInlineEnabled = prefs.coverage_inline_enabled !== false;
    const legacy = localStorage.getItem(COVERAGE_INLINE_KEY);
    if (legacy !== null) {
      const legacyEnabled = legacy !== '0';
      if (legacyEnabled !== state.coverageInlineEnabled) {
        state.coverageInlineEnabled = legacyEnabled;
        await persistCoverageInlinePref(legacyEnabled);
      }
      localStorage.removeItem(COVERAGE_INLINE_KEY);
    }
  } catch {
    state.coverageInlineEnabled = true;
  }
  syncCoverageInlineButton();
}

function getCoverageInlineEnabled() {
  return state.coverageInlineEnabled !== false;
}

function setCoverageInlineEnabled(enabled) {
  state.coverageInlineEnabled = enabled;
  void persistCoverageInlinePref(enabled);
  syncCoverageInlineButton();
  if (!enabled) {
    clearCoverageDecorations();
  } else {
    reapplyCoverageForTab(state.activeTab);
  }
}

function toggleCoverageInline() {
  setCoverageInlineEnabled(!getCoverageInlineEnabled());
}

function clearBlameDecorations() {
  if (!state.editor || !window.monaco) return;
  state.blameDecorationIds = state.editor.deltaDecorations(state.blameDecorationIds ?? [], []);
}

function setBlameEnabled(enabled) {
  state.blameEnabled = enabled;
  const btn = $('#tb-blame');
  if (btn) {
    btn.classList.toggle('is-active', enabled);
    btn.setAttribute('aria-pressed', enabled ? 'true' : 'false');
    btn.title = enabled ? 'Hide git blame in gutter' : 'Show git blame in gutter';
  }
  if (!enabled) {
    clearBlameDecorations();
    return;
  }
  if (state.activeTab) void loadBlameForTab(state.activeTab);
}

function toggleBlameInline() {
  setBlameEnabled(!state.blameEnabled);
}

function syncBlameButton() {
  const btn = $('#tb-blame');
  if (!btn) return;
  const show = Boolean(state.repo && state.activeTab);
  btn.classList.toggle('hidden', !show);
}

async function loadBlameForTab(path) {
  if (!state.repo || !path || !state.blameEnabled) return;
  const blamePath = stripJavaDiagOverlayPath(path);
  try {
    const lines = await api(
      repoApi(state.repo, `/workspace/blame?path=${encodeURIComponent(blamePath)}`),
    );
    state.blameByPath.set(path, lines);
    if (state.activeTab === path) applyBlameDecorations(path, lines);
  } catch (e) {
    const msg = String(e.message || e || 'Blame failed');
    if (/not a git repository/i.test(msg)) {
      toast('Blame needs a git repo — this workspace has no .git (init or clone first)', 'info');
    } else {
      toast(msg, 'error');
    }
    setBlameEnabled(false);
  }
}

function applyBlameDecorations(path, lines) {
  if (!state.blameEnabled || !state.editor || !window.monaco || state.activeTab !== path) return;
  const decorations = (lines || []).map((entry) => {
    const author = entry.author || '?';
    const initials = authorInitials(author);
    const hover = `**${escapeHtml(author)}** · ${escapeHtml(entry.date || '')}\n\n\`${escapeHtml((entry.commit || '').slice(0, 7))}\` ${escapeHtml(entry.summary || '')}`;
    return {
      range: new monaco.Range(entry.line, 1, entry.line, 1),
      options: {
        isWholeLine: false,
        glyphMarginClassName: 'ij-blame-glyph',
        glyphMarginHoverMessage: { value: hover },
        lineDecorationsClassName: 'ij-blame-line',
        hoverMessage: { value: hover },
      },
    };
  });
  state.blameDecorationIds = state.editor.deltaDecorations(state.blameDecorationIds ?? [], decorations);
}

function syncCoverageInlineButton() {
  const btn = $('#tb-coverage-inline');
  const panelBtn = $('#btn-coverage-toggle-inline');
  const show = Boolean(state.repo && state.activeTab?.endsWith('.java'));
  for (const el of [btn, panelBtn]) {
    if (!el) continue;
    el.classList.toggle('hidden', !show);
    const on = getCoverageInlineEnabled();
    el.classList.toggle('is-active', on);
    el.setAttribute('aria-pressed', on ? 'true' : 'false');
    el.title = on
      ? 'Hide covered / uncovered line highlighting in editor'
      : 'Show covered / uncovered line highlighting in editor';
  }
}

function applyCoverageDecorations(path, cov) {
  if (!getCoverageInlineEnabled()) {
    clearCoverageDecorations();
    return;
  }
  if (!state.editor || !window.monaco || state.activeTab !== path) return;
  const lines = cov?.lines ?? [];
  const decorations = lines.map(({ line, status }) => {
    let className = 'ij-cov-missed';
    let color = '#f87171';
    let hover = 'Not covered';
    if (status === 'covered') {
      className = 'ij-cov-covered';
      color = '#4ade80';
      hover = 'Covered';
    } else if (status === 'partial') {
      className = 'ij-cov-partial';
      color = '#fbbf24';
      hover = 'Partially covered';
    }
    return {
      range: new monaco.Range(line, 1, line, 1),
      options: {
        isWholeLine: true,
        className,
        overviewRuler: {
          color,
          position: monaco.editor.OverviewRulerLane.Left,
        },
        hoverMessage: { value: hover },
      },
    };
  });
  state.coverageDecorationIds = state.editor.deltaDecorations(
    state.coverageDecorationIds ?? [],
    decorations,
  );
}

function updateCoverageStatus(cov) {
  const btn = $('#status-coverage');
  const text = $('#status-coverage-text');
  if (!btn || !text) return;
  if (!cov?.total_lines) {
    btn.classList.add('hidden');
    text.textContent = '';
    btn.title = 'Test coverage — click to open report';
    return;
  }
  btn.classList.remove('hidden');
  const fileHint = cov.coverage_path
    ? cov.coverage_path.split('/').pop()
    : null;
  text.textContent = fileHint
    ? `Coverage ${cov.summary} · ${fileHint}`
    : `Coverage ${cov.summary}`;
  btn.title = `${cov.message || cov.report_path || 'Test coverage'} — click for report`;
}

function formatCoveragePct(rate) {
  if (rate == null || Number.isNaN(rate)) return '—';
  return `${Math.round(rate * 100)}%`;
}

function coveragePctClass(rate) {
  const pct = Math.round((rate ?? 0) * 100);
  if (pct >= 80) return 'ij-coverage-file-pct--good';
  if (pct >= 50) return 'ij-coverage-file-pct--warn';
  return 'ij-coverage-file-pct--bad';
}

function coverageBarFillClass(rate) {
  const pct = Math.round((rate ?? 0) * 100);
  if (pct >= 80) return '';
  if (pct >= 50) return 'ij-coverage-metric-fill--mid';
  return 'ij-coverage-metric-fill--low';
}

function renderCoverageMetric(label, counter) {
  if (!counter?.total) {
    return `<div class="ij-coverage-metric">
      <span class="ij-coverage-metric-label">${label}</span>
      <div class="ij-coverage-metric-bar"></div>
      <span class="ij-coverage-metric-pct">—</span>
    </div>`;
  }
  const pct = formatCoveragePct(counter.rate);
  const width = Math.max(2, Math.round((counter.rate ?? 0) * 100));
  return `<div class="ij-coverage-metric">
    <span class="ij-coverage-metric-label">${label}</span>
    <div class="ij-coverage-metric-bar" title="${counter.covered}/${counter.total} covered">
      <div class="ij-coverage-metric-fill ${coverageBarFillClass(counter.rate)}" style="width:${width}%"></div>
    </div>
    <span class="ij-coverage-metric-pct">${pct}</span>
  </div>`;
}

function renderCoveragePanel(summary) {
  const body = $('#coverage-panel-body');
  const subtitle = $('#coverage-panel-subtitle');
  const htmlBtn = $('#btn-coverage-open-html');
  if (!body) return;

  state.coverageReport = summary ?? null;

  if (subtitle) {
    subtitle.textContent = summary?.project_root
      ? summary.project_root.split('/').pop() || summary.project_root
      : '';
  }

  if (htmlBtn) {
    if (summary?.html_report_path) {
      htmlBtn.classList.remove('hidden');
      htmlBtn.disabled = false;
    } else {
      htmlBtn.classList.add('hidden');
      htmlBtn.disabled = true;
    }
  }

  if (!summary) {
    body.innerHTML = '<p class="ij-coverage-empty">No coverage data.</p>';
    return;
  }

  if (summary.message && !summary.files?.length) {
    body.innerHTML = `<p class="ij-coverage-empty">${escapeHtml(summary.message)}</p>
      <p class="ij-coverage-empty ij-coverage-empty-hint">Use <strong>Run with coverage</strong> below to compile, run matching tests, and refresh the report.</p>`;
    return;
  }

  const totals = summary.totals ?? {};
  const current = summary.current_file;
  const activePath = summary.query_path;
  const files = summary.files ?? [];

  let html = `<div class="ij-coverage-metrics">
    ${renderCoverageMetric('Lines', totals.lines)}
    ${totals.branches?.total ? renderCoverageMetric('Branch', totals.branches) : ''}
    ${totals.instructions?.total ? renderCoverageMetric('Instr', totals.instructions) : ''}
  </div>`;

  if (current) {
    html += `<div class="ij-coverage-current">
      <div class="ij-coverage-section-title">Current file</div>
      <div class="ij-coverage-current-name">${escapeHtml(current.name)}</div>
      <div class="ij-coverage-current-detail">${formatCoveragePct(current.lines?.rate)} · ${current.lines?.covered ?? 0}/${current.lines?.total ?? 0} lines</div>
    </div>`;
  }

  if (files.length) {
    const rows = files.map((f) => {
      const isActive = f.path === activePath
        || (current && f.path === current.path);
      return `<button type="button" class="ij-coverage-file-row${isActive ? ' is-active' : ''}" data-coverage-path="${escapeHtml(f.path)}" title="${escapeHtml(f.path)}">
        <span class="ij-coverage-file-name">${escapeHtml(f.name)}</span>
        <span class="ij-coverage-file-pct ${coveragePctClass(f.lines?.rate)}">${formatCoveragePct(f.lines?.rate)}</span>
      </button>`;
    }).join('');
    html += `<div>
      <div class="ij-coverage-section-title">Source files (${files.length})</div>
      <div class="ij-coverage-file-list">${rows}</div>
    </div>`;
  }

  if (summary.report_path) {
    html += `<p class="ij-coverage-report-path" title="${escapeHtml(summary.report_path)}">Report: ${escapeHtml(summary.report_path.split('/').pop())}</p>`;
  }

  body.innerHTML = html;
  body.querySelectorAll('[data-coverage-path]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const path = btn.dataset.coveragePath;
      if (!path) return;
      void openCoverageFile(path);
    });
  });
}

async function openCoverageFile(path) {
  if (!path) return;
  await openFileAt(path);
  await fetchAndApplyCoverage(path);
  if (state.coveragePanelOpen) {
    await refreshCoveragePanel(path);
  }
}

async function fetchCoverageReport(path) {
  if (!state.repo || !path) return null;
  try {
    return await api(
      `${repoApi(state.repo, '/workspace/coverage/report')}?path=${encodeURIComponent(stripJavaDiagOverlayPath(path))}`,
    );
  } catch (e) {
    toast(`Coverage report: ${e.message}`, 'warning');
    return null;
  }
}

async function refreshCoveragePanel(path) {
  const target = path || state.activeTab;
  if (!target) {
    renderCoveragePanel(null);
    return null;
  }
  const summary = await fetchCoverageReport(target);
  renderCoveragePanel(summary);
  return summary;
}

function applyCoveragePanelLayout() {
  const dock = $('#coverage-dock-right');
  const resizer = $('#coverage-right-resizer');
  const open = state.coveragePanelOpen;
  dock?.classList.toggle('hidden', !open);
  resizer?.classList.toggle('hidden', !open);
}

function showCoveragePanel() {
  state.coveragePanelOpen = true;
  applyCoveragePanelLayout();
  void refreshCoveragePanel(state.activeTab);
}

function hideCoveragePanel() {
  state.coveragePanelOpen = false;
  applyCoveragePanelLayout();
}

function toggleCoveragePanel() {
  if (state.coveragePanelOpen) hideCoveragePanel();
  else showCoveragePanel();
}

// --- Debugger ---

function isDebuggablePath(path) {
  if (!path) return false;
  return path.endsWith('.py') || path.endsWith('.pyw')
    || path.endsWith('.go')
    || path.endsWith('.rs')
    || path.endsWith('.js') || path.endsWith('.mjs') || path.endsWith('.cjs')
    || path.endsWith('.ts') || path.endsWith('.tsx')
    || path.endsWith('.java')
    || path.endsWith('.kt') || path.endsWith('.kts')
    || isNativeSourcePath(path);
}

function applyDebugPanelLayout() {
  const dock = $('#debug-dock-right');
  const resizer = $('#debug-right-resizer');
  const open = state.debugPanelOpen;
  dock?.classList.toggle('hidden', !open);
  resizer?.classList.toggle('hidden', !open);
}

function showDebugPanel() {
  state.debugPanelOpen = true;
  applyDebugPanelLayout();
  renderDebugPanel();
}

function hideDebugPanel() {
  state.debugPanelOpen = false;
  applyDebugPanelLayout();
}

function toggleDebugPanel() {
  if (state.debugPanelOpen) hideDebugPanel();
  else showDebugPanel();
}

function syncDebugToolbar() {
  const tbDebug = $('#tb-debug');
  const controls = $('#debug-controls');
  const active = state.debugActive;
  const status = state.debugState?.status;
  const stopped = status === 'stopped';
  const running = status === 'running' || status === 'starting';
  controls?.classList.toggle('hidden', !active);
  tbDebug?.classList.toggle('is-active', active);
  $('#tb-debug-continue')?.toggleAttribute('disabled', !stopped);
  $('#tb-debug-step-over')?.toggleAttribute('disabled', !stopped);
  $('#tb-debug-step-in')?.toggleAttribute('disabled', !stopped);
  $('#tb-debug-step-out')?.toggleAttribute('disabled', !stopped);
  $('#tb-debug-stop')?.toggleAttribute('disabled', !active);
  if (tbDebug && !active) {
    const cap = state.debugCapabilities;
    tbDebug.title = cap?.supported
      ? `Debug ${cap.language} (F6)`
      : cap?.reason || 'Debug (F6)';
  } else if (tbDebug && active) {
    tbDebug.title = 'Restart debug (F6)';
  }
}

async function refreshDebugCapabilities() {
  const path = state.activeTab;
  const tbDebug = $('#tb-debug');
  if (!state.repo || !path || !isDebuggablePath(path)) {
    state.debugCapabilities = null;
    if (tbDebug) {
      tbDebug.classList.add('hidden');
      tbDebug.disabled = true;
    }
    syncDebugToolbar();
    return;
  }
  try {
    const line = state.editor?.getPosition?.()?.lineNumber || 1;
    const content = state.editor?.getValue?.() ?? state.tabContents?.get?.(path);
    state.debugCapabilities = await api(repoApi(state.repo, '/workspace/debug/capabilities'), {
      method: 'POST',
      body: JSON.stringify({ path, line, content }),
      timeoutMs: 15_000,
    });
  } catch {
    // Don't permanently block debug if capabilities check fails.
    state.debugCapabilities = {
      supported: true,
      language: path.endsWith('.java') ? 'Java' : 'Unknown',
      reason: null,
    };
  }
  const runnable = !!state.runTarget?.runnable;
  const pathLooksJvm = /\.(java|kt|kts)$/i.test(path || '');
  const cap = state.debugCapabilities;
  const capBlocked = cap != null && cap.supported === false && !pathLooksJvm;
  if (tbDebug) {
    // Show for any debuggable path; don't hide just because run-target is still loading.
    tbDebug.classList.toggle('hidden', !isDebuggablePath(path));
    // Keep clickable for Java/Kotlin/Spring even if runnable/capabilities are stale.
    tbDebug.disabled = capBlocked && !pathLooksJvm && !runnable;
    if (!state.debugActive) {
      tbDebug.title = capBlocked
        ? (cap.reason || 'Debug unavailable')
        : cap?.supported
          ? `Debug ${cap.language || 'program'} (F6)`
          : 'Debug (F6)';
    } else {
      tbDebug.title = 'Restart debug (F6)';
    }
  }
  syncDebugToolbar();
}

function renderDebugPanel() {
  const st = state.debugState || {};
  const subtitle = $('#debug-panel-subtitle');
  if (subtitle) {
    const parts = [];
    if (st.language) parts.push(st.language);
    if (st.adapter) parts.push(st.adapter);
    subtitle.textContent = parts.join(' · ');
  }
  const statusEl = $('#debug-panel-status');
  if (statusEl) {
    const label = {
      idle: 'Not debugging',
      starting: 'Starting…',
      running: 'Running',
      stopped: st.stop_reason ? `Paused (${st.stop_reason})` : 'Paused',
      terminated: 'Session ended',
    }[st.status] || st.status || '';
    statusEl.textContent = st.message || label;
    statusEl.className = `ij-debug-status${st.status === 'running' || st.status === 'starting' ? ' is-running' : ''}${st.status === 'stopped' ? ' is-stopped' : ''}${st.status === 'terminated' ? ' is-terminated' : ''}`;
  }
  const stackEl = $('#debug-callstack');
  if (stackEl) {
    const frames = st.frames || [];
    if (!frames.length) {
      stackEl.innerHTML = '<div class="ij-debug-frame"><span class="ij-debug-frame-name" style="color:var(--ij-text-muted)">No stack frames</span></div>';
    } else {
      stackEl.innerHTML = frames.map((f, i) => {
        const loc = f.path ? `${f.path.split('/').pop()}${f.line ? `:${f.line}` : ''}` : '';
        return `<button type="button" class="ij-debug-frame${i === 0 ? ' is-active' : ''}" data-frame-id="${f.id}" data-path="${escapeHtml(f.path || '')}" data-line="${f.line || 1}" data-column="${f.column || 1}">
          <span class="ij-debug-frame-name">${escapeHtml(f.name || 'frame')}</span>
          <span class="ij-debug-frame-loc">${escapeHtml(loc)}</span>
        </button>`;
      }).join('');
      stackEl.querySelectorAll('.ij-debug-frame').forEach((btn) => {
        btn.addEventListener('click', () => {
          const raw = btn.dataset.path;
          const p = debugFrameWorkspacePath(raw) || raw;
          const line = Number(btn.dataset.line) || 1;
          const col = Number(btn.dataset.column) || 1;
          if (p) void openFileAt(p, line, col);
        });
      });
    }
  }
  const varsEl = $('#debug-variables');
  if (varsEl) {
    const vars = st.variables || [];
    varsEl.innerHTML = vars.length
      ? vars.map((v) => `<div class="ij-debug-var"><span class="ij-debug-var-name">${escapeHtml(v.name)}</span><span class="ij-debug-var-value">${escapeHtml(v.value || '')}</span></div>`).join('')
      : '<div class="ij-debug-var"><span class="ij-debug-var-name" style="color:var(--ij-text-muted)">—</span></div>';
  }
  renderDebugWatchList();
  renderDebugBreakpointsList();
  highlightDebugCurrentLine();
  syncDebugToolbar();
}

function renderDebugWatchList() {
  const el = $('#debug-watch-list');
  if (!el) return;
  if (!state.debugWatch.length) {
    el.innerHTML = '<div class="ij-debug-watch-item"><span style="color:var(--ij-text-muted)">Add an expression</span></div>';
    return;
  }
  el.innerHTML = state.debugWatch.map((w, i) => `
    <div class="ij-debug-watch-item" data-watch-idx="${i}">
      <span class="ij-debug-var-name">${escapeHtml(w.expr)}</span>
      <span class="ij-debug-var-value">${escapeHtml(w.value ?? '…')}</span>
    </div>`).join('');
}

function renderDebugBreakpointsList() {
  const el = $('#debug-breakpoints');
  if (!el) return;
  compactDebugBreakpointsMap();
  const bps = allDebugBreakpointsList();
  if (!bps.length) {
    el.innerHTML = '<div class="ij-debug-bp-item"><span style="color:var(--ij-text-muted)">Click gutter or press F9</span></div>';
    return;
  }
  el.innerHTML = bps.map((bp) => `
    <button type="button" class="ij-debug-bp-item" data-path="${escapeHtml(bp.path)}" data-line="${bp.line}">
      <span class="ij-debug-bp-line">${bp.line}</span>
      <span class="ij-debug-bp-path">${escapeHtml(bp.path)}</span>
    </button>`).join('');
  el.querySelectorAll('.ij-debug-bp-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      const p = btn.dataset.path;
      const line = Number(btn.dataset.line);
      if (p && line) void openFileAt(p, line, 1);
    });
  });
}

function renderBreakpointGlyphs() {
  if (!state.editor) return;
  const path = state.activeTab;
  const lines = debugBreakpointsForPath(path);
  const decos = lines.map((line) => ({
    range: new monaco.Range(line, 1, line, 1),
    options: {
      isWholeLine: false,
      glyphMarginClassName: 'ij-debug-glyph',
    },
  }));
  state.debugDecorationIds = state.editor.deltaDecorations(state.debugDecorationIds, decos);
}

/** Map DAP absolute paths to workspace-relative tab paths. */
function debugFrameWorkspacePath(path) {
  if (!path) return null;
  const viaTerminal = resolveTerminalFilePath(path);
  if (viaTerminal) return viaTerminal;
  const normalized = workspaceExplorerPath(path);
  if (!normalized || isAbsoluteRepoPath(normalized)) return null;
  return normalized;
}

function highlightDebugCurrentLine() {
  if (!state.editor || state.debugState?.status !== 'stopped') {
    if (state.debugCurrentLineId != null) {
      state.debugCurrentLineId = state.editor?.deltaDecorations(state.debugCurrentLineId || [], []) || [];
    }
    return;
  }
  const frame = state.debugState.frames?.[0];
  if (!frame?.line) return;

  const frameRel = debugFrameWorkspacePath(frame.path);
  const activeRel = workspaceExplorerPath(state.activeTab);
  if (frameRel && activeRel && frameRel !== activeRel) {
    // Open the paused file so the highlight is visible.
    void openFileAt(frameRel, frame.line, frame.column || 1).then(() => {
      // Re-apply after the tab switch finishes.
      if (state.debugState?.status === 'stopped') highlightDebugCurrentLine();
    });
    return;
  }

  state.debugCurrentLineId = state.editor.deltaDecorations(state.debugCurrentLineId || [], [{
    range: new monaco.Range(frame.line, 1, frame.line, Number.MAX_SAFE_INTEGER),
    options: {
      isWholeLine: true,
      className: 'ij-debug-current-line',
      linesDecorationsClassName: 'ij-debug-current-line-margin',
      overviewRuler: {
        color: '#c792ea',
        position: monaco.editor.OverviewRulerLane.Full,
      },
    },
  }]);
  state.editor.revealLineInCenter(frame.line);
}

async function syncBreakpointsToServer() {
  if (!state.repo) return;
  try {
    const res = await api(repoApi(state.repo, '/workspace/debug/breakpoints'), {
      method: 'POST',
      body: JSON.stringify({ breakpoints: allDebugBreakpointsList() }),
    });
    if (res) state.debugState = { ...state.debugState, ...res };
  } catch { /* ignore when no session */ }
}

function applyDebugState(st) {
  if (!st) return;
  const wasStopped = state.debugState?.status === 'stopped';
  state.debugState = st;
  state.debugActive = st.status && st.status !== 'idle' && st.status !== 'terminated';
  if (st.status === 'terminated' || st.status === 'idle') {
    state.debugActive = false;
    state._debugStepping = false;
    state._debugStarting = false;
  }
  if (state.debugActive || state.debugPanelOpen) showDebugPanel();
  renderDebugPanel();
  syncDebugToolbar();
  highlightDebugCurrentLine();
  if (st.status === 'stopped') {
    state._debugStepping = false;
    if (!wasStopped) void refreshDebugWatchValues();
  }
}

function disconnectDebugWs() {
  if (!state.debugWs) return;
  try { state.debugWs.close(); } catch { /* ignore */ }
  state.debugWs = null;
}

async function connectDebugWs() {
  if (!state.repo) return;
  const base = await ensureLoopbackWsBase();
  if (!base) return;
  disconnectDebugWs();
  const url = `${base}${repoApi(state.repo, '/workspace/debug/ws')}`;
  const ws = new WebSocket(url);
  state.debugWs = ws;
  ws.onmessage = (ev) => {
    try {
      const msg = JSON.parse(ev.data);
      if (msg.t === 'state' && msg.state) {
        applyDebugState(msg.state);
      }
      else if (msg.t === 'output' && msg.text) terminalLog(msg.text);
      else if (msg.t === 'message' && msg.text) toast(msg.text, 'info');
    } catch { /* ignore */ }
  };
  ws.onclose = () => {
    if (state.debugWs === ws) state.debugWs = null;
  };
  // Wait briefly for the socket to open so we don't miss the first stopped event.
  await new Promise((resolve) => {
    if (ws.readyState === WebSocket.OPEN) {
      resolve();
      return;
    }
    const t = setTimeout(resolve, 1500);
    ws.addEventListener('open', () => {
      clearTimeout(t);
      resolve();
    }, { once: true });
    ws.addEventListener('error', () => {
      clearTimeout(t);
      resolve();
    }, { once: true });
  });
}

async function startDebugSession() {
  if (!state.repo || !state.activeTab) {
    toast('Open a Java file first', 'info');
    return;
  }
  const path = state.activeTab;
  const pathLooksJava = /\.(java|kt)$/i.test(path || '');
  if (!isDebuggablePath(path) && !pathLooksJava) {
    toast(state.debugCapabilities?.reason || 'Debugging not available for this file', 'warning');
    return;
  }
  if (state._debugStarting) {
    toast('Debug start already in progress…', 'info');
    return;
  }
  state._debugStarting = true;
  const content = state.tabContents.get(path) ?? state.editor?.getValue?.() ?? '';
  const line = state.editor?.getPosition?.()?.lineNumber || 1;
  showDebugPanel();
  const restarting = !!state.debugActive
    || state.debugState?.status === 'terminated'
    || state.debugState?.status === 'stopped'
    || state.debugState?.status === 'running';
  toast(restarting ? 'Restarting debug session…' : 'Starting debug session…', 'info');
  const unlockTimer = setTimeout(() => {
    if (state._debugStarting) {
      state._debugStarting = false;
      toast('Debug start timed out — try again', 'error');
      terminalLogError('Debug start timed out — try again', { label: 'debug' });
      syncDebugToolbar();
    }
  }, 560_000);
  try {
    disconnectDebugWs();
    try {
      await api(repoApi(state.repo, '/workspace/debug/stop'), {
        method: 'POST',
        timeoutMs: 8_000,
      });
    } catch {
      /* start will also stop leftovers */
    }
    state.debugActive = false;
    state._debugStepping = false;
    state.debugState = {
      status: 'starting',
      frames: [],
      variables: [],
      breakpoints: state.debugState?.breakpoints || [],
    };
    highlightDebugCurrentLine();
    syncDebugToolbar();
    if (restarting) await new Promise((r) => setTimeout(r, 1800));

    await syncBreakpointsToServer();
    await connectDebugWs();
    let st;
    try {
      st = await api(repoApi(state.repo, '/workspace/debug/start'), {
        method: 'POST',
        body: JSON.stringify({ path, content, line }),
        timeoutMs: 540_000,
      });
    } catch (e) {
      // One automatic retry for the common "second start" Java DAP race.
      if (restarting) {
        toast('Retrying debug start…', 'info');
        await new Promise((r) => setTimeout(r, 1500));
        st = await api(repoApi(state.repo, '/workspace/debug/start'), {
          method: 'POST',
          body: JSON.stringify({ path, content, line }),
          timeoutMs: 540_000,
        });
      } else {
        throw e;
      }
    }
    applyDebugState(st);
    if (st.status === 'running' || st.status === 'starting') {
      for (let i = 0; i < 30; i++) {
        await new Promise((r) => setTimeout(r, 100));
        try {
          const cur = await api(repoApi(state.repo, '/workspace/debug/state'), {
            timeoutMs: 5_000,
          });
          applyDebugState(cur);
          if (cur.status === 'stopped' || cur.status === 'terminated' || cur.status === 'idle') break;
        } catch {
          break;
        }
      }
    }
    showTerminal();
    const status = state.debugState?.status;
    if (status === 'stopped') {
      toast('Paused — use Step Over (F10) for line-by-line', 'success');
    } else if (status === 'running') {
      toast('Running — set a breakpoint and restart, or wait for hit', 'info');
    } else {
      toast(`Debugging ${st.language || 'program'}`, 'success');
    }
  } catch (e) {
    state.debugActive = false;
    state.debugState = { status: 'idle', frames: [], variables: [], breakpoints: [] };
    syncDebugToolbar();
    const errMsg = e.message || 'Debug start failed';
    toast(errMsg, 'error');
    terminalLogError(errMsg, { label: 'debug' });
  } finally {
    clearTimeout(unlockTimer);
    state._debugStarting = false;
  }
}

async function stopDebugSession() {
  if (!state.repo) return;
  try {
    const st = await api(repoApi(state.repo, '/workspace/debug/stop'), { method: 'POST' });
    applyDebugState(st);
    disconnectDebugWs();
    highlightDebugCurrentLine();
  } catch (e) {
    const errMsg = e.message || 'Stop failed';
    toast(errMsg, 'error');
    terminalLogError(errMsg, { label: 'debug' });
  }
}

async function debugContinue() {
  if (!state.repo) return;
  if (state.debugState?.status !== 'stopped') return;
  state._debugStepping = true;
  try {
    const st = await api(repoApi(state.repo, '/workspace/debug/continue'), {
      method: 'POST',
      timeoutMs: 15_000,
    });
    applyDebugState(st);
  } catch (e) {
    const errMsg = e.message || 'Continue failed';
    toast(errMsg, 'error');
    terminalLogError(errMsg, { label: 'debug' });
    state._debugStepping = false;
  }
}

async function debugStep(kind) {
  if (!state.repo) return;
  if (state._debugStepping) return;
  if (state.debugState?.status !== 'stopped') return;
  state._debugStepping = true;
  try {
    const st = await api(repoApi(state.repo, '/workspace/debug/step'), {
      method: 'POST',
      body: JSON.stringify({ kind }),
      timeoutMs: 8_000,
    });
    applyDebugState(st);
  } catch (e) {
    const errMsg = e.message || 'Step failed';
    toast(errMsg, 'error');
    terminalLogError(errMsg, { label: 'debug' });
    state._debugStepping = false;
  }
}

async function addDebugWatch(expr) {
  const trimmed = (expr || '').trim();
  if (!trimmed || !state.repo) return;
  const entry = { expr: trimmed, value: '…' };
  state.debugWatch.push(entry);
  renderDebugWatchList();
  if (state.debugState?.status === 'stopped') {
    try {
      const res = await api(repoApi(state.repo, '/workspace/debug/evaluate'), {
        method: 'POST',
        body: JSON.stringify({ expression: trimmed }),
      });
      entry.value = res?.value ?? '';
      renderDebugWatchList();
    } catch (e) {
      entry.value = e.message || 'error';
      renderDebugWatchList();
    }
  }
}

async function refreshDebugWatchValues() {
  if (!state.repo || state.debugState?.status !== 'stopped') return;
  for (const w of state.debugWatch) {
    try {
      const res = await api(repoApi(state.repo, '/workspace/debug/evaluate'), {
        method: 'POST',
        body: JSON.stringify({ expression: w.expr }),
      });
      w.value = res?.value ?? '';
    } catch (e) {
      w.value = e.message || 'error';
    }
  }
  renderDebugWatchList();
}

function debugHoverWordAt(model, position) {
  if (!model || !position) return null;
  const word = model.getWordAtPosition(position);
  if (!word?.word) return null;
  // Skip tiny / non-identifier tokens.
  if (!/^[A-Za-z_$][\w$]*$/.test(word.word)) return null;
  return word;
}

function debugLocalValue(name) {
  const vars = state.debugState?.variables;
  if (!Array.isArray(vars) || !name) return null;
  const exact = vars.find((v) => v?.name === name);
  if (exact && exact.value != null && exact.value !== '') return String(exact.value);
  // Java "Fields" scope sometimes prefixes; also match trailing simple name.
  const loose = vars.find((v) => {
    const n = v?.name || '';
    return n === name || n.endsWith('.' + name) || n.endsWith(' ' + name);
  });
  if (loose && loose.value != null && loose.value !== '') return String(loose.value);
  return null;
}

/** Java DAP refs like `String[0]@8` — empty arrays become `[]`. */
function parseJavaArrayRefLen(value) {
  const m = String(value || '').trim().match(/^[\w.$]+\[(\d+)\]@[0-9a-fA-F]+$/);
  return m ? Number(m[1]) : null;
}

async function prettyJavaHoverValue(expression, value) {
  if (value == null || value === '') return value;
  const len = parseJavaArrayRefLen(value);
  if (len == null) return value;
  if (len === 0) return '[]';
  if (len > 64) return `${value} (len=${len})`;
  const pretty = await dapEvaluateExpression(`java.util.Arrays.toString(${expression})`);
  return pretty || value;
}

/** Methods we must not evaluate on hover (side effects / control flow). */
const DEBUG_HOVER_METHOD_DENY = new Set([
  'print', 'println', 'printf', 'write', 'writeln', 'flush', 'close',
  'exit', 'halt', 'destroy', 'notify', 'notifyall', 'wait',
  'start', 'stop', 'interrupt', 'join', 'execute', 'submit',
  'accept', 'foreach', 'run', 'call', 'invoke',
]);

function debugHoverMethodDenied(name) {
  if (!name) return true;
  const lower = name.toLowerCase();
  if (DEBUG_HOVER_METHOD_DENY.has(lower)) return true;
  return /^(set|add|remove|put|clear|delete|insert|update|save|send|fire|offer|push|poll)/i.test(name);
}

function findMatchingParen(line, openIdx) {
  let depth = 0;
  for (let i = openIdx; i < line.length; i++) {
    const ch = line[i];
    if (ch === '(') depth++;
    else if (ch === ')') {
      depth--;
      if (depth === 0) return i;
    } else if (ch === '"' || ch === "'") {
      const q = ch;
      i++;
      while (i < line.length && line[i] !== q) {
        if (line[i] === '\\') i++;
        i++;
      }
    }
  }
  return -1;
}

/** Walk left from `from` (inclusive last char of receiver) to expression start. */
function debugReceiverStart(line, from) {
  let i = from;
  while (i >= 0) {
    while (i >= 0 && /\s/.test(line[i])) i--;
    if (i < 0) return 0;
    if (line[i] === ')') {
      let depth = 0;
      for (; i >= 0; i--) {
        if (line[i] === ')') depth++;
        else if (line[i] === '(') {
          depth--;
          if (depth === 0) {
            i--;
            break;
          }
        } else if (line[i] === '"' || line[i] === "'") {
          const q = line[i];
          i--;
          while (i >= 0 && line[i] !== q) {
            if (i > 0 && line[i - 1] === '\\') i--;
            i--;
          }
        }
      }
      continue;
    }
    if (/[\w$]/.test(line[i])) {
      while (i >= 0 && /[\w$]/.test(line[i])) i--;
      let j = i;
      while (j >= 0 && /\s/.test(line[j])) j--;
      if (j >= 0 && line[j] === '.') {
        i = j - 1;
        continue;
      }
      return i + 1;
    }
    break;
  }
  return i + 1;
}

/**
 * Resolve what to evaluate under the cursor: a local/field name, or a full
 * call like `file.exists()` when hovering the method name.
 */
function debugHoverExpressionAt(model, position) {
  const word = debugHoverWordAt(model, position);
  if (!word) return null;
  const line = model.getLineContent(position.lineNumber);
  const methodStart = word.startColumn - 1;
  const methodEnd = word.endColumn - 1;

  // Ignore words inside string/char literals (e.g. "file exists: " on a println line).
  if (debugHoverIndexInJavaString(line, methodStart)) {
    const call = debugHoverFindCallNamed(line, word.word);
    if (call) return call;
    return null;
  }

  let k = methodEnd;
  while (k < line.length && /\s/.test(line[k])) k++;
  if (line[k] !== '(') {
    // Bare name — if the same line has `recv.name(...)`, prefer evaluating the call
    // (common when occurrence-highlight lands on a non-call token).
    const call = debugHoverFindCallNamed(line, word.word);
    if (call && call.expr.includes('.')) return call;
    return { expr: word.word, label: word.word, kind: 'ident' };
  }

  if (debugHoverMethodDenied(word.word)) {
    return { expr: word.word, label: word.word, kind: 'ident' };
  }

  const close = findMatchingParen(line, k);
  if (close < 0) return { expr: word.word, label: word.word, kind: 'ident' };

  let start = methodStart;
  let j = methodStart - 1;
  while (j >= 0 && /\s/.test(line[j])) j--;
  if (j >= 0 && line[j] === '.') {
    start = debugReceiverStart(line, j - 1);
  }

  const expr = line.slice(start, close + 1).replace(/\s+/g, ' ').trim();
  if (!expr) return { expr: word.word, label: word.word, kind: 'ident' };
  return { expr, label: expr, kind: 'call' };
}

/** True when `idx` lies inside a Java string or char literal on `line`. */
function debugHoverIndexInJavaString(line, idx) {
  let inStr = false;
  let quote = '';
  for (let i = 0; i < line.length && i <= idx; i++) {
    const ch = line[i];
    if (inStr) {
      if (ch === '\\') {
        i++;
        continue;
      }
      if (ch === quote) inStr = false;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inStr = true;
      quote = ch;
    }
  }
  return inStr;
}

/** Find `recv.name(...)` on the line (skips string literals). */
function debugHoverFindCallNamed(line, name) {
  if (!name || debugHoverMethodDenied(name)) return null;
  const re = new RegExp(`\\b${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\(`);
  let searchFrom = 0;
  while (searchFrom < line.length) {
    const slice = line.slice(searchFrom);
    const m = slice.match(re);
    if (!m || m.index == null) return null;
    const methodStart = searchFrom + m.index;
    if (debugHoverIndexInJavaString(line, methodStart)) {
      searchFrom = methodStart + name.length;
      continue;
    }
    const open = methodStart + m[0].length - 1;
    const close = findMatchingParen(line, open);
    if (close < 0) return null;
    let start = methodStart;
    let j = methodStart - 1;
    while (j >= 0 && /\s/.test(line[j])) j--;
    if (j >= 0 && line[j] === '.') {
      start = debugReceiverStart(line, j - 1);
    }
    const expr = line.slice(start, close + 1).replace(/\s+/g, ' ').trim();
    if (!expr) return null;
    return { expr, label: expr, kind: 'call' };
  }
  return null;
}

async function dapEvaluateExpression(expression) {
  if (!state.repo || state.debugState?.status !== 'stopped' || state._debugStepping) return null;
  const frameId = state.debugState?.frames?.[0]?.id;
  const tryEval = async (expr, context) => {
    const res = await api(repoApi(state.repo, '/workspace/debug/evaluate'), {
      method: 'POST',
      body: JSON.stringify({
        expression: expr,
        context,
        frame_id: frameId,
      }),
      timeoutMs: 4_000,
    });
    const value = res?.value;
    if (value == null || value === '') return null;
    return String(value);
  };
  try {
    return (
      (await tryEval(expression, 'watch'))
      || (await tryEval(expression, 'hover'))
    );
  } catch {
    return null;
  }
}

async function evaluateDebugHoverInfo(model, position) {
  if (state.debugState?.status !== 'stopped') return null;
  const info = debugHoverExpressionAt(model, position);
  if (!info) return null;

  if (info.kind === 'ident') {
    let value = debugLocalValue(info.expr);
    if (value == null) {
      value = await dapEvaluateExpression(info.expr)
        || await dapEvaluateExpression(`this.${info.expr}`);
    }
    if (value == null) return null;
    value = await prettyJavaHoverValue(info.expr, value);
    return { label: info.label, value };
  }

  // Method / call expression — evaluate the full call once.
  let evaluated = await dapEvaluateExpression(info.expr);
  if (evaluated == null) return null;
  evaluated = await prettyJavaHoverValue(info.expr, evaluated);
  return { label: info.label, value: evaluated };
}

/** Used by monaco-languages hover so the value appears above javadoc (once). */
async function lookupDebugHoverValue(model, position) {
  return evaluateDebugHoverInfo(model, position);
}

function setupDebugEditorHooks(editor) {
  if (!editor || editor.__reaperDebugHooks) return;
  editor.__reaperDebugHooks = true;
  editor.onMouseDown((e) => {
    if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
    if (!state.activeTab || !isDebuggablePath(state.activeTab)) return;
    const line = e.target.position?.lineNumber;
    if (line) toggleBreakpoint(state.activeTab, line);
  });
  // Debug values are injected once via ReaperLang hover (lookupDebugHoverValue).
  // Do not register a second HoverProvider here — that duplicated `name = value`.
}

async function openCoverageHtmlReport() {
  const path = state.coverageReport?.html_report_path;
  if (!path || !state.repo) {
    toast('HTML report not found — run tests with coverage first', 'info');
    return;
  }
  try {
    await api(repoApi(state.repo, '/workspace/open-external'), {
      method: 'POST',
      body: JSON.stringify({ path }),
    });
  } catch (e) {
    toast(e.message || 'Could not open report', 'error');
  }
}

function applyDbViewerPanelLayout() {
  const dock = $('#db-viewer-dock-right');
  const resizer = $('#db-viewer-right-resizer');
  const open = state.dbViewerPanelOpen;
  dock?.classList.toggle('hidden', !open);
  resizer?.classList.toggle('hidden', !open);
}

function showDbViewerPanel() {
  state.dbViewerPanelOpen = true;
  applyDbViewerPanelLayout();
  syncDbSqlFromActiveTab();
  void refreshDbViewerPanel();
}

function hideDbViewerPanel() {
  state.dbViewerPanelOpen = false;
  applyDbViewerPanelLayout();
}

function toggleDbViewerPanel() {
  if (state.dbViewerPanelOpen) hideDbViewerPanel();
  else showDbViewerPanel();
}

const GIT_VIEWER_QUICK = [
  { label: 'Status', cmd: 'status --short --branch' },
  { label: 'Log', cmd: 'log --oneline -20' },
  { label: 'Branches', cmd: 'branch -vv' },
  { label: 'Diff', cmd: 'diff' },
  { label: 'Staged', cmd: 'diff --staged' },
  { label: 'Remote', cmd: 'remote -v' },
  { label: 'Stash', cmd: 'stash list' },
  { label: 'Tags', cmd: 'tag -l' },
];

const GIT_MUTATING_SUBCOMMANDS = new Set([
  'add', 'commit', 'checkout', 'pull', 'push', 'fetch', 'merge', 'rebase', 'cherry-pick', 'stash',
  'reset', 'switch', 'restore', 'clean', 'mv', 'rm',
]);

function applyGitViewerPanelLayout() {
  const dock = $('#git-viewer-dock-right');
  const resizer = $('#git-viewer-right-resizer');
  const open = state.gitViewerPanelOpen;
  dock?.classList.toggle('hidden', !open);
  resizer?.classList.toggle('hidden', !open);
}

function showGitViewerPanel() {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  state.gitViewerPanelOpen = true;
  applyGitViewerPanelLayout();
  renderGitViewerQuickCommands();
  void refreshGitViewerPanel();
}

function hideGitViewerPanel() {
  state.gitViewerPanelOpen = false;
  applyGitViewerPanelLayout();
}

function toggleGitViewerPanel() {
  if (state.gitViewerPanelOpen) hideGitViewerPanel();
  else showGitViewerPanel();
}

function renderGitViewerQuickCommands() {
  const bar = $('#git-viewer-quick');
  if (!bar) return;
  bar.innerHTML = GIT_VIEWER_QUICK.map(({ label, cmd }) =>
    `<button type="button" class="ij-git-viewer-quick-btn" data-git-cmd="${escapeHtml(cmd)}" title="git ${escapeHtml(cmd)}">${escapeHtml(label)}</button>`,
  ).join('');
  bar.querySelectorAll('.ij-git-viewer-quick-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const cmd = btn.dataset.gitCmd;
      const input = $('#git-viewer-command');
      if (input && cmd) input.value = cmd;
      void runGitViewerCommand(cmd);
    });
  });
}

function splitShellArgs(text) {
  const args = [];
  let cur = '';
  let quote = null;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    if (quote) {
      if (ch === quote) quote = null;
      else cur += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (/\s/.test(ch)) {
      if (cur) {
        args.push(cur);
        cur = '';
      }
    } else {
      cur += ch;
    }
  }
  if (cur) args.push(cur);
  return args;
}

function parseGitViewerArgs(input) {
  const trimmed = String(input || '').trim();
  if (!trimmed) return [];
  return splitShellArgs(trimmed.replace(/^git\s+/i, ''));
}

function renderGitViewerOutput(result) {
  const outEl = $('#git-viewer-output');
  const metaEl = $('#git-viewer-output-meta');
  if (!outEl) return;
  state.gitViewerLastResult = result;
  if (!result) {
    outEl.textContent = '';
    if (metaEl) metaEl.textContent = '';
    return;
  }
  if (result.error && !result.stdout && !result.stderr) {
    outEl.textContent = result.error;
    if (metaEl) metaEl.textContent = 'error';
    outEl.classList.add('is-error');
    return;
  }
  outEl.classList.remove('is-error');
  const chunks = [];
  if (result.stdout) chunks.push(result.stdout.replace(/\n$/, ''));
  if (result.stderr) {
    if (chunks.length) chunks.push('');
    chunks.push(result.stderr.replace(/\n$/, ''));
  }
  outEl.textContent = chunks.join('\n') || '(no output)';
  if (metaEl) {
    const code = result.exit_code ?? result.exitCode ?? 0;
    const ms = result.elapsed_ms != null ? ` · ${result.elapsed_ms} ms` : '';
    metaEl.textContent = `exit ${code}${ms}`;
    metaEl.classList.toggle('is-error', code !== 0);
  }
}

async function refreshGitViewerSubtitle() {
  const subtitle = $('#git-viewer-subtitle');
  if (!subtitle || !state.repo) return;
  try {
    const status = await api(repoApi(state.repo, '/workspace/status'));
    const branch = status?.branch || 'unknown';
    const dirty = status?.clean === false ? ' · modified' : '';
    const ahead = status?.ahead > 0 ? ` · ↑${status.ahead}` : '';
    const behind = status?.behind > 0 ? ` · ↓${status.behind}` : '';
    subtitle.textContent = `${branch}${dirty}${ahead}${behind}`;
  } catch {
    subtitle.textContent = '';
  }
}

async function refreshGitViewerPanel() {
  await refreshGitViewerSubtitle();
  const saved = state.repo && localStorage.getItem(`reaper-git-cmd-${state.repo}`);
  const input = $('#git-viewer-command');
  if (input && saved && !input.value.trim()) input.value = saved;
  if (!state.gitViewerLastResult) {
    await runGitViewerCommand('status --short --branch');
  }
}

async function runGitViewerCommand(commandText) {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  const args = parseGitViewerArgs(commandText ?? $('#git-viewer-command')?.value);
  if (!args.length) {
    toast('Enter a git command', 'info');
    return;
  }
  const input = $('#git-viewer-command');
  const displayCmd = args.join(' ');
  if (input) input.value = displayCmd;
  localStorage.setItem(`reaper-git-cmd-${state.repo}`, displayCmd);
  const outEl = $('#git-viewer-output');
  const metaEl = $('#git-viewer-output-meta');
  const runBtn = $('#btn-git-viewer-run');
  if (outEl) outEl.textContent = 'Running…';
  if (metaEl) metaEl.textContent = '';
  if (runBtn) runBtn.disabled = true;
  const started = performance.now();
  try {
    const result = await api(repoApi(state.repo, '/workspace/git'), {
      method: 'POST',
      body: JSON.stringify({ args }),
    });
    renderGitViewerOutput({ ...result, elapsed_ms: Math.round(performance.now() - started) });
    if (args[0] && GIT_MUTATING_SUBCOMMANDS.has(args[0]) && (result.exit_code ?? 0) === 0) {
      await refreshGitStatus();
      await refreshTree();
      await refreshGitViewerSubtitle();
    }
  } catch (e) {
    renderGitViewerOutput({ error: e.message || 'Git command failed', exit_code: 1 });
    toast(e.message || 'Git command failed', 'error');
  } finally {
    if (runBtn) runBtn.disabled = false;
  }
}

function initGitViewerResize() {
  const savedW = localStorage.getItem(GIT_VIEWER_RIGHT_WIDTH_KEY);
  if (savedW) document.documentElement.style.setProperty('--ij-git-viewer-right-w', savedW);
  bindSideDockResize({
    rightResizer: '#git-viewer-right-resizer',
    widthVar: '--ij-git-viewer-right-w',
    storageKey: GIT_VIEWER_RIGHT_WIDTH_KEY,
    min: 320,
    max: Math.min(window.innerWidth * 0.55, 720),
  });
}

const DOCKER_CONSOLE_QUICK = [
  { id: 'up', label: 'Up', cmd: 'compose up -d', compose: true, title: 'docker compose up -d' },
  { id: 'down', label: 'Down', cmd: 'compose down', compose: true, title: 'docker compose down' },
  { id: 'ps', label: 'Ps', cmd: 'compose ps', compose: true, title: 'docker compose ps' },
  { id: 'build', label: 'Build', cmd: 'compose build', compose: true, title: 'docker compose build' },
  { id: 'logs', label: 'Follow', cmd: 'compose logs -f --tail=100', compose: true, stream: true, title: 'docker compose logs -f' },
  { id: 'refresh', label: 'Refresh', action: 'refresh', title: 'Refresh container list' },
];

function dockerComposeShell(args) {
  const a = String(args || '').trim();
  return `if docker compose version >/dev/null 2>&1; then docker compose ${a}; `
    + `elif command -v docker-compose >/dev/null 2>&1; then docker-compose ${a}; `
    + `else docker compose ${a}; fi`;
}

function parseDockerConsoleArgs(input) {
  const trimmed = String(input || '').trim();
  if (!trimmed) return [];
  return splitShellArgs(trimmed.replace(/^docker\s+/i, ''));
}

function dockerConsoleCwd() {
  const modulePath = state.dockerLogsModulePath
    || dockerLogsComposeModulePath()
    || findDockerLogsFollowCommand()?.modulePath
    || null;
  if (modulePath) return buildTaskWorkdir(modulePath);
  return undefined;
}

function syncDockerConsoleSubtitle() {
  const modulePath = state.dockerLogsModulePath
    || dockerLogsComposeModulePath()
    || findDockerLogsFollowCommand()?.modulePath
    || null;
  state.dockerLogsModulePath = modulePath;
  const subtitleEl = $('#docker-logs-subtitle');
  if (!subtitleEl) return;
  if (modulePath) {
    const cwd = buildTaskWorkdir(modulePath);
    subtitleEl.textContent = cwd ? `${modulePath} · ${cwd}` : modulePath;
  } else {
    subtitleEl.textContent = 'No compose project detected';
  }
}

function renderDockerConsoleQuick() {
  const bar = $('#docker-console-quick');
  if (!bar) return;
  const hasCompose = !!(state.dockerLogsModulePath
    || dockerLogsComposeModulePath()
    || findDockerLogsFollowCommand());
  bar.innerHTML = DOCKER_CONSOLE_QUICK.map((item) => {
    const disabled = item.compose && !hasCompose ? ' disabled' : '';
    const title = escapeHtml(item.title || item.label);
    return `<button type="button" class="ij-docker-console-quick-btn" data-docker-quick="${escapeHtml(item.id)}" title="${title}"${disabled}>${escapeHtml(item.label)}</button>`;
  }).join('');
  bar.querySelectorAll('.ij-docker-console-quick-btn').forEach((btn) => {
    btn.addEventListener('click', () => void runDockerConsoleQuick(btn.dataset.dockerQuick));
  });
}

async function runDockerConsoleQuick(id) {
  const item = DOCKER_CONSOLE_QUICK.find((q) => q.id === id);
  if (!item) return;
  if (item.action === 'refresh') {
    await refreshDockerContainers();
    return;
  }
  const input = $('#docker-console-command');
  if (input && item.cmd) input.value = item.cmd;
  await runDockerConsoleCommand(item.cmd, { stream: !!item.stream });
}

function setDockerContainerSelection(id) {
  state.dockerSelectedId = id || null;
  $$('.ij-docker-container-row').forEach((row) => {
    row.classList.toggle('is-selected', row.dataset.containerId === state.dockerSelectedId);
  });
  const has = !!state.dockerSelectedId;
  ['btn-docker-container-start', 'btn-docker-container-stop', 'btn-docker-container-restart', 'btn-docker-container-logs']
    .forEach((sid) => {
      const el = $(`#${sid}`);
      if (el) el.disabled = !has;
    });
}

function renderDockerContainersList(containers) {
  const el = $('#docker-containers-list');
  if (!el) return;
  state.dockerContainers = containers || [];
  if (!state.dockerContainers.length) {
    el.innerHTML = '<div class="ij-docker-containers-empty">No containers — run Refresh or compose Up</div>';
    setDockerContainerSelection(null);
    return;
  }
  el.innerHTML = state.dockerContainers.map((c) => {
    const selected = c.id === state.dockerSelectedId ? ' is-selected' : '';
    return `<button type="button" class="ij-docker-container-row${selected}" role="option" data-container-id="${escapeHtml(c.id)}" title="${escapeHtml(c.name)}">
      <span class="ij-docker-container-name">${escapeHtml(c.name)}</span>
      <span class="ij-docker-container-image">${escapeHtml(c.image)}</span>
      <span class="ij-docker-container-status">${escapeHtml(c.status)}</span>
    </button>`;
  }).join('');
  el.querySelectorAll('.ij-docker-container-row').forEach((row) => {
    row.addEventListener('click', () => {
      setDockerContainerSelection(row.dataset.containerId);
      void followSelectedDockerContainerLogs();
    });
  });
  if (state.dockerSelectedId && !state.dockerContainers.some((c) => c.id === state.dockerSelectedId)) {
    setDockerContainerSelection(null);
  } else {
    setDockerContainerSelection(state.dockerSelectedId);
  }
}

function parseDockerPsJsonLines(text) {
  const out = [];
  for (const line of String(text || '').split('\n')) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('{')) continue;
    try {
      const row = JSON.parse(trimmed);
      const id = String(row.ID || row.Id || '').trim();
      if (!id) continue;
      const name = String(row.Names || row.Name || id).replace(/^\//, '').split(',')[0];
      out.push({
        id,
        name,
        image: String(row.Image || ''),
        status: String(row.Status || row.State || ''),
        ports: String(row.Ports || ''),
        state: String(row.State || ''),
      });
    } catch { /* skip bad line */ }
  }
  return out;
}

async function collectShellOutput(command, cwd) {
  const res = await fetch(repoApi(state.repo, '/workspace/shell'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ command, cwd: cwd || undefined }),
  });
  if (!res.ok) {
    let errMsg = res.statusText;
    try {
      const err = await res.json();
      errMsg = err.error || errMsg;
    } catch { /* ignore */ }
    throw new Error(errMsg);
  }
  let text = '';
  const exitCode = await consumeExecStreamToCallback(res, (chunk) => { text += chunk; });
  return { text, exitCode };
}

async function refreshDockerContainers() {
  if (!state.repo) return;
  updateDockerLogsStatus('Listing…');
  try {
    // docker ps is global — do not require compose project cwd
    const { text, exitCode } = await collectShellOutput(
      "docker ps -a --format '{{json .}}'",
      undefined,
    );
    if (exitCode !== 0) {
      renderDockerContainersList([]);
      updateDockerLogsStatus(`docker ps exit ${exitCode}`);
      const meta = $('#docker-console-output-meta');
      if (meta) {
        meta.textContent = `exit ${exitCode}`;
        meta.classList.add('is-error');
      }
      appendDockerLogsText(text || '\n(docker ps failed — is Docker running?)\n');
      return;
    }
    renderDockerContainersList(parseDockerPsJsonLines(text));
    updateDockerLogsStatus(`${state.dockerContainers.length} container(s)`);
  } catch (e) {
    renderDockerContainersList([]);
    updateDockerLogsStatus('Error');
    appendDockerLogsText(`\nerror: ${e.message || e}\n`);
    toast(e.message || 'docker ps failed', 'error');
  }
}

async function runSelectedDockerContainerAction(action) {
  const id = state.dockerSelectedId;
  if (!id) {
    toast('Select a container first', 'info');
    return;
  }
  const name = state.dockerContainers.find((c) => c.id === id)?.name || id;
  if (action === 'logs') {
    await followSelectedDockerContainerLogs();
    return;
  }
  const cmd = `docker ${action} ${shellQuoteDocker(id)}`;
  const input = $('#docker-console-command');
  if (input) input.value = `${action} ${name}`;
  await runDockerConsoleCommand(cmd, { raw: true });
  await refreshDockerContainers();
}

async function followSelectedDockerContainerLogs() {
  const id = state.dockerSelectedId;
  if (!id) return;
  const name = state.dockerContainers.find((c) => c.id === id)?.name || id;
  const input = $('#docker-console-command');
  if (input) input.value = `logs -f --tail=200 ${name}`;
  const qid = shellQuoteDocker(id);
  // Finite history first — WKWebView can buffer open-ended fetch streams until close,
  // so `docker logs -f` alone often shows nothing. Then follow only new lines.
  await startDockerLogsStream(
    `docker logs --tail=200 ${qid} 2>&1`,
    undefined,
    { label: `logs · ${name}`, modulePath: state.dockerLogsModulePath },
  );
  await startDockerLogsStream(
    `docker logs -f --tail=0 ${qid} 2>&1`,
    undefined,
    { label: `logs · ${name}`, modulePath: state.dockerLogsModulePath, append: true },
  );
}

function shellQuoteDocker(value) {
  const s = String(value || '');
  if (/^[A-Za-z0-9._:/-]+$/.test(s)) return s;
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

function setDockerConsoleOutputMeta(text, { error = false } = {}) {
  const meta = $('#docker-console-output-meta');
  if (!meta) return;
  meta.textContent = text || '';
  meta.classList.toggle('is-error', !!error);
}

async function runDockerConsoleCommand(commandText, { stream = false, raw = false } = {}) {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  let shellCmd;
  let display;
  if (raw) {
    shellCmd = String(commandText || '').trim();
    display = shellCmd.replace(/^docker\s+/i, '');
  } else {
    const args = parseDockerConsoleArgs(commandText ?? $('#docker-console-command')?.value);
    if (!args.length) {
      toast('Enter a docker command', 'info');
      return;
    }
    display = args.join(' ');
    const input = $('#docker-console-command');
    if (input) input.value = display;
    localStorage.setItem(`reaper-docker-cmd-${state.repo}`, display);
    // Prefer compose wrapper when first token is "compose"
    if (args[0] === 'compose') {
      shellCmd = dockerComposeShell(args.slice(1).join(' '));
    } else {
      shellCmd = `docker ${display}`;
    }
    stream = stream || /\blogs\b/.test(display) && /(^|\s)-f(\s|$)/.test(` ${display} `);
  }
  if (!shellCmd) return;

  syncDockerConsoleSubtitle();
  // Compose needs the project directory; plain docker (ps/logs/start/…) does not.
  const needsComposeCwd = raw
    ? /\bcompose\b/.test(shellCmd)
    : (parseDockerConsoleArgs(display)[0] === 'compose');
  const cwd = needsComposeCwd ? dockerConsoleCwd() : undefined;
  const runBtn = $('#btn-docker-console-run');
  if (runBtn) runBtn.disabled = true;

  if (stream) {
    try {
      const streamCmd = needsComposeCwd || /\s2>&1\s*$/.test(shellCmd)
        ? shellCmd
        : `${shellCmd} 2>&1`;
      const isFollow = /\blogs\b/.test(display || shellCmd)
        && /(^|\s)-f(\s|$)|--follow/.test(` ${display || shellCmd} `);
      if (isFollow && needsComposeCwd) {
        // History then follow — same WKWebView buffering workaround as container logs.
        await startDockerLogsStream(
          dockerComposeShell('logs --tail=200'),
          cwd,
          { label: display || 'docker', modulePath: state.dockerLogsModulePath },
        );
        await startDockerLogsStream(
          `${dockerComposeShell('logs -f --tail=0')} 2>&1`,
          cwd,
          { label: display || 'docker', modulePath: state.dockerLogsModulePath, append: true },
        );
      } else {
        await startDockerLogsStream(streamCmd, cwd, {
          label: display || 'docker',
          modulePath: state.dockerLogsModulePath,
        });
      }
    } finally {
      if (runBtn) runBtn.disabled = false;
    }
    return;
  }

  await stopDockerLogsStream();
  clearDockerLogsOutput();
  setDockerLogsControlsStreaming(true);
  updateDockerLogsStatus('Running…', { live: true });
  setDockerConsoleOutputMeta('');
  const started = performance.now();
  const ac = new AbortController();
  state.dockerLogsAbortController = ac;
  try {
    const res = await fetch(repoApi(state.repo, '/workspace/shell'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ command: shellCmd, cwd: cwd || undefined }),
      signal: ac.signal,
    });
    if (!res.ok) {
      let errMsg = res.statusText;
      try {
        const err = await res.json();
        errMsg = err.error || errMsg;
      } catch { /* ignore */ }
      throw new Error(errMsg);
    }
    const exitCode = await consumeExecStreamToCallback(res, appendDockerLogsText);
    const ms = Math.round(performance.now() - started);
    setDockerConsoleOutputMeta(`exit ${exitCode} · ${ms} ms`, { error: exitCode !== 0 });
    updateDockerLogsStatus(exitCode === 0 ? 'Done' : `Exited ${exitCode}`);
    if (/\b(up|down|start|stop|restart|rm|kill)\b/.test(display || shellCmd)) {
      await refreshDockerContainers();
    }
  } catch (e) {
    if (e?.name === 'AbortError') {
      updateDockerLogsStatus('Stopped');
      setDockerConsoleOutputMeta('stopped');
    } else {
      appendDockerLogsText(`\nerror: ${e.message || e}\n`);
      updateDockerLogsStatus('Error');
      setDockerConsoleOutputMeta('error', { error: true });
      toast(e.message || 'Docker command failed', 'error');
    }
  } finally {
    if (state.dockerLogsAbortController === ac) {
      state.dockerLogsAbortController = null;
      setDockerLogsControlsStreaming(false);
    }
    if (runBtn) runBtn.disabled = false;
  }
}

function isDockerLogsBuildTask(buildTool, taskCommand) {
  return buildTool === 'docker' && /\blogs\b/.test(String(taskCommand || ''));
}

function dockerLogsComposeModulePath() {
  const tree = state.buildTasksTree;
  if (tree?.build_tool !== 'docker') return null;
  const walk = (node) => {
    if (node?.path) return node.path;
    for (const child of node?.children || []) {
      const found = walk(child);
      if (found) return found;
    }
    return null;
  };
  return walk(tree.tree) || state.activeTab;
}

function findDockerLogsFollowCommand() {
  const tree = state.buildTasksTree?.tree;
  if (!tree) return null;
  const walk = (node) => {
    for (const task of node.tasks || []) {
      if (task.id === 'logs-follow' || (task.group === 'logs' && /\blogs\s+-f\b/.test(task.command))) {
        return { modulePath: node.path, command: task.command, label: task.label };
      }
    }
    for (const child of node.children || []) {
      const found = walk(child);
      if (found) return found;
    }
    return null;
  };
  return walk(tree);
}

function setDockerLogsMeta(label, modulePath) {
  state.dockerLogsLabel = label || 'Docker';
  state.dockerLogsModulePath = modulePath || null;
  const titleEl = $('#docker-logs-title');
  const subtitleEl = $('#docker-logs-subtitle');
  if (titleEl) titleEl.textContent = 'Docker';
  if (subtitleEl) {
    const cwd = modulePath ? buildTaskWorkdir(modulePath) : '';
    subtitleEl.textContent = cwd ? `${modulePath} · ${cwd || '.'}` : (modulePath || state.dockerLogsLabel || '');
  }
}

function updateDockerLogsStatus(text, { live = false } = {}) {
  const el = $('#docker-logs-status');
  if (!el) return;
  el.textContent = text || '';
  el.classList.toggle('is-live', !!live);
}

function appendDockerLogsText(text) {
  const out = $('#docker-logs-output');
  if (!out || !text) return;
  out.classList.remove('is-waiting');
  out.textContent += text;
  if (state.dockerLogsAutoScroll) {
    out.scrollTop = out.scrollHeight;
  }
}

function clearDockerLogsOutput() {
  const out = $('#docker-logs-output');
  if (out) {
    out.textContent = '';
    out.classList.remove('is-waiting');
  }
}

function setDockerLogsControlsStreaming(streaming) {
  state.dockerLogsStreaming = streaming;
  const stopBtn = $('#btn-docker-logs-stop');
  const runBtn = $('#btn-docker-console-run');
  if (stopBtn) stopBtn.disabled = !streaming;
  if (runBtn) runBtn.disabled = streaming;
}

async function stopDockerLogsStream() {
  const ac = state.dockerLogsAbortController;
  state.dockerLogsAbortController = null;
  try { state.dockerLogsXhr?.abort(); } catch { /* ignore */ }
  state.dockerLogsXhr = null;
  ac?.abort();
  if (state.repo && state.dockerLogsStreaming) {
    try {
      await fetch(repoApi(state.repo, '/workspace/exec/cancel'), { method: 'POST' });
    } catch { /* ignore */ }
  }
  setDockerLogsControlsStreaming(false);
  updateDockerLogsStatus('');
}

function drainDockerSseBuffer(sseBuffer, onChunk) {
  const parts = String(sseBuffer || '').split('\n\n');
  const rest = parts.pop() || '';
  let exitCode = null;
  for (const part of parts) {
    const line = part.split('\n').find((l) => l.startsWith('data: '));
    if (!line) continue;
    let event;
    try {
      event = JSON.parse(line.slice(6));
    } catch {
      continue;
    }
    if (event.text) onChunk(event.text);
    if (event.t === 'exit' && event.code != null) exitCode = event.code;
  }
  return { rest, exitCode };
}

async function consumeExecStreamToCallback(res, onChunk) {
  if (!res.body) return -1;
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let sseBuffer = '';
  let exitCode = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    sseBuffer += decoder.decode(value, { stream: true });
    const parsed = drainDockerSseBuffer(sseBuffer, onChunk);
    sseBuffer = parsed.rest;
    if (parsed.exitCode != null) exitCode = parsed.exitCode;
  }
  if (sseBuffer.trim()) {
    const parsed = drainDockerSseBuffer(`${sseBuffer}\n\n`, onChunk);
    if (parsed.exitCode != null) exitCode = parsed.exitCode;
  }
  return exitCode;
}

/** XHR streaming — WKWebView often buffers fetch ReadableStream until the response ends. */
function postWorkspaceShellStreamXhr(command, cwd, { signal, onChunk } = {}) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    state.dockerLogsXhr = xhr;
    let offset = 0;
    let sseBuffer = '';
    let exitCode = 0;
    let settled = false;

    const finish = (fn, arg) => {
      if (settled) return;
      settled = true;
      if (state.dockerLogsXhr === xhr) state.dockerLogsXhr = null;
      fn(arg);
    };

    const onAbort = () => {
      try { xhr.abort(); } catch { /* ignore */ }
      const err = new Error('Aborted');
      err.name = 'AbortError';
      finish(reject, err);
    };
    if (signal) {
      if (signal.aborted) {
        onAbort();
        return;
      }
      signal.addEventListener('abort', onAbort, { once: true });
    }

    const consumeNew = () => {
      const text = xhr.responseText || '';
      if (text.length <= offset) return;
      sseBuffer += text.slice(offset);
      offset = text.length;
      const parsed = drainDockerSseBuffer(sseBuffer, onChunk);
      sseBuffer = parsed.rest;
      if (parsed.exitCode != null) exitCode = parsed.exitCode;
    };

    xhr.open('POST', repoApi(state.repo, '/workspace/shell'));
    xhr.setRequestHeader('Content-Type', 'application/json');
    xhr.responseType = 'text';
    xhr.onprogress = () => consumeNew();
    xhr.onload = () => {
      consumeNew();
      if (sseBuffer.trim()) {
        const parsed = drainDockerSseBuffer(`${sseBuffer}\n\n`, onChunk);
        if (parsed.exitCode != null) exitCode = parsed.exitCode;
      }
      finish(resolve, exitCode);
    };
    xhr.onerror = () => finish(reject, new Error('Docker shell request failed'));
    xhr.onabort = () => {
      const err = new Error('Aborted');
      err.name = 'AbortError';
      finish(reject, err);
    };
    xhr.send(JSON.stringify({ command, cwd: cwd || undefined }));
  });
}

async function startDockerLogsStream(command, cwd, { label, modulePath, append = false } = {}) {
  if (!state.repo || !command) return;
  await stopDockerLogsStream();
  if (label || modulePath) setDockerLogsMeta(label, modulePath);
  if (!append) clearDockerLogsOutput();
  setDockerLogsControlsStreaming(true);
  updateDockerLogsStatus(append ? 'Following…' : 'Streaming…', { live: true });
  setDockerConsoleOutputMeta(label || 'streaming');
  const out = $('#docker-logs-output');
  if (out && !out.textContent) out.classList.add('is-waiting');

  const ac = new AbortController();
  state.dockerLogsAbortController = ac;
  try {
    const exitCode = await postWorkspaceShellStreamXhr(command, cwd, {
      signal: ac.signal,
      onChunk: appendDockerLogsText,
    });
    if (state.dockerLogsAbortController !== ac) return;
    if (exitCode === 130) {
      updateDockerLogsStatus('Stopped');
    } else if (exitCode !== 0) {
      updateDockerLogsStatus(`Exited ${exitCode}`);
      setDockerConsoleOutputMeta(`exit ${exitCode}`, { error: true });
    } else {
      updateDockerLogsStatus(append ? 'Following ended' : 'Done');
    }
  } catch (e) {
    if (state.dockerLogsAbortController !== ac && e?.name === 'AbortError') return;
    if (e?.name === 'AbortError') {
      updateDockerLogsStatus('Stopped');
    } else {
      appendDockerLogsText(`\nerror: ${e.message || e}\n`);
      updateDockerLogsStatus('Error');
      setDockerConsoleOutputMeta('error', { error: true });
      toast(e.message || 'Docker logs failed', 'error');
    }
  } finally {
    if (state.dockerLogsAbortController === ac) {
      state.dockerLogsAbortController = null;
      setDockerLogsControlsStreaming(false);
      $('#docker-logs-output')?.classList.remove('is-waiting');
    }
  }
}

async function runDockerLogsTask(modulePath, taskCommand, taskLabel) {
  if (!state.repo || !modulePath || !taskCommand) return;
  if (state.dirty.has(state.activeTab)) await saveFile({ silent: true, skipProjectReload: true });
  openDockerLogsPanel();
  setDockerLogsMeta(taskLabel || 'docker compose logs', modulePath);
  await startDockerLogsStream(taskCommand, buildTaskWorkdir(modulePath));
}

async function followDockerLogsFromPanel() {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  const match = findDockerLogsFollowCommand();
  if (!match) {
    toast('No Docker Compose project — open a compose file or build tasks first', 'warning');
    return;
  }
  openDockerLogsPanel();
  await startDockerLogsStream(match.command, buildTaskWorkdir(match.modulePath), {
    label: match.label,
    modulePath: match.modulePath,
  });
}

function applyDockerLogsDock() {
  const panel = $('#panel-docker-logs');
  const sidebar = $('#sidebar');
  const rightDock = $('#docker-logs-dock-right');
  const bottomDock = $('#docker-logs-dock-bottom');
  const rightResizer = $('#docker-logs-right-resizer');
  if (!panel || !sidebar || !rightDock || !bottomDock) return;

  const dock = state.dockerLogsDock;
  const showLogs = dock === 'left' ? state.activePanel === 'docker-logs' : state.dockerLogsOpen;

  if (rightResizer) {
    const showRightResize = dock === 'right' && showLogs;
    rightResizer.classList.toggle('hidden', !showRightResize);
  }

  if (dock === 'left') {
    sidebar.appendChild(panel);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
    bottomDock.classList.add('hidden');
    bottomDock.classList.remove('flex');
  } else if (dock === 'right') {
    rightDock.appendChild(panel);
    rightDock.classList.toggle('hidden', !showLogs);
    rightDock.classList.toggle('flex', showLogs);
    bottomDock.classList.add('hidden');
    bottomDock.classList.remove('flex');
  } else {
    bottomDock.appendChild(panel);
    bottomDock.classList.toggle('hidden', !showLogs);
    bottomDock.classList.toggle('flex', showLogs);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
  }

  panel.classList.toggle('hidden', !showLogs);
  panel.classList.toggle('flex', showLogs);

  $$('[data-docker-logs-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.dockerLogsDock === dock);
  });
  syncDockMenuControls();
}

function setDockerLogsDock(dock) {
  if (!['left', 'right', 'bottom'].includes(dock)) return;
  const wasOpen = state.dockerLogsOpen || state.activePanel === 'docker-logs';
  state.dockerLogsDock = dock;
  localStorage.setItem(DOCKER_LOGS_DOCK_KEY, dock);
  if (wasOpen && dock === 'left') {
    state.dockerLogsOpen = false;
    switchPanel('docker-logs');
    return;
  }
  if (wasOpen && dock !== 'left' && state.activePanel === 'docker-logs') {
    state.activePanel = 'explorer';
    state.dockerLogsOpen = true;
  }
  applyDockerLogsDock();
}

function openDockerLogsPanel() {
  if (state.dockerLogsDock === 'left') {
    switchPanel('docker-logs');
  } else {
    state.dockerLogsOpen = true;
    applyDockerLogsDock();
  }
  syncDockerConsoleSubtitle();
  renderDockerConsoleQuick();
  const saved = state.repo && localStorage.getItem(`reaper-docker-cmd-${state.repo}`);
  const input = $('#docker-console-command');
  if (input && saved && !input.value.trim()) input.value = saved;
  void refreshDockerContainers();
}

function showDockerLogsPanel() {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  openDockerLogsPanel();
}

function hideDockerLogsPanel() {
  void stopDockerLogsStream();
  if (state.dockerLogsDock === 'left') {
    if (state.activePanel === 'docker-logs') switchPanel('explorer');
    return;
  }
  state.dockerLogsOpen = false;
  applyDockerLogsDock();
}

function toggleDockerLogsPanel() {
  if (state.dockerLogsOpen || state.activePanel === 'docker-logs') hideDockerLogsPanel();
  else showDockerLogsPanel();
}

function applyDockerLogsBottomHeight(px) {
  const dock = $('#docker-logs-dock-bottom');
  if (!dock || !Number.isFinite(px)) return;
  const min = 180;
  const max = Math.min(Math.round(window.innerHeight * 0.65), 640);
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  dock.style.setProperty('--ij-docker-logs-bottom-h', `${clamped}px`);
  return clamped;
}

function applyDockerLogsRightWidth(px) {
  const min = 280;
  const max = Math.min(window.innerWidth * 0.48, 560);
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  document.documentElement.style.setProperty('--ij-docker-logs-right-w', `${clamped}px`);
  return clamped;
}

function bindSideDockResize({ leftResizer, rightResizer, leftDock, widthVar, storageKey, min, max }) {
  const applyWidth = (px) => {
    const clamped = Math.min(max, Math.max(min, Math.round(px)));
    document.documentElement.style.setProperty(widthVar, `${clamped}px`);
    return clamped;
  };

  const bindRight = (sel) => {
    const resizer = $(sel);
    if (!resizer) return;
    let dragging = false;
    const onMove = (e) => {
      if (!dragging) return;
      applyWidth(window.innerWidth - e.clientX);
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      resizer.classList.remove('dragging');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const w = getComputedStyle(document.documentElement).getPropertyValue(widthVar).trim();
      if (w) localStorage.setItem(storageKey, w);
    };
    resizer.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      dragging = true;
      resizer.classList.add('dragging');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('blur', onUp);
  };

  const bindLeft = (resizerSel, dockSel) => {
    const resizer = $(resizerSel);
    const dock = $(dockSel);
    if (!resizer || !dock) return;
    let dragging = false;
    const onMove = (e) => {
      if (!dragging) return;
      const rect = dock.getBoundingClientRect();
      applyWidth(e.clientX - rect.left);
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      resizer.classList.remove('dragging');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const w = getComputedStyle(document.documentElement).getPropertyValue(widthVar).trim();
      if (w) localStorage.setItem(storageKey, w);
    };
    resizer.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      dragging = true;
      resizer.classList.add('dragging');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('blur', onUp);
  };

  bindLeft(leftResizer, leftDock);
  bindRight(rightResizer);
}

function initBuildTasksDockResize() {
  const savedW = localStorage.getItem(BUILD_TASKS_WIDTH_KEY);
  if (savedW) document.documentElement.style.setProperty('--ij-build-tasks-w', savedW);

  const savedManifestW = localStorage.getItem(PACKAGE_MANIFEST_WIDTH_KEY);
  if (savedManifestW) document.documentElement.style.setProperty('--ij-package-manifest-w', savedManifestW);

  bindSideDockResize({
    leftResizer: '#build-tasks-left-resizer',
    rightResizer: '#build-tasks-right-resizer',
    leftDock: '#build-tasks-dock-left',
    widthVar: '--ij-build-tasks-w',
    storageKey: BUILD_TASKS_WIDTH_KEY,
    min: 260,
    max: 520,
  });

  bindSideDockResize({
    leftResizer: '#package-manifest-left-resizer',
    rightResizer: '#package-manifest-right-resizer',
    leftDock: '#package-manifest-dock-left',
    widthVar: '--ij-package-manifest-w',
    storageKey: PACKAGE_MANIFEST_WIDTH_KEY,
    min: 260,
    max: 520,
  });
}

function initDockerLogsDockResize() {
  const savedW = localStorage.getItem(DOCKER_LOGS_RIGHT_WIDTH_KEY);
  if (savedW) document.documentElement.style.setProperty('--ij-docker-logs-right-w', savedW);

  const savedH = parseInt(localStorage.getItem(DOCKER_LOGS_BOTTOM_HEIGHT_KEY), 10);
  if (Number.isFinite(savedH)) applyDockerLogsBottomHeight(savedH);

  const resizer = $('#docker-logs-right-resizer');
  if (resizer) {
    let dragging = false;
    const onMove = (e) => {
      if (!dragging) return;
      applyDockerLogsRightWidth(window.innerWidth - e.clientX);
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      resizer.classList.remove('dragging');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const w = getComputedStyle(document.documentElement).getPropertyValue('--ij-docker-logs-right-w').trim();
      if (w) localStorage.setItem(DOCKER_LOGS_RIGHT_WIDTH_KEY, w);
    };
    resizer.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      dragging = true;
      resizer.classList.add('dragging');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('blur', onUp);
  }

  const handle = $('#docker-logs-bottom-resize');
  if (!handle) return;

  let draggingBottom = false;
  let startY = 0;
  let startH = 0;

  const stopBottomDrag = () => {
    if (!draggingBottom) return;
    draggingBottom = false;
    handle.classList.remove('active');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    const h = applyDockerLogsBottomHeight($('#docker-logs-dock-bottom')?.getBoundingClientRect().height);
    if (h) localStorage.setItem(DOCKER_LOGS_BOTTOM_HEIGHT_KEY, String(h));
  };

  handle.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    draggingBottom = true;
    startY = e.clientY;
    startH = $('#docker-logs-dock-bottom')?.getBoundingClientRect().height || 0;
    handle.classList.add('active');
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  });
  window.addEventListener('mousemove', (e) => {
    if (!draggingBottom) return;
    applyDockerLogsBottomHeight(startH + (startY - e.clientY));
  });
  window.addEventListener('mouseup', stopBottomDrag);
  window.addEventListener('blur', stopBottomDrag);
}

function applyDbConnectionToForm(conn, { clearPassword = true } = {}) {
  state.dbConnection = conn;
  const urlEl = $('#db-viewer-url');
  if (urlEl) urlEl.value = conn?.database_url ?? '';
  const nameEl = $('#db-viewer-connection-name');
  if (nameEl) nameEl.value = conn?.connection_name ?? '';
  const pwEl = $('#db-viewer-password');
  if (pwEl && clearPassword) {
    pwEl.value = '';
    pwEl.placeholder = conn?.password_set ? '•••••••• (saved)' : 'Password';
  }
  populateDbConnectionPicker(conn);
  applyDbSslToForm(conn?.ssl);
  applyDbSshToForm(conn?.ssh, conn?.ssh_local_port);
  updateDbSslPanelVisibility(conn);
  updateDbSshPanelVisibility(conn);
  updateDbViewerStatusDot(conn);
  if (state.activeTab?.endsWith('.sql')) updateRunButtons();
}

function populateDbConnectionPicker(conn) {
  const picker = $('#db-viewer-connection-picker');
  if (!picker) return;
  const list = Array.isArray(conn?.connections) ? conn.connections : [];
  const activeId = conn?.active_id || '';
  const opts = [
    `<option value="">New connection…</option>`,
    ...list.map((c) => {
      const selected = c.id === activeId ? ' selected' : '';
      const label = escapeHtml(`${c.name}${c.display ? ` — ${c.display}` : ''}`);
      return `<option value="${escapeHtml(c.id)}"${selected}>${label}</option>`;
    }),
  ];
  picker.innerHTML = opts.join('');
  if (activeId) picker.value = activeId;
  else picker.value = '';
}

function readDbConnectionPayload() {
  const urlEl = $('#db-viewer-url');
  const nameEl = $('#db-viewer-connection-name');
  const pwEl = $('#db-viewer-password');
  const picker = $('#db-viewer-connection-picker');
  const connectionId = picker?.value?.trim() || null;
  return {
    connection_id: connectionId || null,
    name: nameEl?.value?.trim() || null,
    database_url: urlEl?.value?.trim() || null,
    password: pwEl?.value || null,
    ssl: readDbSslFromForm(),
    ssh: readDbSshFromForm(),
  };
}

async function loadDbConnection() {
  if (!state.repo) return null;
  try {
    const conn = await api(repoApi(state.repo, '/workspace/db/connection'));
    applyDbConnectionToForm(conn);
    return conn;
  } catch (e) {
    toast(`Database: ${e.message}`, 'warning');
    return null;
  }
}

function readDbSslFromForm() {
  const mode = $('#db-viewer-ssl-mode')?.value?.trim() || '';
  const root = $('#db-viewer-ssl-root')?.value?.trim() || '';
  const cert = $('#db-viewer-ssl-cert')?.value?.trim() || '';
  const key = $('#db-viewer-ssl-key')?.value?.trim() || '';
  if (!mode && !root && !cert && !key) return null;
  return {
    ssl_mode: mode || null,
    ssl_root_cert: root || null,
    ssl_cert: cert || null,
    ssl_key: key || null,
  };
}

function applyDbSslToForm(ssl) {
  const modeEl = $('#db-viewer-ssl-mode');
  const rootEl = $('#db-viewer-ssl-root');
  const certEl = $('#db-viewer-ssl-cert');
  const keyEl = $('#db-viewer-ssl-key');
  if (modeEl) modeEl.value = ssl?.ssl_mode || '';
  if (rootEl) rootEl.value = ssl?.ssl_root_cert || '';
  if (certEl) certEl.value = ssl?.ssl_cert || '';
  if (keyEl) keyEl.value = ssl?.ssl_key || '';
  updateDbSslHint(ssl);
}

function updateDbSslHint(ssl) {
  const hint = $('#db-viewer-ssl-hint');
  if (!hint) return;
  if (!ssl || (!ssl.ssl_mode && !ssl.ssl_root_cert && !ssl.ssl_cert && !ssl.ssl_key)) {
    hint.textContent = '';
    return;
  }
  const mode = ssl.ssl_mode || 'prefer';
  hint.textContent = mode;
}

function updateDbSslPanelVisibility(conn) {
  const panel = $('#db-viewer-ssl-panel');
  if (!panel) return;
  const show = !conn || conn.kind === 'postgres' || conn.kind === 'mysql' || conn.kind === 'none';
  panel.classList.toggle('is-hidden', !show);
}

function readDbSshFromForm() {
  const enabled = ($('#db-viewer-ssh-enabled')?.value || '') === '1';
  const host = $('#db-viewer-ssh-host')?.value?.trim() || '';
  const portRaw = $('#db-viewer-ssh-port')?.value?.trim() || '';
  const user = $('#db-viewer-ssh-user')?.value?.trim() || '';
  const identity = $('#db-viewer-ssh-key')?.value?.trim() || '';
  const remoteHost = $('#db-viewer-ssh-remote-host')?.value?.trim() || '';
  const remotePortRaw = $('#db-viewer-ssh-remote-port')?.value?.trim() || '';
  const localPortRaw = $('#db-viewer-ssh-local-port')?.value?.trim() || '';
  if (!enabled && !host && !user && !identity && !remoteHost && !portRaw && !remotePortRaw && !localPortRaw) {
    return null;
  }
  const port = portRaw ? Number.parseInt(portRaw, 10) : null;
  const remotePort = remotePortRaw ? Number.parseInt(remotePortRaw, 10) : null;
  const localPort = localPortRaw ? Number.parseInt(localPortRaw, 10) : null;
  return {
    enabled,
    host: host || null,
    port: Number.isFinite(port) && port > 0 ? port : null,
    user: user || null,
    identity_file: identity || null,
    remote_host: remoteHost || null,
    remote_port: Number.isFinite(remotePort) && remotePort > 0 ? remotePort : null,
    local_port: Number.isFinite(localPort) && localPort > 0 ? localPort : null,
  };
}

function applyDbSshToForm(ssh, localPortActive) {
  const enabledEl = $('#db-viewer-ssh-enabled');
  const hostEl = $('#db-viewer-ssh-host');
  const portEl = $('#db-viewer-ssh-port');
  const userEl = $('#db-viewer-ssh-user');
  const keyEl = $('#db-viewer-ssh-key');
  const remoteHostEl = $('#db-viewer-ssh-remote-host');
  const remotePortEl = $('#db-viewer-ssh-remote-port');
  const localPortEl = $('#db-viewer-ssh-local-port');
  if (enabledEl) enabledEl.value = ssh?.enabled ? '1' : '';
  if (hostEl) hostEl.value = ssh?.host || '';
  if (portEl) portEl.value = ssh?.port != null ? String(ssh.port) : '';
  if (userEl) userEl.value = ssh?.user || '';
  if (keyEl) keyEl.value = ssh?.identity_file || '';
  if (remoteHostEl) remoteHostEl.value = ssh?.remote_host || '';
  if (remotePortEl) remotePortEl.value = ssh?.remote_port != null ? String(ssh.remote_port) : '';
  if (localPortEl) localPortEl.value = ssh?.local_port != null ? String(ssh.local_port) : '';
  updateDbSshHint(ssh, localPortActive);
}

function updateDbSshHint(ssh, localPortActive) {
  const hint = $('#db-viewer-ssh-hint');
  if (!hint) return;
  if (!ssh?.enabled) {
    hint.textContent = '';
    return;
  }
  const port = localPortActive || ssh.local_port;
  hint.textContent = port ? `on :${port}` : 'on';
}

function updateDbSshPanelVisibility(conn) {
  const panel = $('#db-viewer-ssh-panel');
  if (!panel) return;
  const show = !conn || conn.kind === 'postgres' || conn.kind === 'mysql' || conn.kind === 'none';
  panel.classList.toggle('is-hidden', !show);
}

async function saveDbConnection() {
  if (!state.repo) return null;
  const payload = readDbConnectionPayload();
  try {
    const conn = await api(repoApi(state.repo, '/workspace/db/connection'), {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    applyDbConnectionToForm(conn);
    const schema = await refreshDbSchema();
    void refreshRunInfo();
    const err =
      conn?.error ||
      schema?.connection?.error ||
      schema?.error ||
      state.dbConnection?.error ||
      null;
    if (err) {
      toast(err, 'error');
    } else if (conn?.connected) {
      toast(`Connected to ${conn.display}`, 'success');
    } else {
      toast('Connection saved', 'info');
    }
    return conn;
  } catch (e) {
    toast(e.message || 'Could not save connection', 'error');
    return null;
  }
}

async function testDbConnection() {
  if (!state.repo) return null;
  const payload = readDbConnectionPayload();
  try {
    const conn = await api(repoApi(state.repo, '/workspace/db/connection/test'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    updateDbViewerStatusDot(conn);
    updateDbSslPanelVisibility(conn);
    updateDbSshPanelVisibility(conn);
    if (conn?.connected) toast(`Connection OK — ${conn.display}`, 'success');
    else toast(conn?.error || 'Connection failed', 'error');
    return conn;
  } catch (e) {
    toast(e.message || 'Connection test failed', 'error');
    return null;
  }
}

async function selectDbConnection(id) {
  if (!state.repo) return null;
  if (!id) {
    applyDbConnectionToForm({
      ...(state.dbConnection || {}),
      active_id: null,
      connection_name: '',
      database_url: '',
      password_set: false,
      connected: false,
      error: null,
      display: 'Not connected',
      kind: 'none',
      ssl: null,
      ssh: null,
      connections: state.dbConnection?.connections || [],
    });
    const pwEl = $('#db-viewer-password');
    if (pwEl) {
      pwEl.value = '';
      pwEl.placeholder = 'Password';
    }
    return null;
  }
  try {
    const conn = await api(repoApi(state.repo, '/workspace/db/connection/select'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id }),
    });
    applyDbConnectionToForm(conn);
    await refreshDbSchema();
    void refreshRunInfo();
    return conn;
  } catch (e) {
    toast(e.message || 'Could not switch connection', 'error');
    return null;
  }
}

async function deleteDbConnection() {
  if (!state.repo) return null;
  const id = $('#db-viewer-connection-picker')?.value?.trim();
  if (!id) {
    toast('Select a saved connection to delete', 'info');
    return null;
  }
  const label = $('#db-viewer-connection-name')?.value?.trim() || id;
  if (!window.confirm(`Delete connection “${label}”?`)) return null;
  try {
    const conn = await api(repoApi(state.repo, '/workspace/db/connection/delete'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id }),
    });
    applyDbConnectionToForm(conn);
    await refreshDbSchema();
    void refreshRunInfo();
    toast('Connection deleted', 'info');
    return conn;
  } catch (e) {
    toast(e.message || 'Could not delete connection', 'error');
    return null;
  }
}

function updateDbViewerStatusDot(conn) {
  const dot = $('#db-viewer-status-dot');
  if (!dot) return;
  dot.classList.remove('is-connected', 'is-error');
  if (conn?.connected) dot.classList.add('is-connected');
  else if (conn?.error) dot.classList.add('is-error');
}

async function refreshDbSchema() {
  if (!state.repo) return null;
  try {
    const schema = await api(repoApi(state.repo, '/workspace/db/schema'));
    state.dbSchema = schema;
    const conn = dbConnFromPayload(schema);
    if (conn) {
      const prev = state.dbConnection || {};
      const merged = {
        ...prev,
        ...conn,
        connections:
          Array.isArray(conn.connections) && conn.connections.length
            ? conn.connections
            : prev.connections || [],
        active_id: conn.active_id ?? prev.active_id,
        connection_name: conn.connection_name ?? prev.connection_name,
        password_set: conn.password_set ?? prev.password_set,
      };
      state.dbConnection = merged;
      populateDbConnectionPicker(merged);
      updateDbViewerStatusDot(merged);
    }
    renderDbViewerSchema(schema);
    return schema;
  } catch (e) {
    toast(`Schema: ${e.message}`, 'warning');
    renderDbViewerSchema(null);
    return null;
  }
}

function dbConnFromPayload(payload) {
  if (!payload) return null;
  if (payload.connection && typeof payload.connection === 'object') return payload.connection;
  if (payload.display != null || payload.connected != null || payload.kind != null) {
    return {
      database_url: payload.database_url,
      kind: payload.kind,
      resolved_path: payload.resolved_path,
      display: payload.display,
      connected: payload.connected,
      error: payload.error,
      ssl: payload.ssl,
      ssh: payload.ssh,
      ssh_local_port: payload.ssh_local_port,
      password_set: payload.password_set,
      active_id: payload.active_id,
      connection_name: payload.connection_name,
      connections: payload.connections,
    };
  }
  return null;
}

function getDbSqlEl() {
  return $('#db-viewer-sql');
}

function setDbSqlText(text) {
  const el = getDbSqlEl();
  if (!el) return;
  el.value = String(text ?? '');
  el.focus();
}

function loadDbSqlQuery(sql) {
  const snippet = String(sql || '').trim();
  if (!snippet) return;
  const text = snippet.endsWith(';') ? `${snippet}\n` : `${snippet};\n`;
  setDbSqlText(text);
}

function getDbSqlText() {
  return getDbSqlEl()?.value ?? '';
}

function getDbSqlSelectionText() {
  const el = getDbSqlEl();
  if (!el) return '';
  const start = el.selectionStart ?? 0;
  const end = el.selectionEnd ?? 0;
  if (start !== end) return el.value.slice(start, end);
  return el.value;
}

function insertDbSqlSnippet(sql) {
  const el = getDbSqlEl();
  if (!el) return;
  const snippet = String(sql || '').trim();
  if (!snippet) return;
  const text = snippet.endsWith('\n') ? snippet : `${snippet}\n`;
  const start = el.selectionStart ?? el.value.length;
  const end = el.selectionEnd ?? start;
  el.value = `${el.value.slice(0, start)}${text}${el.value.slice(end)}`;
  const pos = start + text.length;
  el.setSelectionRange(pos, pos);
  el.focus();
}

function syncDbSqlFromActiveTab() {
  if (!state.activeTab?.endsWith('.sql')) return;
  const el = getDbSqlEl();
  if (!el) return;
  const fromEditor = state.editor?.getValue?.();
  const fromTab = state.tabContents.get(state.activeTab);
  const text = (fromEditor ?? fromTab ?? '').trim();
  if (text) setDbSqlText(text);
}

/** @deprecated use syncDbSqlFromActiveTab */
function seedDbSqlFromActiveTab() {
  const el = getDbSqlEl();
  if (el && !el.value.trim()) syncDbSqlFromActiveTab();
}

function dbTableKey(table) {
  if (!table) return '';
  return table.schema && table.schema !== 'main'
    ? `${table.schema}.${table.name}`
    : table.name;
}

function dbObjectStateKey(obj) {
  if (!obj) return '';
  const base = dbTableKey(obj);
  const kind = obj.kind || 'table';
  return kind === 'table' ? base : `${base}@${kind}`;
}

const DB_KIND_FOLDERS = [
  { kind: 'table', label: 'Tables', icon: 'folderTables' },
  { kind: 'view', label: 'Views', icon: 'view' },
  { kind: 'materialized_view', label: 'Materialized Views', icon: 'materialized_view' },
];

function partitionDbObjects(objects) {
  const parts = { table: [], view: [], materialized_view: [] };
  for (const obj of objects) {
    const kind = obj.kind || 'table';
    if (parts[kind]) parts[kind].push(obj);
    else parts.table.push(obj);
  }
  return parts;
}

function dbObjectMatchesFilter(obj, filter) {
  if (!filter) return true;
  const label = dbTableKey(obj).toLowerCase();
  if (label.includes(filter)) return true;
  if ((obj.kind || 'table').replace('_', ' ').includes(filter)) return true;
  if ((obj.columns || []).some((col) =>
    col.name.toLowerCase().includes(filter) || col.type_name.toLowerCase().includes(filter),
  )) return true;
  return (obj.indexes || []).some((idx) =>
    idx.name.toLowerCase().includes(filter)
    || (idx.columns || []).some((col) => col.toLowerCase().includes(filter)),
  );
}

function filterDbObjects(objects, filter) {
  if (!filter) return objects;
  return objects.filter((obj) => dbObjectMatchesFilter(obj, filter)).map((obj) => {
    const label = dbTableKey(obj).toLowerCase();
    if (label.includes(filter) || (obj.kind || 'table').includes(filter)) return obj;
    return {
      ...obj,
      columns: (obj.columns || []).filter((col) =>
        col.name.toLowerCase().includes(filter) || col.type_name.toLowerCase().includes(filter),
      ),
      indexes: (obj.indexes || []).filter((idx) =>
        idx.name.toLowerCase().includes(filter)
        || (idx.columns || []).some((col) => col.toLowerCase().includes(filter)),
      ),
    };
  });
}

function stripSqlComments(sql) {
  return String(sql || '')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/--[^\n\r]*/g, ' ');
}

function sqlStatementHead(stmt) {
  return stripSqlComments(stmt).replace(/\s+/g, ' ').trim();
}

function sqlLooksLikeReadQuery(sql) {
  const head = sqlStatementHead(String(sql || '').split(';').map((s) => s.trim()).find(Boolean) || '');
  return /^(select|with|show|explain|describe|desc|pragma)\b/i.test(head);
}

function sqlMayChangeSchema(sql) {
  const chunks = stripSqlComments(sql).split(';').map((s) => s.trim()).filter(Boolean);
  const stmts = chunks.length ? chunks : [stripSqlComments(sql).trim()];
  return stmts.some((stmt) => {
    const head = sqlStatementHead(stmt);
    if (!head) return false;
    return (
      /\b(create|drop|alter|rename|truncate)\s+(table|index|view|schema|database|sequence|type|materialized\s+view)\b/i.test(head)
      || /\bcreate\s+(unique\s+)?index\b/i.test(head)
      || /\bcreate\s+table\b/i.test(head)
      || /\bdrop\s+(index|table|view|schema)\b/i.test(head)
      || /\balter\s+table\b/i.test(head)
    );
  });
}

function shellCommandMayAffectDbSchema(command) {
  const cmd = String(command || '').toLowerCase();
  return (
    /\b(psql|mysql|mariadb|sqlite3)\b/.test(cmd)
    || /\binit-db\b/.test(cmd)
    || /\b(db:?(setup|seed|init|migrate|reset)|migrate:?(fresh|refresh)?|schema:?(load|dump))\b/.test(cmd)
    || /\b(flyway|liquibase|prisma)\b/.test(cmd)
    || /\.sql(\s|$|['"])/.test(cmd)
  );
}

async function maybeRefreshDbSchemaAfterSql(sql, { ok = true, result = null } = {}) {
  if (!ok || !state.repo || !state.dbViewerPanelOpen) return;
  const text = String(sql || '').trim();
  if (!text) return;
  const ddl = sqlMayChangeSchema(text);
  const ddlResult = result && !result.error
    && !result.columns?.length
    && !sqlLooksLikeReadQuery(text);
  if (ddl || ddlResult) await refreshDbSchema();
}

async function maybeRefreshDbSchemaAfterShell(command, exitCode) {
  if (!state.repo || !state.dbViewerPanelOpen || exitCode !== 0) return;
  if (shellCommandMayAffectDbSchema(command)) {
    await refreshDbSchema();
  }
}

function defaultDbColumnWidth(name) {
  const len = String(name || '').length;
  return Math.max(72, Math.min(280, len * 8 + 32));
}

let dbGridColumnDrag = null;

function initDbResultsColumnResize() {
  if (window.__reaperDbColResizeInit) return;
  window.__reaperDbColResizeInit = true;

  const onMove = (e) => {
    if (!dbGridColumnDrag) return;
    const { table, idx, startX, startW, columnNames } = dbGridColumnDrag;
    const w = Math.max(48, Math.round(startW + (e.clientX - startX)));
    const col = table.querySelector(`col[data-col-idx="${idx}"]`);
    if (col) col.style.width = `${w}px`;
    const th = table.querySelector(`th[data-col-idx="${idx}"]`);
    if (th) th.style.width = `${w}px`;
    const name = columnNames[idx];
    if (name) state.dbGridColumnWidths[name] = w;
  };

  const stopDrag = () => {
    if (!dbGridColumnDrag) return;
    dbGridColumnDrag.handle?.classList.remove('active');
    dbGridColumnDrag = null;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  };

  window.addEventListener('mousemove', onMove);
  window.addEventListener('mouseup', stopDrag);
  window.addEventListener('blur', stopDrag);
}

function wireDbResultsGridColumnResize(container, columnNames) {
  initDbResultsColumnResize();
  const table = container.querySelector('.ij-db-viewer-grid');
  if (!table) return;
  container.querySelectorAll('.ij-db-col-resize-handle').forEach((handle) => {
    handle.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      const th = handle.closest('th');
      const idx = parseInt(th?.dataset.colIdx, 10);
      if (!Number.isFinite(idx)) return;
      const col = table.querySelector(`col[data-col-idx="${idx}"]`);
      const startW = col?.getBoundingClientRect().width || th.getBoundingClientRect().width;
      handle.classList.add('active');
      dbGridColumnDrag = { table, idx, startX: e.clientX, startW, columnNames, handle };
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
      e.stopPropagation();
    });
  });
}

function renderDbQueryResult(result) {
  const container = $('#db-viewer-results');
  const queryMeta = $('#db-viewer-query-meta');
  const resultsMeta = $('#db-viewer-results-meta');
  if (!container) return;
  state.dbQueryResult = result;
  if (queryMeta) {
    if (result?.error) queryMeta.textContent = 'Error';
    else if (result) queryMeta.textContent = `${result.elapsed_ms ?? 0} ms`;
    else queryMeta.textContent = '';
  }
  if (resultsMeta) {
    if (result?.error) resultsMeta.textContent = '';
    else if (result) {
      const suffix = result.truncated ? ' · truncated' : '';
      resultsMeta.textContent = `${result.row_count ?? 0} row(s)${suffix}`;
    } else resultsMeta.textContent = '';
  }
  if (!result) {
    container.innerHTML = '';
    return;
  }
  if (result.error) {
    container.innerHTML = `<div class="ij-db-viewer-error">${escapeHtml(result.error)}</div>`;
    return;
  }
  if (!result.columns?.length) {
    container.innerHTML = '<div class="ij-db-viewer-empty">Query completed with no columns.</div>';
    return;
  }
  const colWidths = result.columns.map((c) => state.dbGridColumnWidths[c] || defaultDbColumnWidth(c));
  const colgroup = result.columns.map((c, i) =>
    `<col data-col-idx="${i}" data-col-name="${escapeHtml(c)}" style="width:${colWidths[i]}px">`,
  ).join('');
  const head = `<tr>${result.columns.map((c, i) =>
    `<th data-col-idx="${i}" data-col-name="${escapeHtml(c)}" style="width:${colWidths[i]}px"><span class="ij-db-col-head-inner"><span class="ij-db-col-head-label">${escapeHtml(c)}</span><span class="ij-db-col-resize-handle" role="separator" aria-orientation="vertical" aria-label="Resize ${escapeHtml(c)} column"></span></span></th>`,
  ).join('')}</tr>`;
  const body = (result.rows || []).map((row) =>
    `<tr>${row.map((cell) => `<td>${escapeHtml(cell ?? '')}</td>`).join('')}</tr>`,
  ).join('');
  container.innerHTML = `<table class="ij-db-viewer-grid"><colgroup>${colgroup}</colgroup><thead>${head}</thead><tbody>${body}</tbody></table>`;
  wireDbResultsGridColumnResize(container, result.columns);
}

async function runDbQuery(sql) {
  const query = (sql || '').trim();
  if (!query) {
    toast('Enter SQL to run', 'info');
    return;
  }
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  const container = $('#db-viewer-results');
  const queryMeta = $('#db-viewer-query-meta');
  const resultsMeta = $('#db-viewer-results-meta');
  if (container) container.innerHTML = '<div class="ij-db-viewer-empty">Running…</div>';
  if (queryMeta) queryMeta.textContent = 'Running…';
  if (resultsMeta) resultsMeta.textContent = '';
  const runBtns = ['#btn-db-viewer-run-query', '#btn-db-viewer-run-selection'];
  runBtns.forEach((sel) => { const el = $(sel); if (el) el.disabled = true; });
  try {
    const result = await api(repoApi(state.repo, '/workspace/db/query'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ sql: query }),
    });
    renderDbQueryResult(result);
    if (result?.error) toast(result.error, 'error');
    else await maybeRefreshDbSchemaAfterSql(query, { ok: true, result });
  } catch (e) {
    renderDbQueryResult({ error: e.message || 'Query failed' });
    toast(e.message || 'Query failed', 'error');
  } finally {
    runBtns.forEach((sel) => { const el = $(sel); if (el) el.disabled = false; });
  }
}

function applyDbViewerResultsHeight(px) {
  const min = 80;
  const max = Math.min(window.innerHeight * 0.55, 520);
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  document.documentElement.style.setProperty('--ij-db-viewer-results-h', `${clamped}px`);
  return clamped;
}

function applyDbViewerSchemaRailWidth(px) {
  const min = 140;
  const max = 280;
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  document.documentElement.style.setProperty('--ij-db-schema-rail-w', `${clamped}px`);
  return clamped;
}

function applyDbViewerRightWidth(px) {
  const min = 360;
  const max = Math.min(window.innerWidth * 0.55, 780);
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  document.documentElement.style.setProperty('--ij-db-viewer-right-w', `${clamped}px`);
  return clamped;
}

function initDbViewerResize() {
  const savedPanelW = localStorage.getItem(DB_VIEWER_RIGHT_WIDTH_KEY);
  if (savedPanelW) document.documentElement.style.setProperty('--ij-db-viewer-right-w', savedPanelW);

  const savedRailW = localStorage.getItem(DB_VIEWER_SCHEMA_RAIL_WIDTH_KEY);
  if (savedRailW) document.documentElement.style.setProperty('--ij-db-schema-rail-w', savedRailW);

  const savedH = parseInt(localStorage.getItem('reaper-db-viewer-results-h'), 10);
  if (Number.isFinite(savedH)) applyDbViewerResultsHeight(savedH);

  const panelResizer = $('#db-viewer-right-resizer');
  if (panelResizer) {
    let dragging = false;
    const onMove = (e) => {
      if (!dragging) return;
      applyDbViewerRightWidth(window.innerWidth - e.clientX);
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      panelResizer.classList.remove('dragging');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const w = getComputedStyle(document.documentElement).getPropertyValue('--ij-db-viewer-right-w').trim();
      if (w) localStorage.setItem(DB_VIEWER_RIGHT_WIDTH_KEY, w);
    };
    panelResizer.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      dragging = true;
      panelResizer.classList.add('dragging');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('blur', onUp);
  }

  const schemaResizer = $('#db-viewer-schema-resizer');
  if (schemaResizer) {
    let dragging = false;
    let startX = 0;
    let startW = 0;
    const onMove = (e) => {
      if (!dragging) return;
      applyDbViewerSchemaRailWidth(startW + (e.clientX - startX));
    };
    const onUp = () => {
      if (!dragging) return;
      dragging = false;
      schemaResizer.classList.remove('active');
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      const w = getComputedStyle(document.documentElement).getPropertyValue('--ij-db-schema-rail-w').trim();
      if (w) localStorage.setItem(DB_VIEWER_SCHEMA_RAIL_WIDTH_KEY, w);
    };
    schemaResizer.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      dragging = true;
      startX = e.clientX;
      const rail = $('.ij-db-schema-rail');
      startW = rail?.getBoundingClientRect().width || 148;
      schemaResizer.classList.add('active');
      document.body.style.cursor = 'col-resize';
      document.body.style.userSelect = 'none';
      e.preventDefault();
    });
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    window.addEventListener('blur', onUp);
  }

  const handle = $('#db-viewer-results-resizer');
  const resultsPane = $('.ij-db-results-pane');
  if (!handle || !resultsPane) return;

  let draggingResults = false;
  let startY = 0;
  let startH = 0;

  const stopDrag = () => {
    if (!draggingResults) return;
    draggingResults = false;
    handle.classList.remove('active');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    const h = applyDbViewerResultsHeight(resultsPane.getBoundingClientRect().height);
    if (h) localStorage.setItem('reaper-db-viewer-results-h', String(h));
  };

  handle.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    draggingResults = true;
    startY = e.clientY;
    startH = resultsPane.getBoundingClientRect().height;
    handle.classList.add('active');
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  });

  window.addEventListener('mousemove', (e) => {
    if (!draggingResults) return;
    applyDbViewerResultsHeight(startH + (startY - e.clientY));
  });
  window.addEventListener('mouseup', stopDrag);
  window.addEventListener('blur', stopDrag);
}

function dbTreeIconSvg(kind) {
  const icons = {
    schema: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><ellipse cx="8" cy="4.5" rx="5" ry="2" stroke="currentColor" stroke-width="1.1"/><path d="M3 4.5v7c0 1.1 2.24 2 5 2s5-.9 5-2v-7" stroke="currentColor" stroke-width="1.1"/><path d="M3 8c0 1.1 2.24 2 5 2s5-.9 5-2" stroke="currentColor" stroke-width=".9" opacity=".55"/></svg>',
    table: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="2.5" y="3.5" width="11" height="9" rx="1" stroke="currentColor" stroke-width="1.1"/><path d="M2.5 6.5h11M6 6.5v6M10 6.5v6" stroke="currentColor" stroke-width=".95"/></svg>',
    view: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="2.5" y="4" width="11" height="8" rx="1" stroke="currentColor" stroke-width="1.1"/><path d="M5 8.5c.75-1.25 1.75-2 3-2s2.25.75 3 2" stroke="currentColor" stroke-width="1" stroke-linecap="round"/><circle cx="8" cy="7" r="1" fill="currentColor"/></svg>',
    materialized_view: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="2.5" y="4" width="11" height="7.5" rx="1" stroke="currentColor" stroke-width="1.1"/><path d="M2.5 6.5h11M6 6.5v5M10 6.5v5" stroke="currentColor" stroke-width=".9"/><path d="M11.5 2.5v2M13 3.25h-3" stroke="currentColor" stroke-width="1" stroke-linecap="round"/></svg>',
    column: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><rect x="3" y="2.5" width="2.5" height="11" rx=".5" fill="currentColor" opacity=".45"/><path d="M7 5.5h5.5M7 8h5.5M7 10.5h3.5" stroke="currentColor" stroke-width="1" stroke-linecap="round"/></svg>',
    columns: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3 4h10M3 8h10M3 12h6" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" opacity=".7"/></svg>',
    index: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M3.5 12.5 8 3.5l4.5 9" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/><path d="M5.5 9.5h5" stroke="currentColor" stroke-width="1" stroke-linecap="round"/></svg>',
    folderTables: '<svg viewBox="0 0 16 16" fill="none" aria-hidden="true"><path d="M2.5 4.5h4l1 1.5H13a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5.5a1 1 0 0 1 1-1Z" stroke="currentColor" stroke-width="1.1"/><path d="M5.5 8.5h5M5.5 10.5h3.5" stroke="currentColor" stroke-width=".95" stroke-linecap="round"/></svg>',
  };
  return icons[kind] || icons.column;
}

function groupDbTablesBySchema(tables) {
  const map = new Map();
  for (const table of tables) {
    const schema = table.schema || 'public';
    if (!map.has(schema)) map.set(schema, []);
    map.get(schema).push(table);
  }
  return [...map.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([schema, items]) => [schema, items.sort((x, y) => x.name.localeCompare(y.name))]);
}

function renderDbFolderNode(folderKey, label, iconKind, depth, count, childrenHtml, open) {
  return `<details class="ij-tree-dir ij-db-folder-node" data-folder-key="${escapeHtml(folderKey)}" style="--depth:${depth}"${open ? ' open' : ''}>
    <summary class="ij-tree-row ij-tree-dir-row ij-db-folder-row" style="--depth:${depth}">
      <span class="ij-tree-chevron" aria-hidden="true"></span>
      <span class="ij-tree-icon ij-db-icon-folder">${dbTreeIconSvg(iconKind)}</span>
      <span class="ij-tree-label ij-db-folder-label">${escapeHtml(label)}</span>
      <span class="ij-db-tree-meta">${count}</span>
    </summary>
    <div class="ij-tree-children">${childrenHtml}</div>
  </details>`;
}

function renderDbColumnRow(tableKey, col, depth) {
  const nn = col.nullable ? '' : '<span class="ij-db-tree-badge ij-db-tree-badge--nn" title="NOT NULL">NN</span>';
  return `<button type="button" class="ij-tree-row ij-tree-file-row ij-db-col-row" style="--depth:${depth}" data-table="${escapeHtml(tableKey)}" data-column="${escapeHtml(col.name)}" title="Show column query">
    <span class="ij-tree-icon ij-db-icon-column">${dbTreeIconSvg('column')}</span>
    <span class="ij-tree-label">${escapeHtml(col.name)}</span>
    <span class="ij-db-type-badge">${escapeHtml(col.type_name)}</span>${nn}
  </button>`;
}

function renderDbIndexRow(tableKey, idx, depth) {
  const badge = idx.primary
    ? '<span class="ij-db-tree-badge ij-db-tree-badge--pk" title="Primary key">PK</span>'
    : (idx.unique ? '<span class="ij-db-tree-badge ij-db-tree-badge--uq" title="Unique">UQ</span>' : '');
  const cols = (idx.columns || []).join(', ');
  return `<button type="button" class="ij-tree-row ij-tree-file-row ij-db-index-row" style="--depth:${depth}" data-table="${escapeHtml(tableKey)}" data-index="${escapeHtml(idx.name)}" data-columns="${escapeHtml(cols)}" data-primary="${idx.primary ? '1' : '0'}" data-unique="${idx.unique ? '1' : '0'}" title="Show index details">
    <span class="ij-tree-icon ij-db-icon-index">${dbTreeIconSvg('index')}</span>
    <span class="ij-tree-label">${escapeHtml(idx.name)}</span>${badge}
  </button>`;
}

function renderDbObjectNode(obj, depth, objectOpen, columnsOpen, indexesOpen) {
  const stateKey = dbObjectStateKey(obj);
  const sqlName = dbTableKey(obj);
  const kind = obj.kind || 'table';
  const iconKind = kind === 'materialized_view' ? 'materialized_view' : (kind === 'view' ? 'view' : 'table');
  const cols = obj.columns || [];
  const indexes = obj.indexes || [];
  const colFolderKey = `${stateKey}:columns`;
  const idxFolderKey = `${stateKey}:indexes`;
  const colRows = cols.length
    ? cols.map((col) => renderDbColumnRow(sqlName, col, depth + 2)).join('')
    : `<div class="ij-db-tree-empty-leaf" style="--depth:${depth + 2}">No columns</div>`;
  const idxRows = indexes.length
    ? indexes.map((idx) => renderDbIndexRow(sqlName, idx, depth + 2)).join('')
    : `<div class="ij-db-tree-empty-leaf" style="--depth:${depth + 2}">No indexes</div>`;
  let inner = renderDbFolderNode(colFolderKey, 'Columns', 'columns', depth + 1, cols.length, colRows, columnsOpen);
  if (kind === 'table' || kind === 'materialized_view') {
    inner += renderDbFolderNode(idxFolderKey, 'Indexes', 'index', depth + 1, indexes.length, idxRows, indexesOpen);
  }
  return `<details class="ij-tree-dir ij-db-object-node ij-db-table-node" data-object-key="${escapeHtml(stateKey)}" data-table-key="${escapeHtml(stateKey)}" data-table-sql="${escapeHtml(sqlName)}" data-kind="${escapeHtml(kind)}" style="--depth:${depth}"${objectOpen ? ' open' : ''}>
    <summary class="ij-tree-row ij-tree-dir-row ij-db-table-row" style="--depth:${depth}" title="Show object query">
      <span class="ij-tree-chevron" aria-hidden="true"></span>
      <span class="ij-tree-icon ij-db-icon-${escapeHtml(iconKind)}">${dbTreeIconSvg(iconKind)}</span>
      <span class="ij-tree-label ij-db-table-label">${escapeHtml(obj.name)}</span>
    </summary>
    <div class="ij-tree-children">${inner}</div>
  </details>`;
}

function renderDbKindFolder(schema, kindDef, objects, depth, expandAll) {
  if (!objects.length) return '';
  const folderKey = `folder:${schema}:${kindDef.kind}`;
  const folderOpen = expandAll || state.dbSchemaOpenFolders.has(folderKey);
  const itemsHtml = objects.map((obj) => {
    const stateKey = dbObjectStateKey(obj);
    const legacyKey = dbTableKey(obj);
    const objectOpen = expandAll
      || state.dbSchemaOpenTables.has(stateKey)
      || state.dbSchemaOpenTables.has(legacyKey);
    const columnsOpen = expandAll || state.dbSchemaOpenFolders.has(`${stateKey}:columns`);
    const indexesOpen = expandAll || state.dbSchemaOpenFolders.has(`${stateKey}:indexes`);
    return renderDbObjectNode(obj, depth + 1, objectOpen, columnsOpen, indexesOpen);
  }).join('');
  return renderDbFolderNode(folderKey, kindDef.label, kindDef.icon, depth, objects.length, itemsHtml, folderOpen);
}

function renderDbSchemaBody(objects, depth, expandAll) {
  const schema = objects[0]?.schema || 'public';
  const parts = partitionDbObjects(objects);
  return DB_KIND_FOLDERS.map((def) => renderDbKindFolder(schema, def, parts[def.kind], depth, expandAll))
    .filter(Boolean)
    .join('');
}

function renderDbSchemaNode(schema, objects, depth, open, expandAll) {
  const parts = partitionDbObjects(objects);
  const kindHtml = DB_KIND_FOLDERS.map((def) =>
    renderDbKindFolder(schema, def, parts[def.kind], depth + 1, expandAll),
  ).filter(Boolean).join('');
  const total = objects.length;
  return `<details class="ij-tree-dir ij-db-schema-node" data-schema="${escapeHtml(schema)}" style="--depth:${depth}"${open ? ' open' : ''}>
    <summary class="ij-tree-row ij-tree-dir-row ij-db-schema-row" style="--depth:${depth}">
      <span class="ij-tree-chevron" aria-hidden="true"></span>
      <span class="ij-tree-icon ij-db-icon-schema">${dbTreeIconSvg('schema')}</span>
      <span class="ij-tree-label">${escapeHtml(schema)}</span>
      <span class="ij-db-tree-meta">${total}</span>
    </summary>
    <div class="ij-tree-children">${kindHtml || '<div class="ij-db-tree-empty-leaf" style="--depth:1">No objects</div>'}</div>
  </details>`;
}

function markDbTreeSelected(tableDetails, itemBtn) {
  const tree = $('#db-viewer-schema')?.querySelector('.ij-db-object-tree');
  tree?.querySelectorAll('.is-selected').forEach((el) => el.classList.remove('is-selected'));
  if (tableDetails) {
    tableDetails.classList.add('is-selected');
    state.dbTreeSelection = {
      tableKey: tableDetails.dataset.objectKey || tableDetails.dataset.tableKey || null,
      column: itemBtn?.dataset.column || null,
      index: itemBtn?.dataset.index || null,
    };
  } else {
    state.dbTreeSelection = null;
  }
  itemBtn?.classList.add('is-selected');
}

function restoreDbTreeSelection(container) {
  const sel = state.dbTreeSelection;
  if (!sel?.tableKey || !container) return;
  const table = container.querySelector(
    `.ij-db-object-node[data-object-key="${CSS.escape(sel.tableKey)}"], .ij-db-object-node[data-table-key="${CSS.escape(sel.tableKey)}"]`,
  );
  if (!table) return;
  table.classList.add('is-selected');
  if (sel.column) {
    const col = table.querySelector(`.ij-db-col-row[data-column="${CSS.escape(sel.column)}"]`);
    col?.classList.add('is-selected');
  } else if (sel.index) {
    const idx = table.querySelector(`.ij-db-index-row[data-index="${CSS.escape(sel.index)}"]`);
    idx?.classList.add('is-selected');
  }
}

function wireDbObjectTree(container) {
  container.querySelectorAll('.ij-db-schema-node').forEach((details) => {
    details.addEventListener('toggle', () => {
      const schema = details.dataset.schema;
      if (!schema) return;
      if (details.open) state.dbSchemaOpenSchemas.add(schema);
      else state.dbSchemaOpenSchemas.delete(schema);
    });
  });
  container.querySelectorAll('.ij-db-folder-node').forEach((details) => {
    details.addEventListener('toggle', () => {
      const key = details.dataset.folderKey;
      if (!key) return;
      if (details.open) state.dbSchemaOpenFolders.add(key);
      else state.dbSchemaOpenFolders.delete(key);
    });
  });
  container.querySelectorAll('.ij-db-object-node').forEach((details) => {
    details.addEventListener('toggle', () => {
      const key = details.dataset.objectKey || details.dataset.tableKey;
      if (!key) return;
      if (details.open) state.dbSchemaOpenTables.add(key);
      else state.dbSchemaOpenTables.delete(key);
    });
    const summary = details.querySelector('summary');
    summary?.addEventListener('click', (e) => {
      if (e.target.closest('.ij-tree-chevron')) return;
      e.preventDefault();
      const sqlName = details.dataset.tableSql;
      if (!details.open) {
        details.open = true;
        state.dbSchemaOpenTables.add(details.dataset.objectKey || details.dataset.tableKey || '');
      }
      if (sqlName) loadDbSqlQuery(`SELECT * FROM ${sqlName} LIMIT 100`);
      markDbTreeSelected(details, null);
    });
  });
  container.querySelectorAll('.ij-db-col-row').forEach((btn) => {
    btn.addEventListener('click', () => {
      const table = btn.dataset.table;
      const column = btn.dataset.column;
      if (table && column) {
        loadDbSqlQuery(`SELECT ${column} FROM ${table} LIMIT 100`);
        const tableDetails = btn.closest('.ij-db-object-node');
        if (tableDetails) markDbTreeSelected(tableDetails, btn);
      }
    });
  });
  container.querySelectorAll('.ij-db-index-row').forEach((btn) => {
    btn.addEventListener('click', () => {
      const table = btn.dataset.table;
      const index = btn.dataset.index;
      const columns = btn.dataset.columns || '';
      const flags = [
        btn.dataset.primary === '1' ? 'PRIMARY KEY' : null,
        btn.dataset.unique === '1' && btn.dataset.primary !== '1' ? 'UNIQUE' : null,
      ].filter(Boolean).join(', ');
      if (table && index) {
        const header = [`-- Index: ${index}${flags ? ` (${flags})` : ''}`];
        if (columns) header.push(`-- Columns: ${columns}`);
        loadDbSqlQuery(`${header.join('\n')}\nSELECT * FROM ${table} LIMIT 100`);
        const tableDetails = btn.closest('.ij-db-object-node');
        if (tableDetails) markDbTreeSelected(tableDetails, btn);
      }
    });
  });
  restoreDbTreeSelection(container);
}

function renderDbViewerSchema(schema) {
  const container = $('#db-viewer-schema');
  const subtitle = $('#db-viewer-subtitle');
  if (!container) return;
  const conn = dbConnFromPayload(schema) || state.dbConnection;
  updateDbViewerStatusDot(conn);
  if (subtitle) {
    subtitle.textContent = conn?.connected ? conn.display : (conn?.error || 'Not connected');
  }
  const filter = ($('#db-viewer-schema-filter')?.value || state.dbSchemaFilter || '').trim().toLowerCase();
  if (!schema?.tables?.length) {
    const msg = conn?.error
      || (conn?.connected ? 'No objects in this database' : 'Connect to a database (PostgreSQL, MySQL, or add a .sqlite file in the project)');
    container.innerHTML = `<div class="ij-db-viewer-empty">${escapeHtml(msg)}</div>`;
    return;
  }
  const objects = filterDbObjects(schema.tables, filter);
  if (!objects.length) {
    container.innerHTML = '<div class="ij-db-viewer-empty">No objects match filter</div>';
    return;
  }
  const expandMatching = !!filter;
  const groups = groupDbTablesBySchema(objects);
  const bodyHtml = groups.map(([schemaName, schemaObjects]) => {
    const schemaOpen = expandMatching || state.dbSchemaOpenSchemas.has(schemaName);
    return renderDbSchemaNode(schemaName, schemaObjects, 0, schemaOpen, expandMatching);
  }).join('');
  container.innerHTML = `<div class="ij-tree ij-db-object-tree" role="tree" aria-label="Database objects">${bodyHtml}</div>`;
  wireDbObjectTree(container);
}

async function refreshDbViewerPanel() {
  await loadDbConnection();
  if (state.activeTab?.endsWith('.sql')) {
    await refreshRunInfo();
  }
  await refreshDbSchema();
}

function scheduleDbViewerRefresh() {
  const tab = stripJavaDiagOverlayPath(state.activeTab || '').trim();
  if (!tab || !state.repo) return;
  if (tab.endsWith('.sql') || tab.endsWith('.env') || isDockerComposeFile(tab)) {
    if (tab.endsWith('.sql') && !state.dbViewerPanelOpen) {
      state.dbViewerPanelOpen = true;
      applyDbViewerPanelLayout();
      seedDbSqlFromActiveTab();
    }
    void refreshDbViewerPanel();
  }
}

async function fetchAndApplyCoverage(path) {
  if (!state.repo || !path) return null;
  try {
    const cov = await api(
      `${repoApi(state.repo, '/workspace/coverage')}?path=${encodeURIComponent(stripJavaDiagOverlayPath(path))}`,
    );
    const target = cov?.coverage_path || path;
    if (coverageHasUsableData(cov)) {
      state.fileCoverage.set(target, cov);
      if (target !== path) {
        state.fileCoverage.set(path, cov);
      }
      if (coverageHasLineData(cov) && state.activeTab === target) {
        applyCoverageDecorations(target, cov);
      } else {
        clearCoverageDecorations();
      }
      updateCoverageStatus(cov);
      if (state.coveragePanelOpen) {
        void refreshCoveragePanel(path);
      }
      if (cov.coverage_path && cov.coverage_path !== path && cov.message) {
        toast(cov.message, 'info', { duration: 7000 });
      }
      return cov;
    }
    updateCoverageStatus(null);
    clearCoverageDecorations();
    if (state.coveragePanelOpen) {
      void refreshCoveragePanel(path);
    }
    return cov;
  } catch (e) {
    toast(`Coverage: ${e.message}`, 'warning');
    return null;
  }
}

function reapplyCoverageForTab(path) {
  if (!path) return;
  if (!getCoverageInlineEnabled()) {
    clearCoverageDecorations();
    let covOff = state.fileCoverage?.get(path);
    if (!coverageHasUsableData(covOff) && covOff?.coverage_path) {
      covOff = state.fileCoverage.get(covOff.coverage_path) || covOff;
    }
    updateCoverageStatus(coverageHasUsableData(covOff) ? covOff : null);
    return;
  }
  let cov = state.fileCoverage?.get(path);
  if (!coverageHasLineData(cov) && cov?.coverage_path) {
    cov = state.fileCoverage.get(cov.coverage_path) || cov;
  }
  const decoratePath = cov?.coverage_path || path;
  if (coverageHasLineData(cov) && state.activeTab === decoratePath) {
    applyCoverageDecorations(decoratePath, cov);
    updateCoverageStatus(cov);
    return;
  }
  if (coverageHasUsableData(cov)) {
    updateCoverageStatus(cov);
    clearCoverageDecorations();
    return;
  }
  clearCoverageDecorations();
  updateCoverageStatus(null);
}

function applyConflictDecorations() {
  if (!state.editor || !window.monaco) return;
  const model = state.editor.getModel();
  if (!model || !state.activeTab) return;
  const conflicts = parseConflictMarkers(model.getValue());
  const decorations = conflicts.map((c) => ({
    range: new monaco.Range(c.start + 1, 1, c.end + 1, 1),
    options: {
      isWholeLine: true,
      className: 'ij-conflict-line',
      overviewRuler: {
        color: '#f44747',
        position: monaco.editor.OverviewRulerLane.Full,
      },
    },
  }));
  state.conflictDecorationIds = state.editor.deltaDecorations(
    state.conflictDecorationIds,
    decorations,
  );
}

function renderConflictHunks(conflicts) {
  const container = $('#conflict-hunks');
  if (!container) return;
  if (!conflicts.length) {
    container.innerHTML = '<div class="ij-sbs-empty">No conflict markers in this file</div>';
    return;
  }
  container.innerHTML = conflicts.map((c, idx) => `
    <div class="ij-conflict-hunk" data-hunk="${idx}">
      <div class="ij-conflict-hunk-actions">
        <button type="button" class="ij-conflict-hunk-btn ij-conflict-hunk-btn--ours" data-choice="ours" data-hunk="${idx}">Accept Ours</button>
        <button type="button" class="ij-conflict-hunk-btn ij-conflict-hunk-btn--theirs" data-choice="theirs" data-hunk="${idx}">Accept Theirs</button>
        <button type="button" class="ij-conflict-hunk-btn ij-conflict-hunk-btn--both" data-choice="both" data-hunk="${idx}">Accept Both</button>
      </div>
      <div class="ij-conflict-hunk-label">${escapeHtml(c.marker || 'Current')} vs ${escapeHtml(c.theirsMarker || 'Incoming')}</div>
      <div class="ij-conflict-sbs">
        <pre class="ij-conflict-side ij-conflict-side--ours">${escapeHtml(c.ours || '(empty)')}</pre>
        <pre class="ij-conflict-side ij-conflict-side--theirs">${escapeHtml(c.theirs || '(empty)')}</pre>
      </div>
    </div>
  `).join('');
  container.querySelectorAll('[data-choice]').forEach((btn) => {
    btn.addEventListener('click', () => {
      applyConflictChoice(Number(btn.dataset.hunk), btn.dataset.choice);
    });
  });
}

function applyConflictChoice(hunkIndex, choice) {
  if (!state.editor || !state.activeTab) return;
  const lines = state.editor.getValue().split('\n');
  const conflicts = parseConflictMarkers(lines.join('\n'));
  const c = conflicts[hunkIndex];
  if (!c) return;
  let replacement = [];
  if (choice === 'ours') replacement = c.ours.split('\n');
  else if (choice === 'theirs') replacement = c.theirs.split('\n');
  else if (choice === 'both') replacement = [...c.ours.split('\n'), ...c.theirs.split('\n')];
  const next = [...lines.slice(0, c.start), ...replacement, ...lines.slice(c.end + 1)];
  state.suppressEditorChange = true;
  state.editor.setValue(next.join('\n'));
  state.suppressEditorChange = false;
  state.tabContents.set(state.activeTab, state.editor.getValue());
  state.dirty.add(state.activeTab);
  updateSaveButton();
  renderTabs();
  updateConflictUi();
  toast(`Applied ${choice} side`, 'success');
}

async function markConflictResolved() {
  if (!state.repo || !state.activeTab) return;
  const remaining = parseConflictMarkers(state.editor?.getValue() || '');
  if (remaining.length) {
    toast('Resolve all conflict markers first', 'error');
    return;
  }
  if (state.dirty.has(state.activeTab)) {
    await saveFile();
  }
  try {
    const out = await api(repoApi(state.repo, '/workspace/conflict/resolve'), {
      method: 'POST',
      body: JSON.stringify({ path: state.activeTab }),
    });
    terminalLog(out.stdout || out.stderr || `Marked ${state.activeTab} resolved`);
    await refreshGitStatus();
    updateConflictUi();
    toast('Conflict marked resolved', 'success');
  } catch (e) {
    toast(e.message || 'Failed to mark resolved', 'error');
  }
}

async function continueMerge() {
  if (!state.repo) return;
  try {
    const out = await api(repoApi(state.repo, '/workspace/conflict/continue'), { method: 'POST' });
    terminalLog(out.stdout || out.stderr || 'Merge continued');
    await refreshGitStatus();
    await refreshTree();
    await refreshHistory();
    toast('Merge completed', 'success');
  } catch (e) {
    toast(e.message || 'Could not complete merge', 'error');
  }
}

function setMainView(mode) {
  state.mainView = mode;
  const stage = $('#editor-stage');
  const editor = $('#editor-container');
  const diff = $('#diff-panel');
  const conflict = $('#conflict-panel');
  stage?.classList.toggle('ij-editor-stage--diff', mode === 'diff');
  stage?.classList.toggle('ij-editor-stage--conflict', mode === 'conflict' && !state.conflictPanelHidden);
  if (mode === 'diff') {
    editor?.classList.add('hidden');
    diff?.classList.remove('hidden');
    conflict?.classList.add('hidden');
  } else if (mode === 'conflict') {
    editor?.classList.remove('hidden');
    diff?.classList.add('hidden');
    conflict?.classList.toggle('hidden', state.conflictPanelHidden);
  } else {
    editor?.classList.toggle('hidden', !state.activeTab);
    diff?.classList.add('hidden');
    conflict?.classList.add('hidden');
  }
  if (state.editor) {
    requestAnimationFrame(() => state.editor.layout());
  }
}

function showDiffInMainArea(title, diffText) {
  const label = $('#diff-path');
  if (label) label.textContent = title;
  renderDiffInto($('#diff-content'), diffText || '');
  setMainView('diff');
  $('#editor-toolbar')?.classList.remove('hidden');
  $('#editor-toolbar')?.classList.add('flex');
  $('#empty-state')?.classList.add('hidden');
}

function returnToEditorView() {
  setMainView('editor');
  if (state.activeTab && state.conflictFiles.has(state.activeTab)) {
    setMainView('conflict');
  }
}

function updateMergeBanner(status) {
  const banner = $('#merge-banner');
  const title = $('#merge-banner-title');
  const detail = $('#merge-banner-detail');
  const btn = $('#btn-continue-merge');
  if (!banner) return;
  const merge = status?.merge;
  const conflicts = status?.conflict_count || 0;
  state.mergeState = merge || null;
  if (merge?.active) {
    banner.classList.remove('hidden');
    const kind = merge.kind || 'merge';
    if (title) title.textContent = `${kind.charAt(0).toUpperCase()}${kind.slice(1)} in progress`;
    if (detail) {
      detail.textContent = conflicts
        ? `${conflicts} conflicted file${conflicts === 1 ? '' : 's'} remaining`
        : 'All conflicts resolved — ready to continue';
    }
    if (btn) btn.disabled = conflicts > 0;
  } else {
    banner.classList.add('hidden');
  }
}

function updateConflictUi() {
  const panel = $('#conflict-panel');
  const pathEl = $('#conflict-path');
  if (!panel) return;
  const isConflictFile = state.activeTab && state.conflictFiles.has(state.activeTab);
  if (!isConflictFile || !state.editor) {
    state.conflictDecorationIds = state.editor
      ? state.editor.deltaDecorations(state.conflictDecorationIds, [])
      : [];
    if (state.mainView === 'conflict') setMainView('editor');
    return;
  }
  const content = state.editor.getValue();
  const conflicts = parseConflictMarkers(content);
  if (pathEl) pathEl.textContent = state.activeTab;
  renderConflictHunks(conflicts);
  applyConflictDecorations();
  const markBtn = $('#btn-mark-conflict-resolved');
  if (markBtn) markBtn.disabled = conflicts.length > 0;
  if (state.mainView !== 'diff') {
    setMainView('conflict');
  }
}

async function openConflictFile(path) {
  state.conflictPanelHidden = false;
  if (!state.tabs.includes(path)) {
    await openFile(path);
  } else {
    activateTab(path);
  }
  switchPanel('explorer');
  updateConflictUi();
}

// --- Monaco ---
let pendingEditorPath = null;
let pendingEditorContent = null;

function setEditorContent(path, content) {
  const text = content ?? '';
  if (!state.editor) {
    pendingEditorPath = path;
    pendingEditorContent = text;
    return;
  }
  pendingEditorPath = null;
  pendingEditorContent = null;
  state.suppressEditorChange = true;
  try {
    state.editor.setValue(text);
    const model = state.editor.getModel();
    if (window.ReaperLang?.applyEditorLanguage) {
      window.ReaperLang.applyEditorLanguage(path, model);
    } else {
      const lang = langForPath(path);
      window.ReaperLang?.ensureMonacoBasicLanguage?.(lang);
      monaco.editor.setModelLanguage(model, lang);
    }
  } finally {
    state.suppressEditorChange = false;
  }
  // setValue invalidates prior decoration IDs — reset before reapplying.
  state.debugDecorationIds = [];
  state.debugCurrentLineId = [];
  applyTestRunDecorations();
  reapplyCoverageForTab(path);
  renderBreakpointGlyphs();
  highlightDebugCurrentLine();
  if (state.blameEnabled) {
    const cached = state.blameByPath.get(path);
    if (cached) applyBlameDecorations(path, cached);
    else void loadBlameForTab(path);
  } else {
    clearBlameDecorations();
  }
  syncBlameButton();
  applyEditorReadOnlyForPath(path);
  if (path?.endsWith('.java') && state.repo) {
    void fetchAndApplyCoverage(path);
  }
}

function flushPendingEditorContent() {
  if (!state.editor || pendingEditorPath == null) return;
  const path = pendingEditorPath;
  const content = pendingEditorContent;
  pendingEditorPath = null;
  pendingEditorContent = null;
  setEditorContent(path, content);
}

function flushPendingEditorContent() {
  if (!state.editor || pendingEditorPath == null) return;
  const path = pendingEditorPath;
  const content = pendingEditorContent;
  pendingEditorPath = null;
  pendingEditorContent = null;
  setEditorContent(path, content);
}

function syncEditorFromActiveTab() {
  if (!state.editor || !state.activeTab || !state.tabContents.has(state.activeTab)) return;
  setEditorContent(state.activeTab, state.tabContents.get(state.activeTab));
  flushPendingEditorContent();
}

function defaultReadmeContent(path) {
  const name = path.split('/').pop();
  if (name?.toLowerCase() !== 'readme.md') return '';
  const title = state.repo || 'Project';
  return `# ${title}\n\nManaged by Reaper.\n`;
}

const MONACO_LOCAL_BASE = '/vendor/monaco-editor/min';

function monacoBaseUrl() {
  return MONACO_LOCAL_BASE;
}

function monacoVsBaseUrl() {
  return `${monacoBaseUrl()}/vs`;
}

function configureMonacoEnvironment() {
  const base = monacoBaseUrl();
  const workerMain = `${base}/vs/base/worker/workerMain.js`;
  window.MonacoEnvironment = {
    getWorkerUrl() {
      // WKWebView blocks cross-origin Worker URLs; bootstrap via data URL + same-origin or CDN importScripts.
      return `data:text/javascript;charset=utf-8,${encodeURIComponent(
        `self.MonacoEnvironment = { baseUrl: '${base}' };importScripts('${workerMain}');`,
      )}`;
    },
  };
}

function initEditor() {
  configureMonacoEnvironment();
  require.config({ paths: { vs: monacoVsBaseUrl() } });
  require(['vs/editor/editor.main'], () => {
    ensureGeminiReady().finally(() => {
    void refreshJavaLanguageLevel();
    window.ReaperThemes?.defineMonacoThemes();
    const activeTheme = window.ReaperThemes?.getTheme(window.ReaperThemes.getStoredTheme());
    if (activeTheme) {
      window.ReaperThemes?.syncMonacoEditorTheme?.(activeTheme);
    }
    const fontSize = getEditorFontSize();
    const fontSpec = getEditorFontSpec();
    ensureEditorFontLoaded(fontSpec);
    state.editor = monaco.editor.create($('#editor'), {
      value: '',
      language: 'plaintext',
      theme: window.ReaperThemes?.getMonacoThemeId() || 'reaper-navy',
      fixedOverflowWidgets: true,
      overflowWidgetsDomNode: document.getElementById('editor-overflow-root'),
      fontFamily: fontSpec.family,
      fontSize,
      lineHeight: editorLineHeightFor(fontSize),
      lineNumbers: 'on',
      glyphMargin: true,
      minimap: { enabled: true, showSlider: 'mouseover' },
      scrollBeyondLastLine: false,
      automaticLayout: true,
      padding: { top: 4 },
      folding: true,
      bracketPairColorization: { enabled: true },
      guides: { bracketPairs: true, indentation: true },
      renderLineHighlight: 'all',
      cursorBlinking: 'solid',
      cursorStyle: 'line',
      smoothScrolling: true,
      links: true,
      hover: { enabled: true, delay: 300, sticky: true },
      wordBasedSuggestions: 'off',
      quickSuggestions: { other: true, strings: true, comments: false },
      quickSuggestionsDelay: 120,
      suggestOnTriggerCharacters: true,
      suggest: {
        preview: true,
        showIcons: true,
        snippetsPreventQuickSuggestions: false,
        filterGraceful: true,
        localityBonus: true,
        showStatusBar: false,
        shareSuggestSelections: false,
        showInlineDetails: true,
        acceptSuggestionOnCommitCharacter: false,
      },
      inlineSuggest: {
        enabled: true,
        showToolbar: false,
        suppressSuggestions: false,
        mode: 'prefix',
      },
      lightbulb: {
        enabled: monaco.editor.ShowLightbulbIconMode?.Off ?? false,
      },
      tabCompletion: 'on',
      renderWhitespace: 'selection',
      renderValidationDecorations: 'on',
      unicodeHighlight: {
        ambiguousCharacters: false,
        invisibleCharacters: false,
        nonBasicASCII: false,
      },
      overviewRulerBorder: false,
      overviewRulerLanes: 2,
    });
    setupDiagnosticNavigation(state.editor);
    setupDebugEditorHooks(state.editor);
    try {
      if (window.__reaperLangBundleError) {
        throw new Error('monaco-languages.js failed to load (check console for parse errors)');
      }
      if (!window.__reaperLangBundleLoaded) {
        throw new Error('monaco-languages.js did not finish initializing');
      }
      if (!window.ReaperLang?.setupEditorFeatures) {
        throw new Error('ReaperLang.setupEditorFeatures missing');
      }
      window.ReaperLang.setupEditorFeatures(state.editor, {
      api,
      repoApi,
      getRepo: () => state.repo,
      getActivePath: () => state.activeTab,
      getEditor: () => state.editor,
      openFileAt,
      goToLine,
      isFileDirty: (path) => state.dirty.has(path),
      lookupDebugHoverValue,
      isDebugStopped: () => state.debugState?.status === 'stopped',
      getJavaSourceOverlays: (excludePath) => collectJavaDiagnosticOverlays(excludePath),
      getAiInlineComplete: () => getAiInlineCompleteEnabled(),
      getAiInlineProviderAvailable: () => getAiInlineProviderAvailable(),
      getGeminiConfigured: () => state.geminiConfigured,
      getCursorConfigured: () => state.cursorConfigured,
      getCursorInlineAvailable: () => state.cursorConfigured && state.cursorBridgeOk,
      getAnthropicConfigured: () => state.anthropicConfigured,
      getBedrockConfigured: () => state.bedrockConfigured,
      getJavaLanguageLevel: () => state.javaLanguageLevel || 17,
      getLanguageContext: () => state.languageContext,
      isJdtlsReady: () => !!state.jdtlsReady,
      markJdtlsReady: () => { state.jdtlsReady = true; },
      getActiveTabContent: () => {
        if (!state.activeTab) return '';
        return state.tabContents.get(state.activeTab) ?? state.editor?.getModel()?.getValue() ?? '';
      },
      toast,
      terminalLog,
      setCompleteDebugStatus,
      setStatusMessage,
      runWithNavigationBusy,
      showNavigationResult,
      showQuickFixMenu,
      showRefactorStaircaseMenu,
      hideQuickFixMenu,
      isQuickFixMenuOpen,
      scheduleDiagnostics: () => scheduleDiagnostics(),
      refreshDiagnosticsAfterFix: () => {
        flushEditorContentSync();
        const path = state.activeTab;
        if (path?.endsWith('.java') && state.editor) {
          setTimeout(() => {
            if (state.activeTab !== path) return;
            const latest = state.editor?.getValue();
            if (latest == null) return;
            void (async () => {
              try {
                await writeTabToDisk(path, latest);
              } catch (err) {
                console.warn('[Reaper] failed to persist after quick fix', err);
              }
              queueJavaFullDiagnostics(path, latest, { immediate: true, force: true });
            })();
          }, 0);
          return;
        }
        scheduleDiagnostics();
      },
      getDiagnosticsInRange: (model, range) => {
        if (!model || !range || !fileDiags.length) return [];
        return fileDiags.filter((d) => {
          const span = diagnosticMarkerSpan(model, d);
          if (range.endLineNumber < span.startLineNumber
            || range.startLineNumber > span.endLineNumber) {
            return false;
          }
          return true;
        });
      },
      diagnosticSpan: (model, d) => diagnosticMarkerSpan(model, d),
      diagnosticFriendlyHint,
      setLanguageStatus: () => {
        updateStatusLanguage(state.activeTab);
      },
      getDbSchema: () => state.dbSchema,
      showJavaReferences,
      hideJavaReferences,
      applyJavaWorkspaceEdits,
      renameWorkspacePath,
      promptRename: showRenamePrompt,
    });
      setCompleteDebugStatus('editor features OK');
      if (window.__reaperLangBundleError) {
        console.warn('[Reaper] Completion bundle failed to load — using core JDK stubs only');
      }
    } catch (err) {
      const msg = err?.message || String(err);
      setCompleteDebugStatus(`features ERR: ${msg}`);
      toast(`Editor features failed: ${msg}`, 'error', { duration: 20000 });
      console.error('[Reaper] setupEditorFeatures', err);
    }
    state.editor.onDidChangeModelContent(() => {
      if (state.suppressEditorChange || !state.activeTab) return;
      if (isExternalEditorPath(state.activeTab)) return;
      markActiveTabDirty();
      scheduleEditorContentSync();
      if (isRunToolbarPath(state.activeTab)) refreshRunInfo();
      else if (state.activeTab?.endsWith('.java')) updateRunButtons();
      scheduleRenderTabs();
      scheduleAutoSave();
      scheduleDiagnostics();
      scheduleStructureRefresh();
      if (shouldAutoOpenBuildTasks(state.activeTab)) {
        if (state.buildTasksPanelOpen) scheduleBuildTasksRefresh();
      } else if (shouldAutoOpenPackageManifest(state.activeTab)) {
        schedulePackageManifestRefresh();
      }
      if (state.conflictFiles.has(state.activeTab)) updateConflictUi();
      scheduleTestRunDecorations();
      scheduleCoverageClear();
      const model = state.editor.getModel();
      const position = state.editor.getPosition();
      if (model && position) {
        const ctx = window.ReaperLang?.memberDotContext?.(model, position);
        const linePrefix = ctx?.linePrefix
          ?? model.getValueInRange(new monaco.Range(
            position.lineNumber, 1, position.lineNumber, position.column,
          ));
        if (linePrefix.trimEnd().endsWith('.')) {
          window.ReaperLang?.handleDotCompletion?.(state.editor);
        }
      }
    });
    state.editor.onDidChangeCursorPosition((e) => {
      updateEditorStatus(e.position);
      scheduleStructureCaretHighlight();
      if (state.activeTab?.endsWith('.java') || state.activeTab?.endsWith('.rb')
        || state.activeTab?.endsWith('.py') || state.activeTab?.endsWith('.pyw')
        || state.activeTab?.endsWith('.go') || isNativeSourcePath(state.activeTab)) updateRunButtons();
    });
    window.ReaperThemes?.syncMonacoOverflowWidgetTheme?.(
      window.ReaperThemes.getTheme(window.ReaperThemes.getStoredTheme()).dark,
    );
    state.editorReady = true;
    flushPendingEditorContent();
    syncEditorFromActiveTab();
    });
  }, (err) => {
    const msg = err?.requireType || err?.message || String(err);
    toast(`Monaco failed to load: ${msg}`, 'error', { duration: 25000 });
    console.error('[Reaper] monaco require', err);
  });
}

// --- Repos ---
async function loadRepos() {
  const [repos, hiddenRepos] = await Promise.all([
    api('/api/repos'),
    api('/api/repos/hidden').catch(() => []),
  ]);
  state.repos = repos;
  state.hiddenRepos = hiddenRepos;
  setRepoPickerLabel(state.repo);
  updateHeaderBrand();
  if ($('#repo-picker-overlay')?.classList.contains('open')) {
    renderRepoPickerResults();
  }
  if (!state.tabs.length && !state.repo) {
    renderWelcome();
    $('#empty-state')?.classList.remove('hidden');
    syncWelcomeLayout();
  }
}

let repoSelectChain = Promise.resolve();
let repoSelectToken = 0;

/** Repo metadata, tree, git, history, and README after workspace/open (spinner already dismissed). */
async function hydrateRepoWorkspace(name, opened, token) {
  try {
    const detailP = api(repoApi(name), { allowDuringSave: true });
    await refreshTree({ resetExpanded: true });
    if (token !== repoSelectToken) return;

    const detail = await detailP;
    if (token !== repoSelectToken) return;
    state.branches = normalizeBranchList(detail.branches);
    state.defaultBranch = resolveDefaultBranch(
      detail.default_branch || detail.summary?.default_branch || '',
      state.branches,
    );
    updateBranchSelect();
    updateBranchPickerState();
    updateDefaultBranchUi();
    updateRepoInfo(detail);
    updateAgentUi();
    updateHeaderBrand();

    // Show the project shell as soon as the tree is ready.
    $('#empty-state')?.classList.add('hidden');
    syncWelcomeLayout();
    updateMenuState();
    setGlobalLoading(false);

    if (opened?.jdtls?.warming) {
      state.jdtlsReady = false;
      terminalLog('Starting Java language server…');
    } else if (opened?.jdtls?.ready) {
      state.jdtlsReady = true;
      terminalLog('Java language server ready');
    } else {
      state.jdtlsReady = !!opened?.jdtls?.ready;
    }

    void refreshGitStatus().catch((err) => {
      console.warn('[Reaper] git status during open', err);
    });
    void refreshHistory().catch((err) => {
      console.warn('[Reaper] git history during open', err);
    });

    await openInitialWorkspaceFile();
    if (token !== repoSelectToken) return;
    terminalLog(`Opened workspace: ${name}`);
    void warmCursorSession(name);
  } catch (err) {
    if (token !== repoSelectToken) return;
    console.warn('[Reaper] repo hydrate failed', err);
    toast(err.message || 'Failed to load project files', 'warning');
    $('#empty-state')?.classList.add('hidden');
    syncWelcomeLayout();
  } finally {
    setGlobalLoading(false);
    dismissLaunchSplashNow();
  }
}

async function openInitialWorkspaceFile() {
  await openFile('README.md', { silent: true });
  if (state.activeTab) return;
  for (const fallback of ['pom.xml', 'build.gradle', 'build.gradle.kts']) {
    await openFile(fallback, { silent: true });
    if (state.activeTab) break;
  }
}

async function selectRepoOnce(name, token) {
  if (!name) {
    state.repo = null;
    lastRemoteFetchMs = 0;
    state.projectFolder = null;
    resetUI();
    updateWindowTitle();
    setRepoPickerLabel('');
    return;
  }

  state.lastRepo = name;
  const previousRepo = state.repo;
  const switching = previousRepo !== name;
  const showLoader = switching || !previousRepo;
  if (switching) {
    lastRemoteFetchMs = 0;
    leaveSaveGate();
    closeWorkspaceTabs();
    hideBuildTasksPanel();
    hidePackageManifestPanel();
    hideDbViewerPanel();
    hideGitViewerPanel();
    hideDebugPanel();
    disconnectDebugWs();
    state.debugActive = false;
    state.debugState = { status: 'idle', frames: [], variables: [], breakpoints: [] };
    state.buildTasksTree = null;
    state.packageManifestView = null;
    state.dbQueryResult = null;
    state.gitViewerLastResult = null;
  }

  let loaderOn = false;
  try {
    if (showLoader) {
      dismissLaunchSplashNow();
      setGlobalLoading(true, `Opening ${name}…`);
      loaderOn = true;
    }
    const opened = await api(repoApi(name, '/workspace/open'), {
      method: 'POST',
      allowDuringSave: true,
      timeoutMs: 90_000,
    });
    if (token !== repoSelectToken) return;

    state.repo = name;
    resetTerminalCwds();
    if (isTerminalPanelVisible()) mountActiveTerminal();
    state.projectProfile = opened?.profile || null;
    state.projectFolder = opened?.path || null;
    startProjectIndexPolling();
    updateProjectReloadButton();
    enableControls();
    updateWindowTitle();
    setRepoPickerLabel(name);
    updateHeaderBrand();

    if (loaderOn) {
      setGlobalLoading(false);
      loaderOn = false;
    }

    void hydrateRepoWorkspace(name, opened, token);
  } catch (err) {
    if (token !== repoSelectToken) return;
    toast(err.message, 'error');
    if (switching) {
      state.repo = previousRepo || null;
      if (state.repo) {
        setRepoPickerLabel(state.repo);
      } else {
        resetUI();
        updateWindowTitle();
        setRepoPickerLabel('');
      }
    }
  } finally {
    if (loaderOn) setGlobalLoading(false);
    dismissLaunchSplashNow();
  }
}

function selectRepo(name) {
  const token = ++repoSelectToken;
  const job = repoSelectChain.then(() => selectRepoOnce(name, token));
  repoSelectChain = job.catch(() => {});
  return job;
}

function showNoRepoFileTree() {
  const treeEl = $('#file-tree');
  if (!treeEl) return;
  treeEl.innerHTML = `<div class="ij-tree-empty px-3 py-4 text-center text-xs space-y-2">
    <p class="text-gray-600">No repository open</p>
    <button type="button" class="ij-tree-import-btn" data-import="local">Import local folder…</button>
    <button type="button" class="ij-tree-import-btn" data-import="remote">Import from URL…</button>
  </div>`;
  treeEl.querySelectorAll('[data-import]').forEach((btn) => {
    btn.addEventListener('click', () => showCloneModal(btn.dataset.import === 'local' ? 'local' : 'remote'));
  });
}

function resetUI() {
  state.treeNavAnchor = null;
  state.jdtlsReady = false;
  resetTerminalCwds();
  updateTreeBackButton();
  stopProjectIndexPolling();
  clearTimeout(state.projectReloadTimer);
  state.projectReloadTimer = null;
  state.projectReloadPending = false;
  state.projectReloadBackground = false;
  showNoRepoFileTree();
  $('#git-status-list').innerHTML = '';
  $('#commit-history').innerHTML = '';
  state.commitSelectedPaths = new Set();
  state.commitKnownPaths = new Set();
  state.lastGitStatusFiles = [];
  state.mergeBlockedCommit = false;
  state.repoDetail = null;
  state.projectFolder = null;
  state.defaultBranch = '';
  const btnRepoInfo = $('#btn-repo-info');
  if (btnRepoInfo) btnRepoInfo.disabled = true;
  $('#branch-picker-btn').disabled = true;
  setBranchLabel('');
  updateDefaultBranchUi();
  ['#btn-sync', '#btn-nav-commit', '#btn-nav-push', '#btn-save', '#tb-save', '#tb-format', '#tb-rollback', '#tb-reload-project', '#btn-reload-project', '#tb-run', '#btn-commit-only', '#btn-commit-push', '#btn-suggest-commit', '#btn-new-file', '#gradle-task'].forEach((s) => { const el = $(s); if (el) el.disabled = true; });
  $('#tb-reload-project')?.classList.add('hidden');
  $('#btn-reload-project')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.remove('flex');
  closeAllTabs();
  $('#empty-state')?.classList.remove('hidden');
  syncWelcomeLayout();
  setMainView('editor');
  $('#editor-container')?.classList.add('hidden');
  updateAgentUi();
  updateStatusBar();
  updateMenuState();
  setRepoPickerLabel('');
  state.projectProfile = null;
  updateHeaderBrand();
}

function updateBranchPickerState() {
  const btn = $('#branch-picker-btn');
  if (btn) btn.disabled = !state.repo || !state.branches.length;
}

function enableControls() {
  updateBranchPickerState();
  const btnRepoInfo = $('#btn-repo-info');
  if (btnRepoInfo) btnRepoInfo.disabled = false;
  ['#btn-sync', '#btn-nav-commit', '#btn-nav-push', '#btn-save', '#tb-save', '#tb-format', '#tb-run', '#btn-commit-only', '#btn-commit-push', '#btn-suggest-commit', '#btn-new-file', '#gradle-task'].forEach((s) => { const el = $(s); if (el) el.disabled = false; });
  updateRunButtons();
  updateRollbackButton();
  updateMenuState();
}

function updateBranchSelect() {
  if (state.currentBranch && state.branches.includes(state.currentBranch)) return;
  const def = state.defaultBranch;
  if (def && state.branches.includes(def)) {
    setBranchLabel(def);
  } else if (state.branches.length) {
    setBranchLabel(state.branches[0]);
  }
}

function updateRepoInfo(detail) {
  state.repoDetail = detail;
  const s = detail.summary || detail;
  const projectFolder = state.projectFolder || s.project_folder || '';
  const el = $('#repo-info');
  if (!el) return;
  el.innerHTML = `
    <div>
      <div class="text-white font-medium">${s.name}</div>
      ${s.description ? `<p class="text-gray-500 text-xs mt-1">${s.description}</p>` : ''}
    </div>
    ${projectFolder ? `
    <div class="space-y-2">
      <div class="text-xs text-gray-500">Project folder</div>
      <code class="block text-xs bg-surface-950 border border-surface-700 rounded px-2 py-1.5 text-gray-300 break-all select-all">${escapeHtml(projectFolder)}</code>
      <p class="text-[11px] text-gray-600">All edits and git changes are saved here.</p>
    </div>` : ''}
    <div class="space-y-2">
      <div class="text-xs text-gray-500">Clone URL</div>
      <code class="block text-xs bg-surface-950 border border-surface-700 rounded px-2 py-1.5 text-accent break-all select-all">${s.clone_url}</code>
    </div>
    ${s.remote_url ? `
    <div class="space-y-2">
      <div class="text-xs text-gray-500">Linked remote</div>
      <code class="block text-xs bg-surface-950 border border-surface-700 rounded px-2 py-1.5 text-gray-300 break-all select-all">${s.remote_url}</code>
      ${s.remote_configured ? '' : '<p class="text-[11px] text-git-modified">Add a PAT for this host in <button type="button" id="repo-info-open-settings" class="text-accent hover:underline">Settings → Git hosts</button>.</p>'}
    </div>` : ''}
    <div class="flex flex-col gap-2">
      <button id="btn-publish-github" type="button" class="w-full py-2 text-xs rounded border border-accent/40 text-accent hover:bg-accent/10">Publish to remote</button>
      ${s.remote_url ? '<button id="btn-push-remote" type="button" class="w-full py-2 text-xs rounded border border-surface-700 text-gray-300 hover:bg-surface-800">Push to remote</button>' : ''}
    </div>
    <div class="space-y-2">
      <div class="text-xs text-gray-500">Default branch</div>
      <div class="text-sm text-white font-mono">${s.default_branch || '—'}</div>
    </div>
    <div class="grid grid-cols-2 gap-3">
      <div class="bg-surface-950 rounded p-3 border border-surface-700">
        <div class="text-2xl font-semibold text-white">${s.branch_count}</div>
        <div class="text-xs text-gray-500">Branches</div>
      </div>
      <div class="bg-surface-950 rounded p-3 border border-surface-700">
        <div class="text-2xl font-semibold text-white">${s.commit_count}</div>
        <div class="text-xs text-gray-500">Commits</div>
      </div>
    </div>
    <button id="btn-delete-repo" type="button" class="w-full py-2 text-xs rounded border border-red-900/50 text-red-400 hover:bg-red-900/20">Delete repository</button>
  `;
  $('#btn-delete-repo')?.addEventListener('click', () => {
    hideRepoInfoModal();
    deleteRepo();
  });
  $('#btn-publish-github')?.addEventListener('click', () => {
    hideRepoInfoModal();
    showPublishModal();
  });
  $('#btn-push-remote')?.addEventListener('click', async () => {
    hideRepoInfoModal();
    await pushRemote();
  });
  $('#repo-info-open-settings')?.addEventListener('click', () => {
    hideRepoInfoModal();
    showSettingsModal('git');
  });
}

async function showRepoInfoModal() {
  if (!state.repo) {
    toast('Select a repository first', 'info');
    return;
  }
  try {
    const detail = await api(repoApi(state.repo));
    updateRepoInfo(detail);
  } catch (e) {
    if (state.repoDetail) updateRepoInfo(state.repoDetail);
    else {
      toast(e.message || 'Failed to load repository details', 'error');
      return;
    }
  }
  const overlay = $('#repo-info-overlay');
  overlay?.classList.remove('hidden');
  overlay?.classList.add('flex');
}

function hideRepoInfoModal() {
  const overlay = $('#repo-info-overlay');
  overlay?.classList.add('hidden');
  overlay?.classList.remove('flex');
}

async function deleteRepo() {
  if (!state.repo || !confirm(`Delete ${state.repo}? This cannot be undone.`)) return;
  const repoName = state.repo;
  setGlobalLoading(true, `Deleting ${repoName}…`);
  try {
    await api(repoApi(repoName), { method: 'DELETE' });
    toast(`Deleted ${repoName}`, 'success');
    hideRepoInfoModal();
    state.repo = null;
    state.repoDetail = null;
    setRepoPickerLabel('');
    resetUI();
    await loadRepos();
  } catch (err) {
    toast(err.message, 'error');
  } finally {
    setGlobalLoading(false);
  }
}

async function createRepo(e) {
  e.preventDefault();
  const fd = new FormData(e.target);
  const body = {
    name: fd.get('name'),
    description: fd.get('description') || null,
    init_with_readme: fd.get('init_readme') === 'on',
  };
  setGlobalLoading(true, 'Creating repository…');
  try {
    const repo = await api('/api/repos', { method: 'POST', body: JSON.stringify(body) });
    hideModal();
    e.target.reset();
    await loadRepos();
    setGlobalLoading(false);
    await selectRepo(repo.name);
    toast(`Created ${repo.name}`, 'success');
  } catch (err) {
    toast(err.message, 'error');
  } finally {
    setGlobalLoading(false);
  }
}

function showCloneModal(source = 'remote') {
  const nameInput = $('#clone-local-name');
  if (nameInput) nameInput.dataset.userEdited = '';
  setCloneModalTab(source);
  setCloneModalState({ busy: false, status: '', error: '' });
  void populateCloneRecentDropdowns();
  $('#clone-modal-overlay')?.classList.remove('hidden');
  $('#clone-modal-overlay')?.classList.add('flex');
}

function uniqueNonEmpty(values) {
  const out = [];
  const seen = new Set();
  for (const raw of values) {
    const v = String(raw || '').trim();
    if (!v) continue;
    const key = v.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(v);
  }
  return out;
}

function fillRecentSelect(selectEl, values, placeholder) {
  if (!selectEl) return;
  const list = uniqueNonEmpty(values);
  selectEl.innerHTML = [
    `<option value="">${escapeHtml(placeholder)}</option>`,
    ...list.map((v) => `<option value="${escapeHtml(v)}">${escapeHtml(v)}</option>`),
  ].join('');
  selectEl.disabled = list.length === 0;
  selectEl.value = '';
}

async function populateCloneRecentDropdowns() {
  try {
    const general = await api('/api/settings/general');
    state.lastRepo = general?.last_repo || null;
    state.recentGitRemotes = Array.isArray(general?.recent_git_remotes)
      ? general.recent_git_remotes
      : [];
    state.recentGitLocalPaths = Array.isArray(general?.recent_git_local_paths)
      ? general.recent_git_local_paths
      : [];
  } catch {
    /* keep cached */
  }
  const remotes = [
    ...(state.recentGitRemotes || []),
    ...(state.repos || []).map((r) => r.remote_url).filter(Boolean),
  ];
  const locals = [
    ...(state.recentGitLocalPaths || []),
    ...(state.repos || []).map((r) => r.project_folder).filter(Boolean),
  ];
  fillRecentSelect($('#clone-recent-remote'), remotes, 'Choose a recent URL…');
  fillRecentSelect($('#clone-recent-local'), locals, 'Choose a recent folder…');
}

function hideCloneModal() {
  if (state.cloneBusy) return;
  setCloneModalState({ busy: false, status: '', error: '' });
  $('#clone-modal-overlay')?.classList.add('hidden');
  $('#clone-modal-overlay')?.classList.remove('flex');
}

async function browseLocalRepoFolder() {
  if (state.cloneBusy) return;
  const browseBtn = $('#clone-local-browse');
  if (browseBtn) browseBtn.disabled = true;
  try {
    const res = await api('/api/system/pick-folder', { method: 'POST', body: '{}' });
    const path = res?.path?.trim();
    if (!path) return;
    const input = $('#clone-local-path');
    if (input) {
      input.value = path;
      input.dispatchEvent(new Event('input', { bubbles: true }));
    }
  } catch (err) {
    toast(err.message, 'error');
  } finally {
    if (browseBtn && !state.cloneBusy) browseBtn.disabled = false;
  }
}

function syncPublishHostUi() {
  const host = $('#publish-host')?.value || 'github.com';
  const label = $('#publish-remote-label');
  const input = $('#publish-remote-url');
  const createWrap = $('#publish-create-wrap');
  const createLabel = $('#publish-create-label');
  const custom = host === 'custom';
  if (label) label.textContent = custom ? 'HTTPS remote URL' : 'Repository';
  if (input) {
    input.placeholder = custom
      ? 'https://gitlab.com/group/project.git'
      : host === 'gitlab.com'
        ? 'group/project or https://gitlab.com/group/project.git'
        : host === 'bitbucket.org'
          ? 'workspace/repo or https://bitbucket.org/workspace/repo.git'
          : 'owner/repo or https://github.com/owner/repo';
  }
  if (createWrap) createWrap.classList.toggle('hidden', custom);
  if (createLabel) {
    const hostName = custom ? 'remote' : host.replace('.com', '');
    createLabel.textContent = `Create repository on ${hostName} if it does not exist`;
  }
}

function showPublishModal() {
  if (!state.repo) {
    toast('Select a repository first', 'info');
    return;
  }
  const nameInput = $('#publish-remote-url');
  if (nameInput && !nameInput.value && !state.repo.includes('/')) {
    nameInput.placeholder = `your-user/${state.repo}`;
  }
  syncPublishHostUi();
  setPublishModalState({ busy: false, status: '', error: '' });
  $('#publish-modal-overlay')?.classList.remove('hidden');
  $('#publish-modal-overlay')?.classList.add('flex');
  $('#publish-remote-url')?.focus();
}

function hidePublishModal() {
  if (state.publishBusy) return;
  setPublishModalState({ busy: false, status: '', error: '' });
  $('#publish-modal-overlay')?.classList.add('hidden');
  $('#publish-modal-overlay')?.classList.remove('flex');
}

async function cloneRepo(e) {
  e.preventDefault();
  if (state.cloneSource === 'local') {
    return importLocalRepo();
  }
  const form = e.target;
  const fd = new FormData(form);
  const remoteUrl = normalizeRemoteUrl(fd.get('remote_url'));
  const localName = String(fd.get('name') || '').trim();

  setCloneModalState({ error: '', status: '' });

  if (!remoteUrl) {
    setCloneModalState({ error: 'Enter a remote URL (HTTPS), e.g. https://github.com/owner/repo.git' });
    return;
  }
  if (/^git@/i.test(remoteUrl)) {
    setCloneModalState({ error: 'SSH URLs are not supported. Use HTTPS, e.g. https://github.com/owner/repo.git' });
    return;
  }
  if (localName && !/^[a-zA-Z0-9._-]+(\/[a-zA-Z0-9._-]+)?$/.test(localName)) {
    setCloneModalState({ error: 'Local name must look like owner/repo (letters, numbers, . _ - only).' });
    return;
  }

  setCloneModalState({
    busy: true,
    status: 'Cloning from remote — large repos can take a minute or two…',
  });

  try {
    const body = { remote_url: remoteUrl };
    if (localName) body.name = localName;
    const repo = await api('/api/repos/import', { method: 'POST', body: JSON.stringify(body) });
    setCloneModalState({ busy: false });
    hideCloneModal();
    form.reset();
    state.recentGitRemotes = uniqueNonEmpty([remoteUrl, ...(state.recentGitRemotes || [])]).slice(0, 15);
    await loadRepos();
    await selectRepo(repo.name);
    toast(`Cloned ${repo.name}`, 'success');
  } catch (err) {
    const msg = err.message || String(err);
    setCloneModalState({ busy: false, error: msg, status: '' });
    terminalLog(`clone failed: ${msg}`);
    const host = hostFromUrl(remoteUrl);
    const missingPat = /\bno pat configured\b/i.test(msg);
    const looksAuth = missingPat || /auth|401|403|credential|authentication/i.test(msg);
    if (host && looksAuth && !(await hasPatForHost(host))) {
      toast(`${msg} — add a PAT in Settings → Git hosts`, 'error', { duration: 12000 });
    } else {
      toast(msg, 'error', { duration: 12000 });
    }
  } finally {
    if (state.cloneBusy) setCloneModalState({ busy: false });
  }
}

async function importLocalRepo() {
  const localPath = String($('#clone-local-path')?.value || '').trim();
  const localName = String($('#clone-local-name')?.value || '').trim();

  setCloneModalState({ error: '', status: '' });

  if (!localPath) {
    setCloneModalState({ error: 'Enter the absolute path to a local git repository' });
    return;
  }
  if (localName && !/^[a-zA-Z0-9._-]+(\/[a-zA-Z0-9._-]+)?$/.test(localName)) {
    setCloneModalState({ error: 'Local name must look like owner/repo (letters, numbers, . _ - only).' });
    return;
  }

  setCloneModalState({
    busy: true,
    status: 'Importing from local folder…',
  });

  try {
    const body = { local_path: localPath };
    if (localName) body.name = localName;
    const repo = await api('/api/repos/import/local', { method: 'POST', body: JSON.stringify(body) });
    setCloneModalState({ busy: false });
    hideCloneModal();
    $('#clone-repo-form')?.reset();
    state.recentGitLocalPaths = uniqueNonEmpty([localPath, ...(state.recentGitLocalPaths || [])]).slice(0, 15);
    await loadRepos();
    await selectRepo(repo.name);
    toast(`Imported ${repo.name}`, 'success');
  } catch (err) {
    const msg = err.message || String(err);
    setCloneModalState({ busy: false, error: msg, status: '' });
    terminalLog(`local import failed: ${msg}`);
    toast(msg, 'error', { duration: 12000 });
  } finally {
    if (state.cloneBusy) setCloneModalState({ busy: false });
  }
}

async function publishToRemote(e) {
  e.preventDefault();
  if (!state.repo || state.publishBusy) return;
  const fd = new FormData(e.target);
  const hostSel = String(fd.get('host') || 'github.com');
  const remoteUrl = String(fd.get('remote_url') || '').trim();
  const host = hostSel === 'custom' ? (hostFromUrl(remoteUrl) || '') : hostSel;
  if (!remoteUrl) {
    toast('Enter a repository slug or HTTPS URL', 'error');
    return;
  }
  if (host && !(await hasPatForHost(host))) {
    toast(`Add a PAT for ${host} in Settings → Git hosts`, 'error');
    showSettingsModal('git');
    return;
  }
  setPublishModalState({ busy: true, status: 'Linking remote and pushing — this can take a minute…', error: '' });
  try {
    const body = {
      remote_url: remoteUrl,
      host: hostSel === 'custom' ? undefined : hostSel,
      create: fd.get('create') === 'on',
      private: fd.get('private') === 'on',
    };
    const out = await api(repoApi(state.repo, '/remote/publish'), {
      method: 'POST',
      body: JSON.stringify(body),
    });
    setPublishModalState({ busy: false });
    hidePublishModal();
    e.target.reset();
    syncPublishHostUi();
    const detail = await api(repoApi(state.repo));
    updateRepoInfo(detail);
    await loadRepos();
    const msg = out.created ? `Created and pushed to ${out.remote_url}` : `Pushed to ${out.remote_url}`;
    if (out.exit_code !== 0) {
      terminalLog(out.stderr || out.stdout || 'Push finished with errors');
      toast(`${msg} (exit ${out.exit_code})`, out.exit_code === 0 ? 'success' : 'error');
    } else {
      terminalLog(out.stdout || out.stderr || msg);
      toast(msg, 'success');
    }
  } catch (err) {
    setPublishModalState({ busy: false, error: err.message, status: '' });
    toast(err.message, 'error');
  } finally {
    if (state.publishBusy) setPublishModalState({ busy: false });
  }
}

async function pushRemote() {
  await showPushModal();
}

function hidePushModal() {
  if (state.pushBusy) return;
  setPushModalBusy({ busy: false });
  const overlay = $('#push-modal-overlay');
  overlay?.classList.add('hidden');
  overlay?.classList.remove('flex');
}

function formatSecretWarnings(findings) {
  return (findings || []).map((f) => {
    const line = f.line ? ` (line ${f.line})` : '';
    return `• ${f.path}${line}: ${f.reason}`;
  }).join('\n');
}

function renderSecretWarningsHtml(findings) {
  if (!findings?.length) return '';
  const items = findings.map((f) => {
    const line = f.line ? `<span class="ij-secret-warning-line">line ${f.line}</span>` : '';
    return `<li><code>${escapeHtml(f.path)}</code>${line} — ${escapeHtml(f.reason)}</li>`;
  }).join('');
  return `
    <div class="ij-secret-warning" role="alert">
      <strong>Possible secrets detected</strong>
      <p class="ij-secret-warning-desc">Review before continuing. These may include credentials, keys, or env files.</p>
      <ul class="ij-secret-warning-list">${items}</ul>
    </div>
  `;
}

async function scanSelectedPathsForSecrets(paths) {
  if (!paths?.length) return [];
  const res = await api(repoApi(state.repo, '/workspace/secrets/scan'), {
    method: 'POST',
    body: JSON.stringify({ paths }),
  });
  return res.findings || [];
}

async function confirmSecretsOrProceed(findings, action) {
  if (!findings?.length) return true;
  return confirm(
    `Possible secrets detected in files you are about to ${action}:\n\n${formatSecretWarnings(findings)}\n\nContinue anyway?`,
  );
}

function renderPushPreview(preview) {
  const subtitle = $('#push-modal-subtitle');
  const aheadBadge = $('#push-modal-ahead');
  const body = $('#push-modal-body');
  const confirm = $('#push-modal-confirm');
  if (!body || !confirm) return;

  const branch = preview.branch || 'branch';
  const remote = preview.remote || 'origin';
  const target = preview.upstream || `${remote}/${branch}`;
  if (subtitle) {
    const url = preview.remote_url ? ` · ${preview.remote_url}` : '';
    subtitle.textContent = `${branch} → ${target}${url}`;
  }

  if (aheadBadge) {
    if (preview.ahead > 0) {
      aheadBadge.textContent = `${preview.ahead} commit${preview.ahead === 1 ? '' : 's'}`;
      aheadBadge.classList.remove('hidden');
    } else {
      aheadBadge.classList.add('hidden');
    }
  }

  confirm.disabled = !preview.can_push;
  const hasSecrets = (preview.secret_warnings || []).length > 0;
  confirm.textContent = !preview.can_push
    ? 'Nothing to push'
    : hasSecrets
      ? 'Push anyway'
      : 'Push';

  if (!preview.can_push) {
    body.innerHTML = `
      <p class="ij-push-empty">${escapeHtml(preview.note || 'Nothing to push')}</p>
    `;
    return;
  }

  const note = preview.note
    ? `<p class="ij-push-note">${escapeHtml(preview.note)}</p>`
    : '';
  const commits = (preview.commits || []).map((c) => {
    const hash = (c.hash || '').slice(0, 7);
    return `<li><code class="ij-push-commit-hash">${escapeHtml(hash)}</code><span class="ij-push-commit-subject">${escapeHtml(c.subject || '')}</span></li>`;
  }).join('');
  const files = (preview.files || []).map((f) => `<li title="${escapeHtml(f)}">${escapeHtml(f)}</li>`).join('');
  const secretBanner = renderSecretWarningsHtml(preview.secret_warnings);

  body.innerHTML = `
    ${secretBanner}
    ${note}
    <div>
      <h4 class="ij-push-section-title">Commits</h4>
      <ul class="ij-push-commit-list panel-scroll">${commits || '<li class="ij-push-empty">No commits</li>'}</ul>
    </div>
    <div>
      <h4 class="ij-push-section-title">Files (${(preview.files || []).length})</h4>
      <ul class="ij-push-file-list panel-scroll">${files || '<li class="ij-push-empty">No file changes</li>'}</ul>
    </div>
  `;
}

async function showPushModal() {
  if (!state.repo) {
    toast('Select a repository first', 'info');
    return;
  }
  const overlay = $('#push-modal-overlay');
  const body = $('#push-modal-body');
  const confirm = $('#push-modal-confirm');
  setPushModalBusy({ busy: false });
  overlay?.classList.remove('hidden');
  overlay?.classList.add('flex');
  if (body) body.innerHTML = busyStatusHtml('Loading push preview…');
  if (confirm) {
    confirm.disabled = true;
    confirm.textContent = 'Push';
  }
  $('#push-modal-ahead')?.classList.add('hidden');
  try {
    const preview = await api(repoApi(state.repo, '/remote/push/preview'));
    state.pushPreview = preview;
    renderPushPreview(preview);
  } catch (err) {
    hidePushModal();
    toast(err.message, 'error');
  }
}

async function executePush() {
  if (!state.repo || state.pushBusy) return;
  const warnings = state.pushPreview?.secret_warnings || [];
  if (warnings.length) {
    const ok = await confirmSecretsOrProceed(warnings, 'push');
    if (!ok) return;
  }
  const confirm = $('#push-modal-confirm');
  setPushModalBusy({ busy: true, text: 'Pushing to remote…' });
  try {
    const out = await api(repoApi(state.repo, '/remote/push'), { method: 'POST' });
    terminalLog(out.stdout || out.stderr || 'Pushed');
    setPushModalBusy({ busy: false });
    hidePushModal();
    setStatusMessage(`Pushed ${state.currentBranch || 'branch'} to remote`);
    toast(
      out.exit_code === 0 ? 'Pushed to remote' : `Push failed (exit ${out.exit_code})`,
      out.exit_code === 0 ? 'success' : 'error',
    );
    await refreshGitStatus();
  } catch (err) {
    toast(err.message, 'error');
    if (confirm) confirm.disabled = false;
  } finally {
    setPushModalBusy({ busy: false });
  }
}

// --- File tree ---
const treeState = {
  children: new Map(),
  loading: new Set(),
  expanded: new Set(),
  recursiveNodes: null,
};
const fileFetchInflight = new Map();
const treeLoadInflight = new Map();
const treeLoadAbortControllers = new Map();
let treeDirsPendingLoad = new Set();
let treeLoadGeneration = 0;
let gradleInfoTimer = null;

function abortAllTreeLoads() {
  treeLoadGeneration += 1;
  for (const controller of treeLoadAbortControllers.values()) {
    controller.abort('refresh');
  }
  treeLoadAbortControllers.clear();
  treeLoadInflight.clear();
  treeState.loading.clear();
}

async function loadTreeLevel(dirPath = '', { generation = treeLoadGeneration } = {}) {
  const q = dirPath ? `?dir=${encodeURIComponent(dirPath)}` : '';
  const controller = new AbortController();
  treeLoadAbortControllers.set(dirPath, controller);
  const timer = setTimeout(() => controller.abort('timeout'), 30_000);
  try {
    const nodes = await api(repoApi(state.repo, `/workspace/tree${q}`), { signal: controller.signal });
    if (generation !== treeLoadGeneration) return null;
    treeState.children.set(dirPath, nodes);
    return nodes;
  } catch (err) {
    if (err?.name === 'AbortError') {
      if (controller.signal.reason === 'timeout') {
        throw new Error('File tree request timed out — try Reload project or restart Reaper');
      }
      return null;
    }
    throw err;
  } finally {
    clearTimeout(timer);
    if (treeLoadAbortControllers.get(dirPath) === controller) {
      treeLoadAbortControllers.delete(dirPath);
    }
  }
}

/** Load one lazy tree folder; dedupes concurrent requests for the same path. */
async function ensureTreeDirLoaded(dirPath) {
  if (!dirPath || treeState.recursiveNodes || !state.repo) return null;
  if (isExternalEditorPath(dirPath)) return null;
  if (treeState.children.has(dirPath)) return treeState.children.get(dirPath);
  if (treeLoadInflight.has(dirPath)) return treeLoadInflight.get(dirPath);

  const generation = treeLoadGeneration;
  treeState.loading.add(dirPath);
  const promise = (async () => {
    try {
      const nodes = await loadTreeLevel(dirPath, { generation });
      if (nodes === null) {
        if (generation === treeLoadGeneration
          && treeState.expanded.has(dirPath)
          && !treeState.children.has(dirPath)) {
          queueMicrotask(() => {
            if (!treeLoadInflight.has(dirPath)) void ensureTreeDirLoaded(dirPath);
          });
        }
        return null;
      }
      return nodes;
    } catch (err) {
      if (err?.name === 'AbortError') return null;
      treeState.expanded.delete(dirPath);
      toast(err.message, 'error');
      throw err;
    } finally {
      if (generation !== treeLoadGeneration) return;
      treeState.loading.delete(dirPath);
      if (treeLoadInflight.get(dirPath) === promise) {
        treeLoadInflight.delete(dirPath);
      }
      renderFilteredTree();
    }
  })();
  treeLoadInflight.set(dirPath, promise);
  renderFilteredTree();
  return promise;
}

/** Start fetches for expanded folders that rendered open but have no cached children. */
function scheduleTreeGapLoads() {
  if (treeState.recursiveNodes || !state.repo) return;
  for (const dir of treeState.expanded) {
    if (dir && !treeState.children.has(dir) && !treeState.loading.has(dir) && !treeLoadInflight.has(dir)) {
      void ensureTreeDirLoaded(dir);
    }
  }
}

function flushTreeDirsPendingLoad() {
  if (treeState.recursiveNodes || !state.repo) {
    treeDirsPendingLoad.clear();
    return;
  }
  for (const dir of treeDirsPendingLoad) {
    if (treeState.expanded.has(dir)
      && !treeState.children.has(dir)
      && !treeState.loading.has(dir)
      && !treeLoadInflight.has(dir)) {
      void ensureTreeDirLoaded(dir);
    }
  }
  treeDirsPendingLoad.clear();
}

async function loadTreeRoot(resetExpanded = false) {
  if (resetExpanded) treeState.expanded.clear();
  await loadTreeLevel('');
}

function treeSourceKind(node) {
  if (node?.source_kind) return node.source_kind;
  const p = String(node?.path || '').replace(/\\/g, '/');
  if (/\/src\/(test|integrationTest|intTest|testFixtures|androidTest|unitTest|functionalTest|nativeTest)(\/|$)/.test(p)) {
    return 'test';
  }
  if (/\/src\/main(\/|$)/.test(p)) return 'main';
  return null;
}

function treeSourceKindClass(node) {
  const k = treeSourceKind(node);
  return k ? ` ij-tree-source-${k}` : '';
}

function renderTree(nodes, depth = 0, lazyMode = true) {
  return nodes.map((n) => {
    const sourceClass = treeSourceKindClass(n);
    if (n.type === 'dir') {
      const isLeaf = n.has_children === false;
      const open = lazyMode
        ? treeState.expanded.has(n.path)
        : !!(n.children?.length);
      let childrenHtml = '';
      if (lazyMode) {
        const loading = treeState.loading.has(n.path);
        const loaded = treeState.children.has(n.path);
        if (loading) {
          childrenHtml = '<div class="ij-tree-loading">Loading…</div>';
        } else if (loaded) {
          childrenHtml = renderTree(
            filterHiddenTreeItems(treeState.children.get(n.path) || []),
            depth + 1,
            true,
          );
        } else if (open && !isLeaf) {
          childrenHtml = '<div class="ij-tree-loading">Loading…</div>';
          if (n.path) treeDirsPendingLoad.add(n.path);
        }
      } else {
        childrenHtml = n.children?.length ? renderTree(n.children, depth + 1, false) : '';
      }
      return `
        <details class="ij-tree-dir" data-dir="${escapeHtml(n.path)}" ${open ? 'open' : ''}${isLeaf ? ' data-leaf="1"' : ''}>
          <summary class="ij-tree-row ij-tree-dir-row${sourceClass}" style="--depth:${depth}" aria-expanded="${open ? 'true' : 'false'}">
            <span class="ij-tree-chevron" aria-hidden="true"></span>
            <span class="ij-tree-icon ij-tree-icon-folder">${treeIconSvg('folder')}${treeIconSvg('folderOpen')}</span>
            <span class="ij-tree-label">${escapeHtml(n.name)}</span>
          </summary>
          <div class="ij-tree-children">${childrenHtml}</div>
        </details>`;
    }
    const iconKind = fileIcon(n.name);
    return `
      <button type="button" data-path="${escapeHtml(n.path)}" class="tree-file ij-tree-row ij-tree-file-row${sourceClass}" style="--depth:${depth}">
        <span class="ij-tree-icon ij-tree-icon-file">${treeIconSvg(iconKind)}</span>
        <span class="ij-tree-label">${escapeHtml(n.name)}</span>
      </button>`;
  }).join('');
}

function escapeHtml(text) {
  return String(text)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

async function reloadOpenTabsFromDisk() {
  if (!state.repo || !state.tabs.length) return;
  for (const path of state.tabs) {
    if (state.dirty.has(path)) continue;
    try {
      const data = await api(`${repoApi(state.repo, '/workspace/file')}?path=${encodeURIComponent(path)}`);
      state.tabContents.set(path, data.content);
      if (path === state.activeTab) setEditorContent(path, data.content);
    } catch { /* ignore missing files */ }
  }
  renderTabs();
}

async function reloadAgentTouchedTabsFromDisk() {
  if (!state.repo || !state.agentSeenPaths?.size) return;
  for (const path of state.agentSeenPaths) {
    if (!state.tabs.includes(path) || state.dirty.has(path)) continue;
    try {
      const data = await api(`${repoApi(state.repo, '/workspace/file')}?path=${encodeURIComponent(path)}`);
      state.tabContents.set(path, data.content);
      if (path === state.activeTab) setEditorContent(path, data.content);
    } catch { /* ignore missing files */ }
  }
  renderTabs();
}

let lastTreeNodes = [];

async function refreshTree(options = {}) {
  const { resetExpanded = false } = options;
  const savedExpanded = resetExpanded ? null : [...treeState.expanded];
  abortAllTreeLoads();
  treeState.children.clear();
  treeState.recursiveNodes = null;
  treeLoadInflight.clear();
  const query = $('#tree-filter')?.value?.trim() || '';
  if (query) {
    treeState.recursiveNodes = await api(repoApi(state.repo, '/workspace/tree?recursive=1'));
    lastTreeNodes = treeState.recursiveNodes;
  } else {
    if (resetExpanded) treeState.expanded.clear();
    await loadTreeLevel('');
    if (savedExpanded) {
      for (const dir of savedExpanded) treeState.expanded.add(dir);
    }
    lastTreeNodes = treeState.children.get('') || [];
  }
  renderFilteredTree();
  await loadExpandedTreeGaps();
}

/** Fetch any expanded folder that rendered open but has no cached children yet. */
async function loadExpandedTreeGaps() {
  if (treeState.recursiveNodes) return;
  const pending = [...treeState.expanded].filter(
    (d) => d && !treeState.children.has(d) && !treeState.loading.has(d) && !treeLoadInflight.has(d),
  );
  if (!pending.length) return;
  await Promise.allSettled(pending.map((dir) => ensureTreeDirLoaded(dir)));
}

function isCompiledTreeEntry(n) {
  const name = n.name || '';
  const path = (n.path || '').replace(/\\/g, '/');
  if (name.endsWith('.class')) return true;
  if (n.type === 'dir' && (name === 'build' || name === 'target' || name === 'out' || name === 'bin')) return true;
  if (path.includes('/build/classes/') || path.includes('/target/classes/')) return true;
  return false;
}

function isIgnoredChangesPath(path) {
  const normalized = (path || '').replace(/\\/g, '/').replace(/\/$/, '');
  if (!normalized) return true;
  if (normalized === 'target' || normalized.startsWith('target/') || normalized.split('/').includes('target')) {
    return true;
  }
  const name = normalized.split('/').pop() || normalized;
  if (name === 'mvnw' || name === 'mvnw.cmd' || name === 'gradlew' || name === 'gradlew.bat') return true;
  return false;
}

function isHiddenPath(path) {
  const normalized = (path || '').replace(/\\/g, '/').replace(/\/$/, '');
  if (!normalized) return true;
  const name = normalized.split('/').pop() || normalized;
  if (name.startsWith('.')) return true;
  if (normalized.split('/').some((seg) => seg.startsWith('.') && seg.length > 1)) return true;
  if (name === 'mvnw' || name === 'mvnw.cmd' || name === 'gradlew' || name === 'gradlew.bat') return true;
  if (name === 'gradle.properties' && !normalized.includes('/')) return true;
  if (normalized === 'gradle' || normalized.startsWith('gradle/')) return true;
  return false;
}

function isHiddenTreeEntry(n) {
  return isHiddenPath(n.path || n.name || '');
}

function filterStatusFilesForDisplay(files) {
  return (files || []).filter((f) => {
    const p = (f.path || '').replace(/\\/g, '/');
    if (!p || p.endsWith('/')) return false;
    if (isIgnoredChangesPath(p)) return false;
    if (!getShowDotfiles() && isHiddenPath(p)) return false;
    return true;
  });
}

function filterHiddenTreeItems(nodes) {
  const out = [];
  for (const n of nodes) {
    if (isCompiledTreeEntry(n)) continue;
    if (!getShowDotfiles() && isHiddenTreeEntry(n)) continue;
    if (n.type === 'dir') {
      out.push({ ...n, children: filterHiddenTreeItems(n.children || []) });
    } else {
      out.push(n);
    }
  }
  return out;
}

function filterTreeNodes(nodes, query) {
  if (!query) return nodes;
  const q = query.toLowerCase();
  const out = [];
  for (const n of nodes) {
    if (n.type === 'dir') {
      const children = filterTreeNodes(n.children || [], query);
      if (children.length || n.name.toLowerCase().includes(q)) {
        out.push({ ...n, children });
      }
    } else if (n.path.toLowerCase().includes(q) || n.name.toLowerCase().includes(q)) {
      out.push(n);
    }
  }
  return out;
}

function renderFilteredTree() {
  const query = $('#tree-filter')?.value?.trim() || '';
  const treeEl = $('#file-tree');
  let nodes;
  let lazyMode = true;
  if (query && treeState.recursiveNodes) {
    nodes = filterTreeNodes(filterHiddenTreeItems(treeState.recursiveNodes), query);
    lazyMode = false;
  } else {
    nodes = filterHiddenTreeItems(treeState.children.get('') || lastTreeNodes);
  }
  treeDirsPendingLoad = new Set();
  if (!nodes.length) {
    treeEl.innerHTML = query
      ? '<p class="ij-empty-hint">No files match your filter</p>'
      : '<p class="ij-empty-hint">This repository has no files yet</p>';
    flushTreeDirsPendingLoad();
    scheduleTreeGapLoads();
    return;
  }
  treeEl.innerHTML = `<div class="ij-tree">${renderTree(nodes, 0, lazyMode)}</div>`;
  bindTreeEvents();
  if (state.activeTab) {
    $$('.tree-file').forEach((b) => b.classList.toggle('active', treePathMatchesTab(b.dataset.path, state.activeTab)));
  }
  flushTreeDirsPendingLoad();
  scheduleTreeGapLoads();
}

function isAbsoluteRepoPath(path) {
  const normalized = String(path || '').replace(/\\/g, '/');
  return normalized.startsWith('/') || /^[A-Za-z]:\//.test(normalized);
}

function isExternalEditorPath(path) {
  return isAbsoluteRepoPath(path);
}

const EXTERNAL_READ_ONLY_MESSAGE = 'External file (read-only — system/SDK header)';

function applyEditorReadOnlyForPath(path) {
  if (!state.editor) return;
  const external = isExternalEditorPath(path);
  state.editor.updateOptions({
    readOnly: external,
    readOnlyMessage: external ? EXTERNAL_READ_ONLY_MESSAGE : undefined,
  });
}

function normalizeRepoPath(path) {
  const normalized = String(path || '').replace(/\\/g, '/');
  // Keep absolute paths (e.g. /usr/include/stdio.h from clangd go-to-definition).
  if (isAbsoluteRepoPath(normalized)) return normalized;
  return normalized.replace(/^\/+/, '');
}

/** Map javac overlay copies (`.reaper/java-diagnostics/overlay/…`) to workspace paths. */
function stripJavaDiagOverlayPath(path) {
  if (!path) return path;
  let normalized = path.replace(/\\/g, '/');
  const rootPrefixes = [
    '.reaper/java-diagnostics/overlay/',
    '.reaper/diagnostics/overlay/',
  ];
  for (const prefix of rootPrefixes) {
    if (normalized.startsWith(prefix)) {
      normalized = normalized.slice(prefix.length);
      break;
    }
  }
  const markers = [
    '/.reaper/java-diagnostics/overlay/',
    '/.reaper/diagnostics/overlay/',
  ];
  for (const marker of markers) {
    const idx = normalized.indexOf(marker);
    if (idx >= 0) {
      const prefix = normalized.slice(0, idx);
      const rest = normalized.slice(idx + marker.length);
      normalized = prefix ? `${prefix}/${rest}` : rest;
      break;
    }
  }
  // Any `{module}/.reaper/…/overlay/{rest}` → `{module}/{rest}`
  const reaperIdx = normalized.indexOf('/.reaper/');
  if (reaperIdx >= 0) {
    const fromReaper = normalized.slice(reaperIdx);
    const overlayRel = fromReaper.indexOf('/overlay/');
    if (overlayRel >= 0) {
      const prefix = normalized.slice(0, reaperIdx);
      const rest = normalized.slice(reaperIdx + overlayRel + '/overlay/'.length);
      if (rest) normalized = prefix ? `${prefix}/${rest}` : rest;
    }
  }
  return normalized.replace(/\/{2,}/g, '/');
}

function workspaceExplorerPath(path) {
  return normalizeRepoPath(stripJavaDiagOverlayPath(path));
}

/** Canonical path for breakpoint storage (collapses overlay duplicates). */
function normalizeDebugBreakpointPath(path) {
  return workspaceExplorerPath(path);
}

function debugBreakpointsForPath(path) {
  if (!path) return [];
  const key = normalizeDebugBreakpointPath(path);
  const lines = new Set();
  for (const [p, set] of state.debugBreakpoints) {
    if (normalizeDebugBreakpointPath(p) === key) {
      for (const line of set) lines.add(line);
    }
  }
  return [...lines].sort((a, b) => a - b);
}

function allDebugBreakpointsList() {
  const seen = new Set();
  const out = [];
  for (const [path, lines] of state.debugBreakpoints) {
    const norm = normalizeDebugBreakpointPath(path);
    for (const line of lines) {
      const id = `${norm}:${line}`;
      if (seen.has(id)) continue;
      seen.add(id);
      out.push({ path: norm, line });
    }
  }
  return out;
}

function compactDebugBreakpointsMap() {
  const compacted = new Map();
  for (const [path, lines] of state.debugBreakpoints) {
    const norm = normalizeDebugBreakpointPath(path);
    const set = compacted.get(norm) || new Set();
    for (const line of lines) set.add(line);
    compacted.set(norm, set);
  }
  state.debugBreakpoints = compacted;
}

function toggleBreakpoint(path, line) {
  if (!path || !line) return;
  compactDebugBreakpointsMap();
  const key = normalizeDebugBreakpointPath(path);
  const set = state.debugBreakpoints.get(key) || new Set();
  if (set.has(line)) set.delete(line);
  else set.add(line);
  if (set.size) state.debugBreakpoints.set(key, set);
  else state.debugBreakpoints.delete(key);
  renderBreakpointGlyphs();
  renderDebugBreakpointsList();
  void syncBreakpointsToServer();
}

function treePathMatchesTab(treePath, tabPath) {
  if (!tabPath) return false;
  return workspaceExplorerPath(treePath) === workspaceExplorerPath(tabPath);
}

function treeParentDirPaths(filePath) {
  const normalized = normalizeRepoPath(filePath);
  if (isExternalEditorPath(normalized)) return [];
  const parts = normalized.split('/').filter(Boolean);
  if (parts.length <= 1) return [];
  parts.pop();
  const out = [];
  for (let i = 0; i < parts.length; i++) {
    out.push(parts.slice(0, i + 1).join('/'));
  }
  return out;
}

/** True when a workspace path points at a file, including extensionless Ruby/Rails manifests. */
function isWorkspaceTreeFilePath(normalizedPath) {
  const base = String(normalizedPath || '').replace(/\\/g, '/').split('/').pop() || '';
  if (!base) return false;
  if (/\.[^./\\]+$/.test(base)) return true;
  const lower = base.toLowerCase();
  if (lower.endsWith('.gemspec')) return true;
  return /^(gemfile|rakefile|guardfile|capfile|podfile|brewfile|procfile|config\.ru|thorfile|rackfile|berksfile|cheffile|vagrantfile|dockerfile|makefile|gnumakefile|readme|license|changelog)$/i.test(base)
    || /^dockerfile\./i.test(base);
}

async function ensureTreeRootLoaded() {
  if (treeState.recursiveNodes || !state.repo) return;
  if (!treeState.children.has('')) {
    await loadTreeLevel('');
  }
}

async function waitForTreeInflight() {
  while (treeLoadInflight.size > 0) {
    await Promise.allSettled([...treeLoadInflight.values()]);
  }
}

function scrollTreeFileIntoView(path) {
  const normalized = workspaceExplorerPath(path);
  const btn = [...$$('.tree-file')].find((b) => treePathMatchesTab(b.dataset.path, normalized));
  if (!btn) return false;
  $$('.tree-file').forEach((b) => {
    b.classList.toggle('active', treePathMatchesTab(b.dataset.path, normalized));
  });
  const panel = $('#panel-explorer');
  if (panel) {
    const panelRect = panel.getBoundingClientRect();
    const btnRect = btn.getBoundingClientRect();
    if (btnRect.top < panelRect.top) {
      panel.scrollTop += btnRect.top - panelRect.top - 8;
    } else if (btnRect.bottom > panelRect.bottom) {
      panel.scrollTop += btnRect.bottom - panelRect.bottom + 8;
    }
  } else {
    btn.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }
  return true;
}

/** Expand ancestors, load lazy folders, and scroll the project tree to `path`. */
async function revealFileInExplorer(path) {
  const normalized = workspaceExplorerPath(path);
  if (!normalized || !state.repo) return;
  if (isExternalEditorPath(normalized)) return;

  const isFile = isWorkspaceTreeFilePath(normalized);

  if (document.body.classList.contains('sidebar-collapsed')) {
    toggleSidebar(false);
  }
  switchPanel('explorer');

  const expandSeed = isFile ? normalized : `${normalized}/_.reaper-expand`;
  for (const dir of treeParentDirPaths(expandSeed)) {
    treeState.expanded.add(dir);
  }
  if (!isFile) treeState.expanded.add(normalized);

  const treeFilter = $('#tree-filter');
  const q = treeFilter?.value?.trim() || '';
  if (q && !normalized.toLowerCase().includes(q.toLowerCase())) {
    treeFilter.value = '';
    try {
      await refreshTree();
    } catch (err) {
      toast(err.message, 'error');
    }
  } else {
    try {
      await ensureTreeRootLoaded();
    } catch (err) {
      toast(err.message, 'error');
      return;
    }
    if (!treeState.recursiveNodes) {
      for (const dir of treeParentDirPaths(normalized)) {
        try {
          await ensureTreeDirLoaded(dir);
        } catch {
          /* ensureTreeDirLoaded already toasts */
        }
      }
      await loadExpandedTreeGaps();
      await waitForTreeInflight();
    }
    renderFilteredTree();
  }

  for (let attempt = 0; attempt < 12; attempt++) {
    if (!isFile || scrollTreeFileIntoView(normalized)) return;
    if (!treeState.recursiveNodes) {
      await loadExpandedTreeGaps();
      await waitForTreeInflight();
      renderFilteredTree();
    }
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }
}

let treeContextTarget = null;
let treeContextSuppressUntil = 0;

function armTreeContextDismissGuard(ms = 500) {
  treeContextSuppressUntil = Date.now() + ms;
}

function treeContextMenuIsOpen() {
  return !$('#tree-context-menu')?.classList.contains('hidden');
}

function dismissTreeContextMenuIfOutside(e) {
  if (!treeContextMenuIsOpen()) return;
  if (Date.now() < treeContextSuppressUntil) return;
  if (e?.target?.closest?.('#tree-context-menu')) return;
  hideTreeContextMenu();
}

function onTreeContextMenuScroll() {
  if (Date.now() < treeContextSuppressUntil) return;
  hideTreeContextMenu();
}

function treeRevealLabel() {
  return /Mac|iPhone|iPad/i.test(navigator.userAgent) ? 'Reveal in Finder' : 'Reveal in File Manager';
}

function hideTreeContextMenu() {
  const menu = $('#tree-context-menu');
  if (!menu) return;
  menu.classList.add('hidden');
  menu.setAttribute('aria-hidden', 'true');
  menu.innerHTML = '';
  treeContextTarget = null;
}

function positionTreeContextMenu(menu, x, y) {
  menu.style.left = '0';
  menu.style.top = '0';
  menu.classList.remove('hidden');
  menu.setAttribute('aria-hidden', 'false');
  const rect = menu.getBoundingClientRect();
  const pad = 8;
  let left = x;
  let top = y;
  if (left + rect.width > window.innerWidth - pad) left = window.innerWidth - rect.width - pad;
  if (top + rect.height > window.innerHeight - pad) top = window.innerHeight - rect.height - pad;
  menu.style.left = `${Math.max(pad, left)}px`;
  menu.style.top = `${Math.max(pad, top)}px`;
}

function parentDirForTreePath(path) {
  const normalized = workspaceExplorerPath(path);
  if (!normalized) return '';
  const parts = normalized.split('/').filter(Boolean);
  if (parts.length <= 1) return '';
  parts.pop();
  return parts.join('/');
}

function treeFileMenuProfile(path, kind) {
  if (kind === 'dir') return { type: 'dir', label: 'Folder' };
  const rel = workspaceExplorerPath(path);
  const base = (rel.split('/').pop() || '').toLowerCase();
  const ext = base.includes('.') ? base.slice(base.lastIndexOf('.') + 1) : '';

  if (rel.endsWith('.java')) {
    return isJavaTestFilePath(rel)
      ? { type: 'java-test', label: 'Java Test' }
      : { type: 'java', label: 'Java Source' };
  }
  if (isGradleFilePath(rel)) return { type: 'gradle', label: 'Gradle' };
  if (isMavenFilePath(rel)) return { type: 'maven', label: 'Maven' };
  if (base === 'dockerfile' || base.startsWith('dockerfile.')) return { type: 'dockerfile', label: 'Dockerfile' };
  if (base === 'readme.md' || ext === 'md') return { type: 'markdown', label: 'Markdown' };
  if (ext === 'json') return { type: 'json', label: 'JSON' };
  if (ext === 'yaml' || ext === 'yml') return { type: 'yaml', label: 'YAML' };
  if (ext === 'xml') return { type: 'xml', label: 'XML' };
  if (ext === 'properties') return { type: 'properties', label: 'Properties' };
  if (ext === 'gradle' || ext === 'kts') return { type: 'gradle', label: 'Gradle' };
  if (['js', 'mjs', 'cjs', 'jsx'].includes(ext)) return { type: 'javascript', label: 'JavaScript' };
  if (['ts', 'tsx'].includes(ext)) return { type: 'typescript', label: 'TypeScript' };
  if (ext === 'py' || ext === 'pyw') {
    if ((base.startsWith('test_') && base.endsWith('.py'))
      || base.endsWith('_test.py')
      || rel.includes('/tests/')
      || (rel.includes('/test/') && base.endsWith('.py'))) {
      return { type: 'python-test', label: 'Python Test' };
    }
    return { type: 'python', label: 'Python' };
  }
  if (ext === 'rs') return { type: 'rust', label: 'Rust' };
  if (ext === 'go') {
    if (base.endsWith('_test.go')) {
      return { type: 'go-test', label: 'Go Test' };
    }
    return { type: 'go', label: 'Go' };
  }
  if (ext === 'c') {
    if (isNativeTestPath(rel)) {
      return { type: 'native-test', label: 'C Test' };
    }
    return { type: 'native', label: 'C' };
  }
  if (['cpp', 'cc', 'cxx'].includes(ext)) {
    if (isNativeTestPath(rel)) {
      return { type: 'native-test', label: 'C++ Test' };
    }
    return { type: 'native', label: 'C++' };
  }
  if (rel.endsWith('.rb')) {
    if (rel.endsWith('_spec.rb') || rel.includes('/spec/')) {
      return { type: 'ruby-test', label: 'RSpec' };
    }
    if (rel.endsWith('_test.rb') || rel.includes('/test/')) {
      return { type: 'ruby-test', label: 'Ruby Test' };
    }
    return { type: 'ruby', label: 'Ruby' };
  }
  if (ext === 'rb') return { type: 'ruby', label: 'Ruby' };
  if (ext === 'kt') {
    if (rel.includes('/test/') || rel.includes('/tests/')) return { type: 'kotlin-test', label: 'Kotlin Test' };
    return { type: 'kotlin', label: 'Kotlin' };
  }
  if (ext === 'kts') return { type: 'kotlin', label: 'Kotlin Script' };
  if (ext === 'php') {
    if (isPhpTestPath(rel)) return { type: 'php-test', label: 'PHP Test' };
    return { type: 'php', label: 'PHP' };
  }
  if (ext === 'dart') {
    if (isDartTestPath(rel)) return { type: 'dart-test', label: 'Dart Test' };
    return { type: 'dart', label: 'Dart' };
  }
  if (isShellScriptPath(rel)) return { type: 'shell', label: 'Shell Script' };
  if (ext === 'sql') return { type: 'sql', label: 'SQL' };
  if (ext === 'html' || ext === 'htm') return { type: 'html', label: 'HTML' };
  if (ext === 'css' || ext === 'scss') return { type: 'css', label: 'CSS' };
  return { type: 'file', label: 'File' };
}

function treeContextMenuItem(action, label, { danger = false, disabled = false } = {}) {
  return `<button type="button" class="ij-context-menu-item${danger ? ' ij-context-menu-item--danger' : ''}" data-tree-action="${action}"${disabled ? ' disabled' : ''}>${escapeHtml(label)}</button>`;
}

function treeContextMenuSep() {
  return '<div class="ij-context-menu-sep"></div>';
}

function treeContextCanFormat(path) {
  return isDiagnosablePath(workspaceExplorerPath(path));
}

function renderTreeContextMenu(target) {
  const profile = treeFileMenuProfile(target.path, target.kind);
  const isFile = target.kind === 'file';
  const rows = [
    `<div class="ij-context-menu-heading">${escapeHtml(profile.label)}</div>`,
  ];

  if (isFile) {
    rows.push(treeContextMenuItem('open', 'Open'));
    rows.push(treeContextMenuItem('rename', 'Rename…'));
  }

  switch (profile.type) {
    case 'dir':
      rows.push(treeContextMenuItem('new-file', 'New File…'));
      break;
    case 'java':
      if (state.runInfo?.has_project) {
        rows.push(treeContextMenuItem('run', 'Run'));
        if (projectSupportsCoverage()) {
          rows.push(treeContextMenuItem('run-tests-coverage', 'Run Tests with Coverage'));
        }
      }
      rows.push(treeContextMenuItem('goto-declaration', 'Go to Declaration'));
      rows.push(treeContextMenuItem('goto-class', 'Go to Class…'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'java-test':
      if (state.runInfo?.has_project) {
        rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
        rows.push(treeContextMenuItem('run-tests-coverage', 'Run Tests with Coverage'));
      }
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'gradle':
      if (hasAutoReloadProject()) {
        rows.push(treeContextMenuItem('reload-project', 'Reload Gradle Project'));
      }
      if (state.runInfo?.has_project) {
        rows.push(treeContextMenuItem('run-build', 'Run Gradle Task'));
      }
      break;
    case 'maven':
      if (hasAutoReloadProject()) {
        rows.push(treeContextMenuItem('reload-project', 'Reload Maven Project'));
      }
      if (state.runInfo?.has_project) {
        rows.push(treeContextMenuItem('run-build', 'Run Maven Goal'));
      }
      break;
    case 'ruby':
      rows.push(treeContextMenuItem('run', 'Run'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'ruby-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'python':
      rows.push(treeContextMenuItem('run', 'Run'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'python-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'go':
      rows.push(treeContextMenuItem('run', 'Run'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'go-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'native':
      rows.push(treeContextMenuItem('run', 'Run'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'native-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      if (treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'rust':
      rows.push(treeContextMenuItem('run', 'Run'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'rust-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'kotlin':
      rows.push(treeContextMenuItem('run', 'Run'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'kotlin-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'php':
      rows.push(treeContextMenuItem('run', 'Run'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'php-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'dart':
      rows.push(treeContextMenuItem('run', 'Run'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'dart-test':
      rows.push(treeContextMenuItem('run-tests', 'Run Tests'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'shell':
      rows.push(treeContextMenuItem('run', 'Run'));
      rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    case 'markdown':
    case 'json':
    case 'yaml':
    case 'xml':
    case 'javascript':
    case 'typescript':
    case 'properties':
    case 'dockerfile':
      if (isFile && treeContextCanFormat(target.path)) {
        rows.push(treeContextMenuItem('format', 'Reformat Code'));
      }
      if (isFile) rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      break;
    default:
      if (isFile) {
        if (treeContextCanFormat(target.path)) {
          rows.push(treeContextMenuItem('format', 'Reformat Code'));
        }
        rows.push(treeContextMenuItem('new-file', 'New File in Folder…'));
      }
      break;
  }

  rows.push(treeContextMenuSep());
  rows.push(treeContextMenuItem('copy-path', 'Copy Path'));
  if (state.projectFolder) {
    rows.push(treeContextMenuItem('copy-abs-path', 'Copy Absolute Path'));
  }
  rows.push(treeContextMenuItem('reveal', treeRevealLabel()));

  if (isFile) {
    rows.push(treeContextMenuSep());
    rows.push(treeContextMenuItem('delete', 'Delete…', { danger: true }));
  }

  return rows.join('');
}

async function copyTreePath(path, { absolute = false } = {}) {
  const rel = workspaceExplorerPath(path);
  if (!rel) return;
  let text = rel;
  if (absolute && state.projectFolder) {
    text = `${state.projectFolder.replace(/\\/g, '/').replace(/\/$/, '')}/${rel}`;
  }
  try {
    await navigator.clipboard.writeText(text);
    toast(absolute ? 'Copied absolute path' : 'Copied path', 'success');
  } catch {
    toast('Could not copy to clipboard', 'error');
  }
}

async function revealTreePathInSystem(path) {
  if (!state.repo) return;
  const rel = workspaceExplorerPath(path);
  if (!rel) return;
  try {
    await api(repoApi(state.repo, '/workspace/reveal'), {
      method: 'POST',
      body: JSON.stringify({ path: rel }),
    });
  } catch (err) {
    toast(err.message || 'Could not reveal in file manager', 'error');
  }
}

async function deleteTreePath(path) {
  if (!state.repo) return;
  const rel = workspaceExplorerPath(path);
  if (!rel) return;
  const name = rel.split('/').pop() || rel;
  if (!confirm(`Delete “${name}”? This cannot be undone.`)) return;
  try {
    await api(`${repoApi(state.repo, '/workspace/file')}?path=${encodeURIComponent(rel)}`, {
      method: 'DELETE',
    });
    const openTab = state.tabs.find((t) => workspaceExplorerPath(t) === rel);
    if (openTab) closeTab(openTab);
    hideTreeContextMenu();
    await refreshTree();
    await refreshGitStatus();
    toast('Deleted', 'success');
  } catch (err) {
    toast(err.message || 'Delete failed', 'error');
  }
}

function retargetOpenTabPath(oldRel, newRel) {
  const normOld = workspaceExplorerPath(oldRel);
  const normNew = workspaceExplorerPath(newRel);
  if (!normOld || !normNew) return;
  for (let i = 0; i < state.tabs.length; i += 1) {
    if (workspaceExplorerPath(state.tabs[i]) !== normOld) continue;
    const tab = state.tabs[i];
    state.tabs[i] = normNew;
    const content = state.tabContents.get(tab);
    if (content != null) {
      state.tabContents.delete(tab);
      state.tabContents.set(normNew, content);
    }
    if (state.dirty.has(tab)) {
      state.dirty.delete(tab);
      state.dirty.add(normNew);
    }
  }
  if (state.activeTab && workspaceExplorerPath(state.activeTab) === normOld) {
    state.activeTab = normNew;
  }
  renderTabs();
  updateSaveButton();
}

async function renameWorkspacePath(fromRel, toRel, { skipSymbolEdits = false } = {}) {
  if (!state.repo) return false;
  const from = workspaceExplorerPath(fromRel);
  const to = workspaceExplorerPath(toRel);
  if (!from || !to || from === to) return false;
  try {
    if (!skipSymbolEdits) {
      const plan = await api(repoApi(state.repo, '/workspace/rename-path'), {
        method: 'POST',
        body: JSON.stringify({ path: from, new_path: to, plan_only: true }),
      });
      const edits = Array.isArray(plan) ? [] : (plan.edits || []);
      if (edits.length) {
        await applyJavaWorkspaceEdits(edits);
      }
    }
    await api(repoApi(state.repo, '/workspace/rename-path'), {
      method: 'POST',
      body: JSON.stringify({ path: from, new_path: to }),
    });
    retargetOpenTabPath(from, to);
    await refreshTree();
    await refreshGitStatus();
    return true;
  } catch (err) {
    toast(err.message || 'File rename failed', 'error');
    return false;
  }
}

async function renameTreePath(path) {
  if (!state.repo) return;
  const rel = workspaceExplorerPath(path);
  if (!rel) return;
  const base = rel.split('/').pop() || rel;
  const parent = rel.includes('/') ? rel.slice(0, rel.lastIndexOf('/') + 1) : '';
  const newBase = await showRenamePrompt({
    title: 'Rename file',
    subtitle: rel,
    value: base,
  });
  if (!newBase || newBase === base) return;
  if (/[\\/]/.test(newBase) || newBase.trim() !== newBase) {
    toast('Invalid file name', 'error');
    return;
  }
  const newRel = `${parent}${newBase}`;
  try {
    const ok = await renameWorkspacePath(rel, newRel);
    if (!ok) return;
    hideTreeContextMenu();
    toast('Renamed', 'success');
  } catch (err) {
    toast(err.message || 'Rename failed', 'error');
  }
}

function runTreeContextAction(action) {
  const target = treeContextTarget;
  hideTreeContextMenu();
  if (!target?.path) return;
  const path = target.path;
  switch (action) {
    case 'open':
      void openFileFromTree(path);
      break;
    case 'rename':
      void renameTreePath(path);
      break;
    case 'run':
      void runTreeContextRun(path);
      break;
    case 'run-tests':
      void runTreeContextTests(path);
      break;
    case 'run-tests-coverage':
      void runTreeContextTestsWithCoverage(path);
      break;
    case 'run-build':
      void runTreeContextBuild(path);
      break;
    case 'format':
      void formatTreeContextFile(path);
      break;
    case 'reload-project':
      void reloadProjectIndex();
      break;
    case 'goto-declaration':
      void gotoTreeContextDeclaration(path);
      break;
    case 'goto-class': {
      const stem = workspaceExplorerPath(path).split('/').pop()?.replace(/\.java$/i, '') || '';
      showGoToClass(stem);
      break;
    }
    case 'new-file': {
      const parent = target.kind === 'dir' ? path : parentDirForTreePath(path);
      showFileModal(parent ? `${parent}/` : '');
      break;
    }
    case 'copy-path':
      void copyTreePath(path);
      break;
    case 'copy-abs-path':
      void copyTreePath(path, { absolute: true });
      break;
    case 'reveal':
      void revealTreePathInSystem(path);
      break;
    case 'delete':
      void deleteTreePath(path);
      break;
    default:
      break;
  }
}

async function openTreeFileForAction(path, line = 1, column = 1) {
  const rel = workspaceExplorerPath(path);
  await openFileAt(rel, line, column);
  return rel;
}

async function runTreeContextRun(path) {
  await openTreeFileForAction(path);
  await refreshRunInfo();
  await runActive();
}

async function runTreeContextTests(path) {
  await openTreeFileForAction(path);
  await refreshRunInfo();
  const target = state.runTarget;
  if (target?.mode === 'test') {
    await runProjectTest(target.filter);
  } else {
    await runActive();
  }
}

async function runTreeContextTestsWithCoverage(path) {
  await openTreeFileForAction(path);
  await refreshRunInfo();
  const content = state.tabContents.get(state.activeTab) ?? state.editor?.getModel()?.getValue() ?? '';
  const line = state.editor?.getPosition()?.lineNumber || 1;
  const filter = coverageTestFilterForFile(state.activeTab, content, line)
    || (state.runTarget?.mode === 'test' ? state.runTarget.filter : null);
  if (filter) {
    await runProjectTestWithCoverage(filter);
  } else {
    toast('Open a Java source or test class to run with coverage', 'info');
  }
}

async function runTreeContextBuild(path) {
  await openTreeFileForAction(path);
  await refreshRunInfo();
  await runProjectTask();
}

async function formatTreeContextFile(path) {
  await openTreeFileForAction(path);
  await formatDocument();
}

async function gotoTreeContextDeclaration(path) {
  await openTreeFileForAction(path);
  navigateToPrimarySource(state.activeTab, { force: true });
}

function showTreeContextMenu(x, y, target) {
  if (!state.repo || !target?.path) return;
  closeAllMenus();
  hideTreeContextMenu();
  armTreeContextDismissGuard();
  treeContextTarget = target;
  const menu = $('#tree-context-menu');
  if (!menu) return;
  menu.innerHTML = renderTreeContextMenu(target);
  menu.querySelectorAll('[data-tree-action]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      runTreeContextAction(btn.dataset.treeAction);
    });
  });
  positionTreeContextMenu(menu, x, y);
  menu.querySelector('.ij-context-menu-item')?.focus({ preventScroll: true });
}

function bindTreeEvents() {
  const treeEl = $('#file-tree');
  if (!treeEl || treeEl.dataset.treeBound) return;
  treeEl.dataset.treeBound = '1';

  treeEl.addEventListener('contextmenu', (e) => {
    const fileBtn = e.target.closest('.tree-file');
    if (fileBtn?.dataset.path) {
      e.preventDefault();
      e.stopPropagation();
      showTreeContextMenu(e.clientX, e.clientY, { path: fileBtn.dataset.path, kind: 'file' });
      return;
    }
    const dirRow = e.target.closest('.ij-tree-dir-row');
    if (dirRow) {
      const dir = dirRow.closest('details.ij-tree-dir')?.dataset.dir;
      if (dir != null) {
        e.preventDefault();
        e.stopPropagation();
        showTreeContextMenu(e.clientX, e.clientY, { path: dir, kind: 'dir' });
      }
    }
  });

  treeEl.addEventListener('toggle', async (e) => {
    const details = e.target;
    if (!details.matches?.('details.ij-tree-dir')) return;
    const dir = details.dataset.dir;
    if (!dir || treeState.recursiveNodes) return;
    if (details.open) {
      treeState.expanded.add(dir);
      if (details.dataset.leaf === '1' || treeState.children.has(dir)) return;
      try {
        await ensureTreeDirLoaded(dir);
      } catch {
        details.open = false;
      }
    } else {
      treeState.expanded.delete(dir);
    }
  }, true);

  treeEl.addEventListener('click', (e) => {
    const btn = e.target.closest('.tree-file');
    if (btn?.dataset.path) {
      openFileFromTree(btn.dataset.path);
      return;
    }
    const dirRow = e.target.closest('.ij-tree-dir-row');
    if (!dirRow || treeState.recursiveNodes) return;
    const details = dirRow.closest('details.ij-tree-dir');
    const dir = details?.dataset?.dir;
    if (!dir || details?.dataset?.leaf === '1') return;
    queueMicrotask(() => {
      if (!details.open) return;
      treeState.expanded.add(dir);
      if (!treeState.children.has(dir) && !treeState.loading.has(dir) && !treeLoadInflight.has(dir)) {
        void ensureTreeDirLoaded(dir);
      }
    });
  });

  treeEl.addEventListener('mouseover', (e) => {
    const btn = e.target.closest('.tree-file');
    if (btn?.dataset.path) prefetchFile(btn.dataset.path);
  });
}

async function fetchFileContent(path) {
  if (state.tabContents.has(path)) return state.tabContents.get(path);
  if (fileFetchInflight.has(path)) return fileFetchInflight.get(path);
  const promise = api(`${repoApi(state.repo, '/workspace/file')}?path=${encodeURIComponent(path)}`)
    .then((data) => {
      fileFetchInflight.delete(path);
      return data?.content ?? '';
    })
    .catch((err) => {
      fileFetchInflight.delete(path);
      throw err;
    });
  fileFetchInflight.set(path, promise);
  return promise;
}

async function hydrateTabContent(path) {
  if (state.tabContents.has(path)) return state.tabContents.get(path);
  const content = await fetchFileContent(path);
  const text = content ?? '';
  state.tabContents.set(path, text);
  return text;
}

function prefetchFile(path) {
  if (!path || !state.repo) return;
  if (state.tabContents.has(path) || fileFetchInflight.has(path)) return;
  fetchFileContent(path).catch(() => {});
}

function scheduleGradleInfoRefresh() {
  if (gradleInfoTimer) clearTimeout(gradleInfoTimer);
  gradleInfoTimer = setTimeout(() => {
    gradleInfoTimer = null;
    refreshRunInfo();
  }, 150);
}

// --- Tabs & editor ---
function updateTreeBackButton() {
  const btn = $('#btn-tree-back');
  if (!btn) return;
  const anchor = state.treeNavAnchor;
  const canBack = !!(anchor && state.activeTab && anchor.path !== state.activeTab);
  btn.disabled = !canBack;
  btn.classList.toggle('ij-sidebar-btn--disabled', !canBack);
  btn.setAttribute('aria-disabled', canBack ? 'false' : 'true');
  if (canBack && anchor.path) {
    btn.title = `Back to ${anchor.path.split('/').pop()}`;
  } else {
    btn.title = 'Back to project file (select a file in the tree, then navigate away)';
  }
}

function rememberTreeAnchorCursor() {
  if (!state.treeNavAnchor || state.activeTab !== state.treeNavAnchor.path || !state.editor) return;
  const pos = state.editor.getPosition();
  if (!pos) return;
  state.treeNavAnchor = {
    ...state.treeNavAnchor,
    line: pos.lineNumber,
    column: pos.column,
  };
}

async function openFileFromTree(path) {
  if (state.editor && state.activeTab === path) {
    const pos = state.editor.getPosition();
    state.treeNavAnchor = {
      path,
      line: pos?.lineNumber ?? 1,
      column: pos?.column ?? 1,
    };
  } else {
    state.treeNavAnchor = { path, line: 1, column: 1 };
  }
  await openFile(path);
  updateTreeBackButton();
}

async function goBackToTreeFile() {
  if (!state.treeNavAnchor) return;
  const { path, line = 1, column = 1 } = state.treeNavAnchor;
  switchPanel('explorer');
  await openFileAt(path, line, column);
  updateTreeBackButton();
}

async function openFile(path, { silent = false, skipPrimaryNav = false } = {}) {
  path = workspaceExplorerPath(path);
  if (state.tabs.includes(path)) {
    try {
      if (!state.tabContents.has(path)) await hydrateTabContent(path);
    } catch (err) {
      if (!silent) toast(err.message, 'error');
    }
    activateTab(path);
    if (!skipPrimaryNav) navigateToPrimarySource(path);
    return;
  }
  state.tabs.push(path);
  state.activeTab = path;
  renderTabs();

  try {
    let content = await hydrateTabContent(path);
    if (path.split('/').pop()?.toLowerCase() === 'readme.md' && !content.trim()) {
      content = defaultReadmeContent(path);
      state.dirty.add(path);
      state.tabContents.set(path, content);
    }
    activateTabShell(path);
    setEditorContent(path, state.tabContents.get(path));
    renderTabs();
    $$('.tree-file').forEach((b) => b.classList.toggle('active', treePathMatchesTab(b.dataset.path, path)));
    if (!skipPrimaryNav) navigateToPrimarySource(path);
    scheduleGradleInfoRefresh();
  } catch (err) {
    state.tabs = state.tabs.filter((t) => t !== path);
    state.tabContents.delete(path);
    state.dirty.delete(path);
    if (state.activeTab === path) {
      const next = state.tabs[state.tabs.length - 1] ?? null;
      if (next) activateTab(next);
      else closeAllTabs();
    } else {
      renderTabs();
    }
    if (!silent) toast(err.message, 'error');
  }
}

function scheduleRenderTabs({ immediate = false } = {}) {
  if (immediate) {
    clearTimeout(tabRenderTimer);
    tabRenderTimer = null;
    renderTabs();
    return;
  }
  clearTimeout(tabRenderTimer);
  tabRenderTimer = setTimeout(() => {
    tabRenderTimer = null;
    renderTabs();
  }, TAB_RENDER_DELAY_MS);
}

function markActiveTabDirty() {
  if (!state.activeTab || isExternalEditorPath(state.activeTab)) return;
  const wasDirty = state.dirty.has(state.activeTab);
  state.dirty.add(state.activeTab);
  updateSaveButton();
  if (!wasDirty) {
    const tab = document.querySelector(`.ij-tab[data-tab="${CSS.escape(state.activeTab)}"]`);
    tab?.classList.add('dirty');
  }
}

function scheduleEditorContentSync() {
  clearTimeout(editorContentSyncTimer);
  editorContentSyncTimer = setTimeout(() => {
    editorContentSyncTimer = null;
    if (state.suppressEditorChange || !state.activeTab || !state.editor) return;
    if (isExternalEditorPath(state.activeTab)) return;
    state.tabContents.set(state.activeTab, state.editor.getValue());
  }, EDITOR_CONTENT_SYNC_DELAY_MS);
}

function scheduleTestRunDecorations() {
  if (!state.activeTab?.endsWith('.java')) return;
  clearTimeout(testDecorTimer);
  testDecorTimer = setTimeout(() => {
    testDecorTimer = null;
    applyTestRunDecorations();
  }, TEST_DECOR_DELAY_MS);
}

function scheduleCoverageClear() {
  if (!state.activeTab) return;
  clearTimeout(coverageClearTimer);
  coverageClearTimer = setTimeout(() => {
    coverageClearTimer = null;
    if (!state.activeTab) return;
    state.fileCoverage.delete(state.activeTab);
    clearCoverageDecorations();
    updateCoverageStatus(null);
  }, COVERAGE_CLEAR_DELAY_MS);
}

function renderTabs() {
  const list = $('#tab-list');
  if (!list) return;
  if (!state.tabs.length) {
    list.innerHTML = '';
    return;
  }
  const tabsHtml = state.tabs.map((t, i) => {
    const name = t.split('/').pop();
    const active = state.activeTab === t ? ' active' : '';
    const external = isExternalEditorPath(t);
    const dirty = !external && state.dirty.has(t) ? ' dirty' : '';
    const externalCls = external ? ' external' : '';
    const title = external ? ` title="${escapeHtml(t)}"` : '';
    return `<div class="ij-tab${active}${dirty}${externalCls}" data-tab-idx="${i}" data-tab="${escapeHtml(t)}"${title}><span class="ij-tab-label">${escapeHtml(name)}</span><button type="button" class="ij-tab-close" title="Close" aria-label="Close tab">×</button></div>`;
  }).join('');
  list.innerHTML = tabsHtml;
  updateRunButtons();
}

function resolveTabPath(path) {
  if (!path) return null;
  const idx = state.tabs.indexOf(path);
  if (idx >= 0) return state.tabs[idx];
  const norm = workspaceExplorerPath(path);
  const alt = state.tabs.find((t) => workspaceExplorerPath(t) === norm);
  return alt ?? null;
}

function closeTab(path, e) {
  e?.preventDefault?.();
  e?.stopPropagation?.();
  path = resolveTabPath(path);
  if (!path) return;
  const idx = state.tabs.indexOf(path);
  if (idx < 0) return;

  const wasActive = state.activeTab === path;
  state.tabs.splice(idx, 1);
  state.tabContents.delete(path);
  state.dirty.delete(path);

  if (wasActive) {
    const next = state.tabs[idx] ?? state.tabs[idx - 1] ?? null;
    if (next) activateTab(next, { skipFlush: true });
    else closeAllTabs();
  } else {
    scheduleRenderTabs({ immediate: true });
  }
  updateMenuState();
}

function tabPathFromEl(tabEl) {
  if (!tabEl) return null;
  const idx = tabEl.dataset.tabIdx;
  if (idx !== undefined && state.tabs[Number(idx)] !== undefined) {
    return state.tabs[Number(idx)];
  }
  return resolveTabPath(tabEl.dataset.tab);
}

function bindEditorTabs() {
  const list = $('#tab-list');
  if (!list || list.dataset.tabsBound === '1') return;
  list.dataset.tabsBound = '1';
  list.addEventListener('click', (e) => {
    const tabEl = e.target.closest('.ij-tab');
    if (!tabEl) return;
    const path = tabPathFromEl(tabEl);
    if (!path) return;
    if (e.target.closest('.ij-tab-close')) {
      e.preventDefault();
      e.stopPropagation();
      closeTab(path, e);
      return;
    }
    activateTab(path);
  });
}

function activateTabShell(path) {
  javaCompileFooterGen += 1;
  clearTimeout(compileFooterSafetyTimer);
  compileFooterSafetyTimer = null;
  state.activeTab = path;
  $('#editor-stage')?.classList.remove('hidden');
  $('#editor-toolbar')?.classList.remove('hidden');
  $('#editor-toolbar')?.classList.add('flex');
  $('#empty-state')?.classList.add('hidden');
  if (state.tabContents.has(path)) {
    setEditorContent(path, state.tabContents.get(path));
  }
  if (state.conflictFiles.has(path)) {
    state.conflictPanelHidden = false;
    setMainView('conflict');
  } else {
    setMainView('editor');
  }
  updateBreadcrumbs(path);
  updateStatusLanguage(path);
  if (state.editor) updateEditorStatus(state.editor.getPosition());
  applyEditorReadOnlyForPath(path);
  $$('.tree-file').forEach((b) => b.classList.toggle('active', treePathMatchesTab(b.dataset.path, path)));
  updateSaveButton();
  scheduleGradleInfoRefresh();
  scheduleBuildTasksRefresh();
  schedulePackageManifestRefresh();
  scheduleDbViewerRefresh();
  if (path?.endsWith('.sql')) {
    if (state.dbViewerPanelOpen) syncDbSqlFromActiveTab();
    state.serverRunTarget = null;
    updateRunButtons();
  } else if (isRunToolbarPath(path)) {
    void refreshRunInfo();
  }
  if (path) void refreshLanguageContextForPath(path);
  ensureJavaModuleForPath(path);
  updateMenuState();
  setStatusMessage(path.split('/').pop() || path);
  fileDiags = [];
  diagJumpIndex = 0;
  updateDiagnosticsStatusBar(path, []);
  if (path?.endsWith('.java')) {
    scheduleJavaFullDiagnostics();
  } else {
    scheduleDiagnostics();
  }
  void revealFileInExplorer(path);
  updateTreeBackButton();
  updateConflictUi();
  renderBreakpointGlyphs();
  highlightDebugCurrentLine();
}

function flushEditorContentSync() {
  clearTimeout(editorContentSyncTimer);
  editorContentSyncTimer = null;
  if (state.suppressEditorChange || !state.activeTab || !state.editor) return;
  if (isExternalEditorPath(state.activeTab)) return;
  state.tabContents.set(state.activeTab, state.editor.getValue());
}

function activateTab(path, { skipFlush = false } = {}) {
  if (!skipFlush) flushEditorContentSync();
  // activateTabShell already loads content + breakpoint glyphs; a second
  // setEditorContent here used to wipe red dots after tab switches.
  activateTabShell(path);
  scheduleRenderTabs({ immediate: true });
  if (state.activePanel === 'structure') scheduleStructureRefresh();
}

function isDiagnosablePath(path) {
  return window.ReaperLang?.isDiagnosablePath(path) ?? false;
}

function scheduleDiagnostics() {
  if (diagTimer) clearTimeout(diagTimer);
  if (!state.repo || !state.editor || !state.activeTab) return;
  if (!isDiagnosablePath(state.activeTab)) {
    clearDiagnostics();
    return;
  }
  let delay = DIAG_DELAY_MS;
  if (state.activeTab.endsWith('.java')) delay = JAVA_DIAG_DELAY_MS;
  else if (isProjectBuildFile(state.activeTab)) delay = BUILD_DIAG_DELAY_MS;
  else if (window.ReaperLang?.isMarkupOrConfigPath?.(state.activeTab)) delay = CONFIG_DIAG_DELAY_MS;
  diagTimer = setTimeout(() => {
    diagTimer = null;
    if (!state.repo || !state.editor || !state.activeTab) return;
    if (state.activeTab.endsWith('.java')) {
      queueJavaDiagnostics(state.activeTab, state.editor.getValue(), { scope: 'typing', force: false });
    } else {
      void runDiagnostics();
    }
  }, delay);
}

function clearDiagDecorations() {
  if (!state.editor || !window.monaco || !state.diagDecorationIds.length) return;
  state.diagDecorationIds = state.editor.deltaDecorations(state.diagDecorationIds, []);
}

function clearDiagnostics() {
  fileDiags = [];
  diagJumpIndex = 0;
  updateDiagnosticsStatusBar(state.activeTab, []);
  clearDiagDecorations();
  state.editor?.clearQuickFixBulbs?.();
  const model = state.editor?.getModel();
  if (model && window.monaco) {
    monaco.editor.setModelMarkers(model, 'reaper-diagnostics', []);
  }
}

function adjustDiagnosticPosition(model, d) {
  let line = Math.max(1, d.line || 1);
  let column = Math.max(1, d.column || 1);
  const msg = String(d.message || '').toLowerCase();
  const isSemicolonExpected = msg.includes("'") && msg.includes(';') && msg.includes('expected');
  const isCloseParenExpected = msg.includes("'") && msg.includes(')') && msg.includes('expected');
  const isNotStatement = msg.includes('not a statement');

  if (line > 1 && (isSemicolonExpected || isCloseParenExpected || isNotStatement)) {
    const reported = model.getLineContent(line).trim();
    const looksLikeNextToken = !reported
      || reported === '}'
      || reported.startsWith('}')
      || reported === ')'
      || reported === ');'
      || reported.startsWith(');')
      || reported.startsWith(')')
      || reported.startsWith('catch ')
      || reported.startsWith('finally ')
      || reported.startsWith('else');

    if (looksLikeNextToken) {
      const prevLine = line - 1;
      const prev = model.getLineContent(prevLine);
      if (prev.trim()) {
        line = prevLine;
        column = Math.max(1, prev.trimEnd().length);
      }
    } else if (isSemicolonExpected) {
      // Javac often reports ';' expected on the NEXT statement after an incomplete line (e.g. orphan "a").
      let scan = line - 1;
      while (scan >= 1 && !model.getLineContent(scan).trim()) scan--;
      if (scan >= 1 && scan < line) {
        const prevText = model.getLineContent(scan);
        const trimmed = prevText.trim();
        if (trimmed && !trimmed.endsWith(';') && !trimmed.endsWith('{') && !trimmed.endsWith('}')) {
          line = scan;
          column = Math.max(1, prevText.trimEnd().length);
        }
      }
    }
  }

  return { ...d, line, column };
}

function lineWithoutJavaStringLiterals(lineText) {
  let out = '';
  let inString = false;
  for (const ch of lineText) {
    if (ch === '"') {
      inString = !inString;
      out += ' ';
    } else if (inString) {
      out += ' ';
    } else {
      out += ch;
    }
  }
  return out;
}

function diagnosticMarkerSpan(model, d) {
  const adjusted = adjustDiagnosticPosition(model, d);
  const line = Math.max(1, adjusted.line || 1);
  const lineText = model.getLineContent(line);
  if (!lineText.trim()) {
    return {
      startLineNumber: line,
      startColumn: 1,
      endLineNumber: line,
      endColumn: Math.max(2, lineText.length + 1),
    };
  }
  let startCol = Math.max(1, adjusted.column || 1);

  const msg = String(d.message || '');
  const msgLower = msg.toLowerCase();
  if (msgLower.includes('cannot find symbol') || msgLower.includes('does not exist')) {
    const sym = msg.match(/symbol:\s*(?:class|interface|variable|method|package)?\s*([A-Za-z_][\w.]*)/i)
      || msg.match(/package\s+([A-Za-z_][\w.]*)/i);
    if (sym?.[1]) {
      let name = sym[1].split('.').pop().replace(/\(\).*$/, '').replace(/\(\s*$/, '');
      const scanLine = lineWithoutJavaStringLiterals(lineText);
      const memberRe = new RegExp(`([A-Za-z_][\\w]*)\\.${name}(?!\\s*\\()`, 'g');
      let memberMatch = null;
      let m;
      while ((m = memberRe.exec(scanLine)) !== null) {
        memberMatch = m;
      }
      if (memberMatch) {
        const idx = memberMatch.index;
        startCol = idx + 1;
        return {
          startLineNumber: line,
          startColumn: startCol,
          endLineNumber: line,
          endColumn: startCol + memberMatch[0].length,
        };
      }
      const dotIdx = scanLine.indexOf(`${name}.`);
      const plainIdx = scanLine.indexOf(name);
      const idx = dotIdx >= 0 ? dotIdx : plainIdx;
      if (idx >= 0) {
        startCol = idx + 1;
        return {
          startLineNumber: line,
          startColumn: startCol,
          endLineNumber: line,
          endColumn: startCol + name.length,
        };
      }
      // Bare method call: exists() with no receiver
      const bareRe = new RegExp(`\\b${name}\\s*\\(`);
      const bareMatch = scanLine.match(bareRe);
      if (bareMatch && bareMatch.index != null) {
        startCol = bareMatch.index + 1;
        return {
          startLineNumber: line,
          startColumn: startCol,
          endLineNumber: line,
          endColumn: startCol + name.length + 2,
        };
      }
    }
  }

  if (adjusted.line !== Math.max(1, d.line || 1)) {
    const trimmed = lineText.trimEnd();
    const lastToken = trimmed.match(/(\S+)$/);
    if (lastToken) {
      startCol = trimmed.length - lastToken[1].length + 1;
    }
  } else if (!d.column || d.column <= 1) {
    const first = lineText.search(/\S/);
    if (first >= 0) startCol = first + 1;
  }
  const endLine = Math.max(line, d.end_line || d.line || line);
  let endCol = d.end_column || 0;
  if (!endCol || endCol <= startCol) {
    const rest = lineText.slice(startCol - 1);
    const token = rest.match(/^\S+/);
    endCol = token
      ? startCol + token[0].length
      : Math.max(startCol + 1, lineText.length + 1);
  }
  // Keep underlines short so squiggles don't blanket whole lines of syntax.
  const maxSpan = 40;
  if (endLine === line && endCol - startCol > maxSpan) {
    endCol = startCol + maxSpan;
  }
  if (endLine > line) {
    const lineText = model.getLineContent(line);
    endCol = Math.min(endCol, Math.max(startCol + 1, lineText.length + 1));
  }
  return {
    startLineNumber: line,
    startColumn: startCol,
    endLineNumber: line,
    endColumn: endCol,
  };
}

function updateAiFixControls(errors) {
  const show = errors > 0;
  const title = show
    ? `Quick fix for ${errors} error${errors === 1 ? '' : 's'} (⌘.) — local fixes + Cursor/Gemini`
    : 'Quick fix';
  const dock = $('#ai-bulb-dock');
  const tb = $('#tb-ai-fix');
  const status = $('#status-ai-fix');
  if (dock) {
    dock.classList.toggle('hidden', !show);
    dock.setAttribute('aria-hidden', show ? 'false' : 'true');
  }
  if (tb) {
    tb.disabled = !show;
    tb.classList.toggle('is-glowing', show);
    tb.title = title;
  }
  if (status) {
    status.classList.toggle('hidden', !show);
    status.classList.toggle('is-glowing', show);
    status.title = title;
  }
}

function aiFixMenuAnchor() {
  const dock = $('#ai-bulb-dock');
  const tb = $('#tb-ai-fix');
  if (dock && !dock.classList.contains('hidden') && tb) return tb;
  const status = $('#status-ai-fix');
  if (status && !status.classList.contains('hidden')) return status;
  return tb || status;
}

function updateDiagnosticsStatusBar(path, diags) {
  const el = $('#status-diagnostics');
  const countEl = $('#status-diag-count');
  const primaryEl = $('#status-diag-primary');
  if (!el || !countEl) return;

  const errors = diags.filter((d) => d.severity !== 'warning').length;
  const warnings = diags.filter((d) => d.severity === 'warning').length;

  if (!errors && !warnings) {
    el.classList.add('hidden');
    el.classList.remove('has-errors', 'has-warnings');
    countEl.textContent = '';
    if (primaryEl) primaryEl.textContent = '';
    el.title = 'No problems';
    updateAiFixControls(0);
    hideQuickFixMenu();
    updateDiagnosticHintAtCursor(state.editor?.getPosition());
    return;
  }

  el.classList.remove('hidden');
  el.classList.toggle('has-errors', errors > 0);
  el.classList.toggle('has-warnings', errors === 0 && warnings > 0);

  const parts = [];
  if (errors) parts.push(`${errors} error${errors === 1 ? '' : 's'}`);
  if (warnings) parts.push(`${warnings} warning${warnings === 1 ? '' : 's'}`);
  countEl.textContent = parts.join(', ');

  const model = state.editor?.getModel();
  const primary = primaryDiagnosticForUi(model, diags);
  const primaryText = primary ? formatDiagnosticDisplay(primary) : '';
  if (primaryEl) {
    primaryEl.textContent = primaryText ? truncateDiagnosticText(primaryText, 56) : '';
    primaryEl.title = primaryText;
  }

  const name = path?.split('/').pop() || 'file';
  const titleParts = [`${name}: ${parts.join(', ')}`];
  if (primaryText) titleParts.push(primaryText);
  titleParts.push('click to go to next problem');
  el.title = titleParts.join(' — ');

  updateAiFixControls(errors);
  updateDiagnosticHintAtCursor(state.editor?.getPosition());
}

function hideQuickFixMenu() {
  const pop = $('#ai-quickfix-popover');
  if (pop) {
    pop.classList.add('hidden');
    pop.classList.remove('ij-cascade-popover');
    pop.innerHTML = '';
    delete pop.dataset.anchorLine;
  }
}

function isQuickFixMenuOpen() {
  const pop = $('#ai-quickfix-popover');
  return !!(pop && !pop.classList.contains('hidden'));
}

function quickFixMenuLabel(f) {
  const title = String(f?.title || 'Fix').replace(/</g, '&lt;');
  if (f?.provider === 'loading') return title;
  if (/^(AI|Cursor):/i.test(title)) return title;
  if (f?.provider === 'local') return title;
  const src = f?.provider === 'cursor' ? 'Cursor' : 'AI';
  return `${src}: ${title}`;
}

function isValidAnchorRect(rect) {
  if (!rect) return false;
  const w = Number(rect.width) || 0;
  const h = Number(rect.height) || 0;
  if (w <= 0 || h <= 0) return false;
  // Detached DOM nodes report 0,0,0,0 — treat as invalid (was sending menu to top-left).
  return rect.left !== 0 || rect.top !== 0 || rect.right !== 0 || rect.bottom !== 0;
}

function rectFromAnchor(anchor) {
  if (!anchor) return null;
  if (typeof anchor.getBoundingClientRect === 'function') {
    if (anchor.isConnected === false) return null;
    const rect = anchor.getBoundingClientRect();
    return isValidAnchorRect(rect) ? rect : null;
  }
  const left = Number(anchor.left) || 0;
  const top = Number(anchor.top) || 0;
  const width = Number(anchor.width) || 1;
  const height = Number(anchor.height) || 16;
  const rect = {
    left,
    top,
    width,
    height,
    right: Number(anchor.right ?? left + width),
    bottom: Number(anchor.bottom ?? top + height),
  };
  return isValidAnchorRect(rect) ? rect : null;
}

/** Prefer live gutter bulb for `line`, then given anchor, then toolbar/status bulbs. */
function resolveQuickFixAnchor(anchorEl, line) {
  const lineNum = Number(line);
  if (Number.isFinite(lineNum) && lineNum > 0 && state.editor?._reaperQuickFixBulbs?.length) {
    for (const widget of state.editor._reaperQuickFixBulbs) {
      const pos = widget.getPosition?.();
      const node = widget.getDomNode?.();
      if (pos?.range?.startLineNumber === lineNum && node?.isConnected) {
        const rect = rectFromAnchor(node);
        if (rect) return { el: node, rect, fromGutter: true };
      }
    }
  }

  const primary = rectFromAnchor(anchorEl);
  if (primary) {
    const fromGutter = !!anchorEl?.classList?.contains('ij-quickfix-glyph-bulb');
    return { el: anchorEl, rect: primary, fromGutter };
  }

  const glowing = document.querySelector('.ij-quickfix-glyph-bulb.is-glowing');
  const glowingRect = rectFromAnchor(glowing);
  if (glowingRect) return { el: glowing, rect: glowingRect, fromGutter: true };

  const fallback = aiFixMenuAnchor();
  const fallbackRect = rectFromAnchor(fallback);
  if (fallbackRect) return { el: fallback, rect: fallbackRect, fromGutter: false };

  return null;
}

function positionPopoverNearAnchor(pop, anchorEl, line) {
  if (!pop) return;
  const resolved = resolveQuickFixAnchor(anchorEl, line);
  if (!resolved) {
    pop.style.left = '8px';
    pop.style.top = '72px';
    return;
  }
  const { rect, fromGutter } = resolved;
  // Gutter bulbs: open to the right into the editor. Toolbar/status: align to the control.
  const preferredLeft = fromGutter ? rect.right + 6 : rect.left;
  pop.style.left = `${Math.max(8, Math.min(preferredLeft, window.innerWidth - 280))}px`;

  const placeBelow = () => {
    pop.style.top = `${Math.min(rect.bottom + 6, window.innerHeight - 40)}px`;
  };

  if (fromGutter || rect.top < 96) {
    placeBelow();
    return;
  }

  requestAnimationFrame(() => {
    const popRect = pop.getBoundingClientRect();
    const top = rect.top - popRect.height - 6;
    if (top < 8) placeBelow();
    else pop.style.top = `${top}px`;
  });
}

function showQuickFixMenu(fixes, onPick, anchorEl, line) {
  const pop = $('#ai-quickfix-popover');
  if (!pop || !fixes?.length) return;
  pop.classList.remove('ij-cascade-popover');
  const anchorLine = line != null
    ? line
    : (pop.dataset.anchorLine ? Number(pop.dataset.anchorLine) : null);
  if (anchorLine != null && Number.isFinite(Number(anchorLine))) {
    pop.dataset.anchorLine = String(anchorLine);
  }
  pop.innerHTML = fixes.map((f, i) => {
    const title = quickFixMenuLabel(f);
    const loading = f?.provider === 'loading' || !f?.edits?.length;
    if (loading) {
      return `<div class="ij-quickfix-item ij-quickfix-item--loading" data-idx="${i}">${title}</div>`;
    }
    return `<button type="button" class="ij-quickfix-item" data-idx="${i}">${title}</button>`;
  }).join('');
  pop.classList.remove('hidden');
  positionPopoverNearAnchor(pop, anchorEl, anchorLine);
  // Reposition after layout — glyph widgets / taller menus after AI results load.
  requestAnimationFrame(() => positionPopoverNearAnchor(pop, anchorEl, anchorLine));
  pop.querySelectorAll('.ij-quickfix-item:not(.ij-quickfix-item--loading)').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const fix = fixes[Number(btn.dataset.idx)];
      hideQuickFixMenu();
      if (fix?.edits?.length) onPick(fix);
    });
  });
}

/** Group jdtls refactor actions for the picker (Extract → …). */
function refactorMenuCategory(item) {
  const kind = String(item?.kind || '').toLowerCase();
  const title = String(item?.title || '');
  if (kind.startsWith('refactor.extract') || /\bextract\b/i.test(title)) return 'Extract';
  if (kind.startsWith('refactor.inline') || /\binline\b/i.test(title)) return 'Inline';
  if (kind.startsWith('refactor.rewrite') || /change signature|convert|rewrite/i.test(title)) return 'Rewrite';
  if (kind.startsWith('source')) return 'Source';
  if (kind.startsWith('refactor')) return 'Refactor';
  return 'More';
}

const REFACTOR_MENU_ORDER = ['Extract', 'Inline', 'Rewrite', 'Refactor', 'Source', 'More'];

/**
 * Refactor picker: flat list with section headings (same chrome as quick-fix).
 * Click an action to apply — no nested / cascading panels.
 */
function showRefactorStaircaseMenu(items, onPick, anchorEl) {
  const pop = $('#ai-quickfix-popover');
  if (!pop || !items?.length) return;

  const groups = new Map();
  for (const item of items) {
    const cat = refactorMenuCategory(item);
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat).push(item);
  }
  const categories = REFACTOR_MENU_ORDER.filter((c) => groups.has(c));
  if (!categories.length) return;

  if (items.length === 1) {
    onPick(items[0]);
    return;
  }

  // Flat list — skip headings when everything is one category.
  const flatOnly = categories.length === 1;
  const rows = [];
  for (const cat of categories) {
    if (!flatOnly) {
      rows.push({ type: 'heading', label: cat });
    }
    for (const item of groups.get(cat)) {
      rows.push({ type: 'item', item });
    }
  }

  pop.classList.remove('ij-cascade-popover');
  pop.innerHTML = rows.map((row, i) => {
    if (row.type === 'heading') {
      return `<div class="ij-quickfix-heading">${String(row.label).replace(/</g, '&lt;')}</div>`;
    }
    const title = String(row.item.title || 'Refactor').replace(/</g, '&lt;');
    return `<button type="button" class="ij-quickfix-item" data-row="${i}">${title}</button>`;
  }).join('');
  pop.classList.remove('hidden');
  positionPopoverNearAnchor(pop, anchorEl);
  requestAnimationFrame(() => positionPopoverNearAnchor(pop, anchorEl));
  pop.querySelectorAll('.ij-quickfix-item[data-row]').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const row = rows[Number(btn.dataset.row)];
      hideQuickFixMenu();
      if (row?.type === 'item' && row.item?.edits?.length) onPick(row.item);
    });
  });
}

function findDiagnosticNearLine(model, line) {
  if (!fileDiags.length) return null;
  const exact = fileDiags.find((d) => adjustDiagnosticPosition(model, d).line === line);
  if (exact) return exact;
  let best = null;
  let bestDist = Infinity;
  for (const d of fileDiags) {
    const adj = adjustDiagnosticPosition(model, d);
    const dist = Math.abs(adj.line - line);
    if (dist < bestDist) {
      bestDist = dist;
      best = d;
    }
  }
  return best;
}

function jumpToDiagnostic(d) {
  const model = state.editor?.getModel();
  if (!state.editor || !model || !d) return;
  const span = diagnosticMarkerSpan(model, d);
  state.editor.setPosition({ lineNumber: span.startLineNumber, column: span.startColumn });
  state.editor.setSelection(new monaco.Range(
    span.startLineNumber,
    span.startColumn,
    span.endLineNumber,
    span.endColumn,
  ));
  state.editor.revealLineInCenter(span.startLineNumber);
  state.editor.focus();
}

function jumpToNextDiagnostic() {
  if (!state.editor || !fileDiags.length) return;
  const sorted = [...fileDiags].sort(
    (a, b) => (a.line || 1) - (b.line || 1) || (a.column || 1) - (b.column || 1),
  );
  const d = sorted[diagJumpIndex % sorted.length];
  diagJumpIndex = (diagJumpIndex + 1) % sorted.length;
  jumpToDiagnostic(d);
}

function lineFromEditorMouseEvent(editor, e) {
  if (e.target.position?.lineNumber) {
    return e.target.position.lineNumber;
  }
  const clientX = e.event.browserEvent.clientX;
  const clientY = e.event.browserEvent.clientY;
  const target = editor.getTargetAtClientPoint(clientX, clientY);
  if (target?.position?.lineNumber) {
    return target.position.lineNumber;
  }
  const model = editor.getModel();
  if (!model) return null;
  const dom = editor.getDomNode();
  if (!dom) return null;
  const layout = editor.getLayoutInfo();
  const rect = dom.getBoundingClientRect();
  const yInContent = clientY - rect.top - layout.contentTop + editor.getScrollTop();
  const lineHeight = editor.getOption(monaco.editor.EditorOption.lineHeight);
  if (!lineHeight) return null;
  return Math.max(1, Math.min(Math.floor(yInContent / lineHeight) + 1, model.getLineCount()));
}

function setupDiagnosticNavigation(editor) {
  if (!window.monaco) return;
  const glyph = monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN;
  const ruler = monaco.editor.MouseTargetType.OVERVIEW_RULER;
  let lastNavAt = 0;

  editor.onMouseUp((e) => {
    if (!fileDiags.length) return;
    const t = e.target.type;
    if (t !== glyph && t !== ruler) return;
    const now = Date.now();
    if (now - lastNavAt < 250) return;
    const model = editor.getModel();
    if (!model) return;
    const line = lineFromEditorMouseEvent(editor, e);
    if (!line) return;
    const d = findDiagnosticNearLine(model, line);
    if (!d) return;
    lastNavAt = now;
    jumpToDiagnostic(d);
    e.event.preventDefault();
    e.event.stopPropagation();
  });
}

async function runDiagnostics() {
  diagTimer = null;
  if (!state.repo || !state.editor || !state.activeTab || !isDiagnosablePath(state.activeTab)) {
    clearDiagnostics();
    return;
  }
  if (state.activeTab.endsWith('.java')) return;
  const path = state.activeTab;
  const content = state.editor.getValue();
  const seq = ++diagSeq;
  try {
    const result = normalizeDiagnosticsResponse(
      await fetchDiagnosticsForPath(path, content, { scope: 'typing' }),
    );
    if (seq !== diagSeq || path !== state.activeTab) return;
    if (result.cancelled && !result.diagnostics.length) {
      clearTimeout(diagRetryTimer);
      diagRetryTimer = setTimeout(() => {
        diagRetryTimer = null;
        if (state.activeTab === path) void runDiagnostics();
      }, diagRetryDelayMs);
      diagRetryDelayMs = Math.min(Math.round(diagRetryDelayMs * 1.5), 15000);
      return;
    }
    clearTimeout(diagRetryTimer);
    diagRetryTimer = null;
    diagRetryDelayMs = 2500;
    applyDiagnostics(path, result.diagnostics);
  } catch (err) {
    if (isDiagFetchAbort(err)) return;
    if (seq === diagSeq) clearDiagnostics();
  }
}

/** Classpath / batch edits — refresh every open Java tab (not on typing). */
function scheduleAllJavaDiagnostics() {
  if (!state.repo) return;
  clearTimeout(allJavaDiagTimer);
  allJavaDiagTimer = setTimeout(() => {
    allJavaDiagTimer = null;
    void refreshAllJavaTabDiagnostics();
  }, ALL_JAVA_DIAG_DELAY_MS);
}

function isDiagFetchAbort(err) {
  return err?.name === 'AbortError';
}

/** Free browser HTTP slots before save — long javac POSTs must not starve PUT /workspace/file. */
function abortAllDiagnosticFetches() {
  for (const entry of diagFetchByPath.values()) {
    entry.controller.abort();
  }
  diagFetchByPath.clear();
  allJavaDiagRefreshGen += 1;
  javaFullDiagSeq += 1;
}

function collectJavaDiagnosticOverlays(excludePath) {
  const overlays = [];
  for (const path of state.tabs) {
    if (!isProjectSourceFile(path) || path === excludePath) continue;
    const content = path === state.activeTab && state.editor
      ? state.editor.getValue()
      : state.tabContents.get(path);
    if (content == null) continue;
    overlays.push({ path, content });
  }
  return overlays;
}

function normalizeDiagnosticsResponse(body) {
  if (Array.isArray(body)) {
    return { diagnostics: body, cancelled: false };
  }
  return {
    diagnostics: Array.isArray(body?.diagnostics) ? body.diagnostics : [],
    cancelled: body?.cancelled === true,
  };
}

const JAVA_DIAG_FETCH_TIMEOUT_MS = 60000;
const SAVE_FETCH_TIMEOUT_MS = 15000;
const SAVE_MAX_RETRIES = 2;

async function fetchDiagnosticsForPath(path, content, { scope = 'typing', signal: outerSignal, force = false } = {}) {
  const prev = diagFetchByPath.get(path);
  // Same path+content+scope: share in-flight compile (force must not cancel duplicate saves).
  if (prev && prev.content === content && prev.scope === scope) {
    if (outerSignal?.aborted) {
      throw new DOMException('Aborted', 'AbortError');
    }
    return prev.promise;
  }
  // Full javac with stale buffer: chain after in-flight compile (never abort — matches save→javac integration).
  if (prev && prev.scope === 'full' && scope === 'full' && prev.content !== content) {
    if (outerSignal?.aborted) {
      throw new DOMException('Aborted', 'AbortError');
    }
    return prev.promise.then(
      () => fetchDiagnosticsForPath(path, content, { scope: 'full', signal: outerSignal }),
      (err) => {
        if (err?.name === 'AbortError') throw err;
        return fetchDiagnosticsForPath(path, content, { scope: 'full', signal: outerSignal });
      },
    );
  }
  if (prev) prev.controller.abort();

  const controller = new AbortController();
  if (outerSignal) {
    if (outerSignal.aborted) controller.abort();
    else outerSignal.addEventListener('abort', () => controller.abort(), { once: true });
  }

  const promise = (async () => {
    const timeoutId = scope === 'full'
      ? setTimeout(() => controller.abort(), JAVA_DIAG_FETCH_TIMEOUT_MS)
      : null;
    try {
      return await api(repoApi(state.repo, '/workspace/diagnostics'), {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Connection: 'close',
        },
        signal: controller.signal,
        body: JSON.stringify({
          path,
          content,
          scope,
          overlays: scope === 'full' ? collectJavaDiagnosticOverlays(path) : [],
        }),
      });
    } finally {
      if (timeoutId) clearTimeout(timeoutId);
      if (diagFetchByPath.get(path)?.controller === controller) {
        diagFetchByPath.delete(path);
      }
    }
  })();

  diagFetchByPath.set(path, { controller, promise, content, scope });
  return promise;
}

/** Re-validate every open Java tab after classpath or batch workspace edits. */
async function refreshAllJavaTabDiagnostics() {
  if (!state.repo) return;
  let javaTabs = state.tabs.filter((p) => isDiagnosablePath(p));
  if (!javaTabs.length) return;
  const refreshGen = ++allJavaDiagRefreshGen;
  const activePath = state.activeTab;
  for (const path of javaTabs) {
    if (path === activePath) continue;
    if (refreshGen !== allJavaDiagRefreshGen) return;
    const content = path === activePath && state.editor
      ? state.editor.getValue()
      : (state.tabContents.get(path) ?? '');
    try {
      const result = normalizeDiagnosticsResponse(
        await fetchDiagnosticsForPath(path, content, { scope: 'full' }),
      );
      if (refreshGen !== allJavaDiagRefreshGen) return;
      if (result.cancelled && !result.diagnostics.length) continue;
      if (path === state.activeTab) {
        applyDiagnostics(path, result.diagnostics);
      }
    } catch (err) {
      if (!isDiagFetchAbort(err)) { /* ignore transient errors */ }
    }
    if (refreshGen !== allJavaDiagRefreshGen) return;
    await new Promise((resolve) => setTimeout(resolve, JAVA_DIAG_FULL_STAGGER_MS));
  }
}

let javaFullDiagDeferTimer = null;

/** Coalesce Java javac: one compile at a time, latest buffer wins. */
function queueJavaDiagnostics(path, content, { scope = 'full', immediate = false, force = false } = {}) {
  if (!state.repo || !path?.endsWith('.java')) return;
  if (!usesInProcessApi() && (saveGateActive || state.saveInFlight || state.autoSaveInFlight)) {
    clearTimeout(javaFullDiagDeferTimer);
    javaFullDiagDeferTimer = setTimeout(() => {
      javaFullDiagDeferTimer = null;
      queueJavaDiagnostics(path, content, { scope, immediate, force });
    }, 50);
    return;
  }
  const snapshot = content ?? state.editor?.getValue();
  if (snapshot == null) return;
  javaFullCompilePending = { path, content: snapshot, force, scope };
  clearTimeout(javaFullDiagTimer);
  javaFullDiagTimer = setTimeout(() => {
    void flushJavaDiagnosticQueue();
  }, immediate ? 0 : JAVA_QUEUE_DIAG_DELAY_MS);
}

function queueJavaFullDiagnostics(path, content, opts = {}) {
  return queueJavaDiagnostics(path, content, { scope: 'full', ...opts });
}

async function flushJavaDiagnosticQueue() {
  javaFullDiagTimer = null;
  if (javaFullCompileRunning) return;
  while (javaFullCompilePending) {
    const job = javaFullCompilePending;
    javaFullCompilePending = null;
    if (job.path !== state.activeTab) continue;
    javaFullCompileRunning = true;
    try {
      const latest = state.editor?.getValue() ?? job.content;
      await runJavaDiagnosticsForPath(job.path, latest, {
        scope: job.scope ?? 'full',
        force: job.force,
      });
    } finally {
      javaFullCompileRunning = false;
    }
  }
}

/** Debounced full javac after auto-save or tab switch (auto-save is on by default). */
function scheduleJavaFullDiagnostics() {
  if (!state.repo || !state.activeTab?.endsWith('.java')) return;
  queueJavaFullDiagnostics(state.activeTab, state.editor?.getValue(), { force: false });
}

function refreshProjectClasspathUi() {
  void refreshAllJavaTabDiagnostics();
  if (hasAutoReloadProject()) void refreshRunInfo();
  if (state.activeTab?.endsWith('.java') || isNativeSourcePath(state.activeTab)) applyTestRunDecorations();
}

function applyDiagnostics(path, diags) {
  const model = state.editor?.getModel();
  if (!model || state.activeTab !== path || !window.monaco) return;
  fileDiags = diags;
  diagJumpIndex = 0;
  const markers = diags.map((d) => ({
    ...diagnosticMarkerSpan(model, d),
    severity: d.severity === 'warning'
      ? monaco.MarkerSeverity.Warning
      : (d.severity === 'info' || d.severity === 'hint')
        ? monaco.MarkerSeverity.Info
        : monaco.MarkerSeverity.Error,
    message: String(d.message || '').trim() || `Problem at line ${d.line || 1}`,
    source: d.source || undefined,
  }));
  monaco.editor.setModelMarkers(model, 'reaper-diagnostics', markers);
  updateDiagnosticsStatusBar(path, diags);
  state.editor?.prefetchAiQuickFixes?.(diags);
  state.editor?.refreshQuickFixBulbs?.(diags);
}

function isGradleFilePath(path) {
  if (!path) return false;
  const normalized = path.replace(/\\/g, '/').toLowerCase();
  const base = normalized.split('/').pop() || '';
  if (base.endsWith('.gradle') || base.endsWith('.gradle.kts')) return true;
  if (base === 'gradle.properties' || base === 'gradlew' || base === 'gradlew.bat') return true;
  if (normalized.endsWith('/gradle/libs.versions.toml') || normalized === 'gradle/libs.versions.toml') return true;
  if (normalized.startsWith('gradle/wrapper/')) return true;
  return false;
}

function isMavenFilePath(path) {
  if (!path) return false;
  const normalized = path.replace(/\\/g, '/').toLowerCase();
  const base = normalized.split('/').pop() || '';
  if (base === 'pom.xml') return true;
  if (base === 'mvnw' || base === 'mvnw.cmd') return true;
  if (normalized.endsWith('/.mvn/wrapper/maven-wrapper.properties')) return true;
  if (normalized.startsWith('.mvn/')) return true;
  return false;
}

function isJavaTestClass(path, content) {
  if (!path?.endsWith('.java')) return false;
  if (/@SpringBootTest\b/.test(content)) return true;
  if (isJavaTestFilePath(path)) return true;
  if (/@Test|@ParameterizedTest|@RepeatedTest|@TestFactory|@TestTemplate/.test(content)) {
    return true;
  }
  return listJavaTestMethods(path, content).length > 0;
}

function testFilterForJavaFile(path, content, cursorLine) {
  const methods = listJavaTestMethods(path, content);
  const classEntry = methods.find((m) => m.isClass);
  const classFilter = classEntry?.filter || javaFqcnFromSource(path, content);
  if (!cursorLine) return classFilter;
  if (classEntry && cursorLine >= classEntry.line && cursorLine <= classEntry.end_line) {
    return classFilter;
  }
  const matches = methods.filter(
    (m) => !m.isClass && cursorLine >= m.line && cursorLine <= m.end_line,
  );
  if (matches.length) return matches[matches.length - 1].filter;
  return classFilter;
}

function isJavaMainSourceFile(path) {
  if (!path?.endsWith('.java') || isJavaTestFilePath(path)) return false;
  const normalized = stripJavaDiagOverlayPath(path).replace(/\\/g, '/');
  if (normalized.includes('/src/main/java/') || normalized.includes('/main/java/')) return true;
  return !normalized.includes('/src/test/') && !normalized.includes('/test/java/');
}

/** Test filter for coverage runs from a test or production Java file. */
function coverageTestFilterForFile(path, content, cursorLine) {
  path = stripJavaDiagOverlayPath(path);
  if (!path?.endsWith('.java')) return null;
  if (isJavaTestClass(path, content)) {
    return testFilterForJavaFile(path, content, cursorLine);
  }
  const fqcn = javaFqcnFromSource(path, content);
  if (!fqcn || isJavaTestFilePath(path)) return null;
  return `${fqcn}Test`;
}

async function runActiveFileWithCoverage() {
  if (!state.repo || !state.activeTab?.endsWith('.java')) {
    toast('Open a Java file to run with coverage', 'info');
    return;
  }
  const path = state.activeTab;
  const content = state.editor?.getModel()?.getValue() ?? state.tabContents.get(path) ?? '';
  const line = state.editor?.getPosition()?.lineNumber || 1;
  const filter = coverageTestFilterForFile(path, content, line);
  if (!filter) {
    toast('Could not determine a test filter for this file', 'info');
    return;
  }
  await runProjectTestWithCoverage(filter);
}

function springBootRunTarget(content, info) {
  const spring = detectSpringBootApp(content);
  if (!spring || !info?.has_project) return null;
  return {
    mode: 'spring-boot',
    task: info.build_tool === 'maven'
      ? `spring-boot:run -Dspring-boot.run.mainClass=${spring.qualifiedName}`
      : `bootRun -Dspring-boot.run.main-class=${spring.qualifiedName}`,
    qualifiedName: spring.qualifiedName,
    runnable: true,
  };
}

/** What F5 / Run should do for the active editor file. */
function detectJavaRunTarget(path, content, runInfo, cursorLine) {
  path = stripJavaDiagOverlayPath(path);
  if (!path?.endsWith('.java')) {
    if (path?.endsWith('.rb')) {
      return detectRubyRunTargetFallback(path);
    }
    if (path?.endsWith('.py') || path?.endsWith('.pyw')) {
      return detectPythonRunTargetFallback(path);
    }
    if (path?.endsWith('.go')) {
      return detectGoRunTargetFallback(path);
    }
    if (path?.endsWith('.rs')) {
      return detectRustRunTargetFallback(path, content);
    }
    if (isJsOrTsSourcePath(path)) {
      return detectJsRunTargetFallback(path);
    }
    if (path?.endsWith('.kt') || path?.endsWith('.kts')) {
      return detectKotlinRunTargetFallback(path);
    }
    if (path?.endsWith('.php')) {
      return detectPhpRunTargetFallback(path);
    }
    if (path?.endsWith('.dart')) {
      return detectDartRunTargetFallback(path);
    }
    if (isNativeSourcePath(path)) {
      return detectNativeRunTargetFallback(path, content);
    }
    if (path?.endsWith('.sql')) {
      return detectSqlRunTargetFallback(path);
    }
    if (isShellScriptPath(path)) {
      return detectShellRunTargetFallback(path);
    }
    if (runInfo?.has_project && isRunToolbarPath(path)) {
      return { mode: 'project-task' };
    }
    return { mode: 'none' };
  }

  const spring = detectSpringBootApp(content);
  const main = detectJavaMain(content);

  if (isJavaTestClass(path, content) && runInfo?.has_project) {
    const filter = testFilterForJavaFile(path, content, cursorLine);
    if (filter) {
      return { mode: 'test', filter };
    }
  }

  if (spring && (runInfo?.is_spring_boot || runInfo?.frameworks?.includes('spring-boot'))) {
    return {
      mode: 'spring-boot',
      task: runInfo.default_task,
      qualifiedName: spring.qualifiedName,
    };
  }

  if (spring && runInfo?.has_project) {
    return {
      mode: 'spring-boot',
      task: runInfo.build_tool === 'maven'
        ? `spring-boot:run -Dspring-boot.run.mainClass=${spring.qualifiedName}`
        : `bootRun -Dspring-boot.run.main-class=${spring.qualifiedName}`,
      qualifiedName: spring.qualifiedName,
    };
  }

  if (main) {
    return { mode: 'main', qualifiedName: main.qualifiedName };
  }

  return { mode: 'none' };
}

function shellQuotePath(path) {
  return `'${String(path).replace(/'/g, "'\\''")}'`;
}

function detectRubyRunTargetFallback(path) {
  if (!path?.endsWith('.rb')) return { mode: 'none' };
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  if (base.endsWith('_spec.rb') || (normalized.includes('/spec/') && base.endsWith('.rb'))) {
    return {
      mode: 'ruby-test',
      classType: 'rspec',
      task: `bundle exec rspec ${shellQuotePath(path)}`,
      runnable: true,
    };
  }
  if (base.endsWith('_test.rb') || normalized.includes('/test/')) {
    return {
      mode: 'ruby-test',
      classType: 'rails-test',
      task: `bin/rails test ${shellQuotePath(path)}`,
      runnable: true,
    };
  }
  return {
    mode: 'ruby',
    classType: 'ruby-script',
    task: `ruby ${shellQuotePath(path)}`,
    runnable: true,
  };
}

function detectPythonRunTargetFallback(path) {
  if (!path?.endsWith('.py') && !path?.endsWith('.pyw')) return { mode: 'none' };
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  const isTest = (base.startsWith('test_') && base.endsWith('.py'))
    || base.endsWith('_test.py')
    || normalized.includes('/tests/')
    || (normalized.includes('/test/') && base.endsWith('.py'));
  if (isTest) {
    return {
      mode: 'python-test',
      classType: 'pytest',
      task: `python3 -m pytest ${shellQuotePath(path)}`,
      runnable: true,
    };
  }
  return {
    mode: 'python',
    classType: 'python-script',
    task: `python3 ${shellQuotePath(path)}`,
    runnable: true,
  };
}

function detectGoRunTargetFallback(path) {
  if (!path?.endsWith('.go')) return { mode: 'none' };
  const normalized = path.replace(/\\/g, '/');
  const base = normalized.split('/').pop() || '';
  if (base.endsWith('_test.go')) {
    const pkgDir = normalized.includes('/')
      ? `./${normalized.slice(0, normalized.lastIndexOf('/'))}`
      : '.';
    return {
      mode: 'go-test',
      classType: 'go-test',
      task: `go test ${shellQuotePath(pkgDir)} -v`,
      runnable: true,
    };
  }
  return {
    mode: 'go',
    classType: 'go-program',
    task: `go run ${shellQuotePath(path)}`,
    runnable: true,
  };
}

function detectRustRunTargetFallback(path, content = '') {
  if (!path?.endsWith('.rs')) return { mode: 'none' };
  const normalized = path.replace(/\\/g, '/');
  const text = content || state.tabContents.get(path) || '';
  const hasMain = /\bfn\s+main\s*\(/.test(text);
  const hasTests = /#\[\s*(?:tokio::)?test\s*\]|#\[\s*cfg\s*\(\s*test\s*\)\s*\]/.test(text);
  const isIntegrationTest = normalized.includes('/tests/');
  if (isIntegrationTest || (hasTests && !hasMain)) {
    return {
      mode: 'rust-test',
      classType: 'cargo-test',
      task: 'cargo test',
      frameworks: ['cargo'],
      runnable: true,
    };
  }
  if (hasMain) {
    return {
      mode: 'rust',
      classType: 'cargo-run',
      task: 'cargo run',
      frameworks: ['cargo'],
      runnable: true,
    };
  }
  if (hasTests) {
    return {
      mode: 'rust-test',
      classType: 'cargo-test',
      task: 'cargo test',
      frameworks: ['cargo'],
      runnable: true,
    };
  }
  return {
    mode: 'rust',
    classType: 'rust-source',
    frameworks: ['rust'],
    runnable: false,
    reason: 'No `fn main` or #[test] found in this file',
  };
}

function isJsOrTsTestPath(path) {
  const normalized = String(path || '').replace(/\\/g, '/').toLowerCase();
  const base = normalized.split('/').pop() || '';
  return base.includes('.test.') || base.includes('.spec.')
    || normalized.includes('/__tests__/') || normalized.includes('/tests/');
}

function detectJsRunTargetFallback(path) {
  if (!isJsOrTsSourcePath(path)) return { mode: 'none' };
  const isTs = /\.tsx?$/i.test(path);
  const isTest = isJsOrTsTestPath(path);
  const frameworks = [isTs ? 'typescript' : 'javascript'];
  if (isTest) {
    frameworks.push('test');
    return {
      mode: 'js-test',
      classType: 'js-test',
      task: `npx vitest run ${shellQuotePath(path)}`,
      frameworks,
      runnable: true,
    };
  }
  return {
    mode: 'js',
    classType: isTs ? 'ts-script' : 'js-script',
    task: isTs ? `npx tsx ${shellQuotePath(path)}` : `node ${shellQuotePath(path)}`,
    frameworks,
    runnable: true,
  };
}

function detectKotlinRunTargetFallback(path) {
  if (!path?.endsWith('.kt') && !path?.endsWith('.kts')) return { mode: 'none' };
  const norm = String(path).replace(/\\/g, '/');
  const isTest = norm.includes('/test/') || norm.includes('/tests/');
  if (isTest) {
    return { mode: 'kotlin-test', classType: 'kotlin-test', task: 'gradle test', frameworks: ['kotlin'], runnable: true };
  }
  if (path.endsWith('.kts')) {
    return { mode: 'kotlin', classType: 'kotlin-script', task: `kotlinc -script ${shellQuotePath(path)}`, frameworks: ['kotlin'], runnable: true };
  }
  return { mode: 'kotlin', classType: 'kotlin-script', task: `kotlinc ${shellQuotePath(path)} -include-runtime -d .reaper/kotlin-out.jar && java -jar .reaper/kotlin-out.jar`, frameworks: ['kotlin'], runnable: true };
}

function detectPhpRunTargetFallback(path) {
  if (!path?.endsWith('.php')) return { mode: 'none' };
  const norm = String(path).replace(/\\/g, '/').toLowerCase();
  const base = norm.split('/').pop() || '';
  const isTest = base.includes('test') || norm.includes('/test/') || norm.includes('/tests/');
  if (isTest) {
    return { mode: 'php-test', classType: 'phpunit', task: `php vendor/bin/phpunit ${shellQuotePath(path)}`, frameworks: ['php'], runnable: true };
  }
  return { mode: 'php', classType: 'php-script', task: `php ${shellQuotePath(path)}`, frameworks: ['php'], runnable: true };
}

function detectDartRunTargetFallback(path) {
  if (!path?.endsWith('.dart')) return { mode: 'none' };
  const norm = String(path).replace(/\\/g, '/');
  const base = (norm.split('/').pop() || '').toLowerCase();
  const isTest = base.endsWith('_test.dart') || norm.includes('/test/') || norm.includes('/tests/');
  if (isTest) {
    return { mode: 'dart-test', classType: 'dart-test', task: `dart test ${shellQuotePath(path)}`, frameworks: ['dart'], runnable: true };
  }
  return { mode: 'dart', classType: 'dart-script', task: `dart run ${shellQuotePath(path)}`, frameworks: ['dart'], runnable: true };
}

function cmakeExecutableForSource(cmakeText, sourcePath) {
  if (!cmakeText || !sourcePath) return null;
  const source = normalizeRepoPath(sourcePath);
  const flat = String(cmakeText).replace(/#[^\n]*/g, ' ');
  const re = /add_executable\s*\(\s*([A-Za-z0-9_.-]+)\s+([^)]+)\)/g;
  let m;
  while ((m = re.exec(flat)) !== null) {
    const target = m[1];
    const args = m[2];
    const matches = args.split(/\s+/).some((raw) => {
      const p = raw.replace(/^["']|["']$/g, '').replace(/\\/g, '/');
      return p === source || p.endsWith(`/${source}`) || source.endsWith(`/${p}`);
    });
    if (matches) return target;
  }
  return null;
}

function cmakeListsContentForPath(path) {
  if (state.tabContents.has('CMakeLists.txt')) return state.tabContents.get('CMakeLists.txt');
  const normalized = normalizeRepoPath(path);
  const dir = normalized.includes('/') ? normalized.slice(0, normalized.lastIndexOf('/')) : '';
  const candidates = dir
    ? [`${dir}/CMakeLists.txt`, 'CMakeLists.txt']
    : ['CMakeLists.txt'];
  for (const candidate of candidates) {
    if (state.tabContents.has(candidate)) return state.tabContents.get(candidate);
  }
  return null;
}

function detectNativeRunTargetFallback(path, content = '') {
  if (!isNativeSourcePath(path)) return { mode: 'none' };
  const lower = path.toLowerCase();
  const isCpp = lower.endsWith('.cpp') || lower.endsWith('.cc') || lower.endsWith('.cxx');
  const text = content || state.tabContents.get(path) || '';
  const hasGtest = /gtest\/gtest\.h|<gtest\/gtest\.h>|TEST\s*\(|TEST_F\s*\(/.test(text);
  const hasCatch2 = /catch2\/|TEST_CASE\s*\(/.test(text);
  if (hasGtest) {
    const compiler = isCpp ? 'clang++' : 'clang';
    const std = isCpp ? '-std=c++17' : '-std=c11';
    return {
      mode: 'native-test',
      classType: 'gtest',
      task: `mkdir -p .reaper && ${compiler} ${std}${lang} ${shellQuotePath(path)} -lgtest -lgtest_main -pthread -o .reaper/native-test-out && ./.reaper/native-test-out`,
      runnable: true,
    };
  }
  if (hasCatch2) {
    return {
      mode: 'native-test',
      classType: 'catch2',
      task: `mkdir -p .reaper && clang++ -std=c++17 -DCATCH_CONFIG_MAIN ${shellQuotePath(path)} -o .reaper/native-test-out && ./.reaper/native-test-out`,
      runnable: true,
    };
  }
  if (isNativeTestPath(path)) {
    return {
      mode: 'native-test',
      classType: 'cmake-test',
      task: 'cmake -B build -S . && cmake --build build && ctest --test-dir build --output-on-failure',
      runnable: true,
    };
  }
  if (/\bmain\s*\(/.test(text)) {
    const cmakeLists = cmakeListsContentForPath(path);
    const cmakeTarget = cmakeLists ? cmakeExecutableForSource(cmakeLists, path) : null;
    if (cmakeTarget) {
      return {
        mode: 'native',
        classType: 'cmake-run',
        task: `cmake -B build -S . && cmake --build build --target ${cmakeTarget} && ./build/${cmakeTarget}`,
        runnable: true,
      };
    }
    const compiler = isCpp ? 'clang++' : 'clang';
    const std = isCpp ? '-std=c++17' : '-std=c17';
    const lang = isCpp && !compiler.includes('++') ? ' -x c++' : '';
    return {
      mode: 'native',
      classType: isCpp ? 'cpp-program' : 'c-program',
      task: `mkdir -p .reaper && ${compiler} ${std}${lang} -o .reaper/native-out ${shellQuotePath(path)} && ./.reaper/native-out`,
      runnable: true,
    };
  }
  return {
    mode: 'none',
    classType: isCpp ? 'cpp-source' : 'c-source',
    reason: 'No main() or test framework detected in this file',
    runnable: false,
  };
}

function runTargetLabel(target, content, path) {
  const fw = target.frameworks?.length ? ` · ${target.frameworks.slice(0, 3).join(', ')}` : '';
  if (target.mode === 'test') {
    const short = target.filter?.split('.').pop() || target.filter;
    return `Test · ${short}${fw}`;
  }
  if (target.mode === 'spring-boot') {
    const name = target.qualifiedName || detectSpringBootApp(content)?.qualifiedName || 'app';
    return `Spring Boot · ${name.split('.').pop()}${fw}`;
  }
  if (target.mode === 'main') {
    return `Java · ${target.qualifiedName?.split('.').pop() || target.qualifiedName}${fw}`;
  }
  if (target.mode === 'ruby' || target.mode === 'ruby-test') {
    const base = path?.split('/').pop()?.replace(/\.rb$/, '') || 'script';
    if (target.mode === 'ruby-test') {
      const kind = target.classType === 'rails-test' ? 'Rails test' : 'RSpec';
      return `${kind} · ${base}${fw}`;
    }
    return `Ruby · ${base}${fw}`;
  }
  if (target.mode === 'python' || target.mode === 'python-test') {
    const base = path?.split('/').pop()?.replace(/\.pyw?$/, '') || 'script';
    if (target.mode === 'python-test') {
      const kind = target.classType === 'django-test' ? 'Django test' : 'pytest';
      return `${kind} · ${base}${fw}`;
    }
    return `Python · ${base}${fw}`;
  }
  if (target.mode === 'go' || target.mode === 'go-test') {
    const base = path?.split('/').pop()?.replace(/\.go$/, '') || 'main';
    if (target.mode === 'go-test') {
      return `Go test · ${base}${fw}`;
    }
    return `Go · ${base}${fw}`;
  }
  if (target.mode === 'rust' || target.mode === 'rust-test') {
    const base = path?.split('/').pop()?.replace(/\.rs$/, '') || 'main';
    if (target.mode === 'rust-test') {
      return `Rust test · ${base}${fw}`;
    }
    return `Rust · ${base}${fw}`;
  }
  if (target.mode === 'js' || target.mode === 'js-test') {
    const base = path?.split('/').pop()?.replace(/\.(m|c)?[jt]sx?$/i, '') || 'script';
    const lang = target.frameworks?.includes('typescript') ? 'TypeScript' : 'JavaScript';
    if (target.mode === 'js-test') return `${lang} test · ${base}${fw}`;
    return `${lang} · ${base}${fw}`;
  }
  if (target.mode === 'kotlin' || target.mode === 'kotlin-test') {
    const base = path?.split('/').pop()?.replace(/\.kts?$/i, '') || 'script';
    if (target.mode === 'kotlin-test') return `Kotlin test · ${base}${fw}`;
    return `Kotlin · ${base}${fw}`;
  }
  if (target.mode === 'php' || target.mode === 'php-test') {
    const base = path?.split('/').pop()?.replace(/\.php$/i, '') || 'script';
    if (target.mode === 'php-test') return `PHPUnit · ${base}${fw}`;
    return `PHP · ${base}${fw}`;
  }
  if (target.mode === 'dart' || target.mode === 'dart-test') {
    const base = path?.split('/').pop()?.replace(/\.dart$/i, '') || 'script';
    if (target.mode === 'dart-test') return `Dart test · ${base}${fw}`;
    return `Dart · ${base}${fw}`;
  }
  if (target.mode === 'native' || target.mode === 'native-test') {
    const base = path?.split('/').pop()?.replace(/\.(c|cpp|cc|cxx)$/i, '') || 'program';
    if (target.mode === 'native-test') {
      const kind = target.classType === 'gtest' ? 'Google Test'
        : target.classType === 'catch2' ? 'Catch2'
          : target.classType === 'make-test' ? 'Make test'
            : target.classType === 'meson-test' ? 'Meson test'
              : 'CTest';
      return `${kind} · ${base}${fw}`;
    }
    const lang = target.classType === 'cmake-run' ? 'CMake'
      : target.classType === 'cpp-program' ? 'C++' : 'C';
    return `${lang} · ${base}${fw}`;
  }
  if (target.mode === 'sql') {
    const base = path?.split('/').pop()?.replace(/\.sql$/i, '') || 'query';
    return `SQL · ${base}${fw}`;
  }
  if (target.mode === 'shell') {
    const base = path?.split('/').pop()?.replace(/\.(sh|bash|zsh)$/i, '') || 'script';
    return `Shell · ${base}${fw}`;
  }
  if (target.mode === 'project-task' && state.runInfo) {
    const tool = state.runInfo.build_tool === 'maven' ? 'Maven' : 'Gradle';
    return state.runInfo.is_spring_boot
      ? `Spring Boot · ${state.runInfo.project_root}`
      : `${tool} · ${state.runInfo.project_root}`;
  }
  if (target.classType && target.classType !== 'library') {
    return `${target.classType.replace(/-/g, ' ')}${fw}`;
  }
  return '';
}

function runTargetTitle(target) {
  let base = '';
  if (target.mode === 'test') {
    base = `Run tests: ${target.filter} (F5)`;
  } else if (target.mode === 'spring-boot') {
    base = `Run Spring Boot application (F5)`;
  } else if (target.mode === 'main') {
    base = `Run ${target.qualifiedName} (F5)`;
  } else if (target.mode === 'ruby') {
    base = `Run Ruby script (F5)`;
  } else if (target.mode === 'ruby-test') {
    base = target.classType === 'rails-test'
      ? `Run Rails test (F5)`
      : `Run RSpec (F5)`;
  } else if (target.mode === 'python') {
    base = `Run Python script (F5)`;
  } else if (target.mode === 'python-test') {
    base = target.classType === 'django-test'
      ? `Run Django test (F5)`
      : `Run pytest (F5)`;
  } else if (target.mode === 'go') {
    base = `Run Go program (F5)`;
  } else if (target.mode === 'go-test') {
    base = `Run Go tests (F5)`;
  } else if (target.mode === 'rust') {
    base = `Run Rust program (F5)`;
  } else if (target.mode === 'rust-test') {
    base = `Run Rust tests (F5)`;
  } else if (target.mode === 'js') {
    base = target.frameworks?.includes('typescript') ? `Run TypeScript file (F5)` : `Run JavaScript file (F5)`;
  } else if (target.mode === 'js-test') {
    base = target.frameworks?.includes('typescript') ? `Run TypeScript tests (F5)` : `Run JavaScript tests (F5)`;
  } else if (target.mode === 'kotlin') {
    base = `Run Kotlin (F5)`;
  } else if (target.mode === 'kotlin-test') {
    base = `Run Kotlin tests (F5)`;
  } else if (target.mode === 'php') {
    base = `Run PHP script (F5)`;
  } else if (target.mode === 'php-test') {
    base = `Run PHPUnit tests (F5)`;
  } else if (target.mode === 'dart') {
    base = `Run Dart program (F5)`;
  } else if (target.mode === 'dart-test') {
    base = `Run Dart tests (F5)`;
  } else if (target.mode === 'native') {
    base = `Compile and run (F5)`;
  } else if (target.mode === 'native-test') {
    base = target.classType === 'cmake-test' || target.classType === 'make-test' || target.classType === 'meson-test'
      ? `Run project tests (F5)`
      : `Run C/C++ tests (F5)`;
  } else if (target.mode === 'sql') {
    base = `Run SQL script (F5)`;
  } else if (target.mode === 'project-task' && state.runInfo) {
    const task = $('#gradle-task')?.value || state.runInfo.default_task;
    const tool = state.runInfo.build_tool === 'maven' ? 'Maven' : 'Gradle';
    base = `Run ${tool} '${task}' (F5)`;
  } else if (target.classType) {
    base = `${target.classType.replace(/-/g, ' ')} (F5)`;
  } else {
    base = 'Run (F5)';
  }
  if (target.reason && !target.runnable) {
    return `${base} — ${target.reason}`;
  }
  if (target.missing?.length) {
    return `${base} — missing: ${target.missing.join(', ')}`;
  }
  if (target.aiAssisted) {
    return `${base} (AI)`;
  }
  return base;
}

function isShellScriptPath(path) {
  if (!path) return false;
  const lower = path.toLowerCase();
  return lower.endsWith('.sh') || lower.endsWith('.bash') || lower.endsWith('.zsh');
}

function isPhpTestPath(path) {
  if (!path) return false;
  const norm = path.replace(/\\/g, '/').toLowerCase();
  const base = norm.split('/').pop() || '';
  return base.includes('test') || norm.includes('/test/') || norm.includes('/tests/');
}

function isDartTestPath(path) {
  if (!path) return false;
  const norm = path.replace(/\\/g, '/').toLowerCase();
  const base = norm.split('/').pop() || '';
  return base.endsWith('_test.dart') || norm.includes('/test/') || norm.includes('/tests/');
}

function isScalaSourcePath(path) {
  return path?.toLowerCase().endsWith('.scala') || false;
}

function isClojureSourcePath(path) {
  if (!path) return false;
  const lower = path.toLowerCase();
  return lower.endsWith('.clj') || lower.endsWith('.cljs') || lower.endsWith('.cljc');
}

function isRunToolbarPath(path) {
  if (!path) return false;
  if (isGradleFilePath(path) || isMavenFilePath(path)) return true;
  if (path.endsWith('.rb')) return true;
  if (path.endsWith('.py') || path.endsWith('.pyw')) return true;
  if (path.endsWith('.go')) return true;
  if (path.endsWith('.rs')) return true;
  if (isJsOrTsSourcePath(path)) return true;
  if (path.endsWith('.kt') || path.endsWith('.kts')) return true;
  if (path.endsWith('.php')) return true;
  if (path.endsWith('.dart')) return true;
  if (path.endsWith('.sql')) return true;
  if (isShellScriptPath(path)) return true;
  if (isNativeSourcePath(path)) return true;
  if (path.endsWith('.java') && state.runInfo?.has_project) return true;
  return false;
}

function detectShellRunTargetFallback(path) {
  const target = state.serverRunTarget;
  if (target?.mode === 'shell') {
    return serverRunTargetToClient(target);
  }
  return {
    mode: 'shell',
    classType: 'shell-script',
    frameworks: ['shell'],
    runnable: !!target?.runnable,
    reason: target?.reason,
    task: target?.task,
  };
}

function detectSqlRunTargetFallback(path) {
  const conn = state.dbConnection;
  if (conn?.connected && (conn?.kind === 'postgres' || conn?.kind === 'mysql' || conn?.kind === 'sqlite')) {
    return {
      mode: 'sql',
      classType: 'sql-script',
      frameworks: ['sql'],
      runnable: true,
    };
  }
  return {
    mode: 'sql',
    classType: 'sql-script',
    frameworks: ['sql'],
    runnable: false,
    reason: conn?.error || 'Connect to a database (PostgreSQL, MySQL, docker-compose, DATABASE_URL in .env, or Database panel)',
  };
}

function detectSpringBootApp(content) {
  if (!content || !/@SpringBootApplication\b/.test(content)) return null;
  const cls = content.match(/(?:public\s+)?class\s+(\w+)/)?.[1];
  if (!cls) return null;
  const pkg = content.match(/^\s*package\s+([\w.]+)\s*;/m)?.[1] || null;
  return { className: cls, package: pkg, qualifiedName: pkg ? `${pkg}.${cls}` : cls };
}

function serverRunTargetToClient(t) {
  if (!t) return { mode: 'none', runnable: false };
  return {
    mode: t.mode || 'none',
    filter: t.test_filter,
    task: t.task,
    qualifiedName: t.qualified_name,
    classType: t.class_type,
    frameworks: t.frameworks || [],
    missing: t.missing || [],
    reason: t.reason,
    aiAssisted: t.ai_assisted,
    runnable: !!t.runnable,
  };
}

function resolveRunTarget(path, content, info, cursorLine) {
  path = stripJavaDiagOverlayPath(path);
  if (path?.endsWith('.sql')) {
    if (state.serverRunTarget?.mode === 'sql') {
      return serverRunTargetToClient(state.serverRunTarget);
    }
    return detectSqlRunTargetFallback(path);
  }
  if (state.serverRunTarget) {
    const target = serverRunTargetToClient(state.serverRunTarget);
    if (target.mode === 'test' && cursorLine) {
      target.filter = testFilterForJavaFile(path, content, cursorLine) || target.filter;
    }
    if (target.mode === 'main' || (target.classType === 'spring-boot-app' && target.mode !== 'spring-boot')) {
      const springTarget = springBootRunTarget(content, info);
      if (springTarget) return { ...target, ...springTarget, classType: target.classType || 'spring-boot-app' };
    }
    return target;
  }
  return detectJavaRunTarget(path, content, info, cursorLine);
}

async function refreshRunInfo() {
  if (!state.repo || !state.activeTab) {
    state.runInfo = null;
    state.gradleInfo = null;
    state.serverRunTarget = null;
    updateRunButtons();
    return;
  }
  const content = state.tabContents.get(state.activeTab) ?? state.editor?.getValue?.() ?? '';
  const line = state.editor?.getPosition?.()?.lineNumber || 1;
  try {
    const ctx = await api(repoApi(state.repo, '/workspace/run/target'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        path: stripJavaDiagOverlayPath(state.activeTab),
        content,
        line,
      }),
    });
    state.runInfo = ctx?.has_project ? ctx : (ctx?.target ? ctx : null);
    state.serverRunTarget = ctx?.target || null;
    if (!ctx?.has_project && ctx) {
      state.runInfo = ctx;
    }
    state.gradleInfo = ctx?.build_tool === 'gradle' && ctx?.has_project
      ? {
        is_gradle: true,
        project_root: ctx.project_root,
        has_wrapper: ctx.has_wrapper,
        default_task: ctx.default_task,
        application_main: ctx.application_main,
        tasks: ctx.tasks,
        is_spring_boot: ctx.is_spring_boot,
      }
      : null;
  } catch {
    try {
      const info = await api(
        `${repoApi(state.repo, '/workspace/run/info')}?path=${encodeURIComponent(stripJavaDiagOverlayPath(state.activeTab))}`,
      );
      state.runInfo = info?.has_project ? info : null;
      state.serverRunTarget = null;
      state.gradleInfo = info?.build_tool === 'gradle' && info?.has_project
        ? {
          is_gradle: true,
          project_root: info.project_root,
          has_wrapper: info.has_wrapper,
          default_task: info.default_task,
          application_main: info.application_main,
          tasks: info.tasks,
          is_spring_boot: info.is_spring_boot,
        }
        : null;
    } catch {
      state.runInfo = null;
      state.serverRunTarget = null;
      state.gradleInfo = null;
    }
  }
  updateRunButtons();
  if (state.activeTab?.endsWith('.java') || isNativeSourcePath(state.activeTab)) applyTestRunDecorations();
}

async function refreshGradleInfo() {
  await refreshRunInfo();
}

async function refreshGradleInfo() {
  await refreshRunInfo();
}

function updateRunButtons() {
  const tbRun = $('#tb-run');
  const taskSel = $('#gradle-task');
  const runLabel = $('#toolbar-run-label');
  const gradleSep = $('#gradle-toolbar-sep');
  const info = state.runInfo;
  state.javaRunTarget = null;
  state.runTarget = { mode: 'none' };

  const path = state.activeTab;
  const content = path
    ? (state.tabContents.get(path) ?? state.editor?.getValue() ?? '')
    : '';
  const cursorLine = state.editor?.getPosition?.()?.lineNumber;
  const target = resolveRunTarget(path, content, info, cursorLine);
  state.runTarget = target;

  const showTaskPicker = target.mode === 'project-task' && info?.has_project;
  const showJavaRun = path?.endsWith('.java') && target.mode !== 'none';
  const showRubyRun = path?.endsWith('.rb') && (target.mode === 'ruby' || target.mode === 'ruby-test');
  const showPythonRun = (path?.endsWith('.py') || path?.endsWith('.pyw'))
    && (target.mode === 'python' || target.mode === 'python-test');
  const showGoRun = path?.endsWith('.go') && (target.mode === 'go' || target.mode === 'go-test');
  const showRustRun = path?.endsWith('.rs') && (target.mode === 'rust' || target.mode === 'rust-test');
  const showJsRun = isJsOrTsSourcePath(path) && (target.mode === 'js' || target.mode === 'js-test');
  const showKotlinRun = (path?.endsWith('.kt') || path?.endsWith('.kts')) && (target.mode === 'kotlin' || target.mode === 'kotlin-test');
  const showPhpRun = path?.endsWith('.php') && (target.mode === 'php' || target.mode === 'php-test');
  const showDartRun = path?.endsWith('.dart') && (target.mode === 'dart' || target.mode === 'dart-test');
  const showShellRun = isShellScriptPath(path) && target.mode === 'shell';
  const showNativeRun = isNativeSourcePath(path)
    && (target.mode === 'native' || target.mode === 'native-test');
  const showSqlRun = path?.endsWith('.sql');
  const showRunToolbar = showTaskPicker || showJavaRun || showRubyRun || showPythonRun || showGoRun || showRustRun || showJsRun || showKotlinRun || showPhpRun || showDartRun || showShellRun || showNativeRun || showSqlRun;
  const canRun = target.runnable || showTaskPicker;

  if (showRunToolbar && canRun) {
    if (showTaskPicker) {
      taskSel?.classList.remove('hidden');
      if (taskSel && info.tasks?.length) {
        const current = taskSel.value;
        taskSel.innerHTML = info.tasks.map((t) => `<option value="${t}">${t}</option>`).join('');
        taskSel.value = info.tasks.includes(current) ? current : info.default_task;
      }
    } else {
      taskSel?.classList.add('hidden');
    }

    if (tbRun) {
      tbRun.disabled = false;
      tbRun.title = runTargetTitle(target);
    }
    if (runLabel) {
      runLabel.classList.remove('hidden');
      runLabel.textContent = runTargetLabel(target, content, path);
    }
    gradleSep?.classList.remove('hidden');

    if (target.mode === 'main') {
      state.javaRunTarget = target.qualifiedName;
    }
  } else if ((showJavaRun || showRubyRun || showPythonRun || showGoRun || showNativeRun || showSqlRun) && target.reason) {
    taskSel?.classList.add('hidden');
    runLabel?.classList.remove('hidden');
    runLabel.textContent = runTargetLabel(target, content, path) || 'Not runnable';
    gradleSep?.classList.remove('hidden');
    if (tbRun) {
      tbRun.disabled = true;
      tbRun.title = runTargetTitle(target);
    }
  } else {
    taskSel?.classList.add('hidden');
    runLabel?.classList.add('hidden');
    gradleSep?.classList.add('hidden');
    if (tbRun) {
      tbRun.disabled = true;
      tbRun.title = 'Run (F5)';
    }
  }

  const tbFormat = $('#tb-format');
  const tbSave = $('#tb-save');
  if (tbFormat) tbFormat.disabled = !state.activeTab || isExternalEditorPath(state.activeTab);
  if (tbSave) tbSave.disabled = !state.activeTab || !state.dirty.has(state.activeTab) || isExternalEditorPath(state.activeTab);
  updateRollbackButton();
  updateProjectReloadButton();
  syncCoverageInlineButton();
  void refreshDebugCapabilities();
}

function primaryJavaNavTarget(content) {
  if (!content) return { line: 1, column: 1 };
  const lines = content.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const m = line.match(/(?:public\s+|protected\s+|private\s+)?(?:abstract\s+|static\s+|final\s+)*?(?:class|interface|enum|@interface)\s+(\w+)/);
    if (m) {
      const col = line.indexOf(m[1]) + 1;
      return { line: i + 1, column: col > 0 ? col : 1 };
    }
  }
  return { line: 1, column: 1 };
}

function navigateToPrimarySource(path, { force = false } = {}) {
  if (!state.editor || !path?.endsWith('.java')) return;
  if (!force && path !== state.activeTab) return;
  const content = state.tabContents.get(path) ?? state.editor.getModel()?.getValue?.() ?? '';
  const { line, column } = primaryJavaNavTarget(content);
  if (line <= 1 && column <= 1) return;
  state.editor.revealLineInCenter(line);
  state.editor.setPosition({ lineNumber: line, column });
}

async function openFileAt(path, line = 1, column = 1) {
  path = workspaceExplorerPath(path);
  const activePath = workspaceExplorerPath(state.activeTab);
  if (activePath !== path) {
    rememberTreeAnchorCursor();
    if (state.tabs.some((t) => workspaceExplorerPath(t) === path)) {
      const tab = state.tabs.find((t) => workspaceExplorerPath(t) === path);
      activateTab(tab);
    } else {
      await openFile(path, { skipPrimaryNav: true });
    }
  } else {
    activateTab(state.activeTab);
  }
  if (!state.editor) return;
  state.editor.revealLineInCenter(line);
  state.editor.setPosition({ lineNumber: line, column });
  state.editor.focus();
  updateTreeBackButton();
}

function runEditorMonacoAction(actionId) {
  if (!state.editor) {
    toast('Open a file first', 'info');
    return;
  }
  const action = state.editor.getAction(actionId);
  if (!action) {
    toast('Command not available for this file', 'info');
    return;
  }
  void action.run();
}

async function formatDocument() {
  if (!state.editor || !state.activeTab) return;
  if (isExternalEditorPath(state.activeTab)) {
    toast('External files are read-only', 'info');
    return;
  }
  if (!state.repo) {
    toast('Open a repository first', 'error');
    return;
  }
  const path = state.activeTab;
  const before = state.editor.getValue();
  try {
    const res = await api(repoApi(state.repo, '/workspace/format'), {
      method: 'POST',
      body: JSON.stringify({ path, content: before }),
    });
    const formatted = res?.content;
    if (typeof formatted !== 'string') {
      toast('Format failed', 'error');
      return;
    }
    if (formatted === before) {
      toast('Already formatted', 'info');
      return;
    }
    state.suppressEditorChange = true;
    state.editor.setValue(formatted);
    state.suppressEditorChange = false;
    state.tabContents.set(path, formatted);
    state.dirty.add(path);
    updateSaveButton();
    renderTabs();
    scheduleDiagnostics();
    toast('Formatted', 'success');
  } catch (e) {
    toast(e.message || 'Format failed', 'error');
  }
}

function closeAllTabs() {
  state.tabs = [];
  state.tabContents.clear();
  state.activeTab = null;
  state.dirty.clear();
  if (state.editor) {
    state.suppressEditorChange = true;
    state.editor.setValue('');
    monaco.editor.setModelLanguage(state.editor.getModel(), 'plaintext');
    state.suppressEditorChange = false;
  }
  const list = $('#tab-list');
  if (list) list.innerHTML = '';
  let empty = $('#empty-state');
  if (!empty) {
    empty = document.createElement('div');
    empty.id = 'empty-state';
    $('#editor-stage')?.insertBefore(empty, $('#editor-container'));
  }
  renderWelcome();
  $('#empty-state')?.classList.remove('hidden');
  setMainView('editor');
  syncWelcomeLayout();
  $('#editor-toolbar')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.remove('flex');
  updateBreadcrumbs(null);
  updateRunButtons();
  updateMenuState();
}

function closeWorkspaceTabs() {
  state.treeNavAnchor = null;
  updateTreeBackButton();
  state.conflictFiles = new Set();
  state.conflictPanelHidden = false;
  state.runInfo = null;
  state.gradleInfo = null;
  state.javaRunTarget = null;
  clearDiagnostics();
  void stopDockerLogsStream();
  closeAllTabs();
}

async function readTabFromDisk(path) {
  const q = new URLSearchParams({ path, _t: String(Date.now()) });
  const body = await api(repoApi(state.repo, `/workspace/file?${q}`), { allowDuringSave: true });
  return body?.content ?? '';
}

function editorContentForPath(path) {
  if (state.activeTab === path && state.editor) return state.editor.getValue();
  return state.tabContents.get(path);
}

async function writeTabToDiskOnce(path, content, { attempt = 0 } = {}) {
  if (!state.repo) throw new Error('No repository open — select a repo before saving');
  if (!path) throw new Error('No file path to save');
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), SAVE_FETCH_TIMEOUT_MS);
  try {
    await api(repoApi(state.repo, '/workspace/file'), {
      method: 'PUT',
      body: JSON.stringify({ path, content }),
      signal: controller.signal,
      allowDuringSave: true,
    });
    if (!usesInProcessApi()) {
      const disk = await readTabFromDisk(path);
      if (disk !== content) {
        throw new Error('Save did not persist to disk');
      }
    }
  } catch (err) {
    if (isDiagFetchAbort(err) && attempt < SAVE_MAX_RETRIES) {
      await new Promise((r) => setTimeout(r, 50 * (attempt + 1)));
      return writeTabToDiskOnce(path, content, { attempt: attempt + 1 });
    }
    throw err;
  } finally {
    clearTimeout(timeoutId);
  }
  state.tabContents.set(path, content);
  const current = editorContentForPath(path);
  if (current === content) {
    state.dirty.delete(path);
  } else {
    state.dirty.add(path);
  }
  updateSaveButton();
  renderTabs();
  return state.dirty.has(path);
}

async function flushSaveWriteCoalesce(path, entry, attempt) {
  entry.running = true;
  try {
    let lastStillDirty = false;
    while (entry.pending !== null) {
      const batch = entry.pending;
      entry.pending = null;
      lastStillDirty = await writeTabToDiskOnce(path, batch, { attempt });
    }
    for (const w of entry.waiters) w.resolve(lastStillDirty);
    entry.waiters.length = 0;
    return lastStillDirty;
  } catch (err) {
    for (const w of entry.waiters) w.reject(err);
    entry.waiters.length = 0;
    throw err;
  } finally {
    entry.running = false;
    if (entry.pending !== null) {
      void flushSaveWriteCoalesce(path, entry, attempt);
    } else if (entry.waiters.length === 0) {
      saveWriteCoalesceByPath.delete(path);
    }
  }
}

async function writeTabToDisk(path, content, { attempt = 0 } = {}) {
  let entry = saveWriteCoalesceByPath.get(path);
  if (!entry) {
    entry = { pending: null, running: false, waiters: [] };
    saveWriteCoalesceByPath.set(path, entry);
  }
  entry.pending = content;
  if (entry.running) {
    return new Promise((resolve, reject) => {
      entry.waiters.push({ resolve, reject });
    });
  }
  return flushSaveWriteCoalesce(path, entry, attempt);
}

async function autoSaveToDisk() {
  if (!getAutoSaveEnabled() || !state.repo || !state.activeTab || !state.editor) return;
  if (state.activeTab.startsWith('.reaper/')) return;
  if (isExternalEditorPath(state.activeTab)) return;
  const path = state.activeTab;
  if (!state.dirty.has(path)) return;
  if (state.autoSaveInFlight) return;
  const content = editorContentForPath(path);
  if (shouldDeferAutoSave(content, path)) {
    scheduleAutoSave(AUTO_SAVE_INCOMPLETE_RETRY_MS);
    return;
  }

  state.autoSaveInFlight = true;
  state.saveInFlight = true;
  let saveContext = null;
  let persisted = false;
  try {
    saveContext = await prepareForSave();
    showSavingFooterStatus();
    await writeTabToDisk(path, content);
    persisted = true;
  } catch (err) {
    console.warn('[Reaper] auto-save failed', err);
    showSaveFooterStatus('Auto-save failed', { error: true, auto: true });
  } finally {
    state.autoSaveInFlight = false;
    state.saveInFlight = false;
    if (!saveContext?.lightweight) leaveSaveGate();
    if (saveContext?.wasPolling && state.repo) {
      startProjectIndexPolling();
    }
  }

  // Disk first — javac and git only after verified persist.
  if (!persisted) return;
  if (path.endsWith('.java')) {
    window.ReaperLang?.clearCompletionCache?.();
    clearSaveFooterStatus();
    const el = $('#status-message');
    if (el) {
      el.textContent = 'Saved';
      el.classList.add('is-save-hint', 'is-save-hint-auto');
    }
    clearTimeout(javaFullDiagTimer);
    javaFullDiagTimer = null;
    javaFullCompilePending = null;
    queueJavaDiagnostics(path, content, { scope: 'typing', immediate: true, force: false });
  } else {
    showSaveFooterStatus('Saved', { auto: true });
  }
  void refreshGitStatus();
}

async function saveFile(options = {}) {
  const { silent = false, skipProjectReload = false } = options;
  if (!state.activeTab || !state.editor) return;
  if (isExternalEditorPath(state.activeTab)) {
    if (!silent) toast('External files are read-only', 'info');
    return;
  }
  const savedPath = state.activeTab;
  const content = state.editor.getValue();
  let saveContext = null;
  state.saveInFlight = true;
  try {
    saveContext = await prepareForSave();
    showSavingFooterStatus();
    const stillDirty = await writeTabToDisk(savedPath, content);
    if (stillDirty) {
      if (!silent) showSaveFooterStatus('Saved · pending edits', { ms: 2500 });
    } else if (!silent) {
      showSaveFooterStatus('Saved');
    }
    if (savedPath.endsWith('.java')) {
      window.ReaperLang?.clearCompletionCache?.();
      clearTimeout(javaFullDiagTimer);
      javaFullDiagTimer = null;
      if (silent) {
        scheduleJavaFullDiagnostics();
      } else {
        javaFullCompilePending = null;
        queueJavaFullDiagnostics(savedPath, content, {
          immediate: true,
          force: true,
        });
      }
    }
    if (!silent) {
      void refreshTree();
      void refreshGitStatus();
    }
    if (!skipProjectReload && isProjectClasspathFile(savedPath) && hasAutoReloadProject()) {
      scheduleProjectReload(0);
    }
    if (isDockerComposeFile(savedPath)) {
      scheduleBuildTasksRefresh({ fromDisk: true });
      scheduleDbViewerRefresh();
    } else if (savedPath.endsWith('.env')) {
      scheduleDbViewerRefresh();
    }
  } catch (err) {
    clearTimeout(javaFullDiagTimer);
    javaFullDiagTimer = null;
    javaFullCompilePending = null;
    ++javaFullDiagSeq;
    if (savedPath.endsWith('.java') && state.activeTab === savedPath) {
      clearDiagnostics();
    }
    showSaveFooterStatus('Save failed', { error: true });
    if (!silent) {
      const msg = isDiagFetchAbort(err)
        ? 'Save timed out — try ⌘S again'
        : (err.message || 'Failed to save');
      toast(msg, 'error');
    }
  } finally {
    state.saveInFlight = false;
    if (!saveContext?.lightweight) leaveSaveGate();
    if (saveContext?.wasPolling && state.repo) {
      startProjectIndexPolling();
    }
  }
}

function lineLooksIncompleteForAutoSave(line) {
  const trimmed = line.trimEnd();
  if (!trimmed || trimmed.startsWith('//') || trimmed.startsWith('*')) return false;
  if (/=\s*$/.test(trimmed)) return true;
  if (/\.\s*$/.test(trimmed)) return true;
  if (/,\s*$/.test(trimmed)) return true;
  if (/(\|\||&&|\?|:|\+\+|--)\s*$/.test(trimmed)) return true;
  if (/\(\s*$/.test(trimmed) && !/\)/.test(trimmed)) return true;
  return false;
}

function shouldDeferAutoSave(content, path) {
  if (!content || !path || !AUTO_SAVE_CODE_EXTS.has(path.split('.').pop()?.toLowerCase() ?? '')) {
    return false;
  }
  return content.split('\n').some((line) => lineLooksIncompleteForAutoSave(line));
}

function autoSaveDelayMsForPath(path) {
  return path?.endsWith('.java') ? JAVA_AUTO_SAVE_DELAY_MS : AUTO_SAVE_DELAY_MS;
}

function scheduleAutoSave(retryMs) {
  if (!getAutoSaveEnabled() || !state.repo || !state.activeTab) return;
  if (state.activeTab.startsWith('.reaper/')) return;
  if (isExternalEditorPath(state.activeTab)) return;
  if (!state.dirty.has(state.activeTab)) return;
  const path = state.activeTab;
  clearTimeout(state.autoSaveTimer);
  state.autoSaveTimer = setTimeout(async () => {
    if (!state.dirty.has(path) || state.activeTab !== path) return;
    const content = editorContentForPath(path);
    if (shouldDeferAutoSave(content, path)) {
      scheduleAutoSave(AUTO_SAVE_INCOMPLETE_RETRY_MS);
      return;
    }
    await autoSaveToDisk();
  }, retryMs ?? autoSaveDelayMsForPath(path));
}

async function createFile(e) {
  e.preventDefault();
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  const path = new FormData(e.target).get('path').trim().replace(/\\/g, '/');
  if (!path || path.includes('..')) {
    toast('Invalid file path', 'error');
    return;
  }
  try {
    const content = defaultReadmeContent(path);
    await api(repoApi(state.repo, '/workspace/file'), {
      method: 'POST',
      body: JSON.stringify({ path, content }),
    });
    hideFileModal();
    e.target.reset();
    await refreshTree();
    if (!state.tabs.includes(path)) {
      state.tabContents.set(path, content);
      state.tabs.push(path);
      renderTabs();
    }
    activateTab(path);
    toast(`Created ${path}`, 'success');
  } catch (err) {
    toast(err.message || 'Failed to create file', 'error');
  }
}

function showFileModal(initialPath = '') {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  $('#file-modal-overlay').classList.remove('hidden');
  $('#file-modal-overlay').classList.add('flex');
  const input = $('#new-file-form input[name="path"]');
  if (input) {
    input.value = initialPath ? String(initialPath).replace(/\\/g, '/') : '';
    setTimeout(() => input.focus(), 50);
  }
}

function hideFileModal() {
  $('#file-modal-overlay').classList.add('hidden');
  $('#file-modal-overlay').classList.remove('flex');
}

function updateRollbackButton() {
  const btn = $('#tb-rollback');
  if (!btn) return;
  const model = state.editor?.getModel?.();
  const canUndo = !!(model && !model.isDisposed() && model.canUndo());
  btn.disabled = !state.activeTab || !canUndo;
}

function rollbackLastChange() {
  if (!state.editor || !state.activeTab) return;
  const model = state.editor.getModel();
  if (!model?.canUndo()) {
    toast('Nothing to roll back', 'info');
    return;
  }
  state.editor.focus();
  state.editor.trigger('keyboard', 'undo', null);
  updateRollbackButton();
}

function updateSaveButton() {
  const dirty = !!(state.activeTab && state.dirty.has(state.activeTab) && !isExternalEditorPath(state.activeTab));
  const btn = $('#btn-save');
  if (btn) btn.disabled = !dirty;
  const tbSave = $('#tb-save');
  if (tbSave) tbSave.disabled = !dirty;
  updateRollbackButton();
  updateMenuState();
}

function detectJavaMain(content) {
  const normalized = content.replace(/\s+/g, '');
  if (!normalized.includes('staticvoidmain(')) return null;
  const pkg = content.match(/^\s*package\s+([\w.]+)\s*;/m)?.[1] || null;
  const cls = content.match(/public\s+class\s+(\w+)/)?.[1]
    || content.match(/\bclass\s+(\w+)/)?.[1];
  if (!cls) return null;
  return { className: cls, package: pkg, qualifiedName: pkg ? `${pkg}.${cls}` : cls };
}

function updateJavaRunButton() {
  updateRunButtons();
}

async function runProjectTask(taskOverride) {
  if (!state.repo || !state.activeTab || !state.runInfo?.has_project) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const info = state.runInfo;
  const task = taskOverride || $('#gradle-task')?.value || info.default_task;
  const toolLabel = info.build_tool === 'maven' ? 'mvn' : 'gradle';
  const where = info.project_root || state.activeTab || '.';
  const label = `${toolLabel} ${task}`;
  try {
    const { exitCode, output } = await runWorkspaceCommandStream(
      '/workspace/run/task',
      { path: stripJavaDiagOverlayPath(state.activeTab), task },
      { label, terminalId: term.id },
    );
    if (gradleClassfileVersionToast(output)) return;
  } catch (e) {
    terminalLog(`error: ${e.message}`);
    if (gradleClassfileVersionToast(e.message || '')) return;
  }
}

async function runGradle() {
  await runProjectTask();
}

async function runProjectTest(testFilter) {
  if (!state.repo || !state.activeTab || !testFilter) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  await refreshRunInfo();
  const info = state.runInfo;
  if (!info?.has_project) {
    toast('Not inside a Gradle or Maven project', 'error');
    return;
  }
  const term = getActiveTerminal();
  const filter = normalizeTestFilter(testFilter, info.build_tool);
  if (!filter) return;
  const task = info.build_tool === 'maven'
    ? `-Dtest=${filter} test`
    : `test --tests ${filter}`;
  const toolLabel = info.build_tool === 'maven' ? 'mvn' : 'gradle';
  const label = `${toolLabel} ${task}`;
  try {
    const { exitCode, output } = await runWorkspaceCommandStream(
      '/workspace/run/task',
      { path: stripJavaDiagOverlayPath(state.activeTab), task },
      { label, terminalId: term.id, kind: 'test' },
    );
    if (gradleClassfileVersionToast(output)) return;
    return exitCode;
  } catch (e) {
    terminalLog(`error: ${e.message}`);
    if (gradleClassfileVersionToast(e.message || '')) return;
    return -1;
  }
}

async function runProjectTestWithCoverage(testFilter) {
  if (!state.repo || !state.activeTab || !testFilter) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  await refreshRunInfo();
  const info = state.runInfo;
  if (!info?.has_project) {
    toast('Not inside a Gradle or Maven project', 'error');
    return;
  }
  const term = getActiveTerminal();
  const filter = normalizeTestFilter(testFilter, info.build_tool);
  if (!filter) return;
  const task = info.build_tool === 'maven'
    ? `-Dtest=${filter} test`
    : `test --tests ${filter}`;
  const compileTask = info.build_tool === 'maven'
    ? 'compile test-compile'
    : 'compileJava compileTestJava';
  const reportTask = info.build_tool === 'maven' ? 'jacoco:report' : 'jacocoTestReport';
  const toolLabel = info.build_tool === 'maven' ? 'mvn' : 'gradle';
  const label = `◔ ${toolLabel} ${compileTask} ${task} ${reportTask}  (${state.activeTab})`;
  const path = state.activeTab;
  try {
    const { exitCode, output } = await runWorkspaceCommandStream(
      '/workspace/run/task',
      { path: stripJavaDiagOverlayPath(path), task, coverage: true },
      { label, terminalId: term.id, kind: 'test' },
    );
    if (gradleClassfileVersionToast(output)) return;
    if (exitCode === 0) {
      const cov = await fetchAndApplyCoverage(path);
      await refreshCoveragePanel(path);
      if (cov?.coverage_path && cov.coverage_path !== path) {
        await openFileAt(cov.coverage_path);
        applyCoverageDecorations(cov.coverage_path, cov);
        updateCoverageStatus(cov);
      }
      showCoveragePanel();
    } else {
      toast('Tests failed — coverage not updated', 'warning');
    }
    return exitCode;
  } catch (e) {
    terminalLog(`error: ${e.message}`);
    if (gradleClassfileVersionToast(e.message || '')) return;
    return -1;
  }
}

function normalizeTestFilter(testFilter, buildTool) {
  let filter = String(testFilter || '')
    .replace(/\s*\([^)]*\.java\)\s*$/, '')
    .replace(/\//g, '.')
    .trim();
  if (!filter) return '';
  if (buildTool === 'maven') return mavenSurefireTestFilter(filter);
  return filter;
}

/** Gradle uses fqcn.method; Maven Surefire uses fqcn#method. */
function mavenSurefireTestFilter(filter) {
  const lastDot = filter.lastIndexOf('.');
  if (lastDot <= 0) return filter;
  const method = filter.slice(lastDot + 1);
  if (method && /^[a-z]/.test(method)) {
    return `${filter.slice(0, lastDot)}#${method}`;
  }
  return filter;
}

function normalizeGradleTestFilter(testFilter) {
  return normalizeTestFilter(testFilter, 'gradle');
}

async function runGradleTest(testFilter) {
  await runProjectTest(testFilter);
}

async function runSpringBootApplication(qualifiedName) {
  if (!state.repo || !state.activeTab || !qualifiedName) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  await refreshRunInfo();
  const info = state.runInfo;
  if (!info?.has_project) {
    toast('Not inside a Gradle or Maven project', 'error');
    return;
  }
  const task = info.build_tool === 'maven'
    ? `spring-boot:run -Dspring-boot.run.mainClass=${qualifiedName}`
    : `bootRun -Dspring-boot.run.main-class=${qualifiedName}`;
  await runProjectTask(task);
}

async function runActive() {
  if (!state.repo || !state.activeTab) return;
  await refreshRunInfo();
  const target = state.runTarget;
  if (!target || target.mode === 'none') {
    toast(target?.reason || 'Nothing to run for this file', 'info');
    return;
  }
  if (target.runnable === false && target.mode !== 'project-task') {
    if (target.reason) toast(target.reason, 'error');
    return;
  }

  switch (target.mode) {
    case 'test':
      await runProjectTest(target.filter);
      break;
    case 'spring-boot':
      await runProjectTask(target.task || state.runInfo?.default_task);
      break;
    case 'main':
      await runJavaMain(target.qualifiedName);
      break;
    case 'project-task':
      await runProjectTask();
      break;
    case 'ruby':
    case 'ruby-test':
      await runRubyFile();
      break;
    case 'python':
    case 'python-test':
      await runPythonFile();
      break;
    case 'go':
    case 'go-test':
      await runGoFile();
      break;
    case 'rust':
    case 'rust-test':
      await runRustFile();
      break;
    case 'js':
    case 'js-test':
      await runJsFile();
      break;
    case 'kotlin':
    case 'kotlin-test':
      await runKotlinFile();
      break;
    case 'php':
    case 'php-test':
      await runPhpFile();
      break;
    case 'dart':
    case 'dart-test':
      await runDartFile();
      break;
    case 'shell':
      await runShellFile();
      break;
    case 'native':
    case 'native-test':
      await runNativeFile();
      break;
    case 'sql':
      await runSqlFile();
      break;
    default:
      break;
  }
}
async function runRubyFile() {
  if (!state.repo || !state.activeTab?.endsWith('.rb')) return;
  const target = state.runTarget;
  const command = target?.task;
  if (!command) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runPythonFile() {
  if (!state.repo || !state.activeTab) return;
  const tab = state.activeTab;
  if (!tab.endsWith('.py') && !tab.endsWith('.pyw')) return;
  const target = state.runTarget;
  const command = target?.task;
  if (!command) return;
  if (state.dirty.has(tab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runGoFile() {
  if (!state.repo || !state.activeTab?.endsWith('.go')) return;
  const target = state.runTarget;
  const command = target?.task;
  if (!command) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runRustFile() {
  if (!state.repo || !state.activeTab?.endsWith('.rs')) return;
  const target = state.runTarget;
  const command = target?.task;
  if (!command) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

function isJsOrTsSourcePath(path) {
  if (!path) return false;
  const lower = path.toLowerCase();
  if (lower.endsWith('.d.ts')) return false;
  return lower.endsWith('.js') || lower.endsWith('.mjs') || lower.endsWith('.cjs')
    || lower.endsWith('.jsx') || lower.endsWith('.ts') || lower.endsWith('.tsx');
}

async function runJsFile() {
  if (!state.repo || !isJsOrTsSourcePath(state.activeTab)) return;
  const target = state.runTarget;
  const command = target?.task;
  if (!command) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runKotlinFile() {
  if (!state.repo || !state.activeTab) return;
  const tab = state.activeTab;
  if (!tab.endsWith('.kt') && !tab.endsWith('.kts')) return;
  const command = state.runTarget?.task;
  if (!command) return;
  if (state.dirty.has(tab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream('/workspace/shell', { command, cwd }, { label: command, terminalId: term.id });
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runPhpFile() {
  if (!state.repo || !state.activeTab?.endsWith('.php')) return;
  const command = state.runTarget?.task;
  if (!command) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream('/workspace/shell', { command, cwd }, { label: command, terminalId: term.id });
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runDartFile() {
  if (!state.repo || !state.activeTab?.endsWith('.dart')) return;
  const command = state.runTarget?.task;
  if (!command) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream('/workspace/shell', { command, cwd }, { label: command, terminalId: term.id });
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runShellFile() {
  if (!state.repo || !state.activeTab) return;
  const tab = state.activeTab;
  if (!isShellScriptPath(tab)) return;
  const command = state.runTarget?.task;
  if (!command) return;
  if (state.dirty.has(tab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream('/workspace/shell', { command, cwd }, { label: command, terminalId: term.id });
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runNativeFile() {
  if (!state.repo || !state.activeTab) return;
  const tab = state.activeTab;
  if (!isNativeSourcePath(tab)) return;
  const target = state.runTarget;
  const command = target?.task;
  if (!command) return;
  if (state.dirty.has(tab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const cwd = state.runInfo?.project_root || undefined;
  try {
    await runWorkspaceCommandStream(
      '/workspace/shell',
      { command, cwd },
      { label: command, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runSqlFile() {
  if (!state.repo || !state.activeTab?.endsWith('.sql')) return;
  const path = stripJavaDiagOverlayPath(state.activeTab);
  const target = state.runTarget;
  if (target && !target.runnable) {
    if (target.reason) toast(target.reason, 'error');
    return;
  }
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const content = state.editor?.getValue?.() ?? '';
  const base = path.split('/').pop() || path;
  const viaDocker = target?.task?.includes('docker compose');
  const label = viaDocker ? `docker compose exec psql · ${base}` : `psql · ${base}`;
  try {
    const { exitCode } = await runWorkspaceCommandStream(
      '/workspace/sql/run',
      { path, content },
      { label, terminalId: term.id },
    );
    if (exitCode === 0) {
      await maybeRefreshDbSchemaAfterSql(content, { ok: true });
    }
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLog(`error: ${e.message}`);
  }
}

async function runJavaMain(qualifiedName) {
  if (!state.repo || !state.activeTab?.endsWith('.java')) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const name = qualifiedName || state.javaRunTarget || state.activeTab;
  try {
    await runWorkspaceCommandStream(
      '/workspace/java/run',
      { path: stripJavaDiagOverlayPath(state.activeTab) },
      { label: `java ${name}`, terminalId: term.id },
    );
  } catch (e) {
    if (e?.name !== 'AbortError') terminalLogError(e.message, { label: 'run', show: false });
  }
}

// --- Git ---
function setAgentDiffLive(on) {
  $('#diff-agent-live')?.classList.toggle('hidden', !on);
  $('#diff-panel')?.classList.toggle('ij-diff-panel--live', on);
}

function showAgentDiffPlaceholder() {
  const label = $('#diff-path');
  if (label) label.textContent = 'Waiting for agent edits…';
  const content = $('#diff-content');
  if (content) {
    content.innerHTML = '<div class="ij-sbs-empty">Agent is working — file diffs will appear here as changes are made.</div>';
  }
  setMainView('diff');
  setAgentDiffLive(true);
  $('#empty-state')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.remove('hidden');
  $('#editor-toolbar')?.classList.add('flex');
}

async function showFileDiff(path, staged = false) {
  if (!state.repo || !path) return;
  if (state.conflictFiles.has(path)) {
    await openConflictFile(path);
    return;
  }
  let diff = await api(`${repoApi(state.repo, '/workspace/diff')}?path=${encodeURIComponent(path)}&staged=${staged}`);
  let text = (diff.diff || '').trim();
  if (!text && !staged) {
    diff = await api(`${repoApi(state.repo, '/workspace/diff')}?path=${encodeURIComponent(path)}&staged=true`);
    text = (diff.diff || '').trim();
  }
  showDiffInMainArea(path, text || '(no diff — new or binary file)');
  highlightGitStatusFile(path);
}

function highlightGitStatusFile(path) {
  $$('#git-status-list .ij-git-item').forEach((btn) => {
    btn.classList.toggle('selected', btn.dataset.statusPath === path);
  });
}

function formatActivityBadgeCount(n) {
  if (n > 99) return '99+';
  return String(n);
}

function gitStatusFileName(path) {
  const normalized = (path || '').replace(/\\/g, '/');
  const idx = normalized.lastIndexOf('/');
  return idx >= 0 ? normalized.slice(idx + 1) : normalized;
}

function groupStatusFilesByPath(files) {
  const map = new Map();
  for (const f of files || []) {
    const existing = map.get(f.path);
    if (!existing) {
      map.set(f.path, {
        path: f.path,
        status: f.status,
        staged: f.staged,
        conflict: f.status === 'conflict',
      });
      continue;
    }
    if (f.status === 'conflict') existing.conflict = true;
    existing.staged = existing.staged || f.staged;
    if (f.status === 'deleted' || f.status === 'conflict') existing.status = f.status;
  }
  return [...map.values()];
}

function committablePathsFromFiles(files) {
  return groupStatusFilesByPath(files)
    .filter((f) => !f.conflict)
    .map((f) => f.path);
}

function syncCommitSelection(files) {
  const paths = committablePathsFromFiles(files);
  for (const p of [...state.commitSelectedPaths]) {
    if (!paths.includes(p)) state.commitSelectedPaths.delete(p);
  }
  for (const p of paths) {
    if (!state.commitKnownPaths.has(p)) {
      state.commitSelectedPaths.add(p);
      state.commitKnownPaths.add(p);
    }
  }
  for (const p of [...state.commitKnownPaths]) {
    if (!paths.includes(p)) state.commitKnownPaths.delete(p);
  }
}

function getSelectedCommitPaths() {
  return [...state.commitSelectedPaths];
}

function setCommitPathSelected(path, selected) {
  if (!path) return;
  if (selected) state.commitSelectedPaths.add(path);
  else state.commitSelectedPaths.delete(path);
}

function updateCommitSelectionUi(files, { mergeBlocked = false } = {}) {
  const rows = groupStatusFilesByPath(files);
  const committable = rows.filter((f) => !f.conflict);
  const committablePaths = committable.map((f) => f.path);
  const selected = getSelectedCommitPaths().filter((p) => committablePaths.includes(p));
  const selectedCount = selected.length;

  const selectAll = $('#commit-select-all');
  if (selectAll) {
    selectAll.disabled = committablePaths.length === 0;
    selectAll.checked = committablePaths.length > 0 && selectedCount === committablePaths.length;
    selectAll.indeterminate = selectedCount > 0 && selectedCount < committablePaths.length;
  }

  const countEl = $('#commit-file-count');
  if (countEl) {
    if (!rows.length) {
      countEl.textContent = '';
      countEl.classList.add('hidden');
    } else {
      const total = rows.length;
      countEl.textContent = selectedCount === committablePaths.length && committablePaths.length === total
        ? `${total} file${total === 1 ? '' : 's'}`
        : `${selectedCount} of ${committablePaths.length} selected`;
      countEl.classList.remove('hidden');
    }
  }

  const commitDisabled = rows.length === 0 || mergeBlocked || selectedCount === 0 || state.commitBusy;
  const commitOnlyBtn = $('#btn-commit-only');
  const commitPushBtn = $('#btn-commit-push');
  const suggestBtn = $('#btn-suggest-commit');
  if (commitOnlyBtn) commitOnlyBtn.disabled = commitDisabled;
  if (commitPushBtn) commitPushBtn.disabled = commitDisabled;
  if (suggestBtn) suggestBtn.disabled = commitDisabled;
}

let lastRemoteFetchMs = 0;
const REMOTE_FETCH_INTERVAL_MS = 60_000;

async function maybeFetchRemotes() {
  if (!state.repo || !state.gitBackgroundFetch) return;
  const now = Date.now();
  if (now - lastRemoteFetchMs < REMOTE_FETCH_INTERVAL_MS) return;
  lastRemoteFetchMs = now;
  try {
    await api(repoApi(state.repo, '/workspace/fetch'), { method: 'POST' });
  } catch {
    /* no remote configured */
  }
}

async function refreshGitStatus() {
  if (!state.repo) {
    updateGitNavUi({ ahead: 0, behind: 0 });
    return { clean: true, files: [], branch: '', ahead: 0, behind: 0 };
  }
  await maybeFetchRemotes();
  const status = await api(repoApi(state.repo, '/workspace/status'));
  const displayFiles = filterStatusFilesForDisplay(status.files);
  state.conflictFiles = new Set(
    displayFiles.filter((f) => f.status === 'conflict').map((f) => f.path),
  );
  updateMergeBanner(status);
  const mergeBlocked = !!(status.merge?.active && (status.conflict_count || 0) > 0);
  state.mergeBlockedCommit = mergeBlocked;
  state.lastGitStatusFiles = displayFiles;
  syncCommitSelection(displayFiles);
  updateCommitSelectionUi(displayFiles, { mergeBlocked });
  updateGitNavUi({ ...status, files: displayFiles, clean: displayFiles.length === 0 });
  const branch = withoutMasterBranch(status.branch);
  if (branch) {
    setBranchLabel(branch);
  } else if (state.branches.includes('main')) {
    setBranchLabel('main');
  } else if (state.branches.length) {
    setBranchLabel(state.branches[0]);
  } else {
    setBranchLabel('');
  }
  const badge = $('#git-badge');
  const grouped = groupStatusFilesByPath(displayFiles);
  if (badge) {
    if (!displayFiles.length) {
      badge.classList.add('hidden');
    } else {
      const n = status.conflict_count || grouped.length;
      const label = formatActivityBadgeCount(n);
      badge.textContent = label;
      badge.classList.toggle('wide', label.length > 1);
      badge.title = `${n} changed file${n === 1 ? '' : 's'}`;
      badge.classList.remove('hidden');
      badge.classList.toggle('conflicts', (status.conflict_count || 0) > 0);
      badge.classList.toggle('modified', !(status.conflict_count || 0));
    }
  }
  const list = $('#git-status-list');
  if (!displayFiles.length) {
    list.innerHTML = '<div class="ij-empty-state"><div class="ij-empty-icon">✓</div><p>Nothing to commit — working tree clean</p></div>';
    updateCommitSelectionUi([], { mergeBlocked });
    updateStatusBar({ ...status, files: displayFiles, clean: true });
    updateConflictUi();
    return { clean: true, files: [], branch: status.branch, ahead: status.ahead || 0 };
  }
  list.innerHTML = grouped.map((f) => {
    const checked = !f.conflict && state.commitSelectedPaths.has(f.path);
    const disabled = f.conflict ? ' disabled' : '';
    const checkedAttr = checked ? ' checked' : '';
    const fileName = gitStatusFileName(f.path);
    return `
    <div class="ij-git-row${f.conflict ? ' conflict-item' : ''}">
      <label class="ij-git-check" title="${f.conflict ? 'Resolve conflicts before committing' : 'Include in commit'}">
        <input type="checkbox" class="ij-git-stage-check" data-path="${escapeHtml(f.path)}"${checkedAttr}${disabled}>
      </label>
      <button type="button" data-status-path="${escapeHtml(f.path)}" data-staged="${f.staged}" data-status="${f.status}" class="ij-git-item" title="${escapeHtml(f.path)} — ${escapeHtml(statusLabel(f.status))} — click to preview diff">
        <span class="ij-git-badge ${f.status}" title="${escapeHtml(statusLabel(f.status))}">${statusIcon(f.status)}</span>
        <span class="ij-git-path">${escapeHtml(fileName)}</span>
      </button>
    </div>`;
  }).join('');
  list.querySelectorAll('.ij-git-stage-check').forEach((input) => {
    input.addEventListener('change', () => {
      setCommitPathSelected(input.dataset.path, input.checked);
      updateCommitSelectionUi(displayFiles, { mergeBlocked });
    });
  });
  list.querySelectorAll('.ij-git-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      if (btn.dataset.status === 'conflict') {
        openConflictFile(btn.dataset.statusPath);
      } else {
        showFileDiff(btn.dataset.statusPath, btn.dataset.staged === 'true');
      }
    });
  });
  if (state.agentLiveDiffPath) {
    $$('#git-status-list .ij-git-item').forEach((btn) => {
      btn.classList.toggle('selected', btn.dataset.statusPath === state.agentLiveDiffPath);
    });
  }
  updateStatusBar({ ...status, files: displayFiles, clean: false });
  updateConflictUi();
  const result = {
    clean: false,
    files: displayFiles,
    branch: status.branch,
    merge: status.merge,
    conflict_count: status.conflict_count || 0,
  };
  if (state.activePanel === 'git') maybeAutoSuggestCommit(result);
  return result;
}

async function showCommitDiff(hash, subject) {
  if (!state.repo || !hash) return;
  state.selectedCommitHash = hash;
  $$('.ij-commit-item').forEach((el) => {
    el.classList.toggle('active', el.dataset.hash === hash);
  });
  const title = `${hash.slice(0, 7)} — ${subject}`;
  showDiffInMainArea(title, '');
  const view = $('#diff-content');
  if (view) view.innerHTML = '<div class="ij-sbs-empty">Loading…</div>';
  try {
    const data = await api(repoApi(state.repo, `/workspace/commit/${encodeURIComponent(hash)}/diff`));
    showDiffInMainArea(title, data.diff || '');
  } catch (e) {
    if (view) view.innerHTML = `<div class="ij-sbs-empty">${escapeHtml(e.message || 'Failed to load diff')}</div>`;
  }
}

function formatCommitWhen(raw) {
  if (!raw) return '';
  const normalized = raw.replace(' ', 'T').replace(/ (\d{2})(\d{2})$/, ' +$1:$2');
  const d = new Date(normalized);
  if (Number.isNaN(d.getTime())) {
    return raw.split(' ').slice(0, 2).join(' ');
  }
  const diffMs = Date.now() - d.getTime();
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} hr ago`;
  const days = Math.floor(hrs / 24);
  if (days < 30) return `${days} day${days === 1 ? '' : 's'} ago`;
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    year: d.getFullYear() !== new Date().getFullYear() ? 'numeric' : undefined,
    hour: '2-digit',
    minute: '2-digit',
  });
}

function authorInitials(name) {
  const parts = (name || '?').trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return '?';
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

async function refreshHistory() {
  if (!state.repo) return;
  const commits = await api(`${repoApi(state.repo, '/log')}?limit=50`);
  const list = $('#commit-history');
  const headerTitle = $('#commit-log-title');
  if (headerTitle) {
    headerTitle.textContent = commits.length
      ? `Git Log — ${commits.length} commit${commits.length === 1 ? '' : 's'}`
      : 'Git Log';
  }
  if (!list) return;
  if (!commits.length) {
    list.innerHTML = '<p class="ij-sbs-empty">No commits yet</p>';
    return;
  }
  list.innerHTML = commits.map((c, i) => {
    const when = formatCommitWhen(c.date);
    const title = escapeHtml(c.subject);
    const initials = escapeHtml(authorInitials(c.author));
    const author = escapeHtml(c.author);
    const hash = escapeHtml(c.hash.slice(0, 7));
    const fullHash = escapeHtml(c.hash);
    const active = state.selectedCommitHash === c.hash ? ' active' : '';
    return `
    <div class="ij-commit-item-wrap">
    <button type="button" class="ij-commit-item${active}" data-hash="${fullHash}" data-subject="${title}" title="${hash} · ${author} · ${escapeHtml(c.date || '')}">
      <div class="ij-commit-rail" aria-hidden="true">
        <span class="ij-commit-dot"></span>
        ${i < commits.length - 1 ? '<span class="ij-commit-line"></span>' : ''}
      </div>
      <div class="ij-commit-content">
        <div class="ij-commit-subject">${title}</div>
        <div class="ij-commit-meta">
          <code class="ij-commit-hash">${hash}</code>
          <span class="ij-commit-author">
            <span class="ij-commit-avatar">${initials}</span>
            ${author}
          </span>
          <time class="ij-commit-date">${escapeHtml(when)}</time>
        </div>
      </div>
    </button>
    <button type="button" class="ij-commit-action" data-action="cherry-pick" data-hash="${fullHash}" title="Cherry-pick this commit">Cherry-pick</button>
    </div>`;
  }).join('');
  list.querySelectorAll('.ij-commit-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      showCommitDiff(btn.dataset.hash, btn.dataset.subject);
    });
  });
  list.querySelectorAll('.ij-commit-action[data-action="cherry-pick"]').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      await cherryPickCommit(btn.dataset.hash);
    });
  });
}

async function cherryPickCommit(hash) {
  if (!state.repo || !hash) return;
  try {
    const out = await api(repoApi(state.repo, '/workspace/cherry-pick'), {
      method: 'POST',
      body: JSON.stringify({ hash }),
    });
    terminalLog(out.stdout || out.stderr || `Cherry-picked ${hash.slice(0, 7)}`);
    if (out.exit_code !== 0) {
      toast(out.stderr?.trim() || 'Cherry-pick needs attention', 'error');
    } else {
      toast(`Cherry-picked ${hash.slice(0, 7)}`, 'success');
    }
    await refreshGitStatus();
    await refreshTree();
    await refreshHistory();
  } catch (e) {
    toast(e.message || 'Cherry-pick failed', 'error');
  }
}

function showRebasePanel() {
  state.rebaseMode = true;
  $('#rebase-panel')?.classList.remove('hidden');
  $('#commit-history')?.classList.add('hidden');
  const onto = $('#rebase-onto');
  if (onto && !onto.value) onto.value = state.gitBranch || 'main';
}

function hideRebasePanel() {
  state.rebaseMode = false;
  state.rebaseSteps = [];
  $('#rebase-panel')?.classList.add('hidden');
  $('#commit-history')?.classList.remove('hidden');
  const steps = $('#rebase-steps');
  if (steps) steps.innerHTML = '';
  $('#btn-rebase-start')?.setAttribute('disabled', 'disabled');
}

async function loadRebasePlan() {
  if (!state.repo) return;
  const onto = ($('#rebase-onto')?.value || '').trim();
  if (!onto) {
    toast('Enter a branch or commit to rebase onto', 'error');
    return;
  }
  try {
    const commits = await api(`${repoApi(state.repo, '/workspace/rebase/plan')}?onto=${encodeURIComponent(onto)}&limit=100`);
    state.rebaseSteps = commits.map((c) => ({
      hash: c.hash,
      subject: c.subject,
      action: 'pick',
    }));
    renderRebaseSteps();
    $('#btn-rebase-start')?.removeAttribute('disabled');
  } catch (e) {
    toast(e.message || 'Could not load rebase plan', 'error');
  }
}

function renderRebaseSteps() {
  const el = $('#rebase-steps');
  if (!el) return;
  if (!state.rebaseSteps.length) {
    el.innerHTML = '<p class="ij-sbs-empty">No commits to rebase</p>';
    return;
  }
  el.innerHTML = state.rebaseSteps.map((step, idx) => `
    <div class="ij-rebase-step">
      <select class="ij-rebase-action" data-idx="${idx}" aria-label="Rebase action">
        <option value="pick"${step.action === 'pick' ? ' selected' : ''}>pick</option>
        <option value="squash"${step.action === 'squash' ? ' selected' : ''}>squash</option>
        <option value="fixup"${step.action === 'fixup' ? ' selected' : ''}>fixup</option>
        <option value="reword"${step.action === 'reword' ? ' selected' : ''}>reword</option>
        <option value="edit"${step.action === 'edit' ? ' selected' : ''}>edit</option>
        <option value="drop"${step.action === 'drop' ? ' selected' : ''}>drop</option>
      </select>
      <code class="ij-rebase-hash">${escapeHtml(step.hash.slice(0, 7))}</code>
      <span class="ij-rebase-subject">${escapeHtml(step.subject || '')}</span>
    </div>
  `).join('');
  el.querySelectorAll('.ij-rebase-action').forEach((sel) => {
    sel.addEventListener('change', () => {
      const i = Number(sel.dataset.idx);
      if (state.rebaseSteps[i]) state.rebaseSteps[i].action = sel.value;
    });
  });
}

async function startInteractiveRebase() {
  if (!state.repo || !state.rebaseSteps.length) return;
  const onto = ($('#rebase-onto')?.value || '').trim();
  if (!onto) return;
  try {
    const out = await api(repoApi(state.repo, '/workspace/rebase/start'), {
      method: 'POST',
      body: JSON.stringify({
        onto,
        steps: state.rebaseSteps.map((s) => ({
          hash: s.hash,
          action: s.action,
          subject: s.subject,
        })),
      }),
    });
    terminalLog(out.stdout || out.stderr || 'Rebase started');
    hideRebasePanel();
    switchPanel('git');
    if (out.exit_code !== 0) {
      toast(out.stderr?.trim() || 'Rebase needs attention', 'error');
    } else {
      toast('Rebase complete', 'success');
    }
    await refreshGitStatus();
    await refreshTree();
    await refreshHistory();
  } catch (e) {
    toast(e.message || 'Rebase failed', 'error');
  }
}

async function abortMerge() {
  if (!state.repo) return;
  try {
    const out = await api(repoApi(state.repo, '/workspace/conflict/abort'), { method: 'POST' });
    terminalLog(out.stdout || out.stderr || 'Operation aborted');
    await refreshGitStatus();
    await refreshTree();
    await refreshHistory();
    toast('Operation aborted', 'success');
  } catch (e) {
    toast(e.message || 'Abort failed', 'error');
  }
}

async function followAgentFileChanges(status) {
  if (!status.files?.length) return;

  let pick = null;
  if (state.agentLastToolPath) {
    pick = status.files.find((f) => f.path === state.agentLastToolPath);
  }
  if (!pick) {
    pick = status.files.find((f) => !state.agentSeenPaths.has(f.path))
      || status.files.find((f) => f.path === state.agentLiveDiffPath)
      || status.files[0];
  }
  if (!pick) return;

  state.agentLiveFollow = true;
  state.agentHadFileChanges = true;
  state.agentSeenPaths.add(pick.path);
  state.agentLiveDiffPath = pick.path;

  await showFileDiff(pick.path, pick.staged);
  setAgentDiffLive(true);
}

async function refreshAgentLiveDiff(hintPath) {
  if (hintPath) state.agentLastToolPath = hintPath;
  if (!state.repo || !state.agentBusy || state.cursorMode !== 'agent') return;

  let status = await refreshGitStatus();
  if (!status.files?.length) {
    await new Promise((r) => setTimeout(r, 200));
    status = await refreshGitStatus();
  }
  if (!status.files?.length) return;
  await followAgentFileChanges(status);
  await reloadAgentTouchedTabsFromDisk();
}

async function refreshAfterAgent({ fromAgent = false, final = false, light = false } = {}) {
  if (light) {
    await refreshGitStatus();
    return false;
  }
  await refreshTree();
  const status = await refreshGitStatus();
  if (fromAgent && state.agentHadFileChanges) {
    await reloadAgentTouchedTabsFromDisk();
  } else {
    await reloadOpenTabsFromDisk();
  }
  if (fromAgent && state.agentBusy && !status.clean) {
    await followAgentFileChanges(status);
  }
  if (final) {
    if (state.agentHadFileChanges) {
      toast('Agent finished — review changes in Source Control', 'success');
      state.agentHadFileChanges = false;
    }
    if (!status.clean && state.activePanel !== 'git') {
      const badge = $('#git-badge');
      if (badge) badge.classList.remove('hidden');
    }
  }
  return !status.clean;
}

let agentRefreshTimer = null;
let agentMarkdownTimer = null;
let cursorWarmInflight = null;
let cursorWarmRepo = null;

async function warmCursorSession(repo = state.repo) {
  if (!repo || state.agentProvider !== 'cursor' || !state.cursorConfigured) return;
  if (cursorWarmRepo === repo && cursorWarmInflight) return cursorWarmInflight;
  cursorWarmRepo = repo;
  cursorWarmInflight = api(repoApi(repo, '/cursor/session/warm'), { method: 'POST', body: '{}' })
    .then(() => {
      state.cursorBridgeOk = true;
      state.cursorBridgeError = null;
    })
    .catch(() => {})
    .finally(() => {
      if (cursorWarmRepo === repo) cursorWarmInflight = null;
    });
  return cursorWarmInflight;
}

function scheduleAgentMarkdownPreview(el, text) {
  if (!el || !window.ReaperAgentMarkdown?.libsReady?.()) return;
  clearTimeout(agentMarkdownTimer);
  agentMarkdownTimer = setTimeout(() => {
    void window.ReaperAgentMarkdown.renderAgentContent(el, text);
    scrollAgentToBottom();
  }, 200);
}
function scheduleAgentWorkspaceRefresh(hintPath) {
  if (hintPath) state.agentLastToolPath = hintPath;
  clearTimeout(agentRefreshTimer);
  agentRefreshTimer = setTimeout(() => {
    refreshAgentLiveDiff(hintPath).catch(() => {});
  }, 120);
}

async function runCommit({ push = false } = {}) {
  const message = $('#commit-message').value.trim();
  if (!message) { toast('Enter a commit message', 'error'); return; }
  const paths = getSelectedCommitPaths();
  if (!paths.length) { toast('Select at least one file to commit', 'error'); return; }
  if (state.commitBusy) return;
  let findings = [];
  try {
    findings = await scanSelectedPathsForSecrets(paths);
  } catch (err) {
    toast(err.message, 'error');
    return;
  }
  const action = push ? 'commit and push' : 'commit';
  if (!(await confirmSecretsOrProceed(findings, action))) return;
  state.commitBusy = true;
  updateCommitSelectionUi(state.lastGitStatusFiles, { mergeBlocked: state.mergeBlockedCommit });
  const useGlobalLoading = push;
  if (useGlobalLoading) setGlobalLoading(true, 'Committing & pushing…');
  try {
    const out = await api(repoApi(state.repo, '/workspace/commit'), {
      method: 'POST',
      body: JSON.stringify({ message, paths, push }),
    });
    if (out.exit_code !== 0) {
      terminalLog(out.stderr || out.stdout || (push ? 'Commit & push failed' : 'Commit failed'));
      toast(out.stderr?.trim() || out.stdout?.trim() || (push ? 'Commit & push failed' : 'Commit failed'), 'error');
      if (push) {
        await refreshGitStatus();
        await refreshHistory();
        await refreshTree();
      }
      return;
    }
    terminalLog(out.stdout || out.stderr || (push ? 'Committed and pushed' : 'Committed'));
    $('#commit-message').value = '';
    state.commitKnownPaths.clear();
    await refreshGitStatus();
    await refreshHistory();
    await refreshTree();
    toast(push ? 'Committed & pushed' : 'Committed locally — use Push when ready', 'success');
  } catch (err) {
    toast(err.message, 'error');
  } finally {
    state.commitBusy = false;
    if (useGlobalLoading) setGlobalLoading(false);
    updateCommitSelectionUi(state.lastGitStatusFiles, { mergeBlocked: state.mergeBlockedCommit });
  }
}

function commitOnly() {
  return runCommit({ push: false });
}

function commitAndPush() {
  return runCommit({ push: true });
}

async function maybeAutoSuggestCommit(status) {
  if (!state.repo || state.activePanel !== 'git' || state.commitSuggestInFlight || state.commitSuggestSkipOnce) return;
  const textarea = $('#commit-message');
  if (textarea?.value.trim()) return;
  if (status?.clean) return;
  const mergeBlocked = !!(status?.merge?.active && (status?.conflict_count || 0) > 0);
  if (mergeBlocked) return;
  if (!(await ensureGeminiReady())) return;
  await suggestCommitMessage({ auto: true });
}

function fillCommitMessage(text) {
  const textarea = $('#commit-message');
  if (!textarea || !text?.trim()) return;
  textarea.value = text.trim();
  textarea.focus();
  textarea.setSelectionRange(textarea.value.length, textarea.value.length);
}

async function suggestCommitMessage({ auto = false } = {}) {
  if (!state.repo || state.commitSuggestInFlight) return;
  if (!(await ensureGeminiReady())) {
    if (!auto) {
      state.pendingCommitSuggest = true;
      toast('Add a Gemini API key in Settings → AI', 'info');
      showSettingsModal('ai');
    }
    return;
  }
  const btn = $('#btn-suggest-commit');
  const label = btn?.querySelector('.ij-ai-btn-label');
  const textarea = $('#commit-message');
  const prevLabel = label?.textContent || 'AI';
  const prevPlaceholder = textarea?.placeholder || '';
  state.commitSuggestInFlight = true;
  if (btn) btn.disabled = true;
  if (label) label.textContent = '…';
  if (auto && textarea) textarea.placeholder = 'Generating commit message…';
  try {
    const data = await api(repoApi(state.repo, '/workspace/commit/suggest'), { method: 'POST' });
    fillCommitMessage(data.message || '');
    if (!auto) toast('Commit message ready — review and commit', 'success');
    else setStatusMessage('Commit message generated');
  } catch (err) {
    if (!auto) toast(err.message, 'error');
    else setStatusMessage(`AI commit failed: ${err.message}`);
    if (/gemini|api key/i.test(err.message)) {
      state.geminiConfigured = false;
      updateSuggestCommitButton();
      if (!auto) {
        state.pendingCommitSuggest = true;
        showSettingsModal('ai');
      }
    }
  } finally {
    state.commitSuggestInFlight = false;
    if (label) label.textContent = prevLabel;
    if (textarea) textarea.placeholder = prevPlaceholder;
    state.commitSuggestSkipOnce = true;
    refreshGitStatus()
      .catch(() => {})
      .finally(() => { state.commitSuggestSkipOnce = false; });
  }
}

async function syncPull() {
  if (!state.repo) {
    toast('Select a repository first', 'info');
    return;
  }
  const btn = $('#btn-sync');
  if (btn) btn.disabled = true;
  setGlobalLoading(true, `Pulling ${state.repo}…`);
  try {
    const out = await api(repoApi(state.repo, '/workspace/sync'), { method: 'POST' });
    terminalLog(out.stdout || out.stderr || 'Synced');
    await refreshTree();
    await refreshGitStatus();
    toast('Pulled latest', 'success');
  } catch (err) {
    const msg = err.message || String(err);
    terminalLog(`pull failed: ${msg}`);
    toast(msg, 'error');
  } finally {
    if (btn) btn.disabled = false;
    setGlobalLoading(false);
  }
}

async function checkoutBranch(branch) {
  if (!state.repo || !branch || branch === state.currentBranch) return;
  setGlobalLoading(true, `Switching to ${branch}…`);
  try {
    leaveSaveGate();
    const out = await api(repoApi(state.repo, '/workspace/checkout'), {
      method: 'POST',
      body: JSON.stringify({ branch }),
    });
    if ((out.exit_code ?? 0) !== 0) {
      const msg = (out.stderr || out.stdout || '').trim() || `Could not switch to ${branch}`;
      terminalLog(msg);
      toast(msg, 'error');
      return;
    }
    terminalLog((out.stdout || out.stderr || '').trim() || `Switched to ${branch}`);
    closeWorkspaceTabs();
    try {
      const detail = await api(repoApi(state.repo));
      state.branches = normalizeBranchList(detail.branches);
      state.defaultBranch = resolveDefaultBranch(
        detail.default_branch || detail.summary?.default_branch || '',
        state.branches,
      );
      updateBranchPickerState();
    } catch {
      /* branch list refresh is best-effort */
    }
    await refreshTree({ resetExpanded: true });
    await refreshGitStatus();
    await refreshHistory();
    startProjectIndexPolling();
    await openFile('README.md', { silent: true });
    toast(`Switched to ${branch}`, 'success');
  } catch (err) {
    toast(err.message, 'error');
  } finally {
    setGlobalLoading(false);
  }
}

// --- Terminal (xterm + PTY WebSocket) ---
let terminalNextNum = 1;

const TERM_SOURCE_EXT = 'java|kt|kts|scala|groovy|gradle|xml|properties|json|yaml|yml|rs|py|js|ts|tsx|jsx|go|rb|cs|cpp|c|h|hpp|md|sql|html|css|vue|swift|php|sh|toml|proto';
const TERM_PATH_SEGMENT = `[A-Za-z0-9_.@\\[\\]-]+`;
const TERM_FILE_PATH = `(?:(?:[A-Za-z]:)?\\/)?${TERM_PATH_SEGMENT}(?:[\\/]${TERM_PATH_SEGMENT})*\\.(?:${TERM_SOURCE_EXT})`;

function resolveTerminalFilePath(rawPath) {
  let p = workspaceExplorerPath(String(rawPath || '').trim().replace(/\\/g, '/'));
  if (!p || /^https?:\/\//i.test(p)) return null;

  const projectFolder = (state.projectFolder || '').replace(/\\/g, '/').replace(/\/$/, '');
  if (projectFolder && (p === projectFolder || p.startsWith(`${projectFolder}/`))) {
    p = p === projectFolder ? '' : p.slice(projectFolder.length + 1);
  } else if (p.startsWith('/') || /^[A-Za-z]:\//.test(p)) {
    for (const tab of state.tabs || []) {
      const rel = workspaceExplorerPath(tab);
      if (!rel) continue;
      if (p.endsWith(`/${rel}`) || p.endsWith(rel)) return rel;
    }
    const markers = ['/src/', '/test/', '/main/', '/java/', '/kotlin/', '/resources/'];
    for (const mark of markers) {
      const idx = p.lastIndexOf(mark);
      if (idx >= 0) {
        p = p.slice(idx + 1);
        break;
      }
    }
    if (p.startsWith('/') || /^[A-Za-z]:\//.test(p)) {
      const tail = p.match(/\/((?:src|test|services)\/.+)$/);
      if (!tail) return null;
      p = tail[1];
    }
  }

  p = normalizeRepoPath(p.replace(/^\.\//, ''));
  if (!p || !/\.\w+$/.test(p)) return null;
  return p;
}

function terminalLinkRange(match, lineText = '') {
  const path = match[1];
  let start = match.index + match[0].indexOf(path);
  if (match[0].includes('[')) {
    let slice = match[0].slice(match[0].indexOf(path));
    if (lineText && start > 0 && lineText[start - 1] === '/' && path[0] !== '/') {
      start -= 1;
      slice = `/${slice}`;
    }
    return { start, end: start + slice.length };
  }
  let end = start + path.length + 1 + String(match[2]).length;
  if (match[3] && /:\d+(?::|\]|$)/.test(match[0])) {
    end += 1 + String(match[3]).length;
  }
  return { start, end };
}

function parseTerminalFileLocations(lineText) {
  const out = [];
  const seen = new Set();
  const patterns = [
    {
      re: new RegExp(`(${TERM_FILE_PATH}):\\[\\s*(\\d+)\\s*,\\s*(\\d+)\\s*\\]`, 'gi'),
      pick: (m) => ({ path: m[1], line: +m[2], column: +m[3] }),
    },
    {
      re: new RegExp(`(${TERM_FILE_PATH}):(\\d+):(\\d+):\\s*(?:error|warning|note|fatal)`, 'gi'),
      pick: (m) => ({ path: m[1], line: +m[2], column: +m[3] }),
    },
    {
      re: new RegExp(`-->\\s+(${TERM_FILE_PATH}):(\\d+):(\\d+)`, 'gi'),
      pick: (m) => ({ path: m[1], line: +m[2], column: +m[3] }),
    },
    {
      re: new RegExp(`(${TERM_FILE_PATH}):(\\d+):\\s*(?:error|warning|note|fatal)`, 'gi'),
      pick: (m) => ({ path: m[1], line: +m[2], column: 1 }),
    },
    {
      re: new RegExp(`\\((${TERM_FILE_PATH}):(\\d+)\\)`, 'gi'),
      pick: (m) => ({ path: m[1], line: +m[2], column: 1 }),
    },
  ];

  for (const { re, pick } of patterns) {
    re.lastIndex = 0;
    let match;
    while ((match = re.exec(lineText)) !== null) {
      const hit = pick(match);
      if (match.index > 0 && lineText[match.index - 1] === '/' && !hit.path.startsWith('/')) {
        hit.path = `/${hit.path}`;
      }
      const { start, end } = terminalLinkRange(match, lineText);
      const key = `${hit.path}:${hit.line}:${hit.column}:${start}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ ...hit, start, end });
    }
  }
  out.sort((a, b) => a.start - b.start);
  return out;
}

function registerTerminalFileLinkProvider(term, xterm) {
  if (!term || !xterm?.registerLinkProvider) return;
  if (term.linkProviderDisposable) {
    try { term.linkProviderDisposable.dispose(); } catch { /* ignore */ }
    term.linkProviderDisposable = null;
  }
  term.linkProviderDisposable = xterm.registerLinkProvider({
    provideLinks(bufferLineNumber, callback) {
      const line = xterm.buffer.active.getLine(bufferLineNumber - 1);
      if (!line) {
        callback(undefined);
        return;
      }
      const text = line.translateToString(true);
      const hits = parseTerminalFileLocations(text);
      if (!hits.length) {
        callback(undefined);
        return;
      }
      const links = [];
      for (const hit of hits) {
        const path = resolveTerminalFilePath(hit.path);
        if (!path || !state.repo) continue;
        const label = `${path}:${hit.line}${hit.column > 1 ? `:${hit.column}` : ''}`;
        links.push({
          text: label,
          range: {
            start: { x: hit.start + 1, y: bufferLineNumber },
            end: { x: hit.end + 1, y: bufferLineNumber },
          },
          activate(_event, _text) {
            void openFileAt(path, hit.line, hit.column || 1);
          },
        });
      }
      callback(links.length ? links : undefined);
    },
  });
}

function resolveFitAddonCtor() {
  const raw = globalThis.FitAddon ?? window.FitAddon;
  if (typeof raw === 'function') return raw;
  if (raw && typeof raw.FitAddon === 'function') return raw.FitAddon;
  if (raw?.default && typeof raw.default === 'function') return raw.default;
  if (raw?.default && typeof raw.default.FitAddon === 'function') return raw.default.FitAddon;
  return null;
}

function xtermApi() {
  const Terminal = globalThis.Terminal || window.Terminal;
  if (typeof Terminal !== 'function') return null;
  return { Terminal, FitAddon: resolveFitAddonCtor() };
}

let loopbackWsPromise = null;
let terminalMountGeneration = 0;
let terminalFitTimer = null;

function cacheLoopbackWs(value) {
  if (typeof value !== 'string' || !value.trim()) return;
  window.__REAPER_LOOPBACK_WS__ = value.trim().replace(/\/$/, '');
}

function resolveLoopbackWsBase() {
  const injected = window.__REAPER_LOOPBACK_WS__;
  if (typeof injected === 'string' && injected.trim()) {
    return injected.trim().replace(/\/$/, '');
  }
  const proto = location.protocol;
  if (proto === 'http:' || proto === 'https:') {
    return `${proto === 'https:' ? 'wss:' : 'ws:'}//${location.host}`;
  }
  return null;
}

function ensureLoopbackWsBase() {
  const existing = resolveLoopbackWsBase();
  if (existing) return Promise.resolve(existing);
  if (!loopbackWsPromise) {
    loopbackWsPromise = api('/api/version')
      .then((info) => {
        cacheLoopbackWs(info?.loopback_ws);
        return resolveLoopbackWsBase();
      })
      .catch(() => resolveLoopbackWsBase());
  }
  return loopbackWsPromise;
}

function isTerminalCommandActive(term) {
  return term?.streamLine != null;
}

function restoreTerminalShellIfIdle(term) {
  if (!term || isTerminalCommandActive(term) || term.shellSuspended) return;
  if (term.deferShellUntilInput || term.guardRunOutput) return;
  if (term.streamLine != null) {
    term.streamLine = null;
    term.streamColorPartial = '';
    stopCommandStatus(term);
  }
  term.shellSuspended = false;
  if (!term.ws || term.ws.readyState !== WebSocket.OPEN) {
    void connectTerminalWs(term);
  }
}

function isTerminalPanelVisible() {
  if (state.terminalDock === 'left') return state.activePanel === 'terminal';
  return state.terminalOpen;
}

function defaultTerminalSize() {
  return { cols: 120, rows: 32 };
}

function createTerminalSession(name) {
  const num = terminalNextNum++;
  return {
    id: `term-${num}`,
    name: name || String(num),
    cwd: '',
    container: null,
    xterm: null,
    fitAddon: null,
    ws: null,
    streamLine: null,
    streamColorPartial: '',
    commandStartLine: null,
    deferShellUntilInput: false,
    guardRunOutput: false,
    wsConnecting: false,
    shellInputBuffer: '',
    shellPtyPartial: '',
    shellConnectedAt: 0,
    lastShellSize: null,
    linkProviderDisposable: null,
  };
}

function ensureTerminals() {
  if (state.terminals.length) return;
  const term = createTerminalSession();
  state.terminals.push(term);
  state.activeTerminalId = term.id;
}

function getActiveTerminal() {
  ensureTerminals();
  return state.terminals.find((t) => t.id === state.activeTerminalId) || state.terminals[0];
}

function terminalForId(id) {
  return state.terminals.find((t) => t.id === id);
}

function destroyTerminalInstance(term) {
  if (!term) return;
  disconnectTerminalWs(term, { silent: true });
  if (term.linkProviderDisposable) {
    try { term.linkProviderDisposable.dispose(); } catch { /* ignore */ }
    term.linkProviderDisposable = null;
  }
  if (term.xterm) {
    try { term.xterm.dispose(); } catch { /* ignore */ }
    term.xterm = null;
    term.fitAddon = null;
  }
  if (term.container) {
    term.container.remove();
    term.container = null;
  }
  term.streamLine = null;
  term.shellSuspended = false;
  term.deferShellUntilInput = false;
  term.wsConnecting = false;
  term.shellInputBuffer = '';
  term.shellPtyPartial = '';
  term.guardRunOutput = false;
  term.lastShellSize = null;
}

function spawnTerminalInstance(term, host) {
  if (!term || !host) return;
  destroyTerminalInstance(term);
  initTerminalXterm(term, host);
}

function resetTerminalCwds() {
  const host = $('#terminal-xterm-host');
  const mountNow = isTerminalPanelVisible();
  state.terminals.forEach((t) => {
    t.cwd = '';
    disconnectTerminalWs(t, { silent: true });
    if (mountNow && host) spawnTerminalInstance(t, host);
    else destroyTerminalInstance(t);
  });
}

function fitActiveTerminal() {
  fitTerminal(getActiveTerminal());
}

function syncShellLayout() {
  if (state.editor) {
    try { state.editor.layout(); } catch { /* ignore */ }
  }
  if (state.terminalOpen || state.activePanel === 'terminal') {
    clearTimeout(terminalFitTimer);
    terminalFitTimer = setTimeout(() => fitActiveTerminal(), 100);
  }
}
window.__reaperSyncLayout = syncShellLayout;

function fitTerminal(term) {
  if (!term?.xterm) return;
  if (term.fitAddon) {
    try {
      term.fitAddon.fit();
    } catch {
      /* host may be hidden */
    }
  } else {
    const size = defaultTerminalSize();
    try { term.xterm.resize(size.cols, size.rows); } catch { /* ignore */ }
  }
  sendTerminalResize(term);
}

function releaseTerminalRunOutputGuard(term) {
  if (!term?.guardRunOutput) return;
  term.guardRunOutput = false;
  sendTerminalResize(term);
}

function sendTerminalResize(term) {
  if (!term?.ws || term.ws.readyState !== WebSocket.OPEN || !term.xterm) return;
  if (term.guardRunOutput) return;
  const cols = term.xterm.cols;
  const rows = term.xterm.rows;
  if (term.lastShellSize?.cols === cols && term.lastShellSize?.rows === rows) return;
  term.lastShellSize = { cols, rows };
  term.ws.send(JSON.stringify({
    type: 'resize',
    cols,
    rows,
  }));
}

function terminalWsUrl(term, base) {
  if (!base || !state.repo) return null;
  let url = `${base}/api/repos/${encodeURIComponent(state.repo)}/workspace/terminal`;
  if (term.cwd) url += `?cwd=${encodeURIComponent(term.cwd)}`;
  return url;
}

function disconnectTerminalWs(term, { silent = false } = {}) {
  if (!term?.ws) return;
  if (silent) term.wsSilentClose = true;
  try { term.ws.close(); } catch { /* ignore */ }
  term.ws = null;
  term.wsConnecting = false;
  term.lastShellSize = null;
}

function decodeTerminalChunk(chunk) {
  if (chunk == null) return '';
  if (typeof chunk === 'string') return chunk;
  if (chunk instanceof ArrayBuffer) return new TextDecoder('utf-8', { fatal: false }).decode(chunk);
  if (ArrayBuffer.isView(chunk)) {
    return new TextDecoder('utf-8', { fatal: false }).decode(
      chunk.buffer.slice(chunk.byteOffset, chunk.byteOffset + chunk.byteLength),
    );
  }
  return '';
}

function sanitizeShellPtyOutput(term, text) {
  if (!text) return text;
  let out = text;
  if (term?.guardRunOutput) {
    // cmd.exe clears the current row then redraws the prompt on shell reconnect/resize.
    out = out.replace(/\x1b\[[0-9;]*[Kk]/g, '\r\n');
  }
  // Bare CR (not CRLF) redraws the current row — cmd.exe uses this for prompts on resize.
  out = out.replace(/\r(?!\n)/g, '\r\n');
  if (term?.guardRunOutput) {
    out = out.replace(/([^\r\n])([A-Za-z]:[\\/][^\r\n]*>\s*)/g, '$1\r\n$2');
  }
  return out;
}

function writeShellPtyToXterm(term, chunk) {
  if (!term?.xterm || chunk == null) return;
  const text = decodeTerminalChunk(chunk);
  if (!text) return;
  let combined = `${term.shellPtyPartial || ''}${text}`;
  const trailingCr = combined.match(/\r(?!\n)$/);
  if (trailingCr) {
    term.shellPtyPartial = trailingCr[0];
    combined = combined.slice(0, -trailingCr[0].length);
  } else {
    term.shellPtyPartial = '';
  }
  if (!combined) return;
  combined = sanitizeShellPtyOutput(term, combined);
  if (combined) term.xterm.write(combined);
}

function flushTerminalShellInputBuffer(term) {
  if (!term?.shellInputBuffer || !term.ws || term.ws.readyState !== WebSocket.OPEN) return;
  const pending = term.shellInputBuffer;
  term.shellInputBuffer = '';
  if (pending) term.ws.send(pending);
}

async function connectTerminalWs(term) {
  if (!state.repo || !term) return;
  if (term.shellSuspended || term.streamLine != null) return;
  if (term.wsConnecting || term.ws?.readyState === WebSocket.OPEN) return;
  if (term.ws?.readyState === WebSocket.CONNECTING) return;
  term.wsConnecting = true;
  const base = await ensureLoopbackWsBase();
  if (term.shellSuspended || term.streamLine != null) {
    term.wsConnecting = false;
    return;
  }
  const url = terminalWsUrl(term, base);
  if (!url) {
    term.wsConnecting = false;
    toast('Terminal shell unavailable (loopback WebSocket not configured). Restart Reaper.', 'error');
    return;
  }
  disconnectTerminalWs(term, { silent: true });
  const ws = new WebSocket(url);
  term.ws = ws;
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => {
    term.wsConnecting = false;
    if (term.ws !== ws || term.shellSuspended || term.streamLine != null) {
      disconnectTerminalWs(term, { silent: true });
      return;
    }
    term.shellConnectedAt = Date.now();
    if (term.xterm) {
      try { term.xterm.scrollToBottom(); } catch { /* ignore */ }
      if (term.guardRunOutput) {
        term.xterm.write('\r\n\r\n');
      }
    }
    requestAnimationFrame(() => {
      if (term.ws !== ws) return;
      fitTerminal(term);
      flushTerminalShellInputBuffer(term);
    });
  };
  ws.onerror = () => {
    term.wsConnecting = false;
    if (term.ws !== ws || term.wsSilentClose || term.shellSuspended || term.streamLine != null) return;
    toast('Terminal shell connection failed. Try Restart Shell.', 'error');
  };
  ws.onmessage = (ev) => {
    if (!term.xterm || term.shellSuspended || isTerminalCommandActive(term)) return;
    const writeChunk = (chunk) => writeShellPtyToXterm(term, chunk);
    if (ev.data instanceof ArrayBuffer) {
      writeChunk(ev.data);
    } else if (ArrayBuffer.isView(ev.data)) {
      writeChunk(ev.data.buffer.slice(ev.data.byteOffset, ev.data.byteOffset + ev.data.byteLength));
    } else if (typeof ev.data === 'string') {
      writeChunk(ev.data);
    } else if (typeof Blob !== 'undefined' && ev.data instanceof Blob) {
      ev.data.arrayBuffer().then(writeChunk).catch(() => {});
    }
  };
  ws.onclose = () => {
    term.wsConnecting = false;
    if (term.wsSilentClose) {
      term.wsSilentClose = false;
      return;
    }
    if (term.shellSuspended || isTerminalCommandActive(term)) return;
    if (term.xterm) {
      term.xterm.write('\r\n\x1b[90m[session ended]\x1b[0m\r\n');
    }
  };
}

function ensureTerminalPane(term, host) {
  if (term.container?.isConnected) return term.container;
  const pane = document.createElement('div');
  pane.className = 'ij-terminal-xterm-pane';
  pane.dataset.terminalId = term.id;
  host.appendChild(pane);
  term.container = pane;
  return pane;
}

function initTerminalXterm(term, host) {
  const api = xtermApi();
  if (!api || !host || !term) {
    if (!api) {
      toast('Terminal failed to load (xterm scripts missing). Reload the app.', 'error');
    }
    return;
  }

  const pane = ensureTerminalPane(term, host);
  pane.classList.remove('hidden');
  const xterm = new api.Terminal({
    cursorBlink: true,
    convertEol: true,
    fontSize: getEditorFontSize(),
    lineHeight: 1.2,
    fontFamily: getEditorFontSpec().family,
    theme: terminalThemeFromApp(),
    scrollback: 8000,
    ...defaultTerminalSize(),
  });
  let fitAddon = null;
  if (api.FitAddon) {
    fitAddon = new api.FitAddon();
    xterm.loadAddon(fitAddon);
  }
  xterm.open(pane);
  registerTerminalFileLinkProvider(term, xterm);
  const resumeShellIfDeferred = () => {
    if (term.deferShellUntilInput && term.shellSuspended) {
      resumeTerminalShell(term);
    }
  };
  xterm.textarea?.addEventListener('focus', resumeShellIfDeferred);
  pane.addEventListener('mousedown', resumeShellIfDeferred);
  xterm.onData((data) => {
    if (typeof data === 'string' && data.includes('\x03')) {
      if (term.streamLine != null) {
        void cancelActiveTerminalCommand(term.id);
        return;
      }
    }
    if (term.shellInputBuffer && term.ws?.readyState !== WebSocket.OPEN) {
      term.shellInputBuffer += data;
      return;
    }
    if (term.ws?.readyState === WebSocket.OPEN) {
      releaseTerminalRunOutputGuard(term);
      term.ws.send(data);
    }
  });
  xterm.attachCustomKeyEventHandler((ev) => {
    const mod = ev.metaKey || ev.ctrlKey;
    if (!mod || (ev.key !== 'c' && ev.key !== 'C')) return true;
    if (term.streamLine != null) {
      ev.preventDefault();
      void cancelActiveTerminalCommand(term.id);
      return false;
    }
    return true;
  });
  term.xterm = xterm;
  term.fitAddon = fitAddon;
  fitTerminal(term);
  if (!term.shellSuspended && term.streamLine == null) {
    void connectTerminalWs(term);
  }
}

function mountActiveTerminal({ fresh = false, sync = false } = {}) {
  ensureTerminals();
  const term = getActiveTerminal();
  const host = $('#terminal-xterm-host');
  if (!host || !term) return;
  const mountGen = terminalMountGeneration;

  state.terminals.forEach((t) => {
    if (t.container) t.container.classList.add('hidden');
  });

  if (isTerminalCommandActive(term) && term.xterm) {
    ensureTerminalPane(term, host);
    term.container.classList.remove('hidden');
    term.xterm.focus();
    return;
  }

  if ((term.shellSuspended || term.deferShellUntilInput) && term.xterm?.element?.isConnected) {
    ensureTerminalPane(term, host);
    term.container.classList.remove('hidden');
    fitTerminal(term);
    term.xterm.focus();
    return;
  }

  const xtermMissing = !term.xterm || !term.xterm.element?.isConnected;
  const spawn = () => {
    if (mountGen !== terminalMountGeneration) return;
    if (fresh || xtermMissing) {
      spawnTerminalInstance(term, host);
    } else if (!term.ws || term.ws.readyState !== WebSocket.OPEN) {
      ensureTerminalPane(term, host);
      term.container.classList.remove('hidden');
      restoreTerminalShellIfIdle(term);
      fitTerminal(term);
    } else {
      ensureTerminalPane(term, host);
      term.container.classList.remove('hidden');
      fitTerminal(term);
    }
    term.container?.classList.remove('hidden');
    requestAnimationFrame(() => {
      fitActiveTerminal();
      requestAnimationFrame(() => {
        if (term.container?.contains(document.activeElement)) {
          term.xterm?.focus();
        }
      });
    });
  };
  if (sync) spawn();
  else requestAnimationFrame(() => requestAnimationFrame(spawn));
}

function renderTerminalTabs() {
  const bar = $('#terminal-tabs');
  if (!bar) return;
  ensureTerminals();
  bar.replaceChildren();
  state.terminals.forEach((term) => {
    const tab = document.createElement('button');
    tab.type = 'button';
    tab.className = 'ij-terminal-tab';
    tab.dataset.terminalId = term.id;
    tab.setAttribute('role', 'tab');
    tab.setAttribute('aria-selected', term.id === state.activeTerminalId ? 'true' : 'false');
    tab.title = `Terminal ${term.name}`;
    if (term.id === state.activeTerminalId) tab.classList.add('active');

    const label = document.createElement('span');
    label.className = 'ij-terminal-tab-label';
    label.textContent = term.name;
    tab.appendChild(label);

    const close = document.createElement('span');
    close.className = 'ij-terminal-tab-close';
    close.setAttribute('aria-label', `Close terminal ${term.name}`);
    close.textContent = '×';
    tab.appendChild(close);

    bar.appendChild(tab);
  });
}

function switchTerminal(id) {
  if (!state.terminals.some((t) => t.id === id)) return;
  state.activeTerminalId = id;
  renderTerminalTabs();
  mountActiveTerminal();
}

function newTerminal({ focus = true } = {}) {
  const term = createTerminalSession();
  state.terminals.push(term);
  state.activeTerminalId = term.id;
  renderTerminalTabs();
  if (focus) showTerminal();
  mountActiveTerminal({ fresh: true });
}

function closeTerminal(id) {
  ensureTerminals();
  const term = terminalForId(id);
  if (!term) return;
  const host = $('#terminal-xterm-host');
  if (state.terminals.length <= 1) {
    spawnTerminalInstance(term, host);
    renderTerminalTabs();
    mountActiveTerminal();
    return;
  }
  destroyTerminalInstance(term);
  const idx = state.terminals.findIndex((t) => t.id === id);
  state.terminals.splice(idx, 1);
  if (state.activeTerminalId === id) {
    const next = state.terminals[Math.max(0, idx - 1)];
    state.activeTerminalId = next.id;
  }
  renderTerminalTabs();
  mountActiveTerminal();
}

function suspendTerminalShell(term) {
  if (!term || term.shellSuspended) return;
  term.shellSuspended = true;
  disconnectTerminalWs(term, { silent: true });
}

function resumeTerminalShell(term) {
  if (!term || !term.shellSuspended) return;
  term.shellSuspended = false;
  term.deferShellUntilInput = false;
  term.guardRunOutput = true;
  term.shellPtyPartial = '';
  if (term.xterm) {
    try { term.xterm.scrollToBottom(); } catch { /* ignore */ }
    term.xterm.write('\r\n\r\n');
  }
  void connectTerminalWs(term);
}

function terminalWrite(term, text) {
  if (!text) return;
  const t = term || getActiveTerminal();
  if (!t?.xterm) return;
  const normalized = String(text).replace(/\r?\n/g, '\r\n');
  t.xterm.write(normalized + (normalized.endsWith('\r\n') ? '' : '\r\n'));
}

function writeColorizedStreamChunk(term, text) {
  if (!term?.xterm || !text) return;
  const combined = `${term.streamColorPartial || ''}${text}`;
  const parts = combined.split('\n');
  term.streamColorPartial = combined.endsWith('\n') ? '' : (parts.pop() || '');
  let out = '';
  for (const line of parts) {
    if (!String(line).trimEnd()) {
      term.streamBlankRun = (term.streamBlankRun || 0) + 1;
      if (term.streamBlankRun <= 1) out += '\n';
      continue;
    }
    term.streamBlankRun = 0;
    if (isGradleNoopTaskLine(line)) {
      term.streamNoopTaskCount = (term.streamNoopTaskCount || 0) + 1;
      if (term.streamNoopTaskCount === 1) term.streamNoopTaskSample = line;
      continue;
    }
    out = flushNoopTaskSummary(term, { into: out });
    out += `${colorizeStreamLine(line)}\n`;
  }
  if (out) term.xterm.write(out.replace(/\n/g, '\r\n'));
}

function terminalLog(text, terminalId) {
  const t = terminalForId(terminalId) || getActiveTerminal();
  if (!t?.xterm) return;
  if (t.streamLine != null) {
    terminalStreamChunk(`${text}\n`, terminalId || t.id);
    return;
  }
  terminalWrite(t, `${TERM_ESC.dim}${text}${TERM_ESC.reset}`);
}

/** Mirror API/debug/index failures into the terminal (backend logs stay in reaper.log). */
function terminalLogError(text, { label = 'error', show = true } = {}) {
  if (text == null || text === '') return;
  const msg = String(text).trim();
  if (!msg) return;
  if (show) showTerminal();
  const t = getActiveTerminal();
  if (!t?.xterm) return;
  const line = label ? `${label}: ${msg}` : msg;
  if (t.streamLine != null) {
    terminalStreamChunk(`${line}\n`, t.id);
    return;
  }
  terminalWrite(t, `${TERM_ESC.brightRed}${line}${TERM_ESC.reset}`);
}

function resolveTerminal(terminalId) {
  return terminalForId(terminalId) || getActiveTerminal();
}

function terminalBufferLine(term) {
  if (!term?.xterm) return 0;
  const buf = term.xterm.buffer.active;
  return buf.baseY + buf.cursorY;
}

function scrollTerminalToLine(term, line) {
  if (!term?.xterm || line == null) return;
  requestAnimationFrame(() => {
    try {
      term.xterm.scrollToLine(Math.max(0, line));
    } catch {
      /* viewport may not be ready */
    }
  });
}

function focusCommandTerminal(terminalId) {
  showTerminal();
  if (terminalId && state.activeTerminalId !== terminalId) {
    switchTerminal(terminalId);
  } else {
    mountActiveTerminal();
  }
  const term = resolveTerminal(terminalId);
  term?.xterm?.focus();
  return term;
}

async function ensureCommandTerminalReady(terminalId) {
  state.terminalMountSync = true;
  try {
    if (state.terminalDock === 'left') {
      if (state.activePanel !== 'terminal') {
        state.activePanel = 'terminal';
        syncActivityButtons();
      }
    } else {
      state.terminalOpen = true;
    }
    applyTerminalDock();
    if (terminalId && state.activeTerminalId !== terminalId) {
      state.activeTerminalId = terminalId;
      renderTerminalTabs();
    }
    const term = resolveTerminal(terminalId);
    const host = $('#terminal-xterm-host');
    if (!term || !host) throw new Error('Terminal not ready — try again');
    const needsSpawn = !term.xterm || !term.xterm.element?.isConnected;
    mountActiveTerminal({ fresh: needsSpawn, sync: true });
    for (let i = 0; i < 60; i += 1) {
      if (term.xterm?.element?.isConnected) {
        fitTerminal(term);
        term.xterm.focus();
        return term;
      }
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    throw new Error('Terminal not ready — try again');
  } finally {
    state.terminalMountSync = false;
  }
}

function terminalCommandBegin(label, terminalId, { kind } = {}) {
  const term = resolveTerminal(terminalId);
  if (!term?.xterm) return;
  term.streamColorPartial = '';
  term.streamBlankRun = 0;
  resetStreamCollapseState(term);
  term.commandStartLine = terminalBufferLine(term);
  const labelText = String(label || '').trim().replace(/^▶\s*/, '') || 'command';
  const isTest = kind === 'test' || (/\btest\b/i.test(labelText) && /\b(gradle|mvn|maven)\b/i.test(labelText));
  const accent = isTest ? TERM_ESC.brightCyan : TERM_ESC.cyan;
  if (term.commandStartLine > 0) term.xterm.write('\r\n');
  term.xterm.write(`${accent}${TERM_ESC.bold}${labelText}${TERM_ESC.reset}\r\n\r\n`);
  startCommandStatus(labelText, terminalId);
  scrollTerminalToLine(term, term.commandStartLine);
}

function terminalCommandEnd(exitCode, terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term?.xterm) return;
  stopCommandStatus(term);
  flushNoopTaskSummary(term);
  if (term.streamColorPartial) {
    const partial = term.streamColorPartial;
    term.streamColorPartial = '';
    term.xterm.write(`${colorizeStreamLine(partial).replace(/\n/g, '\r\n')}`);
  }
  if (typeof exitCode === 'number') {
    const icon = exitCode === 0 ? TERM_ESC.brightGreen : TERM_ESC.brightRed;
    term.xterm.write(`\r\n${icon} exit ${exitCode}${TERM_ESC.reset}\r\n`);
  }
  try { term.xterm.scrollToBottom(); } catch { /* ignore */ }
  term.deferShellUntilInput = true;
  term.guardRunOutput = true;
  term.shellPtyPartial = '';
}

function beginTerminalStream(terminalId) {
  terminalMountGeneration += 1;
  const term = resolveTerminal(terminalId);
  if (!term) return;
  suspendTerminalShell(term);
  term.streamLine = '';
  term.streamColorPartial = '';
  term.streamBlankRun = 0;
  term.deferShellUntilInput = true;
  resetStreamCollapseState(term);
}

function terminalStreamChunk(text, terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term || term.streamLine == null) return;
  term.streamLine += text;
  writeColorizedStreamChunk(term, text);
}

function finalizeTerminalStream(terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term) return;
  stopCommandStatus(term);
  term.streamLine = null;
  term.streamColorPartial = '';
}

function clearActiveTerminal() {
  const term = getActiveTerminal();
  if (!term?.xterm) return;
  term.xterm.clear();
  term.streamLine = null;
  term.streamColorPartial = '';
  resetStreamCollapseState(term);
  term.xterm.focus();
}

function restartActiveTerminal() {
  const term = getActiveTerminal();
  const host = $('#terminal-xterm-host');
  if (!term || !host) return;
  term.deferShellUntilInput = false;
  term.guardRunOutput = false;
  term.shellInputBuffer = '';
  term.shellPtyPartial = '';
  spawnTerminalInstance(term, host);
  mountActiveTerminal();
}

function bindTerminalTabs() {
  $('#btn-terminal-new')?.addEventListener('click', () => newTerminal());
  $('#btn-terminal-clear')?.addEventListener('click', () => clearActiveTerminal());
  $('#btn-terminal-restart')?.addEventListener('click', () => restartActiveTerminal());
  $('#terminal-tabs')?.addEventListener('click', (e) => {
    const closeBtn = e.target.closest('.ij-terminal-tab-close');
    const tab = e.target.closest('.ij-terminal-tab');
    if (!tab) return;
    if (closeBtn) {
      e.stopPropagation();
      closeTerminal(tab.dataset.terminalId);
      return;
    }
    switchTerminal(tab.dataset.terminalId);
  });
}

async function consumeWorkspaceExecStream(res, terminalId) {
  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let sseBuffer = '';
  let exitCode = 0;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    sseBuffer += decoder.decode(value, { stream: true });
    const parts = sseBuffer.split('\n\n');
    sseBuffer = parts.pop() || '';
    for (const part of parts) {
      const line = part.split('\n').find((l) => l.startsWith('data: '));
      if (!line) continue;
      const event = JSON.parse(line.slice(6));
      if (event.text) terminalStreamChunk(event.text, terminalId);
      if (event.t === 'exit' && event.code != null) exitCode = event.code;
      if (event.t === 'error' && event.text) terminalStreamChunk(event.text, terminalId);
    }
  }

  return exitCode;
}

async function cancelActiveTerminalCommand(terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term || term.streamLine == null) return false;
  term.xterm?.write(`\r\n${TERM_ESC.brightYellow}^C${TERM_ESC.reset}\r\n`);
  term.execAbortController?.abort();
  if (state.repo) {
    try {
      await fetch(repoApi(state.repo, '/workspace/exec/cancel'), { method: 'POST' });
    } catch { /* ignore */ }
  }
  return true;
}

async function postWorkspaceExecStream(path, body, terminalId) {
  const term = resolveTerminal(terminalId);
  term?.execAbortController?.abort();
  const ac = new AbortController();
  if (term) term.execAbortController = ac;
  const res = await fetch(repoApi(state.repo, path), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal: ac.signal,
  });
  if (!res.ok) {
    let errMsg = res.statusText;
    try {
      const err = await res.json();
      errMsg = err.error || errMsg;
    } catch { /* ignore */ }
    throw new Error(errMsg);
  }
  return consumeWorkspaceExecStream(res, terminalId);
}

async function runWorkspaceCommandStream(path, body, { label, terminalId, kind } = {}) {
  const termId = terminalId ?? getActiveTerminal()?.id;
  await ensureCommandTerminalReady(termId);
  const term = resolveTerminal(termId);
  if (term) suspendTerminalShell(term);
  beginTerminalStream(termId);
  if (label && termId) terminalCommandBegin(label, termId, { kind });
  try {
    const exitCode = await postWorkspaceExecStream(path, body, termId);
    const output = terminalForId(termId)?.streamLine || '';
    finalizeTerminalStream(termId);
    if (termId) terminalCommandEnd(exitCode, termId);
    return { exitCode, output };
  } catch (e) {
    const cancelled = e?.name === 'AbortError';
    finalizeTerminalStream(termId);
    if (termId) terminalCommandEnd(cancelled ? 130 : -1, termId);
    if (cancelled) {
      return { exitCode: 130, output: terminalForId(termId)?.streamLine || '' };
    }
    throw e;
  } finally {
    const term = terminalForId(termId);
    if (term) term.execAbortController = null;
  }
}

// --- Cursor Agent ---
function scrollAgentToBottom() {
  const box = $('#agent-messages');
  if (box) box.scrollTop = box.scrollHeight;
}

function pickAgentFinalText(textBuffer, buffer, summary) {
  const cleaned = window.ReaperAgentMarkdown?.prepareAgentMarkdown(buffer) || String(buffer || '').trim();
  const streamed = String(textBuffer || '').trim();
  const done = String(summary || '').trim();
  if (done.length > streamed.length) return done;
  if (streamed.length > cleaned.length) return streamed;
  return cleaned || streamed || done;
}

function getAgentProvider(id) {
  return AGENT_PROVIDERS[id] || AGENT_PROVIDERS.cursor;
}

function agentProviderDef() {
  return getAgentProvider(state.agentProvider);
}

function anyAgentProviderConfigured() {
  return AGENT_PROVIDER_ORDER.some((id) => {
    const p = AGENT_PROVIDERS[id];
    return !p.comingSoon && p.isConfigured();
  });
}

function agentAssistantLabel(provider) {
  return getAgentProvider(provider || state.agentProvider).messageLabel;
}

function agentEmptyReplyText() {
  return agentProviderDef().emptyReply();
}

async function finalizeAgentMessage(el, { textBuffer, buffer, summary }) {
  const finalText = pickAgentFinalText(textBuffer, buffer, summary);
  if (!finalText) {
    await window.ReaperAgentMarkdown?.renderAgentContent(el, agentEmptyReplyText());
    return;
  }
  if (!window.ReaperAgentMarkdown?.renderAgentContent) {
    console.error('[Reaper] ReaperAgentMarkdown not loaded — check /vendor/*.js scripts');
    el.textContent = finalText;
    return;
  }
  await window.ReaperAgentMarkdown.renderAgentContent(el, finalText);
}

function appendAgentMessage(role, text, opts = {}) {
  const box = $('#agent-messages');
  const placeholder = box.querySelector('.agent-msg-system.text-center');
  if (placeholder) box.innerHTML = '';

  const provider = opts.provider || state.agentProvider || 'cursor';
  const providerDef = getAgentProvider(provider);
  const wrap = document.createElement('div');
  wrap.className = `rounded-lg px-3 py-2 ${
    role === 'user' ? 'agent-msg-user' :
    role === 'assistant'
      ? `agent-msg-assistant agent-provider-${provider}`
      : 'agent-msg-system'
  }`;
  if (role === 'assistant') {
    wrap.dataset.agentProvider = provider;
  }

  if (role !== 'system') {
    const label = document.createElement('div');
    label.className = `agent-msg-label ${role === 'user' ? '' : providerDef.labelClass}`;
    label.textContent = role === 'user' ? 'You' : providerDef.messageLabel;
    wrap.appendChild(label);
  }

  const content = document.createElement('div');
  content.className = 'agent-text break-words';
  if (role === 'assistant' && window.ReaperAgentMarkdown) {
    window.ReaperAgentMarkdown.renderPlain(content, text);
  } else {
    content.className += ' whitespace-pre-wrap';
    content.textContent = text;
  }
  wrap.appendChild(content);
  if (role === 'assistant') {
    const caret = document.createElement('span');
    caret.className = 'agent-stream-caret';
    caret.setAttribute('aria-hidden', 'true');
    wrap.appendChild(caret);
  }
  wrap.classList.add('agent-msg-enter');
  wrap.addEventListener('animationend', () => wrap.classList.remove('agent-msg-enter'), { once: true });
  box.appendChild(wrap);
  scrollAgentToBottom();
  if (role === 'assistant' && text && text !== '…' && window.ReaperAgentMarkdown) {
    void window.ReaperAgentMarkdown.renderAgentContent(content, text);
  }
  return { wrap, content };
}

async function snapshotAgentWorkspacePaths() {
  if (!state.repo) return new Set();
  try {
    const status = await api(repoApi(state.repo, '/workspace/status'));
    return new Set((status.files || []).map((f) => f.path));
  } catch {
    return new Set();
  }
}

async function collectAgentRevertPaths(beforePaths, afterStatus, seenPaths) {
  const paths = new Set(seenPaths || []);
  for (const f of afterStatus?.files || []) {
    if (!beforePaths.has(f.path)) paths.add(f.path);
  }
  return [...paths];
}

async function restoreAgentWorkspacePaths(paths) {
  if (!state.repo || !paths?.length) return;
  const tracked = [];
  const untracked = [];
  const status = await api(repoApi(state.repo, '/workspace/status'));
  const statusByPath = new Map((status.files || []).map((f) => [f.path, f]));
  for (const path of paths) {
    const entry = statusByPath.get(path);
    if (entry?.status === 'untracked') untracked.push(path);
    else tracked.push(path);
  }
  if (tracked.length) {
    await api(repoApi(state.repo, '/workspace/git'), {
      method: 'POST',
      body: JSON.stringify({ args: ['restore', '--staged', '--worktree', '--', ...tracked] }),
    });
  }
  if (untracked.length) {
    await api(repoApi(state.repo, '/workspace/git'), {
      method: 'POST',
      body: JSON.stringify({ args: ['clean', '-fd', '--', ...untracked] }),
    });
  }
}

function shortAgentActivityPath(path) {
  if (!path) return null;
  const parts = String(path).replace(/\\/g, '/').split('/').filter(Boolean);
  if (!parts.length) return null;
  if (parts.length <= 2) return parts.join('/');
  return parts.slice(-2).join('/');
}

/** Friendly one-liner for the work bar from an SSE tool/text event. */
function formatAgentActivity(data) {
  if (!data || typeof data !== 'object') return null;
  const status = data.status;
  if (status === 'completed' || status === 'error') return null;

  const toolRaw = data.tool || '';
  const tool = toolRaw.toLowerCase();
  const path = shortAgentActivityPath(data.path);
  const text = typeof data.text === 'string' ? data.text.trim() : '';

  if (tool.includes('read') || tool === 'readfile') {
    return path ? `Reading ${path}…` : 'Reading…';
  }
  if (
    tool.includes('write')
    || tool.includes('edit')
    || tool.includes('strreplace')
    || tool.includes('search_replace')
    || tool.includes('apply_patch')
  ) {
    return path ? `Editing ${path}…` : 'Editing…';
  }
  if (tool.includes('shell') || tool.includes('bash') || tool.includes('terminal') || tool.includes('powershell')) {
    return 'Running command…';
  }
  if (tool.includes('grep') || tool.includes('glob') || tool.includes('search') || tool.includes('semsearch')) {
    return path ? `Searching ${path}…` : 'Searching…';
  }
  if (tool.includes('delete')) {
    return path ? `Deleting ${path}…` : 'Deleting…';
  }
  if (tool.includes('task') || tool.includes('agent')) {
    return 'Delegating…';
  }
  if (tool.includes('web') || tool.includes('fetch')) {
    return 'Fetching…';
  }
  if (text.startsWith('…') || /^thinking/i.test(text)) {
    return 'Thinking…';
  }
  if (toolRaw) {
    return path ? `${toolRaw}: ${path}` : `${toolRaw}…`;
  }
  if (text) {
    const cleaned = text.replace(/^[→✓✗]\s*/, '').replace(/\n.*/s, '').trim();
    if (!cleaned) return 'Working…';
    return cleaned.length > 56 ? `${cleaned.slice(0, 55)}…` : cleaned;
  }
  return null;
}

function setAgentActivity(detail) {
  const next = detail && String(detail).trim() ? String(detail).trim() : null;
  if (state.agentActivity === next) return;
  state.agentActivity = next;
  updateAgentUi();
}

function clearAgentActivity() {
  if (state.agentActivity == null) return;
  state.agentActivity = null;
  updateAgentUi();
}

async function stopAgentChat() {
  if (!state.agentBusy || !state.repo) return;
  state.agentStopRequested = true;
  state.agentMessageQueue = [];
  state.agentAbortController?.abort();
  const stopPath = agentProviderDef().stopPath;
  if (stopPath) {
    try {
      await api(repoApi(state.repo, stopPath), { method: 'POST' });
    } catch {
      /* stream abort still stops the UI */
    }
  }
  updateAgentUi();
}

async function revertAgentMessage() {
  const turn = state.agentLastRevertibleTurn;
  if (!turn || state.agentBusy || !state.repo) return;
  if (!confirm('Revert the last agent message and undo its file changes?')) return;

  try {
    if (turn.paths?.length) {
      await restoreAgentWorkspacePaths(turn.paths);
    }
    removeAgentMessageWrap(turn.assistantWrap);
    removeAgentMessageWrap(turn.userWrap);
    state.agentLastRevertibleTurn = null;
    await refreshAfterAgent({ final: false });
    toast('Reverted last agent message', 'success');
  } catch (e) {
    toast(e.message || 'Could not revert agent changes', 'error');
  }
  updateAgentUi();
}

function removeAgentMessageWrap(wrap) {
  wrap?.remove();
  const box = $('#agent-messages');
  if (box && !box.querySelector('.agent-msg-user, .agent-msg-assistant')) {
    setAgentWelcomeMessage();
  }
}

function currentAgentModel() {
  return agentProviderDef().currentModel();
}

function agentWelcomeText() {
  return agentProviderDef().welcome();
}

function setAgentWelcomeMessage() {
  const el = $('#agent-welcome-msg');
  if (el) {
    el.textContent = agentWelcomeText();
    return;
  }
  const box = $('#agent-messages');
  if (box && !box.querySelector('.agent-msg-user, .agent-msg-assistant')) {
    box.innerHTML = `<div id="agent-welcome-msg" class="agent-msg-system text-center py-4 px-2">${escapeHtml(agentWelcomeText())}</div>`;
  }
}

function fillAgentModelSelect(select, models, currentId) {
  select.innerHTML = '';
  const ids = new Set();
  const supported = (models || []).filter((m) => m?.id);
  for (const m of supported) {
    ids.add(m.id);
    const opt = document.createElement('option');
    opt.value = m.id;
    opt.textContent = m.label || m.id;
    if (m.description) opt.title = m.description;
    select.appendChild(opt);
  }
  const resolved = ids.has(currentId) ? currentId : (supported[0]?.id || '');
  if (resolved) select.value = resolved;
}

function agentProviderChipStatus(def) {
  if (def.comingSoon) return 'soon';
  if (!def.isConfigured()) return 'needs-key';
  if (!def.isReady()) return 'waiting';
  return 'ready';
}

function agentProviderChipTitle(def) {
  if (def.comingSoon) return `${def.label} agent — coming soon`;
  if (!def.isConfigured()) return `Configure ${def.label} in Settings`;
  if (!def.isReady()) return def.notReadyHint?.() || `${def.label} — not ready`;
  const caps = def.capabilities;
  if (caps.readOnly) return `${def.label} agent — read-only Q&A`;
  return `${def.label} agent — edit files and run tools`;
}

function renderAgentProviderPicker() {
  const picker = $('#agent-provider-picker');
  if (!picker) return;
  picker.innerHTML = '';
  for (const id of AGENT_PROVIDER_ORDER) {
    const def = AGENT_PROVIDERS[id];
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.dataset.agentProvider = id;
    chip.className = 'agent-provider-chip';
    chip.setAttribute('role', 'tab');
    chip.setAttribute('aria-selected', id === state.agentProvider ? 'true' : 'false');
    chip.dataset.status = agentProviderChipStatus(def);
    chip.title = agentProviderChipTitle(def);

    const dot = document.createElement('span');
    dot.className = 'agent-provider-status-dot';
    dot.setAttribute('aria-hidden', 'true');
    chip.appendChild(dot);

    const name = document.createElement('span');
    name.className = 'agent-provider-chip-label';
    name.textContent = def.label;
    chip.appendChild(name);

    if (def.comingSoon) {
      const badge = document.createElement('span');
      badge.className = 'agent-provider-soon';
      badge.textContent = 'Soon';
      chip.appendChild(badge);
    }

    chip.classList.toggle('active', id === state.agentProvider);
    picker.appendChild(chip);
  }
}

function renderAgentProviderControls() {
  const container = $('#agent-provider-controls');
  if (!container) return;
  const def = agentProviderDef();
  container.innerHTML = '';
  container.dataset.provider = def.id;

  if (def.comingSoon) {
    const hint = document.createElement('p');
    hint.className = 'text-[10px] text-gray-600 leading-snug';
    hint.textContent = 'Coming soon — Anthropic API key support is on the way.';
    container.appendChild(hint);
    return;
  }

  if (def.capabilities.modes) {
    const modeBar = document.createElement('div');
    modeBar.className = 'flex items-center border border-surface-700 rounded-md overflow-hidden shrink-0';
    modeBar.title = 'Conversation mode';
    for (const [idx, mode] of ['agent', 'plan', 'ask'].entries()) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.dataset.agentMode = mode;
      btn.className = `agent-mode-btn px-2 py-0.5 text-[10px] text-gray-500 hover:text-accent hover:bg-surface-800 transition-colors${idx ? ' border-l border-surface-700' : ''}`;
      btn.title = mode === 'agent'
        ? 'Agent — edit files and run tools'
        : mode === 'plan'
          ? 'Plan — design before implementing'
          : 'Ask — read-only Q&A';
      btn.textContent = mode === 'agent' ? 'Agent' : mode === 'plan' ? 'Plan' : 'Ask';
      btn.classList.toggle('active', state.cursorMode === mode);
      modeBar.appendChild(btn);
    }
    container.appendChild(modeBar);
  }

  if (def.capabilities.readOnly) {
    const hint = document.createElement('p');
    hint.className = 'text-[10px] text-gray-600 shrink-0';
    hint.textContent = 'Read-only chat';
    container.appendChild(hint);
  }

  const models = def.models();
  if (models.length) {
    const select = document.createElement('select');
    select.id = 'agent-provider-model';
    select.className = 'ij-theme-menu-select flex-1 min-w-0 text-[11px]';
    select.title = `${def.label} model`;
    const current = def.currentModel();
    fillAgentModelSelect(select, models, current);
    if (def.id === 'cursor') state.cursorModel = select.value;
    else if (def.id === 'gemini') state.geminiModel = select.value;
    else if (def.id === 'anthropic') {
      if (state.anthropicBackend === 'bedrock') state.bedrockModelId = select.value;
      else state.anthropicModel = select.value;
    }
    container.appendChild(select);
  } else if (def.id === 'cursor' && state.cursorConfigured && !state.cursorModelsLoaded) {
    const hint = document.createElement('p');
    hint.className = 'text-[10px] text-gray-600 leading-snug';
    hint.textContent = 'Loading models for your API key…';
    container.appendChild(hint);
  } else if (def.id === 'cursor' && state.cursorConfigured && state.cursorModelsLoaded) {
    const hint = document.createElement('p');
    hint.className = 'text-[10px] text-red-400 leading-snug';
    hint.textContent = cursorModelStatusError() || 'No Cursor models available for this API key.';
    container.appendChild(hint);
  }
}

function ensureAgentProviderAvailable() {
  const current = getAgentProvider(state.agentProvider);
  if (!current.comingSoon && current.isConfigured()) return;
  const fallback = AGENT_PROVIDER_ORDER.find((id) => {
    const p = AGENT_PROVIDERS[id];
    return !p.comingSoon && p.isConfigured();
  });
  if (fallback) state.agentProvider = fallback;
  else if (current.comingSoon || !AGENT_PROVIDERS[state.agentProvider]) {
    state.agentProvider = AGENT_PROVIDER_ORDER[0];
  }
}

function refreshAgentProviderUi() {
  ensureAgentProviderAvailable();
  localStorage.setItem(AGENT_PROVIDER_KEY, state.agentProvider);
  renderAgentProviderPicker();
  renderAgentProviderControls();

  const def = agentProviderDef();
  const input = $('#agent-input');
  if (input) input.placeholder = def.placeholder;

  setAgentWelcomeMessage();
}

function agentCanChat() {
  if (!state.repo) return false;
  const def = agentProviderDef();
  if (def.comingSoon) return false;
  if (!def.isReady()) return false;
  if (def.id === 'cursor' && cursorModelStatusError()) return false;
  return true;
}

async function setAgentProvider(provider) {
  const def = getAgentProvider(provider);
  if (!def || provider === state.agentProvider) return;
  if (def.comingSoon) {
    toast('Claude agent is coming soon', 'info');
    return;
  }
  if (!def.isConfigured()) {
    toast(`Add a ${def.label} API key in Settings`, 'error');
    showSettingsModal(def.settingsTab);
    return;
  }
  state.agentProvider = provider;
  refreshAgentProviderUi();
  updateAgentUi();
  if (provider === 'bedrock' && !state.bedrockModels.length) {
    void refreshBedrockModels({ silent: true });
  }
}

function updateAgentUi() {
  const prevProvider = $('#agent-provider-controls')?.dataset.provider;
  ensureAgentProviderAvailable();
  localStorage.setItem(AGENT_PROVIDER_KEY, state.agentProvider);
  renderAgentProviderPicker();
  if (state.agentProvider !== prevProvider) {
    renderAgentProviderControls();
    setAgentWelcomeMessage();
  }

  const def = agentProviderDef();
  const input = $('#agent-input');
  if (input) input.placeholder = def.placeholder;

  const canChat = agentCanChat();
  $('#agent-input').disabled = !canChat;
  $('#btn-agent-send').disabled = !canChat;

  const modelEl = $('#agent-provider-model');
  if (modelEl) {
    modelEl.disabled = !def.isConfigured() || def.comingSoon;
  }

  let status = 'Ready';
  if (!anyAgentProviderConfigured()) {
    status = 'API key not configured';
  } else if (def.comingSoon) {
    status = def.statusText();
  } else if (!def.isConfigured()) {
    status = `${def.label} API key not configured`;
  } else if (!def.isReady()) {
    status = def.notReadyHint?.() || 'Not ready';
  } else if (state.agentBusy) {
    const queued = state.agentMessageQueue.length;
    const detail = state.agentActivity;
    status = detail
      ? (queued ? `${detail} · ${queued} queued` : detail)
      : (queued ? `Working… · ${queued} queued` : 'Working…');
  } else if (!state.repo) {
    status = 'Select a repo';
  } else if (def.id === 'cursor') {
    const modelErr = cursorModelStatusError();
    status = modelErr || def.statusText();
  } else {
    status = def.statusText();
  }
  $('#agent-status').textContent = status;
  $('#btn-agent-retry')?.classList.toggle(
    'hidden',
    def.id !== 'cursor' || state.cursorBridgeOk || !state.cursorConfigured,
  );
  $('#agent-config-banner')?.classList.toggle('hidden', anyAgentProviderConfigured());

  const workBar = $('#agent-work-status');
  const workText = $('#agent-work-status-text');
  if (workBar && workText) {
    const queued = state.agentMessageQueue.length;
    const showWorkBar = state.agentBusy || queued > 0;
    workBar.classList.toggle('hidden', !showWorkBar);
    workBar.classList.toggle('is-working', state.agentBusy);
    if (state.agentBusy && queued > 0) {
      workText.textContent = state.agentActivity
        ? `${state.agentActivity} · ${queued} queued`
        : `Working… · ${queued} queued`;
    } else if (state.agentBusy) {
      workText.textContent = state.agentActivity || 'Working…';
    } else if (queued > 0) {
      workText.textContent = `${queued} queued…`;
    }
  }

  let hint;
  if (!anyAgentProviderConfigured()) {
    hint = 'Configure an agent in Settings (⌘,)';
  } else if (def.comingSoon || !def.isConfigured()) {
    hint = def.notConfiguredHint;
  } else if (!state.repo) {
    hint = 'Select a repo to chat';
  } else if (!def.isReady()) {
    hint = def.notReadyHint?.() || def.notConfiguredHint;
  } else if (state.agentBusy) {
    hint = 'Enter to queue another message · Shift+Enter for newline';
  } else {
    hint = def.hintWhenReady;
  }
  $('#agent-hint').textContent = hint;

  $$('[data-agent-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.agentDock === state.agentDock);
  });

  const agentBadge = $('#agent-badge');
  agentBadge?.classList.toggle('hidden', !state.agentBusy);

  const stopBtn = $('#btn-agent-stop');
  if (stopBtn) stopBtn.disabled = !canChat || !state.agentBusy;
  const revertBtn = $('#btn-agent-revert');
  if (revertBtn) {
    revertBtn.classList.toggle('hidden', !def.capabilities.revert);
    revertBtn.disabled = !canChat || state.agentBusy || !state.agentLastRevertibleTurn;
  }
}

async function loadCursorModels() {
  if (!state.cursorConfigured) {
    state.cursorModels = [];
    state.cursorModelsLoaded = false;
    state.cursorModelsError = null;
    if (state.agentProvider === 'cursor') renderAgentProviderControls();
    return;
  }
  state.cursorModelsLoaded = false;
  state.cursorModelsError = null;
  if (state.agentProvider === 'cursor') renderAgentProviderControls();
  try {
    const data = await api('/api/cursor/models');
    state.cursorModels = (data.models || []).filter((m) => m?.id);
    state.cursorModelsLoaded = true;
    state.cursorModelsError = null;
    if (data.current_model && cursorModelIsSupported(data.current_model)) {
      state.cursorModel = data.current_model;
    } else {
      await reconcileCursorModelSelection();
    }
  } catch (err) {
    state.cursorModels = [];
    state.cursorModelsLoaded = true;
    state.cursorModelsError = err.message || String(err);
  }
  if (state.agentProvider === 'cursor') renderAgentProviderControls();
  updateAgentUi();
}

async function setAgentMode(mode) {
  if (!['agent', 'plan', 'ask'].includes(mode) || mode === state.cursorMode) return;
  state.cursorMode = mode;
  renderAgentProviderControls();
  updateAgentUi();
  try {
    const cfg = await api('/api/settings/cursor/mode', {
      method: 'PATCH',
      body: JSON.stringify({ mode }),
    });
    state.cursorMode = cfg.mode || mode;
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function setCursorAgentModel(modelId) {
  if (!modelId || modelId === state.cursorModel) return;
  if (state.cursorModelsLoaded && !cursorModelIsSupported(modelId)) {
    toast(`Model "${modelId}" isn't available for your Cursor API key.`, 'error');
    renderAgentProviderControls();
    updateAgentUi();
    return;
  }
  state.cursorModel = modelId;
  updateAgentUi();
  try {
    const cfg = await api('/api/settings/cursor/model', {
      method: 'PATCH',
      body: JSON.stringify({ model: modelId }),
    });
    state.cursorModel = cfg.model || modelId;
  } catch (err) {
    toast(err.message, 'error');
    renderAgentProviderControls();
    updateAgentUi();
  }
}

async function setGeminiAgentModel(modelId) {
  if (!modelId || modelId === state.geminiModel) return;
  state.geminiModel = modelId;
  updateAgentUi();
  try {
    const cfg = await api('/api/settings/gemini/model', {
      method: 'PATCH',
      body: JSON.stringify({ model: modelId }),
    });
    state.geminiModel = cfg.model || modelId;
  } catch (err) {
    toast(err.message, 'error');
  }
}

function showAgentKeyForm() {
  showSettingsModal(agentProviderDef().settingsTab);
}

async function loadCursorStatus() {
  try {
    const cfg = await api('/api/cursor/status');
    state.cursorConfigured = cfg.configured;
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    state.cursorKeyMasked = cfg.masked || null;
    state.cursorKeySource = cfg.source || null;
    state.cursorModel = cfg.model || CURSOR_MODEL_DEFAULT;
    state.cursorMode = cfg.mode || 'agent';
    $$('[data-agent-mode]').forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.agentMode === state.cursorMode);
    });
    await loadCursorModels();
    state.cursorModel = normalizeCursorModel(state.cursorModel);
    refreshAgentProviderUi();
    if (state.repo && cfg.configured && cfg.bridge_ok) void warmCursorSession();
  } catch {
    state.cursorConfigured = false;
    state.cursorBridgeOk = false;
    state.cursorBridgeError = null;
    state.cursorKeyMasked = null;
    state.cursorKeySource = null;
    refreshAgentProviderUi();
  }
  updateAgentUi();
}

async function restartBridge() {
  try {
    const cfg = await api('/api/cursor/bridge/restart', { method: 'POST' });
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    updateAgentUi();
    if ($('#settings-modal-overlay')?.classList.contains('flex')) {
      await loadCursorSettingsSection();
    }
    if (cfg.bridge_ok) await loadCursorModels();
    toast(cfg.bridge_ok ? 'Bridge connected' : (cfg.bridge_error || 'Bridge still offline'), cfg.bridge_ok ? 'success' : 'error');
    if (cfg.bridge_ok && state.repo) await warmCursorSession(state.repo);
  } catch (err) {
    toast(err.message, 'error');
  }
}

function terminalBottomHeightLimits() {
  const max = Math.min(Math.round(window.innerHeight * 0.7), 720);
  return { min: 220, max: Math.max(220, max) };
}

function applyTerminalBottomHeight(px) {
  const dock = $('#terminal-dock-bottom');
  if (!dock || !Number.isFinite(px)) return;
  const { min, max } = terminalBottomHeightLimits();
  const clamped = Math.min(max, Math.max(min, Math.round(px)));
  dock.style.setProperty('--ij-terminal-bottom-h', `${clamped}px`);
  return clamped;
}

function initTerminalBottomResize() {
  const saved = parseInt(localStorage.getItem(TERMINAL_BOTTOM_HEIGHT_KEY), 10);
  if (Number.isFinite(saved)) applyTerminalBottomHeight(saved);

  const handle = $('#terminal-bottom-resize');
  if (!handle) return;

  let dragging = false;
  let startY = 0;
  let startH = 0;

  const stopDrag = () => {
    if (!dragging) return;
    dragging = false;
    handle.classList.remove('active');
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    const h = applyTerminalBottomHeight($('#terminal-dock-bottom')?.getBoundingClientRect().height);
    if (h) localStorage.setItem(TERMINAL_BOTTOM_HEIGHT_KEY, String(h));
    fitActiveTerminal();
  };

  handle.addEventListener('mousedown', (e) => {
    if (e.button !== 0) return;
    dragging = true;
    startY = e.clientY;
    startH = $('#terminal-dock-bottom')?.getBoundingClientRect().height || 0;
    handle.classList.add('active');
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    e.preventDefault();
  });

  window.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    applyTerminalBottomHeight(startH + (startY - e.clientY));
  });

  window.addEventListener('mouseup', stopDrag);
  window.addEventListener('blur', stopDrag);
}

function setTerminalDock(dock) {
  if (!['left', 'right', 'bottom'].includes(dock)) return;
  state.terminalDock = dock;
  localStorage.setItem(TERMINAL_DOCK_KEY, dock);
  applyTerminalDock();
  if (dock !== 'left') {
    state.terminalOpen = true;
    openTerminal();
  } else if (state.activePanel === 'terminal') {
    switchPanel('terminal');
  }
}

function applyTerminalDock() {
  const panel = $('#panel-terminal');
  const sidebar = $('#sidebar');
  const rightDock = $('#terminal-dock-right');
  const bottomDock = $('#terminal-dock-bottom');
  if (!panel || !sidebar || !rightDock || !bottomDock) return;

  const dock = state.terminalDock;
  const showTerminal = dock === 'left' ? state.activePanel === 'terminal' : state.terminalOpen;

  if (dock === 'left') {
    sidebar.appendChild(panel);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
    bottomDock.classList.add('hidden');
    bottomDock.classList.remove('flex');
  } else if (dock === 'right') {
    rightDock.appendChild(panel);
    rightDock.classList.toggle('hidden', !showTerminal);
    rightDock.classList.toggle('flex', showTerminal);
    bottomDock.classList.add('hidden');
    bottomDock.classList.remove('flex');
  } else {
    bottomDock.appendChild(panel);
    bottomDock.classList.toggle('hidden', !showTerminal);
    bottomDock.classList.toggle('flex', showTerminal);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
  }

  panel.classList.toggle('hidden', !showTerminal);
  panel.classList.toggle('flex', showTerminal);

  if (!showTerminal) {
    state.terminals.forEach((t) => destroyTerminalInstance(t));
  }

  $$('[data-terminal-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.terminalDock === dock);
  });
  syncDockMenuControls();

  syncActivityButtons();
  updateStatusBar();
  if (showTerminal && !state.terminalMountSync) {
    const term = getActiveTerminal();
    const xtermMissing = !term?.xterm || !term.xterm.element?.isConnected;
    if (xtermMissing) {
      mountActiveTerminal();
    } else {
      requestAnimationFrame(() => {
        fitActiveTerminal();
        requestAnimationFrame(() => fitActiveTerminal());
      });
    }
  }
}

function showTerminal() {
  if (state.terminalDock === 'left') {
    switchPanel('terminal');
    return;
  }
  openTerminal();
}

function openTerminal() {
  if (state.terminalDock === 'left') {
    switchPanel('terminal');
    return;
  }
  const wasOpen = state.terminalOpen;
  state.terminalOpen = true;
  applyTerminalDock();
  // Do not remount fresh on every Run/Debug click. A delayed rAF remount races
  // with ensureCommandTerminalReady and tears down the xterm mid-start, which
  // made the first click look like a no-op and required a second press.
  const term = getActiveTerminal();
  const needsFresh = !term?.xterm || !term.xterm.element?.isConnected;
  if (!wasOpen || needsFresh) {
    mountActiveTerminal({ fresh: needsFresh });
  } else {
    mountActiveTerminal({ fresh: false });
  }
}

function toggleTerminal() {
  if (state.terminalDock === 'left') {
    switchPanel(state.activePanel === 'terminal' ? 'explorer' : 'terminal');
    return;
  }
  const wasOpen = state.terminalOpen;
  state.terminalOpen = !state.terminalOpen;
  applyTerminalDock();
  if (state.terminalOpen) {
    mountActiveTerminal({ fresh: true });
  }
}

function setAgentDock(dock) {
  if (!['left', 'right', 'bottom'].includes(dock)) return;
  state.agentDock = dock;
  localStorage.setItem(AGENT_DOCK_KEY, dock);
  applyAgentDock();
  if (dock !== 'left') {
    state.agentOpen = true;
    openAgent();
  }
}

function applyAgentDock() {
  const panel = $('#panel-agent');
  const sidebar = $('#sidebar');
  const rightDock = $('#agent-dock-right');
  const bottomDock = $('#agent-dock-bottom');
  const rightResizer = $('#agent-right-resizer');
  if (!panel || !sidebar || !rightDock || !bottomDock) return;

  const dock = state.agentDock;
  const showAgent = dock === 'left' ? state.activePanel === 'agent' : state.agentOpen;

  if (rightResizer) {
    const showRightResize = dock === 'right' && showAgent;
    rightResizer.classList.toggle('hidden', !showRightResize);
  }

  if (dock === 'left') {
    sidebar.appendChild(panel);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
    bottomDock.classList.add('hidden');
    bottomDock.classList.remove('flex');
  } else if (dock === 'right') {
    rightDock.appendChild(panel);
    rightDock.classList.toggle('hidden', !showAgent);
    rightDock.classList.toggle('flex', showAgent);
    bottomDock.classList.add('hidden');
    bottomDock.classList.remove('flex');
  } else {
    bottomDock.appendChild(panel);
    bottomDock.classList.toggle('hidden', !showAgent);
    bottomDock.classList.toggle('flex', showAgent);
    rightDock.classList.add('hidden');
    rightDock.classList.remove('flex');
  }

  panel.classList.toggle('hidden', !showAgent);
  panel.classList.toggle('flex', showAgent);

  syncActivityButtons();
  updateAgentUi();
  syncDockMenuControls();
  updateStatusBar();
  updateMenuState();
}

function openAgent() {
  if (state.agentDock === 'left') {
    switchPanel('agent');
    return;
  }
  state.agentOpen = true;
  applyAgentDock();
  loadCursorStatus();
  setTimeout(() => $('#agent-input')?.focus(), 50);
}

function toggleAgent() {
  if (state.agentDock === 'left') {
    switchPanel(state.activePanel === 'agent' ? 'explorer' : 'agent');
    return;
  }
  state.agentOpen = !state.agentOpen;
  applyAgentDock();
  if (state.agentOpen) {
    loadCursorStatus();
    setTimeout(() => $('#agent-input')?.focus(), 50);
  }
}

function closeAgent() {
  if (state.agentDock === 'left') {
    if (state.activePanel === 'agent') switchPanel('explorer');
    return;
  }
  if (!state.agentOpen) return;
  state.agentOpen = false;
  applyAgentDock();
}

async function clearAgentSession() {
  if (state.repo) {
    try {
      await Promise.all([
        api(repoApi(state.repo, '/cursor/session'), { method: 'DELETE' }).catch(() => {}),
        api(repoApi(state.repo, '/gemini/session'), { method: 'DELETE' }).catch(() => {}),
      ]);
    } catch { /* ignore */ }
  }
  setAgentWelcomeMessage();
  state.agentMessageQueue = [];
  state.agentLastRevertibleTurn = null;
  updateAgentUi();
}

async function sendAgentMessage() {
  const prompt = $('#agent-input').value.trim();
  if (!prompt || !state.repo) return;
  if (!agentCanChat()) return;

  $('#agent-input').value = '';

  if (state.agentBusy) {
    state.agentMessageQueue.push(prompt);
    appendAgentMessage('user', prompt);
    updateAgentUi();
    return;
  }

  await runAgentChat(prompt);
}

function drainAgentMessageQueue() {
  if (state.agentBusy || !state.agentMessageQueue.length) return;
  const next = state.agentMessageQueue.shift();
  updateAgentUi();
  void runAgentChat(next, { skipUserBubble: true });
}

async function runAgentChat(prompt, opts = {}) {
  if (state.agentBusy) {
    state.agentMessageQueue.push(prompt);
    return;
  }

  let userWrap = null;
  const def = agentProviderDef();
  const chatProvider = def.id;
  if (!opts.skipUserBubble) {
    ({ wrap: userWrap } = appendAgentMessage('user', prompt, { provider: chatProvider }));
  }
  state.agentBusy = true;
  state.agentStopRequested = false;
  state.agentActivity = 'Connecting…';
  state.agentAbortController = new AbortController();
  state.agentLiveFollow = !!def.capabilities.liveFollow && state.cursorMode === 'agent';
  state.agentLiveDiffPath = null;
  state.agentLastToolPath = null;
  state.agentSeenPaths = new Set();
  state.agentHadFileChanges = false;
  const needsRevertSnapshot = def.capabilities.tools && state.cursorMode === 'agent';
  const pathsBeforePromise = needsRevertSnapshot
    ? snapshotAgentWorkspacePaths()
    : Promise.resolve(new Set());
  updateAgentUi();
  if (state.agentLiveFollow) showAgentDiffPlaceholder();

  const { wrap: assistantWrap, content: assistantEl } = appendAgentMessage('assistant', '…', { provider: chatProvider });
  assistantWrap.classList.add('is-streaming', 'is-waiting');
  let buffer = '';
  let textBuffer = '';
  let doneSummary = null;
  let cancelled = false;

  try {
    if (def.id === 'cursor') {
      const modelErr = cursorModelStatusError();
      if (modelErr) throw new Error(modelErr);
      setAgentActivity('Connecting…');
      // Don't block chat forever if warm hangs while the bridge boots.
      await Promise.race([
        warmCursorSession(state.repo),
        new Promise((resolve) => setTimeout(resolve, 12_000)),
      ]);
    }

    setAgentActivity('Waiting for reply…');
    const chatUrl = repoApi(state.repo, def.chatPath);
    const res = await fetch(chatUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(def.chatBody(prompt)),
      signal: state.agentAbortController.signal,
    });

    if (!res.ok) {
      let errMsg = res.statusText;
      try {
        const err = await res.json();
        errMsg = err.error || errMsg;
      } catch { /* ignore */ }
      throw new Error(errMsg);
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let sseBuffer = '';
    window.ReaperAgentMarkdown?.renderPlain(assistantEl, '');
    setAgentActivity('Thinking…');

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      sseBuffer += decoder.decode(value, { stream: true });
      const parts = sseBuffer.split('\n\n');
      sseBuffer = parts.pop() || '';
      for (const part of parts) {
        const line = part.split('\n').find((l) => l.startsWith('data: '));
        if (!line) continue;
        const data = JSON.parse(line.slice(6));
        if (data.type === 'text') {
          buffer += data.text;
          textBuffer += data.text;
          assistantWrap.classList.remove('is-waiting');
          setAgentActivity('Writing reply…');
          window.ReaperAgentMarkdown?.renderPlain(assistantEl, textBuffer);
          scheduleAgentMarkdownPreview(assistantEl, textBuffer);
          scrollAgentToBottom();
        } else if (data.type === 'tool') {
          buffer += data.text;
          assistantWrap.classList.remove('is-waiting');
          const activity = formatAgentActivity(data);
          if (activity) setAgentActivity(activity);
          window.ReaperAgentMarkdown?.renderPlain(assistantEl, buffer);
          scrollAgentToBottom();
          scheduleAgentWorkspaceRefresh(data.path || null);
        } else if (data.type === 'error') {
          throw new Error(data.error);
        } else if (data.type === 'done') {
          doneSummary = data.summary || null;
          if (data.status === 'cancelled') {
            cancelled = true;
            buffer = buffer || 'Stopped.';
            textBuffer = textBuffer || buffer;
          } else if (data.status === 'error') {
            const errText = data.error || data.summary || 'Agent run failed';
            if (!buffer) throw new Error(errText);
          } else if (!buffer && data.status === 'finished') {
            buffer = def.capabilities.tools
              ? 'Done — check Source Control for file changes, or reopen files in the editor.'
              : def.emptyReply();
            textBuffer = buffer;
          } else if (!buffer && data.status === 'error') {
            throw new Error(data.error || data.summary || 'Agent run failed');
          }
        }
      }
    }

    clearTimeout(agentRefreshTimer);
    clearTimeout(agentMarkdownTimer);
    if (cancelled || state.agentStopRequested) {
      assistantWrap.classList.add('agent-msg-muted');
      window.ReaperAgentMarkdown?.renderPlain(assistantEl, buffer || 'Stopped.');
    } else {
      await finalizeAgentMessage(assistantEl, { textBuffer, buffer, summary: doneSummary });
    }
    if (needsRevertSnapshot) {
      const postStatus = await api(repoApi(state.repo, '/workspace/status'));
      const pathsBefore = await pathsBeforePromise;
      const revertPaths = await collectAgentRevertPaths(pathsBefore, postStatus, state.agentSeenPaths);
      if (revertPaths.length || userWrap || assistantWrap) {
        state.agentLastRevertibleTurn = {
          userWrap,
          assistantWrap,
          paths: revertPaths,
        };
      }
    }
    const lightRefresh = state.cursorMode === 'ask' || !state.agentHadFileChanges;
    await refreshAfterAgent({
      fromAgent: true,
      final: !cancelled && !state.agentStopRequested,
      light: lightRefresh,
    });
  } catch (e) {
    if (e.name === 'AbortError' || state.agentStopRequested) {
      assistantWrap.classList.add('agent-msg-muted');
      window.ReaperAgentMarkdown?.renderPlain(assistantEl, buffer || 'Stopped.');
    } else {
      const msg = e.message || String(e);
      if (/invalid api key/i.test(msg)) {
        showAgentKeyForm();
        toast('Invalid API key — paste a new one from Cursor → Integrations', 'error');
      } else {
        toast(msg, 'error');
      }
      assistantWrap.classList.add('text-red-400');
      window.ReaperAgentMarkdown?.renderPlain(assistantEl, msg);
    }
  } finally {
    assistantWrap.classList.remove('is-streaming', 'is-waiting');
    clearAgentActivity();
    state.agentBusy = false;
    state.agentStopRequested = false;
    state.agentAbortController = null;
    setAgentDiffLive(false);
    updateAgentUi();
    drainAgentMessageQueue();
  }
}
function syncActivityButtons() {
  $$('.activity-btn[data-panel]').forEach((b) => {
    const panel = b.dataset.panel;
    let sidebarActive = false;
    let floatingOpen = false;

    if (panel === 'agent') {
      sidebarActive = state.agentDock === 'left' && state.activePanel === 'agent';
      floatingOpen = state.agentDock !== 'left' && state.agentOpen;
    } else if (panel === 'terminal') {
      sidebarActive = state.terminalDock === 'left' && state.activePanel === 'terminal';
      floatingOpen = state.terminalDock !== 'left' && state.terminalOpen;
    } else {
      sidebarActive = state.activePanel === panel;
    }

    b.classList.toggle('active', sidebarActive);
    b.classList.toggle('floating-open', floatingOpen);
  });
}

function switchPanel(name) {
  if (name === 'agent' && state.agentDock !== 'left') {
    openAgent();
    return;
  }
  if (name === 'terminal' && state.terminalDock !== 'left') {
    openTerminal();
    return;
  }
  if (name === 'docker-logs' && state.dockerLogsDock !== 'left') {
    openDockerLogsPanel();
    return;
  }

  const panelChanged = state.activePanel !== name;
  state.activePanel = name;
  syncActivityButtons();
  const titles = {
    explorer: 'Project',
    structure: 'Structure',
    git: 'Commit',
    history: 'Git Log',
    terminal: 'Terminal',
    agent: 'Agent',
    'docker-logs': 'Docker',
  };
  $('#sidebar-title').textContent = titles[name] || name;
  $$('#sidebar > .panel').forEach((p) => {
    if (p.id === 'panel-agent' || p.id === 'panel-terminal' || p.id === 'panel-docker-logs') return;
    p.classList.toggle('hidden', p.id !== `panel-${name}`);
  });
  applyAgentDock();
  applyTerminalDock();
  applyDockerLogsDock();
  if (name === 'git') refreshGitStatus();
  else if (name === 'history') refreshHistory();
  else if (name === 'structure') {
    syncStructureModeButtons();
    void refreshStructurePanel({ force: true });
  }
  if (name === 'agent') {
    loadCursorStatus();
    setTimeout(() => $('#agent-input')?.focus(), 50);
  }
  if (name === 'terminal') {
    mountActiveTerminal({ fresh: panelChanged && !getActiveTerminal()?.xterm });
  }
}

function showModal() {
  $('#modal-overlay').classList.remove('hidden');
  $('#modal-overlay').classList.add('flex');
  setTimeout(() => $('#new-repo-name')?.focus(), 0);
}

function hideModal() {
  $('#modal-overlay').classList.add('hidden');
  $('#modal-overlay').classList.remove('flex');
}

function isFormField(el) {
  return el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
}

function installFormClipboardShortcuts() {
  document.addEventListener('keydown', async (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
    const el = document.activeElement;
    if (!isFormField(el) || el.disabled || el.readOnly) return;

    const key = e.key.toLowerCase();
    if (key === 'a') {
      e.preventDefault();
      el.select();
      return;
    }

    const start = el.selectionStart;
    const end = el.selectionEnd;
    if (start === null || end === null) return;

    if (key === 'c' || key === 'x') {
      if (start === end) return;
      try {
        await navigator.clipboard.writeText(el.value.slice(start, end));
        if (key === 'x') {
          el.setRangeText('', start, end, 'start');
          el.dispatchEvent(new Event('input', { bubbles: true }));
        }
        e.preventDefault();
      } catch { /* fall back to native Edit menu */ }
    }
  }, true);
}

function isTerminalFocused() {
  const term = getActiveTerminal();
  const pane = term?.container;
  if (!pane) return false;
  if (pane.querySelector('.xterm.focus')) return true;
  const active = document.activeElement;
  return !!(active && pane.contains(active));
}

function installTerminalClipboard() {
  const pasteIntoActiveTerminal = (text) => {
    const term = getActiveTerminal();
    if (!term?.xterm || !text) return;
    term.xterm.paste(text);
  };

  document.addEventListener('keydown', async (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'v' || e.altKey) return;
    if (!isTerminalFocused()) return;
    try {
      const text = await navigator.clipboard.readText();
      if (!text) return;
      e.preventDefault();
      e.stopPropagation();
      pasteIntoActiveTerminal(text);
    } catch { /* fall back to native */ }
  }, true);

  $('#terminal-xterm-host')?.addEventListener('paste', (e) => {
    if (!isTerminalFocused()) return;
    const text = e.clipboardData?.getData('text/plain');
    if (!text) return;
    e.preventDefault();
    pasteIntoActiveTerminal(text);
  });
}

// --- Init ---
function bindEvents() {
  installFormClipboardShortcuts();
  installTerminalClipboard();
  $('#toast .ij-toast-dismiss')?.addEventListener('click', dismissToast);
  $('#status-diagnostics')?.addEventListener('click', jumpToNextDiagnostic);
  $('#status-coverage')?.addEventListener('click', () => toggleCoveragePanel());
  $('#tb-coverage-inline')?.addEventListener('click', () => toggleCoverageInline());
  $('#tb-blame')?.addEventListener('click', () => toggleBlameInline());
  $('#btn-git-rebase')?.addEventListener('click', () => showRebasePanel());
  $('#btn-rebase-load')?.addEventListener('click', () => loadRebasePlan());
  $('#btn-rebase-start')?.addEventListener('click', () => startInteractiveRebase());
  $('#btn-rebase-cancel')?.addEventListener('click', () => hideRebasePanel());
  $('#publish-host')?.addEventListener('change', () => syncPublishHostUi());
  $('#btn-coverage-toggle-inline')?.addEventListener('click', () => toggleCoverageInline());
  $('#btn-coverage-close')?.addEventListener('click', hideCoveragePanel);
  $('#btn-coverage-refresh')?.addEventListener('click', () => void refreshCoveragePanel(state.activeTab));
  $('#btn-coverage-run')?.addEventListener('click', () => void runActiveFileWithCoverage());
  $('#btn-coverage-open-html')?.addEventListener('click', () => void openCoverageHtmlReport());
  $('#btn-debug-close')?.addEventListener('click', hideDebugPanel);
  $('#tb-debug')?.addEventListener('click', () => void startDebugSession());
  $('#tb-debug-continue')?.addEventListener('click', () => void debugContinue());
  $('#tb-debug-step-over')?.addEventListener('click', () => void debugStep('over'));
  $('#tb-debug-step-in')?.addEventListener('click', () => void debugStep('in'));
  $('#tb-debug-step-out')?.addEventListener('click', () => void debugStep('out'));
  $('#tb-debug-stop')?.addEventListener('click', () => void stopDebugSession());
  $('#debug-watch-form')?.addEventListener('submit', (e) => {
    e.preventDefault();
    const input = $('#debug-watch-input');
    const expr = input?.value || '';
    if (input) input.value = '';
    void addDebugWatch(expr);
  });
  $('#btn-db-viewer-close')?.addEventListener('click', hideDbViewerPanel);
  $('#btn-db-viewer-refresh')?.addEventListener('click', () => void refreshDbViewerPanel());
  $('#btn-db-viewer-connect')?.addEventListener('click', () => void saveDbConnection());
  $('#btn-db-viewer-test')?.addEventListener('click', () => void testDbConnection());
  $('#btn-db-viewer-delete')?.addEventListener('click', () => void deleteDbConnection());
  $('#db-viewer-connection-picker')?.addEventListener('change', (e) => {
    void selectDbConnection(e.target?.value || '');
  });
  $('#btn-db-viewer-run-query')?.addEventListener('click', () => void runDbQuery(getDbSqlText()));
  $('#btn-db-viewer-run-selection')?.addEventListener('click', () => void runDbQuery(getDbSqlSelectionText()));
  getDbSqlEl()?.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      void runDbQuery(getDbSqlSelectionText());
    }
  });
  $('#db-viewer-schema-filter')?.addEventListener('input', (e) => {
    state.dbSchemaFilter = e.target.value || '';
    renderDbViewerSchema(state.dbSchema);
  });
  $('#btn-git-viewer-close')?.addEventListener('click', hideGitViewerPanel);
  $('#btn-git-viewer-refresh')?.addEventListener('click', () => void refreshGitViewerPanel());
  $('#btn-git-viewer-run')?.addEventListener('click', () => void runGitViewerCommand());
  $('#git-viewer-command')?.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      void runGitViewerCommand();
    }
  });
  $('#btn-docker-logs-close')?.addEventListener('click', hideDockerLogsPanel);
  $('#btn-docker-logs-stop')?.addEventListener('click', () => void stopDockerLogsStream());
  $('#btn-docker-logs-clear')?.addEventListener('click', clearDockerLogsOutput);
  $('#btn-docker-console-run')?.addEventListener('click', () => void runDockerConsoleCommand());
  $('#btn-docker-containers-refresh')?.addEventListener('click', () => void refreshDockerContainers());
  $('#btn-docker-container-start')?.addEventListener('click', () => void runSelectedDockerContainerAction('start'));
  $('#btn-docker-container-stop')?.addEventListener('click', () => void runSelectedDockerContainerAction('stop'));
  $('#btn-docker-container-restart')?.addEventListener('click', () => void runSelectedDockerContainerAction('restart'));
  $('#btn-docker-container-logs')?.addEventListener('click', () => void runSelectedDockerContainerAction('logs'));
  $('#docker-console-command')?.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      void runDockerConsoleCommand();
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void runDockerConsoleCommand();
    }
  });
  $$('[data-docker-logs-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setDockerLogsDock(btn.dataset.dockerLogsDock));
  });
  $$('[data-build-tasks-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setBuildTasksDock(btn.dataset.buildTasksDock));
  });
  $$('[data-package-manifest-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setPackageManifestDock(btn.dataset.packageManifestDock));
  });
  $('#docker-logs-output')?.addEventListener('scroll', (e) => {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    state.dockerLogsAutoScroll = atBottom;
  });
  $('#status-ai-fix')?.addEventListener('click', (e) => {
    e.stopPropagation();
    state.editor?.runAiQuickFix?.();
  });
  $('#tb-ai-fix')?.addEventListener('click', (e) => {
    e.stopPropagation();
    state.editor?.runAiQuickFix?.();
  });
  document.addEventListener('click', (e) => {
    if (
      !e.target.closest('#ai-quickfix-popover')
      && !e.target.closest('#status-ai-fix')
      && !e.target.closest('#tb-ai-fix')
    ) {
      hideQuickFixMenu();
    }
  });
  $('#branch-picker-btn')?.addEventListener('click', showBranchPicker);
  $('#btn-open-agent').addEventListener('click', toggleAgent);
  $('#btn-new-repo-empty')?.addEventListener('click', showModal);
  $('#btn-new-file').addEventListener('click', showFileModal);
  $('#modal-cancel').addEventListener('click', hideModal);
  $('#modal-close')?.addEventListener('click', hideModal);
  $('#clone-modal-cancel')?.addEventListener('click', hideCloneModal);
  $('#clone-modal-close')?.addEventListener('click', hideCloneModal);
  $('#publish-modal-cancel')?.addEventListener('click', hidePublishModal);
  $('#push-modal-cancel')?.addEventListener('click', hidePushModal);
  $('#push-modal-confirm')?.addEventListener('click', executePush);
  $('#settings-modal-close')?.addEventListener('click', hideSettingsModal);
  $('#settings-modal-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#settings-modal-overlay')) hideSettingsModal();
  });
  $('#settings-pat-form')?.addEventListener('submit', addPatToken);
  $('#settings-cursor-form')?.addEventListener('submit', saveCursorKeyFromSettings);
  $('#settings-cursor-clear')?.addEventListener('click', clearCursorKeyFromSettings);
  $('#settings-cursor-restart')?.addEventListener('click', restartBridge);
  $('#settings-agent-font-match')?.addEventListener('change', onAgentFontMatchChange);
  $('#settings-cursor-open-typography')?.addEventListener('click', openAgentTypographySettings);
  $$('.ij-settings-tab').forEach((btn) => {
    btn.addEventListener('click', () => switchSettingsTab(btn.dataset.settingsTab));
  });
  $('#clone-open-settings')?.addEventListener('click', () => {
    hideCloneModal();
    showSettingsModal('git');
  });
  $('#publish-open-settings')?.addEventListener('click', () => {
    hidePublishModal();
    showSettingsModal('git');
  });
  $('#btn-agent-open-settings')?.addEventListener('click', showAgentKeyForm);
  $('#file-modal-cancel').addEventListener('click', hideFileModal);
  $('#new-repo-form').addEventListener('submit', createRepo);
  $('#clone-repo-form')?.addEventListener('submit', cloneRepo);
  $('#publish-repo-form')?.addEventListener('submit', publishToRemote);
  $('#new-file-form').addEventListener('submit', createFile);
  $('#btn-save')?.addEventListener('click', saveFile);
  $('#tb-save')?.addEventListener('click', saveFile);
  $('#tb-format')?.addEventListener('click', formatDocument);
  $('#tb-rollback')?.addEventListener('click', rollbackLastChange);
  $('#tb-reload-project')?.addEventListener('click', () => reloadProjectIndex());
  $('#btn-reload-project')?.addEventListener('click', () => reloadProjectIndex());
  $('#tb-run')?.addEventListener('click', runActive);
  $('#gradle-task')?.addEventListener('change', () => updateRunButtons());
  $('#build-tasks-refresh')?.addEventListener('click', () => {
    void loadBuildTasksTree(resolveBuildTasksPath(state.activeTab));
  });
  $('#build-tasks-filter')?.addEventListener('input', (e) => {
    state.buildTasksFilter = e.target.value || '';
    rerenderBuildTasksExplorer();
  });
  $('#build-tasks-close')?.addEventListener('click', () => hideBuildTasksPanel());
  $('#btn-build-tasks')?.addEventListener('click', () => toggleBuildTasksPanel());
  $('#structure-refresh')?.addEventListener('click', () => void refreshStructurePanel({ force: true }));
  $('#structure-filter')?.addEventListener('input', (e) => {
    state.structureFilter = e.target.value || '';
    if (state.structureAst) renderStructureTree(state.structureAst);
  });
  $$('[data-ast-mode]').forEach((btn) => {
    btn.addEventListener('click', () => setStructureMode(btn.dataset.astMode));
  });
  $('#structure-tree')?.addEventListener('click', onStructureTreeClick);
  syncStructureModeButtons();
  $('#package-manifest-refresh')?.addEventListener('click', () => {
    void loadPackageManifest(state.activeTab);
  });
  $('#package-manifest-filter')?.addEventListener('input', (e) => {
    state.packageManifestFilter = e.target.value;
    renderPackageManifest(state.packageManifestView, $('#package-manifest-body'));
  });
  $('#package-manifest-close')?.addEventListener('click', () => hidePackageManifestPanel());
  $('#btn-commit-only')?.addEventListener('click', commitOnly);
  $('#btn-commit-push')?.addEventListener('click', commitAndPush);
  $('#btn-suggest-commit')?.addEventListener('click', () => suggestCommitMessage());
  $('#commit-select-all')?.addEventListener('change', (e) => {
    const checked = e.target.checked;
    e.target.indeterminate = false;
    $$('#git-status-list .ij-git-stage-check:not(:disabled)').forEach((input) => {
      input.checked = checked;
      setCommitPathSelected(input.dataset.path, checked);
    });
    updateCommitSelectionUi(state.lastGitStatusFiles, { mergeBlocked: state.mergeBlockedCommit });
  });
  $('#settings-gemini-form')?.addEventListener('submit', saveGeminiKeyFromSettings);
  $('#settings-gemini-clear')?.addEventListener('click', clearGeminiKeyFromSettings);
  $('#settings-gemini-change-key')?.addEventListener('click', showGeminiKeyForm);
  populateGeminiModelSelect();
  $('#settings-gemini-model')?.addEventListener('change', saveGeminiModelFromSettings);
  $('#settings-anthropic-form')?.addEventListener('submit', saveAnthropicFromSettings);
  $('#settings-anthropic-clear')?.addEventListener('click', clearAnthropicFromSettings);
  $('#settings-anthropic-change-key')?.addEventListener('click', showAnthropicKeyForm);
  $('#settings-anthropic-model')?.addEventListener('change', saveAnthropicModelFromSettings);
  $('#settings-bedrock-form')?.addEventListener('submit', saveBedrockFromSettings);
  $('#settings-bedrock-clear')?.addEventListener('click', clearBedrockFromSettings);
  $('#settings-bedrock-change-key')?.addEventListener('click', showBedrockKeyForm);
  $('#settings-bedrock-model')?.addEventListener('change', saveBedrockModelFromSettings);
  $('#settings-bedrock-region')?.addEventListener('change', saveBedrockModelFromSettings);
  $('#settings-bedrock-refresh-models')?.addEventListener('click', () => refreshBedrockModels());
  populateAnthropicModelSelects();
  $('#btn-sync').addEventListener('click', syncPull);
  $('#btn-nav-commit')?.addEventListener('click', () => {
    switchPanel('git');
    setTimeout(() => $('#commit-message')?.focus(), 50);
  });
  $('#btn-nav-push')?.addEventListener('click', pushRemote);
  $('#btn-repo-info')?.addEventListener('click', showRepoInfoModal);
  $('#repo-info-close')?.addEventListener('click', hideRepoInfoModal);
  $('#repo-info-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#repo-info-overlay')) hideRepoInfoModal();
  });
  $('#btn-agent-send').addEventListener('click', sendAgentMessage);
  $('#btn-agent-stop')?.addEventListener('click', stopAgentChat);
  $('#btn-agent-revert')?.addEventListener('click', revertAgentMessage);
  $('#btn-agent-settings').addEventListener('click', showAgentKeyForm);
  $('#btn-agent-clear').addEventListener('click', clearAgentSession);
  $('#btn-agent-close')?.addEventListener('click', closeAgent);
  $('#agent-provider-picker')?.addEventListener('click', (e) => {
    const chip = e.target.closest('[data-agent-provider]');
    if (chip) setAgentProvider(chip.dataset.agentProvider);
  });
  $('#agent-provider-controls')?.addEventListener('click', (e) => {
    const modeBtn = e.target.closest('[data-agent-mode]');
    if (modeBtn) setAgentMode(modeBtn.dataset.agentMode);
  });
  $('#agent-provider-controls')?.addEventListener('change', (e) => {
    if (e.target.id === 'agent-provider-model') {
      agentProviderDef().setModel(e.target.value);
    }
  });
  refreshAgentProviderUi();
  $('#btn-close-diff')?.addEventListener('click', () => {
    state.agentLiveDiffPath = null;
    state.agentLiveFollow = false;
    setAgentDiffLive(false);
    returnToEditorView();
  });
  $('#btn-close-conflict')?.addEventListener('click', () => {
    state.conflictPanelHidden = true;
    if (state.mainView === 'conflict') setMainView('conflict');
  });
  $('#btn-mark-conflict-resolved')?.addEventListener('click', () => markConflictResolved());
  $('#btn-continue-merge')?.addEventListener('click', () => continueMerge());
  $('#btn-abort-merge')?.addEventListener('click', () => abortMerge());
  $('#btn-agent-retry')?.addEventListener('click', restartBridge);
  $$('[data-agent-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setAgentDock(btn.dataset.agentDock));
  });
  $$('[data-terminal-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setTerminalDock(btn.dataset.terminalDock));
  });
  bindTerminalTabs();
  bindEditorTabs();
  const terminalHost = $('#terminal-xterm-host');
  if (terminalHost && typeof ResizeObserver !== 'undefined') {
    new ResizeObserver(() => syncShellLayout()).observe(terminalHost);
  }
  window.addEventListener('resize', () => syncShellLayout());

  $('#agent-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendAgentMessage();
    }
  });

  $$('.activity-btn[data-panel]').forEach((btn) => {
    btn.addEventListener('click', () => {
      if (document.body.classList.contains('sidebar-collapsed') && btn.dataset.panel === 'explorer') {
        toggleSidebar(false);
      }
      if (btn.dataset.panel === 'agent' && state.agentDock !== 'left') {
        toggleAgent();
      } else if (btn.dataset.panel === 'terminal' && state.terminalDock !== 'left') {
        toggleTerminal();
      } else {
        switchPanel(btn.dataset.panel);
      }
    });
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && !$('#tree-context-menu')?.classList.contains('hidden')) {
      hideTreeContextMenu();
      return;
    }
    if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && (e.key === '1' || e.code === 'Digit1')) {
      e.preventDefault();
      switchPanel('explorer');
      return;
    }
    if (e.altKey && !e.metaKey && !e.ctrlKey && !e.shiftKey && (e.key === '7' || e.code === 'Digit7')) {
      e.preventDefault();
      switchPanel('structure');
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      if ($('#palette-overlay')?.classList.contains('open')) hidePalette();
      else showPalette();
      return;
    }
    if (e.key === 'F12' && e.altKey) {
      e.preventDefault();
      toggleTerminal();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === '`') {
      e.preventDefault();
      if (e.shiftKey) newTerminal();
      else toggleTerminal();
      return;
    }
    if (e.key === 'Escape') {
      if (isSettingsOpen()) {
        e.preventDefault();
        hideSettingsModal();
        return;
      }
      if ($('#search-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideSearchEverywhere();
        return;
      }
      if ($('#goto-class-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideGoToClass();
        return;
      }
      if ($('#java-references-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideJavaReferences();
        return;
      }
      if ($('#goto-line-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideGoToLine();
        return;
      }
      if ($('#rename-prompt-overlay') && renamePromptIsOpen()) {
        e.preventDefault();
        hideRenamePrompt(null);
        return;
      }
      if ($('#repo-picker-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideRepoPicker();
        return;
      }
      if ($('#branch-picker-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideBranchPicker();
        return;
      }
      if ($('#palette-overlay')?.classList.contains('open')) {
        hidePalette();
      }
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'b') {
      e.preventDefault();
      if ($('#branch-picker-overlay')?.classList.contains('open')) hideBranchPicker();
      else showBranchPicker();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'p' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      if ($('#search-overlay')?.classList.contains('open')) hideSearchEverywhere();
      else showSearchEverywhere();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      if ($('#search-overlay')?.classList.contains('open')) hideSearchEverywhere();
      else showSearchEverywhere(':');
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'o' && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      if ($('#goto-class-overlay')?.classList.contains('open')) hideGoToClass();
      else showGoToClass();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'g' && !e.shiftKey && !e.altKey) {
      if (isFormField(document.activeElement) && $('#goto-line-overlay')?.classList.contains('open')) return;
      e.preventDefault();
      if ($('#goto-line-overlay')?.classList.contains('open')) hideGoToLine();
      else goToLine();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      e.preventDefault();
      saveFile();
    }
    if (e.key === 'F5') {
      e.preventDefault();
      if (state.debugActive && state.debugState?.status === 'stopped') void debugContinue();
      else runActive();
      return;
    }
    if (e.key === 'F6') {
      e.preventDefault();
      void startDebugSession();
      return;
    }
    if (e.key === 'F9') {
      if (!isFormField(document.activeElement) && state.activeTab && isDebuggablePath(state.activeTab)) {
        e.preventDefault();
        const line = state.editor?.getPosition?.()?.lineNumber || 1;
        toggleBreakpoint(state.activeTab, line);
      }
      return;
    }
    if (e.key === 'F10') {
      if (state.debugActive && state.debugState?.status === 'stopped') {
        e.preventDefault();
        void debugStep('over');
      }
      return;
    }
    if (e.key === 'F11') {
      if (state.debugActive && state.debugState?.status === 'stopped') {
        e.preventDefault();
        if (e.shiftKey) void debugStep('out');
        else void debugStep('in');
      }
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'w') {
      e.preventDefault();
      if (state.activeTab) closeTab(state.activeTab);
    }
    if ((e.metaKey || e.ctrlKey) && e.key === 'n') {
      e.preventDefault();
      showFileModal();
    }
    if ((e.metaKey || e.ctrlKey) && e.key === ',') {
      e.preventDefault();
      showSettingsModal('git');
    }
  });

  $('#btn-tree-back')?.addEventListener('click', () => goBackToTreeFile());
  const treeFilter = $('#tree-filter');
  if (treeFilter && !treeFilter.dataset.bound) {
    treeFilter.dataset.bound = '1';
    let filterTimer = null;
    treeFilter.addEventListener('input', () => {
      clearTimeout(filterTimer);
      filterTimer = setTimeout(() => refreshTree().catch((err) => toast(err.message, 'error')), 200);
    });
  }
  $('#btn-toggle-sidebar')?.addEventListener('click', () => toggleSidebar());
  $('#btn-sidebar-expand')?.addEventListener('click', () => toggleSidebar(false));
  $('#btn-toggle-dotfiles')?.addEventListener('click', () => setShowDotfiles(!getShowDotfiles()));

  $('#modal-overlay').addEventListener('click', (e) => {
    if (e.target === $('#modal-overlay')) hideModal();
  });

  $('#clone-modal-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#clone-modal-overlay')) hideCloneModal();
  });

  $('#publish-modal-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#publish-modal-overlay')) hidePublishModal();
  });

  $('#push-modal-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#push-modal-overlay')) hidePushModal();
  });

  $('#clone-tab-remote')?.addEventListener('click', () => setCloneModalTab('remote'));
  $('#clone-tab-local')?.addEventListener('click', () => setCloneModalTab('local'));
  $('#clone-recent-remote')?.addEventListener('change', () => {
    const url = $('#clone-recent-remote')?.value?.trim();
    if (!url) return;
    const input = $('#clone-remote-url');
    if (input) {
      input.value = url;
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.focus();
    }
  });
  $('#clone-recent-local')?.addEventListener('change', () => {
    const path = $('#clone-recent-local')?.value?.trim();
    if (!path) return;
    const input = $('#clone-local-path');
    if (input) {
      input.value = path;
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.focus();
    }
  });

  $('#clone-remote-url')?.addEventListener('input', (e) => {
    const nameInput = $('#clone-local-name');
    if (!nameInput || nameInput.dataset.userEdited === '1') return;
    const derived = deriveNameFromUrl(e.target.value);
    if (derived) nameInput.value = derived;
  });

  $('#clone-local-browse')?.addEventListener('click', () => browseLocalRepoFolder());

  $('#clone-local-path')?.addEventListener('input', (e) => {
    const nameInput = $('#clone-local-name');
    if (!nameInput || nameInput.dataset.userEdited === '1') return;
    const derived = deriveNameFromLocalPath(e.target.value);
    if (derived) nameInput.value = derived;
  });

  $('#clone-local-name')?.addEventListener('input', (e) => {
    e.target.dataset.userEdited = e.target.value.trim() ? '1' : '';
  });

  $('#file-modal-overlay').addEventListener('click', (e) => {
    if (e.target === $('#file-modal-overlay')) hideFileModal();
  });

  $('#panel-explorer')?.addEventListener('scroll', onTreeContextMenuScroll, true);
  window.addEventListener('resize', hideTreeContextMenu);
}

async function init() {
  const splashSequence = waitForLaunchSplashSequence();
  setStatusMessage('Ready');
  if (!window.ReaperAgentMarkdown?.libsReady?.()) {
    console.error('[Reaper] Agent markdown not ready — tables/diagrams will show as plain text until scripts load.');
  }
  populateFontSizeSelects();
  populateFontFamilySelects();
  populateAgentFontSelects();
  applyUiTypography();
  await loadCoverageInlinePref();
  syncFontSizeControls(getEditorFontSize());
  ensureEditorFontLoaded(getEditorFontSpec());
  applyAgentTypography();
  syncAgentFontControls();
  ensureTerminals();
  renderTerminalTabs();
  syncDotfilesControls(getShowDotfiles());
  await ensureGeminiReady();
  initEditor();
  bindEvents();
  bindMenus();
  bindPalette();
  bindGoToClass();
  bindJavaReferences();
  bindGoToLine();
  bindRenamePrompt();
  bindSearchEverywhere();
  bindBranchPicker();
  bindRepoPicker();
  bindHeaderBrand();
  mountReaperIcons();
  void initStatusFooter();
  initSidebarResize();
  initAgentDockResize();
  initTerminalBottomResize();
  initDockerLogsDockResize();
  initBuildTasksDockResize();
  initDbViewerResize();
  initGitViewerResize();
  applyAgentDock();
  applyTerminalDock();
  applyDockerLogsDock();
  applyBuildTasksDock();
  applyPackageManifestDock();
  switchPanel('explorer');
  renderWelcome();
  $('#empty-state')?.classList.remove('hidden');
  syncWelcomeLayout();
  void loadCursorStatus();
  void loadGeminiSettingsSection();
  void loadAnthropicSettingsSection();
  const initWork = (async () => {
  try {
    await loadRepos();
    void initStatusFooter();
    void ensureStartupIndexPolling();
  } catch (err) {
    toast(`Could not reach Reaper backend: ${err.message}. Quit other Reaper copies and relaunch.`, 'error', { duration: 15000 });
  }
  let repoToOpen = shouldSkipAutoRepoOpen() ? null : getInitialRepoFromUrl();
  try {
    const general = await api('/api/settings/general');
    state.gitBackgroundFetch = !!general?.git_background_fetch;
    state.lastRepo = general?.last_repo || null;
    state.recentGitRemotes = Array.isArray(general?.recent_git_remotes)
      ? general.recent_git_remotes
      : [];
    state.recentGitLocalPaths = Array.isArray(general?.recent_git_local_paths)
      ? general.recent_git_local_paths
      : [];
    if (!repoToOpen && !shouldSkipAutoRepoOpen()) {
      repoToOpen = general?.default_repo || general?.last_repo || null;
    }
  } catch {
    state.gitBackgroundFetch = false;
  }
  void syncGitBackgroundFetchSetting();
  if (repoToOpen && state.repos.some((r) => r.name === repoToOpen)) {
    // Capture/demo URLs must wait for the workspace — otherwise screenshots are empty/stale chrome.
    if (new URLSearchParams(window.location.search).has('capture')) {
      await selectRepo(repoToOpen);
    } else {
      void selectRepo(repoToOpen);
    }
  } else if (!state.repo) {
    showNoRepoFileTree();
  }
  await applyCaptureDemoFromUrl();
  })();
  await Promise.all([splashSequence, initWork]);
  hideLaunchSplash({ immediate: true });
  setInterval(async () => {
    if (state.cursorConfigured && !state.cursorBridgeOk && !state.agentBusy) {
      await loadCursorStatus();
    }
  }, 3000);
  startDemoNavPoller();
}

/** Close leftover capture chrome so the next demo shot starts clean. */
function dismissCaptureChrome() {
  try { hidePalette(); } catch { /* ignore */ }
  try { hideSearchEverywhere(); } catch { /* ignore */ }
  try { hideGoToClass(); } catch { /* ignore */ }
  try { hideGoToLine(); } catch { /* ignore */ }
  try { hideJavaReferences(); } catch { /* ignore */ }
  try { hideRenamePrompt(null); } catch { /* ignore */ }
  try { hideBranchPicker(); } catch { /* ignore */ }
  try { hideRepoPicker(); } catch { /* ignore */ }
  try { hideSettingsModal(); } catch { /* ignore */ }
  try { hideModal(); } catch { /* ignore */ }
  try { hideCloneModal(); } catch { /* ignore */ }
  try { hidePublishModal(); } catch { /* ignore */ }
  try { hidePushModal(); } catch { /* ignore */ }
  try { hideRepoInfoModal(); } catch { /* ignore */ }
  try { hideQuickFixMenu(); } catch { /* ignore */ }
  try { hideCoveragePanel(); } catch { /* ignore */ }
  try { setGlobalLoading(false); } catch { /* ignore */ }
  try { setMainView('editor'); } catch { /* ignore */ }
  document.getElementById('reaper-capture-theme-panel')?.remove();
  document.getElementById('reaper-capture-ghost')?.remove();
  document.getElementById('reaper-capture-providers')?.remove();
}

/**
 * Soft-navigate for live macOS screenshots (no full reload → no splash).
 * Signals readiness via POST http://127.0.0.1:17923 (capture sidecar) and document.title.
 */
async function signalDemoCapture(id, phase) {
  window.__reaperDemoNavId = id;
  window.__reaperCapturePhase = phase;
  document.title = `Reaper · ${phase}:${id}`;
  try {
    await fetch('http://127.0.0.1:17923/demo-capture-status', {
      method: 'POST',
      mode: 'cors',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id, phase, t: Date.now() }),
    });
  } catch { /* sidecar only runs during live capture */ }
}

async function applyDemoNav(rawUrl, id) {
  const next = new URL(rawUrl, window.location.href);
  history.replaceState(null, '', `${next.pathname}${next.search}${next.hash}`);
  window.__reaperCaptureDone = false;
  await signalDemoCapture(id, 'busy');
  dismissLaunchSplashNow();
  dismissCaptureChrome();

  const params = new URLSearchParams(next.search);
  const norepo = params.has('norepo');
  const wantRepo = params.get('repo')?.trim() || null;

  try {
    if (norepo) {
      if (state.repo) await selectRepo('');
      window.ReaperThemes?.applyTheme?.(params.get('theme') || 'navy');
      renderWelcome();
      $('#empty-state')?.classList.remove('hidden');
      syncWelcomeLayout();
      if (!params.has('capture')) {
        await new Promise((r) => setTimeout(r, 1400));
        window.__reaperCaptureDone = true;
      }
    } else if (wantRepo && state.repo !== wantRepo) {
      await selectRepo(wantRepo);
      await new Promise((r) => setTimeout(r, 700));
    }

    if (params.has('capture')) {
      await applyCaptureDemoFromUrl();
    } else if (!window.__reaperCaptureDone) {
      window.ReaperThemes?.applyTheme?.(params.get('theme') || 'navy');
      await new Promise((r) => setTimeout(r, 1200));
      window.__reaperCaptureDone = true;
    }
  } catch (err) {
    console.error('[demo-nav]', err);
    window.__reaperCaptureDone = true;
  } finally {
    await signalDemoCapture(id, 'ready');
  }
}

/** Poll /demo-nav.json so the live macOS app can be driven for screenshot capture. */
function startDemoNavPoller() {
  let lastId = null;
  let busy = false;
  const tick = async () => {
    if (busy) return;
    try {
      const res = await fetch(`/demo-nav.json?t=${Date.now()}`, { cache: 'no-store' });
      if (!res.ok) return;
      const data = await res.json();
      const id = data?.id;
      const url = String(data?.url || '').trim();
      if (!id || !url || id === lastId) return;
      lastId = id;
      busy = true;
      // Soft nav keeps the WKWebView alive — location.assign was capturing the splash.
      await applyDemoNav(url, id);
    } catch { /* ignore */ } finally {
      busy = false;
    }
  };
  setInterval(tick, 400);
  void tick();
}

init().catch(async (e) => {
  toast(e.message, 'error');
  await waitForLaunchSplashSequence();
  hideLaunchSplash();
});
