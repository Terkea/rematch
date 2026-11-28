# The algorithm's maths

<!--
The maths of every computed quantity (ADR 0118). Part A derives and proves the current
algorithm, algo-6.0.0. Part B has one section per entry of algorithm/CHANGELOG.md, headed
`## algo-X.Y.Z`: scripts/algorithm/check.mjs fails when a version has no section here.
algorithm/math_check.py checks every result in the lab image; a result's checks carry its name
(A1, A2, ...). Maths is GitHub's: $...$ inline, ```math fences for display.
-->

Every number Rematch shows comes from a formula in the code. This page writes each formula down, says which function computes it, and proves what we claim about it. Part A covers the current algorithm, `algo-6.0.0`. Part B takes each version in [`CHANGELOG.md`](CHANGELOG.md) and shows, as maths, what it changed, which experiment decided it and which result justifies it.

Each result is checked by [`math_check.py`](math_check.py) in the lab image (`rematch-lab:main`), under the same name (A1, A2 and so on). There are five kinds of check:

- **constants:** every constant used here, read from the Rust and TypeScript sources and from [`manifest.json`](manifest.json). When the image's `cli` is this build, `cli algo show --json` must equal the manifest.
- **symbolic:** identities checked with sympy.
- **simulation:** each probabilistic claim, simulated from a fixed seed, within 4 standard errors.
- **build:** the production code's own outputs, the lab's golden responses in `lab/fixtures/golden/`, recomputed from the formulas here. These are the estimates, the trajectory's rating rows, the focus targets and verdicts, and the timeline's confidences.
- **numbers:** every number quoted on this page and in the README's worked example.

Where a shared lemma has a Lean proof in [`lab/lean/`](../lab/lean/README.md), the result names it. Where an experiment's `math.md` proves a result in full, we state the result and cite it rather than prove it again. Places where the shipped code and its documentation disagree are listed under [Findings](#findings).

## Notation

| Symbol | Meaning | Source |
|---|---|---|
| $x$ | the mover's centipawns, clamped to $[-1000, 1000]$; a mate is $\pm 1000$ | `chess_core::score::score_to_cp` |
| $k_W = 0.00368208$ | the win-chance slope | `score.rs` `WIN_K` |
| $W(x)$ | win chance in $[0, 100]$ | `score::win_percent` |
| $\Delta W$ | $\max(0, W_{\text{before}} - W_{\text{after}})$, from the mover's side | `score::move_accuracy`, detectors |
| $t = 1, \dots, G$ | a player's games in the 183-day window, $g = G$ their count | `stats::priority::estimate_weakness` |
| $c_t, e_t$ | chances and events of one weakness in game $t$ | `weakness_series_point` |
| $a_t$, $w_t = 2^{-a_t/60}$ | the game's age in days and its recency weight | `stats::weights::recency_weight` |
| $\theta$ | a player's true rate per chance | – |
| $\mu$, $\nu$ | the peers' pooled rate (the prior mean) and the band median (the comparison) | `BandReferenceStat::pooled_rate`, `median` |
| $n_0$ | the key's prior strength: W1 300, W2 100, W3 60, W4 40, W5 and W6 30, W7 40, W8 80, W9a 30, W9b 40, W9c 60, W10 600, W11 300 | `profile_core::constants::n0` |
| $L, U$ | the ends of the 80% range | `stats::posterior::posterior` |
| $z = z_{0.80} = \Phi^{-1}(0.9) = 1.2815515655446004$ | the 80% two-sided quantile | `stats::intervals::Z80`, the web's `Z[0.8]` |
| $r_0, \dots, r_{n-1}$ | pre-game ratings in played order | `coaching::trajectory::compute_with` |
| $q = \ln 10 / 400$ | the rating scale | `trajectory::RATING_SCALE` |

# Part A: the current algorithm (algo-6.0.0)

## A1. Win chance from an evaluation

`score::winning_chances` computes $c(x) = 2/(1 + e^{-k_W x}) - 1$, and `score::win_percent` computes $W(x) = 50 + 50\,c(x)$. These are the Lichess formulas (ADR 0008).

**Result A1.** The win chance is a logistic curve:

```math
c(x) = \tanh\!\Big(\frac{k_W x}{2}\Big), \qquad W(x) = \frac{100}{1 + e^{-k_W x}}, \qquad W(-x) = 100 - W(x), \qquad W'(0) = 25\,k_W .
```

So the two sides' win chances always add to 100. Near equality, one centipawn is worth $25 k_W = 0.0921$ points. Because of the clamp, $W$ lies in $[W(-1000), W(1000)] = [2.46, 97.54]$.

*Proof.* Write $u = e^{-k_W x}$. Then $2/(1 + u) - 1 = (1 - u)/(1 + u)$, and $(1 - e^{-y})/(1 + e^{-y}) = \tanh(y/2)$ with $y = k_W x$. Also $50 + 50(1 - u)/(1 + u) = 100/(1 + u)$. For the symmetry, $100/(1 + e^{k_W x}) = 100 e^{-k_W x}/(1 + e^{-k_W x}) = 100 - 100/(1 + e^{-k_W x})$. Differentiating, $W'(x) = 100 k_W u/(1 + u)^2$, which is $25 k_W$ at $u = 1$. $W$ is increasing, so the clamp bounds it by its values at $\pm 1000$. $\blacksquare$

