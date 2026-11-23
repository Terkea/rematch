#!/bin/sh
# Tests for scripts/setup.sh and its .env logic (scripts/lib/setup-env.sh). No Docker, no network:
# `claude` and `curl` are stubbed on PATH. POSIX sh, so it runs in CI, Git Bash and macOS.
#   sh scripts/test-setup.sh
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
# shellcheck source=lib/setup-env.sh
. "$ROOT/scripts/lib/setup-env.sh"

T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT INT TERM
PASS=0
FAIL=0

ok() {
  PASS=$((PASS + 1))
  printf 'ok   %s\n' "$1"
}
no() {
  FAIL=$((FAIL + 1))
  printf 'FAIL %s\n' "$1"
  shift
  for line in "$@"; do printf '     %s\n' "$line"; done
}
eq() { # name expected actual
  if [ "$2" = "$3" ]; then ok "$1"; else no "$1" "expected: $2" "actual:   $3"; fi
}
has() { # name needle haystack
  case $3 in *"$2"*) ok "$1" ;; *) no "$1" "missing: $2" "in:      $3" ;; esac
}
lacks() { # name needle haystack
  case $3 in *"$2"*) no "$1" "unexpected: $2" ;; *) ok "$1" ;; esac
}

EX="$T/example.env"
cat > "$EX" << 'EOF'
# a comment
COMPOSE_FILE=docker-compose.yml:docker-compose.ports.yml
COMPOSE_PATH_SEPARATOR=:
DATABASE_URL=postgres://postgres:postgres@localhost:55432/chess
BETTER_AUTH_SECRET=
ALLOW_UNVERIFIED_LINKS=
INVITE_ALLOWLIST=
ANTHROPIC_API_KEY=
NEW_KEY=from-example
EOF

# --- env_get / env_set ---------------------------------------------------------------------------
F="$T/a.env"
printf 'A=1\nB=x=y\nC="quoted value"\nD=\n' > "$F"
eq "env_get reads a plain value" "1" "$(env_get "$F" A)"
eq "env_get keeps = inside a value" "x=y" "$(env_get "$F" B)"
eq "env_get strips surrounding quotes" "quoted value" "$(env_get "$F" C)"
eq "env_get of an empty value is empty" "" "$(env_get "$F" D)"
if env_get "$F" MISSING > /dev/null; then no "env_get fails for a missing key"; else ok "env_get fails for a missing key"; fi
env_set "$F" A 'a&b\c/d|e'
eq "env_set replaces with special characters intact" 'a&b\c/d|e' "$(env_get "$F" A)"
env_set "$F" E new
eq "env_set appends a missing key" "new" "$(env_get "$F" E)"
eq "env_set keeps one line per key" "1" "$(grep -c '^A=' "$F")"
env_default "$F" B other
eq "env_default keeps a set value" "x=y" "$(env_get "$F" B)"
env_default "$F" D filled
eq "env_default fills an empty value" "filled" "$(env_get "$F" D)"

# --- env_merge -----------------------------------------------------------------------------------
M="$T/m.env"
env_merge "$EX" "$M" > /dev/null
eq "env_merge copies the example when there is no .env" "$(cat "$EX")" "$(cat "$M")"
printf 'DATABASE_URL=postgres://custom\nMY_OWN=1\n' > "$M"
env_merge "$EX" "$M" > /dev/null
eq "env_merge never overwrites a value" "postgres://custom" "$(env_get "$M" DATABASE_URL)"
eq "env_merge keeps the user's own keys" "1" "$(env_get "$M" MY_OWN)"
eq "env_merge adds the example's missing keys" "from-example" "$(env_get "$M" NEW_KEY)"
before=$(cat "$M")
env_merge "$EX" "$M" > /dev/null
eq "env_merge is idempotent" "$before" "$(cat "$M")"

# --- COMPOSE_FILE and the allowlist --------------------------------------------------------------
C="$T/c.env"
printf 'COMPOSE_FILE=docker-compose.yml:docker-compose.ports.yml\n' > "$C"
compose_add "$C" docker-compose.local-claude.yml
compose_add "$C" docker-compose.local-claude.yml
eq "compose_add adds an overlay once" "docker-compose.yml:docker-compose.ports.yml:docker-compose.local-claude.yml" "$(env_get "$C" COMPOSE_FILE)"
compose_remove "$C" docker-compose.local-claude.yml
eq "compose_remove drops it" "docker-compose.yml:docker-compose.ports.yml" "$(env_get "$C" COMPOSE_FILE)"
printf 'COMPOSE_FILE=\n' > "$C"
compose_add "$C" docker-compose.maia.yml
eq "compose_add starts from the base file" "docker-compose.yml:docker-compose.maia.yml" "$(env_get "$C" COMPOSE_FILE)"

