#!/usr/bin/env bash
# Download Eclipse JDT Language Server for bundled Java navigation in Reaper.app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${JDTLS_VERSION:-1.60.0}"
BUILD="${JDTLS_BUILD:-202606262232}"
SHA256="${JDTLS_SHA256:-e94c303d8198f977930803582738771fd18c52c5492878410bf222b1aa81ef1d}"
CACHE="$ROOT/resources/.cache"
DEST="$ROOT/resources/jdtls"
TARBALL="$CACHE/jdt-language-server-${VERSION}-${BUILD}.tar.gz"
URL="https://www.eclipse.org/downloads/download.php?file=/jdtls/milestones/${VERSION}/jdt-language-server-${VERSION}-${BUILD}.tar.gz"
MARKER="$DEST/.vendor-version"

warm_jdtls_configuration() {
  local dest="$1"
  if [[ -d "$dest/configuration/org.eclipse.osgi" ]]; then
    return 0
  fi

  local java=""
  local arch
  arch="$(uname -m)"
  local bundled="$ROOT/resources/jdk-macos-${arch}/Contents/Home/bin/java"
  if [[ -x "$bundled" ]]; then
    java="$bundled"
  elif [[ -n "${JAVA_HOME:-}" && -x "${JAVA_HOME}/bin/java" ]]; then
    java="${JAVA_HOME}/bin/java"
  elif command -v java >/dev/null 2>&1; then
    java="$(command -v java)"
  else
    echo "jdtls warm-start skipped: java not found (vendor JDK 21 with scripts/vendor-jdk-macos.sh)" >&2
    return 1
  fi

  local jar config
  jar="$(find "$dest/plugins" -maxdepth 1 -name 'org.eclipse.equinox.launcher_*.jar' | head -1)"
  if [[ -z "$jar" ]]; then
    echo "jdtls warm-start failed: equinox launcher jar missing" >&2
    return 1
  fi

  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) config="$dest/config_mac_arm" ;;
    Darwin-*) config="$dest/config_mac" ;;
    Linux-aarch64|Linux-arm64) config="$dest/config_linux_arm" ;;
    Linux-*) config="$dest/config_linux" ;;
    *)
      echo "jdtls warm-start skipped: unsupported platform" >&2
      return 1
      ;;
  esac

  local warm_data
  warm_data="$(mktemp -d)"
  echo "Warming bundled jdtls configuration…"
  "$java" \
    -Declipse.application=org.eclipse.jdt.ls.core.id1 \
    -Dosgi.bundles.defaultStartLevel=4 \
    -Declipse.product=org.eclipse.jdt.ls.core.product \
    -Dosgi.checkConfiguration=true \
    "-Dosgi.sharedConfiguration.area=$config" \
    -Dosgi.sharedConfiguration.area.readOnly=true \
    -Dosgi.configuration.cascaded=true \
    -Xms256m \
    --add-modules=ALL-SYSTEM \
    --add-opens java.base/java.util=ALL-UNNAMED \
    --add-opens java.base/java.lang=ALL-UNNAMED \
    -jar "$jar" \
    -data "$warm_data" </dev/null >/dev/null 2>&1 &
  local pid=$!
  for _ in $(seq 1 90); do
    if [[ -d "$dest/configuration/org.eclipse.osgi" ]]; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      rm -rf "$warm_data"
      echo "Bundled jdtls configuration ready"
      return 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      break
    fi
    sleep 1
  done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  rm -rf "$warm_data"
  echo "jdtls warm-start timed out before configuration was ready" >&2
  return 1
}

mkdir -p "$CACHE"

if [[ -f "$MARKER" ]] && [[ "$(cat "$MARKER")" == "${VERSION}-${BUILD}" ]] \
  && [[ -x "$DEST/bin/jdtls" ]] && [[ -d "$DEST/plugins" ]]; then
  warm_jdtls_configuration "$DEST" || true
  echo "jdtls ${VERSION} already vendored at $DEST"
  exit 0
fi

if [[ ! -f "$TARBALL" ]]; then
  echo "Downloading jdtls ${VERSION} (${BUILD})…"
  curl -fsSL "$URL" -o "$TARBALL"
fi

echo "$SHA256  $TARBALL" | shasum -a 256 -c -

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
tar -xzf "$TARBALL" -C "$TMP"

rm -rf "$DEST"
mkdir -p "$DEST"
for item in "$TMP"/*; do
  base="$(basename "$item")"
  if [[ "$base" == config*win* ]]; then
    continue
  fi
  cp -R "$item" "$DEST/"
done

cat > "$DEST/bin/jdtls" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PY="${REAPER_JDTLS_PYTHON:-python3}"
exec "$PY" "$HERE/jdtls.py" "$@"
EOF
chmod +x "$DEST/bin/jdtls"
warm_jdtls_configuration "$DEST"
echo "${VERSION}-${BUILD}" > "$MARKER"
echo "Vendored jdtls ${VERSION} → $DEST"
