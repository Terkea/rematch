---
title: How we ship
description: The life of a change in Rematch, from an issue to a beta release, and how versions work.
sidebar:
  order: 3
---

The full process is [`docs/SDLC.md`](https://github.com/Terkea/rematch/blob/main/docs/SDLC.md), and the reasons behind it are decision [0095](https://github.com/Terkea/rematch/blob/main/docs/decisions/0095-open-source-sdlc-and-releases.md). In short:

```
idea, triage, needs-spec, spec, accepted, milestone, pull request, CI green, review, squash merge,
release PR, release
```

1. **Issue.** Everything starts from a form: a bug, a feature request (the problem a player has and the evidence a fix helps) or a coaching idea. Questions go to Discussions.
2. **Triage.** The maintainer labels it by type and area, and marks it `good first issue` or `help wanted` when anyone can take it. A feature then needs a spec: an issue from the "Feature spec (RFC)" form with the problem, the evidence, the design and checkable acceptance criteria. Once the maintainer labels it `accepted`, it joins a milestone on the [roadmap](/rematch/docs/roadmap/), often as an epic with sub-issues.
3. **Fork and branch.** Contributors work in a fork, on a branch named for what it does.
4. **Pull request.** A Conventional Commit title, the template's checklist, one idea per pull request.
5. **CI.** Every check is required: formatting, the design ban lint, build, lint, typecheck and tests for every package, the end-to-end and visual tests, the Rust gate, and this site's build.
6. **Review and merge.** Only the maintainer merges to `main`, as a squash with the pull request's title. The branch protection is kept as code in `.github/settings/`.
7. **Release.** release-please collects the merged titles into a release pull request with the next version and the `CHANGELOG.md` entry. Merging it tags the release.

## Versions

Rematch has one product version, shared by the app and the backend.

- **0.y.z is a beta.** Every release below 1.0 is published as a GitHub prerelease tagged `v0.y.z`, and the app shows it as "Rematch v0.y.z · beta". While the major is 0, features and fixes bump the patch, and a breaking change bumps the minor.
- **1.0.0 is the MVP.** It is a deliberate maintainer decision, made in its own ADR, not something a commit triggers.
- **The first release is 0.1.0.**

## Definition of done

A change is done when its test was written first and passes, every gate in CI is green, a decision it made has an ADR, UI changes pass `pnpm design:check` with screenshots in both themes, and nothing secret or personal is in it. The full list is in [`docs/SDLC.md`](https://github.com/Terkea/rematch/blob/main/docs/SDLC.md).