allowlist_add "$C" Me@Example.COM
allowlist_add "$C" me@example.com
allowlist_add "$C" two@example.com
eq "allowlist_add lowercases and dedupes" "me@example.com,two@example.com" "$(env_get "$C" INVITE_ALLOWLIST)"

h1=$(random_hex 32)
h2=$(random_hex 32)
if printf '%s' "$h1" | grep -Eq '^[0-9a-f]{64}$'; then ok "random_hex 32 is 64 hex digits"; else no "random_hex 32 is 64 hex digits" "$h1"; fi
if [ "$h1" != "$h2" ]; then ok "random_hex differs between calls"; else no "random_hex differs between calls"; fi

# --- the numbers the checks compare --------------------------------------------------------------
if version_ge 2.24.0 2.24.0 && version_ge 5.0.0-desktop.1 2.24.0 && ! version_ge 2.23.9 2.24.0; then
  ok "version_ge compares dotted versions"
else
  no "version_ge compares dotted versions"
fi
if month_has_clocks 2017-04 && month_has_clocks 2026-10 && ! month_has_clocks 2017-03 && ! month_has_clocks 2013-01; then
  ok "month_has_clocks starts at 2017-04"
else
  no "month_has_clocks starts at 2017-04"
fi
eq "gib rounds bytes down to GiB" "15" "$(gib 16265678848)"

# --- the setup.sh CLI (configure only writes .env) -----------------------------------------------
STUB="$T/bin"
mkdir -p "$STUB"
cat > "$STUB/claude" << 'EOF'
#!/bin/sh
[ "$1 $2" = "auth status" ] && echo '{"loggedIn": true, "authMethod": "claude.ai"}'
exit 0
EOF
cat > "$STUB/curl" << 'EOF'
#!/bin/sh
# Stub: the key check asks for the HTTP status only.
printf '200'
EOF
chmod +x "$STUB/claude" "$STUB/curl"

run_setup() { # env file, args...
  envf=$1
  shift
  PATH="$STUB:$PATH" SETUP_ENV_FILE="$envf" SETUP_EXAMPLE_FILE="$ROOT/.env.example" \
    sh "$ROOT/scripts/setup.sh" "$@"
}

E="$T/fresh.env"
out=$(run_setup "$E" configure --yes --email=Player@Example.com --ai=none 2>&1)
eq "configure: unverified links on" "1" "$(env_get "$E" ALLOW_UNVERIFIED_LINKS)"
eq "configure: the email is invited" "player@example.com" "$(env_get "$E" INVITE_ALLOWLIST)"
if env_get "$E" BETTER_AUTH_SECRET | grep -Eq '^[0-9a-f]{64}$'; then
  ok "configure: a random 64-hex auth secret"
else
  no "configure: a random 64-hex auth secret" "$(env_get "$E" BETTER_AUTH_SECRET | cut -c1-8)..."
fi
eq "configure: the ports overlay" "docker-compose.yml:docker-compose.ports.yml" "$(env_get "$E" COMPOSE_FILE)"
eq "configure: the compose path separator" ":" "$(env_get "$E" COMPOSE_PATH_SEPARATOR)"
eq "configure: AI mode recorded" "none" "$(env_get "$E" SETUP_AI_MODE)"
lacks "configure: the secret is never printed" "$(env_get "$E" BETTER_AUTH_SECRET)" "$out"
first=$(cat "$E")
run_setup "$E" configure --yes --email=player@example.com --ai=none > /dev/null 2>&1
eq "configure: a second run changes nothing" "$first" "$(cat "$E")"

# A value the user chose survives.
env_set "$E" ALLOW_UNVERIFIED_LINKS 0
env_set "$E" BETTER_AUTH_SECRET my-own-secret-that-is-long-enough-000000
run_setup "$E" configure --yes --ai=none > /dev/null 2>&1
eq "configure: keeps the user's ALLOW_UNVERIFIED_LINKS" "0" "$(env_get "$E" ALLOW_UNVERIFIED_LINKS)"
eq "configure: keeps the user's secret" "my-own-secret-that-is-long-enough-000000" "$(env_get "$E" BETTER_AUTH_SECRET)"
eq "configure: no --email keeps the invited list" "player@example.com" "$(env_get "$E" INVITE_ALLOWLIST)"

# Non-interactive without any email: a reserved .test address, so the stack is still usable.
N="$T/noemail.env"
out=$(run_setup "$N" configure --yes --ai=none 2>&1)
eq "configure: default email when none is given" "player@rematch.test" "$(env_get "$N" INVITE_ALLOWLIST)"
has "configure: says which email it invited" "player@rematch.test" "$out"

