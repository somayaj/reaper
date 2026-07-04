#!/usr/bin/env bash
# Copy vendored Temurin JDK 21 into a Reaper.app bundle (jdtls runtime only).
# REAPER_UNIVERSAL=1 copies both arm64 and x86_64 runtimes for one app on all Macs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:?usage: copy-bundled-jdk.sh /path/to/Reaper.app}"

copy_one() {
  local arch="$1"
  export REAPER_MACOS_ARCH="$arch"
  if ! "$ROOT/scripts/vendor-jdk-macos.sh"; then
    return 1
  fi
  local src="$ROOT/resources/jdk-macos-${arch}"
  local java="$src/Contents/Home/bin/java"
  if [[ ! -x "$java" ]]; then
    echo "Bundled JDK missing at $java" >&2
    exit 1
  fi
  local dest="$APP/Contents/Resources/jdk-21-${arch}"
  rm -rf "$dest"
  mkdir -p "$(dirname "$dest")"
  cp -R "$src/." "$dest/"
  # .vendor-version must not live at the .jdk bundle root — it breaks codesign sealing.
  rm -f "$dest/.vendor-version"
  # Temurin legal/ files are read-only; signing runs xattr -cr on the app bundle.
  chmod -R u+w "$dest"
  echo "Bundled JDK 21 for ${arch} → $dest"
}

if [[ "${REAPER_UNIVERSAL:-}" == "1" ]]; then
  copy_one arm64
  if ! copy_one x86_64; then
    if [[ "${REAPER_ALLOW_PARTIAL_JDK:-}" == "1" ]]; then
      echo "Warning: x86_64 JDK missing — universal app will only have Java navigation on Apple Silicon" >&2
    else
      exit 1
    fi
  fi
else
  ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
  copy_one "$ARCH"
  # Legacy path alias — symlink avoids duplicating ~330 MB on native builds.
  rm -rf "$APP/Contents/Resources/jdk-21"
  ln -sf "jdk-21-${ARCH}" "$APP/Contents/Resources/jdk-21"
  echo "Bundled JDK 21 legacy path → jdk-21 → jdk-21-${ARCH}"
fi
