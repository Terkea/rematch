#!/bin/sh
# One command that sets up all of Rematch on this machine: the .env, the Docker stack, the data
# it needs and the AI choice. POSIX sh: Git Bash on Windows, macOS and Linux. It never installs
# anything with sudo; it says what to install. Re-running it repairs what is missing and skips
# what is done. `sh scripts/setup.sh --help` lists everything.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
# shellcheck source=lib/setup-env.sh
. "$ROOT/scripts/lib/setup-env.sh"

ENV_FILE=${SETUP_ENV_FILE:-$ROOT/.env}
# Git Bash rewrites arguments that look like paths (/data/...) before Docker sees them, so dc()
# turns that off for Docker only (pnpm's shim needs it) and passes the checkout as a Windows path.
DOCKER_ROOT=$(cd "$ROOT" && pwd -W 2> /dev/null || pwd)
EXAMPLE=${SETUP_EXAMPLE_FILE:-$ROOT/.env.example}
CACHE_DIR="$ROOT/.cache"
PIDFILE="$CACHE_DIR/ai-sidecar.pid"
SIDECAR_LOG="$CACHE_DIR/ai-sidecar.log"
SIDECAR_URL="http://127.0.0.1:7310"
DEFAULT_EMAIL="player@rematch.test"
DEFAULT_MONTH="2013-01"
# Lichess dumps carry [%clk] comments from this month on; the band reference needs them.
FIRST_CLOCK_MONTH="2017-04"
SYZYGY_FILES=290
# The dry run measured 3.3 GB of images and 1.8 GB of volumes, plus the Rust build cache; full
# mode adds 940 MiB of Syzygy tables and the explorer rows.
MIN_MEMORY_GIB=12
DISK_MINIMAL_GIB=10
DISK_FULL_GIB=12
DISK_MAIA_GIB=6

usage() {
  cat << 'EOF'
Usage: sh scripts/setup.sh [command] [options]

Commands:
  minimal       (default) .env, the Docker stack, no large downloads. Pair with `pnpm seed:demo`.
  full          minimal, plus Syzygy 3-4-5 (about 1 GB), a Lichess month for the opening explorer
                and the band reference, and Maia when --maia is given.
  configure     only write .env (merged with .env.example; your values are kept).
  doctor        re-check everything and explain each failure (also: --doctor).
  data          fetch or build the data only (Syzygy, the Lichess month, Maia).
  ai            switch the AI mode (menu, or --ai=...), then restart the worker.
  ai-start | ai-stop | ai-status
                manage the host AI sidecar of --ai=local (pidfile .cache/ai-sidecar.pid).

Options:
  -y, --yes             never ask: take the defaults (also when stdin is not a terminal).
  --email=ADDRESS       the address you sign in with; it is added to INVITE_ALLOWLIST.
  --ai=local|key|none   local: your own signed-in Claude Code (for you only, never to serve others)
                        key: an Anthropic API key (asked hidden, or $ANTHROPIC_API_KEY), only in .env
                        none: computed reports only (switch later with `sh scripts/setup.sh ai`)
  --month=YYYY-MM       the Lichess month for full mode (default 2013-01, 18 MB, 121k games).
  --no-lichess          full mode without the Lichess month.
  --maia | --no-maia    the Maia-2 difficulty service (full mode).
  --project=NAME        the Compose project name (written to .env as COMPOSE_PROJECT_NAME).
  --no-ports | --ports  publish no host ports, or the default ones again.
  --web-port=N --mailpit-port=N --postgres-port=N --redis-port=N
                        host ports (defaults 3100, 48025, 55432, 56379).
  -h, --help            this text.

Examples:
  sh scripts/setup.sh                                  # asks a few questions
  sh scripts/setup.sh minimal --yes --ai=none --email=you@example.com && pnpm seed:demo
  sh scripts/setup.sh full --ai=local --maia
EOF
}

# --- output --------------------------------------------------------------------------------------
say() { printf '%s\n' "$*"; }
step() { printf '\n==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() {
  printf 'setup: %s\n' "$*" >&2
  exit 1
}
now() { date +%s; }
took() { say "    done in $(($(now) - $1)) s"; }

# --- arguments -----------------------------------------------------------------------------------
CMD=""
YES=0
EMAIL=""
AI=""
MONTH=""
LICHESS=1
MAIA=""
PROJECT=""
PORTS=""
WEB_PORT_ARG=""
MAILPIT_PORT_ARG=""
POSTGRES_PORT_ARG=""
REDIS_PORT_ARG=""

