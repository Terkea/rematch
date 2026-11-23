# 0099. The roadmap as GitHub milestones and spec issues, and scheduled maintenance sweeps

- Status: accepted
- Date: 2026-11-23
- Lane: `chore/roadmap-and-sweeps`

## Context

The repository goes public as `Terkea/rematch`. Until now the roadmap lived in three places that
drifted: `site/src/data/roadmap.ts`, the README's Roadmap section and the ranked list in the
end-user review of 2026-11-23. Contributors need one place to see what is next, a spec they can
build from, and issues sized honestly. The owner asked to "keep track of the roadmap", with "GitHub
issues with the specs", and to "keep things up to date and have regular sweeps".

Two facts shaped the sweeps. `backend/deny.toml` ignores RUSTSEC-2026-0592 until `url` 2.5.9
passes the 7-day crate-age rule on 2026-11-27, and nothing would notice if the ignore outlived its
reason. A first run of the new decision-log check found 0086 citing an R3 decision, number 6, that never
existed (the chat is ADR 0064).

## Decision

1. GitHub is the roadmap's source of truth: milestones are versions with a theme, and each theme is
   an epic issue whose work is linked as sub-issues. `.github/settings/milestones.json` holds the
   milestones (title, description, state and a `now`, `next` or `later` horizon), and
   `.github/settings/roadmap-issues.json` holds the seed: 30 issues, of which 9 are epics with a
   full spec and 4 are good first issues of size S. Each body starts with the player's problem and
   names the files and ADRs to start from.
2. `apply-settings.sh` syncs the milestones by title with the rest of the settings (deleting only
   with `--prune`). `--seed-issues` creates the seed issues whose titles do not exist, open or
   closed, epics first, then links each child with the sub-issues API
   (`POST /repos/{owner}/{repo}/issues/{n}/sub_issues`, by the child's id). It never edits an issue
   that exists: after seeding, GitHub owns it. The dry run prints the whole plan from a read-only
   look at the live repository. `test-settings.mjs` validates both files against each other and
   `labels.json`, and tests the three plans.
3. Specs are a step of the lifecycle: idea, triage, `needs-spec`, a spec issue from the new
   "Feature spec (RFC)" form (`spec`), `accepted`, a milestone, then pull requests. The feature
   request form stays for short ideas. The labels `spec`, `epic`, `needs-spec`, `accepted`,
   `roadmap`, `stale`, `sweep` and `area/infra` join `labels.json`.
4. The site reads the milestones and each epic's `sub_issues_summary` from the public API at build
   time (`site/scripts/roadmap-data.mjs`) and shows Now, Next and Later with done of total. It
   falls back to the seed files, with counts and no progress, whenever the API cannot be read; a
   pull request's CI build always uses the seed (`ROADMAP_OFFLINE=1`). Pages rebuilds daily so
   progress follows the issues. The README carries a summary between markers, rendered from the
   seed by `scripts/roadmap/render-readme.mjs`, and the `ts` job fails when it is stale.
5. A public GitHub Project, "Rematch roadmap" (Status, Milestone, Area), is created by
   `scripts/github/create-project.sh`, idempotent, from the maintainer's login after a one-time
   `gh auth refresh -s project`. `repo.json` turns `has_projects` on so the board shows on the
   repository.
6. Sweeps are scheduled workflows with pinned actions, least permissions and no secrets:
   - `Weekly sweep` (Mondays): lychee over the README, docs and site; `cargo deny check advisories`
     against the day's RustSec database; `check-advisory-ignores.mjs`, which requires
     `# expires: YYYY-MM-DD` on every ignore line and fails the day after; the crate-age rule; the
     README roadmap check; `check-docs.mjs` (the decision log's index matches its files, and every
     ADR mentioned in the docs, README, CLAUDE.md and site pages exists); and `pnpm audit --prod`,
     reported without failing. A failure opens or updates one issue, "Weekly sweep: <date>"; the
     first passing run closes it. Only the report job has `issues: write`.
   - `Stale` (Tuesdays, `actions/stale` v11.0.0): issues at 60 days and pull requests at 30 get
     `stale`; issues close 30 days later; pull requests are never closed automatically. Roadmap,
     spec, epic, accepted, good-first-issue, help-wanted and security items, milestoned items and
     assigned items are exempt.
   - `Monthly sweep` (the 1st): opens "Maintenance sweep: YYYY-MM" from
     `.github/sweeps/monthly-maintenance.md`, once a month.
   - Dependabot keeps its grouped weekly updates with a 7-day cooldown for npm, Cargo, Docker and
     Actions. Cargo pull requests are merged by the maintainer, who owns `Cargo.toml` and
     `Cargo.lock`.

## Alternatives

1. **Keep the roadmap in `roadmap.ts` and the README.** Simple, but it cannot show progress, and
   contributors cannot comment on or claim an item.
2. **A Project board as the source of truth.** Boards need the `project` scope to change and are
   awkward to review as code; milestones and issues work with the default token and the public API.
   The board stays, as a view.
3. **Seeding by syncing issue bodies both ways.** It would overwrite edits made on GitHub, which is
   where discussion happens. Seeding once and then letting GitHub own the issue avoids that.
4. **A sweep that files one issue per finding.** It floods the tracker after a bad week; one
   tracking issue, updated in place, does not.

## Consequences

- Run `sh scripts/github/apply-settings.sh --seed-issues` after the repository is renamed and made
  public (a dry run first). Until the labels and milestones exist, the site and README show the
  seed.
- A new milestone or seed issue is a pull request to the JSON files, checked by `test-settings.mjs`;
  a new issue after seeding is filed on GitHub like any other.
- Every advisory ignore needs a date. The RUSTSEC-2026-0592 ignore expires on 2026-11-27, so the
  weekly sweep of 2026-11-30 fails unless `url` has been bumped.
- The lychee job skips our own repository's URLs and GitHub's search pages, which rate-limit
  anonymous clients; the site's own link check still covers every internal link.

## Amendment 1 (2026-11-23, ADR 0104)

For an open seeded issue, `roadmap-issues.json` is now the source of truth. `--seed-issues` moves it
to its milestone, adds its missing labels and rewrites its body when the text differs, keeping boxes
ticked on GitHub; a closed issue is never edited, and an issue filed by hand stays GitHub's except
for the milestone a milestone adopts (`adopt_issues`). `milestones.json` renames a milestone in place
through `renamed_from`, and closes any other old title it names. Re-ranking is a pull request to the
two files, not an edit on GitHub.
