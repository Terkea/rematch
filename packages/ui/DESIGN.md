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
| Good-move green | `#2F7D5B` | better moves and good play |
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
| `--color-good` | `#2F7D5B` | `#5DBB8E` | good-move strokes and marks |
| `--color-good-text` | `#2A7353` | `#5DBB8E` | green text |
| `--color-pencil` | `#56657F` | `#97A6C0` | neutral annotations, plan arrows |
| `--color-board-light` | `#E3E8EF` | `#7F8DA8` | light squares |
| `--color-board-dark` | `#A7B4C7` | `#55637F` | dark squares |
| `--color-board-lastmove` | `rgb(22 35 59 / 0.16)` | `rgb(231 236 244 / 0.18)` | last-move tint |

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
- **The halo rule:** every board mark (ring, arrow, glyph badge, focus ring, check ring, legal-move dot) sits on a 2px `--color-sheet` halo, so the mark's adjacent colour is always its halo and never a square. That makes the `pen`, `good`, `pencil` and `ink` rows against `sheet` below the binding check. Without the halo, night marks fall to 1.11:1 on light squares, so the halo is not optional.
- **Board coordinates** use `ink` in day. In night they use `on-ink` on light squares and `ink` on dark squares, because no single colour passes on both night squares.
- **Last move** is tinted with `board-lastmove`. The tint is under 3:1 against the bare square on purpose, so it never competes with the pen. It is supplementary: the current move is also marked in the move list and announced to assistive tech.
- **Colour never carries meaning alone.** `pen` and `good` are nearly equal in luminance (1.01:1 day, 1.29:1 night), so every mark also differs in shape: pen marks always carry a `?`-family glyph badge and an open two-stroke arrowhead; good marks carry a `!`-family badge and a solid head. See "Severity and marks".

### Semantic states

| State | Treatment |
|---|---|
| Error (forms, failed actions) | `pen-text` message under the field, Phosphor `WarningCircle` icon, `aria-invalid`, field border `pen` (4.14:1 on `well`). Red is used for form errors, which is red's second meaning. |
| Warning | `ink` text on `well`, Phosphor `Warning` icon in `ink`, 1px `rule-strong` border. No amber token. |
| Info | `ink-soft` text on `well`, Phosphor `Info` icon. |
| Success | `good-text` message, Phosphor `CheckCircle` icon. A toast names the finished action ("chess.com connected"). |

### Interaction states

The tonal ladder runs `paper`, then `sheet`, then `well`. Hover moves one step down the ladder and never lifts.

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

Computed with the WCAG 2.x relative-luminance formula (sRGB to linear, `L = 0.2126 R + 0.7152 G + 0.0722 B`, ratio `(L1 + 0.05) / (L2 + 0.05)`), last run 2026-11-21 in fix round 1 over every pair. Composited colours (selection) are blended over their surface before the check. Text needs 4.5:1. Large text, UI strokes, marks and focus rings need 3:1. No pair fails.

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
| `good-text` | `sheet` | green text | 5.56 | 6.75 | 4.5:1 | pass |
| `good-text` | `paper` | green text | 5.23 | 7.42 | 4.5:1 | pass |
| `good-text` | `well` | green text | 4.78 | 7.87 | 4.5:1 | pass |
| `ink` | selection on `sheet` | `::selection` | 10.34 | 6.92 | 4.5:1 | pass |
| `ink` | selection on `paper` | `::selection` | 9.82 | 7.52 | 4.5:1 | pass |
| `on-ink` | `pen` | glyph badge text (`?`, `??`, `?!`) | 4.82 | 5.76 | 4.5:1 | pass |
| `on-ink` | `good` | glyph badge text (`!`, `!!`) | 4.86 | 7.42 | 4.5:1 | pass |
| `pen` | `sheet` | mark on its halo | 4.82 | 5.24 | 3:1 | pass |
| `pen` | `paper` | mark off the board | 4.53 | 5.76 | 3:1 | pass |
| `pen` | `well` | graph ring, error border | 4.14 | 6.11 | 3:1 | pass |
| `good` | `sheet` | mark on its halo | 4.86 | 6.75 | 3:1 | pass |
| `good` | `paper` | mark off the board | 4.57 | 7.42 | 3:1 | pass |
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
| `ink` | `board-dark` | coordinates | 7.47 | 5.09 | 4.5:1 | pass |
| `on-ink` | `board-light` | coordinates (night only) | n/a | 5.20 | 4.5:1 | pass |
| `piece-dark` | `board-light` | piece outline and black body | 12.75 | 5.51 | 3:1 | pass |
| `piece-dark` | `board-dark` | piece outline and black body | 7.47 | 3.05 | 3:1 | pass |
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
| `pen` | 4.02 | 2.36 | 1.11 | 2.00 |
| `good` | 4.06 | 2.38 | 1.43 | 2.58 |
| `pencil` | 4.79 | 2.81 | 1.36 | 2.45 |
| `ink` (focus ring) | 12.75 | 7.47 | 2.82 | 5.09 |

