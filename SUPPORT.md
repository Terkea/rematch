# Getting help

Rematch has one maintainer, so ask where everyone can see the answer.

- Questions, setup trouble, "is this expected?": ask in
  [Discussions Q&A](https://github.com/Terkea/rematch/discussions/categories/q-a). Say what you ran,
  what you saw, your OS and how much memory Docker has.
- Ideas you want to talk through: [Discussions Ideas](https://github.com/Terkea/rematch/discussions/categories/ideas).
- A bug: open a [bug report](https://github.com/Terkea/rematch/issues/new?template=bug_report.yml).
- A security problem: report it privately, as [SECURITY.md](SECURITY.md) describes.
- The docs: <https://terkea.github.io/rematch/>, and [CONTRIBUTING.md](CONTRIBUTING.md) for
  running it locally.

The most common problem is memory: the full stack needs a 16 GB machine with about 12 GB
assigned to Docker. If containers restart or tests fail with connection errors, check
Docker's memory limit first.

There is no paid support and no response-time promise. Answers come from the maintainer and the
community when they can. Please be patient and kind; the [Code of Conduct](CODE_OF_CONDUCT.md)
applies everywhere.
