# ADR-WEB-6. Age gate, under-13 block and private defaults

Date: 2026-11-21. Status: accepted. Implements M4 Task 20. Builds on ADR-WEB-3 (auth data model) and spec §14 (age gate, cookies). The screens themselves (`/welcome/age`, `/blocked`) are Task 21.

## Context

chess-improver is not for children under 13, and teenagers get the most private settings. The first target is an invite-only private beta (ADR 0041), but the gate has to be right from the first account, because it decides what personal data we hold about a child.

## Decision

### A neutral screen after sign-in

- The age question comes **after** authentication, on its own screen ("When were you born?", month and year). `requireUser` sends every account without a `birthMonth` there, and `requireApiUser` answers 403 `age_required` until it is answered. So no feature can be used first.
- The screen **does not announce the threshold** in advance. The hint only says why we ask ("so we can keep younger players' accounts private by default"). A child who is told "you must be 13" is invited to lie, and a neutral question gets more honest answers.
- The answer is **given once**. `POST /api/age` answers 409 `age_already_recorded` once an age is stored, and `recordAge` checks again under a row lock. The gate can neither change a recorded age (a teen cannot become an adult) nor be replayed to delete someone else's account.

### Month and year only

- We store `birth_month` as `YYYY-MM` (CHECK on the format, ADR-WEB-3) and the time of the answer, `age_checked_at`. Never a day of birth, and nothing at all for an under-13. This is data minimisation: the month and year are enough for every decision the product makes.
- `parseBirthMonth` accepts integers only, months 1–12, years from 1900, and nothing after the current month.

### Conservative arithmetic

- `ageFromBirthMonth` assumes the birthday is the **last day** of the birth month, so the age is never overstated. Someone born in November 2013 is 12 until 30 November 2026. A February birthday in a leap year counts from the 29th, which in other years means 1 March. All dates are UTC.
- Bands: under 13 `under13`, 13–17 `teen`, 18 and over `adult`.
- An account with no recorded age counts as a minor (`isMinor` is true), so `canEnableMarketing` is false until an adult age is recorded.

### Privacy defaults

- `marketing_email` and `public_share_default` default to false for everyone: both are opt-in.
- A teen's answer **forces both to false**, whatever they were. `canEnableMarketing` is false for minors, so no later settings screen may turn marketing on for them.
- An adult's answer leaves both at their opt-in default.

### Deleting a blocked user's data at once

- For an under-13, `recordAge` deletes the `user` row in the same transaction as the check. Sessions, OAuth accounts, `account_links` and chess.com request budgets cascade (ADR-WEB-3, ADR-WEB-5).
- It also deletes every other copy of the email address: the `invites` row and the `magic_link_sends` throttle log. This also means an invite given to a child does not stay usable from another browser. An address in the `INVITE_ALLOWLIST` environment variable cannot be removed by the app; the owner removes it.
- The shared `linked_accounts` rows stay. They describe public platform accounts that other users may link too, and they hold nothing the child gave us.
- Nothing about the child is kept, not even the fact of the block, apart from the cookie in their own browser.

### The 30-day block cookie

- `POST /api/age` sets `ci_age_block=1; Max-Age=2592000; Path=/; HttpOnly; SameSite=Lax` (plus `Secure` whenever the session cookies are Secure, that is over https) and clears the session cookies (`ci.session_token`, `ci.session_data`, `ci.dont_remember`, or their `__Secure-` forms).
- A Better Auth `before` hook refuses `/sign-in/*`, `/sign-up/*`, `/magic-link/*`, `/oauth2/*` and `/callback/*` with 403 `AGE_BLOCKED` while the cookie is present. It runs before the endpoint, so no magic link is mailed. `/callback/*` (Google's callback) is added to the brief's list as defence in depth; the social sign-in that would lead there is already refused at `/sign-in/social`.
- Why: without it, the back button or another provider would let the same child answer again with a different year straight away. The cookie is essential for that purpose. It holds no identifier, only `1`.
- It is a speed bump, not proof of age. Clearing cookies defeats it, as any self-declared age gate can be defeated.

### No cookie banner

- The app sets only essential cookies: the session, the OAuth state and the age block. None is used for analytics or advertising, so no consent banner is needed (spec §14).

### Route conventions

- `POST /api/age` uses `rejectCrossSite` like the Task 19 routes (403 `invalid_origin`, 415 `unsupported_media_type`), before reading the session.
- It is the one route a signed-in user may call before the age is known, so it reads the session itself (401 `unauthenticated`, the same body as `requireApiUser`) instead of calling `requireApiUser`, which would answer 403 `age_required`.

## Deferred

- The DPIA and the review of nudges and dark patterns stay in M7.
- `/api/auth/magic-link/verify` answers a blocked browser with the 403 JSON body. Task 21 should map it to `/blocked`, as `mapAuthResponse` does for `not_invited`.

## Consequences

- An under-13 who answers honestly leaves no account data behind, and cannot sign straight back in from the same browser.
- A recorded age cannot be corrected by the user. A mistaken answer needs the owner (support) to fix it in the database, which is acceptable for the beta.
