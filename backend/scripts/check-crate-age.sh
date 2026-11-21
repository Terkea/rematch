#!/bin/sh
# Supply-chain rule (ADR 0031): every crate version must have been published on crates.io at
# least MIN_AGE_DAYS (default 7) days ago. This checks every registry package in Cargo.lock and
# the cargo tools pinned in the Dockerfile (`cargo-<name>@<version>`).
#
# Usage (from backend/): scripts/check-crate-age.sh [Cargo.lock] [Dockerfile]
#
# Cache (ADR 0042): publish dates never change, so each crate@version is looked up once and kept
# as one file in $CRATE_AGE_CACHE. In rust-dev that is the external volume ci-crate-age
# (/crate-age), shared by every lane and kept across `down -v`; only versions no lane has seen
# reach crates.io. Fallback outside rust-dev: $CARGO_TARGET_DIR/crate-age-cache.
#
# Network: one request per second (crates.io crawler policy), the User-Agent below and nothing
# else identifying. A 429, a 5xx or a network error is retried with exponential backoff plus
# jitter, honouring Retry-After, within CRATE_AGE_RETRY_BUDGET seconds (default 60) per crate.
# Any other answer, or an exhausted budget, fails closed (exit 2).
#
# CRATE_AGE_OFFLINE=1 makes no network call at all: every version must already be cached, and a
# missing one fails closed. scripts/test-check-crate-age.sh uses it to prove cache hits.
# Needs curl and GNU date and sleep (all in the rust-dev image).
set -eu

LOCK=${1:-Cargo.lock}
DOCKERFILE=${2:-Dockerfile}
MIN_AGE_DAYS=${MIN_AGE_DAYS:-7}
CACHE=${CRATE_AGE_CACHE:-${CARGO_TARGET_DIR:-target}/crate-age-cache}
OFFLINE=${CRATE_AGE_OFFLINE:-0}
RETRY_BUDGET=${CRATE_AGE_RETRY_BUDGET:-60}
API=${CRATE_AGE_API:-https://crates.io/api/v1/crates}
UA="chess-improver-ci (+https://github.com/chess-improver)"

valid_timestamp() {
  printf '%s' "$1" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+(Z|[+-][0-9:]+)$' \
    && date -u -d "$1" +%s > /dev/null 2>&1
}

# A random 0-999, for jitter (POSIX sh has no $RANDOM).
jitter_ms() {
  n=$(od -An -N2 -tu2 /dev/urandom | tr -d ' ')
  echo $((n % 1000))
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Prints the created_at of $1 $2, or returns 1 (with the reason on stderr).
fetch_created() {
  start=$(date -u +%s)
  attempt=0
  while :; do
    attempt=$((attempt + 1))
    status=$(curl -sS --max-time 20 -A "$UA" -o "$work/body" -D "$work/headers" \
      -w '%{http_code}' "$API/$1/$2" 2> "$work/err") || status=000
    case $status in
      200)
        created=$(grep -o '"created_at":"[^"]*"' "$work/body" | head -n 1 | cut -d'"' -f4) || true
        if [ -n "$created" ] && valid_timestamp "$created"; then
          printf '%s\n' "$created"
          return 0
        fi
        echo "error: $1 $2: crates.io answered 200 without a valid created_at" >&2
        return 1
        ;;
      429 | 5?? | 000) ;;
      *)
        echo "error: $1 $2: crates.io answered HTTP $status" >&2
        return 1
        ;;
    esac
    # Retry-After in seconds if the server sent one (the HTTP-date form falls back to backoff),
    # else 1, 2, 4, 8, ... seconds; plus up to 1 s of jitter so parallel lanes spread out.
    wait_s=$(tr -d '\r' < "$work/headers" 2> /dev/null \
      | sed -n 's/^[Rr]etry-[Aa]fter: *\([0-9][0-9]*\) *$/\1/p' | tail -n 1)
    [ -n "$wait_s" ] || wait_s=$((1 << (attempt - 1)))
    ms=$(jitter_ms)
    elapsed=$(( $(date -u +%s) - start ))
    if [ $((elapsed + wait_s + 1)) -gt "$RETRY_BUDGET" ]; then
      echo "error: $1 $2: crates.io still answering HTTP $status ($(head -c 200 "$work/err"))" \
        "after $attempt attempts and ${elapsed}s (budget ${RETRY_BUDGET}s)" >&2
      return 1
    fi
    echo "crate-age: $1 $2: HTTP $status, retry $attempt in ${wait_s}.$(printf '%03d' "$ms")s" >&2
    sleep "${wait_s}.$(printf '%03d' "$ms")"
  done
}

mkdir -p "$CACHE"
cutoff=$(( $(date -u +%s) - MIN_AGE_DAYS * 86400 ))

list=$work/list
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
  # A cache entry is one RFC 3339 timestamp that GNU date can parse; anything else is refetched.
  if [ -s "$entry" ] && ! valid_timestamp "$(cat "$entry")"; then
    echo "warning: corrupt cache entry $entry ($(head -c 80 "$entry")), refetching" >&2
    rm -f "$entry"
  fi
  if [ ! -s "$entry" ]; then
    if [ "$OFFLINE" = 1 ]; then
      echo "error: $name $version is not cached and CRATE_AGE_OFFLINE=1" >&2
      exit 2
    fi
    [ "$fetched" -eq 0 ] || sleep 1 # crates.io crawler policy: at most one request per second
    if ! created=$(fetch_created "$name" "$version"); then
      echo "error: could not read the publish date of $name $version from crates.io" >&2
      exit 2
    fi
    # Write then rename, so a lane running at the same time never reads a half-written entry.
    tmp="$entry.tmp.$$"
    printf '%s\n' "$created" > "$tmp"
    mv -f "$tmp" "$entry"
    fetched=$((fetched + 1))
  fi
  created=$(cat "$entry")
  if [ "$(date -u -d "$created" +%s)" -gt "$cutoff" ]; then
    echo "too new: $name $version (published $created, minimum age $MIN_AGE_DAYS days)"
    young=$((young + 1))
  fi
done < "$list"

echo "crate-age: $total crates checked ($fetched fetched, $((total - fetched)) cached), $young too new"
[ "$young" -eq 0 ]
