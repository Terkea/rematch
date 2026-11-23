---
title: Architecture
description: How Rematch turns your chess.com games into analysis, a profile, drills and coaching.
sidebar:
  order: 1
---

Rematch is a monorepo: a Rust backend that does the heavy work, a Next.js web app that shows it, and a small Node sidecar for the optional AI coach. Postgres holds everything, including the job queue. Every choice below has a decision record; the [decision log](/rematch/docs/decisions/) has all of them.

## From a game to advice

1. **Import.** The Rust worker fetches your public chess.com games by username, through a rate-limited client with a circuit breaker, and keeps them in sync every 30 minutes. Requests identify the project and never carry an email address ([0040](/rematch/docs/decisions/0040-chesscom-first-defer-lichess-accounts/), [0043](/rematch/docs/decisions/0043-no-email-in-outbound-requests/), [0076](/rematch/docs/decisions/0076-recurring-account-sync/)).
2. **Queue.** Jobs live in Postgres (`FOR UPDATE SKIP LOCKED`, leases, retries, dead letters). A tiered scheduler decides what to analyse first and shares the engines fairly between players ([0032](/rematch/docs/decisions/0032-postgres-job-queue/), [ADR-R2-16](/rematch/docs/decisions/adr-r2-16-tiered-analysis-scheduler/)).
3. **Analyse.** A pool of checksum-pinned Stockfish 19 engines evaluates every position under a fixed protocol, with a shared evaluation cache. Enrichment adds MultiPV lines, threats, Syzygy endgame tables when installed and, optionally, Maia's estimate of how hard a move was to find at your rating ([0007](/rematch/docs/decisions/0007-analysis-protocol-pinned/), [ADR-R2-14](/rematch/docs/decisions/adr-r2-14-enrichment-protocol/), [ADR-R2-13](/rematch/docs/decisions/adr-r2-13-maia-licence-and-difficulty/)).
4. **Profile.** The profile engine turns analysed moves into features, twelve weakness families (W1 to W12), rates per chance with 80% intervals, a timeline of change points, KPIs, area scores and opening verdicts. Peers come from a rating-band reference built from the open Lichess game database ([ADR-R2-1](/rematch/docs/decisions/adr-r2-1-profile-crates-and-data-layout/), [ADR-R2-4](/rematch/docs/decisions/adr-r2-4-weakness-metric-rulings/), [ADR-R2-11](/rematch/docs/decisions/adr-r2-11-band-reference-method/)).
5. **Coaching input.** One versioned JSON document per account and time class carries every fact the coach may cite. It is stored as a snapshot per profile build, so a report can always be traced to its input ([ADR-R2-2](/rematch/docs/decisions/adr-r2-2-coaching-input-contract/), [0086](/rematch/docs/decisions/0086-coaching-input-snapshot-per-profile-build/)).
6. **Coach.** Rust builds the prompts and sends them to the AI sidecar, which runs Claude through the Agent SDK. What comes back is validated paragraph by paragraph against the coaching input before anything is stored or streamed ([0060](/rematch/docs/decisions/0060-sectioned-generation-and-facts-by-reference/), [0062](/rematch/docs/decisions/0062-claude-agent-sdk-sidecar/), [0064](/rematch/docs/decisions/0064-ask-your-coach-chat/)).
7. **Show.** The web app reads Postgres directly through read models, gets live updates over server-sent events fed by `LISTEN/NOTIFY`, and draws everything with one design system ([0034](/rematch/docs/decisions/0034-user-events-listen-notify/), [ADR-WEB-8](/rematch/docs/decisions/adr-web-8-sse-delivery/), [ADR-WEB-7](/rematch/docs/decisions/adr-web-7-design-tokens/)).

## The repository

| Path | What it is |
|---|---|
| [`backend/`](https://github.com/Terkea/rematch/tree/main/backend) | The Cargo workspace: `worker`, `analysis`, `engine`, `enrich`, `queue`, `scheduler`, `platforms`, `profile`, `profile-core`, `coaching-input`, `ai`, `db`, `events`, `refdata`, `lichess-db`, `cli`, `chess-core`, `common` and `testkit`. Built and tested only in Docker. |
| [`apps/web/`](https://github.com/Terkea/rematch/tree/main/apps/web) | The Next.js app (App Router), with Better Auth, the read models and the API routes. |
| [`packages/ui/`](https://github.com/Terkea/rematch/tree/main/packages/ui) | `@ci/ui`: the board, annotations, eval graph, charts, stats, motion, sounds and the design tokens. [`DESIGN.md`](https://github.com/Terkea/rematch/blob/main/packages/ui/DESIGN.md) is the law for every pixel. |
| [`packages/db/`](https://github.com/Terkea/rematch/tree/main/packages/db), [`packages/chess-core/`](https://github.com/Terkea/rematch/tree/main/packages/chess-core) | The web's database client and generated schema types, and the shared TypeScript chess helpers. |
| [`services/ai-sidecar/`](https://github.com/Terkea/rematch/tree/main/services/ai-sidecar) | The Node service that runs Claude through the Agent SDK. It holds no business rules. |
| [`services/maia/`](https://github.com/Terkea/rematch/tree/main/services/maia) | The optional Maia-2 difficulty service (Python). |
| [`apps/worker/`](https://github.com/Terkea/rematch/tree/main/apps/worker) | The earlier TypeScript worker, the reference the Rust port was proven against ([0038](/rematch/docs/decisions/0038-parity-and-ts-backend-removal/)). |
| [`db/migrations/`](https://github.com/Terkea/rematch/tree/main/db/migrations) | Plain SQL migrations, applied by sqlx: the schema's source of truth ([0033](/rematch/docs/decisions/0033-sql-migrations-own-the-schema/)). |
| [`docs/`](https://github.com/Terkea/rematch/tree/main/docs) | The decision log, the research and the plans. |
| [`site/`](https://github.com/Terkea/rematch/tree/main/site) | This site: Astro and Starlight, built from the same tokens and fonts as the app. |

## Running pieces

`docker compose up` starts Postgres 17, Redis, Mailpit, the web app, the Rust worker and the AI sidecar. The sidecar has no credential until you add one, and the worker then runs computed-only. Maia is an opt-in overlay. The base Compose file publishes no host ports, so several projects can run side by side; `.env.example` opts the main one into ports 3100 (web), 48025 (Mailpit), 55432 (Postgres) and 56379 (Redis) ([0023](/rematch/docs/decisions/0023-local-stack-host-ports/), [0067](/rematch/docs/decisions/0067-ci-gates-in-compose-and-production-e2e/)).

## Why Rust

The backend began in TypeScript. Analysis is CPU-bound and runs for every move of every game, so the owner moved the whole backend to Rust for efficiency, and the port had to match the TypeScript worker's analysis exactly before it replaced it ([0030](/rematch/docs/decisions/0030-rust-backend/), [0038](/rematch/docs/decisions/0038-parity-and-ts-backend-removal/)). The web stays in TypeScript.
