# AGENTS.md

Read `~/dev/autistic-computer/.agents/AGENTS.md` first: the rules every autistic-computer repo
shares. This file adds what is specific to nidus.

@~/dev/autistic-computer/.agents/AGENTS.md

nidus (nid, repo autistic-computer/nidus) shares keyboard, pointer and clipboard between computers over QUIC. It builds on ala and penna from autistic-computer, which live in `libs/` as git subtrees.

## Storybook

`crates/nid-app/storybook` (crate `nid-storybook`) puts the settings app on one web page: the first run, the computers page, the card, settings, the menu bar panel, the desk, the titlebar and the shared widgets, each in its states, on penna's storybook harness in `libs/penna/storybook`. It is its own cargo workspace with its own `Cargo.lock`, and it sits under nid-app so nid-app's `clippy.toml` covers story code.

- Stories live in `src/stories/`, one file per story, listed in `stories/mod.rs` with the sections that float (dialogs, toasts, menus). Fixtures live in `src/fixtures.rs`.
- Sections draw the real views (`NidWindow`, `Panel`, `Board`) on a model from `Model::fixture`. Fixtures are built from the app's own types (`Snapshot`, `Peer`, `Layout`, `Unplaced`, `Health`) and never reach the daemon, the network or the filesystem: everything renders in a browser tab from constants.
- `just storybook run` builds, serves on http://127.0.0.1:8099 and rebuilds on save. `just storybook build` (pass `--release` for an optimized build), `just storybook serve`, `just storybook lint` (host and wasm clippy) and `just storybook test` (the catalog check) do one step each.
