# Security policy

Thank you for helping keep Rematch and the people who use it safe.

## Reporting a vulnerability

**Please don't open a public issue for a security problem.** Report it privately through GitHub:

1. Go to the repository's [Security tab](https://github.com/Terkea/rematch/security).
2. Choose **Report a vulnerability** ([direct link](https://github.com/Terkea/rematch/security/advisories/new)).
3. Describe the problem, how to reproduce it, and what an attacker could do with it.

Only the maintainer can read the report. No email address is published; GitHub private
vulnerability reporting is the one channel.

What to expect:

- an acknowledgement within **7 days**;
- an assessment and a plan, or questions, within **14 days**;
- a fix released as soon as it is ready, with credit to you in the advisory if you want it.

Please give us a reasonable time to fix the problem before you disclose it, and don't access,
change or delete other people's data while testing. Testing against your own local stack is
always fine.

## Supported versions

Rematch is in beta (`0.y.z`). Only the latest release and `main` get security fixes.

| Version | Supported |
| --- | --- |
| latest `0.y.z` release | yes |
| older releases | no, please upgrade |

## Scope

In scope: this repository's code, its Docker images and compose files, and the CI workflows.

Out of scope: chess.com, Lichess, Stockfish, Anthropic and other third-party services and
software (report those to their owners), and findings that need a compromised host.

## How the project protects itself

- `main` is protected by a ruleset kept as code in `.github/settings/`: pull requests only,
  required CI checks, no force pushes. Only the maintainer merges.
- CI uses no secrets, and fork pull requests run with a read-only token.
- Dependencies are pinned exactly, must be at least 7 days old, and Rust crates are checked by
  `cargo deny` for licences and advisories.
