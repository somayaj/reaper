#!/usr/bin/env node
/**
 * Terminal PTY integration — spawn Reaper server and verify websocket shell echo.
 *
 * Usage:
 *   bash scripts/test-terminal-integration.sh
 *   REAPER_SKIP_INTEGRATION=1 bash scripts/test-terminal-integration.sh
 */
process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';

import { fileURLToPath } from 'node:url';
import { testTerminalBackendIntegration } from './lib/terminal-harness.mjs';

let exitCode = 0;
function ok(cond, msg) {
  if (cond) {
    console.log(`  ok  ${msg}`);
    return;
  }
  console.error(`FAIL: ${msg}`);
  exitCode = 1;
}
function fail(msg) {
  console.error(`FAIL: ${msg}`);
  exitCode = 1;
}

const result = await testTerminalBackendIntegration({ ok, fail });
if (result.skipped) {
  console.log('  skip  terminal integration skipped');
  process.exit(0);
}

if (exitCode || !result.passed) {
  console.error('\nTerminal integration FAILED');
  process.exit(1);
}
console.log('\nTerminal integration OK');

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(0);
}
