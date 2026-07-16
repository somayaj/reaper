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
# GUI crates are macOS-only; this binary serves the IDE in --server / browser mode.
(
  cd "$ROOT"
  REAPER_SKIP_EDITOR_TESTS=1 cargo build --release --target "$TARGET"
)

SRC="$ROOT/target/${TARGET}/release/reaper.exe"
if [[ ! -f "$SRC" ]]; then
  echo "Built binary not found: $SRC" >&2
  exit 1
fi

cp "$SRC" "$OUT_EXE"
echo ""
echo "Windows exe: $OUT_EXE ($(du -h "$OUT_EXE" | awk '{print $1}'))"
echo "Note: runs as a local server (open the printed URL in a browser). Native Windows GUI is not in this build."
