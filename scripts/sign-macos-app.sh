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

sign_jdk_bundle() {
  local jdk_dir="$1"
  if [[ -L "$jdk_dir" ]]; then
    return 0
  fi
  if [[ ! -x "$jdk_dir/Contents/Home/bin/java" ]]; then
    return 0
  fi
  # Reaper stores vendor metadata beside the bundle; Temurin .jdk roots must only contain Contents/.
  rm -f "$jdk_dir/.vendor-version"
  while IFS= read -r -d '' bin; do
    codesign "${sign_args[@]}" "$bin"
  done < <(find "$jdk_dir" -type f \( -perm +111 -o -name '*.dylib' -o -name '*.jnilib' \) -print0)
  codesign "${sign_args[@]}" "$jdk_dir"
}

# Temurin JDK legal/ files are shipped read-only; xattr -cr errors without u+w.
for jdk_dir in "$APP/Contents/Resources/jdk-21" "$APP/Contents/Resources/jdk-21-arm64" "$APP/Contents/Resources/jdk-21-x86_64"; do
  if [[ -e "$jdk_dir" && ! -L "$jdk_dir" ]]; then
    chmod -R u+w "$jdk_dir" 2>/dev/null || true
  fi
done

xattr -cr "$APP"

echo "  identity: $IDENTITY"
codesign "${sign_args[@]}" "$APP/Contents/MacOS/reaper"
if [[ -f "$APP/Contents/Resources/node/bin/node" ]]; then
  codesign "${sign_args[@]}" "$APP/Contents/Resources/node/bin/node"
fi
for node_dir in "$APP/Contents/Resources/node-arm64" "$APP/Contents/Resources/node-x86_64"; do
  if [[ -x "$node_dir/bin/node" ]]; then
    codesign "${sign_args[@]}" "$node_dir/bin/node"
  fi
done
for jdk_dir in "$APP/Contents/Resources/jdk-21" "$APP/Contents/Resources/jdk-21-arm64" "$APP/Contents/Resources/jdk-21-x86_64"; do
  sign_jdk_bundle "$jdk_dir"
done
if [[ -x "$APP/Contents/Resources/jdtls/bin/jdtls" ]]; then
  codesign "${sign_args[@]}" "$APP/Contents/Resources/jdtls/bin/jdtls"
fi
codesign "${sign_args[@]}" "$APP"
codesign --verify --verbose=4 "$APP"
