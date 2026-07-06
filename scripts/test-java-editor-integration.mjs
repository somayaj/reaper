#!/usr/bin/env node
/**
 * UI → backend integration: spawn Reaper server, run edit/save/javac cycles like the editor.
 *
 * Usage:
 *   bash scripts/test-java-editor-integration.sh
 *   REAPER_SKIP_INTEGRATION=1 bash scripts/test-java-editor-integration.sh  # skip
 *
 * Requires a built reaper binary (target/debug, target/release, or dist/Reaper.app).
 */
// Reaper serves a self-signed localhost cert over HTTPS.
process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';

import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  DEFAULT_JAVA_REL,
  EDIT_COUNT_DEFAULT,
  editLoopJavaContent,
  runConcurrentDiagnosticsBurst,
  runConcurrentSaveBurst,
  runConcurrentSaveDiagnosticBurst,
  runEditorParityEditLoop,
  sleep,
} from './lib/javac-edit-loop.mjs';
import {
  runCoalescedClientBurst,
  testCoalescerUnit,
} from './lib/java-coalesce-harness.mjs';
import { runTerminalEchoIntegration } from './lib/terminal-harness.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const REPO_NAME = 'javac-loop-integration';
const EDIT_COUNT = Number.parseInt(process.env.REAPER_EDITS || String(EDIT_COUNT_DEFAULT), 10);

let exitCode = 0;
function fail(msg) {
  console.error(`FAIL: ${msg}`);
  exitCode = 1;
}
function ok(cond, msg) {
  if (cond) {
    console.log(`  ok  ${msg}`);
    return true;
  }
  fail(msg);
  return false;
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

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: 'utf8', ...opts });
  if (r.status !== 0) {
    throw new Error(`${cmd} ${args.join(' ')} failed: ${r.stderr || r.stdout}`);
  }
  return r.stdout?.trim() ?? '';
}

function setupIntegrationRepo(dataDir) {
  const ws = path.join(dataDir, 'workspaces', REPO_NAME);
  const bare = path.join(dataDir, 'repos', `${REPO_NAME}.git`);
  const meta = path.join(dataDir, 'metadata', `${REPO_NAME}.json`);
  const javaAbs = path.join(ws, DEFAULT_JAVA_REL);

  fs.rmSync(ws, { recursive: true, force: true });
  fs.rmSync(bare, { recursive: true, force: true });
  fs.mkdirSync(path.dirname(javaAbs), { recursive: true });
  fs.writeFileSync(javaAbs, editLoopJavaContent(0), 'utf8');

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

  return { ws, javaRel: DEFAULT_JAVA_REL };
}

async function waitForServerUrl(dataDir, timeoutMs = 20000) {
  const urlFile = path.join(dataDir, 'reaper.url');
  const portFile = path.join(dataDir, 'reaper.port');
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (fs.existsSync(urlFile)) {
      const url = fs.readFileSync(urlFile, 'utf8').trim();
      if (url.startsWith('https://')) return url.replace(/\/$/, '');
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
  if (preferred.startsWith('https://')) {
    candidates.push(preferred.replace('https://', 'http://'));
  } else if (preferred.startsWith('http://')) {
    candidates.push(preferred.replace('http://', 'https://'));
  }

  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const baseUrl of candidates) {
      try {
        const res = await fetch(`${baseUrl}/api/version`);
        if (res.ok) {
          return { baseUrl: baseUrl.replace(/\/$/, ''), version: await res.json() };
        }
      } catch { /* retry */ }
    }
    await sleep(150);
  }
  throw new Error(`server not ready (tried ${candidates.join(', ')})`);
}

