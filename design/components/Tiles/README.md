# Tiles

Fills the width with a grid of equal tiles, as many per row as fit.

Use it for screens, panes or images shown as cards. Tiles never go below the minimum width; extra room widens every tile equally.

The consumer provides the tiles, a minimum tile width and a gap.

```rust
tiles(px(140.0), Space::RELATED).children(panes)
```

```html
<div class="penna-tiles" data-gap="related" style="--penna-tile: 140px">…</div>
```

- Use `gap-related` between tiles and `gap-group` between a grid and what follows it.

Proposed: `tiles` and the `penna-tiles` class do not exist yet.
