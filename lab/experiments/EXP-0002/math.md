# EXP-0002: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic checks (prefix `R`), simulations from a known truth (also `R`), and the recomputation of every quoted number (prefix `N`).

EXP-0002 is a measurement of one version. It feeds event counts drawn from known rates to the production posterior and scores its 80% interval. So the maths has to show four things: that the interval the experiment scores is the formula in section 3 (the replay in `N1` reproduces all 1,200 committed cells exactly from the registered seeds); why that formula under-covers when the true rate is far from the peers' (Results 3 to 7, which predict the 20 primary coverages to within 0.018 from a closed form); that the estimators, intervals and decision rule have the error control the pre-registration claims (Results 10 to 15); and which of the pre-registration's arguments in words hold (Results 4, 5, 13 and 16, and three Findings).

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $\kappa$ | a weakness key: W1, W4.missed, W7.traps, W9b, W5 | – | `analysis.py` `KEYS` |
| $B \in \{B1, B2, B3\}$, $f_B \in \{1.25, 1, 0.8\}$ | rating band and its odds factor on the peers' rate | – | `BANDS` |
| $\rho \in \{0.5, 1, 2, 4, 8\}$ | the true rate's odds multiple of the peers' rate; primary $\rho \in \{2, 4, 8\}$ | – | `RHOS`, `PRIMARY_RHOS` |
| $\operatorname{odds}(p) = p/(1-p)$, $\operatorname{sc}(p, f) = \operatorname{odds}^{-1}(f \operatorname{odds}(p))$ | odds scaling | – | `odds_scale` |
| $\mu$ | the prior mean: the peers' pooled rate $\operatorname{sc}(\mu_{B2}, f_B)$ ($\mu_{B2}$ = 0.004, 0.30, 0.10, 0.02); $\tfrac12$ for W5 | per chance | `peer_rate`, `stat` `pooled_rate`; `estimate_weakness_against_model` |
| $\nu$ | the band median used by `credible`: equal to $\mu$ for the peer keys, 0.15 for W5 | per chance | `stat` `median` |
| $n_0$ | the prior strength: 300 (W1), 40 (W4.missed, W7.traps, W9b), 2 (W5, flat) | chances | `profile_core::constants::n0`; `estimate_weakness_against_model` `(0.5, 2.0)` |
| $\theta = \operatorname{sc}(\mu_{\text{peer}}, \rho)$ | the true per-chance rate (the ground truth) | per chance | `true_rate` |
| $g \in \{30, 60, 120, 250\}$ | checkpoint, games in the window | games | `CHECKPOINTS` |
| $c_t$, $e_t$ | chances and events in game $t$ | chances, events | `chances`, `events` |
| $\bar c = E[c_t]$ | mean chances a game: 53, 1, 1, 10.6, 0.4 | chances | `KEYS[...]["chances"]` |
| $m_\kappa$ | mean chances of a game with chances, for the dispersion: 53 (W1), 31.8 (W9b) | chances | `per_game` |
| $D = 1.5$, $\gamma$ | the registered design effect, and the Beta concentration $\gamma = (m_\kappa - 1)/(D - 1) - 1$ (103, 60.6) | – | `DESIGN_EFFECT`, `events` `kappa` |
| $a_t$, $w_t = 2^{-a_t/60}$ | game $t$'s age at the checkpoint (days) and its recency weight | days, – | `PLAYED`, `weights::recency_weight` |
| $k = \sum_t w_t e_t$, $n = \sum_t w_t c_t$ | weighted events and chances (sums over the $g$ games) | events, chances | `weights::weighted_counts` |
| $\alpha = n_0\mu + k$, $\beta = n_0(1-\mu) + n - k$, $N = \alpha + \beta = n_0 + n$ | the posterior shapes | – | `posterior::posterior` |
| $F_{\alpha,\beta}$, $q_u$ | the Beta CDF and its $u$-quantile | – | `BetaDist` |
| $\hat m = \alpha/N$ | the posterior mean | per chance | `posterior.mean` |
| $I = [\min(q_{0.1}, \hat m), \max(q_{0.9}, \hat m)]$ | the production 80% interval | per chance | `posterior.interval` |
| $\lambda = n_0/(n_0 + n)$, $\hat p = k/n$ | the prior's share of the mean, and the weighted rate | – | – |
| $r = \sum w_t^2 c_t / \sum w_t c_t$ | the weight concentration ($\le 1$) | – | Result 4 |
| $V_{ij}$ | 1 if interval $j$ of player $i$ holds $\theta$ | – | `rows[].covered` |
| $s$ | a stratum: (band, $\rho$, dispersion, schedule), 200 players | – | `player_means` |
| $\hat C$ | a coverage: the mean over its $S$ strata of the mean over players of the player's share of covered intervals | share | `estimate` |
| $z_{20} = \Phi^{-1}(1 - 0.05/40) = 3.0233$, $z_{21} = \Phi^{-1}(1 - 0.05/42) = 3.0381$, $z = \Phi^{-1}(0.9) = 1.2816$ | Bonferroni and the 80% normal quantiles | – | `z_prim`, `z_bonf` |

Schedules (`PLAYED`): with $j = 0, \dots, g-1$ the number of games after a given game, "spread" has $a = j/2 + 1/24$ (12 hours apart, the clock one hour after the last game), so $w_j = q_0 \varrho^j$ with $q_0 = 2^{-1/1440}$ and $\varrho = 2^{-1/120}$. "Compressed" has $a = j/250 + 1/24$, so $w_j \ge 2^{-(249/250 + 1/24)/60} = 0.98808$.

## 2. Model

**(M1) Chances.** For each player, key and game, independently: W1 $c \sim \operatorname{Poisson}(53)$; W4.missed $\operatorname{Poisson}(1)$; W7.traps $c = 1$; W9b $c = 0$ with probability $2/3$, else $\operatorname{Poisson}(31.8)$; W5 $c \sim \operatorname{Bernoulli}(0.4)$.

**(M2) Events.** Given $c_t$: with no dispersion, $e_t \sim \operatorname{Binomial}(c_t, \theta)$. With `de1.5`, for W1 and W9b only, $\vartheta_t \sim \operatorname{Beta}(\theta\gamma, (1-\theta)\gamma)$ and $e_t \mid \vartheta_t \sim \operatorname{Binomial}(c_t, \vartheta_t)$; the other keys are drawn as without dispersion. Games and players are independent. $\theta$ is fixed: it is not drawn from the prior, and it does not drift.

