#!/usr/bin/env bash
# Spawn Reaper server and verify interactive terminal PTY over websocket.
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
  echo "test-terminal-integration: node not found (vendor with scripts/vendor-node-macos.sh or install node)" >&2
  exit 1
fi

export NODE_TLS_REJECT_UNAUTHORIZED=0

echo "Running terminal integration (node: $NODE)…"
exec "$NODE" "$ROOT/scripts/test-terminal-integration.mjs"
