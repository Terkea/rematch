# Rematch brand implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** roll out the approved brand: a kid at the board learning from his dad, the warm palette, the fatherly coach voice and the chosen logo.

**Architecture:** three independent lanes on separate branches.
- Task 1 changes only design tokens, DESIGN.md and the soft UI touches.
- Task 2 changes only AI prompts and the app's coach-voice copy.
- Task 3 changes only brand assets (favicons, header, site, README, social card, video).

They touch disjoint files except the visual baselines, which are regenerated once in the final pre-launch gate.

**Tech stack:** Next.js + Tailwind tokens (`packages/ui/src/styles/tokens.css`), Rust prompt registry (`backend/crates/ai/prompts`), Astro/Starlight site (`site/`), ffmpeg/Playwright for media.

**Spec:** `docs/superpowers/specs/2026-11-23-rematch-brand-design.md` (approved 2026-11-23).

## Global constraints

- No change to data, statistics, the validator rules, the 80% ranges or the trainer logic.
- Red (`--color-pen`) means "dad's pen on a mistake" and nothing else. The logo is ink only.
- The dad story never appears in app copy, prompts or the site. It stays only in the README section "Why I'm building this", which nobody edits.
- All public text follows the maintainer's voice guide (British spelling, fact first, no banned words).
- Use tokens only: no raw colours in components (`pnpm design:check`). Contrast is at least WCAG AA on sheet and paper, day and night.
- Prompts are versioned: a new version file per changed prompt, the old version kept for stored hashes, and the prompt table and prompt-set tests updated.
- Lane rules: one Rust run and one e2e stack at a time, with at least 4 GB of free memory.

## Review focus

1. **Night theme contrast** after warming. Pencil text and pen marks on the night paper must still pass AA, enforced by a contrast test over the token pairs.
2. **A fatherly line that sneaks in a fact.** "You always rush the opening" is an uncited quantity claim. The validator tests stay green, and a new prompt test asserts the tone prompt forbids uncited generalisations.
3. **The favicon at 16 px** in a real browser tab, light and dark browser chrome, checked from a screenshot.
4. **Mixed old and new prompt versions** in stored reports. The reports render with their stored version, so existing reports must not re-validate under the new tone (an existing test or a new one).
5. **Handwritten notes and screen readers.** Margin notes (e.g. "same knight again") are decorative duplicates of real text, or carry accessible text.

---

### Task 1: warm palette, DESIGN.md and soft hand-made touches

Branch `feat/brand-palette`, worktree `.worktrees/brand-palette`.

**Files:**
- Modify: `packages/ui/src/styles/tokens.css`, `packages/ui/DESIGN.md` (direction, palette tables, bans).
- Create: `packages/ui/src/brand/margin-note.tsx` (the handwritten note), plus a small pencil-texture utility.
- Test: `packages/ui/test/tokens-contrast.test.ts`, `packages/ui/test/margin-note.test.tsx`.

**Interfaces:**
- Produces: the token values listed in the spec (paper `#F4EFE6`, sheet `#FBF8F2`, ink `#1F2A3D`, pen `#C8372D`, good `#2F6B4F`, pencil `#6B6458`, the new `--color-crayon` `#E3A92F`), plus a warmer night set. The final values may move only to pass contrast.
- Produces: `<MarginNote>{text}</MarginNote>`, rendered in the handwritten face, `aria-hidden` when it duplicates visible text, with an accessible `label` prop otherwise.

- [ ] **Step 1:** write `tokens-contrast.test.ts`. Every text/background token pair used in DESIGN.md is ≥ 4.5:1 (body) or ≥ 3:1 (large or graphical), day and night. Run it: it fails on the new values until they're tuned.
- [ ] **Step 2:** update `tokens.css` and the DESIGN.md tables. Replace the ban "paper is never cream" with "paper is faded, warm and photographic, never yellow". Add the crayon rule: celebrations only, never data. Run the contrast test until it passes.
- [ ] **Step 3:** write the `margin-note.test.tsx` cases (renders the text; `aria-hidden` without `label`; accessible name with `label`), then implement `MarginNote` and the pencil texture. Use them only in empty states, the trainer "solved" moment and the landing page. Never on charts or tables.
- [ ] **Step 4:** load the `design-system` skill. Run `pnpm design:check`, the ui and web unit tests, and screenshots of `/styleguide` and a few app pages at 390 and 1440 px, day and night, from fixtures only. Get a fresh critic's review.
- [ ] **Step 5:** commit (Conventional, with trailers). Don't regenerate visual baselines; the final gate does it.