function startReaperServer(binary, dataDir) {
  const env = {
    ...process.env,
    REAPER_DATA_DIR: dataDir,
    REAPER_STATIC_DIR: path.join(ROOT, 'static'),
    REAPER_HOST: '127.0.0.1',
    REAPER_PORT: '0',
    REAPER_SKIP_EDITOR_TESTS: '1',
    REAPER_SKIP_TLS_TRUST: '1',
    NODE_TLS_REJECT_UNAUTHORIZED: '0',
  };
  const proc = spawn(binary, ['--server'], {
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  proc.stdout?.on('data', () => {});
  proc.stderr?.on('data', () => {});
  return proc;
}

async function runSpawnedIntegration() {
  const binary = findReaperBinary();
  if (!binary) {
    fail('reaper binary not found — run cargo build first');
    return;
  }

  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'reaper-javac-integration-'));
  console.log(`  data dir: ${dataDir}`);
  console.log(`  binary:   ${binary}`);

  const { javaRel } = setupIntegrationRepo(dataDir);
  const proc = startReaperServer(binary, dataDir);

  try {
    const { baseUrl, version } = await waitForServer(dataDir);
    ok(!!version?.build, `server up at ${baseUrl} (UI build ${version?.build})`);

    const failures = await runEditorParityEditLoop({
      baseUrl,
      repo: REPO_NAME,
      javaFile: javaRel,
      editCount: EDIT_COUNT,
      baseContent: editLoopJavaContent(0),
      useSpringShape: false,
      simulateTypingDelay: true,
      ok,
      fail,
    });

    ok(failures === 0, `all ${EDIT_COUNT} edit/save/javac cycles passed`);

    console.log(`\n  coalescer unit (${EDIT_COUNT} queue calls)`);
    const unitFailures = await testCoalescerUnit({ burstCount: EDIT_COUNT, ok, fail });
    ok(unitFailures === 0, `coalescer unit: ${EDIT_COUNT} queue calls coalesced correctly`);

    console.log(`\n  client-parallel burst (${EDIT_COUNT} requests fired at once; server may serialize javac)`);
    const baseContent = editLoopJavaContent(0);
    const burstFailures =
      (await runConcurrentDiagnosticsBurst({
        baseUrl,
        repo: REPO_NAME,
        javaFile: javaRel,
        burstCount: EDIT_COUNT,
        ok,
        fail,
      }))
      + (await runConcurrentSaveBurst({
        baseUrl,
        repo: REPO_NAME,
        javaFile: javaRel,
        burstCount: EDIT_COUNT,
        baseContent,
        ok,
        fail,
      }))
      + (await runConcurrentSaveDiagnosticBurst({
        baseUrl,
        repo: REPO_NAME,
        javaFile: javaRel,
        burstCount: EDIT_COUNT,
        baseContent,
        ok,
        fail,
      }));

    ok(burstFailures === 0, `all ${EDIT_COUNT}-request client-parallel bursts passed`);

    console.log(`\n  coalesced client burst (${EDIT_COUNT} save+queue through editor coalescer)`);
    const coalescedFailures = await runCoalescedClientBurst({
      baseUrl,
      repo: REPO_NAME,
      javaFile: javaRel,
      burstCount: EDIT_COUNT,
      baseContent,
      ok,
      fail,
    });
    ok(coalescedFailures === 0, `coalesced client burst: ${EDIT_COUNT} saves → one-at-a-time javac, latest wins`);

    console.log('\n  terminal websocket echo');
    await runTerminalEchoIntegration({ baseUrl, repo: REPO_NAME, ok, fail });
  } finally {
    proc.kill('SIGTERM');
    await sleep(200);
    if (!proc.killed) proc.kill('SIGKILL');
    fs.rmSync(dataDir, { recursive: true, force: true });
  }
}

async function runSpringIntegrationOptional() {
  const springWs = process.env.REAPER_SPRING_WS
    || '/Users/sunny/reaper/workspaces/Spring-maven-complicated';
  if (!fs.existsSync(springWs)) {
    console.log('  skip  Spring workspace not present (optional)');
    return;
  }

  const binary = findReaperBinary();
  if (!binary) return;

  const repoName = process.env.REAPER_SPRING_REPO || 'Spring-maven-complicated';
  const javaFile = process.env.REAPER_JAVA_FILE
    || 'services/analytics-service/src/main/java/com/enterprise/analytics/AnalyticsServiceApplication.java';
  const metaPath = path.join(
    process.env.REAPER_DATA_DIR || path.join(os.homedir(), 'reaper'),
    'metadata',
    `${repoName}.json`,
  );

  if (!fs.existsSync(metaPath)) {
    console.log(`  skip  Spring metadata missing (${metaPath})`);
    return;
  }

  const dataDir = process.env.REAPER_DATA_DIR || path.join(os.homedir(), 'reaper');
  const proc = startReaperServer(binary, dataDir);
  try {
    const { baseUrl } = await waitForServer(dataDir, 30000);

    const q = new URLSearchParams({ path: javaFile });
    const readRes = await fetch(`${baseUrl}/api/repos/${encodeURIComponent(repoName)}/workspace/file?${q}`);
    if (!readRes.ok) {
      console.log(`  skip  cannot read ${javaFile} from Spring repo`);
      return;
    }
    const body = await readRes.json();
    const baseContent = body.content ?? '';
    if (!baseContent.includes('SpringApplication')) {
      console.log('  skip  Spring file shape unexpected');
      return;
    }

    console.log(`  Spring integration: ${EDIT_COUNT} edits on ${javaFile}`);
    const failures = await runEditorParityEditLoop({
      baseUrl,
      repo: repoName,
      javaFile,
      editCount: EDIT_COUNT,
      baseContent,
      useSpringShape: true,
      simulateTypingDelay: true,
      ok,
      fail,
    });
    ok(failures === 0, `Spring ${EDIT_COUNT} edit/save/javac cycles passed`);
  } finally {
    proc.kill('SIGTERM');
    await sleep(200);
  }
}

export async function testJavaEditorIntegration(options = {}) {
  const { includeSpring = false, quiet = false } = options;
  if (!quiet) {
    console.log('Java editor integration (UI → backend API parity)');
    console.log(`Root: ${ROOT}\n`);
  }

  if (process.env.REAPER_SKIP_INTEGRATION === '1') {
    if (!quiet) console.log('  skip  REAPER_SKIP_INTEGRATION=1');
    return { skipped: true, passed: true };
  }

  exitCode = 0;
  await runSpawnedIntegration();
  if (includeSpring && process.env.REAPER_SPRING_INTEGRATION === '1') {
    await runSpringIntegrationOptional();
  }

  return { skipped: false, passed: exitCode === 0 };
}

async function main() {
  await testJavaEditorIntegration({ includeSpring: true });
  if (exitCode) {
    console.error('\nJava editor integration FAILED');
    process.exit(exitCode);
  }
  console.log('\nJava editor integration OK');
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    console.error(e);
    process.exit(1);
  });
}