for arg in "$@"; do
  case $arg in
    minimal | full | configure | doctor | data | ai | ai-start | ai-stop | ai-status) CMD=$arg ;;
    --doctor) CMD=doctor ;;
    -y | --yes | --non-interactive) YES=1 ;;
    --email=*) EMAIL=${arg#*=} ;;
    --ai=*) AI=${arg#*=} ;;
    --month=*) MONTH=${arg#*=} ;;
    --no-lichess) LICHESS=0 ;;
    --maia) MAIA=1 ;;
    --no-maia) MAIA=0 ;;
    --project=*) PROJECT=${arg#*=} ;;
    --no-ports) PORTS=0 ;;
    --ports) PORTS=1 ;;
    --web-port=*) WEB_PORT_ARG=${arg#*=} ;;
    --mailpit-port=*) MAILPIT_PORT_ARG=${arg#*=} ;;
    --postgres-port=*) POSTGRES_PORT_ARG=${arg#*=} ;;
    --redis-port=*) REDIS_PORT_ARG=${arg#*=} ;;
    -h | --help)
      usage
      exit 0
      ;;
    *) die "unknown argument: $arg (see --help)" ;;
  esac
done

INTERACTIVE=0
if [ "$YES" -eq 0 ] && [ -t 0 ]; then INTERACTIVE=1; fi

case $AI in
  "" | local | key | none) ;;
  *) die "--ai must be local, key or none (got \"$AI\")" ;;
esac
if [ -n "$MONTH" ] && ! printf '%s' "$MONTH" | grep -Eq '^[0-9]{4}-(0[1-9]|1[0-2])$'; then
  die "--month must be YYYY-MM (got \"$MONTH\")"
fi
for p in "$WEB_PORT_ARG" "$MAILPIT_PORT_ARG" "$POSTGRES_PORT_ARG" "$REDIS_PORT_ARG"; do
  if [ -n "$p" ] && ! printf '%s' "$p" | grep -Eq '^[0-9]{2,5}$'; then die "not a port: $p"; fi
done

ask() { # question default -> answer (the default when not interactive)
  if [ "$INTERACTIVE" -eq 1 ]; then
    printf '%s [%s]: ' "$1" "$2" >&2
    IFS= read -r _answer || _answer=""
    [ -n "$_answer" ] || _answer=$2
    printf '%s' "$_answer"
  else
    printf '%s' "$2"
  fi
}

# --- docker --------------------------------------------------------------------------------------
dc() {
  if [ -n "${SETUP_ENV_FILE:-}" ]; then
    MSYS_NO_PATHCONV=1 docker compose --project-directory "$DOCKER_ROOT" --env-file "$ENV_FILE" "$@"
  else
    MSYS_NO_PATHCONV=1 docker compose --project-directory "$DOCKER_ROOT" "$@"
  fi
}

setting() { env_get "$ENV_FILE" "$1" 2> /dev/null || true; }
port_of() { # key default
  _v=$(setting "$1")
  [ -n "$_v" ] || _v=$2
  printf '%s' "$_v"
}
ai_mode() {
  _m=$(setting SETUP_AI_MODE)
  [ -n "$_m" ] || _m=none
  printf '%s' "$_m"
}

# --- .env ----------------------------------------------------------------------------------------
follow_port() { # key new-port: moves localhost:<port> in a host URL to the new port
  _cur=$(setting "$1")
  case $_cur in
    *localhost:[0-9]*)
      _new=$(printf '%s' "$_cur" | sed "s/localhost:[0-9][0-9]*/localhost:$2/")
      env_set "$ENV_FILE" "$1" "$_new"
      ;;
  esac
}

read_key() {
  KEY_VALUE=""
  if [ -n "${ANTHROPIC_API_KEY:-}" ]; then
    KEY_VALUE=$ANTHROPIC_API_KEY
    say "    using ANTHROPIC_API_KEY from the environment"
    return 0
  fi
  _have=$(setting ANTHROPIC_API_KEY)
  if [ -n "$_have" ] && [ "$INTERACTIVE" -eq 0 ]; then
    KEY_VALUE=$_have
    say "    keeping the key already in .env"
    return 0
  fi
  if [ -t 0 ]; then
    if [ -n "$_have" ] && [ "$(ask 'Keep the API key already in .env? (y/n)' y)" = y ]; then
      KEY_VALUE=$_have
      return 0
    fi
    printf 'Anthropic API key (input hidden): ' >&2
    stty -echo 2> /dev/null || true
    trap 'stty echo 2> /dev/null || true' EXIT INT TERM
    IFS= read -r KEY_VALUE || KEY_VALUE=""
    stty echo 2> /dev/null || true
    trap - EXIT INT TERM
    printf '\n' >&2
  else
    IFS= read -r KEY_VALUE || KEY_VALUE=""
  fi
  [ -n "$KEY_VALUE" ] || die "no API key given (pass it on stdin, or export ANTHROPIC_API_KEY)"
}

