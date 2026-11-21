# Chess Improver design system

This is the visual identity for the web app. Every UI task cites it: the tokens by name, the bans by ID (`B1`–`B18`), the motion principles by number (`M1`–`M10`) and the choreography by moment. If a value isn't here, don't invent one. Propose it, get it approved, and add it here first.

The tokens are implemented in `packages/ui/src/styles/tokens.css`. That file and this one must agree. If they disagree, this document wins until it is revised.

## Subject, audience and job

- **Subject:** a chess-coaching app that reads a player's own games and marks them up like a strong coach would.
- **Audience:** a 1000–2000 club player reviewing their own games on a laptop or phone.
- **Job:** to see, precisely, where they went wrong and what to practise.

Every screen should answer "where did I go wrong, and what do I do about it?" faster than the one before.

## Direction: annotated scoresheet

Cool paper, navy ink, and a coach's red pen for mistakes. It should feel precise and studious, like a game a strong coach has marked up the night after the tournament. The signature is the red pen: rings, arrows and question marks draw themselves onto the board and the scoresheet, the way a hand would.

**Boldness lives in two places: the board and the eval graph, and in how things move.** The board and graph are the brand. Motion is a headline feature (owner, 2026-11-21: "the animations need to be huge, that makes it premium"), so it is rich, choreographed and physical, but always about the chess. Everything static around the board (navigation, forms, settings, panels) stays quiet, tonal and disciplined.

Red has two meanings and no others: `--color-pen` marks a chess mistake, and `--color-pen-text` also carries form errors. Red is never decoration, never a brand flourish, and never a destructive-button colour.

### Core palette

| Name | Day | Role in the direction |
|---|---|---|
| Paper | `#F3F5F8` | cool, slightly blue page, never cream |
| Sheet | `#FBFCFD` | the scoresheet and panels, raised by tone only |
| Navy ink | `#16233B` | text, primary fills, focus |
| Red pen | `#D3312B` | the coach's marks on mistakes |
| Good-move green | `#24684B` | better moves and good play (deepened in round 2) |
| Pencil | `#56657F` | plans and neutral ideas, quieter than ink |

## Colour tokens

Two themes: **day** (light) and **night study** (dark). Night study is the same scoresheet at a desk lamp: navy paper and pale ink, not black with an accent (reference 5). Contrast is checked against `sheet` and `paper`.

| Token | Day | Night | Role |
|---|---|---|---|
| `--color-paper` | `#F3F5F8` | `#121A2B` | page background |
| `--color-sheet` | `#FBFCFD` | `#18223A` | raised surfaces, scoresheet, panels |
| `--color-well` | `#E7EBF1` | `#0D1421` | inset wells, inputs on sheet |
| `--color-rule` | `#C9D1DD` | `#2C3956` | decorative lines (non-text) |
| `--color-rule-strong` | `#7A879E` | `#6B7B9B` | input borders, chart axes (≥ 3:1) |
| `--color-ink` | `#16233B` | `#E7ECF4` | text, primary fill, focus ring |
| `--color-ink-soft` | `#3D4A63` | `#B4BFD2` | secondary text |
| `--color-ink-faint` | `#5B6780` | `#8E9AB1` | tertiary text (lowest allowed) |
| `--color-on-ink` | `#FBFCFD` | `#121A2B` | text on ink fills |
| `--color-pen` | `#D3312B` | `#F2695E` | red-pen strokes and marks (spec value) |
| `--color-pen-text` | `#C42B26` | `#F2695E` | red text (AA on `well` too) |
| `--color-good` | `#24684B` | `#76D3A4` | good-move strokes and marks (owner-approved change, round 2) |
| `--color-good-text` | `#24684B` | `#76D3A4` | green text (owner-approved change, round 2) |
| `--color-pencil` | `#56657F` | `#97A6C0` | neutral annotations, plan arrows |
| `--color-board-light` | `#E3E8EF` | `#8796B5` | light squares (night: owner-approved change, round 2) |
| `--color-board-dark` | `#A7B4C7` | `#5A6A8C` | dark squares (night: owner-approved change, round 2) |
| `--color-board-lastmove` | `rgb(22 35 59 / 0.16)` | `rgb(231 236 244 / 0.30)` | last-move tint (night: controller-approved change, Task 11) |

Added in fix round 1 (derived from the tokens above, no new hues):

| Token | Day | Night | Role |
|---|---|---|---|
| `--color-selection` | `rgb(86 101 127 / 0.28)` | `rgb(151 166 192 / 0.34)` | `::selection` background (pencil); text keeps its colour |
| `--color-board-select` | `rgb(22 35 59 / 0.24)` | `rgb(231 236 244 / 0.26)` | selected square tint (supplementary) |
| `--color-board-premove` | `rgb(86 101 127 / 0.30)` | `rgb(151 166 192 / 0.30)` | premove tint (pencil, supplementary) |
| `--color-piece-dark` | `#16233B` (ink) | `#0D1421` (well) | black piece body, and every piece's outline |
| `--color-piece-light` | `#FBFCFD` (sheet) | `#E7ECF4` (ink) | white piece body, black piece interior details |

### Usage rules

- **Text** uses `ink`, `ink-soft` or `ink-faint`, and nothing lighter. On dark, body text is `ink` (`#E7ECF4`), never a mid grey (`B17`).
- **Red text** uses `pen-text`, and green text uses `good-text`. The stroke tokens `pen` and `good` are for marks, not words.
- **Lines:** `rule` is decorative only (scoresheet ruling, dividers). Anything a user must see to operate the UI, such as input borders, chart axes or checkbox outlines, uses `rule-strong`.
- **Primary action:** an `ink` fill with `on-ink` text. There is one primary action per view.
- **The halo rule:** every board mark (ring, arrow, glyph badge, focus ring, check ring, legal-move dot) sits on a 2px `--color-sheet` halo, so the mark's adjacent colour is always its halo and never a square. That makes the `pen`, `good`, `pencil` and `ink` rows against `sheet` below the binding check. Without the halo, night marks fall to 1.01:1 on light squares, so the halo is not optional.
- **Board coordinates** use `ink` in day. In night they use `on-ink` on light squares and `ink` on dark squares, because no single colour passes on both night squares. On a tinted night square (last move or selected) they use `on-ink` on both colours, because the pale tint drops `ink` below 3:1 there (controller ruling, Task 11).
- **Last move** is tinted with `board-lastmove`. The tint is under 3:1 against the bare square on purpose, so it never competes with the pen. It is supplementary: the current move is also marked in the move list and announced to assistive tech.
- **Colour never carries meaning alone.** `pen` and `good` are close in luminance (1.34:1 day and 1.67:1 night after the round 2 green change, up from 1.01 and 1.29), so every mark also differs in shape: pen marks always carry a `?`-family glyph badge and an open two-stroke arrowhead; good marks carry a `!`-family badge and a solid head. See "Severity and marks".

### Semantic states

| State | Treatment |
|---|---|
| Error (forms, failed actions) | `pen-text` message under the field, Phosphor `WarningCircle` icon, `aria-invalid`, field border `pen` (4.14:1 on `well`). Red is used for form errors, which is red's second meaning. |
| Warning | `ink` text on `well`, Phosphor `Warning` icon in `ink`, 1px `rule-strong` border. No amber token. |
| Info | `ink-soft` text on `well`, Phosphor `Info` icon. |
| Success | `good-text` message, Phosphor `CheckCircle` icon. A toast names the finished action ("chess.com connected"). |

### Interaction states

The tonal ladder runs `paper`, then `sheet`, then `well`. Hover moves one step down the ladder and never lifts. In code, hover fills use `--surface-hover`: `sheet` on the page, and `well` inside anything that is `bg-sheet` or marked `data-surface="sheet"`.

| State | Surface items (rows, secondary buttons, menu items) | Primary button (ink) |
|---|---|---|
| Rest | its surface (`paper` or `sheet`) | `ink` fill, `on-ink` text |
| Hover | one step down (`paper` to `sheet`, `sheet` to `well`) | `ink-soft` fill (`on-ink` 8.67:1) |
| Pressed | `well` with a 1px inset `rule-strong`, and the pen-press: scale 0.97 for `--dur-instant` | `ink` fill, scale 0.97 |
| Focus-visible | 2px `ink` outline, 2px offset, on any surface | the same, offset over the page |
| Selected | `well` fill, text in `ink` at 600 weight, `aria-selected` or `aria-current` | not applicable |
| Disabled | `ink-faint` text, 1px `rule` border, no fill change, no hover, `aria-disabled` | `well` fill, `ink-faint` text |

`::selection` uses `--color-selection`. Text keeps its own colour (`ink` on it: 10.34:1 day, 6.92:1 night).

### Contrast results

Computed with the WCAG 2.x relative-luminance formula (sRGB to linear, `L = 0.2126 R + 0.7152 G + 0.0722 B`, ratio `(L1 + 0.05) / (L2 + 0.05)`), last run 2026-11-21 in fix round 2 over every pair, after the owner-approved green and night-board changes. Composited colours (selection) are blended over their surface before the check. Text needs 4.5:1. Large text, UI strokes, marks and focus rings need 3:1. No pair fails.

