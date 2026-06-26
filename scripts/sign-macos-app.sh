#!/usr/bin/env bash
# Sign Reaper.app for macOS distribution.
# Default: ad-hoc sign (works locally; downloaded builds need Gatekeeper bypass).
# Set REAPER_SIGN_IDENTITY to a Developer ID Application cert for release builds.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/dist/Reaper.app}"
IDENTITY="${REAPER_SIGN_IDENTITY:--}"
ENTITLEMENTS="$ROOT/packaging/macos/entitlements.plist"

if [[ ! -d "$APP" ]]; then
  echo "App bundle not found: $APP" >&2
  exit 1
fi

sign_args=(--force --sign "$IDENTITY")
if [[ "$IDENTITY" != "-" ]]; then
  sign_args+=(--options runtime --timestamp)
  if [[ -f "$ENTITLEMENTS" ]]; then
    sign_args+=(--entitlements "$ENTITLEMENTS")
  fi
fi

xattr -cr "$APP"

echo "  identity: $IDENTITY"
codesign "${sign_args[@]}" "$APP/Contents/MacOS/reaper"
codesign "${sign_args[@]}" "$APP"
codesign --verify --verbose=4 "$APP"
