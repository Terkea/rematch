# ADR-WEB-4. Lichess OAuth (PKCE) and Lichess linking

Date: 2026-11-22. Status: accepted, revised after the T18 security review (fix round 1). Implements M4 Task 18. Builds on ADR 0040 (chess.com first, Lichess after), ADR 0041 (private beta), ADR 0043 (User-Agent), ADR-WEB-3 (auth data model, admission policy) and ADR-WEB-5 (account links).

## Context

Lichess offers OAuth 2.0 with PKCE for public clients. It needs no app registration and no client secret: any `client_id` works. A Lichess OAuth exchange proves that the person controls that Lichess account. That is stronger than chess.com's location code, so a Lichess link is verified the moment the exchange succeeds.

Better Auth needs an email for every user. Lichess shows the account's email only with the `email:read` scope, and its API does not say whether that address is verified.

## Decision

### Provider (`apps/web/src/server/auth/lichess.ts`)

- **`lichessProvider(env)`** is a generic OAuth provider:
  - `providerId: "lichess"`, `clientId: LICHESS_CLIENT_ID` (default `chess-improver`);
  - no client secret: the token request carries `client_id` and `code_verifier` only, and the tests assert that no `client_secret` is sent;
  - `pkce: true` (S256). Better Auth also sends a `state`, and binds it to the browser with a signed state cookie (tests: a callback without the cookie is refused, a replay is refused, and a link started by an attacker attaches nothing when the victim's browser completes it).
- **No scope.** Since fix round 1 we ask for no scope at all, so there is no `email:read`: the app never uses a Lichess email (below), so it does not ask for one. This deviates from the brief, which asked for `email:read`. We ask for no game or preference scopes either; imports use the public API.
- **Two base URLs.**
  - `LICHESS_AUTHORIZE_BASE_URL` serves `/oauth`, which the browser visits.
  - `LICHESS_API_BASE_URL` serves `/api/token` and `/api/account`, which the server calls.
  - Both default to `https://lichess.org` and must be https outside development and tests (`assertLichessBaseUrl`), because bearer tokens travel to the API base.
  - In e2e, the browser (in the qa container) and the server (in the web container) reach the same fake. The e2e override sets both to `http://fakes:3199`.
- **`getUserInfo`.** It fetches `/api/account` and returns `{ id, name: username, email: lichessPlaceholderEmail(id), emailVerified: false, image: undefined }`. The brief's `image: null` does not type-check against Better Auth 1.7.24; `undefined` stores null.
  - The project User-Agent (ADR 0043) is set on this call and, through `authorizationHeaders`, on the token request. It contains no email.
  - The call has a 5 s timeout and refuses redirects. It caps the body at 64 KB using the declared length and the bytes counted while streaming.
  - Any failure returns null, so Better Auth answers `unable_to_get_user_info`.
  - Neither the token nor the response body is logged.
  - The id must equal the lower-cased username.
- **`mapProfileToUser`** forces the placeholder email and `emailVerified: false`, whatever the profile says.

### No email from Lichess (review I5)

- **The risk.** If a Lichess sign-up stored the address Lichess reports, then with `INVITE_ONLY=0` a Lichess holder could pre-create an account under someone else's address. The real owner's later magic link would sign them into that account, which the Lichess holder can still enter.
- **Placeholder address.** A user created by a Lichess sign-up therefore gets `<lichess id>@lichess.chess-improver.invalid`. That is non-routable (RFC 2606 `.invalid`), and `deliverMagicLink` never sends to it. No email from Lichess is stored or even requested.
- **Magic links.** A magic link for any real address lands in that address's own account, never in a Lichess-created one (tested with `INVITE_ONLY=0`).
- **Attaching an email.** An email can join a Lichess-created account only after its owner proves it with a magic link. No such attach flow exists yet; when one is built, it must use Better Auth's verified change-email.
- **No implicit linking.** The admission policy (`policy.ts`) refuses `link-account` for Lichess and chess.com unless the request's live session belongs to the same user, with `account_not_linked`. This closes the T17 final-review gap. The placeholder email also means a Lichess sign-in can never match an existing account by email.
- **No new accounts under `INVITE_ONLY`.** A Lichess identity cannot prove an invite, so with `INVITE_ONLY` on (the beta default) Lichess cannot create accounts (`not_invited`). Invited players sign in by email, connect Lichess, and from then on can use "Continue with Lichess".
- **Better Auth configuration.**
  - `trustedProviders` is `["google", "lichess"]`. Better Auth refuses an explicit link (`linkSocial`) from an untrusted provider whose email is unverified. Trusting Lichess is safe only because the admission policy blocks every implicit Lichess link.
  - `allowDifferentEmails: true`. The placeholder never matches the sign-in address. Better Auth applies this only to explicit links, which are proven by the live session (`isLinkSessionLive`), the state cookie and the OAuth exchange. It applies to Google links from a live session too, which is equally safe.

### Ownership (`apps/web/src/server/accounts/lichess-link.ts`, `db-hooks.ts`)

- **When it runs.** `databaseHooks.account.create.after` and `update.after`, for `providerId === "lichess"`, call `onLichessAccount(db, { userId, lichessId, username })`. Both run after Better Auth's transaction commits.
  - `create` covers sign-up and a first link.
  - `update` covers a returning sign-in and connecting again while still linked; it keeps the link verified.
  - If recording fails, the error is logged without the row (which holds the encrypted token) and the next sign-in or connect records the link again.
- **What it writes.** It upserts `linked_accounts` (`platform: "lichess"`, `username` lower-cased, `display_name` as Lichess shows it) and the user's `account_links` row, with `method: "lichess_oauth"`.
- **The first holder keeps it (review I3).** This replaces "newest owner wins".
  - Better Auth's unique `(provider_id, account_id)` refuses a second user's OAuth link with `account_already_linked_to_different_user`. The app shows that as its own code, `lichess_taken`: "This Lichess account is connected to another user." The first holder keeps the account and the verified link (tested through the real callback).
  - `onLichessAccount` never clears another user's verified link. If one exists anyway, the new link is recorded unverified. The check runs under the per-account advisory lock shared with chess.com (`OWNER_LOCK_SPACE`).
- **Display name.** The account hook sees only the account row, so `getUserInfo` remembers the username's capitalisation by Lichess id in a small bounded in-process map. The hook falls back to the id, which is the lower-cased username.

### Tokens

- **Storage.** Access tokens are stored encrypted (`encryptOAuthTokens: true`, ADR-WEB-3); the test checks that the stored value is not the token.
- **Endpoints that return tokens are not mounted (review I1).** `disabledPaths` lists:
  - `/get-access-token` (which returns the decrypted token);
  - `/refresh-token`;
  - `/account-info` (the provider profile);
  - `/unlink-account` (unlinking goes through `DELETE /api/accounts/:linkId`).

  All four answer 404 with no token in the body (tested). The account list reads only `access_token_expires_at`. Tokens never reach the client, a URL, a cookie or a log line.
- **Expiry.** Lichess issues one-year tokens (`expires_in`) and no refresh token. `lichessLinkStatus({ accessTokenExpiresAt }, now)` returns:
  - `ok`;
  - `expiring` within 14 days of expiry;
  - `expired` from expiry on.
- **In the account list.** `listAccounts` uses this status for Lichess links. A Lichess link with no OAuth account or no `verified_at` shows as `expired`. Both `expiring` and `expired` offer "Reconnect Lichess", which runs `linkSocial` again.

### Removing a Lichess link (review I2)

- **What is deleted.** `DELETE /api/accounts/:linkId` (`removeLink`) deletes, in one transaction, the `account_links` row and the user's Lichess `account` row with its stored token.
- **Revocation.** After commit, the token is revoked at Lichess (`DELETE /api/token`) as a detached, best-effort task. It is decrypted only for that call and never logged.
- **No silent restore.** Nothing restores the link. A later "Continue with Lichess" is a fresh sign-in: `not_invited` under `INVITE_ONLY`, or a separate new account in open mode. A later "Connect Lichess" is a fresh, explicit connect. Both are tested.
- **The last way in.** A Lichess-created account, with the placeholder email and no other provider, cannot remove its only way to sign in. The route answers 409 `last_sign_in_method`, with the copy "You sign in with this Lichess account, so it can't be removed."

### Errors

`mapAuthResponse` maps errors on `/callback/lichess`. Better Auth appends its `error`, so the last one counts. Each failure keeps the page the flow returns to (`/sign-in`, or `/app/accounts`, which no longer has a preset error); Better Auth's raw error page is replaced by `/sign-in`.

- `access_denied` becomes `lichess_denied`.
- `account_already_linked_to_different_user` becomes `lichess_taken`.
- `account_not_linked` always goes to `/sign-in?error=account_not_linked`.
- `not_invited` and `email_not_verified` keep the general mapping.
- Anything else becomes `lichess_failed`: a missing, stale or replayed state, a failed exchange, `unable_to_get_user_info` or a stale session.

### Enabled

`lichessProvider(env)` always returns the provider, because Lichess needs no registration. "Continue with Lichess" and "Connect Lichess" therefore show by default, which ends the ADR 0040 deferral for Lichess.

## Consequences

- **Tests.** `apps/web/test/auth/lichess.test.ts` drives the real Better Auth endpoints against an in-process fake Lichess on a random port (`test/fakes/main.ts`, `startFakes`), and any other outbound request throws. The fake checks the S256 verifier against the stored challenge and records revoked tokens. The compose `fakes` service serves the same routes on 3199.
- **e2e.** `accounts.spec.ts` signs a player in by email, connects Lichess through the fake, then signs in with "Continue with Lichess". No real Lichess app is needed.
- **Multiple processes.** The display-name map is per process. If a hook ran on another process, the name would fall back to the lower-case id, which is cosmetic only.
