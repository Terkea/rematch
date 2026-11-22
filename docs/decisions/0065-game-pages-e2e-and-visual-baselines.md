# 0065. Game pages e2e: a guarded seed, their own quality and visual specs

Date: 2026-11-22. Status: accepted (M5a Task 13). Builds on 0046 (read model and access) and ADR-WEB-2 (web dev stack).

## Context

Task 13 adds the end-to-end checks for `/app`, `/app/games` and `/app/games/<id>`. Its brief assumes M4 Task 28 has landed (`quality.spec.ts`, `visual.spec.ts`, `__screenshots__/`), but Task 28 is not on `m5a/base`. The e2e stack must also never write to the owner's games if the default compose project is up by mistake.

## Decision

- **A guarded seed.** `e2e/helpers/seed-games.ts` exports `seedE2eGames({ db?, email? })`. Before any write it throws `Refusing to seed: this database holds real accounts` if any `linked_accounts.username` does not start with `e2e-` or `fake`, on top of the seed helpers' own `*_test` and no-real-accounts checks. The other e2e seeds' accounts are renamed `e2e-…` to pass it. The default URL is `chess_e2e_test` on host port 55432, not the brief's `chess`: the e2e web service reads `chess_e2e_test`, and `assertSafeTestDatabase` refuses a name without `_test`. It is idempotent (fixed `platform_game_id`s), because several specs seed.
- **The refusal test uses a synthetic name**, not the owner's handle the brief names: the owner's handle is never committed.
- **Own spec files.** The quality loop and the baselines for the game pages live in `game-pages-quality.spec.ts` and `game-pages-visual.spec.ts`, so Task 28's `quality.spec.ts` and `visual.spec.ts` merge without an add/add conflict. Task 28 may fold them in.
- **Snapshot path.** `snapshotPathTemplate` is `e2e/__screenshots__/<spec>/<name>-<project>.png`, the directory the M4 plan names. Baselines are made only in the Linux `qa` container.
- **No axe yet.** `@axe-core/playwright` is Task 28's dependency and is not installed; the game-pages quality loop checks sideways overflow at 360 px, a visible focus outline on every tab stop up to the page's content, and no `NaN`, `Infinity`, `undefined` or `null` in the visible text. Task 28 adds axe to the loop.
- **Labels as shipped.** The brief's "Show engine move" switch was renamed in Task 10's fix round to "Show the engine's next move"; the e2e uses the shipped label.

- **The trend's keyboard budget is measured on a production build only.** At 4× CPU throttle it measures key to paint (two animation frames after the keydown) against the same wait with no key; the key's cost must fit in one 16 ms frame. `next dev` runs React's development build, so the test skips without `E2E_PRODUCTION=1`. Enforcing it on every change needs a production-build e2e lane in CI; that lane is **parked for the CI and launch work** (ruling on review-t13 I2), and until then the number is measured by hand and recorded in the task report.
- **Projects own their specs.** The visual projects match only `visual.spec.ts` and `game-pages-visual.spec.ts`; the `e2e` project ignores them. A plain `playwright test` runs each spec where it belongs.

## Consequences

- If the owner's database is ever the target, the seed refuses before writing; a test pins it.
- Task 28 has two spec files to merge or keep, and adds axe to the game pages.
