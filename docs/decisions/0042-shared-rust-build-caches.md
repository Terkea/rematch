# 0042. Shared Rust build caches across lanes

Date: 2026-11-21. Status: accepted.

## Context

Every lane runs `docker compose -p ci-<lane> ...`. In `rust-dev`, the `rust_target`, `cargo_registry` and `cargo_git` volumes were project-scoped, so each lane started with empty volumes: it downloaded every crate again and compiled every dependency from scratch. Lanes finish with `down -v`, which deleted all of it. The per-lane `CARGO_TARGET_DIR` (`/target/$LANE`) was meant to let lanes share one target volume. It never did, because each project had its own volume.

## Decision

- **External cache volumes.** `ci-cargo-registry` (`/usr/local/cargo/registry`), `ci-cargo-git` (`/usr/local/cargo/git`) and `ci-sccache` (`/sccache`) are `external: true` with fixed names.
  - No compose project owns them, so `down -v` never removes them.
  - `scripts/rust-cache-init.sh` creates them. It is idempotent and runs once per Docker host, and CI runs it before compose.
  - The entrypoint cannot create them, because compose checks external volumes before the container starts. A missing one fails clearly: `external volume "ci-sccache" not found`.
  - Cargo locks the package cache, so concurrent lanes can share the registry.
- **sccache 0.19.0** in the dev image, set with `RUSTC_WRAPPER`, `SCCACHE_DIR=/sccache` and `SCCACHE_CACHE_SIZE=20G`. It is the official release binary, pinned by version and SHA-256, as the Stockfish layer is.
  - sccache cannot cache incremental compilations. In the dev and test profiles, cargo compiles workspace members incrementally and registry dependencies never.
  - So workspace crates stay incremental, for fast rebuilds in the lane's own target dir, and sccache passes them through. Dependencies are cached once and reused by every lane.
  - Not cacheable: build-script and other `bin` crates, and proc-macros. Those still compile in each new lane.
- **One target path for every lane.** sccache's cache key includes the `--out-dir`, `-L dependency=` and `OUT_DIR` paths. With `CARGO_TARGET_DIR=/target/<lane>`, each lane missed every Rust entry: 0 % hits across lanes in the measurement.
  - The entrypoint therefore links `/lane-target` to `/target/<lane>` and points cargo at `/lane-target`.
  - The symlink lives in the container's own filesystem. Each lane still writes only its own directory, and every lane presents the same path to sccache (100 % hits).
  - `rust_target` stays per project, so `down -v` cleans up the lane's build output.
- **crates.io age check without 429s.** Several lanes running the gate at once got HTTP 429 from crates.io (R3 Task 6 failed twice on it). Changes to `scripts/check-crate-age.sh`:
  - Publish dates are cached per crate@version, one file each, on a fourth external volume, `ci-crate-age` (`/crate-age`, set through `CRATE_AGE_CACHE`). Entries are written to a temp file and renamed, so parallel lanes never read half an entry. Only versions no lane has seen are queried.
  - A 429, a 5xx or a network error is retried with exponential backoff (1, 2, 4, 8, 16 s) plus up to 1 s of jitter, honouring a numeric `Retry-After`. The budget is `CRATE_AGE_RETRY_BUDGET`, 60 s per crate by default.
  - Fails closed (exit 2) when the budget runs out, on any other status, or on a 200 without a valid `created_at`. A failed lookup is never cached.
  - Keeps the one-request-per-second pace and sends only `chess-improver-ci (+https://github.com/chess-improver)` as User-Agent. No email or other identity is sent.
  - `CRATE_AGE_OFFLINE=1` is a dry-run mode that makes no network calls and fails if any version is uncached.
  - `scripts/test-check-crate-age.sh` puts a fake `curl` on `PATH` and proves six cases:
    - a miss makes one call with the right User-Agent;
    - a cache hit makes zero calls, in online and offline mode;
    - an uncached version in offline mode fails without calling;
    - 429 with `Retry-After`, then 503, then 200 succeeds;
    - endless 429 fails at the budget and caches nothing;
    - a 404 is not retried.

    CI runs it before the age check. CI's own age step keeps its `actions/cache` directory, because runners have no persistent volumes.
- **Linker: keep rustc's bundled rust-lld.** Since Rust 1.90 it is the default on `x86_64-unknown-linux-gnu`, so the image already links with LLD 23. The alternatives were measured in the dev image. Each run touched `crates/common` and relinked every test binary, three to five alternating runs each:
  - rust-lld: about 7.8 s.
  - clang 14 + lld 14 from Debian bookworm: about 10.3 s.
  - mold 2.42.1 from its official MIT release: about 13.8 s, with or without `--no-fork`. Bookworm itself ships only mold 1.10, which is AGPL.
  - GNU ld: about 18.6 s.

  The fastest linker is therefore the one already in use, and the image adds none. The Dockerfile records the numbers, so nobody adds `-C linker-features=-lld` or `-fuse-ld` without measuring again.
- **cargo-nextest 0.9.151** (Apache-2.0 OR MIT, released 2026-11-03) is installed from its release binary, pinned by version and SHA-256.
  - The `NEXTEST_CRATE=cargo-nextest@<version>` spelling lets `scripts/check-crate-age.sh` check it against crates.io.
  - It is an optional local runner. The gate stays `cargo test`, which also runs doctests.
- **Release age.** sccache 0.19.0 was released 2026-10-24 and is not a `cargo-*` tool, so the age script does not see it. Its age was checked by hand when it was pinned, and the same applies to every later bump.

## Consequences

- A new lane downloads nothing and compiles only the workspace crates, the build scripts and the proc-macros. The measurement was `cargo test --locked --workspace --no-run` on 24 cores, with other lanes running, so the numbers are noisy:
  - Before, every lane was cold: 43 s and 59 s, with 215 crates downloaded each time.
  - After, the first lane is cold: 48 s, which includes sccache's writes.
  - After, a second project and LANE: 24 s (27 s in an earlier run). Nothing was downloaded, and the sccache Rust hit rate was 100 %.
- The first build on a host with an empty cache pays sccache's write overhead once.
- The caches grow until sccache's 20 GiB LRU limit, and the registry grows without bound. To reset one: `docker volume rm ci-sccache && sh scripts/rust-cache-init.sh`. The crate-age cache holds one small file per crate@version and never needs a reset: publish dates never change. Lane cleanup must never remove these volumes.
- Several sccache servers (one per running container) write to the same local cache directory. Entries are written atomically, but each server enforces the size limit only for its own writes, so the cache can briefly exceed 20 GiB.
- Host builds are unaffected: `RUSTC_WRAPPER` and the linker flags are set only in the `dev` image.
