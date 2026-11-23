# 0103. The Rematch palette, a crayon for celebrations and dad's handwriting

- Status: accepted; the warm palette withdrawn by Amendment 1
- Date: 2026-11-23
- Lane: `feat/brand-palette`

## Context

The approved Rematch brand (`docs/superpowers/specs/2026-11-23-rematch-brand-design.md`) is a kid at the board, learning from his dad. The "annotated scoresheet" direction (ADR-WEB-7) had cool blue-grey paper and a navy night, and `packages/ui/DESIGN.md` banned cream paper. The spec proposed warmer values, a new crayon colour for celebrations and a handwritten face for dad's margin notes, and left the exact values, the texture technique and the face to the builder, with one constraint: the face must cover Romanian diacritics.

## Decision

1. **The palette warms, the structure stays.** Day: paper `#F4EFE6`, sheet `#FBF8F2`, ink `#1F2A3D`, pen `#C8372D`, good `#2F6B4F`, pencil `#6B6458`, the spec's values exactly, with well, rules, the softer inks and a greige board derived to pass contrast. Night study becomes a desk lamp on warm dark paper (paper `#1C1915`, ink `#EFE8DC`, a shallow warm `well` `#191612`, pen `#EE6258`, a true red rather than coral). Every pair in DESIGN.md "Contrast results" passes; the tightest are `ink` on a selected day `board-dark` (4.58) and night pieces on `board-dark` (3.28).
2. **`B1` becomes "paper is faded, warm and photographic, never yellow".** `packages/ui/test/tokens-contrast.test.ts` holds it as an OKLCH rule: every surface in both themes has a hue between 40° and 100° and a chroma of at most 0.025. A cream page sits well above that.
3. **`--color-crayon`** (`#E3A92F` day, `#D9A441` night) is for celebrations only: the crayon stroke under a solved drill or a milestone. It is decorative (the words beside it say what happened), at least 1.8:1 on `paper` and `sheet`, and at least 30° of hue from the pen, so it never reads as a mistake. A test fails any use of the token outside `@ci/ui/brand`, the styleguide and `tokens.css` (`B19`).
4. **Caveat** (SIL OFL 1.1, the Caveat Project Authors, from `google/fonts`) is dad's handwriting, `--font-hand`, for margin notes only. It is variable (`wght` 400–700), self-hosted like Literata, not preloaded, and subset to Basic Latin, Latin-1, Latin Extended-A and ș ț Ș Ț, so Romanian is covered. 78 KB.
5. **The hand-made touches are components** in `@ci/ui/brand`: `MarginNote` (hidden from assistive tech when it repeats visible text, a named `note` when it carries its own words), `CrayonStroke` (a seeded wavering line drawn once on `--ease-pen`) and `PencilTexture` (seeded SVG `feTurbulence` in `pencil` at 5%). They appear only in empty states, the landing page, a drill solved and milestones, never on charts, tables, the board or a number (`B20`).

## Options considered

1. **Spec values plus derived tokens and a contrast-held warmth rule (chosen).** The brand's promises become tests, so a later tweak can't drift into cream or into a navy night.
2. **Keep the cool palette and warm only the site.** Rejected in the spec: the app would not match the story the site tells.
3. **A handwritten face that is a cursive school hand (Playwrite RO).** Rejected: beautiful at display size, hard to read at 20px, and it reads as a school exercise rather than a note in the margin. Caveat reads at `xl`.
4. **Literata italic as the "hand".** Rejected: it is already the annotation comment, and the note would not read as a different, personal voice.

## Consequences

- Every colour token changes in both themes, so every visual baseline changes. They are regenerated once in the final pre-launch gate, not in this lane.
- Pieces need no rebuild: their sprites use `var(--color-piece-dark)` and `var(--color-piece-light)`.
- `/licenses` credits Caveat with its OFL text.

## Amendment 1 (2026-11-23)

The owner preferred the original cool scoresheet palette; the warm palette is withdrawn, and the logo, wordmark, margin notes and crayon stay.

1. **Decision 1 is reversed.** Every colour token in both themes is back at its value from before this ADR (`bfbc3bd^1`): paper `#F3F5F8`, sheet `#FBFCFD`, ink `#16233B`, pen `#D3312B`, good `#24684B`, pencil `#56657F`, and the navy night study (paper `#121A2B`, ink `#E7ECF4`). The contrast tables in `packages/ui/DESIGN.md` are the fix round 2 tables again, with two crayon rows added.
2. **Decision 2 is reversed.** `B1` is "paper is never cream" again. `packages/ui/test/tokens-contrast.test.ts` holds it as an OKLCH rule: every surface in both themes has a hue between 230° and 285°, day chroma is at most 0.015, and night paper is navy (lightness under 0.3, hue 240°–280°).
3. **Decisions 3 to 5 stand.** The crayon keeps `#E3A92F` and `#D9A441`: on the cool paper it is 1.93:1 on day `paper`, 2.05:1 on `sheet` and 7.73:1 on night `paper`, and 53° of hue from the pen, so it still shows and never reads as a mistake. Caveat, `MarginNote`, `CrayonStroke` and `PencilTexture` are unchanged.
4. **The brand rasters follow the tokens.** `apps/web/scripts/brand-assets-lib.ts` draws in `#16233B` on `#F3F5F8`, with `#E7ECF4` and `#97A6C0` for dark chrome, and `apps/web/test/brand-assets.test.ts` now reads those values from `tokens.css`, so the two can't drift apart. Every visual baseline changes again, regenerated once in the final gate.
