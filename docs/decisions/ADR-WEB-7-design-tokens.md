# ADR-WEB-7. Design tokens, themes and self-hosted fonts

Date: 2026-11-21
Status: Accepted

## Context

`packages/ui/DESIGN.md` (the annotated-scoresheet system) defines every colour, type step, radius, spacing role, shadow, duration, easing, spring and stagger. Code needs one implementation of those values, in CSS for Tailwind 4 and in JS for Motion, and the two must never drift from the document or from each other. The spec's red pen, `#D3312B`, is 4.14:1 on `well`, which passes for a stroke (3:1) but fails AA for text (4.5:1). Fonts must load without a network fetch at build or run time.

## Decision

### One CSS file, checked against DESIGN.md

- `packages/ui/src/styles/tokens.css` is the only place raw token values live (besides `packages/ui/src/board/**` and `packages/ui/src/annotations/**`). Day values are a Tailwind `@theme static` block, so every variable is emitted even when no utility uses it. Tailwind's default palette, fonts, text sizes, radii, shadows and easings are reset to `initial`, so only our tokens exist (`B14`).
- Night study redefines the theme-dependent tokens (every colour and `--shadow-lift`) under `[data-theme="dark"]`. `[data-theme="light"]` restates the day values, so a day scope can sit inside a night page (the styleguide shows both themes side by side). Dark mode is `@custom-variant dark (&:where([data-theme=dark], [data-theme=dark] *))`. next-themes sets `data-theme` on `<html>` (`defaultTheme="system"`, `enableColorScheme`).
- Responsive spacing roles (`--space-panel-pad`, `--space-gutter`) are set mobile first, with `@media (width >= N)` overrides on `:root`.
- `packages/ui/test/tokens.test.ts` parses DESIGN.md's tables and `tokens.css` and fails on any difference: every colour in both themes, the type scale (`2xs` to `5xl`, and that `opsz` equals the px size, which is what `font-optical-sizing: auto` gives), radii, spacing roles with their breakpoints, the shadow in both themes, durations, easings and staggers. It recomputes **every ratio in DESIGN.md's contrast table** from `tokens.css` (alpha tokens composited over their surface, rounded to 8 bits as the browser renders them) to ±0.01, and checks each meets its stated minimum. It also checks the brief's floor (text tokens ≥ 4.5 on `paper`, `sheet` and `well`; `pen`, `good`, `pencil` and `rule-strong` ≥ 3 on `sheet`; `on-ink` ≥ 4.5 on `ink`), that both themes define the same names, and that `#0B0B0B` and `#111111` never appear.

### Motion tokens in JS, mirrored and checked

`@ci/ui/motion` exports `dur`, `ease`, `stagger` (seconds and bezier arrays mirroring the CSS), `spring` (`settle`, `land`, `pen`, `swing`, `swingBig`, `page`, `tilt`), `pieceWeight` and `settleFor(piece)` (the mass table: mass, lift scale and landing squash per piece), `dampingRatio`, `overshoot` and `useReducedMotion()`. `packages/ui/test/motion.test.ts` checks every value against both DESIGN.md and `tokens.css`, and recomputes each spring's ζ and overshoot against the stated columns. Springs have no CSS form, so they exist only in JS. The module also re-exports Motion's `motion`, `animate`, `useAnimate` and `AnimatePresence`, so the app has one motion entry point.

A value changed in only one of DESIGN.md, `tokens.css` or `motion/tokens.ts` fails a test. This was checked by changing `--dur-draw`, night `--color-pen-text`, `land`'s damping and one contrast-table ratio: each failed its test.

### `pen` and `pen-text` are split, and marks sit on a halo

- `--color-pen` keeps the spec's `#D3312B` for strokes and marks (4.14:1 on `well`, ≥ 3:1 everywhere it is drawn).
- `--color-pen-text` (`#C42B26` day) carries red words: mistake glyphs in notation and form errors. It is 4.72:1 on `well`, 5.49:1 on `sheet`. `good` and `good-text` are both `#24684B` since the owner-approved round 2 change.
- **The halo rule:** every board mark (ring, arrow, glyph badge, focus ring, check ring, legal-move dot) is drawn on a 2px `--color-sheet` halo, so the mark's neighbour is always `sheet`, never a square. The binding contrast check for marks is therefore against `sheet`. Without the halo, night pen marks fall to 1.01:1 on light squares.

### Fonts are self-hosted with `next/font/local`

