#!/usr/bin/env bash
# edit → save → javac validation (default 25 cycles; override with REAPER_EDITS).
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
  echo "test-java-save-javac-loop: node not found" >&2
  exit 1
fi

exec "$NODE" "$ROOT/scripts/test-java-save-javac-loop.mjs" "$@"
