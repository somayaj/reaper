#!/usr/bin/env bash
# Bundle monaco-editor into static/vendor for same-origin workers (required in WKWebView).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/static/vendor/monaco-editor/min/vs/loader.js"

if [[ -f "$DEST" ]]; then
  echo "monaco-editor already vendored at static/vendor/monaco-editor"
  exit 0
fi

echo "Downloading monaco-editor@0.52.2…"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$ROOT/static/vendor/monaco-editor"
curl -fsSL "https://registry.npmjs.org/monaco-editor/-/monaco-editor-0.52.2.tgz" \
  | tar -xz -C "$TMP"
mv "$TMP/package/min" "$ROOT/static/vendor/monaco-editor/min"

echo "Vendored monaco-editor to static/vendor/monaco-editor/min"