Checked by `A1 symbolic` and `numbers` ($W(100) = 59.10$, $W(300) = 75.11$, the README's $W(271) = 73.06$ and $W(-111) = 39.92$).

## A2. Judgements

`chess_core::judgement::judge` calls a move a blunder, mistake or inaccuracy when the mover's chances fall by at least 0.3, 0.2 or 0.1. A mate has its own rule: walking into a mate is a blunder unless $x_{\text{before}} < -700$ (a mistake) or $< -999$ (an inaccuracy), and losing one's own mate is judged by $x_{\text{after}}$ against $700$ and $999$ in the same way.

**Result A2.** On the 0 to 100 scale, the thresholds are exactly $\Delta W \ge 15$, $10$ and $5$. Outside the mate rules, `DECISIVE_DW` = 15 (detectors.rs) is therefore the blunder line. Because the clamp holds $x$ at $\ge -1000$, "$x < -999$" means exactly $x = -1000$, the clamp's floor.

*Proof.* $W_{\text{before}} - W_{\text{after}} = 50\,(c_{\text{before}} - c_{\text{after}})$ by A1, so a drop of $c$ by $0.3$ is a drop of $W$ by $15$. The clamp's range is the integers in $[-1000, 1000]$, and the only one below $-999$ is $-1000$. $\blacksquare$ Checked by `A2 symbolic`.

## A3. Accuracy per move

`score::move_accuracy` returns 100 when $W_{\text{after}} \ge W_{\text{before}}$. Otherwise it returns $\operatorname{clamp}(A e^{-\kappa d} - B + 1, 0, 100)$, with $d = \Delta W$, $A = 103.1668100711649$, $\kappa = 0.04354415386753951$ and $B = 3.166924740191411$ (lila's curve).

**Result A3.** The accuracy is non-increasing in $d$. It equals 100 for every $d \le d_{100}$ and 0 for every $d \ge d_0$, where

```math
d_{100} = \frac{1}{\kappa}\ln\frac{A}{99 + B} = 0.2237, \qquad d_0 = \frac{1}{\kappa}\ln\frac{A}{B - 1} = 88.72 .
```

Between them it falls strictly. The README's 28. Nd2?? loses $\Delta W = 33.14$ and scores 22.2.

*Proof.* The raw curve has derivative $-A\kappa e^{-\kappa d} < 0$, so it is strictly decreasing. Clamping a decreasing function to $[0, 100]$ keeps it non-increasing. Setting $A e^{-\kappa d} - B + 1 = 100$ gives $e^{-\kappa d} = (99 + B)/A$, and setting it to 0 gives $e^{-\kappa d} = (B - 1)/A$. The raw value at $d = 0$ is $A - B + 1 = 101.0$, which the clamp cuts to 100, so a drop smaller than $d_{100}$ scores 100. That is the "+1" of lila's curve. $\blacksquare$ Checked by `A3 symbolic` and `numbers`.

## A4. Accuracy per game

`chess_core::game_accuracy::game_accuracy_from` ports lila's `gameAccuracy`. Let the colour's moves have accuracies $a_1, \dots, a_m$ (A3). Each move gets a weight $v_i = \operatorname{clamp}(\operatorname{sd}(\text{window}_i), 0.5, 12)$. Here $\operatorname{sd}$ is the population standard deviation of White's win chances over a window of $\operatorname{clamp}(\lfloor M/10 \rfloor, 2, 8)$ positions, $M$ is the game's number of moves, and the first window is shared by the first moves, as in lila. The game accuracy is

```math
\mathrm{GA} = \tfrac12\Big(\frac{\sum_i v_i a_i}{\sum_i v_i} + \frac{m}{\sum_i 1/\max(1, a_i)}\Big).
```

**Result A4.** (a) The harmonic half $H$ lies between $\min_i \max(1, a_i)$ and the arithmetic mean of the $\max(1, a_i)$. (b) $\min_i a_i \le \mathrm{GA} \le \max_i \max(1, a_i)$. (c) One move at accuracy $a_j \le 1$ caps the harmonic half at $H \le 100m/(m + 99)$, however good the other moves are: 28.8 in a game of 40 own moves. That is the sense in which the README says the mean "punishes one bad move more than an average would".

*Proof.* (a) The harmonic mean of positive numbers is at least their minimum and at most their arithmetic mean (the AM–HM inequality). (b) The weighted half is a convex combination of the $a_i$, because the weights are positive (at least 0.5). So it lies in $[\min a_i, \max a_i]$. Also $H \ge \min_i \max(1, a_i) \ge \min_i a_i$ by (a). Hence $\mathrm{GA} \ge \min a_i$, and $\mathrm{GA} \le \max \max(1, a_i)$. (c) Every $a_i \le 100$, so $\sum_i 1/\max(1, a_i) \ge 1 + (m - 1)/100$, and $H \le m/(1 + (m - 1)/100) = 100m/(m + 99)$. $\blacksquare$ Checked by `A4 grid` (20,000 vectors) and `A4 simulation` (2,000 random games through a Python reimplementation of the port, both colours).

## A5. The rate per chance: the posterior

`stats::priority::estimate_weakness` runs the production prior rule, `PriorRule::CHAMPION` (variant.rs). This is EXP-0006's challenger, shipped in `algo-2.0.0`. From the points no older than 183 days (`WINDOW_DAYS`), `data_counts` forms

```math
k = \sum_t w_t e_t, \quad n = \sum_t w_t c_t, \quad S_2 = \sum_t w_t^2 c_t, \quad \hat p = \operatorname{clamp}(k/n, 0, 1), \quad n_{\text{eff}} = \frac{n^2}{S_2},
```

```math
\varphi = \operatorname{clamp}\Big(\frac{1}{G_c - 1}\sum_{t:\,c_t > 0}\frac{(e_t - c_t\hat p)^2}{c_t\,\hat p(1 - \hat p)},\ 1,\ 10\Big), \qquad \tilde n = n_{\text{eff}}/\varphi, \qquad \tilde k = \hat p\,\tilde n,
```

with $\varphi = 1$ when fewer than two games have a chance ($G_c < 2$) or $\hat p \in \lbrace 0, 1 \rbrace$. `prior_strength` caps the prior at two of the player's games, $s_0 = \min(n_0, 2\tilde n/g)$, and `posterior` returns

```math
\theta \mid \text{data} \sim \operatorname{Beta}\big(s_0\mu + \tilde k,\ s_0(1 - \mu) + \tilde n - \tilde k\big).
```

If a shape would be 0 or less, it is floored at $10^{-3}$, and counts whose total would exceed $10^7$ (`MAX_SHAPE`) are scaled down together; no real series comes near it. Without a benchmark the prior is the flat $\operatorname{Beta}(1, 1)$ ($\mu = \tfrac12$, $s = 2$), and so is W5's (A7).

**Result A5.**

(a) *Conjugacy.* With unit weights and no dispersion ($w_t = 1$, $\varphi = 1$), a $\operatorname{Beta}(s\mu, s(1-\mu))$ prior and a binomial likelihood give exactly the posterior above.

(b) *Mixture.* The posterior mean is $\hat m = \lambda\mu + (1 - \lambda)\hat p$, with $\lambda = s_0/(s_0 + \tilde n)$.

(c) *The cap fixes the prior's share.* For a peer key, $\lambda \le 2/(g + 2)$, with equality exactly when $2\tilde n/g \le n_0$. That is 0.0625, 0.0323, 0.0164 and 0.0079 at 30, 60, 120 and 250 games. Then $E[\hat m \mid c] - \theta = \tfrac{2}{g+2}(\mu - \theta)$.

(d) *The effective count.* Given the chances, $\operatorname{Var}[\hat p \mid c] = \theta(1-\theta)/n_{\text{eff}}$ for binomial games, and $n \le n_{\text{eff}} \le \sum_t c_t$.

(e) *The dispersion.* $\sum_t (e_t - c_t p)^2/c_t = \sum_t (e_t - c_t\bar e)^2/c_t + (\sum_t c_t)(\bar e - p)^2$, with $\bar e = \sum e_t/\sum c_t$. At the true rate, the summand has mean $d_t$, the game's design effect. So $\varphi$ estimates the game-to-game overdispersion, and $\tilde n$ discounts the evidence by it.

*Proof.* (a) The prior density times the likelihood is proportional to $\theta^{s\mu + k - 1}(1-\theta)^{s(1-\mu) + n - k - 1}$ on $(0, 1)$.

Machine-checked: `lab/lean/Rematch/BetaBinomial.lean` theorem `Rematch.beta_binomial_posterior`, for natural $k \le n$ and positive shapes. Its hypotheses hold in the unit-weight case. They do not cover real-valued $\tilde k$, $\tilde n$: there the Beta is the moment-matched approximation that (d) and (e) justify.

(b) $(s_0\mu + \hat p\tilde n)/(s_0 + \tilde n) = \lambda\mu + (1 - \lambda)\hat p$.

(c) $\lambda(s) = s/(s + \tilde n)$ has derivative $\tilde n/(s + \tilde n)^2 > 0$. It is largest at $s = 2\tilde n/g$, where it equals $2/(g + 2)$. The bias follows by linearity, since $E[\hat p \mid c] = \theta$.

(d) The $e_t$ are independent with variance $c_t\theta(1-\theta)$, so $\operatorname{Var}[\hat p \mid c] = \theta(1-\theta)S_2/n^2$. Next, $n_{\text{eff}} - n = n(n - S_2)/S_2 \ge 0$, because $0 < w_t \le 1$. For the upper end, Lagrange's identity $(\sum c_t)(\sum w_t^2c_t) - (\sum w_tc_t)^2 = \sum_{s<t}c_sc_t(w_s - w_t)^2 \ge 0$ gives $n^2 \le S_2 \sum c_t$.

(e) Expand $e_t - c_tp = (e_t - c_t\bar e) + c_t(\bar e - p)$. The cross term sums to $2(\bar e - p)\sum_t(e_t - c_t\bar e) = 0$. $E[(e_t - c_t\theta)^2] = c_t\theta(1-\theta)d_t$ by the law of total variance. $\blacksquare$

These are EXP-0002's Results 1 and 3 and EXP-0006's Results 1 to 3, which prove them in full and measure the approximations. Checked by `A5 symbolic`, `A5 simulation`, and by `A5-A7 build`, which recomputes every estimate of `lab/fixtures/golden/profile-response.json` from its request to $10^{-9}$. For `lab-hangs`, W1 has $\tilde k = 45.773$ and $\tilde n = 2885.42$ (raw $\varphi = 0.73$, raised to 1), $s_0 = 96.18 < n_0 = 300$, and $\lambda = 2/62$ exactly.

**Remark (what the rule claims against what the data do).** EXP-0006 Result 4 shows that, to first order, the posterior variance over the sampling variance is $(1 - \lambda)\bar D/\varphi$. The old rule's ratio was $r\bar D(1 - \lambda)$, with $r = S_2/n$ the weight concentration (EXP-0002 Result 6). Result 5 there gives the coverage and its lean towards the peers. EXP-0006 measured the shipped rule at 0.75 to 0.85 on all 20 key × size cells (Part B, `algo-2.0.0`).

## A6. The 80% range, the ranking floor and the false-weakness guard

The range is $I = [L, U]$ with $L = \min(q_{0.1}, \hat m)$ and $U = \max(q_{0.9}, \hat m)$, the quantiles of the posterior of A5 (`posterior`, `INTERVAL_LEVEL` = 0.8). The confidence is $P(\theta > \nu \mid \text{data}) = 1 - F(\nu)$ (`prob_above`), set to 0 when the window has no event.

An estimate is **credible**, and may be ranked, when all of these hold (`estimate_with_prior`, ADR 0101):

- the reason is `ok`: $g \ge 30$ (`MIN_GAMES`), a benchmark exists, and there is at least one event;
- $L > \nu$;
- the excess of A7 is at least 0.02 points a game (`RANK_MIN_EXCESS_PER_GAME`).

**Result A6.**

(a) *Credible implies confidence above 0.9.* So the 0.7 gate of ADR 0084 ("Costing you games" needs confidence $\ge 0.7$) is always met by a ranked weakness.

(b) *The Bayesian false-weakness bound.* Suppose the model of A5(a) is correctly specified: $\theta$ drawn from the prior the posterior uses, unit weights, binomial games. Then the share of credible estimates whose true rate is at or below the median is below 0.10:

```math
P(\theta \le \nu \mid \text{credible}) = E\big[F(\nu) \mid \text{credible}\big] < 0.10 .
```

(c) *The frequentist form.* For a fixed $\theta \le \nu$, $P(\text{credible} \mid \theta) \le P(L > \theta \mid \theta)$. That is the share of ranges lying wholly above the truth, about 0.10 for a calibrated equal-tailed 80% range (EXP-0006 Result 6).

*Proof.* (a) $L \le q_{0.1}$, so $q_{0.1} \ge L > \nu$. The Beta CDF is strictly increasing on $(0, 1)$, so $F(\nu) < F(q_{0.1}) = 0.1$, and the confidence $1 - F(\nu) > 0.9 \ge 0.7$.

(b) Credible is a function of the data. On that event $F(\nu) = P(\theta \le \nu \mid \text{data}) < 0.1$ by (a). Under a correctly specified model the posterior is the conditional law of $\theta$. So $P(\theta \le \nu, \text{credible}) = E[\mathbf 1_{\text{credible}} F(\nu)] < 0.1\,P(\text{credible})$, by the tower property.

(c) $\theta \le \nu < L$ on the event, so $\mathbf 1_{\text{credible}} \le \mathbf 1_{L > \theta}$. Take expectations. $\blacksquare$

The bound in (b) is exact only when the posterior is the true one. A correctly specified model needs $\varphi = 1$ and a prior that does not depend on the data. The shipped rule estimates $\varphi$ (raised to at least 1, so about half of binomial samples get $\varphi > 1$) and sets $s_0$ from $\tilde n$, so the posterior is an approximation, and (b) holds only as far as A5's ranges are calibrated. That is what EXP-0006 measured. The registered guard is (c) on its simulated players: the rise in credible estimates for players at or below their peers' rate, at most 0.01. It measured 0.0089. Checked by `A6 simulation`, which tests the textbook conjugate rule ($n_0 = 300$ fixed, no cap, no $\varphi$, $\nu = \mu$), not the production rule: 200,000 draws with $\theta$ from the prior. Every credible draw has confidence $\ge 0.9$. The flagged share at or below the median is 0.038, against its posterior average of 0.036, and (c), a pointwise implication, holds on every draw at $\theta = 0.01$ and $0.02$. On the golden profile, every credible estimate has confidence $\ge 0.9$ (`A5-A7 build`).

## A7. The excess, the priority and W5's benchmark

With the band's expected points lost per event $\epsilon$ (`eplPerEvent`) and $G$ games in the window, `estimate_with_prior` computes

```math
\text{excess} = \max(0, \hat m - \nu)\cdot\frac{\sum_t c_t}{G}\cdot\epsilon, \qquad
\text{priority} = \text{excess}\cdot\tau_{\kappa, B}\cdot\text{confidence}\cdot\mathbf 1_{\text{credible}},
```

with $\tau_{\kappa, B}$ the key's trainability in the band (`constants::trainability`). Fatigue is 0 until plans exist.

W5 has no peer rate (ADR 0101). `model_baseline` gives it the rate $\nu_5 = \min(0.15/e, 1)$ and $\epsilon = e$ ($e$ is taken as 1 when it is missing or invalid, and capped at 1). Here $0.15 = (100 - T_{\text{win}})/100$ with $T_{\text{win}} = 85$, and $e \in (0, 1]$ is the player's own score lost per failure (0.5 for a draw, 1 for a loss). The posterior keeps the flat $\operatorname{Beta}(1, 1)$.

**Result A7.**

(a) If each event costs $\epsilon$ points on average and the player has $\sum c_t/G$ chances a game, then $\text{excess} = \max(0, \hat m - \nu)(\sum c_t/G)\epsilon$ is the plug-in estimate of the points a game lost beyond a player at the median rate.

(b) For W5, $e(\theta - 0.15/e) = e\theta - 0.15$. The excess is the player's points lost per winning episode minus 0.15, times episodes per game.

(c) A winning episode is three own moves in a row at $W \ge 85$. Read as an expected score (A1), the evaluation predicts a loss of $1 - W/100 \le 0.15$ per episode. So the benchmark 0.15 is the largest loss the evaluation allows, and W5's excess is a lower bound on the excess over the evaluation's own prediction.

*Proof.* (a) By linearity of expectation, the expected points lost a game are $\theta \cdot (\text{chances a game}) \cdot \epsilon$, and the difference at $\theta$ against $\nu$ is $(\theta - \nu)(\text{chances a game})\epsilon$. The posterior mean is plugged in, and the difference is floored at 0. (b) is algebra. (c) $W \ge 85$ at each move, so $1 - W/100 \le 0.15$ at the episode's start. Replacing the prediction by its upper bound can only lower $e\theta - \text{prediction}$. $\blacksquare$ Checked by `A7 symbolic` and `A5-A7 build`. On `lab-hangs`, W5's excess is $(0.2703 - 0.15)(20/60)\cdot 1 = 0.0401$.

## A8. Comparisons between two posteriors

`posterior::p_improvement` estimates $P(\theta_{\text{now}} < (1 - \delta)\theta_{\text{before}})$ with $\delta = 0.15$ (`P_IMP_DELTA_REL`) for the timeline's $P_{\text{imp}}$. It uses 10,000 Monte Carlo draws from the two independent Beta posteriors, seed 1. `p_regression` is the mirror image. The trajectory's and the period comparison's rate rows (`trajectory::compare_rates`, the web's `compareRates`) use $\delta = 0$ (`p_improvement(..., 0.0)`, `p_regression(..., 1.0)`): they call a change when $P(\theta_b < \theta_a) \ge 0.8$ or $P(\theta_b > \theta_a) \ge 0.8$ and the relative difference of the posterior means is at least 15%.

**Result A8.** (a) The estimate is the mean of 10,000 independent indicators. It is unbiased for the probability, with standard error $\sqrt{P(1-P)/10^4} \le 0.005$. (b) Because the seed is fixed, the same posteriors give the same number every time.

*Proof.* (a) Each draw's indicator is Bernoulli($P$), so the mean has variance $P(1-P)/N$, and $P(1-P) \le \tfrac14$. (b) The generator (`stats::rng`) is a deterministic function of the seed. $\blacksquare$ Checked by `A8 simulation`, which is only the arithmetic bound; the estimator's determinism is checked by the Rust tests that pin $P_{\text{imp}}$ in the golden responses.

## A9. The timeline: change points

`timeline::series::evaluate_series` runs Adams and MacKay's run-length detector (`timeline::bocpd`) on the per-game counts $(k_t, n_t)$. The hazard is $H = 1/200$ (`DEFAULT_HAZARD`), and each segment's prior is $\operatorname{Beta}(50\mu, 50(1-\mu))$ (`BOCPD_PRIOR_STRENGTH`). The predictive of game $t$ on a run with posterior $\operatorname{Beta}(a, b)$ is

```math
p(k_t \mid a, b) = \binom{n_t}{k_t}\frac{B(a + k_t,\ b + n_t - k_t)}{B(a, b)},
```

and the recursion is $P(r_t = r + 1) \propto P(r_{t-1} = r)\,p(k_t \mid a_r, b_r)(1 - H)$ and $P(r_t = 0) \propto H \sum_r P(r_{t-1} = r)\,p(k_t \mid a_r, b_r)$.

`change_summary`'s $p_{\text{change}}$ is the mass on runs shorter than $\min(50, t - \text{segment start} - 10)$. A change is confirmed when $p_{\text{change}} \ge 0.8$ (`CONFIRM_P_CHANGE`), the rates before and after differ by at least 15%, and the chances since the onset reach the key's `min_evidence`. The segment's posterior for the states (A11) is $\operatorname{Beta}(n_0\mu + k, n_0(1 - \mu) + n - k)$ on its unweighted counts. This is the uncapped rule of `algo-1.x`, kept for the timeline in `algo-2.0.0`.

**Result A9.**

(a) The predictive is the Beta-binomial: $\int_0^1 \binom{n}{k}\theta^k(1-\theta)^{n-k}\,\operatorname{Beta}(\theta; a, b)\,d\theta$ equals the formula above.

(b) The recursion is the exact posterior of the run length under the product-partition model: change points independently after each game with probability $H$, a fresh $\theta \sim \operatorname{Beta}(50\mu, 50(1-\mu))$ in each segment, and binomial games. This holds as long as no run reaches the 500-game cap.

(c) With no evidence (every $n_t = 0$), the mass on runs shorter than $L$ after $t \ge L$ games is $1 - (1 - H)^L$. That is $0.2217$ at $L = 50$, so the threshold 0.8 is never met on the prior alone.

*Proof.* (a) $\int_0^1 \theta^{a + k - 1}(1 - \theta)^{b + n - k - 1}\,d\theta = B(a + k, b + n - k)$.

(b) Condition on the run length. The last $r$ games form one segment, whose marginal likelihood is the product of the sequential predictives (a), because the Beta updates conjugately. The prior of the run length is geometric with the hazard. Bayes' rule, applied one game at a time, is the recursion.

(c) With all predictives equal to 1, the run-length prior after $t$ games puts $H(1-H)^j$ on $j < t$ and $(1-H)^t$ on $t$. So $P(r < L) = \sum_{j < L} H(1-H)^j = 1 - (1 - H)^L$. $\blacksquare$

Past the cap, the code merges the run that would exceed 500 games into the last bucket and keeps the heavier run's parameters. That is an approximation, stated in `bocpd.rs`; no fixture or benchmark series reaches it. Checked by `A9 symbolic` (three exact integrals and the geometric sum), `A9 exact` (the recursion against all $2^7$ partitions of a 7-game series, to $10^{-10}$; the no-evidence mass) and `A9 build` (every timeline row's confidence in the golden response).

