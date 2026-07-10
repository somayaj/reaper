#!/usr/bin/env bash
# Build a universal Reaper.app (arm64 + x86_64) for all Apple Silicon and Intel Macs.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Reaper.app"
ARM_BIN="$ROOT/target/release/reaper"
INTEL_BIN="$ROOT/target/x86_64-apple-darwin/release/reaper"

echo "Running editor regression suite…"
"$ROOT/scripts/test-editor-regression.sh"

echo "Building arm64 release binary…"
env -u CARGO_TARGET_DIR cargo build --release --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target"

echo "Building x86_64 release binary…"
env -u CARGO_TARGET_DIR cargo build --release \
  --manifest-path "$ROOT/Cargo.toml" \
  --target x86_64-apple-darwin \
  --target-dir "$ROOT/target"

echo "Vendoring debug adapters for arm64 + x86_64…"
"$ROOT/scripts/vendor-all-debug-adapters-macos.sh" || true

echo "Creating universal reaper binary…"
lipo -create -output "$ROOT/target/reaper-universal" "$ARM_BIN" "$INTEL_BIN"
lipo -info "$ROOT/target/reaper-universal"

echo "Assembling universal Reaper.app…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/static" "$APP/Contents/Resources/cursor-bridge" "$APP/Contents/Resources/gradle" "$APP/Contents/Resources/gradle/wrapper"

cp "$ROOT/target/reaper-universal" "$APP/Contents/MacOS/reaper"
chmod +x "$APP/Contents/MacOS/reaper"
cp "$ROOT/packaging/macos/Info.plist" "$APP/Contents/Info.plist"

"$ROOT/scripts/stamp-ui-build.sh"
"$ROOT/scripts/sync-launch-splash.sh"
"$ROOT/scripts/vendor-monaco.sh"
"$ROOT/scripts/vendor-google-java-format.sh"
REAPER_UNIVERSAL=1 "$ROOT/scripts/copy-bundled-node.sh" "$APP"
REAPER_UNIVERSAL=1 "$ROOT/scripts/copy-bundled-jdk.sh" "$APP"
"$ROOT/scripts/copy-bundled-jdtls.sh" "$APP"
REAPER_UNIVERSAL=1 "$ROOT/scripts/copy-bundled-debug-adapters.sh" "$APP"

if [[ ! -f "$ROOT/packaging/macos/Reaper.icns" ]] \
  || [[ "$ROOT/static/logo-icon.svg" -nt "$ROOT/packaging/macos/Reaper.icns" ]]; then
  echo "Generating app icon from logo-icon.svg..."
  "$ROOT/scripts/generate-macos-icon.sh"
fi
if [[ -f "$ROOT/packaging/macos/Reaper.icns" ]]; then
  cp "$ROOT/packaging/macos/Reaper.icns" "$APP/Contents/Resources/Reaper.icns"
fi

cp -R "$ROOT/static/." "$APP/Contents/Resources/static/"

if [[ -d "$ROOT/resources/google-java-format" ]]; then
  mkdir -p "$APP/Contents/Resources/google-java-format"
  if [[ -f "$ROOT/resources/google-java-format/google-java-format" ]]; then
    cp "$ROOT/resources/google-java-format/google-java-format" \
      "$APP/Contents/Resources/google-java-format/"
    chmod +x "$APP/Contents/Resources/google-java-format/google-java-format"
  fi
  if [[ -f "$ROOT/resources/google-java-format/google-java-format-all-deps.jar" ]]; then
    cp "$ROOT/resources/google-java-format/google-java-format-all-deps.jar" \
      "$APP/Contents/Resources/google-java-format/"
  fi
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
if [[ -n "$VERSION" ]]; then
  sed -i '' "s/name=\"reaper-app-version\" content=\"[^\"]*\"/name=\"reaper-app-version\" content=\"$VERSION\"/" \
    "$APP/Contents/Resources/static/index.html"
fi

cp "$ROOT/gradle/reaper-classpath.init.gradle" "$APP/Contents/Resources/gradle/"
cp "$ROOT/gradle/reaper-coverage.init.gradle" "$APP/Contents/Resources/gradle/"
cp "$ROOT/Cargo.toml" "$APP/Contents/Resources/Cargo.toml"
cp "$ROOT/gradlew" "$APP/Contents/Resources/gradlew"
chmod +x "$APP/Contents/Resources/gradlew"
cp "$ROOT/gradle/wrapper/gradle-wrapper.jar" "$APP/Contents/Resources/gradle/wrapper/"
cp "$ROOT/gradle/wrapper/gradle-wrapper.properties" "$APP/Contents/Resources/gradle/wrapper/"

if [[ -f "$ROOT/cursor-bridge/server.mjs" ]]; then
  BRIDGE_TMP="$ROOT/dist/cursor-bridge-universal-staging"
  rm -rf "$BRIDGE_TMP"
  mkdir -p "$BRIDGE_TMP"
  cp "$ROOT/cursor-bridge/server.mjs" \
    "$ROOT/cursor-bridge/install-deps.mjs" \
    "$ROOT/cursor-bridge/package.json" \
    "$BRIDGE_TMP/"
  cp "$ROOT/cursor-bridge/package-lock.json" "$BRIDGE_TMP/" 2>/dev/null || true
  if [[ ! -f "$BRIDGE_TMP/node_modules/@connectrpc/connect/package.json" ]]; then
    echo "Installing cursor-bridge dependencies for universal bundle…"
    BUILD_NODE="$ROOT/resources/node-macos-arm64/bin/node"
    if [[ -x "$BUILD_NODE" ]]; then
      (cd "$BRIDGE_TMP" && "$BUILD_NODE" install-deps.mjs)
    elif command -v npm >/dev/null 2>&1; then
      (cd "$BRIDGE_TMP" && npm install --omit=dev)
    else
      echo "Warning: cursor-bridge node_modules missing and bundled node/npm unavailable" >&2
    fi
  fi
  cp -R "$BRIDGE_TMP/." "$APP/Contents/Resources/cursor-bridge/"
fi

echo "Signing Reaper.app…"
"$ROOT/scripts/sign-macos-app.sh" "$APP"

echo "Built universal $APP"
echo "Open with: open \"$APP\""
