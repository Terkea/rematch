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
  - `linked_account_id` cascades too, so deleting a platform account (the R2 account-deletion path) is never blocked by the links to it.
- **`chesscom_verifications`** holds one code per link: `account_link_id` (unique, cascades), `code` (CHECK on the format), `expires_at`, `last_checked_at`, `checks` (default 0) and `created_at`.
- **`chesscom_request_budgets`** (per user, cascades) and **`chesscom_profile_requests`** (per lower-case username) hold the outbound throttles (below). They are keyed outside `account_links`, so deleting and re-adding a link resets neither.
- `linked_accounts.user_id` stays for the M1 CLI. The web app reads only `account_links`.
- The Drizzle schema is regenerated from the migrations, and `@ci/db` exports `accountLinks`, `chesscomVerifications` and `linkMethod`.

### The flow

1. **Start.** `POST /api/accounts/chesscom { username }`.
   - The username must match `^[A-Za-z0-9_-]{3,25}$` and is stored lower-cased.
   - The profile must exist on chess.com. A username that some user has already linked is known to exist, so it needs no request. Any other name costs one throttled profile request.
   - A user can have at most 3 chess.com links.
   - The response is an unverified link (`method: username`) and a code. Asking again for the same name returns the same link and pending code, with no request to chess.com. Once the code has expired or been used, asking again issues a fresh one. A link that is already verified answers `verified: true` with `code: null`.
2. **The user puts the code into their chess.com location.** It can go anywhere in the field, for example "Paris CI-7K3QXM".
3. **Check.** `POST /api/accounts/chesscom/:linkId/verify` reads the profile once.
   - The match is case-insensitive: the code must appear anywhere in the trimmed location.
   - On a match, the link gets `verified_at` and `method: location_code`, and the code is spent (`expires_at = now`). Codes are single-use: a spent code is never shown, accepted or handed back again.
   - The verifying transaction re-reads the link and its code under a lock. If the link was verified meanwhile, or the code was reissued meanwhile, the stale result is not applied.

### The code

The code is `CI-` plus 6 characters from `23456789ABCDEFGHJKMNPQRSTUVWXYZ`, each drawn with `crypto.randomInt(0, 31)`, which is unbiased.

- The alphabet leaves out 0/O and 1/I/L, so the code survives being retyped from a phone.
- It allows about 8.9 × 10⁸ codes. That is far more than enough, because a code is never a secret credential. It only has to be unlikely to be in someone's location already. The `CI-` prefix makes that even less likely, and it tells the user what the code is for.

### Timing rules and outbound throttles

One user must not be able to get the project's User-Agent blocked by chess.com (review ruling: enforced even for the beta). Every profile request made for a user (starts and checks alike, unknown usernames included) needs two things, claimed in one transaction with row locks:

- **The username's slot: at most one request per username per 60 s, across all users.**
  - A check inside the window answers `pending` with `nextCheckAt` and makes no request.
  - A start for an unknown name inside the window answers 429 `rate_limited`.
  - The slot is keyed by username, not by link. This covers the per-(user, account) check cooldown and is stricter than it: deleting and re-adding a link, or a second user on the same account, does not buy an extra request (tested).
  - Concurrent checks therefore make exactly one request (tested).
- **A token from the user's bucket: 10 per hour, refilled continuously (one token every 6 minutes).**
  - An empty bucket answers 429 `rate_limited` with `retryAt` and `Retry-After`.
  - The username slot is checked first, so a blocked request spends no token.
- **A claimed request counts even if chess.com then fails,** which errs on the side of fewer requests.
- Username rows older than an hour are pruned when a request is claimed.
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
- **User-Agent: `chess-improver/0.1 (+https://github.com/chess-improver)`** (ADR 0043). It is the same string the worker sends (its `USER_AGENT`), and a test keeps the two equal. No email address goes into any header.
- **Serial.** One chess.com request is in flight per process at a time.
- **Hardening.**
  - `redirect: "error"`: a redirect is treated as unavailable, never followed.
  - The body is capped at 256 KB.
  - The profile's `username` must equal the requested name (case-insensitive) and match the username pattern.
  - `CHESSCOM_API_BASE_URL` must be https, except in development and tests (the e2e fakes and the test mock server are plain http).
- **Timeouts and errors.**
  - Each request times out after 5 s.
  - A 429 is retried once, after its `Retry-After` (default 1 s, capped at 2 s because the request is interactive).
  - A second 429, a 403, a 5xx, a timeout, a network error or a malformed body raises `ChesscomUnavailableError`, which the routes return as 503 `chesscom_unavailable`.
  - A 404 or 410 means the player does not exist.
- **Tests make no live calls.** They use a fake fetch and a local mock HTTP server serving a synthetic, recorded-shape profile (`apps/web/test/fixtures/chesscom/player-hikaru.json`). No real user's data is committed.

### Flags

- **`CHESSCOM_ENABLED`.** When it is off, both chess.com routes answer 404.
  - `GET /api/accounts` and `DELETE /api/accounts/:linkId` are platform-neutral and stay available, but the list hides chess.com links while the flag is off.
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
| `POST /api/accounts/chesscom` | 201 `{ linkId, username, code, verified }`; 400 `invalid_username`, 404 `not_found`, 409 `limit_reached`, 429 `rate_limited` (`retryAt`), 503 `chesscom_unavailable` |
| `POST /api/accounts/chesscom/:linkId/verify` | 200 `{ status, nextCheckAt }`; 404 for a link that is missing or someone else's; 429 `rate_limited`; 503 `chesscom_unavailable` |
| `DELETE /api/accounts/:linkId` | 204; 404 for a link that is missing or someone else's |

- **Every route needs a signed-in user** who has answered the age screen (401 `unauthenticated` or 403 `age_required`).
- **Origin and CSRF.** These checks sit on top of the session cookie's `SameSite=Lax`.
  - `POST` and `DELETE` need an `Origin` equal to `BETTER_AUTH_URL`'s or one of `BETTER_AUTH_TRUSTED_ORIGINS`. A missing or foreign `Origin` gets 403 `invalid_origin`.
  - They also need `Content-Type: application/json`, otherwise 415 `unsupported_media_type`. A cross-site HTML form cannot send that type without a CORS preflight.
  - `GET` refuses only a foreign `Origin`, because browsers may omit the header on same-origin reads.

### The verified-email rule through the real OAuth callback (T17 hand-off)

`apps/web/test/auth/oauth-callback.test.ts` drives Better Auth's real `/sign-in/social`, `/link-social` and `/callback/:id` endpoints, with only the provider's token, userinfo and discovery requests faked. It covers seven cases:

- **Google, live session, unverified email:** the identity is linked. This is the exemption.
- **Google, session dropped before the callback:** `email_not_verified`. The exemption needs the live session on the callback request itself.
- **Google, implicit link with an unverified email:** `email_not_verified`. With a verified email, the implicit link goes through.
- **chess.com, implicit link** (even when its userinfo claims a verified email): `account_not_linked`.
- **chess.com, new account under `INVITE_ONLY`:** `not_invited`.
- **chess.com, live session:** `unable_to_link_account`.

## Consequences

- Task 21's linking screen calls these four routes, with `Origin` (automatic in browsers) and a JSON body. It shows the code and `nextCheckAt` while a link is pending, and explains three outcomes:
  - `expired`: start again;
  - 429: try again at `retryAt`;
  - 503: chess.com is busy, try again in a minute.
- The first check can come no sooner than 60 s after a start that requested the username. In practice that is the time it takes to edit the location.
- M7 adds the background recheck. Lichess OAuth (Task 18) adds `lichess_oauth` links and the `expiring` status.
