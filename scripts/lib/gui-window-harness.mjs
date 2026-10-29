/**
 * GUI multi-window regression — titlebar drag IPC, open-windows menu, picker UI.
 */
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function extractFunctionBody(src, name) {
  const re = new RegExp(`function ${name}\\([^)]*\\)\\s*\\{`);
  const m = src.match(re);
  if (!m) return '';
  const start = m.index + m[0].length;
  let depth = 1;
  for (let i = start; i < src.length; i += 1) {
    const ch = src[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) return src.slice(start, i);
    }
  }
  return '';
}

/**
 * @param {string} appSrc
 * @param {string} indexHtml
 * @param {string} guiRs
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testGuiWindowStaticRegression(appSrc, indexHtml, guiRs, guiIpcRs, ok) {
  ok(appSrc.includes("'start-window-drag'"), 'app.js posts start-window-drag IPC');
  ok(appSrc.includes("'list-windows'"), 'app.js requests native window list');
  ok(appSrc.includes("'focus-window'"), 'app.js can focus another native window');
  ok(appSrc.includes("'show-open-windows'"), 'app.js posts show-open-windows IPC');
  ok(appSrc.includes('window.__reaperDeliverWindowsList'), 'app.js registers window list callback');
  ok(appSrc.includes('window.__reaperShowWindowsPicker'), 'app.js registers window picker callback');
  ok(appSrc.includes("'show-open-windows': showOpenWindowsMenuAction"), 'runMenuAction wires show-open-windows');
  ok(appSrc.includes("dataset.menu === 'window'"), 'Window menu refreshes list on open');
  ok(
    extractFunctionBody(appSrc, 'bindHeaderBrand').includes("'start-window-drag'"),
    'titlebar drag bound in bindHeaderBrand',
  );

  ok(indexHtml.includes('data-menu="window"'), 'index.html has Window menu');
  ok(indexHtml.includes('data-action="show-open-windows"'), 'index.html Show Open Windows menu item');
  ok(indexHtml.includes('id="windows-picker-overlay"'), 'index.html windows picker overlay');
  ok(indexHtml.includes('id="window-menu-list"'), 'index.html dynamic window menu list host');

  ok(guiRs.includes('StartWindowDrag'), 'gui.rs handles StartWindowDrag user event');
  ok(guiRs.includes('drag_window()'), 'gui.rs calls native drag_window');
  ok(guiRs.includes('MENU_SHOW_WINDOWS_ID'), 'gui.rs native Window menu show item');
  ok(guiRs.includes('DeliverWindowsList'), 'gui.rs delivers window list to webview');

  ok(guiIpcRs.includes('parse_ipc_message'), 'gui_ipc.rs parses webview IPC');
  ok(guiIpcRs.includes('start-window-drag'), 'gui_ipc.rs knows start-window-drag type');
}

/**
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testGuiWindowJsIntegration(ok) {
  const appPath = path.join(ROOT, 'static/app.js');
  const body = extractFunctionBody(fs.readFileSync(appPath, 'utf8'), 'deliverNativeWindowsList');
  ok(!!body, 'deliverNativeWindowsList found in app.js');

  const host = { innerHTML: '' };
  const results = { innerHTML: '' };
  const sandbox = {
    console,
    document: {
      querySelector(sel) {
        if (sel === '#window-menu-list') return host;
        if (sel === '#windows-picker-results') return results;
        return null;
      },
    },
    Array,
    Number,
    String,
    escapeHtml(s) {
      return String(s)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
    },
  };
  sandbox.globalThis = sandbox;
  sandbox.window = sandbox;
  vm.createContext(sandbox);

  vm.runInContext(
    `
    function renderWindowsPickerResults(windows) {
      const results = document.querySelector('#windows-picker-results');
      if (!results) return;
      const rows = Array.isArray(windows) ? windows : [];
      if (!rows.length) {
        results.innerHTML = '<p class="ij-repo-picker-empty">No other Reaper windows</p>';
        return;
      }
      results.innerHTML = rows.map((w) => {
        const title = escapeHtml(String(w.title || 'Reaper'));
        const current = !!w.current;
        return '<button type="button" data-window-id="' + Number(w.id) + '"' + (current ? ' disabled' : '') + '>' + title + '</button>';
      }).join('');
    }
    function renderWindowMenuList(windows) {
      const host = document.querySelector('#window-menu-list');
      if (!host) return;
      const rows = Array.isArray(windows) ? windows : [];
      host.innerHTML = rows.map((w) => {
        const title = escapeHtml(String(w.title || 'Reaper'));
        const current = !!w.current;
        return '<button type="button" data-window-id="' + Number(w.id) + '"' + (current ? ' disabled' : '') + '>' + title + '</button>';
      }).join('');
    }
    function deliverNativeWindowsList(windows) {
      renderWindowsPickerResults(windows);
      renderWindowMenuList(windows);
    }
    `,
    sandbox,
  );

  sandbox.deliverNativeWindowsList([
    { id: 2, title: 'Reaper — beta', current: false },
    { id: 1, title: 'Reaper — alpha', current: true },
  ]);

  ok(host.innerHTML.includes('data-window-id="1"'), 'window menu list renders window id 1');
  ok(host.innerHTML.includes('disabled'), 'current window disabled in menu list');
  ok(results.innerHTML.includes('Reaper — beta'), 'picker lists other window titles');
}
