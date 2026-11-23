# The pipeline benchmark

Rematch's pipeline finds two of the five injected flaws today. On the first smoke run (2026-11-23, branch `feat/pipeline-benchmark`), early blunders (W7.traps) and low-clock blunders (W9b) are ranked first and their drills fit. Hanging pieces (W1), missed mates (W4.missed) and thrown-away wins (W5) are measured correctly but never ranked, and a clean control is given a rank-1 weakness from noise. The 80% intervals hold the true rate 80% of the time (16 of 20). These are known gaps with issues, not tuned away:

- W1 and W5 have no peer benchmark, so they are never ranked ([#10](https://github.com/Terkea/chess-improvement-engine/issues/10));
- the W4.missed peer row is low-sample at 80 games ([#11](https://github.com/Terkea/chess-improvement-engine/issues/11));
- ranking has no confidence or effect floor, so the clean control's W9b (11 of 846 against a peer median of 0.008) ranks first ([#12](https://github.com/Terkea/chess-improvement-engine/issues/12)).

## The current scorecard

Stockfish at 12000 nodes per search. Tier 1 (synthetic players). Method: `docs/benchmark.md`.

| Score | Value | Threshold | Result |
|---|---|---|---|
| (a) Flaw in the top 2 (recall) | 40% | at least 80% | fail |
| (a) Flaw ranked first | 40% | – | – |
| (a) Top-1 precision | 40% | – | – |
| (a) Clean controls with a rank-1 weakness | 1 of 1 | at most 0 | fail |
| (b) 80% intervals holding the true rate | 80% (20 intervals) | 65% to 95% | pass |
| (c) Top weakness's drills train the flaw | 40% | – | – |
| (d) Mean games to a top-2 detection | 55 | – | – |
| (e) Same data, same weaknesses | yes | – | – |
| (e) Top-2 overlap without 10% of the games | 83% | – | – |

Thresholds: not all met.

| Player | Flaw (rate) | Target | True rate | Estimate [80%] | Rank | Top 3 | Drills hit | Games to detect | Note |
|---|---|---|---|---|---|---|---|---|---|
| bench-hang | hang (0.05) | W1 | 0.023 | 0.019 [0.016, 0.023] | – | W9b | no | – | not ranked: no peer benchmark for this key |
| bench-missmate | miss_mate (0.5) | W4.missed | 0.400 | 0.304 [0.155, 0.467] | – | none | no | – | not ranked: no peer benchmark for this key |
| bench-opening | opening_blunder (0.3) | W7.traps | 0.309 | 0.247 [0.194, 0.302] | 1 | W7.traps | yes | 80 |  |
| bench-lowclock | low_clock_blunder (0.25) | W9b | 0.103 | 0.088 [0.070, 0.107] | 1 | W9b, W7.traps | yes | 30 |  |
| bench-noconvert | no_convert (0.5) | W5 | 0.545 | 0.526 [0.379, 0.671] | – | W9b | no | – | not ranked: no peer benchmark for this key |
| bench-clean | clean | – | – | – | – | W9b, W4.allowed | – | – | false alarm: W9b at rank 1 |

The run took 5 minutes 3 seconds with 4 engines: 72,972 Stockfish searches, 58,097 requests served from the cache. `bench/baseline.json` holds these numbers.

## What it measures

The benchmark asks one question of Rematch's analysis: given a player whose weakness we put there ourselves, at a rate we chose, does the pipeline find it, rank it first, size it correctly and point the plan at it? Tier 1 answers it with synthetic players. Tiers 2 to 4 are planned and described at the end.

Tier 1 lives in `backend/crates/bench` (ADR 0098). It runs the production code, in-process, against a throwaway **Postgres** database:

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

- `smoke` (CI): six players, 80 games each (one per flaw above and one clean control), checkpoints at 30, 45, 60 and 80 games, and the perturbation.
- `full` (weekly): 160 games per player, the rate sweep `hang` 0.02/0.05/0.1, `miss_mate` 0.25/0.5/0.75, `opening_blunder` 0.15/0.3/0.5, `low_clock_blunder` 0.1/0.25/0.4, `no_convert` 0.5, two clean controls, checkpoints up to 160 games. Its results give the detection curves (rate against games to detection).

`.github/workflows/bench.yml` runs the smoke suite on pull requests that touch `enrich`, `profile`, `profile-core`, `coaching-input` or `bench`, compares it with `bench/baseline.json` and fails on a regression beyond the baseline's tolerance. It runs the full suite every Monday and on demand. Both upload the scorecard and the results as an artifact. The absolute thresholds are reported, not enforced, because the first baseline does not meet them (below).

## What Tier 1 does not show

- **The games are not human.** The clean policy is an engine with a loss cap, so the profile it gives is not a typical 1500 player's. The clock model is harsh: 18 to 35 of each player's 80 games end on time, and W9a (getting short of time) sits near 70% for everyone, flawed or not.
- **Search noise reaches the detectors.** Two node-limited searches can disagree, so a capped move sometimes reads as decisive: the clean control has 14 W1 events in 4,274 eligible moves and 11 W9b events in 846. The true rate includes this noise through the clean control's rate (above), and the clean control's false alarm comes from it.
- **Not every weakness is covered.** There is no Maia (W11 needs it), no tablebase (W8), no injector for W2, W3, W6, W7.prep, W9a, W9c or W10, and no colour split. Each is a new injector type in `bench::flaws`.
- **One run, one seed per flaw.** The smoke suite has five flawed players, so one detection is 20 points of recall. The full suite's second seeds and rate sweep are what narrow it.
- **12,000 nodes, not 300,000.** Weaker searches than production make the lines noisier; the conclusions about ranking and benchmarks do not depend on depth, the exact rates do.

## Planned tiers

- **Tier 2, predictive validity.** Anonymised real players from the Lichess database dump (the explorer pipeline already streams it, ADR-R2-11): build the profile from games before a date T and test whether the top weakness predicts where the points are lost after T, against the base rates.
- **Tier 3, plan quality.** A rubric for the written plan, scored by an LLM judge and by a blind panel of coaches, against a baseline plan (a generic "do tactics" plan). It needs the AI text, so it stays out of CI.
- **Tier 4, real-user efficacy.** The trainer's held-out positions (ADR 0090): whether players who drill a weakness miss fewer of its held-out positions than before, with the "Is it working?" rules.
