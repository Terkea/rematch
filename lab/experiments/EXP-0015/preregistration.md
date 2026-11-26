# EXP-0015: is "clearly worse than your peak" a false alarm?

- Status: pre-registered
- Issue: #110
- Author: the maintainer, with Claude (lane `exp/0015-peak`)
- Kind: measurement (one version, `algo-4.0.1`)
- Follows: none
- Registered: 2026-11-26. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: the trajectory rule (ADR 0089). "Then" is the `W = min(30, floor(n / 2))` games centred on the rating peak (clamped so it ends before "now"; the first `W` games when the peak is in the last `W`), "now" the last `W`, and their pre-game ratings are compared by `compareMeans` (2,000 bootstrap resamples, the 80% interval of the difference). A "worse" verdict makes the trajectory `falling`: it heads Progress ("Your rapid rating peaked at 1,097 in April 2026 and is 816 now (−281)."), the Overview's `TrajectoryCard`, and the coach's `trajectory` input, which tells the coach to point at a clearly worse measure as the likeliest place to look; "better" makes it `rising` ("up from an average of …"), and the then | now table prints "Worse" or "Better" for the rating row either way.
- If the result is that the false-alarm rate is within the bound at 60, 120 and 250 games (section 9), we will keep the rule and publish its false-alarm and detection rates beside it on the site; if it is above the bound at any of them, we will register a challenger rule for its own lane and keep the verdict meanwhile; if it is above the hide line, we will also file the production change that stops stating the verdict until a challenger clears this experiment, in its own lane. The no-peak read and the 60-game cell (section 2) say which challenger: one that counts the rating's memory from game to game, or one that stops comparing with a selected peak. Nothing in production changes here.
- Expected cost: 1,322,000 simulated games and 8,100 trajectories through `cli lab trajectory`, about 3 CPU-minutes in the lab container (measured on the timing namespace: 5 µs a simulated game, 13 ms a 250-game trajectory); no engine, no database, no AI tokens; about half a day of maintainer time to review.
- Value score: impact 3 × uncertainty 2 ÷ cost 1 = 6
- Cheaper alternative? No. The direction is expected (section 2), and a reviewer's rough approximation (an Elo world with a normal stand-in for the bootstrap, not the cli) put the rate far above the bound, which is why uncertainty is 2, not 3. What only the production code gives is the rate itself, the headline we would publish beside the rule, whether false "rising" is as common, and which of the two mechanisms carries it, which chooses the challenger; the run also gives the arena its champion baseline on registered seeds. No cached run measures the trajectory. EXP-0009's cohort (ADR 0114) cannot stand in: it keeps the first and last five rapid games of each player-month, not the whole series the rule reads, and a real player has no known true strength to score a verdict against.
- Compute budget: 1,322,000 `simulated_games` (rating updates, burn-in included) and 8,100 `trajectory_players` (players sent to `cli lab trajectory`); cap 150%

## 1. Question

For a player whose strength stays the same, how often does Rematch say their rating is clearly worse than it was? Concretely: a 1,500 Lichess rapid player who is exactly as good after 120 rated games as before them, whose rating wanders around 1,500 as ratings do, is told "Your rapid rating peaked at 1,562 in February 2026 and is 1,488 now (−74)." How often does that happen, how often is the same player told the opposite, and how often is a player whose strength really fell by 100 or 200 points over the same games told so?

The estimand is the player's strength. The two windows' ratings do differ for a constant-strength player, so "worse" can be true of the ratings; it is a false alarm because the product reads it as strength: the coach is told to look for the likeliest reason in a clearly worse measure (ADR 0089 §7), and the headline is read as "I got worse". Rated game counts of 60, 120 and 250 cover the windows production uses: from 60 games on, both windows hold 30 games.

## 2. Hypotheses

- H0: at each of 60, 120 and 250 rated games, the share of constant-strength players whose trajectory is `falling` is at most 0.15 (section 5).
- H1: at one or more of them it is above 0.15. The expected direction is up, well past the bound: the reviewer's approximation (`review.md`) gave 0.37 to 0.82 across Elo K = 12 to 28. Two mechanisms push it:
  - memory: the bootstrap resamples the 30 pre-game ratings of a window as if they were independent, while consecutive ratings differ by a few points and share their history, so the interval is about the spread within a window (a few points over √30) while the difference between windows varies with the rating's wander (tens of points);
  - selection: the "then" window is placed around the highest rating of the series, a maximum chosen from the same noise.
