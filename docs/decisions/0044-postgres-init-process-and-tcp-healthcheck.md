# 0044. Postgres runs under an init process with an exec-form TCP healthcheck

Date: 2026-11-21. Status: accepted.

## Context

Under host load, with many lanes running at once, the Rust suite failed now and then on Postgres connections. A plain rerun passed. Lanes saw these failures:

- `analysis` `analyse_game`: 24 tests failed with connection EOF.
- `sqlx::test` setup panics at `testing/mod.rs:227` and `:256` / `:262` ("failed to connect to setup test database", "failed to apply migrations": `UnexpectedEof`, `Connection reset by peer`), in the `platforms`, `queue` and `analysis` suites.
- Up to 37 tests failed in one run.

The investigation ([report](../../.superpowers/sdd/pg-flake-report.md)) found that **Postgres itself crash-restarted** in the middle of a run. The Postgres log shows:

```
LOG:  server process (PID 13627) exited with exit code 2
LOG:  terminating any other active server processes
LOG:  all server processes terminated; reinitializing
LOG:  syncing data directory (fsync), elapsed time: 40.00 s ...
FATAL:  the database system is in recovery mode      (repeated for ~45 s)
```

Every symptom falls inside that recovery window.

**What exits with code 2.** The PID is not a Postgres backend. The process never logged `connection received`, and it never received a SIGQUIT. A kernel trace (`signal_generate` plus `sys_enter_exit_group`) caught two crashes. In both, a `pg_isready` process exited with code 2, and 1 to 3 ms later the postmaster sent SIGQUIT to every backend. The exiting process's libc base address differed from the postmaster's, so it was a separately exec'd program, not a fork of the postmaster.

**The mechanism.**

1. The image runs `postgres` as PID 1 of the container, with no init process.
2. The healthcheck was `CMD-SHELL "pg_isready ..."` with `timeout: 3s`. Under CPU starvation it takes longer than 3 s, so Docker kills the healthcheck's process, which is the shell (`/bin/sh` is dash, which forks `pg_isready` and does not exec it).
3. The orphaned `pg_isready` is reparented to the namespace's PID 1, the postmaster. containerd-shim's subreaper is outside the container's PID namespace.
4. `pg_isready` then hits its own 3 s timeout and exits 2 ("no response").
5. The postmaster reaps it. In `CleanupBackend` (PG 17 `postmaster.c`), any exit status other than 0 or 1 calls `HandleChildCrash` before the PID is checked against the backend list. Postgres SIGQUITs every backend and runs crash recovery. With dozens of `sqlx::test` databases, the recovery fsync of the data directory took 40 to 45 s under load.

Two deterministic reproductions on `postgres:17`, both 100 %:

- `docker exec <pg> sh -c '(sleep 1; exit 2) & exit 0'` triggers a full reset.
- A healthcheck `sh -c "sleep 3; exit 2"; true` with `timeout: 1s` resets Postgres on its first timeout.

With `--init` (tini as PID 1), neither reproduction crashes.

**Hypotheses ruled out.**

- **H1 (healthcheck passes against the init-phase temporary server).** It is true: `pg_isready` over the unix socket answers the image's temporary init server, which listens on no TCP port. But it does not cause the flake. The lane's compile (47 s or more on a fresh target) outlasts the init restart, and the crashes happened minutes into runs on long-running servers. It is fixed anyway, at no cost.
- **H2 (`max_connections` 100 exhausted).** The peak was 35 client backends during full runs, sampled every 0.2 s. `cargo test` runs one test binary at a time, and sqlx caps each binary at a 20-connection master pool, whose per-test pools are children of it. No "too many clients" was ever logged.
- **H3 (template or lock contention in sqlx's test-DB creation).** sqlx serialises the schema setup with an advisory lock, and `CREATE DATABASE` contention produces waits, not EOFs. No lock errors were logged.
- **H4 (CPU starvation causing connect timeouts).** Starvation is the trigger, because it makes the healthcheck time out, but it is not the failure. No client-side connect timeout appeared. Every failure was the server resetting.
- **OOM kills.** These were ruled out. The VM's `dmesg` has no OOM-killer entries, and an OOM kill would be logged as "terminated by signal 9", not "exit code 2".

## Decision

In `docker-compose.yml`, on the `postgres` service:

- **`init: true`.** Docker's tini runs as PID 1 and reaps orphans, so the postmaster only ever sees its own children. This fixes the root cause for any orphan, not just the healthcheck's.
- **Exec-form TCP healthcheck:** `["CMD", "pg_isready", "-h", "127.0.0.1", "-U", "postgres", "-d", "chess"]`.
  - Exec form has no shell wrapper, so a timeout kills `pg_isready` itself and orphans nothing. This is defence in depth.
  - `-h 127.0.0.1` makes the check fail against the unix-socket-only init server. The service becomes healthy only once the final server is up, which fixes H1.
- **A regression test,** `scripts/test-postgres-orphan-reap.sh`. It orphans a process that exits with 2 inside the postgres container, asserts that Postgres did not reinitialise, and asserts that the healthcheck uses the exec form over TCP. It fails without the fix and passes with it.

These were not done:

- **No retries in tests,** and no changes to the sqlx pool settings or `--test-threads`. Connection limits were never the cause.
- **No `fsync=off`, `full_page_writes=off` or memory caps.** Once the crash is gone, recovery cost does not matter, and the dev stack shares this service. They remain an option for a test-only override if recovery time ever matters again.

## Consequences

- With the fix, the same load reproduction shows no Postgres resets and no connection failures. See the report for the before and after rates.
- The dev, test and CI stacks all get the fix, since they share this service. tini forwards SIGTERM, so `docker compose stop` still shuts Postgres down cleanly.
- Any other service whose PID 1 treats unknown children specially should get `init: true` too. Redis does not, so it is unchanged.
