<!--
Many thanks for the pull request.

The PR title becomes the commit on main (squash merge) and a line in the changelog, so write it
as a Conventional Commit: "feat(coach): cite the move number", "fix(web): keep the filter on
back". The PR-title check enforces this. See CONTRIBUTING.md.
-->

## What and why

<!-- What does this change for a player or a contributor? Link the issue: "Closes #123". -->

## How I tested it

<!-- Commands you ran and what you checked by hand. Screenshots for anything visible. -->

## Checklist

- [ ] I wrote the failing test first, then the code that makes it pass.
- [ ] The gates pass locally for what I touched: `pnpm lint && pnpm typecheck && pnpm format:check`, the tests
      (web on the host, the Rust gate in Docker), and `node scripts/github/test-settings.mjs` if I
      changed `.github/`.
- [ ] A decision that others should know about has an ADR in `docs/decisions/` (next number,
      listed in its README), or none was needed.
- [ ] UI changes: I followed `packages/ui/DESIGN.md`, `pnpm design:check` passes, and I attached
      screenshots at 390 px and 1440 px, day and night.
- [ ] No secrets, tokens, personal data, or other players' private games in the code, fixtures,
      screenshots or this description.
- [ ] The title is a Conventional Commit, and a breaking change says `!` or `BREAKING CHANGE:`.