# A models-list call: it proves the key without spending a token. The key goes to curl in a
# private header file, never on its command line.
check_key() {
  _hdr=$(mktemp)
  chmod 600 "$_hdr"
  printf 'x-api-key: %s\nanthropic-version: 2023-06-01\n' "$KEY_VALUE" > "$_hdr"
  _code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 20 -H "@$_hdr" \
    https://api.anthropic.com/v1/models 2> /dev/null || true)
  rm -f "$_hdr"
  case $_code in
    200) say "    key accepted (a models-list call, no tokens spent)" ;;
    401 | 403) die "Anthropic rejected that API key (HTTP $_code). Nothing was written." ;;
    *) warn "could not check the key (HTTP ${_code:-none}); it is saved, check the worker log later" ;;
  esac
}

claude_logged_in() {
  command -v claude > /dev/null 2>&1 || return 1
  claude auth status --json 2> /dev/null | grep -Eq '"loggedIn"[[:space:]]*:[[:space:]]*true'
}

explain_claude() {
  if ! command -v claude > /dev/null 2>&1; then
    say "Claude Code is not on PATH. Install it (https://docs.claude.com/en/docs/claude-code), then"
  else
    say "Claude Code is installed but not signed in."
  fi
  say "run \`claude\` and type /login (or \`claude auth login\`), then re-run this command."
  say "Or choose --ai=key (an Anthropic API key) or --ai=none (computed reports only)."
}

choose_ai() {
  if [ -n "$AI" ]; then return 0; fi
  _cur=$(setting SETUP_AI_MODE)
  if [ "$INTERACTIVE" -eq 0 ]; then
    AI=${_cur:-none}
    return 0
  fi
  cat >&2 << 'EOF'

How should the coach write its reports and answers?
  1) local  your own signed-in Claude Code on this machine, no key (for your own use only)
  2) key    an Anthropic API key, kept only in .env (git-ignored)
  3) none   computed reports only; switch any time with: sh scripts/setup.sh ai
EOF
  case $(ask 'Choose 1, 2 or 3' "$(case ${_cur:-none} in local) echo 1 ;; key) echo 2 ;; *) echo 3 ;; esac)") in
    1 | local) AI=local ;;
    2 | key) AI=key ;;
    *) AI=none ;;
  esac
}

apply_ai() {
  case $AI in
    local)
      if ! claude_logged_in; then
        explain_claude >&2
        die "--ai=local needs a signed-in Claude Code"
      fi
      compose_add "$ENV_FILE" docker-compose.local-claude.yml
      say "    AI: local Claude Code login. For your own local use only, never to serve other people."
      ;;
    key)
      read_key
      check_key
      env_set "$ENV_FILE" ANTHROPIC_API_KEY "$KEY_VALUE"
      KEY_VALUE=""
      compose_remove "$ENV_FILE" docker-compose.local-claude.yml
      say "    AI: Anthropic API key, saved in .env"
      ;;
    none)
      if [ -n "$(setting ANTHROPIC_API_KEY)" ]; then
        env_set "$ENV_FILE" ANTHROPIC_API_KEY ""
        say "    removed ANTHROPIC_API_KEY from .env (AI off)"
      fi
      compose_remove "$ENV_FILE" docker-compose.local-claude.yml
      say "    AI: off. Reports are computed only."
      ;;
  esac
  env_set "$ENV_FILE" SETUP_AI_MODE "$AI"
}

