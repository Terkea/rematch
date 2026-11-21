# Technical facts (verified 2026-11-21)

How we checked: the Lichess OpenAPI spec (v2.0.180) at github.com/lichess-org/api, live `curl` probes, live npm registry `latest` tags, the GitHub releases API, official docs and blog posts, and Anthropic's bundled `claude-api` skill (model table cached 2026-10-06). lichess.org/api and the Lichess blog return 403 to automated fetchers, so the Lichess facts come from the spec source.

---

## 1. Lichess

Spec: https://github.com/lichess-org/api/blob/master/doc/specs/lichess-api.yaml (rendered at https://lichess.org/api)

### General etiquette
- **User-Agent is mandatory.** Send your site URL, app name or a contact email. Requests without a suitable User-Agent "may be rejected or rate limited".
- **Make only one request at a time.** If you get HTTP 429, wait at least 1 minute and lower your request frequency. Some limits last longer.
- **NDJSON** responses carry one JSON object per line (`Accept: application/x-ndjson`). Empty keep-alive lines may appear. Helper: https://gist.github.com/ornicar/a097406810939cf7be1df8ea30e94f3e

### OAuth2 (Authorization Code + PKCE)
- Authorize URL: `https://lichess.org/oauth`. Token URL: `POST https://lichess.org/api/token` (form-urlencoded: `grant_type=authorization_code`, `code`, `code_verifier`, `redirect_uri`, `client_id`). Revoke: `DELETE https://lichess.org/api/token`.
- **No app registration is needed.** Lichess "supports unregistered and public clients (no client authentication, choose any unique client id)".
- Only `code_challenge_method=S256` is accepted. Authorize params: `response_type=code`, `client_id`, `redirect_uri`, `code_challenge`, `scope` (space separated), `state`, and an optional `username` hint.
- Access tokens last about 1 year. **Refresh tokens are not supported.** Tokens match `^[A-Za-z0-9_]+$`; budget at least 512 characters.
- Scopes: `preference:read|write`, `email:read`, `engine:read|write`, `challenge:read|write|bulk`, `study:read|write`, `tournament:write`, `racer:write`, `puzzle:read|write`, `team:read|write|lead`, `follow:read|write`, `msg:write`, `note:write`, `board:play`, `bot:play`, `web:mod`.
- Login and identity need **no scope**: request with an empty scope, then call `GET /api/account`. Ask for `email:read` only if you need the email address.
- Personal access tokens: https://lichess.org/account/oauth/token

