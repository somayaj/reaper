/**
 * Terminal regression helpers — xterm load, FitAddon export shapes, PTY websocket echo.
 */
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const STATIC = path.join(ROOT, 'static');

const XTERM_API_SRC = `
function resolveFitAddonCtor() {
  const raw = globalThis.FitAddon;
  if (typeof raw === 'function') return raw;
  if (raw && typeof raw.FitAddon === 'function') return raw.FitAddon;
  if (raw?.default && typeof raw.default === 'function') return raw.default;
  if (raw?.default && typeof raw.default.FitAddon === 'function') return raw.default.FitAddon;
  return null;
}
function xtermApi() {
  const Terminal = globalThis.Terminal;
  if (typeof Terminal !== 'function') return null;
  return { Terminal, FitAddon: resolveFitAddonCtor() };
}
xtermApi();
`;

const XTERM_API_OLD_SRC = `
(function () {
  const Terminal = globalThis.Terminal;
  const FitAddon = globalThis.FitAddon?.FitAddon;
  if (!Terminal || !FitAddon) return null;
  return { Terminal, FitAddon };
})()
`;

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

function createXtermSandbox() {
  const sandbox = { console };
  sandbox.globalThis = sandbox;
  sandbox.self = sandbox;
  vm.createContext(sandbox);
  return sandbox;
}

function loadVendoredFitAddon(sandbox) {
  vm.runInContext(
    fs.readFileSync(path.join(STATIC, 'vendor/xterm/addon-fit.js'), 'utf8'),
    sandbox,
    { filename: 'addon-fit.js' },
  );
}

function evalXtermApi(sandbox) {
  return vm.runInContext(XTERM_API_SRC, sandbox);
}

function evalXtermApiOld(sandbox) {
  return vm.runInContext(XTERM_API_OLD_SRC, sandbox);
}

export function testTerminalStaticRegression(appSrc, indexHtml, ok) {
  ok(
    appSrc.includes('function resolveFitAddonCtor()'),
    'terminal: resolveFitAddonCtor handles UMD export shapes',
  );
  ok(
    appSrc.includes('function isTerminalPanelVisible()'),
    'terminal: defer xterm spawn until panel is visible',
  );
  ok(
    appSrc.includes('typeof Terminal !== \'function\''),
    'terminal: xtermApi requires Terminal constructor',
  );

  const resetBody = extractFunctionBody(appSrc, 'resetTerminalCwds');
  ok(resetBody.includes('isTerminalPanelVisible()'), 'terminal: resetTerminalCwds defers spawn when panel hidden');
  ok(resetBody.includes('destroyTerminalInstance(t)'), 'terminal: resetTerminalCwds tears down hidden sessions');

  const initBody = extractFunctionBody(appSrc, 'initTerminalXterm');
  ok(initBody.includes('const api = xtermApi()'), 'terminal: initTerminalXterm loads xterm via xtermApi');
  ok(initBody.includes('xterm.onData'), 'terminal: keystrokes wired through xterm.onData');
  ok(initBody.includes('term.ws.send(data)'), 'terminal: onData forwards input to PTY websocket');
  ok(initBody.includes('connectTerminalWs(term)'), 'terminal: init connects PTY websocket');

  const mountBody = extractFunctionBody(appSrc, 'mountActiveTerminal');
  ok(
    mountBody.includes('WebSocket.OPEN'),
    'terminal: remount reconnects when websocket is not open',
  );
  ok(
    mountBody.includes('xtermMissing') || mountBody.includes('element?.isConnected'),
    'terminal: remount respawns when xterm is missing or detached',
  );
  ok(
    mountBody.includes('spawnTerminalInstance(term, host)'),
    'terminal: fresh mount spawns xterm instance',
  );

  const connectBody = extractFunctionBody(appSrc, 'connectTerminalWs');
  ok(connectBody.includes('ensureLoopbackWsBase()'), 'terminal: connectTerminalWs resolves loopback WS base before opening socket');
  ok(connectBody.includes('new WebSocket(url)'), 'terminal: connectTerminalWs opens workspace terminal WS');
  ok(connectBody.includes('term.xterm.write'), 'terminal: websocket output writes to xterm');

  ok(indexHtml.includes('/vendor/xterm/xterm.js'), 'terminal: index.html loads vendored xterm.js');
  ok(indexHtml.includes('/vendor/xterm/addon-fit.js'), 'terminal: index.html loads vendored addon-fit.js');
  ok(indexHtml.includes('id="terminal-xterm-host"'), 'terminal: index.html defines xterm host element');

  const terminalRs = fs.readFileSync(path.join(ROOT, 'src/workspace/terminal.rs'), 'utf8');
  ok(terminalRs.includes('pub async fn run_terminal_websocket'), 'backend: terminal websocket handler exists');
  ok(terminalRs.includes('spawn_pty_session'), 'backend: terminal spawns interactive PTY session');

  const javaIntegrationSrc = fs.readFileSync(
    path.join(ROOT, 'scripts/test-java-editor-integration.mjs'),
    'utf8',
  );
  ok(
    javaIntegrationSrc.includes('runTerminalEchoIntegration'),
    'integration: java editor harness verifies terminal PTY websocket echo',
  );
}

