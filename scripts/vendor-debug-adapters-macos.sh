#!/usr/bin/env bash
# Download debug adapters for bundling in Reaper.app (DAP stdio plugins).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ARCH="${REAPER_MACOS_ARCH:-$(uname -m)}"
case "$ARCH" in
  arm64) ;;
  x86_64) ;;
  *)
    echo "No bundled debug adapters for macOS arch: $ARCH" >&2
    exit 1
    ;;
esac

DEST="$ROOT/resources/debug-adapters-macos-${ARCH}"
CACHE="$ROOT/resources/.cache"
mkdir -p "$CACHE" "$DEST"

JS_DEBUG_VERSION="${JS_DEBUG_VERSION:-1.117.0}"
DELVE_VERSION="${DELVE_VERSION:-1.26.3}"
CODELLDB_VERSION="${CODELLDB_VERSION:-1.12.2}"
DEBUGPY_VERSION="${DEBUGPY_VERSION:-1.8.14}"
JAVA_DEBUG_VERSION="${JAVA_DEBUG_VERSION:-0.59.0}"

vendor_js_debug() {
  local marker="$DEST/js-debug/src/dapDebugServer.js"
  if [[ -f "$marker" ]]; then
    echo "js-debug DAP already present at $marker"
    return 0
  fi
  local tgz="js-debug-dap-v${JS_DEBUG_VERSION}.tar.gz"
  local url="https://github.com/microsoft/vscode-js-debug/releases/download/v${JS_DEBUG_VERSION}/${tgz}"
  echo "Downloading js-debug DAP ${JS_DEBUG_VERSION}…"
  curl -fsSL "$url" -o "$CACHE/$tgz"
  rm -rf "$DEST/js-debug"
  tar -xzf "$CACHE/$tgz" -C "$DEST"
  [[ -f "$marker" ]] || { echo "missing $marker" >&2; exit 1; }
  echo "js-debug → $marker"
}

vendor_delve() {
  local bin="$DEST/delve/bin/dlv"
  if [[ -x "$bin" ]]; then
    echo "delve already present at $bin"
    return 0
  fi
  local asset
  case "$ARCH" in
    arm64) asset="dlv_${DELVE_VERSION}_darwin_arm64.tar.gz" ;;
    x86_64) asset="dlv_${DELVE_VERSION}_darwin_amd64.tar.gz" ;;
  esac
  local url="https://github.com/go-delve/delve/releases/download/v${DELVE_VERSION}/${asset}"
  echo "Downloading delve ${DELVE_VERSION} (${ARCH})…"
  curl -fsSL "$url" -o "$CACHE/$asset"
  local tmp
  tmp="$(mktemp -d)"
  tar -xzf "$CACHE/$asset" -C "$tmp"
  mkdir -p "$DEST/delve/bin"
  install -m 755 "$tmp/dlv" "$bin"
  rm -rf "$tmp"
  echo "delve → $bin"
}

vendor_codelldb() {
  local bin="$DEST/codelldb/adapter/codelldb"
  if [[ -x "$bin" ]]; then
    echo "codelldb already present at $bin"
    return 0
  fi
  local vsix
  case "$ARCH" in
    arm64) vsix="codelldb-darwin-arm64.vsix" ;;
    x86_64) vsix="codelldb-darwin-x64.vsix" ;;
  esac
  local url="https://github.com/vadimcn/vscode-lldb/releases/download/v${CODELLDB_VERSION}/${vsix}"
  echo "Downloading codelldb ${CODELLDB_VERSION} (${ARCH})…"
  curl -fsSL "$url" -o "$CACHE/$vsix"
  local tmp
  tmp="$(mktemp -d)"
  unzip -q "$CACHE/$vsix" -d "$tmp"
  rm -rf "$DEST/codelldb"
  mkdir -p "$DEST/codelldb"
  cp -R "$tmp/extension/." "$DEST/codelldb/"
  chmod +x "$bin"
  echo "codelldb → $bin"
}

vendor_debugpy() {
  local marker="$DEST/debugpy/debugpy/__init__.py"
  if [[ -f "$marker" ]]; then
    echo "debugpy already present at $DEST/debugpy"
    return 0
  fi
  local py=""
  for candidate in python3 python; do
    if command -v "$candidate" >/dev/null 2>&1; then
      py="$candidate"
      break
    fi
  done
  if [[ -z "$py" ]]; then
    echo "python3 not found — skipping debugpy vendor (install Python to bundle debugpy)" >&2
    return 0
  fi
  echo "Installing debugpy ${DEBUGPY_VERSION} into $DEST/debugpy…"
  rm -rf "$DEST/debugpy"
  mkdir -p "$DEST/debugpy"
  "$py" -m pip install "debugpy==${DEBUGPY_VERSION}" -t "$DEST/debugpy" --upgrade --no-compile -q
  [[ -f "$marker" ]] || { echo "debugpy install failed" >&2; exit 1; }
  echo "debugpy → $DEST/debugpy"
}

vendor_java_debug_plugin() {
  local jar_dir="$DEST/java-debug/server"
  if compgen -G "$jar_dir/com.microsoft.java.debug.plugin-"*.jar >/dev/null 2>&1; then
    echo "java-debug plugin already present in $jar_dir"
    return 0
  fi
  local vsix="vscjava.vscode-java-debug-${JAVA_DEBUG_VERSION}.vsix"
  local url="https://open-vsx.org/api/vscjava/vscode-java-debug/${JAVA_DEBUG_VERSION}/file/${vsix}"
  echo "Downloading java-debug ${JAVA_DEBUG_VERSION}…"
  curl -fsSL "$url" -o "$CACHE/$vsix"
  local tmp
  tmp="$(mktemp -d)"
  unzip -q "$CACHE/$vsix" -d "$tmp"
  rm -rf "$DEST/java-debug"
  mkdir -p "$jar_dir"
  cp -R "$tmp/extension/server/." "$jar_dir/"
  cp -R "$tmp/extension/bundled" "$DEST/java-debug/" 2>/dev/null || true
  echo "java-debug plugin → $jar_dir"
}

vendor_js_debug
vendor_delve
vendor_codelldb
vendor_debugpy
vendor_java_debug_plugin
echo "Debug adapters ready under $DEST"
