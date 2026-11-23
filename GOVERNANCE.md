# Governance

One maintainer decides, in the open, and writes the reasons down.

## The maintainer

Rematch has one maintainer, [@Terkea](https://github.com/Terkea), who:

- sets the direction and the roadmap;
- triages issues and reviews pull requests;
- is the only person who merges into `main` (enforced by the ruleset in `.github/settings/`);
- cuts releases, and decides when Rematch is ready for `1.0.0`;
- enforces the [Code of Conduct](CODE_OF_CONDUCT.md).

## How decisions are made

- Day to day: in issues and pull requests, in the open. The maintainer has the final say, and
  explains it.
- Anything someone will later ask "why?" about gets an Architecture Decision Record in
  `docs/decisions/`. Anyone can propose one in a pull request. A record is never deleted; a later
  one supersedes it.
- Bigger changes (a new platform, a new service, a change to the licence or to how the coach
  works) start in [Discussions](https://github.com/Terkea/rematch/discussions) so the community can
  weigh in before code is written.

## Becoming a trusted contributor

There are no formal roles yet. Trust comes from merged pull requests and help given to others:

- A few pull requests that follow [CONTRIBUTING.md](CONTRIBUTING.md) and pass review with little
  back-and-forth.
- Help for others: answers in Discussions, reviews of pull requests, reproduced bugs.
- Good judgement about scope, honesty in the numbers, and the design system.

Trusted contributors are asked to review pull requests in their area and are listed here. When
the project grows, this document will grow too: more maintainers with merge rights, recorded in
an ADR. If you want to help steer, say so in Discussions.

## Trusted contributors

None yet.

## Licence

Rematch is under the [GNU AGPL-3.0](LICENSE). Contributions are accepted under the same licence;
there is no contributor licence agreement.