### Game export: `GET /api/games/user/{username}`
Spec file: https://github.com/lichess-org/api/blob/master/doc/specs/tags/games/api-games-user-username.yaml
- Games come newest first. Output is PGN by default, or NDJSON with `Accept: application/x-ndjson`.
- Throughput limits: anonymous 20 games/s, OAuth 30 games/s, a user downloading their own games 60 games/s.
- Parameters:
  - `since` / `until`: epoch **ms**.
  - `max`: game count.
  - `vs`, `rated`, `perfType` (comma separated), `color`.
  - `analysed`: filter to games that do (or don't) have computer analysis.
  - `moves`, `tags`.
  - `pgnInJson`: puts the full PGN in a `pgn` field. NDJSON only.
  - `clocks`: `[%clk]` comments in PGN, or a `clocks` array in JSON (centiseconds).
  - `evals`: `[%eval]` comments in PGN, or an `analysis` array in JSON, **"when available"**.
  - `accuracy`: per-player accuracy. JSON only, when available.
  - `opening`: opening name and ECO.
  - `division`: middlegame and endgame start plies. JSON only.
  - `literate`: textual annotations.
  - `lastFen`, `ongoing`, `finished`, `withBookmarked`, `sort`.
- **Caveat on evals:** evals exist only for games that already have a server computer analysis, meaning someone requested analysis on lichess.org. Most casual games do not. Use `analysed=true` to find the games that do. Everything else needs our own Stockfish pass.
- **Cloud eval:** `GET /api/cloud-eval?fen=&multiPv=` (no auth) returns cached evals for about 320M positions, mostly openings. It is meant for "a few positions here and there"; for bulk, use the eval dump.

### Opening Explorer (`https://explorer.lichess.org`)
- `/masters`, `/lichess` and `/player` all declare `security: OAuth2` in the spec. Lichess's blog post ["The opening explorer now requires authentication"](https://lichess.org/@/thibault/blog/the-opening-explorer-now-requires-authentication/FSWh9Zg3) says anonymous requests are no longer allowed. The explorer is still free and unlimited for logged-in accounts, and rate limiting moved to the user layer.
- **Live probe on 2026-11-21:**
  - `/lichess` → **401 without a token**
  - `/player` → **401 without a token**
  - `/masters` → still 200 anonymously, but assume it will require auth too.
- Send `Authorization: Bearer <token>`, either the user's OAuth token or our app's personal token. No scope seems to be needed (unverified; test it).
- `/lichess` parameters:
  - `variant`, `fen`, `play` (comma-separated UCI moves).
  - `speeds`.
  - `ratings`: buckets `0,1000,1200,1400,1600,1800,2000,2200,2500`. Each bucket runs from its value to the next.
  - `since` / `until` (YYYY-MM), `moves` (default 12), `topGames`, `recentGames`, `history`.
- `/masters` parameters: `fen`, `play`, `since`, `until`, `moves`, `topGames` (default 15). `/masters/pgn/{gameId}` returns a game's PGN.
- `/player` parameters: `player`, `color`, `variant`, `fen`, `play`, `speeds`, `modes`, `since`, `until`, `moves`, `recentGames`.
  - It streams NDJSON while it indexes on demand.
  - It indexes new games at most once a minute.
  - It returns 429 when the indexing queue is full.

### Tablebase
- `GET https://tablebase.lichess.org/standard?fen=<X-FEN, underscores allowed>&dtc=never|auxiliary|always`.
- No auth; returned 200 in the probe. Atomic and antichess variants are also available.

### Puzzle database
- `https://database.lichess.org/lichess_db_puzzle.csv.zst` holds **6,193,530 puzzles**. License: **CC0** ("Database exports are released under the Creative Commons CC0 license").
- CSV columns: `PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags,DailyDate`.
  - The FEN is the position *before* the opponent's move. The first UCI move in `Moves` is the opponent's; the solution starts at the second move.
  - Popularity runs from -100 to 100.
  - `OpeningTags` is filled only for puzzles that start before move 20.
- Themes are space separated (e.g. `mate mateIn2 fork middlegame`). Definitions: https://github.com/lichess-org/lila/blob/master/translation/source/puzzleTheme.xml and https://lichess.org/training/themes
- Eval dump: `https://database.lichess.org/lichess_db_eval.jsonl.zst` has 417,862,375 positions. Each line holds a FEN plus evals (knodes, depth, PVs with cp or mate and a UCI line).
- Source page: https://database.lichess.org/

### chess-openings dataset
- https://github.com/lichess-org/chess-openings. License **CC0-1.0** (GitHub license API).
- Files `a.tsv` … `e.tsv` have columns `eco, name, pgn`. The built `dist/` adds `uci` and `epd`, generated with `make` and python-chess; CI artifacts are also available.
- A Parquet copy is at https://hf.co/datasets/Lichess/chess-openings
- To classify a game, play its moves backwards until you reach a named position.

---

## 2. chess.com Published-Data API (PubAPI)
Docs: https://www.chess.com/news/view/published-data-api · https://support.chess.com/en/articles/9650547-published-data-api

- Archives: `GET https://api.chess.com/pub/player/{username}/games/archives` returns `{"archives":[url,...]}`, oldest first. The username must be lowercase.
- Monthly games: `GET https://api.chess.com/pub/player/{username}/games/{YYYY}/{MM}`.
  - Fields: `url`, `pgn`, `time_control`, `end_time`, `rated`, `accuracies {white,black}` (only when someone ran Game Review), `fen`, `time_class` (bullet/blitz/rapid/daily), `rules`, `eco` (a URL), `tournament`/`match`, and `white`/`black {username, rating, result, @id, uuid}`.
  - The PGN includes `[%clk h:mm:ss]` comments for live games. The `uuid`, `tcn` and `initial_setup` fields show up in practice but are not fully documented.
- Multi-game PGN: `.../games/{YYYY}/{MM}/pgn` (`application/x-chess-pgn`).
- Profile: `GET https://api.chess.com/pub/player/{username}`.
  - Fields: `@id, url, username, player_id, title?, status, name?, avatar?, location?, country, joined, last_online, followers, is_streamer, twitch_url, fide?`.
  - **`location` and `name` are user-editable free text.** We can ask the user to paste a verification code into `location`, then poll the profile. Responses may be cached for up to 12h, so allow for the delay.
- Rate limits:
  - Serial access is unlimited. Parallel requests may get 429.
  - Abnormal activity can lead to a block.
  - Data refreshes at most every 12h. Use `ETag` / `Last-Modified` with `If-None-Match` / `If-Modified-Since` to get 304s.
- **User-Agent with contact info is REQUIRED** (since 2023-06-20). Requests without one get **403**. Include an email and/or a chess.com username, e.g. `chess-improver/1.0 (contact: ops@example.com)`. https://www.chess.com/news/view/breaking-change-user-agent-contact-info-required
- **OAuth exists, but you have to apply for it.** chess.com runs an OAuth 2.0 / OIDC server at `oauth.chess.com`.
  - It has authorize, token, revoke, JWKS and discovery endpoints. There is no UserInfo endpoint; claims come in the ID token.
  - Access is by application form. It was announced 2024-01-16 for "connected board or Chess.com login".
  - Links: https://www.chess.com/blog/CHESScom/chess-com-oauth-login-connected-board-application · https://chesscom.notion.site/Getting-started-with-Chess-com-OAuth-2-0-Server-5958e57c8c934a3aa7abda2d670969e8 · https://chess.com/clubs/forum/view/guide-applying-for-oauth-access
  - We could not confirm the approval criteria, scopes, or whether it grants access to private data. Until we're approved, use the profile-code check above as the fallback.

---

## 3. Stockfish
- **Latest is Stockfish 19** (tag `sf_19`, released 2026-09-05). https://github.com/official-stockfish/Stockfish/releases · https://stockfishchess.org/download/
  - New SFNNv16 NNUE architecture. The secondary net is retired. The release claims up to +44 Elo over SF18.
  - **Universal binaries** detect the CPU at runtime, so you no longer pick AVX2 or AVX-512 builds.
  - Reworked shared memory on Linux, macOS and BSD, plus 1GB huge page support.
  - **Stricter input validation:** an invalid FEN or UCI command now prints `info string CRITICAL ERROR ...` and **the engine exits**. The worker must validate FENs (e.g. with chess.js) before sending them and must restart the engine if it dies.
  - SF18 came out 2026-01-31.
- Release assets: `stockfish-linux-x86-64-universal.tar.gz`, `stockfish-linux-arm64-universal.tar.gz`, `stockfish-windows-x86-64-universal.zip`, `stockfish-windows-arm64-universal.zip`, `stockfish-macos-universal.tar.gz`, and android/riscv builds.
- License: GPL-3.0. Running it as a separate process and talking UCI over stdio does not make our code GPL. If we ship the binary in our Docker image, we must also offer its source.
- Key UCI options:
  - `Threads` (default 1)
  - `Hash` (MB, default 16)
  - `MultiPV` (default 1)
  - `UCI_ShowWDL`
  - `Move Overhead`
  - `SyzygyPath`
  - `UCI_LimitStrength` / `UCI_Elo`
  - Per position: `position fen ... moves ...` then `go depth N` / `go nodes N` / `go movetime ms`.
- NNUE nets are embedded in the binary; no separate download.
- Docker/Linux:
  - In a multi-stage build, download the `linux-x86-64-universal` release tarball and verify its checksum. The Debian/Ubuntu `apt` package lags behind releases.
  - Alternatively, build from source with `make -j profile-build`.
  - Run one engine process per worker slot with `Threads=1` and a modest `Hash` (64–256MB). For batch throughput this beats one engine with many threads.
- Windows dev: download `stockfish-windows-x86-64-universal.zip` and point `STOCKFISH_PATH` at the `.exe`.
- Node UCI wrappers:
  - `node-uci` (MIT) was last released 2020 and is unmaintained.
  - `chess-uci` 0.5.1 (MIT, 2024) is small.
  - **Recommendation:** write a thin `child_process.spawn` wrapper of about 150 LOC that parses `info depth ... score cp|mate ... multipv ... pv ...` and `bestmove`. The protocol is trivial, and this keeps restart-on-crash under our control.
  - npm `stockfish` 19.0.1 (GPL-3.0) is the WASM build, intended for in-browser analysis.
- Throughput: plan on roughly **1–2 Mnps per thread** on modern x86.
  - A Ryzen 9 9950X does about 61.9 Mnps on 32 threads with SF17, about 1.9 M per thread ([OpenBenchmarking](https://openbenchmarking.org/test/pts/stockfish-1.6.0&search)).
  - Laptop i9 parts run about 0.9 M per thread.
  - Example: 1M nodes per position (about depth 18–22 in middlegames) is about 0.5–1 s of one core. A 40-move game (80 plies) is then about 1–1.5 core-minutes.
  - Benchmark the actual host with `stockfish bench`.

---

## 4. Lichess win% and accuracy
Source: https://lichess.org/page/accuracy · implementation: https://github.com/lichess-org/lila/blob/master/modules/analyse/src/main/AccuracyPercent.scala and `WinPercent.scala`
- **Win%** = `50 + 50 * (2 / (1 + exp(-0.00368208 * cp)) - 1)`, which is the same as `100 / (1 + exp(-0.00368208 * cp))`.
  - Lila clamps cp to ±1000 before applying it.
  - A mate score counts as an extreme cp value.
  - It is always computed from the side to move's perspective, or from White's with a sign flip.
- **Move accuracy%** = `103.1668 * exp(-0.04354 * (winBefore - winAfter)) - 3.1669`, clamped to 0–100. `winBefore` and `winAfter` are from the mover's point of view. (Lila adds +1 "uncertainty bonus" in the code; check against the source.)
- **Game accuracy** in lila, from reading the source (check against the file): the average of
  - a volatility-weighted mean of move accuracies (weights are the stdev of win% over sliding windows of size `clamp(n/10, 2, 8)`, clamped to [0.5, 12]), and
  - the harmonic mean of move accuracies.
- Lichess says the equation may change in future.

---

## 5. Claude API (from Anthropic's `claude-api` skill, model table cached 2026-10-06; live pricing: https://platform.claude.com/docs/en/about-claude/pricing)

| Model | ID | Context | Input $/MTok | Output $/MTok |
|---|---|---|---|---|
| Claude Opus 5.5 (current Opus, default) | `claude-opus-5-5` | 1M | 4.00 | 20.00 |
| Claude Sonnet 5.5 (current Sonnet) | `claude-sonnet-5-5` | 1M | 2.00 | 10.00 |
| Claude Haiku 5.5 (current Haiku) | `claude-haiku-5-5` | 1M | 0.10 (≤100K prompt; 0.50 above) | 0.50 (2.50 above) |
| Claude Fable 5.1 (top tier) | `claude-fable-5-1` | 1M | 10.00 | 50.00 |
| Previous: Opus 5 / Sonnet 5 / Haiku 4.5 | `claude-opus-5`, `claude-sonnet-5`, `claude-haiku-4-5` | | 5/25, 2/10, 1/5 | |

- Use the exact IDs with no date suffixes. Check live capabilities with `GET /v1/models`.
- **Structured outputs are GA.**
  - `output_config: { format: { type: "json_schema", schema } }` on `messages.create`. The old `output_format` is deprecated.
  - The TS SDK has `client.messages.parse()` with Zod helpers.
  - Strict tools: `strict: true` on the tool definition (needs `additionalProperties:false`).
  - **Forced `tool_choice` (`any`/`tool`) returns 400 on Opus 5.5 / Sonnet 5.5.** Use structured outputs instead.
  - Assistant prefill is removed on current models.
- **Thinking:** on Opus 5.5, `thinking` can't be disabled. Control depth with `output_config.effort` (`low`…`max`; Opus 5.5 defaults to `medium`). On Sonnet 5.5, `{type:"between_tools"}` turns thinking off. `temperature`/`top_p`/`top_k` are rejected on current models.
- **Prompt caching:**
  - `cache_control: {type:"ephemeral"}` (5 min) or `ttl: "1h"`.
  - Writes cost about 1.25× base input (5 min) or 2× (1h). Reads cost about 0.1× base (Opus 5.5 / Sonnet 5.5 reads are $0.20/MTok).
  - Up to 4 breakpoints. The minimum cacheable prefix is 512–4096 tokens depending on the model.
  - Prefix order is `tools → system → messages`. Check that it works with `usage.cache_read_input_tokens`.
- **Message Batches API:** `POST /v1/messages/batches`, **50% discount**, asynchronous (finishes within 24h). Results arrive in any order, so match on `custom_id`. It works with caching. Fits nightly game-review jobs.
- SDK: `@anthropic-ai/sdk` 1.12.0 (Apache-2.0). TS timeouts are in ms. Stream when `max_tokens` is large.
- Opus 5.5, Opus 5 and Sonnet 5.5 can stop with a `refusal` reason. The skill suggests adding server-side `fallbacks: "default"` (beta `server-side-fallback-2026-07-01`) and checking `stop_reason`.

---

## 6. Package versions and licenses (npm `latest` dist-tag, 2026-11-21)

| Package | Version | License | Notes |
|---|---|---|---|
| next | 16.4.6 | MIT | canary 16.5 |
| react / react-dom | 19.3.1 | MIT | |
| tailwindcss | 4.3.5 | MIT | v4 CSS-first config |
| motion (formerly framer-motion) | 14.14.0 | MIT | import from `motion/react`; `framer-motion` publishes the same version |
| drizzle-orm | 0.45.4 | Apache-2.0 | 1.0.0-rc.6 on the `rc` tag; drizzle-kit 0.31.11 (MIT) |
| bullmq | 6.7.8 | MIT | ioredis 6.0.0 |
| chess.js | 1.4.0 | **BSD-2-Clause** | |
| react-chessboard | 5.12.1 | **MIT** (confirmed) | |
| chessground / @lichess-org/chessground | 9.2.1 / 10.5.0 | **GPL-3.0-or-later** (confirmed) | the new package name is `@lichess-org/chessground` |
| chessops | 0.15.1 | GPL-3.0-or-later | avoid; use chess.js |
| cm-chessboard | 8.15.8 | MIT | MIT alternative board |
| better-auth | 1.7.27 | MIT | **recommended** |
| next-auth / @auth/core | 4.24.15 (v5 still `5.0.0-beta.32`) / 0.41.3 | ISC | |
| stripe (Node SDK) | 23.2.0 | MIT | |
| turbo | 2.11.13 | MIT | |
| pnpm | 12.21.0 | MIT | |
| ts-fsrs | 6.0.0 | MIT | |
| @playwright/test | 1.65.1 | Apache-2.0 | |
| vitest | 5.1.1 | MIT | |
| zod | 4.6.7 | MIT | |
| stockfish (npm, WASM) | 19.0.1 | GPL-3.0 | browser only |

**Auth recommendation: use Better Auth.**
- Auth.js (NextAuth) has been maintained by the Better Auth team since 2025-09-22 ([announcement](https://better-auth.com/blog/authjs-joins-better-auth)). That announcement recommends Better Auth for new projects; Auth.js gets security fixes only.
- NextAuth v5 is still in beta.
- Better Auth has a Drizzle adapter and a generic OAuth2 plugin. The plugin can handle Lichess PKCE, which needs no client secret, and later chess.com OIDC.
- Better Auth may have been acquired by Vercel ([blog](https://better-auth.com/blog/better-auth-joins-vercel)). We only saw this in secondary sources.

---

## Assumptions checked against findings
| Assumption | Verdict |
|---|---|
| Lichess evals are reusable via export | **Partly true.** `evals=true` returns `[%eval]` / `analysis` only for games that already have server analysis (`analysed=true`). Most games have none, so we still need our own Stockfish pass. `/api/cloud-eval` and the 418M-position eval dump can fill in openings and common positions. |
| chess.com has no OAuth | **False.** chess.com runs an OAuth2/OIDC server, but access needs an application and approval. The PubAPI itself has no auth, and the profile `location` code check still works as a fallback. |
| chessground is GPL | **True** (GPL-3.0-or-later; it moved to `@lichess-org/chessground`). |
| react-chessboard is MIT | **True** (v5.12.1). |
| (extra) The Lichess opening explorer is anonymous | **False.** `/lichess` and `/player` return 401 without a Bearer token. `/masters` is still anonymous for now. |
