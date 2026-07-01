const AGENT_DOCK_KEY = 'reaper-agent-dock';
const AGENT_RIGHT_WIDTH_KEY = 'reaper-agent-right-w';
const AGENT_BOTTOM_HEIGHT_KEY = 'reaper-agent-bottom-h';
const AGENT_PROVIDER_KEY = 'reaper-agent-provider';
const TERMINAL_DOCK_KEY = 'reaper-terminal-dock';
const TERMINAL_BOTTOM_HEIGHT_KEY = 'reaper-terminal-bottom-height';
const EDITOR_FONT_SIZE_KEY = 'reaper-editor-font-size';
const EDITOR_FONT_FAMILY_KEY = 'reaper-editor-font-family';
const AGENT_FONT_SIZE_KEY = 'reaper-agent-font-size';
const AGENT_FONT_FAMILY_KEY = 'reaper-agent-font-family';
const AGENT_FONT_MATCH_EDITOR_KEY = 'reaper-agent-font-match-editor';
const AUTO_SAVE_KEY = 'reaper-auto-save';
const AI_INLINE_COMPLETE_KEY = 'reaper-ai-inline-complete';
const SHOW_DOTFILES_KEY = 'reaper-show-dotfiles';
const NEW_WINDOW_ON_REPO_KEY = 'reaper-new-window-on-repo';
const AUTO_SAVE_DELAY_MS = 800;
const DIAG_DELAY_MS = 150;
const ALL_JAVA_DIAG_DELAY_MS = 250;
const PROJECT_RELOAD_DELAY_MS = 2000;
const PROJECT_BUILD_RELOAD_DELAY_MS = 1500;
const PROJECT_INDEX_POLL_MS = 750;
const PROJECT_AUTO_REFRESH_MAX = 3;

/** xterm ANSI styling for streamed command output */
const TERM_ESC = {
  reset: '\x1b[0m',
  dim: '\x1b[90m',
  bold: '\x1b[1m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  brightCyan: '\x1b[96m',
};
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

const CURSOR_MODELS_FALLBACK = [
  { id: 'composer-2.5-fast', label: 'Composer 2.5 Fast' },
  { id: 'composer-2.5', label: 'Composer 2.5' },
  { id: 'gpt-5.3-codex', label: 'GPT-5.3 Codex' },
  { id: 'gpt-5.5-medium', label: 'GPT-5.5 Medium' },
  { id: 'claude-4.6-sonnet-medium-thinking', label: 'Claude 4.6 Sonnet' },
  { id: 'claude-opus-4-8-thinking-high', label: 'Claude Opus 4.8' },
];

/** Registered chat agents — add providers here as backends land. */
const AGENT_PROVIDER_ORDER = ['cursor', 'gemini', 'anthropic'];

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
    models: () => (state.cursorModels?.length ? state.cursorModels : CURSOR_MODELS_FALLBACK),
    currentModel: () => state.cursorModel,
    setModel: (id) => setCursorAgentModel(id),
    statusText: () => {
      const modeLabel = state.cursorMode === 'plan' ? 'Plan' : state.cursorMode === 'ask' ? 'Ask' : 'Agent';
      return `Cursor ${modeLabel} · ${state.cursorModel || 'composer-2.5'}`;
    },
    chatPath: '/cursor/chat',
    stopPath: '/cursor/stop',
    chatBody: (prompt) => ({ prompt, model: state.cursorModel, mode: state.cursorMode }),
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
    comingSoon: true,
    capabilities: { readOnly: true },
    isConfigured: () => false,
    isReady: () => false,
    welcome: () => 'Claude agent is coming soon — Anthropic API key support is on the way.',
    placeholder: 'Claude agent — coming soon',
    emptyReply: () => 'No response from Claude.',
    hintWhenReady: 'Claude agent — coming soon',
    messageLabel: 'Claude agent',
    labelClass: 'agent-msg-label-anthropic',
    models: () => [],
    currentModel: () => '',
    setModel: () => {},
    statusText: () => 'Claude — coming soon',
    chatPath: '/anthropic/chat',
    stopPath: null,
    chatBody: () => ({}),
    notConfiguredHint: 'Claude agent — coming soon',
  },
};

const state = {
  repo: null,
  repos: [],
  branches: [],
  tabs: [],
  tabContents: new Map(),
  activeTab: null,
  editor: null,
  dirty: new Set(),
  activePanel: 'explorer',
  agentDock: localStorage.getItem(AGENT_DOCK_KEY) || 'left',
  terminalDock: localStorage.getItem(TERMINAL_DOCK_KEY) || 'bottom',
  terminalOpen: false,
  terminals: [],
  activeTerminalId: null,
  agentOpen: true,
  cursorConfigured: false,
  cursorBridgeOk: false,
  cursorBridgeError: null,
  cursorKeyMasked: null,
  cursorKeySource: null,
  cursorModel: 'composer-2.5',
  agentProvider: localStorage.getItem(AGENT_PROVIDER_KEY) || 'cursor',
  cursorMode: 'agent',
  cursorModels: [],
  agentBusy: false,
  agentMessageQueue: [],
  agentAbortController: null,
  agentStopRequested: false,
  agentLastRevertibleTurn: null,
  agentLiveFollow: false,
  agentLiveDiffPath: null,
  agentLastToolPath: null,
  agentSeenPaths: new Set(),
  agentHadFileChanges: false,
  cloneBusy: false,
  cloneSource: 'remote',
  currentBranch: '',
  defaultBranch: '',
  editorReady: false,
  suppressEditorChange: false,
  autoSaveTimer: null,
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
  coveragePanelOpen: false,
  coverageReport: null,
  conflictFiles: new Set(),
  selectedCommitHash: null,
  mainView: 'editor',
  conflictPanelHidden: false,
  geminiConfigured: false,
  javaLanguageLevel: 17,
  languageContext: null,
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
let diagSeq = 0;
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

async function api(path, opts = {}) {
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json', ...opts.headers },
    ...opts,
  });
  const text = await res.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = text; }
  if (!res.ok) throw new Error(data?.error || res.statusText);
  return data;
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

function setGlobalLoading(on, text = 'Loading…') {
  const overlay = $('#loading-overlay');
  const label = $('#loading-text');
  if (label) label.textContent = text;
  overlay?.classList.toggle('hidden', !on);
  overlay?.classList.toggle('flex', on);
}

function hideLaunchSplash() {
  const splash = $('#launch-splash');
  if (!splash || splash.dataset.dismissing) return;
  splash.dataset.dismissing = '1';
  const started = window.__reaperSplashAt || Date.now();
  const minVisible = 4500;
  const wait = Math.max(0, minVisible - (Date.now() - started));
  setTimeout(() => {
    document.body?.classList.add('reaper-ui-ready');
    splash.remove();
  }, wait);
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
  const idleLabel = isLocal ? 'Import' : 'Import';

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
    submitBtn.textContent = busy ? busyLabel : idleLabel;
  }
  ['#clone-remote-url', '#clone-local-name', '#clone-local-path', '#clone-local-browse'].forEach((sel) => {
    const input = $(sel);
    if (input) input.disabled = busy;
  });
  state.cloneBusy = busy;
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
    const ver = jdk?.effective_version || jdk?.version || '';
    const m = String(ver).match(/(\d+)/);
    state.javaLanguageLevel = m ? parseInt(m[1], 10) : 17;
  } catch {
    state.javaLanguageLevel = 17;
  }
}