/** Guards for xterm spawn lifecycle — hidden panel, repo switch, reopen. */
export function testTerminalBuildTaskStreamRegression(appSrc, ok) {
  const activeBody = extractFunctionBody(appSrc, 'isTerminalCommandActive');
  ok(
    activeBody.includes('streamLine != null') && !activeBody.includes('execAbortController'),
    'build task stream: active command detected via streamLine only',
  );

  const restoreBody = extractFunctionBody(appSrc, 'restoreTerminalShellIfIdle');
  ok(restoreBody.includes('term.shellSuspended'), 'build task stream: restore skips while shell is suspended');

  const connectBody = extractFunctionBody(appSrc, 'connectTerminalWs');
  ok(
    connectBody.includes('term.shellSuspended || term.streamLine != null'),
    'build task stream: connectTerminalWs skipped during exec stream',
  );
  ok(
    connectBody.includes('disconnectTerminalWs(term, { silent: true })'),
    'build task stream: stray shell websocket closed on open during exec stream',
  );
  ok(
    connectBody.includes('term.shellSuspended || term.streamLine != null') &&
      connectBody.includes('Terminal shell connection failed'),
    'build task stream: suppress shell error toast during exec stream',
  );
  ok(
    connectBody.indexOf('ensureLoopbackWsBase') < connectBody.lastIndexOf('term.shellSuspended'),
    'build task stream: connectTerminalWs re-checks suspend state after async loopback lookup',
  );
  ok(connectBody.includes('term.ws !== ws'), 'build task stream: ignore stale websocket error handlers');

  const runBody = extractFunctionBody(appSrc, 'runWorkspaceCommandStream');
  ok(runBody.includes('ensureCommandTerminalReady'), 'build task stream: waits for xterm before streaming output');
  const readyPos = runBody.indexOf('ensureCommandTerminalReady');
  const suspendPos = runBody.indexOf('suspendTerminalShell(term)');
  const beginPos = runBody.indexOf('beginTerminalStream');
  ok(readyPos >= 0, 'build task stream: mounts terminal before exec stream');
  ok(suspendPos > readyPos, 'build task stream: suspends shell after terminal is ready');
  ok(beginPos > suspendPos, 'build task stream: begins exec stream after shell suspend');

  const mountBody = extractFunctionBody(appSrc, 'mountActiveTerminal');
  ok(mountBody.includes('sync'), 'build task stream: mountActiveTerminal supports synchronous spawn');
  ok(mountBody.includes('isTerminalCommandActive(term)'), 'build task stream: mountActiveTerminal respects active exec stream');

  const readyBody = extractFunctionBody(appSrc, 'ensureCommandTerminalReady');
  ok(readyBody.includes('mountActiveTerminal({ fresh: needsSpawn, sync: true })'), 'build task stream: command path mounts xterm synchronously');
  ok(readyBody.includes('terminalMountSync'), 'build task stream: command path suppresses competing async terminal mounts');

  const dockBody = extractFunctionBody(appSrc, 'applyTerminalDock');
  ok(dockBody.includes('terminalMountSync'), 'build task stream: applyTerminalDock skips async mount during command setup');

  const initBody = extractFunctionBody(appSrc, 'initTerminalXterm');
  ok(
    initBody.includes('term.shellSuspended') && initBody.includes('connectTerminalWs(term)'),
    'build task stream: initTerminalXterm skips shell connect while suspended',
  );

  const postBody = extractFunctionBody(appSrc, 'postWorkspaceExecStream');
  ok(!postBody.includes('beginTerminalStream'), 'build task stream: postWorkspaceExecStream does not double-begin stream');

  const runBuildBody = extractFunctionBody(appSrc, 'runBuildTask');
  ok(!runBuildBody.includes('showTerminal()'), 'build task stream: runBuildTask relies on runWorkspaceCommandStream to open terminal');

  testTerminalBuildTaskStreamSimulation(ok);
}

