/**
 * Cursor SDK (ESM build) does `import * as u from "node:child_process"`.
 * Patching only the CJS default export does NOT affect those live bindings.
 * After mutating the CJS exports, call syncBuiltinESMExports() so ESM `spawn`
 * sees the patched function too.
 *
 * Also:
 * - force windowsHide: true
 * - force detached: false (SDK sets detached:true → new console on Windows)
 * - inject powershell -WindowStyle Hidden
 */
import childProcess from "node:child_process";
import module from "node:module";
import path from "node:path";

if (process.platform === "win32") {
  const log = (...args) => {
    try {
      console.error("[windows-hide-patch]", ...args);
    } catch {
      /* ignore */
    }
  };

  function isPowerShell(command) {
    if (command == null) return false;
    const base = path.basename(String(command)).toLowerCase();
    return (
      base === "powershell.exe" ||
      base === "powershell" ||
      base === "pwsh.exe" ||
      base === "pwsh"
    );
  }

  function withPowerShellHiddenArgs(command, args) {
    if (!isPowerShell(command)) return args;
    const list = Array.isArray(args) ? [...args] : [];
    const lower = list.map((a) => String(a).toLowerCase());
    if (lower.includes("-windowstyle")) return list;
    return ["-NoProfile", "-WindowStyle", "Hidden", ...list];
  }

  function withHideOptions(options) {
    if (options == null || typeof options !== "object") {
      return { windowsHide: true, detached: false };
    }
    return {
      ...options,
      windowsHide: true,
      detached: false,
    };
  }

  const patch = (name) => {
    const original = childProcess[name];
    if (typeof original !== "function") return;

    childProcess[name] = function patched(command, args, options) {
      if (args != null && !Array.isArray(args) && typeof args === "object") {
        return original.call(this, command, withHideOptions(args));
      }
      return original.call(
        this,
        command,
        withPowerShellHiddenArgs(command, args),
        withHideOptions(options),
      );
    };

    Object.defineProperty(childProcess[name], "name", {
      value: `windowsHide_${name}`,
    });
  };

  for (const name of [
    "spawn",
    "spawnSync",
    "exec",
    "execSync",
    "execFile",
    "execFileSync",
  ]) {
    patch(name);
  }

  // Critical: sync ESM namespace bindings used by @cursor/sdk's ESM bundle.
  try {
    module.syncBuiltinESMExports();
  } catch (e) {
    log("syncBuiltinESMExports failed:", e?.message || e);
  }

  log(
    "active — CJS+ESM child_process patched (windowsHide, no detached, PS Hidden)",
  );
}