async function refreshLanguageContextForPath(path) {
  if (!state.repo || !path) {
    state.languageContext = null;
    return refreshJavaLanguageLevel();
  }
  try {
    const q = new URLSearchParams({ path });
    const res = await api(repoApi(state.repo, `/workspace/language-context?${q}`));
    if (res) {
      state.languageContext = res;
      if (res.java_level) state.javaLanguageLevel = res.java_level;
      else if (path.endsWith('.java')) await refreshJavaLanguageLevel();
      return;
    }
  } catch {
    /* fall back */
  }
  if (path.endsWith('.java')) return refreshJavaLanguageLevelForPath(path);
  state.languageContext = null;
  return refreshJavaLanguageLevel();
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

function shouldOpenRepoInNewWindow(repoName) {
  return !!(
    repoName
    && state.repo
    && repoName !== state.repo
    && getNewWindowOnRepoChange()
  );
}

function requestRepoSelection(repoName, { revertSelect = true } = {}) {
  if (shouldOpenRepoInNewWindow(repoName)) {
    if (revertSelect) {
      setRepoPickerLabel(state.repo || '');
    }
    openRepoInNewWindow(repoName);
    return;
  }
  setRepoPickerLabel(repoName || '');
  void selectRepo(repoName);
}

function updateWindowTitle() {
  document.title = state.repo ? `Reaper — ${state.repo}` : 'Reaper';
}

function getInitialRepoFromUrl() {
  const repo = new URLSearchParams(window.location.search).get('repo')?.trim();
  return repo || null;
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
    term.xterm.options.lineHeight = 1.28;
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
  const newWindowOnRepo = $('#settings-new-window-on-repo');
  if (newWindowOnRepo) {
    newWindowOnRepo.checked = getNewWindowOnRepoChange();
    if (!newWindowOnRepo.dataset.bound) {
      newWindowOnRepo.dataset.bound = '1';
      newWindowOnRepo.addEventListener('change', (e) => setNewWindowOnRepoChange(e.target.checked));
    }
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
    'java', 'kotlin', 'groovy', 'gradle',
    'python', 'ruby', 'bundle', 'rails',
    'rustc', 'cargo', 'go',
    'node', 'tsc',
    'jsonlint', 'ajv',
    'yamllint',
    'clang', 'gcc',
    'swiftc', 'dart', 'php', 'luac', 'csc', 'bash',
  ];

  function compilerStatus(tool) {
    if (tool.configured) {
      return { cls: 'custom', label: 'Custom' };
    }
    if (tool.effective) {
      return { cls: 'ready', label: 'PATH' };
    }
    return { cls: 'missing', label: 'Missing' };
  }

  function renderCompilerRow(tool, { javaInstalled, gradleInstalled }) {
    const isJava = tool.id === 'java';
    const isGradle = tool.id === 'gradle';
    const status = compilerStatus(tool);
    const placeholder = tool.kind === 'home'
      ? '/Library/Java/JavaVirtualMachines/…/Contents/Home'
      : isGradle
        ? '/opt/homebrew/bin/gradle or GRADLE_HOME'
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
    const gradleSelect = isGradle && gradleInstalled.length
      ? `<div class="ij-compiler-extra">
          <label class="ij-compiler-extra-label">Installed Gradle</label>
          <select class="ij-settings-select settings-compiler-gradle-select" data-tool-id="gradle" title="Pick a Gradle version">
            <option value="">— pick installed Gradle —</option>
            ${gradleInstalled.map((g) => `<option value="${escapeHtml(g.path)}"${installSelected(tool.path || tool.effective, g.path) ? ' selected' : ''}>${escapeHtml(g.label || g.path)}</option>`).join('')}
          </select>
        </div>`
      : '';
    const using = tool.effective
      ? `<div class="ij-compiler-using" title="${escapeHtml(tool.effective)}">Using ${escapeHtml(tool.effective)}</div>`
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
      ${jdkSelect}
      ${gradleSelect}
    </article>`;
  }

  function bindCompilerRows(root) {
    root.querySelectorAll('.settings-compiler-jdk-select, .settings-compiler-gradle-select').forEach((sel) => {
      sel.addEventListener('change', () => {
        const input = root.querySelector(`.settings-compiler-input[data-tool-id="${sel.dataset.toolId}"]`);
        if (input && sel.value) input.value = sel.value;
      });
    });
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
    const cfg = await api('/api/settings/compilers');
    const tools = cfg.compilers || cfg.tools || [];
    const javaInstalled = cfg.java_installed || [];
    const gradleInstalled = cfg.gradle_installed || [];
    const byId = Object.fromEntries(tools.map((t) => [t.id, t]));
    const ordered = [
      ...COMPILER_ORDER.map((id) => byId[id]).filter(Boolean),
      ...tools.filter((t) => !COMPILER_ORDER.includes(t.id)),
    ];
    list.innerHTML = `<div class="ij-compiler-table">
      <div class="ij-compiler-head" aria-hidden="true">
        <span>Language</span>
        <span>Status</span>
        <span>Path override</span>
        <span></span>
      </div>
      <div class="ij-compiler-body">${ordered.map((tool) => renderCompilerRow(tool, { javaInstalled, gradleInstalled })).join('')}</div>
    </div>`;
    bindCompilerRows(list);

    const search = $('#settings-compiler-search');
    if (search && !search.dataset.bound) {
      search.dataset.bound = '1';
      search.addEventListener('input', (e) => filterCompilerRows(e.target.value));
    }
    filterCompilerRows(search?.value || '');
  } catch (err) {
    list.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
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
    await api('/api/settings/compilers', {
      method: 'PATCH',
      body: JSON.stringify({ id, path }),
    });
    await loadCompilersSettingsSection();
    void refreshJavaLanguageLevel();
    toast(`${id} compiler saved`, 'success');
  } catch (err) {
    toast(err.message || `Failed to save ${id}`, 'error');
  }
}

async function clearCompilerFromSettings(id) {
  try {
    await api(`/api/settings/compilers/${encodeURIComponent(id)}`, { method: 'DELETE' });
    await loadCompilersSettingsSection();
    void refreshJavaLanguageLevel();
    toast(`${id} using system default`, 'success');
  } catch (err) {
    toast(err.message || `Failed to clear ${id}`, 'error');
  }
}

async function loadSettingsModal() {
  await Promise.all([loadPatTokensList(), loadCursorSettingsSection(), loadGeminiSettingsSection(), loadCompilersSettingsSection()]);
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
  const parts = displayPath.split('/').filter(Boolean);
  el.innerHTML = parts.map((part, i) => {
    const seg = parts.slice(0, i + 1).join('/');
    const sep = i < parts.length - 1 ? '<span class="ij-crumb-sep"> › </span>' : '';
    return `<button type="button" class="ij-crumb" data-crumb="${escapeHtml(seg)}">${escapeHtml(part)}</button>${sep}`;
  }).join('');
  $$('.ij-crumb').forEach((btn) => {
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

function stopProjectIndexPolling() {
  if (state.projectIndexPoll) {
    clearInterval(state.projectIndexPoll);
    state.projectIndexPoll = null;
  }
  clearIndexingProgressUi();
}

function projectIndexNeedsFreeze(status) {
  if (!status) return false;
  const indexers = status.profile?.indexers || [];
  const needsJava = indexers.includes('java');
  const javaRunning = status.java?.state === 'running';
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
    case 'writing': return 'Saving index';
    case 'starting': return 'Starting';
    case 'ready': return 'Ready';
    default: return phase ? phase.replace(/-/g, ' ') : 'Indexing';
  }
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
  if (status.state === 'ready') return 100;
  const java = status.java || {};
  const phase = java.phase || status.phase || '';
  const wsN = status.workspace_symbols || 0;
  const rawJavaN = java.symbol_count || 0;

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
  const parts = [];
  if (wsN > 0) parts.push(`${wsN.toLocaleString()} workspace`);
  if (javaN > 0) parts.push(`${javaN.toLocaleString()} Java`);
  if (java.spring_symbols > 0) parts.push(`${java.spring_symbols.toLocaleString()} Spring`);
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
  // Banner only — never cover the editor; the veil blocks Monaco suggest widgets.
  overlay?.classList.add('hidden');
  overlay?.setAttribute('aria-hidden', 'true');
}

function clearIndexingProgressUi() {
  $('#java-index-banner')?.classList.add('hidden');
  applyIndexingProgressUi({ show: false });
  state.editorIndexFrozen = false;
  state.indexFreezeActive = false;
  if (state.editor && window.monaco) {
    state.editor.updateOptions({ readOnly: false });
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

function startProjectIndexPolling() {
  stopProjectIndexPolling();
  state.projectIndexNotified = false;
  state.projectIndexStartedAt = Date.now();
  pollProjectIndexStatus();
  state.projectIndexPoll = setInterval(pollProjectIndexStatus, PROJECT_INDEX_POLL_MS);
}

function updateProjectIndexUi(status) {
  const banner = $('#java-index-banner');

  state.projectIndexRunning = projectIndexNeedsFreeze(status);
  const javaReady = status?.java?.state === 'ready'
    && (status?.java?.symbol_count || 0) > 0;
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
    banner?.classList.remove('hidden');
    applyIndexingProgressUi({
      title: progress.title,
      label: status?.label || indexingLabelFromProfile(status?.profile),
      phase: progress.phase,
      stats: progress.stats,
      percent: progress.percent ?? 5,
      show: true,
    });
    return;
  }

  const bgPhase = status?.java?.phase;
  if (isBackgroundToolingPhase(bgPhase)) {
    const phaseLabel = indexingPhaseLabel(bgPhase);
    setStatusMessage(phaseLabel);
    banner?.classList.remove('hidden');
    applyIndexingProgressUi({
      title: `Updating ${status?.label || indexingLabelFromProfile(status?.profile) || 'project'} classpath…`,
      label: status?.label || indexingLabelFromProfile(status?.profile),
      phase: phaseLabel,
      stats: '',
      percent: indexingProgressPercent(status),
      show: true,
    });
    return;
  }

  clearIndexingProgressUi();

  if (status?.state === 'ready' && !state.projectIndexNotified) {
    state.projectIndexNotified = true;
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
  } else if (status?.state === 'error' && !state.projectIndexNotified) {
    state.projectIndexNotified = true;
    state.projectReloadBackground = false;
    if (!state.projectReloadPending) {
      toast(`Project indexing failed: ${status.error || status.java?.error || 'unknown error'}`, 'error');
    }
  }

  if (!projectIndexNeedsFreeze(status) && state.projectReloadPending) {
    state.projectReloadPending = false;
    scheduleProjectReload(0);
  }
}

function isProjectBuildFile(path) {
  if (!path) return false;
  const normalized = path.replace(/\\/g, '/').toLowerCase();
  const base = normalized.split('/').pop() || '';
  if (base === 'pom.xml') return true;
  if (base === 'build.gradle' || base === 'build.gradle.kts') return true;
  if (base === 'settings.gradle' || base === 'settings.gradle.kts') return true;
  if (base === 'gradle.properties') return true;
  if (normalized.endsWith('/gradle/libs.versions.toml')) return true;
  return false;
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
    if (!silent) toast(err.message || 'Failed to reload project', 'error');
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
    const javaStillRunning = status?.profile?.indexers?.includes('java') && status?.java?.state === 'running';
    if ((status?.state === 'ready' || status?.state === 'error' || status?.state === 'idle') && !javaStillRunning) {
      stopProjectIndexPolling();
    }
  } catch {
    /* ignore transient poll errors */
  }
}

function welcomeScreenHtml() {
  const recent = state.repos.slice(0, 5);
  const recentHtml = recent.length
    ? `<div class="ij-recent">
        <div class="ij-recent-title">Recent repositories</div>
        <div class="ij-recent-list">
          ${recent.map((r) => `<button type="button" class="ij-recent-item" data-recent="${r.name}">${r.name}</button>`).join('')}
        </div>
      </div>`
    : '';
  const icons = window.ReaperIcons || {};
  return `
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
      <div class="ij-shortcut"><dt>⌘K</dt><dd>Command palette</dd></div>
      <div class="ij-shortcut"><dt>⌘S</dt><dd>Save file</dd></div>
      <div class="ij-shortcut"><dt>F5</dt><dd>Run / Gradle</dd></div>
      <div class="ij-shortcut"><dt>⌘N</dt><dd>New file</dd></div>
      <div class="ij-shortcut"><dt>⌘W</dt><dd>Close tab</dd></div>
      <div class="ij-shortcut"><dt>Alt+1</dt><dd>Project tool window</dd></div>
    </dl>`;
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
  if (branchEl) branchEl.textContent = branch ? `⎇ ${branch}` : '';
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
  const dirty = !!(hasTab && state.dirty.has(state.activeTab));
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
  const navPush = $('#btn-nav-push');
  if (navPush) {
    navPush.classList.toggle('ij-header-btn-pending', ahead > 0);
    navPush.title = ahead > 0
      ? `Push ${ahead} commit${ahead === 1 ? '' : 's'} to remote`
      : 'Push to remote';
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
    'panel-git': () => switchPanel('git'),
    'panel-history': () => switchPanel('history'),
    'panel-terminal': () => showTerminal(),
    'terminal-new': () => newTerminal(),
    'panel-agent': () => { switchPanel('agent'); if (state.agentDock !== 'left') toggleAgent(); },
    'panel-coverage': () => showCoveragePanel(),
    'terminal-dock-left': () => setTerminalDock('left'),
    'terminal-dock-right': () => setTerminalDock('right'),
    'terminal-dock-bottom': () => setTerminalDock('bottom'),
    'agent-dock-left': () => setAgentDock('left'),
    'agent-dock-right': () => setAgentDock('right'),
    'agent-dock-bottom': () => setAgentDock('bottom'),
    'toggle-sidebar': () => toggleSidebar(),
    'toggle-dotfiles': () => setShowDotfiles(!getShowDotfiles()),
    'command-palette': showPalette,
    'search-everywhere': () => showSearchEverywhere(),
    'goto-class': showGoToClass,
    'switch-branch': showBranchPicker,
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
  { id: 'switch-branch', label: 'Switch branch…', kbd: '⌘⇧B', run: showBranchPicker, needsRepo: true },
  { id: 'reload-project', label: 'Reload Maven/Gradle project', run: () => reloadProjectIndex(), needsRepo: true, needsBuildTool: true },
  { id: 'new-file', label: 'New file', kbd: '⌘N', run: showFileModal, needsRepo: true },
  { id: 'save', label: 'Save', kbd: '⌘S', run: saveFile, needsTab: true, needsDirty: true },
  { id: 'format', label: 'Reformat code', kbd: '⇧⌥F', run: formatDocument, needsTab: true },
  { id: 'run', label: 'Run', kbd: 'F5', run: runActive, needsRun: true },
  { id: 'commit', label: 'Commit…', run: () => switchPanel('git'), needsRepo: true },
  { id: 'pull', label: 'Pull', run: syncPull, needsRepo: true },
  { id: 'push', label: 'Push to remote', run: pushRemote, needsRepo: true },
  { id: 'publish', label: 'Publish to GitHub…', run: showPublishModal, needsRepo: true },
  { id: 'repo-info', label: 'Repository details', run: showRepoInfoModal, needsRepo: true },
  { id: 'explorer', label: 'Show Project', kbd: 'Alt+1', run: () => switchPanel('explorer') },
  { id: 'git-panel', label: 'Show Commit', kbd: 'Alt+9', run: () => switchPanel('git') },
  { id: 'terminal', label: 'Show Terminal', run: () => showTerminal() },
  { id: 'terminal-new', label: 'New Terminal', kbd: '⌘⇧`', run: () => newTerminal(), needsRepo: true },
  { id: 'coverage', label: 'Coverage report', run: () => showCoveragePanel(), needsRepo: true },
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
  if (cmd.needsDirty && !(state.activeTab && state.dirty.has(state.activeTab))) return false;
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
  if (gotoClassTimer) {
    clearTimeout(gotoClassTimer);
    gotoClassTimer = null;
  }
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
  state.repoPickerUnregisterName = null;
  $('#repo-picker-overlay')?.classList.remove('open');
}

function renderRepoPickerResults() {
  const results = $('#repo-picker-results');
  if (!results) return;
  if (!state.repos.length) {
    results.innerHTML = '<p class="ij-repo-picker-empty">No repositories yet — import or create one from the welcome screen.</p>';
    return;
  }
  const current = state.repo;
  const pending = state.repoPickerUnregisterName;
  results.innerHTML = state.repos.map((r) => {
    const label = r.name;
    const isCurrent = r.name === current;
    if (pending === r.name) {
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
}

async function unregisterRepo(name) {
  const repoName = name || state.repo;
  if (!repoName) return;
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
    hideRepoPicker();
    await loadRepos();
  } catch (err) {
    toast(err.message, 'error');
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
      hideRepoPicker();
      requestRepoSelection(openBtn.dataset.repoOpen, { revertSelect: false });
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

function findJavaTestClassLine(lines) {
  for (let i = 0; i < lines.length; i += 1) {
    const t = lines[i].trim();
    if (/^(public\s+|protected\s+|private\s+)?(abstract\s+|static\s+)?class\s+[A-Za-z_]\w*/.test(t) && !t.includes('(')) {
      return i;
    }
  }
  return -1;
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
  return Boolean(state.runInfo?.has_project);
}

function createTestRunWidget(method) {
  const domNode = document.createElement('button');
  domNode.type = 'button';
  domNode.className = 'ij-test-run-widget';
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

function createTestCoverageWidget(method) {
  const domNode = document.createElement('button');
  domNode.type = 'button';
  domNode.className = 'ij-test-cov-widget';
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
  if (!state.activeTab?.endsWith('.java')) return;
  const paint = () => {
    const content = state.editor.getModel()?.getValue() ?? '';
    const methods = listJavaTestMethods(state.activeTab, content);
    if (!methods.length || typeof state.editor.addGlyphMarginWidget !== 'function') return;
    clearTestRunWidgets();
    const showCoverage = projectSupportsCoverage();
    state.testRunWidgets = [];
    state.testCovWidgets = [];
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
  };
  requestAnimationFrame(() => requestAnimationFrame(paint));
}

function clearCoverageDecorations() {
  if (!state.editor || !window.monaco) return;
  state.coverageDecorationIds = state.editor.deltaDecorations(state.coverageDecorationIds ?? [], []);
}

function applyCoverageDecorations(path, cov) {
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
    body.innerHTML = `<p class="ij-coverage-empty">${escapeHtml(summary.message)}</p>`;
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

async function openCoverageHtmlReport() {
  const path = state.coverageReport?.html_report_path;
  if (!path || !state.repo) {
    toast('HTML report not found — run tests with coverage first', 'info');
    return;
  }
  try {
    await api(repoApi(state.repo, '/workspace/open-external'), { method: 'POST', body: { path } });
  } catch (e) {
    toast(e.message || 'Could not open report', 'error');
  }
}

async function fetchAndApplyCoverage(path) {
  if (!state.repo || !path) return null;
  try {
    const cov = await api(
      `${repoApi(state.repo, '/workspace/coverage')}?path=${encodeURIComponent(stripJavaDiagOverlayPath(path))}`,
    );
    const target = cov?.coverage_path || path;
    if (cov?.lines?.length) {
      state.fileCoverage.set(target, cov);
      if (state.activeTab === target) {
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
    if (cov?.message) toast(cov.message, 'info');
    return cov;
  } catch (e) {
    toast(`Coverage: ${e.message}`, 'warning');
    return null;
  }
}

function reapplyCoverageForTab(path) {
  const cov = state.fileCoverage?.get(path);
  if (cov?.lines?.length) {
    applyCoverageDecorations(path, cov);
    updateCoverageStatus(cov);
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
    const lang = langForPath(path);
    window.ReaperLang?.ensureMonacoBasicLanguage?.(lang);
    monaco.editor.setModelLanguage(state.editor.getModel(), lang);
  } finally {
    state.suppressEditorChange = false;
  }
  applyTestRunDecorations();
  reapplyCoverageForTab(path);
  if (path?.endsWith('.java') && !state.fileCoverage?.has(path)) {
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
        shareSuggestSelections: true,
        showInlineDetails: true,
        acceptSuggestionOnCommitCharacter: false,
      },
      inlineSuggest: {
        enabled: true,
        showToolbar: false,
        suppressSuggestions: false,
        mode: 'subwordSmart',
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
    try {
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
      isFileDirty: (path) => state.dirty.has(path),
      getJavaSourceOverlays: (excludePath) => collectJavaDiagnosticOverlays(excludePath),
      getAiInlineComplete: () => getAiInlineCompleteEnabled(),
      getGeminiConfigured: () => state.geminiConfigured,
      getJavaLanguageLevel: () => state.javaLanguageLevel || 17,
      getLanguageContext: () => state.languageContext,
      toast,
      terminalLog,
      setCompleteDebugStatus,
      setStatusMessage,
      showQuickFixMenu,
      hideQuickFixMenu,
      isQuickFixMenuOpen,
      scheduleDiagnostics: () => scheduleDiagnostics(),
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
      setLanguageStatus: (label) => {
        const el = $('#status-language');
        if (el) el.textContent = label;
      },
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
      state.tabContents.set(state.activeTab, state.editor.getValue());
      state.dirty.add(state.activeTab);
      updateSaveButton();
      if (isRunToolbarPath(state.activeTab)) refreshRunInfo();
      else if (state.activeTab?.endsWith('.java')) updateRunButtons();
      renderTabs();
      scheduleAutoSave();
      scheduleDiagnostics();
      if (isProjectBuildFile(state.activeTab)) {
        scheduleProjectReload();
      } else if (isProjectSourceFile(state.activeTab)) {
        scheduleAllJavaDiagnostics();
      }
      updateConflictUi();
      applyTestRunDecorations();
      if (state.activeTab) {
        state.fileCoverage.delete(state.activeTab);
        clearCoverageDecorations();
        updateCoverageStatus(null);
      }
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
      if (state.activeTab?.endsWith('.java')) updateRunButtons();
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
  state.repos = await api('/api/repos');
  setRepoPickerLabel(state.repo);
  if ($('#repo-picker-overlay')?.classList.contains('open')) {
    renderRepoPickerResults();
  }
  if (!state.tabs.length && !state.repo) {
    renderWelcome();
    $('#empty-state')?.classList.remove('hidden');
    syncWelcomeLayout();
  }
}

async function selectRepo(name) {
  if (!name) {
    state.repo = null;
    state.projectFolder = null;
    resetUI();
    updateWindowTitle();
    setRepoPickerLabel('');
    return;
  }

  const switching = state.repo !== name;
  if (switching) {
    closeWorkspaceTabs();
  }

  state.repo = name;
  resetTerminalCwds();
  const opened = await api(repoApi(name, '/workspace/open'), { method: 'POST' });
  state.projectProfile = opened?.profile || null;
  state.projectFolder = opened?.path || null;
  startProjectIndexPolling();
  updateProjectReloadButton();
  const detail = await api(repoApi(name));
  state.branches = normalizeBranchList(detail.branches);
  state.defaultBranch = resolveDefaultBranch(
    detail.default_branch || detail.summary?.default_branch || '',
    state.branches,
  );
  updateBranchSelect();
  updateDefaultBranchUi();
  updateRepoInfo(detail);
  enableControls();
  updateAgentUi();
  await refreshTree({ resetExpanded: true });
  await refreshGitStatus();
  await refreshHistory();
  try {
    await openFile('README.md');
  } catch {
    /* no readme in repo */
  }
  terminalLog(`Opened workspace: ${name}`);
  if (state.activeTab) {
    $('#empty-state')?.classList.add('hidden');
  } else {
    $('#empty-state')?.classList.remove('hidden');
  }
  syncWelcomeLayout();
  updateMenuState();
  updateWindowTitle();
  setRepoPickerLabel(name);
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
}

function enableControls() {
  $('#branch-picker-btn').disabled = false;
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
      <button id="btn-publish-github" type="button" class="w-full py-2 text-xs rounded border border-accent/40 text-accent hover:bg-accent/10">Publish to GitHub</button>
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
  await api(repoApi(state.repo), { method: 'DELETE' });
  toast(`Deleted ${state.repo}`, 'success');
  hideRepoInfoModal();
  state.repo = null;
  state.repoDetail = null;
  setRepoPickerLabel('');
  resetUI();
  await loadRepos();
}

async function createRepo(e) {
  e.preventDefault();
  const fd = new FormData(e.target);
  const body = {
    name: fd.get('name'),
    description: fd.get('description') || null,
    init_with_readme: fd.get('init_readme') === 'on',
  };
  try {
    const repo = await api('/api/repos', { method: 'POST', body: JSON.stringify(body) });
    hideModal();
    e.target.reset();
    await loadRepos();
    await selectRepo(repo.name);
    toast(`Created ${repo.name}`, 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

function showCloneModal(source = 'remote') {
  const nameInput = $('#clone-local-name');
  if (nameInput) nameInput.dataset.userEdited = '';
  setCloneModalTab(source);
  setCloneModalState({ busy: false, status: '', error: '' });
  $('#clone-modal-overlay')?.classList.remove('hidden');
  $('#clone-modal-overlay')?.classList.add('flex');
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

function showPublishModal() {
  if (!state.repo) {
    toast('Select a repository first', 'info');
    return;
  }
  const nameInput = $('#publish-github-repo');
  if (nameInput && !nameInput.value && !state.repo.includes('/')) {
    nameInput.placeholder = `your-github-user/${state.repo}`;
  }
  $('#publish-modal-overlay')?.classList.remove('hidden');
  $('#publish-modal-overlay')?.classList.add('flex');
  $('#publish-github-repo')?.focus();
}

function hidePublishModal() {
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

async function publishToGitHub(e) {
  e.preventDefault();
  if (!state.repo) {
    toast('Select a repository first', 'info');
    return;
  }
  if (!(await hasGitHubPat())) {
    toast('Add a GitHub PAT in Settings → Git hosts', 'error');
    showSettingsModal('git');
    return;
  }
  const fd = new FormData(e.target);
  const githubRepo = String(fd.get('github_repo') || '').trim();
  try {
    const body = {
      github_repo: githubRepo,
      create: fd.get('create') === 'on',
      private: fd.get('private') === 'on',
    };
    const out = await api(repoApi(state.repo, '/remote/publish'), {
      method: 'POST',
      body: JSON.stringify(body),
    });
    hidePublishModal();
    e.target.reset();
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
    toast(err.message, 'error');
  }
}

async function pushRemote() {
  await showPushModal();
}

function hidePushModal() {
  const overlay = $('#push-modal-overlay');
  overlay?.classList.add('hidden');
  overlay?.classList.remove('flex');
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
  confirm.textContent = preview.can_push ? 'Push' : 'Nothing to push';

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

  body.innerHTML = `
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
  overlay?.classList.remove('hidden');
  overlay?.classList.add('flex');
  if (body) body.innerHTML = '<p class="text-sm text-gray-500">Loading push preview…</p>';
  if (confirm) {
    confirm.disabled = true;
    confirm.textContent = 'Push';
  }
  $('#push-modal-ahead')?.classList.add('hidden');
  try {
    const preview = await api(repoApi(state.repo, '/remote/push/preview'));
    renderPushPreview(preview);
  } catch (err) {
    hidePushModal();
    toast(err.message, 'error');
  }
}

async function executePush() {
  if (!state.repo) return;
  const confirm = $('#push-modal-confirm');
  if (confirm) confirm.disabled = true;
  setGlobalLoading(true, 'Pushing…');
  try {
    const out = await api(repoApi(state.repo, '/remote/push'), { method: 'POST' });
    terminalLog(out.stdout || out.stderr || 'Pushed');
    hidePushModal();
    setStatusMessage(`Pushed ${state.currentBranch || 'branch'} to remote`);
    toast(
      out.exit_code === 0 ? 'Pushed to remote' : `Push failed (exit ${out.exit_code})`,
      out.exit_code === 0 ? 'success' : 'error',
    );
    await refreshGitStatus();
  } catch (err) {
    toast(err.message, 'error');
  } finally {
    if (confirm) confirm.disabled = false;
    setGlobalLoading(false);
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
let gradleInfoTimer = null;

async function loadTreeLevel(dirPath = '') {
  const q = dirPath ? `?dir=${encodeURIComponent(dirPath)}` : '';
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 30_000);
  try {
    const nodes = await api(repoApi(state.repo, `/workspace/tree${q}`), { signal: controller.signal });
    treeState.children.set(dirPath, nodes);
    return nodes;
  } catch (err) {
    if (err?.name === 'AbortError') {
      throw new Error('File tree request timed out — try Reload project or restart Reaper');
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

/** Load one lazy tree folder; dedupes concurrent requests for the same path. */
async function ensureTreeDirLoaded(dirPath) {
  if (!dirPath || treeState.recursiveNodes || !state.repo) return;
  if (treeState.children.has(dirPath)) return;
  if (treeLoadInflight.has(dirPath)) return treeLoadInflight.get(dirPath);

  treeState.loading.add(dirPath);
  const promise = (async () => {
    try {
      return await loadTreeLevel(dirPath);
    } catch (err) {
      treeState.expanded.delete(dirPath);
      toast(err.message, 'error');
      throw err;
    } finally {
      treeState.loading.delete(dirPath);
      treeLoadInflight.delete(dirPath);
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
    if (dir && !treeState.children.has(dir) && !treeLoadInflight.has(dir)) {
      void ensureTreeDirLoaded(dir);
    }
  }
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

let lastTreeNodes = [];

async function refreshTree(options = {}) {
  const { resetExpanded = false } = options;
  const savedExpanded = resetExpanded ? null : [...treeState.expanded];
  treeState.children.clear();
  treeState.loading.clear();
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
    (d) => d && !treeState.children.has(d) && !treeLoadInflight.has(d),
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
  if (!nodes.length) {
    treeEl.innerHTML = query
      ? '<p class="ij-empty-hint">No files match your filter</p>'
      : '<p class="ij-empty-hint">This repository has no files yet</p>';
    return;
  }
  treeEl.innerHTML = `<div class="ij-tree">${renderTree(nodes, 0, lazyMode)}</div>`;
  bindTreeEvents();
  if (state.activeTab) {
    $$('.tree-file').forEach((b) => b.classList.toggle('active', treePathMatchesTab(b.dataset.path, state.activeTab)));
  }
  scheduleTreeGapLoads();
}

function normalizeRepoPath(path) {
  return String(path || '').replace(/\\/g, '/').replace(/^\/+/, '');
}

/** Map javac overlay copies (`.reaper/java-diagnostics/overlay/…`) to workspace paths. */
function stripJavaDiagOverlayPath(path) {
  if (!path) return path;
  const normalized = path.replace(/\\/g, '/');
  const prefix = '.reaper/java-diagnostics/overlay/';
  if (normalized.startsWith(prefix)) return normalized.slice(prefix.length);
  const idx = normalized.indexOf(`/${prefix}`);
  if (idx >= 0) return normalized.slice(idx + prefix.length + 1);
  return normalized;
}

function workspaceExplorerPath(path) {
  return normalizeRepoPath(stripJavaDiagOverlayPath(path));
}

function treePathMatchesTab(treePath, tabPath) {
  if (!tabPath) return false;
  return workspaceExplorerPath(treePath) === workspaceExplorerPath(tabPath);
}

function treeParentDirPaths(filePath) {
  const parts = normalizeRepoPath(filePath).split('/').filter(Boolean);
  if (parts.length <= 1) return [];
  parts.pop();
  const out = [];
  for (let i = 0; i < parts.length; i++) {
    out.push(parts.slice(0, i + 1).join('/'));
  }
  return out;
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

  const isFile = /\.[^/]+$/.test(normalized.split('/').pop() || '');

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
  if (ext === 'py') return { type: 'python', label: 'Python' };
  if (ext === 'rs') return { type: 'rust', label: 'Rust' };
  if (ext === 'go') return { type: 'go', label: 'Go' };
  if (ext === 'rb') return { type: 'ruby', label: 'Ruby' };
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

  if (isFile) rows.push(treeContextMenuItem('open', 'Open'));

  switch (profile.type) {
    case 'dir':
      rows.push(treeContextMenuItem('new-file', 'New File…'));
      break;
    case 'java':
      if (state.runInfo?.has_project) {
        rows.push(treeContextMenuItem('run', 'Run'));
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
    case 'markdown':
    case 'json':
    case 'yaml':
    case 'xml':
    case 'javascript':
    case 'typescript':
    case 'python':
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

function runTreeContextAction(action) {
  const target = treeContextTarget;
  hideTreeContextMenu();
  if (!target?.path) return;
  const path = target.path;
  switch (action) {
    case 'open':
      void openFileFromTree(path);
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
  const target = state.runTarget;
  if (target?.mode === 'test' && target.filter) {
    await runProjectTestWithCoverage(target.filter);
  } else {
    toast('Open a test class or test method to run with coverage', 'info');
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
    if (btn?.dataset.path) openFileFromTree(btn.dataset.path);
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

async function openFile(path) {
  path = workspaceExplorerPath(path);
  if (state.tabs.includes(path)) {
    try {
      if (!state.tabContents.has(path)) await hydrateTabContent(path);
    } catch (err) {
      toast(err.message, 'error');
    }
    activateTab(path);
    navigateToPrimarySource(path);
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
    navigateToPrimarySource(path);
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
    toast(err.message, 'error');
  }
}

function renderTabs() {
  const list = $('#tab-list');
  if (!list || !state.tabs.length) return;
  const tabsHtml = state.tabs.map((t) => {
    const name = t.split('/').pop();
    const active = state.activeTab === t ? ' active' : '';
    const dirty = state.dirty.has(t) ? ' dirty' : '';
    return `<div class="ij-tab${active}${dirty}" data-tab="${t}"><span class="ij-tab-label">${name}</span><button type="button" class="ij-tab-close" data-close="${t}" title="Close">×</button></div>`;
  }).join('');
  list.innerHTML = tabsHtml;
  $$('.ij-tab').forEach((tab) => {
    tab.addEventListener('click', (e) => {
      if (e.target.closest('.ij-tab-close')) return;
      activateTab(tab.dataset.tab);
    });
  });
  $$('.ij-tab-close').forEach((btn) => {
    btn.addEventListener('click', (e) => closeTab(btn.dataset.close, e));
  });
  updateRunButtons();
}

function closeTab(path, e) {
  e?.stopPropagation();
  const idx = state.tabs.indexOf(path);
  if (idx < 0) return;
  if (state.dirty.has(path) && !confirm(`Discard changes to ${path.split('/').pop()}?`)) return;
  state.tabs.splice(idx, 1);
  state.tabContents.delete(path);
  state.dirty.delete(path);
  if (state.activeTab === path) {
    const next = state.tabs[idx] ?? state.tabs[idx - 1] ?? null;
    if (next) activateTab(next);
    else closeAllTabs();
  } else {
    renderTabs();
  }
}

function activateTabShell(path) {
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
  const langEl = $('#status-language');
  if (langEl) {
    const lang = window.ReaperLang?.langLabel?.(langForPath(path)) || 'Plain Text';
    const compilers = window.ReaperLang?.compilerLabelsForPath?.(path);
    langEl.textContent = compilers ? `${lang} · ${compilers}` : lang;
    langEl.title = compilers ? `Language: ${lang}. Compiler(s): ${compilers}` : `Language: ${lang}`;
  }
  if (state.editor) updateEditorStatus(state.editor.getPosition());
  $$('.tree-file').forEach((b) => b.classList.toggle('active', treePathMatchesTab(b.dataset.path, path)));
  updateSaveButton();
  scheduleGradleInfoRefresh();
  if (path) void refreshLanguageContextForPath(path);
  updateMenuState();
  setStatusMessage(path.split('/').pop() || path);
  fileDiags = [];
  diagJumpIndex = 0;
  updateDiagnosticsStatusBar(path, []);
  scheduleDiagnostics();
  void revealFileInExplorer(path);
  updateTreeBackButton();
  updateConflictUi();
}

function activateTab(path) {
  activateTabShell(path);
  if (state.tabContents.has(path)) {
    setEditorContent(path, state.tabContents.get(path));
  }
  renderTabs();
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
  // Keep existing squiggles until the next compile finishes (no flicker while typing).
  diagTimer = setTimeout(runDiagnostics, DIAG_DELAY_MS);
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
  if (msgLower.includes('cannot find symbol') || msgLower.includes('package') && msgLower.includes('does not exist')) {
    const sym = msg.match(/symbol:\s*(?:class|interface|variable|method|package)?\s*([A-Za-z_][\w.]*)/i)
      || msg.match(/package\s+([A-Za-z_][\w.]*)/i);
    if (sym?.[1]) {
      const idx = lineText.indexOf(sym[1]);
      if (idx >= 0) {
        startCol = idx + 1;
        return {
          startLineNumber: line,
          startColumn: startCol,
          endLineNumber: line,
          endColumn: startCol + sym[1].length,
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
  if (pop) pop.classList.add('hidden');
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

function showQuickFixMenu(fixes, onPick, anchorEl) {
  const pop = $('#ai-quickfix-popover');
  const anchor = anchorEl || aiFixMenuAnchor();
  if (!pop || !fixes?.length) return;
  pop.innerHTML = fixes.map((f, i) => {
    const title = quickFixMenuLabel(f);
    const loading = f?.provider === 'loading' || !f?.edits?.length;
    if (loading) {
      return `<div class="ij-quickfix-item ij-quickfix-item--loading" data-idx="${i}">${title}</div>`;
    }
    return `<button type="button" class="ij-quickfix-item" data-idx="${i}">${title}</button>`;
  }).join('');
  pop.classList.remove('hidden');
  if (anchor) {
    const rect = anchor.getBoundingClientRect();
    pop.style.left = `${Math.max(8, Math.min(rect.left, window.innerWidth - 240))}px`;
    if (rect.top < 72) {
      pop.style.top = `${rect.bottom + 6}px`;
    } else {
      pop.style.top = `${rect.top - 8}px`;
      requestAnimationFrame(() => {
        const popRect = pop.getBoundingClientRect();
        pop.style.top = `${Math.max(8, rect.top - popRect.height - 6)}px`;
        if (parseFloat(pop.style.top) < 8) {
          pop.style.top = `${rect.bottom + 6}px`;
        }
      });
    }
  }
  pop.querySelectorAll('.ij-quickfix-item:not(.ij-quickfix-item--loading)').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const fix = fixes[Number(btn.dataset.idx)];
      hideQuickFixMenu();
      if (fix?.edits?.length) onPick(fix);
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
  const path = state.activeTab;
  const content = state.editor.getValue();
  const seq = ++diagSeq;
  try {
    const diags = await fetchDiagnosticsForPath(path, content);
    if (seq !== diagSeq || path !== state.activeTab) return;
    applyDiagnostics(path, Array.isArray(diags) ? diags : []);
  } catch {
    if (seq === diagSeq) clearDiagnostics();
  }
}

function scheduleAllJavaDiagnostics(delayMs = ALL_JAVA_DIAG_DELAY_MS) {
  if (!state.repo) return;
  clearTimeout(allJavaDiagTimer);
  allJavaDiagTimer = setTimeout(() => {
    allJavaDiagTimer = null;
    void refreshAllJavaTabDiagnostics();
  }, delayMs);
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

async function fetchDiagnosticsForPath(path, content) {
  return api(repoApi(state.repo, '/workspace/diagnostics'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      path,
      content,
      overlays: collectJavaDiagnosticOverlays(path),
    }),
  });
}

/** Re-validate imports/diagnostics on every open Java tab after classpath changes. */
async function refreshAllJavaTabDiagnostics() {
  if (!state.repo) return;
  const javaTabs = state.tabs.filter((p) => isDiagnosablePath(p));
  if (!javaTabs.length) return;
  const activePath = state.activeTab;
  await Promise.all(javaTabs.map(async (path) => {
    const content = path === activePath && state.editor
      ? state.editor.getValue()
      : (state.tabContents.get(path) ?? '');
    try {
      const diags = await fetchDiagnosticsForPath(path, content);
      if (path === state.activeTab) {
        applyDiagnostics(path, Array.isArray(diags) ? diags : []);
      }
    } catch { /* ignore transient errors */ }
  }));
}

function refreshProjectClasspathUi() {
  void refreshAllJavaTabDiagnostics();
  if (hasAutoReloadProject()) void refreshRunInfo();
  if (state.activeTab?.endsWith('.java')) applyTestRunDecorations();
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
      : monaco.MarkerSeverity.Error,
    message: d.message,
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

function runTargetLabel(target, content) {
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

function isRunToolbarPath(path) {
  if (!path) return false;
  if (isGradleFilePath(path) || isMavenFilePath(path)) return true;
  if (path.endsWith('.java') && state.runInfo?.has_project) return true;
  return false;
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
  if (state.activeTab?.endsWith('.java')) applyTestRunDecorations();
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
  const canRun = target.runnable || showTaskPicker;

  if ((showTaskPicker || showJavaRun) && canRun) {
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
      runLabel.textContent = runTargetLabel(target, content);
    }
    gradleSep?.classList.remove('hidden');

    if (target.mode === 'main') {
      state.javaRunTarget = target.qualifiedName;
    }
  } else if (showJavaRun && target.reason) {
    taskSel?.classList.add('hidden');
    runLabel?.classList.remove('hidden');
    runLabel.textContent = runTargetLabel(target, content) || 'Not runnable';
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
  if (tbFormat) tbFormat.disabled = !state.activeTab;
  if (tbSave) tbSave.disabled = !state.activeTab || !state.dirty.has(state.activeTab);
  updateRollbackButton();
  updateProjectReloadButton();
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
      await openFile(path);
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

async function formatDocument() {
  if (!state.editor || !state.activeTab) return;
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
  closeAllTabs();
}

async function saveFile(options = {}) {
  const { silent = false, skipProjectReload = false } = options;
  if (!state.activeTab || !state.editor) return;
  const savedPath = state.activeTab;
  const content = state.editor.getValue();
  try {
    await api(repoApi(state.repo, '/workspace/file'), {
      method: 'PUT',
      body: JSON.stringify({ path: savedPath, content }),
    });
    state.tabContents.set(savedPath, content);
    state.dirty.delete(savedPath);
    updateSaveButton();
    renderTabs();
    if (!silent) {
      await refreshTree();
      await refreshGitStatus();
      toast('Saved', 'success');
    }
    if (!skipProjectReload && isProjectClasspathFile(savedPath) && hasAutoReloadProject()) {
      if (isProjectSourceFile(savedPath)) {
        window.ReaperLang?.clearCompletionCache?.();
        scheduleAllJavaDiagnostics(0);
      }
      scheduleProjectReload(0);
    }
  } catch (err) {
    if (!silent) toast(err.message || 'Failed to save', 'error');
  }
}

function scheduleAutoSave() {
  if (!getAutoSaveEnabled() || !state.repo || !state.activeTab) return;
  if (state.activeTab.startsWith('.reaper/')) return;
  if (!state.dirty.has(state.activeTab)) return;
  clearTimeout(state.autoSaveTimer);
  state.autoSaveTimer = setTimeout(async () => {
    if (state.dirty.has(state.activeTab)) {
      await saveFile({ silent: true });
      await refreshGitStatus();
    }
  }, AUTO_SAVE_DELAY_MS);
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
  const dirty = !!(state.activeTab && state.dirty.has(state.activeTab));
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
  const label = `▶ ${toolLabel} ${task}  (${where})`;
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
  const label = `▶ ${toolLabel} ${task}  (${state.activeTab})`;
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
  const toolLabel = info.build_tool === 'maven' ? 'mvn' : 'gradle';
  const label = `◔ ${toolLabel} ${task} + jacoco  (${state.activeTab})`;
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

async function runActive() {
  if (!state.repo || !state.activeTab) return;
  await refreshRunInfo();
  const target = state.runTarget;
  if (!target || target.mode === 'none') return;
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
    default:
      break;
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
      { label: `▶ java ${name}`, terminalId: term.id },
    );
  } catch (e) {
    terminalLog(`error: ${e.message}`);
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

  const commitDisabled = rows.length === 0 || mergeBlocked || selectedCount === 0;
  const commitOnlyBtn = $('#btn-commit-only');
  const commitPushBtn = $('#btn-commit-push');
  const suggestBtn = $('#btn-suggest-commit');
  if (commitOnlyBtn) commitOnlyBtn.disabled = commitDisabled;
  if (commitPushBtn) commitPushBtn.disabled = commitDisabled;
  if (suggestBtn) suggestBtn.disabled = commitDisabled;
}

async function refreshGitStatus() {
  if (!state.repo) {
    updateGitNavUi({ ahead: 0 });
    return { clean: true, files: [], branch: '', ahead: 0 };
  }
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
  const header = $('#commit-log-header');
  if (header) {
    header.textContent = commits.length
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
    </button>`;
  }).join('');
  list.querySelectorAll('.ij-commit-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      showCommitDiff(btn.dataset.hash, btn.dataset.subject);
    });
  });
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

  for (let attempt = 0; attempt < 5; attempt++) {
    if (attempt) await new Promise((r) => setTimeout(r, 180 * attempt));
    const status = await refreshGitStatus();
    if (status.files?.length) {
      await followAgentFileChanges(status);
      await reloadOpenTabsFromDisk();
      return;
    }
  }
}

async function refreshAfterAgent({ fromAgent = false, final = false } = {}) {
  await refreshTree();
  const status = await refreshGitStatus();
  await reloadOpenTabsFromDisk();
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
  const out = await api(repoApi(state.repo, '/workspace/commit'), {
    method: 'POST',
    body: JSON.stringify({ message, paths, push }),
  });
  if (out.exit_code !== 0) {
    terminalLog(out.stderr || out.stdout || 'Commit failed');
    toast(out.stderr?.trim() || out.stdout?.trim() || 'Commit failed', 'error');
    return;
  }
  terminalLog(out.stdout || out.stderr || (push ? 'Committed and pushed' : 'Committed'));
  $('#commit-message').value = '';
  state.commitKnownPaths.clear();
  await refreshGitStatus();
  await refreshHistory();
  await refreshTree();
  toast(push ? 'Committed & pushed' : 'Committed locally — use Push when ready', push ? 'success' : 'success');
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
  const out = await api(repoApi(state.repo, '/workspace/checkout'), {
    method: 'POST',
    body: JSON.stringify({ branch }),
  });
  terminalLog(out.stdout || out.stderr || `Switched to ${branch}`);
  await refreshTree();
  await refreshGitStatus();
  await refreshHistory();
  startProjectIndexPolling();
}

// --- Terminal (xterm + PTY WebSocket) ---
let terminalNextNum = 1;

const TERM_SOURCE_EXT = 'java|kt|kts|scala|groovy|gradle|xml|properties|json|yaml|yml|rs|py|js|ts|tsx|jsx|go|rb|cs|cpp|c|h|hpp|md|sql|html|css|vue|swift|php|sh|toml|proto';
const TERM_FILE_PATH = `[A-Za-z0-9_.@\\[\\]-]+(?:[\\/][A-Za-z0-9_.@\\[\\]-]+)*\\.(?:${TERM_SOURCE_EXT})`;

function resolveTerminalFilePath(rawPath) {
  let p = workspaceExplorerPath(String(rawPath || '').trim());
  if (!p || /^https?:\/\//i.test(p)) return null;

  const projectFolder = (state.projectFolder || '').replace(/\\/g, '/').replace(/\/$/, '');
  if (projectFolder && (p === projectFolder || p.startsWith(`${projectFolder}/`))) {
    p = p === projectFolder ? '' : p.slice(projectFolder.length + 1);
  } else if (p.startsWith('/') || /^[A-Za-z]:\//.test(p)) {
    const markers = ['/src/', '/test/', '/main/', '/java/', '/kotlin/', '/resources/'];
    for (const mark of markers) {
      const idx = p.indexOf(mark);
      if (idx >= 0) {
        p = p.slice(idx + 1);
        break;
      }
    }
    if (p.startsWith('/') || /^[A-Za-z]:\//.test(p)) {
      const tail = p.match(/\/((?:src|test)\/.+)$/);
      if (!tail) return null;
      p = tail[1];
    }
  }

  p = normalizeRepoPath(p.replace(/^\.\//, ''));
  if (!p || !/\.\w+$/.test(p)) return null;
  return p;
}

function terminalLinkRange(match) {
  const path = match[1];
  const start = match.index + match[0].indexOf(path);
  if (match[0].includes('[')) {
    return { start, end: start + match[0].slice(match[0].indexOf(path)).length };
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
      const { start, end } = terminalLinkRange(match);
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

function xtermApi() {
  const Terminal = globalThis.Terminal;
  const FitAddon = globalThis.FitAddon?.FitAddon;
  if (!Terminal || !FitAddon) return null;
  return { Terminal, FitAddon };
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
  disconnectTerminalWs(term);
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
}

function spawnTerminalInstance(term, host) {
  if (!term || !host) return;
  destroyTerminalInstance(term);
  initTerminalXterm(term, host);
}

function resetTerminalCwds() {
  const host = $('#terminal-xterm-host');
  state.terminals.forEach((t) => {
    t.cwd = '';
    if (host) spawnTerminalInstance(t, host);
    else destroyTerminalInstance(t);
  });
}

function fitActiveTerminal() {
  fitTerminal(getActiveTerminal());
}

function fitTerminal(term) {
  if (!term?.fitAddon || !term.xterm) return;
  try {
    term.fitAddon.fit();
    sendTerminalResize(term);
  } catch {
    /* host may be hidden */
  }
}

function sendTerminalResize(term) {
  if (!term?.ws || term.ws.readyState !== WebSocket.OPEN || !term.xterm) return;
  term.ws.send(JSON.stringify({
    type: 'resize',
    cols: term.xterm.cols,
    rows: term.xterm.rows,
  }));
}

function terminalWsUrl(term) {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  let url = `${proto}//${location.host}/api/repos/${encodeURIComponent(state.repo)}/workspace/terminal`;
  if (term.cwd) url += `?cwd=${encodeURIComponent(term.cwd)}`;
  return url;
}

function disconnectTerminalWs(term) {
  if (!term?.ws) return;
  try { term.ws.close(); } catch { /* ignore */ }
  term.ws = null;
}

function connectTerminalWs(term) {
  if (!state.repo || !term) return;
  disconnectTerminalWs(term);
  const ws = new WebSocket(terminalWsUrl(term));
  term.ws = ws;
  ws.binaryType = 'arraybuffer';
  ws.onopen = () => fitTerminal(term);
  ws.onmessage = (ev) => {
    if (!term.xterm) return;
    if (ev.data instanceof ArrayBuffer) {
      term.xterm.write(new Uint8Array(ev.data));
    } else if (typeof ev.data === 'string') {
      term.xterm.write(ev.data);
    }
  };
  ws.onclose = () => {
    if (term.xterm) {
      term.xterm.write('\r\n\x1b[90m[session ended]\x1b[0m\r\n');
    }
  };
}

function ensureTerminalPane(term, host) {
  if (term.container?.isConnected) return term.container;
  const pane = document.createElement('div');
  pane.className = 'ij-terminal-xterm-pane hidden';
  pane.dataset.terminalId = term.id;
  host.appendChild(pane);
  term.container = pane;
  return pane;
}

function initTerminalXterm(term, host) {
  const api = xtermApi();
  if (!api || !host || !term) return;

  const pane = ensureTerminalPane(term, host);
  const xterm = new api.Terminal({
    cursorBlink: true,
    convertEol: true,
    fontSize: getEditorFontSize(),
    lineHeight: 1.28,
    fontFamily: getEditorFontSpec().family,
    theme: terminalThemeFromApp(),
    scrollback: 8000,
  });
  const fitAddon = new api.FitAddon();
  xterm.loadAddon(fitAddon);
  xterm.open(pane);
  registerTerminalFileLinkProvider(term, xterm);
  xterm.onData((data) => {
    if (term.ws?.readyState === WebSocket.OPEN) {
      term.ws.send(data);
    }
  });
  term.xterm = xterm;
  term.fitAddon = fitAddon;
  fitTerminal(term);
  connectTerminalWs(term);
}

function mountActiveTerminal({ fresh = false } = {}) {
  ensureTerminals();
  const term = getActiveTerminal();
  const host = $('#terminal-xterm-host');
  if (!host || !term) return;

  state.terminals.forEach((t) => {
    if (t.container) t.container.classList.add('hidden');
  });

  if (fresh || !term.xterm || !term.ws || term.ws.readyState === WebSocket.CLOSED) {
    spawnTerminalInstance(term, host);
  } else {
    ensureTerminalPane(term, host);
    term.container.classList.remove('hidden');
    fitTerminal(term);
  }
  term.container?.classList.remove('hidden');
  term.xterm?.focus();
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

function terminalWrite(term, text) {
  if (!text) return;
  const t = term || getActiveTerminal();
  if (!t?.xterm) return;
  const normalized = String(text).replace(/\r?\n/g, '\r\n');
  t.xterm.write(normalized + (normalized.endsWith('\r\n') ? '' : '\r\n'));
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

function colorizeStreamLine(line) {
  const trimmed = line.trimEnd();
  if (!trimmed) return line;
  if (/^BUILD SUCCESSFUL/i.test(trimmed)) {
    return `${TERM_ESC.green}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^BUILD FAILED/i.test(trimmed)) {
    return `${TERM_ESC.red}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^> Task /i.test(trimmed)) return `${TERM_ESC.dim}${line}${TERM_ESC.reset}`;
  if (/FAILED/i.test(trimmed) && !/UP-TO-DATE/i.test(trimmed)) {
    return `${TERM_ESC.red}${line}${TERM_ESC.reset}`;
  }
  if (/\bPASSED\b/i.test(trimmed) || /^Tests run:/i.test(trimmed)) {
    return `${TERM_ESC.green}${line}${TERM_ESC.reset}`;
  }
  if (/^FAILURE:/i.test(trimmed) || /^error:/i.test(trimmed)) {
    return `${TERM_ESC.red}${line}${TERM_ESC.reset}`;
  }
  if (/^\* What went wrong:/i.test(trimmed)) {
    return `${TERM_ESC.yellow}${TERM_ESC.bold}${line}${TERM_ESC.reset}`;
  }
  if (/^warning:/i.test(trimmed) || /^WARNING:/i.test(trimmed)) {
    return `${TERM_ESC.yellow}${line}${TERM_ESC.reset}`;
  }
  if (/^> /i.test(trimmed) && /\bcompile\b|\btest\b/i.test(trimmed)) {
    return `${TERM_ESC.magenta}${line}${TERM_ESC.reset}`;
  }
  if (/^\$ /.test(trimmed)) return `${TERM_ESC.cyan}${line}${TERM_ESC.reset}`;
  return line;
}

function writeColorizedStreamChunk(term, text) {
  if (!term?.xterm || !text) return;
  const combined = `${term.streamColorPartial || ''}${text}`;
  const parts = combined.split('\n');
  term.streamColorPartial = combined.endsWith('\n') ? '' : (parts.pop() || '');
  let out = '';
  for (const line of parts) {
    out += `${colorizeStreamLine(line)}\n`;
  }
  if (out) term.xterm.write(out.replace(/\n/g, '\r\n'));
}

function terminalLog(text, terminalId) {
  terminalWrite(terminalForId(terminalId) || getActiveTerminal(), text);
}

function terminalCommandBegin(label, terminalId, { kind } = {}) {
  const term = resolveTerminal(terminalId);
  if (!term?.xterm) return;
  term.streamColorPartial = '';
  term.xterm.write('\r\n');
  term.commandStartLine = terminalBufferLine(term);
  const labelText = String(label || '').trim() || 'command';
  const isTest = kind === 'test' || (/\btest\b/i.test(labelText) && labelText.includes('▶'));
  const accent = isTest ? TERM_ESC.brightCyan : TERM_ESC.cyan;
  term.xterm.write(`${TERM_ESC.dim}╭─ run ─${TERM_ESC.reset}\r\n`);
  term.xterm.write(`${accent}${TERM_ESC.bold}${labelText}${TERM_ESC.reset}\r\n`);
  scrollTerminalToLine(term, term.commandStartLine);
  term.xterm.focus();
}

function terminalCommandEnd(exitCode, terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term?.xterm) return;
  if (term.streamColorPartial) {
    term.xterm.write(term.streamColorPartial.replace(/\n/g, '\r\n'));
    term.streamColorPartial = '';
  }
  if (typeof exitCode === 'number') {
    if (exitCode === 0) {
      term.xterm.write(`${TERM_ESC.green}${TERM_ESC.bold}✓ finished (exit 0)${TERM_ESC.reset}\r\n`);
    } else {
      term.xterm.write(`${TERM_ESC.red}${TERM_ESC.bold}✗ failed (exit ${exitCode})${TERM_ESC.reset}\r\n`);
    }
  }
  term.xterm.write(`${TERM_ESC.dim}╰─ done ─${TERM_ESC.reset}\r\n\r\n`);
  if (typeof exitCode === 'number' && exitCode !== 0) {
    try { term.xterm.scrollToBottom(); } catch { /* ignore */ }
  } else {
    scrollTerminalToLine(term, term.commandStartLine ?? terminalBufferLine(term));
  }
  term.xterm.focus();
}

function beginTerminalStream(terminalId) {
  const term = resolveTerminal(terminalId);
  if (!term) return;
  term.streamLine = '';
  term.streamColorPartial = '';
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
  term.streamLine = null;
  term.streamColorPartial = '';
}

function clearActiveTerminal() {
  const term = getActiveTerminal();
  if (!term?.xterm) return;
  term.xterm.clear();
  term.streamLine = null;
  term.streamColorPartial = '';
  term.xterm.focus();
}

function restartActiveTerminal() {
  const term = getActiveTerminal();
  const host = $('#terminal-xterm-host');
  if (!term || !host) return;
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

async function postWorkspaceExecStream(path, body, terminalId) {
  beginTerminalStream(terminalId);
  const res = await fetch(repoApi(state.repo, path), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
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
  focusCommandTerminal(termId);
  if (label && termId) terminalCommandBegin(label, termId, { kind });
  try {
    const exitCode = await postWorkspaceExecStream(path, body, termId);
    const output = terminalForId(termId)?.streamLine || '';
    finalizeTerminalStream(termId);
    if (termId) terminalCommandEnd(exitCode, termId);
    return { exitCode, output };
  } catch (e) {
    finalizeTerminalStream(termId);
    if (termId) terminalCommandEnd(-1, termId);
    throw e;
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
    role === 'user' ? 'agent-msg-user text-gray-200' :
    role === 'assistant'
      ? `agent-msg-assistant text-gray-300 agent-provider-${provider}`
      : 'agent-msg-system'
  }`;
  if (role === 'assistant') {
    wrap.dataset.agentProvider = provider;
  }

  if (role !== 'system') {
    const label = document.createElement('div');
    label.className = `agent-msg-label text-[10px] uppercase tracking-wide mb-1 ${providerDef.labelClass}`;
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
  for (const m of models) {
    ids.add(m.id);
    const opt = document.createElement('option');
    opt.value = m.id;
    opt.textContent = m.label || m.id;
    if (m.description) opt.title = m.description;
    select.appendChild(opt);
  }
  if (currentId && !ids.has(currentId)) {
    const opt = document.createElement('option');
    opt.value = currentId;
    opt.textContent = currentId;
    select.appendChild(opt);
  }
  select.value = currentId;
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
    container.appendChild(select);
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
  return def.isReady();
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
    status = queued ? `Working… · ${queued} queued` : 'Working…';
  } else if (!state.repo) {
    status = 'Select a repo';
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
      workText.textContent = `Working… · ${queued} queued`;
    } else if (state.agentBusy) {
      workText.textContent = 'Working…';
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
    if (state.agentProvider === 'cursor') renderAgentProviderControls();
    return;
  }
  try {
    const data = await api('/api/cursor/models');
    state.cursorModels = data.models || [];
  } catch {
    /* keep fallback list */
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
    state.cursorModel = cfg.model || 'composer-2.5';
    state.cursorMode = cfg.mode || 'agent';
    $$('[data-agent-mode]').forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.agentMode === state.cursorMode);
    });
    await loadCursorModels();
    refreshAgentProviderUi();
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
    toast(cfg.bridge_ok ? 'Bridge connected' : (cfg.bridge_error || 'Bridge still offline'), cfg.bridge_ok ? 'success' : 'error');
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

  $$('[data-terminal-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.terminalDock === dock);
  });
  syncDockMenuControls();

  syncActivityButtons();
  updateStatusBar();
  if (showTerminal) {
    requestAnimationFrame(() => fitActiveTerminal());
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
  mountActiveTerminal({ fresh: !wasOpen });
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
    mountActiveTerminal({ fresh: !wasOpen });
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
  state.agentAbortController = new AbortController();
  state.agentLiveFollow = !!def.capabilities.liveFollow && state.cursorMode === 'agent';
  state.agentLiveDiffPath = null;
  state.agentLastToolPath = null;
  state.agentSeenPaths = new Set();
  state.agentHadFileChanges = false;
  const pathsBefore = await snapshotAgentWorkspacePaths();
  updateAgentUi();
  if (state.agentLiveFollow) showAgentDiffPlaceholder();

  const { wrap: assistantWrap, content: assistantEl } = appendAgentMessage('assistant', '…', { provider: chatProvider });
  let buffer = '';
  let textBuffer = '';
  let doneSummary = null;
  let cancelled = false;

  try {
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
          scheduleAgentMarkdownPreview(assistantEl, textBuffer);
          scrollAgentToBottom();
        } else if (data.type === 'tool') {
          buffer += data.text;
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
      assistantWrap.classList.add('text-gray-500');
      window.ReaperAgentMarkdown?.renderPlain(assistantEl, buffer || 'Stopped.');
    } else {
      await finalizeAgentMessage(assistantEl, { textBuffer, buffer, summary: doneSummary });
    }
    const postStatus = await api(repoApi(state.repo, '/workspace/status'));
    const revertPaths = await collectAgentRevertPaths(pathsBefore, postStatus, state.agentSeenPaths);
    if (revertPaths.length || userWrap || assistantWrap) {
      state.agentLastRevertibleTurn = {
        userWrap,
        assistantWrap,
        paths: revertPaths,
      };
    }
    await refreshAfterAgent({
      fromAgent: true,
      final: !cancelled && !state.agentStopRequested,
    });
  } catch (e) {
    if (e.name === 'AbortError' || state.agentStopRequested) {
      assistantWrap.classList.add('text-gray-500');
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

  const panelChanged = state.activePanel !== name;
  state.activePanel = name;
  syncActivityButtons();
  const titles = {
    explorer: 'Project',
    git: 'Commit',
    history: 'Git Log',
    terminal: 'Terminal',
    agent: 'Agent',
  };
  $('#sidebar-title').textContent = titles[name] || name;
  $$('#sidebar > .panel').forEach((p) => {
    if (p.id === 'panel-agent' || p.id === 'panel-terminal') return;
    p.classList.toggle('hidden', p.id !== `panel-${name}`);
  });
  applyAgentDock();
  applyTerminalDock();
  if (name === 'git') refreshGitStatus();
  else if (name === 'history') refreshHistory();
  if (name === 'agent') {
    loadCursorStatus();
    setTimeout(() => $('#agent-input')?.focus(), 50);
  }
  if (name === 'terminal') {
    mountActiveTerminal({ fresh: panelChanged });
  }
}

function showModal() {
  $('#modal-overlay').classList.remove('hidden');
  $('#modal-overlay').classList.add('flex');
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
  $('#btn-coverage-close')?.addEventListener('click', hideCoveragePanel);
  $('#btn-coverage-refresh')?.addEventListener('click', () => void refreshCoveragePanel(state.activeTab));
  $('#btn-coverage-open-html')?.addEventListener('click', () => void openCoverageHtmlReport());
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
  $('#clone-modal-cancel')?.addEventListener('click', hideCloneModal);
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
  $('#publish-repo-form')?.addEventListener('submit', publishToGitHub);
  $('#new-file-form').addEventListener('submit', createFile);
  $('#btn-save')?.addEventListener('click', saveFile);
  $('#tb-save')?.addEventListener('click', saveFile);
  $('#tb-format')?.addEventListener('click', formatDocument);
  $('#tb-rollback')?.addEventListener('click', rollbackLastChange);
  $('#tb-reload-project')?.addEventListener('click', () => reloadProjectIndex());
  $('#btn-reload-project')?.addEventListener('click', () => reloadProjectIndex());
  $('#tb-run')?.addEventListener('click', runActive);
  $('#gradle-task')?.addEventListener('change', () => updateRunButtons());
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
  $('#btn-agent-retry')?.addEventListener('click', restartBridge);
  $$('[data-agent-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setAgentDock(btn.dataset.agentDock));
  });
  $$('[data-terminal-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setTerminalDock(btn.dataset.terminalDock));
  });
  bindTerminalTabs();
  window.addEventListener('resize', () => {
    if (state.terminalOpen || state.activePanel === 'terminal') {
      fitActiveTerminal();
    }
  });

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
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      e.preventDefault();
      saveFile();
    }
    if (e.key === 'F5') {
      e.preventDefault();
      runActive();
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
  setStatusMessage('Ready');
  if (!window.ReaperAgentMarkdown?.libsReady?.()) {
    console.error('[Reaper] Agent markdown not ready — tables/diagrams will show as plain text until scripts load.');
  }
  populateFontSizeSelects();
  populateFontFamilySelects();
  populateAgentFontSelects();
  applyUiTypography();
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
  bindSearchEverywhere();
  bindBranchPicker();
  bindRepoPicker();
  mountReaperIcons();
  void initStatusFooter();
  initSidebarResize();
  initAgentDockResize();
  initTerminalBottomResize();
  applyAgentDock();
  applyTerminalDock();
  switchPanel('explorer');
  renderWelcome();
  $('#empty-state')?.classList.remove('hidden');
  syncWelcomeLayout();
  void loadCursorStatus();
  void loadGeminiSettingsSection();
  try {
    await loadRepos();
    void initStatusFooter();
  } catch (err) {
    toast(`Could not reach Reaper backend: ${err.message}. Quit other Reaper copies and relaunch.`, 'error', { duration: 15000 });
  }
  const initialRepo = getInitialRepoFromUrl();
  if (!initialRepo && !state.repo) {
    showNoRepoFileTree();
    try {
      const general = await api('/api/settings/general');
      const defaultRepo = general?.default_repo;
      if (defaultRepo && state.repos.some((r) => r.name === defaultRepo)) {
        await selectRepo(defaultRepo);
      }
    } catch {
      /* settings unavailable */
    }
  }
  if (initialRepo && state.repos.some((r) => r.name === initialRepo)) {
    await selectRepo(initialRepo);
  }
  hideLaunchSplash();
  setInterval(async () => {
    if (state.cursorConfigured && !state.cursorBridgeOk && !state.agentBusy) {
      await loadCursorStatus();
    }
  }, 3000);
}

init().catch((e) => {
  toast(e.message, 'error');
  hideLaunchSplash();
});
