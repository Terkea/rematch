# syntax=docker/dockerfile:1
# Build context: the repository root (so db/migrations is visible to sqlx::migrate!).

# Toolchain plus the cargo tools used by the dev image and CI.
FROM rust:1.100.0-slim-bookworm AS chef
RUN cargo install --locked cargo-chef@0.1.78 cargo-deny@0.20.2 \
 && rm -rf /usr/local/cargo/registry /usr/local/cargo/git \
 && rustup component add rustfmt clippy

# Stockfish 19, generic x86-64 Linux build (same asset and checksum as apps/worker/Dockerfile).
FROM debian:bookworm-slim AS stockfish
ARG STOCKFISH_ASSET=stockfish-linux-x86-64-universal.tar.gz
ARG STOCKFISH_SHA256=9defc0d4e55d49c65a6d042f3e571a39fcea499ade6dbe741b53b8c65e03611f
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && curl -fsSL -o /tmp/sf.tar.gz \
      "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/${STOCKFISH_ASSET}" \
 && echo "${STOCKFISH_SHA256}  /tmp/sf.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/sf.tar.gz -C /tmp \
 && install -m 0755 /tmp/stockfish/stockfish-linux-x86-64-universal /usr/local/bin/stockfish \
 && rm -rf /tmp/sf.tar.gz /tmp/stockfish

# Build tools for the dev image (ADR 0042), each an official GitHub release binary pinned by
# version and sha256, and at least 7 days old when pinned:
# - sccache 0.19.0 (Apache-2.0, released 2026-10-24): compiler cache shared by every lane.
# - cargo-nextest 0.9.151 (Apache-2.0 OR MIT, released 2026-11-03): optional test runner.
#   NEXTEST_CRATE keeps the `cargo-<name>@<version>` spelling so scripts/check-crate-age.sh
#   checks it against crates.io too.
FROM debian:bookworm-slim AS buildtools
ARG SCCACHE_VERSION=0.19.0
ARG SCCACHE_SHA256=aa73c7270c997894038c0d8f0d297865f57569e1687883322bc431713d4bb9dd
ARG NEXTEST_CRATE=cargo-nextest@0.9.151
ARG NEXTEST_SHA256=1d4ce302b820c41f4b89c5fb9bce45f0f0c94882c1cf08fc360caf08be6e7df4
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && cd /tmp \
 && sccache="sccache-v${SCCACHE_VERSION}-x86_64-unknown-linux-musl" \
 && curl -fsSL -o sccache.tar.gz \
      "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/${sccache}.tar.gz" \
 && echo "${SCCACHE_SHA256}  sccache.tar.gz" | sha256sum -c - \
 && tar -xzf sccache.tar.gz \
 && install -m 0755 "${sccache}/sccache" /usr/local/bin/sccache \
 && nextest="cargo-nextest-${NEXTEST_CRATE#cargo-nextest@}" \
 && curl -fsSL -o nextest.tar.gz \
      "https://github.com/nextest-rs/nextest/releases/download/${nextest}/${nextest}-x86_64-unknown-linux-gnu.tar.gz" \
 && echo "${NEXTEST_SHA256}  nextest.tar.gz" | sha256sum -c - \
 && tar -xzf nextest.tar.gz -C /usr/local/bin cargo-nextest \
 && rm -rf /tmp/*

# Development and CI image: the repo is bind-mounted at /work, the target dir and the cargo
# registry live in volumes. git is needed by cargo-deny (advisory database), curl by
# scripts/check-crate-age.sh. Every cargo command in this repo runs with --locked.
FROM chef AS dev
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git postgresql-client \
 && rm -rf /var/lib/apt/lists/*
COPY --from=stockfish /usr/local/bin/stockfish /usr/local/bin/stockfish
COPY --from=buildtools /usr/local/bin/sccache /usr/local/bin/sccache
COPY --from=buildtools /usr/local/bin/cargo-nextest /usr/local/cargo/bin/cargo-nextest
ENV STOCKFISH_PATH=/usr/local/bin/stockfish
# Compiler cache (ADR 0042). SCCACHE_DIR is the external volume ci-sccache, shared by every compose
# project, so a dependency compiled in one lane is a cache hit in every other lane.
# Incremental compilation and sccache do not mix: sccache refuses to cache a crate compiled with
# -C incremental. In the dev and test profiles cargo compiles workspace members incrementally and
# registry dependencies never, so the split falls out by itself. Workspace crates stay
# incremental (fast edit-rebuild inside the lane's own CARGO_TARGET_DIR, sccache just passes
# them through), and every dependency is compiled once and then served from the shared cache.
# Only this image sets the wrapper; host builds are untouched.
ENV RUSTC_WRAPPER=/usr/local/bin/sccache \
    SCCACHE_DIR=/sccache \
    SCCACHE_CACHE_SIZE=20G
# Linker: rustc's bundled rust-lld (LLD 23) is kept on purpose. It has been the default on
# x86_64-unknown-linux-gnu since Rust 1.90. Measured in this image (ADR 0042), relinking every test
# binary after touching crates/common took about 7.8 s with rust-lld, 10.3 s with clang 14 + lld 14
# (Debian bookworm), 13.8 s with mold 2.42.1 (bookworm only ships the AGPL mold 1.10) and 18.6 s
# with GNU ld. Re-measure before adding -C linker-features=-lld or a -fuse-ld override.
WORKDIR /work/backend

# ---- Runtime image (Task 22) ----------------------------------------------------------------
# cargo-chef: `planner` reduces the workspace to a dependency recipe, so `builder` compiles every
# dependency in a layer that only changes with Cargo.toml/Cargo.lock.
FROM chef AS planner
WORKDIR /work
COPY backend backend
COPY db db
WORKDIR /work/backend
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
WORKDIR /work/backend
COPY --from=planner /work/backend/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY backend /work/backend
COPY db /work/db
RUN cargo build --release --locked -p worker -p cli

# The worker: a digest-pinned Debian base (bookworm-slim of 2026-11-04), CA roots for the platform
# clients' TLS, the checksum-pinned Stockfish 19 from the `stockfish` stage, and both binaries.
# It runs as an unprivileged user, publishes no ports, and carries no CONTACT_EMAIL (ADR 0043).
FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS worker
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=stockfish /usr/local/bin/stockfish /usr/local/bin/stockfish
COPY --from=builder /work/backend/target/release/worker /usr/local/bin/worker
COPY --from=builder /work/backend/target/release/cli /usr/local/bin/cli
ENV STOCKFISH_PATH=/usr/local/bin/stockfish
USER 10001:10001
ENTRYPOINT ["worker"]