The halo itself against the squares is 1.20 and 2.05 (day) and 4.72 and 2.62 (night). It never needs 3:1, because it is the mark's background, not the mark. Last-move tint over the bare square: 1.37 and 1.31 (day), 1.24 and 1.43 (night), supplementary by design.

## Type

- **Literata** (variable, with the `opsz` optical-size axis) sets headlines, panel headings, annotations and notation. Weights 400 and 600. Italic 400 is used only for whole annotation comments, never for one word in a sentence or headline (`B16`), and never for moves.
- **Schibsted Grotesk** sets the UI: labels, buttons, navigation, forms and settings. Weights 400, 500 and 700.
- Both load through `next/font/google` (self-hosted at build, no runtime request to Google). No other families. In particular, none of the defaults in `B6`, and no monospace anywhere, including for numbers (`B5`).

### Scale and optical size

Size and line height in px, from the classic scale. Literata's `opsz` tracks the rendered size (`font-optical-sizing: auto`). Where a size is set with `font-variation-settings`, use the value in the table.

| Step | Size / line height | Literata `opsz` | Typical use |
|---|---|---|---|
| `xs` | 12/16 | 12 | board coordinates, chart ticks, eval bar label |
| `sm` | 14/20 | 14 | secondary UI text, captions, graph labels |
| `base` | 16/24 | 16 | UI body, buttons, inputs, move list (15px rows use `opsz` 15) |
| `prose` | 18/30 | 18 | serif body (annotations, explanations), with more leading |
| `lg` | 18/26 | 18 | sans lead text |
| `xl` | 21/28 | 21 | **panel headings: Literata 600** |
| `2xl` | 24/30 | 24 | page section headings |
| `3xl` | 36/42 | 36 | page titles |
| `4xl` | 48/54 | 48 | rare: the result line of a reviewed game |
| `5xl` | 60/64 | 60 | landing only (M5) |

- **Notation** (`31. Rxe6?? −4.2`) is a typographic feature in its own right. Set it in Literata 600 with `font-variant-numeric: lining-nums tabular-nums`. Glyphs are kerned tight to the move with no space (`b5?`, `Nf6?!`), using `letter-spacing: -0.01em` on the glyph. Mistake glyphs use `pen-text`, good-move glyphs use `good-text`, and the rest is `ink`. The evaluation follows after a space in `ink-faint` at weight 400. Negative evaluations use the real minus sign, U+2212 (`−4.2`). Castling stays `O-O` and `O-O-O`, with letter O and hyphens.
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
| `--space-move-row` | 26px | one row of the move list and the scoresheet |
| `--space-panel-pad` | 20px (16px below 480px) | inner padding of sheets and panels |
| `--space-gutter` | 16px below 768px, 24px from 768px, 32px from 1280px | page side gutters and column gaps |

**Board sizes.** The board is always a whole number of pixels per square (`8 × floor(size / 8)`):