**(M3) The estimate** (`compute_estimates` → `estimate_weakness` or, for W5, `estimate_weakness_against_model`, both in `backend/crates/profile-core/src/stats/priority.rs` and `backend/crates/profile/src/estimates.rs` at the run's commit 7409f457). Every game is inside the 183-day window. The posterior is `posterior(k, n, mu, n0)`: $\operatorname{Beta}(\alpha, \beta)$ with the interval $I$ of section 1 (`posterior.rs`, `INTERVAL_LEVEL = 0.8`). An estimate is *credible* when it has an event, $\min I > \nu$, and $\max(0, \hat m - \nu)\,(\sum_t c_t/g)\,\epsilon \ge 0.02$ with $\epsilon = 0.3$ (peer rows) or 1 (W5).

**(M4) Scoring.** $V = \mathbf 1\{\min I \le \theta \le \max I\}$. The four checkpoints of a player are prefixes of one series of 250 games, scored on both schedules from the same draws.

## 3. Estimators

- **(E1) The posterior** (`posterior::posterior`): $\alpha = n_0\mu + k$, $\beta = n_0(1-\mu) + n - k$; $\hat m = \alpha/(\alpha+\beta)$; $I = [\min(q_{0.1}, \hat m), \max(q_{0.9}, \hat m)]$. The floor ($10^{-3}$ for a zero shape) and the cap ($\alpha + \beta \le 10^7$) are part of the code and never bind here (Result 2).
- **(E2) A player's share** (`player_means`): $\bar V_i = \frac{1}{L_i}\sum_{j} V_{ij}$ over the $L_i$ intervals in scope (1 for a primary cell, 4 for a key margin, 20 for a band, $\rho$ or pooled margin).
- **(E3) A coverage** (`estimate`): $\hat C = \frac1S \sum_{s=1}^S \frac{1}{n_s}\sum_{i \in s} \bar V_i$, with $n_s = 200$; $S = 18$ for a primary cell (3 bands × 3 $\rho$ × 2 dispersions).
- **(E4) The stratified cluster bootstrap** (`stratified_boot`): $B = 2000$ resamples; in each, every stratum's players are drawn with replacement, $n_s$ of them, and (E3) is recomputed. $\widehat{\operatorname{se}}$ is the resamples' standard deviation (divisor $B - 1$); the 95% interval is their 2.5% and 97.5% quantiles; the Bonferroni interval is $\hat C \pm z_{20}\widehat{\operatorname{se}}$ (primary) or $\hat C \pm z_{21}\widehat{\operatorname{se}}$ (margins).
- **(E5) Classification** (`classify`): "within the band" if the Bonferroni interval lies in $[0.75, 0.85]$; "outside" if it lies wholly below 0.75 or wholly above 0.85; else "need more data". The decision is "outside" if any of the 20 is outside, "within" if all 20 are within.
- **(E6) The margins' tests** (`main`, `holm`): $Z = (\hat C - 0.8)/\widehat{\operatorname{se}}$, $p = 2(1 - \Phi(|Z|))$, Holm's step-down adjusted p values $\tilde p_{(i)} = \max_{l \le i}\min(1, (21 - l + 1)\,p_{(l)})$.
- **(E7) Wilson 95%** (`wilson95`): for $x$ of $n$, $\hat p = x/n$, $\big(\hat p + \frac{z^2}{2n} \pm z\sqrt{\frac{\hat p(1-\hat p)}{n} + \frac{z^2}{4n^2}}\big)/(1 + \frac{z^2}{n})$, $z = 1.959964$; per exploratory cell, per #13 row, and over the credible intervals.
- **(E8) Calibration** (`calibration`): the share of intervals with $|F_{\alpha,\beta}(\theta) - \tfrac12| \le q/2$, for $q$ in the grid.
- **(E9) Bias** (`mean_ratio`): the mean over a cell's players of $\hat m/\theta$.

## 4. Derivations and proofs

**Result 1 (the posterior).** With unit weights ($w_t = 1$, so $k$ and $n$ are integers), the prior $\operatorname{Beta}(n_0\mu, n_0(1-\mu))$ and the binomial likelihood give the posterior $\operatorname{Beta}(n_0\mu + k, n_0(1-\mu) + n - k)$, with mean $(n_0\mu + k)/(n_0 + n)$. With weights, (E1) is the normalised product of the prior and the *tempered* likelihood $\prod_t (\theta^{e_t}(1-\theta)^{c_t - e_t})^{w_t}$: a generalised-Bayes posterior, not the posterior of any model of the data. For W5, $(\mu, n_0) = (\tfrac12, 2)$ is the flat $\operatorname{Beta}(1, 1)$.

*Proof.* Under (M2) without dispersion, given $c$, $\prod_t \binom{c_t}{e_t}\theta^{e_t}(1-\theta)^{c_t - e_t} \propto \theta^{\sum e_t}(1-\theta)^{\sum(c_t - e_t)}$ in $\theta$. Times the prior kernel $\theta^{n_0\mu - 1}(1-\theta)^{n_0(1-\mu) - 1}$, exponents add: $\theta^{\alpha - 1}(1-\theta)^{\beta - 1}$, the $\operatorname{Beta}(\alpha, \beta)$ kernel. Both sides are densities, so the constants agree. The mean of $\operatorname{Beta}(\alpha, \beta)$ is $B(\alpha + 1, \beta)/B(\alpha, \beta) = \Gamma(\alpha+1)\Gamma(\alpha+\beta)/(\Gamma(\alpha)\Gamma(\alpha+\beta+1)) = \alpha/(\alpha+\beta)$ by $\Gamma(x+1) = x\Gamma(x)$. With weights, $(\theta^{e_t}(1-\theta)^{c_t-e_t})^{w_t} = \theta^{w_te_t}(1-\theta)^{w_t(c_t-e_t)}$, so the product has exponents $k$ and $n - k$ and the same algebra gives (E1) for real $k, n$. No binomial model has the likelihood $\theta^k(1-\theta)^{n-k}$ with fractional $k$, so (E1) is then a rule, and its calibration is not guaranteed by Bayes' theorem (Result 8 needs unit weights). $\blacksquare$

Machine-checked: `lab/lean/Rematch/BetaBinomial.lean` theorem `Rematch.beta_binomial_posterior`, for the unit-weight case: its hypotheses are $a = n_0\mu > 0$, $b = n_0(1-\mu) > 0$ and natural $k \le n$, which hold with $\mu \in (0, 1)$, $n_0 > 0$. It does not cover the weighted case (real $k$, $n$) or the dispersed model, where the binomial likelihood is not the data's. Checked by R1 (the kernels with real $k, n$, the tempered product, the mean).

**Result 2 (the interval is the central interval here, and coverage is the PIT).** (a) In this experiment $\alpha \ge 0.64$ and $\beta \ge 1$, so neither the shape floor nor the cap binds. (b) If $F_{\alpha,\beta}(\hat m) \in [0.1, 0.9]$ then $I = [q_{0.1}, q_{0.9}]$, and then $V = 1 \iff 0.1 \le F_{\alpha,\beta}(\theta) \le 0.9 \iff |F_{\alpha,\beta}(\theta) - \tfrac12| \le 0.4$. (c) With equal players per stratum and equal intervals per player, (E3) equals the plain share of covered intervals, so the pooled primary coverage equals the calibration curve's 0.8 point.

*Proof.* (a) $\alpha \ge n_0\mu$ and $\beta \ge n_0(1-\mu)$ since $0 \le k \le n$; the smallest prior part is $n_0\mu = 40 \times 0.0161 = 0.64$ (W9b at B3) in the grid and $300 \times 0.0008 = 0.24$ in the #13 cell (where every replayed $\alpha$ is at least 18.9); for W5, $\alpha, \beta \ge 1$. Both are positive, so the floor (applied only to a shape exactly 0) never applies, and $N \le 300 + 250 \cdot \max c$ is far below $10^7$. (b) $F$ is continuous and strictly increasing on $(0, 1)$, so $\hat m \ge q_{0.1} \iff F(\hat m) \ge 0.1$ and $\hat m \le q_{0.9} \iff F(\hat m) \le 0.9$; then the min and max in $I$ return the quantiles. And $q_{0.1} \le \theta \le q_{0.9} \iff 0.1 \le F(\theta) \le 0.9$ by the same monotonicity. (c) With $n_s = n$ and $L_i = L$ for every stratum and player, $\frac1S\sum_s\frac1n\sum_i \frac1L\sum_j V_{ij} = \frac{1}{SnL}\sum_{s,i,j} V_{ij}$. $\blacksquare$

The premise of (b) is checked two ways. On a grid of $260^2$ shapes from 0.2 to $2\times10^5$, $F(\hat m)$ lies in $[0.236, 0.764]$ (R2; no symbolic proof, a grid). And the replay finds no interval widened among all 241,600 posteriors of the run, the #13 cell included (N1). The replay matches the committed cells' counts and means, not each interval one by one; that is strong evidence, not proof, that the Rust interval is (E1) on every draw. The widening matters only for tiny shapes: $\operatorname{Beta}(0.02, 40)$ has $q_{0.9} < \hat m$ (R2). (c) is why `calibration_primary` at 0.8 and `primary.pooled_coverage` are both 0.571625, and each $\rho$ margin equals its calibration curve at 0.8 (N3, N6).

**Result 3 (the bias of the posterior mean).** $\hat m = \lambda\mu + (1 - \lambda)\hat p$. Under (M1)–(M2), with any weights and either dispersion, $E[\hat m \mid c] - \theta = \lambda(\mu - \theta)$, and

```math
\frac{E[\hat m]}{\theta} = 1 - E[\lambda]\Big(1 - \frac{\mu}{\theta}\Big).
```

*Proof.* $\hat m = (n_0\mu + k)/(n_0 + n) = \frac{n_0}{n_0+n}\mu + \frac{n}{n_0+n}\cdot\frac{k}{n}$. Given the chances $c$, $n$ and $\lambda$ are fixed, and $E[e_t \mid c] = c_t\theta$ in both dispersions (with `de1.5`, $E[e_t \mid c] = c_t E[\vartheta_t] = c_t\theta$, as $E[\vartheta_t] = \theta\gamma/\gamma$). So $E[k \mid c] = \theta n$ and $E[\hat m \mid c] = (n_0\mu + \theta n)/(n_0 + n) = \lambda\mu + (1-\lambda)\theta$. Subtract $\theta$; then take the expectation over $c$ and divide by $\theta$. $\blacksquare$

For $\theta > \mu$ (every primary cell of the four peer keys) the mean is pulled down by the share $\lambda$ of the gap. W5's $\theta$ (0.26 to 0.59 at B2) lies on both sides of its $\mu = \tfrac12$, and its $\lambda$ is small, so its pull is small either way. The plug-in $\lambda$ at $n = \bar c\sum w_t$ on the spread schedule:

| Key | $\bar c$ | $n_0$ | $\lambda$ at 30 | 60 | 120 | 250 |
|---|---|---|---|---|---|---|
| W1 | 53 | 300 | 0.170 | 0.100 | 0.061 | 0.041 |
| W4.missed, W7.traps | 1 | 40 | 0.592 | 0.440 | 0.316 | 0.232 |
| W9b | 10.6 | 40 | 0.120 | 0.069 | 0.042 | 0.028 |
| W5 | 0.4 | 2 | 0.153 | 0.090 | 0.054 | 0.036 |

This is the quantitative form of the decision's inferred mechanism: what sets the pull is $n_0$ against the chances the data bring ($\bar c \sum w_t$), not $n_0$ alone. The pre-registration expected W1 (largest $n_0$) to be pulled most. By $\lambda$ it is pulled about a quarter as much as W4.missed and W7.traps, whose prior is worth 40 chances against about one chance a game. Checked by R3 (symbolic; a weighted, dispersed simulation to 4 SE), and N5 and N2: for B2 W4.missed at $\rho = 4$ ($\theta = 0.632$), $1 - 0.592(1 - 0.3/0.632) = 0.69$ and at 250 games 0.88, as quoted. For the #13 cell, $E[\lambda] = 0.170$ from the replayed chances gives 0.836 against the sample's 0.828 (within 4 SE).

**Result 4 (the recency weights).** (a) Weighting can only raise the prior's share: $\lambda_w = n_0/(n_0 + \sum w_tc_t) \ge n_0/(n_0 + \sum c_t)$. (b) On the spread schedule, $\sum_{j<g} w_j = q_0(1 - \varrho^g)/(1 - \varrho)$ and $r_g = \sum w_j^2/\sum w_j = q_0(1 + \varrho^g)/(1 + \varrho)$ for equal chances. (c) At two games a day the data never weigh more than $q_0/(1 - \varrho) = 173.5$ games' worth, and inside the 183-day window (at most 366 games) never more than 152.6. So the prior's *expected* share never falls below $n_0/(n_0 + 152.6\,\bar c)$; for W4.missed and W7.traps, 0.208, and the expected W4.missed mean at B2, $\rho = 4$ stays below 0.89 of the truth however long the history. (One player's $\lambda$ can be lower: with 366 Poisson(1) games it is below 0.208 about half the time.) (d) $r_g \to q_0/(1+\varrho) = 0.501$.