Committed in `apps/web/src/app/fonts/` with their SIL Open Font License 1.1 texts, and loaded with `next/font/local` in `apps/web/src/app/layout.tsx` (`--font-literata`, `--font-schibsted`). `--font-serif` and `--font-sans` in `tokens.css` read those variables. `next build --webpack` was run with `--network none` and succeeded.

| File | Source | Licence | Processing |
|---|---|---|---|
| `Literata-VF.woff2`, `Literata-Italic-VF.woff2` | `github.com/googlefonts/literata`, `fonts/variable/Literata[opsz,wght].ttf` and `Literata-Italic[opsz,wght].ttf`, commit `0c2761b727a1b3a7cffd313c37f0f5163dfc7a63` (v3.103) | OFL 1.1 (`Literata-OFL.txt`), no Reserved Font Name | Subset to Latin, Latin Extended-A and general punctuation (including U+2212 minus and U+2026 ellipsis), all layout features and both variation axes kept (`opsz` 7–72, `wght` 200–900), WOFF2, with fontTools 4.68 |
| `SchibstedGrotesk-VF.woff2` | `github.com/schibsted/schibsted-grotesk`, `fonts/variable/SchibstedGrotesk[wght].woff2`, commit `d485f61f105e1b3935f4d21dfb4d371359798603` | OFL 1.1 (`SchibstedGrotesk-OFL.txt`), no Reserved Font Name | None (upstream WOFF2, `wght` 400–900) |

Literata has no upstream variable WOFF2, so it was subset and converted. OFL 1.1 allows modified versions, and neither font declares a Reserved Font Name. Schibsted Grotesk's italic is not shipped, because the UI never uses it.

### Supporting changes

- `biome.json` enables `css.parser.tailwindDirectives`, so Biome parses `@theme`, `@custom-variant` and `@source`.
- `apps/web/next.config.ts` sets `allowedDevOrigins: ["web"]`. Next 16 blocks dev assets for unlisted origins, so the Compose `qa` service (which loads `http://web:3100`) saw an unhydrated page.
- `apps/web` depends on `next-themes` (the version `@ci/ui` already pins).

## Consequences

- Changing a token means changing DESIGN.md first. The tests then point at every place that disagrees.
- Theme-dependent tokens are listed three times in `tokens.css` (theme, night, day scope). The tests keep the three in step.
- Glyphs outside the Latin subset (for example Cyrillic names) fall back to the next font in the stack. Re-run the subset with a wider range if that's needed.

## Fix round 1

- **Weight without overtaking.** Piece travel springs keep ζ at 0.92 and scale only the speed: stiffness `520 / mass factor`, damping holding ζ, Motion `mass` 1 (pawn 650/47, knight and bishop 520/42, rook 452/39.2, queen and king 400/36.8). 90% arrival: 138, 154, 165 and 175ms. The old table (one spring, Motion `mass` by piece) lowered ζ as mass grew, so heavier pieces reached 90% *sooner* and overtook lighter ones by up to 1.4% of the distance. `@ci/ui/motion` now exports `springProgress` (the analytic step response) and `arrivalTime`. The tests use them to check each 90% time and that no heavier piece is ever ahead on the way in (after the lighter one arrives, the difference stays under 0.03%). The board can use the same functions to predict the contact frame for sound.
- **New tokens for values DESIGN.md already had:** `--tracking-glyph` (-0.01em), `--board-size` (`round(down, min(100vw − 2 × gutter, 480px), 8px)`, then 448, 512, 560 and 640px at 768, 1024, 1280 and 1600px) and `--font-serif-italic`. Literata italic is now a separate `localFont` with `preload: false`. Tailwind's default `--tracking-*` scale is reset too.
- **Stricter drift tests:** row counts and "no extra tokens" checks for the radii, spacing roles, type steps, durations, easings, staggers and fonts, plus a check of `--board-size` against the board sizes table. `apps/web/test/fonts.test.ts` reads the committed WOFF2 files' `fvar` tables. It checks that Literata (roman and italic) has an `opsz` axis covering every opsz in the type scale, and that `globals.css` sets `font-optical-sizing: auto`, which is what makes the table's opsz equal the px size.
- **Shared contrast maths:** `@ci/ui/lib` exports `parseColour` (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`/`rgba()`), `over`, `luminance` and `contrast`. The tests and `/styleguide` both use it.
- **Biome** ignores `**/*.md` (docs aren't code). **Next** sets `devIndicators: false`.
