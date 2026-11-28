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
- If the result is that a candidate keeps all six false-alarm cells (falling and rising, at 60, 120 and 250 games) within 0.15 on fresh seeds and passes every guard of section 9, we will ship it as a major algorithm version, turn `RATING_VERDICT_SHOWN` back on and write the ADR; if no candidate does, the verdict stays hidden, the trajectory keeps stating the ratings as facts, the copy change of section 9 is filed as its own issue, and any later challenger needs its own registration that answers the failure this run finds; if a candidate's only failure is an undecided cell, one follow-up with more players for that candidate.
- Expected cost: 4,079,000 simulated games and 65,400 trajectories through `cli lab trajectory` (three rules on every request), and one pass over 3,955,242 lines of the cohort's `players.jsonl`. From EXP-0015's run (98 seconds for 1.3 million games and 8,100 trajectories) about 10 to 20 CPU-minutes in the lab container, plus a few minutes to read the cohort; the budget caps counted units, not time. No engine, no database, no AI tokens. About two days of maintainer time, most of it the candidates' Rust and, if one ships, the web's parity.
- Value score: impact 3 × uncertainty 2 ÷ cost 1 = 6
- Cheaper alternative? Dropping the verdict for good is cheaper, and it is the fallback. It gives up telling a player that a real fall of 200 points is not noise, which is what ADR 0089 (F6) was for. Even a perfectly calibrated rule leaves many headlines false: at a false "falling" rate of 0.10, a detection of 0.67 at 60 games and a fifth of players declining, about 37% of "falling" headlines at 60 games would be false, and about 47% at the bound of 0.15. That is the price of any verdict at 60 games, and the copy says what the verdict means (section 9). The feasibility check in section 7 suggests the candidates are calibrated in EXP-0015's world, so the open questions are the ones only this run answers: whether they stay within the bound with the power to show it, whether they still catch real falls, and whether they hold up with breaks, streaks and real series. No cached run measures a candidate.
- Compute budget: 4,079,000 `simulated_games`, 65,400 `trajectory_players` (players sent to `cli lab trajectory`, counted once per rule) and 3,955,242 `cohort_lines`; cap 150%

## 1. Question

A 1,500 Lichess rapid player plays 120 rated games and is exactly as strong at the end as at the start. Their rating wanders, as ratings do. Can Rematch tell them whether their rating has really moved, without telling one such player in four that they are falling? And when a player really does lose 200 points of strength over those games, is most of that caught?

The estimand, as in EXP-0015, is the player's strength. A verdict of `falling` or `rising` for a player whose true strength is constant is a false alarm about strength, whichever way it points.

## 2. Hypotheses

- H0 (for each candidate): at one or more of 60, 120 and 250 rated games, the share of constant-strength players told `falling`, or the share told `rising`, is above 0.15.
- H1 (for each candidate): all six shares are at most 0.15. The expected value is about 0.10 each, the nominal rate of an 80% two-sided interval: the dev-namespace feasibility check (section 7) gave 0.09 to 0.12 per cell on 300 players.

Why the candidates should work. A rating is close to a mean-reverting process around the player's strength. For an Elo-type update the error `e` after each game follows `e(t+1) = φ e(t) + noise`, with a reversion of `a = 1 − φ = q K p(1 − p)` a game and noise of variance `K² var(s)`, where `q = ln 10 / 400` is the rating scale every chess site uses, `K` the step size and `s` the result. Its stationary variance is `σ² = K² var(s) / (2a − a²)`. With `p(1 − p) ≈ var(s) ≈ 0.25` (opponents paired near the rating) and the root mean square of the observed steps `s_rms = K √var(s)`, both parameters come from the player's own series:

- `a = q s_rms / 2` (at most 1), `φ = 1 − a`;
- `σ² = s_rms² / (a (2 − a))`, which is `s_rms / (q (1 − a / 2))` whenever `a < 1` (a step below 347 points).

At lila's floor deviation (45, an effective K of about 11.6) this gives `σ ≈ 31` points and `φ ≈ 0.984`, against EXP-0015's measured 31 and 0.97 (its lag-1 estimate from 250 games is biased down by about 0.015). Nothing here is fitted to EXP-0015's difference SDs: the inputs are the rating scale and each player's own steps, so the interval widens for a K = 32 rating, after breaks, and on a new account, without a constant from this world.

