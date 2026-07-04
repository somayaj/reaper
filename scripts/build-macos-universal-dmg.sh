#!/usr/bin/env bash
# Build a universal Reaper.app (arm64 + x86_64) and wrap it in a .dmg (~590 MB).
# Prefer split per-arch DMGs for releases: scripts/build-macos-split-dmgs.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export REAPER_UNIVERSAL=1
exec "$ROOT/scripts/build-macos-dmg.sh"
