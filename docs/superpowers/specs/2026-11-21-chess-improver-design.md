# Chess Improver: Design

Date: 2026-11-21
Status: Approved by delegation. The owner put Claude in charge ("you are in charge of everything"). Owner actions are listed in §18.

## 1. Intent

A web platform. A chess player links their Lichess or chess.com account. We import their game history, analyse every game with Stockfish, work out how they play, and give them a full, honest coaching programme: what they already do well, what is costing them games, which openings to keep, fix or learn, and drills to practise, all pitched at their current rating and aimed at the next rating band.

The owner wants a "robust proper platform" that works for players at every level from beginner to master, and that has a premium, animated UI that does not look AI-generated.

### Why it can win

The market has an incumbent, Aimchess (owned by chess.com, about $10/month), and many hobby "AI coach" projects. The most common complaint about AI chess coaching is that language models invent moves and evaluations. Our core rule answers that complaint:

> **Stockfish and our own statistics produce every fact. Claude only explains and prioritises those facts. Every claim in a report links to real positions from the player's games.**

## 2. Goals

1. Link accounts: Lichess through OAuth, chess.com by username (chess.com has no third-party login).
2. Import a player's full game history and keep it in sync.
3. Analyse every game with Stockfish in a background pipeline that scales and survives failures.
4. Build a **playstyle profile** and a **weakness model**, with each weakness ranked by how many games or rating points it actually costs.
5. Benchmark the player against players one rating band above them, in the same time control.
6. Generate a rating-aware **coaching report** with Claude: strengths, weaknesses, an opening review and recommendations, and a climb plan.
7. Provide **training**: puzzles from the player's own mistakes, an opening repertoire trainer, endgame drills, "retry your mistake", and clock-pressure drills.
8. Track **progress** over time and re-analyse automatically as new games arrive.
9. **Stay free**: Rematch is free and open source, with no paid plans.
10. Deliver a distinctive, premium, animated UI that works on mobile and desktop.

