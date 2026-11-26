# EXP-0030: a trajectory verdict that counts the rating's memory

- Status: pre-registered
- Issue: "EXP-0030: a trajectory verdict that counts the rating's memory" (`.github/settings/roadmap-issues.json`)
- Author: the maintainer, with Claude (lane `exp/0030-memory`)
- Kind: comparison (two frozen candidates against the champion rule, on one build)
- Follows: none (the challenger EXP-0015's decision names; EXP-0015 decided "hide", not "need more data")
- Registered: 2026-11-26. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: whether the trajectory's rating verdict comes back. `algo-5.0.0` hides it (ADR 0116) because EXP-0015 found that, for players whose strength never changed, it said "falling" to 0.44, 0.62 and 0.78 of them at 60, 120 and 250 rated games, and "rising" to 0.44, 0.30 and 0.17, against a bound of 0.15. The rule fails twice: its bootstrap treats 30 consecutive ratings as 30 independent ones (its 80% interval of the difference is about 4 points each side where the difference's spread across such players is 21 to 37 points), and "then" sits on a selected peak.
- If the result is that a candidate keeps all six false-alarm cells (falling and rising, at 60, 120 and 250 games) within 0.15 on fresh seeds and passes every guard of section 9, we will ship it as a major algorithm version, turn `RATING_VERDICT_SHOWN` back on and write the ADR; if no candidate does, the verdict stays hidden for good and the trajectory keeps stating the ratings as facts (with the copy change of section 9 filed as its own issue); if the only failure is an undecided cell, one follow-up with more players.
- Expected cost: 3,144,000 simulated games and 50,100 trajectories through `cli lab trajectory` (three rules on every request), and one pass over 3,955,242 lines of the cohort's `players.jsonl`. EXP-0015's measured rate (5 µs a simulated game, 13 ms a 250-game trajectory) puts it at about 6 CPU-minutes in the lab container, plus about 2 minutes to read the cohort. No engine, no database, no AI tokens. About two days of maintainer time, most of it the candidates' Rust and, if one ships, the web's parity.
- Value score: impact 3 × uncertainty 2 ÷ cost 1 = 6
- Cheaper alternative? Dropping the verdict for good is cheaper, and it is the fallback. It gives up telling a player that a real fall of 150 points is not noise, which is what ADR 0089 (F6) was for. The feasibility check in section 7 suggests the candidates are calibrated in EXP-0015's world, so the open questions are the ones only this run answers: whether they stay within the bound with the power to show it, whether they still catch real falls, and whether they hold up with breaks, streaks and real series. No cached run measures a candidate.
- Compute budget: 3,144,000 `simulated_games`, 50,100 `trajectory_players` (players sent to `cli lab trajectory`, counted once per rule) and 3,955,242 `cohort_lines`; cap 150%

## 1. Question

A 1,500 Lichess rapid player plays 120 rated games and is exactly as strong at the end as at the start. Their rating wanders, as ratings do. Can Rematch tell them whether their rating has really moved, without telling one such player in four that they are falling? And when a player really does lose 200 points of strength over those games, is most of that caught?

The estimand, as in EXP-0015, is the player's strength. A verdict of `falling` or `rising` for a player whose true strength is constant is a false alarm about strength, whichever way it points.

## 2. Hypotheses

- H0 (for each candidate): at one or more of 60, 120 and 250 rated games, the share of constant-strength players told `falling`, or the share told `rising`, is above 0.15.
- H1 (for each candidate): all six shares are at most 0.15. The expected value is about 0.10 each, the nominal rate of an 80% two-sided interval: the dev-namespace feasibility check (section 7) gave 0.09 to 0.12 per cell on 300 players.

Why the candidates should work. A rating is close to a mean-reverting process around the player's strength. For an Elo-type update the error `e` after each game follows `e(t+1) = φ e(t) + noise`, with a reversion of `a = 1 − φ = q K p(1 − p)` a game and noise of variance `K² var(s)`, where `q = ln 10 / 400` is the rating scale every chess site uses, `K` the step size and `s` the result. Its stationary variance is `σ² = K² var(s) / (2a − a²)`. With `p(1 − p) ≈ var(s) ≈ 0.25` (opponents paired near the rating) and the root mean square of the observed steps `s_rms = K √var(s)`, both parameters come from the player's own series:

- `a = q s_rms / 2`, `φ = 1 − a`;
- `σ² = s_rms / (q (1 − a / 2))`.