configure() {
  step "Writing $(basename "$ENV_FILE") (your values are kept)"
  [ -f "$EXAMPLE" ] || die "missing $EXAMPLE"
  _merged=$(env_merge "$EXAMPLE" "$ENV_FILE")
  case $_merged in
    created) say "    created from .env.example" ;;
    0) say "    already has every key of .env.example" ;;
    *) say "    added $_merged new keys from .env.example" ;;
  esac
  env_set "$ENV_FILE" COMPOSE_PATH_SEPARATOR ":"
  env_default "$ENV_FILE" ALLOW_UNVERIFIED_LINKS 1
  env_default "$ENV_FILE" BETTER_AUTH_SECRET "$(random_hex 32)"

  _list=$(setting INVITE_ALLOWLIST)
  if [ -z "$EMAIL" ] && [ -z "$_list" ]; then
    EMAIL=$(ask 'Your email, for the sign-in link (mail stays on this machine, in Mailpit)' "$DEFAULT_EMAIL")
  fi
  if [ -n "$EMAIL" ]; then
    case $EMAIL in *@*.*) ;; *) die "not an email address: $EMAIL" ;; esac
    allowlist_add "$ENV_FILE" "$EMAIL"
  fi
  say "    invited: $(setting INVITE_ALLOWLIST)"

  [ -z "$PROJECT" ] || env_set "$ENV_FILE" COMPOSE_PROJECT_NAME "$PROJECT"
  [ -z "$PORTS" ] || env_set "$ENV_FILE" SETUP_PORTS "$PORTS"
  if [ "$(setting SETUP_PORTS)" = 0 ]; then
    compose_remove "$ENV_FILE" docker-compose.ports.yml
    say "    host ports: none published"
  else
    compose_add "$ENV_FILE" docker-compose.ports.yml
  fi
  if [ -n "$WEB_PORT_ARG" ]; then
    env_set "$ENV_FILE" WEB_PORT "$WEB_PORT_ARG"
    follow_port BETTER_AUTH_URL "$WEB_PORT_ARG"
  fi
  [ -z "$MAILPIT_PORT_ARG" ] || env_set "$ENV_FILE" MAILPIT_PORT "$MAILPIT_PORT_ARG"
  if [ -n "$POSTGRES_PORT_ARG" ]; then
    env_set "$ENV_FILE" POSTGRES_PORT "$POSTGRES_PORT_ARG"
    for k in DATABASE_URL DATABASE_URL_TEST WORKER_DATABASE_URL_TEST WEB_DATABASE_URL_TEST; do
      follow_port "$k" "$POSTGRES_PORT_ARG"
    done
  fi
  if [ -n "$REDIS_PORT_ARG" ]; then
    env_set "$ENV_FILE" REDIS_PORT "$REDIS_PORT_ARG"
    follow_port REDIS_URL "$REDIS_PORT_ARG"
  fi
  [ -z "$MONTH" ] || env_set "$ENV_FILE" SETUP_LICHESS_MONTH "$MONTH"
  case $MAIA in
    1) compose_add "$ENV_FILE" docker-compose.maia.yml ;;
    0) compose_remove "$ENV_FILE" docker-compose.maia.yml ;;
  esac
  # Apple Silicon: the images pin x86-64 Stockfish, so Docker runs them emulated.
  if [ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ]; then
    env_default "$ENV_FILE" DOCKER_DEFAULT_PLATFORM linux/amd64
  fi
  choose_ai
  apply_ai
  say "    COMPOSE_FILE=$(setting COMPOSE_FILE)"
}

# --- checks --------------------------------------------------------------------------------------
PROBLEMS=0
problem() {
  PROBLEMS=$((PROBLEMS + 1))
  printf '  FAIL  %s\n' "$1"
  shift
  for line in "$@"; do printf '        %s\n' "$line"; done
}
fine() { printf '  ok    %s\n' "$*"; }
note() { printf '  warn  %s\n' "$*"; }

stack_running() { [ -n "$(dc ps --status running -q 2> /dev/null || true)" ]; }

port_free() { # 0 when nothing answers on 127.0.0.1:port (curl exit 7: connection refused)
  _rc=0
  curl -s -o /dev/null --max-time 2 "http://127.0.0.1:$1/" > /dev/null 2>&1 || _rc=$?
  [ "$_rc" -eq 7 ]
}

