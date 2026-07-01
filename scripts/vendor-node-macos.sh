#!/usr/bin/env bash
# Download Node.js for bundling in Reaper.app (Cursor agent bridge).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${NODE_VERSION:-22.13.1}"
ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"

case "$ARCH" in
  arm64) NODE_ARCH=darwin-arm64 ;;
  x86_64) NODE_ARCH=darwin-x64 ;;
  *)
    echo "No bundled Node.js for macOS arch: $ARCH" >&2
    exit 1
    ;;
esac

DEST="$ROOT/resources/node-macos-${ARCH}"
BIN="$DEST/bin/node"
TARBALL="node-v${VERSION}-${NODE_ARCH}.tar.xz"
URL="https://nodejs.org/dist/v${VERSION}/${TARBALL}"
CACHE="$ROOT/resources/.cache/${TARBALL}"

if [[ -f "$BIN" ]]; then
  CURRENT="$("$BIN" --version 2>/dev/null || true)"
  if [[ "$CURRENT" == "v${VERSION}" ]]; then
    echo "Node.js ${VERSION} (${ARCH}) already present at $BIN"
    exit 0
  fi
  echo "Replacing Node.js ${CURRENT:-unknown} with ${VERSION} at $BIN"
fi

mkdir -p "$DEST/bin" "$(dirname "$CACHE")"
echo "Downloading Node.js ${VERSION} (${NODE_ARCH})…"
curl -fsSL "$URL" -o "$CACHE"
TMP="$(mktemp -d)"
tar -xJf "$CACHE" -C "$TMP"
cp "$TMP/node-v${VERSION}-${NODE_ARCH}/bin/node" "$BIN"
rm -rf "$TMP"
chmod +x "$BIN"
echo "Saved $BIN"
