#!/usr/bin/env bash
# Vendor debug adapters for every macOS distribution Reaper ships (arm64 + x86_64).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

for arch in arm64 x86_64; do
  echo "== Debug adapters: ${arch} =="
  REAPER_MACOS_ARCH="$arch" "$ROOT/scripts/vendor-debug-adapters-macos.sh"
done

echo "All macOS debug adapter distributions ready under $ROOT/resources/debug-adapters-macos-{arm64,x86_64}"
