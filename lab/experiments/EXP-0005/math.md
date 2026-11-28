# EXP-0005: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py` or `decision.md` was changed. EXP-0005 was closed unrun by its value gate (ADR 0107), so there is no `results.json`. The maths here is the maths of the design it would have run and of the argument that closed it: the Jeffreys classification of the churn and its exact power (section 6), the classification's error rates, and the claim that no branch of section 9 could change the benchmark's suites. Each result is proved in prose and checked by `math_check.py` in this folder (prefix `R` for symbolic, exact and simulated checks, `N` for the quoted numbers).

The simulation the value gate rests on is EXP-0001's recall model, whose maths is in `../EXP-0001/math.md` (its (M1)–(M2), (E1)–(E7) and Results 1 to 8). `math_check.py` implements that model independently, checks it against EXP-0001's committed curve, and then evaluates it where EXP-0005's branches would have.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $n$ | the reference's top-2 hits at 80 games over the 45 flawed players (pilot 30, smoke 15); $n = 28$ | players | section 6; EXP-0001 `results.json` `pilot.players`, the smoke recall 0.6 |
| $X$ | the hits the candidate loses | players | `tables` `n01` |
| $b$ | the true churn, the probability a reference hit is lost | – | section 4 |
| $b_0 = 0.10$ | EXP-0001's assumed churn | – | `CHURN_ASSUMED` |
| $[J_L(x, n), J_U(x, n)]$ | the Jeffreys 95% interval of $x$ of $n$ | – | `jeffreys` |
| $\pi_f$, $h_f$, $n_f$ | flaw $f$'s hit probability at 80 (or 120) games, and the pilot's hits and players | –, players | EXP-0001 (M1), `hits_at` |
| $m$, $G$ | players a flaw and games a player of a suite | players, games | EXP-0001 `M_GRID`, `FULL_GAMES_OPTIONS` |
| $\delta$ | the true change in recall at 2 the suite is sized to detect, 0.10 | – | EXP-0001 `DELTA` |
| $a$ | the gain rate that gives the change: $a = (\delta + b\bar\pi)/(1 - \bar\pi)$ | – | EXP-0001 (M2) |
| $A(m, G, \delta, b)$ | the assurance that a suite ships: the 97.5% Newcombe lower end above 0 | – | EXP-0001 `simulate_recall` |
| $m^\*(G, b)$ | EXP-0001's required size: the smallest grid $m$ from which $A \ge 0.8$ | players | EXP-0001 `required_m` |
| $B = 9{,}000$, $C = 2$ | the full suite's game budget and clean controls | games, players | `FULL_BUDGET_GAMES`, `FULL_CLEAN` |

## 2. Model

