# .env helpers for scripts/setup.sh (sourced, POSIX sh). Values travel to awk through the
# environment, never through its argv, so a secret never shows up in a process listing. Nothing
# here prints a value.

# Prints KEY's value (the last `KEY=` line; surrounding quotes stripped). Fails when the key is
# absent.
env_get() {
  [ -f "$1" ] || return 1
  ENV_KEY=$2 awk '
    BEGIN { k = ENVIRON["ENV_KEY"]; n = length(k) + 1 }
    substr($0, 1, n) == k "=" { v = substr($0, n + 1); found = 1 }
    END {
      if (!found) exit 1
      if (v ~ /^".*"$/ || v ~ /^\047.*\047$/) v = substr(v, 2, length(v) - 2)
      print v
    }
  ' "$1"
}

# Sets KEY to VALUE: replaces every `KEY=` line, or appends one. Writes through a temporary file in
# the same directory, created private (umask 077), then renames it into place.
env_set() {
  _file=$1
  _tmp="$_file.tmp.$$"
  [ -f "$_file" ] || (umask 077 && : > "$_file")
  (umask 077 && ENV_KEY=$2 ENV_VALUE=$3 awk '
    BEGIN { k = ENVIRON["ENV_KEY"]; v = ENVIRON["ENV_VALUE"]; n = length(k) + 1 }
    substr($0, 1, n) == k "=" { print k "=" v; done = 1; next }
    { print }
    END { if (!done) print k "=" v }
  ' "$_file" > "$_tmp") && mv "$_tmp" "$_file"
}

# Sets KEY only when it is absent or empty: a value the user chose is never overwritten.
env_default() {
  _cur=$(env_get "$1" "$2" || true)
  [ -n "$_cur" ] || env_set "$1" "$2" "$3"
}

# Creates FILE from EXAMPLE, or appends the example's keys FILE lacks (with their example values).
# Never changes an existing line. Prints how many keys it added.
env_merge() {
  _example=$1
  _file=$2
  if [ ! -f "$_file" ]; then
    (umask 077 && cp "$_example" "$_file")
    echo "created"
    return 0
  fi
  _added=0
  _keys=$(sed -n 's/^\([A-Za-z_][A-Za-z0-9_]*\)=.*/\1/p' "$_example")
  for _k in $_keys; do
    if ! env_get "$_file" "$_k" > /dev/null; then
      if [ "$_added" -eq 0 ]; then
        printf '\n# Added by scripts/setup.sh from .env.example\n' >> "$_file"
      fi
      grep "^$_k=" "$_example" | tail -n 1 >> "$_file"
      _added=$((_added + 1))
    fi
  done
  echo "$_added"
}

# COMPOSE_FILE as a list: adds OVERLAY once (after docker-compose.yml), or removes it.
compose_add() {
  _cf=$(env_get "$1" COMPOSE_FILE || true)
  [ -n "$_cf" ] || _cf=docker-compose.yml
  case ":$_cf:" in
    *":$2:"*) ;;
    *) _cf="$_cf:$2" ;;
  esac
  env_set "$1" COMPOSE_FILE "$_cf"
}

compose_remove() {
  _cf=$(env_get "$1" COMPOSE_FILE || true)
  _out=$(printf '%s' "$_cf" | tr ':' '\n' | grep -vx "$2" | grep -v '^$' | paste -sd: -)
  [ -n "$_out" ] || _out=docker-compose.yml
  env_set "$1" COMPOSE_FILE "$_out"
}

compose_has() {
  _cf=$(env_get "$1" COMPOSE_FILE || true)
  case ":$_cf:" in *":$2:"*) return 0 ;; *) return 1 ;; esac
}

# Adds EMAIL (lowercased) to INVITE_ALLOWLIST once.
allowlist_add() {
  _email=$(printf '%s' "$2" | tr '[:upper:]' '[:lower:]')
  _list=$(env_get "$1" INVITE_ALLOWLIST || true)
  case ",$_list," in
    *",$_email,"*) return 0 ;;
  esac
  if [ -n "$_list" ]; then _list="$_list,$_email"; else _list=$_email; fi
  env_set "$1" INVITE_ALLOWLIST "$_list"
}

# N random bytes as 2N lowercase hex digits.
random_hex() {
  od -An -tx1 -N"$1" /dev/urandom | tr -d ' \n'
}

# version_ge A B: true when dotted version A >= B (suffixes such as -desktop.1 are ignored).
version_ge() {
  _a=$(printf '%s' "$1" | sed 's/^v//; s/[^0-9.].*//')
  _b=$(printf '%s' "$2" | sed 's/^v//; s/[^0-9.].*//')
  awk -v a="$_a" -v b="$_b" 'BEGIN {
    na = split(a, x, "."); nb = split(b, y, ".")
    n = na > nb ? na : nb
    for (i = 1; i <= n; i++) {
      p = x[i] + 0; q = y[i] + 0
      if (p > q) exit 0
      if (p < q) exit 1
    }
    exit 0
  }'
}

# Bytes to whole GiB, rounded down.
gib() {
  awk -v b="$1" 'BEGIN { printf "%d", b / 1073741824 }'
}