check_prereqs() { # mode
  _mode=$1
  step "Checking prerequisites ($_mode)"
  if ! command -v docker > /dev/null 2>&1; then
    problem "Docker is not installed" \
      "Windows/macOS: Docker Desktop, https://docs.docker.com/desktop/" \
      "Linux: Docker Engine and the compose plugin, https://docs.docker.com/engine/install/"
    return 0
  fi
  if ! docker info > /dev/null 2>&1; then
    problem "Docker is installed but not running" "Start Docker Desktop (or: sudo systemctl start docker)."
    return 0
  fi
  _dv=$(docker version --format '{{.Server.Version}}' 2> /dev/null || echo 0)
  if version_ge "$_dv" 24.0.0; then fine "Docker $_dv"; else problem "Docker $_dv is too old" "Rematch needs Docker 24 or newer."; fi
  _cv=$(docker compose version --short 2> /dev/null || echo 0)
  if version_ge "$_cv" 2.24.0; then
    fine "Docker Compose $_cv"
  else
    problem "Docker Compose ${_cv} is too old or missing" "Rematch needs Compose 2.24 or newer (docker compose, not docker-compose)."
  fi
  _mem=$(docker info --format '{{.MemTotal}}' 2> /dev/null || echo 0)
  _memg=$(gib "$_mem")
  if [ "$_memg" -ge "$MIN_MEMORY_GIB" ]; then
    fine "Docker memory ${_memg} GiB"
  else
    note "Docker has ${_memg} GiB of memory; ${MIN_MEMORY_GIB} GiB or more is recommended (Docker Desktop: Settings > Resources; WSL 2: memory= in %UserProfile%\\.wslconfig). It may still work, slowly."
  fi
  _need=$DISK_MINIMAL_GIB
  [ "$_mode" != full ] || _need=$DISK_FULL_GIB
  if [ "$_mode" = full ] && compose_has "$ENV_FILE" docker-compose.maia.yml; then _need=$((_need + DISK_MAIA_GIB)); fi
  _free=$(df -Pk "$ROOT" 2> /dev/null | awk 'NR == 2 { printf "%d", $4 / 1048576 }')
  if [ -n "$_free" ] && [ "$_free" -lt "$_need" ]; then
    note "${_free} GiB free on this disk; $_mode mode needs about ${_need} GiB (images, volumes, data)."
  else
    fine "disk: ${_free:-?} GiB free (about ${_need} GiB needed)"
  fi
  if command -v node > /dev/null 2>&1 && version_ge "$(node --version)" 22.0.0; then
    fine "Node $(node --version)"
  else
    note "Node 22 is not on PATH. The stack runs without it; host tools and --ai=local need it (see .nvmrc)."
  fi
  if command -v pnpm > /dev/null 2>&1 && version_ge "$(pnpm --version)" 12.0.0; then
    fine "pnpm $(pnpm --version)"
  else
    note "pnpm 12 is not on PATH (corepack enable, or npm i -g pnpm@12). \`sh scripts/seed-demo.sh\` works without it."
  fi
  command -v curl > /dev/null 2>&1 || problem "curl is missing" "Install curl; setup uses it for downloads and health checks."
  if [ "$(ai_mode)" = local ] && [ "$(setting SETUP_AI_MODE)" = local ]; then
    if claude_logged_in; then fine "Claude Code signed in (--ai=local)"; else problem "Claude Code is not signed in (--ai=local)" "$(explain_claude)"; fi
  fi
  if compose_has "$ENV_FILE" docker-compose.ports.yml && ! stack_running; then
    for pair in "WEB_PORT:3100" "MAILPIT_PORT:48025" "POSTGRES_PORT:55432" "REDIS_PORT:56379"; do
      _p=$(port_of "${pair%%:*}" "${pair#*:}")
      if port_free "$_p"; then
        fine "port $_p is free"
      else
        problem "port $_p is in use" "Free it, or pick another: --$(printf '%s' "${pair%%_*}" | tr '[:upper:]' '[:lower:]')-port=N (or --no-ports)."
      fi
    done
  fi
}

# --- the stack -----------------------------------------------------------------------------------
services() {
  _s="postgres redis mailpit web rust-worker"
  [ "$(ai_mode)" = local ] || _s="$_s ai-sidecar"
  printf '%s' "$_s"
}

start_stack() {
  step "Creating the shared Rust build caches (once per Docker host)"
  sh "$ROOT/scripts/rust-cache-init.sh"
  mkdir -p "$ROOT/data/syzygy"
  if [ "$(ai_mode)" = local ]; then ai_start; fi
  step "Building and starting: $(services)"
  say "    The first build compiles the Rust worker and installs the web app. With a warm Docker"
  say "    build cache it took about 3 minutes on the dry run; a cold first build takes longer."
  _t=$(now)
  # shellcheck disable=SC2046
  dc up -d --build --wait --wait-timeout 3600 $(services)
  took "$_t"
  step "Waiting for the database migrations (the worker runs them)"
  _t=$(now)
  _i=0
  until [ "$(dc exec -T postgres psql -U postgres -d chess -Atc \
    "select count(*) from _sqlx_migrations where success" 2> /dev/null || echo 0)" -gt 0 ] 2> /dev/null; do
    _i=$((_i + 1))
    [ "$_i" -lt 120 ] || die "the worker did not migrate the database in 10 minutes (see: docker compose logs rust-worker)"
    sleep 5
  done
  took "$_t"
}

