import 'common.just'

set working-directory := ".."

[default]
[private]
default:
    @just --list release

[doc("Configure Sparkle update signing from a macOS or Linux maintainer computer.")]
setup:
    ./scripts/setup-updates.sh

[doc("Check the committed version and GitHub state without pushing or tagging.")]
check:
    ./scripts/release.sh --dry-run

[doc("Push main, wait for CI, tag the Cargo version, and wait for GitHub publication.")]
publish *args:
    ./scripts/release.sh "$@"
