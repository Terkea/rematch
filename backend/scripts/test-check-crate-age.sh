#!/bin/sh
# Tests for scripts/check-crate-age.sh without touching crates.io: a fake `curl` on PATH records
# every call and plays back scripted answers. Run from backend/: sh scripts/test-check-crate-age.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
check="$here/check-crate-age.sh"
t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT

mkdir -p "$t/bin"
cat > "$t/bin/curl" << 'EOF'
#!/bin/sh
# Fake curl: supports -o <body>, -D <headers>, -w '%{http_code}' and a final URL. Each call takes
# the next "<status> [retry-after]" line from $FAKE_ANSWERS (default 200) and logs args to $FAKE_LOG.
body=/dev/null headers=/dev/null url=
while [ $# -gt 0 ]; do
  case $1 in
    -o) body=$2; shift ;;
    -D) headers=$2; shift ;;
    -A) printf 'UA=%s\n' "$2" >> "$FAKE_LOG"; shift ;;
    -w | --max-time) shift ;;
    -*) ;;
    *) url=$1 ;;
  esac
  shift
done
printf 'URL=%s\n' "$url" >> "$FAKE_LOG"
line=$(head -n 1 "$FAKE_ANSWERS" 2> /dev/null || true)
[ -n "$line" ] && sed -i 1d "$FAKE_ANSWERS"
set -- ${line:-200}
printf 'HTTP/2 %s\r\n' "$1" > "$headers"
[ -z "${2:-}" ] || printf 'retry-after: %s\r\n' "$2" >> "$headers"
printf '{"version":{"created_at":"2020-01-01T00:00:00.000000Z"}}' > "$body"
printf '%s' "$1"
EOF
chmod +x "$t/bin/curl"

cat > "$t/Cargo.lock" << 'EOF'
[[package]]
name = "alpha"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "local"
version = "0.1.0"
EOF

fail() { echo "FAIL: $*" >&2; exit 1; }
run() { PATH="$t/bin:$PATH" FAKE_LOG="$t/log" FAKE_ANSWERS="$t/answers" CRATE_AGE_CACHE="$t/cache" \
  sh "$check" "$t/Cargo.lock" /nonexistent "$@"; }
reset() { rm -rf "$t/cache" "$t/log" "$t/answers"; : > "$t/log"; : > "$t/answers"; }
calls() { grep -c '^URL=' "$t/log" || true; }

# 1. A cache miss queries crates.io once, with the project User-Agent and nothing else identifying.
reset
run > "$t/out" || fail "first run failed: $(cat "$t/out")"
[ "$(calls)" = 1 ] || fail "expected 1 call on a cold cache, got $(calls)"
grep -qx 'UA=rematch/0.1 (+https://github.com/Terkea/rematch)' "$t/log" || fail "wrong User-Agent"
grep -q '@' "$t/log" && fail "a request carried an email-like value: $(cat "$t/log")"
grep -q '1 fetched, 0 cached' "$t/out" || fail "unexpected summary: $(cat "$t/out")"

# 2. A cache hit makes no network call, also in offline (dry-run) mode.
: > "$t/log"
run > "$t/out" || fail "cached run failed"
[ "$(calls)" = 0 ] || fail "cache hit still called curl $(calls) times"
CRATE_AGE_OFFLINE=1 run > "$t/out" || fail "offline run with a full cache failed"
[ "$(calls)" = 0 ] || fail "offline run called curl"
grep -q '0 fetched, 1 cached' "$t/out" || fail "unexpected summary: $(cat "$t/out")"

# 3. Offline with a cold cache fails closed without calling curl.
reset
if CRATE_AGE_OFFLINE=1 run > "$t/out" 2>&1; then fail "offline cold run passed"; fi
[ "$(calls)" = 0 ] || fail "offline cold run called curl"

# 4. 429 with Retry-After, then 503, then 200: retried and succeeds.
reset
printf '429 1\n503\n200\n' > "$t/answers"
run > "$t/out" 2>&1 || fail "retry run failed: $(cat "$t/out")"
[ "$(calls)" = 3 ] || fail "expected 3 calls (429, 503, 200), got $(calls)"
grep -q 'HTTP 429, retry 1 in 1\.' "$t/out" || fail "Retry-After not honoured: $(cat "$t/out")"

# 5. Endless 429 fails closed once the retry budget is spent, and caches nothing.
reset
printf '429\n429\n429\n429\n429\n429\n' > "$t/answers"
if CRATE_AGE_RETRY_BUDGET=3 run > "$t/out" 2>&1; then fail "endless 429 passed"; fi
grep -q 'budget 3s' "$t/out" || fail "no budget error: $(cat "$t/out")"
[ ! -e "$t/cache/alpha-1.0.0" ] || fail "a failed lookup was cached"

# 6. A 404 is not retried and fails closed.
reset
printf '404\n' > "$t/answers"
if run > "$t/out" 2>&1; then fail "404 passed"; fi
[ "$(calls)" = 1 ] || fail "404 was retried"

echo "test-check-crate-age: all 6 cases passed"
