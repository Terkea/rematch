# ADR-WEB-9. Third-party design tooling review and the ban lint

Date: 2026-11-21
Status: Accepted

## Context

M4 wants automated design QA (spec ban list B5-B18, token rules TOK1/TOK2) and two third-party aids were proposed: Impeccable (design anti-pattern detector) and Vercel's `web-design-guidelines` skill. Both had to be reviewed before they enter the repo or CI.

## Decision

### Ban lint (ours)

`scripts/ban-rules.ts` holds the rules, `scripts/ban-lint.ts` scans `apps/web/src` and `packages/ui/src` (`.ts`, `.tsx`, `.css`), and `pnpm design:check` runs it (exit 1 on findings). Rules: B12, B13, B5a, B5b, B5c, B6, B7, B15, B18, TOK1, TOK2.

- `apps/web/src/app/lab/**` is exempt (controller ruling): lab pages 404 in production and exist to try off-token looks.
- TOK2 does not flag container-query breakpoints (`@min-[560px]:`): they are breakpoints, not lengths, and Tailwind has no token for them yet. Real arbitrary lengths are flagged.
- The first run found four TOK2 hits in `area-score.tsx` and `weakness-timeline.tsx` (`mt-[9px]`, `w-[3px]` x2, `rounded-[2px]`). They were replaced by the equivalent scale and token classes (`mt-2.25`, `w-0.75`, `rounded-board`); computed values are unchanged.

### Impeccable: NOT adopted in `design:check`

Reviewed `impeccable@4.10.0` (npm, Apache-2.0, author Paul Bakaus), tarball only, never installed or run.

- Tarball: `impeccable-4.10.0.tgz`, 17958 bytes, SHA-512 `784c96fa5262c301daea6090118a082ed03df1a4dfba2233eb5a04164ba9befb5f631e7f52875e7b7c67ad4c2285c2f0e7f5f10bb7a2e155741f85ec874f28c7` (npm integrity `sha512-eEyW+lJiwwHa6mCQEYoILtA98aTfuiIz61oEFkupvvtfYx5/Uodee3xnrUwihcLw5/XxC7ei4VV0H4Xsh08oxw==`).
- No lifecycle scripts (`postinstall` etc.). `bin` is `cli/bin/cli.js`, a 140-line shim.
- The shim does not contain the detector. It execs a native binary, found in this order: `$IMPECCABLE_BIN`, the `@impeccable/cli-<os>-<arch>` optional dependency, `~/.impeccable/bin/<version>/`, then a download from `github.com/pbakaus/impeccable/releases` into that cache (sha-256 verified against a manifest shipped in the tarball; fails closed).
- So `detect` is closed-box native code: it cannot be audited here for network calls, file writes or telemetry. The shim itself writes only to the user cache, and sets `IMPECCABLE_SELF`.
- Version 4.10.0 was published 2026-11-18, under our 7-day age rule (4.9.2 is five days old; 4.8.2, 2026-11-09, would pass). The platform binaries are also separate unpinned-by-us optional packages.
- Verdict: not clean (unauditable binary, too new). `design:check` runs ban-lint only. Revisit if the engine source is reviewed or a vetted version is at least 7 days old and its binary digest recorded.
- Recommendation (owner's call, no settings changed): the Impeccable Claude Code plugin may be installed by the owner for interactive design critique; it is not part of CI.

### Vercel `web-design-guidelines`: vendored

- Source `vercel-labs/agent-skills`, `skills/web-design-guidelines`, commit `5a37a6329fbad3666c3207392a6a7c0110e92efb` (2026-11-20). Licence MIT (repo README; the rules repo `vercel-labs/web-interface-guidelines` is MIT too).
- It does not fetch at runtime: SKILL.md says "Do not fetch rules from the network" and reads `references/command.md`, pinned upstream to `web-interface-guidelines@434b7f9...` with SHA-256 `d246b026...0234`. We verified that digest on the vendored copy.
- Vendored to `.claude/skills/web-design-guidelines/` (SKILL.md with a header naming commit and licence, plus the `references/command.md` it reads).

### Deferred

gsap-skills: deferred to M5 (gsap is also on the B12 ban list for now).

## Consequences

- `pnpm design:check` is cheap, offline and deterministic.
- Impeccable's detector coverage is absent; the ban list is the gate.
- Rules are regex-based: they can miss class names built dynamically and may flag hex-like strings that are not colours. Fix by tightening the rule, not by exempting files.
