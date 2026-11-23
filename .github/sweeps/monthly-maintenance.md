The monthly maintenance sweep for {{MONTH}} (ADR 0099). Tick each line when it is done, or write
what was found next to it; close the issue when every line has an answer. The weekly sweep covers
links, advisories, crate age, the README roadmap and the decision log's index, so this list is what
a machine cannot judge.

## Decisions

- [ ] Read the ADRs touched by last month's merges. Mark any that no longer describe the code as
      superseded, in a new ADR, and update `docs/decisions/README.md`.
- [ ] Check the dated exceptions: `# expires:` in `backend/deny.toml`, `release-as` in
      `release-please-config.json` (removed after the release it names).

## Toolchains and pins

- [ ] Node: `.nvmrc` against the current Node 22 LTS release; pnpm in `package.json` and the
      workflows.
- [ ] Rust: `backend/rust-toolchain.toml` and the image in `backend/Dockerfile` against the latest
      stable release at least 7 days old.
- [ ] Stockfish: `STOCKFISH_SHA256` in `backend/Dockerfile` and `apps/worker/Dockerfile` against
      the latest official release (ADR 0007). A new version changes evaluations, so it gets its
      own ADR.
- [ ] Maia: `services/maia/requirements.lock` and the model it downloads (ADR-R2-13).
- [ ] Syzygy: the 3-4-5 manifest `backend/crates/lichess-db/data/syzygy-345.sha256` still verifies
      against its source (ADR-R2-12).
- [ ] Dependabot: merge or close every open dependency pull request. Cargo bumps are merged by the
      maintainer, who owns `Cargo.toml` and `Cargo.lock`.

## Quality

- [ ] Visual baselines: open the latest `e2e` run's report and check no baseline was regenerated
      to hide a real change.
- [ ] Docs accuracy: follow the site's getting-started page on a clean machine or container, and
      read the README's claims against the app.
- [ ] Benchmark scorecard: add this month's row from the maintainer's machine and compare the trend
      (the v0.3 evaluation lab).

## Roadmap and community

- [ ] Re-rank the roadmap: move epics between milestones in `.github/settings/roadmap-issues.json`,
      and milestones between Now, Next and Later (`horizon` in `.github/settings/milestones.json`).
      Then run `sh scripts/github/apply-settings.sh --seed-issues` and
      `node scripts/roadmap/render-readme.mjs`.
- [ ] Triage: no issue older than a week still has `needs-triage`; every `spec` issue has had a
      maintainer reply.
- [ ] Good first issues: at least three open, each with the files to start from.
- [ ] Security policy: `SECURITY.md` matches how reports are handled, and private vulnerability
      reporting is on.
