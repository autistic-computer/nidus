# Layout

Six primitives place everything. Each takes tokens and nothing else, and none draws: they set gaps, insets and widths, and components go inside them.

| Primitive | Does | Takes |
|---|---|---|
| Stack | children in a column | a gap |
| Inline | children in a row, centered on one line, wrapping when asked | a gap |
| Inset | padding around one child | an inset |
| Split | a fixed side beside a flexible main area | the side's width and a gap |
| Center | a column no wider than a measure, centered | a measure |
| Tiles | equal tiles that fill the width | a minimum tile width and a gap |

## Rust (proposed)

```rust
stack(Space::SECTION)
    .child(stack(Space::TIGHT).child(title).child(description))
    .child(inline(Space::RELATED).child(spacer()).child(reset).child(save))
```

## DOM (proposed)

```html
<div class="penna-stack" data-gap="section">
  <div class="penna-stack" data-gap="tight">…</div>
  <div class="penna-inline" data-gap="related">…</div>
</div>
```

Both take a gap level (`tight`, `related`, `group`, `section`) or a space step (`8`), and the same names on both sides.

## Building a screen

The Composition card is a settings window built only from these:

1. The window is a stack with no gap: a bar and a split.
2. The split keeps the sidebar at 200px beside the content.
3. The content is an inset (`inset-pane`) around a stack (`gap-section`) of sections.
4. A section is a stack (`gap-related`) of a heading and a card.
5. A card is an inset (`inset-card`) around a stack (`gap-group`) of settings.
6. A setting is an inline (`gap-related`) with a spacer that pushes its control to the end.

Turn on Show spacing on that card to see each token.

## What stays with the app

Arrangement geometry is the app's: where computers sit on nidus's board, the width a person dragged a sidebar to, terminal cells. Primitives are for the chrome around it.
