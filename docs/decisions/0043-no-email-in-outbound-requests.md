# 0043. No email address in outbound requests

Date: 2026-11-21. Status: accepted.

## Context

The Rust platform client built its User-Agent as `chess-improver/0.1 (+mailto:{CONTACT_EMAIL})`, so the owner's email went to chess.com and Lichess on every request.

## Decision

- No outbound request to a third party (chess.com, Lichess, or any other) may carry an email address, in any header, query string or body. The reason is the owner's privacy: the project owner's standing rule is never to send their email to external services.
- Clients identify themselves by the project URL only. The fixed User-Agent is `chess-improver/0.1 (+https://github.com/chess-improver)`.
- `ClientOptions.contact_email` is removed; `user_agent()` takes no arguments. `CONTACT_EMAIL` is no longer read by the Rust backend or passed to `rust-dev`.
- The same rule applies to the R2 lichess-db download and to the web app's chess.com calls.
- Regression test: `backend/crates/platforms/tests/client.rs` asserts the chess.com and Lichess clients send (with `CONTACT_EMAIL` set when run) exactly the fixed User-Agent, with no `@`.

## Consequences

- chess.com may rate-limit or block an anonymous agent more readily; if so, revisit with a project-owned address, not a personal one.
