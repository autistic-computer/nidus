#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

usage='usage: ./scripts/build-macos-dmg.sh APP OUTPUT.dmg [--sign IDENTITY]'

die() {
    printf 'build-macos-dmg: %s\n' "$*" >&2
    exit 1
}

if [[ "${1:-}" == -h || "${1:-}" == --help ]]; then
    printf '%s\n' "$usage"
    printf 'Write a disk image named zflow holding APP and a link to /Applications.\n'
    printf 'With --sign, sign the image with a secure timestamp (Developer ID for releases).\n'
    exit 0
fi
[[ $# -ge 2 ]] || die "$usage"
app=$1
output=$2
shift 2
sign_identity=''
while [[ $# -gt 0 ]]; do
    case "$1" in
        --sign)
            [[ $# -ge 2 && -n "$2" ]] || die '--sign requires a signing identity'
            sign_identity="$2"
            shift
            ;;
        *) die "unknown argument: $1" ;;
    esac
    shift
done

[[ "$(uname -s)" == Darwin ]] || die 'macOS is required'
[[ -d "$app" && ! -L "$app" ]] || die "missing app bundle: $app"
[[ "$output" == *.dmg ]] || die "output must end in .dmg: $output"
[[ ! -L "$output" && ! -d "$output" ]] || die "refusing to replace: $output"
codesign --verify --strict --deep "$app"

mkdir -p -- "$(dirname -- "$output")"
work="$(mktemp -d "$(dirname -- "$output")/.zflow-dmg.XXXXXXXX")"
mount_dir=''
cleanup() {
    if [[ -n "$mount_dir" ]]; then
        hdiutil detach -quiet "$mount_dir" || hdiutil detach -quiet -force "$mount_dir" || true
    fi
    rm -rf -- "$work"
}
trap cleanup EXIT

mkdir "$work/zflow"
ditto "$app" "$work/zflow/zflow.app"
ln -s /Applications "$work/zflow/Applications"
for attempt in 1 2 3 4; do
    hdiutil create -quiet -ov -volname zflow -srcfolder "$work/zflow" -fs HFS+ \
        -format UDZO -imagekey zlib-level=9 "$work/zflow.dmg" && break
    [[ "$attempt" -lt 4 ]] || die 'hdiutil could not create the image'
    sleep $((attempt * 5))
done
hdiutil verify -quiet "$work/zflow.dmg"

mount_dir="$work/mount"
mkdir "$mount_dir"
hdiutil attach -quiet -readonly -nobrowse -noautoopen -mountpoint "$mount_dir" "$work/zflow.dmg"
codesign --verify --strict --deep "$mount_dir/zflow.app"
[[ "$(readlink "$mount_dir/Applications")" == /Applications ]] || die 'the image has no Applications link'
hdiutil detach -quiet "$mount_dir" || hdiutil detach -quiet -force "$mount_dir"
mount_dir=''

if [[ -n "$sign_identity" ]]; then
    codesign --force --timestamp --sign "$sign_identity" "$work/zflow.dmg"
    codesign --verify --strict "$work/zflow.dmg"
fi
mv -f -- "$work/zflow.dmg" "$output"
printf 'Built %s\n' "$output"
