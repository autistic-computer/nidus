---
type: Design Plan
title: "Foundations: penna's design language"
description: "The spec for penna's design language and the source of truth for every penna screen: principles, the rules and scales (space, type, sizes, color, corners, depth, focus, motion, layout), and each component's purpose, anatomy, states, behavior, content and placement. The docs site draws it with real components."
status: Built (2026-11-04, steps 1 to 9)
resource: src/foundation/space.rs
tags:
- ui
- design-system
- tokens
- design-plan
timestamp: 2026-11-03T00:00:00Z
---

# Foundations

penna's design language: the principles, the rules and scales, and how each component looks and behaves. It is the source of truth for every penna screen and the spec agents read before building or changing UI. The docs site draws it: every section here has a page there made of real penna components (`docs/pages/principles.md`, `foundations.md`, `color.md`, `typography.md`, `spacing.md`, `sizes.md`, and a page per component), so people can see it. When a page and this file disagree, this file wins and the page gets fixed; when the code and this file disagree, the code gets fixed, and each component's Status says where it stands.

Values are design pixels: one unit is 1/16 of the root rem, so the interface font size and window zoom scale everything together. `rems_from_px` converts.

Status marks: **built** is in the code; **step N** is planned in the work order at the end.

## Principles

penna draws zz and nidus: tools people keep open all day, mostly driven from the keyboard, where the work (a terminal, a transcript, a document, the computers on a desk) matters more than the interface around it. Six principles follow from that. The rules and scales below are how they are kept; when a case is not covered, decide it by these.

1. **The keyboard comes first.** Every action has a keyboard path, and the interface shows it: shortcuts sit beside menu rows and in which-key, and a focus ring appears whenever the keyboard moves focus. Pointer and touch reach the same actions, sized for them. So: every control is focusable unless it is disabled, activates with Space and Enter, and shows its shortcut where it is offered.
2. **Calm, dense screens.** Most text on a screen is one size (`body`); weight and a few larger steps carry the hierarchy, and color is kept for meaning. Spacing follows relationships, so a dense screen still groups clearly, and density is a setting, never a squeeze. So: two or three text styles in a view, gaps picked by relationship, nothing added for decoration.
3. **One highlight.** One color marks what is on, picked or in focus. In the default look it is the ink itself, so the interface stays black and white until someone picks an accent. Status colors mean status and always come with an icon or a word. So: at most one primary action in a view, and no color that only decorates.
4. **The work leads, chrome recedes.** Bars, headers and toolbars use quiet controls (no fill at rest), muted labels and the smaller rungs, so the eye goes to the content. Text people read (documents, chat) is larger and more open than text people scan (`read-body` against `body`). So: chrome never competes with content for the strongest ink, size or color.
5. **Every state shows.** A control always tells what it is doing: rest, hover, pressed, focused, selected, disabled, loading or in error. Feedback comes in the same frame as the input. Motion is short (120 to 220ms), shows where something came from or went, and turns off with reduced motion. So: no state without a look, and no animation that only decorates.
6. **One kit everywhere.** The same components draw native windows on every desktop, the phone and the web, in three looks, light and dark. A look changes materials, corners and faces, never sizes, spacing or behavior. So: a component that only works in one look, one mode or one platform has a bug.

## Rules

