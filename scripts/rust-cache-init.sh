#!/bin/sh
# Creates the shared Rust build caches used by the rust-dev service (ADR 0042). Idempotent: an
# existing volume is left as it is. Run it once per Docker host (and in CI before compose).
#
#   ci-cargo-registry  /usr/local/cargo/registry  crates.io index and downloaded crates
#   ci-cargo-git       /usr/local/cargo/git       git dependencies
#   ci-sccache         /sccache                   sccache's compiled-artifact cache
#   ci-crate-age       /crate-age                 crates.io publish dates (check-crate-age.sh)
#
# They are `external: true` in docker-compose.yml, so no compose project owns them and
# `docker compose -p <lane> down -v` never removes them. To reset a cache on purpose:
#   docker volume rm ci-sccache && sh scripts/rust-cache-init.sh
set -eu

for volume in ci-cargo-registry ci-cargo-git ci-sccache ci-crate-age; do
  if docker volume inspect "$volume" > /dev/null 2>&1; then
    echo "rust-cache-init: $volume exists"
  else
    docker volume create --label chess-improver.cache=rust "$volume" > /dev/null
    echo "rust-cache-init: $volume created"
  fi
done
