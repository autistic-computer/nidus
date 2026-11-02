#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TARGET_DIR="$ROOT/target/storybook" exec "$ROOT/libs/penna/storybook/build.sh" \
    --manifest "$ROOT/crates/nid-app/storybook/Cargo.toml" --out "$ROOT/crates/nid-app/storybook/dist" "$@" -- --locked
