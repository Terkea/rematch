# Chess Improver design system

This is the visual identity for the web app. Every UI task cites it: the tokens by name, the bans by ID (`B1`–`B18`), and the motion principles by number. If a value isn't here, don't invent one. Propose it, get it approved, and add it here first.

The tokens are implemented in `packages/ui/src/styles/tokens.css`. That file and this one must agree. If they disagree, this document wins until it is revised.

## Subject, audience and job

- **Subject:** a chess-coaching app that reads a player's own games and marks them up like a strong coach would.
- **Audience:** a 1000–2000 club player reviewing their own games on a laptop or phone.
- **Job:** to see, precisely, where they went wrong and what to practise.

Every screen should answer "where did I go wrong, and what do I do about it?" faster than the one before.

## Direction: annotated scoresheet

Cool paper, navy ink, and a coach's red pen for mistakes. It should feel precise and studious, like a game a strong coach has marked up the night after the tournament. The signature is the red pen: circles, arrows and question marks draw themselves onto the board and the scoresheet, the way a hand would.

**Boldness is spent in two places only: the board and the eval graph.** They are the brand. Everything else (navigation, forms, settings, panels) is quiet, tonal and disciplined, so the marks on the board are the loudest thing on screen.

Red has one meaning. `--color-pen` marks a chess mistake, and `--color-pen-text` also carries form errors. Red is never decoration, never a brand flourish, and never a "danger" button colour.

### Core palette

The named colours behind the token table:

| Name | Day | Role in the direction |
|---|---|---|
| Paper | `#F3F5F8` | cool, slightly blue page, never cream |
| Sheet | `#FBFCFD` | the scoresheet and panels, raised by tone only |
| Navy ink | `#16233B` | text, primary fills, focus |
| Red pen | `#D3312B` | the coach's marks on mistakes |
| Good-move green | `#2F7D5B` | better moves and good play |
| Pencil | `#56657F` | plans and neutral ideas, quieter than ink |

## Colour tokens

Two themes: **day** (light) and **night study** (dark). Night study is the same scoresheet at a desk lamp: navy paper and pale ink, not black with an accent. Contrast is checked against `sheet` and `paper`.

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

### Usage rules

- **Text** uses `ink`, `ink-soft` or `ink-faint`, and nothing lighter. On dark, body text is `ink` (`#E7ECF4`), never a mid grey (`B17`).
- **Red text** uses `pen-text`, and green text uses `good-text`. The stroke tokens `pen` and `good` are for marks, not words.
- **Lines:** `rule` is decorative only (scoresheet ruling, dividers). Anything a user must see to operate the UI, such as input borders, chart axes or checkbox outlines, uses `rule-strong`.
- **Primary action:** an `ink` fill with `on-ink` text. There is one primary action per view.
- **The halo rule:** every board stroke (circle, arrow, glyph badge) has a 2px `--color-sheet` halo underneath it, so a mark keeps ≥ 3:1 on either square colour. The mark's adjacent colour is always its halo, which makes the `pen`, `good` and `pencil` rows against `sheet` below the binding check. Without the halo, night marks fall to 1.1:1 on light squares, so the halo is not optional.
- **Board coordinates** (rank and file labels) use `ink` in day. In night they use `on-ink` on light squares and `ink` on dark squares, because no single colour passes on both night squares.
- **Last move** is tinted with `board-lastmove`. The tint is under 3:1 against the bare square on purpose, so it never competes with the pen. That makes it supplementary: the current move is also marked in the move list and announced to assistive tech, so the tint never carries the information alone.

### Contrast results

Computed with the WCAG 2.x relative-luminance formula (sRGB to linear, `L = 0.2126 R + 0.7152 G + 0.0722 B`, ratio `(L1 + 0.05) / (L2 + 0.05)`), run on 2026-11-21. Text needs 4.5:1. Large text, UI strokes and focus rings need 3:1.

