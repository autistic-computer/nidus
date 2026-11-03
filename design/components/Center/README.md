# Center

Keeps a column no wider than a measure and centers it in its container.

Use it for reading text: the docs site, long descriptions, empty states. `measure` (640px) holds about seventy-five characters of `read-body`, past which lines get hard to follow.

The consumer provides the content and, rarely, another measure.

```rust
center(Size::MEASURE).child(markdown)
```

```html
<div class="penna-center">…</div>
```

- Don't center interface panes; tools use the whole width.

Proposed: `center` and the `penna-center` class do not exist yet.
