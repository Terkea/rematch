# EXP-0015: is "clearly worse than your peak" a false alarm?

- Status: pre-registered
- Issue: #110
- Author: the maintainer, with Claude (lane `exp/0015-peak`)
- Kind: measurement (one version, `algo-4.0.1`)
- Follows: none
- Registered: 2026-11-26. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: the trajectory rule (ADR 0089). "Then" is the `W = min(30, floor(n / 2))` games centred on the rating peak (or the first `W` games when the peak is in the last `W`), "now" the last `W`, and their pre-game ratings are compared by `compareMeans` (2,000 bootstrap resamples, the 80% interval of the difference). A "worse" verdict makes the trajectory `falling`: it heads Progress ("Your rapid rating peaked at 1,097 in April 2026 and is 816 now (−281)."), the Overview's `TrajectoryCard`, and the coach's `trajectory` input, which tells the coach to point at a clearly worse measure as the likeliest place to look.
- If the result is that the false-alarm rate is within the bound at 60, 120 and 250 games (section 9), we will keep the rule and publish its false-alarm and detection rates beside it on the site; if it is above the bound at any of them, we will register a challenger rule (one that does not compare with a selected peak, or that counts the rating's memory from game to game) for its own lane and keep the verdict meanwhile; if it is above the hide line, we will also file the production change that stops stating "falling" until a challenger clears this experiment, in its own lane. Nothing in production changes here.
- Expected cost: 1,160,000 simulated games and 5,300 trajectories through `cli lab trajectory`, about 2 CPU-minutes in the lab container (measured on the timing namespace: 5 µs a simulated game, 13 ms a 250-game trajectory); no engine, no database, no AI tokens; about half a day of maintainer time to review.
- Value score: impact 3 × uncertainty 3 ÷ cost 1 = 9
- Cheaper alternative? No. A peak is a selected maximum and a rating is a running sum, so some bias towards "worse" is expected; its size under this window, this bootstrap and a Glicko-2 rating is what decides, and only the production code gives it. No cached run measures the trajectory. EXP-0009's cohort (ADR 0114) cannot stand in: it keeps the first and last five rapid games of each player-month, not the whole series the rule reads, and a real player has no known true strength to score a verdict against.
- Compute budget: 1,160,000 `simulated_games` (Glicko-2 or Elo updates, burn-in included) and 5,300 `trajectory_players` (players sent to `cli lab trajectory`); cap 150%

## 1. Question

For a player whose strength stays the same, how often does Rematch say their rating is clearly worse than it was? Concretely: a 1,500 Lichess rapid player who is exactly as good after 120 rated games as before them, whose rating wanders around 1,500 as ratings do, is told "Your rapid rating peaked at 1,562 in February 2026 and is 1,488 now (−74)." How often does that happen, and how often is a player whose strength really fell by 100 or 200 points over the same games told so?

Rated game counts of 60, 120 and 250 cover the windows production uses: from 60 games on, both windows hold 30 games.

## 2. Hypotheses

- H0: at each of 60, 120 and 250 rated games, the share of constant-strength players whose trajectory is `falling` is at most 0.15 (section 5).
- H1: at one or more of them it is above 0.15. The expected direction is up, for two reasons the rule does not account for: the bootstrap resamples the 30 pre-game ratings of a window as if they were independent, while consecutive ratings differ by a few points and share their history, so the interval is too narrow; and the "then" window is centred on the highest rating of the series, a maximum selected from the same noise.

Rival explanations that would give the same numbers, and how each is separated:

- The simulated rating moves more, or remembers longer, than a real one: an exploratory sensitivity runs the same players under a plain Elo rating with K = 10 and K = 32 (slower and faster than Glicko-2's settled step, which is about K = 15), and the diagnostics report the rating's error around the truth, its game-to-game correlation and the spread of 30-game window means.
- The rating had not settled when the observed games start, so a residual trend reads as a fall: 300 burn-in games come first, and the diagnostics report the mean error and the deviation at the first observed game.
- A new account's rating really moves while it settles, which is not a false alarm about the rating but is one about strength: kept out of the primary, and reported as an exploratory population (no burn-in).
- The world's accuracy and blunders are tied to results, so the peak window's wins carry better accuracy: this is real in chess and is why the accuracy and blunders verdicts are secondaries, not the primary.

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-4.0.1+fcda2762eeeb` | – |

From `cli algo show` on the cli built from main at `ef0789b9`. The rule measured is `profile::coaching::trajectory` (`trajectory_input`, `compute`, `contract`), reached through `cli lab trajectory` (`bench::lab_rules::trajectory`, ADR 0115).

## 4. Primary metric and ground truth

- Primary metric (exactly one family, Bonferroni across its three cells): the share of settled, constant-strength players whose trajectory has `kind == "falling"` (`thenVsNow.rating.verdict == "worse"`), at 60, 120 and 250 rated games. The step called is `cli lab trajectory` (`rematch_lab.trajectory`). Both headlines of a `falling` trajectory count: "peaked at … and is … now" (basis `peak`) and "is … now, down from an average of … over your first N rated games" (basis `start`); the split is reported.
- Ground truth: the true strength, constant by construction. Each player has one series of 300 burn-in games and 250 observed games, played from a fresh account (1500, deviation 500, volatility 0.09); the request at `g` games is the first `g` observed games, as a player with `g` rated games in Rematch.
- Secondary metrics:
  - confirmatory: the share of the same players whose accuracy verdict is "worse", and whose blunders-per-game verdict is "worse", at 60, 120 and 250 games (six shares), each tested against the nominal 0.10 by a one-sided exact binomial test, Holm across the six;
  - confirmatory: the detection rate, the share of players told `falling` whose true strength falls linearly by 100 or by 200 points across the observed games, at 60, 120 and 250 games (six cells, 200 players each, a fresh series for each cell after the same burn-in), with Wilson intervals at 1 − 0.05/6;
  - exploratory: the primary at 30 games (W = 15); the split by basis; the false "rising" share; the score verdict "worse"; any of the rating, accuracy, blunders and score rows "worse"; the mean `change` when `falling`; new accounts (no burn-in), overall and by a true strength below or above 1500; Elo K = 10 and K = 32 at 120 games; the diagnostics of the world.
- Not measured: the weakness rows of then against now (no weaknesses are sent; EXP-0013 and EXP-0014 cover the weakness rates); chess.com's storage convention (the step's `chesscom: true` shifts the same series by one game).

## 5. The minimum effect that matters

- Minimum effect: 0.05 above the nominal 0.10, so the acceptable false-alarm rate is 0.15. The hide line is 0.30.
- Why this size: "worse" is called when the 80% two-sided interval of the difference excludes 0, which promises at most 10% "worse" for a player whose rating is not moving (ADR 0083 §6: "better" or "worse" only when the evidence separates). Another 0.05 is one stable player in twenty told their rating is falling beyond that promise, the tolerance EXP-0002 used around its 0.80 (±0.05). The hide line: at a false-alarm rate of 0.30, if a fifth of active players were truly declining and 80% of them were caught, false "falling" headlines would outnumber true ones (0.8 × 0.30 = 0.24 against 0.2 × 0.8 = 0.16). The base rate of real declines is an assumption we have not measured; the line is three times the nominal, and stated here before the run.

## 6. Power analysis

- Method: a simulation of the section 9 rule on binomial counts (`power.py`, output `power.json`, 20,000 draws a point): Wilson intervals at 1 − 0.05/3 per cell, the three cells drawn independently. The cells are the same players read at three lengths, so they are positively correlated, which makes "keep" (all three must clear) more likely than simulated; the simulation is the harder case.
- Inputs: the true false-alarm rate in each cell, from 0.05 to 0.40; two-sided intervals; the player as the unit.
- Result: 700 settled players, the smallest of 200 to 800 in steps of 100 at which every power below reaches 0.80:
  - "keep" when the rate is the nominal 0.10 in all three cells: 0.83 (600 players: 0.73);
  - "change" or "hide" when the rate is 0.20 in one cell and 0.10 in the others: 0.88; when 0.20 in all three: 0.998;
  - "hide" when the rate is 0.40 in one cell: 0.9996; when 0.35 in all three: 0.96.
  - The detection cells (200 players each) are descriptive: the Bonferroni (6) half-width is at most 0.093.
- Sensitivity: `power.json` has every grid point; the half-width of a cell at 0.10 is 0.027 with 700 players.

## 7. Design

- Unit of analysis: the player. Games are nested in a player's series and are never counted as replicates; each player gives one verdict per checkpoint.
- The world, simulated by `analysis.py` (the data-generating process, as EXP-0002 simulates its counts; the rule under test is only the cli's):
  - true strength per player from Normal(1500, 300), clipped to [700, 2300];
  - Glicko-2 as Lichess runs it, one game a rating period (Glickman 2013, the Illinois step): τ 0.75, a fresh account at 1500 with deviation 500 and volatility 0.09, the deviation kept in [45, 500] and the volatility at most 0.1;
  - each game's opponent rated Normal(player's rating, 75), with a true strength Normal(their rating, 50) and a deviation of 60;
  - the result from the true strengths' expected score E, with draws 4 × 0.06 × E(1 − E) and the expected score kept;
  - accuracy Normal(79 after a win, 75 after a draw, 69 after a loss, plus 0.02 per point of change in true strength since the first observed game; SD 8), clipped to [10, 99] and rounded to 0.1; blunders Poisson(0.7, 0.9 or 1.5 by result, times exp(−0.002 per point of that change));
  - every game rated, analysed and finished, two games a day from 2026-01-05 09:00 UTC; the stored rating is the pre-game rating rounded to an integer, sent with `chesscom: false`, as Lichess stores it;
  - each request's bootstrap seed is production's form, `trajectory:<account id>:rapid`, the account id a UUID from `sha256("<namespace>/account/<player>")`; no weaknesses.
- Populations: 700 settled constant-strength players (300 burn-in games, 250 observed, read at 30, 60, 120 and 250 games); for each decline of 100 and 200 points and each of 60, 120 and 250 games, 200 players with a fresh series (300 burn-in games at constant strength, then the observed games with the true strength falling linearly by the decline from the first observed game to the last); 300 new accounts at constant strength with no burn-in, read at 60, 120 and 250 games; and for Elo K = 10 and K = 32, 200 settled constant-strength players each (300 burn-in games, 120 observed).
- Pairing and blocking: none between populations; the checkpoints of a population are nested (the same players).
- Seeds: `seed(namespace, name)` for each name in `analysis.py`'s `SEED_NAMES`: `settled`; `decline/100/60`, `decline/100/120`, `decline/100/250`, `decline/200/60`, `decline/200/120`, `decline/200/250`; `new-account`; `elo/10`, `elo/32`. The registered namespace is `EXP-0015`.
- Development and confirmatory data: there is no candidate and nothing is tuned. Before registration, two runs checked the code and timed it, on the namespaces `EXP-0015-timing` (40 settled players, timing only) and `EXP-0015-smoke` (8 settled players, every population at a few players), printing no share of any verdict.
- Held fixed: `algo-4.0.1`; the world above; no weaknesses.
- Data: no file inputs (`inputs.sha256`); no real player's games.
- Leakage controls: not a predictive question; the truth is the simulation's.
- Reuse: none. No earlier run measures the trajectory, and EXP-0009's cohort does not hold full series (section 0).

## 8. Analysis plan

- Model or test: shares. For each primary cell, the share `falling` with Wilson intervals at 95%, 80% and 1 − 0.05/3 (98.33%, Bonferroni across the three cells); each cell is read against the bound and the hide line by its Bonferroni interval:
  - "above the hide line": the lower end above 0.30;
  - "above the bound": the lower end above 0.15 (and not above 0.30);
  - "within the bound": the upper end at most 0.15;
  - "undecided": anything else.
- Effect size: each share with the intervals above; for the confirmatory secondaries, the share with Wilson 95% and 80% intervals, the one-sided exact binomial p against 0.10 and its Holm adjustment across the six; for detection, the share with Wilson 95% and 1 − 0.05/6 intervals and the mean observed rating fall.
- Multiple comparisons: Bonferroni across the three primary cells; Holm across the six accuracy and blunders shares; Bonferroni across the six detection cells. Exploratory results get Wilson 95% intervals and no correction, and decide nothing.
- Diagnostics: no fitted model. The world's diagnostics (the mean and SD of the rating's error around the truth, its lag-1 correlation, the deviation and the absolute error at the first observed game, the SD of 15- and 30-game window means of the error) are reported; a mean error at the first observed game above 25 points in absolute value is reported as a burn-in that did not settle.
- Exclusions and missing data: none. A request whose trajectory is not sufficient stops the run (it cannot happen at 30 games or more).

## 9. Decision rule

Measurement on one version, read on the primary's three cells (section 8), in this order:

1. **Hide the verdict until fixed:** any cell is "above the hide line". We file, as its own lane, the production change that stops stating `falling` (the Progress and Overview headline and the coach's `kind`) until a challenger clears this experiment, and register the challenger.
2. **Change the threshold:** otherwise, any cell is "above the bound". We register the challenger (a rule that does not compare with a selected peak, or that accounts for the series' memory) as the next experiment, in its own lane; the verdict stays meanwhile.
3. **Keep the threshold:** every cell is "within the bound". The rule stays, and its false-alarm and detection rates are published beside it.
4. **Need more data:** anything else. At most one follow-up, with `Follows:` this experiment, new seeds and its own power analysis; a second "need more data" keeps the rule.

Only the primary decides. The detection rate is reported whatever it shows and is the guard of the comparison below.

The comparison rule for a challenger (the arena, ADR 0109), on the registered seeds, the player as the unit and the two versions paired on the same players:

- Primary, each of the three cells: the change in the false-alarm share, challenger minus champion, with the paired Newcombe interval (method 10) at 1 − 0.05/3. Better when it lies wholly below 0, worse when wholly above 0.
- Guard: the detection rate at a 200-point decline, the 600 players of its three cells pooled, with the paired Newcombe interval at 80%. Worse when it lies wholly below −0.10: a fix may give up some detection, not more than one real decline in ten.
- Reported, never gating: the detection rate at a 100-point decline, pooled the same way.

### Stopping rules

- Early stopping: none. The run takes minutes; a look would save nothing.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0015/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `06489676604b577f476045e40b391c66b642e2adacb75f0e7090aa92c8f33e07`

## 11. Deviations

None.
