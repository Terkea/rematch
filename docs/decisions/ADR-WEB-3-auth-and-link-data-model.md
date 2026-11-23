# ADR-WEB-3. Auth data model, Better Auth configuration and invite-only sign-up

Date: 2026-11-21. Status: accepted. Implements M4 Task 17, which also took over Task 2's auth schema (controller ruling). Builds on ADR 0005 (Better Auth), ADR 0033 (SQL migrations own the schema) and ADR 0041 (private beta; `docs/decisions/0041-private-beta-owner-and-friends.md`, on `r1/base`).

## Context

The web app needs users, sessions, OAuth accounts and magic-link tokens. The plan put these in a Drizzle-generated migration (Task 2). Since then, ADR 0033 made `db/migrations/` (plain SQL, applied by sqlx) the only source of the schema. The web app's Drizzle schema is now generated from it. Task 2 was never built, so Task 17 adds the auth tables itself.

## Decision

### Tables

- **One SQL migration, `db/migrations/20261121010000_web_auth.sql`.** The web range starts after R1's reserved `20261121000000`–`20261121009999`. It creates the Better Auth 1.7 core tables `user`, `session`, `account` and `verification`, plus `invites` and `magic_link_sends` (the per-address throttle). Column names are the snake_case form of Better Auth's field names. The generated Drizzle schema maps them back to the camelCase keys the drizzle adapter writes.
- **uuid ids everywhere** (`gen_random_uuid()` defaults, `advanced.database.generateId: "uuid"`), because `linked_accounts.user_id` is already a uuid.
- **`session` and `account` cascade on user delete.** The under-13 deletion in Task 20 relies on this.
- **`user` carries the age and privacy columns.** These are `birth_month` (`YYYY-MM`, with a CHECK on the format), `age_checked_at`, and `marketing_email` and `public_share_default`, both default false. They are Better Auth `additionalFields` with `input: false`, so no client can set them through the auth API.
- **`user.email` is lower-case** (CHECK), like Better Auth writes it.
- **`account (provider_id, account_id)` is unique** and has an index on `user_id`. `verification.identifier` is indexed.
- **Out of this migration:** the `account_links` and `chesscom_verifications` tables from Task 2's plan. They belong to Task 19, which adds them as its own migration.
- **The schema is regenerated with `scripts/db/generate-drizzle-schema.sh`**, so the drift check stays clean. `@ci/db` re-exports `user`, `session`, `account`, `verification` and `invites` from the generated schema. Task 27 later moves the whole main export over to it.

### Migrations for the web test database

`runMigrations(db, dir?)` (in `@ci/db`) applies `db/migrations/*.sql` in version order, all in one transaction, under an advisory lock.

- It uses sqlx's own bookkeeping: the same `_sqlx_migrations` DDL, a SHA-384 checksum of the file, and the description with `_` turned into spaces. A database migrated by it is therefore accepted by `db::MIGRATOR`, and the other way round.
- A file that changed after it was applied is an error, as in sqlx.
- **Known differences from sqlx** (acceptable for a test-database tool):
  - all pending migrations run in one transaction, where sqlx uses one per migration;
  - there is no support for `-- no-transaction` migrations or for reversible (down) migrations;
  - it takes its own advisory lock, not sqlx's, so a concurrent sqlx run against the same database is not excluded;
  - `execution_time` is measured around the statement only.
- The web tests use it on `chess_web_test` (`WEB_DATABASE_URL_TEST`, guarded by `assertSafeTestDatabase`). Dev and production databases are still migrated by the backend.

### Better Auth (1.7.24, the repo's existing pin)

The `auth` instance is built lazily by `getAuth()`, so importing the module during `next build` needs no environment. It is configured as follows:

| Setting | Value |
|---|---|
| Adapter | drizzle, provider `pg` |
| `advanced.cookiePrefix` | `ci`: the cookie is `ci.session_token` over http and `__Secure-ci.session_token` over https |
| `advanced.useSecureCookies` | follows `BETTER_AUTH_URL`; in production an http base URL is refused unless the host is local |
| `advanced.disableOriginCheck`, `disableCSRFCheck` | explicitly `false`: Better Auth skips both under `NODE_ENV=test`, so the tests would otherwise not exercise them |
| `account.encryptOAuthTokens` | `true` (the option exists in 1.7, so no hand-rolled AES) |
| `account.accountLinking` | `{ enabled: true, trustedProviders: ["google"] }` |
| Plugins | `magicLink({ expiresIn: 600, storeToken: "hashed" })`, `genericOAuth` (Lichess and chess.com OAuth stubs until Tasks 18 and 19) |
| Google | configured only when both `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` are set |

- **Rate limiting is on in every environment**, with the custom rule `/sign-in/magic-link` at 3 per 600 s per client IP.
  - **The client IP comes only from trusted proxies.** Next route handlers do not expose the socket address. So `advanced.ipAddress` reads `IP_ADDRESS_HEADERS` (default `x-forwarded-for`) only when `TRUSTED_PROXIES` (CIDRs) is set, walking the header from the right past the trusted hops. With no proxy configured, no header is read and all clients share one bucket per path (127.0.0.1 in dev and test), so a forged `X-Forwarded-For` never buys a fresh bucket.
  - **A per-email throttle sits on top:** at most 3 links *sent* per address per 600 s, whatever the IP.
    - Sends are recorded in `magic_link_sends`. Counting, pruning old rows and recording happen in one transaction under a per-address advisory lock (`pg_advisory_xact_lock(7262013, hashtext(email))`).
    - So a burst of N concurrent requests sends exactly min(N, 3) links.
    - A throttled request records nothing, so it never extends the lockout.
    - A recorded send whose SMTP delivery then fails still counts.
  - **The header shape** behind a proxy is `<whatever the client sent>, <address the proxy saw>`. Better Auth walks it from the right, skipping `TRUSTED_PROXIES`, so forged left-hand entries are ignored.
  - **Blocker on friend access:** the app may be reachable only through a trusted reverse proxy or tunnel that sets `X-Forwarded-For`.
    - `TRUSTED_PROXIES` is the compose network CIDR.
    - web's port is not published (the opt-in `docker-compose.ports.yml` mapping is for local use only).
    - Without that, a client that can reach web directly can write its own `X-Forwarded-For`.
    - Until then, local-only use with no proxy is accepted: the shared bucket (review ruling).
  - It uses the in-memory store, which is enough for the single-process private beta.
  - A multi-instance deployment needs Redis `secondaryStorage` (deferred with the rest of launch hardening).