| Foreground | Background | Use | Day | Night | Required | Result |
|---|---|---|---|---|---|---|
| `ink` | `sheet` | body text | 15.28 | 13.32 | 4.5:1 | pass |
| `ink` | `paper` | body text | 14.37 | 14.65 | 4.5:1 | pass |
| `ink` | `well` | text in wells | 13.12 | 15.54 | 4.5:1 | pass |
| `ink-soft` | `sheet` | secondary text | 8.67 | 8.52 | 4.5:1 | pass |
| `ink-soft` | `paper` | secondary text | 8.15 | 9.37 | 4.5:1 | pass |
| `ink-soft` | `well` | secondary text | 7.44 | 9.94 | 4.5:1 | pass |
| `ink-faint` | `sheet` | tertiary and disabled text | 5.53 | 5.57 | 4.5:1 | pass |
| `ink-faint` | `paper` | tertiary text | 5.20 | 6.13 | 4.5:1 | pass |
| `ink-faint` | `well` | tertiary text | 4.75 | 6.50 | 4.5:1 | pass |
| `on-ink` | `ink` | primary button | 15.28 | 14.65 | 4.5:1 | pass |
| `on-ink` | `ink-soft` | primary button, hover | 8.67 | 9.37 | 4.5:1 | pass |
| `pen-text` | `sheet` | red text | 5.49 | 5.24 | 4.5:1 | pass |
| `pen-text` | `paper` | red text | 5.17 | 5.76 | 4.5:1 | pass |
| `pen-text` | `well` | red text | 4.72 | 6.11 | 4.5:1 | pass |
| `good-text` | `sheet` | green text | 6.48 | 8.75 | 4.5:1 | pass |
| `good-text` | `paper` | green text | 6.09 | 9.62 | 4.5:1 | pass |
| `good-text` | `well` | green text | 5.56 | 10.20 | 4.5:1 | pass |
| `ink` | selection on `sheet` | `::selection` | 10.34 | 6.92 | 4.5:1 | pass |
| `ink` | selection on `paper` | `::selection` | 9.82 | 7.52 | 4.5:1 | pass |
| `on-ink` | `pen` | glyph badge text (`?`, `??`, `?!`) | 4.82 | 5.76 | 4.5:1 | pass |
| `on-ink` | `good` | glyph badge text (`!`, `!!`) | 6.48 | 9.62 | 4.5:1 | pass |
| `pen` | `sheet` | mark on its halo | 4.82 | 5.24 | 3:1 | pass |
| `pen` | `paper` | mark off the board | 4.53 | 5.76 | 3:1 | pass |
| `pen` | `well` | graph ring, error border | 4.14 | 6.11 | 3:1 | pass |
| `good` | `sheet` | mark on its halo | 6.48 | 8.75 | 3:1 | pass |
| `good` | `paper` | mark off the board | 6.09 | 9.62 | 3:1 | pass |
| `pencil` | `sheet` | mark on its halo | 5.74 | 6.42 | 3:1 | pass |
| `pencil` | `paper` | mark off the board | 5.40 | 7.06 | 3:1 | pass |
| `rule-strong` | `sheet` | borders, axes | 3.53 | 3.71 | 3:1 | pass |
| `rule-strong` | `paper` | borders, axes | 3.32 | 4.08 | 3:1 | pass |
| `rule-strong` | `well` | borders, axes | 3.03 | 4.33 | 3:1 | pass |
| `ink` | `sheet` | focus ring (2px, 2px offset) | 15.28 | 13.32 | 3:1 | pass |
| `ink` | `paper` | focus ring | 14.37 | 14.65 | 3:1 | pass |
| `ink` | `well` | focus ring | 13.12 | 15.54 | 3:1 | pass |
| **Board** | | | | | | |
| `ink` | `sheet` (halo) | board focus, check and legal-dot rings on their 2px halo | 15.28 | 13.32 | 3:1 | pass |
| `ink` | `board-light` | coordinates (day only) | 12.75 | n/a | 4.5:1 | pass |
| `ink` | `board-dark` | coordinates | 7.47 | 4.57 | 4.5:1 | pass |
| `on-ink` | `board-light` | coordinates (night only) | n/a | 5.84 | 4.5:1 | pass |
| `ink` | last move on `board-light` | coordinates on a last-move square (day) | 9.33 | n/a | 4.5:1 | pass |
| `ink` | last move on `board-dark` | coordinates on a last-move square (day) | 5.71 | n/a | 4.5:1 | pass |
| `on-ink` | last move on `board-light` | coordinates on a last-move square (night) | n/a | 7.96 | 4.5:1 | pass |
| `on-ink` | last move on `board-dark` | coordinates on a last-move square (night) | n/a | 5.48 | 4.5:1 | pass |
| `ink` | selected on `board-light` | coordinates on the selected square (day) | 7.95 | n/a | 4.5:1 | pass |
| `ink` | selected on `board-dark` | coordinates on the selected square (day) | 4.92 | n/a | 4.5:1 | pass |
| `on-ink` | selected on `board-light` | coordinates on the selected square (night) | n/a | 7.62 | 4.5:1 | pass |
| `on-ink` | selected on `board-dark` | coordinates on the selected square (night) | n/a | 5.14 | 4.5:1 | pass |
| `piece-dark` | `board-light` | piece outline and black body | 12.75 | 6.20 | 3:1 | pass |
| `piece-dark` | `board-dark` | piece outline and black body | 7.47 | 3.40 | 3:1 | pass |
| `piece-light` | `piece-dark` | white body inside its outline | 15.28 | 15.54 | 3:1 | pass |
| **Charts** | | | | | | |
| `ink` | `sheet` | series 1, highlighted series | 15.28 | 13.32 | 3:1 | pass |
| `ink-soft` | `sheet` | series 2 | 8.67 | 8.52 | 3:1 | pass |
| `pencil` | `sheet` | series 3 | 5.74 | 6.42 | 3:1 | pass |
| `rule-strong` | `sheet` | series 4, axes | 3.53 | 3.71 | 3:1 | pass |
| `ink` | `well` | series 1 on a well | 13.12 | 15.54 | 3:1 | pass |
| `ink-soft` | `well` | series 2 on a well | 7.44 | 9.94 | 3:1 | pass |
| `pencil` | `well` | series 3 on a well | 4.93 | 7.49 | 3:1 | pass |
| `rule-strong` | `well` | series 4 on a well | 3.03 | 4.33 | 3:1 | pass |

Informational only, to show why the halo rule exists (a mark drawn straight on a square, with no halo):

| Mark | `board-light` day | `board-dark` day | `board-light` night | `board-dark` night |
|---|---|---|---|---|
| `pen` | 4.02 | 2.36 | 1.01 | 1.80 |
| `good` | 5.40 | 3.17 | 1.65 | 3.00 |
| `pencil` | 4.79 | 2.81 | 1.21 | 2.20 |
| `ink` (focus ring) | 12.75 | 7.47 | 2.51 | 4.57 |

The halo itself against the squares is 1.20 and 2.05 (day) and 5.31 and 2.92 (night). It never needs 3:1, because it is the mark's background, not the mark. Last-move tint over the bare square: 1.37 and 1.31 (day), 1.36 and 1.71 (night, after the Task 11 change from 0.18 to 0.30, which also made the night board livelier), supplementary by design.

## Type

- **Literata** (variable, with the `opsz` optical-size axis) sets headlines, panel headings, annotations and notation. Weights 400 and 600. Italic 400 is used only for whole annotation comments, never for one word in a sentence or headline (`B16`), and never for moves.
- **Schibsted Grotesk** sets the UI: labels, buttons, navigation, forms and settings. Weights 400, 500 and 700.
- Italic is loaded as its own family, `--font-serif-italic` (Literata italic, not preloaded), so pages without an annotation comment never download it. Use it only with `font-style: italic`.
- Both are self-hosted through `next/font/local` from WOFF2 files committed with their SIL OFL 1.1 licence texts in `apps/web/src/app/fonts/` (ADR-WEB-7), so neither the build nor the browser ever calls Google. Literata is the upstream variable font (`opsz` 7–72, `wght` 200–900, roman and italic), subset to Latin. Schibsted Grotesk is the upstream variable WOFF2 (`wght` 400–900). No other families. In particular, none of the defaults in `B6`, and no monospace anywhere, including for numbers (`B5`).

### Scale and optical size

Size and line height in px, from the classic scale. Literata's `opsz` tracks the rendered size (`font-optical-sizing: auto`). Where a size is set with `font-variation-settings`, use the value in the table.

| Step | Size / line height | Literata `opsz` | Typical use |
|---|---|---|---|
| `2xs` | 11/14 | 11 | only glyph-badge text (its floor) and the eval bar label. Nothing else. |
| `xs` | 12/16 | 12 | chart ticks (board coordinates scale with the square, see "The board") |
| `sm` | 14/20 | 14 | secondary UI text, captions, graph labels |
| `base` | 16/24 | 16 | UI body, buttons, inputs, move list (15px rows use `opsz` 15) |
| `prose` | 18/30 | 18 | serif body (annotations, explanations), with more leading |
| `lg` | 18/26 | 18 | sans lead text |
| `xl` | 21/28 | 21 | **panel headings: Literata 600** |
| `2xl` | 24/30 | 24 | page section headings |
| `3xl` | 36/42 | 36 | page titles |
| `4xl` | 48/54 | 48 | rare: the result line of a reviewed game |
| `5xl` | 60/64 | 60 | landing only (M5) |

- **Notation** (`31. Rxe6?? −4.2`) is a typographic feature in its own right. Set it in Literata 600 with `font-variant-numeric: lining-nums tabular-nums`. Glyphs are kerned tight to the move with no space (`b5?`, `Nf6?!`), using `letter-spacing: -0.01em` on the glyph (`--tracking-glyph`). Mistake glyphs use `pen-text`, good-move glyphs use `good-text`, and the rest is `ink`. The evaluation follows after a space in `ink-faint` at weight 400. Negative evaluations use the real minus sign, U+2212 (`−4.2`). Castling stays `O-O` and `O-O-O`, with letter O and hyphens.
- **Measure:** ≤ 72ch for serif prose and ≤ 66ch for sans.
- **Case:** sentence case everywhere, including buttons, headings and table headers. No all-caps labels and no letter-spaced eyebrows (`B5`).
- **Alignment:** left aligned. Centring is reserved for a diagram's caption under the board and for an empty state's single line.

## Shape, space and elevation

**Radii by role.** Radius reflects what a thing is, not a single global value (`B4`, `B14`):

| Token | Value | Applies to |
|---|---|---|
| `--radius-board` | 2px | the board, mini-boards, the eval bar, the eval graph's plot area |
| `--radius-field` | 6px | inputs, buttons, menus, list rows, chart wells |
| `--radius-sheet` | 10px | sheets and panels, the scoresheet, drawers, dialogs |
| `--radius-pill` | 9999px | toggles, segmented controls, small counts |

**Spacing roles.** Tailwind 4's base unit (`--spacing: 0.25rem`) for general layout, plus these named roles:

| Token | Value | Use |
|---|---|---|
| `--space-move-row` | 26px | one row of the move list (dense, beside the board) |
| `--space-scoresheet-row` | 34px | one row of the full scoresheet, tall enough for the scoresheet ring (ry 14px plus stroke and halo) |
| `--space-panel-pad` | 20px (16px below 480px) | inner padding of sheets and panels |
| `--space-gutter` | 16px below 768px, 24px from 768px, 32px from 1280px | page side gutters and column gaps |

**Board sizes.** The board is always a whole number of pixels per square (`8 × floor(size / 8)`). The token is `--board-size`, which rounds down to a multiple of 8px where CSS `round()` is supported (an unrounded `min()` is the fallback):

| Viewport | Board size |
|---|---|
| below 768px | `100vw − 2 × gutter`, capped at 480px (328px at 360px) |
| 768–1023px | 448px |
| 1024–1279px | 512px |
| 1280–1599px | 560px |
| 1600px and up | 640px |

**Elevation.** There are no card shadows. Surfaces separate by tone: `paper` is the page, `sheet` sits on it, and `well` is pressed into a sheet. The one shadow is `--shadow-lift: 0 10px 30px -12px rgb(22 35 59 / 0.35)` (night: `rgb(0 0 0 / 0.5)`), used only for things that float: popovers, the drawer, the command menu, and a piece while it is being dragged.

**Ruling.** Ruled lines are scoresheet-native, so they appear only on scoresheet and move-list surfaces, in `rule`. They never decorate a page, never separate every section, and never form newspaper columns (`B3`). Panels keep `--radius-sheet`.

**Focus.** Everywhere outside the board: a 2px `ink` outline with a 2px offset, so the ring sits on the surface behind (`paper`, `sheet` or `well`, all ≥ 13:1). On the board: an inset **dashed** ring (dash 4, gap 3), a 2px `sheet` halo at 2px inside the square's edge, with a 2px dashed `ink` ring inside it, so the ring's neighbour is always the halo. The dashes keep focus distinct from the solid 3px check ring. The focused square is also announced ("e4, white knight").

**Touch targets.** On a coarse pointer every control's hit area is at least 44 × 44px, without changing its visual size: buttons (`md` 40px and `sm` 32px tall), tabs (32px), switches and icon-only controls (such as the dialog close, 32px) grow an invisible centred hit area. It reaches 2px past an `md` control and 6px past an `sm` one, so stacked controls need a gap of at least **4px between `md` controls** and at least **12px between `sm` controls** (and 6px from an `sm` control to the edge of any scrolling container), or neighbouring hit areas overlap and the later one wins. `apps/web/e2e/touch-targets.spec.ts` checks this with a real hit test.

**Icons.** Phosphor, regular weight, `currentColor`. 20px in UI controls and 16px inline with text. Always paired with a text label unless the control has an accessible name and a universal meaning (close, menu). Never Lucide or emoji (`B13`).

## Pieces

- **Style:** flat, in the ink metaphor. They are line drawings, like a printed diagram, with no gradients, bevels, wood or shadows.
- **Day:** black pieces are filled `piece-dark` (ink) with `piece-light` interior details. White pieces are filled `piece-light` (sheet) with a `piece-dark` outline about 1.5px at a 45px square, scaling with the square.
- **Night:** black pieces are filled `piece-dark` (`well`, `#0D1421`), which holds 6.20:1 and 3.40:1 on the two night squares, with `piece-light` interior details. White pieces are filled `piece-light` (pale ink) with the same `piece-dark` outline. Every silhouette therefore has a ≥ 3:1 edge on both squares in both themes.
- **Default set: Cburnett** (Colin M.L. Burnett, Wikimedia Commons, used under its BSD-3-Clause option). It is drawn as flat outlines with a 1.5px stroke at 45px, which is this exact look, and it recolours with two values. Decision: `docs/decisions/0039-piece-set.md` (Accepted).
- Never use Unicode chess glyphs for pieces.