1. Take every length from a scale in this file. A pixel literal in a component is a bug, except hairlines (0.5 and 1), optical offsets (computed from font metrics with `cap_center_drop`, or a glyph's own kerning, such as the control mark pulled toward the key it modifies on a touch key row), and the geometry listed under Space as staying in pixels.
2. Every element answers three questions, each with a token: its height (Size), its inset (padding, owned by the element), and the gap to its neighbors (owned by the parent).
3. Components never set an outer margin. The layout that holds them sets the gap.
4. Pick a gap by relationship, not by eye: tight for a label and what it names, related for siblings that work together, group for groups inside a pane, section for the parts of a page. When two levels both seem right, the grouping is unclear: add a group, never a value between levels.
5. Every piece of text has a text style: size, line height and weight together. Never set a size without its line height.
6. Every control in one row has the same size.
7. A token has one name everywhere. Its number is its pixel value. Rust and CSS spell it the same way (see Names).
8. Color comes from the theme's roles and looks, never a literal. Status colors always come with an icon or a word.

## Names

| Token | CSS | Rust | Status |
|---|---|---|---|
| `space-8` | `--penna-space-8` | `Space::S8` | built |
| `gap-related` | `--penna-gap-related` | `Space::GAP_RELATED` | built |
| `inset-card` | `--penna-inset-card` | `Space::INSET_CARD` | built |
| text style `body` | `--penna-text-body-size`, `-line`, `-weight` | `TextStyle::Body` | built |
| `control-md` | `--penna-control-md` | `Size::Medium` | built |
| colors | `--penna-fg`, `--penna-fg-muted`, ... | `cx.theme()` fields and looks | built |

Until step 1, `Space` variants were named in 2px steps (`Space::S4` was 8px), while ala's helpers count 4px steps (`gap_4()` is 16px). Don't use ala's spacing helpers (`gap_2()`, `px_3()`, ...) in penna; use `Space`.

## Space

Scale: 2, 4, 6, 8, 10, 12, 16, 24, 32, 40, 64.

- 2, 6 and 10 only inside a control (icon gaps, field insets, segment padding), with two exceptions: 6 is also `inset-chrome`, around the controls in a bar, and 2 separates the rows of an inset list.
- Layout uses 4, 8, 12 and 16 upward.
- 1, 3, 5, 7, 9, 11, 14, 18, 20, 22 and 28 are off the scale and fold by eye to a neighbor. 96 folds to 64.

Gaps, set by the parent:

| Token | px | Between |
|---|---|---|
| `gap-tight` | 4 | a label and what it names: a title and its description, a field label and its field |
| `gap-related` | 8 | siblings that work together: buttons in a bar, chips, a heading and its card |
| `gap-group` | 16 | groups inside a pane: form groups, cards in a column, settings in a card |
| `gap-section` | 32 | sections of a page |

Insets, set by the container:

| Token | px | Inside |
|---|---|---|
| `inset-chrome` | 6 | bars and pane headers, around their controls (today `CHROME_GAP`) |
| `inset-card` | 12 | cards and popovers |
| `inset-pane` | 16 | panes and dialogs |
| `inset-page` | 32 | document pages |
| row inset | theme | lists and menus around their rows: `theme.row_inset` at the sides, a theme knob (4, or 5 in the default theme); menus add `space-4` above and below |

Rows in a plain list (row inset 0) touch: their height keeps the rhythm. In an inset list (`theme.inset_rows()`, a row inset above 0, as in every shipped theme) rows sit `space-2` apart, so their rounded highlights never merge: menus, select lists, settings navigation and the touch which-key list. `list_inset` sets both the side inset and that gap. The docs navigation keeps its rows `space-2` apart in every theme.

Components take every padding, gap and margin from `Space` (built in step 6), so spacing scales with the rem like type does. Settings pages are a pane (`inset-pane` 16 around the page, `gap-group` 16 between groups); a settings row is inside a card (`inset-card` 12 all round, title and description `gap-tight` 4 apart). Dialogs are panes too (`inset-pane` 16). Sidebar sections are `gap-group` 16 apart. Which-key's columns and lines are `gap-group` 16 apart inside an `inset-card` sheet. What stays in pixels: drawing geometry inside a fixed-size picture (Appearance's theme miniatures), focus halos and swatch rings that feed radius math, window snap margins, and the sidebar title bar's insets, alignment geometry computed from the rows' action column (and the traffic lights on macOS) so the window's buttons line up over the rows.

An inset reaches the text's line box, not its letters. In `body` (13 on 18) capitals start 4.27px inside the box and the baseline sits 4.27px above its bottom, so an `inset-card` reads as about 16px to the capitals. Keep the token; never nudge text natively. An HTML twin may move a text run by a pixel at most (`position: relative; top`, usually ±0.5px) where ala rounds the baseline differently from the browser, so parity holds; that is the only nudge, and it lives in the twin's CSS.

## Type

Faces: the platform system face natively (`.SystemUIFont`), Inter Variable in the storybook and on the web. The code face is the platform's natively (`Theme::mono_font_family`: Menlo on macOS and iOS, Consolas on Windows, DejaVu Sans Mono elsewhere) and Lilex in the storybook and on the web. Weights: 400 text, 500 emphasis in the interface (`strong`), 600 headings. No other weights.

