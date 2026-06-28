const AGENT_DOCK_KEY = 'reaper-agent-dock';
const TERMINAL_DOCK_KEY = 'reaper-terminal-dock';
const TERMINAL_BOTTOM_HEIGHT_KEY = 'reaper-terminal-bottom-height';
const EDITOR_FONT_SIZE_KEY = 'reaper-editor-font-size';
const EDITOR_FONT_FAMILY_KEY = 'reaper-editor-font-family';
const AGENT_FONT_SIZE_KEY = 'reaper-agent-font-size';
const AGENT_FONT_FAMILY_KEY = 'reaper-agent-font-family';
const AGENT_FONT_MATCH_EDITOR_KEY = 'reaper-agent-font-match-editor';
const AUTO_SAVE_KEY = 'reaper-auto-save';
const SHOW_DOTFILES_KEY = 'reaper-show-dotfiles';
const NEW_WINDOW_ON_REPO_KEY = 'reaper-new-window-on-repo';
const AUTO_SAVE_DELAY_MS = 800;
const DIAG_DELAY_MS = 1200;
const PROJECT_INDEX_POLL_MS = 2000;
const DEFAULT_EDITOR_FONT_SIZE = 13;
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
  editorReady: false,
  suppressEditorChange: false,
  autoSaveTimer: null,
  gradleInfo: null,
  repoDetail: null,
  projectIndexPoll: null,
  projectIndexNotified: false,
  projectIndexRunning: false,
  projectIndexReady: false,
  projectIndexStartedAt: 0,
  projectProfile: null,
  treeNavAnchor: null,
  mergeState: null,
  conflictDecorationIds: [],
  testRunWidgets: [],
  testMethodsByLine: new Map(),
  conflictFiles: new Set(),
  selectedCommitHash: null,
  mainView: 'editor',
  conflictPanelHidden: false,
  geminiConfigured: false,
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

function toast(msg, type = 'info', { duration } = {}) {
  const el = $('#toast');
  el.textContent = msg;
  el.className = `ij-toast ${type}`;
  el.classList.remove('hidden');
  clearTimeout(toast._timer);
  const ms = duration ?? (type === 'error' ? 9000 : 3500);
  toast._timer = setTimeout(() => el.classList.add('hidden'), ms);
}

function setGlobalLoading(on, text = 'Loading…') {
  const overlay = $('#loading-overlay');
  const label = $('#loading-text');
  if (label) label.textContent = text;
  overlay?.classList.toggle('hidden', !on);
  overlay?.classList.toggle('flex', on);
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
  return Math.round(size * (20 / 13));
}

function getEditorFontSize() {
  const n = parseInt(localStorage.getItem(EDITOR_FONT_SIZE_KEY), 10);
  if (Number.isFinite(n) && n >= MIN_EDITOR_FONT_SIZE && n <= MAX_EDITOR_FONT_SIZE) return n;
  return DEFAULT_EDITOR_FONT_SIZE;
}

function getEditorFontSpec() {
  const id = localStorage.getItem(EDITOR_FONT_FAMILY_KEY) || DEFAULT_EDITOR_FONT_ID;
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
  const id = localStorage.getItem(AGENT_FONT_FAMILY_KEY) || DEFAULT_EDITOR_FONT_ID;
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

function getShowDotfiles() {
  const stored = localStorage.getItem(SHOW_DOTFILES_KEY);
  if (stored === null) return false;
  return stored === '1' || stored === 'true';
}

function setShowDotfiles(show) {
  localStorage.setItem(SHOW_DOTFILES_KEY, show ? '1' : '0');
  syncDotfilesControls(show);
  renderFilteredTree();
}

function getNewWindowOnRepoChange() {
  const stored = localStorage.getItem(NEW_WINDOW_ON_REPO_KEY);
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
      const sel = $('#repo-select');
      if (sel) sel.value = state.repo || '';
    }
    openRepoInNewWindow(repoName);
    return;
  }
  const sel = $('#repo-select');
  if (sel) sel.value = repoName || '';
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
  const root = document.documentElement;
  root.style.setProperty('--ij-ui-font-size', `${size}px`);
  root.style.setProperty('--ij-ui-font-family', spec.family);
  root.style.setProperty('--ij-ui-line-height', String(20 / 13));
}

function applyAgentFontSize(size) {
  const clamped = Math.min(MAX_EDITOR_FONT_SIZE, Math.max(MIN_EDITOR_FONT_SIZE, Math.round(size)));
  localStorage.setItem(AGENT_FONT_SIZE_KEY, String(clamped));
  applyAgentTypography();
  syncAgentFontControls();
  return clamped;
}

function applyAgentFontFamily(fontId) {
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

function applyEditorFontSize(size) {
  const clamped = Math.min(MAX_EDITOR_FONT_SIZE, Math.max(MIN_EDITOR_FONT_SIZE, Math.round(size)));
  localStorage.setItem(EDITOR_FONT_SIZE_KEY, String(clamped));
  if (state.editor) {
    state.editor.updateOptions({
      fontSize: clamped,
      lineHeight: editorLineHeightFor(clamped),
    });
  }
  applyAgentTypography();
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
  if (state.editor) {
    state.editor.updateOptions({ fontFamily: spec.family });
  }
  applyAgentTypography();
  syncFontFamilyControls(spec.id);
  syncAgentFontControls();
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
  const size = parseInt(e.target.value, 10);
  if (!Number.isFinite(size)) return;
  applyAgentFontSize(size);
}

function onAgentFontFamilyChange(e) {
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
  const newWindowOnRepo = $('#settings-new-window-on-repo');
  if (newWindowOnRepo) {
    newWindowOnRepo.checked = getNewWindowOnRepoChange();
    if (!newWindowOnRepo.dataset.bound) {
      newWindowOnRepo.dataset.bound = '1';
      newWindowOnRepo.addEventListener('change', (e) => setNewWindowOnRepoChange(e.target.checked));
    }
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
      const removeBtn = removable
        ? `<button type="button" class="ij-settings-remove" data-host="${escapeHtml(t.host)}">Remove</button>`
        : '<span class="text-[10px] text-gray-600 shrink-0">read-only</span>';
      return `<div class="ij-settings-token-row">
        <div class="min-w-0">
          <div class="ij-settings-token-host">${escapeHtml(t.host)}</div>
          <div class="ij-settings-token-meta">${escapeHtml(t.masked)} · ${escapeHtml(t.source)}</div>
        </div>
        ${removeBtn}
      </div>`;
    }).join('');
    list.querySelectorAll('.ij-settings-remove').forEach((btn) => {
      btn.addEventListener('click', () => removePatToken(btn.dataset.host));
    });
  } catch (err) {
    list.innerHTML = `<p class="ij-settings-empty">${escapeHtml(err.message)}</p>`;
  }
}