All of goals 1–10 ship together as one release (owner's decision: "build everything at once"). The plan may still order the work internally.

## 3. Non-goals

- Playing chess against other people, or hosting games.
- A native mobile app. The web app is responsive, and a PWA is fine.
- Over-the-board PGN upload as the main flow. Pasting a PGN is a nice extra, not a must-have.
- Live in-game assistance of any kind. That is cheating, and both platforms ban it.

## 4. Anti-goals: it fails if any of these happen

- **The AI states chess facts it wasn't given.** A move, evaluation, or "you often do X" that isn't backed by computed data counts as a failure.
- **Generic advice.** "Study tactics" with no link to the player's own games is worthless.
- **It looks AI-generated or templated.** See §11 for what to avoid.
- **The first experience is a dead wait.** The player must see live progress, with first insights within a couple of minutes, not a blank "come back later".
- **Runaway cost.** Engine CPU and AI tokens per user must be bounded, so no user can cost more than the limits allow.
- **Getting banned by chess.com or Lichess** for API abuse.

## 5. Users and rating-aware coaching

Players range from about 400 to 2500+ across bullet, blitz, rapid, classical and daily games. The thing holding a player back changes as they climb, so weakness ranking and report tone both adapt to the player's band.

| Band | Typical bottleneck | Coaching emphasis |
|---|---|---|
| <1000 | Hanging pieces, missing one-move threats, king safety | Simple habits (a blunder check every move), basic tactics, opening principles |
| 1000–1600 | Missed 2–3 move tactics, clock management, endgame basics, openings played by rote | Tactical patterns, time use, key endgames, understanding their openings |
| 1600–2000 | Converting advantages, middlegame plans, repertoire gaps | Conversion, planning, a real repertoire |
| 2000+ | Small accuracy leaks, preparation depth, technical endgames | Deep opening prep, precision, technique |

Bands use each platform's own rating scale. Lichess and chess.com ratings are not comparable, and we never mix them.

## 6. System architecture

> **Superseded in part by ADR 0030 (2026-11-21): the whole backend is Rust.** The cargo workspace `backend/` holds the crates chess-core, db (sqlx), queue (Postgres SKIP LOCKED), engine, platforms, analysis, events (LISTEN/NOTIFY), profile, ai (an Anthropic HTTP client), plus the `worker` and `cli` binaries. Only `apps/web` (Next.js) and `packages/ui` stay TypeScript. Redis and BullMQ are removed. The schema source of truth is SQL migrations in `db/migrations/`, and the web app's Drizzle schema is generated from them with a CI drift check. The TypeScript backend code is the reference spec until Rust parity is reached. Where the text below names TypeScript, BullMQ, Redis or Drizzle for the backend, ADR 0030 wins.

```
            ┌──────────────┐   Lichess / chess.com public APIs
 Browser ──▶│  apps/web    │◀──────────────┐
            │  Next.js     │               │
            └──────┬───────┘        ┌──────┴────────┐
                   │ Postgres       │ apps/worker   │  Stockfish (native, UCI)
                   │ Redis (BullMQ) │ Node + TS     │  Claude API
                   └───────────────▶└───────────────┘
```

Monorepo using pnpm workspaces and Turborepo, all TypeScript:

- `apps/web`: Next.js (App Router), React, Tailwind CSS, Motion (formerly Framer Motion) for animation, and Radix primitives for accessible components, restyled so they look nothing like stock shadcn.
- `apps/worker`: long-running Node workers on BullMQ. They handle import, engine analysis, feature extraction, report generation and puzzle generation. Stockfish runs as a native binary pool, one process per core.
- `packages/db`: Postgres schema and migrations with Drizzle ORM.
- Auth: **Better Auth**, with email magic link, Google, and a custom Lichess OAuth provider (PKCE S256, no app registration needed; tokens last about a year with no refresh token, so we prompt to re-link when one expires).
- Versions verified 2026-11-21 (`docs/research/technical-facts.md`): Next 16, React 19.3, Tailwind 4.3, Motion 14, BullMQ 6, chess.js 1.4 (BSD-2), react-chessboard 5 (MIT), ts-fsrs 6, Turborepo 2.11, pnpm 12, Playwright 1.65, Vitest 5. Avoid chessground and chessops, which are GPL.
- `packages/chess-core`: pure functions built on chess.js for move classification, phase detection, motif tagging, feature extraction and benchmarks. These get heavy unit tests.
- `packages/ai`: Claude prompts, structured-output schemas and the claim validator.
- `packages/ui`: the design system (tokens, components, the board and the eval graph).

Local development uses Docker Compose (Postgres, Redis, worker). Production runs the web app on Vercel and the workers plus Redis and Postgres on a container host with dedicated CPU (Fly.io or Hetzner). **The builder picks the provider** and records the choice in an ADR.

## 7. Data ingestion

- **Lichess:** OAuth 2 with PKCE. Games are streamed as NDJSON from `/api/games/user/{username}`, with clocks, opening data and existing server evals (`evals=true`). Games that Lichess has already analysed reuse those evals, which saves CPU.
- **chess.com:** the published-data API. Read the monthly archive list, then each month's games, with requests in series and a back-off on 429 responses. The User-Agent must include contact info, or the API returns 403. Linking only needs a username. chess.com *does* run an OAuth2/OIDC server (oauth.chess.com), but third parties must apply for access. **Action for the owner:** apply for chess.com OAuth access. Until we're approved, a paid user verifies ownership by putting a short code in their profile's location field. Profile data can be cached for up to 12 hours, so the UI says verification can take a while. The code supports OAuth too, behind a feature flag.
- **Lichess Opening Explorer** `/lichess` and `/player` endpoints need a Bearer token, either the user's OAuth token or our app's own token. `/masters` works without one.
- Each game is stored once, keyed by platform and game ID, with its PGN, clock times (`%clk`), ratings, time control, result and opening.
- **Incremental sync:** a scheduled job every few hours pulls new games for active users, and those games flow through the pipeline automatically.
- All outbound calls go through a per-platform rate limiter with an identifying `User-Agent` that includes a contact email, as both APIs request.

## 8. Analysis pipeline

1. **Engine pass.** Evaluate every position at a fixed node budget (roughly 1M nodes, to be tuned). Use MultiPV 2 only on critical positions, where the eval swings or there is one only move. A **position cache keyed by FEN** is shared across all users, so common opening positions are only analysed once. We pre-seed it from the CC0 Lichess evaluation dump (`lichess_db_eval.jsonl.zst`, about 418M positions), filtered to positions that appear in real games at the depth we need. Lichess server evals (`evals=true`) are reused when a game has them, which is a minority of games. The engine is **Stockfish 19**, called through our own thin UCI wrapper, because the existing Node wrappers are stale. Every FEN and move is validated before it's sent, because Stockfish 19 exits on invalid input. The supervisor restarts it if it does.
2. **Move classification.** Use a win-probability model (the Lichess-style conversion from centipawns to win%). Each move gets win% lost, accuracy, and a label: best, good, inaccuracy, mistake, blunder or missed win. Thresholds are on win%, not raw centipawns, so they behave sensibly in won or lost positions.
3. **Context tags per move:** game phase (opening, middlegame or endgame, by material and move number), time left and time spent, whether the opponent's last move was a mistake (a missed punish), and the tactical motif of a missed or allowed tactic (hanging piece, fork, pin or skewer, discovered attack, back-rank mate, mate threat, promotion), found by heuristics on the engine's principal variation.
4. **Opening mapping.** Map each game to an ECO/opening name using the open Lichess `chess-openings` dataset, and find the move where the player left known theory, or left the move that Lichess Opening Explorer statistics favour at their rating band.
5. **Aggregation into a player profile**, per time control:
   - Accuracy by phase, blunder rate, and how blunder rate changes with clock time.
   - Conversion rate (wins from clearly winning positions) and resilience (draws or wins saved from clearly losing positions).
   - Opening results tree: score, frequency and average accuracy per line, as White and Black.
   - Endgame types reached and how each went (rook, minor piece, pawn, queen).
   - Motif profile: which tactics they miss and which they fall for.
   - **Playstyle axes**, each computed from measurable signals rather than invented: aggression (sacrifices, early pawn storms, opposite-side castling, attacking moves), solidity (inaccuracy rate, trades when ahead), tactical sharpness, time discipline, opening breadth, and endgame skill.
6. **Benchmarks.** Every game also gives us analysed moves for the *opponent* at a known rating. Pooling those across all users gives band-by-band population statistics ("blunders per game at 1400 blitz on chess.com") without needing any outside dataset. Before we have enough data, seed the benchmarks with a one-off analysis of a sample from the Lichess open database.
7. **Weakness ranking.** For each weakness, estimate its cost: games lost or drawn where that weakness caused the decisive swing, converted into estimated rating points. Rank by cost, weighted by how far the player is from the next band's benchmark.

**Budgets:** the smallest limit is the last 50 games in each time control. The largest is the full history, analysed newest first. Results stream in: the profile updates as each batch finishes, and the UI shows live progress.

## 8c. Handling large game histories (how many games we fetch and analyse)

Owner's requirement: some players have thousands of games, so we must decide how many to poll and analyse.

The costs are very different:
- **Importing** a game (downloading its PGN with clocks, opening and ratings) is almost free. Lichess streams about 20 games/s without a token and 60/s with one. chess.com serves a whole month in one request.
- **Engine analysis** costs about 15–20 core-seconds per game (around 80 positions at roughly 0.25 s each, minus cache hits).

So we import broadly and analyse selectively.

**Tier 1: import (no engine).**
- Up to **10,000 most recent games per account** (the default plan sets 1,000; the limit is configuration).
- Stored as compressed PGN plus metadata.
- This alone powers several features across the player's whole history with no engine cost:
  - the full rating curve;
  - opening repertoire and results;
  - time-control mix;
  - clock-usage patterns;
  - session and tilt statistics;
  - game counts.
- Runs in the background at low priority, newest first, streamed and resumable (it checkpoints the oldest month or `until` cursor reached).

**Tier 2: engine analysis, by priority.** Each user gets a priority-ordered analysis queue:

| Priority | Which games | Small limit | Large limit |
|---|---|---|---|
| P0 Onboarding | Most recent games in the primary time control | 50 | 200 |
| P1 Other active time controls | Most recent per other time control with ≥ 20 games in the last 90 days | — | 100 each |
| P2 History sample | **Stratified sample across the history**: up to 15 games per calendar month per time control, going back up to 3 years. This feeds the weakness timeline (§12b) and the 90-day median baseline | — | backfilled at low priority |
| P3 New games from sync | Every new rated game in an active time control | up to 20 per week | all, with a fair-use cap of 300 per week |
| P4 On demand | A game the user opens in the explorer | free in the browser (WASM) | free in the browser (WASM) |

**Adaptive stopping.** P1 and P2 stop early for a time control once every weakness posterior there is precise enough. That means the 80% credible-interval width is ≤ 25% of the band median, or the §6.6 minimum evidence is met. Extra games beyond that change nothing, so we don't pay for them.

**Selection rules:**
- Rated games come before casual ones.
- Games under 10 plies and aborted games are skipped.
- Bullet is analysed only if it's the player's primary time control.
- Games that already have Lichess server evals are analysed first, since they cost no CPU.
- The analysis protocol is the same in every tier, so sampled and recent games stay comparable.

**Cost envelope.** A heavy user's onboarding (P0 + P1) takes about 200–400 games, roughly 1–2 core-hours. On 16 cores that's about 5–8 minutes of wall time, and results stream in as they finish. P2 backfill adds about 500 games over the following hours at low priority. Per-user CPU-seconds are metered, and hard caps from §13 apply.

**Shown to the user.** Each profile and KPI states its basis ("based on 214 analysed rapid games from 3,912 imported") along with how far the background backfill has got, so a sampled history is never mistaken for a complete one.

## 8d. Capacity and scaling (measured 2026-11-21)

**Measurements:**
- Stockfish 19 benchmark in the worker image: about 1.97M nodes/s per thread.
- At 300k nodes, that's about 0.15 s per position.
- The live smoke run averaged about 70 positions per game, so with overhead one game takes about **12 core-seconds**.

**One user:**

| Games | 24-core dev box | 16-vCPU production worker |
|---|---|---|
| 100 | under 1 min | about 1.5 min |
| 1,000 | about 9 min | about 13 min |

**Sign-up waves set the requirement.** 100 users × 50 games is 5,000 games, about 17 core-hours. One 16-vCPU worker needs about 70 minutes to drain that. The money cost is negligible (about €0.01 per vCPU-hour, so about €0.25 for the whole wave). The binding constraint is **time to first insight**, so the design targets latency:

1. **Fair scheduling.** The analysis scheduler (§8c) serves users round-robin in slices of 5 games, ordered by priority tier, then by when the user joined. Nobody waits behind another user's whole history. Every new user gets their first 10 games before anyone's 11th.
2. **Preview pass.** A user's first 20 P0 games are first analysed at **30k nodes** (about 10× faster, giving a provisional "early read" within about 1–2 minutes even during a wave). Those preview evals are labelled `preview`. They are never written to the shared cache, and never used for KPIs, posteriors or the weakness timeline. The full protocol pass replaces them in the background. This is the only exception to "one protocol everywhere", and it's visibly marked as provisional in the UI.
3. **Cache multiplication.** The shared FEN cache, seeded from the Lichess eval dump, gets more effective with every user: openings and common endgames repeat. Expect about 15–30% fewer engine calls at scale, plus free Lichess server evals for Lichess users.
4. **Horizontal autoscaling.** Workers are stateless consumers of the queue. An autoscaler watches queue lag (the oldest P0 job's age, and core-seconds queued). When P0 lag exceeds 3 minutes, it adds dedicated-vCPU cloud servers with hourly billing, for example 48-vCPU instances. It scales back down after 15 minutes idle, with a monthly spend cap in config. The baseline is one always-on 16-vCPU worker, burst capacity is about 4 extra servers, and the caps are approved by the owner before deploying (§18).
5. **Back-pressure UI.** The live analysis screen shows the user's place and an ETA, for example "about 3 min, analysing 38 / 50", computed from queue depth and measured throughput. It's never a spinner.

**Burst capacity target.** With 1 baseline + 2 burst 48-vCPU servers (about 110 engines), a 100-user wave reaches every user's preview within about 2 minutes, and every user's full 50 within about 10 minutes.

Claude cost per report is estimated at $0.05–0.15 (Sonnet, cached prefix, five sections). Monthly regenerations go through the Batch API at a 50% discount.

## 8. (cont.) Improvement engine: normative spec

The section **"Proposed improvement engine spec" in `docs/research/improvement-engine.md` is normative** for weakness detection, prioritisation, prescriptions, re-measurement and progress wording. Where it differs from the sketches in §8 and §12a, the spec wins. The key points:

- **Classification** uses the exact Lichess win%, accuracy and blunder/mistake/inaccuracy formulas (documented in the research). We do not attempt to copy chess.com's formula.
- **Analysis protocol:** a pinned Stockfish version and node count, plus 7-piece tablebases. If the protocol changes, games are re-analysed so metrics stay comparable over time.
- **Difficulty adjustment:** position difficulty predicts errors better than skill does, so weakness metrics are adjusted for difficulty. The target model is **Maia-2**, run as a separate Python service (`services/maia`), after checking its licence. Until that's in, the fallback difficulty proxy uses MultiPV eval spread and the number of playable moves.
- **Twelve weaknesses**, each with a detection rule and a per-opportunity metric (for example, hung pieces per eligible move rather than per game), with estimates pulled toward the player's band average when there's little data.
- **One common unit:** *expected points lost per game compared with peers*. Priority = excess × how trainable the weakness is at this band × confidence.
- **Re-measurement:** minimum evidence before any verdict, a 10-week maximum per plan, and an adherence gate (no verdict if the drills weren't done). The decision table has seven outcomes: resolved, improving, inconclusive, transfer failure, not working, regression, park. Weaknesses already fixed are watched for drift.
- **The baseline rating is the 90-day median**, not the rating at sign-up. People often sign up after a slump, so they'd appear to improve on their own.
- **Rating is noisy.** One performance rating has an error of about ±347/√n Elo, so the UI never claims a rating change is caused by the plan. Progress is shown in three tiers, each with its certainty stated: practice, your games, then rating.
- **Claude's wording rules** come from the spec: no promises of rating gains, and every claim is evidence-backed.
- **Cohort-level proof:** improvement is validated across all users with randomised experiments (feature-flagged prescription variants), because it can't be proven for one player. These results are the only basis for any marketing claim.
- **Tactic tagging** is our own implementation, reimplemented from the published description of the Lichess puzzle tagger. We don't copy `cook.py`, which is AGPL.
- **What we can honestly promise:** a rigorous, evidence-based process with honest measurement, not a guaranteed rating gain.

## 8a. Opening intelligence

The owner wants us to monitor "what openings they play, what openings they struggle with, everything".

**Repertoire tree.** We build one tree per account × time control × colour, keyed by **position (FEN), not move order**, so transpositions merge into one line. Each node records:
- frequency, and win/draw/loss and score %;
- performance vs expected (Elo-adjusted);
- average engine eval when the player leaves the opening (around move 12–15), which measures **what the opening gets them**, separately from how the game ended;
- average accuracy and time spent in the opening phase;
- the **deviation point**: where they leave known theory or the move most popular at their rating, plus how often that deviation is an engine mistake;
- the most common opponent replies, and which ones hurt them.

**Both sides of the board:** what they choose (as White, and as Black against 1.e4, 1.d4 and others), and what they **face**. "You score 31% against the Caro-Kann" is as important as "your Italian scores 58%".

**Struggle detection.** An opening line is flagged as a *struggle* when it has enough games (n ≥ 8, configurable) and at least one of these holds:
- its score is well below the player's overall expected score;
- they come out of the opening at a disadvantage on average;
- they repeatedly go wrong at the same deviation ply.

Struggles are ranked by cost (games lost × frequency), just like other weaknesses, and feed the improvement engine.

**Opening verdicts.** Each line gets one of:
- **Keep**: it scores well and suits them.
- **Fix**: they lose at one specific spot. We show that move and drill it.
- **Learn deeper**: they leave theory early in a line they play often.
- **Replace**: it consistently performs badly or clashes with their style. We suggest alternatives that fit their playstyle axes and rating band, using Lichess Opening Explorer stats for their rating range.

Claude writes the explanation, but the verdict itself comes from the computed stats.

**Over time.** Opening KPIs (score per main line, deviation ply, eval after the opening, and which openings are played most) are snapshotted like every other KPI (§12a). That lets the player see "your Caro-Kann score went from 31% to 47% since you started the trainer", and also notice repertoire drift, such as when they stop playing what they drilled.

**UI.** An interactive repertoire explorer: a board plus a tree with a heat colour per node using the scoresheet's red and green. Click a node to see its stats and the games that reached it, then jump straight into the opening trainer for that line.

## 8b. Skill areas: the backbone of the report, training and KPIs

Everything is organised into seven **skill areas**. Each area gets its own report section, its own page with a board, charts and example games, its own KPIs tracked over time (§12a), its own weaknesses feeding the improvement engine, and its own drills. Each area also gets an **area score** from 0 to 100: the player's KPIs placed as percentiles against the benchmark for their rating bucket, so they can see at a glance where they stand compared with players around them and with the +200 target.

| Area | What we detect | KPIs (each per account × time control, with sample size and confidence band) | Drills |
|---|---|---|---|
| **Openings** (§8a) | Repertoire, lines faced, deviation points, struggle lines | Score and performance vs expected per line; eval after the opening; opening accuracy; deviation ply; time spent in the opening; share of games where they come out of the opening worse | Opening trainer (FSRS), "fix this move" drills, new-line learning |
| **Middlegame** | Planning and positional errors: drifting in equal positions, bad trades, pawn-structure damage, king-safety lapses, failing to use an advantage | Middlegame accuracy; win% lost per move in quiet (non-tactical) positions; eval trend from move 15 to 35; king-safety incidents; conversion % from +2 or better; resilience % from −2 or worse | "Find the plan" positions from their games, conversion drills (beat the engine from a won position), defend-a-worse-position drills |
| **Endgame** | Endgame types reached and how each went; technique errors flagged by the tablebase | Endgame accuracy; conversion % of winning endgames and hold % of drawn ones, per type (pawn, rook, minor piece, queen, mixed); tablebase-verified errors per endgame | Theory positions (Lucena, Philidor, opposition, Vancura and more), their own endgames replayed against the engine or tablebase |
| **Tactics** | Missed wins and allowed tactics, tagged by motif | Missed tactics per game; tactics allowed per game; miss rate per motif (fork, pin, skewer, discovered attack, back rank, mating net, deflection, hanging piece); missed-punish rate; puzzle rating | Own-mistake puzzles, themed puzzles matched to weak motifs, "retry your mistake" |
| **Calculation and blunder checking** | Blunders that a one-move safety check would catch, versus deep miscalculations | Blunders per game; one-move blunders (hung piece, missed mate in 1) per game; error rate in sharp positions (several only-moves) | Blunder-check drills, longer calculation puzzles |
| **Time management** | Clock use by phase; rushing or overthinking | Average time per move by phase; time-trouble rate (share of games below 10% of the clock); blunder rate by clock bucket; losses on time; time used in critical vs easy moves | Clock-pressure puzzle sets, pacing targets shown in the report |
| **Mindset and consistency** | Tilt and form swings | Score after a loss vs after a win; accuracy decline across long sessions; resignation in positions that could be saved; first-game vs later-games accuracy | Coaching advice rather than drills, such as session-length and stop-after-two-losses recommendations, tracked as KPIs |

**Rules**
- Every area's detectors live in `packages/chess-core` as pure functions over the per-move records (§8.3), with fixture-based tests.
- An area with too little data says so ("12 endgames so far: need 20 for a reliable read") instead of guessing.
- The report shows all seven areas. The **climb plan** draws on the top-ranked weaknesses across all areas, at most 3 active at a time, so a player is never handed 20 things at once.
- The weakness taxonomy and the prescription rules for each area are detailed in the improvement engine spec (research in `docs/research/improvement-engine.md`).

## 9. AI coaching (Claude API)

- Runs in the worker through the Anthropic API, not Claude Code. Reports use `claude-sonnet-5-5` ($2 input / $10 output per million tokens). The short per-weakness and per-position explanations use `claude-haiku-5-5`. Scheduled monthly regenerations go through the Batch API, at a 50% discount. JSON comes from **structured outputs** (`output_config.format`), not forced tool calls, which Sonnet 5.5 rejects.
- **Input:** the computed profile, the ranked weaknesses, benchmarks, opening tree, and a handful of selected example positions (FEN, played move, best move, evals). **Output:** structured JSON through a schema (strengths, weaknesses, opening verdicts, climb plan, drill assignments), and each item must cite `gameId` plus ply references.
- **Claim validator:** after generation, check that every cited game and ply exists, that the move and eval quoted match stored data, and that every number quoted appears in the input. On a failure, regenerate once with the errors attached. If it fails again, drop the bad item.
- Tone adapts to the rating band (§5). The report is direct and specific, written like a strong human coach.
- Reports are regenerated on a schedule (monthly where the limit allows) or when the profile changes a lot. Reports are versioned, so progress can be compared.
- Prompt caching is used for the large, stable system prompt.

## 10. Product surfaces

1. **Landing page.** A premium hero that shows the product working, such as a real sample report animating in.
2. **Onboarding:** sign up (email magic link or Google), link an account, choose time controls, then a **live analysis screen**. Games count up, the eval graph draws in, and early findings appear as they're computed. This is the signature "wow" moment.
3. **Report**, the core page:
   - Snapshot: rating, target band, playstyle profile, and the seven area scores (§8b).
   - One section per skill area (Openings, Middlegame, Endgame, Tactics, Calculation, Time, Mindset), each with KPIs, trend sparklines, example positions and drills.
   - What you do well (with evidence).
   - What's costing you games, ranked, each with its cost, example positions on an interactive board, and a "train this" button.
   - Openings: repertoire tree with keep / fix / replace verdicts, and suggested new openings matched to style and level.
   - Climb plan: a weekly plan with assigned drills.
4. **Game explorer.** Every analysed game with an eval graph, classified moves and board replay.
5. **Training hub:**
   - *Your puzzles:* positions from the player's own missed tactics and allowed blunders, plus themed puzzles from the open Lichess puzzle database matched to their weak motifs and rating.
   - *Retry mistakes:* replay the critical moment and find the better move.
   - *Opening trainer:* drill the recommended repertoire with spaced repetition (FSRS), including the exact spots where they usually leave theory.
   - *Endgame drills:* key theoretical positions (Lucena, Philidor, K+P opposition and more), played against the engine, with exact results from the Lichess tablebase API.
   - *Clock drills:* timed puzzle sets that simulate low-clock decisions.
   - Daily training queue and streaks.
6. **Progress.** Metrics over time (accuracy by phase, blunder rate, conversion, rating) with report-to-report comparisons.
7. **Account.** Linked accounts.
8. **Shareable report card.** A public, image-friendly summary for social sharing, which also brings in new users.

## 10a. Product rules from user research

Source: `docs/research/competitors-and-users.md`. Paid rivals cost $8–14/month, and single-game review is now free elsewhere, so we win on **cross-game diagnosis, data-backed coaching, and proof that training works**. In scope for launch:

1. **Bounded daily queue.** It's sized to a session length the user picks (10, 20 or 30 minutes). Spaced-repetition load is capped and a backlog count is never shown; overdue items are quietly rescheduled.
2. **Forgiving streaks.** Streak freezes, a tiny minimum (one drill counts), an optional weekly goal instead, and the option to hide streaks entirely.
3. **Lenient grading.** A drill move passes if it keeps the win% within a tolerance of the best move. Only clearly worse moves fail, and the exact best move gets extra credit.
4. **Before-and-after results lead the product.** The Progress page and the weekly email open with before/after for each targeted weakness (§12a), shown with its certainty. No competitor does this.
5. **One critical moment per newly synced game** is offered as an optional 30-second "retry" drill.
6. **Maia** (licence check pending, shared with §8 difficulty):
   - tags each mistake as "typical for your rating" or "unusual for you";
   - predicts realistic opponent replies in the opening trainer;
   - powers a human-like sparring bot for conversion and defence drills.
7. **Own-mistake puzzles default to the last 30 days,** with filters by motif, phase, opening and time control.
8. **Refutations play out on the board,** drawn as red-pen arrows. This is the signature motion moment from §11.
9. **"Ask your coach" chat** (expanded at the owner's request, 2026-11-21: "allow the user to talk to the AI to ask clarifying questions"):
   - It opens anchored to whatever the player is looking at (a report section, weakness, move, position, opening line or game).
   - It answers follow-ups ("why is this a mistake?", "what should I play instead?", "show me another game like this").
   - For "what if I'd played X?" questions it runs an **on-demand server-side engine check**. That's a high-priority interactive job, MultiPV 3, never used for stats. Results are cited through `{{eng:…}}` tokens, so answers stay facts-by-reference.
   - Threads are saved and can be deleted.
   - Limits: the larger limit is 30 messages/day and 300/month, plus 20 engine checks/day. The smaller one is 3 messages/week and 3 engine checks/week. They protect the owner's API bill and are configuration.
   - Safety: chess and own-games only, a minor-safe tone, no rating promises, prompt-injection resistant, and every paragraph validated.
   - The original definition follows. The "Ask your coach" chat: Claude answers questions about the player's own games using only their computed data and the engine (as tools), through the same claim validator. With a fair-use cap.
10. **Free acquisition tools:**
    - unlimited single-game review in the browser (WASM Stockfish), with no signup needed;
    - a no-signup **quick diagnosis** by username: the last 20 games and a teaser report, cached and rate-limited.
11. **Recaps:** a monthly report card and an annual **"Climb Wrapped"** with shareable images and video export.

**Post-launch backlog** (recorded in an ADR, not built now): Coach/Club tier, streamer/Twitch integration, offline drills.

## 11. Visual and interaction design

Owner's words: "sexy design", "amazing UI with animations and a premium feel", "no AI slop".

- **Direction (owner's pick): "Annotated scoresheet".** Cool paper, navy ink, and a coach's red pen for mistakes. It should feel precise and studious, like a game a strong coach has marked up. Starting palette: paper `#F3F5F8`, ink `#16233B`, red pen `#D3312B`, good-move green `#2F7D5B`. Starting type: a text serif (Literata in the mockup) for headlines and annotations, and a grotesk (Schibsted Grotesk in the mockup) for UI. Chess notation (`31. Rxe6?? −4.2`) is a typographic feature in its own right. The builder refines the tokens and adds a dark "night study" theme with the same character. Red-pen motion is the signature animation: annotations, circles and arrows draw themselves onto the board and scoresheet, like a coach marking a game.
- **Hard rules (from the frontend-design guidance):** no cream background with terracotta accent; no near-black with one acid accent; no stock SaaS card grid with identical radius and grey shadows; no all-caps eyebrow labels on every section; no gradient washes as decoration; no "01/02/03" numbering unless the content really is a sequence; no `→` tacked onto buttons; no default font families.
- **One signature element**, used consistently: the board and the evaluation graph are the brand. Everything around them stays quiet.
- **Motion** uses Motion (formerly Framer Motion) and must be purposeful:
  - Orchestrated moments: the live analysis screen, and the report reveal (weaknesses resolve in ranked order as the eval graph draws).
  - Board pieces animate moves, and eval bars spring to their new values.
  - Responses to user actions: drill right/wrong feedback, streak increments, plan completion.
  - Not allowed: a fade-up on every section, or a hover effect on every card.
  - Fully respects `prefers-reduced-motion`.
- **Motion is a headline feature (owner, 2026-11-21: "the animations need to be huge, that makes it premium").** This overrides the "use motion sparingly" default. The rule is now **rich, choreographed and physical, never generic**.
  - **Signature moments:**
    - the **analysis reveal**, a full choreographed sequence: the eval graph draws across, critical moments pulse in, then red-pen marks draw onto the board in ranked order and the scoresheet annotates itself line by line;
    - the **report reveal**: sections assemble with staggered, spring-based motion;
    - **onboarding**: games stream onto a live board, replaying at speed while the counter climbs.
  - **Continuity:**
    - shared-element transitions, using the View Transitions API with a Motion fallback. A position thumbnail morphs into the full board, a weakness card expands into its area page, a KPI tile grows into its chart;
    - route changes animate as page turns on the scoresheet metaphor, not cross-fades.
  - **Physical feel:**
    - pieces move with spring physics, with a small lift and settle on drop and a dragged piece that tilts slightly;
    - captures knock the captured piece off;
    - the eval bar swings with momentum;
    - numbers roll (NumberFlow);
    - progress fills like ink soaking into paper.
  - **Landing page:** scroll-driven storytelling with GSAP ScrollTrigger and Lenis. A real game plays out as you scroll, with the coach's annotations writing themselves in.
  - **Micro-interactions everywhere they mean something:** drill right/wrong, streak ticks, button presses (a pen-press feel), hover previews of moves on the scoresheet. Every one shows what changed. None is decoration with no meaning.
  - **Quality bar:**
    - 60 fps on mid-range phones: only transform and opacity, plus GPU-friendly SVG stroke-dashoffset;
    - motion never blocks input, because all of it is interruptible and skippable;
    - long sequences have a "skip" and run only on first view, with later visits much shorter;
    - full `prefers-reduced-motion` support, which swaps choreography for instant state changes with brief opacity cues.
  - **Still banned:** identical fade-up-on-scroll on every section, hover lift and shadow on every card, gratuitous parallax, and motion with no meaning.
  - DESIGN.md gets a full motion system to match: duration and easing tokens, spring presets (`settle`, `pen`, `swing`, `page`), a stagger scale, and a choreography spec for each signature moment.
- **Sound (owner's request: "sounds for moving pieces").**
  - **The sound set:**
    - move (a soft wooden placement);
    - capture (heavier, with a slight knock);
    - check (a brighter tick);
    - castle (two quick placements);
    - promotion (a rising tone);
    - game end;
    - drill correct (a short warm chime) and drill wrong (a muted low thud, never harsh);
    - optionally, a very subtle pen scratch when a red-pen annotation draws on, off by default.
  - **Sources:**
    - All sounds are **original**. Ideally they're synthesised procedurally with the Web Audio API: tiny, consistent, licence-free, and tunable to match the scoresheet character (paper and wood, not arcade).
    - The alternative is recorded samples under CC0, each with its source logged.
    - **No sounds are copied from chess.com or Lichess** or any other product.
  - **Behaviour:**
    - Sound starts only after the first user gesture, because of browser autoplay rules.
    - There's a global mute and volume control in the header and in settings, remembered per user. Sound is on by default for board interactions and off by default for ambient effects.
    - `prefers-reduced-motion` does not mute sound. Sound has its own setting.
    - Sound never carries information on its own: every sound has a visual equivalent.
    - Repeated sounds are throttled during fast replay or scrubbing. Scrubbing the move list doesn't play a sound per move; only stepping or play does.
  - **Implementation:** a small `@ci/ui` audio module, `useBoardSounds()`, with preloading and a single shared AudioContext. Sounds are triggered from board events (move, capture, check and so on, worked out with chess.js), so every board in the app sounds the same.
- **Quality floor:** responsive down to 360px, keyboard-accessible board and drills, visible focus, WCAG AA contrast, and light and dark themes.
- **Board component:** `react-chessboard` (MIT) or a custom board. **Not** chessground, which is GPL-3.0 and would force the web client to be open source.
- **UI stack** (from `docs/research/ui-tooling.md`):
  - Tailwind 4 `@theme` tokens, with shadcn 4 used only as a source of components: Base UI primitives, every token rewritten, never the stock zinc/Inter/one-radius look.
  - Motion 14 for app animation. GSAP 3.15 plus Lenis only on the landing page.
  - visx for the eval graph and KPI charts.
  - NumberFlow for animated stats; sonner, vaul and cmdk for toasts, drawers and the command menu.
  - Phosphor icons, not Lucide, because Lucide is part of the stock look.
  - Not used: Aceternity or Magic UI components, Three.js.
- **Process (this is what produces premium results, more than any library):**
  1. `DESIGN.md` with tokens, a named ban list and annotated reference screenshots, written *before* any UI code.
  2. Tokens rendered on a `/styleguide` page.
  3. For each major surface, three genuinely different mockups on `/lab` routes using real game data; the builder picks one and records why.
  4. Build the chosen one.
  5. Playwright screenshots at 390px and 1440px, in light and dark.
  6. Critique by a **fresh subagent** (never the agent that built it), plus the Impeccable `detect` linter and the Vercel web-design guidelines.
  7. Fix and repeat, two to three passes per surface.
  8. Visual regression baselines (`toHaveScreenshot`) once stable.
- **Skills and tools:**
  - `frontend-design` generates the designs; Impeccable critiques them. Use one generator and one critic, because stacking taste skills drifts back to generic.
  - `dataviz` for the charts.
  - Project skills `.claude/skills/design-system/` and `.claude/skills/chess-ui/`, written once the tokens are settled.
  - The Playwright MCP, run with `--isolated`, configured by hand in `.mcp.json` with a `cmd /c npx` wrapper for Windows. A plain Playwright `scripts/shots.ts` is the fallback.
  - Third-party skills get read and reviewed before they're installed.

## 12. Cost

Rematch is free and open source, with no paid plans. The AI coach is optional. Whoever turns it on uses their own Anthropic API key, Anthropic bills them, and a spending cap protects their bill (ADR 0061). The internal limit tiers (import size, analysis depth, report interval, chat quota) are configuration that bounds server load and API spend. They are not products.

**Sync cadence:** Lichess games sync every 3 hours. chess.com's data only refreshes every 12–24 hours, so chess.com syncs daily.

## 12a. Measuring improvement (KPIs and progression)

This is how we show, honestly, that the plan is working.

**Collection: polling and recomputation**
- A sync job polls each active user's platforms for new games, every 3 hours for Lichess and daily for chess.com. Users can also tap "Sync now". New games flow through engine → classification → metrics automatically.
- Every analysed game writes one row of **per-game metrics** (`game_metrics`: accuracy by phase, blunders, time-pressure blunders, conversion opportunity and result, motif misses, opening line and deviation ply, endgame type and result, and more). Any time window is then a cheap aggregate over these rows, and nothing needs re-analysing.
- After each sync, the system recomputes **KPI snapshots** (`kpi_snapshot`: user, account, time control, metric, window, value, sample size `n`, confidence interval low/high, computed_at) for two kinds of window: **rolling last-N games** (default 50) and **calendar weeks**. Snapshots are append-only, so history is never rewritten.

**The KPIs**, per account and time control:

| Layer | KPIs |
|---|---|
| Outcome | Rating, from each game's recorded rating; performance vs expected (actual score minus the Elo-expected score against those opponents) |
| Process: one per weakness in the model | Blunders/game; accuracy in opening, middlegame and endgame; blunder rate below 30 s; conversion % from clearly winning positions; resilience % from clearly losing ones; missed-punish rate; miss rate per tactical motif; score and theory depth per opening line; result per endgame type |
| Training | Puzzle rating; drills completed; opening trainer retention (FSRS); streak |
| Transfer, the key proof | For each targeted weakness, its rate in real games **before vs after** the plan assigned drills for it |

**Making it honest**
- Every KPI is shown with its sample size and a confidence band: Wilson intervals for rates, and bootstrap intervals for accuracy. Below a minimum sample, the UI says "need N more games" instead of showing a noisy line.
- Process KPIs are **opponent-adjusted** against the benchmark for the opponent's rating bucket, so playing stronger players doesn't look like getting worse.
- A change is only called "improved" or "worse" when the intervals separate, or a simple two-proportion test passes. Otherwise it's "no clear change yet".

**Plotting it**
- **Progress page:** a rating line with confidence-banded KPI small-multiples underneath. All charts share one time axis, annotated with **events** (report generated, plan started, drill block completed), so before/after is visible at a glance.
- Every weakness card in the report has a sparkline and a "since you started training this" delta.
- Report-to-report comparisons show which weaknesses shrank, stayed or grew.
- Charts follow the `dataviz` skill and the scoresheet design language.

**Feeding back into the plan:** the improvement engine reads the same snapshots to decide when a weakness is fixed (drop it, promote the next one), stuck (change the prescription), or getting worse (escalate). See the improvement engine spec.

**Notifications:** a weekly digest email ("your endgame accuracy rose from 58% to 66% over 40 games") and in-app milestone moments. Both can be switched off.

## 12b. Weakness timeline: when mistakes start, improve, or come back

Owner's requirement: "players may evolve making new mistakes over a period of time, we need to be able to figure out when they start making certain mistakes and when they are improving them".

**What is tracked.** Every weakness W1–W12 (improvement-engine §6.2) is tracked, with each tactical motif, each opening line and each endgame type as its own series. Each series is a time-ordered sequence of per-game counts: events and opportunities, taken from `game_metrics`. All series are monitored all the time, not only the ones in the active plan.

**Detecting a change.**
- Each series runs a **Bayesian online change-point detector** (Beta-binomial, run-length posterior) on per-opportunity rates.
- A **CUSUM** check runs alongside it as a fast early warning.
- A change is confirmed when the posterior probability of a change in the last *k* games is ≥ 0.8 and the new rate differs from the old by ≥ 15% relative, with at least the minimum evidence per weakness from improvement-engine §6.6.
- The detector records the **estimated onset**: the game and date where the change most likely began, with an interval.

**Weakness states.** Each series is always in one state, and every transition is stored with its onset game, its evidence and its confidence (`weakness_state_history`):

| State | Meaning |
|---|---|
| `not_an_issue` | Rate at or below the band median |
| `emerging` | A change upward was detected, but the rate isn't yet clearly above peers |
| `active` | Clearly above the band median (the Confidence rule in improvement-engine §6.3) |
| `improving` | A downward change was detected, or `P_imp` ≥ 0.85 (§6.6) |
| `resolved` | At or below band P40 and holding |
| `relapsed` | A `resolved` weakness has drifted back, caught by the EWMA drift rule in §6.6 |

**Explaining why.** When a change is confirmed, the system checks what else changed around the onset:
- time-control mix;
- a new opening adopted;
- session length or games per day (fatigue);
- rating level, since stronger opponents punish more;
- the protocol version, which rules out engine artefacts.

Claude gets the change, the candidate causes and 2–3 example games from either side of the onset. It writes a short, evidence-backed note, for example: *"Since about 14 Oct you've been missing back-rank threats about twice as often. That started when you switched mostly to 3+0 blitz. Here are three examples."* Correlated changes are presented as possible causes, never as proven ones.

**UI.**
- A **weakness timeline** on the Progress page: one swimlane per weakness, coloured by state, with onset markers, plan events and drill blocks on the same time axis.
- Each area page shows the timeline for its own weaknesses.
- "New this month" and "Fixed this month" appear in the report and the monthly card.
- An `emerging` weakness gets a gentle in-app heads-up. It is promoted to the plan only through the normal Priority rule (§5.4), so players aren't sent chasing noise.

**Data.**
- `weakness_series_point`: per game, per series, the events and opportunities.
- `weakness_state_history`: series, state, onset game, from/to, posterior probability, detector version.

Detection reruns incrementally after each sync. If the analysis protocol changes, the series are recomputed from scratch.

## 13a. Async pipeline, crash detection and recovery

The owner requires that "the flow needs to be async and we need to be able to figure out when things crash and be able to pick up things if errors happen for whatever reason".

**Everything is asynchronous.**
- Web requests never wait on the engine, imports or Claude. They create work and return straight away.
- Work runs as BullMQ jobs: `import-account`, `analyse-game`, `build-profile`, `generate-report`, `sync-account`, `reconcile`.
- Progress is pushed to the browser over server-sent events, read from job progress plus DB state. The UI never polls the engine.

**State lives in Postgres; the queue is only a delivery mechanism.**
- Each unit of work has a durable state row: `games.analysis_status` (`pending → analysing → done | failed | skipped`), `import_runs`, and `report_runs`.
- Each row records `attempts`, `last_error`, `started_at`, `heartbeat_at` and `finished_at`.
- If Redis lost every job, the DB alone would be enough to rebuild the queues.

**Idempotent, resumable jobs.**
- Re-running any job is safe:
  - games are upserted on `(platform, platformGameId)`;
  - a game's moves are replaced in one transaction;
  - evals are cached per position.
- An analysis that crashes halfway therefore resumes almost for free. Positions already evaluated come from the cache, so only the remainder is sent to the engine.
- Imports checkpoint a cursor on the account (`import_cursor`: the oldest month or `until` timestamp reached, plus `newest_seen_at`) after each batch, and resume from it.

**Crash detection.**
1. **Worker crash or kill:** BullMQ's stalled-job detection (lock duration about 60 s, renewed while a job runs) moves the job of a dead worker back to waiting. It's retried up to `maxStalledCount`, then failed.
2. **Engine crash or hang:** handled inside the pool (restart, retry once, 30 s per-position timeout, §6).
3. **A job that silently stops making progress:** jobs write `heartbeat_at` during work. The **reconciler** catches stale heartbeats.
4. **External API outages:** typed errors (`PlatformRateLimitError`, `PlatformTimeoutError`, 5xx). A per-platform **circuit breaker** pauses that platform's import queue after 5 consecutive failures, then probes again with back-off. Analysis carries on unaffected.
5. **Poison jobs:** after 3 attempts with exponential back-off, a job is marked `failed` with its reason and moved to a dead-letter view. Nothing retries forever. The user sees an honest count, such as "3 games couldn't be analysed".

**The reconciler** is a repeatable job that runs every 5 minutes and is idempotent. Its sweeps:
- Re-enqueue games that are `pending` with no live job. This covers a lost enqueue, for example a crash between the DB commit and the enqueue.
- Find games stuck in `analysing` whose `heartbeat_at` is more than 10 minutes old, reset them to `pending` and re-enqueue them.
- Resume import runs stuck in `running` from their cursor.
- Report counts and anomalies as metrics.

The queue is always fed *after* the DB commit, and the reconciler covers the gap, so no separate outbox table is needed (ADR 0026).

**Graceful shutdown.** On SIGTERM, workers stop taking new jobs. In-flight work is either finished within 25 s or left for the stalled-job and reconciler path, and engines are closed. Deploys and restarts lose nothing.

**Visibility.**
- Structured logs (pino) carry a `jobId`, `userId` and `gameId` correlation.
- Sentry captures every failure along with its job context.
- A Bull Board admin view shows queue state.
- The DB and queue reports carry counts by state.
- Health endpoints check Redis, Postgres, engine pool liveness and queue lag.
- Alerts fire when the failure rate goes above 5%, the oldest waiting job is older than 15 minutes, or a circuit breaker opens.

**Non-blocking, progressive UI.** The owner requires that "the UI needs to be able to pick up things as they surface in a non-blocking way".
- **Event stream.** Workers publish domain events after each DB commit, for example:
  - `import.progress {imported, total?}`
  - `game.analysed {gameId, accuracy}`
  - `profile.updated {area, kpis}`
  - `weakness.changed {id, state}`
  - `report.section.ready {section}`
  - `job.failed {kind, reason}`

  Events are appended to a per-user `user_events` table with an increasing id, and then fanned out through Redis pub/sub.
- **Delivery.** The browser keeps one **server-sent events** connection per session (`/api/events`). On reconnect it sends `Last-Event-ID`, and the server replays any missed events from `user_events`, so a dropped connection never loses anything. Events are kept for 7 days. Older state comes from normal queries.
- **Client state.** TanStack Query holds server data. Each event either patches the relevant query cache entry directly or invalidates that one entry, never the whole page.
- **Rendering.**
  - The first load uses React Server Components with Suspense streaming, so the page shell and each section render on their own as their data resolves.
  - A section with no data yet shows a designed placeholder that states progress ("analysing 38 / 200 rapid games"), never a blocking spinner.
  - Sections fill in with the orchestrated reveal motion from §11 as their events arrive.
  - Partial results are labelled with their basis (§8c) and refine as more games land.
- **Never blocking.**
  - No page waits for the engine, imports or Claude.
  - Every action (sync, re-analyse, generate report) returns immediately with a toast, and its result arrives through the stream.
  - The user can navigate, train or read while work continues. A small global activity indicator shows background work, and failures appear as a non-modal notice with a retry button.
- **Fallback.** If SSE is unavailable (some proxies block it), the client falls back to polling `/api/events?after=<id>` every 5 s, using the same event format.

**Tests (required).**
- Kill a worker mid-analysis, and the game still finishes, with no duplicate moves.
- Drop a job from Redis, and the reconciler re-enqueues it.
- An import that dies mid-stream resumes from its cursor without duplicates.
- A poison PGN ends up `failed` after 3 attempts.
- An API that keeps failing trips the circuit breaker.
- SIGTERM during work loses nothing.
- An SSE client that disconnects and reconnects with `Last-Event-ID` receives every missed event, exactly once, in order.

## 13. Robustness

- Idempotent jobs with retries and exponential back-off, a dead-letter queue, and per-user job priority (onboarding first).
- Engine process supervision, with restart on crash and a timeout per position.
- Rate limiters per external API. Per-user AI and engine budgets are enforced.
- Observability: structured logs, Sentry errors, queue dashboards (Bull Board, behind an admin login), and cost metrics (CPU-seconds and tokens per user).
- Tests: heavy unit tests on `chess-core` (classification, motifs, aggregation, with fixed PGN fixtures), contract tests on the import adapters using recorded responses, validator tests for AI output, and Playwright end-to-end tests for onboarding, the report, drills and checkout.
- CI on GitHub Actions: typecheck, lint, test and build.
- Privacy: we store only public game data plus account email. OAuth tokens are encrypted at rest. Users can delete their account and data.

## 14. Licensing, terms and compliance

Source: `docs/research/legal-and-business.md`. This is not legal advice.

**chess.com terms: a launch risk.** The User Agreement §4(D) (March 2026) bans using their content to build educational tools or competing products without written permission. Their API help page says commercial use is fine. The two conflict, and chess.com owns Aimchess.
- **Owner action:** ask chess.com Business Development for written permission, together with the OAuth application.
- **Build:** the chess.com integration is built and tested in full, behind a `CHESSCOM_ENABLED` flag.
- **Launch:** if permission hasn't arrived, launch **Lichess-first** with chess.com off, or limited, depending on the owner's risk call.
- Test chess.com API access from the production host early, because Cloudflare often blocks cloud IP addresses.

**Lichess:** commercial use is explicitly allowed (ToS, August 2026), subject to the request etiquette in §7.

**Own opening explorer (replaces live Lichess Explorer calls).** Since March 2026 the Lichess Explorer needs a user token and is capped at about 20 requests per minute per user, which can't support §8.4 or §8a at scale. Instead, we build a **rating-band opening explorer**: position (FEN) → move statistics, bucketed by rating band and time control, aggregated from the CC0 Lichess game database. The builder picks the sample size and ply depth (for example several recent months, up to ply 30) and records it in an ADR. It's stored in Postgres and refreshed quarterly. The live Lichess Explorer is used only for occasional on-demand lookups with the user's own token.

**Tablebases.** Bulk endgame checks use local Syzygy files (5-piece set, about 1 GB) probed through Stockfish's `SyzygyPath`. Interactive drills with 6–7 pieces call the Lichess tablebase API.

**Stockfish:**
- *Server side:* using the binary in our service is fine.
- *Browser:* sending `stockfish.wasm` to browsers **counts as distribution**. The compliance checklist:
  - pin the exact build;
  - serve it as separate files;
  - host the matching source;
  - add a `/licenses` page;
  - talk to it only over UCI in a Web Worker.

  chess.com's own stockfish.js is the precedent for a closed-source site doing this.

**AI disclosure:** reports and chat are labelled as AI-written, with a note that facts can be wrong and that the engine data is the source of truth.

**Children and privacy:**
- Sign-up asks for age. Under-13s are blocked, per COPPA's amended rule.
- 13–17s get privacy-protective defaults: no public share card by default and no marketing email.
- A DPIA is written, and streak and notification mechanics are checked against the UK Children's Code's ban on nudge techniques.
- GDPR/CCPA: data export and account deletion, OAuth tokens encrypted at rest, cookie consent only if non-essential cookies are used.

**Licences in use:** react-chessboard (MIT), chess.js (BSD-2), the Lichess openings, puzzle and game databases (CC0), Stockfish (GPL, compliant as above), and Maia (licence check pending). chessground, chessops and `cook.py` are not used.

The original licensing notes:

- Stockfish is GPL-3.0. Running it as a separate process on our servers is fine. The browser build is covered above.
- Lichess's openings, puzzle and game databases are CC0. The Lichess API and tablebase API are free, within their rate limits.
- chess.com's and Lichess's names and logos are used only to say "link your account". No implied endorsement.

## 15. Decision records

Every significant decision is recorded as a Markdown ADR in `docs/decisions/NNNN-title.md` (context, decision, alternatives, consequences). The plan must create ADRs for the decisions above, and new decisions made while building get their own ADR.

## 16. Calls made by Claude (owner delegated: "you are in charge of everything")

1. **Cost envelope** (§8c now replaces the game counts below). We evaluate every position once. That gives the quality of both the player's and the opponent's moves at no extra cost, so the §8.6 benchmarks need no extra engine work. The default budget is about 300k nodes per position (about 0.2–0.3 s on one core), deepened only on critical positions. The smallest limit is the last 50 games in their *primary* time control, which works out to roughly 4,000 positions, or about 15–20 core-minutes before cache hits. The largest limit analyses the full history, newest first, with a hidden fair-use cap of the last 2,000 games per time control. Launch is sized for one worker host with 16 dedicated vCPUs, scaled by adding workers. We meter CPU-seconds and tokens per user, and hard-stop at budget.
2. **Limits are configuration.** They live in a single module, so changing a limit is a config change, not a rewrite.
3. **Report scope.** One report per (linked account × time control). The smaller limit gives one, for the primary time control. The larger gives one for every time control with at least 20 games, plus a short cross-report overview. Reports never mix platforms.
4. **Browser engine.** Stockfish (WASM build) runs in a Web Worker for interactive features: drills, retry, and explorer eval. It runs as a separate program, with its license and a source link shown, which meets GPL-3.0 without affecting our own code. Batch analysis stays server-side. Endgame drills also use the Lichess tablebase API for exact results with 7 pieces or fewer.
5. **"Next band" = current rating +200**, on the same platform and time control. Benchmarks are bucketed per 100 points. The §5 coaching bands only control tone and emphasis.
6. **Timeline and team.** Built autonomously by Claude Code agents, in milestones that each leave a runnable, tested system: foundation → ingestion and analysis → profile and AI report → UI → training → sync and progress → hardening.

## 18. Owner actions (things only the owner can do)

1. **chess.com:** request written permission for commercial use from Business Development, and apply for OAuth access. This decides whether chess.com is on at launch (§14).
2. **API keys and accounts:** Anthropic API key, Sentry, an email provider (the builder defaults to Resend), and a Google OAuth client.
3. **Product name and domain.** The working name in code is `chess-improver`. The brand is swappable from one config file.
4. **Hosting:** approve the provider and spend before anything is deployed. Nothing is deployed or published without the owner's go-ahead.

## 17. Left to the builder

- Hosting provider (§6), with an ADR.
- Exact engine node budget and classification thresholds, tuned against fixtures.
- Exact motif heuristics.
- Database schema details.
- Copywriting, which follows §11.
