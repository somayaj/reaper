/**
 * Patch @cursor/sdk shipped JS so local Shell does not open visible consoles on Windows.
 *
 * Root cause: the SDK spawns with `detached: true`, which allocates a new console.
 * Monkey-patching node:child_process + syncBuiltinESMExports hangs agent chat streams,
 * so we rewrite the SDK bundles on disk before they load instead.
 *
 * Idempotent. Safe to import at the top of server.mjs before `@cursor/sdk`.
 */
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const MARK = "/*reaper-win-hide*/";
const FROM = "detached:!0";
const TO = `${MARK}detached:!1,windowsHide:!0`;

function log(...args) {
  try {
    console.error("[patch-sdk-windows-hide]", ...args);
  } catch {
    /* ignore */
  }
}

function patchFile(file) {
  if (!fs.existsSync(file)) return false;
  const original = fs.readFileSync(file, "utf8");
  if (original.includes(MARK) || original.includes("detached:!1,windowsHide:!0")) {
    return false;
  }
  if (!original.includes(FROM)) return false;
  const next = original.split(FROM).join(TO);
  if (next === original) return false;
  fs.writeFileSync(file, next);
  log("patched", file);
  return true;
}

function resolveSdkDist() {
  try {
    const require = createRequire(import.meta.url);
    const pkg = require.resolve("@cursor/sdk/package.json");
    return path.join(path.dirname(pkg), "dist");
  } catch {
    const here = path.dirname(fileURLToPath(import.meta.url));
    return path.join(here, "node_modules", "@cursor", "sdk", "dist");
  }
}

if (process.platform === "win32") {
  const dist = resolveSdkDist();
  const targets = [
    path.join(dist, "esm", "357.js"),
    path.join(dist, "cjs", "174.js"),
  ];
  let n = 0;
  for (const file of targets) {
    if (patchFile(file)) n += 1;
  }
  if (n > 0) {
    log(`applied ${n} file patch(es) — Shell uses windowsHide, no detached`);
  } else {
    log("SDK already patched or targets missing");
  }
}
