# 0096. The public face: the Rematch name, the README and a GitHub Pages site

Date: 2026-11-23. Status: accepted (lane `site`, the owner's open-source launch plan, Lane L2).
Number 0095 is the open-source SDLC and releases (lane `oss`).

## Context

The repository goes public as **Rematch, chess improver** ("Stop losing to the same move.") to
build interest and bring in contributors. The old README was 73 lines of developer notes, the app
still said "chess-improver", and nothing explained what the product does, why it can be trusted or
how to help. The owner wants the brag video in the repository and a site that feels like the app,
not a template.

## Decision

1. **The name.** What a person reads says "Rematch": the page title, the header and auth
   wordmarks, the sign-in mail, the age gate, the invite message, Better Auth's `appName` and the
   `MAIL_FROM` display name (`apps/web/src/brand.ts` stays the one place). Package scopes (`@ci/*`),
   databases, compose projects, user agents and the Lichess client id keep `chess-improver`, so no
   data, volume or OAuth registration moves.
2. **A site package, `site/` (`@ci/site`).** Astro with Starlight, static, served by GitHub Pages
   at `https://terkea.github.io/rematch/` (base `/rematch/`). A custom landing page
   (`src/pages/index.astro`) and Starlight docs under `/docs/`. Dependencies are pinned exactly and
   pass the 7-day age rule; Astro is held at 7.4.1 because 7.4.2 and later refuse Node below
   22.23.4 and the host runs 22.22.
3. **One design law.** The site reads `packages/ui/src/styles/tokens.css` at build time
   (`site/scripts/sync-content.mjs` turns its Tailwind `@theme` block into plain custom
   properties) and the app's own WOFF2 fonts, and maps Starlight's palette onto the tokens. Day and
   night follow `data-theme`, which both Starlight and the tokens use. `pnpm design:check` now
   scans `site/src` (including `.astro` files). Docs code is set in Schibsted Grotesk on a `well`
   with no syntax colours: DESIGN.md allows no monospace and no decorative hues.
4. **Generated docs, never copied.** The decision log and the research notes are generated at
   build time from `docs/decisions/*.md` (with the README as the index) and `docs/research/*.md`.
   Links between records become site links; links to any other file go to GitHub. The generated
   files are git-ignored.
5. **Real interface, fixture data only.** Feature screenshots are captured headless from
   `/styleguide` (its fixture games and demo numbers, labelled as such), never from `/app` or a
   signed-in session. The private player in the eval-graph fixture is cropped out.
6. **The video is in the repository.** `site/public/media/rematch.mp4` (60 s, about 5 MB), its
   poster and an 8-second preview GIF (0.75 MB) are normal git blobs, no LFS. The README shows the
   GIF and links the MP4 and the site; the site plays it only when the visitor presses play (M1).
7. **Live good first issues.** The landing page fetches open `good first issue` issues from
   GitHub's public search API in the browser, without a token. A private repository, a rate limit
   or a network error shows a sentence and the issues link instead.
8. **Pages and CI.** `.github/workflows/pages.yml` builds on every pull request and on `main`, and
   deploys from `main` with `actions/upload-pages-artifact` and `actions/deploy-pages`
   (`pages: write` and `id-token: write` on the deploy job only, no secrets, actions pinned by
   SHA). `ci.yml` gains a `site` job: the site's tests, its build and a local link check
   (`site/scripts/check-links.mjs`: every internal link and anchor in `site/dist`, and every
   relative link in the README).

## Consequences

- The `main` ruleset requires `site` too (`.github/settings/rulesets/main.json`; the settings test
  compares the required checks with the job names in `ci.yml`), and `docs/SDLC.md` lists it.
- Visual baselines that show the wordmark change once, regenerated in the `qa` container.
- A new ADR, research note or README change appears on the site at the next build of `main`.
- The repository grows by about 6 MB of media, once.
- Raising Node to 22.23.4 or later (`.nvmrc` and the images) unblocks newer Astro releases.
