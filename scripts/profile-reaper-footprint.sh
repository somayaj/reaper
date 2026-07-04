#!/usr/bin/env bash
# Sample CPU and memory for the Reaper stack (app, server, jdtls, javac, node bridge).
#
# Usage:
#   ./scripts/profile-reaper-footprint.sh --compare
#     Interactive: sample idle, then typing (press Enter between phases).
#
#   ./scripts/profile-reaper-footprint.sh --seconds 30 --phase idle
#     Single timed sample with a label.
#
#   ./scripts/profile-reaper-footprint.sh --watch
#     Stream samples until Ctrl-C.
#
# Requirements: macOS ps(1). Reaper should be running before you start.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SECONDS_PER_PHASE=20
INTERVAL=1
PHASE="sample"
MODE="once"
OUT=""
WATCH=0

usage() {
  sed -n '2,14p' "$0" | sed 's/^# \?//'
  exit "${1:-0}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage 0 ;;
    --seconds) SECONDS_PER_PHASE="${2:?}"; shift 2 ;;
    --interval) INTERVAL="${2:?}"; shift 2 ;;
    --phase) PHASE="${2:?}"; shift 2 ;;
    --out) OUT="${2:?}"; shift 2 ;;
    --compare) MODE="compare"; shift ;;
    --watch) WATCH=1; MODE="watch"; shift ;;
    *) echo "Unknown option: $1" >&2; usage 1 ;;
  esac
done

if ! [[ "$SECONDS_PER_PHASE" =~ ^[0-9]+$ && "$INTERVAL" =~ ^[0-9]+$ && "$INTERVAL" -ge 1 ]]; then
  echo "Invalid --seconds or --interval" >&2
  exit 1
fi

if [[ -n "$OUT" && ! -f "$OUT" ]]; then
  echo "timestamp,phase,bucket,pid,rss_kb,cpu_pct" >"$OUT"
fi

# Classify a process into a bucket from ps args (macOS).
classify_bucket() {
  local args="$1"
  local comm="$2"
  local lower
  lower="$(printf '%s' "$args" | tr '[:upper:]' '[:lower:]')"

  if [[ "$lower" == *"jdt.ls"* || "$lower" == *"jdtls"* || "$lower" == *"eclipse.jdt"* ]]; then
    printf 'jdtls\n'
  elif [[ "$lower" == *"javac"* ]]; then
    printf 'javac\n'
  elif [[ "$comm" == "reaper" || "$lower" == *"/macos/reaper"* || "$lower" == *"/reaper.app/contents/macos/reaper"* ]]; then
    printf 'reaper_server\n'
  elif [[ "$comm" == "Reaper" ]]; then
    printf 'reaper_app\n'
  elif [[ "$lower" == *"cursor-bridge"* || ( "$comm" == "node" && "$lower" == *"reaper"* ) ]]; then
    printf 'node_bridge\n'
  elif [[ "$comm" == *"WebKit"* || "$comm" == *"Helper"* && "$lower" == *"reaper"* ]]; then
    printf 'webkit\n'
  elif [[ "$comm" == "java" || "$comm" == "java"* ]]; then
    printf 'java_other\n'
  elif [[ "$lower" == *"reaper"* ]]; then
    printf 'reaper_other\n'
  else
    printf 'other\n'
  fi
}

# Return 0 if any Reaper-related process is visible.
has_reaper_processes() {
  ps -axo comm=,args= 2>/dev/null | while IFS= read -r line; do
    local comm="${line%% *}"
    local args="${line#"$comm"}"
    args="${args# }"
    local bucket
    bucket="$(classify_bucket "$args" "$comm")"
    if [[ "$bucket" != "other" ]]; then
      exit 0
    fi
  done
  return 1
}

sample_once() {
  local phase="$1"
  local now bucket comm args pid pcpu rss
  now="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    pid="${line%% *}"
    rest="${line#"$pid"}"
    rest="${rest# }"
    pcpu="${rest%% *}"
    rest="${rest#"$pcpu"}"
    rest="${rest# }"
    rss="${rest%% *}"
    rest="${rest#"$rss"}"
    rest="${rest# }"
    comm="${rest%% *}"
    args="${rest#"$comm"}"
    args="${args# }"

    bucket="$(classify_bucket "$args" "$comm")"
    [[ "$bucket" == "other" ]] && continue

    printf '%s\n' "$now,$phase,$bucket,$pid,$rss,$pcpu"
    if [[ -n "$OUT" ]]; then
      printf '%s\n' "$now,$phase,$bucket,$pid,$rss,$pcpu" >>"$OUT"
    fi
  done < <(ps -axo pid=,pcpu=,rss=,comm=,args= 2>/dev/null || true)
}