| Foreground | Background | Day | Night | Required | Result |
|---|---|---|---|---|---|
| `ink` | `sheet` | 15.28 | 13.32 | 4.5:1 | pass |
| `ink` | `paper` | 14.37 | 14.65 | 4.5:1 | pass |
| `ink` | `well` | 13.12 | 15.54 | 4.5:1 | pass |
| `ink-soft` | `sheet` | 8.67 | 8.52 | 4.5:1 | pass |
| `ink-soft` | `paper` | 8.15 | 9.37 | 4.5:1 | pass |
| `ink-soft` | `well` | 7.44 | 9.94 | 4.5:1 | pass |
| `ink-faint` | `sheet` | 5.53 | 5.57 | 4.5:1 | pass |
| `ink-faint` | `paper` | 5.20 | 6.13 | 4.5:1 | pass |
| `ink-faint` | `well` | 4.75 | 6.50 | 4.5:1 | pass |
| `on-ink` | `ink` | 15.28 | 14.65 | 4.5:1 | pass |
| `pen-text` | `sheet` | 5.49 | 5.24 | 4.5:1 | pass |
| `pen-text` | `paper` | 5.17 | 5.76 | 4.5:1 | pass |
| `pen-text` | `well` | 4.72 | 6.11 | 4.5:1 | pass |
| `good-text` | `sheet` | 5.56 | 6.75 | 4.5:1 | pass |
| `good-text` | `paper` | 5.23 | 7.42 | 4.5:1 | pass |
| `pen` (mark on its halo) | `sheet` | 4.82 | 5.24 | 3:1 | pass |
| `pen` | `paper` | 4.53 | 5.76 | 3:1 | pass |
| `good` (mark on its halo) | `sheet` | 4.86 | 6.75 | 3:1 | pass |
| `good` | `paper` | 4.57 | 7.42 | 3:1 | pass |
| `pencil` (mark on its halo) | `sheet` | 5.74 | 6.42 | 3:1 | pass |
| `pencil` | `paper` | 5.40 | 7.06 | 3:1 | pass |
| `rule-strong` | `sheet` | 3.53 | 3.71 | 3:1 | pass |
| `rule-strong` | `paper` | 3.32 | 4.08 | 3:1 | pass |
| `rule-strong` | `well` | 3.03 | 4.33 | 3:1 | pass |
| `ink` (focus ring) | `sheet` | 15.28 | 13.32 | 3:1 | pass |
| `ink` (focus ring) | `paper` | 14.37 | 14.65 | 3:1 | pass |
| `ink` (coordinates, day) | `board-light` | 12.75 | n/a | 4.5:1 | pass |
| `ink` (coordinates) | `board-dark` | 7.47 | 5.09 | 4.5:1 | pass |
| `on-ink` (coordinates, night) | `board-light` | n/a | 5.20 | 4.5:1 | pass |

Informational only, to show why the halo rule exists (a stroke without its halo):

| Mark | `board-light` day | `board-dark` day | `board-light` night | `board-dark` night |
|---|---|---|---|---|
| `pen` | 4.02 | 2.36 | 1.11 | 2.00 |
| `good` | 4.06 | 2.38 | 1.43 | 2.58 |
| `pencil` | 4.79 | 2.81 | 1.36 | 2.45 |

Last-move tint over the bare square (composited): day 1.37 (light) and 1.31 (dark), night 1.24 (light) and 1.43 (dark). This is supplementary by design (see the usage rules).

## Type

- **Literata** (variable, with the `opsz` optical-size axis) sets headlines, annotations and notation. Weights 400 and 600. Italic 400 is used only for whole annotation comments, never for one word in a sentence or headline (`B16`).
- **Schibsted Grotesk** sets the UI: labels, buttons, navigation, forms and tables of settings. Weights 400, 500 and 700.
- Both load through `next/font/google` (self-hosted at build, no runtime request to Google). No other families. In particular, none of the defaults in `B6`, and no monospace anywhere, including for numbers (`B5`).

### Scale

Size and line height in px, from the classic scale:

