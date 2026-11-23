# ADR-FIX-N-1. An unverified link sees analysis only; Lichess is opt-in

Date: 2026-11-23. Status: accepted (holistic review F10, the owner's ruling "analysis only"; F8).
Builds on ADR-WEB-3 (auth and the link data model), ADR-WEB-5 (chess.com verification),
ADR 0064 §18 (report rows are account data) and ADR 0070 (the coach chat).

## Context

- **F10.** A new user who linked someone else's public chess.com username, without verifying it,
  saw the account's full shared profile at once: every analysed game, the AI coaching report,
  the area notes and the shared coach allowance ("97 of 100 left", the owner's usage). The games
  are public, but the AI report and its usage are personal.
- **F8.** Sign-in showed "Continue with Lichess" and Accounts "Connect Lichess", although this
  deployment is chess.com only.

## Decision

1. **Analysis only until verified.** Anyone may link a public username and see the computed
   engine analysis of its games (overview, games, weaknesses, areas, strengths, progress). The
   account's AI artefacts are read only through the requesting user's own verified link
   (`account_links.verified_at`, `isVerifiedLink`):
   - the report (`getReport` returns `locked: "unverified"` and no payload);
   - the weakness change notes (`changeNotes`) and the weekly reviews (`listWeeklyReviews`),
     which return nothing;
   - the coach allowance (`coachAllowance` returns null), which is the account's usage.
2. **No coach and no report build until verified.** Creating a conversation and posting a
   question answer 403 `not_verified` (`createConversation`, `createInTx`,
   `startConversation`, `postInTx`), and so does `POST /api/coach/report`
   (`requestReport`), so an unverified link never spends the account's AI budget. The user's own
   past conversations stay readable (they are the user's, ADR 0070).
3. **Server-side.** The gates sit in the read models and the routes, never only in the UI. The
   pages then show a calm `computed_only` state with one action, "Verify this account"
   (`/app/accounts`): "Verify this account to see your coach's report", "… your coach's note",
   "Verify to ask your coach" (`lib/verify-copy.ts`).
4. **Lichess is opt-in (F8).** `LICHESS_ENABLED` (a flag, default off) turns on Lichess sign-in
   and linking. Off, `lichessProvider` is null, so Better Auth registers no Lichess provider and
   neither button renders. The e2e overlay turns it on, because its Lichess specs run against the
   fake.

## Consequences

- One verified owner per account (`account_links_one_verified_owner`), so an account's report and
  coach have at most one reader. Verifying a link that another user verified first is refused as
  before (ADR-WEB-5).
- The Rust chat job keeps accepting any link (`AnyLink`): the web is the only way a question is
  enqueued, and it refuses an unverified one.
- Test seeds now link verified by default (`seedAccount`); a test of an unverified link says so.
