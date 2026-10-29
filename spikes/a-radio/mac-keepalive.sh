#!/usr/bin/env bash

set -euo pipefail
export LC_ALL=C

TARGET=${1:?usage: sudo ./mac-keepalive.sh <linux-host> [seconds-per-arm]}
ARM_SECS=${2:-60}
RATES=(0 10 20 50 100)

PID=""
cleanup() { [[ -n "$PID" ]] && kill "$PID" 2>/dev/null || true; }
trap cleanup EXIT

echo "awdl0 status: $(ifconfig awdl0 2>/dev/null | grep -o 'status: .*' || echo unknown)"

for RATE in "${RATES[@]}"; do
  echo "ARM ${RATE}Hz START $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  if [[ "$RATE" != "0" ]]; then
    INT=$(awk "BEGIN{printf \"%.3f\", 1/$RATE}")
    ping -q -i "$INT" "$TARGET" >/dev/null 2>&1 &
    PID=$!
    sleep 2
    if ! kill -0 "$PID" 2>/dev/null; then
      echo "ARM ${RATE}Hz FAILED: keepalive ping exited (need sudo for sub-0.1s intervals)"
      PID=""
      continue
    fi
  fi
  sleep "$ARM_SECS"
  if [[ -n "$PID" ]]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
    PID=""
  fi
  echo "ARM ${RATE}Hz END $(date -u +%Y-%m-%dT%H:%M:%SZ)"
done
echo "all arms done"
