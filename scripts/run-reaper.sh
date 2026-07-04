#!/usr/bin/env bash
# Build Reaper.app, launch it, wait for quit, then shut down the Java/LSP stack cleanly.
#
# Usage:
#   ./scripts/run-reaper.sh
#     Full build (regression + release app), open dist/Reaper.app, wait, cleanup.
#
#   ./scripts/run-reaper.sh --fast
#     Skip editor regression during the pre-launch build.
#
#   ./scripts/run-reaper.sh --skip-build
#     Launch the existing dist/Reaper.app without rebuilding.
#
#   ./scripts/run-reaper.sh --rebuild-on-exit
#     Run a second full build after Reaper quits (useful before packaging or CI smoke).
#
#   ./scripts/run-reaper.sh --fast --rebuild-on-exit
#     Fast pre-launch build, full cleanup on exit, fast post-exit build.
#
# Environment:
#   REAPER_SKIP_REGRESSION=1   Same as --fast (honored by build-macos-app.sh).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/dist/Reaper.app"
REAPER_BIN="$APP/Contents/MacOS/reaper"

SKIP_BUILD=0
FAST_BUILD=0
REBUILD_ON_EXIT=0

usage() {
  sed -n '2,21p' "$0" | sed 's/^# \?//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --fast) FAST_BUILD=1; shift ;;
    --rebuild-on-exit) REBUILD_ON_EXIT=1; shift ;;
    *) echo "Unknown option: $1" >&2; usage 1 ;;
  esac
done

reaper_pids() {
  pgrep -f 'dist/Reaper.app/Contents/MacOS/reaper' 2>/dev/null \
    || pgrep -f 'Reaper.app/Contents/MacOS/reaper' 2>/dev/null \
    || true
}

reaper_running() {
  [[ -n "$(reaper_pids | head -1)" ]]
}

build_reaper() {
  local label="${1:-Build}"
  echo "== $label =="
  if [[ "$FAST_BUILD" == 1 || "${REAPER_SKIP_REGRESSION:-}" == "1" ]]; then
    export REAPER_SKIP_REGRESSION=1
  fi
  "$ROOT/scripts/build-macos-app.sh"
}

quit_reaper() {
  if ! reaper_running; then
    return 0
  fi
  local pid
  pid="$(reaper_pids | head -1)"
  echo "Quitting Reaper (pid ${pid})…"
  osascript -e 'tell application id "dev.reaper.app" to quit' 2>/dev/null \
    || osascript -e 'tell application "Reaper" to quit' 2>/dev/null \
    || kill "$pid" 2>/dev/null \
    || true
  local i
  for i in $(seq 1 30); do
    reaper_running || return 0
    sleep 0.5
  done
  echo "Force-quitting Reaper…"
  while read -r pid; do
    [[ -n "$pid" ]] && kill -TERM "$pid" 2>/dev/null || true
  done < <(reaper_pids)
  sleep 1
  while read -r pid; do
    [[ -n "$pid" ]] && kill -KILL "$pid" 2>/dev/null || true
  done < <(reaper_pids)
}

shutdown_stack() {
  echo "== Shutdown: Reaper language-server stack =="
  quit_reaper

  local reaper_pid
  reaper_pid="$(reaper_pids | head -1 || true)"
  if [[ -n "$reaper_pid" ]]; then
    local child_pids
    child_pids="$(ps -o pid=,ppid= -ax 2>/dev/null | awk -v rp="$reaper_pid" '$2==rp {print $1}' || true)"
    if [[ -n "$child_pids" ]]; then
      echo "Stopping Reaper child processes…"
      while read -r cp; do
        [[ -n "$cp" ]] && kill -TERM "$cp" 2>/dev/null || true
      done <<< "$child_pids"
      sleep 1
    fi
  fi

  if pgrep -f 'jdtls|eclipse.jdt.ls' >/dev/null 2>&1; then
    echo "Stopping jdtls…"
    pkill -TERM -f 'jdtls|eclipse.jdt.ls' 2>/dev/null || true
    sleep 1
    pkill -KILL -f 'jdtls|eclipse.jdt.ls' 2>/dev/null || true
  fi
  if pgrep -f '[/ ]javac ' >/dev/null 2>&1; then
    echo "Stopping stray javac diagnostics…"
    pkill -TERM -f '[/ ]javac ' 2>/dev/null || true
    sleep 1
    pkill -KILL -f '[/ ]javac ' 2>/dev/null || true
  fi

  if reaper_running; then
    echo "Warning: Reaper still running after shutdown cleanup." >&2
    return 1
  fi
  echo "Shutdown complete."
}

launch_reaper() {
  if [[ ! -x "$REAPER_BIN" ]]; then
    echo "Missing $REAPER_BIN — run without --skip-build first." >&2
    exit 1
  fi
  if reaper_running; then
    echo "Reaper is already running; quit it first or rerun with shutdown only." >&2
    exit 1
  fi
  echo "== Launch =="
  echo "Opening $APP (waiting until you quit Reaper)…"
  open -W "$APP"
}

on_exit() {
  local code=$?
  trap - EXIT INT TERM
  shutdown_stack || true
  if [[ "$REBUILD_ON_EXIT" == 1 ]]; then
    build_reaper "Post-exit build" || code=$?
  fi
  exit "$code"
}

trap on_exit EXIT INT TERM

if [[ "$SKIP_BUILD" == 0 ]]; then
  quit_reaper
  build_reaper "Pre-launch build"
else
  quit_reaper
fi

launch_reaper
