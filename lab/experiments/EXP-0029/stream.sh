#!/bin/sh
# EXP-0029: stream the nine monthly Lichess dumps with `lichess-db cohort` (ADR 0114), one month
# at a time, into data/exp29-cohort (section 7 of the pre-registration).
#
#   sh lab/experiments/EXP-0029/stream.sh --dry-run   # the guards, the preflight, the commands
#   sh lab/experiments/EXP-0029/stream.sh             # the stream
#
# Runs from Git Bash on the Windows host and needs Docker, a python on PATH (to read JSON), and
# the `ci-exp29-rust-worker` image built from this branch (`COMPOSE_FILE=docker-compose.yml
# docker compose -p ci-exp29 build rust-worker`).
#
# Before any month: the df guard must parse to a number; the image must exist, run `sh`, show a
# cgroup v2 cpu.stat with usage_usec, and report the registered algorithm id (read as JSON).
# For each month:
#   - a complete month (meta.json with EXP-0009's salt and the 0.9 rate, its data files and its
#     row in stream-cpu.tsv) is skipped; any other existing folder is a partial month and the
#     stream refuses it (delete it by hand and record the re-stream);
#   - E: must keep 50 GB free plus the month's output, and data/exp29-cohort must stay within
#     70 GB with it; a watcher checks both every 60 s and stops the container if either fails;
#   - the container's own CPU (cpu.stat usage_usec) is appended to stream-cpu.tsv, whose sum is
#     the streaming CPU-hours the sizing step reads;
#   - on a failed exit, or on INT, TERM or HUP, the container is stopped, the watcher killed,
#     the month's whole folder removed, the stop logged, and the stream ends non-zero.

set -u
export MSYS_NO_PATHCONV=1

DRY=0
case "${1:-}" in
  --dry-run) DRY=1 ;;
  "") ;;
  *) echo "usage: stream.sh [--dry-run]" >&2; exit 64 ;;
esac

DOCKER=${EXP29_DOCKER:-docker}
DF=${EXP29_DF:-df}
PYTHON=${EXP29_PYTHON:-python}
IMAGE=${EXP29_IMAGE:-ci-exp29-rust-worker}
ALGO=${EXP29_ALGO:-algo-6.0.0+f40a2b27677a}   # section 3
OUT=${EXP29_OUT:-/e/code/chess-improver/data/exp29-cohort}          # this shell's path
OUT_HOST=${EXP29_OUT_HOST:-E:/code/chess-improver/data/exp29-cohort}  # Docker's path
DF_PATH=${EXP29_DF_PATH:-/e}
MIN_FREE_GB=${EXP29_MIN_FREE_GB:-50}
CAP_GB=${EXP29_CAP_GB:-70}
WATCH_SECONDS=${EXP29_WATCH_SECONDS:-60}
SALT=80b7ed61df63b5e94e96089d100887cef8465b13028260352639573bee9ae5d4
RATE=0.9
# month:kind, in streaming order; kind is players (players file only) or games (--games).
MONTHS="2025-09:players 2026-04:players 2026-06:players 2025-10:games 2025-11:games
2025-12:games 2026-01:games 2026-02:games 2026-03:games"
PLAYERS_GB=1   # a players file, with margin (EXP-0009: 0.35 to 0.42 GB)
GAMES_GB=11    # a month with --games, with margin (EXP-0009: 9.7 to 10.8 GB)
TSV="$OUT/stream-cpu.tsv"

LOG=/dev/null
log() {
  line="$(date -u +%Y-%m-%dT%H:%M:%SZ) $*"
  printf '%s\n' "$line"
  if [ "$LOG" != /dev/null ]; then printf '%s\n' "$line" >> "$LOG"; fi
}

stop() {
  log "stop: $*"
  exit 1
}

is_number() {
  case "$1" in '' | *[!0-9]*) return 1 ;; esac
  return 0
}

# The free space on E: in whole GB; empty if df gives nothing usable.
free_gb() {
  "$DF" --output=avail -BG "$DF_PATH" 2> /dev/null | tail -n 1 | tr -dc 0-9
}

# The size of data/exp29-cohort in whole GB (0 before it exists).
used_gb() {
  if [ -d "$OUT" ]; then du -s -BG "$OUT" 2> /dev/null | cut -f1 | tr -dc 0-9; else echo 0; fi
}