Interface (`TextStyle`, built):

| Style | Size | Line | Weight |
|---|---|---|---|
| `micro` | 10 | 14 | 400 |
| `caption` | 11 | 16 | 400 |
| `small` | 12 | 16 | 400 |
| `body` | 13 | 18 | 400 |
| `lead` | 14 | 20 | 400 |
| `heading` | 16 | 22 | 600 |
| `title` | 20 | 26 | 600 |
| `display` | 26 | 32 | 600 |
| `mono` | 13 | 18 | 400, code face |

Reading, for documents and chat (`TextStyle::ReadH1` to `ReadH5` and `ReadBody`, built). Sizes reuse the interface sizes with lines made for paragraphs:

| Style | Size | Line | Weight | Before step 2 |
|---|---|---|---|---|
| `read-h1` | 26 | 32 | 600 | 28 on 45.5, bold |
| `read-h2` | 20 | 28 | 600 | 21 on 34 |
| `read-h3` | 16 | 24 | 600 | 17.5 on 28.5 |
| `read-h4` | 14 | 22 | 600 | 15.75 on 25.5 |
| `read-h5` (h5 and h6) | 13 | 20 | 600 | 14 on 22.5 |
| `read-body` | 14 | 22 | 400 | 16 on 26 |
| `read-small` | 13 | 20 | 400 | planned, for captions and table cells |

Reading rhythm (built): blocks (paragraphs, lists, quotes, code, tables) sit `space-12` apart (`TextViewStyle::paragraph_gap`); a heading has `space-24` above it (h1 and h2) or `space-16` (h3 to h6) and `space-8` below. The first block has nothing above it. Bold in reading text is 600.

`TextView` sets `read-body` on itself (`TextViewStyle::text`, default `Some(TextStyle::ReadBody)`). `None` makes it inherit its container's text style instead, without the measure: zz's agent transcript sets `read-body` on its containers and lets its markdown inherit it. Headings always take the reading ladder. A task list's checkbox is as tall as the font and sits in a box one line tall.

Touch, for the compact, touch and sheet blocks (`TextStyle::TouchSmall`, `TouchBody`, `TouchTitle`, built): `touch-small` 15 on 20, `touch-body` 17 on 22, `touch-title` 34 on 40, weights 400, 400, 600; strong is 500 for the first two.

Component text (step 3, built): list and tree rows `lead`, centered in the row height (which follows density and the rem), their details `small`; sidebar headings strong `small`, under the `body` rows they head; menu rows `small` with 12 icons, and `body` with 14 icons on a 44 row under a coarse pointer; their shortcuts `small`; menu section labels `caption`. Controls take the text of their size from `Size::control_text()`, the Text column of the control ladder: buttons, fields, selects and segmented controls set it once on the control and their label, icon and children inherit it, so a caller's weight (a selected sidebar row's 500) still wins. A field's caret, selection and rows follow its line (18 at `body`). Badges are strong `caption` (`micro` at the `xs` and `sm` rungs), keycaps `caption` (`micro`), the stepped slider `small`, and the touch slider's readout `title`. Surfaces (dialogs, alerts, notifications, callouts, notices, popovers) set `body` on the surface, before the caller's own style, so content with no style of its own is 13 on 18; their titles are `body` at the heading weight, 600, and their descriptions and messages `small`; a stacked notice's title is `lead` at 600. An empty state sets `small` on itself, since its message is its bare content, and its title is `body` at 600. A marker beside a first line (a notification, callout or notice icon) sits in a box as tall as that line: 18 beside a `body` title, 20 beside a stacked notice's `lead` title, 16 beside a message alone. Tooltips are `small`. Chrome: the title bar's content area and the tray panel set `body`; a window title in the title bar is `body` at 600, with a path beside it regular and muted. The docs header's brand is `lead` at 600. Which-key sets `small` on itself, its table name strong `small`, and `caption` for block titles, the repeat mark and its key caps in the code face. Settings: the page heading is `title`, group headers `body` at 600 (so a disclosure's chevron box is 18 tall), row titles `body`, descriptions `small`; the about page's name is `title`, its tagline `small`, its values `caption` in the code face. The picker's rows are `small`, its empty label `caption`, its footer `micro`. The color picker's label is `caption`, the touch trigger's hex `mono`. Appearance: slider values `small`, tile labels `caption`, and the theme picker's label takes the style of the button or menu row around it. Compact and touch: large titles `touch-title`; the HUD strong `touch-body`; the folding title `touch-body` at 600; pill titles and menu chips strong `touch-small`, pill meta `small`, the mini pill strong `body`; key rows `touch-small`; the touch which-key list `touch-body`, its caps `touch-small` in the code face, its group headers strong `body`; popover keys `touch-body`. Sheets: titles `display`, row labels `touch-body`, details `touch-small`. The code editor sets `mono` on itself (13 on 18) and draws line numbers at the `caption` size on the code line. The HTML versions read the `--penna-text-*` variables instead of copying sizes.