async function removePatToken(host) {
  if (!host || !confirm(`Remove token for ${host}?`)) return;
  try {
    await api(`/api/settings/tokens/${encodeURIComponent(host)}`, { method: 'DELETE' });
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
    'java', 'kotlin', 'groovy',
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

  function renderCompilerRow(tool, installed) {
    const isJava = tool.id === 'java';
    const status = compilerStatus(tool);
    const placeholder = tool.kind === 'home'
      ? '/Library/Java/JavaVirtualMachines/…/Contents/Home'
      : `/opt/homebrew/bin/${tool.id === 'python' ? 'python3' : tool.id}`;
    const version = tool.version ? `<span class="ij-compiler-version" title="${escapeHtml(tool.version)}">${escapeHtml(tool.version.split('\n')[0].slice(0, 48))}</span>` : '';
    const exts = (tool.extensions || []).length
      ? `<span class="ij-compiler-exts" title="File extensions">${escapeHtml(tool.extensions.join(' '))}</span>`
      : '';
    const jdkSelect = isJava && installed.length
      ? `<div class="ij-compiler-extra">
          <label class="ij-compiler-extra-label">Installed JDKs</label>
          <select class="ij-settings-select settings-compiler-jdk-select" data-tool-id="java" title="Pick a JDK">
            <option value="">— pick installed JDK —</option>
            ${installed.map((j) => `<option value="${escapeHtml(j.path)}"${tool.path === j.path ? ' selected' : ''}>${escapeHtml(j.label || j.path)}</option>`).join('')}
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
    </article>`;
  }

  function bindCompilerRows(root) {
    root.querySelectorAll('.settings-compiler-jdk-select').forEach((sel) => {
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
    const installed = cfg.java_installed || [];
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
      <div class="ij-compiler-body">${ordered.map((tool) => renderCompilerRow(tool, installed)).join('')}</div>
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
    toast(`${id} compiler saved`, 'success');
  } catch (err) {
    toast(err.message || `Failed to save ${id}`, 'error');
  }
}

async function clearCompilerFromSettings(id) {
  try {
    await api(`/api/settings/compilers/${encodeURIComponent(id)}`, { method: 'DELETE' });
    await loadCompilersSettingsSection();
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
  setTimeout(() => {
    if (tab === 'cursor') $('#settings-cursor-key')?.focus();
    else if (tab === 'ai') $('#settings-gemini-key')?.focus();
    else if (tab === 'appearance') $('#settings-editor-font-size')?.focus();
    else if (tab === 'compilers') $('#settings-compiler-search')?.focus();
    else $('#settings-pat-host')?.focus();
  }, 50);
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
    toast('Paste your Cursor API key', 'error');
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
    syncGeminiModelSelect(cfg.model);
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
  } catch (err) {
    statusEl.innerHTML = `<span class="err">${escapeHtml(err.message)}</span>`;
    state.geminiConfigured = false;
    form?.classList.remove('hidden');
    changeBtn?.classList.add('hidden');
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

async function hasGitHubPat() {
  try {
    const tokens = await api('/api/settings/tokens');
    return tokens.some((t) => t.host === 'github.com' || t.host === '*');
  } catch {
    return false;
  }
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
    java: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#9876aa" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#9876aa" font-family="JetBrains Mono,Consolas,monospace">J</text></svg>',
    gradle: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#6a8759" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#6a8759" font-family="JetBrains Mono,Consolas,monospace">G</text></svg>',
    kotlin: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#7f52ff" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#7f52ff" font-family="JetBrains Mono,Consolas,monospace">K</text></svg>',
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
    git: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="2" y="2" width="12" height="12" rx="2" fill="#f14c28" fill-opacity=".22"/><text x="8" y="11.5" text-anchor="middle" font-size="8" font-weight="700" fill="#f14c28" font-family="JetBrains Mono,Consolas,monospace">G</text></svg>',
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
  const parts = path.split('/');
  el.innerHTML = parts.map((part, i) => {
    const seg = parts.slice(0, i + 1).join('/');
    const sep = i < parts.length - 1 ? '<span class="ij-crumb-sep"> › </span>' : '';
    return `<button type="button" class="ij-crumb" data-crumb="${seg}">${part}</button>${sep}`;
  }).join('');
  $$('.ij-crumb').forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = btn.dataset.crumb;
      if (target === path) return;
      const node = state.tabs.includes(target) ? target : null;
      if (node) activateTab(node);
    });
  });
}

function updateEditorStatus(pos) {
  const el = $('#status-cursor');
  if (el && pos) el.textContent = `${pos.lineNumber}:${pos.column}`;
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
  $('#java-index-banner')?.classList.add('hidden');
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
  const bannerText = $('#java-index-banner-text');
  banner?.classList.add('hidden');

  state.projectIndexRunning = status?.state === 'running';
  const javaReady = status?.java?.state === 'ready' && (status?.java?.dependency_jars || 0) > 0;
  state.projectIndexReady = status?.state === 'ready' && (javaReady || (status?.workspace_symbols || 0) > 0);
  state.projectProfile = status?.profile || null;

  if (status?.state === 'running') {
    const label = status.label || 'project';
    const parts = [];
    if (status.java?.symbol_count > 0) {
      parts.push(`${Number(status.java.symbol_count).toLocaleString()} Java symbols`);
    }
    if (status.workspace_symbols > 0) {
      parts.push(`${Number(status.workspace_symbols).toLocaleString()} workspace symbols`);
    }
    const detail = parts.length ? ` (${parts.join(', ')})` : '';
    setStatusMessage(`Indexing ${label}${detail}…`);
    if (bannerText) bannerText.textContent = `Indexing ${label}…`;
    banner?.classList.remove('hidden');
    return;
  }

  if (status?.state === 'ready' && !state.projectIndexNotified) {
    state.projectIndexNotified = true;
    const label = status.label || 'Project';
    const javaN = status.java?.symbol_count ?? 0;
    const wsN = status.workspace_symbols ?? 0;
    const springN = status.java?.spring_symbols ?? 0;
    const jdkN = status.java?.jdk_symbols ?? 0;
    const total = javaN + wsN;
    const langs = (status.profile?.languages || []).join(', ') || label.toLowerCase();
    const detail = [
      springN ? `${springN.toLocaleString()} Spring` : '',
      jdkN ? `${jdkN.toLocaleString()} JDK` : '',
    ].filter(Boolean).join(', ');
    toast(
      `${label} index ready — ${total.toLocaleString()} symbols${detail ? ` (${detail})` : ''} [${langs}]`,
      springN > 0 || !status.profile?.frameworks?.includes('spring-boot') ? 'success' : 'warning',
    );
    terminalLog(`${label} index ready: ${total.toLocaleString()} symbols`);
    if (state.activeTab?.endsWith('.java')) scheduleDiagnostics();
  } else if (status?.state === 'error' && !state.projectIndexNotified) {
    state.projectIndexNotified = true;
    toast(`Project indexing failed: ${status.error || status.java?.error || 'unknown error'}`, 'error');
  }
}

async function pollProjectIndexStatus() {
  if (!state.repo) return;
  try {
    const status = await api(repoApi(state.repo, '/workspace/project/index-status'));
    updateProjectIndexUi(status);
    const elapsed = Date.now() - (state.projectIndexStartedAt || Date.now());
    const timedOut = status?.state === 'running' && elapsed > 5 * 60 * 1000;
    if (timedOut && !state.projectIndexNotified) {
      state.projectIndexNotified = true;
      toast('Indexing is taking longer than expected — you can keep working; check Settings → Java if Gradle is stuck', 'warning', { duration: 12000 });
      stopProjectIndexPolling();
      return;
    }
    if (status?.state === 'ready' || status?.state === 'error' || status?.state === 'idle') {
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
  if (welcomeVisible) {
    editor?.classList.add('hidden');
    toolbar?.classList.add('hidden');
    toolbar?.classList.remove('flex');
  } else if (state.activeTab) {
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
    'terminal-dock-left': () => setTerminalDock('left'),
    'terminal-dock-right': () => setTerminalDock('right'),
    'terminal-dock-bottom': () => setTerminalDock('bottom'),
    'agent-dock-left': () => setAgentDock('left'),
    'agent-dock-right': () => setAgentDock('right'),
    'agent-dock-bottom': () => setAgentDock('bottom'),
    'toggle-sidebar': () => toggleSidebar(),
    'toggle-dotfiles': () => setShowDotfiles(!getShowDotfiles()),
    'command-palette': showPalette,
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
  { id: 'goto-class', label: 'Go to Class', kbd: '⌘O', run: showGoToClass, needsRepo: true },
  { id: 'switch-branch', label: 'Switch branch…', kbd: '⌘⇧B', run: showBranchPicker, needsRepo: true },
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
  { id: 'agent', label: 'Show Agent', run: toggleAgent },
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

function showGoToClass() {
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
  if (input) {
    input.value = '';
    const results = $('#goto-class-results');
    if (results) results.innerHTML = '<p class="px-4 py-3 text-xs text-gray-500">Loading…</p>';
    scheduleGoToClassSearch('');
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

function setBranchLabel(branch) {
  state.currentBranch = branch || '';
  const el = $('#branch-picker-label');
  if (el) el.textContent = branch || 'branch';
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
  results.innerHTML = branchPickerBranches.map((b, i) => `
    <button type="button" id="branch-picker-option-${i}" class="ij-branch-picker-item ij-palette-item${i === branchPickerIndex ? ' active' : ''}${b === current ? ' current' : ''}" data-branch-idx="${i}" role="option" aria-selected="${i === branchPickerIndex ? 'true' : 'false'}">
      <span class="ij-branch-picker-name">⎇ ${highlightBranchName(b, query)}</span>
      ${b === current ? '<span class="ij-branch-picker-tag">current</span>' : ''}
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
    closeAllMenus();
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
    return `
      <section class="ij-sbs-file">
        <div class="ij-sbs-file-header">${escapeHtml(file.path)}</div>
        ${hunksHtml || '<div class="ij-sbs-empty">No hunks</div>'}
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

function lineHasTestAnnotation(lines, idx) {
  let i = idx;
  while (i > 0 && !lines[i].trim()) i -= 1;
  let scan = i;
  for (;;) {
    const trimmed = lines[scan].trim();
    if (trimmed.startsWith('@')
      && (trimmed.includes('@Test')
        || trimmed.includes('@ParameterizedTest')
        || trimmed.includes('@RepeatedTest')
        || trimmed.includes('@TestFactory')
        || trimmed.includes('@TestTemplate'))) {
      return true;
    }
    if (!trimmed.startsWith('@') && trimmed) return false;
    if (scan === 0) return false;
    scan -= 1;
  }
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
  const before = line.trim().split('(')[0];
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
    if (!lineHasTestAnnotation(lines, i)) {
      i += 1;
      continue;
    }
    const sigIdx = findJavaMethodSignatureIndex(lines, i);
    if (sigIdx < 0) {
      i += 1;
      continue;
    }
    const name = parseJavaMethodName(lines[sigIdx]);
    if (!name) {
      i = sigIdx + 1;
      continue;
    }
    const end = javaMethodBlockEnd(lines, sigIdx);
    const glyphIdx = findTestAnnotationLineIndex(lines, i, sigIdx);
    out.push({
      name,
      line: sigIdx + 1,
      glyphLine: glyphIdx + 1,
      end_line: end + 1,
      filter: `${className}.${name}`,
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
  if (!state.editor || !state.testRunWidgets?.length) return;
  for (const widget of state.testRunWidgets) {
    try {
      state.editor.removeGlyphMarginWidget(widget);
    } catch {
      /* ignore stale widgets */
    }
  }
  state.testRunWidgets = [];
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
    state.testRunWidgets = methods.map((method) => {
      const widget = createTestRunWidget(method);
      state.editor.addGlyphMarginWidget(widget);
      state.testMethodsByLine.set(method.glyphLine, method);
      return widget;
    });
  };
  requestAnimationFrame(() => requestAnimationFrame(paint));
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
    editor?.classList.remove('hidden');
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
function setEditorContent(path, content) {
  if (!state.editor) return;
  state.suppressEditorChange = true;
  state.editor.setValue(content ?? '');
  monaco.editor.setModelLanguage(state.editor.getModel(), langForPath(path));
  state.suppressEditorChange = false;
  applyTestRunDecorations();
}

function syncEditorFromActiveTab() {
  if (!state.editor || !state.activeTab || !state.tabContents.has(state.activeTab)) return;
  setEditorContent(state.activeTab, state.tabContents.get(state.activeTab));
}

function defaultReadmeContent(path) {
  const name = path.split('/').pop();
  if (name?.toLowerCase() !== 'readme.md') return '';
  const title = state.repo || 'Project';
  return `# ${title}\n\nManaged by Reaper.\n`;
}

function initEditor() {
  require.config({ paths: { vs: 'https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min/vs' } });
  require(['vs/editor/editor.main'], () => {
    window.ReaperThemes?.defineMonacoThemes();
    const fontSize = getEditorFontSize();
    const fontSpec = getEditorFontSpec();
    ensureEditorFontLoaded(fontSpec);
    state.editor = monaco.editor.create($('#editor'), {
      value: '',
      language: 'plaintext',
      theme: window.ReaperThemes?.getMonacoThemeId() || 'reaper-navy',
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
      wordBasedSuggestions: 'currentDocument',
      quickSuggestions: { other: true, strings: true, comments: false },
      suggestOnTriggerCharacters: true,
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
    state.editor.onDidChangeModelContent(() => {
      if (state.suppressEditorChange || !state.activeTab) return;
      state.tabContents.set(state.activeTab, state.editor.getValue());
      state.dirty.add(state.activeTab);
      updateSaveButton();
      if (isGradleFilePath(state.activeTab)) refreshGradleInfo();
      else if (state.activeTab?.endsWith('.java')) updateRunButtons();
      renderTabs();
      scheduleAutoSave();
      scheduleDiagnostics();
      updateConflictUi();
      applyTestRunDecorations();
    });
    state.editor.onDidChangeCursorPosition((e) => updateEditorStatus(e.position));
    window.ReaperLang?.setupEditorFeatures(state.editor, {
      api,
      repoApi,
      getRepo: () => state.repo,
      getActivePath: () => state.activeTab,
      getEditor: () => state.editor,
      openFileAt,
      isFileDirty: (path) => state.dirty.has(path),
      toast,
      setLanguageStatus: (label) => {
        const el = $('#status-language');
        if (el) el.textContent = label;
      },
    });
    state.editorReady = true;
    syncEditorFromActiveTab();
  });
}

// --- Repos ---
async function loadRepos() {
  state.repos = await api('/api/repos');
  const sel = $('#repo-select');
  sel.innerHTML = '<option value="">Select repository…</option>';
  state.repos.forEach((r) => {
    const opt = document.createElement('option');
    opt.value = r.name;
    opt.textContent = r.name;
    sel.appendChild(opt);
  });
  if (!state.tabs.length && !state.repo) {
    renderWelcome();
    $('#empty-state')?.classList.remove('hidden');
    syncWelcomeLayout();
  }
}

async function selectRepo(name) {
  if (!name) {
    state.repo = null;
    resetUI();
    updateWindowTitle();
    return;
  }
  state.repo = name;
  resetTerminalCwds();
  updateTerminalCwdUi();
  const opened = await api(repoApi(name, '/workspace/open'), { method: 'POST' });
  state.projectProfile = opened?.profile || null;
  if (opened?.indexing) {
    startProjectIndexPolling();
  }
  const detail = await api(repoApi(name));
  state.branches = detail.branches;
  updateBranchSelect();
  updateRepoInfo(detail);
  enableControls();
  updateAgentUi();
  await refreshTree();
  await refreshGitStatus();
  await refreshHistory();
  try {
    await openFile('README.md');
  } catch {
    /* no readme in repo */
  }
  terminalLog(`Opened workspace: ${name}`);
  $('#empty-state')?.classList.add('hidden');
  syncWelcomeLayout();
  updateMenuState();
  updateWindowTitle();
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
  updateTerminalCwdUi();
  updateTreeBackButton();
  stopProjectIndexPolling();
  showNoRepoFileTree();
  $('#git-status-list').innerHTML = '';
  $('#commit-history').innerHTML = '';
  state.commitSelectedPaths = new Set();
  state.commitKnownPaths = new Set();
  state.lastGitStatusFiles = [];
  state.mergeBlockedCommit = false;
  state.repoDetail = null;
  const btnRepoInfo = $('#btn-repo-info');
  if (btnRepoInfo) btnRepoInfo.disabled = true;
  $('#branch-picker-btn').disabled = true;
  setBranchLabel('');
  ['#btn-sync', '#btn-nav-commit', '#btn-nav-push', '#btn-save', '#tb-save', '#tb-format', '#tb-run', '#btn-commit-only', '#btn-commit-push', '#btn-suggest-commit', '#btn-new-file', '#gradle-task', '#terminal-input'].forEach((s) => { const el = $(s); if (el) el.disabled = true; });
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
}

function enableControls() {
  $('#branch-picker-btn').disabled = false;
  const btnRepoInfo = $('#btn-repo-info');
  if (btnRepoInfo) btnRepoInfo.disabled = false;
  ['#btn-sync', '#btn-nav-commit', '#btn-nav-push', '#btn-save', '#tb-save', '#tb-format', '#tb-run', '#btn-commit-only', '#btn-commit-push', '#btn-suggest-commit', '#btn-new-file', '#gradle-task', '#terminal-input'].forEach((s) => { const el = $(s); if (el) el.disabled = false; });
  updateRunButtons();
  updateMenuState();
}

function updateBranchSelect() {
  if (state.currentBranch && state.branches.includes(state.currentBranch)) return;
  if (state.branches.length) setBranchLabel(state.branches[0]);
}

function updateRepoInfo(detail) {
  state.repoDetail = detail;
  const s = detail.summary || detail;
  const el = $('#repo-info');
  if (!el) return;
  el.innerHTML = `
    <div>
      <div class="text-white font-medium">${s.name}</div>
      ${s.description ? `<p class="text-gray-500 text-xs mt-1">${s.description}</p>` : ''}
    </div>
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
  $('#repo-select').value = '';
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
    $('#repo-select').value = repo.name;
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
    $('#repo-select').value = repo.name;
    await selectRepo(repo.name);
    toast(`Cloned ${repo.name}`, 'success');
  } catch (err) {
    const msg = err.message || String(err);
    setCloneModalState({ busy: false, error: msg, status: '' });
    terminalLog(`clone failed: ${msg}`);
    const host = hostFromUrl(remoteUrl);
    if (host && /auth|401|403|credential|token|pat/i.test(msg)) {
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
    $('#repo-select').value = repo.name;
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
let gradleInfoTimer = null;

async function loadTreeLevel(dirPath = '') {
  const q = dirPath ? `?dir=${encodeURIComponent(dirPath)}` : '';
  const nodes = await api(repoApi(state.repo, `/workspace/tree${q}`));
  treeState.children.set(dirPath, nodes);
  return nodes;
}

async function prefetchTreeLevel(dirPath) {
  if (treeState.children.has(dirPath) || treeState.loading.has(dirPath)) return;
  treeState.loading.add(dirPath);
  try {
    await loadTreeLevel(dirPath);
  } catch {
    /* ignore background prefetch errors */
  } finally {
    treeState.loading.delete(dirPath);
  }
}

async function loadTreeRoot() {
  treeState.expanded.clear();
  await loadTreeLevel('');
}

function renderTree(nodes, depth = 0, lazyMode = true) {
  return nodes.map((n) => {
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
          childrenHtml = renderTree(treeState.children.get(n.path) || [], depth + 1, true);
        } else if (open && !isLeaf) {
          childrenHtml = '<div class="ij-tree-loading">Loading…</div>';
        }
      } else {
        childrenHtml = n.children?.length ? renderTree(n.children, depth + 1, false) : '';
      }
      return `
        <details class="ij-tree-dir" data-dir="${escapeHtml(n.path)}" ${open ? 'open' : ''}${isLeaf ? ' data-leaf="1"' : ''}>
          <summary class="ij-tree-row ij-tree-dir-row" style="--depth:${depth}" aria-expanded="${open ? 'true' : 'false'}">
            <span class="ij-tree-chevron" aria-hidden="true"></span>
            <span class="ij-tree-icon ij-tree-icon-folder">${treeIconSvg('folder')}${treeIconSvg('folderOpen')}</span>
            <span class="ij-tree-label">${escapeHtml(n.name)}</span>
          </summary>
          <div class="ij-tree-children">${childrenHtml}</div>
        </details>`;
    }
    const iconKind = fileIcon(n.name);
    return `
      <button type="button" data-path="${escapeHtml(n.path)}" class="tree-file ij-tree-row ij-tree-file-row" style="--depth:${depth}">
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

async function refreshTree() {
  treeState.children.clear();
  treeState.loading.clear();
  treeState.recursiveNodes = null;
  const query = $('#tree-filter')?.value?.trim() || '';
  if (query) {
    treeState.recursiveNodes = await api(repoApi(state.repo, '/workspace/tree?recursive=1'));
    lastTreeNodes = treeState.recursiveNodes;
  } else {
    await loadTreeRoot();
    lastTreeNodes = treeState.children.get('') || [];
  }
  renderFilteredTree();
  await loadExpandedTreeGaps();
}

/** Fetch any expanded folder that rendered open but has no cached children yet. */
async function loadExpandedTreeGaps() {
  if (treeState.recursiveNodes) return;
  const pending = [...treeState.expanded].filter(
    (d) => d && !treeState.children.has(d) && !treeState.loading.has(d),
  );
  if (!pending.length) return;
  await Promise.all(pending.map(async (dir) => {
    treeState.loading.add(dir);
    try {
      await loadTreeLevel(dir);
    } catch (err) {
      treeState.expanded.delete(dir);
      toast(err.message, 'error');
    } finally {
      treeState.loading.delete(dir);
    }
  }));
  renderFilteredTree();
}

function isHiddenTreeEntry(n) {
  const name = n.name || '';
  const path = (n.path || '').replace(/\\/g, '/');
  if (name.startsWith('.')) return true;
  if (name === 'gradlew' || name === 'gradlew.bat') return true;
  if (name === 'gradle.properties') return true;
  if (path === 'gradle' || path.startsWith('gradle/')) return true;
  return false;
}

function filterHiddenTreeItems(nodes) {
  if (getShowDotfiles()) return nodes;
  const out = [];
  for (const n of nodes) {
    if (isHiddenTreeEntry(n)) continue;
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
    $$('.tree-file').forEach((b) => b.classList.toggle('active', b.dataset.path === state.activeTab));
  }
}

function bindTreeEvents() {
  const treeEl = $('#file-tree');
  if (!treeEl || treeEl.dataset.treeBound) return;
  treeEl.dataset.treeBound = '1';

  treeEl.addEventListener('toggle', async (e) => {
    const details = e.target;
    if (!details.matches?.('details.ij-tree-dir')) return;
    const dir = details.dataset.dir;
    if (!dir || treeState.recursiveNodes) return;
    if (details.open) {
      treeState.expanded.add(dir);
      if (details.dataset.leaf === '1' || treeState.children.has(dir)) return;
      treeState.loading.add(dir);
      renderFilteredTree();
      try {
        await loadTreeLevel(dir);
      } catch (err) {
        toast(err.message, 'error');
        treeState.expanded.delete(dir);
        details.open = false;
      } finally {
        treeState.loading.delete(dir);
        renderFilteredTree();
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
      return data.content;
    })
    .catch((err) => {
      fileFetchInflight.delete(path);
      throw err;
    });
  fileFetchInflight.set(path, promise);
  return promise;
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
    refreshGradleInfo();
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
  if (state.tabs.includes(path)) {
    activateTab(path);
    return;
  }
  state.tabs.push(path);
  state.activeTab = path;
  renderTabs();
  activateTabShell(path);

  try {
    let content = await fetchFileContent(path);
    if (path.split('/').pop()?.toLowerCase() === 'readme.md' && !content.trim()) {
      content = defaultReadmeContent(path);
      state.dirty.add(path);
    }
    state.tabContents.set(path, content);
    if (state.activeTab === path) {
      setEditorContent(path, content);
      if (!state.editorReady) syncEditorFromActiveTab();
    }
    renderTabs();
    $$('.tree-file').forEach((b) => b.classList.toggle('active', b.dataset.path === path));
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
    const lang = window.ReaperLang?.langLabel(langForPath(path)) || 'Plain Text';
    const compilers = window.ReaperLang?.compilerLabelsForPath(path);
    langEl.textContent = compilers ? `${lang} · ${compilers}` : lang;
    langEl.title = compilers ? `Language: ${lang}. Compiler(s): ${compilers}` : `Language: ${lang}`;
  }
  if (state.editor) updateEditorStatus(state.editor.getPosition());
  $$('.tree-file').forEach((b) => b.classList.toggle('active', b.dataset.path === path));
  updateSaveButton();
  scheduleGradleInfoRefresh();
  updateMenuState();
  setStatusMessage(path.split('/').pop() || path);
  fileDiags = [];
  diagJumpIndex = 0;
  updateDiagnosticsStatusBar(path, []);
  scheduleDiagnostics();
  updateTreeBackButton();
  updateConflictUi();
}

function activateTab(path) {
  activateTabShell(path);
  renderTabs();
}

function isDiagnosablePath(path) {
  return window.ReaperLang?.isDiagnosablePath(path) ?? false;
}

function scheduleDiagnostics() {
  if (diagTimer) clearTimeout(diagTimer);
  if (!isDiagnosablePath(state.activeTab)) {
    clearDiagnostics();
    return;
  }
  diagTimer = setTimeout(runDiagnostics, DIAG_DELAY_MS);
}

function clearDiagnostics() {
  fileDiags = [];
  diagJumpIndex = 0;
  updateDiagnosticsStatusBar(state.activeTab, []);
  const model = state.editor?.getModel();
  if (model && window.monaco) {
    monaco.editor.setModelMarkers(model, 'reaper-diagnostics', []);
  }
}

function diagnosticMarkerSpan(model, d) {
  const line = Math.max(1, d.line || 1);
  const startCol = Math.max(1, d.column || 1);
  const endLine = Math.max(line, d.end_line || d.line || line);
  let endCol = d.end_column || 0;
  if (!endCol || endCol <= startCol) {
    const lineText = model.getLineContent(line);
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

function updateDiagnosticsStatusBar(path, diags) {
  const el = $('#status-diagnostics');
  const countEl = $('#status-diag-count');
  if (!el || !countEl) return;

  const errors = diags.filter((d) => d.severity !== 'warning').length;
  const warnings = diags.filter((d) => d.severity === 'warning').length;

  if (!errors && !warnings) {
    el.classList.add('hidden');
    el.classList.remove('has-errors', 'has-warnings');
    countEl.textContent = '';
    el.title = 'No problems';
    return;
  }

  el.classList.remove('hidden');
  el.classList.toggle('has-errors', errors > 0);
  el.classList.toggle('has-warnings', errors === 0 && warnings > 0);

  const parts = [];
  if (errors) parts.push(`${errors} error${errors === 1 ? '' : 's'}`);
  if (warnings) parts.push(`${warnings} warning${warnings === 1 ? '' : 's'}`);
  countEl.textContent = parts.join(', ');

  const name = path?.split('/').pop() || 'file';
  el.title = `${name}: ${parts.join(', ')} — click to go to next problem`;
}

function jumpToNextDiagnostic() {
  if (!state.editor || !fileDiags.length) return;
  const sorted = [...fileDiags].sort(
    (a, b) => (a.line || 1) - (b.line || 1) || (a.column || 1) - (b.column || 1),
  );
  const d = sorted[diagJumpIndex % sorted.length];
  diagJumpIndex = (diagJumpIndex + 1) % sorted.length;
  const line = Math.max(1, d.line || 1);
  const column = Math.max(1, d.column || 1);
  state.editor.setPosition({ lineNumber: line, column });
  state.editor.revealLineInCenter(line);
  state.editor.focus();
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
    const diags = await api(repoApi(state.repo, '/workspace/diagnostics'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path, content }),
    });
    if (seq !== diagSeq || path !== state.activeTab) return;
    applyDiagnostics(path, Array.isArray(diags) ? diags : []);
  } catch {
    if (seq === diagSeq) clearDiagnostics();
  }
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

async function refreshGradleInfo() {
  if (!state.repo || !state.activeTab) {
    state.gradleInfo = null;
    updateRunButtons();
    return;
  }
  try {
    const info = await api(
      `${repoApi(state.repo, '/workspace/gradle/info')}?path=${encodeURIComponent(state.activeTab)}`,
    );
    state.gradleInfo = info?.is_gradle ? info : null;
  } catch {
    state.gradleInfo = null;
  }
  updateRunButtons();
}

function updateRunButtons() {
  const tbRun = $('#tb-run');
  const taskSel = $('#gradle-task');
  const runLabel = $('#toolbar-run-label');
  const gradleSep = $('#gradle-toolbar-sep');
  const info = state.gradleInfo;
  state.javaRunTarget = null;

  const showGradle = !!(info?.is_gradle && isGradleFilePath(state.activeTab));
  if (showGradle) {
    taskSel?.classList.remove('hidden');
    if (taskSel && info.tasks?.length) {
      const current = taskSel.value;
      taskSel.innerHTML = info.tasks.map((t) => `<option value="${t}">${t}</option>`).join('');
      taskSel.value = info.tasks.includes(current) ? current : info.default_task;
    }
    const task = taskSel?.value || info.default_task;
    if (tbRun) {
      tbRun.disabled = false;
      tbRun.title = `Run Gradle '${task}' (F5)`;
    }
    if (runLabel) {
      runLabel.classList.remove('hidden');
      runLabel.textContent = info.application_main ? `Gradle · ${info.application_main}` : `Gradle · ${info.project_root}`;
    }
    gradleSep?.classList.remove('hidden');
  } else {
    taskSel?.classList.add('hidden');
    runLabel?.classList.add('hidden');
    gradleSep?.classList.add('hidden');
    const isJava = !!(state.repo && state.activeTab?.endsWith('.java'));
    if (tbRun) {
      tbRun.disabled = !isJava;
      if (isJava) {
        const content = state.tabContents.get(state.activeTab) ?? state.editor?.getValue() ?? '';
        const main = detectJavaMain(content);
        state.javaRunTarget = main?.qualifiedName || null;
        tbRun.title = main ? `Run ${main.qualifiedName} (F5)` : 'Run Java (F5) — needs static void main';
      } else {
        tbRun.title = 'Run (F5)';
      }
    }
  }

  const tbFormat = $('#tb-format');
  const tbSave = $('#tb-save');
  if (tbFormat) tbFormat.disabled = !state.activeTab;
  if (tbSave) tbSave.disabled = !state.activeTab || !state.dirty.has(state.activeTab);
}

async function openFileAt(path, line = 1, column = 1) {
  if (state.activeTab !== path) {
    rememberTreeAnchorCursor();
    if (state.tabs.includes(path)) {
      activateTab(path);
    } else {
      await openFile(path);
    }
  } else {
    activateTab(path);
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
  syncWelcomeLayout();
  setMainView('editor');
  $('#editor-toolbar')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.remove('flex');
  updateBreadcrumbs(null);
  updateRunButtons();
  updateMenuState();
}

async function saveFile(options = {}) {
  const { silent = false } = options;
  if (!state.activeTab || !state.editor) return;
  const content = state.editor.getValue();
  try {
    await api(repoApi(state.repo, '/workspace/file'), {
      method: 'PUT',
      body: JSON.stringify({ path: state.activeTab, content }),
    });
    state.tabContents.set(state.activeTab, content);
    state.dirty.delete(state.activeTab);
    updateSaveButton();
    renderTabs();
    if (!silent) {
      await refreshTree();
      await refreshGitStatus();
      toast('Saved', 'success');
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
      await refreshTree();
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

function showFileModal() {
  if (!state.repo) {
    toast('Select a repository first', 'error');
    return;
  }
  $('#file-modal-overlay').classList.remove('hidden');
  $('#file-modal-overlay').classList.add('flex');
  const input = $('#new-file-form input[name="path"]');
  if (input) {
    input.value = '';
    setTimeout(() => input.focus(), 50);
  }
}

function hideFileModal() {
  $('#file-modal-overlay').classList.add('hidden');
  $('#file-modal-overlay').classList.remove('flex');
}

function updateSaveButton() {
  const dirty = !!(state.activeTab && state.dirty.has(state.activeTab));
  const btn = $('#btn-save');
  if (btn) btn.disabled = !dirty;
  const tbSave = $('#tb-save');
  if (tbSave) tbSave.disabled = !dirty;
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

async function runGradle() {
  if (!state.repo || !state.activeTab || !state.gradleInfo?.is_gradle) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const task = $('#gradle-task')?.value || state.gradleInfo.default_task;
  const label = `▶ gradle ${task}  (${state.gradleInfo.project_root})`;
  try {
    const exitCode = await runWorkspaceCommandStream(
      '/workspace/gradle/run',
      { path: state.activeTab, task },
      { label, terminalId: term.id },
    );
    const output = term.lines.filter((e) => typeof e === 'string').pop() || '';
    if (exitCode !== 0 && /Unsupported class file major version/i.test(output)) {
      toast('Gradle needs an older JDK — open Settings → Java and pick Java 17 or 21', 'error', { duration: 12000 });
    }
  } catch (e) {
    terminalLog(`error: ${e.message}`);
    if (/Unsupported class file major version/i.test(e.message || '')) {
      toast('Gradle needs an older JDK — open Settings → Java and pick Java 17 or 21', 'error', { duration: 12000 });
    }
  }
}

function normalizeGradleTestFilter(testFilter) {
  return String(testFilter || '')
    .replace(/\s*\([^)]*\.java\)\s*$/, '')
    .replace(/\//g, '.')
    .trim();
}

async function runGradleTest(testFilter) {
  if (!state.repo || !state.activeTab || !testFilter) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  await refreshGradleInfo();
  if (!state.gradleInfo?.is_gradle) {
    toast('Not inside a Gradle project', 'error');
    return;
  }
  showTerminal();
  const term = getActiveTerminal();
  const filter = normalizeGradleTestFilter(testFilter);
  if (!filter) return;
  const task = `test --tests ${filter}`;
  const label = `▶ gradle ${task}  (${state.activeTab})`;
  try {
    const exitCode = await runWorkspaceCommandStream(
      '/workspace/gradle/run',
      { path: state.activeTab, task },
      { label, terminalId: term.id },
    );
    const output = term.lines.filter((e) => typeof e === 'string').pop() || '';
    if (exitCode !== 0 && /Unsupported class file major version/i.test(output)) {
      toast('Gradle needs an older JDK — open Settings → Java and pick Java 17 or 21', 'error', { duration: 12000 });
    }
  } catch (e) {
    terminalLog(`error: ${e.message}`);
    if (/Unsupported class file major version/i.test(e.message || '')) {
      toast('Gradle needs an older JDK — open Settings → Java and pick Java 17 or 21', 'error', { duration: 12000 });
    }
  }
}

async function runActive() {
  if (state.gradleInfo?.is_gradle && isGradleFilePath(state.activeTab)) await runGradle();
  else await runJavaMain();
}
async function runJavaMain() {
  if (!state.repo || !state.activeTab?.endsWith('.java')) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  showTerminal();
  const term = getActiveTerminal();
  const name = state.javaRunTarget || state.activeTab;
  try {
    await runWorkspaceCommandStream(
      '/workspace/java/run',
      { path: state.activeTab },
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
  const diff = await api(`${repoApi(state.repo, '/workspace/diff')}?path=${encodeURIComponent(path)}&staged=${staged}`);
  showDiffInMainArea(path, diff.diff || '(no diff — new or binary file)');
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
  state.conflictFiles = new Set(
    (status.files || []).filter((f) => f.status === 'conflict').map((f) => f.path),
  );
  updateMergeBanner(status);
  const mergeBlocked = !!(status.merge?.active && (status.conflict_count || 0) > 0);
  state.mergeBlockedCommit = mergeBlocked;
  state.lastGitStatusFiles = status.files || [];
  syncCommitSelection(status.files || []);
  updateCommitSelectionUi(status.files || [], { mergeBlocked });
  updateGitNavUi(status);
  setBranchLabel(status.branch);
  const badge = $('#git-badge');
  if (badge) {
    if (status.clean) {
      badge.classList.add('hidden');
    } else {
      const n = status.conflict_count || status.files.length;
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
  if (status.clean) {
    list.innerHTML = '<div class="ij-empty-state"><div class="ij-empty-icon">✓</div><p>Nothing to commit — working tree clean</p></div>';
    updateCommitSelectionUi([], { mergeBlocked });
    updateStatusBar(status);
    updateConflictUi();
    return { clean: true, files: [], branch: status.branch, ahead: status.ahead || 0 };
  }
  const grouped = groupStatusFilesByPath(status.files);
  list.innerHTML = grouped.map((f) => {
    const checked = !f.conflict && state.commitSelectedPaths.has(f.path);
    const disabled = f.conflict ? ' disabled' : '';
    const checkedAttr = checked ? ' checked' : '';
    return `
    <div class="ij-git-row${f.conflict ? ' conflict-item' : ''}">
      <label class="ij-git-check" title="${f.conflict ? 'Resolve conflicts before committing' : 'Include in commit'}">
        <input type="checkbox" class="ij-git-stage-check" data-path="${escapeHtml(f.path)}"${checkedAttr}${disabled}>
      </label>
      <button type="button" data-status-path="${escapeHtml(f.path)}" data-staged="${f.staged}" data-status="${f.status}" class="ij-git-item" title="${escapeHtml(statusLabel(f.status))} — click to preview diff">
        <span class="ij-git-badge ${f.status}" title="${escapeHtml(statusLabel(f.status))}">${statusIcon(f.status)}</span>
        <span class="ij-git-path">${escapeHtml(f.path)}</span>
      </button>
    </div>`;
  }).join('');
  list.querySelectorAll('.ij-git-stage-check').forEach((input) => {
    input.addEventListener('change', () => {
      setCommitPathSelected(input.dataset.path, input.checked);
      updateCommitSelectionUi(status.files || [], { mergeBlocked });
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
  updateStatusBar(status);
  updateConflictUi();
  const result = {
    clean: false,
    files: status.files,
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
  if (final && state.agentHadFileChanges) {
    toast('Agent finished — review diffs in the main panel or Source Control', 'success');
    state.agentHadFileChanges = false;
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
  startProjectIndexPolling();
}

// --- Terminal ---
let terminalNextNum = 1;

function createTerminalSession(name) {
  const num = terminalNextNum++;
  return {
    id: `term-${num}`,
    name: name || String(num),
    cwd: '',
    history: [],
    historyIndex: -1,
    lines: [],
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

function resetTerminalCwds() {
  state.terminals.forEach((t) => {
    t.cwd = '';
  });
}

function clearTerminalSession(term) {
  term.lines = [];
  term.cwd = '';
  term.history = [];
  term.historyIndex = -1;
}

function appendTerminalLine(text, container = $('#terminal-output')) {
  if (!container) return;
  const line = document.createElement('div');
  line.className = 'mb-1 whitespace-pre-wrap ij-terminal-line';
  line.textContent = text;
  container.appendChild(line);
  container.scrollTop = container.scrollHeight;
}

function appendTerminalEntry(entry, container = $('#terminal-output')) {
  if (!container) return;
  if (typeof entry === 'string') {
    appendTerminalLine(entry, container);
    return;
  }
  if (entry?.kind === 'cmd-start') {
    const head = document.createElement('div');
    head.className = 'ij-terminal-cmd-head';
    head.textContent = entry.label;
    container.appendChild(head);
  } else if (entry?.kind === 'cmd-end') {
    const foot = document.createElement('div');
    foot.className = 'ij-terminal-cmd-foot';
    if (entry.exitCode != null && entry.exitCode !== 0) {
      foot.classList.add('ij-terminal-cmd-foot--failed');
      const label = document.createElement('span');
      label.className = 'ij-terminal-cmd-foot-label';
      label.textContent = `exit ${entry.exitCode}`;
      foot.appendChild(label);
    }
    container.appendChild(foot);
  }
  container.scrollTop = container.scrollHeight;
}

function renderTerminalOutput() {
  const out = $('#terminal-output');
  const term = getActiveTerminal();
  if (!out || !term) return;
  out.replaceChildren();
  term.lines.forEach((entry) => appendTerminalEntry(entry, out));
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
  renderTerminalOutput();
  updateTerminalCwdUi();
  setTimeout(() => $('#terminal-input')?.focus(), 30);
}

function newTerminal({ focus = true } = {}) {
  ensureTerminals();
  const term = createTerminalSession();
  state.terminals.push(term);
  state.activeTerminalId = term.id;
  renderTerminalTabs();
  renderTerminalOutput();
  updateTerminalCwdUi();
  if (focus) {
    showTerminal();
    setTimeout(() => $('#terminal-input')?.focus(), 50);
  }
}

function closeTerminal(id) {
  ensureTerminals();
  const term = state.terminals.find((t) => t.id === id);
  if (!term) return;
  if (state.terminals.length <= 1) {
    clearTerminalSession(term);
    renderTerminalOutput();
    updateTerminalCwdUi();
    return;
  }
  const idx = state.terminals.findIndex((t) => t.id === id);
  state.terminals.splice(idx, 1);
  if (state.activeTerminalId === id) {
    const next = state.terminals[Math.max(0, idx - 1)];
    state.activeTerminalId = next.id;
  }
  renderTerminalTabs();
  renderTerminalOutput();
  updateTerminalCwdUi();
}

function terminalPromptFor(term = getActiveTerminal()) {
  const repo = state.repo || 'repo';
  const cwd = term?.cwd ? `${repo}/${term.cwd}` : repo;
  return `${cwd} ❯`;
}

function updateTerminalCwdUi() {
  const el = $('#terminal-cwd');
  if (el) el.textContent = terminalPromptFor();
}

function terminalLog(text, terminalId) {
  const term = terminalId
    ? state.terminals.find((t) => t.id === terminalId)
    : getActiveTerminal();
  if (!term) return;
  term.streamLine = null;
  term.lines.push(text);
  if (term.id === state.activeTerminalId) {
    appendTerminalLine(text);
  }
}

function terminalCommandBegin(label, terminalId) {
  const term = terminalId
    ? state.terminals.find((t) => t.id === terminalId)
    : getActiveTerminal();
  if (!term) return;
  const entry = {
    kind: 'cmd-start',
    label: String(label || '').trim() || 'command',
  };
  term.lines.push(entry);
  if (term.id === state.activeTerminalId) {
    appendTerminalEntry(entry);
  }
}

function terminalCommandEnd(exitCode, terminalId) {
  const term = terminalId
    ? state.terminals.find((t) => t.id === terminalId)
    : getActiveTerminal();
  if (!term) return;
  const entry = {
    kind: 'cmd-end',
    exitCode: typeof exitCode === 'number' ? exitCode : null,
  };
  term.lines.push(entry);
  if (term.id === state.activeTerminalId) {
    appendTerminalEntry(entry);
  }
}

function beginTerminalStream(terminalId) {
  const term = terminalId
    ? state.terminals.find((t) => t.id === terminalId)
    : getActiveTerminal();
  if (!term) return;
  term.streamLine = '';
  term.lines.push('');
}

function terminalStreamChunk(text, terminalId) {
  const term = terminalId
    ? state.terminals.find((t) => t.id === terminalId)
    : getActiveTerminal();
  if (!term || term.streamLine == null) return;
  term.streamLine += text;
  term.lines[term.lines.length - 1] = term.streamLine;
  if (term.id === state.activeTerminalId) {
    updateTerminalStreamLine(term);
  }
}

function finalizeTerminalStream(terminalId) {
  const term = terminalId
    ? state.terminals.find((t) => t.id === terminalId)
    : getActiveTerminal();
  if (!term) return;
  term.streamLine = null;
  const out = $('#terminal-output');
  const last = out?.lastElementChild;
  if (last?.dataset?.streaming) delete last.dataset.streaming;
}

function updateTerminalStreamLine(term) {
  const out = $('#terminal-output');
  if (!out) return;
  let el = out.lastElementChild;
  if (!el?.dataset?.streaming) {
    el = document.createElement('div');
    el.className = 'mb-1 whitespace-pre-wrap ij-terminal-line';
    el.dataset.streaming = '1';
    out.appendChild(el);
  }
  el.textContent = term.streamLine || '';
  out.scrollTop = out.scrollHeight;
}

async function handleTerminalCd(cmd) {
  const term = getActiveTerminal();
  const target = cmd.trim() === 'cd' ? '' : cmd.trim().slice(2).trim();
  try {
    const body = { target };
    if (term.cwd) body.cwd = term.cwd;
    const res = await api(repoApi(state.repo, '/workspace/shell/cd'), {
      method: 'POST',
      body: JSON.stringify(body),
    });
    term.cwd = res.cwd || '';
    updateTerminalCwdUi();
    return true;
  } catch (e) {
    terminalLog(`cd: ${e.message}`);
    return false;
  }
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

async function runWorkspaceCommandStream(path, body, { label, terminalId } = {}) {
  const termId = terminalId ?? getActiveTerminal()?.id;
  if (label && termId) terminalCommandBegin(label, termId);
  try {
    const exitCode = await postWorkspaceExecStream(path, body, termId);
    finalizeTerminalStream(termId);
    if (termId) terminalCommandEnd(exitCode, termId);
    return exitCode;
  } catch (e) {
    finalizeTerminalStream(termId);
    if (termId) terminalCommandEnd(-1, termId);
    throw e;
  }
}

async function runTerminalCommand(raw) {
  if (!state.repo) return;
  const term = getActiveTerminal();
  const trimmed = raw.trim();
  if (!trimmed) return;

  if (!term.history.length || term.history[0] !== trimmed) {
    term.history.unshift(trimmed);
    if (term.history.length > 100) term.history.pop();
  }
  term.historyIndex = -1;

  const label = `${terminalPromptFor(term)} ${trimmed}`;

  if (trimmed === 'cd' || trimmed.startsWith('cd ') || trimmed.startsWith('cd\t')) {
    terminalCommandBegin(label, term.id);
    const ok = await handleTerminalCd(trimmed);
    terminalCommandEnd(ok ? 0 : 1, term.id);
    return;
  }

  try {
    const body = { command: trimmed };
    if (term.cwd) body.cwd = term.cwd;
    const exitCode = await runWorkspaceCommandStream('/workspace/shell', body, {
      label,
      terminalId: term.id,
    });
    if (/^git\b/.test(trimmed)) {
      await refreshGitStatus();
      await refreshHistory();
    }
    return exitCode;
  } catch (e) {
    terminalLog(`error: ${e.message}`, term.id);
  }
}

function terminalHistoryUp(input) {
  const term = getActiveTerminal();
  if (!term.history.length) return;
  term.historyIndex = Math.min(
    term.historyIndex + 1,
    term.history.length - 1,
  );
  input.value = term.history[term.historyIndex] || '';
}

function terminalHistoryDown(input) {
  const term = getActiveTerminal();
  if (!term.history.length) return;
  term.historyIndex = Math.max(term.historyIndex - 1, -1);
  input.value = term.historyIndex < 0
    ? ''
    : term.history[term.historyIndex] || '';
}

function bindTerminalTabs() {
  $('#btn-terminal-new')?.addEventListener('click', () => newTerminal());
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

async function finalizeAgentMessage(el, { textBuffer, buffer, summary }) {
  const finalText = pickAgentFinalText(textBuffer, buffer, summary);
  if (!finalText) {
    await window.ReaperAgentMarkdown?.renderAgentContent(el, 'Done — check Source Control for changes.');
    return;
  }
  if (!window.ReaperAgentMarkdown?.renderAgentContent) {
    console.error('[Reaper] ReaperAgentMarkdown not loaded — check /vendor/*.js scripts');
    el.textContent = finalText;
    return;
  }
  await window.ReaperAgentMarkdown.renderAgentContent(el, finalText);
}

function appendAgentMessage(role, text) {
  const box = $('#agent-messages');
  const placeholder = box.querySelector('.agent-msg-system.text-center');
  if (placeholder) box.innerHTML = '';

  const wrap = document.createElement('div');
  wrap.className = `rounded-lg px-3 py-2 ${
    role === 'user' ? 'agent-msg-user text-gray-200' :
    role === 'assistant' ? 'agent-msg-assistant text-gray-300' :
    'agent-msg-system'
  }`;

  if (role !== 'system') {
    const label = document.createElement('div');
    label.className = 'text-[10px] uppercase tracking-wide text-gray-500 mb-1';
    label.textContent = role === 'user' ? 'You' : 'Cursor';
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

function removeAgentMessageWrap(wrap) {
  wrap?.remove();
  const box = $('#agent-messages');
  if (box && !box.querySelector('.agent-msg-user, .agent-msg-assistant')) {
    box.innerHTML = '<div class="agent-msg-system text-center py-4 px-2">Ask the Cursor agent to edit files, run git commands, or explain the codebase.</div>';
  }
}

async function stopAgentChat() {
  if (!state.agentBusy || !state.repo) return;
  state.agentStopRequested = true;
  state.agentMessageQueue = [];
  state.agentAbortController?.abort();
  try {
    await api(repoApi(state.repo, '/cursor/stop'), { method: 'POST' });
  } catch {
    /* stream abort still stops the UI */
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

function updateAgentUi() {
  const canChat = state.repo && state.cursorConfigured && state.cursorBridgeOk;
  $('#agent-input').disabled = !canChat;
  $('#btn-agent-send').disabled = !canChat;
  const modelEl = $('#agent-model');
  if (modelEl) modelEl.disabled = !state.cursorConfigured || !state.cursorBridgeOk;

  let status = 'Ready';
  if (!state.cursorConfigured) status = 'API key not configured';
  else if (!state.cursorBridgeOk) {
    status = state.cursorBridgeError ? `Bridge offline — ${state.cursorBridgeError}` : 'Bridge offline';
  } else if (state.agentBusy) {
    const queued = state.agentMessageQueue.length;
    status = queued ? `Working… · ${queued} queued` : 'Working…';
  } else if (!state.repo) status = 'Select a repo';
  else {
    const modeLabel = state.cursorMode === 'plan' ? 'Plan' : state.cursorMode === 'ask' ? 'Ask' : 'Agent';
    status = `${modeLabel} · ${state.cursorModel || 'composer-2.5'}`;
  }
  $('#agent-status').textContent = status;
  $('#btn-agent-retry')?.classList.toggle('hidden', state.cursorBridgeOk || !state.cursorConfigured);
  $('#agent-config-banner')?.classList.toggle('hidden', state.cursorConfigured);

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

  const hint = !state.cursorConfigured ? 'Configure Cursor in Settings (⌘,)' :
    !state.repo ? 'Select a repo to chat' :
    !state.cursorBridgeOk ? (state.cursorBridgeError || 'Bridge starting… click Retry or restart Reaper') :
    state.agentBusy ? 'Enter to queue another message · Shift+Enter for newline' :
    'Enter to send · Shift+Enter for newline';
  $('#agent-hint').textContent = hint;

  $$('[data-agent-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.agentDock === state.agentDock);
  });

  $$('[data-agent-mode]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.agentMode === state.cursorMode);
  });

  const agentBadge = $('#agent-badge');
  agentBadge?.classList.toggle('hidden', !state.agentBusy);

  const stopBtn = $('#btn-agent-stop');
  if (stopBtn) stopBtn.disabled = !canChat || !state.agentBusy;
  const revertBtn = $('#btn-agent-revert');
  if (revertBtn) revertBtn.disabled = !canChat || state.agentBusy || !state.agentLastRevertibleTurn;
}

function populateAgentModelSelect(models, selectedId) {
  const el = $('#agent-model');
  if (!el) return;
  const list = models?.length ? models : CURSOR_MODELS_FALLBACK;
  const current = selectedId || state.cursorModel || 'composer-2.5';
  el.innerHTML = '';
  const ids = new Set();
  for (const m of list) {
    ids.add(m.id);
    const opt = document.createElement('option');
    opt.value = m.id;
    opt.textContent = m.label || m.id;
    if (m.description) opt.title = m.description;
    el.appendChild(opt);
  }
  if (!ids.has(current)) {
    const opt = document.createElement('option');
    opt.value = current;
    opt.textContent = current;
    el.appendChild(opt);
  }
  el.value = current;
  state.cursorModel = current;
}

async function loadCursorModels() {
  if (!state.cursorConfigured) {
    populateAgentModelSelect(CURSOR_MODELS_FALLBACK, state.cursorModel);
    return;
  }
  try {
    const data = await api('/api/cursor/models');
    const models = data.models || [];
    state.cursorModels = models;
    populateAgentModelSelect(models.length ? models : CURSOR_MODELS_FALLBACK, state.cursorModel);
  } catch {
    populateAgentModelSelect(CURSOR_MODELS_FALLBACK, state.cursorModel);
  }
}

async function setAgentMode(mode) {
  if (!['agent', 'plan', 'ask'].includes(mode) || mode === state.cursorMode) return;
  state.cursorMode = mode;
  $$('[data-agent-mode]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.agentMode === mode);
  });
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

async function setAgentModel(modelId) {
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

function showAgentKeyForm() {
  showSettingsModal('cursor');
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
  } catch {
    state.cursorConfigured = false;
    state.cursorBridgeOk = false;
    state.cursorBridgeError = null;
    state.cursorKeyMasked = null;
    state.cursorKeySource = null;
    populateAgentModelSelect(CURSOR_MODELS_FALLBACK, state.cursorModel);
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
  state.terminalOpen = true;
  applyTerminalDock();
  updateTerminalCwdUi();
  setTimeout(() => $('#terminal-input')?.focus(), 50);
}

function toggleTerminal() {
  if (state.terminalDock === 'left') {
    switchPanel(state.activePanel === 'terminal' ? 'explorer' : 'terminal');
    return;
  }
  state.terminalOpen = !state.terminalOpen;
  applyTerminalDock();
  if (state.terminalOpen) {
    setTimeout(() => $('#terminal-input')?.focus(), 50);
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
  if (!panel || !sidebar || !rightDock || !bottomDock) return;

  const dock = state.agentDock;
  const showAgent = dock === 'left' ? state.activePanel === 'agent' : state.agentOpen;

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

  sidebar.classList.toggle('w-72', dock === 'left' && state.activePanel === 'agent');
  sidebar.classList.toggle('lg:w-80', dock === 'left' && state.activePanel === 'agent');
  sidebar.classList.toggle('w-60', !(dock === 'left' && state.activePanel === 'agent'));
  sidebar.classList.toggle('lg:w-64', !(dock === 'left' && state.activePanel === 'agent'));

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
      await api(repoApi(state.repo, '/cursor/session'), { method: 'DELETE' });
    } catch { /* ignore */ }
  }
  $('#agent-messages').innerHTML = '<div class="agent-msg-system text-center py-6 px-2">New conversation started.</div>';
  state.agentMessageQueue = [];
  state.agentLastRevertibleTurn = null;
  updateAgentUi();
}

async function sendAgentMessage() {
  const prompt = $('#agent-input').value.trim();
  if (!prompt || !state.repo) return;
  if (!state.cursorConfigured || !state.cursorBridgeOk) return;

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
  if (!opts.skipUserBubble) {
    ({ wrap: userWrap } = appendAgentMessage('user', prompt));
  }
  state.agentBusy = true;
  state.agentStopRequested = false;
  state.agentAbortController = new AbortController();
  state.agentLiveFollow = state.cursorMode === 'agent';
  state.agentLiveDiffPath = null;
  state.agentLastToolPath = null;
  state.agentSeenPaths = new Set();
  state.agentHadFileChanges = false;
  const pathsBefore = await snapshotAgentWorkspacePaths();
  updateAgentUi();
  if (state.agentLiveFollow) showAgentDiffPlaceholder();

  const { wrap: assistantWrap, content: assistantEl } = appendAgentMessage('assistant', '…');
  let buffer = '';
  let textBuffer = '';
  let doneSummary = null;
  let cancelled = false;

  try {
    const res = await fetch(repoApi(state.repo, '/cursor/chat'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        prompt,
        model: state.cursorModel,
        mode: state.cursorMode,
      }),
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
          } else if (!buffer && data.status === 'finished') {
            buffer = 'Done — check Source Control for file changes, or reopen files in the editor.';
            textBuffer = buffer;
          } else if (!buffer && data.status === 'error') {
            throw new Error('Agent run failed');
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
    setTimeout(() => $('#terminal-input')?.focus(), 50);
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
      return;
    }

    if (key === 'v') {
      try {
        const text = await navigator.clipboard.readText();
        if (!text) return;
        el.setRangeText(text, start, end, 'end');
        el.dispatchEvent(new Event('input', { bubbles: true }));
        e.preventDefault();
      } catch { /* fall back to native Edit menu */ }
    }
  }, true);
}

// --- Init ---
function bindEvents() {
  installFormClipboardShortcuts();
  $('#status-diagnostics')?.addEventListener('click', jumpToNextDiagnostic);
  $('#repo-select').addEventListener('change', (e) => {
    const name = e.target.value;
    if (shouldOpenRepoInNewWindow(name)) {
      e.target.value = state.repo || '';
      openRepoInNewWindow(name);
      return;
    }
    selectRepo(name);
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
  $('#btn-agent-open-settings')?.addEventListener('click', () => showSettingsModal('cursor'));
  $('#file-modal-cancel').addEventListener('click', hideFileModal);
  $('#new-repo-form').addEventListener('submit', createRepo);
  $('#clone-repo-form')?.addEventListener('submit', cloneRepo);
  $('#publish-repo-form')?.addEventListener('submit', publishToGitHub);
  $('#new-file-form').addEventListener('submit', createFile);
  $('#btn-save')?.addEventListener('click', saveFile);
  $('#tb-save')?.addEventListener('click', saveFile);
  $('#tb-format')?.addEventListener('click', formatDocument);
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
  $$('[data-agent-mode]').forEach((btn) => {
    btn.addEventListener('click', () => setAgentMode(btn.dataset.agentMode));
  });
  $('#agent-model')?.addEventListener('change', (e) => setAgentModel(e.target.value));
  populateAgentModelSelect(CURSOR_MODELS_FALLBACK, state.cursorModel);
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

  $('#terminal-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      runTerminalCommand(e.target.value);
      e.target.value = '';
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      terminalHistoryUp(e.target);
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      terminalHistoryDown(e.target);
    }
  });

  document.addEventListener('keydown', (e) => {
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
      if ($('#goto-class-overlay')?.classList.contains('open')) {
        e.preventDefault();
        hideGoToClass();
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
}

async function init() {
  if (!window.ReaperAgentMarkdown?.libsReady?.()) {
    console.error('[Reaper] Agent markdown not ready — tables/diagrams will show as plain text until scripts load.');
  }
  populateFontSizeSelects();
  populateFontFamilySelects();
  syncFontSizeControls(getEditorFontSize());
  ensureEditorFontLoaded(getEditorFontSpec());
  applyAgentTypography();
  ensureTerminals();
  renderTerminalTabs();
  renderTerminalOutput();
  syncDotfilesControls(getShowDotfiles());
  initEditor();
  bindEvents();
  bindMenus();
  bindPalette();
  bindGoToClass();
  bindBranchPicker();
  mountReaperIcons();
  const build = document.querySelector('meta[name="reaper-ui-build"]')?.content;
  const buildEl = $('#status-build');
  if (buildEl && build) buildEl.textContent = `ui-${build}`;
  initSidebarResize();
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
  } catch (err) {
    toast(`Could not reach Reaper backend: ${err.message}. Quit other Reaper copies and relaunch.`, 'error', { duration: 15000 });
  }
  const initialRepo = getInitialRepoFromUrl();
  if (!initialRepo && !state.repo) {
    showNoRepoFileTree();
  }
  if (initialRepo) {
    const sel = $('#repo-select');
    if (sel && [...sel.options].some((o) => o.value === initialRepo)) {
      sel.value = initialRepo;
      await selectRepo(initialRepo);
    }
  }
  setInterval(async () => {
    if (state.cursorConfigured && !state.cursorBridgeOk && !state.agentBusy) {
      await loadCursorStatus();
    }
  }, 3000);
}

init().catch((e) => toast(e.message, 'error'));
