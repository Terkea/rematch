# 0031. Rust workspace, toolchain and crates

Date: 2026-11-21
Status: Accepted. Implements 0030.

## Context

ADR 0030 moves the backend to Rust. Milestone R1 builds it in up to nine parallel lanes, each in its own git worktree. The lanes must not fight over shared files (the workspace manifest, the lockfile, crate module lists, the migration sequence), and they must build identically on a Windows host with no Rust toolchain. Supply-chain rules match the TypeScript side: no copyleft crates, and nothing newer than seven days.

## Decision

- **Workspace layout (`backend/`).** Resolver 3, edition 2024, `rust-version = "1.100"`, `publish = false`, `unsafe_code = "forbid"` for every crate.
  - Library crates in `backend/crates/`: `chess-core`, `db`, `queue`, `engine`, `platforms`, `analysis`, `events`, and the dev-only `testkit`.
  - Binary crates: `worker` and `cli` (each a lib plus `src/main.rs`), and the dev tool `backend/tools/parity`.
  - Recorded fixtures live in `backend/fixtures/`, copied byte for byte from the TS tests.
  - Release profile: `lto = "thin"`, `codegen-units = 1`.
- **Toolchain pin.** `backend/rust-toolchain.toml` pins `1.100.0` (released 2026-11-12) with `rustfmt` and `clippy`. The images are `rust:1.100.0-slim-bookworm` (build) and `debian:bookworm-slim` (runtime). `rustfmt.toml`: edition 2024, `max_width = 100`.
- **Crates (pinned exactly with `=x.y.z` in `[workspace.dependencies]`).** Every version was published at least 7 days before adoption. Where the newest release was younger, the newest qualifying release is used.

| crate | version | published | licence | why |
|---|---|---|---|---|
| tokio | 1.54.1 | 2026-10-21 | MIT | runtime; `rt-multi-thread, macros, process, io-util, time, signal, sync, net` (+ `test-util` in dev) |
| sqlx | 0.9.0 | 2026-05-21 | MIT OR Apache-2.0 | `postgres, runtime-tokio, tls-rustls-ring-native-roots, macros, migrate, chrono, uuid, json`. Ring plus native roots avoids aws-lc's OpenSSL licence and webpki-roots' CDLA licence. 0.9.1 (2026-11-18) was too new. |
| reqwest | 0.13.5 | 2026-09-08 | MIT OR Apache-2.0 | `default-features = false`, `rustls-no-provider, stream, json`; the ring provider is installed at startup |
| rustls | 0.23.46 | 2026-10-14 | Apache-2.0 OR ISC OR MIT | `default-features = false`, `ring, std` |
| serde | 1.0.229 | 2026-07-18 | MIT OR Apache-2.0 | `derive` |
| serde_json | 1.0.154 | 2026-10-11 | MIT OR Apache-2.0 | |
| cozy-chess | 0.3.4 | 2024-04-05 | MIT | move generation; default features (no `pext`, so binaries stay portable) |
| thiserror | 2.0.21 | 2026-09-23 | MIT OR Apache-2.0 | |
| anyhow | 1.0.104 | 2026-07-18 | MIT OR Apache-2.0 | binaries only |
| tracing | 0.1.44 | 2025-12-18 | MIT | |
| tracing-subscriber | 0.3.23 | 2026-03-13 | MIT | `json, env-filter, fmt` |
| clap | 4.6.7 | 2026-09-14 | MIT OR Apache-2.0 | `derive`. 4.6.8 and 4.6.9 were too new. |
| uuid | 1.29.2 | 2026-11-09 | Apache-2.0 OR MIT | `v4, serde`. 1.29.3 was too new. |
| chrono | 0.4.45 | 2026-06-04 | MIT OR Apache-2.0 | `default-features = false`, `clock, std, serde` |
| futures | 0.3.34 | 2026-08-11 | MIT OR Apache-2.0 | BoxFuture, BoxStream |
| tokio-util | 0.7.20 | 2026-10-09 | MIT | CancellationToken |
| bytes | 1.12.1 | 2026-07-08 | MIT | stream chunks. 1.12.2 was too new. |
| regex | 1.13.1 | 2026-07-15 | MIT OR Apache-2.0 | PGN headers, clock comments, chess.com headers. 1.13.2 was too new. |
| rand | 0.10.3 | 2026-09-20 | MIT OR Apache-2.0 | breaker re-delay jitter. 0.10.4 was too new. |
| wiremock (dev) | 0.6.5 | 2025-08-24 | MIT/Apache-2.0 | HTTP client tests |
| cargo-chef (tool) | 0.1.78 | 2026-08-12 | Apache-2.0 OR MIT | Docker layer caching of dependencies (runtime image, Task 22) |
| cargo-deny (tool) | 0.20.2 | 2026-07-09 | MIT OR Apache-2.0 | licence, advisory, ban and source checks. 0.21.0 and 0.22.0 were too new. |

  Transitive crates in `Cargo.lock` follow the same seven-day rule. Younger ones are held back with `cargo update -p <crate> --precise <version>`. At bootstrap, all 267 registry packages in the lockfile were checked against crates.io. Ten were held back: cc 1.11.2, clap_lex 1.1.1, form_urlencoded 1.2.2, pin-project-lite 0.2.17, regex-automata 0.4.18, regex-syntax 0.8.11, smallvec 1.16.5, url 2.5.8, walkdir 2.5.0 and zeroize 1.9.1. A plain `cargo update` would undo this, so the coordinator re-checks ages whenever the lockfile changes.
