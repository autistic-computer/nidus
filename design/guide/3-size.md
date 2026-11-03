# Size and density

## The ladder

| Token | px | Text | Inset | Field inset | Icon | Icon gap | Today |
|---|---|---|---|---|---|---|---|
| `control-xs` | 20 | `caption` | 8 | 6 | 12 | 4 | small chips |
| `control-sm` | 24 | `small` | 8 | 6 | 12 | 4 | `Size::Small` |
| `control-md` | 28 | `body` | 12 | 8 | 14 | 6 | `Size::Medium`, the default |
| `control-lg` | 32 | `body` | 16 | 10 | 16 | 6 | new |
| `control-xl` | 40 | `lead` | 16 | 12 | 16 | 8 | new, for two-line rows |
| `touch` | 44 | `touch-body` | 16 | 12 | 16 | 8 | compact and touch layouts |

Every control at a size shares its row. A button, a select, a segmented control and a tab at `control-md` are all 28px tall with a 12px inset and 14px icons, so they line up in a toolbar without help. Fields keep a smaller inset because their text starts at the edge with nothing before it.

Today chips are 20 and 24, tabs 24, 28 and 36, and menu rows 26. In the draft chips take `control-xs` and `control-sm`, tabs take the ladder (36 becomes 32 or 40) and menu rows become `row`.

## Rows and bars

- `row` (28): list, sidebar and menu rows. Rows touch, and `inset-row` keeps them off the list's edge.
- `bar` (34): the title bar and pane headers, a `control-chrome` (22) plus `inset-chrome` above and below.

## In a line of text

Keycaps, badges and checkboxes take the line height of the text they sit in, never a control size: 16px in `small` text, 18px in `body`. A line never grows to fit them.

## Density

Density is one setting that picks the default control and row height. Text sizes and gaps stay.

| Density | Controls | Rows |
|---|---|---|
| Compact | 24 | 24 |
| Regular | 28 | 28 |
| Comfortable | 32 | 32 |

Touch layouts use `touch` whatever the density. penna has no density setting today; this is for later.

## Width

`measure` (640) is the widest line of reading text, about seventy-five characters of `read-body`. Sidebars are layout geometry, not tokens: 200 to 256px.
