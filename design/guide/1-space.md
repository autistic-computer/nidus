# Space

## The scale

| Token | px | Where |
|---|---|---|
| `space-0` | 0 | rows that touch |
| `space-2` | 2 | inside controls |
| `space-4` | 4 | `gap-tight`, `inset-row`, icon gap in small controls |
| `space-6` | 6 | inside controls, `inset-chrome` |
| `space-8` | 8 | `gap-related` |
| `space-10` | 10 | inside controls |
| `space-12` | 12 | `inset-card`, medium control inset |
| `space-16` | 16 | `gap-group`, `inset-pane` |
| `space-24` | 24 | dialogs, narrow pages |
| `space-32` | 32 | `gap-section`, `inset-page` |
| `space-40` | 40 | wide document pages |
| `space-64` | 64 | the top of a document |

Steps are 2px apart up to 12, where the eye still tells 10 from 12 inside a control. Past 12 each step is about half again the last, because nobody sees 18 against 20 across a pane.

Today 91% of penna's spacing values (397 of 434) already land on this scale, so the work is naming them, not moving them. The rest (1, 3, 5, 7, 11, 14, 18, 20, 44, 50, 96) are optical nudges and one-offs, and each folds by eye. The Audit card has the counts.

## Gaps belong to the parent

A component never sets space outside its own edge. The layout that holds it sets the gap between its children, so one button sits 8px from its neighbor in a toolbar and 16px from the next group in a form without knowing either.

| Level | Token | px | Between |
|---|---|---|---|
| tight | `gap-tight` | 4 | a label and what it names: a title and its description, a field label and its field |
| related | `gap-related` | 8 | siblings that work together: buttons in a bar, chips, fields in one row |
| group | `gap-group` | 16 | groups inside a pane: form groups, cards in a column |
| section | `gap-section` | 32 | sections of a page |

Each level is twice the one before, so grouping reads at a glance: things that belong together sit closer than things that don't. When two levels both seem right, the grouping is unclear: add a group instead of a value between levels.

Rows in a list touch (`space-0`). Their height keeps the rhythm.

## Insets belong to the container

| Token | px | Inside |
|---|---|---|
| `inset-row` | 4 | a list, around its rows (a theme knob) |
| `inset-chrome` | 6 | a bar, above and below its controls |
| `inset-card` | 12 | cards, menus, popovers |
| `inset-pane` | 16 | panes and dialogs |
| `inset-page` | 32 | document pages |

Controls carry their own inset, set by their size: see Size and density.

## Space you set and space you see

An inset reaches the text's line box, not its letters. Body text (13 on 18) starts its capitals 4.27px inside the box and puts its baseline 4.27px above the bottom. A card with `inset-card` therefore reads about 16px from its edge to the capitals. Keep the token and never nudge the text: when a space looks too big, pick the next step down. The Text box card draws this.

## Choosing a value

1. Inside one control: the control's anatomy (Size and density).
2. Between siblings: how related they are, then that gap level.
3. Content to its container's edge: the container's inset.
4. None of these: the layout is missing a level. Add a group or a container, not a number.