function testTerminalBuildTaskStreamSimulation(ok) {
  const term = {
    shellSuspended: false,
    streamLine: null,
    ws: null,
    wsSilentClose: false,
    xterm: null,
    xtermReady: false,
  };

  let connectAttempts = 0;
  let restoreAttempts = 0;
  let errorToasts = 0;

  const isTerminalCommandActive = (t) => t?.streamLine != null;

  const suspendTerminalShell = (t) => {
    t.shellSuspended = true;
  };

  const beginTerminalStream = (t) => {
    suspendTerminalShell(t);
    t.streamLine = '';
  };

  const connectTerminalWs = (t, afterAwait = false) => {
    if (t.shellSuspended || t.streamLine != null) {
      return afterAwait ? 'aborted' : undefined;
    }
    connectAttempts += 1;
  };

  const restoreTerminalShellIfIdle = (t) => {
    if (!t || isTerminalCommandActive(t) || t.shellSuspended) return;
    restoreAttempts += 1;
    connectTerminalWs(t);
  };

  const wsOnError = (t, ws, activeWs) => {
    if (activeWs !== ws || t.wsSilentClose || t.shellSuspended || t.streamLine != null) return;
    errorToasts += 1;
  };

  const ensureReady = () => {
    term.xterm = { element: { isConnected: true } };
    term.xtermReady = true;
  };

  // Mirror runWorkspaceCommandStream ordering.
  ensureReady();
  suspendTerminalShell(term);
  beginTerminalStream(term);
  restoreTerminalShellIfIdle(term);
  ok(connectAttempts === 0, 'build task sim: no connect while suspended');
  ok(connectTerminalWs(term, true) === 'aborted', 'build task sim: async connect aborted after suspend');
  wsOnError(term, {}, term.ws);
  ok(errorToasts === 0, 'build task sim: shell error toast suppressed during stream');

  term.streamLine = null;
  term.shellSuspended = false;
  restoreTerminalShellIfIdle(term);
  ok(restoreAttempts === 1, 'build task sim: shell restore allowed after stream ends');
  ok(connectAttempts === 1, 'build task sim: shell reconnect allowed after stream ends');
}

export function testTerminalLifecycleRegression(appSrc, ok) {
  const resetBody = extractFunctionBody(appSrc, 'resetTerminalCwds');
  ok(resetBody.includes('const mountNow = isTerminalPanelVisible()'), 'lifecycle: resetTerminalCwds checks panel visibility before spawn');
  ok(
    resetBody.includes('if (mountNow && host) spawnTerminalInstance(t, host)'),
    'lifecycle: resetTerminalCwds only spawns xterm when panel is visible',
  );
  ok(
    resetBody.includes('else destroyTerminalInstance(t)'),
    'lifecycle: resetTerminalCwds destroys xterm when panel is hidden',
  );

  const dockBody = extractFunctionBody(appSrc, 'applyTerminalDock');
  ok(dockBody.includes('if (!showTerminal)'), 'lifecycle: applyTerminalDock handles hidden terminal panel');
  ok(
    dockBody.includes('destroyTerminalInstance(t)'),
    'lifecycle: applyTerminalDock destroys xterm instances when panel is hidden',
  );

  const openBody = extractFunctionBody(appSrc, 'openTerminal');
  ok(
    openBody.includes('mountActiveTerminal({ fresh: true })'),
    'lifecycle: openTerminal always respawns xterm after panel is shown',
  );

  const toggleBody = extractFunctionBody(appSrc, 'toggleTerminal');
  ok(
    toggleBody.includes('mountActiveTerminal({ fresh: true })'),
    'lifecycle: toggleTerminal respawns xterm when reopening floating dock',
  );

  const mountBody = extractFunctionBody(appSrc, 'mountActiveTerminal');
  ok(
    mountBody.includes('requestAnimationFrame(() => requestAnimationFrame(spawn))'),
    'lifecycle: mountActiveTerminal waits for layout before spawning xterm',
  );
  ok(mountBody.includes('xtermMissing'), 'lifecycle: mountActiveTerminal respawns missing or detached xterm');
  ok(
    mountBody.includes('requestAnimationFrame(() => {') && mountBody.includes('fitActiveTerminal()'),
    'lifecycle: mountActiveTerminal refits xterm after spawn',
  );

  const connectBody = extractFunctionBody(appSrc, 'connectTerminalWs');
  ok(connectBody.includes('ArrayBuffer.isView'), 'lifecycle: websocket output handles typed arrays');
  ok(connectBody.includes('instanceof Blob'), 'lifecycle: websocket output handles Blob payloads');

  const fitBody = extractFunctionBody(appSrc, 'fitTerminal');
  ok(fitBody.includes('defaultTerminalSize()'), 'lifecycle: fitTerminal uses default PTY size when FitAddon missing');

  testTerminalLifecycleSimulation(ok);
}