## A10. The timeline: CUSUM alarms

The early warning (`series::cusum_now`) is Page's one-sided CUSUM. It runs on the segment's games against $p_0 = $ the segment's posterior mean and $p_1 = 1.5\,p_0$ (`CUSUM_FACTOR`), with $h = 4$ (`CUSUM_DEFAULT_H`):

```math
S_t = \max\big(0,\ S_{t-1} + k_t\ln\tfrac{p_1}{p_0} + (n_t - k_t)\ln\tfrac{1 - p_1}{1 - p_0}\big).
```

The relapse watch (`ResolvedWatch`) is the same statistic from the resolved level. It uses $p_1 = \max(\nu, 1.15\,p_0, p_0 + 0.005)$ and $h = 10$ (`RELAPSE_CUSUM_H`).

**Result A10.** (a) The increment is the log-likelihood ratio of game $t$. Under $p_0$ its mean is $-n_t\,\mathrm{KL}(p_0 \,\Vert\, p_1) < 0$. (b) With $p_0$ known and fixed, the in-control average run length is at least $e^h$ games: 54.6 at $h = 4$ and 22,026 at $h = 10$ (Lorden 1971; it needs the games independent and binomial at $p_0$, so that each likelihood ratio has mean 1 under $p_0$). ADR-R2-6's simulated 140,000 games at $h = 10$ is consistent with this.