run_phase() {
  local phase="$1"
  local duration="$2"
  local end=$((SECONDS + duration))
  local n=0

  echo "── phase: $phase (${duration}s, every ${INTERVAL}s) ──"
  while (( SECONDS < end )); do
    sample_once "$phase"
    n=$((n + 1))
    if (( WATCH == 0 )); then
      printf '  sample %d/%d\r' "$n" "$((duration / INTERVAL))"
    fi
    sleep "$INTERVAL"
  done
  echo
}

summarize_csv() {
  awk -F, '
    NR == 1 && $1 ~ /^timestamp/ { next }
    $3 != "" && $3 != "other" {
      key = $2 SUBSEP $3
      rss[key] += $5
      cpu[key] += $6
      cnt[key]++
      if ($5 > maxrss[key]) maxrss[key] = $5
      if ($6 > maxcpu[key]) maxcpu[key] = $6
      phases[$2] = 1
      buckets[$3] = 1

      sample = $1 SUBSEP $2
      sample_rss[sample] += $5
      sample_cpu[sample] += $6
      sample_cnt[sample]++
    }
    END {
      printf "\nSummary (RSS MB, CPU %% — avg / max per bucket):\n"
      printf "%-12s %-16s %8s %8s %8s %8s\n", "phase", "bucket", "avg_mb", "max_mb", "avg_cpu", "max_cpu"
      for (p in phases) {
        for (b in buckets) {
          key = p SUBSEP b
          if (cnt[key] > 0) {
            printf "%-12s %-16s %8.1f %8.1f %8.1f %8.1f\n", p, b, (rss[key]/cnt[key])/1024, maxrss[key]/1024, cpu[key]/cnt[key], maxcpu[key]
          }
        }
      }
      for (p in phases) {
        total_rss = 0
        total_cpu = 0
        total_n = 0
        max_stack_rss = 0
        max_stack_cpu = 0
        for (s in sample_rss) {
          split(s, parts, SUBSEP)
          if (parts[2] != p) continue
          total_rss += sample_rss[s]
          total_cpu += sample_cpu[s]
          total_n++
          if (sample_rss[s] > max_stack_rss) max_stack_rss = sample_rss[s]
          if (sample_cpu[s] > max_stack_cpu) max_stack_cpu = sample_cpu[s]
        }
        if (total_n > 0) {
          printf "\nPhase %-8s stack total: avg %.1f MB RSS (max %.1f MB), avg %.1f%% CPU (max %.1f%%)\n", p, (total_rss/total_n)/1024, max_stack_rss/1024, total_cpu/total_n, max_stack_cpu
        }
      }
    }
  '
}

TMP="$(mktemp "${TMPDIR:-/tmp}/reaper-footprint.XXXXXX")"
trap 'rm -f "$TMP"' EXIT

echo "Reaper footprint profiler"
echo "Root: $ROOT"
if ! has_reaper_processes; then
  echo "Warning: no Reaper/jdtls/java processes found. Launch Reaper.app or \`cargo run\` first." >&2
fi

case "$MODE" in
  compare)
    echo
    echo "1) Leave Reaper idle (no typing). Press Enter to sample idle…"
    read -r
    run_phase idle "$SECONDS_PER_PHASE" >>"$TMP"
    echo "2) Type steadily in the editor for the next ${SECONDS_PER_PHASE}s…"
    run_phase typing "$SECONDS_PER_PHASE" >>"$TMP"
    ;;
  watch)
    echo "Watching until Ctrl-C (phase=watch)…"
    while true; do
      sample_once watch >>"$TMP"
      sleep "$INTERVAL"
    done
    ;;
  once)
    run_phase "$PHASE" "$SECONDS_PER_PHASE" >>"$TMP"
    ;;
esac

cat "$TMP" | summarize_csv

if [[ -n "$OUT" ]]; then
  echo
  echo "CSV appended to: $OUT"
fi

echo
echo "Tip: run twice with different --phase values, or use --compare for idle vs typing."