# The free space, or exit 2 if the guard does not parse to a number.
checked_free_gb() {
  v=$(free_gb)
  if ! is_number "$v"; then
    log "stop: the disk guard did not parse a number from '$DF --output=avail -BG $DF_PATH'" >&2
    exit 2
  fi
  printf '%s\n' "$v"
}

json_field() {  # json_field FIELD < json: the top-level field, or nothing
  "$PYTHON" -c 'import json, sys
try:
    print(json.load(sys.stdin).get(sys.argv[1], ""))
except Exception:
    pass' "$1"
}

# A month is complete when its meta.json has the salt and rate, its data files are there, and
# stream-cpu.tsv has its row.
month_complete() {
  m=$1
  kind=$2
  [ -f "$OUT/$m/meta.json" ] && [ -f "$OUT/$m/players.jsonl" ] || return 1
  if [ "$kind" = games ] && [ ! -f "$OUT/$m/games.jsonl" ]; then return 1; fi
  [ "$(json_field salt < "$OUT/$m/meta.json")" = "$SALT" ] || return 1
  [ "$(json_field player_rate < "$OUT/$m/meta.json")" = "$RATE" ] || return 1
  [ -f "$TSV" ] && grep -q "^$m	" "$TSV"
}

command_for() {
  flags="--month $1 --salt $SALT --player-rate $RATE --out /out/$1"
  if [ "$2" = games ]; then flags="$flags --games"; fi
  printf '%s' "lichess-db cohort $flags; s=\$?; echo exp29-cpu-stat; cat /sys/fs/cgroup/cpu.stat; exit \$s"
}

# ---- the checks before any byte is streamed ----------------------------------------------------

free=$(checked_free_gb) || exit 2
log "disk guard: $free GB free at $DF_PATH (reserve $MIN_FREE_GB GB); data/exp29-cohort $(used_gb) GB (cap $CAP_GB GB)"

refuse() {  # stop in a real run; report in a dry run
  if [ "$DRY" = 1 ]; then log "dry run: would stop: $*"; else stop "$*"; fi
}

digest=$("$DOCKER" image inspect --format '{{.Id}}' "$IMAGE" 2> /dev/null || true)
algo=""
if [ -z "$digest" ]; then
  refuse "image $IMAGE is not built"
else
  log "image: $IMAGE $digest"
  usage0=$("$DOCKER" run --rm --entrypoint sh "$IMAGE" -c 'cat /sys/fs/cgroup/cpu.stat' 2> /dev/null \
    | sed -n 's/^usage_usec \([0-9][0-9]*\)$/\1/p' | head -n 1)
  if is_number "$usage0"; then
    log "preflight: sh and cgroup v2 cpu.stat read in the image (usage_usec $usage0)"
  else
    refuse "the image gives no usage_usec in /sys/fs/cgroup/cpu.stat"
  fi
  algo=$("$DOCKER" run --rm --entrypoint cli "$IMAGE" algo show --json 2> /dev/null | json_field id)
  if [ "$algo" = "$ALGO" ]; then
    log "preflight: the image's cli is $algo, the registered id"
  else
    refuse "the image's cli reports '${algo:-nothing}', not the registered $ALGO"
  fi
  if ! "$DOCKER" run --rm --entrypoint lichess-db "$IMAGE" cohort --help > /dev/null 2>&1; then
    refuse "image $IMAGE has no cohort command"
  else
    log "preflight: lichess-db cohort is in the image"
  fi
fi

