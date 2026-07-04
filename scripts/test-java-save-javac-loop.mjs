#!/usr/bin/env node
/**
 * Validate edit → save (persist) → full javac (default 25 cycles).
 *
 *   bash scripts/test-java-save-javac-loop.sh --backend   # cargo test
 *   REAPER_URL=... REAPER_REPO=... bash scripts/test-java-save-javac-loop.sh --http
 */
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  DEFAULT_JAVA_REL,
  EDIT_COUNT_DEFAULT,
  editLoopJavaContent,
  readDiskFile,
  runEditorParityEditLoop,
} from './lib/javac-edit-loop.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EDIT_COUNT = Number.parseInt(process.env.REAPER_EDITS || String(EDIT_COUNT_DEFAULT), 10);
const DEFAULT_JAVA_FILE = process.env.REAPER_JAVA_FILE || DEFAULT_JAVA_REL;

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

async function runHttpLoop() {
  const baseUrl = (process.env.REAPER_URL || '').replace(/\/$/, '');
  const repo = process.env.REAPER_REPO || '';
  if (!baseUrl || !repo) {
    fail('HTTP mode requires REAPER_URL and REAPER_REPO');
    return;
  }

  console.log(`HTTP javac loop: ${EDIT_COUNT} edits`);
  console.log(`  ${baseUrl} repo=${repo} file=${DEFAULT_JAVA_FILE}`);

  let base = '';
  try {
    base = await readDiskFile(baseUrl, repo, DEFAULT_JAVA_FILE);
  } catch {
    base = editLoopJavaContent(0);
  }
  const useSpringShape = base.includes('SpringApplication.run');

  const failures = await runEditorParityEditLoop({
    baseUrl,
    repo,
    javaFile: DEFAULT_JAVA_FILE,
    editCount: EDIT_COUNT,
    baseContent: useSpringShape ? base : editLoopJavaContent(0),
    useSpringShape,
    simulateTypingDelay: false,
    ok,
    fail,
  });
  if (failures === 0) console.log('  restored original file content via loop harness');
}

function runBackendTests() {
  console.log('Running Rust ten_edit_save_javac_loop tests…');
  const r = spawnSync('cargo', ['test', '-q', 'ten_edit_save_javac_loop', '--', '--nocapture'], {
    cwd: ROOT,
    stdio: 'inherit',
    env: process.env,
  });
  if (r.status !== 0) exitCode = r.status || 1;
  else console.log('Backend javac edit loop OK');
}

async function main() {
  const mode = process.argv.includes('--http') ? 'http'
    : process.argv.includes('--backend') ? 'backend'
      : 'both';

  console.log('Reaper javac edit-save loop test');
  console.log(`Root: ${ROOT}\n`);

  if (mode === 'http' || mode === 'both') {
    if (process.env.REAPER_URL && process.env.REAPER_REPO) {
      console.log('== HTTP (live Reaper) ==');
      await runHttpLoop();
      console.log('');
    } else if (mode === 'http') {
      fail('HTTP mode requires REAPER_URL and REAPER_REPO');
    } else {
      console.log('== HTTP skipped (set REAPER_URL + REAPER_REPO) ==\n');
    }
  }

  if (mode === 'backend' || mode === 'both') {
    console.log('== Backend (cargo test) ==');
    runBackendTests();
  }

  if (exitCode) console.error('\nJavac edit-save loop FAILED');
  else console.log('\nJavac edit-save loop OK');
  process.exit(exitCode);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
