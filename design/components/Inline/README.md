# Inline

Puts its children in a row, centered on one line, a gap apart, and wraps them when asked.

Use it for toolbars, button groups, chips and a setting's label beside its control. Children are centered vertically, so controls of one size share a line and text sits centered on their capitals.

The consumer provides the children, one gap and, for chips or tags, `wrap`. A spacer child pushes what follows it to the end.

```rust
inline(Space::RELATED).child(back).child(forward).child(spacer()).child(share)
inline(Space::RELATED).wrap().children(chips)
```

```html
<div class="penna-inline" data-gap="related">…</div>
<div class="penna-inline" data-gap="related" data-wrap>…</div>
```

- Do keep every control in one inline the same size.
- Don't mix sizes to make one action louder; use a primary look instead.

Proposed: `inline` and the `penna-inline` class do not exist yet. Today this is `h_flex().gap(..)`.
