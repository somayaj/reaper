/**
 * Cursor SDK (ESM build) does `import * as u from "node:child_process"`.
 * Patching only the CJS default export does NOT affect those live bindings.
 * After mutating the CJS exports, call syncBuiltinESMExports() so ESM `spawn`
 * sees the patched function too.
 *
 * Only force hide options for PowerShell — applying detached:false / windowsHide
 * to every SDK child previously hung agent chat on Windows.
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

  function withPowerShellHideOptions(command, options) {
    if (!isPowerShell(command)) return options;
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
      // spawn(command, options) overload
      if (args != null && !Array.isArray(args) && typeof args === "object") {
        return original.call(this, command, withPowerShellHideOptions(command, args));
      }
      return original.call(
        this,
        command,
        withPowerShellHiddenArgs(command, args),
        withPowerShellHideOptions(command, options),
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

  try {
    module.syncBuiltinESMExports();
  } catch (e) {
    log("syncBuiltinESMExports failed:", e?.message || e);
  }

  log("active — PowerShell-only hide (windowsHide, no detached)");
}