| Step | Size / line height | Typical use |
|---|---|---|
| `xs` | 12/16 | board coordinates, chart tick labels |
| `sm` | 14/20 | secondary UI text, captions |
| `base` | 16/24 | UI body, buttons, inputs |
| `prose` | 18/30 | serif body (annotations, explanations), with more leading |
| `lg` | 18/26 | sans lead text |
| `xl` | 21/28 | section headings in panels |
| `2xl` | 24/30 | page section headings |
| `3xl` | 36/42 | page titles |
| `4xl` | 48/54 | rare: the result line of a reviewed game |
| `5xl` | 60/64 | landing only (M5) |

- **Notation** (`31. Rxe6?? −4.2`) is a typographic feature in its own right. Set it in Literata 600 with `font-variant-numeric: lining-nums tabular-nums`, so move numbers and evaluations align in columns. Mistake glyphs and their evaluations use `pen-text`, good moves use `good-text`, and the rest is `ink`. Negative evaluations use the real minus sign, U+2212 (`−4.2`, not `-4.2`). Castling stays `O-O` and `O-O-O` with letter O and hyphens.
- **Measure:** ≤ 72ch for serif prose and ≤ 66ch for sans.
- **Case:** sentence case everywhere, including buttons, headings and table headers. No all-caps labels and no letter-spaced eyebrows (`B5`).
- **Alignment:** left aligned. Centring is reserved for a diagram's caption under the board and for an empty state's single line.

## Shape and elevation

**Radii by role.** Radius reflects what a thing is, not a single global value (`B4`, `B14`):

| Token | Value | Applies to |
|---|---|---|
| `--radius-board` | 2px | the board, mini-boards, the eval graph's plot area |
| `--radius-field` | 6px | inputs, buttons, menus, list rows, chart wells |
| `--radius-sheet` | 10px | sheets and panels, the scoresheet, drawers, dialogs |
| `--radius-pill` | 9999px | toggles, segmented controls, small counts |

**Elevation.** There are no card shadows. Surfaces separate by tone: `paper` is the page, `sheet` sits on it, and `well` is pressed into a sheet. The one shadow is `--shadow-lift: 0 10px 30px -12px rgb(22 35 59 / 0.35)` (night: `rgb(0 0 0 / 0.5)`), used only for things that float above the page: popovers, the drawer and the command menu.

**Ruling.** Ruled lines are scoresheet-native, so they appear only on scoresheet and move-list surfaces, in `rule`. They never decorate a page, never separate every section, and never form newspaper columns (`B3`). Panels keep `--radius-sheet`.

**Spacing.** Tailwind 4's base unit (`--spacing: 0.25rem`) and its multiples. No custom spacing tokens yet. If a later task needs a named spacing role, it proposes it here.

**Icons.** Phosphor (regular weight, `currentColor`), used sparingly and always paired with a text label unless the control has an accessible name and a universal meaning (close, menu). Never Lucide or emoji (`B13`).

## The board and the eval graph

These are the two loud things, so they get the most specific rules.

### Board

- `react-chessboard` (MIT) with chess.js (BSD-2). Never chessground or chessops (`B12`).
- Squares are `board-light` and `board-dark`, cool and low in contrast, so pieces and marks carry the eye. The board has `--radius-board` and no frame, shadow or wood.
- **Annotation semantics.** One colour, one meaning:
  - `pen`: the mistake. A circle marks the square, an arrow marks the move that was played, and a `?`, `??` or `?!` glyph badge sits on the destination square.
  - `good`: the better move, or the punishment of a mistake.
  - `pencil`: a plan or a neutral idea (a pawn break, a target). It is drawn thinner than pen and green.
- **Stroke character.** Strokes are opaque, drawn once, with round caps and joins, on a 2px `sheet` halo. They are not translucent and not geometrically perfect. A circle starts near 1 o'clock, drifts slightly in radius, and overruns to 380°. An arrow's shaft has a slight bend, and its head is two short strokes. Wobble is deterministic, seeded by the squares, so the same mark always looks the same. Stroke geometry constants live in `packages/ui/src/annotations/**`, the one place raw values are allowed besides `tokens.css` and `board/**`.
- **Keyboard.** The board is operable from the keyboard, with a visible `ink` focus ring on the focused square.

