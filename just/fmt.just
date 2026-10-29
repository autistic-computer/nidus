import 'common.just'

set working-directory := ".."

[default]
[private]
default:
    @just --list fmt

[doc("Format Rust code.")]
apply:
    cargo fmt --all

[doc("Check formatting without changing files.")]
check:
    cargo fmt --all --check
