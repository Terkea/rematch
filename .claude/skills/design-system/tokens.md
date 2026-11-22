# Tokens: what each is for

Values live only in `packages/ui/src/styles/tokens.css` (Tailwind theme for day, `[data-theme="dark"]` for night, `[data-theme="light"]` restated for a day scope in a night page). Tables and rulings are in `packages/ui/DESIGN.md` ("Colour tokens", "Type", "Shape, space and elevation"). Use the Tailwind class form (`bg-sheet`, `text-ink-soft`, `rounded-field`, `text-sm`).

## Colour

| Token | Use |
|---|---|
| `--color-paper` | the page |
| `--color-sheet` | a raised sheet, panels, the halo ground for board marks |
| `--color-well` | inset surfaces: fields, chart plots, the board's well |
| `--color-rule` | decorative lines only (ruling, dividers) |
| `--color-rule-strong` | anything a user must see to operate: input borders, axes, outlines |
| `--color-ink`, `--color-ink-soft`, `--color-ink-faint` | text, strongest to quietest. Nothing lighter. On dark, body text is `ink` (B17) |
| `--color-on-ink` | text on an `ink` fill (the primary button) |
| `--color-pen`, `--color-pen-text` | red: the player's mistakes and form errors. `pen` for marks, `pen-text` for words |
| `--color-good`, `--color-good-text` | the better move, success. `good` for marks, `good-text` for words |
| `--color-pencil` | plans, ideas, the opponent's marks, leaders |
| `--color-board-light`, `--color-board-dark` | board squares |
| `--color-board-lastmove`, `--color-board-select`, `--color-board-premove` | board tints (supplementary) |
| `--color-selection` | `::selection` |
| `--color-piece-dark`, `--color-piece-light` | piece body and outline recolouring |

Hover uses `--surface-hover` (one step down the ladder paper, sheet, well; inside a night sheet it steps up). Do not hand-pick hover colours. `--eval-graph-surface` and `--stat-surface` are set by a host on another surface and never defined in `tokens.css`.

## Type

- `--font-serif` (Literata): titles, verdicts, notation, KPI values. `--font-sans` (Schibsted Grotesk): UI. `--font-serif-italic`: annotation comments only. Never add a font (B6).
- Scale: `2xs` 11px (the smallest allowed text), `xs`, `sm`, `base`, `prose` (running text), `lg`, `xl`, `2xl`, `3xl` to `5xl` (`4xl` and `5xl` are headline scale). `--tracking-glyph` kerns `?` and `??` against the move.

## Shape, space, elevation

- Radii: `--radius-board` (2px: board, plots, bars), `--radius-field` (6px: controls), `--radius-sheet` (10px: sheets, dialogs), `--radius-pill`. Not one radius for everything (B4).
- Space roles: `--space-move-row`, `--space-scoresheet-row`, `--space-panel-pad`, `--space-gutter`, and `--board-size` (responsive, whole pixels per square). `--ruled-sheet` is the placeholder's writing lines. Layout sizes: `--container-graph`, `--eval-graph-height`, `--eval-graph-height-wide`.
- `--shadow-lift`: the one shadow, for things that float (popover, toast, a dragged piece). Never on cards (M5).

## Motion tokens

`--dur-*`, `--ease-*`, `--stagger-*` and the overlay values are in `motion.md`.

## Adding a token

Propose it first with its role and contrast. Then edit DESIGN.md, `tokens.css` (day and night), and let `packages/ui/test/tokens.test.ts` prove they match.
