#!/usr/bin/env bash
# Generate jdtls configuration/org.eclipse.osgi inside the app bundle before codesign.
# Stale configuration/ from a dev machine breaks the resource seal (notarization fails).
set -euo pipefail

APP="${1:?usage: warm-bundled-jdtls-config.sh /path/to/Reaper.app}"

JDTLS="$APP/Contents/Resources/jdtls"
if [[ ! -x "$JDTLS/bin/jdtls" ]]; then
  echo "warm-bundled-jdtls: skip (no bundled jdtls)" >&2
  exit 0
fi

JAVA=""
PREFERRED_ARCH="${REAPER_MACOS_ARCH:-}"
for jdk in \
  "$APP/Contents/Resources/jdk-21-${PREFERRED_ARCH}/Contents/Home/bin/java" \
  "$APP/Contents/Resources/jdk-21-arm64/Contents/Home/bin/java" \
  "$APP/Contents/Resources/jdk-21-x86_64/Contents/Home/bin/java" \
  "$APP/Contents/Resources/jdk-21/Contents/Home/bin/java"; do
  [[ -n "$PREFERRED_ARCH" && "$jdk" != *"jdk-21-${PREFERRED_ARCH}/"* ]] && continue
  if [[ -x "$jdk" ]]; then
    JAVA="$jdk"
    break
  fi
done
if [[ -z "$JAVA" ]]; then
  for jdk in "$APP/Contents/Resources/jdk-21-"*/Contents/Home/bin/java; do
    if [[ -x "$jdk" ]]; then JAVA="$jdk"; break; fi
  done
fi

if [[ "$JAVA" == *"jdk-21-arm64"* ]] || [[ "$PREFERRED_ARCH" == "arm64" ]]; then
  SHARED_CONFIG="$JDTLS/config_mac_arm"
else
  SHARED_CONFIG="$JDTLS/config_mac"
fi
if [[ -z "$JAVA" ]]; then
  echo "warm-bundled-jdtls: bundled JDK java not found" >&2
  exit 1
fi

rm -rf "$JDTLS/configuration"

LAUNCHER="$(find "$JDTLS/plugins" -maxdepth 1 -name 'org.eclipse.equinox.launcher_*.jar' | sort | head -1)"
if [[ -z "$LAUNCHER" ]]; then
  echo "warm-bundled-jdtls: equinox launcher jar missing" >&2
  exit 1
fi

WARM_DATA="$(mktemp -d /tmp/reaper-jdtls-warm.XXXXXX)"
cleanup() { rm -rf "$WARM_DATA"; }
trap cleanup EXIT

CONFIG_AREA="-Dosgi.sharedConfiguration.area=${SHARED_CONFIG}"
echo "warm-bundled-jdtls: generating configuration in bundle…"
"$JAVA" \
  -Declipse.application=org.eclipse.jdt.ls.core.id1 \
  -Dosgi.bundles.defaultStartLevel=4 \
  -Declipse.product=org.eclipse.jdt.ls.core.product \
  -Dosgi.checkConfiguration=true \
  "$CONFIG_AREA" \
  -Dosgi.sharedConfiguration.area.readOnly=true \
  -Dosgi.configuration.cascaded=true \
  -Xms256m \
  --add-modules=ALL-SYSTEM \
  --add-opens java.base/java.util=ALL-UNNAMED \
  --add-opens java.base/java.lang=ALL-UNNAMED \
  -jar "$LAUNCHER" \
  -data "$WARM_DATA" \
  >/dev/null 2>&1 &
PID=$!

deadline=$((SECONDS + 120))
while (( SECONDS < deadline )); do
  if [[ -d "$JDTLS/configuration/org.eclipse.osgi" ]]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    echo "warm-bundled-jdtls: ready ($JDTLS/configuration)"
    exit 0
  fi
  if ! kill -0 "$PID" 2>/dev/null; then
    echo "warm-bundled-jdtls: warm process exited early" >&2
    exit 1
  fi
  sleep 0.5
done

kill "$PID" 2>/dev/null || true
echo "warm-bundled-jdtls: timed out" >&2
exit 1
