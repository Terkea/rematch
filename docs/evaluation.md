# How Rematch is evaluated

Every recommendation Rematch makes is meant to be a fact, derived by one versioned, deterministic algorithm, and every sentence the coach writes is checked against those facts. This page says what we measure, against which truth, and how to propose a change to the algorithm. The design is in `docs/superpowers/specs/2026-11-24-factual-deterministic-evaluation-design.md` and the decision in [ADR 0104](decisions/0104-factual-and-deterministic-four-layers.md). The work runs from v0.2 to v0.4 on the [roadmap](https://github.com/Terkea/rematch/milestones); what exists today is the Tier 1 benchmark ([the method and the scorecard](benchmark.md)).

## Four layers, four ground truths

| Layer | What it is | What it is measured against |
|---|---|---|
| Facts | Engine evaluations, accuracy, judgements, motif tags, weakness events | **Stockfish 19** at a pinned build and node count; the **Syzygy** tablebases, exact for 5 pieces or fewer; the **Lichess** puzzle database's themes (CC0) for the motif tagger; the lila accuracy cases in `backend/fixtures/accuracy/lila-examples.json` |
| Profile | Rates per chance with 80% ranges, comparisons with your peers, trends | Synthetic players whose true rates we chose (Tier 1), and whether the 80% ranges hold the true rate about 80% of the time |
| Recommendation | The ranked weaknesses, the weekly focus, the drills | Tier 1: is the flaw we injected ranked first? Tier 2: on anonymised Lichess players, does the top weakness predict where points are lost later? Tier 4: do drilled players miss fewer held-out positions? |
| Language | The report, the coach's answers, the weekly review | Deterministic checks on every text: is every number cited, how many statements did the validator drop, and is the recommendation the text names the one that was computed? |

The language model never produces a fact. It writes citation tokens, the server fills in the numbers, and the validator drops anything uncited (the [honesty rules](https://terkea.github.io/rematch/docs/honesty/)). Its wording varies from run to run; which recommendation it names may not. We may report an LLM judge's score of a text, but it never decides whether a change ships, because it can score the same text twice and disagree.

## Determinism

The same games and the same algorithm version give byte-identical facts, profile rows and coaching input. The engine runs one thread with a 16 MB hash, starts every search with `ucinewgame`, and stops at a fixed node count (300,000 for the main analysis). Every random draw in the statistics comes from a generator seeded by the data, so the same games always give the same 80% range. A replay suite (`backend/crates/algo/tests/replay.rs`) runs the 19 committed fixture games through import, analysis, enrichment, the profile build and the coaching input twice, in two throwaway databases, and compares canonical dumps with each other and with the goldens in `backend/fixtures/replay/`; a difference fails CI unless the major version moved. A test also bans unseeded randomness and the wall clock in the maths modules, in Rust (`backend/crates/algo/tests/ban.rs`) and TypeScript (`apps/web/test/determinism.test.ts`).

## What we publish

The lab is built in v0.3; today the Tier 1 scorecard in [benchmark.md](benchmark.md) is published.

- Quality: per-motif precision and recall, tablebase agreement, the accuracy formula's difference from lila, recall at 1 and 2, top-1 precision, false alarms on clean players, calibration, predictive validity, and the language checks.
- Cost: engine CPU-seconds, positions and cache hits per game; AI cost per report, answer and weekly review; a modelled cost per active player per month.
- Latency: per-stage p50 and p95, import to analysed per game, profile build time, report time, the coach's first word and last, and games an hour per core.

Every number comes with its 80% interval over seeds and players. Each run is stored on the `bench-history` branch with the algorithm version, the commit, the machine and the seeds, and the site's Evaluation page charts each one over time, coloured by algorithm version.

## Proposing a new algorithm version

A change to a constant, threshold, detector, protocol, prompt, model or rule is a new algorithm version. `algorithm/manifest.json` names every component, `backend/crates/algo/tests/manifest.rs` fails when it no longer matches the code, and `scripts/algorithm/check.mjs` fails a pull request when a component changes without the version bump and a changelog entry ([ADR 0105](decisions/0105-algorithm-manifest-stamping-and-replay.md)).

1. **Change the code** on a branch, and say in an issue what you expect to improve and by how much, before you run anything.
2. **Bump the version.** Major when facts, profile or recommendations can change; minor when only the coach's prompts, model or validator change; patch when nothing a replay can see changes. Regenerate the manifest with `cargo run -p cli -- algo manifest --write`, and the replay goldens with `REPLAY_UPDATE=1` for a major bump.
3. **Run the benchmark** on the same seeds as the reference, then `cli bench compare <reference> <yours>`. It prints each KPI's difference with an 80% interval and a verdict: better, worse, or no clear difference.
4. **Write the changelog entry** in `algorithm/CHANGELOG.md`: what changed, why, the ADR, and the comparison table.
5. **Open the pull request.** CI reruns the deterministic suites and fails on a clear regression in a guarded KPI, which is one whose 80% interval of the difference lies wholly on the worse side.

A version that makes one KPI better and another worse is a decision, not a fault: say which you chose and why, in the ADR.

## Credits

The ground truths are other people's work. **Stockfish** and its developers give us the evaluations; the **Syzygy** tables, mirrored by Lichess, the endgame truth; the **Lichess** puzzle database, released under CC0 by Lichess and its players, the motif labels; and lila, Lichess's open-source server, the accuracy formula our implementation is checked against. We use their data and published formulas, never their code (ADR-R2-3).
