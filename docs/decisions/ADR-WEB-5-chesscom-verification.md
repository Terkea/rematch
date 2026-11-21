# ADR-WEB-5. chess.com linking and location-code verification

Date: 2026-11-21. Status: accepted. Implements M4 Task 19. Builds on ADR 0040 (chess.com first), ADR 0041 (private beta), ADR-WEB-3 (auth data model) and ADR 0033 (SQL migrations own the schema).

## Context

chess.com's Published-Data API is public, read-only and unauthenticated, so it cannot prove that a person owns a username. chess.com OAuth exists but needs chess.com's approval of the application (spec §18.1). Until then, ownership has to be shown with something only the account holder can change: the free-text `location` field on the chess.com profile.

## Decision

### Tables: migration `20261121010100_web_linking.sql`

These are Task 2's linking tables. Task 2 was never built, so this task builds them with Task 2's values.

- **`account_links`** joins a user to a shared `linked_accounts` row.
  - Columns: `id` uuid, `user_id` (cascades on user delete), `linked_account_id`, `method` (enum `link_method`: `username`, `location_code`, `lichess_oauth`, `chesscom_oauth`), `verified_at` (nullable), `created_at`.
  - `(user_id, linked_account_id)` is unique.
  - A **partial unique index on `linked_account_id` where `verified_at is not null`** allows one verified owner per account.
  - `linked_account_id` does not cascade, because `linked_accounts` rows and their games are shared and are never deleted by the web app.
- **`chesscom_verifications`** holds one code per link: `account_link_id` (unique, cascades), `code` (CHECK on the format), `expires_at`, `last_checked_at`, `checks` (default 0) and `created_at`.
- `linked_accounts.user_id` stays for the M1 CLI. The web app reads only `account_links`.
- The Drizzle schema is regenerated from the migrations, and `@ci/db` exports `accountLinks`, `chesscomVerifications` and `linkMethod`.

### The flow

1. **Start.** `POST /api/accounts/chesscom { username }`.
   - The username must match `^[A-Za-z0-9_-]{3,25}$` and is stored lower-cased.
   - The profile must exist on chess.com.
   - A user can have at most 3 chess.com links.
   - The response is an unverified link (`method: username`) and a code. Asking again for the same name returns the same link and code, with no request to chess.com. Once the code has expired, asking again issues a fresh one.
2. **The user puts the code into their chess.com location.** It can go anywhere in the field, for example "Paris CI-7K3QXM".
3. **Check.** `POST /api/accounts/chesscom/:linkId/verify` reads the profile once.
   - The match is case-insensitive: the code must appear anywhere in the trimmed location.
   - On a match, the link gets `verified_at` and `method: location_code`.

### The code

The code is `CI-` plus 6 characters from `23456789ABCDEFGHJKMNPQRSTUVWXYZ`, drawn with `crypto.randomInt`.

- The alphabet leaves out 0/O and 1/I/L, so the code survives being retyped from a phone.
- It allows about 8.9 × 10⁸ codes. That is far more than enough, because a code is never a secret credential. It only has to be unlikely to be in someone's location already. The `CI-` prefix makes that even less likely, and it tells the user what the code is for.

### Timing rules

- **60-second cooldown per link.** Within 60 s of `last_checked_at`, a check answers `pending` with `nextCheckAt` and makes no request.
  - The cooldown is claimed with a conditional `UPDATE … WHERE last_checked_at IS NULL OR last_checked_at <= now − 60 s`, so concurrent checks make exactly one request (tested).
  - A claimed check counts even if chess.com then fails, which errs on the side of fewer requests.
- **7-day expiry.** An unverified code older than 7 days answers `expired` without a request, and the list shows `expired` with no code.
- **On demand, not in the background.** The check runs when the user presses the button. It costs one profile request and no engine work. A background recheck (for example, noticing that an owner has removed the code, or re-verifying periodically) belongs to M7.

### Ownership: unverified links never block, and the newest verification wins

- Any number of users may hold an **unverified** link to the same account, so someone who links a username that isn't theirs cannot lock out the real owner (tested: `unverified links do not block others`).
- On a successful check, one transaction makes this link the verified owner and clears every other user's verified link to that account (`verified_at = null`, `method = username`). The transaction runs under a per-account advisory lock, and the partial unique index backs it up.
  - **Why the newest wins:** only the current account holder can edit the location, so the latest proof is the strongest evidence. A stale owner (an old session, or an account that has changed hands) gives way.
  - **The cost:** a real owner who verifies, then removes the code, and is later followed by someone else who briefly controls the account, loses the verification until they check again. That is acceptable. Control of the account is exactly what is being proven.
- **Removing a link** deletes only the user's own `account_links` row and, by cascade, its code. The shared `linked_accounts` row and its games stay.

