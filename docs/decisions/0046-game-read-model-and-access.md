# 0046. The games read model: scoped, read-only, no API routes

Date: 2026-11-22. Status: accepted.

## Context

M5a puts the player's analysed games on screen: an overview, a games explorer and a single-game review. The data already exists. The TS worker writes `games`, `moves`, `position_evals`, `import_runs` and `linked_accounts`, and the web app links users to accounts through `account_links`. The web app must read these tables without ever changing them, and must never show one user another user's games. The owner's real data (500 analysed games) lives in the same database as the app.

## Decision

- **R1, scope.** A user may read a game only when its `account_id` is one of their `account_links`, verified or pending. This is exactly M4 T22's `userAccountIds(db, userId)`, so the event stream and the read model agree on what a user owns. chess.com games are public, and the overview labels an unverified link "Unverified". If this proves wrong, the fix is one `where verified_at is not null` in `userAccountIds`.
- **One scoping function.** `scopedAccountIds(db, userId, only?)` in `apps/web/src/server/games/scope.ts` returns `userAccountIds`, narrowed to `only` when `only` is one of them, and `[]` when `only` is foreign. Every read starts from it, and an empty list short-circuits to an empty result. A foreign id therefore behaves exactly like an id that does not exist: the list is empty and the review is `notFound()`, never 403, so no id is confirmed to exist.
- **Read-only transactions.** `readOnly(db, fn)` runs `fn` inside `db.transaction(..., { accessMode: "read only", isolationLevel: "repeatable read" })`. Every exported read in `server/games`, `server/stats` and `server/overview` goes through it, so a stray write fails in Postgres with code 25006 instead of changing data. Repeatable read gives every statement in one read the same snapshot, so a page's "of 412 games" count and its rows agree even while an import is running (under the default read committed, each statement would take its own snapshot).
- **R2, no API routes.** Pages are server components that call the read model directly, and the URL holds all list state (ADR 0048). Client islands get only the rows they render.
- **Module boundary.** `apps/web/src/server/games` (the list and the review loader), `server/stats` (the honest statistics, ADR 0047) and `server/overview` (the overview aggregates). Later milestones add siblings, never a shared grab bag: R2 adds `server/profile` and R3 adds `server/coaching`, beside `server/games`. Each exports through its `index.ts`.
- **No migration.** M5a adds no migration and no index. The existing `games_account_played_idx` (`account_id, played_at desc`) serves the list. Judgement counts and ply counts come from one grouped subquery over `moves`, counting the player's own moves (`moves.color = games.player_color`). Except when sorting by blunders, the page's ids are chosen first and the subquery covers only those 25 games, so a page costs the same whatever the length of the player's history; the blunders sort needs the counts of every filtered game. The only M5a code that writes is test helpers, which write synthetic or public fixture rows and refuse (`assertSeedable`) unless the database name ends in `_test` and it holds no imported or really verified accounts, and the dev-only link script (Task 12), which writes only `account_links`.

## Consequences

Adding a new page means adding a read in one of these modules, wrapped in `readOnly` and started from `scopedAccountIds`; reviewers check both. Tests seed a second user and assert that the first never sees the second's games. If list queries get slow at a few thousand games, a covering index is a later migration with its own ADR.