| Viewport | Board size |
|---|---|
| below 768px | `100vw − 2 × gutter`, capped at 480px (328px at 360px) |
| 768–1023px | 448px |
| 1024–1279px | 512px |
| 1280–1599px | 560px |
| 1600px and up | 640px |

**Elevation.** There are no card shadows. Surfaces separate by tone: `paper` is the page, `sheet` sits on it, and `well` is pressed into a sheet. The one shadow is `--shadow-lift: 0 10px 30px -12px rgb(22 35 59 / 0.35)` (night: `rgb(0 0 0 / 0.5)`), used only for things that float: popovers, the drawer, the command menu, and a piece while it is being dragged.

**Ruling.** Ruled lines are scoresheet-native, so they appear only on scoresheet and move-list surfaces, in `rule`. They never decorate a page, never separate every section, and never form newspaper columns (`B3`). Panels keep `--radius-sheet`.

**Focus.** Everywhere outside the board: a 2px `ink` outline with a 2px offset, so the ring sits on the surface behind (`paper`, `sheet` or `well`, all ≥ 13:1). On the board: an inset ring, a 2px `sheet` halo at 2px inside the square's edge, with a 2px `ink` ring inside it, so the ring's neighbour is always the halo. The focused square is also announced ("e4, white knight").

**Icons.** Phosphor, regular weight, `currentColor`. 20px in UI controls and 16px inline with text. Always paired with a text label unless the control has an accessible name and a universal meaning (close, menu). Never Lucide or emoji (`B13`).

## Pieces

- **Style:** flat, in the ink metaphor. They are line drawings, like a printed diagram, with no gradients, bevels, wood or shadows.
- **Day:** black pieces are filled `piece-dark` (ink) with `piece-light` interior details. White pieces are filled `piece-light` (sheet) with a `piece-dark` outline about 1.5px at a 45px square, scaling with the square.
- **Night:** black pieces are filled `piece-dark` (`well`, `#0D1421`), which holds 5.51:1 and 3.05:1 on the two night squares, with `piece-light` interior details. White pieces are filled `piece-light` (pale ink) with the same `piece-dark` outline. Every silhouette therefore has a ≥ 3:1 edge on both squares in both themes.
- **Recommended set: Cburnett** (Colin M.L. Burnett, Wikimedia Commons, used under its BSD-3-Clause option). It is drawn as flat outlines with a 1.5px stroke at 45px, which is this exact look, and it recolours with two values. The other candidates were Chessnut (Apache-2.0) and RhosGFX (CC0). Licences, sources and the comparison are in `design/references/REFERENCES.md` and reference 6. The owner makes the final taste call, and the decision is recorded in `docs/decisions/ADR-WEB-n-piece-set.md`. Attribution goes on `/licenses`.
- Never use Unicode chess glyphs for pieces.

## The board

The board is one of the two loud things, so it gets the most specific rules.

- `react-chessboard` (MIT) with chess.js (BSD-2). Never chessground or chessops (`B12`).
- Squares are `board-light` and `board-dark`, cool and low in contrast, so pieces and marks carry the eye. The board has `--radius-board` and no frame, shadow or wood.
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
| Keyboard focus | the inset ring described under "Focus" |

- **Keyboard.** The board is operable from the keyboard: arrow keys move focus, Enter or Space picks up and drops, and Escape cancels.

## Severity and marks

Severity comes from the mover's loss of win chance, using the Lichess thresholds on the win-chance scale defined under "The eval graph": a loss of at least 0.3 is a blunder, at least 0.2 a mistake, and at least 0.1 an inaccuracy. In the reference game, this marks 4…Bxf3?!, 6. Bc4?!, 6…Nf6?, 9…b5? and 10…cxb5?!.

| Mark | On the board | Glyph | Stroke |
|---|---|---|---|
| Blunder | pen ring on the destination, pen arrow for the move played, badge | `??` | pen, open two-stroke head |
| Mistake | pen ring on the destination, badge | `?` | pen |
| Inaccuracy | badge only | `?!` | thin (0.75×) wherever a ring is drawn for it (graph, scoresheet) |
| Good move | good arrow for the better move or the punishment, badge when it was played | `!` | good, solid head |
| Brilliant | good ring, good arrow, badge | `!!` | good, solid head |
| Plan or idea | pencil arrow or ring, never a badge | none | pencil, 0.7×, open head |

