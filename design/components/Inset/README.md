# Inset

Puts padding around one child, taken from the inset tokens.

Use it for the content of a container: `inset-card` inside cards, menus and popovers, `inset-pane` inside panes and dialogs, `inset-page` on document pages, `inset-row` around list rows and `inset-chrome` inside bars.

The consumer provides one child and one inset. A painted container (a card, a pane) carries its inset itself; a bare Inset is for content that sits on the window.

```rust
inset(Inset::CARD).child(stack(Space::TIGHT).child(title).child(text))
```

```html
<div class="penna-inset" data-inset="card">…</div>
```

- Inset is measured to the text's line box, not its letters: body text adds about 4px before its capitals. Keep the token; don't nudge the text.
- When a container is nested in another, its inset adds up: a card in a pane reads 28px from the pane edge to the card's text. Pick tokens with that sum in mind.

Proposed: `inset`, `Inset::CARD` and the `penna-inset` class do not exist yet.
