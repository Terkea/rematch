# Progress

<!--
Add an entry when a release ships, an experiment is decided, or a milestone closes.
Format: `## YYYY-MM-DD: title`, then 2 to 4 hyphen bullets with real numbers and links. Newest first.
The site builds this file into /docs/progress/ and shows the newest entry on the landing page.
-->

See also the [experiments](/rematch/docs/experiments/) and the [roadmap](/rematch/docs/roadmap/).

## 2026-11-24: EXP-0007, the motif tags against the Lichess puzzle themes

- We ran the real Rust tagger on 54,987 Lichess puzzles (CC0, many thanks to Lichess and its players), each solution standing in for the engine's best line, and scored it against Lichess's themes. Fork is right 98% of the time and finds 72% of the labelled forks; discovered attack 99% and 86%; promotion and smothered mate match almost exactly ([EXP-0007](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0007)).
- Six tags can't be shown to be right four times in five: deflection (56%, mostly attraction) is ours to fix; skewer (81%, not shown to clear 80%), back-rank mate, hanging piece, sacrifice and pin wait for a blind human audit, since the themes don't name every motif in a puzzle. Pin also finds only 20% of the labelled pins.
- Next: stop naming those six to players until they clear, the audit (EXP-0019), fixes for deflection and pin (EXP-0020, EXP-0021), motifs after the first capture (EXP-0022), and the engine's own line (EXP-0023).

## 2026-11-24: The first two experiments are decided

- EXP-0001 sized the benchmark with a power analysis on the real Rust pipeline. Reliably seeing a 0.1 gain in "finds the flaw in its top 2" needs 40 synthetic players per flaw; the weekly budget allows 22, which reliably sees 0.15. The suites were resized to match (smoke 1,280 games, full 8,960), and 10 of the pilot's 11 misses are missed mates and early traps, so that's where gains have to come from ([EXP-0001](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0001)).
- EXP-0002 checked whether the 80% ranges hold 80% of the time. For players well above their peers, 12 of 20 weakness-by-sample-size cells fall outside 75–85%, worst for missed mates at 30 games (10%). The ranges are pulled towards the peers, so they miss high ([EXP-0002](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0002), [#13](https://github.com/Terkea/rematch/issues/13)).
- Both were pre-registered, reviewed by an independent critic before and after the run, and re-run to byte-identical results. Next: EXP-0005 measures how much the rankings move between two real versions, and EXP-0006 tests a fix for the priors.

## 2026-11-23: The algorithm science lab and algo-1.1.0

- Every change to the recommendation algorithm now needs a pre-registered experiment: the question, H0 and H1, the primary metric, the minimum effect, the sample size and the decision rule are committed before anything runs ([ADR 0106](decisions/0106-algorithm-science-lab.md), [evaluation](evaluation.md#how-we-do-science-on-the-algorithm)).
- The lab in `lab/` drives the real Rust algorithm from Python (`cli lab analyse|profile|rank`), so no second implementation exists. Lab CI checks the commit order in git, the frozen sections, the input hashes and that two runs give byte-identical results.
- Nine vetted scientific skills are vendored and pinned, and the first four experiments are planned (EXP-0001 to EXP-0004).
- `algo-1.1.0` is the first version bump through the new guard: a refactor in the language layer with no output change, replay goldens byte-identical ([changelog](https://github.com/Terkea/rematch/blob/main/algorithm/CHANGELOG.md)).

## 2026-11-23: Tier 1 benchmark, and the ranking fix

- The Tier 1 benchmark runs the production pipeline on synthetic players with injected flaws of known rates, five flaws and one clean control, Stockfish at 12,000 nodes ([ADR 0100](decisions/0100-pipeline-benchmark-synthetic-players.md), [benchmark](benchmark.md)). Its first scorecard found two of the five flaws in the top 2, and the clean control had a rank-1 weakness.
- [ADR 0101](decisions/0101-weakness-ranking-floor-and-baselines.md) ranks a weakness only when its 80% range lies above the benchmark and it costs at least 0.02 points a game, and adds peer baselines for hanging pieces (W1) and thrown-away wins (W5), which had none and so were never ranked.
- Before and after: flaw in the top 2 from 40% to 60% (the target is 80%), the clean control with a rank-1 weakness from 1 of 1 to 0 of 1, mean games to a top-2 detection from 55 to 45.
- Still failing, and published: the 80% ranges hold the true rate 60% of the time (12 of 20 intervals), down from 80% and below the 65% threshold, and missed mates are measured but not ranked.

## 2026-11-23: A versioned recommendation algorithm

- Every recommendation now names the rules that produced it: `algo-<version>+<hash>`, from `algorithm/manifest.json`, which lists about thirty constants, protocols, prompt hashes and model ids ([ADR 0105](decisions/0105-algorithm-manifest-stamping-and-replay.md)). The first version is `algo-1.0.0`.
- A replay suite imports 19 fixture games and runs analysis, the profile and the coaching input twice; both runs must equal the committed goldens byte for byte.
- A CI guard fails a pull request that changes a component without the version bump and a changelog entry in `algorithm/CHANGELOG.md`, and a ban test fails any unseeded randomness or clock read in the algorithm's crates.

## 2026-11-23: Factual and deterministic, evaluation first

- [ADR 0104](decisions/0104-factual-and-deterministic-four-layers.md) splits the product into four layers (facts, statistics, recommendations, language), each with its own ground truth: **Stockfish**, **Syzygy** tablebases, the Lichess puzzle database and lila's accuracy formula ([evaluation](evaluation.md)).
- The roadmap was rewritten evaluation-first: measure what the algorithm does before changing it.
- Stockfish now gets `Threads 1` and `Hash 16` explicitly, instead of relying on defaults that happen to equal them; the evaluations are identical with and without.

## 2026-11-23: Open source as Rematch

- The repository went public as Rematch under the AGPL-3.0, with a README, community files, a contributing guide and a release process ([ADR 0096](decisions/0096-public-face-readme-and-site.md)).
- This site went live on GitHub Pages at terkea.github.io/rematch, built from `site/` on every push to `main`.
- `main` is protected by a ruleset: changes arrive by pull request with the required checks passing, and the repository settings live in `.github/settings` as code ([ADR 0095](decisions/0095-open-source-sdlc-and-releases.md)).

## 2026-11-23: Self-hosting in one script

- `scripts/setup.sh` merges `.env`, checks Docker, memory, disk and ports, starts the services the app needs and waits for the worker's migrations; `doctor` re-checks everything and explains each failure ([ADR 0097](decisions/0097-self-hosting-setup-and-demo-data.md)).
- A demo player (`pnpm seed:demo`) and the data downloads (Syzygy tables, the Lichess reference) are part of the setup, so a fresh clone is not an empty app.
- Written after a clean-clone run on Windows 11 found six snags, among them `nvm use` ignoring `.nvmrc` and a missing shared cargo volume stopping `docker compose up`.