### Piece sets

Piece sets are user-configurable (owner decision, round 2).

- **The setting:** "Pieces" in Settings, under Board. It is a radio group of sets, each shown as a live mini-row (king, queen, knight and pawn of both colours) on the user's current theme squares. It is stored per user and applies to every board in the app. Changing it re-renders boards instantly. Pieces don't animate between sets.

| Set | Licence | Source | Status |
|---|---|---|---|
| Cburnett | BSD-3-Clause (chosen from its multi-licence: GFDL 1.2+, CC BY-SA 3.0, BSD-3-Clause or GPLv2+) | Wikimedia Commons, `Chess_{k,q,r,b,n,p}{l,d}t45.svg` | default |
| Chessnut | Apache-2.0 | github.com/LexLuengas/chessnut-pieces | option |
| Signature set | to be decided (a commissioned original, licensed to us) | to be commissioned | placeholder slot, not shipped |

- **Recolouring rules.** Every set must fit ink and sheet before it ships:
  - All dark paint maps to `--color-piece-dark`. All light paint maps to `--color-piece-light`. Unpainted shapes inherit `--color-piece-dark`.
  - At most two colours per piece. Sets with more colours, gradients or shading are redrawn or rejected (this is why RhosGFX was not taken).
  - Every silhouette has a `piece-dark` outline of about 1.5px at 45px, scaling with the square, so it holds ≥ 3:1 on both squares in both themes (see the contrast table).
  - Recolouring happens at build time into per-theme SVG sprites. Pieces never use runtime `filter` recolouring.
  - The set is checked in reference 6's format (day and night strips) before it is offered.
- **The licence gate.** No GPL, AGPL, NC (non-commercial), "freeware" or unlicensed sets. Each set's licence, author and exact source URL are recorded in `design/references/REFERENCES.md` and the ADR before it is added. Every shipped set gets its attribution and licence text on `/licenses` (the BSD-3-Clause notice for Cburnett, and the Apache-2.0 licence and NOTICE for Chessnut). Files come from the original source. Cburnett is taken from Commons, not from Lichess's GPL-labelled copy.
- **The embedded copy.** react-chessboard bundles its own Cburnett copy (`defaultPieces`) in the client JavaScript whatever we pass to `pieces`, so it ships even though it is never rendered. The Cburnett BSD-3-Clause notice and attribution on `/licenses` therefore also cover that embedded copy, from the first release that bundles react-chessboard. The notice stays for as long as react-chessboard is bundled, even if the default set later changes to Chessnut or the signature set (controller ruling, Task 10 review; ADR 0039 addendum).
- **Signature set (future).** There is a reserved slot for a commissioned original set drawn in the ink metaphor. It needs a written licence or assignment to us and must pass the same recolouring rules. Until then, the slot doesn't appear in Settings.

## The board

The board is one of the two loud things, so it gets the most specific rules.