### Eval graph

- Built with visx. It sits full width directly under the board (on narrow screens, under the board and above the move list), so the eye drops from position to trend.
- Plot area: a `well` with `--radius-board`. The zero line and axes use `rule-strong`. The eval line is `ink` at 2px over a 10% `ink` area fill toward zero, from White's point of view. Mate is clamped to the top or bottom edge.
- Mistakes are short `pen` ticks on the line itself, labelled in notation with `pen-text`. There is no legend.
- A thin `ink` cursor marks the move shown on the board and moves with it.
- Long games and many series scroll or compress inside the graph's own container, never the page.

## Motion

Motion (formerly Framer Motion) for all app animation. No GSAP or Lenis in the app.

| Token | Value | Use |
|---|---|---|
| `--dur-instant` | 90ms | press feedback, arrowhead strokes |
| `--dur-quick` | 160ms | menus, toggles, focus moves |
| `--dur-base` | 240ms | panels, drawers, tab content |
| `--dur-draw` | 420ms | one pen stroke (circle or arrow shaft) |
| `--dur-reveal` | 600ms | orchestrated reveals (the eval graph drawing) |
| `--ease-out` | `cubic-bezier(0.2, 0.7, 0.2, 1)` | UI responses |
| `--ease-pen` | `cubic-bezier(0.6, 0.05, 0.3, 1)` | pen strokes: a slow start, a committed sweep and a soft stop |

Springs (Motion `type: "spring"`):

- `piece`: `{ stiffness: 520, damping: 42 }`, for pieces moving between squares.
- `evalBar`: `{ stiffness: 260, damping: 30 }`, for the eval bar settling on a new value.

Annotation stagger is 120ms: when several marks appear for one move, each starts 120ms after the previous, in reading order (circle, then arrow, then glyph).

### Principles

1. **Only user-caused or orchestrated moments animate.** A move played, a drill answered, a panel opened, or one of the two orchestrated moments (live analysis and the report reveal). Nothing moves on its own to attract attention.
2. **Pen strokes draw by `pathLength`.** A circle starts at about 1 o'clock and overruns to 380°, like a hand. Each stroke takes `--dur-draw` with `--ease-pen`.
3. **Arrowheads appear after 85% of the shaft**, drawn as two strokes in `--dur-instant`.
4. **Nothing fades up on scroll.** Content is simply there (`B11`).
5. **There are no hover lifts.** Hover changes tone or underline, never position or shadow.
6. **With reduced motion, everything is shown in its final state at once.** Under `prefers-reduced-motion: reduce`, nothing moves: marks appear fully drawn, pieces jump, and the eval bar sets its value without a spring.

Loading never blocks with a spinner. Use the progress placeholder: the shape of the content to come, in `well`, with real progress text when it is known (`B18`).

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
- No `→` on links or buttons, no all-caps labels, and no lorem ipsum. Real games, real evaluations and real names from the start (`B15`).

## References

Four annotated references live in `design/references/`. `REFERENCES.md` there says what each one gives us, what not to copy, and its sources and licences. All four were drawn from scratch in these tokens with a real game (Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858) and real Stockfish 19 evaluations.

1. `01-analysis-density-and-eval-graph.png`: move-list density and the eval graph under the board.
2. `02-arrow-semantics-and-pen-stroke.png`: the colour semantics of marks, the hand-drawn stroke, the halo in night study, and the draw sequence.
3. `03-scoresheet-grid-and-corrections.png`: the ruled grid, ink weight and red corrections.
4. `04-book-notation-and-diagram-caption.png`: notation in running text and the captioned diagram.

## Styleguide hero (for `/styleguide`)

The first thing on `/styleguide` is the system doing its job on a real game, not a swatch grid:

