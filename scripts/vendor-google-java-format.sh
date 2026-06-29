#!/usr/bin/env bash
# Download Google Java Format for bundled Java formatting in Reaper.app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${GOOGLE_JAVA_FORMAT_VERSION:-1.25.2}"
DEST="$ROOT/resources/google-java-format"
BIN="$DEST/google-java-format"
JAR="$DEST/google-java-format-all-deps.jar"
BASE="https://github.com/google/google-java-format/releases/download/v${VERSION}"

mkdir -p "$DEST"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) ASSET="google-java-format_darwin-arm64" ;;
  Darwin-x86_64) ASSET="google-java-format_darwin-x86-64" ;;
  Linux-x86_64) ASSET="google-java-format_linux-x86-64" ;;
  *)
    echo "No native google-java-format binary for $(uname -s)-$(uname -m); jar fallback only." >&2
    ASSET=""
    ;;
esac

if [[ -n "$ASSET" && ! -f "$BIN" ]]; then
  echo "Downloading google-java-format ${VERSION} (${ASSET})…"
  curl -fsSL "$BASE/$ASSET" -o "$BIN"
  chmod +x "$BIN"
  echo "Saved $BIN"
elif [[ -f "$BIN" ]]; then
  echo "google-java-format ${VERSION} binary already present at $BIN"
fi

if [[ ! -f "$JAR" ]]; then
  echo "Downloading google-java-format ${VERSION} jar (fallback)…"
  curl -fsSL \
    "https://repo1.maven.org/maven2/com/google/googlejavaformat/google-java-format/${VERSION}/google-java-format-${VERSION}-all-deps.jar" \
    -o "$JAR"
  echo "Saved $JAR"
else
  echo "google-java-format ${VERSION} jar already present at $JAR"
fi