- **Licences.** `backend/deny.toml` allows only MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0 and Zlib (and their combinations). It denies yanked crates, known advisories, unknown registries and git sources. Duplicate versions only warn. CI fails on any violation, and the allowlist is widened only by an ADR addendum. No copyleft crates: no shakmaty or pgn-reader (GPL-3.0).
- **Runtime-checked SQL.** Queries use `sqlx::query`/`query_as` with `#[derive(sqlx::FromRow)]`, not the `query!` macros. Builds need no `DATABASE_URL`, and there is no `.sqlx` offline cache for parallel lanes to fight over. Postgres enums are bound as text with explicit casts and read back as `col::text`. `chess-core` stays free of sqlx.
- **Containers-only builds.** Every cargo command runs in the compose service `rust-dev` (`docker compose run --rm rust-dev <cmd>`, working directory `/work/backend`). The image has the toolchain, cargo-chef, cargo-deny, `postgresql-client` and Stockfish 19 (the same pinned asset and SHA-256 as `apps/worker/Dockerfile`). The source is bind-mounted. The cargo registry and git caches are volumes.
- **Per-lane target dirs.** `CARGO_TARGET_DIR=/target/${LANE:-main}` inside the shared `rust_target` volume, so parallel lanes never block on each other's build lock. `#[sqlx::test]` creates and drops its own database per test, using `DATABASE_URL` (`chess_test`) as the master connection, so lanes can share one Postgres.
- **Lockfile ownership.** Task 1 declares every dependency of every crate and commits `Cargo.lock`. No later task edits a `Cargo.toml` dependency list or `Cargo.lock`. A lane that needs a new crate stops and reports it. The coordinator adds it in one commit on the base branch, and lanes rebase. Pre-declared dependencies are unused until their task lands, so the cargo lint `unused_dependencies` is allowed workspace-wide until R1 is complete.
- **Shared module lists.** Task 1 creates every crate's `lib.rs` module list and the empty module files, and the shared types that cross lane boundaries (`chess_core::types`, `engine::types`, `platforms::types`, `queue::{types,kinds}`, `db::MIGRATOR`, `testkit`). Later tasks implement behaviour but do not change these definitions.
- **Migration sequence.** `db/migrations/20261121000000_m1_baseline.sql` is the M1 Drizzle schema (`0000`–`0002`, concatenated, breakpoints removed). The following versions are reserved. Each file is created only by its owning task, because an applied placeholder would be checksummed and editing it would break `migrate`:
  - `20261121000100_jobs.sql` (Task 13)
  - `20261121000200_user_events_watermark.sql` (Task 16)
  - `20261121000300_analysis_generation.sql` (Task 19)
  - `20261121000400_job_metrics.sql` (Task 19)

## Known exceptions

- **Target scope of `cargo deny`.** The check graph is limited to `x86_64-unknown-linux-gnu`, the only target we build and ship. Without this limit, cargo-deny would also check `webpki-root-certs` (CDLA-Permissive-2.0). `rustls-platform-verifier` (via reqwest 0.13) pulls that crate in only on `wasm32`, so it is never compiled for us. The licence allowlist itself is unchanged.
- **RUSTSEC-2026-0543 (chrono is unmaintained).** This is an informational advisory. It reports no vulnerability, and no maintained chrono release exists. `chrono` is ignored by ID in `deny.toml`, with a reason, because sqlx 0.9 and the shared types (`DateTime<Utc>`) use it. Replacing it with `jiff` or `time` (both supported by sqlx) changes cross-lane interfaces, so that change needs its own ADR. Any other advisory against chrono still fails CI.

## Alternatives considered

- Floating semver ranges: these give a smaller manifest, but they break the seven-day rule and make lane builds non-reproducible.
- `query!` macros with an offline `.sqlx` cache: these give compile-time checking, but every lane would regenerate one shared cache.
- A host toolchain: Windows-only builds would diverge from CI and production.
- Shakmaty: it is the best-known move generator, but it is GPL-3.0.

## Consequences

Lanes can work in parallel without merge conflicts in manifests or the lockfile. A new dependency costs a coordinator round-trip. Version bumps are deliberate, one commit at a time, and must pass the seven-day rule and `cargo deny check`. The first build in a fresh volume compiles every dependency (several minutes). After that, builds are incremental per lane.