### Task 2: the fatherly coach voice

Branch `feat/brand-voice`, worktree `.worktrees/brand-voice`.

**Files:**
- Create: new versions under `backend/crates/ai/prompts/tone/`, and wherever report, chat or weekly-review embed tone.
- Modify: the prompt registry or table and the prompt-set tests.
- Modify: the app's coach-voice copy (trainer feedback, empty states, focus card, practise card) in `apps/web/src`.

**Interfaces:**
- Produces: the tone prompt "dad at the board". It speaks to the player directly, short and plain. It names the move and what it left undefended. It praises effort and progress only with cited evidence. It never flatters. It says "let's try this one again" style invitations. It never uses generalisations without a citation ("always", "never", "every time" need numbers). British spelling.
- Consumes: the validator, unchanged.

- [ ] **Step 1:** write a failing prompt test: the new tone version exists, it's referenced by report/chat/weekly, and its text contains the no-uncited-generalisation rule. Run it and see it fail.
- [ ] **Step 2:** write the tone vN+1 prompt plus any version bumps for prompts that embed it. Keep the old versions. Update the table, so the prompt-set test expects the new versions.
- [ ] **Step 3:** run the full ai crate tests (the validator tests must be unchanged and green), plus a stored-report rendering test proving old reports keep their version.
- [ ] **Step 4:** rewrite the app's coach-voice copy in the same voice. Update the e2e and unit copy assertions, and run the web unit tests.
- [ ] **Step 5:** run the Rust gate (one run) and commit. One optional live check by the coordinator afterwards, with one coach question on the owner's stack.

### Task 3: logo rollout

Branch `feat/brand-logo`, worktree `.worktrees/brand-logo`. It starts after Task 1's token values exist; read them from Task 1's branch, or use the spec values.

**Files:**
- Source: `packages/ui/assets/brand/rematch-mark.svg`, `rematch-icon.svg`.
- Modify:
  - `apps/web/src/app/icon.svg` and the favicon set (`favicon.ico` 16/32/48, apple-touch-icon 180, manifest icons 192/512);
  - the app header wordmark (a lockup component: mark plus "rematch" in Literata italic);
  - the site logo and favicon (`site/`), and the README header (mark above the title);
  - the social card (`site/public/og.png`, 1200×630) and the GitHub social preview image saved in `site/public/media/` (the owner uploads it at launch).
- Modify: the video's title and outro cards. Re-render from `brag-output/v2/work` with the mark, update `site/public/media/rematch.mp4`, the poster and the GIF, and keep the 60 s length.

- [ ] **Step 1:** write a test that `icon.svg` and the manifest icons exist with the right sizes, and the header renders the lockup with the accessible name "Rematch". See it fail.
- [ ] **Step 2:** generate the raster sizes from `rematch-icon.svg` (Playwright or resvg; no new runtime deps), wire them in, and build the lockup component (tokens only).
- [ ] **Step 3:** update the site and README header, keeping "Why I'm building this" untouched. Make the social card.
- [ ] **Step 4:** re-render the video title and outro cards with the mark. Check the frame count is 1800 and that frame 0 is the poster.
- [ ] **Step 5:** screenshot the browser tab at 16 px on light and dark chrome, the header, the site and the social card. Get a fresh critic's review, then commit.

## Execution

The owner asked for speed, so Tasks 1 and 2 run in parallel now, and Task 3 starts at the same time using the spec's token values. The coordinator merges each one after its gates. Visual baselines are regenerated once in the final pre-launch gate.
