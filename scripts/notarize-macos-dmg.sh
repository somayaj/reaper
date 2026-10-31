#!/usr/bin/env bash
# Notarize a signed Reaper DMG and staple the ticket (Gatekeeper-friendly download).
#
# Prerequisites:
#   - Developer ID–signed Reaper.app (REAPER_SIGN_IDENTITY when building)
#   - Signed DMG (build-macos-dmg.sh signs when REAPER_SIGN_IDENTITY is set)
#   - notarytool credentials (one of):
#       REAPER_NOTARY_KEYCHAIN_PROFILE  — from: xcrun notarytool store-credentials
#       or App Store Connect API key:
#       REAPER_NOTARY_KEY_PATH, REAPER_NOTARY_KEY_ID, REAPER_NOTARY_ISSUER_ID
#
# Usage:
#   export REAPER_SIGN_IDENTITY="Developer ID Application: …"
#   ./scripts/build-macos-dmg.sh
#   export REAPER_NOTARY_KEYCHAIN_PROFILE=AC_NOTARY
#   ./scripts/notarize-macos-dmg.sh dist/reaper-0.1.21-macos-arm64.dmg
#
# Release integration:
#   REAPER_NOTARIZE=1 REAPER_NOTARY_KEYCHAIN_PROFILE=AC_NOTARY ./scripts/release-macos.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DMG="${1:-}"

if [[ -z "$DMG" ]]; then
  VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
  ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
  DMG="$ROOT/dist/reaper-${VERSION}-macos-${ARCH}.dmg"
fi

if [[ ! -f "$DMG" ]]; then
  echo "DMG not found: $DMG" >&2
  exit 1
fi

if ! command -v xcrun >/dev/null 2>&1; then
  echo "xcrun is required (Xcode command line tools)" >&2
  exit 1
fi

if ! xcrun notarytool --help >/dev/null 2>&1; then
  echo "notarytool not available — install Xcode CLT or Xcode" >&2
  exit 1
fi

if ! codesign --verify --verbose=2 "$DMG" 2>/dev/null; then
  echo "DMG is not signed or signature invalid: $DMG" >&2
  echo "Build with REAPER_SIGN_IDENTITY=\"Developer ID Application: …\" then re-create the DMG." >&2
  exit 1
fi

if codesign -dv "$DMG" 2>&1 | grep -q 'Signature=adhoc'; then
  echo "DMG is ad-hoc signed; Apple notarization requires Developer ID." >&2
  exit 1
fi

notary_submit_args=(submit "$DMG" --wait)
if [[ -n "${REAPER_NOTARY_KEYCHAIN_PROFILE:-}" ]]; then
  notary_submit_args+=(--keychain-profile "$REAPER_NOTARY_KEYCHAIN_PROFILE")
elif [[ -n "${REAPER_NOTARY_KEY_PATH:-}" && -n "${REAPER_NOTARY_KEY_ID:-}" && -n "${REAPER_NOTARY_ISSUER_ID:-}" ]]; then
  notary_submit_args+=(
    --key "$REAPER_NOTARY_KEY_PATH"
    --key-id "$REAPER_NOTARY_KEY_ID"
    --issuer "$REAPER_NOTARY_ISSUER_ID"
  )
else
  cat >&2 <<'EOF'
Notary credentials missing. Set one of:

  export REAPER_NOTARY_KEYCHAIN_PROFILE=AC_NOTARY
  # after: xcrun notarytool store-credentials AC_NOTARY \
  #   --apple-id YOU@icloud.com --team-id TEAMID --password 'app-specific-password'

  or App Store Connect API key:
  export REAPER_NOTARY_KEY_PATH=/path/AuthKey_XXXX.p8
  export REAPER_NOTARY_KEY_ID=XXXX
  export REAPER_NOTARY_ISSUER_ID=uuid-issuer
EOF
  exit 1
fi

echo "Submitting to Apple notary service: $DMG"
NOTARY_OUT="$(mktemp -t reaper-notary.XXXXXX)"
xcrun notarytool "${notary_submit_args[@]}" | tee "$NOTARY_OUT"
if grep -q 'status: Invalid' "$NOTARY_OUT"; then
  echo "Notarization failed (Invalid). See log with: xcrun notarytool log <id> --keychain-profile ..." >&2
  rm -f "$NOTARY_OUT"
  exit 1
fi
rm -f "$NOTARY_OUT"

echo "Stapling notarization ticket…"
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

echo "Notarized and stapled: $DMG"
