#!/usr/bin/env bash
# Copy vendored debug adapters into a Reaper.app bundle.
# REAPER_UNIVERSAL=1 copies both arm64 and x86_64 trees.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: copy-bundled-debug-adapters.sh /path/to/Reaper.app}"

copy_one() {
  local arch="$1"
  export REAPER_MACOS_ARCH="$arch"
  "$ROOT/scripts/vendor-debug-adapters-macos.sh"
  local src="$ROOT/resources/debug-adapters-macos-${arch}"
  if [[ ! -d "$src/js-debug" ]]; then
    echo "Bundled debug adapters missing at $src" >&2
    exit 1
  fi
  local dest="$APP/Contents/Resources/debug-adapters-${arch}"
  rm -rf "$dest"
  mkdir -p "$dest"
  cp -R "$src/." "$dest/"
  if [[ -x "$dest/delve/bin/dlv" ]]; then chmod +x "$dest/delve/bin/dlv"; fi
  if [[ -x "$dest/codelldb/adapter/codelldb" ]]; then chmod +x "$dest/codelldb/adapter/codelldb"; fi
  echo "Bundled debug adapters for ${arch} → $dest"
}

if [[ "${REAPER_UNIVERSAL:-}" == "1" ]]; then
  copy_one arm64
  copy_one x86_64
else
  ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
  export REAPER_MACOS_ARCH="$ARCH"
  "$ROOT/scripts/vendor-debug-adapters-macos.sh"
  SRC="$ROOT/resources/debug-adapters-macos-${ARCH}"
  DEST="$APP/Contents/Resources/debug-adapters"
  if [[ ! -d "$SRC/js-debug" ]]; then
    echo "Bundled debug adapters missing at $SRC" >&2
    exit 1
  fi
  rm -rf "$DEST"
  mkdir -p "$DEST"
  cp -R "$SRC/." "$DEST/"
  if [[ -x "$DEST/delve/bin/dlv" ]]; then chmod +x "$DEST/delve/bin/dlv"; fi
  if [[ -x "$DEST/codelldb/adapter/codelldb" ]]; then chmod +x "$DEST/codelldb/adapter/codelldb"; fi
  echo "Bundled debug adapters for ${ARCH} → $DEST"
fi
