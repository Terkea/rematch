# The pipeline benchmark

Rematch's pipeline finds three of the five injected flaws and gives the clean control no weakness. Since EXP-0001 (2026-11-23) the smoke suite has three players per flaw: hanging pieces (W1), low-clock blunders (W9b) and thrown-away wins (W5) are ranked first for all nine of their players and their drills fit; missed mates (W4.missed) are measured but ranked for none of three, and early blunders (W7.traps) reach the top 3 for one of three, behind W1. The 80% intervals hold the true rate 65% of the time (39 of 60), at the bottom of the 65% to 95% band. These are known gaps, not tuned away:

- resolved: W1 and W5 had no benchmark, so they were never ranked ([#10](https://github.com/Terkea/rematch/issues/10)). W1 is now compared with the opponents' refutation lines from the cached main evals, and W5 with what the evaluation predicts from a winning episode (ADR 0101);
- resolved: ranking had no floor, so the clean control's W9b (11 of 846 against a peer median of 0.008) ranked first ([#12](https://github.com/Terkea/rematch/issues/12)). A weakness now ranks only when its 80% range lies above the benchmark and it costs at least 0.02 points per game;
- the W4.missed peer row is low-sample at 80 games ([#11](https://github.com/Terkea/rematch/issues/11)), so `bench-missmate`'s rank 1 is W1: 0.0043 against its opponents' 0.0018, the bias of comparing with players who only played you;
- `bench-opening`'s injected opening blunders mostly hang a piece. Each move's cost goes once, to the first key in W4 > W2 > W1 > W3 > W8 > W7 > W10, so their cost is W1's and W1 ranks first. W7.traps keeps 1.2 of the 31 events' points lost, and its peer row's EPL per event is 0, so it is not ranked. No issue yet;
- coverage is 65%, and W1's intervals miss: W1's detector reads 0.017 against a true rate of 0.023 for `bench-hang`, and the interval is shrunk towards the opponents' rate (0.0008, prior strength 300 chances). EXP-0002 measured the prior alone: with exact counts, a W1 player at that rate is covered 53% of the time at 30 games ([#13](https://github.com/Terkea/rematch/issues/13); the fix is EXP-0006).

## The current scorecard

Stockfish at 12000 nodes per search. Tier 1 (synthetic players). Method: `docs/benchmark.md`.

| Score | Value | Threshold | Result |
|---|---|---|---|
| (a) Flaw in the top 2 (recall) | 60% | at least 80% | fail |
| (a) Flaw ranked first | 60% | – | – |
| (a) Top-1 precision | 64% | – | – |
| (a) Clean controls with a rank-1 weakness | 0 of 1 | at most 0 | pass |
| (b) 80% intervals holding the true rate | 65% (60 intervals) | 65% to 95% | pass |
| (c) Top weakness's drills train the flaw | 60% | – | – |
| (d) Mean games to a top-2 detection | 42 | – | – |
| (e) Same data, same weaknesses | yes | – | – |
| (e) Top-2 overlap without 10% of the games | 93% | – | – |

Thresholds: not all met.

| Player | Flaw (rate) | Target | True rate | Estimate [80%] | Rank | Top 3 | Drills hit | Games to detect | Note |
|---|---|---|---|---|---|---|---|---|---|
| bench-hang | hang (0.05) | W1 | 0.023 | 0.017 [0.014, 0.020] | 1 | W1 | yes | 45 |  |
| bench-hang-2 | hang (0.05) | W1 | 0.021 | 0.019 [0.015, 0.022] | 1 | W1 | yes | 30 |  |
| bench-hang-3 | hang (0.05) | W1 | 0.023 | 0.021 [0.017, 0.025] | 1 | W1 | yes | 30 |  |
| bench-missmate | miss_mate (0.5) | W4.missed | 0.400 | 0.304 [0.155, 0.467] | – | W1 | no | – | not ranked: no peer benchmark for this key |
| bench-missmate-2 | miss_mate (0.5) | W4.missed | 0.450 | 0.545 [0.386, 0.700] | – | none | no | – | not ranked: no peer benchmark for this key |
| bench-missmate-3 | miss_mate (0.5) | W4.missed | 0.500 | 0.581 [0.419, 0.739] | – | W5, W9b | no | – | not ranked: no peer benchmark for this key |
| bench-opening | opening_blunder (0.3) | W7.traps | 0.309 | 0.247 [0.194, 0.302] | – | W1 | no | – | not ranked: not enough evidence yet |
| bench-opening-2 | opening_blunder (0.3) | W7.traps | 0.309 | 0.220 [0.170, 0.273] | 3 | W1, W5, W7.traps | no | 60 | ranked 3 |
| bench-opening-3 | opening_blunder (0.3) | W7.traps | 0.309 | 0.324 [0.252, 0.398] | – | W1 | no | – | not ranked: no peer benchmark for this key |
| bench-lowclock | low_clock_blunder (0.25) | W9b | 0.103 | 0.088 [0.070, 0.107] | 1 | W9b, W1 | yes | 30 |  |
| bench-lowclock-2 | low_clock_blunder (0.25) | W9b | 0.087 | 0.068 [0.052, 0.085] | 1 | W9b, W1, W9a | yes | 45 |  |
| bench-lowclock-3 | low_clock_blunder (0.25) | W9b | 0.111 | 0.094 [0.075, 0.114] | 1 | W9b, W1 | yes | 30 |  |
| bench-noconvert | no_convert (0.5) | W5 | 0.545 | 0.526 [0.379, 0.671] | 1 | W5 | yes | 60 |  |
| bench-noconvert-2 | no_convert (0.5) | W5 | 0.581 | 0.593 [0.454, 0.727] | 1 | W5 | yes | 45 |  |
| bench-noconvert-3 | no_convert (0.5) | W5 | 0.605 | 0.460 [0.329, 0.591] | 1 | W5, W1 | yes | 45 |  |
| bench-clean | clean | – | – | – | – | none | – | – |  |

The run took 8 minutes 50 seconds with 8 engines: 195,945 Stockfish searches, 157,660 requests served from the cache. The first player of each flaw gives the same row as the six-player suite before EXP-0001. `bench/baseline.json` holds these numbers.

## What it measures

The benchmark asks one question of Rematch's analysis: given a player whose weakness we put there ourselves, at a rate we chose, does the pipeline find it, rank it first, size it correctly and point the plan at it? Tier 1 answers it with synthetic players. Tiers 2 to 4 are planned and described at the end. They measure accuracy; the speed and cost of each pipeline stage are measured in the v0.3 evaluation lab ([the evaluation method](evaluation.md)).

Tier 1 lives in `backend/crates/bench` (ADR 0100). It runs the production code, in-process, against a throwaway **Postgres** database:

1. the `analyse-game` handler (`analysis::jobs::analyse`) with the production profile hook (`profile::hook_impl`): **Stockfish** evaluation, the enrichment (best and refutation lines, threats, MultiPV on critical plies), move features, per-game metrics and weakness series;
2. the `build-profile` handler (`profile::build_profile`) with the default peer benchmark (ADR 0080): weakness estimates, posteriors and priorities;
3. the CoachingInput builder (`profile::coaching::build_coaching_input_report`): the ranked weaknesses the coach, the weekly focus and the drills read;
4. the drill catalogue (`ai::output::drills::drills_for`) for the top weakness. The AI-written text is not run and not scored.

## The synthetic players

Every move of every game, on both sides, comes from a **Stockfish 19** search of the position (4 lines, 12,000 nodes, one thread). The clean player picks among those lines with weights `exp(-loss / 2.5)` and never plays a line that loses 8 or more win-percentage points against the best one; a forced mate is always played (`bench::policy`). A clean player therefore makes inaccuracies but no mistakes or blunders (the detectors' decisive threshold is 15 points, `DECISIVE_DW`).

A flawed player is the clean player plus one injector (`bench::flaws`). Each injector names the weakness it targets, counts that weakness's opportunities the way its detector does, and, when its coin comes up, asks the engine for a move that does the damage:

| Flaw | Target | What is injected | Opportunity |
|---|---|---|---|
| `hang` | W1 | at rate p per eligible move after ply 20 on a clock of 60 s or more, a move that costs at least 20 points and whose refutation captures a knight, bishop, rook or queen, not a mate | eligible own move |
| `miss_mate` | W4.missed | at rate p, a move that keeps no forced mate when the best line mates in 1 to 4 | own move with a mate in 1 to 4 |
| `opening_blunder` | W7.traps | in a share p of games, one move costing at least 20 points between plies 7 and 20 | game |
| `low_clock_blunder` | W9b | at rate p per eligible move under 30 s after ply 20, a move costing at least 20 points | eligible own move under 30 s |
| `no_convert` | W5 | in a share p of games that reach a winning episode (3 own moves at 85% or more from ply 21), moves costing at least 20 points until the player is at 50% or less | game with a winning episode |

"Eligible" is the detectors' rule: the mover's win percentage within 40 points of 50.

The games are chess.com rapid games (10 minutes, no increment), in the importer's PGN shape with a `[%clk]` comment after every move. Each side draws a pace for the game, and about a third of the games reach a clock under 30 seconds. A side resigns after two own moves at minus 10 pawns or a mate against it, from ply 30. Each player meets 16 fixed opponents rated 1400 to 1600 (the peer benchmark needs 10 with evidence), and plays two games a day, so 80 games span 40 days.

Each game has its own seed (mulberry32, `profile_core::stats::rng`). The engine sends `ucinewgame` before every node-limited search, so the same seed gives the same game: `tests/pipeline.rs` generates a game twice and compares them.

All engine requests go through one in-memory cache (`bench::memo`). The analysis and the enrichment are served the generator's own searches of the same positions, so the pipeline judges each injected move on the numbers the injector saw, and a smoke run spends its time on one search per position. The searches run at 12,000 nodes, not the production protocol's 300,000 (`engine::ENGINE_NODES`); the stored protocol string is unchanged, which only matters inside the throwaway database.

## The true rate

The scorer needs the rate the pipeline should report for the targeted weakness. The generator keeps four counts per player: opportunities, armed opportunities, attempts (coin came up) and successes (the engine found a move that does it). The injected rate is

`p * (successes / attempts) * (armed / opportunities)`

and the true rate adds the clean control's own rate `b` of the same weakness (search noise between two node-limited searches can make a clean move read as decisive): `1 - (1 - injected) * (1 - b)` (`bench::flaws::Tally::truth`).

## The five scores

1. **Diagnosis (a).** Recall at 2: the share of flawed players whose target is in the CoachingInput's top 2 ranked weaknesses at the last checkpoint. Top-1 precision: of all players with a rank-1 weakness, the share whose rank 1 is their injected flaw (a clean control with a rank-1 weakness counts against it). Clean false alarms: clean controls with any rank-1 weakness.
2. **Calibration (b).** The share of the target's 80% intervals (the `weakness_estimates` row, every checkpoint of every flawed player) that hold the true rate. Well calibrated is about 80%; the threshold band is 65% to 95%.
3. **Plan targeting (c).** The share of flawed players whose top-1 weakness's drills (`drills_for`) include a drill for their flaw, for example `mate_patterns` for `miss_mate`. A player with no rank-1 weakness gets no drill and counts as a miss.
4. **Data efficiency (d).** The first checkpoint (30, 45, 60, 80 games) at which the target is in the top 2. Under 30 games nothing is ranked (`MIN_GAMES`).
5. **Stability (e).** Determinism: the CoachingInput built twice from the same rows and clock gives the same weaknesses. Perturbation: each player is rebuilt as a second account with about 10% of its games left out, and the top 2 are compared (mean Jaccard overlap).

The smoke suite's thresholds are recall at 2 of at least 0.8, no clean false alarm, and coverage between 0.65 and 0.95 (`bench::results::THRESHOLDS`). They were set before the first run and are not tuned to pass.

## Running it

The benchmark needs `DATABASE_URL` (a database `bench_<uuid>` is created next to it and dropped afterwards) and `STOCKFISH_PATH`. In the Rust dev container both are set:

```sh
COMPOSE_FILE=docker-compose.yml docker compose -p ci-bench run --rm -e LANE=bench rust-dev \
  cargo run --locked -p cli -- bench run --suite smoke --out /work/bench-out
COMPOSE_FILE=docker-compose.yml docker compose -p ci-bench down -v
```

Options: `--suite smoke|full`, `--json` (print `bench-results.json` instead of the scorecard), `--out DIR`, `--baseline FILE` (exit 2 on a regression), `--engines N` (parallel Stockfish processes, default 4), `--nodes N`, `--keep-db`. The run writes `bench-results.json` (the scorecard, the suite, and every player's games summary, tallies, rankings and estimate rows per checkpoint) and `bench-scorecard.md`.

The suites:

- `smoke` (CI): three players per flaw above and one clean control, 80 games each (16 players, 1,280 games), checkpoints at 30, 45, 60 and 80 games, and the perturbation. The first player of each flaw keeps its old name and seed (`bench-hang`, seed 11).
- `full` (weekly): 22 players per flaw at the smoke rates and two clean controls, 80 games each (112 players, 8,960 games), checkpoints at 30, 45, 60 and 80 games. Its checkpoints give the detection curves (games to detection).

The sizes come from EXP-0001 ([the decision](../lab/experiments/EXP-0001/decision.md)): with the player as the unit, detecting a change of 0.1 in recall at 2 80% of the time needs about 40 players a flaw at 80 games under the assumed churn, more than the weekly budget of 9,000 games allows; the full suite detects 0.15 that often, and the smoke suite is a tripwire for large drops. The rate sweep left the full suite so that the recall it scores is the recall that was sized.

`.github/workflows/bench.yml` runs the smoke suite on pull requests that touch `enrich`, `profile`, `profile-core`, `coaching-input` or `bench`, compares it with `bench/baseline.json` and fails on a regression beyond the baseline's tolerance: a fall in recall at 2 of more than 0.1 (two net lost detections of 15) or coverage moving more than 0.05 further from 0.8, both from EXP-0001. It runs the full suite every Monday and on demand. Both upload the scorecard and the results as an artifact. The absolute thresholds are reported, not enforced, because the first baseline does not meet them (below).

## What Tier 1 does not show

- The games are not human. The clean policy is an engine with a loss cap, so the profile it gives is not a typical 1500 player's. The clock model is harsh: 18 to 35 of each player's 80 games end on time, and W9a (getting short of time) sits near 70% for everyone, flawed or not.
- Search noise reaches the detectors. Two node-limited searches can disagree, so a capped move sometimes reads as decisive: the clean control has 14 W1 events in 4,274 eligible moves and 11 W9b events in 846. The true rate includes this noise through the clean control's rate (above). The clean control's old false alarm came from it; the ranking floor (ADR 0101) now holds it back.
- Not every weakness is covered. There is no Maia (W11 needs it), no tablebase (W8), no injector for W2, W3, W6, W7.prep, W9a, W9c or W10, and no colour split. Each is a new injector type in `bench::flaws`.
- Few players. The smoke suite has 15 flawed players, so one detection is about 7 points of recall, and it detects only large drops (a fall of 0.2 four times in five, EXP-0001). The full suite's 110 detect a change of 0.15 four times in five, not 0.1.
- 12,000 nodes, not 300,000. Weaker searches than production make the lines noisier; the conclusions about ranking and benchmarks do not depend on depth, the exact rates do.

## Planned tiers

- Tier 2, predictive validity: anonymised real players from the Lichess database dump (the `lichess-db` pipeline already streams it, ADR-R2-9): build the profile from games before a date T and test whether the top weakness predicts where the points are lost after T, against the base rates.
- Tier 3, plan quality: a rubric for the written plan, scored by an LLM judge and by a blind panel of coaches, against a baseline plan (a generic "do tactics" plan). It needs the AI text, so it stays out of CI.
- Tier 4, real-user efficacy: the trainer's held-out positions (ADR 0090): whether players who drill a weakness miss fewer of its held-out positions than before, with the "Is it working?" rules.