Code takes the size and line of the text around it, in the code face: `mono` in the interface, `read-body` size in reading text. ala can't size one run differently inside a line.

Root text (built): `Root` sets `body` on every window, before the caller's own style, and `penna.css` the same on `html`, so text with no style is 13 on 18 on both, where ala alone gives 16 on 25.89. A storybook section sets `body` too, and the storybook's own labels and sample text take text styles like any component (state labels are `caption` in the code face).

Centering: Inter centers its capitals in the line box (within 0.01px at 13 on 18); the macOS system face sits them 0.33px low and Lilex 0.34px. Center a line box to center text; center icons beside text on the capitals with `cap_center_drop`.

## Size

Control ladder. Every control at a size shares one anatomy:

| Token | px | Text | Inset | Field inset | Icon | Icon gap | Status |
|---|---|---|---|---|---|---|---|
| `control-xs` | 20 | `caption` | 8 | 6 | 12 | 4 | built: `Size::XSmall` (the small chip) |
| `control-sm` | 24 | `small` | 8 | 6 | 12 | 4 | built: `Size::Small` |
| `control-md` | 28 | `body` | 12 | 8 | 14 | 6 | built: `Size::Medium`, the default |
| `control-lg` | 32 | `body` | 16 | 10 | 16 | 6 | built: `Size::Large`, the tabs' default height |
| `control-xl` | 40 | `lead` | 16 | 12 | 16 | 8 | built: `Size::XLarge` |
| `control-touch` | 44 | `touch-body` | 16 | 12 | 16 | 8 | built: `Size::Touch` |

Fields use the field inset because their text starts at the edge. `Size` carries the whole row of the table: `control_px`, `control_text`, `inset`, `field_inset`, `icon_px` and `icon_gap`, and `Size::Size(px)` takes the nearest rung (`Size::rung()`) for everything but its height. Buttons are the exception: a `Size(px)` button keeps a proportional inset (a fifth of its height) and icon (three quarters of it) and a `space-6` icon gap, for custom shapes such as the title bar's chrome button. Chips take their rung's inset, icon gap and icon like buttons, and selects their rung's field inset and icon gap like fields; badges and keycaps pick their look by rung too, `Size(px)` included, while a checkbox at `Size(px)` takes that side. Badges, keycaps and checkboxes sit in a line of text, so from `Large` up they draw like `Medium`. A field's own marks (clear button, spinner, select chevron) are a rung smaller than the field from `Medium` up; `xs` and `sm` fields keep their own size.

Other heights: `control-chrome` 22 (controls in bars, today `PANE_HEADER_CONTROL_HEIGHT`); `bar` 34 = `control-chrome` + 2 × `inset-chrome` for pane headers when the app gives panes a margin (`Theme::pane_margin` above 0), and 40 when panes sit flush (the default, `pane_margin` 0); the title bar is the pane header plus 2 × `pane_margin`, and its chrome buttons are 32 with a 17 icon (`CHROME_BUTTON_SIZE`, `CHROME_ICON_SIZE`); `row` is `control-md` (28 at regular density) for list, tree, sidebar, picker and menu rows, and a menu row under a coarse pointer is 44; a pane's own list rows (`picker::list_row_height`) and the settings navigation's Back slot are `control-lg` 32. Tabs take only a rung's height (`control-lg` 32 by default, any rung with `.with_size()`); their label, inset and icons are the caller's (`small` text in the stories and in `tab.css`).

