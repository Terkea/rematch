---
name: design-system
description: Use whenever creating or changing any UI, page, component, style, animation, or chart in this repo.
---

# Design system: the annotated scoresheet

The UI is a premium, deliberately non-generic chess-coaching app. A page that could belong to any
SaaS template fails review, however tidy. The law is `packages/ui/DESIGN.md` (read the section you
touch before you build). The values are in `packages/ui/src/styles/tokens.css`; a test keeps the two in step.

Companion files here: `tokens.md` (what each token is for), `components.md` (which primitive, which
states), `motion.md` (what animates). For boards, notation, evals and chess charts also load `chess-ui`.

## Non-negotiables

- **Tokens only.** Colour, type, radius, space, shadow, duration, easing and spring all come from `tokens.css` and `@ci/ui/motion`. No raw hex or rgb outside `tokens.css`, `packages/ui/src/board/**` and `packages/ui/src/annotations/**`. No arbitrary `[#abc]` or `[13px]` values.
- **No new font, colour, radius, shadow or easing without proposing it first.** Tailwind's defaults are switched off in `tokens.css` (B14), so an invented class silently does nothing. Propose the token (its role, contrast numbers), get a ruling, then add it to DESIGN.md, `tokens.css` and the test.
- **The ban list is binding.** `B1` to `B18` in DESIGN.md ("Ban list"). A review names the ID it finds. `pnpm design:check` (`scripts/ban-rules.ts`) enforces what a machine can.
- **Honesty rules** (the product's integrity, not decoration):
  - State **n with every rate** ("Based on 38 rapid games"). Say it once per section; keep it for screen readers on repeats.
  - **Unknown is never 0.** A missing or below-minimum value shows a dash and the shortfall ("Need 9 more games"), never `0`, `0%`, or an empty chart that looks like a result.
  - **No `NaN`, `Infinity` or `undefined` text**, ever. Guard every division and formatter; the quality specs grep rendered pages for them.
  - **Labels never overclaim.** No "improved" or "worse" unless the intervals separate. A rate is an estimate with a stated range. A fixture or synthetic number is never presented as the user's.
  - Real chess data only (B15): real games, real evals, real names.
- **Copy:** sentence case, British spelling, active verbs, an action keeps its name from button to toast ("Connect chess.com" then "chess.com connected"), errors say what happened and how to fix it. No `→`, no all-caps eyebrows.
- **Icons** only through `Icon` from `@ci/ui/icons` (Phosphor, never Lucide or emoji, B13). Add to the curated list in `packages/ui/src/icons/index.ts`; never import Phosphor elsewhere.
- **Colour never carries meaning alone.** Pen red means "your mistake" (and form errors), nothing else.
- Source-only UI packages (ADR-WEB-1): import from `@ci/ui/<folder>` (`primitives`, `overlays`, `board`, `annotations`, `charts`, `stats`, `review`, `motion`, `icons`, `progress`, `eval-graph`, `timeline`, `area-score`, `audio`, `fixtures`).

## Workflow

1. **Plan.** Name the surface's one job, its states (loading, empty, error, long content, night) and which primitives and tokens it uses. Read DESIGN.md for the parts it touches. Decide what moves and why (`motion.md`).
2. **Lab for a new surface.** A new kind of surface gets two or three real alternatives at `apps/web/src/app/lab/<surface>/<a|b|c>/page.tsx` in `LabFrame` (ADR-WEB-10), a rationale per variant, and a ruling on the pick. Examples to imitate: `packages/ui/design/decisions/board.md`, `packages/ui/design/decisions/styleguide-hero.md`. Small changes to an existing surface skip this.
3. **Build.** Test first (`packages/ui/test/*.test.tsx`). Compose from `@ci/ui`. Add reusable pieces to `/styleguide` (`apps/web/src/app/styleguide/sections/`).
4. **CL, the critique loop.** Capture with `pnpm shots` (`.shots/<route>/<width>-<theme>.png`). Judge against `packages/ui/qa/critique-prompt.md`: ideally a fresh critic who sees only the screenshots, DESIGN.md and `packages/ui/design/references/REFERENCES.md`, never your code. It returns at most 10 ranked issues and a verdict. Fix, retake, repeat until `pass` (a `pass` allows no blocker or major). Log each round in `packages/ui/qa/log/<surface>.md`: shots, the JSON, what changed, the verdict. If you could not get an independent critic, write "self-critique only"; the gate still wants a fresh `pass`.

## Definition of done

- Screenshots at **390 and 1440, in both day and night**, each looked at, with no horizontal scroll (also at 360), visible focus, text 4.5:1 and strokes 3:1.
- `pnpm design:check` reports 0 findings.
- A critique verdict of `pass`, logged.
- Every state exists: loading through `ProgressPlaceholder` (no spinner, B18), empty, error, long content.
- Reduced motion works (M10); nothing animates outside the motion rules.
- `pnpm lint` and `pnpm typecheck` clean; tests written first and passing.
- A decision that bends DESIGN.md gets an ADR in `docs/decisions/` and a dated line in DESIGN.md "Revisions".

## Tried and rejected

Collected from `packages/ui/qa/log/*.md` and `packages/ui/design/decisions/*.md`. Do not reintroduce without a ruling.

- **A frame or outside coordinate lanes on the board** (variant a, "Printed diagram"): costs 24px of phone board and breaks "no frame". Coordinates sit on the squares.
- **Book hatching on dark squares** (variant b): charming at 1440, but fights piece outlines, vanishes at night and hides the last-move tint.
- **A constant-width, vector-looking pen stroke:** reads as a focus ring at 41px squares. A broad marker (0.11 square) turns the arrowhead into a blob. The pen has variable width and a seeded drift.
- **A ruler beside a KPI band that repeats the sparkline's range:** more ink, no information.
- **Rolling numbers from JavaScript alone:** a hard load paints the final page, then reassembles it on hydration. Use the CSS-keyframe reveal gated by the session cookie.
- **Hover that steps down to `well` inside a night sheet:** about 1.1:1, invisible. Night hover steps up toward `ink`.
- **`ink-faint` as a control's hover edge:** too close to `rule-strong`. The hover edge steps to `ink-soft`.
- **A colour swatch for the result colour in game rows:** replaced by a scoresheet mark (1, ½ or 0 on a square in the colour played).
- **Stat banners, icon-topped feature grids, coloured card borders** (B9, B10), and bold or accented words in running text.
- **Full-page screenshots of charts that measure their parent:** captured mid-measure and blank. Wait for the clip to reach full width and shoot one tall viewport.
- **Spinners** (B18): a soaking ink line and `ProgressPlaceholder` instead.