- **`/api/auth/*` error redirects are mapped** to the sign-in messages that Task 21 renders.
  - A used, expired or unknown magic link goes to `/sign-in?error=link_expired`.
  - An uninvited address goes to `/sign-in?error=not_invited`, from any sign-in method.
  - The mapping lives in `mapAuthResponse`, not in `hooks.ts`, which belongs to Task 20.
- **The schema check replaces the CLI.** `@better-auth/cli` stops at 1.4.x, so `pnpm dlx @better-auth/cli@1.7.27 generate` cannot run (and 1.7.27 is also under 7 days old). Instead, `test/auth/schema.test.ts` compares Better Auth's own `getSchema(auth.options)` with the generated tables: every field must be present and required fields must be NOT NULL. The same check runs on every test run rather than once.

## Addendum: invite-only sign-up for the private beta (ADR 0041)

- **`INVITE_ONLY` is on by default.** It is off only when set to exactly `"0"`, the opposite of the other flags, so a missing setting fails closed.
  - With it on, an address may sign in or sign up only when it appears in `INVITE_ALLOWLIST` (comma-separated, case-insensitive) or in the `invites` table (emails stored lower-cased; CHECK enforced).
  - With it off, sign-up is open again with no other change.
- **Magic link.** `sendMagicLink` only queues the delivery (`runAfterResponse`: at most 100 queued or running, extras dropped with a logged warning and a counter, at most 4 running at once; under test, `flushBackgroundTasks()` rethrows task errors). The invite check, the per-email throttle and the SMTP send all run after the response. The endpoint answers `200 { status: true }` on the same code path for every address, so neither the body nor the timing reveals invites or accounts. Mail goes to the normalised address. The person who isn't invited simply gets no mail.
- **Addresses are normalised** with NFKC, trimmed and lower-cased. Anything that is not printable ASCII afterwards never matches an invite, so look-alike characters cannot.
- **New accounts from any method** (magic link, Google, generic OAuth) are gated by `user.validateUserInfo` on `create-user`. A rejection redirects with `error=not_invited`.
- **OAuth needs a provider-verified email.** For email-asserting providers (Google), `validateUserInfo` rejects anything but `emailVerified === true` (`error=email_not_verified`). This applies to `create-user` and to implicit `link-account`, where a sign-in is linked to the account with the same email.
  - Why: Google is a trusted linking provider, and that skips Better Auth's own verified-email check. Without this rule, a Google account that merely claims an invited address could link itself to that account.
  - **Exempt:** `link-account` started from a live session of the same user (`linkOAuthAccount`), where the session, not the email, grants trust.
  - **Exempt:** providers that do not assert email (`lichess`, `chesscom`; `NON_EMAIL_PROVIDERS`). Their linking is not gated on email. Under `INVITE_ONLY` their email cannot prove an invite, so they cannot create accounts. Task 18 revisits this for Lichess sign-in.
- **Returning users** are gated by a `session.create.before` guard (composed around `db-hooks.ts`, which stays Task 18's file). A user whose invite was withdrawn gets no new session.
  - Linking a provider to an already signed-in user is not invite-gated, because the user is already admitted (it still needs a verified email).
- **Withdrawing an invite signs the person out.** Deleting an `invites` row, or changing its email, deletes the sessions of the old address (trigger `invites_revoke_sessions`). The guard then refuses new ones.
  - TRUNCATE is not covered: row triggers do not fire on it.
  - `INVITE_ONLY` and `INVITE_ALLOWLIST` are read at start-up, so changing them needs a restart.
  - Removing an address from `INVITE_ALLOWLIST` does not delete its sessions; delete them by hand.
- **The message is neutral.** `not_invited` says only that this address cannot sign in yet, never whether an account exists. Suggested copy for Task 21: "This email can't sign in yet. chess-improver is invite-only for now."
- **Inviting someone, for now:**
  - add the address to `INVITE_ALLOWLIST` in `.env`, or
  - run `insert into invites (email, note) values ('friend@example.com', 'beta')`.
- **Invite codes are not built.** They need a sign-in field (Task 21) and only matter for public growth.

### Left out (public launch, deferred by ADR 0041)

Public sign-up flows (codes, waitlist, invite emails), a shared rate-limit store, a durable mail queue (today a send lost to a crash is not retried), and production domain hardening.

## Consequences

- Turning `INVITE_ONLY` off restores open sign-up with no code change, and both modes are tested.
- Adding a web table means writing a new SQL migration with a version after `20261121010000`, then rerunning the generator.
- `createTestSession(db, overrides?)` (in `apps/web/test/helpers/session.ts`) inserts a user and a session directly and returns a signed `ci.session_token` cookie.
  - It defaults to an adult who has answered the age screen. Pass `{ birthMonth: null }` for a fresh account.
  - `webTestDb()` points the app's `DATABASE_URL` at `chess_web_test`, so routes under test use it.
