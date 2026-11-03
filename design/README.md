penna is the widget kit on ala. One set of tokens drives the native build and the DOM build, so a control has the same size, spacing and color in both. This system is penna's foundations: color, type, space, size, corners and depth, and the rules for putting components together.

**Status: draft for review.** The interface type ladder, the 2px space steps, the colors and the shadows are in the code today. The names, the gap and inset levels, the reading and touch ladders, the extra control sizes, density and the layout primitives are proposals. Decisions lists each one and what it changes on screen.

## Three questions

Every element answers three questions, and each answer is a token.

| Question | Tokens | Owned by |
|---|---|---|
| How tall is it? | `control-md`, `row`, `bar` | the element |
| How far is its content from its edge? | `inset-card`, `inset-pane`, a control's inset | the element |
| How far is it from its neighbors? | `gap-tight`, `gap-related`, `gap-group`, `gap-section` | the parent |

- Never give a component an outer margin. The layout that holds it sets the gap.
- Pick a gap by how related two things are, never by eye. A label and its field: `gap-tight`. Buttons in a bar: `gap-related`. Groups in a pane: `gap-group`. Sections of a page: `gap-section`.
- Take every length from a scale. A pixel literal in a component is a bug, except hairlines (0.5px and 1px) and optical offsets computed from font metrics.
- Give every piece of text a style: size, line height and weight together.
- Build screens from the layout primitives (Stack, Inline, Inset, Split, Center, Tiles). The Composition card shows a whole window built that way.

## Names

A token has one name everywhere, and its number is its pixel value.

| Token | CSS | Rust |
|---|---|---|
| `space-8` | `--penna-space-8` | `Space::S8` |
| `gap-related` | `--penna-gap-related` | `Space::RELATED` |
| `inset-card` | `--penna-inset-card` | `Inset::CARD` |
| `body` | `--penna-text-body-size`, `--penna-text-body-line` | `TextStyle::Body` |
| `control-md` | `--penna-control-md` | `Size::Medium` |

Colors keep the CSS names they have today (`--penna-fg-muted`).

## Visual foundations

**Color.** Grounds are `bg`, `panel` and `popover`. Ink is `fg` for text, `fg-muted` for secondary text, `muted` for muted labels, `fg-faint` only for placeholders and disabled text. `accent` marks selection, focus and links; text on it is `on-accent`. `success`, `warning` and `danger` always come with an icon or a word. The values here are the default theme's; the macOS and Yaru themes resolve the same names. Four pairs sit under 4.5:1 and are flagged in their notes: `fg-muted` in dark, `accent` and `success` as text in light, `on-accent` in light. This draft changes no color.

**Type.** Inter in the storybook and on the web, the platform's system face natively, Lilex for code. Interface text uses the Interface group, with `body` (13 on 18) as the default. Documents and chat use Reading, compact and touch layouts use Touch. Code takes the size of the text around it. Weights are 400, 500 for emphasis in the interface and 600 for headings.

**Space.** 2px steps up to 12, then 16, 24, 32, 40 and 64. Use 2, 6 and 10 only inside a control. Layout uses the four gap levels and the five insets.

**Size.** Heights come from one ladder: `control-xs` 20, `control-sm` 24, `control-md` 28, `control-lg` 32, `control-xl` 40 and `touch` 44, plus `row` 28 and `bar` 34. Every control at a size shares one anatomy: height, text style, inset, icon and icon gap. Icons are 12, 14 or 16, by the text beside them.

**Corners.** `radius-theme` is a knob people change. A control's radius is the theme radius capped at `corner-fraction` (45%) of its height. A corner inside another corner is concentric: its radius is the outer radius minus the inset between them.

**Depth.** `shadow-contact` for controls on a surface, `shadow-lifted` for cards that move, `shadow-float` for menus, popovers, dialogs and the window. Themes can turn outlines and shadows off, so never let a shadow alone separate two surfaces.

**Icons.** penna's icon set, adapted from the Solar icons (credited in NOTICE.md). Size an icon by the text beside it and center it on the text's capitals, not its line box: `cap_center_drop` computes the offset from the font's metrics.

## Reading this system

- Composition, Text box, Controls and Audit are pages that explain the rules. Turn on Show spacing to see each gap and inset with its token.
- Stack, Inline, Inset, Split, Center and Tiles are the layout primitives, with Rust and DOM sketches.
- Space, Type, Size and density, Layout and Decisions are the long form of the rules above.
