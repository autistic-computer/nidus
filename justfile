import 'just/common.just'

[doc("Capture app or daemon diagnostics.")]
mod debug 'just/debug.just'
[doc("Apply or check Rust formatting.")]
mod fmt 'just/fmt.just'
[doc("Install a native service.")]
mod install 'just/install.just'
[doc("Package distributable artifacts.")]
mod package 'just/package.just'
[doc("Check or publish a GitHub release.")]
mod release 'just/release.just'
[doc("Run a test suite.")]
mod test 'just/test.just'

[default]
[private]
default:
    @just --list

[doc("Build and open the app.")]
run platform *args: (_platform platform)
    #!/usr/bin/env bash
    set -euo pipefail
    platform="$1"
    shift
    if [[ "$platform" == mac ]]; then
        ./scripts/build-macos-app.sh --debug
        open target/debug/Nidus.app --args "$@"
    else
        cargo build --locked -p nid-app
        cargo run --locked --bin nid -- settings "$@"
    fi

[doc("Package the Mac app, or build the Linux service, its command and the app.")]
build platform *args: (_platform platform)
    #!/usr/bin/env bash
    set -euo pipefail
    platform="$1"
    shift
    if [[ "$platform" == mac ]]; then
        ./scripts/build-macos-app.sh "$@"
    else
        cargo build --locked --bin nid --bin nidd "$@"
        cargo build --locked -p nid-app "$@"
    fi

[doc("Lint Rust code.")]
lint:
    cargo clippy --locked --all-targets -- -D warnings

[doc("Check formatting, lint, and run tests.")]
check: fmt::check lint test::rust test::app test::desktop test::install test::release
