#!/usr/bin/env bash
# Download Node.js win-x64 for the Windows portable package (Cursor agent bridge).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${NODE_VERSION:-22.13.1}"
DEST="$ROOT/resources/node-windows-x64"
BIN="$DEST/node.exe"
ZIP="node-v${VERSION}-win-x64.zip"
URL="https://nodejs.org/dist/v${VERSION}/${ZIP}"
CACHE="$ROOT/resources/.cache/${ZIP}"

if [[ -f "$BIN" ]]; then
  # node.exe --version needs wine on macOS; just trust the marker file.
  MARKER="$DEST/.node-version"
  if [[ -f "$MARKER" ]] && [[ "$(cat "$MARKER")" == "v${VERSION}" ]]; then
    echo "Node.js ${VERSION} (windows-x64) already present at $BIN"
    exit 0
  fi
fi

mkdir -p "$DEST" "$(dirname "$CACHE")"
echo "Downloading Node.js ${VERSION} (win-x64)…"
curl -fsSL "$URL" -o "$CACHE"
TMP="$(mktemp -d)"
unzip -q "$CACHE" -d "$TMP"
# Official zip extracts to node-vVERSION-win-x64/
SRC_DIR="$TMP/node-v${VERSION}-win-x64"
if [[ ! -f "$SRC_DIR/node.exe" ]]; then
  echo "node.exe missing from $ZIP" >&2
  rm -rf "$TMP"
  exit 1
fi
rsync -a --delete "$SRC_DIR/" "$DEST/"
echo "v${VERSION}" >"$DEST/.node-version"
rm -rf "$TMP"
echo "Saved $BIN"