function testTerminalLifecycleSimulation(ok) {
  const state = {
    terminalDock: 'bottom',
    terminalOpen: false,
    activePanel: 'explorer',
    terminals: [{ id: 'term-1', xterm: { live: true }, container: { live: true } }],
  };

  const isVisible = (s) => (s.terminalDock === 'left' ? s.activePanel === 'terminal' : s.terminalOpen);

  const destroy = (t) => {
    t.xterm = null;
    t.container = null;
  };

  const spawn = (t) => {
    t.xterm = { live: true };
    t.container = { live: true };
  };

  const resetCwds = (s) => {
    const mountNow = isVisible(s);
    s.terminals.forEach((t) => {
      if (mountNow) spawn(t);
      else destroy(t);
    });
  };

  const hidePanel = (s) => {
    s.terminals.forEach((t) => destroy(t));
  };

  // Repo switch while terminal panel is closed — must not keep a stale hidden xterm.
  resetCwds(state);
  ok(state.terminals[0].xterm === null, 'lifecycle sim: hidden panel + repo reset destroys xterm');

  // User opens terminal — spawn only after visible.
  state.terminalOpen = true;
  resetCwds(state);
  ok(state.terminals[0].xterm?.live === true, 'lifecycle sim: visible panel + repo reset spawns xterm');

  // User closes floating terminal — destroy so reopen does not reuse zero-size xterm.
  state.terminalOpen = false;
  hidePanel(state);
  ok(state.terminals[0].xterm === null, 'lifecycle sim: closing panel destroys xterm');

  // Reopen — fresh spawn path (mirrors openTerminal fresh: true).
  state.terminalOpen = true;
  spawn(state.terminals[0]);
  ok(state.terminals[0].xterm?.live === true, 'lifecycle sim: reopen spawns fresh xterm');
}

export function testTerminalVendorLoadRegression(ok) {
  ok(fs.existsSync(path.join(STATIC, 'vendor/xterm/xterm.js')), 'terminal: vendored xterm.js present');
  ok(fs.existsSync(path.join(STATIC, 'vendor/xterm/addon-fit.js')), 'terminal: vendored addon-fit.js present');

  const sandbox = createXtermSandbox();
  sandbox.globalThis.Terminal = class Terminal {
    constructor() {
      this.onDataHandlers = [];
    }

    onData(handler) {
      this.onDataHandlers.push(handler);
    }

    input(data) {
      for (const handler of this.onDataHandlers) handler(data);
    }
  };
  loadVendoredFitAddon(sandbox);

  const api = evalXtermApi(sandbox);
  ok(typeof api?.Terminal === 'function', 'terminal: xtermApi returns Terminal with vendored FitAddon loaded');
  ok(typeof api?.FitAddon === 'function', 'terminal: xtermApi resolves vendored FitAddon constructor');

  const fitAddon = new api.FitAddon();
  ok(typeof fitAddon.fit === 'function', 'terminal: FitAddon instance supports fit()');

  const term = new api.Terminal();
  term.onData((data) => { term.lastInput = data; });
  term.input('echo hi');
  ok(term.lastInput === 'echo hi', 'terminal: xterm onData receives typed input');

  // Namespace export shape from addon-fit UMD.
  const namespaceSandbox = createXtermSandbox();
  namespaceSandbox.globalThis.Terminal = sandbox.globalThis.Terminal;
  loadVendoredFitAddon(namespaceSandbox);
  ok(
    typeof namespaceSandbox.globalThis.FitAddon?.FitAddon === 'function',
    'terminal: vendored addon-fit exposes FitAddon on module namespace',
  );
  ok(!!evalXtermApi(namespaceSandbox), 'terminal: xtermApi works with namespace FitAddon export');

  // Direct constructor export shape — regression for the build-418 lookup bug.
  const directSandbox = createXtermSandbox();
  directSandbox.globalThis.Terminal = sandbox.globalThis.Terminal;
  directSandbox.globalThis.FitAddon = namespaceSandbox.globalThis.FitAddon?.FitAddon
    ?? namespaceSandbox.globalThis.FitAddon;
  ok(!!evalXtermApi(directSandbox), 'terminal: xtermApi works when FitAddon is exported as constructor');
  ok(evalXtermApiOld(directSandbox) === null, 'terminal: old FitAddon lookup fails on direct constructor export');

  const noFitSandbox = createXtermSandbox();
  noFitSandbox.globalThis.Terminal = sandbox.globalThis.Terminal;
  const noFitApi = evalXtermApi(noFitSandbox);
  ok(!!noFitApi && typeof noFitApi.Terminal === 'function' && noFitApi.FitAddon == null,
    'terminal: xtermApi still loads Terminal when FitAddon script is missing');
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: 'utf8', ...opts });
  if (r.status !== 0) {
    throw new Error(`${cmd} ${args.join(' ')} failed: ${r.stderr || r.stdout}`);
  }
  return r.stdout?.trim() ?? '';
}

