const AGENT_DOCK_KEY = 'reaper-agent-dock';

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
  agentOpen: true,
  cursorConfigured: false,
  cursorBridgeOk: false,
  cursorBridgeError: null,
  cursorKeyMasked: null,
  agentKeyFormOpen: true,
  agentBusy: false,
  agentLiveFollow: false,
  agentLiveDiffPath: null,
  agentSeenPaths: new Set(),
  editorReady: false,
  suppressEditorChange: false,
  gradleInfo: null,
  repoDetail: null,
};

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => document.querySelectorAll(sel);

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

function toast(msg, type = 'info') {
  const el = $('#toast');
  el.textContent = msg;
  el.className = `fixed bottom-4 right-4 px-4 py-3 rounded-lg shadow-lg text-sm font-medium z-50 ${
    type === 'error' ? 'bg-red-900/90 text-red-200 border border-red-700' :
    type === 'success' ? 'bg-green-900/90 text-green-200 border border-green-700' :
    'bg-surface-800 text-gray-200 border border-surface-600'
  }`;
  el.classList.remove('hidden');
  setTimeout(() => el.classList.add('hidden'), 3500);
}

function langForPath(path) {
  return window.ReaperLang?.langForPath(path) || 'plaintext';
}

function defineDarculaTheme() {
  if (window.__darculaDefined) return;
  window.__darculaDefined = true;
  monaco.editor.defineTheme('darcula', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'comment', foreground: '808080', fontStyle: 'italic' },
      { token: 'keyword', foreground: 'CC7832' },
      { token: 'string', foreground: '6A8759' },
      { token: 'number', foreground: '6897BB' },
      { token: 'type', foreground: 'A9B7C6' },
      { token: 'type.identifier', foreground: 'FFB86C' },
      { token: 'identifier', foreground: 'A9B7C6' },
      { token: 'delimiter', foreground: 'A9B7C6' },
      { token: 'operator', foreground: 'A9B7C6' },
      { token: 'annotation', foreground: 'BBB529' },
    ],
    colors: {
      'editor.background': '#2B2B2B',
      'editor.foreground': '#A9B7C6',
      'editorLineNumber.foreground': '#606366',
      'editorLineNumber.activeForeground': '#A9B7C6',
      'editor.lineHighlightBackground': '#323232',
      'editor.selectionBackground': '#214283',
      'editor.inactiveSelectionBackground': '#21428380',
      'editorCursor.foreground': '#A9B7C6',
      'editorWhitespace.foreground': '#3C3F41',
      'editorIndentGuide.background': '#373737',
      'editorIndentGuide.activeBackground': '#606366',
      'editorGutter.background': '#313335',
      'minimap.background': '#2B2B2B',
      'editorWidget.background': '#3C3F41',
      'editorWidget.border': '#515658',
    },
  });
}

function fileIcon(name) {
  const lower = name.toLowerCase();
  if (lower.endsWith('.java')) return { cls: 'ij-icon-java', label: 'J' };
  if (lower.endsWith('.gradle') || lower === 'gradlew') return { cls: 'ij-icon-gradle', label: 'G' };
  if (lower.endsWith('.kt') || lower.endsWith('.kts')) return { cls: 'ij-icon-kotlin', label: 'K' };
  if (lower.endsWith('.rs')) return { cls: 'ij-icon-rust', label: 'R' };
  if (lower.endsWith('.js') || lower.endsWith('.ts')) return { cls: 'ij-icon-js', label: 'JS' };
  if (lower.endsWith('.json')) return { cls: 'ij-icon-json', label: '{}' };
  if (lower.endsWith('.md')) return { cls: 'ij-icon-md', label: 'M' };
  return { cls: 'ij-icon-file', label: '·' };
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
  return '•';
}

