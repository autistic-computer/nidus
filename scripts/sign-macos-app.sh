#!/usr/bin/env bash
set -Eeuo pipefail
identity=${1:?usage: sign-macos-app.sh IDENTITY APP [--no-timestamp]}
app=${2:?usage: sign-macos-app.sh IDENTITY APP [--no-timestamp]}
timestamp=--timestamp
if [[ "${3:-}" == --no-timestamp || "$identity" == - ]]; then timestamp=--timestamp=none; fi
options=(--options runtime)
if [[ "$identity" == - ]]; then options=(--options 0); fi
framework="$app/Contents/Frameworks/Sparkle.framework"
[[ -d "$framework" ]] || { printf 'Missing Sparkle framework: %s\n' "$framework" >&2; exit 1; }
codesign --force "${options[@]}" "$timestamp" --sign "$identity" "$framework/Versions/B/XPCServices/Installer.xpc"
codesign --force "${options[@]}" "$timestamp" --preserve-metadata=entitlements --sign "$identity" "$framework/Versions/B/XPCServices/Downloader.xpc"
codesign --force "${options[@]}" "$timestamp" --sign "$identity" "$framework/Versions/B/Autoupdate"
codesign --force "${options[@]}" "$timestamp" --sign "$identity" "$framework/Versions/B/Updater.app"
codesign --force "${options[@]}" "$timestamp" --sign "$identity" "$framework"
codesign --force "${options[@]}" "$timestamp" --identifier io.nid.awdl-daemon --sign "$identity" "$app/Contents/MacOS/nid-awdl-daemon"
codesign --force "${options[@]}" "$timestamp" --identifier io.nid.nid --sign "$identity" "$app"
codesign --verify --strict --deep "$app"
