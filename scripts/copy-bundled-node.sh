#!/usr/bin/env bash
# Copy vendored Node.js into a Reaper.app bundle.
# REAPER_UNIVERSAL=1 copies both arm64 and x86_64 runtimes for one app on all Macs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: copy-bundled-node.sh /path/to/Reaper.app}"

copy_one() {
  local arch="$1"
  export REAPER_MACOS_ARCH="$arch"
  "$ROOT/scripts/vendor-node-macos.sh"
  local src="$ROOT/resources/node-macos-${arch}/bin/node"
  if [[ ! -f "$src" ]]; then
    echo "Bundled Node.js missing at $src" >&2
    exit 1
  fi
  local dest="$APP/Contents/Resources/node-${arch}/bin/node"
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
  chmod +x "$dest"
  echo "Bundled Node.js for ${arch} → $dest"
}

if [[ "${REAPER_UNIVERSAL:-}" == "1" ]]; then
  copy_one arm64
  copy_one x86_64
else
  ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
  export REAPER_MACOS_ARCH="$ARCH"
  "$ROOT/scripts/vendor-node-macos.sh"
  SRC="$ROOT/resources/node-macos-${ARCH}/bin/node"
  DEST="$APP/Contents/Resources/node/bin/node"
  if [[ ! -f "$SRC" ]]; then
    echo "Bundled Node.js missing at $SRC" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$DEST")"
  cp "$SRC" "$DEST"
  chmod +x "$DEST"
  echo "Bundled Node.js for ${ARCH} → $DEST"
fi
