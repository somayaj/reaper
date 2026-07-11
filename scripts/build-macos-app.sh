#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Reaper.app"
BINARY="$ROOT/target/release/reaper"

if [[ "${REAPER_SKIP_REGRESSION:-}" != "1" ]]; then
  echo "Running editor regression suite…"
  "$ROOT/scripts/test-editor-regression.sh"
else
  echo "Skipping editor regression (REAPER_SKIP_REGRESSION=1)…"
fi

echo "Building release binary…"
env -u CARGO_TARGET_DIR cargo build --release --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target"

echo "Vendoring debug adapters for $(uname -m)…"
REAPER_MACOS_ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}" "$ROOT/scripts/vendor-debug-adapters-macos.sh" || true

echo "Assembling Reaper.app…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/static" "$APP/Contents/Resources/cursor-bridge" "$APP/Contents/Resources/gradle" "$APP/Contents/Resources/gradle/wrapper"

cp "$BINARY" "$APP/Contents/MacOS/reaper"
chmod +x "$APP/Contents/MacOS/reaper"
cp "$ROOT/packaging/macos/Info.plist" "$APP/Contents/Info.plist"

"$ROOT/scripts/stamp-ui-build.sh"
"$ROOT/scripts/sync-launch-splash.sh"
"$ROOT/scripts/vendor-monaco.sh"
"$ROOT/scripts/vendor-google-java-format.sh"
"$ROOT/scripts/copy-bundled-node.sh" "$APP"
"$ROOT/scripts/copy-bundled-jdk.sh" "$APP"
"$ROOT/scripts/copy-bundled-jdtls.sh" "$APP"
"$ROOT/scripts/copy-bundled-debug-adapters.sh" "$APP"

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
  /usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP/Contents/Info.plist" >/dev/null
  /usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP/Contents/Info.plist" >/dev/null
fi

cp "$ROOT/gradle/reaper-classpath.init.gradle" "$APP/Contents/Resources/gradle/"
cp "$ROOT/gradle/reaper-coverage.init.gradle" "$APP/Contents/Resources/gradle/"
cp "$ROOT/Cargo.toml" "$APP/Contents/Resources/Cargo.toml"
cp "$ROOT/gradlew" "$APP/Contents/Resources/gradlew"
chmod +x "$APP/Contents/Resources/gradlew"
cp "$ROOT/gradle/wrapper/gradle-wrapper.jar" "$APP/Contents/Resources/gradle/wrapper/"
cp "$ROOT/gradle/wrapper/gradle-wrapper.properties" "$APP/Contents/Resources/gradle/wrapper/"

if [[ -f "$ROOT/cursor-bridge/server.mjs" ]]; then
  if [[ ! -f "$ROOT/cursor-bridge/node_modules/@connectrpc/connect/package.json" ]]; then
    echo "Installing cursor-bridge dependencies for bundle…"
    ARCH="$(uname -m)"
    export REAPER_MACOS_ARCH="$ARCH"
    "$ROOT/scripts/vendor-node-macos.sh"
    BUILD_NODE="$ROOT/resources/node-macos-${ARCH}/bin/node"
    if [[ -x "$BUILD_NODE" ]]; then
      (cd "$ROOT/cursor-bridge" && "$BUILD_NODE" install-deps.mjs)
    elif command -v npm >/dev/null 2>&1; then
      (cd "$ROOT/cursor-bridge" && npm install --omit=dev)
    else
      echo "Warning: cursor-bridge node_modules missing and bundled node/npm unavailable" >&2
    fi
  fi
  cp -R "$ROOT/cursor-bridge/." "$APP/Contents/Resources/cursor-bridge/"
fi

echo "Signing Reaper.app…"
"$ROOT/scripts/sign-macos-app.sh" "$APP"

echo "Built $APP"
echo "Open with: open \"$APP\""
