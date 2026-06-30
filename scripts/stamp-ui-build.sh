#!/usr/bin/env bash
# Stamp reaper-ui-build meta and cache-bust query params from static/BUILD.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$(tr -d '[:space:]' < "$ROOT/static/BUILD")"
TARGET="${1:-$ROOT/static/index.html}"

if [[ ! -f "$TARGET" ]]; then
  echo "stamp-ui-build: file not found: $TARGET" >&2
  exit 1
fi

sed -i '' "s/name=\"reaper-ui-build\" content=\"[^\"]*\"/name=\"reaper-ui-build\" content=\"$BUILD\"/" "$TARGET"
sed -i '' "s|/reaper-ui.css?v=[0-9]*|/reaper-ui.css?v=$BUILD|g" "$TARGET"
sed -i '' "s|/reaper-lang-core.js?v=[0-9]*|/reaper-lang-core.js?v=$BUILD|g" "$TARGET"
sed -i '' "s|/monaco-languages.js?v=[0-9]*|/monaco-languages.js?v=$BUILD|g" "$TARGET"
sed -i '' "s|/app.js?v=[0-9]*|/app.js?v=$BUILD|g" "$TARGET"

echo "Stamped UI build $BUILD in $TARGET"
