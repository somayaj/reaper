#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH="$(uname -m)"
NODE=""

for candidate in \
  "$ROOT/resources/node-macos-${ARCH}/bin/node" \
  "$ROOT/resources/node-macos-arm64/bin/node" \
  "$ROOT/resources/node-macos-x64/bin/node" \
  "$(command -v node 2>/dev/null || true)"; do
  if [[ -n "$candidate" && -x "$candidate" ]]; then
    NODE="$candidate"
    break
  fi
done

if [[ -z "$NODE" ]]; then
  echo "test-gui-window-integration: node not found" >&2
  exit 1
fi

echo "Running GUI window integration (node: $NODE)…"
exec "$NODE" "$ROOT/scripts/test-gui-window-integration.mjs"