At lila's floor deviation (45, an effective K of about 11.6) this gives `σ ≈ 31` points and `φ ≈ 0.984`, against EXP-0015's measured 31 and 0.97 (its lag-1 estimate from 250 games is biased down by about 0.015). Nothing here is fitted to EXP-0015's difference SDs: the inputs are the rating scale and each player's own steps, so the interval widens for a K = 32 rating, after breaks, and on a new account, without a constant from this world.

Rival explanations that would give the same numbers, and how each is separated:

- The world has no memory beyond the rating, so any rule built on the rating's own steps looks calibrated in it: the rough world (section 7) adds a form that lasts for weeks, a day's form, tilt after a loss, sessions and long breaks; the cohort check reads real Lichess series, where streaks and improvement are what they are.
- The candidates are calibrated only because the model's `p(1 − p) = 0.25` happens to suit EXP-0015's pairing: the Elo K = 10 and K = 32 arms and the chess.com-like Glicko-1 arm (exploratory) change the step and the rating system.
- A candidate is within the bound only because it says nothing: the detection guard per length (section 9).

## 3. Algorithm versions compared

All three rules are one build: `cli lab trajectory` takes a `rule` in the request (lab only; production calls `compute`, which runs the production rule, unchanged). The build is the branch `exp/0030-memory` at the freeze commit named in section 10, `algo-5.0.0` with the candidates added and no production output changed, so its manifest id is `algo-5.0.1+<hash>` (a patch: the profile's source digest moves and nothing a player or the coach reads changes).

| Role | `rule` | What differs |
|---|---|---|
| Champion | `peak-bootstrap` | The rule of every version up to `algo-5.0.0` (ADR 0089): "then" is the `W = min(30, floor(n / 2))` games centred on the peak (or the first `W`), "now" the last `W`, compared by the 2,000-resample bootstrap of their pre-game ratings (`compareMeans`). |
| Candidate 1 | `memory-halves` | "Then" is the first `W` games, "now" the last `W` (no selected peak). The rating row's interval comes from the memory model (section 7) instead of the bootstrap. |
| Candidate 2 | `memory-before` | "Then" is every game before "now" (`n − W` games), "now" the last `W`. The same memory interval. |

Both candidates keep everything else of the trajectory: the accuracy, blunders, score and weakness rows are compared over the candidate's own windows by the rules they use today, the peak and the current rating are still reported as facts, and `change` becomes the difference the verdict tests, `round(mean(now) − mean(then))`, not the current rating against the peak.

Considered and not registered:

- A block bootstrap over games inside each window. A 30-game window holds about one correlation time of the rating (`1 / a ≈ 60` games), so resampling blocks inside it cannot see variance it does not contain: its interval is bounded by the within-window spread, the failure EXP-0015 measured.
- The rating system's own deviation (RD) per game. Neither the importer nor the request stores it (EXP-0015, "The arena rule's limits").
- A fixed minimum change in rating points. Any fixed number is a property of one rating system and one world, and EXP-0015's Next item forbids taking one from its difference SDs; the memory interval is the per-player version of the same idea.
- A slope with an autocorrelation-robust error. It answers "is there a trend", where the product and the coach state "then against now"; with the same memory model it would add a third candidate to the multiplicity for little.

## 4. Primary metric and ground truth

- Primary metric (one family, co-primary): for each candidate, the share of settled constant-strength players (EXP-0015's world: Glicko-2 as on Lichess, two games a day) whose trajectory has `kind == "falling"`, and the share with `kind == "rising"`, at 60, 120 and 250 rated games. Six cells a candidate, twelve in all, Bonferroni across the twelve.
- Ground truth: the true strength, constant by construction (2,000 players, 300 burn-in games, 250 observed, read at the first 60, 120 and 250 observed games, under one account id and one production seed each).
- Secondary metrics:
  - confirmatory (guards, section 9): the arena comparison against the champion on these players (EXP-0015's registered rule: the paired change in false "falling" at each length, Newcombe at 1 − 0.05/3); the detection rate at a linear 200-point fall at each of 60, 120 and 250 games (300 players a cell, a fresh series a cell), always reported with the same length's false-alarm rate beside it; the six false-alarm cells in the rough world (1,000 players); the cohort's share of real players the memory model would call falling within a month;
  - reported, never gating: detection at a 100-point fall per length; the champion's six cells on the same players (EXP-0015 on fresh seeds);
  - exploratory: the 30-game cells; the accuracy and blunders "worse" shares under each rule; the mean `change` (all players, and when `falling`); detection at a 200-point fall over 120 games in the rough world; the Elo K = 10, K = 32 and chess.com-like Glicko-1 arms and new accounts (300 players each, all three rules, both sides); each rule's calibration (the mean half-width of its 80% interval of the difference against 1.28 SD of the difference across constant-strength players); the step diagnostics of every world and of the cohort (the median RMS step, and the lag-1 correlation of consecutive steps); the cohort's rising share, mean z and mean z².

## 5. The minimum effect that matters

- Minimum effect: a false-alarm share of 0.15 in any cell, either side: the nominal 0.10 of an 80% two-sided interval (ADR 0083 §6) plus EXP-0015's tolerance of 0.05. EXP-0015's hide line, 0.20, is where false "falling" headlines outnumber true ones if a fifth of active players are declining and 80% of them are caught.
- Why this size: the same promise EXP-0015 measured the champion against, so a candidate that clears is held to exactly the bar the champion failed. Nothing here is tuned to a measured value: the bound is EXP-0015's, registered before its run.

## 6. Power analysis

- Method: a simulation of the section 9 primary rule on counts (`power.py`, output `power.json`, 20,000 draws a point): at each length the falling and rising counts of the same players are one multinomial draw; the three lengths are drawn independently, though they are the same players read three times and so positively correlated, which makes "all six within" more likely than simulated.
- Inputs: the true rate in every cell from 0.08 to 0.16; the expected rate is the nominal 0.10 (the dev feasibility check gave 0.09 to 0.12, section 7); a cell clears when the upper end of its Wilson interval at 1 − 0.05/12 is at most 0.15; the player is the unit.
- Result: 2,000 settled players, the smallest of 700, 1,000, 1,500, 2,000 and 2,500 at which the chance that all six cells clear is at least 0.80 when every cell's true rate is 0.11: 0.95 (1,500 players: 0.75; at 0.10: 0.9997 with 2,000, 0.81 with 1,000). At 0.12 it is 0.34, and at 0.13 it is 0.002, so a candidate that is slightly miscalibrated will mostly read "undecided", which is the rule's intent.
- Guards: the detection guard (300 players a length, the lower end of the 80% Wilson interval at least 0.50 at all three lengths) passes 0.96 of the time when the true detection is 0.60 at every length and 0.29 at 0.55; the feasibility check gave 0.67, 0.90 and 0.98. The rough-world guard (1,000 players, a cell fails when the lower end at 1 − 0.05/12 is above 0.15) trips in 0.002 of draws at a true 0.15 in one cell, 0.42 at 0.18 and 0.92 at 0.20.
- Sensitivity: every grid point is in `power.json`.

## 7. Design

- Unit of analysis: the player. Games are nested in a player's series and are never counted as replicates; each player gives one verdict per rule and length. The three rules run on the identical request, so the comparison is exactly paired.
- The candidates' memory interval, which the Rust implements (`profile::coaching::trajectory`) and the cohort check restates:
  - the series is the pre-game ratings `r(0) … r(n − 1)` the production loader builds (`trajectory_input`), in its order; the steps are `d(i) = r(i + 1) − r(i)`, and `s_rms = sqrt(mean d²)`;
  - `a = q s_rms / 2` with `q = ln 10 / 400`; `φ = 1 − a`, clamped to [0, 1); `σ² = s_rms / (q (1 − a / 2))`;
  - for windows `A = [a0, a1)` and `B = [b0, b1)` with `b0 ≥ a1`: `Var(mean B − mean A) = σ² (S(mA) / mA² + S(mB) / mB² − 2 C / (mA mB))`, where `S(m) = m (1 + φ) / (1 − φ) − 2 φ (1 − φ^m) / (1 − φ)²` (the sum of `φ^|i − j|` over a block of `m`) and `C = φ^(b0 − a1 + 1) (1 − φ^mA)(1 − φ^mB) / (1 − φ)²`;
  - the 80% interval of the difference is `diff ± 1.2816 √Var`, and "better" or "worse" is called when it excludes 0, as ADR 0083 §6 has it; each window's own 80% interval is its mean `± 1.2816 σ √(S(m)) / m`, clamped to [0, 4000]. A series with every step 0 has no spread and gets no verdict.
- Worlds and populations (all simulated by `analysis.py`; the rule under test is only the cli's):
  - Plain (EXP-0015's world, unchanged: Glicko-2 with lila's constants, two games a day, opponents rated Normal(rating, 75) with a true strength Normal(their rating, 50), draws `4 × 0.06 × E(1 − E)`, accuracy and blunders by result as EXP-0015 assumed): 2,000 settled constant-strength players (300 burn-in games, 250 observed, read at 30, 60, 120 and 250); for each fall of 100 and 200 points and each of 60, 120 and 250 games, 300 players with a fresh series (burn-in at constant strength, then the strength falling linearly over the observed games); 300 new accounts (no burn-in); Elo K = 10 and K = 32 and the chess.com-like Glicko-1 of EXP-0015 (stored rating after the game, `chesscom: true`), 300 settled players each.
  - Rough (assumptions, chosen before any run and not from data): the same rating, opponents and results, but games come in sessions of 1 + Poisson(2.5) games 20 minutes apart; sessions are an exponential 1.2 days apart (at least an hour), and after 4% of sessions comes a long break of Uniform(14, 90) days; the Glicko-2 deviation grows by `0.21436` rating periods per day that actually passes. The game is played at the true strength plus a slow form (an AR(1) over sessions, SD 40, correlation 0.8), a day's form (Normal(0, 30), fresh each session) and tilt (20 points off after each loss in a session, at most 60). The long-run strength is constant, so a verdict is a false alarm about strength even when the player's form really dipped for a few weeks. 1,000 settled players, read at 60, 120 and 250; 300 players with a 200-point fall over 120 games (exploratory).
- The cohort check (real series): the anonymised Lichess cohort of ADR 0114, EXP-0009's `players.jsonl` for 2026-07 to 2026-10 (by checksum, `inputs.sha256`), which holds for each pseudonymised player-month the number of rapid games and the first and last five rapid games with the rating before and after each. The confirmatory slice is the pseudonyms whose first hex digit is `8`; each pseudonym counts once, in its first month with at least 40 rapid games. For each: `diff` = the mean pre-game rating of the last five minus the first five; `s_rms` from those ten games' own steps (after minus before); the windows `[0, 5)` and `[n − 5, n)`; `z = diff / √Var` from the formula above. It would be called "falling" when `z < −1.2816`. Real players also improve and decline within a month, so the falling share is an upper bound on the model's false alarms there; it is a check of the assumption, not a verdict anyone is told, and it is written from this section's formula, not from the rule's code. The step lag-1 correlation (consecutive steps within each five-game block, centred on the block) says whether real results carry streaks the plain world lacks; the same statistic is reported for every simulated world.
- Pairing and blocking: the rules are paired on identical requests; populations are separate draws.
- Seeds: `seed(namespace, name)` for each name in `analysis.py`'s `SEED_NAMES`: `settled`; `decline/100/60`, `decline/100/120`, `decline/100/250`, `decline/200/60`, `decline/200/120`, `decline/200/250`; `rough`; `rough-decline/200/120`; `elo/10`, `elo/32`; `glicko1`; `new-account`. Account ids are `sha256("<namespace>/account/<player>")`, so the production bootstrap seed `trajectory:<account id>:rapid` differs by namespace too.
- Development and confirmatory data:
  - Development: the namespace `EXP-0015-dev`, never read by EXP-0015, and the cohort's pseudonyms whose first hex digit is `0`. Before this registration one feasibility run on `EXP-0015-dev` (300 plain settled players read at 60, 120 and 250, and 200 players a length at a 200-point fall; Elo K = 10 and K = 32) computed the memory interval in a scratch script, not the cli: false "falling" and "rising" 0.09 to 0.12 per cell for both candidates in all three ratings, detection 0.67, 0.90 to 0.92 and 0.96 to 0.99. The cohort was only counted (about 11,000 to 12,500 players a month with 40 or more rapid games in one sixteenth of the pseudonyms), not read for any statistic. Between this registration and the freeze, the cli's candidates are developed and tested on `EXP-0015-dev` and the `0` slice only; what was run there is listed in `review.md`.
  - Confirmatory: a namespace drawn after the freeze: `EXP-0030-` followed by the first 12 hex characters of the freeze commit's sha (section 10), which cannot be known while the candidates can still change, and the cohort's `8` slice. Neither is read before the run.
- Held fixed: the build at the freeze commit; the worlds above; no weaknesses are sent.
- Data: the cohort files above (external, checksummed); no Rematch user, no linked account and no chess.com player's games (ADR 0114).
- Leakage controls: not a predictive question; the truth is the simulation's, and the confirmatory seeds and slice are disjoint from the development ones.
- Reuse: EXP-0015's world and its arena rule; EXP-0009's cohort files (ADR 0114), by checksum. No earlier run measures a candidate.

## 8. Analysis plan

- Model or test: shares. Each co-primary cell gets Wilson intervals at 95%, 80% and 1 − 0.05/12 (Bonferroni across two candidates × six cells), and is read by its Bonferroni interval as EXP-0015 reads its cells: "above the hide line" (lower end above 0.20), "above the bound" (lower end above 0.15), "within the bound" (upper end at most 0.15) or "undecided".
- The guards:
  - beats the champion: for each length, the paired change in false "falling", candidate minus champion, with the Newcombe interval (method 10) at 1 − 0.05/3, EXP-0015's arena rule; "better" when it lies wholly below 0;
  - detection: at each of 60, 120 and 250 games, the share told `falling` of the players with a 200-point fall, with its 80% Wilson interval; it passes when the lower end is at least 0.50 (the candidate is shown to catch most such falls at every length, where EXP-0015's floor asked only that the upper end of the pooled cells reach 0.50);
  - rough world: its six cells with Wilson intervals at 1 − 0.05/12; it passes when none is "above the bound" or "above the hide line";
  - transfer: the cohort's falling share with its 95% Wilson interval; it passes when the lower end is at most 0.15. It is shared by both candidates, since both use the same model.
- Effect size: each share with its intervals; for the arena, the paired difference with its interval.
- Multiple comparisons: Bonferroni across the twelve co-primary cells; the arena comparison uses EXP-0015's registered level; each guard is one test per candidate. A candidate must pass every guard, which only makes shipping harder, so the guards need no correction for the ship decision. Exploratory results get Wilson 95% intervals and decide nothing.
- Diagnostics: no fitted model. The burn-in is checked as in EXP-0015 (the signed mean error at the first observed game, flagged past 25 points) for the plain, decline and rough populations; a flagged population's reads are stated with it. Each rule's calibration and every world's step diagnostics are reported.
- Exclusions and missing data: none in the simulations; a request whose trajectory is not sufficient stops the run (it cannot happen at 30 games or more). In the cohort, a player-month without five first and five last games, or whose ten steps are all 0, is skipped and counted. A run past the budget's cap writes a `results.json` that says it failed, with its `compute`.

## 9. Decision rule

A candidate clears when all of these hold:

1. every one of its six co-primary cells is "within the bound";
2. it beats the champion on all three false-"falling" cells by EXP-0015's arena rule;
3. its detection guard passes at 60, 120 and 250 games;
4. its rough-world guard passes;
5. the transfer guard passes.

Then, in this order:

- **Ship:** if `memory-halves` clears, it ships; otherwise, if `memory-before` clears, it ships. The order is fixed now, not by which does better: `memory-halves` is the simpler statement ("your first 30 games against your last 30"), its "then" does not grow with the history, and the product already has the copy for it. Shipping is a major algorithm version: the candidate becomes the production rule in Rust and the web (`backend/fixtures/trajectory/cases.json` keeps them in parity), `RATING_VERDICT_SHOWN` turns on with `RATING_VERDICT_SOURCE` moving to EXP-0030, and a new ADR records it.
- **Need more data:** no candidate clears, and every candidate's only failure is one or more "undecided" co-primary cells (none above the bound or the hide line, every guard passing). At most one follow-up, with `Follows:` this experiment, the candidates unchanged, fresh seeds and its own power analysis; a second "need more data" keeps the verdict hidden.
- **Keep the verdict hidden:** anything else. The verdict stays hidden for good; ADR 0116's switch stays off and its source moves to this experiment's results.

The copy, decided now whatever the result. "Peaked at 1,097 in April 2026 and is 816 now (−281)" compares a maximum with one draw, so its difference is negative for a stable player whatever the rule (EXP-0015 measured −39 to −59 points on average). The difference shown next to the rating is the one between the two windows' averages, never the current rating against the peak; the peak stays as a plain fact ("your highest was 1,097, in April 2026") with no difference against it. If a candidate ships, this copy ships with it. If none does, it is filed as its own issue for the hidden trajectory, since the headline's −N against the peak misleads with or without a verdict.

The decision is computed by `analysis.py` (`decision`), from the cells and guards above.

### Stopping rules

- Early stopping: none. The run takes minutes.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0030/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `46417d059b1cf5ac2cad1f8b68f10f48b89f7cf30a3d9dc6ab0fd6d694584b22`

## 11. Deviations

None.
