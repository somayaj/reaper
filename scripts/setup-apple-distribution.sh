#!/usr/bin/env bash
# One-time Apple direct-distribution setup for Reaper (Team 54X5V9C9JM).
# Generates a CSR, helps install Developer ID Application, stores notary credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CSR_DIR="$ROOT/packaging/macos/signing"
CSR="$CSR_DIR/DeveloperIDApplication.csr"
KEY="$CSR_DIR/DeveloperIDApplication.key"
TEAM_ID="${REAPER_APPLE_TEAM_ID:-54X5V9C9JM}"
APPLE_ID="${REAPER_APPLE_ID:-ashlie94@yahoo.com}"
NOTARY_PROFILE="${REAPER_NOTARY_KEYCHAIN_PROFILE:-AC_NOTARY}"

mkdir -p "$CSR_DIR"
chmod 700 "$CSR_DIR"

if [[ ! -f "$CSR" ]]; then
  echo "Generating CSR + private key (keep the .key private)…"
  openssl req -new -newkey rsa:2048 -nodes \
    -keyout "$KEY" \
    -out "$CSR" \
    -subj "/emailAddress=${APPLE_ID}/CN=Asha Somayajula/C=US"
  chmod 600 "$KEY"
fi

echo ""
echo "=== 1) Create Developer ID Application certificate ==="
echo "CSR: $CSR"
echo "Open: https://developer.apple.com/account/resources/certificates/add"
echo "  → Developer ID Application → upload the .csr file above → download .cer"
echo ""

if [[ -n "${1:-}" && -f "$1" ]]; then
  CER="$1"
elif latest="$(ls -t "$HOME/Downloads"/developerID_application*.cer "$HOME/Downloads"/DeveloperID*.cer 2>/dev/null | head -1)" && [[ -n "$latest" ]]; then
  CER="$latest"
else
  CER=""
fi

if [[ -n "$CER" ]]; then
  echo "Installing certificate: $CER"
  security import "$CER" -k ~/Library/Keychains/login.keychain-db -T /usr/bin/codesign -T /usr/bin/security
else
  echo "After download, run:"
  echo "  $0 ~/Downloads/developerID_application.cer"
  echo ""
fi

echo "=== 2) Verify signing identity ==="
security find-identity -v -p codesigning | grep -F "Developer ID Application" || {
  echo "No Developer ID Application identity yet — complete step 1." >&2
  exit 1
}

echo ""
echo "=== 3) Store notary credentials (app-specific password) ==="
if xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1; then
  echo "Keychain profile '$NOTARY_PROFILE' already exists."
else
  if [[ -z "${REAPER_NOTARY_PASSWORD:-}" ]]; then
    echo "Create an app-specific password at https://appleid.apple.com (Sign-In and Security)."
    read -rsp "App-specific password for ${APPLE_ID}: " REAPER_NOTARY_PASSWORD
    echo ""
  fi
  xcrun notarytool store-credentials "$NOTARY_PROFILE" \
    --apple-id "$APPLE_ID" \
    --team-id "$TEAM_ID" \
    --password "$REAPER_NOTARY_PASSWORD"
  unset REAPER_NOTARY_PASSWORD
fi

IDENT="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p' | head -1)"
echo ""
echo "=== Done ==="
echo "export REAPER_SIGN_IDENTITY=\"$IDENT\""
echo "export REAPER_NOTARY_KEYCHAIN_PROFILE=$NOTARY_PROFILE"
echo "export REAPER_NOTARIZE=1"
echo ""
echo "Then: cd $ROOT && ./scripts/release-macos.sh"
