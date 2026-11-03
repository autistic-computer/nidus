# Stack

Puts its children in a column, a gap apart, and draws nothing itself.

Use it for anything that reads top to bottom: a form, a pane's sections, a title over its description. The gap says how related the children are: `gap-tight` for a label and its field, `gap-related` for siblings that work together, `gap-group` for groups, `gap-section` for sections.

The consumer provides the children and one gap. Children stretch to the stack's width unless they size themselves.

```rust
stack(Space::GROUP)
    .child(stack(Space::TIGHT).child(label).child(field))
    .child(inline(Space::RELATED).child(spacer()).child(cancel).child(create))
```

```html
<div class="penna-stack" data-gap="group">
  <div class="penna-stack" data-gap="tight">…</div>
</div>
```

- Do nest stacks to show grouping: a tight stack inside a group stack.
- Don't give a child a margin to push it away; wrap the children that belong together in their own stack.
- Rows in a list touch: use `space-0` and let the row height keep the rhythm.

Proposed: `stack`, `Space::TIGHT` and the `penna-stack` class do not exist yet. Today this is `v_flex().gap(..)`.
