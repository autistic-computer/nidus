# Type

A text style is a size, a line height and a weight, always together, and every piece of text has one.

## Interface

| Style | Size | Line | Weight | For |
|---|---|---|---|---|
| `micro` | 10 | 14 | 400 | badges and keycaps in small text |
| `caption` | 11 | 16 | 400 | extra-small controls, chips, hints |
| `small` | 12 | 16 | 400 | small controls, dense lists |
| `body` | 13 | 18 | 400 | controls, rows, interface text: the default |
| `lead` | 14 | 20 | 400 | extra-large controls, short introductions |
| `heading` | 16 | 22 | 600 | pane, card and dialog headings |
| `title` | 20 | 26 | 600 | page titles inside a pane |
| `display` | 26 | 32 | 600 | empty states, onboarding |

These are `TextStyle`'s values today. Every line height is even, so a line box always has a whole-pixel middle.

## The root style

ala gives text with no style a line height of 1.618 times its size: 21.03px for 13px text. Most native penna text has no style, so most native text boxes are 21.03px tall and land between pixels. `Root` sets `body` on every window, the way it already sets the window's corner smoothing, so text with no style gets 13 on 18. This is the largest visual change in the draft: unstyled native text boxes shrink by 3px.

## Reading

For documents and chat: the docs site and markdown in the agent view.

| Style | Size | Line | Weight | Today |
|---|---|---|---|---|
| `read-h1` | 28 | 36 | 600 | 28 on 45.5 |
| `read-h2` | 22 | 28 | 600 | 21 on 34 |
| `read-h3` | 18 | 24 | 600 | 17.5 on 28.5 |
| `read-h4` | 16 | 24 | 600 | 15.75 on 25.5 |
| `read-body` | 16 | 26 | 400 | 16 on 26 |
| `read-small` | 14 | 22 | 400 | new: captions, tables, footnotes |

Today's headings are fractions of the body size on 1.618 lines, so they land on half pixels and sit loosely. The draft keeps their sizes within a pixel and tightens their lines.

## Touch

| Style | Size | Line | Weight | Today |
|---|---|---|---|---|
| `touch-small` | 15 | 20 | 400 | raw 15 |
| `touch-body` | 17 | 22 | 400 | raw 17 |
| `touch-title` | 34 | 40 | 600 | 34 on 41 |

For the compact and touch blocks, where targets are 44px.

## Code

Mono is a family, not a size. Code takes the size and line of the text around it: 13 on 18 in the interface (`code`), 16 on 26 inside reading text. ala can't change the size of one run inside a line, so inline code has to match its paragraph anyway.

## Weights

400 for text, 500 for emphasis in the interface (a selected row, a strong label), 600 for headings. Nothing else.

## Centering

Inter puts the middle of its capitals at the middle of the line box (within 0.01px at 13 on 18), so in the storybook and on the web a control centers its text by centering the line box. The macOS system face sits its capitals 0.33px below the middle at 13 on 18, and Lilex 0.34px. That is two thirds of a device pixel on a 2x screen; correcting it is left for later. Icons beside text are centered on the capitals by `cap_center_drop`, which reads the drop from the font in use.