**(M1) The churn.** Given the 45 flawed players and the $n$ reference hits among them, each hit is lost independently with probability $b$: $X \sim \operatorname{Binomial}(n, b)$. $n$ is taken as fixed at 28 (the hits are a property of the reference side, which section 7 rebuilds and checks against EXP-0001's stored rows).

**(M2) The re-sizing** (EXP-0001's model, used unchanged by section 9). A suite of $m$ players a flaw at $G$ games: $\pi_f \sim \operatorname{Beta}(h_f + \tfrac12, n_f - h_f + \tfrac12)$ from EXP-0001's pilot, each reference outcome $\operatorname{Bernoulli}(\pi_f)$, each reference hit lost with probability $b$ and each miss gained with probability $a$; a draw with $a > 1$ is infeasible and does not ship. Ship when the 97.5% Newcombe lower end of the paired difference is above 0.

## 3. Estimators

- **(E1) Jeffreys 95%** (`jeffreys`): the 2.5% and 97.5% quantiles of $\operatorname{Beta}(x + \tfrac12, n - x + \tfrac12)$, with 0 below at $x = 0$ and 1 above at $x = n$.
- **(E2) The classification** (`classify`): "below" if $J_U < 0.10$, "above" if $J_L > 0.10$, else "straddles".
- **(E3) The design's power** (`design_power`): $P(\text{class} \mid b) = \sum_{x=0}^{n}\binom nx b^x(1-b)^{n-x}\,\mathbf 1\{\text{class}(x) = \cdot\}$.
- **(E4) The section 9 actions** (`resize`, `mde_at`): "below" runs EXP-0001's rule at $b = J_U$ (curves over the 20 grid sizes at $G \in \{80, 120\}$, 4,000 suites each; the cheapest fitting suite, else the budget's suite with its detectable change); "above" registers the detectable change of 22 a flaw at 80 games at $b = J_L$; "straddles" keeps the suites.
- **(E5) EXP-0001's rule** (`required_m`, `resize`): a size fits when $(5m^\* + C)G \le B$.

## 4. Derivations and proofs

**Result 1 (the classification is two thresholds in $x$).** (a) (E1) is the equal-tailed 95% posterior interval under the $\operatorname{Beta}(\tfrac12, \tfrac12)$ prior, with an end fixed at 0 or 1. (b) Both of its ends rise strictly with $x$. (c) Hence at $n = 28$, "below" happens exactly when $x = 0$ ($J_U = 0.0851$, while $J_U(1, 28) = 0.155$), and "above" exactly when $x \ge 7$ ($J_L(6, 28) = 0.095$, $J_L(7, 28) = 0.119$).

*Proof.* (a) The prior kernel $t^{-1/2}(1-t)^{-1/2}$ times the binomial kernel $t^x(1-t)^{n-x}$ is the $\operatorname{Beta}(x + \tfrac12, n - x + \tfrac12)$ kernel; both normalise to densities, so they are equal. (b) The density ratio of $\operatorname{Beta}(x + \tfrac32, n - x - \tfrac12)$ to $\operatorname{Beta}(x + \tfrac12, n - x + \tfrac12)$ is a constant times $t/(1-t)$, whose logarithm has derivative $1/t + 1/(1-t) > 0$. A monotone likelihood ratio implies first-order stochastic dominance, so every quantile rises with $x$; the fixed ends (0 at $x = 0$, 1 at $x = n$) keep the order. (c) By (b), $\{x : J_U(x) < 0.1\}$ is an initial segment and $\{x : J_L(x) > 0.1\}$ a final one; the four values locate them. $\blacksquare$

Machine-checked: `lab/lean/Rematch/BetaBinomial.lean` theorem `Rematch.beta_binomial_posterior`, for (a), with $a = b = \tfrac12 > 0$ and natural $x \le n$, which match. Checked by R1 (sympy for (a) and the likelihood ratio, the exact quantiles for (b) and (c)).

**Result 2 (the design's power, exactly).** Under (M1),

```math
P(\text{below} \mid b) = (1 - b)^{28}, \qquad P(\text{above} \mid b) = P(X \ge 7) = \sum_{x=7}^{28}\binom{28}{x}b^x(1-b)^{28-x},
```

and straddles is the rest. $P(\text{above})$ rises in $b$ and $P(\text{below})$ falls. These give every entry of section 6's table.

*Proof.* By Result 1(c) and (E3). The derivative of the binomial tail is $\frac{d}{db}P(X \ge k) = n\binom{n-1}{k-1}b^{k-1}(1-b)^{n-k} > 0$ (the sum telescopes); $(1-b)^{28}$ falls. $\blacksquare$

At $b$ = 0, 0.02, 0.05, 0.10, 0.20 and 0.30: below 1.00, 0.57, 0.24, 0.05, 0.00, 0.00; above 0, 0.00, 0.00, 0.02, 0.32, 0.78, as section 6 states. Checked by R2 (sympy for the derivative; the 18 entries).

**Result 3 (the classification's error rates).** If the true churn is at least 0.10, "below" is wrong with probability at most $0.9^{28} = 0.0523$; if it is at most 0.10, "above" is wrong with probability at most $P(X \ge 7 \mid b = 0.10) = 0.0179$. The first is about twice the nominal 0.025 of one tail of a 95% interval, because the interval's discreteness puts its whole "below" region on $x = 0$. With the pilot alone ($n = 19$), no loss gives $J_U = 0.122$, so "below" is impossible; the smallest $n$ at which it is possible is 24. At $n = 40$ one loss straddles, and $P(\text{below} \mid 0.02) = 0.98^{40} = 0.45$ against 0.57 at $n = 28$.

*Proof.* By Result 2 the two error probabilities are monotone in $b$, so their suprema over the wrong side are at $b = 0.10$. The rest is Result 1(b) applied at the stated $n$. $\blacksquare$

Section 6's last bullet ("At $b$ = 0.10 the rule says 'below' 5% of the time") is this result. Its "with the pilot alone ... an upper end of 0.124" is 0.122 (Finding 3). Checked by R3 (exact, with grids on both sides of 0.10).

**Result 4 (the re-sizing model, and its implementation).** Under (M2) the assurance is $A(m, G, \delta, b) = E_\pi\big[P(\text{ship} \mid \pi)\big]$, with ship needing $a \le 1$. In the normal limit (EXP-0001 Result 8),

```math
A \approx E_\pi\Big[\Phi\Big(\frac{\delta\sqrt{5m} - z_{0.9875}\,\sigma_u}{\sigma_s}\Big)\mathbf 1\{a \le 1\}\Big], \quad \sigma_u^2 = \delta + 2b\bar\pi - \delta^2, \quad \sigma_s^2 = \sigma_u^2 - \tfrac15\textstyle\sum_f(\delta_f - \delta)^2,
```

with $\delta_f = (1 - \pi_f)a - \pi_fb$. $A$ falls as $b$ rises for $\delta > 0$.

*Proof.* The formula is EXP-0001 Result 8 (proved there) with $N = 5m$. For the monotonicity in the normal limit: $\sigma_u^2$ rises in $b$, so the numerator $\delta\sqrt{5m} - z\sigma_u$ falls, and while it is positive a larger $\sigma_s$ (which also rises in $b$ through $\sigma_u$) lowers the ratio; $\Phi$ is increasing. $\blacksquare$

The monotonicity is shown in the normal limit only; R6 checks it on the simulated curve. `math_check.py`'s simulation of (M2) is written from EXP-0001's `math.md`, not imported, and reproduces ten points of EXP-0001's committed curve at $b = 0.10$ within 1.4 SE and its 22-a-flaw power 0.525 (0.527). At $b = 0.0851$ and 22 a flaw the normal limit gives 0.560 against the simulation's 0.571. Checked by R4.

**Result 5 (no branch of section 9 changes the suites).** For every outcome $x \in \{0, \dots, 28\}$ the full suite stays at 22 players a flaw and 80 games, the size `backend/crates/bench/src/suite.rs` has (`FULL_PER_FLAW = 22`).

*Proof.* By Result 1(c) there are three cases.

- $x = 0$, "below". Section 9 reruns EXP-0001's rule at $b = J_U(0, 28) = 0.0851$. At that churn the simulated assurance at 80 games is 0.53 at 20 a flaw, 0.77 at 32 and 0.86 at 40, so $m^\*(80) = 40$ and $(5 \cdot 40 + 2) \cdot 80 = 16{,}160 > 9{,}000$; at 120 games $m^\*(120) = 40$ and 24,240 games. No size fits (E5), so the rule returns the budget's suite: the largest $m$ with $(5m + 2) \cdot 80 \le 9{,}000$ is 22, today's. Its detectable change at $b = 0.0851$ is the first grid $\delta$ with $A \ge 0.8$: 0.58 at 0.10 and 0.88 at 0.15, so 0.15, which is what EXP-0001 registered at $b = 0.10$. Nothing moves.
- $1 \le x \le 6$, "straddles". The suites stay by the rule's own text.
- $x \ge 7$, "above". The weekly suite keeps its size; only its registered detectable change is recomputed at $J_L(x, 28) \ge J_L(7, 28) = 0.119$, where 22 a flaw ships +0.1 with assurance 0.48. That changes a line of `docs/benchmark.md`, not a suite.

The case $x = 0$ gives the smallest churn any branch evaluates, and by Result 4 a smaller churn is the most favourable to a resize. $\blacksquare$

The proof rests on simulated assurances, so it holds up to Monte Carlo error. The margins are wide: at 20 a flaw, the largest grid size within the budget at 80 games, the assurance is 0.53, about 35 of its 4,000-suite standard errors below 0.8, and 0.31 at 14 a flaw and 120 games. The review's 0.56, 0.76 and 0.85 at 22, 32 and 40 a flaw are reproduced within its 2,000-draw error (0.571, 0.765, 0.858). Checked by R5.

**Result 6 (what would move the suites).** On EXP-0001's rule a resize needs the assurance at 20 a flaw and 80 games to reach 0.8 (no grid size between 20 and 24 exists, and $(5 \cdot 24 + 2) \cdot 80 = 9{,}760 > 9{,}000$). That needs a churn upper end below about 0.022, and with no hit lost, $J_U(0, n) \le 0.022$ needs about 112 reference hits. A suite of exactly 22 a flaw, which is not on the grid, would need about 0.030 and about 83 hits.

*Proof.* At 120 games the budget's 14 a flaw has assurance 0.784 even at $b = 0$ (EXP-0001's committed curve), so only 80 games can fit. At 80 games a fitting size is a grid $m \le 22$ whose assurance, and that of every larger grid size, is at least 0.8; by monotonicity in $m$ that is $A(20) \ge 0.8$. By Result 4, $A(20)$ falls in $b$, so the condition is $b \le b^\*$ with $A(20, b^\*) = 0.8$; $b^\*$ is found on a grid of churns with 40,000 simulated suites each. $J_U(0, n)$ falls in $n$ (the $\operatorname{Beta}(\tfrac12, n + \tfrac12)$ distribution is stochastically decreasing in $n$), so the hits are the first $n$ below $b^\*$. $\blacksquare$

The thresholds are Monte Carlo estimates: the assurance falls about 0.007 a 0.001 of churn near $b^\*$, against a standard error of 0.002, so $b^\*$ is good to about $\pm 0.0003$ and the hits to about $\pm 2$. The decision and the review use the 22-a-flaw figures (0.03, 83 hits); EXP-0001's rule, which section 9 says it runs, uses the grid (Finding 2). Checked by R6.

## 5. Numbers

There is no `results.json`. The numbers are recomputed from EXP-0001's `results.json`, EXP-0006's `results.json` (which ran the side-by-side run EXP-0005 had registered) and the formulas above.

| Quoted as | Formula | Source | Value |
|---|---|---|---|
| 28 reference hits: 19 pilot and 9 smoke | count | EXP-0001 `pilot.players` at 80; EXP-0006 smoke recall 0.6 × 15 | 19 + 9 |
| no hit lost gives 0.0851 (summary: 0.085) | (E1) | – | 0.085071 |
| EXP-0006's run: 28 hits, none lost, [0, 0.085] | (E1) | EXP-0006 `exploratory.churn.by_checkpoint` | as quoted |
| at that churn no suite within 9,000 games; the one `suite.rs` has; summary: "keeps today's 22 players a flaw" | Result 5 | `suite.rs` `FULL_PER_FLAW` | 22 |
| review: 0.56, 0.76, 0.85 at 22, 32, 40 a flaw; 40 a flaw, 16,160 games; detectable change still 0.15 | Results 4, 5 | simulation | 0.571, 0.765, 0.858; 40, 16,160; 0.15 |
| review: "above" needs at least 7 of 28 lost | Result 1(c) | – | 7 |
| decision and review: about 83 hits for an upper end near 0.03 | Result 6 | – | 83 at 22 a flaw; 112 on the grid (Finding 2) |
| full suite (110) and smoke (15) give about 80 hits at today's recall | $110 \times 19/30 + 9$ | EXP-0001 pilot recall | 78.7 |
| section 6 table, 18 entries | Result 2 | – | as quoted |
| section 6: $n = 19$, no loss, 0.124 | (E1) | – | **0.122** (Finding 3) |
| section 6: $n = 40$, 0.45 against 0.57 at $b = 0.02$; 8,960 games | Result 3; $(110 + 2) \times 80$ | – | 0.446, 0.568; 8,960 |
| section 6: "below" 5% at $b = 0.10$ | Result 3 | – | 0.0523 |
| section 1: the churn moved 14 to 40 a flaw at 80 games | – | EXP-0001 `required_players_per_flaw` | 14, 40 |
| section 5: 0.05 "halves the players needed (28 against 40)" | – | EXP-0001 `required_players_per_flaw` `80/0.05` | 28/40 = 0.70 (Finding 1) |
| review: "EXP-0001's published powers of 0.51, 0.71 and 0.82" at 22, 32, 40 | – | EXP-0001 `at_budget[1]`, `power_recall` | 0.525, 0.706, 0.826 (the review's are its own 2,000-draw values, within their error) |
| compute: 0 of 5,120 games, 0 of 172,000 suites | $3{,}840 + 1{,}280$; $2 \cdot 20 \cdot 4{,}000 + 3 \cdot 4{,}000$ | – | none ran |

> **Finding 1 (section 5, "halves").** Section 5 says a churn of 0.05 against 0.10 "already halves the players needed (EXP-0001: 28 against 40 a flaw at 80 games)". 28 is 0.70 of 40, not half. What halves is the churn's extra cost over no churn (14 a flaw): 26 extra players at 0.10 against 14 at 0.05, a ratio of 0.54. It changes nothing: the experiment was closed for another reason.

> **Finding 2 (decision "Next" and review concern 1, the hits a resize needs).** The decision says "the design needs about 83 reference hits for a 'below' that can move the suites", from the review's "a resize would need an upper end near 0.03". That is the threshold of a suite of exactly 22 a flaw. EXP-0001's rule, which section 9 runs, picks sizes from its grid, and the largest grid size within 9,000 games at 80 games is 20, which reaches 0.8 only for a churn below about 0.022 (Result 6). With no loss that needs about 112 reference hits, not 83. The decision adds that the full suite and the smoke suite "give about 80 at today's recall" (78.7). Under either reading, about 79 hits with no loss give $J_U = 0.031$, above both thresholds (0.022 and 0.030), so a re-registration on the full and smoke suites at today's recall could not move the suites even with no hit lost. It would fail the same value gate unless recall rises, the rule is allowed sizes off the grid, or more players are added. The coordinator should weigh this before EXP-0024 is registered.

> **Finding 3 (section 6, rounding).** "With the pilot alone (n = 19) the interval can never lie below 0.10 (no losses give a Jeffreys 95% upper end of 0.124)": the upper end is 0.1223. The conclusion stands.

## 6. Assumptions that can fail

- **Independent losses with one probability: Results 2 and 3.** (M1) treats each hit as lost independently with the same $b$. Losses may cluster by flaw (review concern 6), which makes $X$ overdispersed and the Jeffreys interval too narrow; it would only widen the straddle region and does not touch Result 5.
- **$n$ fixed at 28: Results 1 to 3.** The hits are the reference side's, which the design rebuilt and checked against EXP-0001's rows; EXP-0006's run of the same players found 28, so the assumption held.
- **EXP-0001's model is the right one for re-sizing: Results 4 to 6.** It is the registered model (section 9 says so), and its own assumptions (a fixed churn on every player, Jeffreys posteriors on six players a flaw) are discussed in `../EXP-0001/math.md` section 6. Result 5's conclusion has margins of 35 standard errors, so no plausible Monte Carlo or approximation error moves it.
- **Monotonicity in $b$ and $m$: Results 5 and 6.** Shown in the normal limit (Result 4) and on the simulated curves, not proved for the exact finite-sample assurance.
- **The rule's grid: Result 6 and Finding 2.** The threshold depends on EXP-0001's grid of sizes. A re-registration that allows any size changes it to 0.030.

### Verification

To be completed after the blind re-derivation and the referee.