*Proof.* (a) $0 < w_t \le 1$ gives $\sum w_tc_t \le \sum c_t$, and $\lambda$ falls in $n$ ($\partial\lambda/\partial n = -n_0/(n_0+n)^2 < 0$). (b) $a_j/60 = j/120 + 1/1440$, so $w_j = q_0\varrho^j$; a geometric series gives $\sum w_j = q_0(1-\varrho^g)/(1-\varrho)$ and $\sum w_j^2 = q_0^2(1-\varrho^{2g})/(1-\varrho^2)$. Divide, using $1 - \varrho^{2g} = (1-\varrho^g)(1+\varrho^g)$ and $1 - \varrho^2 = (1-\varrho)(1+\varrho)$. (c) $\varrho^g \to 0$ gives the limit; the window admits $a_j \le 183$, that is $j \le 365$, 366 games, and $q_0(1 - \varrho^{366})/(1-\varrho) = 152.6$, so $E[n] \le 152.6\,\bar c$. $\lambda(n) = n_0/(n_0 + n)$ is convex in $n$ ($\partial^2\lambda/\partial n^2 = 2n_0/(n_0+n)^3 > 0$), so by Jensen $E[\lambda] \ge \lambda(E[n]) \ge n_0/(n_0 + 152.6\,\bar c)$. With Result 3, $E[\hat m]/\theta \le 1 - 0.208(1 - 0.3/0.632) = 0.891$. (d) Same limit. $\blacksquare$

