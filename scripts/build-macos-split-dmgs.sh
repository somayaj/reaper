#!/usr/bin/env bash
# Build separate arm64 and x86_64 DMGs (split release — ~half the size per download).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"

if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

echo "== Apple Silicon (arm64) =="
"$ROOT/scripts/build-macos-dmg.sh"

echo ""
echo "== Intel (x86_64) =="
"$ROOT/scripts/build-macos-intel-dmg.sh"

echo ""
echo "Split release DMGs:"
ls -lh \
  "$ROOT/dist/reaper-${VERSION}-macos-arm64.dmg" \
  "$ROOT/dist/reaper-${VERSION}-macos-x86_64.dmg"

ARM_APP="$ROOT/dist/Reaper.app"
INTEL_APP="$ROOT/dist/Reaper-intel.app"
if [[ -d "$ARM_APP" && -d "$INTEL_APP" ]]; then
  echo ""
  echo "App bundle sizes:"
  du -sh "$ARM_APP" "$INTEL_APP"
fi
