# 0056. Live refresh of the game pages, and a dev-only account link

Date: 2026-11-22. Status: accepted (M5a Task 12). Builds on 0046 (read model and access) and ADR-WEB-8 (event delivery). The plan named this record 0055, which Task 8 had already taken.

## Context

The overview and the games explorer are server components that read the database directly (0046, R2). While the worker analyses a player's games, the stream (ADR-WEB-8, M4 T22/T23) tells the browser `game.analysed`, and M4 T23's `applyEvent` invalidates the TanStack Query key `games(accountId)`. The pages hold no TanStack data of their own, so something has to turn that invalidation into a fresh server render.

Separately, the owner already has 500 analysed games imported by the worker, but the chess.com location-code flow (M4 T19/T21) is the only way to link them to a signed-in user, and the owner wants to open their games now.

## Decision

1. **An observer plus `router.refresh()`.** `LiveRefresh({ accountIds })` mounts one `useQueries` observer per `games(accountId)`, with `staleTime: Infinity` and a `queryFn` that returns `Date.now()`. It holds no game data; it only gives the stream's invalidation a live query to land on. It watches the query cache for an `invalidate` on exactly those keys and then calls `router.refresh()`. Server components stay the single read path: no API route, no client copy of the read model, no second source of truth to keep consistent.
2. **Throttled to one refresh per 3000 ms, with a trailing call.** The first invalidation refreshes at once; any more inside the window collapse into one trailing refresh at the window's end, so a burst of 50 analysed games costs at most one refresh every 3 s and the last game is never missed. Unmounting drops a pending trailing refresh. `staleTime: Infinity` means focus and reconnect never refetch, so only an invalidation refreshes.
3. **Mounted on `/app` and `/app/games`** with the user's scoped account ids (`scopedAccountIds`). The review page is a single finished game and does not refresh under the reader.
4. **`src/lib/query-keys.ts` lands here,** with exactly T23's specified shape, because T23 has not merged yet. Until T23 (stream hook) and T27 (provider wiring) land, nothing invalidates these keys in the running app, so `LiveRefresh` is inert but harmless.
5. **The front door picks the page.** `/` sends a signed-in user with at least one link to `/app`, one with none to `/app/accounts`, and a signed-out visitor to `/sign-in`. The magic-link and social `callbackURL` and the signed-in `/sign-in` redirect point at `/`, so a returning player lands on their overview. New users still go to `/welcome/age`, then Accounts.
6. **`dev:link` for local development only.** `pnpm --filter @ci/web dev:link --email <e> --platform chesscom --username <name>` (`scripts/dev-link-account.ts`, run through tsx with the `development` export condition so it needs no package build):
   - refuses with "dev:link is for local development only" when `NODE_ENV` is `production` or when the database is not local. The host is judged as the driver connects, not as WHATWG `URL` reads it (review fix round 1): postgres.js honours `?host=`, splits the userinfo at the last `@` and does not end the authority at `#`. So any URL with a `host` or `hostaddr` parameter, a fragment, or more than one `@` is refused outright (that also covers unix-socket paths such as `?host=/cloudsql/…`); then the WHATWG host **and** every host postgres.js itself parses (`postgres(url).options.host`, which opens nothing) must be exactly `postgres`, `localhost` or `127.0.0.1`, never a prefix or suffix match. An unparsable URL is refused. The CLI checks all this before opening a connection;
   - after connecting, and before any read or write, it also asks the server for `inet_server_addr()` and refuses unless that is a unix socket (null), loopback, or a private address (10/8, 172.16/12, 192.168/16, fc00::/7), which is where the compose service lives;
   - needs a user who has signed in once and an account the worker has already imported, and says which is missing;
   - inserts one `account_links` row with method `username` and `verified_at` null, doing nothing when it exists, and writes nothing else. It never imports and never touches `games`, `linked_accounts` or any other table.

## The link makes no ownership claim

The row is **unverified**. It is the same kind of link a user gets by typing a chess.com username before verifying (0046, R1: chess.com games are public, and an unverified link is labelled "Unverified" on the overview). Proving ownership stays with the location-code flow (M4 T19/T21), which upgrades the link to verified. Nothing here sets `verified_at`, `linked_accounts.user_id` or `linked_accounts.verified_at`.

## Consequences

- A refresh re-renders the whole server page, which costs a few database reads per window; the 3 s throttle bounds that.
- A second store of query keys exists until T23 merges; T23 must reuse this file, not add another.
- The dev script is safe to keep in the repo: production and remote databases are refused, and it only ever adds an unverified link.