Values: $\sum w$ is 27.6, 50.8, 86.8, 132.6 at the four checkpoints (against 30, 60, 120, 250), and $r_g$ = 0.923, 0.856, 0.752, 0.620. On the compressed schedule $r_g \ge 0.994$. A prior of 40 chances weighs as much as the first 46 games of W4.missed on the spread schedule ($\sum_{j<45} w_j = 39.7$, $\sum_{j<46} w_j = 40.5$; `decision.md`'s "first 40 games" is the unweighted count). This turns the pre-registration's rival 1 ("it shrinks with more games") into a bound: on the spread schedule the pull shrinks but has a floor. Checked by R4 (closed forms, limits, the window).

**Result 5 (the variance of the weighted rate, and the design effect).** Given $c$,

```math
\operatorname{Var}[\hat p \mid c] = \frac{\theta(1-\theta)}{n^2}\sum_t w_t^2 c_t d_t, \qquad d_t = 1 + \frac{c_t - 1}{\gamma + 1},
```

with $d_t = 1$ without dispersion. The registered $\gamma = (m - 1)/(D - 1) - 1$ gives $d = D$ at $c_t = m$. With Poisson($m$) chances (W1; W9b given that the game reaches the low clock), the pooled design effect $E[c\,d]/E[c]$ is $1 + (D-1)m/(m-1)$: 1.510 for W1 and 1.516 for W9b, not exactly 1.5.

*Proof.* Games are independent given $c$, so $\operatorname{Var}[k \mid c] = \sum_t w_t^2\operatorname{Var}[e_t \mid c_t]$. By the law of total variance over $\vartheta_t$: $\operatorname{Var}[e_t] = E[c_t\vartheta_t(1 - \vartheta_t)] + \operatorname{Var}[c_t\vartheta_t] = c_t(\theta - E\vartheta_t^2) + c_t^2\operatorname{Var}\vartheta_t$. For $\operatorname{Beta}(\theta\gamma, (1-\theta)\gamma)$, $\operatorname{Var}\vartheta_t = \theta(1-\theta)/(\gamma + 1)$ and $E\vartheta_t^2 = \operatorname{Var}\vartheta_t + \theta^2$, so $\operatorname{Var}[e_t] = c_t\theta(1-\theta) - c_t\theta(1-\theta)/(\gamma+1) + c_t^2\theta(1-\theta)/(\gamma+1) = c_t\theta(1-\theta)(1 + (c_t-1)/(\gamma+1))$. Divide by $n^2$. With $\gamma + 1 = (m-1)/(D-1)$, $d = 1 + (D-1)(c-1)/(m-1)$, which is $D$ at $c = m$. For $c \sim \operatorname{Poisson}(m)$, $E[c(c-1)] = m^2$, so $E[c\,d]/E[c] = 1 + m^2/(m(\gamma+1)) = 1 + (D-1)m/(m-1)$; the zero-inflation of W9b multiplies numerator and denominator by $1/3$. $\blacksquare$

The registered "design effect of 1.5" is therefore 1.51 and 1.52: about 2% to 3% more extra variance than stated, which moves no conclusion. The design-effect formula of the review ("$1 + (E[m^2]/E[m] - 1)\rho_{icc}$, about $m+1$ rather than $m$") is this result with $\rho_{icc} = 1/(\gamma+1)$. Checked by R5 (symbolic; a fixed-$c$ simulation of the weighted variance; the Poisson design effect).

**Result 6 (what the posterior claims against what the data do).** The posterior variance is $\hat m(1-\hat m)/(N+1)$. To first order, at $\hat m \approx \theta$ and with $\sum w_t^2c_td_t = r\bar D n$ ($\bar D = \sum w_t^2c_td_t/\sum w_t^2c_t$, the $w^2c$-weighted mean of $d_t$, and $r = \sum w_t^2c_t/\sum w_tc_t$), the ratio of the sampling variance of $\hat m$ to the posterior variance is

```math
\frac{\operatorname{Var}[\hat m \mid c]}{\hat m(1-\hat m)/(N+1)} \approx r\,\bar D\,(1 - \lambda).
```

*Proof.* For a Beta, $\alpha\beta/(N^2(N+1)) = \hat m(1-\hat m)/(N+1)$. By Result 3, $\operatorname{Var}[\hat m \mid c] = (1-\lambda)^2\operatorname{Var}[\hat p \mid c] = \theta(1-\theta)\sum w^2cd/(n_0+n)^2$ by Result 5, since $(1 - \lambda)/n = 1/(n_0 + n)$. Replacing $\hat m(1-\hat m)$ by $\theta(1-\theta)$ and $N + 1$ by $N = n_0 + n$ (the approximations), the ratio is $\sum w^2cd/(n_0 + n) = r\bar D n/(n_0+n) = r\bar D(1-\lambda)$. $\blacksquare$

The three factors are the three rivals of section 2: the weights ($r < 1$) make the interval too wide; dispersion ($\bar D > 1$) makes it too narrow; the prior ($1 - \lambda < 1$) makes it too wide around a mean that is biased (Result 3). The approximation drops $O(1/N)$ and the difference between $\hat m(1-\hat m)$ and $\theta(1-\theta)$; Result 7 measures its total error. Checked by R6.

**Result 7 (the coverage, approximately).** Let $\bar m = \lambda\mu + (1-\lambda)\theta$, $s = \sqrt{\bar m(1-\bar m)/(n_0+n)}$, $\sigma = \sqrt{\theta(1-\theta)\sum w^2c\,d}/(n_0+n)$ and $b = \lambda(\mu - \theta)$, at the expected counts ($n = \bar c\sum w$, $\sum w^2 c\,d = \sum w^2\,E[c\,d]$). Then

```math
C \approx \Phi\Big(\frac{z s - b}{\sigma}\Big) - \Phi\Big(\frac{-z s - b}{\sigma}\Big), \qquad z = 1.2816.
```

In particular, at $\theta = \mu$ ($b = 0$), $C \approx 2\Phi\big(z/\sqrt{r\bar D(1-\lambda)}\big) - 1$, which exceeds 0.8 exactly when $r\bar D(1-\lambda) < 1$; and with a negligible prior and bias, $C \approx 2\Phi(z/\sqrt{r\bar D}) - 1$ (a heuristic for W5, where $\lambda \approx 0.04$ at 250 games but $b \ne 0$). On the registered grid the formula is within 0.018 of every one of the 20 primary coverages and within 0.038 of every $\rho = 1$ key × checkpoint coverage.

*Proof (of the formula, given its approximations).* Approximate the posterior by $N(\hat m, s^2)$, so $I \approx \hat m \pm zs$, and $\hat m$ by $N(\theta + b, \sigma^2)$, its mean and variance from Results 3 and 5. Then $V = 1 \iff -zs \le \hat m - \theta \le zs$, which has probability $\Phi((zs - b)/\sigma) - \Phi((-zs - b)/\sigma)$. At $b = 0$ this is $2\Phi(zs/\sigma) - 1$ with $s^2/\sigma^2 = 1/(r\bar D(1-\lambda))$ (Result 6). $x \mapsto 2\Phi(x) - 1$ is strictly increasing and equals 0.8 at $x = z$, so $C > 0.8 \iff s/\sigma > 1$. $\blacksquare$

The approximations (normal posterior, normal $\hat m$, expected counts in place of random ones, $s$ at $\bar m$) are not bounded analytically. Their total error is measured instead: R7 compares the formula with the replayed production coverages. The formula gives 0.653, 0.715, 0.768, 0.820 for W1 against 0.636, 0.709, 0.763, 0.802; 0.098 to 0.266 for W4.missed against 0.100 to 0.260; and 0.902 for W5 at 250 games against 0.904. So the primary is explained by two numbers per cell: the standardised bias $b/\sigma$, large for W4.missed and W7.traps because $\lambda$ is large, and the width ratio $s/\sigma$. W5 at 250 games, by the heuristic: $r = 0.620$, $2\Phi(1.2816/\sqrt{0.620}) - 1 = 0.897$, against 0.904; on the compressed schedule ($r \approx 1$) 0.80 against 0.805. That is the decision's "the weighting widens the intervals", in a formula. Checked by R7.

**Result 8 (the posterior is calibrated on average over its prior).** With unit weights, no dispersion and $\theta \sim \operatorname{Beta}(n_0\mu, n_0(1-\mu))$ drawn afresh for each player, $P(q_{0.1} \le \theta \le q_{0.9}) = 0.8$ exactly, whatever $n$; and $P(V = 1) = 0.8$ when every reachable posterior has $F_{\alpha,\beta}(\hat m) \in [0.1, 0.9]$, which the grid of Result 2 shows for shapes of at least 0.2.

*Proof.* By Result 1, given the data, $\theta$ has the posterior $\operatorname{Beta}(\alpha, \beta)$, so $P(q_{0.1} \le \theta \le q_{0.9} \mid \text{data}) = 0.8$ (continuity of $F$). Average over the data (tower property). Under that shape condition, Result 2(b) says the widening does not change the event. $\blacksquare$

So a miss in EXP-0002 is not a fault of the Beta arithmetic. It comes from fixing $\theta$ at 2 to 8 times the prior's odds, where the prior puts little mass, and from the weights and dispersion breaking Result 1's premise. Coverage is a frequentist property at a fixed $\theta$; the posterior promises 0.8 only on average over $\theta$ drawn from the prior. Checked by R8 (200,000 draws each, a W4-like and a W1-like prior; 0.7999 and 0.7995).

**Result 9 (exact coverage, unit weights).** With $w_t = 1$ and $c_t = 1$ (W7.traps), $C(\theta) = \sum_{x=0}^{g}\binom gx\theta^x(1-\theta)^{g-x}\,\mathbf 1\{\theta \in I(x)\}$, a step function of $\theta$ (the discreteness rival).

*Proof.* $k = x$ is $\operatorname{Binomial}(g, \theta)$ and $I$ is a function of $x$ alone; sum over $x$. $\blacksquare$

For W7.traps at $\mu = 0.1$, $\rho = 4$, 30 games, $C = 0.100$. Checked by R9 against simulation.

**Result 10 (the variance bound behind section 6).** If $X \in [0, 1]$ has mean $p$, $\operatorname{Var}X \le p(1-p)$ (Bhatia–Davis). For (E3) over $S$ strata of $n$ independent players each, $\operatorname{Var}\hat C = \frac{1}{S^2}\sum_s\frac{\sigma_s^2}{n} \le \frac{C(1-C)}{Sn}$, with $C$ the target coverage.

*Proof.* $X^2 \le X$ on $[0, 1]$, so $E X^2 \le p$ and $\operatorname{Var}X \le p - p^2$. The strata are independent and players within one are i.i.d., so the variance of the mean of stratum means is $\frac{1}{S^2}\sum_s \sigma_s^2/n$. Apply the bound to each stratum, $\sigma_s^2 \le C_s(1 - C_s)$, and then Jensen's inequality for the concave $x(1-x)$: $\frac1S\sum_s C_s(1-C_s) \le C(1-C)$ with $C = \frac1S\sum_s C_s$. $\blacksquare$

At $C = 0.8$ and $Sn = 3{,}600$, the standard error is at most $\sqrt{0.16/3600} = 0.0067$ and the Bonferroni half-width at most $3.02 \times 0.0067 = 0.020$, as section 6 says. For a $\rho$ margin ($Sn = 1{,}200$), $3.038\sqrt{0.16/1200} = 0.035$. With 100 players a cell, $3.023\sqrt{0.16/1800} = 0.0285$, quoted 0.029. Checked by R10 and N7.

**Result 11 (the stratified bootstrap).** Conditional on the data, the variance of the bootstrap replicate of (E3) is exactly $\frac{1}{S^2}\sum_s \hat\sigma_s^2/n_s$, with $\hat\sigma_s^2$ the stratum's player variance with divisor $n_s$. So $\widehat{\operatorname{se}}$ estimates the plug-in standard error, which is low by the factor $\sqrt{(n_s - 1)/n_s} = 0.9975$, and carries Monte Carlo error of relative size about $1/\sqrt{2(B-1)} = 1.6\%$.

*Proof.* Within a stratum the $n_s$ resampled players are i.i.d. draws from the empirical distribution, whose variance is $\hat\sigma_s^2$, so their mean has variance $\hat\sigma_s^2/n_s$; strata are resampled independently, and the replicate is $\frac1S$ times the sum of the stratum means. The unbiased variance has divisor $n_s - 1$, hence the factor. The standard deviation of $B$ normal draws has relative standard error about $1/\sqrt{2(B-1)}$. $\blacksquare$

For a primary cell each player gives one interval, so $\hat\sigma_s^2 = \hat C_s(1 - \hat C_s)$ is known from the committed cell coverage. N3 recomputes the 20 standard errors from the cells alone and finds them within 3.9% of the bootstrap's (4 MC SE is 6.3%). N4 does the same for the 21 margins from the replayed player shares. Checked by R11 (simulated strata) and N3, N4.

**Result 12 (the error control of the decision rule).** If each of the 20 Bonferroni intervals holds its true coverage with probability at least $1 - 0.05/20$, then with probability at least 0.95 every one does. On that event the classification is never wrong in direction: no cell whose true coverage is in $[0.75, 0.85]$ is called "outside", and no cell outside the band is called "within". So the decision "outside the band" is false with probability at most 0.05, and so is "within the band".

*Proof.* By the union bound, $P(\text{some interval misses}) \le \sum_{i=1}^{20} 0.05/20 = 0.05$. If interval $i$ holds $C_i$ and is wholly below 0.75 (or above 0.85), then $C_i < 0.75$ (or $> 0.85$), so $C_i$ is outside the band. If it lies in $[0.75, 0.85]$, then so does $C_i$. A false "outside" decision needs a cell called outside whose $C_i$ is inside, a false "within" a cell called within whose $C_i$ is outside; either needs a miss. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.bonferroni`, with $A_i$ the event that interval $i$ misses, $m = 20$, $I_0$ the whole family. Its hypothesis $P(A_i) \le \alpha/m$ holds here only approximately: each interval is $\hat C \pm z_{20}\widehat{\operatorname{se}}$, a normal interval, whose miss rate is $0.05/20$ in the limit. With 3,600 players a cell and $\hat C$ a mean of bounded variables the normal limit is close (Berry–Esseen gives an error of order $1/\sqrt{3600}$ in each tail, which is not small against $0.00125$ at the extreme tail, so the 0.95 is approximate). The decision is far from the edge: the low outside cells' upper bounds are at most 0.731 (W1 at 60 games and W9b at 30), and every outside cell's estimate lies 5.5 to 154 standard errors beyond the band's edge (N3). Checked by R12 (normal model at the band edges: $P(\text{any miss}) = 0.0486$).

**Result 13 (the power of the classification).** For a true coverage $C$ with $\operatorname{se} = \sqrt{v/N}$ and $h = z_{20}\operatorname{se}$, in the normal limit,

```math
P(\text{within}) = \Phi\Big(\frac{0.85 - h - C}{\operatorname{se}}\Big) - \Phi\Big(\frac{0.75 + h - C}{\operatorname{se}}\Big), \quad
P(\text{outside}) = \Phi\Big(\frac{0.75 - h - C}{\operatorname{se}}\Big) + 1 - \Phi\Big(\frac{0.85 + h - C}{\operatorname{se}}\Big).
```

*Proof.* Within $\iff \hat C - h \ge 0.75$ and $\hat C + h \le 0.85 \iff \hat C \in [0.75 + h, 0.85 - h]$; outside $\iff \hat C < 0.75 - h$ or $\hat C > 0.85 + h$; and $\hat C \sim N(C, \operatorname{se}^2)$. $\blacksquare$

With the pre-registration's $v = 0.16$ and $N = 3{,}600$: $P(\text{within} \mid 0.80) = 0.99999$; at 0.78 and 0.82, 0.930; at 0.72 and 0.88, $P(\text{outside}) = 0.930$. Those are the section 6 claims. Two of them do not survive scrutiny (Findings 1 and 2). Checked by R13 (formula against simulation) and N7.

> **Finding 1 (section 6, power at 0.72).** Section 6 says a coverage at 0.72 "or further out lands outside with probability above 0.9". That uses the player variance of a coverage at 0.80 (0.16). At 0.72 the worst-case variance (Result 10) is $0.72 \times 0.28 = 0.2016$, and then $P(\text{outside}) = 0.838$. The claim holds at 0.88 (0.994) and at 0.72 only when the strata differ enough to bring the player variance below about 0.175. In a primary cell each player gives one interval, so a stratum's player variance is $\hat C_s(1-\hat C_s)$, which the replayed strata take up to 0.25; the worst case is not hypothetical. It changes nothing in the decision, whose outside cells are at least 5.5 standard errors beyond the edge.

> **Finding 2 (section 6, sensitivity).** Section 6 says that with 100 players a cell "a true 0.78 would then pass only about two times in three". By Result 13 with $v = 0.16$ and $N = 1{,}800$ it passes with probability 0.563, and with $v = 0.78 \times 0.22$, 0.52: about one time in two. The conclusion it supported (200 a cell, not 100) is stronger, not weaker.

**Result 14 (Holm over the 21 margins).** If the 21 p values of the true nulls are valid ($P(p_i \le t) \le t$), Holm's procedure rejects at least one true null with probability at most 0.05, under any dependence. The adjusted p values of (E6) satisfy $\tilde p_i \le \alpha \iff$ Holm rejects $H_i$.

*Proof.* Order the p values $p_{(1)} \le \dots \le p_{(21)}$ and let $m_0$ be the number of true nulls. Suppose Holm rejects some true null, and let $j$ be the first true null in the order. All of $p_{(1)}, \dots, p_{(j-1)}$ are rejected, and at most $21 - m_0$ of them are false nulls, so $j \le 21 - m_0 + 1$ and $p_{(j)} \le \alpha/(21 - j + 1) \le \alpha/m_0$. So the event is inside $\bigcup_{i \in I_0}\{p_i \le \alpha/m_0\}$, which has probability at most $m_0 \cdot \alpha/m_0 = \alpha$. For the adjusted p values: $\tilde p_{(i)} \le \alpha$ iff $(21 - l + 1)p_{(l)} \le \alpha$ for every $l \le i$, which is Holm's step-down condition for $H_{(i)}$. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.holm_fwer`, with `holmRejects` the unsorted form of the step-down rule. R14 checks that `analysis.py`'s adjusted p values agree with `holmRejects` on 3,000 random vectors with ties. The Lean hypothesis (valid p values) holds here only approximately: each $p$ is a two-sided normal test on the bootstrap standard error. As the pre-registration says, the p values decide nothing: every one is below $10^{-9}$ before rounding (the closest margin, W9b, has $|Z| = 6.5$), and with this many players every visible difference from 0.80 rejects. Checked by R14 (equivalence; FWER 0.027 for 21 correlated true nulls).

**Result 15 (Wilson).** (E7) is the set of $p$ with $(\hat p - p)^2 \le z^2p(1-p)/n$, and it has coverage tending to 0.95 for independent intervals. Over the credible intervals it is too narrow, because a player gives up to four and $\operatorname{Var}\big(\frac1n\sum_i S_i\big) = \frac{1}{n^2}\sum_i\operatorname{Var}S_i$ with $S_i$ a player's covered count, which exceeds the binomial variance when a player's intervals are positively correlated.

*Proof.* The inequality is a quadratic in $p$, $(1 + z^2/n)p^2 - (2\hat p + z^2/n)p + \hat p^2 \le 0$, whose roots are the bounds of (E7) (R15 substitutes them). Under independence, $P(\hat p\text{'s score statistic} \le z) \to 0.95$ by the central limit theorem. For clusters, $\operatorname{Var}S_i = \sum_j\operatorname{Var}V_{ij} + \sum_{j \ne j'}\operatorname{Cov}(V_{ij}, V_{ij'})$, larger than the binomial part when the covariances are positive. $\blacksquare$

Every exploratory cell and every #13 row has one interval a player, so their Wilson intervals are the right ones; only the credible secondary has the clustering. N1 and N2 recompute all 1,208 Wilson intervals.

**Result 16 (the true rates and bands).** $\operatorname{odds}(\operatorname{sc}(p, f)) = f\operatorname{odds}(p)$, so every $\theta \in (0, 1)$; the largest is W4.missed at B1 and $\rho = 8$, $0.811$. The band factors move a rate $p$ by $\operatorname{sc}(p, f)/p - 1$: $+25\%/-20\%$ for small $p$ (W1, W9b), $+16\%/-15\%$ at 0.30 (W4.missed), which is the "about 20%" of section 7. The band enters the interval only through $\mu$ and $\nu$.

*Proof.* $\operatorname{sc}(p, f) = o/(1+o)$ with $o = f p/(1-p) \in (0, \infty)$, so it lies in $(0, 1)$ and its odds are $o$. W4.missed at B1: $\operatorname{odds} = 0.3/0.7 \times 1.25 \times 8 = 4.286$, $\theta = 0.811$. For small $p$, $\operatorname{sc}(p, f) \approx fp$. $\blacksquare$

Checked by R16.

## 5. Numbers

Every number `decision.md` and `summary.md` quote, recomputed by `math_check.py` `numbers` (rows `N`) from `results.json` and from the replay: the registered seeds (`seed`, sha256 of `EXP-0002/<name>`) drawn through (M1)–(M2) and scored by (E1) with scipy's Beta quantiles. The replay reproduces every committed cell's covered, below, above, credible and no-event counts, mean ratio, median width ratio and Wilson interval exactly (N1), the #13 rows (N2), and, with the two bootstrap seeds, every primary and margin interval and p value to six decimals (N3, N4). So the Rust posterior is (E1) on every draw, and the numbers below are recomputed rather than copied.

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| 12 outside, 2 within, 6 undecided; decision outside | (E5) | `primary.cells[].classification`, `decision` | 12, 2, 6 |
| the 20 coverages and Bonferroni intervals (table) | (E3), (E4) | `primary.cells` | all 20 match at 2 dp |
| worst W4.missed at 30, 0.10 [0.09, 0.11] | (E4) | `primary.worst` | 0.1003 [0.0875, 0.1131] |
| W1 at 30, 0.64 [0.61, 0.66] (summary) | (E4) | `primary.cells` | 0.6361 [0.6123, 0.6599] |
| outside: W1 30, 60; W4.missed, W7.traps all; W9b 30 (low); W5 250 (high); within: W1 250, W9b 120 | (E5) | `primary.cells` | as quoted |
| pooled 0.572 [0.568, 0.575]; 3,600 players a coverage | (E3), Result 2(c) | `primary.pooled_coverage`, `pooled_ci95`, `players` | 0.5716 [0.5682, 0.5750], 3600 |
| credible W4.missed: truth above the interval 80% | count | `credible[].below` | 0.7957 |
| B2 W4.missed $\rho = 4$: true 0.63; mean 0.69 then 0.88 of the truth | Result 3 | `cells[].mean_ratio` (mean of the two dispersion cells) | 0.632; 0.686, 0.878 |
| 53 chances against 300, about six games | $300/53$ | – | 5.66 |
| schedule contrast: W1 250 0.80 → 0.73; W9b 120 0.78 → 0.73; W5 250 0.90 → 0.81; W4.missed 250 0.26 → 0.39; W7.traps 0.43 → 0.54 | mean of 18 cells | `cells[].coverage` | 0.802/0.734, 0.785/0.730, 0.904/**0.805**, 0.260/0.391, 0.432/0.542 |
| key margins W1 0.73 [0.71, 0.74], W4.missed 0.18 [0.17, 0.19], W7.traps 0.32 [0.30, 0.33], W9b 0.77 [0.76, 0.79], W5 0.86 [0.85, 0.87]; classes | (E3), (E4) | `margins` | all match |
| $\rho$: 0.78 within, 0.87 [0.86, 0.88], 0.72, 0.54, 0.46 outside | same | `margins` | 0.776, 0.870 [0.862, 0.877], 0.724, 0.535, 0.456 |
| bands 0.56, 0.57, 0.58; checkpoints 0.50, 0.55, 0.60, 0.65; dispersion 0.59, 0.56 ("about three points"); schedule 0.572, 0.566 | same | `margins` | all match; 0.030 |
| all 21 Holm p below 0.001 | (E6) | `margins[].p_holm` | all 0 at 6 dp |
| margins hold 1,200 (band, $\rho$), 1,800 (dispersion), 3,600 (key, checkpoint, schedule) players; every interval narrower than 0.025 a side | count; (E4) | replay; `margins[].ci_bonferroni` | as quoted; widest 0.0140 |
| credible W1 0.73, W4.missed 0.20, W7.traps 0.25, W9b 0.77, W5 0.83 | (E7) | `credible[].coverage` | 0.733, 0.199, 0.254, 0.773, 0.834 |
| #13 none: 0.53 [0.46, 0.60], 0.65, 0.65, 0.73 [0.66, 0.79]; de1.5: 0.53, 0.615, 0.675, 0.755; every estimate credible; mean 0.83 of the truth at 30 | (E7), Result 3 | `issue13.rows` | 0.53 [0.4609, 0.5980], 0.645, 0.65, 0.73 [0.6646, 0.7868]; as quoted; 1.0; 0.828 |
| calibration: 50% → 34%, 80% → 57%, 95% → 74% | (E8), Result 2 | `calibration_primary.empirical` | 0.338, 0.572, 0.737 |
| W4.missed and W7.traps at $\rho = 1$ covered 0.93 | mean of 24 cells each | `cells[].coverage` | 0.9375 and 0.9281 (0.933 together) |
| EXP-0001's end-to-end W4.missed coverage 0.79 | – | `../EXP-0001/results.json` `observed.coverage[3].per_flaw.miss_mate` | 0.7917 |
| 6,000 players; 49,600 profile computations | $30 \times 200$; $30 \cdot 200 \cdot 2 \cdot 4 + 2 \cdot 200 \cdot 4$ | design | 6000, 49600 |
| 1,592 s; 0.44 CPU hours; 27 minutes | $1592/3600$, $1592/60$ | prose (not in `results.json`) | 0.442, 26.5 |
| `algo-1.1.0+fa5e2186e1dd` | – | `algorithm_id` | as quoted |
| section 6: se 0.0067, half-width 0.020 (z 3.02), $\rho$ margin 0.035, 100 a cell 0.029, Wilson about 0.056 | Results 10, 15 | – | 0.00667, 0.0202, 0.0350, 0.0285, 0.0552 |
| section 6: within > 0.99 at 0.80; about 0.93 at 0.78, 0.82; outside > 0.9 at 0.72, 0.88; 100 a cell passes 0.78 "two in three" | Result 13 | – | 0.99999; 0.930; 0.930 at $v = 0.16$ (0.838 at $v = C(1-C)$, Finding 1); 0.563 (Finding 2) |
| $z = 3.02$ (20), $z = 3.03$ (21) | $\Phi^{-1}(1 - 0.05/2m)$ | `primary.z_bonferroni` | 3.0233; **3.0381** |
| W4.missed at B1, $\rho = 8$ is 0.811; compressed weights above 0.988; 250 games over 125 days inside 183 | Results 16, 4 | – | 0.8108; 0.98808; 124.5 days |

> **Finding 3 (rounding).** (a) `decision.md` (the schedule contrast) and `review.md` part two (concern 1) quote W5 at 250 games without the weighting as 0.81; the mean of the 18 compressed cells is 0.8047, which rounds to 0.80. (b) `preregistration.md` sections 6 and 8 give $z = 3.03$ for the 21 margins, and `review.md` part one endorses it; $z_{21} = 3.0381$, 3.04 to two decimals (the code computes it, so only the text is off; the half-width 0.035 stands). (c) Section 6's Wilson half-width "about 0.056" at 0.80 and 200 players is 0.0552. None changes a classification.

Not recomputed: the 1,592 s run time and the detector's under-read (0.019 against 0.023, from `docs/benchmark.md`); neither is in `results.json`.

## 6. Assumptions that can fail

- **The replay is the production code: every number.** It is shown, not assumed: N1 reproduces the 1,200 cells from the seeds. It is tied to numpy's generator streams; a numpy version that changes `poisson`, `binomial` or `beta` would break N1 without the experiment being wrong.
- **$\theta$ fixed, no drift: Results 3, 7, 8.** The rate is constant over 125 days. A player who improves has a rate at `now` below the window's mean, and the interval is centred on a weighted mean of the past; nothing in the design covers drift (pre-registration section 7 says so).
- **The dispersion model: Results 5 to 7.** One source (within-game Beta) at one level, design effect 1.51 and 1.52 (Result 5), for two keys. A larger or a between-game effect lowers coverage by the factor $\bar D$ in Result 6. The registered dispersion margin (0.59 against 0.56) is the only bracket.
- **Independent games and players: Results 5, 10, 11.** True by construction in (M1)–(M2). In real data the games of a player share opponents and form, which Result 5's $d_t$ does not capture.
- **The assumed peer rates and chances: Results 3, 4, 7.** $\lambda$ depends on $n_0/(\bar c\sum w)$ and the bias on $\mu/\theta$. Other chances a game or peer rates move every cell. Section 7 calls them assumptions; the #13 cell is the one measured case. No sensitivity varies them.
- **Normal approximations: Results 7, 12, 13, 14.** Result 7's error is measured on the grid (at most 0.018 on the primary, 0.038 at $\rho = 1$), not bounded; off the grid (fewer chances, smaller $g$) it can be larger, and Result 9 is exact where it applies. Results 12 to 14 use the normal limit for $\hat C$ and its tests; the decision's outside cells (5.5 SE or more beyond the edge) make this immaterial for EXP-0002, but a close cell in EXP-0006 would need care.
- **The weighted posterior is not a posterior: Results 1, 8.** Result 8's calibration needs unit weights and the right likelihood. The spread schedule breaks both, which is why the compressed schedule is the cleaner test of the prior alone.
- **Checkpoints as prefixes: all results at $g < 250$.** A 30-game checkpoint is the first 30 games of a 250-game player, scored with the clock one hour after game 30; that is what a 30-game player would produce under (M1)–(M2), since games are i.i.d.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py`, `results.json` and the production code at commit 7409f457, but not this file. It re-derived five results independently:

- the exact bias $E[\hat m]/\theta = 1 - (1 - \mu/\theta)E[\lambda]$ (Result 3), with 0.688 and 0.878 for B2 W4.missed at $\rho = 4$ and 0.836 for the #13 cell;
- the spread-schedule sums, their limits 173.5 and 0.501, and the window's floor of 0.208 on the prior's share (Result 4);
- the weighted Beta-binomial variance and the pooled design effects 1.510 and 1.516 (Result 5);
- the coverage approximation in the same form as Result 7, with W5 at 250 games 0.90 spread and 0.805 compressed;
- the section 6 arithmetic (Results 10 and 13), and the primary bootstrap standard errors recomputed from the committed cells (Result 11).

Every formula and number agreed. It found the same three discrepancies independently: the 0.72 power is 0.84, not above 0.9 (Finding 1); the 100-player sensitivity is 0.56, not two in three (Finding 2); and $z_{21} = 3.038$, not 3.03 (Finding 3). It also confirmed the margin sizes that `decision.md` lists as deviation 1.

While `math_check.py` was being written, five checks failed on a first draft. Each was traced to the text rather than widened:

- W5 at 250 games without the weighting is 0.805, which rounds to 0.80, not 0.81 (now Finding 3(a));
- W4.missed and W7.traps at $\rho = 1$ are 0.94 and 0.93 separately, 0.93 only together;
- the 0.69 mean ratio is the mean of the two dispersion cells (0.689 and 0.684);
- the #13 plug-in $\lambda$ gives 0.836, against the sample's 0.828;
- the 100-player half-width is 0.0285 at $z = 3.023$.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee. It confirmed that the replay is an independent reproduction and that the three Lean citations match their hypotheses, with the last two approximate as stated. It raised 14 points, all accepted:

1. **Major.** Result 4(c) bounded a player's $\lambda$, but only $E[\lambda]$ is bounded. It is now stated for $E[\lambda]$, with the Jensen step (convexity of $\lambda$ in $n$) in the proof and checked in R4.
2. **Major.** "More than 300 standard errors" is now 154, and N3 asserts the maximum and which cells carry 0.731.
3. **Major.** R7's tolerances (0.03 and 0.06) were looser than the text's claims. They are now 0.0185 and 0.0385, and the quoted pairs are asserted at three decimals.
4. **Minor.** R12, R13 and R9 were circular. R12 and R13 are now labelled as arithmetic on the normal model. A new R12 check runs the real procedure: Bernoulli strata, a bootstrapped standard error and the Bonferroni interval. R9's simulation now draws its own intervals with the production widening.
5. **Minor.** R4, R10 and R1 checked identities or literals. The inequalities are now also tested in sign on a grid, and the arithmetic check is labelled as such.
6. **Minor.** $\bar D$ in Result 6 is now the $w^2c$-weighted mean.
7. **Minor.** W5's $\theta$ lies on both sides of $\mu$. Result 3's text now says so, and Result 7 calls the W5 formula a heuristic.
8. **Minor.** The #13 $E[\lambda]$ is 0.170.
9. **Minor.** Result 2(a) now gives the #13 floor (0.24, with every replayed $\alpha \ge 18.9$). N1's widening and shape checks include the #13 cell. The replay is said to match aggregates, not each interval.
10. **Minor.** Result 8 now states its shape condition.
11. **Minor.** Finding 1's threshold is a variance of about 0.175. The replayed strata reach 0.25.
12. **Minor.** Finding 3 now names the right documents. Result 10 and N7 use $z_{21}$.
13. **Minor.** The Wilson "about 0.056" (0.0552) is added to Finding 3.
14. **Minor.** The following numbers are now checked:
    - the band moves of Result 16;
    - Result 9's 0.100;
    - the credible W9b and W5 inside the band, with their Wilson intervals;
    - "the first 40 games" against the weighted 46.

    A missing EXP-0001 file now fails instead of being skipped.
