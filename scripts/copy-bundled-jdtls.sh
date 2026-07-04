#!/usr/bin/env bash
# Copy vendored jdtls into a Reaper.app bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: copy-bundled-jdtls.sh /path/to/Reaper.app}"

"$ROOT/scripts/vendor-jdtls-macos.sh"

SRC="$ROOT/resources/jdtls"
DEST="$APP/Contents/Resources/jdtls"
if [[ ! -x "$SRC/bin/jdtls" ]]; then
  echo "Bundled jdtls missing at $SRC/bin/jdtls" >&2
  exit 1
fi

rm -rf "$DEST"
mkdir -p "$DEST"
cp -R "$SRC/." "$DEST/"
chmod +x "$DEST/bin/jdtls"
echo "Bundled jdtls → $DEST"