for spec in $MONTHS; do
  m=${spec%%:*}
  kind=${spec#*:}
  if [ -e "$OUT/$m" ] && ! month_complete "$m" "$kind"; then
    refuse "$OUT/$m is a partial month: a partial month is never resumed (delete it by hand and record the re-stream)"
  fi
done

if [ "$DRY" = 0 ]; then
  mkdir -p "$OUT" || stop "cannot create $OUT"
  LOG="$OUT/stream.log"
  log "stream: started, salt $SALT, player rate $RATE, image $IMAGE $digest, cli $algo"
  if [ ! -f "$TSV" ]; then printf 'month\tkind\tusage_usec\n' > "$TSV"; fi
fi

# ---- the stream, one month at a time -----------------------------------------------------------

m=""
name=""
watcher=""
child=""
on_signal() {
  log "stop: signal received during ${m:-no month}"
  if [ -n "$name" ]; then "$DOCKER" stop "$name" > /dev/null 2>&1; fi
  if [ -n "$child" ]; then kill "$child" 2> /dev/null; wait "$child" 2> /dev/null; fi
  if [ -n "$watcher" ]; then kill "$watcher" 2> /dev/null; fi
  if [ -n "$m" ] && [ -n "$name" ]; then
    rm -rf "${OUT:?}/$m"
    log "stop: removed $OUT/$m"
  fi
  exit 130
}
trap on_signal INT TERM HUP

for spec in $MONTHS; do
  m=${spec%%:*}
  kind=${spec#*:}
  name=""
  if [ "$kind" = games ]; then reserve=$GAMES_GB; else reserve=$PLAYERS_GB; fi
  if month_complete "$m" "$kind"; then
    log "$m: complete, skipped"
    continue
  fi
  free=$(checked_free_gb) || exit 2
  used=$(used_gb)
  cmd=$(command_for "$m" "$kind")
  if [ "$DRY" = 1 ]; then
    if [ "$free" -lt $((MIN_FREE_GB + reserve)) ]; then
      log "dry run: $m would stop: $free GB free, needs $((MIN_FREE_GB + reserve)) GB"
    fi
    if [ $((used + reserve)) -gt "$CAP_GB" ]; then
      log "dry run: $m would stop: $used GB used + $reserve GB passes the $CAP_GB GB cap"
    fi
    log "dry run: $DOCKER run --rm --name exp29-cohort-$m --entrypoint sh -v $OUT_HOST:/out $IMAGE -c '$cmd'"
    continue
  fi
  if [ "$free" -lt $((MIN_FREE_GB + reserve)) ]; then
    stop "$m: $free GB free, needs $((MIN_FREE_GB + reserve)) GB"
  fi
  if [ $((used + reserve)) -gt "$CAP_GB" ]; then
    stop "$m: $used GB used + $reserve GB passes the $CAP_GB GB cap"
  fi
  name="exp29-cohort-$m"
  log "$m: start ($kind), $free GB free, $used GB used"
  (
    while sleep "$WATCH_SECONDS"; do
      f=$(free_gb)
      is_number "$f" || f=0
      u=$(used_gb)
      is_number "$u" || u=0
      if [ "$f" -lt "$MIN_FREE_GB" ] || [ "$u" -gt "$CAP_GB" ]; then
        log "watcher: $f GB free (reserve $MIN_FREE_GB), $u GB used (cap $CAP_GB); stopping $name"
        "$DOCKER" stop "$name" > /dev/null 2>&1
      fi
    done
  ) &
  watcher=$!
  "$DOCKER" run --rm --name "$name" --entrypoint sh -v "$OUT_HOST:/out" "$IMAGE" -c "$cmd" \
    > "$OUT/$m.log" 2>&1 &
  child=$!
  wait "$child"
  status=$?
  child=""
  kill "$watcher" 2> /dev/null
  wait "$watcher" 2> /dev/null
  watcher=""
  usage=$(sed -n '/^exp29-cpu-stat$/,$ s/^usage_usec \([0-9][0-9]*\)$/\1/p' "$OUT/$m.log" | tail -n 1)
  if [ "$status" -ne 0 ]; then
    rm -rf "${OUT:?}/$m"
    stop "$m: exit $status; removed $OUT/$m; see $OUT/$m.log"
  fi
  if ! is_number "$usage"; then
    rm -rf "${OUT:?}/$m"
    stop "$m: no usage_usec in $OUT/$m.log; removed $OUT/$m"
  fi
  printf '%s\t%s\t%s\n' "$m" "$kind" "$usage" >> "$TSV"
  log "$m: done, usage_usec $usage"
done
trap - INT TERM HUP

if [ "$DRY" = 1 ]; then
  log "dry run: nothing streamed"
  exit 0
fi
total=$(awk -F '\t' 'NR > 1 { s += $3 } END { printf "%.3f", s / 3.6e9 }' "$TSV")
log "stream: done, $total CPU-hours in the containers (stream-cpu.tsv)"
