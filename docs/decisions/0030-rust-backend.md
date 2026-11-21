# 0030. Rust for the entire backend

Date: 2026-11-21
Status: Accepted. Supersedes 0021; changes 0003 (BullMQ) and 0004 (Drizzle as the schema owner); amends 0011 (the Claude client).

## Context

The owner wants the platform to be as efficient as possible: "5% of cpu is huge, lets transition to rust". Measurements show that Stockfish search is about 90% of the analysis CPU per game (about 12 core-seconds per game). The rest is our own orchestration, parsing and persistence. The owner chose "Rust for all backend" over keeping the AI service in TypeScript.

## Decision

- **Rust (cargo workspace `backend/`)** owns everything server-side except the web app:
  - `chess-core`: score, judgement, PGN with clocks, phases, move records;
  - `db` (sqlx);
  - `queue` (a Postgres job queue);
  - `engine` (the Stockfish UCI pool);
  - `platforms` (the rate-limited HTTP client, circuit breaker, and the Lichess and chess.com importers);
  - `analysis` (the eval cache, game evaluation, the import and analyse jobs);
  - `events` (user_events plus LISTEN/NOTIFY);
  - `profile` (Milestone 2);
  - `ai` (Milestone 3: an Anthropic Messages, Batch and streaming client over HTTP, JSON-schema structured outputs, the claim validator with reference tokens, prompts);
  - binaries `worker` and `cli`.
- **TypeScript** stays only for `apps/web` (Next.js) and `packages/ui`.
- **The job queue moves from Redis/BullMQ into Postgres**: `FOR UPDATE SKIP LOCKED`, leases with heartbeats, retries with backoff, delayed jobs, dedupe keys, priorities and a dead-letter state. One language-agnostic queue, with state and delivery in one database. Redis is removed. Live events use `user_events` plus Postgres `LISTEN/NOTIFY`.
- **The schema source of truth is plain SQL migrations in `db/migrations/`**, applied by sqlx. The web app's Drizzle schema is generated from the database (`drizzle-kit pull`), and CI fails on drift. The Better Auth tables are also defined in SQL migrations.
- **Licences:** no copyleft crates. In particular no shakmaty or pgn-reader (GPL-3.0). Use cozy-chess (MIT) for move generation, our own PGN parser, and `cargo-deny` for licence and advisory checks in CI.
- **Process:** the TypeScript implementation from Milestone 1 (`packages/chess-core`, `packages/db`, `apps/worker`) and the Milestone 3 work so far (`packages/ai`) become the **reference specification**. Their tests are ported to Rust and must pass against the Rust code (parity). The TypeScript backend packages are deleted once parity is reached.
- **Measurement:** per-stage timing (engine, parse, persistence, queue) is built in from the start, to verify the efficiency goal.

## Alternatives considered

Keep TypeScript (estimated <5% of CPU in our own code); Rust only for the compute core, with TypeScript for the AI service.

## Consequences

The backend is re-planned as Rust milestones R1 (port of Milestone 1), R2 (profile engine) and R3 (AI coaching). The web app (Milestone 4) continues unchanged, except that its realtime layer uses LISTEN/NOTIFY instead of Redis pub/sub, and it reads its schema from SQL migrations. Two toolchains have to be maintained. There is no official Anthropic Rust SDK, so the AI client is our own code with recorded-fixture tests.