- **Redundancy, not colour alone:** pen marks always carry a `?`-family badge and an open head. Good marks carry a `!`-family badge and a solid head. Pencil never carries a badge.
- **Glyph badge:** a circle of radius 20% of the square in the square's top-right corner (inset 1px), filled `pen` or `good`, with the glyph in `on-ink` Literata 600 (35% of the square for one character, 26% for two), on a 2px `sheet` halo.

### Pen stroke geometry

Marks are filled, variable-width outlines built around a centreline, never constant-width strokes. They are opaque and drawn once. All values below are at a 56px square and scale linearly with the square (minimum 1.1px). Geometry lives in `packages/ui/src/annotations/**`, the one place raw values are allowed besides `tokens.css` and `board/**`. It is specified here so the code has a contract.

- **Width profile along the stroke:** 2.2px at the start, rising to 3.2px between 35% and 65% of the length, then tapering to 1.4px at the end. The ends are rounded.
- **Width by mark type:** pen 1.0×, good 1.0×, pencil 0.7× (thinner than pen and good strokes), inaccuracy (thin) 0.75×, arrowhead strokes 0.9× of their shaft's width.
- **Ring:** an ellipse, not a circle. The radius is 40% of the square, with rx/ry ≈ 1.06, tilted 8–12° (sign and amount seeded). The radius drifts ±6% around the path (two low-frequency sines with seeded phases). It starts near 1 o'clock (−60° from three o'clock in the ellipse's frame) and runs to 380°, spiralling outward so the overrun tail sits 2.5px outside the start. It is a spiral, not a closed ring.
- **Scoresheet ring:** the same stroke, fitted to the word as an ellipse with `rx = word width / 2 + 10px` and `ry = 14px`, tilted 2–4°. It is used only for blunders and mistakes on the scoresheet.
- **Arrow shaft:** starts 22% of a square from the origin centre and ends 16% short of the target centre. It bows 2–4% of its length to one side (seeded) as a quadratic curve.
- **Open head (pen and pencil):** two separate strokes of 0.9 and 1.1 times a base length of 30% of the square, at 26° and 32° to the shaft (which side gets which is seeded).
- **Solid head (good):** a filled triangle with the same unequal sides, on the halo.
- **Halo:** the outline is drawn first in `sheet` with a 4px stroke (2px each side), then filled in its colour.
- **Determinism:** every random value comes from a seeded generator keyed by the mark type and its squares (`ring:b5`, `arrow:c3b5`), so the same mark always looks the same, on every device and every visit.
- **Drawing technique:** the filled outline is revealed through an SVG mask whose centreline stroke animates `stroke-dashoffset` (`pathLength = 1`). This keeps the hand-drawn shape and stays inside the performance budget.

## The eval graph and eval bar

### Eval graph

- Built with visx. It sits full width directly under the board (on narrow screens, under the board and above the move list).
- **Mapping:** the y-axis is win chance, not pawns, using Lichess's `winningChances`: `wc = 2 / (1 + exp(−0.00368208 × centipawns)) − 1`, clamped to [−1, 1], from White's point of view. A forced mate maps to ±1. The +8 and +12 of a won game then don't flatten the early mistakes.
- **Size:** the plot is 120px tall on desktop and 88px tall on mobile, plus a 22px label lane above it.
- **Plot:** a `well` with `--radius-board`. The zero line uses `rule-strong`. The eval line is `ink` at 2px over a 10% `ink` area fill toward zero.
- **Markers:** each blunder, mistake or inaccuracy gets a small ring drawn with the same pen geometry (radius 7px, tilted 10°), thin for inaccuracies, centred on the line.
- **Labels:** in the label lane, never on the line: the move in notation with its glyph (`9…b5?`) in `pen-text`, Literata 600 at `sm`. A 1px `rule-strong` leader tick runs from each label down to its ring. Labels are placed left to right and pushed right to keep a 10px gap. On mobile the lane keeps only blunders and mistakes, and inaccuracies are rings without labels.
- **Cursor:** a 1.5px `ink` rule marks the move shown on the board and moves with it.
- Long games scroll or compress inside the graph's own container, never the page.