worker_ai_line() {
  dc logs --no-log-prefix rust-worker 2> /dev/null | grep -Eo 'ai: (enabled|disabled)[^"]*' | tail -n 1
}

# --- data ----------------------------------------------------------------------------------------
remote_size() { # url -> bytes
  curl -sIL --max-time 30 "$1" 2> /dev/null | tr -d '\r' | awk 'tolower($1) == "content-length:" { n = $2 } END { print n + 0 }'
}
mb() { awk -v b="$1" 'BEGIN { printf "%d", b / 1000000 + 0.5 }'; }

# The data service writes data/ as the host user on Linux (Docker Desktop maps any uid).
data_user() {
  if [ "$(uname -s)" = Linux ]; then
    DATA_UID=$(id -u)
    DATA_GID=$(id -g)
    export DATA_UID DATA_GID
  fi
}

data_syzygy() {
  _have=$(find "$ROOT/data/syzygy" -name '*.rtb[wz]' 2> /dev/null | wc -l | tr -d ' ')
  step "Syzygy 3-4-5 endgame tables: $SYZYGY_FILES files, about 984 MB (have $_have)"
  say "    Resumable: verified files are kept, a partial file is fetched again. Every file is"
  say "    checked against backend/crates/lichess-db/data/syzygy-345.sha256. 101 s on the dry run."
  _t=$(now)
  dc --profile data run --rm --no-deps data syzygy
  took "$_t"
}

built_kinds() { # month -> the reference kinds already active or building for it
  dc exec -T postgres psql -U postgres -d chess -Atc     "select kind from reference_builds where source_month = '$1' and status in ('active', 'building')" 2> /dev/null || true
}

data_lichess() {
  _m=$(setting SETUP_LICHESS_MONTH)
  [ -n "$_m" ] || _m=$DEFAULT_MONTH
  _file="lichess_db_standard_rated_$_m.pgn.zst"
  _url="https://database.lichess.org/standard/$_file"
  _kinds=$(built_kinds "$_m")
  _flags=""
  case $_kinds in *opening_explorer*) ;; *) _flags="--explorer" ;; esac
  if month_has_clocks "$_m"; then
    case $_kinds in *band_reference*) ;; *) _flags="$_flags --band-reference" ;; esac
  fi
  if [ -z "$_flags" ]; then
    step "Lichess $_m: already built, skipped"
    return 0
  fi
  _bytes=$(remote_size "$_url")
  _games=$(curl -s --max-time 30 https://database.lichess.org/standard/counts.txt 2> /dev/null | awk -v f="$_file" '$1 == f { print $2 }')
  [ "${_bytes:-0}" -gt 0 ] || die "Lichess has no dump for $_m ($_url)"
  step "Lichess $_m: $(mb "$_bytes") MB streamed, never stored on disk; ${_games:-?} games"
  say "    Verified against Lichess's sha256sums.txt. A stopped build starts the month again."
  say "    On the dry run, the 2013-01 explorer took about 40 s."
  if ! month_has_clocks "$_m"; then
    say "    No band reference from this month: it needs [%clk] comments, which Lichess dumps carry"
    say "    from $FIRST_CLOCK_MONTH on (that month is 3.5 GB, 11.3M games). Without it, weaknesses"
    say "    say \"Not compared yet\". Build one with: sh scripts/setup.sh data --month=$FIRST_CLOCK_MONTH"
  else
    say "    The band reference then queues its reference games for the worker's Stockfish, which"
    say "    analyses them in the background."
  fi
  _t=$(now)
  _max=2000000
  if [ -n "$_games" ] && [ "$_games" -lt "$_max" ]; then _max=$_games; fi
  # shellcheck disable=SC2086
  dc --profile data run --rm data build --month "$_m" $_flags     --max-games "$_max" ${_games:+--expected-games "$_games"}
  took "$_t"
}

data_maia() {
  step "Maia-2 difficulty service: PyTorch (CPU) and the Maia-2 weights, SHA-256-checked by maia2"
  _t=$(now)
  dc --profile maia up -d --build --wait maia
  dc up -d rust-worker
  took "$_t"
}

run_data() {
  data_user
  data_syzygy
  dc restart rust-worker > /dev/null
  if [ "$LICHESS" -eq 1 ]; then data_lichess; fi
  if compose_has "$ENV_FILE" docker-compose.maia.yml; then data_maia; fi
}

