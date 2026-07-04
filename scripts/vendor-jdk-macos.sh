#!/usr/bin/env bash
# Vendor Temurin JDK 21 for bundled jdtls runtime in Reaper.app.
# Prefers Homebrew on macOS (reliable, no GitHub curl). Falls back to Adoptium download.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MAJOR="${JDK_MAJOR:-21}"
JDK_TAG="${JDK_TAG:-21.0.11+10}"
JDK_TAG_FILE="${JDK_TAG//+/_}"
ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
HOST_ARCH="$(uname -m)"

case "$ARCH" in
  arm64) ADOPTIUM_ARCH=aarch64; GH_ARCH=aarch64 ;;
  x86_64) ADOPTIUM_ARCH=x64; GH_ARCH=x64 ;;
  *)
    echo "No bundled JDK for macOS arch: $ARCH" >&2
    exit 1
    ;;
esac

DEST="$ROOT/resources/jdk-macos-${ARCH}"
JAVA="$DEST/Contents/Home/bin/java"
MARKER="$ROOT/resources/jdk-macos-${ARCH}.vendor-version"
CACHE="$ROOT/resources/.cache"
TARBALL="$CACHE/temurin-jdk-${MAJOR}-mac-${ADOPTIUM_ARCH}.tar.gz"
FORMULA="openjdk@${MAJOR}"
API_URL="https://api.adoptium.net/v3/binary/version/${JDK_TAG}/mac/${ADOPTIUM_ARCH}/jdk/hotspot/normal/eclipse?project=jdk"
PINNED_URL="https://github.com/adoptium/temurin${MAJOR}-binaries/releases/download/jdk-${JDK_TAG}/OpenJDK${MAJOR}U-jdk_${GH_ARCH}_mac_hotspot_${JDK_TAG_FILE}.tar.gz"

find_brew_java_home() {
  local prefix home
  for prefix in /opt/homebrew /usr/local; do
    home="$prefix/opt/${FORMULA}/libexec/openjdk.jdk/Contents/Home"
    if [[ -x "$home/bin/java" ]]; then
      echo "$home"
      return 0
    fi
  done
  return 1
}

ensure_brew_jdk() {
  if find_brew_java_home >/dev/null; then
    return 0
  fi
  if ! command -v brew >/dev/null 2>&1; then
    return 1
  fi
  echo "Installing ${FORMULA} via Homebrew…" >&2
  brew install "$FORMULA"
  find_brew_java_home >/dev/null
}

copy_brew_jdk() {
  local home="$1"
  rm -rf "$DEST"
  mkdir -p "$DEST"
  cp -R "$(dirname "$home")" "$DEST/Contents"
  echo "homebrew-${MAJOR}" > "$MARKER"
  echo "Bundled JDK ${MAJOR} from Homebrew (${ARCH}) → $DEST"
}

tarball_valid() {
  [[ -f "$1" ]] && tar -tzf "$1" >/dev/null 2>&1
}

install_from_tarball() {
  local tarball="$1"
  local tmp jdk_root
  tmp="$(mktemp -d)"
  tar -xzf "$tarball" -C "$tmp"
  jdk_root="$(find "$tmp" -maxdepth 1 -type d -name 'jdk-*' | head -1)"
  if [[ -z "$jdk_root" || ! -x "$jdk_root/Contents/Home/bin/java" ]]; then
    echo "Unexpected Temurin JDK layout in $tarball" >&2
    find "$tmp" -maxdepth 4 | head -30 >&2
    rm -rf "$tmp"
    return 1
  fi
  rm -rf "$DEST"
  mv "$jdk_root" "$DEST"
  rm -rf "$tmp"
  echo "$JDK_TAG" > "$MARKER"
  echo "Saved JDK ${JDK_TAG} (${ARCH}) → $DEST"
}

download_tarball() {
  local url="$1"
  local label="$2"
  mkdir -p "$CACHE"
  echo "Downloading Temurin JDK ${JDK_TAG} (${ADOPTIUM_ARCH}) via ${label}…" >&2
  if curl -fsSL \
    --connect-timeout 30 \
    --max-time 900 \
    --retry 5 \
    --retry-all-errors \
    --retry-delay 3 \
    -C - \
    "$url" \
    -o "$TARBALL.part"; then
    mv "$TARBALL.part" "$TARBALL"
    return 0
  fi
  rm -f "$TARBALL.part"
  return 1
}

if [[ -f "$DEST/.vendor-version" && ! -f "$MARKER" ]]; then
  cp "$DEST/.vendor-version" "$MARKER"
  rm -f "$DEST/.vendor-version"
fi

if [[ -x "$JAVA" && -f "$MARKER" ]]; then
  CURRENT="$("$JAVA" -version 2>&1 | head -1 || true)"
  if [[ "$CURRENT" == *"${MAJOR}"* ]]; then
    echo "JDK ${MAJOR} (${ARCH}) already present at $DEST"
    exit 0
  fi
  rm -rf "$DEST"
fi

mkdir -p "$CACHE"

# 1. Homebrew for native arch (your Mac's CPU) — no GitHub download needed.
if [[ "$ARCH" == "$HOST_ARCH" ]]; then
  if ensure_brew_jdk; then
    copy_brew_jdk "$(find_brew_java_home)"
    exit 0
  fi
fi

# 2. Cached tarball.
if tarball_valid "$TARBALL"; then
  echo "Using cached JDK tarball $TARBALL"
  install_from_tarball "$TARBALL"
  exit 0
fi

# 3. Adoptium download (needed for cross-arch universal builds, e.g. x64 on Apple Silicon).
if download_tarball "$API_URL" "Adoptium API" || download_tarball "$PINNED_URL" "GitHub release"; then
  install_from_tarball "$TARBALL"
  exit 0
fi

cat >&2 <<EOF
vendor-jdk-macos: could not vendor JDK ${MAJOR} (${ARCH}).

Native arch (your Mac) — use Homebrew:
  brew install ${FORMULA}
  scripts/build-macos-app.sh

Cross-arch (x64 for universal build on Apple Silicon) needs a network download to GitHub.
If that fails, either retry later or:
  REAPER_ALLOW_PARTIAL_JDK=1 scripts/build-macos-universal-app.sh
EOF
exit 1
