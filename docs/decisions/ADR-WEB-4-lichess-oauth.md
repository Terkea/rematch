# ADR-WEB-4. Lichess OAuth (PKCE) and Lichess linking

Date: 2026-11-22. Status: accepted. Implements M4 Task 18. Builds on ADR 0040 (chess.com first, Lichess after), ADR 0041 (private beta), ADR 0043 (User-Agent), ADR-WEB-3 (auth data model, admission policy) and ADR-WEB-5 (account links, newest verification wins).

## Context

Lichess offers OAuth 2.0 with PKCE for public clients. It needs no app registration and no client secret: any `client_id` works. A Lichess OAuth exchange proves that the person controls that Lichess account. That is stronger than chess.com's location code, so a Lichess link is verified the moment the exchange succeeds.

Better Auth needs an email for every user. Lichess shows the account's email only with the `email:read` scope, and its API does not say whether that address is verified.

## Decision

### Provider (`apps/web/src/server/auth/lichess.ts`)

- **`lichessProvider(env)`** is a generic OAuth provider:
  - `providerId: "lichess"`, `clientId: LICHESS_CLIENT_ID` (default `chess-improver`);
  - no client secret: the token request carries `client_id` and `code_verifier` only, and the tests assert that no `client_secret` is sent;
  - `pkce: true` (S256). Better Auth also sends and checks a `state`.
- **Scope.** The only scope is `email:read`, which is the least Better Auth needs. We ask for no game or preference scopes; imports use the public API.
- **Two base URLs.**
  - `LICHESS_AUTHORIZE_BASE_URL` serves `/oauth`, which the browser visits.
  - `LICHESS_API_BASE_URL` serves `/api/token`, `/api/account` and `/api/account/email`, which the server calls.
  - Both default to `https://lichess.org`. In e2e the browser (in the qa container) and the server (in the web container) reach the same fake under different names. The e2e override sets both to `http://fakes:3199`; host-side runs can use the published `localhost:53199`.
