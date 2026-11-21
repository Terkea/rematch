#!/bin/sh
# Supply-chain rule (ADR 0031): every crate version must have been published on crates.io at
# least MIN_AGE_DAYS (default 7) days ago. This checks every registry package in Cargo.lock and
# the cargo tools pinned in the Dockerfile (`cargo install --locked name@version`).
#
# Usage (from backend/): scripts/check-crate-age.sh [Cargo.lock] [Dockerfile]
# Publish dates never change, so responses are cached in $CRATE_AGE_CACHE
# (default: $CARGO_TARGET_DIR/crate-age-cache, or target/crate-age-cache).
# Needs curl and GNU date (both in the rust-dev image). Fails closed if crates.io cannot be read.
set -eu

LOCK=${1:-Cargo.lock}
DOCKERFILE=${2:-Dockerfile}
MIN_AGE_DAYS=${MIN_AGE_DAYS:-7}
CACHE=${CRATE_AGE_CACHE:-${CARGO_TARGET_DIR:-target}/crate-age-cache}
UA="chess-improver-ci (+https://github.com/chess-improver)"

mkdir -p "$CACHE"
cutoff=$(( $(date -u +%s) - MIN_AGE_DAYS * 86400 ))

list=$(mktemp)
trap 'rm -f "$list"' EXIT

# "name version" for every [[package]] whose source is a registry (skips path/workspace crates).
awk '
  /^\[\[package\]\]/ { name = ""; version = "" }
  /^name = /    { gsub(/"/, "", $3); name = $3 }
  /^version = / { gsub(/"/, "", $3); version = $3 }
  /^source = "registry\+/ { print name, version }
' "$LOCK" > "$list"
if [ -f "$DOCKERFILE" ]; then
  grep -o 'cargo-[a-z-]*@[0-9][0-9.]*' "$DOCKERFILE" | tr '@' ' ' >> "$list" || true
fi

total=0
fetched=0
young=0
while read -r name version; do
  total=$((total + 1))
  entry="$CACHE/$name-$version"
  if [ ! -s "$entry" ]; then
    created=$(curl -fsS --retry 3 --retry-delay 2 -A "$UA" \
        "https://crates.io/api/v1/crates/$name/$version" \
      | grep -o '"created_at":"[^"]*"' | head -n 1 | cut -d'"' -f4) || true
    if [ -z "$created" ]; then
      echo "error: could not read the publish date of $name $version from crates.io" >&2
      exit 2
    fi
    printf '%s\n' "$created" > "$entry"
    fetched=$((fetched + 1))
    sleep 1 # crates.io crawler policy: at most one request per second
  fi
  created=$(cat "$entry")
  if [ "$(date -u -d "$created" +%s)" -gt "$cutoff" ]; then
    echo "too new: $name $version (published $created, minimum age $MIN_AGE_DAYS days)"
    young=$((young + 1))
  fi
done < "$list"

echo "crate-age: $total crates checked ($fetched fetched, $((total - fetched)) cached), $young too new"
[ "$young" -eq 0 ]
