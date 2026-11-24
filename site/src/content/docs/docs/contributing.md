---
title: Contributing
description: How to help build Rematch with Rust, TypeScript, chess advice or design.
sidebar:
  order: 2
---

The full guide is [CONTRIBUTING.md](https://github.com/Terkea/rematch/blob/main/CONTRIBUTING.md) in the repository; this page is the short version.

## Ways to help

- **Code.** Pick a [good first issue](https://github.com/Terkea/rematch/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) or one marked [help wanted](https://github.com/Terkea/rematch/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22), and comment on it before you start so two people don't build the same thing.
- **Chess knowledge.** Coaches and strong players: open a [coaching idea](https://github.com/Terkea/rematch/issues/new?template=coaching_idea.yml). What should a 1200 practise, and how would a good coach say it? No code needed.
- **Design.** The annotated-scoresheet system is in [`packages/ui/DESIGN.md`](https://github.com/Terkea/rematch/blob/main/packages/ui/DESIGN.md). Critiques with screenshots are welcome.
- **Testing.** Run it on your own games and report what is wrong, unclear or slow with the bug form. Keep personal data and game links you don't want public out of issues.
- **Questions and proposals** go to [Discussions](https://github.com/Terkea/rematch/discussions), before a large change, so we agree on the shape first.

## House rules

- **Tests first.** Write the failing test, then the code. Vitest for TypeScript, `cargo test` for Rust.
- **Rust builds only in Docker.** Never on the host; the commands are in [CLAUDE.md](https://github.com/Terkea/rematch/blob/main/CLAUDE.md).
- **Conventional Commits.** The pull request title becomes the commit on `main` and the changelog line.
- **Every decision gets an ADR** in [`docs/decisions/`](/rematch/docs/decisions/), with the next number.
- **UI follows DESIGN.md.** Tokens only (no raw colours, sizes or fonts), `pnpm design:check` passes, and screenshots at 390 and 1440 pixels, day and night, go with the pull request.
- **Honesty rules hold.** See [the honesty rules](/rematch/docs/honesty/): no number without its count, no "improved" without separated ranges, no AI fact without a reference.
- **chess.com first.** Lichess accounts are deferred ([0040](/rematch/docs/decisions/0040-chesscom-first-defer-lichess-accounts/)); a Lichess import behind a flag is a welcome proposal.
- **No secrets and no personal data** in commits, fixtures, screenshots or issues.
- **Outside code is reviewed as untrusted.** Fork workflows run after the maintainer approves them, with a read-only token. A change to workflows, Dockerfiles, lockfiles, scripts, `CLAUDE.md`, `.claude/` or the coach's prompts is read by hand before the `sensitive-reviewed` label goes on ([0108](/rematch/docs/decisions/0108-reviewing-outside-code/), [security](https://github.com/Terkea/rematch/blob/main/SECURITY.md#how-we-review-outside-code)).

## AI-assisted contributions

Pull requests written with an AI assistant are welcome. [CLAUDE.md](https://github.com/Terkea/rematch/blob/main/CLAUDE.md) and the skills in [`.claude/skills`](https://github.com/Terkea/rematch/tree/main/.claude/skills) are the house rules for people and agents alike. You are responsible for every line you submit: read it, test it and be ready to explain it.

## Community

- [Code of conduct](https://github.com/Terkea/rematch/blob/main/CODE_OF_CONDUCT.md): the Contributor Covenant 2.1.
- [Governance](https://github.com/Terkea/rematch/blob/main/GOVERNANCE.md): one maintainer merges, and how to become a trusted contributor.
- [Support](https://github.com/Terkea/rematch/blob/main/SUPPORT.md) and [security](https://github.com/Terkea/rematch/blob/main/SECURITY.md) (report vulnerabilities privately through GitHub).
- [How we ship](/rematch/docs/sdlc/): from an issue to a release.