# --- the host AI sidecar (--ai=local) ------------------------------------------------------------
sidecar_pid() { [ -f "$PIDFILE" ] && cat "$PIDFILE"; }
sidecar_alive() {
  _pid=$(sidecar_pid || true)
  [ -n "$_pid" ] && kill -0 "$_pid" 2> /dev/null
}
sidecar_health() { curl -s --max-time 3 "$SIDECAR_URL/health" 2> /dev/null; }
sidecar_local() { sidecar_health | grep -Eq '"auth"[[:space:]]*:[[:space:]]*"local"'; }

ai_status() {
  if sidecar_alive && sidecar_local; then
    say "host AI sidecar: running (pid $(sidecar_pid)), $SIDECAR_URL/health reports auth \"local\""
    return 0
  fi
  if sidecar_local; then
    say "host AI sidecar: answering on $SIDECAR_URL (auth \"local\"), started outside setup.sh"
    return 0
  fi
  say "host AI sidecar: not running (AI mode: $(ai_mode))"
  return 1
}

ai_start() {
  step "Host AI sidecar (--ai=local)"
  if sidecar_local; then
    say "    already running: $SIDECAR_URL/health reports auth \"local\""
    return 0
  fi
  claude_logged_in || {
    explain_claude >&2
    die "the host sidecar needs a signed-in Claude Code"
  }
  command -v pnpm > /dev/null 2>&1 || die "--ai=local runs the sidecar on the host: install Node 22 and pnpm 12"
  if [ ! -d "$ROOT/services/ai-sidecar/node_modules" ]; then
    say "    installing the workspace (pnpm install)"
    (cd "$ROOT" && pnpm install --frozen-lockfile)
  fi
  mkdir -p "$CACHE_DIR"
  (cd "$ROOT" && nohup pnpm --filter @ci/ai-sidecar start:local > "$SIDECAR_LOG" 2>&1 &
    echo $! > "$PIDFILE")
  _i=0
  until sidecar_local; do
    _i=$((_i + 1))
    if [ "$_i" -ge 60 ] || ! sidecar_alive; then
      tail -n 20 "$SIDECAR_LOG" >&2 || true
      ai_stop > /dev/null 2>&1 || true
      die "the host sidecar did not report auth \"local\" on $SIDECAR_URL/health (log: $SIDECAR_LOG)"
    fi
    sleep 1
  done
  say "    running (pid $(sidecar_pid)); /health reports auth \"local\". Log: .cache/ai-sidecar.log"
  say "    For your own local use only: a personal login must never serve other people."
}

ai_stop() {
  _pid=$(sidecar_pid || true)
  if [ -z "$_pid" ]; then
    say "host AI sidecar: no pidfile, nothing to stop"
    return 0
  fi
  if [ -r "/proc/$_pid/winpid" ] && command -v taskkill > /dev/null 2>&1; then
    # Git Bash: the pid is MSYS's; end the whole native process tree.
    taskkill //F //T //PID "$(cat "/proc/$_pid/winpid")" > /dev/null 2>&1 || true
  else
    command -v pkill > /dev/null 2>&1 && pkill -TERM -P "$_pid" 2> /dev/null || true
    kill -TERM "$_pid" 2> /dev/null || true
  fi
  rm -f "$PIDFILE"
  say "host AI sidecar: stopped"
}

switch_ai() {
  configure
  if [ "$(ai_mode)" = local ]; then ai_start; else ai_stop > /dev/null; fi
  if stack_running; then
    step "Restarting the worker with the new AI mode"
    # shellcheck disable=SC2046
    dc up -d --wait $(services)
    sleep 5
    say "    worker: $(worker_ai_line || echo 'no ai: line yet; see docker compose logs rust-worker')"
  fi
}

