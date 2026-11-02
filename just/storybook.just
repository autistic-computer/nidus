import 'common.just'

set working-directory := ".."

[default]
[private]
default:
    @just --list storybook

[doc("Build the storybook, serve it on 127.0.0.1:8099 and rebuild on save.")]
run:
    @scripts/storybook-dev.sh

[doc("Build the storybook into crates/nid-app/storybook/dist; pass --release for an optimized build.")]
build *args:
    @scripts/build-storybook.sh "$@"

[doc("Serve the last build without rebuilding.")]
serve:
    @libs/penna/storybook/serve.py crates/nid-app/storybook/dist 8099

[doc("Lint the storybook under the app's clippy rules, for the host and for wasm.")]
lint:
    cargo clippy --locked --manifest-path crates/nid-app/storybook/Cargo.toml --all-targets -- -D warnings
    cargo clippy --locked --manifest-path crates/nid-app/storybook/Cargo.toml --target wasm32-unknown-unknown --lib -- -D warnings

[doc("Check the catalog: ids, duplicates and floating entries.")]
test:
    cargo test --locked --manifest-path crates/nid-app/storybook/Cargo.toml
