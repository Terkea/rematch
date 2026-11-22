# 0066. Postgres connection limit for the integrated worker

Date: 2026-11-22. Status: accepted (R3/R2 integration merge). Builds on ADR 0044 (the
`postgres` service), ADR-R2-25 (R2 worker wiring) and R3 Task 26 (AI runners, ADR 0062).

## Context

R2 Task 35 and R3 Task 26 each added runners to `start_workers`. On the integrated tree one
worker starts 18 runners (4 from R1, 4 from R2, 9 AI kinds, the reconciler) and sizes its pool
as `pool_size + max(1, pool_size / 2) + POOL_HEADROOM (24) + AI_POOL_HEADROOM (30)`: 56
connections at one engine, 81 at 18 engines. Postgres's default `max_connections` is 100. At
start every runner opens its NOTIFY listener and claims at once, so four workers started in
parallel by the worker tests (`RUST_TEST_THREADS=4`) were refused with "sorry, too many clients
already" (seen as `PoolTimedOut`), and a production worker at 18 engines would leave the web app
under 20 connections.

## Decision

The compose `postgres` service runs with `max_connections=300`. The worker's own budget is
unchanged: it is what the runners need (see `worker::start` and `worker::ai_jobs`).

## Consequences

- Parallel worker tests and a full worker beside the web app fit.
- Each idle slot costs a little shared memory; 300 is well within a dev host.
- Production must set the same (or a higher) limit, or put a pooler in front of Postgres.
