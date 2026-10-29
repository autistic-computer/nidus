#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app="$here/build/zflow-inject-probe.app"
[[ -d "$app" ]] || { echo "build it first: bash $here/build.sh" >&2; exit 1; }
[[ $# -gt 0 ]] || { echo "usage: $0 <test>... [--secs N]" >&2; exit 64; }

mkdir -p "$here/out"
tag="$(date +%Y%m%d-%H%M%S)-$(printf '%s' "$*" | tr -cs 'a-zA-Z0-9' '-')"
out="$here/out/${tag%-}.txt"
err="$here/out/${tag%-}.err"
: > "$out"
: > "$err"

open -n -W --stdin "${STDIN:-/dev/null}" --stdout "$out" --stderr "$err" "$app" --args "$@"

cat "$out"
if [[ -s "$err" ]]; then
    echo '--- stderr'
    cat "$err"
fi
echo "saved $out"