function findReaperBinary() {
  const candidates = [
    process.env.REAPER_BIN,
    path.join(ROOT, 'target/release/reaper'),
    path.join(ROOT, 'target/debug/reaper'),
    path.join(ROOT, 'dist/Reaper.app/Contents/MacOS/reaper'),
  ].filter(Boolean);
  for (const p of candidates) {
    try {
      fs.accessSync(p, fs.constants.X_OK);
      return p;
    } catch { /* try next */ }
  }
  return null;
}

function setupTerminalIntegrationRepo(dataDir) {
  const repoName = 'terminal-integration';
  const ws = path.join(dataDir, 'workspaces', repoName);
  const bare = path.join(dataDir, 'repos', `${repoName}.git`);
  const meta = path.join(dataDir, 'metadata', `${repoName}.json`);

  fs.rmSync(ws, { recursive: true, force: true });
  fs.rmSync(bare, { recursive: true, force: true });
  fs.mkdirSync(ws, { recursive: true });
  fs.writeFileSync(path.join(ws, 'README.md'), '# terminal integration\n', 'utf8');

  run('git', ['init', '-b', 'main'], { cwd: ws });
  run('git', ['config', 'user.email', 'reaper@test.local'], { cwd: ws });
  run('git', ['config', 'user.name', 'Reaper Test'], { cwd: ws });
  run('git', ['add', '.'], { cwd: ws });
  run('git', ['commit', '-m', 'init'], { cwd: ws });

  run('git', ['init', '--bare', bare]);
  run('git', ['remote', 'add', 'origin', bare], { cwd: ws });
  run('git', ['push', '-u', 'origin', 'main'], { cwd: ws });

  fs.mkdirSync(path.dirname(meta), { recursive: true });
  fs.writeFileSync(meta, JSON.stringify({
    imported: true,
    local_path: ws,
    remote_url: null,
    remote_host: null,
  }, null, 2));

  return { repoName, ws };
}

async function waitForServerUrl(dataDir, timeoutMs = 20000) {
  const urlFile = path.join(dataDir, 'reaper.url');
  const portFile = path.join(dataDir, 'reaper.port');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(urlFile)) {
      const url = fs.readFileSync(urlFile, 'utf8').trim();
      if (url.startsWith('https://') || url.startsWith('http://')) return url.replace(/\/$/, '');
    }
    if (fs.existsSync(portFile)) {
      const port = Number.parseInt(fs.readFileSync(portFile, 'utf8').trim(), 10);
      if (port > 0) return `https://127.0.0.1:${port}`;
    }
    await sleep(100);
  }
  throw new Error(`reaper.url not written within ${timeoutMs}ms`);
}

async function waitForServer(dataDir, timeoutMs = 45000) {
  const preferred = await waitForServerUrl(dataDir, timeoutMs);
  const candidates = [preferred];
  if (preferred.startsWith('https://')) candidates.push(preferred.replace('https://', 'http://'));
  else if (preferred.startsWith('http://')) candidates.push(preferred.replace('http://', 'https://'));

  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const baseUrl of candidates) {
      try {
        const res = await fetch(`${baseUrl}/api/version`);
        if (res.ok) return { baseUrl: baseUrl.replace(/\/$/, ''), version: await res.json() };
      } catch { /* retry */ }
    }
    await sleep(150);
  }
  throw new Error(`server not ready (tried ${candidates.join(', ')})`);
}