### chess.com requests

- The only endpoint used is `GET {CHESSCOM_API_BASE_URL}/pub/player/{username}`.
- **User-Agent: `chess-improver-ci (+https://github.com/chess-improver)`.** No email address goes into any header.
  - This deliberately departs from the plan's `chess-improver/0.1 (+mailto:<CONTACT_EMAIL>)`: by controller ruling, the web app never sends an address in a header.
  - The worker's importer keeps its own User-Agent.
- **Serial.** One chess.com request is in flight per process at a time.
- **Timeouts and errors.**
  - Each request times out after 5 s.
  - A 429 is retried once, after its `Retry-After` (default 1 s, capped at 2 s because the request is interactive).
  - A second 429, a 403, a 5xx, a timeout, a network error or a malformed body raises `ChesscomUnavailableError`, which the routes return as 503 `chesscom_unavailable`.
  - A 404 or 410 means the player does not exist.
- **Tests make no live calls.** They use a fake fetch and a local mock HTTP server serving a synthetic, recorded-shape profile (`apps/web/test/fixtures/chesscom/player-hikaru.json`). No real user's data is committed.

### Flags

- **`CHESSCOM_ENABLED`.** When it is off, both chess.com routes answer 404.
  - `GET /api/accounts` and `DELETE /api/accounts/:linkId` are platform-neutral and stay available.
  - Compose turns it on in development (`${CHESSCOM_ENABLED:-1}`, per ADR 0040). The env default stays off, so production needs an explicit go-ahead.
- **`chesscomOAuthProvider(env)`** uses OIDC discovery at `https://oauth.chess.com/.well-known/openid-configuration`.
  - It is registered only when `CHESSCOM_OAUTH_ENABLED=1` and both `CHESSCOM_CLIENT_ID` and `CHESSCOM_CLIENT_SECRET` are set.
  - Its endpoints are unverified until chess.com approves the application.
  - **Its email is never treated as verified** (`mapProfileToUser` forces `emailVerified: false`), and chess.com is not a trusted linking provider.
    - As a result, Better Auth never links a chess.com identity to an account by email.
    - It does not link one through `/link-social` either, so chess.com OAuth currently adds no Better Auth `account` row.
    - This keeps ADR-WEB-3's `NON_EMAIL_PROVIDERS` exemption safe.
  - Once chess.com approves the application, OAuth ownership will be written to `account_links` as `method: chesscom_oauth`, not to `account`.

### API

| Route | Result |
|---|---|
| `GET /api/accounts` | 200 `[{ linkId, platform, username, displayName, verified, status, code? }]`. `status` is `ok`, `pending_verification`, `expired`, or `expiring` (reserved for OAuth, Task 18) |
| `POST /api/accounts/chesscom` | 201 `{ linkId, username, code, verified }`; 400 `invalid_username`, 404 `not_found`, 409 `limit_reached`, 503 `chesscom_unavailable` |
| `POST /api/accounts/chesscom/:linkId/verify` | 200 `{ status, nextCheckAt }`; 404 for a link that is missing or someone else's; 503 `chesscom_unavailable` |
| `DELETE /api/accounts/:linkId` | 204; 404 for a link that is missing or someone else's |

Every route needs a signed-in user who has answered the age screen (401 `unauthenticated` or 403 `age_required`). The mutating routes rely on the session cookie's `SameSite=Lax`: a cross-site POST or DELETE carries no session, so it gets 401.

### The verified-email rule through the real OAuth callback (T17 hand-off)

`apps/web/test/auth/oauth-callback.test.ts` drives Better Auth's real `/sign-in/social`, `/link-social` and `/callback/:id` endpoints, with only the provider's token, userinfo and discovery requests faked. It covers six cases:

- **Google, live session, unverified email:** the identity is linked. This is the exemption.
- **Google, session dropped before the callback:** `email_not_verified`. The exemption needs the live session on the callback request itself.
- **Google, implicit link with an unverified email:** `email_not_verified`. With a verified email, the implicit link goes through.
- **chess.com, implicit link** (even when its userinfo claims a verified email): `account_not_linked`.
- **chess.com, new account under `INVITE_ONLY`:** `not_invited`.
- **chess.com, live session:** `unable_to_link_account`.

## Consequences

- Task 21's linking screen calls these four routes. It shows the code and `nextCheckAt` while a link is pending, and explains `expired` (start again) and 503 (chess.com is busy, try in a minute).
- A burst of start requests is bounded by chess.com's serial queue, not by a per-user throttle. That is accepted for the single-process private beta. A public launch needs a per-user limit on starts.
- M7 adds the background recheck. Lichess OAuth (Task 18) adds `lichess_oauth` links and the `expiring` status.