### Eval bar

- A vertical bar to the left of the board, 12px wide and the board's height, with `--radius-board` and a 1px `rule-strong` outline. Below 768px it becomes an 8px horizontal bar above the board.
- White's share is `(wc + 1) / 2` on the same win-chance scale as the graph. Day: White is `sheet` and Black is `ink`. Night: White is `ink` and Black is `well`. White is at the bottom when the board is seen from White's side, and it flips with the board.
- The label (`+5.7`, `M2`) is Literata 600 at `xs` at the winning side's end, in the opposite colour of its fill.
- The boundary moves with the `swing` spring.

### Chart series

For the KPI timelines (up to 12 series), we use a tonal ink ramp, not a rainbow:

- **Tones:** `ink`, `ink-soft`, `pencil` and `rule-strong` (all ≥ 3:1 on `sheet` and `well`).
- **Dashes:** each tone comes solid, dashed (6 3) and dotted (1.5 3). Four tones times three dash patterns give 12 distinct series.
- **Highlight:** one highlighted series is drawn in `ink` at 2.5px. While one is highlighted, the others drop to `rule-strong` at 1.5px.
- **Labels:** direct labels at the line ends, in the series' tone. There is no legend box.
- Red and green are never series colours, because they keep their chess meanings.

## Motion

Motion is a headline feature. The rule is rich, choreographed and physical, never generic. Motion (formerly Framer Motion) is used for all app animation, the View Transitions API for shared elements, and NumberFlow for rolling numbers. GSAP and Lenis are landing-only (M5).

### Duration tokens

| Token | Value | Use |
|---|---|---|
| `--dur-instant` | 90ms | press feedback, each arrowhead stroke, drag lift |
| `--dur-quick` | 160ms | menus, toggles, hover previews, badge pop |
| `--dur-base` | 240ms | panels, drawers, tab content, a captured piece flying off |
| `--dur-draw` | 420ms | one pen stroke (ring or arrow shaft) |
| `--dur-reveal` | 600ms | the eval graph drawing across |
| `--dur-turn` | 520ms | a full page turn between routes |
| `--dur-stream` | 180ms | one replayed move in the onboarding stream |
| `--dur-soak` | 900ms | an ink-soak progress segment |

### Easing tokens

| Token | Value | Use |
|---|---|---|
| `--ease-out` | `cubic-bezier(0.2, 0.7, 0.2, 1)` | UI responses |
| `--ease-pen` | `cubic-bezier(0.6, 0.05, 0.3, 1)` | pen strokes: a slow start, a committed sweep and a soft stop |
| `--ease-turn` | `cubic-bezier(0.7, 0, 0.2, 1)` | the outgoing page of a page turn |
| `--ease-soak` | `cubic-bezier(0.3, 0, 0.1, 1)` | progress fills soaking in |

### Spring presets

