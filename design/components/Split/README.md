# Split

Puts a fixed side beside a flexible main area.

Use it for a sidebar and its content, a settings label column beside its controls, or a picture beside its caption. The side keeps its width; the main area takes the rest and shrinks first.

The consumer provides the side, the main area, the side's width and a gap (`space-0` for a sidebar that draws its own edge).

```rust
split(px(200.0), Space::S0).aside(sidebar).main(content)
split(px(96.0), Space::GROUP).aside(thumbnail).main(caption)
```

```html
<div class="penna-split" data-gap="group" style="--penna-aside: 96px">…</div>
```

- The side's width is layout geometry, not a space token: sidebars are 200 to 256px.
- Don't put a split inside a narrow popover; stack instead.

Proposed: `split` and the `penna-split` class do not exist yet.
