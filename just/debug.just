import 'common.just'

set working-directory := ".."

[default]
[private]
default:
    @just --list debug

[doc("Run Mac app diagnostics and save output under target/logs.")]
mac *args:
    @just debug _app mac "$@"

[doc("Run Linux desktop-agent diagnostics and save output under target/logs.")]
linux *args:
    @just debug _app linux "$@"

[doc("Enable Linux daemon diagnostics until reboot; requires sudo and restarts the service.")]
daemon: (_platform "linux")
    #!/usr/bin/env bash
    set -euo pipefail
    sudo install -d -m 0755 /run/systemd/system/nidd.service.d
    printf '[Service]\nEnvironment="RUST_LOG=warn,nid=debug"\n' | sudo tee /run/systemd/system/nidd.service.d/debug.conf >/dev/null
    sudo systemctl daemon-reload
    sudo systemctl restart nidd.service
    printf 'Daemon debug logging enabled until reboot. Read with: journalctl -u nidd -f -o short-iso-precise\n'

[private]
_app platform *args: (_platform platform)
    #!/usr/bin/env bash
    set -euo pipefail
    umask 077
    platform="$1"
    shift
    mkdir -p target/logs
    logfile="target/logs/nid-$platform-$(date -u +%Y%m%dT%H%M%SZ)-$$.log"
    printf 'Saving diagnostics to %s/%s\n' "$PWD" "$logfile"
    if [[ "$platform" == mac ]]; then
        ./scripts/build-macos-app.sh --debug
        RUST_LOG=warn,nid=debug target/debug/Nidus.app/Contents/MacOS/nid-app "$@" 2>&1 | tee "$logfile"
    else
        RUST_LOG=warn,nid=debug cargo run --locked --bin nid -- desktop-agent "$@" 2>&1 | tee "$logfile"
    fi