*Proof.* (a) $E[k] = n p_0$, so $E[\text{increment}] = n\big(p_0 \ln\tfrac{p_1}{p_0} + (1 - p_0)\ln\tfrac{1 - p_1}{1 - p_0}\big) = -n\,\mathrm{KL}(p_0 \Vert p_1)$. KL is positive for $p_1 \ne p_0$ (Gibbs' inequality). (b) The CUSUM stops at the first $t$ at which some suffix of the log-likelihood ratios sums past $h$. Each suffix's likelihood ratio is a mean-1 martingale under $p_0$, so by Ville's inequality the chance that one started at a given game ever exceeds $e^h$ is at most $e^{-h}$. Lorden (1971, Theorem 2) turns this into $E_0[T] \ge e^h$; we cite the theorem rather than prove it again. $\blacksquare$

The bound needs $p_0$ fixed. The early warning plugs in the segment's own posterior mean, estimated from the same games. That makes it a test of whether the latest games run above the segment's own level, as ADR-R2-6 §11 states, and the bound is then a guide, not a guarantee. Overdispersion (A5(e), $\varphi > 1$) breaks it too: the binomial likelihood ratio then has mean above 1 under $p_0$, and alarms come sooner. A CUSUM alarm sets "emerging" only with the evidence gate, above the band median, and after 10 games in the segment. Checked by `A10 symbolic`, `A10 grid` and `A10 simulation` (known $p_0 = 0.02$, 50 chances a game: mean run length 864 games, at least $e^4$).

## A11. The timeline: the states

`timeline::states::next_state` is a first-match rule list over the signals of A9 and A10, the confidence $P(\theta > \nu)$ of the segment's posterior, a streak of games below the band's 40th percentile (`RESOLVE_STREAK` = 10), and $P_{\text{imp}}$ (A8, with $\delta = 0.15$).

**Result A11.** (a) The rule is a total function. Every previous state and every signal value gives exactly one next state, and a NaN or out-of-range probability satisfies no comparison, so it claims nothing. (b) No weakness becomes `active` on fewer chances than its key's `min_evidence`, or on none. (c) `resolved` is left only through `relapsed`, which needs the relapse CUSUM (A10, $h = 10$), the evidence gate, a rise of at least 0.005 and 15%, and confidence $\ge 0.7$ on the games since the CUSUM's change point.

*Proof.* (a) The list ends with an unconditional `not_an_issue`, and `prob` maps any value outside $[0, 1]$ to NaN, for which every comparison is false. (b) Entering `active` needs confidence $\ge 0.7$ (`ACTIVE_CONFIDENCE`), and `series.rs` sets the confidence to 0 below `min_evidence` or with no event. Once active, the persistence rule (tracked, above the median) keeps it at any confidence, but every segment it can then be on has at least the minimum evidence: a confirmed change needs the chances since its onset to reach `min_evidence`, the relapse rebaseline needs the same, and a segment's counts only grow. (c) Read off the rules and the signal definitions in `evaluate_series`. $\blacksquare$ This is logic, not probability, and it is checked by `backend/crates/profile-core/tests/timeline_states.rs`. `A9 build` recomputes the confidences the rules read.

## A12. The trajectory's rating verdict: memory-halves

`coaching::trajectory::compute_with` under `RatingRule::PRODUCTION = MemoryHalves` (ADR 0117) works on the pre-game ratings $r_0, \dots, r_{n-1}$. On chess.com, the pre-game rating is the rating after the previous game (`pre_game_ratings`). A verdict needs $n \ge 20$ (`TRAJECTORY_MIN_GAMES`). The rule takes

```math
W = \min(30, \lfloor n/2 \rfloor), \qquad A = [0, W), \quad B = [n - W, n), \qquad D = \bar r_B - \bar r_A .
```

`Memory::of` computes $s^2 = \frac{1}{n-1}\sum_t (r_{t+1} - r_t)^2$, $\hat a = \min(qs/2, 1)$, $\hat\varphi = \min(1 - \hat a, 1 - 10^{-12})$ and $\hat\sigma^2 = s^2/(\hat a(2 - \hat a))$. With $A = [a_0, a_1)$ and $B = [b_0, b_1)$ half-open, $m_A = a_1 - a_0$ and $m_B = b_1 - b_0$, `Memory::block` and `Memory::diff_var` give

```math
S(m) = \frac{m(1 + \varphi)}{1 - \varphi} - \frac{2\varphi(1 - \varphi^m)}{(1 - \varphi)^2}, \qquad
C = \frac{\varphi^{\,b_0 - a_1 + 1}(1 - \varphi^{m_A})(1 - \varphi^{m_B})}{(1 - \varphi)^2},
```

```math
V = \sigma^2\Big(\frac{S(m_A)}{m_A^2} + \frac{S(m_B)}{m_B^2} - \frac{2C}{m_A m_B}\Big).
```

`memory_compare` says "falling" when $D + z\sqrt{\hat V} < 0$ and "rising" when $D - z\sqrt{\hat V} > 0$. Each window's own range is $\bar r \pm z\hat\sigma\sqrt{S(m)}/m$. The change it states is $D$, rounded.

**Result A12.** Model the rating error $e_t = r_t - \theta$ of a player of constant strength as a stationary AR(1), $e_{t+1} = \varphi e_t + \varepsilon_t$, with uncorrelated innovations of variance $\sigma^2(1 - \varphi^2)$. Then:

(a) $\operatorname{Cov}(e_s, e_t) = \sigma^2\varphi^{|s - t|}$, and $1 - \varphi^2 = a(2 - a)$ with $a = 1 - \varphi$.

(b) $\sum_{i, j \in A}\varphi^{|i - j|} = S(m_A)$, and $\sum_{i \in A, j \in B}\varphi^{j - i} = C$.

(c) $\operatorname{Var}(D) = V$ exactly, and $\theta$ cancels.

(d) With Gaussian innovations and known $(\sigma^2, \varphi)$, "falling" and "rising" each have probability exactly $\Phi(-z) = 0.10$.

(e) The plug-in follows from an Elo-type update to first order. Its error is $O(a)$: under $p(1-p) = v = \tfrac14$, both $\hat a/a = \sqrt{2/(2 - a)}$ and $\hat\sigma^2/\sigma^2$ (a different expression) are $1 + a/4 + O(a^2)$.

*Proof.* (a) to (d) are EXP-0030's Results 2, 5, 6 and 7, proved there in full. In outline: (a) iterate the recursion, and note that each innovation is uncorrelated with the past. (b) Count the $2(m - k)$ pairs at lag $k$ and sum the geometric series. Across windows the lag factorises as $(j - b_0) + (b_0 - a_1 + 1) + (a_1 - 1 - i)$. (c) $D = w^\top e$ with weights $-1/m_A$ on $A$ and $1/m_B$ on $B$, so $\operatorname{Var}(D) = w^\top\Gamma w$, which is (b) term by term. The weights sum to 0, so $\theta$ drops out. (d) $D$ is a linear function of a Gaussian vector with mean 0, so $D/\sqrt V \sim N(0, 1)$ and $P(D < -z\sqrt V) = \Phi(-z)$. (e) is EXP-0030 Results 1 and 4. $\blacksquare$

Section 6 of EXP-0030 covers what the model leaves out: Glicko's changing step, the plug-in's error, and real players. Its measured false-alarm rates on 2,000 settled simulated players per cell are under Part B, `algo-6.0.0`. `window_mean_independence` (`lab/lean/Rematch/WindowMean.lean`) needs uncorrelated window noise, which this model does not have, so it is not cited. The block sums would need a new Lean lemma. Checked by `A12 symbolic` (the closed forms against the double sums, exactly in $\varphi$, and $V = w^\top\Gamma w$), `A12 simulation` (20,000 Gaussian AR(1) players at $\varphi = 0.984$, $\sigma = 31$, 120 games: $\operatorname{Var}(D)/V = 0.986$, falling 0.0985 and rising 0.1016; with the plug-in, falling 0.105) and `A12 build` (the golden rating rows of `lab-peaked` and `lab-steady` to $10^{-9}$, and no verdict for `lab-new` at 12 games).

## A13. The weekly focus: target and verdict

`apps/web/src/server/focus/goal.ts` `chooseTarget` takes the weakness's posterior mean $b \in (0, 1]$ (`focus/index.ts`) and the peer median $\pi$. With $\operatorname{tenth}(v) = \operatorname{round}(1000v + 10^{-9})/1000$, it computes

```math
\tau_{\text{rel}} = \operatorname{tenth}(0.75\,b), \qquad
\tau = \begin{cases} \operatorname{tenth}(\pi) & \text{if } \pi \le 0.9\,b \text{ and } \operatorname{tenth}(\pi) > \tau_{\text{rel}},\\ \tau_{\text{rel}} & \text{otherwise.}\end{cases}
```

`focusProgress` reads $k$ events in $n$ chances from the focus's games. Below 10 chances (`MIN_VERDICT_CHANCES`) it is too early to tell. Otherwise the verdict is "on track" when $k/n \le \tau$ and "not yet" otherwise. When the $N$-th game exists, the focus closes as met, not met or inconclusive by the same rule. The range shown is the 80% Wilson interval.

**Result A13.**

(a) $\tau$ is the easier (higher) of the two candidates.

(b) The target is strictly below $b$ for every $b > 0.005$, whatever the peer median. Below that the rounding can remove the step. Without a peer, the relative target is above $b$ for $b \in [0.000667, 0.001)$ and in a sliver just below 0.002 of width about $10^{-12}$, equals $b$ at $b = 0.001$ and $b = 0.002$, and is 0 below $b = 0.000667$. With a peer median just under $0.9\,b$, the peer target $\operatorname{tenth}(\pi)$ can round up to or past $b$ for baselines up to 0.005: for example $b = 0.0029$, $\pi = 0.0026$ gives $\tau = 0.003 > b$ (Finding 2).

(c) The verdict uses the point estimate, not the range. If the true rate has not moved from $b$, then $P(\text{on track}) = P(\operatorname{Bin}(n, b) \le \lfloor\tau n\rfloor)$. At $b = 0.2$ ($\tau = 0.15$) that is 0.376 after 10 chances and 0.411 after 20.

*Proof.* (a) Read off the cases. (b) The epsilon is added to $1000v$, so in rate units it is $10^{-12}$, and $\operatorname{tenth}$ moves a value up by at most $0.0005 + 10^{-12}$. So $\tau_{\text{rel}} \le 0.75b + 0.0005 + 10^{-12} < b$ when $0.25b > 0.0005 + 10^{-12}$, that is $b > 0.002$. On the peer branch $\pi \le 0.9b$ and $\operatorname{tenth}$ is non-decreasing, so $\tau \le 0.9b + 0.0005 + 10^{-12} < b$ when $b > 0.005$. The small baselines follow from the rounding grid: $\tau_{\text{rel}} = 0.001$ for $0.75b \in [0.0005 - 10^{-12}, 0.0015 - 10^{-12})$, which exceeds $b$ for $b < 0.001$. Likewise $\tau_{\text{rel}} = 0.002$ from $0.75b \ge 0.0015 - 10^{-12}$, that is $b \ge 0.0019999999987$. The worst peer is $\pi = 0.9b$, since $\operatorname{tenth}$ is monotone, which gives the peer branch's examples. (c) $k \sim \operatorname{Bin}(n, b)$, and $k/n \le \tau$ exactly when $k \le \lfloor \tau n \rfloor$. $\blacksquare$

The verdict therefore says "on track" by luck often. It is a progress cue, and nothing claims a level for it, so this is a property, not a finding. Checked by `A13 build` (the golden target and progress items), `A13 simulation` and `numbers` (a grid of $10^6$ baselines).

## A14. The trainer

`apps/web/src/server/train/model.ts`:

- `acceptedSet` accepts the best move, plus any MultiPV line within 5 win-chance points of it (`WIN_BAND`). When the best move mates in $m$, it accepts any mate in $m + 1$ or fewer instead.
- `isHeldOut` holds a position out when $\operatorname{fnv1a}(\texttt{holdout:}\langle\text{game}\rangle\texttt{:}\langle\text{ply}\rangle) \bmod 100 < 20$.
- `verdictOf` claims "lower" or "higher" only when both sides have at least 30 chances (`MIN_CHANCES`) and the two 80% Wilson ranges do not overlap.
- `afterAttempt` spaces the reviews at 1, 3, 7, 21 and 60 days (`INTERVALS`). It retires an item after 3 clean passes (`RETIRE_PASSES`): right and sure, at least 7 days after the previous attempt (`CLEAN_GAP_DAYS`).

**Result A14.**

(a) If the hash were uniform on 32-bit integers, the held-out share would be $20 \cdot \lceil 2^{32}/100 \rceil / 2^{32} = 0.200000000186$. The excess comes from $2^{32} = 100 \cdot 42{,}949{,}672 + 96$, which gives residues 0 to 95 one extra value.

(b) When nothing changed, two independent rates with equal large samples give non-overlapping 80% ranges with probability $\to 2\big(1 - \Phi(\sqrt2\,z)\big) = 0.070$. At 30 against 30 chances and a true rate of 0.2, the exact rate is 0.070.

(c) An item answered right and sure at every due date retires at its sixth attempt, on day $1 + 3 + 7 + 21 + 60 = 92$. A guess never counts as a clean pass.

*Proof.* (a) Count residues. (b) For large $n$, the Wilson interval is $\hat p \pm z\sqrt{p(1-p)/n} + o(n^{-1/2})$. The ranges miss each other when $|\hat p_1 - \hat p_2| > 2z\sqrt{p(1-p)/n}$. The difference has standard deviation $\sqrt2\sqrt{p(1-p)/n}$, so the standardised threshold is $\sqrt2 z = 1.812$, and the two-sided normal tail follows. For 30 against 30, enumerate the $31^2$ outcomes. (c) The first attempt has no previous one, and the next gaps are 1, 3, 7, 21 and 60 days. Only the gaps of 7, 21 and 60 days are at least 7, so the third clean pass comes at the sixth attempt. $\blacksquare$

So a non-overlap claim is a test at about 7% two-sided, stricter than its 80% ranges suggest (Schenker and Gentleman 2001). Checked by `A14 simulation` (both rates, and the holdout share over 200,000 keys) and `numbers`.

# Part B: version by version

Each section states what changed between versions as a map from inputs to outputs. "No output change" means the new map equals the old one on every input. The replay goldens' byte identity is the evidence for that, and each section says why it holds.

## algo-6.0.0

**What changed.** The trajectory's rating verdict is shown again, from a new rule. Before, it was `PeakBootstrap`, withheld (5.0.0). After, it is `MemoryHalves` (A12), shown:

```math
\text{before: } A = [\max(0, \min(i^\ast - \lfloor W/2\rfloor, n - 2W)), \dots),\ \widehat{\operatorname{Var}}(D) = \text{i.i.d. bootstrap} \qquad
\text{after: } A = [0, W),\ \widehat{\operatorname{Var}}(D) = \hat V \text{ of A12}.
```

Here $i^\ast$ is the peak's index, when it lies before $B$. The stated change was $\text{current} - \text{peak}$ when "then" sat on the peak. It is now $D = \bar r_B - \bar r_A$, the quantity the verdict tests. `player_visible` passes a trajectory unchanged only when `RATING_VERDICT_SHOWN` holds and its rule is `memory-halves`, so an input stored before 6.0.0 keeps its verdict withheld.

**Decided by** EXP-0030 (ADR 0117). On 2,000 fresh settled players per cell, memory-halves said "falling" to 0.103, 0.117 and 0.105, and "rising" to 0.089, 0.084 and 0.095, at 60, 120 and 250 games. The worst upper end at $1 - 0.025/2$ is 0.136, within the bound of 0.15.

**Justified by** A12(c, d): $V$ is the exact variance of $D$ under the memory model, so each side's false-alarm rate is 0.10 when the parameters are known. EXP-0030 Result 12 bounds the chance that a rule whose true rate exceeds 0.15 clears the primary: 0.00598 per look, 0.024 over two candidates and two looks.

## algo-5.0.1

**What changed.** Nothing in the output. `RatingRule` names the rating row's rule, and `compute` is `compute_with(·, RatingRule::PRODUCTION)` with `PRODUCTION = PeakBootstrap`.

**Justified by** this lemma: `compute_with(x, PeakBootstrap)` takes the bootstrap branch and the peak-centred window, which is the 5.0.0 `compute` line for line. So $f_{5.0.1} = f_{5.0.0}$. The candidates run only through `cli lab trajectory`. Evidence: the replay goldens and the lab's golden responses are byte-identical, and only `profile.source_digest` moved.

**Decided by** nothing: it prepares EXP-0030.

## algo-5.0.0

**What changed.** A visibility map is applied after the trajectory: $f_{5.0.0} = \mathrm{pv} \circ f_{4.0.1}$. `player_visible` sets `kind` to null, sets the rating row's verdict to `withheld`, and nulls its ranges. The means, $n$, the difference, the peak and the change stay. `RATING_VERDICT_SHOWN` = false.

**Decided by** EXP-0015 (ADR 0116). Of 700 settled simulated players, the trajectory told 44%, 62% and 78% that they were falling at 60, 120 and 250 games, against a bound of 15%.

**Justified by** EXP-0030 Result 10, which explains the failure. Under the memory model, the i.i.d. bootstrap estimates the variance of a $W$-game window mean with expectation $\sigma^2(W^2 - S(W))/W^3$, against the true $\sigma^2 S(W)/W^2$. The ratio is $(W^2 - S(W))/(W\,S(W)) = 0.0055$ at $W = 30$ and $\hat\varphi = 0.9843$ (EXP-0030's median), so the bootstrap's standard error is about 13.5 times too small. On top of that, "then" was centred on a selected maximum, which biases $D$ downwards. The peak is $\max(\max_t r_t, \text{current}) \ge \text{current}$, so "current − peak" is never positive. Withholding is the identity on everything else, so the other rows are unchanged.

## algo-4.0.1

**What changed.** Nothing in the output. `obviousCapture` moved, unchanged, into `profile/obvious-capture.ts`, and `trajectory_input` is the extracted pure part of `load_trajectory`.

**Justified by** this lemma: moving a function or extracting a pure part does not change the composed map. The extracted function is the same expression, applied to the same arguments. Evidence: byte-identical replay goldens and lab golden responses; only source digests moved.

**Decided by** nothing: it lets EXP-0011, 0012, 0014 and 0015 call the production rules (ADR 0115).

## algo-4.0.0

**What changed.** The node budget of every main search went from $N = 3 \times 10^5$ to $N = 10^6$ (`engine::ENGINE_NODES`, protocol `sf19-n1m-v1`). Every A1 to A7 quantity is now computed from $x^{(10^6)}$ instead of $x^{(3\cdot 10^5)}$. The MultiPV probe keeps $3 \times 10^5$ (`MULTIPV_NODES`). Stored games keep their own protocol, so the map of a stored game is unchanged.

**Decided by** EXP-0008 (ADR 0113). Against a $3 \times 10^7$-node reference on 160 rapid games, $10^6$ nodes disagree on 0.0246 of moves fewer than $3 \times 10^5$ nodes, with 95% interval [0.0197, 0.0296]. Against $10^8$ nodes on 40 games the figure is 0.0221 [0.0115, 0.0327]. Both intervals lie wholly above the tolerance $\delta = 0.01$.

**Justified by** EXP-0008 Result 1: the paired contrast depends on the reference only on the moves where the two settings disagree. So scoring just those moves loses nothing. Result 4 is the floor test: the lower end exceeds $\delta$ iff $(\hat R - \delta)/\widehat{\mathrm{SE}} > t_{\gamma, n-1}$, with probability $(1 - \gamma)/2$ at $\theta = \delta$. The cost is 39.1 CPU-seconds a game instead of 11.8. That makes a 300-game import $300 \times 39.1 / 3600 = 3.3$ CPU-hours, or 49 minutes on 4 engines.

## algo-3.0.0

**What changed.** Six motif tags stopped being named. Before, the motif series were built over all 16 tags $\mathcal T$. After, they are built over the visible set $\mathcal V$ (`PLAYER_VISIBLE_MOTIFS`, 10 tags), and a stored row of a tag in $\mathcal T \setminus \mathcal V$ is skipped wherever it is read. The W1 and W3 counts do not change, because a hidden-tag line still counts as a chance and an event: $c_t$ and $e_t$ of every W key are the same.

**Decided by** EXP-0007 (ADR 0112). A tag is shown when the lower end of its precision interval against Lichess puzzle themes is at least 0.80. `hangingPiece` (0.572), `pin` (0.402), `skewer` (0.810, interval [0.778, 0.838]), `deflection` (0.562), `sacrifice` (0.528) and `backRankMate` (0.606) did not clear it.

**Justified by** EXP-0007 Results 10 and 12. The floor test $\ell \ge 0.8$ is equivalent to $\hat R \ge 0.8 + z\sqrt{0.8 \cdot 0.2/n}$, and it shows a tag of true precision at most 0.8 with probability at most about $\Phi(-z_D) = 0.05/60$. Over the 30 intervals, Bonferroni bounds the family-wise chance of any wrong call by 0.025, under any dependence.

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.bonferroni` (each event of a family of $m$ has probability at most $\alpha/m$; here $\alpha = 0.025$ and $m = 30$ one-sided misses, each at most $\alpha/m = 0.05/60$, so the family-wise bound is 0.025).

## algo-2.0.0

**What changed.** The prior rule of A5 changed from `PriorRule::ALGO_1` to EXP-0006's challenger:

```math
\text{before: } \operatorname{Beta}\big(n_0\mu + k,\ n_0(1-\mu) + n - k\big), \qquad
\text{after: } \operatorname{Beta}\big(s_0\mu + \tilde k,\ s_0(1-\mu) + \tilde n - \tilde k\big),
```

where $k, n$ are the weighted sums, $\tilde n = n_{\text{eff}}/\varphi$, $\tilde k = \hat p\tilde n$ and $s_0 = \min(n_0, 2\tilde n/g)$ (A5). W5's flat prior, the timeline's posterior (A9) and the ranking rule (A6) are unchanged.

**Decided by** EXP-0006 (ADR 0111). On fresh seeds, every one of the challenger's 20 key × size coverages lies in [0.75, 0.85]. The worst is 0.828 (W5 at 30 games), with $D = 0.028$ [0.022, 0.043], against the champion's worst of 0.103 ($D = 0.697$). The coverage at the peers' rate moved from 0.874 to 0.804. Credible estimates for players at or below their peers' rate rose by 0.0089 [0.0086, 0.0093], inside the margin of 0.01.

**Justified by** two results.

- *Why the old ranges failed.* EXP-0002 Results 3, 6 and 7: the prior's share $n_0/(n_0 + n)$ reached 0.59 for W4.missed at 30 games, biasing the mean by that share of the gap $\mu - \theta$. Counting the weighted sums as chances made the posterior claim $1/r$ times the information the data carry. Both narrow the range around a biased centre.
- *Why the new ones work.* A5(c, d, e), which are EXP-0006 Results 1 to 3: the effective count removes the weight concentration exactly, the cap fixes the prior's share at $2/(g+2)$, and $\varphi$ puts back the game-to-game spread. EXP-0006 Result 6, A6(c) here, turns the false-weakness margin into a bound: at $\rho \le 1$, credible implies $L > \theta$.

## algo-1.1.1

**What changed.** Nothing in the output. `PriorRule` parameterises `estimate_weakness`, with production at `ALGO_1 = {prior_games: None, effective_counts: false, dispersion: false}`.

**Justified by** this lemma: under `ALGO_1`, `data_counts` returns the weighted sums $(k, n)$, since the `effective_counts` branch is not taken, and `prior_strength` returns $n_0$, since `prior_games` is `None`. So the posterior is $\operatorname{Beta}(n_0\mu + k, n_0(1-\mu) + n - k)$, the 1.1.0 rule exactly. Only `cli lab` and the bench call `variant::set`. Evidence: byte-identical replay goldens.

## algo-1.1.0

**What changed.** Nothing in the output. `select_focuses` became `select_focuses_of` applied to the ranked weaknesses.

**Justified by** this lemma: `select_focuses` on an input equals `select_focuses_of` on that input's ranked weaknesses, with the same order and limits, so the two are the same function. It is a minor version only because the version policy counts any change under `ai/src/validator` as one. Evidence: byte-identical replay goldens.

## algo-1.0.0

**What it is.** The first named version. Its maps are A1 to A4 at $N = 3 \times 10^5$ nodes (`sf19-n300k-v1`). The posterior is the `ALGO_1` rule above. The ranking rule is `rank-v1` (A6, A7, ADR 0101). The timeline is `tl-v1` (A9 to A11). The trajectory is `PeakBootstrap`, shown. The trainer is `trainer-v1` (A14) and the focus `focus-v1` (A13).

**What changed.** `Threads 1` and `Hash 16` are now sent explicitly. These are Stockfish's defaults, so the engine's map from position to evaluation is the same function. `backend/crates/engine/tests/uci_audit.rs` shows identical evaluations.

**Justified by** ADR 0101's floor, A6(a, c): a ranked weakness has its range wholly above the median and confidence above 0.9. On the first benchmark run, the floor removed the clean control's false alarm.

# KPIs

The engine's Tier 1 KPIs (ADR 0119), as `backend/crates/bench/src/kpi.rs` computes them. Every one is reported as $\{\text{value}, [\text{lo}, \text{hi}], \text{level}, n\}$ at level 0.8, from one row per synthetic player. They are synthetic numbers: they say how the pipeline does on simulated players with injected flaws, not on people. Checked by `math_check.py` under K0 to K5 (the constants of `kpi.rs` are K0).

Notation: $T$ flaw types, $n_t$ players of type $t$, $y_{ti} \in \{0, 1\}$ whether player $i$'s flaw is in the top 2 at the last checkpoint (80 games), $z = 1.2815515655446004$.

## K1. Recall at 2, pooled and per type: the Wilson interval

$\hat p_t = k_t / n_t$ with $k_t = \sum_i y_{ti}$; pooled $\hat p = \sum_t k_t / \sum_t n_t$ (`recall_at_2`, unchanged, EXP-0001's primary). The interval is Wilson's:

```math
\frac{\hat p + \frac{z^2}{2n} \pm z\sqrt{\frac{\hat p(1-\hat p)}{n} + \frac{z^2}{4n^2}}}{1 + \frac{z^2}{n}} .
```

**Result K1.** (a) It is the set of $p$ the score test does not reject at level 0.2: $\{p : (\hat p - p)^2 \le z^2 p(1-p)/n\}$. (b) At $k = n$ its lower end is $n/(n + z^2)$; at $k = 0$ its upper end is $z^2/(n + z^2)$.

*Proof.* (a) The inequality is a quadratic in $p$ with positive leading coefficient $1 + z^2/n$; its roots are the formula above, so the set is the interval between them. (b) Substitute $\hat p = 1$ (or 0): the square root becomes $z/(2n)$, and the ends simplify to the stated values. $\blacksquare$

Its coverage is not exactly 0.8 for a binomial (the lattice makes it oscillate), but its average over $p \in [0.05, 0.95]$ is within 0.02 of 0.8 at $n = 6, 22, 32$ (`K1 exact`, computed by summing the binomial, not simulated). Why Wilson: with $n_t$ as small as 3 and $\hat p_t \in \{0, 1\}$ common, the Wald interval collapses to a point.

## K2. D1a: flaw types detectable

D1a $= \#\{t : \text{lo}_t \ge 0.8\}$, the types whose Wilson lower end clears the target, and beside it $\#\{t : \hat p_t \ge 0.8\}$ by point estimate (`flaw_types_detectable`, `flaw_types_detectable_point`). $n$ is the number of types.

**Result K2 (power).** By K1(b), a type with $n_t$ players can clear 0.8 only if $n_t/(n_t + z^2) \ge 0.8$, that is $n_t \ge 4z^2 = 6.57$. So the smoke suite (3 a type) and EXP-0001's pilot (6 a type) can never count a type by its interval: their D1a by interval is 0 whatever the engine does, and only the point count says anything. The full suite (22 a type) needs 21 hits of 22; the probability of that is 0.339 when the true recall is 0.90 and 0.698 at 0.95. *Target reasoning:* "every type at least 0.8" is the same 0.8 bar as recall at 2 since ADR 0100, applied per type, because a pooled 0.8 can hide a type that is never found (today 0 of 6 for missed mates at every version).

## K3. D1b: macro recall at 2, with a stratified bootstrap

```math
M = \frac{1}{T}\sum_{t=1}^{T} \hat p_t .
```

Each type weighs the same whatever its player count. With equal $n_t$ (every suite today), $M$ equals the pooled recall; the two part when the counts differ.

The interval is the percentile interval of $B = 2000$ stratified resamples: within each type, $n_t$ players drawn with replacement; values sorted, the ends taken at indices $\lfloor 0.1B \rfloor$ and $\lfloor 0.9B \rfloor - 1$, as EXP-0001's arena does. The generator is mulberry32 seeded with 119 (`kpi::SEED`), so a run's interval is reproducible.

**Result K3.** (a) The types are independent, so $\operatorname{Var} M = T^{-2}\sum_t p_t(1-p_t)/n_t$, and the stratified bootstrap's variance is its plug-in, $T^{-2}\sum_t \hat p_t(1-\hat p_t)/n_t$. (b) The interval is first-order correct as every $n_t \to \infty$ with $p_t \in (0, 1)$.

*Proof.* (a) Within a stratum the resampled mean has variance $\hat p_t(1 - \hat p_t)/n_t$ given the data; the strata are resampled independently, and $M^*$ is their average with weights $1/T$. (b) Each $\hat p_t$ is asymptotically normal, so $M$ is too (a fixed linear combination), and the percentile interval of a smooth function of means is first-order correct (Hall 1992, 3.10). $\blacksquare$

Limits: a type with $\hat p_t \in \{0, 1\}$ adds no bootstrap variance, so with today's all-or-nothing types the interval is narrower than the truth; at 22 a type the plug-in standard error at today's per-type recalls (1, 1, 5/6, 0, 1/3) is 0.026, an 80% half-width of 0.033. `K3 simulation`: 400 samples of 22 players for each of five types with true recalls 0.95, 0.9, 0.8, 0.3 and 0.5; the 80% interval held the true macro recall 0.782 of the time (Monte Carlo standard error 0.02). *Target:* 0.8, as K2.

## K4. D5: the Kaplan–Meier median games to detection, with Greenwood

The time grid is the checkpoints (30, 45, 60, 80 games). A player's time is the first checkpoint with the flaw in the top 2 (an event), or 80 if it never is (censored at the last checkpoint). At each event time $t_j$, $r_j$ players are at risk (time $\ge t_j$) and $d_j$ are detected:

```math
\hat S(t) = \prod_{t_j \le t}\Big(1 - \frac{d_j}{r_j}\Big), \qquad
\widehat{\operatorname{Var}}\,\hat S(t) = \hat S(t)^2 \sum_{t_j \le t} \frac{d_j}{r_j(r_j - d_j)} .
```

The median is the first $t_j$ with $\hat S(t_j) \le 0.5$; its interval runs from the first $t_j$ with $\hat S - z\,\mathrm{se} \le 0.5$ to the first with $\hat S + z\,\mathrm{se} \le 0.5$ (Brookmeyer and Crowley's inversion, with the plain Greenwood band). When fewer than half are ever detected, the median is "not reached" (value null); when the upper band never reaches 0.5, the interval is null and the note says its upper end lies beyond 80 games. It replaces `mean_games_to_detection`, which averaged only the detected players and so ignored the ones never found (kept in the JSON for one version, deprecated).

**Result K4.** (a) With censoring only at the last checkpoint, $\hat S(t)$ is the share of players not yet detected at $t$, and Greenwood's variance is exactly the binomial $\hat S(1 - \hat S)/N$. (b) The median's interval has coverage at least about 0.8 when the true median is a grid point.

*Proof.* (a) Before the last time no one is censored, so $r_j = N\hat S(t_{j-1})$ and $d_j = N(\hat S(t_{j-1}) - \hat S(t_j))$. The product telescopes to $\hat S(t_j) = r_{j+1}/N$, and

```math
\sum_j \frac{d_j}{r_j(r_j - d_j)} = \frac1N\sum_j\Big(\frac{1}{\hat S(t_j)} - \frac{1}{\hat S(t_{j-1})}\Big) = \frac1N\Big(\frac{1}{\hat S(t)} - 1\Big),
```

so $\hat S^2$ times it is $\hat S(1 - \hat S)/N$. (b) Let $m$ be the true median. The interval misses $m$ below only if the upper band is at most 0.5 before $m$, at a $t < m$ where $S(t) > 0.5$: a one-sided normal-approximation event of probability at most about 0.1. It misses above only if the lower band is above 0.5 at $m$ itself, where $S(m) \le 0.5$: again about 0.1 at most. A grid with gaps of 15 or 20 games makes both events rarer. $\blacksquare$

The median is in checkpoint units: "45" means detected by the 45-game checkpoint, not at game 45 (interval-censored data read at the grid). `K4 simulation`: on 2,000 samples of 110 players with $S = 0.75, 0.55, 0.40, 0.30$ at the four checkpoints, the mean $\hat S$ is within 4 standard errors of the truth at each, Greenwood's standard error is within 5% of the spread, the median is 60 as often as $P(\hat S(45) > 0.5 \ge \hat S(60))$ predicts, and the interval holds 60 in 0.989 of samples (conservative, as (b) says). A second check uses random censoring before the end and recovers $S(50)$ of an exponential. *Target:* at most 45 games, a little more than half of the 80-game window (plan §2), reached today by the two always-found types at 30 games.

## K5. Comparing two runs

`cli bench compare A.json B.json` reports $\Delta = \theta(B) - \theta(A)$ for each metric of `kpi::values` and an 80% percentile interval of $B = 2000$ bootstrap differences. When both runs have the same players on the same seeds (`same_players`: names, seeds, flaws and checkpoints), each resample draws one set of player indices, stratified by flaw, and evaluates both runs on it (paired); otherwise each run is resampled on its own. The verdict reads only the interval: "higher" when it lies above 0, "lower" when below, "no clear difference" otherwise. Which way is better is a separate column.

**Result K5.** Under no change in expectation, the paired verdict reads "higher" with probability at most about 0.10, and likewise "lower". Pairing removes the between-player variance the two runs share: $\operatorname{Var}(\hat\theta_B - \hat\theta_A) = \operatorname{Var}\hat\theta_A + \operatorname{Var}\hat\theta_B - 2\operatorname{Cov}$, and on the same players the covariance is large (only the players whose outcome changed contribute to the paired difference).

*Proof sketch.* The difference of two smooth functions of stratified means is a smooth function of the paired means, so (K3 b) the percentile interval is first-order correct for $\Delta$, and an 80% two-sided interval excludes a true 0 on each side with probability about 0.1. $\blacksquare$

`K5 simulation`: 300 pairs of runs, 22 players for each of five types, B re-drawing 10% of the players from the same rate: "higher" 0.050 and "lower" 0.063, at most 0.10 as stated (the discrete differences make it conservative). The interval is for one metric; reading several at once needs Holm, as the arena does.

## Not a KPI: plan hit rate

`plan_hit_rate` is the share of flawed players whose top-1 weakness's drills include one for their flaw. Each flaw's expected drills are in its target key's catalogue, so a player ranked first on its flaw always hits: the plan hit rate is at least recall at 1, and it equals recall at 1 when no other key at rank 1 shares the flaw's drills. It measures the catalogue, not the engine, so it is reported but not a KPI. `every_rankable_key_has_a_drill` (in `results.rs`) holds the catalogue side: every key in `RANKABLE` has a drill other than `own_game_analysis`, and every flaw's expected drill is in its target's list.

# Findings

Places where the shipped code and its documented maths disagree. No product code was changed here.

1. **The README's posterior is the `algo-1.x` rule.** README, "A rate per chance is a Beta posterior", gives $\operatorname{Beta}(n_0\mu + k, n_0(1-\mu) + n - k)$ with $k, n$ the weighted counts and $n_0$ "the prior strength in chances" (W1 300 … W10 600). Since `algo-2.0.0`, production uses $\tilde k, \tilde n$ (the effective count over the dispersion) and $s_0 = \min(n_0, 2\tilde n/g)$ (A5). The difference is large. For `lab-hangs`' W1 (60 games), the prior strength is 96.2, not 300, and the data count is 2885 against a weighted 2152. The README's "ten chances can't move a W10 rate far from the peer rate ($n_0 = 600$)" is false under the shipped rule: with 10 chances over 30 games, $s_0 \le 2 \cdot 10/30 = 0.67$. The README's formula is right for the timeline's posterior (A9), but there the counts are unweighted.
2. **The weekly focus can set a target that is no step at all.** ADR 0091 and `chooseTarget`'s comment promise "the current rate less a meaningful step". Rounding to a tenth of a percentage point can remove the step for baselines at or below 0.5%. Without a peer, the target exceeds the baseline for $b \in [0.000667, 0.001)$ and a sliver just below 0.002, equals it at 0.001 and 0.002, and is 0, an impossible goal, below 0.000667: 335 of a grid of $10^6$ baselines, all at $b \le 0.002$. With a peer median just under $0.9b$, the peer target can round up to or past the baseline for $b \le 0.005$ ($b = 0.0029$, $\pi = 0.0026$ gives 0.003): 1,115 baselines of the grid admit such a peer, from 0.000556 to 0.005 (A13(b)). Rates that low are possible for the per-move keys (W1, W10, W11), whose chances are every eligible move.
3. **The trajectory module's comment is stale.** `backend/crates/profile/src/coaching/trajectory.rs`'s module comment says "then" is "the games around the peak, or the first games", and that the rating uses the web's bootstrap line for line. Under the production rule since `algo-6.0.0`, "then" is always the first $W$ games, and the rating's interval comes from `Memory` (A12). The `RatingRule` comment is correct.
4. **W5's benchmark is an upper bound, described as a point.** The README says a winning episode is one the evaluation "expects … to lose 0.15 of a point". It expects at most 0.15, since $W \ge 85$ (A7(c)). The code is consistent with its constant, but the excess it reports is a lower bound on the excess over the evaluation's own prediction. W5 flags exactly as the stated 0.15 rule implies, and less often than a rule using the evaluation's actual prediction would.
5. **A comment's example.** `goal.ts`'s `tenth` says the epsilon "keeps 92.55 from flooring to 92". `tenth` rounds a rate at its third decimal, and the epsilon only matters where floating point puts an exact half-thousandth just below the half: $0.5005 \times 1000 = 500.49999999999994$, which rounds to 0.500 without the epsilon and 0.501 with it. The arithmetic is right; the example is not.
6. **The README's trajectory summary is the `algo-5.0.0` behaviour.** README, "What changed since your peak", says "then" is "the games around the peak" and that the rating is shown "with no verdict". Since `algo-6.0.0` "then" is the first $W$ games and the rating row has its verdict and range (A12), as the README's own Trajectory section says.
7. **A field comment names the wrong detector.** `timeline::states::Signals::drift` is documented as an "EWMA drift alarm"; it is set by the relapse CUSUM (`ResolvedWatch`, A10).

# Verification

`math_check.py` in `rematch-lab:main` (its `cli` is this build, `algo-6.0.0+f40a2b27677a`, and `cli algo show --json` equals `manifest.json`): 112 checks passed and 0 failed, 18 of them the KPIs' (K0 to K5, ADR 0119). It exits 0.

**Blind re-derivation.** A fresh agent, given the code and the fixtures but not this file, re-derived A1, A3, A5 (with the `lab-hangs` W1 estimate), A6, A12 (with `lab-peaked`'s rating row), A13 and A14.

- It agreed on every statement and every number: $W(271) = 73.0634$, $\Delta W = 33.1413$, the accuracy 22.2002, $\tilde n = 2885.42$, $s_0 = 96.1807$, $\lambda = 2/62$, the golden estimate to $10^{-16}$ (its interval to $3 \times 10^{-11}$), $V$, the 0.10 false-alarm rate, the rating row to the golden digits, the focus targets' failure set, the luck rates 0.3758 and 0.4114, and the holdout share $0.200000000186$.
- Two figures were corrected in the draft check from the blind values. The accuracy's first breakpoint is 0.2237, not 0.2245. The non-overlap rate at 30 against 30 is 0.070, not 0.057.
- It independently found Findings 2, 3 and 5, and noted that under-dispersion is raised to $\varphi = 1$ (stated in A5).

**Hostile referee.** A second fresh agent reviewed this file and `math_check.py` against the code, the cited experiments and the Lean README. It confirmed A1, A3, A5(c, d, e), A9(c), A12's formulas against `Memory`, A14's numbers and the Part B summaries against the changelog. Its corrections, all made:

- A13(b) and Finding 2 were wrong: with a peer median the target can round up past the baseline up to $b = 0.005$, not only to 0.002, and the rounding epsilon is $10^{-12}$ in rate units, not $10^{-9}$. The check now tests the peer branch (1,115 baselines).
- Finding 5's example was wrong (0.92555 rounds the same either way); it is now 0.5005, which the check asserts.
- A11(b)'s proof relied on `active` needing confidence 0.7 at every game; the persistence rule keeps it at any confidence, so the proof now uses the evidence every segment starts with.
- Findings 6 (the README's trajectory summary) and 7 (the `drift` field comment) were missed and are added.
- Gaps stated: A6(b) needs $\varphi = 1$ and a data-free prior, and its simulation tests the textbook rule only; Lorden's bound in A10 needs binomial games at $p_0$; A8's rate rows use $\delta = 0$, only the timeline's $P_{\text{imp}}$ uses 0.15; the Bonferroni citation in `algo-3.0.0` is at $\alpha = 0.025$, $m = 30$.
- Nits fixed: A4(b) strengthened to $\mathrm{GA} \ge \min a_i$ and its check run on both colours; A12's window ends defined and (e) stated as two expressions; the 5.0.0 ratio quoted at $\hat\varphi = 0.9843$; W5's $\min(0.15/e, 1)$ and the `MAX_SHAPE` rescale stated; Finding 4's last sentence corrected; three rendering hazards removed (braces, `\_` in `\text`, `\|`). The quoted simulation values, $W(100) = 59.10$, $W(300) = 75.11$, the 5.0.0 ratio and `model_baseline`'s form are now asserted.
- Left as they are: the A8 simulation is an arithmetic bound, and the A6 frequentist check is a pointwise implication; both are said to be so.
