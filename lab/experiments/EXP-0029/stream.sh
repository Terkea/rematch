#!/bin/sh
# EXP-0029: stream the nine monthly Lichess dumps with `lichess-db cohort` (ADR 0114), one month
# at a time, into data/exp29-cohort (section 7 of the pre-registration).
#
#   sh lab/experiments/EXP-0029/stream.sh --dry-run   # the guards and the commands, no stream
#   sh lab/experiments/EXP-0029/stream.sh             # the stream
#
# Runs from Git Bash on the Windows host and needs Docker and the `ci-exp29-rust-worker` image
# built from this branch (`COMPOSE_FILE=docker-compose.yml docker compose -p ci-exp29 build
# rust-worker`). For each month it:
#   - refuses to start if the month's folder already exists: a partial month is never resumed;
#     a re-run starts from an empty folder and is recorded as one;
#   - checks that E: keeps the 50 GB reserve plus the month's output, and that the df guard
#     parses to a number;
#   - runs the container with a watcher that stops it if E: falls below 50 GB free (every 60 s);
#   - reads the container's own CPU from /sys/fs/cgroup/cpu.stat (usage_usec) and appends it
#     to stream-cpu.tsv, whose sum is the streaming CPU-hours the sizing step reads;
#   - on a failed exit, deletes the month's *.part files, logs it and stops.
# The log records the image's digest and the algorithm id of its cli.

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
IMAGE=${EXP29_IMAGE:-ci-exp29-rust-worker}
OUT=${EXP29_OUT:-/e/code/chess-improver/data/exp29-cohort}          # this shell's path
OUT_HOST=${EXP29_OUT_HOST:-E:/code/chess-improver/data/exp29-cohort}  # Docker's path
DF_PATH=${EXP29_DF_PATH:-/e}
MIN_FREE_GB=${EXP29_MIN_FREE_GB:-50}
WATCH_SECONDS=${EXP29_WATCH_SECONDS:-60}
SALT=80b7ed61df63b5e94e96089d100887cef8465b13028260352639573bee9ae5d4
RATE=0.9
# month:kind, in streaming order; kind is players (players file only) or games (--games).
MONTHS="2025-09:players 2026-04:players 2026-06:players 2025-10:games 2025-11:games
2025-12:games 2026-01:games 2026-02:games 2026-03:games"
PLAYERS_GB=1   # a players file, with margin (EXP-0009: 0.35 to 0.42 GB)
GAMES_GB=11    # a month with --games, with margin (EXP-0009: 9.7 to 10.8 GB)

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

# The free space on E: in whole GB; empty if df gives nothing usable.
free_gb() {
  "$DF" --output=avail -BG "$DF_PATH" 2>/dev/null | tail -n 1 | tr -dc 0-9
}

# The free space, or exit 2 if the guard does not parse to a number.
checked_free_gb() {
  v=$(free_gb)
  case "$v" in
    '' | *[!0-9]*)
      log "stop: the disk guard did not parse a number from '$DF --output=avail -BG $DF_PATH'" >&2
      exit 2
      ;;
  esac
  printf '%s\n' "$v"
}

command_for() {
  m=$1
  kind=$2
  flags="--month $m --salt $SALT --player-rate $RATE --out /out/$m"
  if [ "$kind" = games ]; then flags="$flags --games"; fi
  printf '%s' "lichess-db cohort $flags; s=\$?; echo exp29-cpu-stat; cat /sys/fs/cgroup/cpu.stat; exit \$s"
}

# ---- the checks before any byte is streamed ----------------------------------------------------

free=$(checked_free_gb) || exit 2
log "disk guard: $free GB free at $DF_PATH (reserve $MIN_FREE_GB GB)"

digest=$("$DOCKER" image inspect --format '{{.Id}}' "$IMAGE" 2>/dev/null || true)
if [ -z "$digest" ]; then
  if [ "$DRY" = 1 ]; then
    log "image: $IMAGE is not built yet (build it before the stream)"
  else
    stop "image $IMAGE is not built"
  fi
