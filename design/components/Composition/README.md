# Composition

A settings window built only from tokens and the layout primitives, with every gap and inset labeled.

Read it with Show spacing on. Each colored band is one token:

1. The window is a stack with no gap: a `bar` and a split.
2. The bar is an inline with `gap-related`, inset by `inset-chrome` around its `control-chrome` buttons.
3. The split keeps the sidebar at 200px; the sidebar insets its `row`s by `inset-row`.
4. The content is inset by `inset-pane` around a stack with `gap-section` between the header and each section.
5. The header is a stack with `gap-tight`: a `title` over a `body` description.
6. Each section is a stack with `gap-related`: a `heading` over a card.
7. A card is inset by `inset-card` around a stack with `gap-group` between settings.
8. A setting is an inline with `gap-related` and a spacer that pushes its control to the end.

Drawn from the tokens to show the geometry; the real widgets live in the storybook.
