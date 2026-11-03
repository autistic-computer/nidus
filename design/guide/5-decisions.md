# Decisions

What this draft asks you to approve. Each row lists the proposal, what penna does today and what changes on screen.

| # | Decision | Proposal | Today | On screen |
|---|---|---|---|---|
| 1 | Names carry pixel values | `space-8` is 8px: `--penna-space-8`, `Space::S8` | `Space::S4` is 8px while ala's `gap_4()` is 16px | nothing |
| 2 | One way to write a space | `Space` and the gap and inset tokens only | `Space` 25 uses, ala helpers 55, raw `px()` 100, raw CSS 254 | nothing for the 91% on the scale; the rest move 1 or 2px |
| 3 | Four gap levels | tight 4, related 8, group 16, section 32 | each component picks | some gaps move to the nearest level |
| 4 | Parents own gaps | no outer margins on components | a handful of margins | nothing |
| 5 | Root text style | `Root` sets `body` (13 on 18) | ala's 1.618 line height | unstyled native text boxes shrink from 21px to 18px: the largest change |
| 6 | Reading ladder on whole pixels | 28/36, 22/28, 18/24, 16/24, body 16/26 | 28/45.5, 21/34, 17.5/28.5, 15.75/25.5 | document headings sit tighter |
| 7 | Touch ladder | 15/20, 17/22, 34/40 | raw 15, 17 and 34/41 | none; the other way is to fold 15 and 17 into `heading` |
| 8 | Control ladder | xs 20, sm 24, md 28, lg 32, xl 40, touch 44, one anatomy each | only sm and md; chips, tabs and menu rows pick their own | menu rows 26 to 28, tabs 36 to 32 or 40 |
| 9 | Density | a setting for the default control: 24, 28 or 32 | none | later step |
| 10 | Bars | `bar` = `control-chrome` + 2 × `inset-chrome` = 34 | the same | nothing |
| 11 | Large steps | 40 and 64 for pages; 20 and 48 fold | the docs site uses 40 and 96 | the docs hero drops from 96 to 64 |
| 12 | Layout primitives | Stack, Inline, Inset, Split, Center, Tiles in Rust and the DOM | `h_flex` and `v_flex` with gaps | nothing by themselves |

Not in this draft: color changes (the four flagged pairs stay), motion and focus rings.

## Order of work once approved

Each step ends with something to look at here or in the storybook.

1. Tokens and names in Rust and CSS from one table, with this system's `tokens.json` generated from it. Nothing moves on screen.
2. The six layout primitives, native and DOM, with storybook pages scored by parity.
3. The root text style: the visible change, reviewed story by story.
4. Components move to the primitives and the control ladder one family at a time, each with its card here.
5. The reading and touch ladders.
6. Density.