// --- Monaco ---
function setEditorContent(path, content) {
  if (!state.editor) return;
  state.suppressEditorChange = true;
  state.editor.setValue(content ?? '');
  monaco.editor.setModelLanguage(state.editor.getModel(), langForPath(path));
  state.suppressEditorChange = false;
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
    defineDarculaTheme();
    state.editor = monaco.editor.create($('#editor'), {
      value: '',
      language: 'plaintext',
      theme: 'darcula',
      fontFamily: 'JetBrains Mono, Consolas, monospace',
      fontSize: 13,
      lineHeight: 20,
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
      overviewRulerBorder: false,
    });
    state.editor.onDidChangeModelContent(() => {
      if (state.suppressEditorChange || !state.activeTab) return;
      state.tabContents.set(state.activeTab, state.editor.getValue());
      state.dirty.add(state.activeTab);
      updateSaveButton();
      refreshGradleInfo();
      renderTabs();
    });
    state.editor.onDidChangeCursorPosition((e) => updateEditorStatus(e.position));
    window.ReaperLang?.setupEditorFeatures(state.editor, {
      api,
      repoApi,
      getRepo: () => state.repo,
      getActivePath: () => state.activeTab,
      openFileAt,
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
}

async function selectRepo(name) {
  if (!name) {
    state.repo = null;
    resetUI();
    return;
  }
  state.repo = name;
  await api(repoApi(name, '/workspace/open'), { method: 'POST' });
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
}

function resetUI() {
  $('#file-tree').innerHTML = '<p class="px-2 py-4 text-center text-gray-600 text-xs">Open a repository to browse files</p>';
  $('#git-status-list').innerHTML = '';
  $('#commit-history').innerHTML = '';
  state.repoDetail = null;
  const btnRepoInfo = $('#btn-repo-info');
  if (btnRepoInfo) btnRepoInfo.disabled = true;
  $('#branch-select').disabled = true;
  ['#btn-sync', '#btn-save', '#tb-save', '#tb-format', '#tb-run', '#btn-commit', '#btn-new-file', '#gradle-task', '#terminal-input'].forEach((s) => { const el = $(s); if (el) el.disabled = true; });
  $('#editor-toolbar')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.remove('flex');
  closeAllTabs();
  $('#empty-state')?.classList.remove('hidden');
  $('#editor-container').classList.add('hidden');
  updateAgentUi();
}

function enableControls() {
  $('#branch-select').disabled = false;
  const btnRepoInfo = $('#btn-repo-info');
  if (btnRepoInfo) btnRepoInfo.disabled = false;
  ['#btn-sync', '#btn-save', '#tb-save', '#tb-format', '#tb-run', '#btn-commit', '#btn-new-file', '#gradle-task', '#terminal-input'].forEach((s) => { const el = $(s); if (el) el.disabled = false; });
  updateRunButtons();
}

function updateBranchSelect() {
  const sel = $('#branch-select');
  sel.innerHTML = '';
  state.branches.forEach((b) => {
    const opt = document.createElement('option');
    opt.value = b;
    opt.textContent = b;
    sel.appendChild(opt);
  });
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
    toast(err.message || 'Failed to create repository', 'error');
  }
}

// --- File tree ---
function renderTree(nodes, depth = 0) {
  return nodes.map((n) => {
    if (n.type === 'dir') {
      const children = n.children?.length ? renderTree(n.children, depth + 1) : '';
      return `
        <details ${depth < 2 ? 'open' : ''} class="group">
          <summary class="tree-item flex items-center gap-1.5 px-2 py-0.5 cursor-pointer text-gray-400 hover:text-gray-200" style="padding-left:${depth * 14 + 6}px">
            <span class="ij-icon-dir text-[11px] font-bold w-4 text-center">📁</span>
            <span class="truncate">${n.name}</span>
          </summary>
          ${children}
        </details>`;
    }
    const icon = fileIcon(n.name);
    return `
      <button data-path="${n.path}" class="tree-file tree-item w-full flex items-center gap-1.5 px-2 py-0.5 text-gray-400 hover:text-gray-200 text-left" style="padding-left:${depth * 14 + 6}px">
        <span class="${icon.cls} text-[10px] font-bold w-4 text-center shrink-0">${icon.label}</span>
        <span class="truncate">${n.name}</span>
      </button>`;
  }).join('');
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

async function refreshTree() {
  const tree = await api(repoApi(state.repo, '/workspace/tree'));
  $('#file-tree').innerHTML = renderTree(tree);
  $$('.tree-file').forEach((btn) => {
    btn.addEventListener('click', () => openFile(btn.dataset.path));
  });
}

// --- Tabs & editor ---
async function openFile(path) {
  if (state.tabs.includes(path)) {
    activateTab(path);
    return;
  }
  const data = await api(`${repoApi(state.repo, '/workspace/file')}?path=${encodeURIComponent(path)}`);
  let content = data.content;
  if (path.split('/').pop()?.toLowerCase() === 'readme.md' && !content.trim()) {
    content = defaultReadmeContent(path);
  }
  state.tabContents.set(path, content);
  state.tabs.push(path);
  if (content !== data.content) state.dirty.add(path);
  renderTabs();
  activateTab(path);
  if (!state.editorReady) syncEditorFromActiveTab();
  $$('.tree-file').forEach((b) => b.classList.toggle('active', b.dataset.path === path));
  refreshGradleInfo();
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

function activateTab(path) {
  state.activeTab = path;
  renderTabs();
  $('#editor-container').classList.remove('hidden');
  $('#editor-toolbar')?.classList.remove('hidden');
  $('#editor-toolbar')?.classList.add('flex');
  $('#empty-state')?.classList.add('hidden');
  if (state.tabContents.has(path)) {
    setEditorContent(path, state.tabContents.get(path));
  }
  updateBreadcrumbs(path);
  const langEl = $('#status-language');
  if (langEl) langEl.textContent = window.ReaperLang?.langLabel(langForPath(path)) || 'Plain Text';
  if (state.editor) updateEditorStatus(state.editor.getPosition());
  $$('.tree-file').forEach((b) => b.classList.toggle('active', b.dataset.path === path));
  updateSaveButton();
  refreshGradleInfo();
}

async function refreshGradleInfo() {
  if (!state.repo || !state.activeTab) {
    state.gradleInfo = null;
    updateRunButtons();
    return;
  }
  try {
    state.gradleInfo = await api(
      `${repoApi(state.repo, '/workspace/gradle/info')}?path=${encodeURIComponent(state.activeTab)}`,
    );
  } catch {
    state.gradleInfo = null;
  }
  updateRunButtons();
}

function updateRunButtons() {
  const tbRun = $('#tb-run');
  const taskSel = $('#gradle-task');
  const runLabel = $('#toolbar-run-label');
  const info = state.gradleInfo;
  state.javaRunTarget = null;

  if (info?.is_gradle) {
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
  } else {
    taskSel?.classList.add('hidden');
    runLabel?.classList.add('hidden');
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
    await openFile(path);
  } else {
    activateTab(path);
  }
  if (!state.editor) return;
  state.editor.revealLineInCenter(line);
  state.editor.setPosition({ lineNumber: line, column });
  state.editor.focus();
}

async function formatDocument() {
  if (!state.editor || !state.activeTab) return;
  try {
    await state.editor.getAction('editor.action.formatDocument')?.run();
    toast('Formatted', 'success');
  } catch (e) {
    toast(e.message || 'Format failed — install a formatter for this language', 'error');
  }
}

function closeAllTabs() {
  state.tabs = [];
  state.tabContents.clear();
  state.activeTab = null;
  state.dirty.clear();
  const list = $('#tab-list');
  if (!list) return;
  list.innerHTML = '';
  const empty = document.createElement('div');
  empty.id = 'empty-state';
  empty.className = 'flex-1 flex flex-col items-center justify-center text-center p-8';
  empty.innerHTML = `
    <div class="w-16 h-16 rounded-2xl bg-surface-800 border border-surface-700 flex items-center justify-center mb-4">
      <svg class="w-8 h-8 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"/></svg>
    </div>
    <h2 class="text-lg font-semibold text-white mb-1">Reaper</h2>
    <p class="text-sm text-gray-500 max-w-md">Create or select a repository, then edit files with syntax highlighting, manage changes, and run git commands.</p>
    <button id="btn-new-repo-empty" class="mt-6 px-4 py-2 rounded-md bg-accent hover:bg-accent-hover text-white text-sm font-medium">Create your first repo</button>`;
  list.appendChild(empty);
  $('#btn-new-repo-empty')?.addEventListener('click', showModal);
  $('#editor-container').classList.add('hidden');
  $('#editor-toolbar')?.classList.add('hidden');
  $('#editor-toolbar')?.classList.remove('flex');
  updateBreadcrumbs(null);
  updateRunButtons();
}

async function saveFile() {
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
    await refreshTree();
    await refreshGitStatus();
    toast('Saved', 'success');
  } catch (err) {
    toast(err.message || 'Failed to save', 'error');
  }
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
  switchPanel('terminal');
  const task = $('#gradle-task')?.value || state.gradleInfo.default_task;
  terminalLog(`▶ gradle ${task}  (${state.gradleInfo.project_root})`);
  try {
    const out = await api(repoApi(state.repo, '/workspace/gradle/run'), {
      method: 'POST',
      body: JSON.stringify({ path: state.activeTab, task }),
    });
    if (out.stdout) terminalLog(out.stdout.trimEnd());
    if (out.stderr) terminalLog(out.stderr.trimEnd());
    if (out.exit_code !== 0) terminalLog(`exit ${out.exit_code}`);
  } catch (e) {
    terminalLog(`error: ${e.message}`);
  }
}

async function runActive() {
  if (state.gradleInfo?.is_gradle) await runGradle();
  else await runJavaMain();
}
async function runJavaMain() {
  if (!state.repo || !state.activeTab?.endsWith('.java')) return;
  if (state.dirty.has(state.activeTab)) await saveFile();
  switchPanel('terminal');
  const name = state.javaRunTarget || state.activeTab;
  terminalLog(`▶ java ${name}`);
  try {
    const out = await api(repoApi(state.repo, '/workspace/java/run'), {
      method: 'POST',
      body: JSON.stringify({ path: state.activeTab }),
    });
    if (out.stdout) terminalLog(out.stdout.trimEnd());
    if (out.stderr) terminalLog(out.stderr.trimEnd());
    if (out.exit_code !== 0) terminalLog(`exit ${out.exit_code}`);
  } catch (e) {
    terminalLog(`error: ${e.message}`);
  }
}

// --- Git ---
async function showFileDiff(path, staged = false) {
  if (!state.repo || !path) return;
  const diff = await api(`${repoApi(state.repo, '/workspace/diff')}?path=${encodeURIComponent(path)}&staged=${staged}`);
  $('#diff-panel').classList.remove('hidden');
  const label = $('#diff-path');
  if (label) label.textContent = path;
  $('#diff-content').textContent = diff.diff || '(no diff)';
  $$('#git-status-list button').forEach((btn) => {
    btn.classList.toggle('bg-surface-800', btn.dataset.statusPath === path);
    btn.classList.toggle('ring-1', btn.dataset.statusPath === path);
    btn.classList.toggle('ring-accent/30', btn.dataset.statusPath === path);
  });
}

async function refreshGitStatus() {
  if (!state.repo) return { clean: true, files: [], branch: '' };
  const status = await api(repoApi(state.repo, '/workspace/status'));
  $('#branch-select').value = status.branch;
  const badge = $('#git-badge');
  if (badge) {
    if (status.clean) {
      badge.classList.add('hidden');
    } else {
      badge.textContent = String(status.files.length);
      badge.classList.remove('hidden');
    }
  }
  const list = $('#git-status-list');
  if (status.clean) {
    list.innerHTML = '<p class="px-2 py-4 text-center text-gray-600 text-xs">Working tree clean</p>';
    return { clean: true, files: [], branch: status.branch };
  }
  list.innerHTML = status.files.map((f) => `
    <button data-status-path="${f.path}" data-staged="${f.staged}" data-status="${f.status}" class="w-full flex items-center gap-2 px-2 py-1.5 rounded hover:bg-surface-800 text-left group">
      <span class="w-4 text-center font-mono text-xs font-bold ${statusColor(f.status)}">${statusIcon(f.status)}</span>
      <span class="truncate text-gray-300 text-xs flex-1">${f.path}</span>
      <span class="text-[10px] text-gray-600 uppercase">${f.staged ? 'staged' : 'worktree'}</span>
    </button>
  `).join('');
  list.querySelectorAll('button').forEach((btn) => {
    btn.addEventListener('click', () => {
      showFileDiff(btn.dataset.statusPath, btn.dataset.staged === 'true');
    });
  });
  if (state.agentLiveDiffPath) {
    $$('#git-status-list button').forEach((btn) => {
      btn.classList.toggle('bg-surface-800', btn.dataset.statusPath === state.agentLiveDiffPath);
      btn.classList.toggle('ring-1', btn.dataset.statusPath === state.agentLiveDiffPath);
      btn.classList.toggle('ring-accent/30', btn.dataset.statusPath === state.agentLiveDiffPath);
    });
  }
  return { clean: false, files: status.files, branch: status.branch };
}

async function followAgentFileChanges(status) {
  if (!status.files?.length) return;

  const pick = status.files.find((f) => !state.agentSeenPaths.has(f.path))
    || status.files.find((f) => f.path === state.agentLiveDiffPath)
    || status.files[0];
  if (!pick) return;

  const isNewPath = !state.agentSeenPaths.has(pick.path);
  state.agentSeenPaths.add(pick.path);

  if (!state.agentLiveFollow) {
    state.agentLiveFollow = true;
    if (state.agentDock !== 'left') switchPanel('git');
  }

  if (isNewPath || pick.path !== state.agentLiveDiffPath) {
    state.agentLiveDiffPath = pick.path;
    await showFileDiff(pick.path, pick.staged);
    if (pick.status !== 'deleted') {
      try { await openFile(pick.path); } catch { /* ignore */ }
    }
  } else {
    await showFileDiff(pick.path, pick.staged);
  }
}

async function refreshAfterAgent({ fromAgent = false } = {}) {
  await refreshTree();
  const status = await refreshGitStatus();
  await reloadOpenTabsFromDisk();
  if (fromAgent && state.agentBusy && !status.clean) {
    await followAgentFileChanges(status);
  }
  if (!status.clean && fromAgent) toast('Agent updated files', 'success');
  return !status.clean;
}

let agentRefreshTimer = null;
function scheduleAgentWorkspaceRefresh() {
  clearTimeout(agentRefreshTimer);
  agentRefreshTimer = setTimeout(() => {
    refreshAfterAgent({ fromAgent: true }).catch(() => {});
  }, 400);
}

async function commit() {
  const message = $('#commit-message').value.trim();
  if (!message) { toast('Enter a commit message', 'error'); return; }
  const out = await api(repoApi(state.repo, '/workspace/commit'), {
    method: 'POST',
    body: JSON.stringify({ message }),
  });
  terminalLog(out.stdout || out.stderr || 'Committed');
  $('#commit-message').value = '';
  await refreshGitStatus();
  await refreshHistory();
  await refreshTree();
  toast('Committed & pushed', 'success');
}

async function refreshHistory() {
  if (!state.repo) return;
  const commits = await api(`${repoApi(state.repo, '/log')}?limit=30`);
  $('#commit-history').innerHTML = commits.map((c) => `
    <div class="px-2 py-2 rounded hover:bg-surface-800 border border-transparent hover:border-surface-700">
      <div class="text-gray-200 text-xs font-medium truncate">${c.subject}</div>
      <div class="flex items-center gap-2 mt-1">
        <code class="text-[10px] text-accent">${c.hash.slice(0, 7)}</code>
        <span class="text-[10px] text-gray-600">${c.author}</span>
      </div>
    </div>
  `).join('') || '<p class="text-gray-600 text-xs text-center py-4">No commits yet</p>';
}

async function syncPull() {
  const out = await api(repoApi(state.repo, '/workspace/sync'), { method: 'POST' });
  terminalLog(out.stdout || out.stderr || 'Synced');
  await refreshTree();
  await refreshGitStatus();
  toast('Pulled latest', 'success');
}

async function checkoutBranch(branch) {
  const out = await api(repoApi(state.repo, '/workspace/checkout'), {
    method: 'POST',
    body: JSON.stringify({ branch }),
  });
  terminalLog(out.stdout || out.stderr || `Switched to ${branch}`);
  await refreshTree();
  await refreshGitStatus();
}

// --- Terminal ---
function terminalLog(text) {
  const out = $('#terminal-output');
  const line = document.createElement('div');
  line.className = 'mb-1 whitespace-pre-wrap';
  line.textContent = text;
  out.appendChild(line);
  out.scrollTop = out.scrollHeight;
}

async function runTerminalCommand(raw) {
  const trimmed = raw.trim();
  if (!trimmed) return;
  terminalLog(`$ ${trimmed}`);
  const args = trimmed.startsWith('git ') ? trimmed.slice(4).split(/\s+/) : trimmed.split(/\s+/);
  try {
    const out = await api(repoApi(state.repo, '/workspace/git'), {
      method: 'POST',
      body: JSON.stringify({ args }),
    });
    if (out.stdout) terminalLog(out.stdout);
    if (out.stderr) terminalLog(out.stderr);
    if (out.exit_code !== 0) terminalLog(`exit ${out.exit_code}`);
    await refreshGitStatus();
    await refreshHistory();
  } catch (e) {
    terminalLog(`error: ${e.message}`);
  }
}

// --- Cursor Agent ---
function scrollAgentToBottom() {
  const box = $('#agent-messages');
  if (box) box.scrollTop = box.scrollHeight;
}

function appendAgentMessage(role, text) {
  const box = $('#agent-messages');
  const placeholder = box.querySelector('.agent-msg-system.text-center');
  if (placeholder) box.innerHTML = '';

  const wrap = document.createElement('div');
  wrap.className = `rounded-lg px-3 py-2 text-sm ${
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
  content.className = 'agent-text whitespace-pre-wrap break-words';
  content.textContent = text;
  wrap.appendChild(content);
  box.appendChild(wrap);
  scrollAgentToBottom();
  return content;
}

function updateAgentUi() {
  const canChat = state.repo && state.cursorConfigured && state.cursorBridgeOk && !state.agentBusy;
  $('#agent-input').disabled = !canChat;
  $('#btn-agent-send').disabled = !canChat;

  let status = 'Ready';
  if (!state.cursorConfigured) status = 'Paste API key below';
  else if (!state.cursorBridgeOk) {
    status = state.cursorBridgeError ? `Bridge offline — ${state.cursorBridgeError}` : 'Bridge offline';
  } else if (state.agentBusy) status = 'Working…';
  else if (!state.repo) status = 'Select a repo';
  $('#agent-status').textContent = status;
  $('#btn-agent-retry')?.classList.toggle('hidden', state.cursorBridgeOk || !state.cursorConfigured);

  const showForm = !state.cursorConfigured || state.agentKeyFormOpen;
  $('#agent-setup').classList.toggle('hidden', !showForm);
  $('#agent-key-saved').classList.toggle('hidden', !state.cursorConfigured || showForm);
  $('#btn-cancel-cursor-key').classList.toggle('hidden', !state.cursorConfigured);
  if (state.cursorKeyMasked) {
    $('#cursor-key-masked').textContent = state.cursorKeyMasked;
  }

  const hint = !state.cursorConfigured ? 'Save your API key above to enable chat' :
    !state.repo ? 'Select a repo to chat' :
    !state.cursorBridgeOk ? (state.cursorBridgeError || 'Bridge starting… click Retry or restart Reaper') :
    'Enter to send · Shift+Enter for newline';
  $('#agent-hint').textContent = hint;

  $$('[data-agent-dock]').forEach((btn) => {
    btn.classList.toggle('active', btn.dataset.agentDock === state.agentDock);
  });
}

function showAgentKeyForm() {
  state.agentKeyFormOpen = true;
  updateAgentUi();
  setTimeout(() => $('#cursor-api-key')?.focus(), 50);
}

function hideAgentKeyForm() {
  state.agentKeyFormOpen = false;
  $('#cursor-api-key').value = '';
  updateAgentUi();
}

async function loadCursorStatus() {
  try {
    const cfg = await api('/api/cursor/status');
    state.cursorConfigured = cfg.configured;
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    state.cursorKeyMasked = cfg.masked || null;
    state.agentKeyFormOpen = !cfg.configured;
  } catch {
    state.cursorConfigured = false;
    state.cursorBridgeOk = false;
    state.cursorBridgeError = null;
    state.cursorKeyMasked = null;
    state.agentKeyFormOpen = true;
  }
  updateAgentUi();
  if (state.agentKeyFormOpen) {
    setTimeout(() => $('#cursor-api-key')?.focus(), 100);
  }
}

async function restartBridge() {
  try {
    const cfg = await api('/api/cursor/bridge/restart', { method: 'POST' });
    state.cursorBridgeOk = cfg.bridge_ok;
    state.cursorBridgeError = cfg.bridge_error || null;
    updateAgentUi();
    toast(cfg.bridge_ok ? 'Bridge connected' : (cfg.bridge_error || 'Bridge still offline'), cfg.bridge_ok ? 'success' : 'error');
  } catch (err) {
    toast(err.message, 'error');
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

  const agentBtn = $('.activity-btn[data-panel="agent"]');
  agentBtn?.classList.toggle('text-accent', dock === 'left' ? state.activePanel === 'agent' : state.agentOpen);
  agentBtn?.classList.toggle('bg-surface-800', dock === 'left' ? state.activePanel === 'agent' : state.agentOpen);
  agentBtn?.classList.toggle('text-gray-400', dock === 'left' ? state.activePanel !== 'agent' : !state.agentOpen);

  updateAgentUi();
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

async function saveCursorKey(e) {
  e?.preventDefault();
  const key = $('#cursor-api-key').value.trim();
  if (!key) {
    toast('Paste your Cursor API key', 'error');
    $('#cursor-api-key')?.focus();
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
    state.agentKeyFormOpen = false;
    $('#cursor-api-key').value = '';
    updateAgentUi();
    toast('Cursor API key saved', 'success');
  } catch (err) {
    toast(err.message, 'error');
  }
}

async function clearAgentSession() {
  if (state.repo) {
    try {
      await api(repoApi(state.repo, '/cursor/session'), { method: 'DELETE' });
    } catch { /* ignore */ }
  }
  $('#agent-messages').innerHTML = '<div class="agent-msg-system text-center py-6 px-2">New conversation started.</div>';
}

async function sendAgentMessage() {
  const prompt = $('#agent-input').value.trim();
  if (!prompt || !state.repo || state.agentBusy) return;

  appendAgentMessage('user', prompt);
  $('#agent-input').value = '';
  state.agentBusy = true;
  state.agentLiveFollow = false;
  state.agentLiveDiffPath = null;
  state.agentSeenPaths = new Set();
  updateAgentUi();

  const assistantEl = appendAgentMessage('assistant', '…');
  let buffer = '';

  try {
    const res = await fetch(repoApi(state.repo, '/cursor/chat'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ prompt }),
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
    assistantEl.textContent = '';

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
        if (data.type === 'text' || data.type === 'tool') {
          buffer += data.text;
          assistantEl.textContent = buffer;
          scrollAgentToBottom();
          if (data.type === 'tool') scheduleAgentWorkspaceRefresh();
        } else if (data.type === 'error') {
          throw new Error(data.error);
        } else if (data.type === 'done') {
          if (data.summary) buffer += (buffer ? '\n\n' : '') + data.summary;
          if (!buffer && data.status === 'finished') {
            buffer = 'Done — check Source Control for file changes, or reopen files in the editor.';
          } else if (!buffer && data.status === 'error') {
            throw new Error('Agent run failed');
          }
        }
      }
    }

    if (!buffer) assistantEl.textContent = 'Done — check Source Control for changes.';
    else assistantEl.textContent = buffer;
    clearTimeout(agentRefreshTimer);
    await refreshAfterAgent({ fromAgent: true });
  } catch (e) {
    const msg = e.message || String(e);
    if (/invalid api key/i.test(msg)) {
      showAgentKeyForm();
      toast('Invalid API key — paste a new one from Cursor → Integrations', 'error');
    } else {
      toast(msg, 'error');
    }
    assistantEl.textContent = msg;
    assistantEl.classList.add('text-red-400');
  } finally {
    state.agentBusy = false;
    updateAgentUi();
  }
}

// --- Panels ---
function switchPanel(name) {
  if (name === 'agent' && state.agentDock !== 'left') {
    openAgent();
    return;
  }

  state.activePanel = name;
  $$('.activity-btn').forEach((b) => {
    let on = b.dataset.panel === name;
    if (b.dataset.panel === 'agent' && state.agentDock !== 'left') {
      on = state.agentOpen;
    }
    b.classList.toggle('text-accent', on);
    b.classList.toggle('bg-surface-800', on);
    b.classList.toggle('text-gray-400', !on);
  });
  const titles = {
    explorer: 'Project',
    git: 'Commit',
    history: 'Git Log',
    terminal: 'Terminal',
    agent: 'Agent',
  };
  $('#sidebar-title').textContent = titles[name] || name;
  $$('#sidebar > .panel').forEach((p) => {
    if (p.id === 'panel-agent') return;
    p.classList.toggle('hidden', p.id !== `panel-${name}`);
  });
  applyAgentDock();
  if (name === 'git') refreshGitStatus();
  if (name === 'history') refreshHistory();
  if (name === 'agent') {
    loadCursorStatus();
    setTimeout(() => $('#agent-input')?.focus(), 50);
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

// --- Init ---
function bindEvents() {
  $('#repo-select').addEventListener('change', (e) => selectRepo(e.target.value));
  $('#branch-select').addEventListener('change', (e) => checkoutBranch(e.target.value));
  $('#btn-new-repo').addEventListener('click', showModal);
  $('#btn-open-agent').addEventListener('click', toggleAgent);
  $('#btn-new-repo-empty')?.addEventListener('click', showModal);
  $('#btn-new-file').addEventListener('click', showFileModal);
  $('#modal-cancel').addEventListener('click', hideModal);
  $('#file-modal-cancel').addEventListener('click', hideFileModal);
  $('#new-repo-form').addEventListener('submit', createRepo);
  $('#new-file-form').addEventListener('submit', createFile);
  $('#btn-save')?.addEventListener('click', saveFile);
  $('#tb-save')?.addEventListener('click', saveFile);
  $('#tb-format')?.addEventListener('click', formatDocument);
  $('#tb-run')?.addEventListener('click', runActive);
  $('#gradle-task')?.addEventListener('change', () => updateRunButtons());
  $('#btn-commit').addEventListener('click', commit);
  $('#btn-sync').addEventListener('click', syncPull);
  $('#btn-repo-info')?.addEventListener('click', showRepoInfoModal);
  $('#repo-info-close')?.addEventListener('click', hideRepoInfoModal);
  $('#repo-info-overlay')?.addEventListener('click', (e) => {
    if (e.target === $('#repo-info-overlay')) hideRepoInfoModal();
  });
  $('#btn-agent-send').addEventListener('click', sendAgentMessage);
  $('#agent-setup').addEventListener('submit', saveCursorKey);
  $('#btn-agent-settings').addEventListener('click', showAgentKeyForm);
  $('#btn-edit-cursor-key').addEventListener('click', showAgentKeyForm);
  $('#btn-cancel-cursor-key').addEventListener('click', hideAgentKeyForm);
  $('#btn-agent-clear').addEventListener('click', clearAgentSession);
  $('#btn-close-diff')?.addEventListener('click', () => {
    $('#diff-panel').classList.add('hidden');
    state.agentLiveDiffPath = null;
  });
  $('#btn-agent-retry')?.addEventListener('click', restartBridge);
  $$('[data-agent-dock]').forEach((btn) => {
    btn.addEventListener('click', () => setAgentDock(btn.dataset.agentDock));
  });

  $('#agent-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendAgentMessage();
    }
  });

  $$('.activity-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      if (btn.dataset.panel === 'agent' && state.agentDock !== 'left') {
        toggleAgent();
      } else {
        switchPanel(btn.dataset.panel);
      }
    });
  });

  $('#terminal-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      runTerminalCommand(e.target.value);
      e.target.value = '';
    }
  });

  document.addEventListener('keydown', (e) => {
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
  });

  $('#modal-overlay').addEventListener('click', (e) => {
    if (e.target === $('#modal-overlay')) hideModal();
  });

  $('#file-modal-overlay').addEventListener('click', (e) => {
    if (e.target === $('#file-modal-overlay')) hideFileModal();
  });
}

async function init() {
  initEditor();
  bindEvents();
  applyAgentDock();
  switchPanel('explorer');
  await loadCursorStatus();
  await loadRepos();
  setInterval(async () => {
    if (state.cursorConfigured && !state.cursorBridgeOk && !state.agentBusy) {
      await loadCursorStatus();
    }
  }, 3000);
}

init().catch((e) => toast(e.message, 'error'));
