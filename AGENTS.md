# AGENTS.md

Read `~/dev/autistic-computer/.agents/AGENTS.md` first: the rules every autistic-computer repo
shares. This file adds what is specific to penna.

@~/dev/autistic-computer/.agents/AGENTS.md

penna is the widget kit on ala, a maintained fork of gpui-component. Read `UPSTREAM.md` before changing ported code or pulling an
upstream fix.

penna is the only place UI chrome is defined: tokens, surface roles, theme patches and components.
Read `knowledge/designs/kit.md` before adding a component or a role. Two rules from it:

- A component is in the kit when the storybook shows every state of it and it has been looked at
  on each theme in light and dark.
- Break the API freely. The commit that pulls penna into an app fixes that app; no deprecated paths.

## Storybook

`storybook/` is penna's own page and the harness apps build theirs on: an app's storybook crate
depends on `penna-storybook`, hands `start` a `Catalog` of its stories, and builds and serves with
the two scripts here.

```bash
storybook/build.sh      # build penna's page into storybook/dist against ala from GitHub (--release for an optimized build)
storybook/serve.py      # serve storybook/dist on 127.0.0.1:8098 (PORT overrides)
storybook/build.sh --manifest <app storybook>/Cargo.toml --out <dir>   # build an app's catalog
storybook/serve.py <dir> <port>                                         # serve it
node storybook/parity.mjs button --theme macos --mode dark   # score src/button's DOM page against the canvas story (--all, --section s, --update, --check)
```

`#/<story>?solo=1` shows only the section canvases. A component's DOM version lives next to its Rust
(`src/<component>/{index.html, parity.json}`, laid out with `web/story.css`); its elements carry `data-id`
set to the canvas node's author id, which is the id the story passed to the component (`("tab", 3)` reads
`tab-3`). parity matches by id and falls back to `role:name` only for nodes with no id. Results land in
`target/parity/report.html`.

A section scores 100 × √(geometry × pixels): pixels allow 3 levels per channel, so fills and hover tints count,
and the canvas's own ink is the denominator. Steps score only the target's box and need the DOM to change where
the canvas did. A section scores 0 if it holds an `img`, `canvas`, `iframe` or similar, or a `url()` image other
than the kit's icon masks. The DOM page is served only `web/`, `storybook/fonts/` and the HTML, CSS and JS under `src/`, so pages can share a component's files.

How parity drives the pages:

- The DOM page loads with `?theme=<t>&mode=<m>&bare&flat&motion=0` and `prefers-reduced-motion: reduce`
  emulated, so `kit.js` skips the glass lens (an `feImage`, still forbidden otherwise) and motion is off
  through the media query. Leave `--penna-glass` on; pages need no motion switch of their own.
- A step in `parity.json` is `{ "name", "section", "hover" | "press" | "click" | "key", "wait", "keep" }`.
  `hover`, `press` and `click` take an id, `{ "id": "penna-number-decrement", "nth": 3 }` for the nth node
  sharing an id (from 1, in tree order on the canvas and document order in the DOM),
  `{ "at": [x, y] }` for a point in CSS px from the section's top-left corner on both sides, or
  `{ "selector": ".thumb" }` for the centre of a DOM element in the section, whose section-relative point
  the canvas gets too. Point and selector steps score the whole section; id steps score the node's box.
- Each component loads the story fresh, so entity ids don't depend on what ran before. When a step
  leaves its canvas section changed (an open menu, a picked value, a focus ring), both pages reload
  before the next step and the next theme and mode, so no step has to undo itself or another.
  `"keep": true` runs a step on the previous step's state instead; key steps keep by default
  (`"keep": false` starts one fresh). Steps that leave nothing behind keep the fast path: one load
  per component, knobs switched in place. The line after each component counts story loads.
- `--timeout` is per component (default 5 minutes); Ctrl-C and `kill` close Chrome and the servers.

`src/button` is the template to copy. What it learned about matching ala in the DOM:

- Wrap every text run in a span: ala ceils a text box's width to a whole px, so `button.js` does the same,
  or labels drift left along a row.
- `--penna-k` is the smoothing penna draws the role with (`Layer::smoothing`, as `superellipse(log2)`): the
  theme's, capped at 2.5 on rows, and 1 (a plain round) for a full radius. `penna.css` applies it; don't
  hardcode a `corner-shape`. A radius set on one button drops the full-radius round and draws the role's own
  smoothing, which every role exports as `--penna-style-k`: set `--penna-k: var(--penna-style-k)` next to it.
- Focus is the tokens' `data-state="focused"`: on a button it leaves the fill alone and sets `--penna-focus`.
  ala draws the ring 1.5px outside the border box with its own adaptive radius, which a CSS outline can't
  follow, so `button.css` swaps `penna.css`'s outline for a ring that `button.js` sizes. Link and text
  buttons have no role, so their ring takes the root `--penna-focus-neutral`. Only fields change their
  fill or edge on focus, and only their rule also follows `:focus-within`.
- `kit.js` and a page's own script are classic scripts sharing one global scope, so a top-level function
  named like one of kit.js's (`settle`, `round`) replaces it and kit.js stops rounding corners.
- `story.css` takes `data-columns="3"` for a grid that snaps its tracks to device pixels like ala does.
- What is left is text raster: ala snaps glyph baselines and dilates light text, which the DOM can't copy exactly.