The plain world (section 7) is close to the model's own assumption, an Elo-type update at the floor deviation, so its six cells are close to a test of the implementation and the sample size, not of the assumption. The tests of the assumption are the guards: the rough world, the chess.com-like world and the cohort.

Rival explanations that would give the same numbers, and how each is separated:

- The world has no memory beyond the rating, so any rule built on the rating's own steps looks calibrated in it: the rough world adds a form that lasts for weeks, a day's form, tilt after a loss, sessions and long breaks; the cohort check reads real Lichess series, where streaks and improvement are what they are.
- The candidates are calibrated only because the model's `p(1 − p) = 0.25` suits EXP-0015's Glicko-2 and pairing: the chess.com-like Glicko-1 world (a guard) changes the rating system and how the pre-game rating is derived; the Elo K = 10 and K = 32 arms (exploratory) change the step.
- A candidate is within the bound only because it says nothing: the detection guard per length.

## 3. Algorithm versions compared

All three rules are one build: `cli lab trajectory` takes a `rule` in the request (lab only; production calls `compute`, which runs the production rule, unchanged). The build is the branch `exp/0030-memory` at the freeze commit (the commit tagged `exp-0030-freeze`; its sha is recorded in `review.md` and `decision.md`), `algo-5.0.0` with the candidates added and no production output changed, so its manifest id is `algo-5.0.1+<hash>` (a patch: the profile's source digest moves and nothing a player or the coach reads changes).

| Role | `rule` | What differs |
|---|---|---|
| Champion | `peak-bootstrap` | The rule of every version up to `algo-5.0.0` (ADR 0089): "then" is the `W = min(30, floor(n / 2))` games centred on the peak (or the first `W`), "now" the last `W`, compared by the 2,000-resample bootstrap of their pre-game ratings (`compareMeans`). |
| Candidate 1 | `memory-halves` | "Then" is the first `W` games, "now" the last `W` (no selected peak). The rating row's interval comes from the memory model (section 7) instead of the bootstrap. |
| Candidate 2 | `memory-before` | "Then" is every game before "now" (`n − W` games), "now" the last `W`. The same memory interval. At 60 games it compares the same windows as `memory-halves`, so the two candidates' 60-game cells are identical. |

Both candidates keep everything else of the trajectory: the accuracy, blunders, score and weakness rows are compared over the candidate's own windows by the rules they use today, the peak and the current rating are still reported as facts, and `change` becomes the difference the verdict tests, `round(mean(now) − mean(then))`, not the current rating against the peak.

Considered and not registered:

- A block bootstrap over games inside each window. A 30-game window holds about one correlation time of the rating (`1 / a ≈ 60` games), so resampling blocks inside it cannot see variance it does not contain: its interval is bounded by the within-window spread, the failure EXP-0015 measured.
- The rating system's own deviation (RD) per game. Neither the importer nor the request stores it (EXP-0015, "The arena rule's limits").
- A fixed minimum change in rating points. Any fixed number is a property of one rating system and one world, and EXP-0015's Next item forbids taking one from its difference SDs; the memory interval is the per-player version of the same idea.
- A slope with an autocorrelation-robust error. It answers "is there a trend", where the product and the coach state "then against now"; with the same memory model it would add a third candidate to the multiplicity for little.

## 4. Primary metric and ground truth

- Primary metric (one family, co-primary): for each candidate, the share of settled constant-strength players (EXP-0015's world: Glicko-2 as on Lichess, two games a day) whose trajectory has `kind == "falling"`, and the share with `kind == "rising"`, at 60, 120 and 250 rated games. Six cells a candidate, each read by its Wilson interval at 1 − 0.025/2 (section 8).
- Ground truth: the true strength, constant by construction (2,000 players, 300 burn-in games, 250 observed, read at the first 60, 120 and 250 observed games, under one account id and one production seed each).
- Secondary metrics:
  - confirmatory (guards, section 9): the arena comparison against the champion on these players (EXP-0015's registered rule: the paired change in false "falling" at each length, Newcombe at 1 − 0.05/3); the detection rate at a linear 200-point fall at each of 60, 120 and 250 games (300 players a cell, a fresh series a cell), always reported with the same length's false-alarm rate beside it; the six false-alarm cells in the rough world (1,000 players); the six false-alarm cells in the chess.com-like Glicko-1 world (1,000 players); the cohort's two-sided share of real players the memory model would call either way within a month;
  - a positive control: the champion's false "falling" share at 250 games must be above 0.5 (EXP-0015 measured 0.78), or the lab path is not returning the rule's own `kind` and the run stops;
  - reported, never gating: detection at a 100-point fall per length; the champion's cells on the same players (EXP-0015 on fresh seeds) and in the rough and chess.com-like worlds;
  - exploratory: the 30-game cells; the accuracy and blunders "worse" shares under each rule; the mean `change` (all players, and when `falling`); detection at a 200-point fall over 120 games in the rough world; the rough world with a slow form of SD 0 and 20 instead of 40 (500 players each); the Elo K = 10 and K = 32 arms and new accounts (300 players each, all three rules, both sides); each rule's calibration (the mean half-width of its 80% interval of the difference against 1.28 SD of the difference across constant-strength players); the step diagnostics of every world and of the cohort (section 7); the cohort's one-sided shares, mean z and mean z², over every series and with one pooled RMS step.

## 5. The minimum effect that matters

- Minimum effect: a false-alarm share of 0.15 in any cell, either side: the nominal 0.10 of an 80% two-sided interval (ADR 0083 §6) plus EXP-0015's tolerance of 0.05. EXP-0015's hide line, 0.20, is where false "falling" headlines outnumber true ones if a fifth of active players are declining and 80% of them are caught.
- Why this size: the same promise EXP-0015 measured the champion against, so a candidate that clears is held to exactly the bar the champion failed. Nothing here is tuned to a measured value: the bound is EXP-0015's, registered before its run.

## 6. Power analysis

- Method: a simulation of the section 9 rules on counts (`power.py`, output `power.json`, 20,000 draws a point): at each length the falling and rising counts of the same players are one multinomial draw; the three lengths are drawn independently, though they are the same players read three times, so the result is approximate.
- Inputs: the true rate in every cell from 0.08 to 0.16; the expected rate is the nominal 0.10 (the dev feasibility check gave 0.09 to 0.12, section 7); a cell clears when the upper end of its Wilson interval at 1 − 0.025/2 is at most 0.15; the player is the unit.
- Result: 2,000 settled players, the smallest of 700, 1,000, 1,500, 2,000 and 2,500 at which the chance that all six cells clear is at least 0.80 when every cell's true rate is 0.11: 0.99 (1,500 players: 0.89, but 0.25 at 0.12 against 0.60 with 2,000; at 0.10: 1.00 with 2,000, 0.92 with 1,000). At 0.13 it is 0.015, so a candidate that is miscalibrated by three points mostly reads "undecided", which is the rule's intent.
- Guards: the detection guard (300 players a length, the lower end of the 80% Wilson interval at least 0.50 at all three lengths) passes 0.96 of the time when the true detection is 0.60 at every length and 0.29 at 0.55; the feasibility check gave 0.67, 0.90 and 0.98. A world guard (1,000 players, six cells, failing when any share is above 0.15 or any upper end of an 80% interval above 0.20) passes 0.99 of the time at a true 0.12 in every cell, 0.84 at 0.13, 0.015 at 0.15 and never at 0.18: it is a test the candidate must pass, not one it must be shown to fail.
- Sensitivity: every grid point is in `power.json`.

## 7. Design

- Unit of analysis: the player. Games are nested in a player's series and are never counted as replicates; each player gives one verdict per rule and length. The three rules run on the identical request, so the comparison is exactly paired.
- The candidates' memory interval, which the Rust implements (`profile::coaching::trajectory`) and the cohort check restates. It is frozen at this registration: between registration and the freeze, development means implementing it and checking the implementation (the Rust's closed forms against brute-force sums, the lab path end to end), never changing the formula, its constants or the windows. A change to any of them is a deviation (section 11) that needs a new review before the run.
  - the series is the pre-game ratings `r(0) … r(n − 1)` the production loader builds (`trajectory_input`), in its order; the steps are `d(i) = r(i + 1) − r(i)`, and `s_rms = sqrt(mean d²)`;
  - `a = min(q s_rms / 2, 1)` with `q = ln 10 / 400`; `φ = 1 − a`, kept below 1; `σ² = s_rms² / (a (2 − a))`;
  - for windows `A = [a0, a1)` and `B = [b0, b1)` with `b0 ≥ a1`: `Var(mean B − mean A) = σ² (S(mA) / mA² + S(mB) / mB² − 2 C / (mA mB))`, where `S(m) = m (1 + φ) / (1 − φ) − 2 φ (1 − φ^m) / (1 − φ)²` (the sum of `φ^|i − j|` over a block of `m`) and `C = φ^(b0 − a1 + 1) (1 − φ^mA)(1 − φ^mB) / (1 − φ)²`;
  - the 80% interval of the difference is `diff ± 1.2816 √Var`, and "better" or "worse" is called when it excludes 0, as ADR 0083 §6 has it; each window's own 80% interval is its mean `± 1.2816 σ √(S(m)) / m`, clamped to [0, 4000]. A series with every step 0 has no spread and gets no verdict.
- Worlds and populations (all simulated by `analysis.py`; the rule under test is only the cli's):
  - Plain (EXP-0015's world, unchanged: Glicko-2 with lila's constants, two games a day, opponents rated Normal(rating, 75) with a true strength Normal(their rating, 50), draws `4 × 0.06 × E(1 − E)`, accuracy and blunders by result as EXP-0015 assumed): 2,000 settled constant-strength players (300 burn-in games, 250 observed, read at 30, 60, 120 and 250); for each fall of 100 and 200 points and each of 60, 120 and 250 games, 300 players with a fresh series (burn-in at constant strength, then the strength falling linearly over the observed games); 300 new accounts (no burn-in); Elo K = 10 and K = 32, 300 settled players each.
  - chess.com-like: EXP-0015's Glicko-1 (assumed constants: a fresh account at 1500 with deviation 350, grown by 10 a game, kept in [50, 350]; the stored rating is the one after the game and the request says `chesscom: true`, so the loader derives the pre-game rating as production does), 1,000 settled players read at 60, 120 and 250. chess.com publishes neither its constants nor its deviations, so this world is a guard on the loader's path and on a second rating system, not a measurement of chess.com.
  - Rough (assumptions, chosen before any run and not from data): the same rating, opponents and results, but games come in sessions of 1 + Poisson(2.5) games 20 minutes apart; sessions are an exponential 1.2 days apart (at least an hour), and after 4% of sessions comes a long break of Uniform(14, 90) days; the Glicko-2 deviation grows by `0.21436` rating periods per day that actually passes. The game is played at the true strength plus a slow form (an AR(1) over sessions, SD 40, correlation 0.8), a day's form (Normal(0, 30), fresh each session) and tilt (20 points off after each loss in a session, at most 60). The long-run strength is constant, so a verdict is a false alarm about strength even when the player's form really dipped for a few weeks. 1,000 settled players, read at 60, 120 and 250; 300 players with a 200-point fall over 120 games, and 500 players each with a slow form of SD 0 and 20 (exploratory).
- The cohort check (real series): the anonymised Lichess cohort of ADR 0114, EXP-0009's `players.jsonl` for 2026-07 to 2026-10 (by checksum, `inputs.sha256`), which holds for each pseudonymised player-month the number of rapid games and the first and last five rapid games with the rating before and after each. The confirmatory slice is the pseudonyms whose first hex digit is `8`; a development run reads the `0` slice instead. Each pseudonym counts once, at its first month with at least 40 rapid games; if that month lacks five first and five last games, or its ten steps are all 0, the pseudonym is skipped and counted, and not used in a later month. For each: `diff` = the mean pre-game rating of the last five minus the first five; `s_rms` from those ten games' own steps (after minus before); the windows `[0, 5)` and `[n − 5, n)`; `z = diff / √Var` from the formula above.
  - What it can and cannot show. Real players also improve and decline within a month. If real change is independent of the rating's noise, it can only widen the spread of `z`, so the two-sided share `|z| > 1.2816` and the mean `z²` are upper bounds on what the model's false alarms would be there (the one-sided shares are not: a cohort that mostly improves lowers the falling share). A small two-sided share bounds the model; a large one cannot tell a wrong model from real change. The guard therefore catches only a gross failure (section 8).
  - The guard reads the settled series: a player-month with any of its ten steps above 30 points in absolute value looks provisional (a Lichess deviation well above its floor) and is left out; every series is also reported, and the settled series with one pooled RMS step (the median of the slice) instead of each player's ten-step estimate, since ten steps estimate `s_rms` with noise that fattens the tails of `z`.
  - Streaks: the pooled lag-1 correlation of consecutive steps within each five-game block, centred on the pooled mean (not each block's, which would bias it by about −1/4), and the same for the steps' signs, a proxy for the results. Each simulated world reports the same statistics on the same blocks (the steps of its first and last five observed games), so the real numbers can be read against a world with no streaks (plain) and one with them (rough).
- Pairing and blocking: the rules are paired on identical requests; populations are separate draws.
- Seeds: `seed(namespace, name)` for each name in `analysis.py`'s `SEED_NAMES`: `settled`; `decline/100/60`, `decline/100/120`, `decline/100/250`, `decline/200/60`, `decline/200/120`, `decline/200/250`; `rough`; `rough-decline/200/120`; `elo/10`, `elo/32`; `glicko1`; `new-account`; `rough-form/0`, `rough-form/20`. Account ids are `sha256("<namespace>/account/<player>")`, so the production bootstrap seed `trajectory:<account id>:rapid` differs by namespace too.
- Development and confirmatory data:
  - Development: the namespace `EXP-0015-dev`, never read by EXP-0015, and the cohort's pseudonyms whose first hex digit is `0`. Before this registration one feasibility run on `EXP-0015-dev` (300 plain settled players read at 60, 120 and 250, and 200 players a length at a 200-point fall; Elo K = 10 and K = 32) computed the memory interval in a scratch script, not the cli (`dev/feasibility.py`): false "falling" and "rising" 0.09 to 0.12 per cell for both candidates in all three ratings, detection 0.67, 0.90 to 0.92 and 0.96 to 0.99. The cohort was only counted (about 11,000 to 12,500 players a month with 40 or more rapid games in one sixteenth of the pseudonyms), not read for any statistic. Between this registration and the freeze, development runs use `EXP-0015-dev` and the `0` slice only; each is listed in `review.md`.
  - Confirmatory: a namespace no one can choose. After the freeze commit is made and tagged `exp-0030-freeze`, the namespace is `EXP-0030-` followed by the first 12 hex characters of the randomness of the first round of the drand quicknet beacon (`https://api.drand.sh/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971`) whose time is at least 10 minutes after the freeze commit's committer time. It is written to `namespace.txt`, which `inputs.sha256` lists, in a commit after the freeze and before the run; `analysis.py` runs only that namespace or the development one, so its sha does not change. The round, its randomness and its time go in `review.md`. If the beacon cannot be reached, the namespace is the Bitcoin block hash of the first block mined at least 10 minutes after the freeze commit's time, recorded the same way. The `8` slice of the cohort is not read before the run.
- Held fixed: the build at the freeze commit; the worlds above; no weaknesses are sent.
- Data: the cohort files above (external, checksummed); no Rematch user, no linked account and no chess.com player's games (ADR 0114).
- Leakage controls: not a predictive question; the truth is the simulation's, and the confirmatory seeds and slice are disjoint from the development ones.
- Reuse: EXP-0015's world and its arena rule; EXP-0009's cohort files (ADR 0114), by checksum. No earlier run measures a candidate.

## 8. Analysis plan

- Model or test: shares. Each co-primary cell gets Wilson intervals at 95%, 80% and 1 − 0.025/2, and is read by the last as EXP-0015 reads its cells: "above the hide line" (lower end above 0.20), "above the bound" (lower end above 0.15), "within the bound" (upper end at most 0.15) or "undecided". Within a candidate, "all six within" is an intersection-union test, so the six cells need no correction among themselves; the 5% is split between the two candidates and the two looks the rule allows (this run and at most one follow-up).
- The guards, each uncorrected, since a correction on a guard makes it easier to pass:
  - beats the champion: for each length, the paired change in false "falling", candidate minus champion, with the Newcombe interval (method 10) at 1 − 0.05/3, EXP-0015's arena rule; "better" when it lies wholly below 0. With the champion at 0.44 to 0.78 it is not expected to fail; it is kept because ADR 0109 compares every challenger with the champion;
  - detection: at each of 60, 120 and 250 games, the share told `falling` of the players with a 200-point fall, with its 80% Wilson interval; it passes when the lower end is at least 0.50 (shown to catch most such falls at every length, where EXP-0015's floor asked only that the upper end of the pooled cells reach 0.50);
  - rough world and chess.com-like world: each world's six cells fail when a share is above 0.15 or the upper end of its 80% Wilson interval is above 0.20; the world passes when no cell fails;
  - transfer: the cohort's two-sided share on the settled series with its 95% Wilson interval; it fails when the lower end is above 0.40, twice the model's nominal two-sided 0.20. It is shared by both candidates, since both use the same model. A pass bounds the model; a fail at this level means the model is grossly wrong or real players change far more within a month than within the bound, and either way the candidates do not ship.
- Effect size: each share with its intervals; for the arena, the paired difference with its interval.
- Multiple comparisons: as above. Exploratory results get Wilson 95% intervals and decide nothing.
- Diagnostics: no fitted model. The positive control (section 4) stops the run if it fails. The burn-in is checked as in EXP-0015 (the signed mean error at the first observed game, flagged past 25 points) for the plain, decline, rough and chess.com-like populations; a flagged population's reads are stated with it. Each rule's calibration and every world's step diagnostics are reported.
- Exclusions and missing data: none in the simulations; a request whose trajectory is not sufficient stops the run (it cannot happen at 30 games or more). In the cohort, as section 7 says, with the counts reported. A run past the budget's cap writes a `results.json` that says it failed, with its `compute`.

## 9. Decision rule

A candidate clears when all of these hold:

1. every one of its six co-primary cells is "within the bound";
2. it beats the champion on all three false-"falling" cells by EXP-0015's arena rule;
3. its detection guard passes at 60, 120 and 250 games;
4. its rough-world guard passes;
5. its chess.com-like guard passes;
6. the transfer guard passes.

Then, in this order:

- **Ship:** if `memory-halves` clears, it ships; otherwise, if `memory-before` clears, it ships. The order is fixed now, not by which does better: `memory-halves` is the simpler statement ("your first 30 games against your last 30"), its "then" does not grow with the history, and the product already has the copy for it. Shipping is a major algorithm version: the candidate becomes the production rule in Rust and the web (`backend/fixtures/trajectory/cases.json` keeps them in parity), `RATING_VERDICT_SHOWN` turns on with `RATING_VERDICT_SOURCE` moving to EXP-0030, and a new ADR records it.
- **Need more data:** no candidate clears, and at least one candidate's only failure is one or more "undecided" co-primary cells (none above the bound or the hide line, every guard passing). One follow-up, with `Follows:` this experiment, carrying only those candidates, unchanged, on fresh seeds (not pooled with these), its own power analysis and the same rule at the same level, 1 − 0.025/2, so the two looks share the 5%. A second "need more data" keeps the verdict hidden.
- **Keep the verdict hidden:** anything else. ADR 0116's switch stays off, its source moves to this experiment's results, and the decision states which condition each candidate failed. A later challenger needs its own registration that answers that failure; nothing in this run is retried on these seeds.

The copy, decided now whatever the result. "Peaked at 1,097 in April 2026 and is 816 now (−281)" compares a maximum with one draw, so its difference is negative for a stable player whatever the rule (EXP-0015 measured −39 to −59 points on average). The difference shown next to the rating is the one between the two windows' averages, never the current rating against the peak; the peak stays as a plain fact ("your highest was 1,097, in April 2026") with no difference against it. If a candidate ships, this copy ships with it, and the verdict's words say what it means: the rating moved by more than its own wander, not that the player's chess got better or worse. If none does, the copy is filed as its own issue for the hidden trajectory, since the −N against the peak misleads with or without a verdict.

The decision is computed by `analysis.py` (`decision`), from the cells and guards above.

### Stopping rules

- Early stopping: none. The run takes minutes.
- Futility: none, for the same reason.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0030/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` it ran on, and `DIR/figures/`)
- Analysis code sha256: `85f282962d9d1f2942833b0eb845673b603bfedc0309c0684a12a263cb51f146`

## 11. Deviations

None during the run. From the maths audit (2026-11-28, ADR 0118), recorded after the decision; sections 0 to 10, `analysis.py` and `results.json` are unchanged and the decision stands (`math.md`, Findings):

1. Section 2's lag-1 bias is 0.0185, not "about 0.015"; with it the model at φ = 0.984 reproduces EXP-0015's measured 0.9655 exactly.
2. Section 2's "σ ≈ 31, φ ≈ 0.984 at K ≈ 11.6" is 31.9 and 0.983 under its own assumptions; the quoted pair matches the measured plain world instead.
3. The family-wise level is about 0.024, not the stated 5%: reading a two-sided interval's upper end spends half of α, so the rule is conservative.
4. Rounding and wording: the detection upper end at 250 games is 0.9951 (quoted 0.99); `run_minutes: 15` is 14.3; "17%" is 17.7%; "74 games apart" is the month's game count, and the windows start about 69 games apart.
5. `decision.md`'s "between its nominal rate and about twice it" is not a proven bound. It holds only if the model's variance is too small by the cohort's mean z² of 2.37 at every window, which gives 0.20 a side. Without that assumption the real-player rate is unknown, which is EXP-0032's question.