```
+----------------------------------------------------------------------+
| Chess Improver                                      Day  Night       |
+----------------------------------------------------------------------+
| Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858        |
|                                                                      |
|  +-----------------------+   +------------------------------------+  |
|  |                       |   |  8  Nc3      c6                    |  |
|  |   board, 9...b5       |   |  9  Bg5      b5?  +5.7   <- pen    |  |
|  |   (pen ring on b5     |   | 10  Nxb5     cxb5? +8.1            |  |
|  |    draws itself,      |   |     ruled scoresheet, sheet tone   |  |
|  |    then green Nxb5)   |   |                                    |  |
|  +-----------------------+   +------------------------------------+  |
|  [ eval graph, full width, pen ticks on 6...Nf6, 9...b5, 10...cxb5 ] |
|                                                                      |
|  Tokens follow below as plain sections: colour, type, shape, motion. |
+----------------------------------------------------------------------+
```

Left aligned. The heading is the game itself, set in Literata 600 at `3xl`. There is no marketing headline, eyebrow or badge.

## How this direction was chosen

### Plan pass

- **Direction:** the owner's pick from §11, "Annotated scoresheet".
- **Colour:** paper `#F3F5F8`, sheet `#FBFCFD`, navy ink `#16233B`, red pen `#D3312B`, green `#2F7D5B`, pencil `#56657F`.
- **Type:** Literata for anything a coach would write (headlines, notation, comments), and Schibsted Grotesk for anything the app says (controls).
- **Layout:** the board left, the scoresheet right, and the eval graph spanning under both. On a phone: board, then graph, then scoresheet. Everything left aligned.
- **Principles:** boldness only on the board and the eval graph; red means a mistake; the pen draws, nothing else performs; tone instead of shadow.

### Cliché review and what changed

The plan was checked against the five common generated looks:

1. **Cream and terracotta (`B1`).** The paper is cool blue-grey, not cream, and the red is a true pen red, not clay. A big serif headline on light paper still drifts toward this look, so a guard was added: Literata stays at text and title sizes in the app (`3xl` for page titles, `4xl` rarely), `5xl` is landing-only, and the styleguide hero is the game, not a display headline.
2. **Near-black with an acid accent (`B2`).** Night study is navy (`#121A2B`), not `#0B0B0B`. It has two semantic accents, not one decorative one, and the body text is near-white `ink`, not grey.
3. **Broadsheet (`B3`).** Ruled lines are scoresheet-native, which borders on this look. **Changed:** the first draft ruled the whole styleguide page into sections. Ruling is now only on scoresheet and move-list surfaces, panels keep `--radius-sheet`, and there are no newspaper columns.
4. **SaaS card kit (`B4`).** There are no card shadows. Surfaces separate by tone, and radii vary by role (2, 6, 10 and pill). There are no gradients.
5. **Template chrome (`B5`).** **Changed:** the first draft put small letter-spaced section labels above each styleguide block and set evaluations in a monospace face. Headings are now plain sentence case, and evaluations use Literata's tabular lining figures. The game line used a middle-dot meta string ("Paris · 1858 · Philidor"). It is now a plain sentence ("Paris 1858").

Two more changes came from the review. The first draft let the hero headline carry one red word for emphasis, which is `B16`, so it was removed. The first draft also used `pen` for destructive buttons. Red is now reserved for chess mistakes and form errors, and destructive actions are ink-outlined with an explicit verb ("Delete account").

## Revisions

- **2026-11-21, self-review.** The brief asks for a fresh subagent to read this document only. The implementing agent was told not to dispatch subagents, so that review is still owed (see the task report). The self-review against the same two questions (does any section read like a default for a similar product, and does any token pair fail its stated contrast) found:
  - No token pair fails. The coordinate labels needed a theme-specific rule, because `ink` fails on night light squares (2.82:1) and `on-ink` fails on day squares, so coordinates switch colour by square in night.
  - The last-move tint is below 3:1 against its square. It is now documented as supplementary, with the move list and assistive text carrying the information.
  - The night board marks were checked against the bare night squares and fail badly (down to 1.11:1), which confirms that the halo rule is load-bearing. It is now stated as non-optional, with the informational table as evidence.
  - Reference 2 first drew the pencil plan arrow c4 to b5, on top of the green arrow. It was changed to e1 to c1 (castling long, played at move 12) so the two meanings never overlap.
