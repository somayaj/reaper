/**
 * Startup / new-window / import open-path regression (UI 504+).
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
 * @param {string} guiRs
 * @param {string} apiRs
 * @param {string} cursorRs
 * @param {string} stateRs
 * @param {string} cursorModRs
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testStartupOpenStaticRegression(
  appSrc,
  guiRs,
  apiRs,
  cursorRs,
  stateRs,
  cursorModRs,
  ok,
) {
  ok(
    guiRs.includes('PageLoadEvent::Started') && !guiRs.includes('PageLoadEvent::Finished'),
    'gui: native window shows on page Started (not after Monaco Finished)',
  );
  ok(
    guiRs.includes('Show as soon as HTML/splash is parsing'),
    'gui: documents early window show rationale',
  );

  ok(appSrc.includes('function shouldEagerWarmCursorSession'), 'app: shouldEagerWarmCursorSession defined');
  const eagerBody = extractFunctionBody(appSrc, 'shouldEagerWarmCursorSession');
  ok(
    eagerBody.includes('__reaperWindowLabel') && eagerBody.includes('label === 1'),
    'app: only window label 1 eagerly warms Cursor',
  );

  const warmBody = extractFunctionBody(appSrc, 'warmCursorSession');
  ok(
    /function warmCursorSession\([^)]*force\s*=/.test(appSrc),
    'app: warmCursorSession accepts force option',
  );
  ok(
    warmBody.includes('shouldEagerWarmCursorSession'),
    'app: warmCursorSession skips eager warm on secondary windows',
  );

  const hydrateBody = extractFunctionBody(appSrc, 'hydrateRepoWorkspace');
  ok(
    hydrateBody.includes('void openInitialWorkspaceFile()'),
    'app: initial file open is async (tree can show first)',
  );
  ok(
    !hydrateBody.includes('await openInitialWorkspaceFile()'),
    'app: hydrate does not block on README/pom open',
  );
  ok(
    hydrateBody.includes('shouldEagerWarmCursorSession()') && hydrateBody.includes('void warmCursorSession'),
    'app: repo hydrate warms Cursor only on primary window',
  );

  const initMatch = appSrc.match(/async function init\(\)\s*\{[\s\S]*?\n\}/);
  const initBody = initMatch?.[0] || '';
  ok(initBody.includes('void ensureGeminiReady()'), 'app: init does not await Gemini (non-blocking startup)');
  ok(!initBody.includes('await ensureGeminiReady()'), 'app: Gemini settings load in background during init');

  ok(
    apiRs.includes('spawn_blocking') && apiRs.includes('import_local_repo'),
    'api: local import runs on blocking thread pool',
  );
  ok(
    apiRs.includes('spawn_blocking') && apiRs.includes('import_repo(&config'),
    'api: remote import runs on blocking thread pool',
  );

  ok(
    cursorRs.includes('"reused": true') && cursorRs.includes('cursor_sessions.get(&name)'),
    'cursor: warm returns immediately when session already cached',
  );
  ok(
    cursorRs.includes('cursor_session_create_locks.guard(name)'),
    'cursor: Agent.create serialized per repo under concurrent warm',
  );
  ok(stateRs.includes('cursor_session_create_locks'), 'state: session create locks wired on AppState');
  ok(cursorModRs.includes('struct SessionCreateLocks'), 'cursor: SessionCreateLocks defined');
}

/**
 * VM unit test for shouldEagerWarmCursorSession semantics.
 * @param {(pass: boolean, label: string) => void} ok
 */
export function testShouldEagerWarmCursorSessionUnit(ok) {
  const appPath = path.join(ROOT, 'static/app.js');
  const eagerSrc = extractFunctionBody(fs.readFileSync(appPath, 'utf8'), 'shouldEagerWarmCursorSession');
  ok(!!eagerSrc, 'shouldEagerWarmCursorSession body extracted');

  const sandbox = { console };
  sandbox.globalThis = sandbox;
  sandbox.window = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(
    `function shouldEagerWarmCursorSession() {
${eagerSrc}
    }`,
    sandbox,
  );

  delete sandbox.window.__reaperWindowLabel;
  ok(sandbox.shouldEagerWarmCursorSession() === true, 'eager warm: undefined window label (browser) → true');
  sandbox.window.__reaperWindowLabel = 1;
  ok(sandbox.shouldEagerWarmCursorSession() === true, 'eager warm: primary window label 1 → true');
  sandbox.window.__reaperWindowLabel = 2;
  ok(sandbox.shouldEagerWarmCursorSession() === false, 'eager warm: secondary window label 2 → false');
}
