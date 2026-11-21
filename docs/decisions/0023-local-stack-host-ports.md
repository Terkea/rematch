# 0023. Local stack host ports 55432 and 56379

Date: 2026-11-21
Status: Accepted

## Context

Developer machines commonly already run Postgres on 5432 and Redis on 6379 (for example other projects' containers), so publishing the defaults makes `docker compose up` fail.

## Decision

The Compose stack publishes Postgres on host port 55432 and Redis on host port 56379; container ports stay 5432 and 6379. Services inside the Compose network use the service hostnames (`postgres:5432`, `redis:6379`); `.env.example` documents the `localhost:55432` and `localhost:56379` URLs for tools run from the host. The Compose project is named `chess-improver` so volumes and networks do not collide with other projects.

## Alternatives considered

Stopping the conflicting local services; not publishing ports at all (breaks host-side tooling).

## Consequences

No collisions with common local services. Host-side connection strings differ from in-container ones.

See ADR 0003 and ADR 0004.

## Addendum

Ports are bound to `127.0.0.1` only, so the development databases are not exposed on the network. `docker/postgres/init.sql` (which creates `chess_test`) runs only when the `pgdata` volume is first initialised; on an existing volume run `docker compose exec postgres psql -U postgres -c "CREATE DATABASE chess_test"`.