- **`getUserInfo`.** It fetches `/api/account` and `/api/account/email`, then returns `{ id, name: username, email, emailVerified: false, image: undefined }` (the brief's `image: null` does not type-check against Better Auth 1.7.24; `undefined` stores null).
  - The project User-Agent (ADR 0043) is set on these calls and, through `authorizationHeaders`, on the token request. It contains no email.
  - Each call has a 5 s timeout, refuses redirects and caps the body at 64 KB.
  - Any failure returns null, so Better Auth answers `unable_to_get_user_info`.
  - Neither the token nor the response body is logged.
  - The id must equal the lower-cased username.
- **`mapProfileToUser` forces `emailVerified: false`** (T19 review), whatever the profile says.

### A Lichess email is never trusted

- **No implicit linking.** The admission policy (`policy.ts`) now refuses `link-account` for Lichess and chess.com unless the request's live session belongs to the same user. The error is `account_not_linked`. So a Lichess sign-in whose email matches an existing account never joins that account. It goes to `/sign-in?error=account_not_linked`, whose copy says to sign in by email and connect Lichess from Accounts. This closes the T17 final-review gap: the `NON_EMAIL_PROVIDERS` early return used to let an implicit link through.
- **No new accounts under `INVITE_ONLY`.** A Lichess email cannot prove an invite, so with `INVITE_ONLY` on (the beta default) Lichess cannot create accounts (`not_invited`). Invited players sign in by email, connect Lichess, and from then on can use "Continue with Lichess". With `INVITE_ONLY=0`, Lichess sign-up creates a user with `emailVerified: false`.
- **Better Auth configuration.**
  - `trustedProviders` is now `["google", "lichess"]`. Better Auth refuses an explicit link (`linkSocial`) from an untrusted provider whose email is unverified, and a Lichess email is always unverified. Trusting Lichess is safe only because the admission policy above blocks every implicit Lichess link.
  - `allowDifferentEmails: true`. A player's Lichess email need not match their sign-in address. Better Auth applies this only to explicit links, and those are proven by the live session plus the OAuth exchange. It applies to Google links from a live session too, which is equally safe.

### Ownership (`apps/web/src/server/accounts/lichess-link.ts`, `db-hooks.ts`)

- **When it runs.** `databaseHooks.account.create.after` and `update.after`, for `providerId === "lichess"`, call `onLichessAccount(db, { userId, lichessId, username })`. Both run after Better Auth's transaction commits.
  - `create` covers sign-up and a first link.
  - `update` covers a returning sign-in and a reconnect, so a link removed from Accounts comes back once the player proves ownership again.
- **What it writes.** In one transaction it upserts `linked_accounts` (`platform: "lichess"`, `username` lower-cased, `display_name` as Lichess shows it) and the user's `account_links` row with `method: "lichess_oauth"` and `verified_at: now`.
- **Newest owner wins** (as in ADR-WEB-5). Any other user's verified link to that account loses its `verified_at`, under the same per-account advisory lock that chess.com verification uses (`OWNER_LOCK_SPACE`). Better Auth's unique `(provider_id, account_id)` already stops two users from holding the same Lichess OAuth account. The clear covers a chess.com-style squatter row and any future link method.
- **Display name.** The account hook sees only the account row, so `getUserInfo` remembers the username's capitalisation by Lichess id in a small bounded in-process map. The hook falls back to the id, which is the lower-cased username.

### Tokens

- **Storage.** Access tokens are stored encrypted (`encryptOAuthTokens: true`, ADR-WEB-3); the test checks that the stored value is not the token.
- **Exposure.** The account list reads only `access_token_expires_at`. Tokens never reach the client, a URL, a cookie or a log line.
- **Expiry.** Lichess issues one-year tokens (`expires_in`) and no refresh token. `lichessLinkStatus({ accessTokenExpiresAt }, now)` returns:
  - `ok`;
  - `expiring` within 14 days of expiry;
  - `expired` from expiry on.
- **In the account list.** `listAccounts` uses this status for Lichess links. A Lichess link with no OAuth account, or one displaced by a newer owner, shows as `expired`. Both `expiring` and `expired` offer "Reconnect Lichess", which runs `linkSocial` again and refreshes the token through `update.after`.

### Errors

`mapAuthResponse` maps errors on `/callback/lichess`:

- `access_denied` becomes `lichess_denied`, keeping the target page. The sign-in flow lands on `/sign-in?error=lichess_denied` and the Accounts flow on `/app/accounts?error=lichess_denied`. Better Auth's default error page is replaced with `/sign-in`.
- `account_not_linked` always goes to `/sign-in?error=account_not_linked`.

### Enabled

`lichessProvider(env)` always returns the provider, because Lichess needs no registration. "Continue with Lichess" and "Connect Lichess" therefore show by default, which ends the ADR 0040 deferral for Lichess.

## Consequences

- **Tests.** `apps/web/test/auth/lichess.test.ts` drives the real Better Auth endpoints against an in-process fake Lichess on a random port (`test/fakes/main.ts`, `startFakes`), and any other outbound request throws. The fake checks the S256 verifier against the stored challenge. The compose `fakes` service serves the same routes on 3199.
- **e2e.** `accounts.spec.ts` is enabled. It signs a player in by email, connects Lichess through the fake, then signs in with "Continue with Lichess". No real Lichess app is needed.
- **Removing a link.** Removing a Lichess link from Accounts deletes the `account_links` row only. The Better Auth `account` row (the encrypted token) stays, so "Continue with Lichess" still signs that player in and restores the link. Also revoking the Lichess token and deleting the `account` row is a follow-up for account deletion (R2).
- **Multiple processes.** The display-name map is per process. If a hook ran on another process, the name would fall back to the lower-case id, which is cosmetic only.