# --- doctor --------------------------------------------------------------------------------------
doctor() {
  cd "$ROOT"
  _mode=minimal
  [ "$(find "$ROOT/data/syzygy" -name '*.rtb[wz]' 2> /dev/null | wc -l | tr -d ' ')" -eq 0 ] || _mode=full
  check_prereqs "$_mode"
  step "Configuration"
  if [ ! -f "$ENV_FILE" ]; then
    problem ".env is missing" "Run: sh scripts/setup.sh configure"
  else
    fine ".env present"
    [ -n "$(setting INVITE_ALLOWLIST)" ] || [ "$(setting INVITE_ONLY)" = 0 ] ||
      problem "INVITE_ALLOWLIST is empty: nobody can sign in" "Run: sh scripts/setup.sh configure --email=you@example.com"
    [ "$(setting ALLOW_UNVERIFIED_LINKS)" = 1 ] ||
      note "ALLOW_UNVERIFIED_LINKS is not 1: a chess.com account you link shows its analysis only (local default is 1)"
    [ "$(setting COMPOSE_PATH_SEPARATOR)" = ":" ] ||
      problem "COMPOSE_PATH_SEPARATOR is not \":\"" "On Windows, Compose splits COMPOSE_FILE on ';' otherwise. Run configure."
    fine "AI mode: $(ai_mode)"
  fi
  step "Stack"
  if stack_running; then
    for s in $(services); do
      _state=$(dc ps --format '{{.Service}} {{.State}} {{.Health}}' 2> /dev/null | awk -v s="$s" '$1 == s { print $2, $3 }')
      case $_state in
        "running healthy" | "running ") fine "$s: $_state" ;;
        running*) note "$s: $_state" ;;
        *) problem "$s: ${_state:-not created}" "Run: sh scripts/setup.sh (or docker compose logs $s)" ;;
      esac
    done
    _ai=$(worker_ai_line || true)
    [ -z "$_ai" ] || fine "worker: $_ai"
  else
    note "the stack is not running (sh scripts/setup.sh starts it)"
  fi
  step "Data"
  _n=$(find "$ROOT/data/syzygy" -name '*.rtb[wz]' 2> /dev/null | wc -l | tr -d ' ')
  if [ "$_n" -eq "$SYZYGY_FILES" ]; then
    fine "Syzygy: $_n tables (re-verified on every worker start)"
  else
    note "Syzygy: $_n of $SYZYGY_FILES tables; endgame probes are off until \`sh scripts/setup.sh data\`"
  fi
  if [ "$(ai_mode)" = local ]; then
    step "Host AI sidecar"
    if ai_status > /dev/null; then fine "$(ai_status)"; else problem "the host sidecar is not running" "Run: sh scripts/setup.sh ai-start"; fi
  fi
  printf '\n'
  if [ "$PROBLEMS" -eq 0 ]; then say "doctor: everything checks out"; else say "doctor: $PROBLEMS problem(s) above"; fi
  [ "$PROBLEMS" -eq 0 ]
}

summary() {
  step "Rematch is running"
  if compose_has "$ENV_FILE" docker-compose.ports.yml; then
    say "    App:     http://localhost:$(port_of WEB_PORT 3100)"
    say "    Mailpit: http://localhost:$(port_of MAILPIT_PORT 48025) (your sign-in links arrive here)"
  else
    say "    No host ports are published (--no-ports): reach the app through docker compose."
  fi
  say "    Sign in with: $(setting INVITE_ALLOWLIST | cut -d, -f1)"
  say "    Demo data:    sh scripts/seed-demo.sh   (or pnpm seed:demo; --remove takes it away)"
  say "    Worker:       $(worker_ai_line || echo 'ai: line not logged yet (docker compose logs rust-worker)')"
  say "    Check later:  sh scripts/setup.sh doctor"
}

# --- main ----------------------------------------------------------------------------------------
cd "$ROOT"
if [ -z "$CMD" ]; then
  CMD=minimal
  if [ "$INTERACTIVE" -eq 1 ]; then
    CMD=$(ask 'Set up minimal (no large downloads) or full (Syzygy, Lichess data, optional Maia)?' minimal)
    case $CMD in minimal | full) ;; *) die "choose minimal or full" ;; esac
  fi
fi

case $CMD in
  configure) configure ;;
  doctor) doctor ;;
  ai) switch_ai ;;
  ai-start) ai_start ;;
  ai-stop) ai_stop ;;
  ai-status) ai_status ;;
  data)
    [ -z "$MONTH" ] || env_set "$ENV_FILE" SETUP_LICHESS_MONTH "$MONTH"
    [ "$MAIA" != 1 ] || compose_add "$ENV_FILE" docker-compose.maia.yml
    run_data
    ;;
  minimal | full)
    _start=$(now)
    if [ "$CMD" = full ] && [ -z "$MAIA" ] && [ "$INTERACTIVE" -eq 1 ]; then
      [ "$(ask 'Also run Maia-2 (move difficulty by rating, about 3 GB)? (y/n)' n)" != y ] || MAIA=1
    fi
    configure
    check_prereqs "$CMD"
    [ "$PROBLEMS" -eq 0 ] || die "fix the $PROBLEMS problem(s) above, then run this again"
    start_stack
    if [ "$CMD" = full ]; then run_data; fi
    summary
    say "    Total: $(($(now) - _start)) s"
    ;;
esac
