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

# Portable layout: exe + cursor-bridge next to it (so the VM can find the bridge).
STAGE="$ROOT/dist/reaper-${VERSION}-windows-x64"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "$OUT_EXE" "$STAGE/reaper.exe"
if [[ -f "$ROOT/cursor-bridge/server.mjs" ]]; then
  echo "== Staging cursor-bridge beside exe =="
  rsync -a --delete \
    --exclude node_modules \
    --exclude '.bridge-version' \
    "$ROOT/cursor-bridge/" "$STAGE/cursor-bridge/"
fi

echo ""
echo "Windows exe: $OUT_EXE ($(du -h "$OUT_EXE" | awk '{print $1}'))"
echo "Windows folder (copy this whole folder to the VM): $STAGE"
echo "  Inside the VM: run reaper.exe — console prints https://127.0.0.1:<port>"
echo "  Cursor agent needs Node.js installed on Windows (nodejs.org)."
echo "Note: native Windows GUI is not in this build (browser UI only)."