function startReaperServer(binary, dataDir) {
  const proc = spawn(binary, ['--server'], {
    env: {
      ...process.env,
      REAPER_DATA_DIR: dataDir,
      REAPER_STATIC_DIR: path.join(ROOT, 'static'),
      REAPER_HOST: '127.0.0.1',
      REAPER_PORT: '0',
      REAPER_SKIP_EDITOR_TESTS: '1',
      REAPER_SKIP_TLS_TRUST: '1',
      NODE_TLS_REJECT_UNAUTHORIZED: '0',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  proc.stdout?.on('data', () => {});
  proc.stderr?.on('data', () => {});
  return proc;
}

export async function runTerminalEchoIntegration({ baseUrl, repo, ok, fail }) {
  const wsBase = baseUrl.replace(/^http/i, 'ws');
  const url = `${wsBase}/api/repos/${encodeURIComponent(repo)}/workspace/terminal`;
  const marker = 'REAPER_TERM_OK';

  await new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    let output = '';
    let settled = false;

    const finish = (err) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      try { ws.close(); } catch { /* ignore */ }
      if (err) reject(err);
      else resolve();
    };

    const timer = setTimeout(() => {
      finish(new Error(`terminal websocket timed out; output=${JSON.stringify(output.slice(-500))}`));
    }, 20000);

    ws.addEventListener('open', () => {
      setTimeout(() => {
        try {
          ws.send(`echo ${marker}\n`);
        } catch (e) {
          finish(e);
        }
      }, 700);
    });

    ws.addEventListener('message', async (ev) => {
      let chunk;
      if (typeof ev.data === 'string') {
        chunk = ev.data;
      } else if (ev.data instanceof ArrayBuffer) {
        chunk = Buffer.from(ev.data).toString('utf8');
      } else if (ArrayBuffer.isView(ev.data)) {
        chunk = Buffer.from(ev.data.buffer, ev.data.byteOffset, ev.data.byteLength).toString('utf8');
      } else if (typeof Blob !== 'undefined' && ev.data instanceof Blob) {
        chunk = Buffer.from(await ev.data.arrayBuffer()).toString('utf8');
      } else {
        chunk = String(ev.data ?? '');
      }
      output += chunk;
      if (output.includes(marker)) finish(null);
    });

    ws.addEventListener('error', (ev) => {
      finish(ev.error || new Error('terminal websocket error'));
    });

    ws.addEventListener('close', () => {
      if (!settled && output.includes(marker)) finish(null);
    });
  });

  ok(true, 'terminal websocket: shell accepts echo command and returns output');
}

export async function testTerminalBackendIntegration(options = {}) {
  const { quiet = false, ok = (cond, msg) => { if (!cond) throw new Error(msg); }, fail = (msg) => { throw new Error(msg); } } = options;

  if (process.env.REAPER_SKIP_INTEGRATION === '1') {
    if (!quiet) console.log('  skip  REAPER_SKIP_INTEGRATION=1');
    return { skipped: true, passed: true };
  }

  const binary = findReaperBinary();
  if (!binary) {
    if (!quiet) console.log('  skip  reaper binary not built yet');
    return { skipped: true, passed: true };
  }

  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'reaper-terminal-integration-'));
  const { repoName } = setupTerminalIntegrationRepo(dataDir);
  const proc = startReaperServer(binary, dataDir);

  try {
    const { baseUrl, version } = await waitForServer(dataDir);
    ok(!!version?.build, `terminal integration: server up at ${baseUrl}`);

    const openRes = await fetch(`${baseUrl}/api/repos/${encodeURIComponent(repoName)}/workspace/open`, {
      method: 'POST',
    });
    ok(openRes.ok, 'terminal integration: workspace open succeeds');

    await runTerminalEchoIntegration({ baseUrl, repo: repoName, ok, fail });
    return { skipped: false, passed: true };
  } catch (e) {
    fail(e.message || String(e));
    return { skipped: false, passed: false };
  } finally {
    proc.kill('SIGTERM');
    await sleep(200);
    if (!proc.killed) proc.kill('SIGKILL');
    fs.rmSync(dataDir, { recursive: true, force: true });
  }
}
