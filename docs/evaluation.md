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

1. **Pre-register the experiment** when the change alters behaviour (below), then change the code on a branch. A refactor, a comment or a copy change needs only an issue.
2. **Bump the version.** Major when facts, profile or recommendations can change; minor when only the coach's prompts, model or validator change; patch when nothing a replay can see changes. Regenerate the manifest with `cargo run -p cli -- algo manifest --write`, and the replay goldens with `REPLAY_UPDATE=1` for a major bump.
3. **Run the benchmark** on the same seeds as the reference, then `cli bench compare <reference> <yours>`. It prints each KPI's difference with an 80% interval and a verdict: better, worse, or no clear difference.
4. **Write the changelog entry** in `algorithm/CHANGELOG.md`: what changed, why, the ADR, and the comparison table.
5. **Open the pull request.** CI reruns the deterministic suites and fails on a clear regression in a guarded KPI, which is one whose 80% interval of the difference lies wholly on the worse side.

A version that makes one KPI better and another worse is a decision, not a fault: say which you chose and why, in the ADR.

## How we do science on the algorithm

A behaviour change ships only with a pre-registered experiment behind it: the question, the hypothesis, the metric, the smallest effect that matters, the sample size, the analysis and the decision rule are written down and committed before anything runs (ADR 0106). A behaviour change is any change that can move the replay goldens or a Tier 1 KPI, which is a major algorithm version (ADR 0104): a threshold, the ranking, a detector, a prior, a protocol. A refactor, a comment or a copy change is not one; the unchanged goldens prove it. The experiments run the real Rust algorithm from Python, in the lab (`lab/`), so the numbers we analyse are the numbers the product would produce. No Python re-implementation exists and none runs in the product.

1. **Propose it** with the "Experiment on the algorithm" issue form: the question, H0 and H1, the algorithm versions, one primary metric and its ground truth, the minimum effect, the design and the decision rule.
2. **Pre-register it.** Copy [the template](experiments/TEMPLATE.md) to `lab/experiments/EXP-NNNN/preregistration.md` and write `analysis.py` next to it. The power analysis simulates the registered decision rule and sets the number of synthetic players, games and seeds so that a true effect of the minimum size ships at least 80% of the time. The design counts players, never games, because a player's games are not independent of each other: 80 games of one player are one unit, not 80. Every seed is named, and the candidate is developed on seeds disjoint from the confirmatory ones. The analysis plan names the effect size with its intervals and the correction for multiple comparisons, and the sha256 of `analysis.py` goes into section 10. The pull request that adds both is reviewed before anything runs.
3. **Have it criticised.** A fresh reviewer (a person, or an agent with the `scientific-critical-thinking` and `peer-review` skills in `.claude/skills/science/`) reads the pre-registration for room to p-hack, leakage between training and test data, pseudoreplication, and a decision rule that cannot fail. Fixes land before the run, and `review.md` lists them.
4. **Run it once.** `analysis.py` drives the algorithm through `rematch_lab` (`cli lab analyse|profile|rank`, `cli bench run --json`) on the registered seeds and writes `results.json` with the `algorithm_id` it ran on. A Bayesian fit whose diagnostics fail (R-hat above 1.01, bulk or tail ESS below 400 in total across 4 chains, divergences) is a failed result, reported as one.
5. **Decide by the rule written in step 2.** For a comparison: ship when the two-sided 97.5% interval of the primary effect lies wholly on the better side and no guarded KPI crosses its registered non-inferiority margin; reject when the interval lies wholly below the minimum effect; otherwise need more data, which allows one follow-up with the candidate unchanged and no more. Only the primary metric decides. `decision.md` records it, `lab/findings.md` logs it whatever it shows, and a shipped change becomes a new algorithm version in Rust with its `algorithm/CHANGELOG.md` entry citing the EXP id, and an ADR.

When an experiment's stage changes, update its `lab/experiments/EXP-NNNN/summary.md` (front matter `id`, `title`, `status: pre-registered|running|decided`, `algorithm`, `issue`, `updated`, then 3 to 6 bullets); the site rebuilds on push and shows it on its Experiments page and on the landing page. `site/test/lab.test.ts` fails when an experiment has a `decision.md` but a summary whose status is not `decided`, or no summary at all. A release, a decided experiment or a closed milestone also gets an entry in [`docs/progress.md`](progress.md).

The lab CI (`.github/workflows/lab.yml`) checks what it can rather than trusting it. The pre-registration and `analysis.py` must be in a commit strictly older than the one that adds `results.json`. Once results exist, sections 1 to 10 of the pre-registration must read as they did in that commit (a change is a deviation in section 11, or a new experiment), and `analysis.py` must keep the sha256 of section 10. Every input listed in `inputs.sha256` must match. Each experiment is run twice and the two `results.json` must be byte-identical, and equal to the committed file while the experiment's `algorithm_id` is the current one. An experiment that ran on an older version is historical: its registration, freeze and inputs are still checked, but it is not rerun on an algorithm it never measured. Negative results are published: a rejected change keeps its folder and its line in the findings log.

The first four experiments answer questions we already have. EXP-0001 sizes the benchmark: how many synthetic players and games per flaw the Tier 1 suite needs to detect a change of 0.1 in recall at 2 (the primary), with the interval width coverage then reaches as a secondary. EXP-0002 measures whether the 80% ranges hold the true rate 80% of the time ([#13](https://github.com/Terkea/rematch/issues/13)). EXP-0003 compares partial pooling across players with the current fixed priors ([#11](https://github.com/Terkea/rematch/issues/11)). EXP-0004 pre-registers the leakage-free design of Tier 2. The methods follow **K-Dense**'s Scientific Agent Skills, nine of which are vendored and pinned (`.claude/skills/science/SOURCE.md`).

## Credits

The ground truths are other people's work. **Stockfish** and its developers give us the evaluations; the **Syzygy** tables, mirrored by Lichess, the endgame truth; the **Lichess** puzzle database, released under CC0 by Lichess and its players, the motif labels; and lila, Lichess's open-source server, the accuracy formula our implementation is checked against. We use their data and published formulas, never their code (ADR-R2-3).