- `react-chessboard` (MIT) with chess.js (BSD-2). Never chessground or chessops (`B12`).
- Squares are `board-light` and `board-dark`, cool and low in contrast, so pieces and marks carry the eye. The board has `--radius-board` and no frame, shadow or wood.
- **Well (from 768px only):** the board sits in a `well` with 8px padding, and both have `--radius-board`. The board keeps `--board-size`, and the well is 16px wider. Below 768px there is no well and the board is flush, so a phone gets every pixel. Decision: `design/decisions/board.md` (Task 10, variant c "Ink well").
- **Coordinates** sit on the squares, never in a lane outside the board: ranks in the top-left corner of the a-file squares and files in the bottom-right corner of the first-rank squares (as seen from the board's orientation), in Schibsted 500 at **0.2 of the square** (8.2px on a 41px square, 14px on a 70px one; `calc(var(--board-size) * 0.025)`) with a **2px inset**, line height 1, coloured as in "Board coordinates" under the usage rules (controller ruling, Task 11).
- **No piece covers a coordinate,** on any square, from 41px squares up: pieces are drawn centred at **0.84** of the square below 768px and **0.88** from 768px (`--piece-scale`). The binding reaches are a bishop's base toward the bottom-right corner and a queen's crown toward the top-left. `packages/ui/test/board.test.tsx` checks every point each piece paints (its outline sampled along its curves, plus half its stroke) against both glyph boxes (0.2 of the square tall, 0.61em wide, the widest glyph's advance) at 41, 44, 56, 60 and 80px squares.
- **Pieces** come only from the shipped sets under "Piece sets", passed through react-chessboard's `pieces` option on every board. react-chessboard's built-in default pieces are never rendered, because they are Cburnett drawings without attribution (controller ruling, Task 10).
- **Board states:**

| State | Treatment |
|---|---|
| Last move | `board-lastmove` tint on both squares (supplementary) |
| Selected square | `board-select` tint, and its legal moves are shown |
| Legal move (empty square) | an `ink` dot, 24% of the square's width, on a 2px `sheet` halo |
| Legal move (capture) | an `ink` ring inset 4% from the square's edge, 8% of the square thick, on a 2px halo |
| Check | the king's square gets a 3px `ink` inset ring on a 2px `sheet` halo, the move list shows `+`, and assistive tech hears "check". It is never pen red, because check isn't a mistake. |
| Premove | `board-premove` tint with a 2px dashed `pencil` inset ring on a halo |
| Drag ghost | the origin square keeps the piece at 30% opacity, and the dragged piece floats under the pointer with `--shadow-lift` (see the choreography) |
| Keyboard focus | the dashed inset ring described under "Focus". With check on the same square, focus sits inside the check ring. |

- **Keyboard.** The board is operable from the keyboard: arrow keys move focus, Enter or Space picks up and drops, and Escape cancels.

## Severity and marks

Severity comes from the mover's loss of win chance, using the Lichess thresholds on the win-chance scale defined under "The eval graph": a loss of at least 0.3 is a blunder, at least 0.2 a mistake, and at least 0.1 an inaccuracy. In the reference game, this marks 4…Bxf3?!, 6. Bc4?!, 6…Nf6?, 9…b5? and 10…cxb5?!.

| Mark | On the board | Glyph | Stroke |
|---|---|---|---|
| Blunder | pen ring on the destination, pen arrow for the move played, badge | `??` | pen, open two-stroke head |
| Mistake | pen ring on the destination, badge | `?` | pen |
| Inaccuracy | badge only | `?!` | thin (0.75×) for its graph marker ring. It never gets a ring on the board or the scoresheet. |
| Good move | good arrow for the better move or the punishment, badge when it was played | `!` | good, solid head |
| Brilliant | good ring, good arrow, badge | `!!` | good, solid head |
| Plan or idea | pencil arrow or ring, never a badge | none | pencil, 0.7×, open head |
| Link | one stroke joining a scoresheet or notation ring to the board ring of the same move, never a badge | none | pen, 0.9×, no head |

- **Redundancy, not colour alone:** pen marks always carry a `?`-family badge and an open head. Good marks carry a `!`-family badge and a solid head. Pencil never carries a badge. The one exception is the link mark below: it has neither, because both of its ends already carry the glyph or the badge.
- **Glyph badge:** a circle of radius 20% of the square in the square's top-right corner (inset 1px), or in its top-left corner when the move's arrow arrives from the right of the target as drawn (the origin's centre is right of the target's in the board's orientation), so the badge never covers the arrowhead, filled `pen` or `good`, with the glyph in `on-ink` Literata 600 (35% of the square for one character, 28% for two, and never below `2xs`, 11px), on a 2px `sheet` halo. A 328px mobile board (41px squares) gives 14px and 11.5px.
- **Link mark** (controller ruling, Task 6 fix round 1). One pen stroke that ties a move's notation to its square: it runs from the scoresheet ring (or a notation ring) to the board ring of the same move.
  - *Colour:* `pen`, and only when both ends are pen-marked (a ringed `?`/`??` move and its ringed square with its badge). It has no badge and no head.
  - *Shape:* the variable-width profile under "Pen stroke geometry" at 0.9× of the board pen at the board's current square size, so it scales with the square exactly as the board marks do (0.9 × square / 56 times the profile). It keeps the `sheet` halo wherever it crosses the board.
  - *Routing:* never across a text box or an occupied square, and at least 16px from any text and from the viewport edge. It enters the board through the gutter between the board and the text, over empty squares only, and stops short of the board ring. When no legal route exists, the link is omitted.
  - *Timing:* it draws after the badge lands, from 1400 to 1820 in the climax (`--dur-draw` with `--ease-pen`), and it retracts with the other marks.
  - *Reduced motion:* it appears fully drawn (M10).
  - *Budget and visibility:* at most one per view, and it counts toward the 12 animating marks (M9). It is drawn only when both ends are fully in view at its start time (an `IntersectionObserver` on both ends, checked at 1400ms, or at once under reduced motion); otherwise it is omitted for that view.

### Pen stroke geometry

Marks are filled, variable-width outlines built around a centreline, never constant-width strokes. They are opaque and drawn once. All values below are at a 56px square and scale linearly with the square (minimum 1.1px). Geometry lives in `packages/ui/src/annotations/**`, the one place raw values are allowed besides `tokens.css` and `board/**`. It is specified here so the code has a contract.

- **Width profile along the stroke:** 2.2px at the start, rising to 3.2px between 35% and 65% of the length, then tapering to 1.4px at the end. The ends are rounded.
- **Width by mark type:** pen 1.0×, good 1.0×, pencil 0.7× (thinner than pen and good strokes), inaccuracy (thin) 0.75×, arrowhead strokes 0.9× of their shaft's width.
- **Ring:** an ellipse, not a circle, so it reads at a glance. ry is 38% of the square and rx/ry ≈ 1.12, tilted 8–12° (sign and amount seeded). The radius drifts by `1 + 0.04 sin(2θ + φ₁) + 0.02 sin(3θ + φ₂)`, which is ±6% at most, with seeded phases. The integer frequencies (2 and 3 cycles per revolution) make the drift identical at θ and θ + 360°, so the only difference in the overrun is the outward spiral and the tail never crosses the start. It starts near 1 o'clock (−60° from three o'clock in the ellipse's frame) and runs to 380°, growing linearly by 2.5px per revolution, so the overrun tail sits 2.5px outside the start. It is a spiral, not a closed ring.
- **Scoresheet ring:** the same stroke at 0.8× width, fitted to the word as an ellipse with `rx = word width / 2 + 10px` and `ry = 14px`, tilted 2–4°. It needs `--space-scoresheet-row` (34px) and is used only for blunders and mistakes on the scoresheet, never in the 26px move list.
- **Arrow shaft:** starts 22% of a square from the origin centre and ends 16% short of the target centre. When the target also carries a ring, it ends 46% short instead, so the tip stops on the ring's outer edge and the head is not buried inside the ring. It bows 2–4% of its length to one side (seeded) as a quadratic curve.
- **Open head (pen and pencil):** two separate strokes of 0.9 and 1.1 times a base length of 30% of the square, at 26° and 32° to the shaft (which side gets which is seeded).
- **Solid head (good):** a filled triangle with the same unequal sides, on the halo.
- **Halo:** the outline is drawn first in `sheet` with a 4px stroke (2px each side), then filled in its colour.
- **Determinism:** the seed is the FNV-1a 32-bit hash (offset basis 2166136261, prime 16777619, using `Math.imul`) of a key made from the mark type and its squares (`ring:b5`, `arrow:c3b5`, `badge:b5`). That seed feeds mulberry32, and values are drawn in a fixed order: tilt, tilt sign, φ₁, φ₂ for rings; bow and bow sign, then head-side swap for arrows. The same mark therefore looks the same on every device and every visit, and a unit test pins the first values for a known key.
- **Drawing technique:** the filled outline is revealed through an SVG mask whose centreline stroke animates `stroke-dashoffset` (`pathLength = 1`). The mask stroke's width is the local outline width plus the 4px halo plus 1px. It never uses one fat width, which would reveal the overlapping part of the spiral early. For rings, the last 20° (360° to 380°) is a separate mask segment that animates after the first segment finishes, so the overrun draws over the start rather than revealing it. This keeps the hand-drawn shape and stays inside the performance budget.

## The eval graph and eval bar

### Eval graph

- Built with visx. It sits full width directly under the board (on narrow screens, under the board and above the move list).
- **Mapping:** the y-axis is win chance, not pawns, using Lichess's `winningChances`: `wc = 2 / (1 + exp(−0.00368208 × centipawns)) − 1`, clamped to [−1, 1], from White's point of view. A forced mate maps to ±1. The +8 and +12 of a won game then don't flatten the early mistakes.
- **Size:** the plot is 120px tall, or 88px in a container narrower than `--container-graph` (496px, which sits between the 448px and 512px board sizes), plus a 22px label lane above it and an 18px phase lane below it. The whole box is `--eval-graph-height` (128px) or `--eval-graph-height-wide` (160px), set by the `@graph` container query, so nothing shifts when the graph measures itself. The component's JS uses the same 496px threshold, and a test keeps the two in step.
- **Plot:** a `well` with `--radius-board`. The zero line uses `rule-strong`. The eval line is `ink` at 2px over a 10% `ink` area fill toward zero.
- **Markers:** each blunder, mistake or inaccuracy gets a small ring drawn with the same pen geometry (radius 7px, tilted 10°), thin for inaccuracies, centred on the line.
- **Labels:** in the label lane, never on the line: the move in notation with its glyph (`9…b5?`) in `pen-text`, Literata 600 at `sm`. A 1px `pencil` leader runs from each label down to its ring. Labels are placed in priority order (the player's blunders, the player's mistakes, then the opponent's), each pushed either way by at most 56px to keep a 10px gap, and always order-preserving: a label sits in the same left-to-right order as its mark, so no two leaders cross. Leaders leave the label as a short 7px diagonal, then run straight down; a mark at the top of the plot gets a straight vertical. One label form is used per lane: the full notation, or SAN plus glyph when that places more labels (always below `--container-graph`). A label that can't be placed shows its glyph straight above its mark (within 4px, no leader), or is dropped; the text alternative lists every mistake and blunder. Inaccuracies are labelled only in containers 768px and wider, in Literata 400 `ink-faint`, at least 24px clear of serious labels and at most one per 120px. Label text has a halo in `--eval-graph-surface` (default `sheet`), which a host on another surface sets.
- **Cursor:** a 1.5px `ink` rule marks the move shown on the board and moves with it.
- **Whose move (ruled value, controller ruling, Task 13 fix round 3):** when the player's colour is known, only the player's own mistakes and blunders are `pen` ticks with `pen-text` glyphs. The opponent's marks (any judgement) are `pencil` hairline ticks, 1px wide with no halo, the same length as their severity, and their labels (in `ink-soft`, glyph included) are placed only after all of the player's labels. Red therefore always means "your mistake", and dense clusters lose half their red. Contrast for a chart mark (3:1): `pencil` on `well` 4.93 day and 7.49 night, and 4.08 day and 5.88 night on the tinted `well`.
- **Corner labels:** "White" and "Black" (with "(you)" on the player's side) sit in the plot's top and bottom left corners, in `ink-soft` on a `well` halo. Each is measured in its real font, and its box grows by 3px (the line's stroke and halo). It moves to the right end only when nothing (no leader, tick or the eval line) touches it there; otherwise it stays left and its halo breaks what crosses it.
- **Lines over the tint:** phase boundaries and label leaders cross the area tint, where `rule-strong` drops to 2.51:1 (day). They use `pencil` instead: 4.08:1 day and 5.88:1 night on the tinted `well` (controller ruling, Task 13 fix round 1). The zero line stays `rule-strong`, because it is the tint's edge, with `well` on its other side.
- Long games scroll or compress inside the graph's own container, never the page.

### Eval bar

- A vertical bar to the left of the board, 16px wide and the board's height, with `--radius-board` and a 1px `rule-strong` outline. Below 768px it becomes an 8px horizontal bar above the board.
- White's share is `(wc + 1) / 2` on the same win-chance scale as the graph. Day: White is `sheet` and Black is `ink`. Night: White is `ink` and Black is `well`. White is at the bottom when the board is seen from White's side, and it flips with the board.
- The label (`+5.7`, `M2`) is Literata 600 at `2xs` (11px), set vertically (rotated −90°) inside the bar at the winning side's end, in the losing side's fill colour, so it is always ink on sheet or the reverse. On the 8px mobile bar, the label moves outside the bar's end.
- The boundary moves with the `swing` spring for changes below 0.3 win chance and with `swing-big` (visible overshoot) for changes of 0.3 or more.

### Chart series

For the KPI timelines (up to 12 series), we use a tonal ink ramp, not a rainbow:

- **Tones:** `ink`, `ink-soft`, `pencil` and `rule-strong` (all ≥ 3:1 on `sheet` and `well`).
- **Dashes:** each tone comes solid, dashed (6 3) and dotted (1.5 3). Four tones times three dash patterns give 12 distinct series.
- **Highlight:** one highlighted series is drawn in `ink` at 2.5px. While one is highlighted, the others drop to `rule-strong` at 1.5px.
- **Labels:** direct labels at the line ends, with the text in `ink-soft` (`ink` at 600 weight for the highlighted series), Schibsted 500 at `sm`. Beside each label is a 16px line swatch in the series' tone and dash pattern, so the label text keeps 4.5:1 while the swatch carries the identity. There is no legend box.
- Red and green are never series colours, because they keep their chess meanings.

### KPI stats (ADR-WEB-12)

A KPI is a list row, never a stat banner (`B9`): label and basis ("Based on 38 rapid games"), the value rolling with NumberFlow (Literata 600 at `3xl`, tabular lining figures), the credible interval as a band, and a sparkline. The label is a heading whose level the host sets (default `h3`).

- **Value precision:** percent to 0.1 (`64.2%`), decimal to 0.01 (`1.84`), integer whole.
- **Interval ends are rounded outward** at their shown precision: the low end floored, the high end ceiled, so the stated range is never narrower than the computed one and never crosses the rounded value. Decimals show ends one place coarser than the value (`61.0 and 67.5` beside `64.20`); percent and integer ends use the value's precision. The band is written out in words ("80% likely between 61.0 and 67.5"), so it never rests on the drawing.
- **Targets** use the ends' precision. When a target would read the same as an interval end, it gets one more place (`Next target 1.50` beside `between 1.5 and 2.2`). The fixture label is "Next target".
- **The scale (controller ruling, Task 14 review).** Every percent KPI keeps one **40-point window** and never falls back to 0–100%. When the band, value and target fit, the window is centred on them, with its start rounded to 10 points and widened back if rounding would cut them. Otherwise it is centred on the value. It is clamped inside 0–100%. A wider interval always *looks* wider: one that doesn't fit runs off the track's end, square-cut, with an open `rule-strong` chevron past the end ("interval continues"), so the overflow is shown by shape, not colour. The words stay exact. A target outside the window is named only in text. Other formats run from zero (or below it, for negatives) to a nice value about 15% past the band, value and target. Both ends are labelled in `xs` `ink-faint`.
- **Marks:** a decorative `rule` track, the band in `rule-strong` (8px, `--radius-board`, the same for provisional KPIs, never faded), the value as a solid 2.5px `ink` tick, and the target as a dashed 2px `good` tick with a caret, so shape and not colour tells them apart. The target's text label carries the same caret swatch. A provisional KPI shows "Early estimate" (a `well` pill, `ink-soft`).
- **Halo:** both ticks and the sparkline's last dot sit on a halo read as `var(--stat-surface, var(--color-sheet))`, the eval graph's `--eval-graph-surface` pattern. A host on another surface sets `--stat-surface`. The components and `tokens.css` never define it, so an ancestor's value always wins.
- **Sparkline:** no axes, `pencil` at 1.5px, linear segments, a null breaks the line (a lone value is a 3px pencil dot), and the latest value is a 6px `ink` dot. Its vertical span is at least `minSpan`, centred on the series range. KpiStat passes twice the current 80% interval width (a 90% interval is scaled by 1.2816 / 1.6449), so a move smaller than the uncertainty looks small. Fewer than two values reads "Not enough games yet", and its accessible name names the KPI ("Accuracy trend: not enough games yet"). Otherwise the name gives the snapshot count, the gaps and the first and last values.
- **Motion:** one clock per row. The sparkline reveals through one clip sweeping left to right over `--dur-reveal` on `--ease-pen`, so the pieces after a gap draw in order, and each dot fades in as the sweep reaches it. On a value change, the number rolls and the value tick travels on one progress value over the same `--dur-reveal` and `--ease-pen`. Reduced motion jumps both to the end (M10). No change is called "improved" or "worse" unless the intervals separate.

## Motion

Motion is a headline feature. The rule is rich, choreographed and physical, never generic, and the owner wants it **huge**: the analysis reveal is a full performance of the game's turning points, pieces have visible weight, and the eval bar swings. Motion (formerly Framer Motion) is used for all app animation, the View Transitions API for shared elements, and NumberFlow for rolling numbers. GSAP and Lenis are landing-only (M5).

### Duration tokens

| Token | Value | Use |
|---|---|---|
| `--dur-instant` | 90ms | press feedback, each arrowhead stroke, piece lift |
| `--dur-quick` | 160ms | menus, toggles, hover previews, board cross-fades, retracting marks |
| `--dur-base` | 240ms | panels, drawers, tab content, a captured piece flying off |
| `--dur-draw-small` | 240ms | one small stroke: graph marker rings, scoresheet rings, condensed redraws |
| `--dur-draw` | 420ms | one board stroke (ring or arrow shaft) |
| `--dur-reveal` | 600ms | the eval graph drawing across |
| `--dur-turn` | 520ms | a full page turn between routes |
| `--dur-stream` | 180ms | one replayed move in the onboarding stream |
| `--dur-soak` | 900ms | an ink-soak progress segment |

### Easing tokens

| Token | Value | Use |
|---|---|---|
| `--ease-out` | `cubic-bezier(0.2, 0.7, 0.2, 1)` | UI responses, lift |
| `--ease-pen` | `cubic-bezier(0.6, 0.05, 0.3, 1)` | pen strokes: a slow start, a committed sweep and a soft stop |
| `--ease-turn` | `cubic-bezier(0.7, 0, 0.2, 1)` | the outgoing page of a page turn |
| `--ease-soak` | `cubic-bezier(0.3, 0, 0.1, 1)` | progress fills soaking in |

### Spring presets

Every spring states its damping ratio, `ζ = damping / (2 × √(stiffness × mass))`, with mass 1 unless stated. Below 1 it overshoots. The overshoot is `exp(−ζπ / √(1 − ζ²))` of the distance travelled.

| Preset | Values | ζ | Overshoot | Use |
|---|---|---|---|---|
| `settle` | `{ stiffness: 520, damping: 42 }` | 0.92 | 0.06% | piece travel between squares (precise, no visible bounce), as for a knight or bishop. Other pieces use their row in the mass table. This is the brief's `piece`, renamed. |
| `land` | `{ stiffness: 700, damping: 22 }` | 0.42 | 24% | the landing: a piece's squash and y on arrival, glyph badges landing, streak ticks |
| `pen` | `{ stiffness: 150, damping: 24 }` | 0.98 | 0% | stroke progress when a stroke must follow input mid-draw (a user-drawn arrow, an interrupted sequence) |
| `swing` | `{ stiffness: 260, damping: 30 }` | 0.93 | 0.03% | eval bar and graph cursor for changes below 0.3 win chance. This is the brief's `evalBar`, renamed. |
| `swing-big` | `{ stiffness: 260, damping: 20 }` | 0.62 | 8% | eval bar for changes of 0.3 win chance or more: it overshoots past the new value and swings back |
| `page` | `{ stiffness: 300, damping: 34 }` | 0.98 | 0% | the incoming page of a page turn, layout morphs, and the Motion fallback for shared elements |
| `tilt` | `{ stiffness: 400, damping: 30 }` | 0.75 | 3% | a dragged piece's tilt returning to upright |

**Mass makes weight visible.** Every piece travels on its own `settle`, all at the same ζ (0.92), so they all come to rest the same way, and mass only slows them. Stiffness is `520 / mass factor`, and damping keeps ζ at 0.92 (`damping = 2 × 0.92 × √stiffness`, rounded to one decimal), with Motion's `mass` left at 1. Travel time grows with the square root of the mass factor: a queen takes 27% longer than a pawn to cover 90% of the distance. That 27% spread is accepted (controller ruling, Task 5 fix round): it follows from the 0.8 and 1.3 factors, and narrowing it would weaken the weight contrast the owner asked for. A heavier piece is never ahead of a lighter one on the way in. Checked by step response: the lighter piece leads until it reaches the target, and after that they differ by less than 0.03% of the distance (0.2px over eight 80px squares). Heavier pieces also land harder:

| Piece | Mass factor | `settle` values | ζ | 90% arrival | Lift scale | Landing squash (scaleX / scaleY) |
|---|---|---|---|---|---|---|
| Pawn | 0.8 | `{ stiffness: 650, damping: 47 }` | 0.92 | 138ms | 1.04 | 1.015 / 0.98 |
| Knight, bishop | 1.0 | `{ stiffness: 520, damping: 42 }` | 0.92 | 154ms | 1.05 | 1.02 / 0.97 |
| Rook | 1.15 | `{ stiffness: 452, damping: 39.2 }` | 0.92 | 165ms | 1.06 | 1.025 / 0.965 |
| Queen, king | 1.3 | `{ stiffness: 400, damping: 36.8 }` | 0.92 | 175ms | 1.07 | 1.03 / 0.96 |

### Stagger scale

| Token | Value | Use |
|---|---|---|
| `--stagger-tight` | 40ms | scoresheet lines annotating, list rows |
| `--stagger-base` | 80ms | report sections, critical-moment rings on the graph |
| `--stagger-pen` | 120ms | ring to arrow within one moment, and nothing else |

The badge is timed explicitly, not by stagger: it lands with `land` when the moment's last stroke finishes. That is at 420ms for a ring only, and at 540ms for a ring and an arrow. The arrow starts at 120, and its shaft reaches 85% of its eased progress at 120 + 266 = 386ms, so the head strokes run 386–476 and 416–506. The shaft's soft stop ends at 120 + 420 = 540.

### Overlay values

Floating things enter with a small, quiet move, and under reduced motion only their opacity runs (M10). Mirrored in `tokens.css` and `@ci/ui/motion` (`overlay`).

| Token | Value | Use |
|---|---|---|
| `--enter-dialog-y` | `8px` | a dialog starts this far below its place and rises in over `--dur-base` |
| `--enter-dialog-scale` | `0.98` | a dialog's starting scale |
| `--enter-float-scale` | `0.96` | popovers and tooltips grow from this scale at their trigger's origin over `--dur-quick` |
| `--offset-float` | `6px` | the gap between a trigger and its popover or tooltip |

### Physical feel

This is what the user should be able to see:

- **Lift and land.** On every move, the piece lifts over `--dur-instant` (uniform scale up to its lift scale from the mass table, y −4px, `--ease-out`). It travels on its `settle` from the mass table. At contact it is set to its landing squash (for a knight, scaleX 1.02 and scaleY 0.97, with `transform-origin: 50% 90%`, its base) at y 0, and `land` springs both axes back to 1. With ζ 0.42 it rebounds past 1 (scaleY about 1.007) before settling, so it is a real squash and stretch, using transform only. The move sound plays on the contact frame.
- **Momentum.** The eval bar uses `swing-big` for big changes, so it swings past the new value by about 8% and comes back. Stepping quickly through moves carries the spring's velocity from one value to the next.
- **Knights arc** (controller ruling, Task 6 fix round 2). A knight jumps; every other piece slides. During its travel the knight's lift grows into a raised arc driven by the travel spring's own progress `p`: scale = lift scale + 0.08 × sin(πp), and y = −4px − 0.2 square × sin(πp). At the midpoint a knight is at 1.13 and about 0.2 of a square above its line, so it visibly passes over the pieces it jumps (in the reference blunder, between d3's queen and e3's bishop) instead of sliding through them. Contact is unchanged: squash, then `land`.
- **Impact.** A capture knocks the captured piece off in the mover's direction (choreography 6).
- **Weight.** The mass table: a queen lands harder than a pawn.

### Principles

- `M1` **Every animation means something.** It shows what changed, what is ranked or where you are going, and it is user-caused or one of the named orchestrated moments. Nothing moves on its own to attract attention.
- `M2` **Pen strokes draw by `pathLength`,** revealing the filled outline through a mask. A ring starts at about 1 o'clock and overruns to 380°. One board stroke takes `--dur-draw` with `--ease-pen`, and a small stroke takes `--dur-draw-small`. A ring's time is split by arc: on the un-eased timeline, 0–360° gets the first 398ms of a 420ms stroke and the 360–380° overrun gets the last 22ms (360/380 and 20/380). One `--ease-pen` progress value drives both mask segments. With the easing applied, 360° is reached at about 318ms of real time and the overrun takes the last ~102ms, so the soft stop lands on the overrun and it visibly slows into place. Head timings are in eased progress too: a shaft reaches 85% at 63% of its duration (266ms of 420, 152ms of 240).
- `M3` **Arrowheads start when the shaft reaches 85%.** Each head stroke takes `--dur-instant`, and the second starts 30ms after the first.
- `M4` **Nothing fades up on scroll,** and nothing is scroll-triggered in the app (`B11`).
- `M5` **There are no hover lifts.** Hover changes tone, never position or shadow.
- `M6` **Physical, not decorative.** Pieces have mass and land, captures have impact, and the eval bar has momentum. There is no bounce without a physical cause.
- `M7` **Interruptible.** Input never waits for an animation. During an orchestrated sequence, any input on the board, the move list or the graph, the Skip button or Escape jumps to the end state: unfinished marks complete over `--dur-quick` (they are not snapped or dropped), pieces jump to their final squares, bars go to their final value on `swing`, and pending sounds are cancelled. Outside sequences, new input finishes or reverses in-flight motion, and springs keep their velocity.
- `M8` **Long sequences run in full once.** Anything over 1s runs in full only on the first view of that game or report. "Seen" is stored on the server per user, per game and per report, so it holds across devices. Later views run the condensed version (at most 600ms). The cap counts to the last scheduled event, which includes the badge landing (at 400ms) and the last graph ring ending (by 600ms). The `land` spring's settle after the badge lands is excluded. While a long sequence runs, a **Skip** control shows: a ghost button above the board, right-aligned in the board column's header row (never over the board or its corner squares), Schibsted 500 at `sm`, `ink-soft` text, `sheet` on hover, keyboard-focusable. Escape does the same. When the sequence ends, the control disappears and a polite live region announces "Analysis ready".
- `M9` **Performance budget.** Animate only `transform`, `opacity` and SVG `stroke-dashoffset` (including mask strokes). Never animate layout properties, `filter`, `box-shadow` or `clip-path`. The target is 60 fps on a mid-range phone, tested as a Pixel 6a-class device or Chrome DevTools at 4× CPU throttling, with no frame over 16ms and at most 12 SVG marks animating at once. `will-change` is set only while animating.
- `M10` **Reduced motion.** Under `prefers-reduced-motion: reduce`, choreography is replaced by instant state changes with a brief opacity cue: the changed element appears at full state with a single opacity fade of at most `--dur-quick` and no movement. Marks appear fully drawn, pieces jump, bars set their value, page turns become an instant swap, and long sequences show their end state. Sound is unaffected.

### Choreography

Times are in ms from the start of the moment. "Seeded" means deterministic per game.

**1. Analysis reveal: the performance.** This runs on the first view of a freshly analysed game (M8), and it is the "huge" beat. It replays the game's turning points: a prologue on the graph, then the three worst moments *played* on the board in ascending severity, so it builds and ends on the worst.

*Prologue (0–1200, fixed).* `n` is the number of marked moments (blunders, mistakes and inaccuracies). Real club games have 8–15 of them.

| Time | What happens |
|---|---|
| 0–600 | The eval graph draws left to right: the line's `stroke-dashoffset` over `--dur-reveal` with `--ease-pen`, and the area fill revealed by a mask rect with `scaleX` from 0 to 1. |
| 600–1200 | Graph marker rings draw in game order, each over `--dur-draw-small` (240ms). The stagger is capped at `s = min(80, 360 / (n − 1))` ms (80ms when `n = 1`), so the last ring starts by 600 + 360 = 960 and ends by 1200, whatever `n` is. Each label and leader tick fades in while its ring draws: it starts at ring start + 120ms and fades over the ring's remaining 120ms, so it is fully in when its ring ends, and the last label is in by 1200. |
| 1200 | The climax starts, always at this fixed time. |

Worked examples: with `n = 5`, `s = min(80, 90) = 80`, and the last ring runs 920–1160. With `n = 15`, `s = min(80, 360 / 14) = 25.7`, and the last ring starts at 600 + 14 × 25.7 = 960 and ends at 1200.

*Climax: one beat for each of the top three moments, in ascending severity (third worst, second worst, worst).* A beat lasts 2060ms, and the last beat lasts 1900ms because it doesn't retract. The beats start at t0 = 1200, 3260 and 5320.

| t0 + | What happens |
|---|---|
| 0–160 | The board cross-fades (`--dur-quick`) to the position **before** the bad move. The eval bar swings to the pre-move value on `swing`, the graph cursor moves to that ply on `swing`, and the scoresheet row gets the selected `well`. |
| 160–400 | Hold. The eye reads the position. |
| 400–490 | The mover lifts (`--dur-instant`). |
| 490–760 | The **move is played**: travel on the piece's `settle` from the mass table (it is within 1% of the square at 214ms for a pawn and 271ms for a queen). |
| 760 | Contact: the piece lands with its squash on `land`, and the move or capture sound plays. A capture knocks the captured piece off. |
| 760 | The eval bar swings to the post-move value: on `swing-big` (visible overshoot) when the change is ≥ 0.3 win chance, otherwise on `swing`. The graph cursor steps on `swing`. |
| 860–1280 | The pen ring draws on the destination (`--dur-draw` with `--ease-pen` over the whole stroke: 398 + 22ms by arc on the un-eased timeline, so 360° arrives at about 1178 and the overrun slows in until 1280). In parallel, the scoresheet row gets its glyph and evaluation (opacity over `--dur-quick`), and its scoresheet ring draws (`--dur-draw-small`). |
| 980–1400 | The arrow draws (`--stagger-pen` after the ring): the pen arrow of the played move for a blunder, or the good arrow of Stockfish's move otherwise. The shaft reaches 85% at 1246, the head strokes run 1246–1336 and 1276–1366, and the shaft's soft stop ends at 1400. |
| 1400 | The glyph badge lands on `land` as the last stroke ends. |
| 1400–1900 | Hold, with every mark visible. |
| 1900–2060 | Retract (first and second beats only): every mark reverses its `stroke-dashoffset` over `--dur-quick`, and the badge scales to 0.8 and fades. The next beat starts at 2060. |

**Total:** 1200 + 2 × 2060 + 1900 = **7220ms**. The sequence ends on the worst beat's 500ms hold, with its marks drawn, and the board stays on the worst moment (the end state). There is no separate finale and no redraw. With `k` moments in the climax (k ≤ 3), the total is `1200 + (k − 1) × 2060 + 1900`. With no marked moments, the sequence ends after the 600ms graph draw, on the final position.

*Condensed version* (M8, later visits, at most 600ms):

| Time | What happens |
|---|---|
| 0 | The graph appears fully drawn, and the board shows the worst moment (the position after the move). |
| 0–600 | Graph marker rings draw over `--dur-draw-small`, with the stagger capped at `min(40, 360 / (n − 1))` ms (40ms when `n = 1`), so the last ring starts by 360 and ends by 600. Labels fade in as in the full version, from ring start + 120ms. |
| 0–392 | The board marks draw: the ring over `--dur-draw-small` (0–240), and the arrow from 120 (shaft 120–360, reaching 85% at 272, with heads 272–362 and 302–392). |
| 400 | The badge lands on `land`. |

Worked example: with `n = 15`, the stagger is `min(40, 25.7) = 25.7`, and the last graph ring runs 360–600.

**2. Report reveal.** This runs on the first view of a report, for the sections in the first viewport only. Sections below the fold are simply there (M4).

| Time | What happens |
|---|---|
| 0–600 | The report's eval graph or timeline draws as in the analysis prologue. |
| 200 onward | Weakness sections assemble in ranked order, `--stagger-base` apart. Each section's ruled top line draws with `scaleX` from its left end (`--dur-draw-small`, `--ease-pen`). When the line reaches 60%, its heading and figures appear with opacity only (`--dur-quick`), with no translate. Its rank number and counts roll with NumberFlow. |
| end + 0–420 | The top weakness's example position draws its pen mark. |

This is not a generic fade-up: it runs once, in rank order, never on scroll. The ruled line is what moves.

**3. Onboarding stream.** Games stream onto a live board while they import and analyse.

- Each replayed move takes `--dur-stream`: travel on `settle` and land on `land` with the lift scale reduced to 1.03, capped at 5 moves a second.
- Between games, the board does a mini page turn (240ms) to the next game's start.
- The games counter rolls with NumberFlow each time a game finishes, and the progress bar soaks forward with `--dur-soak`.
- If games arrive faster than they replay, the board skips to the latest game's final position and continues from there. It never queues a backlog.

**4. Shared-element morphs.** These use the View Transitions API, with Motion `layoutId` and the `page` spring as the fallback. **On a route change that has a shared element, the morph replaces the page turn.** The two never run together.

- **Position thumbnail to full board:** 360ms. The board morphs, the move list and graph fade in 120ms after the morph ends, and marks then draw (as in the condensed reveal, or the full reveal on first view).
- **Weakness card to area page:** the card becomes the page header in 420ms, and the page content arrives 120ms later with opacity only.
- **KPI tile to chart:** the tile's number travels to the chart title in 360ms, then the chart line draws over `--dur-reveal`.

**5. Page turn (route change without a shared element).** This is never a cross-fade. The header and navigation stay still, and only the sheet turns. The sheet container has `perspective: 1600px`, and both sheets have `backface-visibility: hidden`.

| Time | Outgoing sheet | Incoming sheet |
|---|---|---|
| 0–260 | `transform-origin: left center`. It goes from `rotateY(0) translateX(0)` to `rotateY(-14deg) translateX(-24px)`, with opacity 1 to 0, on `--ease-turn`. | waits |
| 140–520 | gone | `transform-origin: right center`. It goes from `rotateY(14deg) translateX(24px)` to `rotateY(0) translateX(0)` on the `page` spring. Opacity goes 0 to 1 over the first 160ms. |

Back navigation mirrors it: the outgoing sheet turns on its right edge toward +14deg and +24px, and the incoming sheet comes from the left.

**6. Piece moves, captures and drag.**

- **Click or keyboard move:** lift, travel and land as described under "Physical feel". The sound plays on the arrival frame.
- **Capture:** the captured piece is knocked off at contact, when the mover is within 30% of a square. It travels 0.6 of a square further in the move's direction, rotates 12–20° (seeded) the way the mover was travelling, scales to 0.9 and fades out over `--dur-base` with `--ease-out`. The capture sound plays at contact.
- **Drag:** on pickup the piece scales to its lift scale over `--dur-instant`, and the origin keeps a 30% ghost. Its `--shadow-lift` lives on a separate layer under the piece (a pseudo-element or sibling with the shadow already applied), and only that layer's opacity animates, 0 to 1 over `--dur-instant` and back on drop. `box-shadow` itself is never animated (M9). While dragging it tilts with the pointer's horizontal velocity `vx` in px/s: `rotate = clamp(vx × 0.008 deg per px/s, −8°, +8°)`, so 1000px/s gives the full 8°. When the pointer slows, the tilt returns to 0° on `tilt`. On drop, the piece travels to the square centre on `settle`, then lands on `land`. An illegal drop sends the piece back to its origin on `settle` with no landing squash, plus the drill-wrong cue where it applies.

**7. Micro-interactions.** Each one shows what changed.

- **Drill right:** the piece lands on `land`, a good ring draws on the destination (`--dur-draw`), a `!` badge lands, the streak count ticks up with NumberFlow and its number lands on `land`, and the correct sound plays.
- **Drill wrong:** the piece springs back to its origin on `settle`, a pen `?` badge lands on the attempted square for 900ms, then fades. The wrong sound plays. If the user asks for a hint, a pencil arrow draws.
- **Button press:** the pen-press, scale 0.97 over `--dur-instant`, released on `land` (a small visible rebound).
- **Scoresheet hover preview:** hovering a move shows that move on the board as a pencil arrow drawn over `--dur-quick` and a 40% ghost of the piece on its destination. Moving off retracts it.
- **Progress:** fills soak forward like ink, with `scaleX` on `--ease-soak` over `--dur-soak` per update. It never blocks with a spinner (`B18`).

### Still banned

Fade-up on every section, hover lift and shadow on cards, parallax and scroll-jacking in the app (`B11`), bounce or wobble with no physical cause, and any animation that blocks input.

## Sound

The owner asked for "sounds for moving pieces". The character is **paper and wood: quiet, close and dry, like a club game in a library**. It is never arcade, never bright synth bleeps, and never louder than a pen on paper. Synthesis parameters are left to Task 12b.

| Sound | Character | Plays when |
|---|---|---|
| Move | a soft wooden placement | a piece lands (the arrival frame) |
| Capture | heavier, with a slight knock | a capture's contact frame (choreography 6) |
| Check | a brighter tick | a move gives check, layered on the move or capture sound |
| Castle | two quick placements, 70ms apart | king lands, then rook lands |
| Promotion | a short rising tone | the promoted piece appears |
| Game end | a low, settled close | mate, resignation or draw |
| Drill correct | a short warm chime | with the good ring |
| Drill wrong | a muted low thud, never harsh | with the piece springing back |
| Pen scratch (optional) | a very faint scratch | a red-pen mark draws. Off by default. |

**Priority when events coincide:**

- A capture replaces the move sound.
- Check layers on top of the move or capture sound, 20ms after it.
- Game end replaces the final move's sound entirely: no move, capture or check sound for the mating move.
- A promotion with capture plays both, capture first and the promotion tone 60ms later.
- Castling is the castle sound only (its two placements), not two move sounds.

**Timing.** Sound is scheduled against `AudioContext.currentTime` to land on the visual contact frame. When a move starts, the board predicts the contact time from the spring and schedules the sound at `ctx.currentTime + Δ` (Δ ≤ 400ms). If the motion is interrupted, the scheduled sound is cancelled and rescheduled for the new contact. Sound is never fired from a timer that can drift from the animation.

Rules (spec §11):

- **Original sounds only.** They are synthesised with the Web Audio API, or recorded CC0 samples with every source logged. Never copy sounds from chess.com, Lichess or any other product.
- **Sound starts only after the first user gesture,** because of browser autoplay rules. A first-view reveal on a cold page load (no gesture yet) therefore plays silently. Its sounds are not queued, and they are not replayed after the first gesture. Sound simply starts with the next sounded event.
- There is a **global mute and volume control** in the header and in settings, remembered per user. The **default volume is 60%**. Board sounds are on by default, and ambient sounds (the pen scratch) are off by default.
- **`prefers-reduced-motion` does not mute sound.** Sound has its own setting.
- **Sound never carries information alone.** Every sound has a visual equivalent.
- **Throttling:** fast replay and scrubbing don't play a sound per move. Only stepping and play do. The onboarding stream plays at most one move sound per 250ms, plus the game-end sound.
- **One module:** `useBoardSounds()` in `@ci/ui`, with one shared `AudioContext` and preloading, triggered from board events worked out with chess.js. Every board in the app sounds the same.

## Ban list

Every later task cites these IDs. A review that finds one names its ID.

- `B1` Warm cream with terracotta.
- `B2` Near-black with one acid accent.
- `B3` Broadsheet: hairlines everywhere, zero radius, newspaper columns.
- `B4` SaaS card kit: identical rounded cards, one radius, grey shadow.
- `B5` Template chrome: ALL-CAPS tracked eyebrows, `A · B · C` meta strings, `WORD — fragment` labels, `#0B0B0B` or `#111`, monospace data labels, `→` on links.
- `B6` Default fonts: Inter, Geist, Space Grotesk, Instrument Serif, the system stack.
- `B7` Purple or indigo accents, gradient text or washes, glows, glassmorphism.
- `B8` A badge above an H1.
- `B9` Icon-topped 3-up feature grids, stat-banner rows, and "1-2-3 steps" unless the content is a sequence.
- `B10` Coloured top or left borders on cards.
- `B11` Fade-up on every section, hover-lift on every card, parallax, scroll-jacking.
- `B12` Aceternity, Magic UI, three.js, chessground, chessops.
- `B13` Lucide or emoji icons.
- `B14` Stock shadcn tokens (zinc, `--radius: 0.625rem`, Vega style).
- `B15` Lorem ipsum or invented chess data.
- `B16` Accenting one word in a headline (italic, bold or colour).
- `B17` Grey body text on dark.
- `B18` Blocking spinners. Use the progress placeholder instead.

## Copy

- Sentence case, British spelling ("analysing", "colour", "practise" as the verb), and active verbs: "Review game", "Start drill", "Connect chess.com".
- An action keeps its name from button to toast: "Connect chess.com" leads to "chess.com connected".
- Errors say what happened and how to fix it, with no apology: "This link has expired. Request a new one."
- No `→` on links or buttons, no all-caps labels, and no lorem ipsum. Use real games, real evaluations and real names from the start (`B15`).

## References

Six annotated references live in `design/references/`. `REFERENCES.md` there says what each one gives us, what not to copy, and its sources and licences. All of them were drawn from scratch in these tokens with a real game (Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858), real Stockfish 19 evaluations and the default Cburnett pieces.

1. `01-analysis-density-and-eval-graph.png`: move-list density, the win-chance eval graph with rings and a label lane, and the eval bar.
2. `02-arrow-semantics-and-pen-stroke.png`: hand-drawn versus mechanical strokes, the severity scale, marks in day and night, and the draw sequence in ms. Frame 3's caption (520ms) is "ring complete (360° at 318ms, overrun to 420ms); heads from 386ms". Where the PNG's caption differs, this text wins.
3. `03-scoresheet-grid-and-corrections.png`: the ruled grid, roman 600 moves, scoresheet rings and margin comments.
4. `04-book-notation-and-diagram-caption.png`: notation in running text and the captioned diagram.
5. `05-night-study-analysis.png`: reference 1 in night study.
6. `06-piece-set-candidates.png`: the three researched piece sets side by side (Cburnett is the default, Chessnut is an option, and RhosGFX was rejected).

## Styleguide hero (for `/styleguide`)

The first thing on `/styleguide` is the system doing its job on a real game, not a swatch grid. The scoresheet sits directly under the board so the link mark can rise from the ringed move into its square; the eval graph is the second loud surface beside the board; the coach's comment is a margin comment on the sheet (reference 3). Decision: `design/decisions/styleguide-hero.md` (Task 6, variant A).

```
+----------------------------------------------------------------------+
| Chess Improver                                      Day  Night       |
+----------------------------------------------------------------------+
| +-+ +---------------------------+   Game title (h1, Literata 600 3xl)|
| |e| | board in its 8px well     |   one line of context, side to move|
| |v| | (the key move is played:  |                                    |
| |a| |  lift, travel, land; pen  |   [ label lane: the key move     ] |
| |l| |  ring, arrow, ?? badge)   |   [ eval graph, win-chance scale,] |
| | | |            ^ link         |   [ rings on every mark, cursor  ] |
| +-+ +------------|--------------+   [ on the move shown            ] |
|                  | (gutter)                                          |
| +----------------|----------------------------------------------------+
| | 28 | (Nd2??) −1.1 | e4?! −0.2   |  Margin comment, italic, level  |
| | 29 |  Nxe4        | Bxf3+       |  with the ringed row            |
| | 30 |  Kd2         | Ra8?! +0.1  |                                 |
| +---------------------------------------------------------------------+
|  Tokens follow below as plain sections: colour, type, shape, motion. |
+----------------------------------------------------------------------+
```

- **From 1024px:** two columns: the eval bar, the board in its well, and then the header and the eval graph beside them (the graph's bottom level with the board's). The sheet spans both columns: the ruled table under the board, and the comment in the sheet's right margin, level with the ringed row.
- **On a phone:** board (with the 8px horizontal eval bar above it), scoresheet, note, then graph.
- **`--hero-graph-height`** (hero-only token in `tokens.css`, controller ruling, Task 6 fix rounds 3 and 4): `calc(--board-size + 16px)`, the height of the board in its well. From 1024px the right column is exactly this tall: the header sits at its top, and after a 24px gap the eval graph (label lane and plot) fills the rest, so the graph's top meets the header block and its bottom meets the board's. The plot never drops below its 120px desktop height. Elsewhere the graph keeps its real sizes.
- **The excerpt** opens on the ringed move and ends before the next blunder, so no second `??` competes with the key move. The hero's label lane names only the move on the board (accepted by controller ruling: the hero tells one moment).
- **One pen mark in the hero graph** (controller ruling, Task 6 fix round 4): only the key move gets a pen ring. Every other mistake, blunder or inaccuracy in the hero graph is a quiet `pencil` hairline tick in the opponent-mark style of "Whose move" (1px, no halo, severity length), so the `??` stays the only loud red and narrow graphs don't crowd.
- **The graph's own beat** (controller rulings, Task 6 fix rounds 4 and 5): when the eval graph is fully in view at the start (an `IntersectionObserver`, as for the link), the hero opens with the graph prologue. Over `--dur-reveal` on `--ease-pen` the eval line draws in and a left-to-right mask reveals the tint and ticks. The key move's beat (choreography 1's climax rows) then starts at 600ms, so everything shifts by `--dur-reveal`: lift 1000, contact 1360, rings 1460, badge and link 2000–2420, comment 2420. At contact the cursor drops onto the key move, growing down from the plot's top edge on `swing` while the eval bar swings on `swing-big`. The cursor is `ink` in day and `ink-soft` at night (9.94:1 on night `well`), so it doesn't compete with the key ring. When the graph is not in view at the start (phones, where it sits below the fold), there is no prologue and no stillness: the line is already drawn, the key move's beat runs at the original climax times (lift 400, contact 760, badge and link 1400–1820, comment 1820), and the cursor simply rests on the key move.
- Left aligned. The heading is the game itself, set in Literata 600 at `3xl`. There is no marketing headline, eyebrow or badge. On first view, the hero plays the key move's beat: the climax rows of choreography 1, then the link mark, then the comment.

## How this direction was chosen

### Plan pass

- **Direction:** the owner's pick from §11, "Annotated scoresheet".
- **Colour:** paper `#F3F5F8`, sheet `#FBFCFD`, navy ink `#16233B`, red pen `#D3312B`, green `#2F7D5B` (now `#24684B`), pencil `#56657F`.
- **Type:** Literata for anything a coach would write (headlines, notation, comments), and Schibsted Grotesk for anything the app says (controls).
- **Layout:** the board left with its eval bar, the eval graph beside it, and the ruled scoresheet under the board with the coach's comment in its margin. On a phone: board, scoresheet, note, then graph. Everything left aligned. (Revised in Task 6 fix round 2; the original plan put the scoresheet right of the board and the graph under both.)
- **Principles:** boldness on the board, the eval graph and motion; red means a mistake; tone instead of shadow.

### Cliché review and what changed

The plan was checked against the five common generated looks:

1. **Cream and terracotta (`B1`).** The paper is cool blue-grey, not cream, and the red is a true pen red, not clay. A big serif headline on light paper still drifts toward this look, so a guard was added: Literata stays at text and title sizes in the app (`3xl` for page titles, `4xl` rarely), `5xl` is landing-only, and the styleguide hero is the game, not a display headline.
2. **Near-black with an acid accent (`B2`).** Night study is navy (`#121A2B`), not `#0B0B0B`. It has two semantic accents, not one decorative one, and the body text is near-white `ink`, not grey.
3. **Broadsheet (`B3`).** Ruled lines are scoresheet-native, which borders on this look. **Changed:** the first draft ruled the whole styleguide page into sections. Ruling is now only on scoresheet and move-list surfaces, panels keep `--radius-sheet`, and there are no newspaper columns.
4. **SaaS card kit (`B4`).** There are no card shadows. Surfaces separate by tone, and radii vary by role (2, 6, 10 and pill). There are no gradients.
5. **Template chrome (`B5`).** **Changed:** the first draft put small letter-spaced section labels above each styleguide block and set evaluations in a monospace face. Headings are now plain sentence case, and evaluations use Literata's tabular lining figures. The game line used a middle-dot meta string, and it is now a plain sentence.

Two more changes came from the review. The first draft let the hero headline carry one red word for emphasis, which is `B16`, so it was removed. The first draft also used `pen` for destructive buttons. Red is now reserved for chess mistakes and form errors, and destructive actions are ink-outlined with an explicit verb ("Delete account").

## Revisions

### 2026-11-21, initial self-review

The implementing agent checked the same two questions as the brief's Step 4 review: does any section read like a default for a similar product, and does any token pair fail its stated contrast? It found:

- No token pair failed. Coordinates needed a theme-specific rule.
- The last-move tint is supplementary by design.
- The halo rule is load-bearing.
- The reference 2 pencil arrow overlapped the green one, so it was moved.

### 2026-11-21, fix round 1: fresh design critique (Step 4) and new owner directions

**New owner directions:**

- *"The animations need to be huge, that makes it premium."* The motion section was rewritten as a full system: duration, easing and stagger tokens; the spring presets `settle`, `pen`, `swing` and `page`; principles `M1`–`M10`; choreography in ms for the analysis reveal, report reveal, onboarding stream, shared-element morphs, page turn, captures, drag tilt and micro-interactions; the performance budget; interruptibility; first-view-only long sequences; and reduced-motion fallbacks. The direction now names motion as a place for boldness.
- *"Sounds for moving pieces."* A new Sound section covers the character, the sound list and the rules from spec §11.

**Critic findings and fixes:**

1. **Critical: night focus ring.** `ink` on night `board-light` is 2.82:1. Focus geometry is now specified (2px `ink`, 2px offset off the board; on the board an inset ring on a 2px `sheet` halo), and board rows were added to the contrast table, all ≥ 3:1.
2. **Critical: the stroke didn't look hand-drawn.** "Pen stroke geometry" now specifies variable-width filled outlines (2.2, 3.2, 1.4px), the tilted ellipse with ±6% drift, the 380° spiral overrun 2.5px outside the start, a 2–4% shaft bow, unequal head strokes (0.9 and 1.1 at 26° and 32°), widths per mark type, seeded determinism and the mask reveal. The geometry is no longer deferred to code. Reference 2 was redrawn with a large mechanical-versus-hand comparison.
3. **Pieces.** A new Pieces section covers style, day and night fills, and the night outline (`well`, 5.51 and 3.05:1). Three licensed candidates were researched, and Cburnett (BSD-3-Clause option) is recommended. All references were regenerated with it (no Unicode glyphs), reference 6 compares the three, and `docs/decisions/ADR-WEB-n-piece-set.md` holds the decision for the owner.
4. **Severity and colour-blind redundancy.** The severity scale (`??`, `?`, `?!`, `!`, `!!`, plan) comes from Lichess win-chance thresholds, with glyph badges and head shapes as the redundant channel. Glyphs are kerned tight to moves. **Darkening `good` to `#24684B` was considered and not done.** It only raises pen versus good to 1.34:1, the brief's values are binding without a contrast failure, and shape redundancy solves the problem. It remains an owner option.
5. **Eval graph.** It now uses the win-chance mapping, pen-ring markers, a label lane with leader ticks, and heights of 120 and 88px. The eval bar spec was added. Reference 1 was redrawn.
6. **Reference 3.** Moves are now in Literata roman 600, the grid is wider, and only documented marks are used: the "scoresheet ring" is defined for blunders and mistakes, and inaccuracies get the glyph only.
7. **Missing tokens and rules.** Added: interaction states (the tonal ladder), `::selection` (`--color-selection`), board states (`--color-board-select`, `--color-board-premove`, legal dots, check in ink, drag ghost), the eval bar, the chart series ramp (4 tones × 3 dashes and one highlight), warning, info and success semantics (form errors keep red), the Literata `opsz` mapping, Phosphor at 20 and 16px, spacing roles (`--space-move-row` 26px, `--space-panel-pad`, `--space-gutter`), board sizes per breakpoint, and panel headings in Literata 600 at `xl`.
8. **Night study reference.** Reference 5 adds reference 1 in night. **A livelier night `board-light` was considered and not done.** No contrast failure forces it, and with the halo rule and the `well` piece outline, the current squares pass everywhere they need to. It remains an owner option.
9. **Minor.** "Thinner than pen and green" became "pencil 0.7× (thinner than pen and good strokes)" under "Pen stroke geometry".

**Other changes:**

- The spring names changed, but no brief value did. `piece` is now `settle` and `evalBar` is now `swing`, each with the brief's exact values, to match spec §11's preset names.
- Principle 6 ("everything shown in its final state at once") became `M10`, which allows one opacity cue of at most 160ms and no movement, to match the updated spec §11 ("instant state changes with brief opacity cues").
- `--shadow-lift` now also covers a dragged piece, because it floats.

**Token values from the brief changed in this round:** none. All additions are new tokens derived from existing values (`--color-selection`, `--color-board-select`, `--color-board-premove`, `--color-piece-dark`, `--color-piece-light`, spacing roles, motion tokens).

### 2026-11-21, fix round 2: re-critique ("polished but timid, not huge") and owner decisions

**Owner decisions (binding):**

- **A. Colour, owner-approved token changes.** The brief's values changed only here:

| Token | Before | After | Why |
|---|---|---|---|
| `--color-good` (day) | `#2F7D5B` | `#24684B` | Darker, so pen and good also differ by luminance: 1.01:1 became 1.34:1. On `sheet` it is now 6.48:1. |
| `--color-good-text` (day) | `#2A7353` | `#24684B` | It now matches `good` and passes 5.56:1 on `well`. |
| `--color-good` and `--color-good-text` (night) | `#5DBB8E` | `#76D3A4` | The night counterpart goes lighter (the pen is the darker of the two at night): pen versus good went from 1.29:1 to 1.67:1, and it is 8.75:1 on `sheet`. |
| `--color-board-light` (night) | `#7F8DA8` | `#8796B5` | Livelier: brighter and a touch bluer, still cool (`B1`). |
| `--color-board-dark` (night) | `#55637F` | `#5A6A8C` | Livelier, and it lifts the night piece outline from 3.05 to 3.40:1. |

The full contrast script was re-run over every pair, and there are 0 failures. The tightest pairs are now `rule-strong` on day `well` (3.03), night pieces on `board-dark` (3.40) and night coordinates in `ink` on `board-dark` (4.57, which passes 4.5). References 1–6 were regenerated with the new values.
- **B. Piece sets are user-configurable.** A new "Piece sets" subsection covers the Settings control, recolouring rules, the licence gate, `/licenses` attribution and a reserved slot for a commissioned signature set. Cburnett (BSD-3-Clause) is the default, and Chessnut (Apache-2.0) is an option. `ADR-WEB-n-piece-set.md` is now Accepted.

**Critic important issues:**

1. **Springs.** Every spring now states its ζ and overshoot. Three overshooting presets were added: `land` (700/22, ζ 0.42, 23%) for landing scale and y; `swing-big` (260/20, ζ 0.62, 8%) for eval-bar changes ≥ 0.3 win chance; and `tilt` (ζ 0.75). Mass by piece type makes weight visible. A new "Physical feel" subsection says what the user should see. `settle`, `swing`, `pen` and `page` keep their values for travel.
2. **The analysis-reveal climax** is fixed at 1200ms. Each of the top three moments now plays the position before the move, then the move itself (lift, travel, `land` and sound, with a `swing-big` bar), then the ring, arrow and badge. Marks retract (reverse dashoffset over `--dur-quick`) and the board cross-fades between moments. The finale returns to the worst moment with a condensed redraw. It runs about 8s in full on first view, with Skip available.
3. **Page turn:** `perspective: 1600px`. The outgoing sheet uses `transform-origin: left center` to `rotateY(-14deg) translateX(-24px)`, and the incoming sheet mirrors it from the right. Shared-element morphs replace the page turn on routes that have one.
4. **Pen determinism:** the drift is now specified as integer frequencies (2 and 3 per revolution, 4% and 2%), so it is identical at θ and θ + 360°. Seeding is FNV-1a 32-bit into mulberry32, with a fixed draw order. The mask stroke is the local width plus the halo plus 1px, and the last 20° gets its own mask segment. rx/ry went from 1.06 to 1.12 (ry 38% of the square). Reference 2 was redrawn so the hand arrow reaches its ring and shows its bow.
5. **Scoresheet rows:** I chose the separate `--space-scoresheet-row: 34px` token. The move list stays at 26px. The scoresheet ring is 0.8× width and lives only in 34px rows (reference 3 matches).
6. **Chart labels:** text is in `ink-soft` (`ink` 600 when highlighted), with a 16px line swatch in the series' tone and dash beside it.
7. **Reveal interruption:** any board, move-list or graph input, Skip or Escape jumps to the end state, and unfinished marks complete over `--dur-quick`. "Seen" is stored on the server per user, per game and per report. Skip is a ghost button at the top right of the board, Schibsted 500 at `sm` (M7, M8).

**Critic minors:**

- `--dur-draw-small` (240ms) is named. `--stagger-pen` is ring to arrow only, and the badge time is explicit (420 or about 600ms). The unused `--stagger-beat` was removed: the climax has its own timeline.
- The inaccuracy rule no longer mentions the scoresheet: inaccuracies get a ring only as a graph marker.
- Focus on the board is a dashed (4 3) inset ring, distinct from the solid 3px check ring, and it sits inside the check ring when both apply.
- Drag tilt is in px/s: `clamp(vx × 0.008 deg per px/s, ±8°)`.
- Sound now covers event priority, the 70ms castle gap, 60% default volume, and scheduling on `AudioContext.currentTime` at the visual contact frame.
- Reference 4 shows the check ring on e8. References 1 and 5 show the eval bar label: the bar is now 16px wide, with the label vertical inside it.
- The report reveal no longer uses a 6px translateY. Only the ruled line moves, and numbers roll.
- The badge text has an 11px floor, and two-character glyphs are 28% of the square.

**Token values from the brief changed in this round:** `--color-good` and `--color-good-text` (both themes), and night `--color-board-light` and `--color-board-dark`. All are owner-approved (decision A above). No other brief value changed. New tokens: `--dur-draw-small`, `--space-scoresheet-row`, and the springs `land`, `swing-big` and `tilt`.

### 2026-11-21, fix round 3: re-critique ("the right kind of big"), polish

**Important:**

1. **Prologue stagger overflow.** I chose the capped stagger, not grouping inaccuracies. Graph rings stagger by `min(80, 360 / (n − 1))` ms in the full version and `min(40, 360 / (n − 1))` ms in the condensed one, so the prologue always ends by 1200ms and the condensed version by 600ms. Worked examples for `n = 5` and `n = 15` are in choreography 1.
2. **Climax order.** The top three now play in ascending severity (third, second, worst), so the sequence builds and ends on the worst. The duplicate finale was removed: the last beat doesn't retract and ends on its 500ms hold. The total is now 1200 + 2 × 2060 + 1900 = 7220ms (was about 8000ms). Both timeline tables were updated, and the condensed version got its own table.

**Minor:**

3. Each beat's 0–160ms row now swings the eval bar to the pre-move value on `swing`.
4. A `2xs` step (11/14, `opsz` 11) is used only by the badge text floor and the eval bar label. The eval bar was removed from the `xs` row.
5. A ring's 420ms is split by arc, 398ms for 0–360° and 22ms for 360–380° on the un-eased timeline, with one `--ease-pen` progress driving both mask segments (M2 and the climax table). I also found that every head and badge time had been computed linearly. With `--ease-pen`, a shaft reaches 85% at 63% of its duration, so the heads, the badge times (540 standalone, 1400 in the climax, 400 condensed) and the hold (now 500ms) were corrected. Reference 2's frames now use eased progress, with matching captions.
6. The drag shadow lives on its own layer, and only its opacity animates. `box-shadow` is never animated.
7. Landing is a non-uniform squash (knight: scaleX 1.02, scaleY 0.97, origin at the base), springing back on `land` with a real rebound. Squash amounts are now per piece in the mass table.
8. Skip sits above the board, right-aligned in the board column's header row.
9. Overshoots corrected: `settle` 0.06%, `swing` 0.04% and `land` 24%. The piece-set ADR is numbered **0039**: renamed to `docs/decisions/0039-piece-set.md` and listed in `docs/decisions/README.md`.
10. Sound: a cold-load first-view reveal is silent (autoplay), and its sounds are not replayed after the first gesture.

**Token values from the brief changed in this round:** none. New: the `2xs` type step.

### 2026-11-21, Task 5: tokens implemented, minor fixes

`tokens.css` and `@ci/ui/motion` now implement every token here, and `packages/ui/test/tokens.test.ts` and `motion.test.ts` parse this file's tables and fail on any drift, including every ratio in the contrast table and every spring's ζ and overshoot (ADR-WEB-7).

1. **Condensed stagger, `n = 1`.** The condensed graph-ring stagger is 40ms when `n = 1`, as the full version's is 80ms.
2. **Prologue labels end by 1200.** Labels and leader ticks no longer fade in after their ring (the last one would have ended at 1360). Each starts at ring start + 120ms and fades over the ring's remaining 120ms, so the last is in by 1200 when the climax starts.
3. **The condensed cap.** M8's 600ms cap counts to the last scheduled event, including the badge landing, with the `land` spring's settle excluded.
4. **Reference 2, frame 3.** The caption text is "ring complete (360° at 318ms, overrun to 420ms); heads from 386ms" (in References here and in `REFERENCES.md`). The PNG is not re-rendered.
5. **`swing` overshoot.** The token test recomputed it as 0.035%, so the table now says 0.03% (round 3 had 0.04%). No spring value changed.
6. **Fonts.** Literata and Schibsted Grotesk are self-hosted through `next/font/local` from committed WOFF2 files (OFL 1.1), replacing `next/font/google`, so builds never need the network.

**Token values changed in this round:** none.

### 2026-11-21, Task 5 fix round

1. **Mass now reads as weight.** The old table varied Motion's `mass` at a fixed stiffness and damping. That changed ζ as well as speed, so heavier pieces were *faster* to 90% (queen 151ms, pawn 159ms) and overtook lighter ones by up to 1.4% of the distance at about 200ms. Now ζ stays at 0.92 and only the speed changes: stiffness is `520 / mass factor`, damping holds ζ, and the queen and king factor is 1.3 (was 1.25). The 90% arrivals are now 138, 154, 165 and 175ms, and a step-response test checks that no heavier piece is ahead of a lighter one on the way in. The two choreography lines that said "with its mass" now point to the table.
2. **New tokens for existing values:** `--tracking-glyph` (-0.01em, from Notation), `--board-size` (the board sizes table, rounded down to a multiple of 8px) and `--font-serif-italic` (Literata italic, loaded separately and not preloaded).
3. **The styleguide hero has a reserved slot** at the top of `/styleguide`: the game title (the page's `h1`), a board placeholder at `--board-size`, and the scoresheet and eval graph placeholders. The board and annotation lanes fill it in.

4. **Arrival spread accepted.** The 27% pawn-to-queen spread at 90% arrival (138ms to 175ms) stays, by controller ruling. It follows from the ruled 0.8 and 1.3 factors, and a narrower spread would weaken the weight contrast.
5. **`--board-size` fallback.** It is declared as plain `min(100vw − 2 × gutter, 480px)`, with the rounded form under `@supports (width: round(down, 10px, 8px))`.

**Token values changed in this round:** the four piece `settle` springs (the minor-piece row keeps 520/42) and the queen and king mass factor (1.25 to 1.3).

### 2026-11-21, Task 10: board lab rulings

The board treatment lab (`/lab/board/{a,b,c}`) compared a printed diagram, book hatching and an ink well. The controller approved variant c, "Ink well", and all three of its proposals. The full record is `design/decisions/board.md`.

1. **Arrow end with a ring:** the shaft ends 46% of a square short of a ringed target (it stays 16% for a bare target), so the tip meets the ring's outer edge ("Pen stroke geometry").
2. **Badge corner:** the glyph badge moves to the top-left corner when the move's arrow arrives from the right of the target ("Severity and marks").
3. **Well:** the board sits in an 8px `well` from 768px up, and there is no well below 768px ("The board").

Also recorded: coordinates sit on the squares, and react-chessboard's built-in default pieces are never rendered (they are unattributed Cburnett). The package still bundles them, so the `/licenses` Cburnett notice also covers that embedded copy ("Piece sets", "The embedded copy"). Task 11 provides Cburnett from Commons under BSD-3-Clause at `packages/ui/assets/pieces/cburnett/` with its `LICENSE`.

### 2026-11-21, Task 6 fix round 1: the link mark

The styleguide hero lab (`packages/ui/design/decisions/styleguide-hero.md`) adopted variant C's pen stroke from the notation to the board into the winning variant A, by controller ruling. "Severity and marks" gains a **Link** row and its rules (colour, shape, routing, timing, reduced motion, budget), and the redundancy rule names it as its one exception.

**Token values changed in this round:** none.

### 2026-11-21, Task 6 fix round 2: hero composition, knights arc, link visibility

Controller rulings on the styleguide hero lab:

1. **Hero composition.** The board heads the scoresheet (no legal side-by-side route exists for the link in the reference position). The eval graph and label lane sit beside the board as the second loud surface, with the eval bar left of the board. The comment is a margin comment on the sheet, level with the ringed row. "Styleguide hero" and the plan pass's layout line are rewritten to match.
2. **Knights arc** is a ruled value under "Physical feel".
3. **Link mark:** its 0.9× is relative to the board pen at the current square size; it is omitted when no legal route exists; it is drawn only when both ends are in view at its start time.

**Token values changed in this round:** none.

### 2026-11-21, Task 11: corner coordinates, night last-move tint

Controller rulings on the Task 11 board:

1. **Coordinates** are 0.2 of the square with a 2px inset (files bottom right, ranks top left), and no piece covers one on 41–44px squares, any piece on any rank. Pieces are drawn at 0.84 of the square below 768px and 0.88 from 768px, checked geometrically ("The board"). The `xs` row no longer lists board coordinates.
2. **Night last-move tint** goes from `rgb(231 236 244 / 0.18)` to `rgb(231 236 244 / 0.30)`: 1.36:1 over the light square (day is 1.37) and 1.71:1 over the dark one, still supplementary, and a livelier night board for the owner. Coordinates on a tinted night square (last move or selected) use `on-ink`: 7.96 and 5.48:1 (last move), 7.62 and 5.14:1 (selected). With the old tint an `ink` coordinate on a tinted dark square was 3.30:1. New rows in the contrast table.

**Token values changed in this round:** night `--color-board-lastmove` (0.18 to 0.30).
