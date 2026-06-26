#!/usr/bin/env bash
# Build Reaper.icns from static/logo-icon.svg (macOS only).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SVG="$ROOT/static/logo-icon.svg"
ICONSET="$ROOT/packaging/macos/Reaper.iconset"
ICNS="$ROOT/packaging/macos/Reaper.icns"
TMPDIR="${TMPDIR:-/tmp}/reaper-icon-$$"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "generate-macos-icon.sh requires macOS (iconutil)" >&2
  exit 1
fi

if [[ ! -f "$SVG" ]]; then
  echo "Missing $SVG" >&2
  exit 1
fi

mkdir -p "$TMPDIR"
MASTER="$TMPDIR/master.png"

echo "Rendering logo-icon.svg..."
qlmanage -t -s 1024 -o "$TMPDIR" "$SVG" >/dev/null 2>&1
mv "$TMPDIR/$(basename "$SVG").png" "$MASTER"

echo "Building icon set..."
rm -rf "$ICONSET"
mkdir -p "$ICONSET"

declare -a SIZES=(
  "16:16x16"
  "32:16x16@2x"
  "32:32x32"
  "64:32x32@2x"
  "128:128x128"
  "256:128x128@2x"
  "256:256x256"
  "512:256x256@2x"
  "512:512x512"
  "1024:512x512@2x"
)

for entry in "${SIZES[@]}"; do
  size="${entry%%:*}"
  name="${entry##*:}"
  sips -z "$size" "$size" "$MASTER" --out "$ICONSET/icon_${name}.png" >/dev/null
done

echo "Writing $ICNS..."
iconutil -c icns "$ICONSET" -o "$ICNS"
rm -rf "$ICONSET" "$TMPDIR"
echo "Done: $ICNS"