- The two are separated without a second rule. At 60 games the windows are always the first and the last 30 games (the clamp leaves no room to move "then"), so the 60-game cell carries memory alone, with false "rising" about as common as false "falling". The exploratory no-peak read sends the production rule the first and the last 30 games of the 120- and 250-game series (60 games, so the same fixed halves); the primary minus the no-peak read at the same length is what the peak adds.

Rival explanations that would give the same numbers, and how each is separated:

- The simulated rating moves more, or remembers longer, than a real one: exploratory arms run the same design under a plain Elo rating with K = 10 and K = 32 and under a chess.com-like Glicko-1 with the stored rating taken after the game (`chesscom: true`), at all three lengths; the diagnostics report the rating's error around the truth, its lag-1 correlation, the mean deviation, the mean step a game, and the spread of 15- and 30-game window means.
- The rating had not settled when the observed games start, so a residual trend reads as a fall: 300 burn-in games come first, and the diagnostics report the signed mean error at the first observed game for every population, with a flag past 25 points.
- A new account's rating really moves while it settles: kept out of the primary, and reported as an exploratory population (no burn-in).
- The world's accuracy and blunders are tied to results, and opponents follow the rating, so the peak window's wins carry better accuracy: this is why those verdicts are secondaries, and their parameters are assumptions (section 7).

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-4.0.1+fcda2762eeeb` | – |

From `cli algo show` on the cli built from main at `ef0789b9`. The rule measured is `profile::coaching::trajectory` (`trajectory_input`, `compute`, `contract`), reached through `cli lab trajectory` (`bench::lab_rules::trajectory`, ADR 0115).

## 4. Primary metric and ground truth

- Primary metric (exactly one family, Bonferroni across its three cells): the share of settled, constant-strength players whose trajectory has `kind == "falling"` (`thenVsNow.rating.verdict == "worse"`), at 60, 120 and 250 rated games. The step called is `cli lab trajectory` (`rematch_lab.trajectory`). Both headlines of a `falling` trajectory count: "peaked at … and is … now" (basis `peak`) and "is … now, down from an average of … over your first N rated games" (basis `start`); the split is reported.
- Ground truth: the true strength, constant by construction. Each player has one series of 300 burn-in games and 250 observed games, played from a fresh account (1500, deviation 500, volatility 0.09); the request at `g` games is the first `g` observed games, as a player with `g` rated games in Rematch, under one account id and so one production seed.
- Secondary metrics:
  - confirmatory: false "rising", the share of the same players whose trajectory is `rising`, at the three lengths, read like the primary (Bonferroni across three); it extends the hide action (section 9);
  - confirmatory: the share of the same players whose accuracy verdict is "worse", and whose blunders-per-game verdict is "worse", at the three lengths (six shares), read like the primary with Bonferroni across six; they extend the hide action to those rows (section 9);
  - confirmatory: the detection rate, the share told `falling` of players whose true strength falls linearly by 100 or by 200 points across the observed games, at 60, 120 and 250 games (six cells, 200 players each, a fresh series for each cell after the same burn-in), with Wilson intervals at 1 − 0.05/6 and the mean observed rating fall;
  - exploratory: the primary and false "rising" at 30 games (W = 15); the split by basis; the no-peak read at 120 and 250 games; the score verdict "worse"; any of the rating, accuracy, blunders and score rows "worse"; the mean `change` when `falling`; new accounts (no burn-in), overall and by a true strength below or above 1500; the Elo K = 10, K = 32 and Glicko-1 arms at the three lengths; the rule's calibration (the mean half-width of its 80% interval of the rating difference against 1.28 times the SD of that difference across constant-strength players); the diagnostics of the world.
- Not measured: the weakness rows of then against now (no weaknesses are sent; EXP-0013 and EXP-0014 cover the weakness rates).

## 5. The minimum effect that matters

- Minimum effect: 0.05 above the nominal 0.10, so the acceptable false-alarm rate is 0.15. The hide line is 0.20.
- Why this size: "worse" is called when the 80% two-sided interval of the difference excludes 0, which promises at most 10% "worse" for a player whose strength has not moved (ADR 0083 §6: "better" or "worse" only when the evidence separates). The 0.05 on top is a convention, the tolerance EXP-0002 used around its 0.80 (±0.05): one stable player in twenty told they are falling beyond the promise. It is not derived from a cost.
- The hide line is where false "falling" headlines outnumber true ones. With a share b of active players truly declining and a detection rate d, false headlines outnumber true ones when the false-alarm rate exceeds b × d / (1 − b): at b = 0.2 and d = 0.8 that is 0.20 (at b = 0.1, 0.09; at b = 0.3, 0.34). We have not measured b; 0.2 is an assumption, stated before the run.

## 6. Power analysis

- Method: a simulation of the section 9 rule on binomial counts (`power.py`, output `power.json`, 20,000 draws a point): Wilson intervals at 1 − 0.05/3 per cell, the three cells drawn independently. The cells are the same players read at three lengths, so they are positively correlated, which makes "keep" (all three must clear) more likely than simulated; the simulation is the harder case.
- Inputs: the true false-alarm rate in each cell, from 0.05 to 0.85; the expected rate is well above 0.30 (section 2, from the reviewer's approximation, not the cli); two-sided intervals; the player as the unit.
- Result: 700 settled players. In the expected regime 200 would decide "hide" (power 1.00 at 0.30 and above). The 700 are for the regime a challenger is measured in on these same registered seeds (the arena, ADR 0109): the smallest of 200 to 800 in steps of 100 at which every power below reaches 0.80:
  - "keep" when the rate is the nominal 0.10 in all three cells: 0.83 (600 players: 0.73). At 0.12 it is 0.08: a rule between 0.12 and 0.15 will almost always read "need more data", and that is the rule's intent, not a gap;
  - "change" or "hide" when the rate is 0.20 in one cell and 0.10 in the others: 0.88; "hide" when 0.30 in one cell: 1.00, and when 0.25 in all three: 0.99.
  - The detection cells (200 players each) are descriptive: the Bonferroni (6) half-width is at most 0.093.
- Sensitivity: `power.json` has every grid point; the half-width of a cell at 0.10 is 0.027 with 700 players.

## 7. Design

- Unit of analysis: the player. Games are nested in a player's series and are never counted as replicates; each player gives one verdict per checkpoint.
- The world, simulated by `analysis.py` (the data-generating process, as EXP-0002 simulates its counts; the rule under test is only the cli's):
  - true strength per player from Normal(1500, 300), clipped to [700, 2300];
  - Glicko-2 as Lichess runs it (Glickman 2013, the Illinois step), with lila's constants (`modules/rating/src/main/Glicko.scala`: τ 0.75, a fresh account at 1500 with deviation 500 and volatility 0.09, the deviation in [45, 500], the volatility at most 0.1, `RatingPeriodsPerDay(0.21436)`): each game grows the deviation by half a day of rating periods, 0.107;
  - each game's opponent rated Normal(player's rating, 75), with a true strength Normal(their rating, 50) and a deviation of 60;
  - the result from the true strengths' expected score E, with draws 4 × 0.06 × E(1 − E) and the expected score kept;
  - accuracy Normal(79 after a win, 75 after a draw, 69 after a loss, plus 0.02 per point of change in true strength since the first observed game; SD 8), clipped to [10, 99] and rounded to 0.1; blunders Poisson(0.7, 0.9 or 1.5 by result, times exp(−0.002 per point of that change)). These are assumptions with no measured source; they shape only the accuracy and blunders secondaries;
  - every game rated, analysed and finished, two games a day from 2026-01-05 09:00 UTC, no streaks or breaks; the stored rating is the pre-game rating rounded to an integer, sent with `chesscom: false`, as Lichess stores it;
  - each player's bootstrap seed is production's form, `trajectory:<account id>:rapid`, the account id a UUID from `sha256("<namespace>/account/<player>")`, the same at every checkpoint; no weaknesses.
- Populations:
  - 700 settled constant-strength players (300 burn-in games, 250 observed, read at 30, 60, 120 and 250 games, and the no-peak read at 120 and 250);
  - for each decline of 100 and 200 points and each of 60, 120 and 250 games, 200 players with a fresh series (300 burn-in games at constant strength, then the observed games with the true strength falling linearly by the decline from the first observed game to the last);
  - 300 new accounts at constant strength with no burn-in, read at 60, 120 and 250 games;
  - for Elo K = 10 and K = 32, and for a chess.com-like Glicko-1 (assumed constants: a fresh account at 1500 with deviation 350, grown by 10 a game, kept in [50, 350]; the stored rating is the one after the game and the request says `chesscom: true`, so the loader derives the pre-game rating as production does), 200 settled constant-strength players each (300 burn-in games, 250 observed, read at 60, 120 and 250).
- Transfer: the primary's world is Lichess's rating system with an idealised schedule; real players' streaks, breaks and tilt add memory to a rating and would push false alarms up, not down. The chess.com arm is indicative only, since chess.com's constants are not public.
- Pairing and blocking: none between populations; the checkpoints of a population are nested (the same players).
- Seeds: `seed(namespace, name)` for each name in `analysis.py`'s `SEED_NAMES`: `settled`; `decline/100/60`, `decline/100/120`, `decline/100/250`, `decline/200/60`, `decline/200/120`, `decline/200/250`; `new-account`; `elo/10`, `elo/32`; `glicko1`. The registered namespace is `EXP-0015`.
- Development and confirmatory data: there is no candidate and nothing is tuned. Before registration, runs checked the code and timed it, on the namespaces `EXP-0015-timing` (40 settled players, timing only) and `EXP-0015-smoke` (a few players a population), printing no share of any verdict; the smoke run's world diagnostics were read (the settled deviation sits at the 45 floor, about 5 points a game).
- Held fixed: `algo-4.0.1`; the world above; no weaknesses.
- Data: no file inputs (`inputs.sha256`); no real player's games.
- Leakage controls: not a predictive question; the truth is the simulation's.
- Reuse: none. No earlier run measures the trajectory, and EXP-0009's cohort does not hold full series (section 0).

## 8. Analysis plan

- Model or test: shares. For each primary cell, the share `falling` with Wilson intervals at 95%, 80% and 1 − 0.05/3 (98.33%, Bonferroni across the three cells); each cell is read against the bound and the hide line by its Bonferroni interval:
  - "above the hide line": the lower end above 0.20;
  - "above the bound": the lower end above 0.15 (and not above 0.20);
  - "within the bound": the upper end at most 0.15;
  - "undecided": anything else.
- The confirmatory secondaries are read the same way: false "rising" with Bonferroni across three, the accuracy and blunders "worse" shares with Bonferroni across six.
- Effect size: each share with the intervals above; for detection, the share with Wilson 95% and 1 − 0.05/6 intervals and the mean observed rating fall.
- Multiple comparisons: Bonferroni across the three primary cells, across the three false "rising" cells, across the six accuracy and blunders cells and across the six detection cells. Exploratory results get Wilson 95% intervals and no correction, and decide nothing.
- Diagnostics: no fitted model. The world's diagnostics (above) are reported; a signed mean error at the first observed game past 25 points in absolute value, in any population, is reported as a burn-in that did not settle, and that population's reads are then stated with it.
- Exclusions and missing data: none. A request whose trajectory is not sufficient stops the run (it cannot happen at 30 games or more). A run past the budget's cap writes a `results.json` that says it failed, with its `compute`.

## 9. Decision rule

Measurement on one version, read on the primary's three cells (section 8), in this order:

1. **Hide the verdict until fixed:** any cell is "above the hide line". We file, as its own lane, the production change that stops stating `falling` (the Progress and Overview headline, the rating row's "Worse" and the coach's `kind`) until a challenger clears this experiment; the same filing covers `rising` and "Better" when a false "rising" cell is above the hide line, and the accuracy or blunders row's "Worse" when its cell is. We register the challenger.
2. **Change the threshold:** otherwise, any cell is "above the bound". We register the challenger as the next experiment, in its own lane; the verdict stays meanwhile.
3. **Keep the threshold:** every cell is "within the bound". The rule stays, and its false-alarm and detection rates are published beside it.
4. **Need more data:** anything else. At most one follow-up, with `Follows:` this experiment, new seeds and its own power analysis; a second "need more data" keeps the rule.

Only the primary decides which outcome; the confirmatory secondaries set only what a hide filing covers. The detection rate is reported whatever it shows and is the guard of the comparison below.

The comparison rule for a challenger (the arena, ADR 0109), on the registered seeds, the player as the unit and the two versions paired on the same players:

- Primary, each of the three cells: the change in the false-alarm share, challenger minus champion, with the paired Newcombe interval (method 10) at 1 − 0.05/3. Better when it lies wholly below 0, worse when wholly above 0. A challenger beats the champion only when all three cells are better; whether it ships is its own registered experiment's rule, which also asks for its cells to be within the bound.
- Guard: the detection rate at a 200-point decline, the 600 players of its three cells pooled. The champion's detection is inflated by its false alarms, so a change against it would block every honest fix; the guard is an absolute floor instead: worse when the upper end of the challenger's own 80% Wilson interval lies below 0.50, a rule that misses most real 200-point falls. The paired change (Newcombe, 80%) is reported with it.
- Reported, never gating: the paired change in the detection rate at a 100-point decline, pooled the same way.

### Stopping rules

- Early stopping: none. The run takes minutes; a look would save nothing.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0015/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `97f34774eee7211a68bd285318a9ba22754d0fc1318dd0d2734355c2fee59518`

## 11. Deviations

None.
