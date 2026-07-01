#!/usr/bin/env bash
# Copy vendored Node.js into a Reaper.app bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: copy-bundled-node.sh /path/to/Reaper.app}"
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