Icons: 12 beside `small` and `caption` (menu rows included), 14 beside `body`, 16 beside `lead` and `heading`. List and tree rows are the exception: their icons stay 14 beside `lead`.

Inside a line of text, keycaps and badges take the line height of the text they sit in (16 in `small`, 18 in `body`), and checkboxes are 14 at `xs` and `sm` and 16 from `md` up, never a control size.

Width: reading text stops at a measure of 640 (about 75 characters of `read-body`), natively and in HTML alike. Sidebars are 200 to 256 wide, layout geometry rather than tokens.

Density (built): one setting picks the default control and row height: compact 24, regular 28, comfortable 32. Text and gaps stay, and so do the other rungs. It is `Appearance::density` (Advanced, under UI zoom), and `Size::Medium.control_px()` follows it, so buttons, fields, selects, segmented controls and list, tree, menu and picker rows move together. The HTML reads `--penna-control-md`, which `:root[data-density]` changes (`?density=` on a page). `Size::rung()` measures `Size(px)` against the regular ladder, so a fixed-height control keeps its rung at any density.

## Color

Roles (CSS root names, default theme):

| Role | Light | Dark | Use |
|---|---|---|---|
| `bg` | #fcfcfc | #18181a | window and page |
| `panel` | #efefef | #1f1f21 | sidebars and side panels |
| `popover` | #e2e2e2 | #272729 | menus and popovers |
| `fg` | #1d1d1f | #e4e4e7 | text and icons |
| `fg-muted` | #58585a | #8d8d8f | descriptions, captions, story labels |
| `muted` | #585859 | #a0a0a3 | muted labels and the ghost paint |
| `fg-faint` | #909091 | #4b4b4c | placeholders and disabled text only |
| `accent` | #325fe3 | #6b8cff | links, text selection, picked accent |
| `on-accent` | #ebebeb | #010101 | text on accent fills |
| `primary` | #1d1d1f | #e4e4e7 | primary fills; follows `fg` unless an accent is picked |
| `success`, `warning`, `danger` | #007b3e, #a75400, #cc2737 | #3fbf74, #f0a14a, #f2555f | status, always with an icon or a word |

The values are `themes/default.json` and the generated `web/tokens/default-*.css`. Muted is 0.30 of the way to the pole: `fg-muted` mixes `fg` 0.30 toward its contrast pole (white under dark ink, black under light), and `muted` mixes the ghost paint's ink 0.30 toward the ground under it.

Surfaces stack in this order, each layer a step away from the one under it (darker in light, lighter in dark): window `bg`, then `panel`, then cards (`settings.card`), then `popover` and floating panels.

Highlight: what is on, picked or in focus uses `primary`, which is the ink itself in the default look and the accent once one is picked in Appearance.

Contrast (WCAG 2, built in step 8): in all six theme variants (three looks, light and dark) and for every pickable accent at any tint, muted text, accent, success, warning and danger as text reach 4.5:1 on `bg`, and so does the text on a solid fill of any of them. The muted mix is 0.30, as above. Light variants darken their accent and status colors until near-white text on them reads at 4.5:1, which also makes them read as text. Text on a solid fill takes whichever of near-white or near-black gives more contrast, so dark variants put near-black labels on their mid-tone accent and status fills. `interface_theme`'s tests assert all of it. `fg-faint` stays under 4.5 on purpose: placeholders and disabled text only.

## Corners, depth, focus, motion