| Preset | Values | Use |
|---|---|---|
| `settle` | `{ stiffness: 520, damping: 42 }` | piece travel and drop, badge settle (the brief's `piece`, renamed) |
| `pen` | `{ stiffness: 150, damping: 24 }` | stroke progress when a stroke must respond to input mid-draw (a user-drawn arrow, an interrupted sequence) |
| `swing` | `{ stiffness: 260, damping: 30 }` | eval bar and graph cursor (the brief's `evalBar`, renamed). Velocity carries over when values change in quick succession, which gives the bar its momentum. |
| `page` | `{ stiffness: 300, damping: 34 }` | the incoming page of a page turn, layout morphs and the Motion fallback for shared elements |

### Stagger scale

| Token | Value | Use |
|---|---|---|
| `--stagger-tight` | 40ms | scoresheet lines annotating, list rows |
| `--stagger-base` | 80ms | report sections, critical-moment rings on the graph |
| `--stagger-pen` | 120ms | marks within one moment (ring, then arrow, then badge) |
| `--stagger-beat` | 900ms | ranked moments in the analysis reveal |

### Principles

- `M1` **Every animation means something.** It shows what changed, what is ranked or where you are going, and it is user-caused or one of the named orchestrated moments. Nothing moves on its own to attract attention.
- `M2` **Pen strokes draw by `pathLength`,** revealing the filled outline through a mask. A ring starts at about 1 o'clock and overruns to 380°. One stroke takes `--dur-draw` with `--ease-pen`.
- `M3` **Arrowheads start when the shaft reaches 85%.** Each head stroke takes `--dur-instant`, and the second starts 30ms after the first.
- `M4` **Nothing fades up on scroll,** and nothing is scroll-triggered in the app (`B11`).
- `M5` **There are no hover lifts.** Hover changes tone, never position or shadow.
- `M6` **Physical, not decorative.** Pieces have mass (springs), captures have impact, and the eval bar has momentum. There is no bounce for its own sake.
- `M7` **Interruptible.** Input never waits for an animation. New input finishes or reverses in-flight motion, and springs keep their velocity.
- `M8` **Long sequences run in full once.** Anything over 1s runs in full only on the first view of that game or report, with a "Skip" button (and Escape). Later views run a condensed version of at most 600ms.
- `M9` **Performance budget.** Animate only `transform`, `opacity` and SVG `stroke-dashoffset` (including mask strokes). Never animate layout properties, `filter`, `box-shadow` or `clip-path`. The target is 60 fps on a mid-range phone, tested as a Pixel 6a-class device or Chrome DevTools at 4× CPU throttling, with no frame over 16ms and at most 12 SVG marks animating at once. `will-change` is set only while animating.
- `M10` **Reduced motion.** Under `prefers-reduced-motion: reduce`, choreography is replaced by instant state changes with a brief opacity cue: the changed element appears at full state with a single opacity fade of at most `--dur-quick` and no movement. Marks appear fully drawn, pieces jump, bars set their value, and page turns become an instant swap. Sound is unaffected.

### Choreography

Times are in ms from the start of the moment. "Seeded" means deterministic per game.

**1. Analysis reveal.** This runs on the first view of a freshly analysed game, and it is skippable. Roughly 3.6s in full, or 600ms condensed on later visits.

| Time | What happens |
|---|---|
| 0–600 | The eval graph draws left to right: the line's `stroke-dashoffset` over `--dur-reveal` with `--ease-pen`, and the area fill revealed by a mask rect with `scaleX` from 0 to 1. |
| 600 onward | Critical-moment rings draw on the graph in game order, at `--stagger-base` apart. Each ring takes 240ms, then its label and leader tick fade in over `--dur-quick`. |
| about 1200 | The board goes to the worst moment. Pieces that differ cross-fade at `--dur-quick` (a jump between distant positions does not slide). |
| +0 / +120 / +540 | That moment's marks draw: ring at +0, arrow at +120, badge pops at +540 with `settle`. |
| in parallel | Its scoresheet line annotates itself: the glyph and evaluation fade in, and the scoresheet ring draws (240ms). |
| every `--stagger-beat` (900) | The next ranked moment, for up to three moments. |
| end | Free play. The board lands on the worst moment with its marks drawn. |

Condensed version (M8): the graph appears drawn, the rings draw together with `--stagger-tight`, and the board shows the worst moment with its marks drawing (ring, arrow, badge) within 600ms.

**2. Report reveal.** This runs on the first view of a report, for the sections in the first viewport only. Sections below the fold are simply there (M4).

| Time | What happens |
|---|---|
| 0–600 | The report's eval graph or timeline draws as in the analysis reveal. |
| 200 onward | Weakness sections assemble in ranked order, `--stagger-base` apart. Each section's ruled top line draws with `scaleX` (240ms, `--ease-out`), then its heading and figures settle into place with opacity and a 6px `translateY` on `settle`. Its rank number and counts roll with NumberFlow. |
| end + 0–420 | The top weakness's example position draws its pen mark. |

This is not a generic fade-up: it runs once, in rank order, never on scroll, and it shows the ranking.

**3. Onboarding stream.** Games stream onto a live board while they import and analyse.

- Each replayed move takes `--dur-stream`, with pieces travelling on `settle`, capped at 5 moves a second.
- Between games, the board does a mini page turn (240ms) to the next game's start.
- The games counter rolls with NumberFlow each time a game finishes, and the progress bar soaks forward with `--dur-soak`.
- If games arrive faster than they replay, the board skips to the latest game's final position and continues from there. It never queues a backlog.

**4. Shared-element morphs.** These use the View Transitions API, with Motion `layoutId` and the `page` spring as the fallback.

- **Position thumbnail to full board:** 360ms. The board morphs, the move list and graph fade in 120ms after the morph ends, and marks then draw (as in 1).
- **Weakness card to area page:** the card becomes the page header in 420ms, and the page content arrives 120ms later with opacity only.
- **KPI tile to chart:** the tile's number travels to the chart title in 360ms, then the chart line draws over `--dur-reveal`.

**5. Page turn (route change).** This is never a cross-fade. Header and navigation stay still, and only the sheet turns.

| Time | Outgoing sheet | Incoming sheet |
|---|---|---|
| 0–260 | rotates −4° around its left edge (`perspective: 1600px`), shifts −24px, opacity 1 to 0, `--ease-turn` | waits |
| 140–520 | gone | starts 24px right with a 4° rotation, and settles flat with the `page` spring. Opacity goes 0 to 1 over the first 160ms. |

Back navigation mirrors the direction.

**6. Piece moves, captures and drag.**

- **Click or keyboard move:** the piece lifts (scale 1.04, −4px y) over `--dur-instant`, travels to the square on `settle`, and lands with the spring's small settle. The sound plays on landing.
- **Capture:** the captured piece is knocked off at contact, when the mover is within 30% of a square. It travels 0.6 of a square further in the move's direction, rotates 12–20° (seeded), scales to 0.9 and fades out over `--dur-base` with `--ease-out`. The capture sound plays at contact.
- **Drag:** on pickup the piece scales to 1.08 over `--dur-instant` and gains `--shadow-lift`, and the origin keeps a 30% ghost. While dragging it tilts with horizontal velocity, `rotate = clamp(vx × 0.02, −8°, 8°)`, driven by a spring `{ stiffness: 400, damping: 30 }` back to 0° when still. On drop it settles to the square centre on `settle` and loses the lift. An illegal drop returns the piece to its origin on `settle`.

**7. Micro-interactions.** Each one shows what changed.

- **Drill right:** a good ring draws on the destination (`--dur-draw`), the streak count ticks up with NumberFlow, and the correct sound plays.
- **Drill wrong:** the piece springs back to its origin on `settle`, a pen `?` badge pops on the attempted square for 900ms, then fades. The wrong sound plays. If the user asks for a hint, a pencil arrow draws.
- **Button press:** the pen-press, scale 0.97 over `--dur-instant`, released on `settle`.
- **Scoresheet hover preview:** hovering a move shows that move on the board as a pencil arrow drawn at `--dur-quick` and a 40% ghost of the piece on its destination. Moving off retracts it.
- **Progress:** fills soak forward like ink, with `scaleX` on `--ease-soak` over `--dur-soak` per update. It never blocks with a spinner (`B18`).

### Still banned

Fade-up on every section, hover lift and shadow on cards, parallax and scroll-jacking in the app (`B11`), bounce or wobble with no meaning, and any animation that blocks input.

## Sound

The owner asked for "sounds for moving pieces". The character is **paper and wood: quiet, close and dry, like a club game in a library**. It is never arcade, never bright synth bleeps, and never louder than a pen on paper. Synthesis parameters are left to Task 12b.

| Sound | Character | Plays when |
|---|---|---|
| Move | a soft wooden placement | a piece lands |
| Capture | heavier, with a slight knock | a capture's contact moment (choreography 6) |
| Check | a brighter tick | a move gives check, together with the board's check ring |
| Castle | two quick placements | king, then rook |
| Promotion | a short rising tone | the promoted piece appears |
| Game end | a low, settled close | mate, resignation or draw |
| Drill correct | a short warm chime | with the good ring |
| Drill wrong | a muted low thud, never harsh | with the piece springing back |
| Pen scratch (optional) | a very faint scratch | a red-pen mark draws. Off by default. |

Rules (spec §11):

- **Original sounds only.** They are synthesised with the Web Audio API, or recorded CC0 samples with every source logged. Never copy sounds from chess.com, Lichess or any other product.
- **Sound starts only after the first user gesture,** because of browser autoplay rules.
- There is a **global mute and volume control** in the header and in settings, remembered per user. Board sounds are on by default, and ambient sounds (the pen scratch) are off by default.
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

Six annotated references live in `design/references/`. `REFERENCES.md` there says what each one gives us, what not to copy, and its sources and licences. All of them were drawn from scratch in these tokens with a real game (Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858), real Stockfish 19 evaluations and the recommended Cburnett pieces.

1. `01-analysis-density-and-eval-graph.png`: move-list density, the win-chance eval graph with rings and a label lane, and the eval bar.
2. `02-arrow-semantics-and-pen-stroke.png`: hand-drawn versus mechanical strokes, the severity scale, marks in day and night, and the draw sequence in ms.
3. `03-scoresheet-grid-and-corrections.png`: the ruled grid, roman 600 moves, scoresheet rings and margin comments.
4. `04-book-notation-and-diagram-caption.png`: notation in running text and the captioned diagram.
5. `05-night-study-analysis.png`: reference 1 in night study.
6. `06-piece-set-candidates.png`: the three licensed piece sets side by side.

## Styleguide hero (for `/styleguide`)

The first thing on `/styleguide` is the system doing its job on a real game, not a swatch grid:

```
+----------------------------------------------------------------------+
| Chess Improver                                      Day  Night       |
+----------------------------------------------------------------------+
| Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858        |
|                                                                      |
|  +-+-----------------------+   +------------------------------------+|
|  |b|                       |   |  8  Nc3      c6                    ||
|  |a|   board, 9...b5       |   |  9  Bg5      b5?  +5.7   <- pen    ||
|  |r|   (pen ring on b5     |   | 10  Nxb5     cxb5?! +8.1           ||
|  | |    draws itself,      |   |     ruled scoresheet, sheet tone   ||
|  | |    then green Nxb5)   |   |                                    ||
|  +-+-----------------------+   +------------------------------------+|
|  [ label lane: 4...Bxf3?!  6. Bc4?!  6...Nf6?  9...b5?  10...cxb5?! ]|
|  [ eval graph, win-chance scale, pen rings on the line             ]|
|                                                                      |
|  Tokens follow below as plain sections: colour, type, shape, motion. |
+----------------------------------------------------------------------+
```

Left aligned. The heading is the game itself, set in Literata 600 at `3xl`. There is no marketing headline, eyebrow or badge. On first view, the hero plays the analysis reveal.

## How this direction was chosen

### Plan pass

- **Direction:** the owner's pick from §11, "Annotated scoresheet".
- **Colour:** paper `#F3F5F8`, sheet `#FBFCFD`, navy ink `#16233B`, red pen `#D3312B`, green `#2F7D5B`, pencil `#56657F`.
- **Type:** Literata for anything a coach would write (headlines, notation, comments), and Schibsted Grotesk for anything the app says (controls).
- **Layout:** the board left, the scoresheet right, and the eval graph spanning under both. On a phone: board, then graph, then scoresheet. Everything left aligned.
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
