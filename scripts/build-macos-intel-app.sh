#!/usr/bin/env bash
# Assemble Reaper.app for Intel macOS (cross-compiled on Apple Silicon).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Reaper-intel.app"
BINARY="$ROOT/target/x86_64-apple-darwin/release/reaper"
GJF_VERSION="${GOOGLE_JAVA_FORMAT_VERSION:-1.25.2}"

echo "Building Intel release binary…"
env -u CARGO_TARGET_DIR cargo build --release \
  --manifest-path "$ROOT/Cargo.toml" \
  --target x86_64-apple-darwin \
  --target-dir "$ROOT/target"

echo "Assembling Reaper.app (Intel)…"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" \
  "$APP/Contents/Resources/static" \
  "$APP/Contents/Resources/cursor-bridge" \
  "$APP/Contents/Resources/gradle" \
  "$APP/Contents/Resources/gradle/wrapper" \
  "$APP/Contents/Resources/google-java-format"

cp "$BINARY" "$APP/Contents/MacOS/reaper"
chmod +x "$APP/Contents/MacOS/reaper"
cp "$ROOT/packaging/macos/Info.plist" "$APP/Contents/Info.plist"

"$ROOT/scripts/stamp-ui-build.sh"
"$ROOT/scripts/sync-launch-splash.sh"
"$ROOT/scripts/vendor-monaco.sh"
REAPER_MACOS_ARCH=x86_64 "$ROOT/scripts/copy-bundled-node.sh" "$APP"

GJF_BIN="$APP/Contents/Resources/google-java-format/google-java-format"
GJF_JAR="$APP/Contents/Resources/google-java-format/google-java-format-all-deps.jar"
if [[ ! -f "$GJF_BIN" ]]; then
  echo "Downloading google-java-format ${GJF_VERSION} (darwin-x86-64)…"
  curl -fsSL \
    "https://github.com/google/google-java-format/releases/download/v${GJF_VERSION}/google-java-format_darwin-x86-64" \
    -o "$GJF_BIN"
  chmod +x "$GJF_BIN"
fi
if [[ ! -f "$GJF_JAR" ]]; then
  echo "Downloading google-java-format ${GJF_VERSION} jar…"
  curl -fsSL \
    "https://repo1.maven.org/maven2/com/google/googlejavaformat/google-java-format/${GJF_VERSION}/google-java-format-${GJF_VERSION}-all-deps.jar" \
    -o "$GJF_JAR"
fi

if [[ ! -f "$ROOT/packaging/macos/Reaper.icns" ]] \
  || [[ "$ROOT/static/logo-icon.svg" -nt "$ROOT/packaging/macos/Reaper.icns" ]]; then
  echo "Generating app icon from logo-icon.svg..."
  "$ROOT/scripts/generate-macos-icon.sh"
fi
if [[ -f "$ROOT/packaging/macos/Reaper.icns" ]]; then
  cp "$ROOT/packaging/macos/Reaper.icns" "$APP/Contents/Resources/Reaper.icns"
fi

cp -R "$ROOT/static/." "$APP/Contents/Resources/static/"

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
  BRIDGE_TMP="$ROOT/dist/cursor-bridge-x64-staging"
  BUILD_NODE="$ROOT/resources/node-macos-x86_64/bin/node"
  rm -rf "$BRIDGE_TMP"
  mkdir -p "$BRIDGE_TMP"
  cp "$ROOT/cursor-bridge/server.mjs" \
    "$ROOT/cursor-bridge/install-deps.mjs" \
    "$ROOT/cursor-bridge/package.json" \
    "$BRIDGE_TMP/"
  cp "$ROOT/cursor-bridge/package-lock.json" "$BRIDGE_TMP/" 2>/dev/null || true
  if [[ ! -f "$BRIDGE_TMP/node_modules/@connectrpc/connect/package.json" ]]; then
    echo "Installing cursor-bridge dependencies for Intel bundle…"
    if [[ -x "$BUILD_NODE" ]]; then
      (cd "$BRIDGE_TMP" && "$BUILD_NODE" install-deps.mjs)
    elif command -v npm >/dev/null 2>&1; then
      (cd "$BRIDGE_TMP" && npm install --omit=dev --os=darwin --cpu=x64)
    else
      echo "Warning: cursor-bridge node_modules missing and bundled node/npm unavailable" >&2
    fi
  fi
  cp -R "$BRIDGE_TMP/." "$APP/Contents/Resources/cursor-bridge/"
fi

echo "Signing Reaper.app (Intel)…"
"$ROOT/scripts/sign-macos-app.sh" "$APP"

echo "Built $APP"
