#!/usr/bin/env bash
# Cross-compile a Windows x86_64 reaper.exe from macOS (server / browser mode).
#
# Requires: rustup target x86_64-pc-windows-gnu, mingw-w64 (brew install mingw-w64)
#
# Usage:
#   ./scripts/build-windows-exe.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
TARGET="x86_64-pc-windows-gnu"
OUT_EXE="$ROOT/dist/reaper-${VERSION}-windows-x64.exe"
LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}"

if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required" >&2
  exit 1
fi

if ! command -v "$LINKER" >/dev/null 2>&1; then
  echo "Windows cross linker not found: $LINKER" >&2
  echo "Install with: brew install mingw-w64" >&2
  exit 1
fi

echo "== Ensuring Rust target ${TARGET} =="
rustup target add "$TARGET"

mkdir -p "$ROOT/dist"

echo "== Cross-compiling reaper (${TARGET}, release) =="
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$LINKER"
# Prefer the repo-local target dir for predictable dist/ copy (ignore Cursor sandbox CARGO_TARGET_DIR).
unset CARGO_TARGET_DIR
# GUI crates are macOS-only; this binary serves the IDE in --server / browser mode.
(
  cd "$ROOT"
  REAPER_SKIP_EDITOR_TESTS=1 cargo build --release --target "$TARGET"
)

TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
SRC="$TARGET_ROOT/${TARGET}/release/reaper.exe"
if [[ ! -f "$SRC" ]]; then
  # Fallback: ask cargo where the artifact landed (handles external CARGO_TARGET_DIR).
  SRC="$(
    cd "$ROOT"
    REAPER_SKIP_EDITOR_TESTS=1 cargo build --release --target "$TARGET" --message-format=json \
      | sed -n 's/.*"executable":"\([^"]*reaper\.exe\)".*/\1/p' \
      | tail -1 \
      | sed 's#\\\\#/#g'
  )"
fi
if [[ ! -f "$SRC" ]]; then
  echo "Built binary not found under ${TARGET}/release/reaper.exe" >&2
  exit 1
fi

cp "$SRC" "$OUT_EXE"
file "$OUT_EXE" || true

# WebView2Loader.dll is required next to reaper.exe for the native window.
WEBVIEW2_DLL="$ROOT/resources/webview2-win-x64/WebView2Loader.dll"
if [[ ! -f "$WEBVIEW2_DLL" ]]; then
  echo "== Fetching WebView2Loader.dll =="
  CACHE="$ROOT/resources/.cache"
  mkdir -p "$CACHE" "$ROOT/resources/webview2-win-x64"
  NUPKG="$CACHE/Microsoft.Web.WebView2.nupkg"
  if [[ ! -f "$NUPKG" ]]; then
    curl -fsSL -o "$NUPKG" "https://www.nuget.org/api/v2/package/Microsoft.Web.WebView2/1.0.2903.40"
  fi
  unzip -o -j "$NUPKG" "runtimes/win-x64/native/WebView2Loader.dll" -d "$ROOT/resources/webview2-win-x64"
fi

# Portable zip: exe + WebView2Loader + static UI + cursor-bridge
STAGE_NAME="reaper-${VERSION}-windows-x64"
STAGE="$ROOT/dist/${STAGE_NAME}"
OUT_ZIP="$ROOT/dist/${STAGE_NAME}.zip"
rm -rf "$STAGE" "$OUT_ZIP"
mkdir -p "$STAGE"
cp "$OUT_EXE" "$STAGE/reaper.exe"
cp "$WEBVIEW2_DLL" "$STAGE/WebView2Loader.dll"
cat >"$STAGE/README.txt" <<EOF
Reaper ${VERSION} for Windows x64 (portable)

Contents:
  reaper.exe           - native WebView2 app (default)
  WebView2Loader.dll   - required next to reaper.exe
  static\\              - IDE UI (required)
  node\\                 - bundled Node.js (Cursor agent bridge)
  cursor-bridge\\       - Cursor agent bridge (+ node_modules; no npm at runtime)

Setup:
  1. Unzip this folder anywhere (keep ALL files together).
  2. Install WebView2 Runtime (needed for the native window):
     https://go.microsoft.com/fwlink/p/?LinkId=2124703
  3. Double-click reaper.exe

If the native window fails to open:
  - Install WebView2 Runtime (link above), then try again
  - Or run:  reaper.exe --server
    then open the https://127.0.0.1:… URL printed in the console
EOF

if [[ -d "$ROOT/static" ]]; then
  echo "== Staging static UI beside exe =="
  rsync -a --delete \
    --exclude '*.map' \
    "$ROOT/static/" "$STAGE/static/"
fi

echo "== Vendoring Node.js (windows-x64) =="
"$ROOT/scripts/vendor-node-windows.sh"
if [[ -f "$ROOT/resources/node-windows-x64/node.exe" ]]; then
  echo "== Staging bundled Node.js (node.exe only) =="
  mkdir -p "$STAGE/node"
  # Official win-x64 node.exe is self-contained; skip npm/node_modules to keep the zip small.
  cp "$ROOT/resources/node-windows-x64/node.exe" "$STAGE/node/node.exe"
