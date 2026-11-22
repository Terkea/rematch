# ADR-WEB-16. App shell, sign-in, age and account-linking screens

Date: 2026-11-22
Status: Accepted (Task 21, review fix round 1)

## Context

Task 21 builds the first screens a person sees: sign-in, the age question, the under-13 block and the accounts page. It also builds the signed-in shell that the dashboard, games list and game view (M5a, the owner's next goal) will live in. Lichess accounts are deferred (ADR 0040), and Task 18 (Lichess OAuth and the provider fakes) has not been built.

## Decision

- **Routes.** `(auth)` holds `/sign-in`, `/sign-in/check-email`, `/welcome/age` and `/blocked`. `(app)` holds `/app` (Dashboard), `/app/games` (Games) and `/app/accounts`. `/` redirects to `/app/accounts` when signed in and to `/sign-in` otherwise. Dashboard and Games are quiet placeholder pages, so their nav entries and routes are already final and M5a only replaces their `page.tsx`.
- **The address a link went to never travels in a URL** (review I1). A successful `POST /sign-in/magic-link` sets `ci_pending_email` (HttpOnly, SameSite=Lax, `Max-Age=600`, `Path=/sign-in/check-email`). `src/proxy.ts` hands it to that page once as a request header (dropping any copy the browser sent) and clears the cookie. The page reads nothing from the URL, and without the cookie it says "your email". The same cookie is set for invited and uninvited addresses.
- **Shell.** `(app)/layout.tsx` calls `requireUser()` and renders the sticky header (components in the private folder `(app)/_shell/`, which can never become a route): the wordmark, the nav (`_shell/nav.ts`: Dashboard, Games, Accounts), an empty `data-slot="activity"` for Task 27, the command-menu trigger (Ctrl K), the theme toggle and the user menu with "Sign out". Every page calls `requireUser()` again, so none relies on the layout alone.
- **Motion** (DESIGN.md):
  - The ink rule under the current section slides to the next one on the `page` spring (it shows where you went, M1).
  - `(app)/template.tsx` runs the incoming half of the page turn (choreography 5) on client navigations only. The first paint is the server render, never a hidden page waiting for JavaScript. The outgoing half needs the View Transitions work and is left for later.
  - The signed-out pages show a real board at every width (beside the form from `lg`, under it on phones). Sign-in and check-email show the Opera game after 9…b5?, with the pen ring and `?` badge drawing on load. The age step shows the same game a move earlier with no marks, because the pen is reserved for a mistake. /blocked shows the starting position.
  - A newly verified account's badge lands on `land`.
  - Every async button names its work ("Sending link", "Checking") and soaks an ink line along its foot (`PendingButton`). No spinners (B18).
- **Lichess stays hidden** until it is configured. "Continue with Lichess" and "Connect Lichess" render only when `lichessProvider(env)` returns a provider, which the Task 18 stub never does. The Lichess e2e step is `fixme` unless `E2E_LICHESS=1`. Once Task 18 lands, the UI needs no changes.
- **Check again** waits for chess.com's per-username minute. After a start that returned a code, the button stays unavailable for 60 s, with a visible countdown. After a pending check, it waits until the server's `nextCheckAt` (Task 19 hand-off).
- **The e2e fakes.** `test/fakes/{chesscom.ts,main.ts}` serve only the chess.com half of Task 18's fake contract: `/chesscom/pub/player/:u`, `POST /chesscom/__set-location`, plus `POST /chesscom/__reset`. Task 18 adds the Lichess routes to `main.ts`.
- **Providers.** `providers.tsx` now mounts the TanStack `QueryClientProvider` and the one `Toaster`, which this task's mutations and toasts need. Task 27 adds the realtime provider next to them.
- **E2E state.** The e2e web service uses its own database, `chess_e2e_test`, never the dev `chess` database that holds analysed games. A Playwright `globalSetup` creates, migrates and empties it, empties Mailpit and resets the fakes, so a fresh compose project works. The `qa` service shares web's network namespace (`docker-compose.e2e.yml`). The production-build e2e and env-driven limits are handed to T24 and T28.
- **The e2e invite list** also holds `adult@example.invalid` and `kid@example.invalid`. Each sign-in in a run uses its own address, which keeps each run within the per-email throttle (3 per 10 minutes, ADR-WEB-3). The shared in-memory IP limit of 3 per 10 minutes still applies. To run the suite again within 10 minutes, restart `web`. `next dev` compiles each route on its first request, so the suite runs one worker, and Playwright's expect timeout is 15 s.

## Consequences

- M5a can add its pages inside the shell without touching navigation.
- The accounts list uses the inline key `["accounts"]` (`accountsKey`) until Task 23's `queryKeys.accounts()` exists. Task 27 swaps it in.
- An under-13 browser keeps seeing the blocked notice on `/sign-in` for as long as the 30-day cookie lasts (ADR-WEB-6).
