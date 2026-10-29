#!/usr/bin/env bash
set -Eeuo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
output="$root/target/dist"
zip="$output/zflow@demfabris.shell-extension.zip"
files=(client.js extension.js indicator.js metadata.json shell.js)
command -v gnome-extensions >/dev/null 2>&1 || { echo 'Packing needs gnome-extensions from GNOME Shell.' >&2; exit 1; }
mkdir -p "$output"
gnome-extensions pack "$root/packaging/gnome-extension" --force --out-dir="$output" \
    --extra-source=shell.js --extra-source=indicator.js --extra-source=client.js
packed=$(unzip -Z1 "$zip" | sort)
if [[ "$packed" != "$(printf '%s\n' "${files[@]}")" ]]; then
    printf 'Unexpected files in %s:\n%s\n' "$zip" "$packed" >&2
    exit 1
fi
printf 'Created %s\n' "$zip"