else
  log "image: $IMAGE $digest"
  algo=$("$DOCKER" run --rm --entrypoint cli "$IMAGE" algo show --json 2>/dev/null \
    | sed -n 's/.*"id": *"\([^"]*\)".*/\1/p' | head -n 1)
  log "image cli algorithm id: ${algo:-unknown}"
  if ! "$DOCKER" run --rm --entrypoint lichess-db "$IMAGE" cohort --help > /dev/null 2>&1; then
    if [ "$DRY" = 1 ]; then log "image: $IMAGE has no cohort command"; else stop "image $IMAGE has no cohort command"; fi
  fi
fi

for spec in $MONTHS; do
  m=${spec%%:*}
  if [ -e "$OUT/$m" ]; then
    if [ "$DRY" = 1 ]; then log "exists: $OUT/$m (the stream would refuse it)"; else stop "$OUT/$m exists: a partial month is never resumed"; fi
  fi
done

if [ "$DRY" = 0 ]; then
  mkdir -p "$OUT" || stop "cannot create $OUT"
  LOG="$OUT/stream.log"
  log "stream: started, salt $SALT, player rate $RATE, image $IMAGE $digest, cli ${algo:-unknown}"
  printf 'month\tkind\tusage_usec\n' > "$OUT/stream-cpu.tsv"
fi

# ---- the stream, one month at a time -----------------------------------------------------------

for spec in $MONTHS; do
  m=${spec%%:*}
  kind=${spec#*:}
  if [ "$kind" = games ]; then reserve=$GAMES_GB; else reserve=$PLAYERS_GB; fi
  free=$(checked_free_gb) || exit 2
  name="exp29-cohort-$m"
  cmd=$(command_for "$m" "$kind")
  if [ "$DRY" = 1 ]; then
    if [ "$free" -lt $((MIN_FREE_GB + reserve)) ]; then
      log "dry run: $m would stop: $free GB free, needs $((MIN_FREE_GB + reserve)) GB"
    fi
    log "dry run: $DOCKER run --rm --name $name --entrypoint sh -v $OUT_HOST:/out $IMAGE -c '$cmd'"
    continue
  fi
  if [ "$free" -lt $((MIN_FREE_GB + reserve)) ]; then
    stop "$m: $free GB free, needs $((MIN_FREE_GB + reserve)) GB"
  fi
  log "$m: start ($kind), $free GB free"
  (
    while sleep "$WATCH_SECONDS"; do
      f=$(free_gb)
      case "$f" in '' | *[!0-9]*) f=0 ;; esac
      if [ "$f" -lt "$MIN_FREE_GB" ]; then
        log "watcher: $f GB free, below $MIN_FREE_GB; stopping $name"
        "$DOCKER" stop "$name" > /dev/null 2>&1
      fi
    done
  ) &
  watcher=$!
  "$DOCKER" run --rm --name "$name" --entrypoint sh -v "$OUT_HOST:/out" "$IMAGE" -c "$cmd" \
    > "$OUT/$m.log" 2>&1
  status=$?
  kill "$watcher" 2> /dev/null
  wait "$watcher" 2> /dev/null
  usage=$(sed -n '/^exp29-cpu-stat$/,$ s/^usage_usec \([0-9][0-9]*\)$/\1/p' "$OUT/$m.log" | tail -n 1)
  if [ "$status" -ne 0 ]; then
    rm -f "$OUT/$m"/*.part
    stop "$m: exit $status; removed $OUT/$m/*.part; see $OUT/$m.log"
  fi
  if [ -z "$usage" ]; then
    stop "$m: no usage_usec in $OUT/$m.log"
  fi
  printf '%s\t%s\t%s\n' "$m" "$kind" "$usage" >> "$OUT/stream-cpu.tsv"
  log "$m: done, usage_usec $usage"
done

if [ "$DRY" = 1 ]; then
  log "dry run: nothing streamed"
  exit 0
fi
total=$(awk -F '\t' 'NR > 1 { s += $3 } END { printf "%.3f", s / 3.6e9 }' "$OUT/stream-cpu.tsv")
log "stream: done, $total CPU-hours in the containers (stream-cpu.tsv)"
