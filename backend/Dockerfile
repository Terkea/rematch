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

# Development and CI image: the repo is bind-mounted at /work, the target dir and the cargo
# registry live in volumes. git is needed by cargo-deny (advisory database), curl by
# scripts/check-crate-age.sh. Every cargo command in this repo runs with --locked.
FROM chef AS dev
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git postgresql-client \
 && rm -rf /var/lib/apt/lists/*
COPY --from=stockfish /usr/local/bin/stockfish /usr/local/bin/stockfish
ENV STOCKFISH_PATH=/usr/local/bin/stockfish
WORKDIR /work/backend