fi

echo "== Vendoring java-debug (windows-x64) =="
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoProfile -File "$ROOT/scripts/vendor-debug-adapters-windows.ps1"
  pwsh -NoProfile -File "$ROOT/scripts/vendor-jdtls-windows.ps1"
  pwsh -NoProfile -File "$ROOT/scripts/vendor-jdk-windows.ps1"
elif command -v powershell >/dev/null 2>&1; then
  powershell -NoProfile -ExecutionPolicy Bypass -File "$ROOT/scripts/vendor-debug-adapters-windows.ps1"
  powershell -NoProfile -ExecutionPolicy Bypass -File "$ROOT/scripts/vendor-jdtls-windows.ps1"
  powershell -NoProfile -ExecutionPolicy Bypass -File "$ROOT/scripts/vendor-jdk-windows.ps1"
else
  echo "Warning: PowerShell not found — skipping java-debug/jdtls/jdk vendor" >&2
fi
JAVA_DEBUG_SRC="$ROOT/resources/debug-adapters-windows-x64/java-debug"
if [[ -d "$JAVA_DEBUG_SRC/server" ]]; then
  echo "== Staging bundled java-debug plugin =="
  mkdir -p "$STAGE/debug-adapters"
  rsync -a --delete "$JAVA_DEBUG_SRC/" "$STAGE/debug-adapters/java-debug/"
fi
JDTLS_SRC="$ROOT/resources/jdtls"
if [[ -d "$JDTLS_SRC/plugins" ]]; then
  echo "== Staging bundled jdtls =="
  rsync -a --delete "$JDTLS_SRC/" "$STAGE/jdtls/"
fi
JDK21_SRC="$ROOT/resources/jdk-windows-x64"
if [[ -x "$JDK21_SRC/bin/java.exe" || -f "$JDK21_SRC/bin/java.exe" ]]; then
  echo "== Staging bundled JDK 21 for jdtls =="
  rsync -a --delete "$JDK21_SRC/" "$STAGE/jdk-21/"
fi

if [[ -f "$ROOT/cursor-bridge/server.mjs" ]]; then
  echo "== Staging cursor-bridge (with node_modules; offline-ready) =="
  # Same as macOS: ship deps in the package. Runtime install hits npm/Cloudflare
  # (e.g. 104.16.x.x:443) and times out in locked-down / UTM VMs.
  if [[ ! -f "$ROOT/cursor-bridge/node_modules/@connectrpc/connect/package.json" ]]; then
    echo "Installing cursor-bridge dependencies for Windows package…"
    BUILD_NODE=""
    if [[ -x "$ROOT/resources/node-arm64/bin/node" ]]; then
      BUILD_NODE="$ROOT/resources/node-arm64/bin/node"
    elif [[ -x "$ROOT/resources/node-x64/bin/node" ]]; then
      BUILD_NODE="$ROOT/resources/node-x64/bin/node"
    elif command -v node >/dev/null 2>&1; then
      BUILD_NODE="$(command -v node)"
    fi
    if [[ -n "$BUILD_NODE" && -f "$ROOT/cursor-bridge/install-deps.mjs" ]]; then
      (cd "$ROOT/cursor-bridge" && "$BUILD_NODE" install-deps.mjs)
    elif command -v npm >/dev/null 2>&1; then
      (cd "$ROOT/cursor-bridge" && npm install --omit=dev)
    else
      echo "Warning: cursor-bridge node_modules missing and no node/npm to install" >&2
    fi
  fi
  if [[ ! -f "$ROOT/cursor-bridge/node_modules/@cursor/sdk/package.json" ]]; then
    echo "error: cursor-bridge node_modules incomplete; cannot build offline Windows package" >&2
    exit 1
  fi
  rsync -a --delete \
    --exclude '.bridge-version' \
    "$ROOT/cursor-bridge/" "$STAGE/cursor-bridge/"
fi

echo "== Creating ${STAGE_NAME}.zip =="
rm -f "$OUT_ZIP"
(
  cd "$ROOT/dist"
  # Avoid AppleDouble / __MACOSX junk in the archive
  COPYFILE_DISABLE=1 zip -r -X -q "${STAGE_NAME}.zip" "$STAGE_NAME" \
    -x "*.DS_Store" -x "*__MACOSX*"
)

echo ""
echo "Windows portable folder: $STAGE"
echo "Windows zip:             $OUT_ZIP ($(du -h "$OUT_ZIP" | awk '{print $1}'))"
echo "  Unzip on Windows, keep files together, run reaper.exe"
echo "  WebView2 Runtime: https://go.microsoft.com/fwlink/p/?LinkId=2124703"
echo "  Use --server for browser-only mode."
