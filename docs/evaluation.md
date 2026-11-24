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

1. **Make the value case** before anything else ([ADR 0107](decisions/0107-experiment-value-gate-and-catalogue.md)): the decision the result informs; what we will do if it is X and what if it is Y; the expected cost in CPU-hours, wall time, AI tokens (USD) and maintainer time; the value score, impact (1 to 3) × uncertainty (1 to 3) ÷ cost (1 to 3); and the cheaper alternative. If both results lead to the same action, the experiment is not run. If a smaller design, existing data, a cached run, a simulation or the literature answers the question, that is done instead.
2. **Propose it** with the "Experiment on the algorithm" issue form: the value case, the question, H0 and H1, the algorithm versions, one primary metric and its ground truth, the minimum effect, the design and the decision rule.
3. **Pre-register it.** Copy [the template](experiments/TEMPLATE.md) to `lab/experiments/EXP-NNNN/preregistration.md` and write `analysis.py` next to it. The power analysis simulates the registered decision rule and sets the number of synthetic players, games and seeds so that a true effect of the minimum size ships at least 80% of the time, and no larger: the smallest sufficient design, never "as big as possible". Where it is decisive, the rule has pre-registered looks with group-sequential or Bayesian stopping boundaries, and a futility boundary, so a run stops when its answer is clear or can no longer come. Section 7 names what the run reuses (cached engine evaluations in `position_evals`, another experiment's synthetic players or cohort, an earlier RunRecord). Section 0 sets a compute budget in counted units (games, nodes, profile calls, tokens), and the run aborts at 150% of it unless section 0 registers more; `results.json` records the budget against the actual (`compute`). The design counts players, never games, because a player's games are not independent of each other: 80 games of one player are one unit, not 80. Every seed is named, and the candidate is developed on seeds disjoint from the confirmatory ones. The analysis plan names the effect size with its intervals and the correction for multiple comparisons, and the sha256 of `analysis.py` goes into section 10. The pull request that adds both is reviewed before anything runs.
4. **Have it criticised.** A fresh reviewer (a person, or an agent with the `scientific-critical-thinking` and `peer-review` skills in `.claude/skills/science/`) reads the pre-registration for room to p-hack, leakage between training and test data, pseudoreplication, and a decision rule that cannot fail. Fixes land before the run, and `review.md` lists them.
5. **Run it once.** `analysis.py` drives the algorithm through `rematch_lab` (`cli lab analyse|profile|rank`, `cli bench run --json`) on the registered seeds and writes `results.json` with the `algorithm_id` it ran on. A Bayesian fit whose diagnostics fail (R-hat above 1.01, bulk or tail ESS below 400 in total across 4 chains, divergences) is a failed result, reported as one.
6. **Decide by the rule written in step 3.** For a comparison: ship when the two-sided 97.5% interval of the primary effect lies wholly on the better side and no guarded KPI crosses its registered non-inferiority margin; reject when the interval lies wholly below the minimum effect; otherwise need more data, which allows one follow-up with the candidate unchanged and no more. Only the primary metric decides. `decision.md` records it, `lab/findings.md` logs it whatever it shows, and a shipped change becomes a new algorithm version in Rust with its `algorithm/CHANGELOG.md` entry citing the EXP id, and an ADR.

When an experiment's stage changes, update its `lab/experiments/EXP-NNNN/summary.md` (front matter `id`, `title`, `status: pre-registered|running|decided`, `algorithm`, `issue`, `updated`, and once decided `live`, `rerun`, `run_cpu_hours` and `run_minutes`, then 3 to 6 bullets); the site rebuilds on push and shows it on its Experiments page and on the landing page. `site/test/lab.test.ts` fails when an experiment has a `decision.md` but a summary whose status is not `decided`, or no summary at all. A release, a decided experiment or a closed milestone also gets an entry in [`docs/progress.md`](progress.md).

The lab CI (`.github/workflows/lab.yml`) checks what it can rather than trusting it. The pre-registration and `analysis.py` must be in a commit strictly older than the one that adds `results.json`. Once results exist, sections 0 to 10 of the pre-registration must read as they did in that commit (a change is a deviation in section 11, or a new experiment), and `analysis.py` must keep the sha256 of section 10. Every input listed in `inputs.sha256` must match. Each experiment is run twice and the two `results.json` must be byte-identical, and equal to the committed file while the experiment's `algorithm_id` is the current one. Once the algorithm moves on, a decided experiment stays live: the arena re-runs it against the champion on its schedule (below), and its registration, freeze and inputs are still checked here. Negative results are published: a rejected change keeps its folder and its line in the findings log. The kill rule: a pre-registered experiment that no longer informs an open decision is closed with a one-line reason in its `summary.md` and the findings log, not run. The lab CI refuses a pre-registration without a complete section 0, except EXP-0001 and EXP-0002, registered before the gate.

The first four experiments answer questions we already have. EXP-0001 sizes the benchmark: how many synthetic players and games per flaw the Tier 1 suite needs to detect a change of 0.1 in recall at 2 (the primary), with the interval width coverage then reaches as a secondary. EXP-0002 measures whether the 80% ranges hold the true rate 80% of the time ([#13](https://github.com/Terkea/rematch/issues/13)). EXP-0003 compares partial pooling across players with the current fixed priors ([#11](https://github.com/Terkea/rematch/issues/11)). EXP-0004 pre-registers the leakage-free design of Tier 2. The methods follow **K-Dense**'s Scientific Agent Skills, nine of which are vendored and pinned (`.claude/skills/science/SOURCE.md`).

## The loop: champion and challenger

The champion is the production algorithm and every change is a challenger, measured on every live experiment with the seeds and players that experiment registered ([ADR 0109](decisions/0109-champion-and-challenger.md)). A decided experiment is never retired by age: its design measures each later version for as long as it informs a decision.

1. **Live, with a schedule.** A decided experiment's `summary.md` says `live: true`, `rerun: per-pr`, `weekly` or `release`, and the measured cost of one run (`run_cpu_hours`, `run_minutes`), and its comparison rule sits in `arena.py` beside `analysis.py`. An experiment that no longer informs a decision says `live: false` and the reason in `retired`. EXP-0002 (0.44 CPU-hours on one core, no engine) is `per-pr`; EXP-0001 (2.44 CPU-hours with 8 engines and **Postgres**, its recall at 2 already gated by the smoke suite and measured weekly by the full one) is `release`, and so is EXP-0007 (0.02 CPU-hours, no engine, but its input is the 1.2 GB Lichess puzzle snapshot).
2. **The arena.** `python -m rematch_lab.arena` runs each live experiment's frozen `analysis.py` for the champion and the challenger on the registered seeds, reusing whatever is cached (the committed `results.json` of the version it ran on, and the leaderboard's records with the same seeds, analysis, inputs and rule; the champion's always from the champion's own commit), and compares them with the experiment's rule. Each metric gets the effect, its interval and a verdict: better, worse or no clear difference. EXP-0001's recall at 2 is better by its registered ship rule (the paired 97.5% Newcombe interval wholly above 0) and worse by ADR 0104's guard (the 80% interval wholly below 0); its coverage is guarded with the registered 0.05 tolerance; its two clean controls are reported and never gate. EXP-0002 registered no comparison, so it uses the one drafted for EXP-0006, adopted by ADR 0109: the largest distance of its 20 coverages from 0.8, with the interval of the maximum, and the coverage at the peers' rate as a guard. EXP-0007's rule is its registered section 9 per tag: the set of tags the rule trusts is the primary, and a challenger may not take a trusted tag's precision interval below 0.80. The arena refuses a comparison when the experiment's own inputs outside the manifest changed between the two commits (EXP-0001's synthetic players in `backend/crates/bench/src`), unless `lab/source-reviews.json` accepts that exact content of the file with an ADR that found the measured path unchanged (ADR 0109 Amendment 1). An experiment whose external input is absent, as on a hosted runner, is reported as "not run: external input missing", never passed; the operator runs it where the snapshot is, as ADR 0109 Amendment 1 sets out, and commits the leaderboard.
3. **The budget comes first.** The arena costs the runs the caches cannot answer before it starts any, and refuses a plan past 1 CPU-hour for a pull request, 4 for the weekly run or 8 for a release. A run stops at 150% of its measured minutes, and each experiment's own `Budget` still caps what it counts (ADR 0107).
4. **The gate.** A pull request that changes `algorithm/manifest.json` runs the `per-pr` experiments against main (`.github/workflows/lab.yml`); the Monday run takes the `weekly` ones against the newest release; `release.yml` runs all of them against the release before, and the images wait for it. A clear regression on a primary or a guarded metric fails the job. A behaviour change ships when the experiment registered for it says ship and the arena shows no clear regression anywhere else; a challenger replaces the champion only when it beats it on the primaries.
5. **The leaderboard.** Every run appends a record to `lab/leaderboard.json`: the version, the experiment, the metrics with their intervals, the champion it was compared with, the verdict and the date. A pull request that bumps the version commits its challenger's record (the job's artifact holds the file), and CI re-runs the challenger and fails when it differs from that record. The [Experiments page](https://terkea.github.io/rematch/docs/experiments/#leaderboard) shows a row per version and a column per live experiment's primary metric, with the champion marked.
6. **Next.** Every `decision.md` ends with a `## Next` whose items each name a roadmap issue (an `EXP-NNNN` id or `Issue: "<title>"`), at least one of them an experiment; `scripts/github/test-settings.mjs` fails a decision without one. The loop closes there: a result names the next experiment, and that experiment's challenger meets the champion on everything decided so far.

The emergency override: when a regression must ship (a security fix that cannot wait), the owner writes an ADR in `docs/decisions/` naming the version, the experiment, the metrics and the reason, and adds the same entry to `lab/overrides.json`. The arena then reports the regression as overridden and passes; an entry whose ADR is missing or does not name the version, the experiment and every metric does nothing. `lab/overrides.json`, `lab/leaderboard.json` and every `arena.py` are sensitive paths owned by the maintainer. A release the arena held back is published by re-publishing its Release once the override is merged.

To run it locally, in the lab image:

```sh
python -m rematch_lab.arena --scope release --dry-run            # the plan and the cached verdicts
python -m rematch_lab.arena --scope per-pr --write               # run what is missing, then record it
python -m rematch_lab.arena --champion-cli /path/to/main/cli --challenger-cli /usr/local/bin/cli
```

## The experiment catalogue

Twelve more experiments, EXP-0007 to EXP-0018, check the learning framework layer by layer; each is an issue that opens with its value case, under one epic per layer ([ADR 0107](decisions/0107-experiment-value-gate-and-catalogue.md)). EXP-0005 and EXP-0006 are the follow-ups of EXP-0001 (the churn between two versions) and EXP-0002 (the fix for the 80% ranges).

- Facts: are the engine's judgements and the motif tags right? EXP-0007 (the motif tagger against the Lichess puzzle themes), EXP-0008 (is 300,000 nodes enough).
- Recommendation: do we tell a player the right thing? EXP-0009 (what actually helps people climb), EXP-0010 (rank by points lost or by frequency), EXP-0011 (are the weekly targets realistic), EXP-0012 (do drills resemble the next mistakes).
- Profile: are the rates, the change states and the trajectory right? EXP-0013 (false weaknesses for a clean player), EXP-0014 (the change detectors), EXP-0015 (the trajectory's "worse" verdict); with EXP-0002, EXP-0003 and EXP-0006.
- Language: is the coach faithful to the facts? EXP-0016 (what the validator misses).
- Learning, which needs real players (v0.6): EXP-0017 (how many players the learning experiments need), EXP-0018 (the review schedule and the retire rule).
- The follow-ups of EXP-0007: EXP-0019 (a blind human audit of the tags the themes cannot judge), EXP-0020 (deflection), EXP-0021 (pin), EXP-0022 (motifs after the first gain and past ply 8), EXP-0023 (the engine's best line against the solution).

The order: the facts first, because every layer above them inherits their errors and EXP-0009's cost depends on EXP-0008's node count; then EXP-0009, the top priority, since it is the only one that can change what we teach; then the rest by value score. The profile experiments need no engine, so they run alongside the facts.

| Run | Experiment | Layer | Impact × uncertainty ÷ cost | Runs |
|---|---|---|---|---|
| 1 | EXP-0007 the motif tagger | facts | 3 × 3 ÷ 1 = 9 | now |
| 2 | EXP-0008 engine depth | facts | 3 × 2 ÷ 2 = 3 | now |
| 3 | EXP-0009 what helps people climb | recommendation | 3 × 3 ÷ 2 = 4.5 | after EXP-0007 and EXP-0008 |
| 4 | EXP-0015 the trajectory's "worse" | profile | 3 × 3 ÷ 1 = 9 | now (a lab-v2 step) |
| 5 | EXP-0013 false weaknesses | profile | 3 × 2 ÷ 1 = 6 | now |
| 6 | EXP-0014 the change detectors | profile | 2 × 3 ÷ 1 = 6 | now (a lab-v2 step) |
| 7 | EXP-0017 learning power | learning | 2 × 3 ÷ 1 = 6 | now |
| 8 | EXP-0010 points lost or frequency | recommendation | 3 × 2 ÷ 1 = 6 | after EXP-0004 and Tier 2 |
| 9 | EXP-0011 weekly targets | recommendation | 2 × 3 ÷ 1 = 6 | after Tier 2 |
| 10 | EXP-0012 drill relevance | recommendation | 2 × 2 ÷ 1 = 4 | after EXP-0007 and Tier 2 |
| 11 | EXP-0016 the validator | language | 2 × 2 ÷ 1 = 4 | now |
| 12 | EXP-0018 the schedule | learning | 2 × 2 ÷ 2 = 2 | needs real players |

Six proposals were merged or dropped by their value case:

- Judgements against Lichess's own merges into EXP-0008: the judgement rule is lila's, so a disagreement can only come from the evaluations, which EXP-0008 measures; Lichess's `[%eval]` comments are its secondary.
- Syzygy agreement and the accuracy formula against `backend/fixtures/accuracy/lila-examples.json` are dropped as experiments: the lab's sub-issues measure both, and a disagreement is a bug to fix whatever its size.
- A simulation of the spacing schedule is dropped: it would return its memory model's assumptions, and the literature already supports expanding intervals. EXP-0018 uses real reviews, and confidence calibration ("Sure" against "A guess") is its secondary.
- The weekly focus effect merges into EXP-0017, which sizes it; it is registered only if EXP-0017 says the users exist.
- Recommendation fidelity is dropped as an experiment: it is a deterministic check with a 100% target (v0.2), and a failure is a bug.
- The validator's drop rate and tone per prompt version are dropped as an experiment: the lab tracks the drop rate per prompt-set hash, a comparison is registered when a prompt change is proposed, and tone needs an LLM judge, which never gates.

## Credits

The ground truths are other people's work. **Stockfish** and its developers give us the evaluations; the **Syzygy** tables, mirrored by Lichess, the endgame truth; the **Lichess** puzzle database, released under CC0 by Lichess and its players, the motif labels; and lila, Lichess's open-source server, the accuracy formula our implementation is checked against. We use their data and published formulas, never their code (ADR-R2-3).