# AI: local adds the overlay once and removing it is clean.
L="$T/local.env"
run_setup "$L" configure --yes --ai=local > /dev/null 2>&1
run_setup "$L" configure --yes --ai=local > /dev/null 2>&1
eq "ai local: overlay added once" "docker-compose.yml:docker-compose.ports.yml:docker-compose.local-claude.yml" "$(env_get "$L" COMPOSE_FILE)"
run_setup "$L" configure --yes --ai=none > /dev/null 2>&1
eq "ai none: overlay removed" "docker-compose.yml:docker-compose.ports.yml" "$(env_get "$L" COMPOSE_FILE)"

# AI: local refuses without a login and says how to fix it.
cat > "$STUB/claude" << 'EOF'
#!/bin/sh
[ "$1 $2" = "auth status" ] && echo '{"loggedIn": false}'
exit 1
EOF
if out=$(run_setup "$T/nologin.env" configure --yes --ai=local 2>&1); then
  no "ai local: refuses when claude is not logged in"
else
  ok "ai local: refuses when claude is not logged in"
fi
has "ai local: explains /login" "/login" "$out"

# AI: key, read from stdin (no tty), stored only in .env, never printed.
K="$T/key.env"
KEY="sk-ant-test-$(random_hex 8)"
out=$(printf '%s\n' "$KEY" | run_setup "$K" configure --yes --ai=key 2>&1)
eq "ai key: stored in .env" "$KEY" "$(env_get "$K" ANTHROPIC_API_KEY)"
lacks "ai key: never printed" "$KEY" "$out"
has "ai key: checked" "key accepted" "$out"
out=$(ANTHROPIC_API_KEY="$KEY-2" run_setup "$K" configure --yes --ai=key 2>&1 < /dev/null)
eq "ai key: taken from the environment" "$KEY-2" "$(env_get "$K" ANTHROPIC_API_KEY)"
lacks "ai key: the environment key is never printed" "$KEY-2" "$out"
cat > "$STUB/curl" << 'EOF'
#!/bin/sh
printf '401'
EOF
if out=$(printf 'sk-ant-bad\n' | run_setup "$T/bad.env" configure --yes --ai=key 2>&1); then
  no "ai key: a rejected key fails"
else
  ok "ai key: a rejected key fails"
fi
lacks "ai key: the rejected key is never printed" "sk-ant-bad" "$out"
run_setup "$K" configure --yes --ai=none > /dev/null 2>&1
eq "ai none: clears the key" "" "$(env_get "$K" ANTHROPIC_API_KEY)"

# Ports, project and Maia.
P="$T/ports.env"
run_setup "$P" configure --yes --ai=none --project=ci-x --web-port=3101 --postgres-port=55440 --maia > /dev/null 2>&1
eq "--project writes COMPOSE_PROJECT_NAME" "ci-x" "$(env_get "$P" COMPOSE_PROJECT_NAME)"
eq "--web-port writes WEB_PORT" "3101" "$(env_get "$P" WEB_PORT)"
eq "--web-port moves BETTER_AUTH_URL" "http://localhost:3101" "$(env_get "$P" BETTER_AUTH_URL)"
eq "--postgres-port moves the host DATABASE_URL" "postgres://postgres:postgres@localhost:55440/chess" "$(env_get "$P" DATABASE_URL)"
has "--maia adds the Maia overlay" "docker-compose.maia.yml" "$(env_get "$P" COMPOSE_FILE)"
run_setup "$P" configure --yes --ai=none --no-ports > /dev/null 2>&1
lacks "--no-ports drops the ports overlay" "docker-compose.ports.yml" "$(env_get "$P" COMPOSE_FILE)"
has "--no-ports keeps Maia" "docker-compose.maia.yml" "$(env_get "$P" COMPOSE_FILE)"

if out=$(run_setup "$T/bad-ai.env" configure --yes --ai=maybe 2>&1); then
  no "an unknown --ai value fails"
else
  ok "an unknown --ai value fails"
fi
has "an unknown --ai value says the choices" "local, key or none" "$out"
if out=$(run_setup "$T/bad-month.env" configure --yes --ai=none --month=2013-13 2>&1); then
  no "a bad --month fails"
else
  ok "a bad --month fails"
fi

out=$(run_setup "$T/help.env" --help 2>&1)
has "--help lists the modes" "minimal" "$out"
has "--help lists the AI flag" "--ai=local|key|none" "$out"

printf '\n%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
