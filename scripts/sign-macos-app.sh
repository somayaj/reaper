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

if [[ -x "$APP/Contents/Resources/jdtls/bin/jdtls" ]]; then
  "$ROOT/scripts/warm-bundled-jdtls-config.sh" "$APP"
fi

sign_args=(--force --sign "$IDENTITY")
if [[ "$IDENTITY" != "-" ]]; then
  sign_args+=(--options runtime --timestamp)
  if [[ -f "$ENTITLEMENTS" ]]; then
    sign_args+=(--entitlements "$ENTITLEMENTS")
  fi
fi

_reaper_is_mach_o() {
  local f="$1"
  [[ -f "$f" ]] || return 1
  file -b "$f" 2>/dev/null | grep -q 'Mach-O'
}

# Sign every Mach-O under a directory, deepest paths first (required for nested bundles).
sign_mach_o_tree() {
  local dir="$1"
  [[ -d "$dir" ]] || return 0
  local f
  while IFS= read -r f; do
    codesign "${sign_args[@]}" "$f"
  done < <(
    find "$dir" -type f -print0 \
      | while IFS= read -r -d '' f; do
          if _reaper_is_mach_o "$f"; then
            printf '%s\0' "$f"
          fi
        done \
      | while IFS= read -r -d '' f; do
          printf '%s\t%s\n' "$(printf '%s' "$f" | wc -c | tr -d ' ')" "$f"
        done \
      | sort -t $'\t' -k1,1nr \
      | cut -f2-
  )
}

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
  rm -rf "$jdk_dir/Contents/Home/jmods"
  sign_mach_o_tree "$jdk_dir"
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
# Debug adapters, formatters, and other vendored CLI/dylibs (notary scans all Mach-O).
sign_mach_o_tree "$APP/Contents/Resources/debug-adapters"
sign_mach_o_tree "$APP/Contents/Resources/google-java-format"
if [[ -d "$APP/Contents/Resources/jdtls" ]]; then
  sign_mach_o_tree "$APP/Contents/Resources/jdtls"
fi
codesign "${sign_args[@]}" "$APP"
codesign --verify --verbose=4 "$APP"
