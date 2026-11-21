# 0004. Postgres with Drizzle ORM

Date: 2026-11-21
Status: Accepted

## Context

The data is relational (users, games, moves, metrics, KPI snapshots) and needs heavy aggregation.

## Decision

Postgres 17 with Drizzle ORM (Apache-2.0) and SQL migrations.

## Alternatives considered

Prisma, MongoDB, ClickHouse for analytics.

## Consequences

Only one database to run. Revisit a columnar store only if KPI aggregation gets slow.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
