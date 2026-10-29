#!/usr/bin/env node
/**
 * Standalone GUI window integration suite (Rust IPC + static/JS checks).
 */
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { testGuiWindowJsIntegration, testGuiWindowStaticRegression } from './lib/gui-window-harness.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const STATIC = path.join(ROOT, 'static');

let failed = 0;
function ok(pass, label) {
  if (!pass) {
    failed += 1;
    console.error(`FAIL: ${label}`);
  }
}

const cargo = spawnSync('cargo', ['test', '--quiet', 'gui_ipc::'], {
  cwd: ROOT,
  stdio: 'inherit',
});
if (cargo.status !== 0) {
  process.exit(cargo.status ?? 1);
}

const appSrc = fs.readFileSync(path.join(STATIC, 'app.js'), 'utf8');
const indexHtml = fs.readFileSync(path.join(STATIC, 'index.html'), 'utf8');
const guiRs = fs.readFileSync(path.join(ROOT, 'src/gui.rs'), 'utf8');
const guiIpcRs = fs.readFileSync(path.join(ROOT, 'src/gui_ipc.rs'), 'utf8');

testGuiWindowStaticRegression(appSrc, indexHtml, guiRs, guiIpcRs, ok);
testGuiWindowJsIntegration(ok);

if (failed) {
  process.exit(1);
}
console.log('GUI window integration: ok');