- Corners: `radius-theme` is a theme knob people change (24 in default and macOS, 16 in Yaru; 25 at most). A control's radius is the theme radius capped at 45% of its height (`corner-fraction`). A corner inside another is concentric: inner radius = outer radius minus the inset between them. Rows draw at most smoothing 2.5.
- Depth: `shadow-contact` (0 1 4 #00000033) for controls on a surface; `shadow-lifted` for cards that move; `shadow-float` for menus, popovers, dialogs and the window. Themes can turn outlines and shadows off, so never let a shadow alone separate two surfaces.
- Focus: a 1.5px ring (`FOCUS_RING_WIDTH`) drawn outside the border box with its own adaptive radius. Only fields change their fill or edge on focus.
- Motion: appear 120ms, enter 200ms, turn 220ms, scaled by the animation speed setting (0.25 to 2). Respect reduced motion.

## Layout primitives (built)

Six primitives lay out app screens, stories and new components. They take tokens and draw nothing. The components that existed before them already lay out with `Space` directly and move onto the primitives only when they are rewritten.

| Primitive | Does | Rust | DOM |
|---|---|---|---|
| Stack | children in a column, a gap apart | `stack(Space::GAP_GROUP)` | `.penna-stack[data-gap="group"]` |
| Inline | children in a row on one line, wrapping when asked | `inline(Space::GAP_RELATED).flex_wrap()` | `.penna-inline[data-gap="related"][data-wrap]` |
| Inset | padding around one child | `inset(Space::INSET_CARD)` | `.penna-inset[data-inset="card"]` |
| Split | a fixed side beside a main that takes the rest | `split(width, gap, side, main)` | `.penna-split[data-gap]` with `--penna-aside` |
| Center | a column no wider than a measure | `center(px(640.))` | `.penna-center` with `--penna-measure` |
| Tiles | equal tiles that fill the width | `tiles(columns, gap)` | `.penna-tiles[data-gap]` with `--penna-tiles` |

Tiles takes a column count, not a minimum width: ala's grid takes a fixed number of equal columns, so both sides do the same. The Layout story shows each primitive, native and HTML.

Arrangement geometry stays with the app: where nidus puts computers on its board, a sidebar width someone dragged, terminal cells.

## Components

Every component has a section here, written in one shape: what it is for (and what to use instead), anatomy, kinds, sizes, states, behavior, content, placement, its HTML twin, and Status. Its docs page draws each part with the real component and carries the anatomy in plain words; the values stay here. The widget and its HTML twin take the same properties.

Every component page and story shows, where they apply: sizes, variants, tones, and the states rest, hover, pressed, focused, selected, disabled, loading and error, plus icon support. Known gaps across the kit: error states on select, text field, checkbox and radio; helper text; an indeterminate checkbox; radio groups; prefix and suffix on fields; equal-width tabs; a size API on segmented controls and sliders.

Status marks for a component: **matches** (the code and its page follow its section), or a list of what does not yet.

### Button

A button starts an action now. To go somewhere, use a link button; inside a menu, a menu row; for a setting that applies at once, a switch; to pick one of a few views, a segmented control.

Kinds, by tone and emphasis (`ButtonVariants`):

| Kind | Call | Paint | Use |
|---|---|---|---|
| Primary | `.primary()` | `Tone::Primary`, `Emphasis::Solid` | the main action of a view: at most one per dialog, form or pane |
| Secondary | default | `Tone::Neutral`, `Emphasis::Soft` | every other action |
| Ghost | `.ghost()` | `Tone::Neutral`, `Emphasis::Ghost` | actions in bars, toolbars and rows, where a fill at rest is noise; it fills on hover |
| Outline | `.emphasis(Emphasis::Outline)` | the tone's outline ink, no fill | a secondary action on a ground that has its own fill, where a soft fill would blend in |
| Danger | `.danger()` | `Tone::Danger`, `Emphasis::Soft` | an action that destroys something or can't be undone; the confirm button of a destructive dialog |
| Warning, Success | `.warning()`, `.success()` | the tone, `Emphasis::Soft` | not for an action on its own: status colors mean status (Principles 3). A notice's or callout's action is a primary placed on its ground (`.primary().on(Notice::ground(tone, cx))`), which paints it in that tone |
| Link | `.link()` | underlined ink, no padding | goes somewhere: a page, a URL, a settings pane; sits in text or at a row's end |
| Text | `.text()` | ink, no padding, muted while pressed | a plain action inside a line of text |

Apps' chrome uses the kit looks in `ButtonLook` (`Bar`, `Chrome`, `RowIcon`, `Floating`, `Send`, and the rest in kit.md); they follow this section like any button.

Anatomy, left to right, all from the button's rung of the control ladder (Size):

- Container: height `control_px`, radius the theme radius capped at 45% of the height, inset `inset` at both sides (8, 8, 12, 16, 16, 16 from `xs` to `touch`). Link and text buttons have no container inset.
- Icon (optional): `icon_px` (12, 12, 14, 16, 16, 16), before the label, `icon_gap` from it (4, 4, 6, 6, 8, 8). While loading it is a spinner of the same size.
- Label: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`) at weight 400. A primary's label takes whichever of near-white or near-black reads better on its fill.
- Caret (optional, `.dropdown_caret(true)`): a chevron at the end, `icon_gap` after the label, for a button that opens a menu. It is the same mark as a select's chevron at that size: a rung smaller than the button's icon from `md` up.
- Focus ring: 1.5px, outside the container, its own radius (Corners, depth, focus, motion).

Icon only: the button is square (width = height) and must carry a tooltip that names the action, which is also its accessible name. A row's trailing actions use `Button::compact_icon` (24, ghost).

Sizes: `md` by default (follows density). `sm` and `xs` in toolbars, lists and rows that repeat; `lg` and `xl` for the one main action of a sparse screen (an empty state, a first run); `touch` under a coarse pointer. Every button in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the kind's paint | |
| hover | the paint's hover step (a lift) | not under a coarse pointer, not when selected or disabled |
| pressed | the paint's active step, while held | not under a coarse pointer, which draws the press highlight instead |
| focused | the focus ring; fill unchanged | from the keyboard only, never from a click |
| selected | the picked look (`data-state="current"`) | a button that stands for an on state in a group |
| disabled | faint label, no hover or press | leaves the focus order; its tooltip still shows |
| loading | the icon becomes a spinner | clicks are ignored; the label and width stay |

In a window that is not active, a glass button flattens and its label fades, like the rest of the window's chrome.

Behavior: it fires on release of a click or tap inside it, and on Space or Enter while focused. A button with a caret opens its menu below it. A tooltip shows after the hover delay.

Content: the label is a verb or a short verb phrase in sentence case, one to three words ("Save", "Add host", "Open in browser"), with no closing period. It names the action, never "OK" or "Yes". End it with an ellipsis ("Allow…") when the action asks for more before it happens, such as opening a dialog. A danger button names what it removes ("Delete branch").

Placement: buttons in a group sit `gap-related` 8 apart. A dialog's or a form's actions sit at its end, aligned to the trailing edge, the primary last and its cancel just before it. A destructive dialog confirms with a danger button, not a primary one. Ghost buttons in a bar sit `inset-chrome` from its edges.

HTML: `.penna-button` with `data-penna` (the recipe: `button`, `button.quiet` for ghost, `button.outline`), `data-tone`, `data-prominent` (solid), `data-size` (`xsmall` to `touch`), `data-state="current"` (selected), `data-variant` (`link`, `text`), `disabled`, `data-loading`; the label is `.penna-label`, the icon `<i data-icon>`, and a button with a caret takes `data-caret` and holds a `.penna-caret`.

Status: two gaps. The caret is 12 at every size, where this section asks for the select's chevron at the same size. `.warning()` and `.success()` are still on `ButtonVariants` and in the buttons story's variants, where this section rules them out.

## Work order

| Step | What | Visible change | Status |
|---|---|---|---|
| 1 | Token names in Rust and CSS: `Space` named by pixels with gap and inset constants; space, gap, inset and text style CSS variables | none | built |
| 2 | Reading text: `TextView` and `text.css` on the reading ladder and rhythm | documents and chat get smaller, tighter text | built |
| 3 | Text styles: every component sets a full text style on its text, native and HTML, one family at a time: rows, lists and menus first, then controls, surfaces, chrome and the editor blocks, with the touch ladder | per family: rows that grew from a 1.618 line return to their row height | built |
| 4 | Root text style: `Root` and `penna.css` set `body` | app text with no style goes from 16 on 26 to 13 on 18; penna itself doesn't move | built |
| 5 | Layout primitives, native and DOM, with stories | none by themselves | built |
| 6 | Components onto the space tokens, one family at a time, starting with Appearance (its 11 and 14 insets); the primitives stay for app layouts and new code | small spacing moves, reviewed per family | built |
| 7 | Control ladder: `control-xs`, `-lg`, `-xl`, `-touch`, rows on `control-md`, tab heights on the ladder | menu rows grow 2px, tabs move | built |
| 8 | Color: contrast fixes across the six theme variants | flagged pairs darken or lighten | built |
| 9 | Density setting | new option | built |

Each step lands in penna with its stories, scores parity, rebaselines only what the step meant to move, and is pulled into zz and nidus.
