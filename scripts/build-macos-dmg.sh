#!/usr/bin/env bash
# Build Reaper.app and wrap it in a distributable macOS .dmg.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Reaper.app"
STAGING="$ROOT/dist/dmg-staging"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
ARCH="$(uname -m)"
DMG="$ROOT/dist/Reaper-${VERSION}-macos-${ARCH}.dmg"

if [[ -z "$VERSION" ]]; then
  echo "Could not read version from Cargo.toml" >&2
  exit 1
fi

if [[ "${REAPER_SKIP_APP_BUILD:-}" != "1" ]]; then
  "$ROOT/scripts/build-macos-app.sh"
elif [[ ! -d "$APP" ]]; then
  echo "Reaper.app not found at $APP (run without REAPER_SKIP_APP_BUILD=1 first)" >&2
  exit 1
fi

echo "Preparing DMG staging..."
rm -rf "$STAGING"
mkdir -p "$STAGING"
cp -R "$APP" "$STAGING/"
ln -sf /Applications "$STAGING/Applications"
xattr -cr "$STAGING"

echo "Creating ${DMG}..."
rm -f "$DMG"
hdiutil create \
  -volname "Reaper" \
  -srcfolder "$STAGING" \
  -ov \
  -format UDZO \
  -fs HFS+ \
  "$DMG" >/dev/null

if [[ -n "${REAPER_SIGN_IDENTITY:-}" && "${REAPER_SIGN_IDENTITY}" != "-" ]]; then
  echo "Signing DMG with ${REAPER_SIGN_IDENTITY}…"
  codesign --force --sign "$REAPER_SIGN_IDENTITY" --timestamp "$DMG"
fi

rm -rf "$STAGING"

echo ""
echo "Done."
echo "  App:  $APP"
echo "  DMG:  $DMG"
echo ""
echo "Install: open the DMG once, drag Reaper.app to Applications, then eject the volume."
echo "Test:    open \"$APP\"   (runs without mounting a DMG)"
echo ""
echo "Note: Each time you open the .dmg, macOS mounts a new 'Reaper' drive in Finder."
echo "      Old mounts are not removed automatically. Eject them in Finder, or run:"
echo "        $ROOT/scripts/eject-reaper-dmgs.sh"
