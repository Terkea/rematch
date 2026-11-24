#!/usr/bin/env bash
# Apply the repository settings kept as code in .github/settings (ADR 0095) to this repository:
# the "main" ruleset (created, or updated in place by name), the repository settings and topics,
# private vulnerability reporting, Pages built by Actions, the Actions settings (every outside
# contributor's fork pull request waits for approval, a read-only default GITHUB_TOKEN; ADR 0108),
# the labels, and the roadmap milestones (milestones.json, matched by title, renamed through
# `renamed_from`; ADRs 0099 and 0104). With --seed-issues it also syncs the seed issues in
# roadmap-issues.json, epics first: it creates the ones that do not exist, moves and updates the
# open ones that differ from the file (labels are only added, ticked boxes are kept), moves each
# milestone's `adopt_issues`, then links each one under its epic as a sub-issue. A closed issue
# is never edited.
#
# Run it from the maintainer's own `gh` login (an admin of the repository). No token is stored
# anywhere. It is idempotent: it reads the live state first, prints the diff, and writes only what
# differs, so a second run prints "No changes".
#
#   sh scripts/github/apply-settings.sh --dry-run                 # print the plan, change nothing
#   sh scripts/github/apply-settings.sh                           # print the plan, then apply it
#   sh scripts/github/apply-settings.sh --prune                   # also delete labels and
#                                                                 # milestones not in the files
#   sh scripts/github/apply-settings.sh --seed-issues --dry-run   # also plan the seed issues
#   sh scripts/github/apply-settings.sh --seed-issues             # and create and link them
#
# Works under Git Bash on Windows and on Linux. Needs gh (logged in) and node.
set -eu

dry_run=0
prune=""
seed=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    --prune) prune="--prune" ;;
    --seed-issues) seed=1 ;;
    -h | --help)
      sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

# Git Bash rewrites arguments that look like POSIX paths (a leading slash) into Windows paths.
# Every gh endpoint here is relative, and this keeps any other argument as written.
export MSYS_NO_PATHCONV=1
export MSYS2_ARG_CONV_EXCL='*'

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)

command -v gh >/dev/null || { echo "gh is not installed" >&2; exit 1; }
command -v node >/dev/null || { echo "node is not installed" >&2; exit 1; }

repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
[ -n "$repo" ] || { echo "could not read owner/repo from gh repo view" >&2; exit 1; }

work=$(mktemp -d "${TMPDIR:-/tmp}/rematch-settings.XXXXXX")
trap 'rm -rf "$work"' EXIT
# node and gh on Windows are native programs: give them a C:/... path, not /tmp/...
if command -v cygpath >/dev/null 2>&1; then
  work=$(cygpath -m "$work")
  root=$(cygpath -m "$root")
fi
mkdir -p "$work/live"

# Save one GET response as <name>, or gh's error as <name>.err (a 403 or 404 is a state, not a
# failure: a private repository on the free plan has no rulesets, Pages may be off).
get() {
  name=$1
  shift
  if ! gh api "$@" >"$work/live/$name" 2>"$work/live/$name.err"; then
    rm -f "$work/live/$name"
  else
    rm -f "$work/live/$name.err"
  fi
}

get_milestones() {
  get milestones.jsonl "repos/$repo/milestones?state=all&per_page=100" --paginate -q '.[]'
}

# Issues only: the issues endpoint also lists pull requests.
get_issues() {
  get issues.jsonl "repos/$repo/issues?state=all&per_page=100" --paginate \
    -q '.[] | select(.pull_request == null)'
}

# Plan one phase into $work/plan-<phase>; apply it unless this is a dry run.
failed=0
phase() {
  name=$1
  shift
  out="$work/plan-$name"
  mkdir -p "$out"
  echo
  node "$root/scripts/github/settings-plan.mjs" --live "$work/live" --repo "$repo" --out "$out" \
    --phase "$name" "$@"
  [ "$dry_run" -eq 0 ] || return 0
  calls="$out/calls.tsv"
  [ -s "$calls" ] || return 0
  echo
  tab=$(printf '\t')
  while IFS="$tab" read -r method path body; do
    [ -n "$method" ] || continue
    printf '%s %s ... ' "$method" "$path"
    if [ "$body" = "-" ]; then
      set -- gh api -X "$method" "$path" --silent
    else
      set -- gh api -X "$method" "$path" --input "$body" --silent
    fi
    if err=$("$@" 2>&1); then
      echo "ok"
    else
      echo "FAILED: $err"
      failed=$((failed + 1))
    fi
  done <"$calls"
}

ruleset_name=$(node -p "JSON.parse(require('fs').readFileSync('$root/.github/settings/rulesets/main.json','utf8')).name")

echo "Repository: $repo"
get repo.json "repos/$repo"
get labels.jsonl "repos/$repo/labels" --paginate -q '.[]'
get rulesets.jsonl "repos/$repo/rulesets" --paginate -q '.[]'
if [ -f "$work/live/rulesets.jsonl" ]; then
  id=$(gh api "repos/$repo/rulesets" --paginate \
    -q ".[] | select(.name == \"$ruleset_name\" and .source_type == \"Repository\") | .id" | head -n 1)
  if [ -n "$id" ]; then
    get ruleset.json "repos/$repo/rulesets/$id"
  fi
fi
get vulnerability-reporting.json "repos/$repo/private-vulnerability-reporting"
get pages.json "repos/$repo/pages"
get actions-fork-approval.json "repos/$repo/actions/permissions/fork-pr-contributor-approval"
get actions-workflow.json "repos/$repo/actions/permissions/workflow"
get_milestones

# shellcheck disable=SC2086 # $prune is empty or one flag
phase settings $prune

if [ "$seed" -eq 1 ]; then
  # Fresh reads after each phase: the issues need the milestone numbers, the links need the
  # issue ids, and on a real run both were just created.
  get_milestones
  get_issues
  phase issues
  get_issues
  for n in $(node "$root/scripts/github/settings-plan.mjs" --live "$work/live" --epic-numbers); do
    get "sub-issues-$n.jsonl" "repos/$repo/issues/$n/sub_issues?per_page=100" --paginate -q '.[]'
  done
  phase sub-issues
fi

echo
if [ "$dry_run" -eq 1 ]; then
  echo "Dry run: nothing was changed."
  exit 0
fi
if [ "$failed" -gt 0 ]; then
  echo "$failed write(s) failed. Fix the cause and run the script again; it only redoes what still differs." >&2
  exit 1
fi
echo "Applied. Run with --dry-run again to confirm: it should print \"No changes\"."
