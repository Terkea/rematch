#!/usr/bin/env bash
# Create (or bring up to date) the public "Rematch roadmap" board, a GitHub Project (v2) owned by
# the repository owner and linked to the repository (ADR 0099). It is idempotent:
#
# - the project is found by title, else created, then made public and linked to the repository;
# - Status (built in) and Milestone (built in for issues) need nothing; Area, a single-select
#   field with one option per area/* label in .github/settings/labels.json, is created when
#   missing, and missing options are reported (gh cannot edit a field's options);
# - every open issue is added (adding an issue that is already on the board is a no-op) and its
#   Area is set from its first area/* label.
#
# One-time setup: projects need the `project` scope on the maintainer's gh login, which the
# default login does not have:
#
#   gh auth refresh -s project
#
#   sh scripts/github/create-project.sh             # create or update the board
#   sh scripts/github/create-project.sh --dry-run   # say what it would do, change nothing
#
# Works under Git Bash on Windows and on Linux. Needs gh (logged in, with the project scope) and
# node. No token is stored anywhere.
set -eu

dry_run=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) dry_run=1 ;;
    -h | --help)
      sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg (try --help)" >&2
      exit 2
      ;;
  esac
done

export MSYS_NO_PATHCONV=1
export MSYS2_ARG_CONV_EXCL='*'

TITLE="Rematch roadmap"
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
if command -v cygpath >/dev/null 2>&1; then root=$(cygpath -m "$root"); fi

command -v gh >/dev/null || { echo "gh is not installed" >&2; exit 1; }
command -v node >/dev/null || { echo "node is not installed" >&2; exit 1; }
if ! gh auth status 2>&1 | grep -q "'project'"; then
  echo "Your gh login has no 'project' scope. Run once: gh auth refresh -s project" >&2
  exit 1
fi

repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
owner=${repo%%/*}
# A JSON value from stdin: json '.a.b' (a tiny jq, so jq is not needed).
json() { node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{let v=JSON.parse(s);for(const k of process.argv[1].split(".").filter(Boolean))v=v?.[k];console.log(typeof v==="object"?JSON.stringify(v):v??"")})' "$1"; }
run() {
  if [ "$dry_run" -eq 1 ]; then
    echo "would run: $*"
  else
    "$@"
  fi
}

areas=$(node -e "console.log(JSON.parse(require('fs').readFileSync('$root/.github/settings/labels.json','utf8')).filter(l=>l.name.startsWith('area/')).map(l=>l.name.slice(5)).join(','))")
echo "Repository: $repo; board: \"$TITLE\" owned by $owner; areas: $areas"

number=$(gh project list --owner "$owner" --format json --limit 200 |
  node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const p=JSON.parse(s).projects.find(x=>x.title===process.argv[1]);console.log(p?p.number:"")})' "$TITLE")
if [ -z "$number" ]; then
  echo "+ project \"$TITLE\" (create)"
  if [ "$dry_run" -eq 1 ]; then
    echo "Dry run: the project does not exist yet, so nothing further can be planned."
    exit 0
  fi
  number=$(gh project create --owner "$owner" --title "$TITLE" --format json | json .number)
fi
echo "  project #$number"
run gh project edit "$number" --owner "$owner" --visibility PUBLIC \
  --description "The roadmap of $repo: milestones, epics and their sub-issues (ADR 0099)." >/dev/null
# Linking twice is refused with "already linked"; that is the state we want.
if [ "$dry_run" -eq 0 ]; then
  gh project link "$number" --owner "$owner" --repo "$repo" 2>/dev/null || true
else
  echo "would run: gh project link $number --owner $owner --repo $repo"
fi

project_id=$(gh project view "$number" --owner "$owner" --format json | json .id)
fields=$(gh project field-list "$number" --owner "$owner" --format json --limit 100)
area_field=$(printf '%s' "$fields" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const f=JSON.parse(s).fields.find(x=>x.name==="Area");console.log(f?JSON.stringify(f):"")})')
if [ -z "$area_field" ]; then
  echo "+ field Area (single select: $areas)"
  run gh project field-create "$number" --owner "$owner" --name Area \
    --data-type SINGLE_SELECT --single-select-options "$areas" >/dev/null
  if [ "$dry_run" -eq 0 ]; then
    fields=$(gh project field-list "$number" --owner "$owner" --format json --limit 100)
    area_field=$(printf '%s' "$fields" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const f=JSON.parse(s).fields.find(x=>x.name==="Area");console.log(f?JSON.stringify(f):"")})')
  fi
else
  printf '%s' "$area_field" | node -e '
    let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{
      const have=new Set(JSON.parse(s).options.map(o=>o.name));
      const missing=process.argv[1].split(",").filter(a=>!have.has(a));
      if(missing.length) console.log(`  Area is missing the options ${missing.join(", ")}: add them on the board (gh cannot).`);
    })' "$areas"
fi

# Every open issue, with its first area/* label.
gh issue list --repo "$repo" --state open --limit 1000 --json url,labels \
  --jq '.[] | [.url, ([.labels[].name | select(startswith("area/"))][0] // "" | sub("^area/"; ""))] | @tsv' \
  >"${TMPDIR:-/tmp}/rematch-project-issues.tsv"
count=0
tab=$(printf '\t')
while IFS="$tab" read -r url area; do
  [ -n "$url" ] || continue
  count=$((count + 1))
  if [ "$dry_run" -eq 1 ]; then
    echo "would add $url (Area: ${area:-none})"
    continue
  fi
  item=$(gh project item-add "$number" --owner "$owner" --url "$url" --format json | json .id)
  if [ -n "$area" ] && [ -n "$area_field" ]; then
    ids=$(printf '%s' "$area_field" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const f=JSON.parse(s);const o=f.options.find(x=>x.name===process.argv[1]);console.log(o?`${f.id} ${o.id}`:"")})' "$area")
    if [ -n "$ids" ]; then
      set -- $ids
      gh project item-edit --id "$item" --project-id "$project_id" --field-id "$1" \
        --single-select-option-id "$2" >/dev/null
    fi
  fi
  echo "  $url (Area: ${area:-none})"
done <"${TMPDIR:-/tmp}/rematch-project-issues.tsv"
rm -f "${TMPDIR:-/tmp}/rematch-project-issues.tsv"

echo "$count open issue(s) on the board."
[ "$dry_run" -eq 0 ] || echo "Dry run: nothing was changed."
