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
  cursor-bridge\\       - Cursor agent bridge (optional; needs Node.js)

Setup:
  1. Unzip this folder anywhere (keep files together).
  2. Install WebView2 Runtime if prompted:
     https://go.microsoft.com/fwlink/p/?LinkId=2124703
  3. Double-click reaper.exe

Optional:
  reaper.exe --server   browser-only mode
  Install Node.js from https://nodejs.org for Cursor agent chat.
EOF

if [[ -d "$ROOT/static" ]]; then
  echo "== Staging static UI beside exe =="
  rsync -a --delete \
    --exclude '*.map' \
    "$ROOT/static/" "$STAGE/static/"
fi
if [[ -f "$ROOT/cursor-bridge/server.mjs" ]]; then
  echo "== Staging cursor-bridge beside exe =="
  rsync -a --delete \
    --exclude node_modules \
    --exclude '.bridge-version' \
    "$ROOT/cursor-bridge/" "$STAGE/cursor-bridge/"
fi

echo "== Creating ${STAGE_NAME}.zip =="
(
  cd "$ROOT/dist"
  ditto -c -k --sequesterRsrc --keepParent "$STAGE_NAME" "${STAGE_NAME}.zip"
)

echo ""
echo "Windows portable folder: $STAGE"
echo "Windows zip:             $OUT_ZIP ($(du -h "$OUT_ZIP" | awk '{print $1}'))"
echo "  Unzip on Windows, keep files together, run reaper.exe"
echo "  WebView2 Runtime: https://go.microsoft.com/fwlink/p/?LinkId=2124703"
echo "  Use --server for browser-only mode."
