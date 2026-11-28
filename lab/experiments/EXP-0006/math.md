# EXP-0006: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic checks and simulations from a known truth (prefix `R`), and the recomputation of every quoted number (prefix `N`).

EXP-0006 compares two prior rules on EXP-0002's generator and on a Tier 1 run. The maths has to show four things. First, that the challenger's interval is the formula of section 3: `math_check.py` replays the 32 registered cells from their seeds through both rules and reproduces every committed coverage, credible share, bootstrap interval and the decision (N1 to N4), and it reproduces `dev/power.json` from its own namespace (N5). Second, why the challenger is calibrated where the champion was not: the effective count removes the weight concentration exactly, the cap fixes the prior's share at $2/(g+2)$, and the dispersion factor puts back the variance the weights hid (Results 1 to 5). Third, that the decision rule has the error control the pre-registration claims (Results 6 to 12). Fourth, which of its arguments in words hold (Result 13 and four Findings).

The model, the generator and the champion's posterior are EXP-0002's; its `math.md` proves them (Results 1 to 7 there). They are restated here where a proof below needs them.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $\kappa$ | a weakness key: W1, W4.missed, W7.traps, W9b, W5 | – | EXP-0002 `KEYS` |
| $B$, $f_B \in \{1.25, 1, 0.8\}$ | rating band and its odds factor | – | EXP-0002 `BANDS` |
| $\rho \in \{0.5, 1, 2, 4, 8\}$ | the true rate's odds multiple of the peers'; primary $\rho \in \{2, 4, 8\}$ | – | `PRIMARY_RHOS` |
| $\mu$ | the prior mean: the peers' pooled rate for the four peer keys, $\tfrac12$ for W5; 0.0008 in the #13 cell | per chance | EXP-0002 `peer_rate`, `stat` |
| $\nu$ | the band median `credible` compares with: $\mu$ for the peer keys, 0.15 for W5 | per chance | `stat` `median` |
| $\theta$ | the true per-chance rate, $\operatorname{odds}^{-1}(\rho \operatorname{odds}(\nu))$; 0.023 in the #13 cell | per chance | `theta_of` |
| $n_0$ | the band prior's strength: 300 (W1), 40 (W4.missed, W7.traps, W9b), 2 (W5, flat) | chances | `profile_core::constants::n0`; `estimate_weakness_against_model` |
| $g \in \{30, 60, 120, 250\}$ | the checkpoint, games in the window | games | `CHECKPOINTS` |
| $c_t$, $e_t$ | chances and events in game $t$ | chances, events | EXP-0002 `chances`, `events` |
| $\bar c$ | mean chances a game: 53, 1, 1, 10.6, 0.4 | chances | `KEYS[...]["chances"]` |
| $d_t = 1 + (c_t - 1)/(\gamma + 1)$, $\gamma + 1 = (m - 1)/(D - 1)$ | a game's design effect under `de1.5` (1 without); $m$ = 53 (W1), 31.8 (W9b), $D = 1.5$ | – | EXP-0002 Result 5 |
| $w_t = 2^{-a_t/60}$, $a_t = j/2 + 1/24$ | recency weight of the game $j$ games before the last, two games a day | – | `weights::recency_weight` |
| $k = \sum w_t e_t$, $n = \sum w_t c_t$, $S_2 = \sum w_t^2 c_t$ | weighted events, chances and squared-weight chances | – | `weights::weighted_counts`, `priority::data_counts` |
| $\hat p = k/n$ | the weighted rate | per chance | `data_counts` `rate` |
| $n_{\text{eff}} = n^2/S_2$ | Kish's effective number of chances | chances | `data_counts` `n_eff` |
| $G_c$ | the games with at least one chance | games | `dispersion` `games` |
| $\varphi$ | the Pearson dispersion, clamped to $[1, 10]$ | – | `priority::dispersion` |
| $\tilde n = n_{\text{eff}}/\varphi$, $\tilde k = \hat p\,\tilde n$ | the challenger's counts | chances, events | `data_counts` |
| $s_0 = \min(n_0, 2\tilde n/g)$ | the challenger's prior strength (peer keys; W5 keeps 2) | chances | `priority::prior_strength` |
| $\lambda = s/(s + n_{\text{data}})$ | the prior's share of the posterior mean, for prior strength $s$ and data count $n_{\text{data}}$ | – | – |
| $\hat m$, $I = [L, U]$ | posterior mean and production 80% interval | per chance | `posterior::posterior` |
| $\bar D = \sum w_t^2c_td_t / S_2$ | the $w^2c$-weighted design effect | – | Result 4 |
| $V$ | 1 if $L \le \theta \le U$ | – | `analysis.py` `run_cell` |
| $C_j$, $\hat C_j$ | coverage $j$ of the 20 primary (5 keys × 4 checkpoints) and its estimate | share | `stat`, `profile_statistics` |
| $D = \max_j \lvert C_j - 0.8\rvert$, $\hat D$ | the primary metric and its estimate | share | `profile_statistics` |
| $F$ | the false-credible share at $\rho \le 1$ | share | `profile_statistics` `false_credible` |
| $z_{0.9} = 1.2816$, $z_{0.9875} = 2.2414$, $z_{20} = 3.0233$, $z_4 = 2.4977$ | normal quantiles: 80% two-sided, 97.5% two-sided, Bonferroni for 20 and for 4 | – | `GUARD_LEVEL`, `SHIP_LEVEL`, `z_bonf`, `z4` |
| $n_{11}, n_{10}, n_{01}, n_{00}$ | a paired Tier 1 table: both sides, challenger only, champion only, neither | players | `table` |

On the spread schedule $w_j = q_0\varrho^j$ with $q_0 = 2^{-1/1440}$, $\varrho = 2^{-1/120}$; $\sum_{j<g} w_j$ is 27.6, 50.8, 86.8 and 132.6 at the four checkpoints (EXP-0002 Result 4).

## 2. Model

**(M1) Chances and events** (EXP-0002 (M1)–(M2), imported). For each player, key and game, independently: W1 $c \sim \operatorname{Poisson}(53)$; W4.missed $\operatorname{Poisson}(1)$; W7.traps $c = 1$; W9b $c = 0$ with probability $2/3$, else $\operatorname{Poisson}(31.8)$; W5 $\operatorname{Bernoulli}(0.4)$. Given $c_t$, $e_t \sim \operatorname{Binomial}(c_t, \theta)$; under `de1.5`, for W1 and W9b only, $e_t \mid \vartheta_t \sim \operatorname{Binomial}(c_t, \vartheta_t)$ with $\vartheta_t \sim \operatorname{Beta}(\theta\gamma, (1-\theta)\gamma)$. $\theta$ is fixed. Players and games are independent. Only the two-games-a-day schedule runs; checkpoints are prefixes of one 250-game series.

**(M2) The two rules** (`backend/crates/profile-core/src/stats/priority.rs`, `estimate_with_prior`, `data_counts`, `dispersion`, `prior_strength`; unchanged in code from badb094a, the run's variant commit, to main). Both feed $(k_\star, n_\star, \mu, s)$ to `posterior`, which returns $\operatorname{Beta}(s\mu + k_\star, s(1-\mu) + n_\star - k_\star)$ and the interval $[\min(q_{0.1}, \hat m), \max(q_{0.9}, \hat m)]$.

- Champion: $(k_\star, n_\star) = (k, n)$, $s = n_0$.
- Challenger: $(k_\star, n_\star) = (\tilde k, \tilde n)$, $s = s_0$ for the peer keys and $s = 2$, $\mu = \tfrac12$ for W5.

**(M3) Credible** (ADR 0101, `estimate_with_prior`). An estimate is credible when it has an event, $L > \nu$, and $\max(0, \hat m - \nu)(\sum_t c_t/g)\,\epsilon \ge 0.02$, with $\epsilon = 0.3$ (peer keys) or 1 (W5).

**(M4) The Tier 1 tables.** Per player, an outcome on each side (a top-2 hit, a rank-1 weakness). Players are independent; the two sides of one player are paired.

## 3. Estimators

- **(E1) The champion** (`estimate_weakness` with `PriorRule::ALGO_1` behaviour): $\alpha = n_0\mu + k$, $\beta = n_0(1-\mu) + n - k$.
- **(E2) The challenger's counts** (`data_counts`): $\hat p = \operatorname{clamp}(k/n, 0, 1)$, $n_{\text{eff}} = n^2/S_2$,

```math
\varphi = \operatorname{clamp}\Big(\frac{1}{G_c - 1}\sum_{t : c_t > 0}\frac{(e_t - c_t\hat p)^2}{c_t\,\hat p(1 - \hat p)},\ 1,\ 10\Big)
```

  ($\varphi = 1$ when $G_c < 2$ or $\hat p \in \{0, 1\}$), $\tilde n = n_{\text{eff}}/\varphi$, $\tilde k = \hat p\tilde n$.
- **(E3) The challenger's posterior**: $s_0 = \min(n_0, 2\tilde n/g)$, $\alpha = s_0\mu + \tilde k$, $\beta = s_0(1-\mu) + \tilde n - \tilde k$; for W5, $s = 2$, $\mu = \tfrac12$.
- **(E4) A coverage** (`stat`): the mean over the cells in scope of the mean over the cell's players of the player's share of covered intervals. Primary: 18 cells of 200 players. $\rho = 1$ guard: 6 cells of 800, all 20 key × checkpoint intervals a player. False-credible guard: 12 cells of 800 ($\rho \in \{0.5, 1\}$), the credible indicator in place of $V$.
- **(E5) The paired stratified bootstrap** (`resamples`, `stat`): $B = 2000$ draws of player indices per cell (seed `EXP-0006/bootstrap/paired`, cells in plan order), the same draw for every key, checkpoint and both rules. Percentile intervals are the replicates' quantiles (numpy's linear interpolation).
- **(E6) The primary** (`profile_statistics`): $\hat D = \max_j\lvert\hat C_j - 0.8\rvert$, $D^\* = \max_j\lvert C^\*_j - 0.8\rvert$ per replicate; $\Delta D = \hat D_{\text{champion}} - \hat D_{\text{challenger}}$, likewise.
- **(E7) The guards**: $\rho = 1$: $\lvert\hat C^{\text{ch}}_1 - 0.8\rvert - \lvert\hat C^{\text{cm}}_1 - 0.8\rvert$; false credible: $\hat F^{\text{ch}} - \hat F^{\text{cm}}$; 80% percentile intervals.
- **(E8) Bonferroni classification** (secondary): $\hat C_j \pm z_{20}\,\widehat{\operatorname{se}}_j$, $\widehat{\operatorname{se}}_j$ the replicates' SD (divisor $B - 1$); "within" when it lies in $[0.75, 0.85]$.
- **(E9) The #13 cell**: Wilson's interval at $z_4$ over the 1,600 players of both dispersion halves (`wilson`).
- **(E10) Newcombe's method 10** (EXP-0001 `newcombe_paired`, imported): $\hat p_1 = (n_{11} + n_{10})/N$, $\hat p_2 = (n_{11} + n_{01})/N$, $(l_i, u_i)$ their Wilson intervals, $\hat\phi = (n_{11}n_{00} - n_{10}n_{01})/\sqrt{(n_{11}+n_{10})(n_{01}+n_{00})(n_{11}+n_{01})(n_{10}+n_{00})}$ (0 when the root is 0),

```math
\hat d \mp \sqrt{(\hat p_1 - l_1)^2 - 2\hat\phi(\hat p_1 - l_1)(u_2 - \hat p_2) + (u_2 - \hat p_2)^2}\ \text{(lower)},\quad
\hat d + \sqrt{(u_1 - \hat p_1)^2 - 2\hat\phi(u_1 - \hat p_1)(\hat p_2 - l_2) + (\hat p_2 - l_2)^2}\ \text{(upper)}.
```

- **(E11) The rule** (`decide`): ship if the 97.5% interval of $D^\*$ is at or below 0.07, the 97.5% interval of $\Delta D$ is above 0, the $\rho = 1$ guard's 80% upper end is at most 0.05, the false-credible guard's at most 0.01, recall's 80% Newcombe lower end at least $-0.10$, and the smoke CI rule finds nothing. Reject if the primary's lower end is above 0.07 or a guard lies wholly beyond its margin.
- **(E12) Jeffreys 95%** (`jeffreys`): the 2.5% and 97.5% quantiles of $\operatorname{Beta}(x + \tfrac12, n - x + \tfrac12)$, with 0 below at $x = 0$ and 1 above at $x = n$.
- **(E13) The power pilot** (`power.py`): the same design on `EXP-0006-power`; in each of its 2,000 replicates the rule (E11) is applied with the replicates' SDs as half-widths ($z_{0.9875}$ for the primary and $\Delta D$, $z_{0.9}$ for the guards).

## 4. Derivations and proofs

**Result 1 (the effective count).** (a) Given the chances, without dispersion, $\operatorname{Var}[\hat p \mid c] = \theta(1-\theta)/n_{\text{eff}}$; with dispersion, $\theta(1-\theta)\bar D/n_{\text{eff}}$. So $n_{\text{eff}}$ is the number of unweighted chances whose binomial rate has the weighted rate's variance. (b) $n \le n_{\text{eff}} \le \sum_t c_t$. (c) With one chance a game on the spread schedule, $n_{\text{eff}} = (\sum w)^2/\sum w^2$ = 29.9, 59.4, 115.4 and 214.0 at 30, 60, 120 and 250 games.

*Proof.* (a) $\hat p = \sum_t w_te_t/n$ with $n$ fixed given $c$. The $e_t$ are independent with $\operatorname{Var}[e_t \mid c_t] = c_t\theta(1-\theta)d_t$ (EXP-0002 Result 5; $d_t = 1$ without dispersion). So $\operatorname{Var}[\hat p \mid c] = \theta(1-\theta)\sum_t w_t^2c_td_t/n^2 = \theta(1-\theta)\bar D S_2/n^2 = \theta(1-\theta)\bar D/n_{\text{eff}}$. (b) $n_{\text{eff}} - n = n(n - S_2)/S_2$ and $n - S_2 = \sum_t w_t(1 - w_t)c_t \ge 0$ since $0 < w_t \le 1$. For the upper bound, Lagrange's identity gives $(\sum c_t)(\sum w_t^2c_t) - (\sum w_tc_t)^2 = \sum_{s<t}c_sc_t(w_s - w_t)^2 \ge 0$, so $n^2 \le S_2\sum c_t$. (c) Put $c_t = 1$. $\blacksquare$

So the champion's posterior, which counts $n$, treats the data as $n$ chances, while their variance is that of $n_{\text{eff}} = n/r$ chances, $r = S_2/n \le 1$ (EXP-0002's weight concentration). The weights already discount old games, and the champion discounts them a second time in the width. Checked by R1 (identities, a grid of 20,000 random vectors for (b), and a simulated $\operatorname{Var}[\hat p]$ at fixed chances, with and without dispersion).

**Result 2 (the dispersion factor).** (a) For any $p$, with $\bar e = \sum_te_t/\sum_tc_t$ and $N_c = \sum_tc_t$,

```math
\sum_t\frac{(e_t - c_tp)^2}{c_t} = \sum_t\frac{(e_t - c_t\bar e)^2}{c_t} + N_c(\bar e - p)^2 .
```

(b) At the true rate, $E[(e_t - c_t\theta)^2/(c_t\theta(1-\theta))] = d_t$, so the unclamped statistic with $\theta$ in place of $\hat p$ has mean $\frac{1}{G_c - 1}\sum_{t:c_t>0}d_t$. (c) With exactly one chance a game (W7.traps), before the clamp,

```math
\varphi_{\text{raw}} = \frac{G}{G-1}\Big[1 + \frac{(\bar e - \hat p)(1 - 2\hat p)}{\hat p(1-\hat p)}\Big].
```

(d) The clamp only widens: $E[\max(1, \varphi_{\text{raw}})] \ge \max(1, E[\varphi_{\text{raw}}])$. If $(G_c - 1)\varphi_{\text{raw}}$ is $\chi^2_\nu$ with $\nu = G_c - 1$, then $E[\max(1, \varphi_{\text{raw}})] = 1 + P(\chi^2_{\nu+2} > \nu) - P(\chi^2_\nu > \nu)$: 1.104 at $\nu = 29$, 1.185 at $\nu = 9$. (e) Under `de1.5` with Poisson($m$) chances, the mean of $d_t$ over games with a chance is $1 + (D-1)\big(\tfrac{m}{1 - e^{-m}} - 1\big)/(m - 1) = 1.500$ for W1 and W9b, while the variance needs $\bar D \to E[c\,d]/E[c] = 1 + (D-1)m/(m-1)$, 1.510 and 1.516. So $\varphi$ aims 0.6% (W1) and 1.1% (W9b) below $\bar D$. (f) With the weighted $\hat p$ in place of $\bar e$, the statistic gains $N_c(\bar e - \hat p)^2/(\hat p(1-\hat p))$ by (a): the weights can only raise $\varphi$.

*Proof.* (a) Write $e_t - c_tp = (e_t - c_t\bar e) + c_t(\bar e - p)$ and square: the cross term summed over $t$, divided by $c_t$, is $2(\bar e - p)\sum_t(e_t - c_t\bar e) = 0$ by the definition of $\bar e$, and the last term sums to $(\bar e - p)^2\sum_t c_t$. (b) $E[(e_t - c_t\theta)^2] = \operatorname{Var}[e_t] = c_t\theta(1-\theta)d_t$ by the law of total variance over $\vartheta_t$ (EXP-0002 Result 5); divide and sum. (c) With $c_t = 1$, $e_t \in \{0,1\}$ so $e_t^2 = e_t$, and $\sum_t(e_t - p)^2 = G(\bar e - 2p\bar e + p^2)$. And $\bar e - 2p\bar e + p^2 = p(1-p) + (\bar e - p)(1 - 2p)$ by expansion. Divide by $(G-1)p(1-p)$. (d) $x \mapsto \max(1, x)$ is convex, so Jensen's inequality gives the first claim; it is also $\ge 1$. With $X \sim \chi^2_\nu$, $E[\max(1, X/\nu)] = 1 + \frac1\nu E[(X - \nu)\mathbf 1\{X > \nu\}]$, and $xf_\nu(x) = \nu f_{\nu+2}(x)$ for the $\chi^2$ densities (the ratio of the normalising constants is $2\Gamma(\nu/2 + 1)/\Gamma(\nu/2) = \nu$), so $E[X\mathbf 1\{X > \nu\}] = \nu P(\chi^2_{\nu+2} > \nu)$. (e) For $c \sim \operatorname{Poisson}(m)$, $E[c \mid c \ge 1] = m/(1 - e^{-m})$, and $d$ is linear in $c$; the zero-inflation of W9b conditions it away. $E[c\,d]/E[c]$ is EXP-0002 Result 5. (f) Put $p = \hat p$ in (a); the added term is a square. $\blacksquare$

The clamp also has an upper end at 10, under which $\max(1, \min(10, x))$ is not convex; (d) uses only the lower end, which is valid because the upper end never binds here (the replay's largest $\varphi$ is 8.2, a W9b draw).

The $\chi^2$ form of (d) is an approximation: it ignores that $\hat p$ is estimated and that W1 at $\theta = 0.004$ has 0.2 events a game, far from the many-events limit. The replay measures it: W1 at 30 games without dispersion has $E[\varphi] = 1.088$ against 1.104. With `de1.5` the replay's mean $\varphi$ for W1 and W9b is 1.47 to 1.55. At 30 games and $\rho = 1$ without dispersion, the replay's mean $\varphi$ is 1.172 (W9b), 1.094 (W4.missed), 1.088 (W1), 1.082 (W5) and 1.035 (W7.traps). Two mechanisms set it. Where a game's count is 0 or 1 (W7.traps, W5), (c) makes $\varphi_{\text{raw}}$ nearly the constant $G_c/(G_c - 1)$, so the clamp adds little: W7.traps 1.035 against $30/29 = 1.0345$, W5 1.082 against $E[G_c/(G_c-1)] = 1.098$ for $G_c \sim \operatorname{Binomial}(30, 0.4)$. Where games have many chances (W1, W9b), $\varphi_{\text{raw}}$ varies like $\chi^2_\nu/\nu$ with $\nu = G_c - 1$ and the clamp adds (d)'s excess, which falls strictly in $\nu$: W9b, with about 10 games with a chance, 1.172 against 1.191 averaged over $G_c \sim \operatorname{Binomial}(30, \tfrac13)$; W1, with 30, 1.088 against 1.104. W4.missed (mostly 0 to 2 chances) lies between. The mean falls from 30 to 120 games for W1, W4.missed, W9b and W5, but not for W7.traps (1.035 at 30, 1.034 at 250): as the window grows, the weighted $\hat p$ drifts from $\bar e$ and (c)'s second term, cut at 1, adds back what $G/(G-1)$ loses (Finding 2). Checked by R2 (sympy for (a) to (e), all 64 binary series of six games for (c), quadrature for (d), simulation for (b), the replay for the rest).

**Result 3 (the cap fixes the prior's share).** For a peer key, the challenger's prior share $\lambda' = s_0/(s_0 + \tilde n)$ satisfies $\lambda' \le 2/(g + 2)$, with equality exactly when $\tilde n \le n_0g/2$; a sufficient condition is $\sum_tc_t \le n_0g/2$. When the cap binds, $\hat m = \lambda'\mu + (1 - \lambda')\hat p$ and

```math
E[\hat m \mid c] - \theta = \frac{2}{g + 2}(\mu - \theta), \qquad \frac{E[\hat m]}{\theta} = 1 - \frac{2}{g+2}\Big(1 - \frac{\mu}{\theta}\Big),
```

the same for every key and independent of the data: 0.0625, 0.0323, 0.0164 and 0.0079 of the gap at the four checkpoints.

*Proof.* $\lambda'(s) = s/(s + \tilde n)$ has derivative $\tilde n/(s + \tilde n)^2 > 0$, so it is largest at the largest $s$. $s_0 \le 2\tilde n/g$, and at $s = 2\tilde n/g$, $\lambda' = (2\tilde n/g)/(2\tilde n/g + \tilde n) = 2/(2 + g)$. Equality needs $s_0 = 2\tilde n/g$, that is $2\tilde n/g \le n_0$. Since $\varphi \ge 1$, $\tilde n \le n_{\text{eff}} \le \sum c_t$ (Result 1(b)), so $\sum c_t \le n_0g/2$ suffices. With $\tilde k = \hat p\tilde n$, $\hat m = (s_0\mu + \hat p\tilde n)/(s_0 + \tilde n) = \lambda'\mu + (1 - \lambda')\hat p$. (Here $\tilde n > 0$, which holds whenever any game has a chance: $\lambda'$ is undefined at $\tilde n = 0$, where the shape floor gives $\operatorname{Beta}(10^{-3}, 10^{-3})$; no replayed draw has it.) When the cap binds, $\lambda'$ is the constant $2/(g+2)$ whatever $\varphi$ is, and $E[\hat p \mid c] = E[k \mid c]/n = \theta$ (EXP-0002 Result 3), so linearity gives the bias. The shapes are positive ($s_0\mu > 0$ when $n > 0$), so the shape floor never applies. $\blacksquare$

The sufficient condition holds on every draw for W1 ($\sum c \approx 53g$ against $150g$), W4.missed and W7.traps ($\approx g$ against $20g$). For W9b ($\approx 10.6g$ against $20g$) the replay finds the cap free on 3 of 217,600 peer-key draws, all W9b at 30 games, where $\lambda' < 2/(g+2)$. Against the champion's shares (EXP-0002 Result 3; on these seeds 0.17, 0.59, 0.59 and 0.13 at 30 games for W1, W4.missed, W7.traps and W9b), the challenger's is 0.0625 for all four. For B2 W4.missed at $\rho = 4$ ($\theta = 0.632$, $\mu = 0.3$), the expected mean is 0.967 of the truth at 30 games against the champion's 0.69 (EXP-0002 Result 3). W5's flat prior is not capped; its share is about 0.16 at 30 games under both rules. Checked by R3 (sympy; the replay's $\lambda'$ on every draw; every peer-key cell mean of $\hat m$ against the formula, max $\lvert z\rvert$ 3.05 over 488 means).

**Result 4 (what the challenger's posterior claims against what the data do).** To first order, at $\hat m \approx \theta$,

```math
\frac{\operatorname{Var}[\hat m \mid c]}{\hat m(1-\hat m)/(s_0 + \tilde n + 1)} \approx (1 - \lambda')\frac{\bar D}{\varphi},
```

against the champion's $r\bar D(1 - \lambda)$ (EXP-0002 Result 6). The weight concentration $r$ is gone, the dispersion enters as $\bar D/\varphi$, and the prior's share is at most $2/(g+2)$.

*Proof.* The Beta variance is $\alpha\beta/((\alpha+\beta)^2(\alpha+\beta+1)) = \hat m(1-\hat m)/(s_0 + \tilde n + 1)$. By Result 3, $\operatorname{Var}[\hat m \mid c] = (1 - \lambda')^2\operatorname{Var}[\hat p \mid c]$ when $\lambda'$ is fixed, which is $(1 - \lambda')^2\theta(1-\theta)\bar D/n_{\text{eff}}$ by Result 1(a). Replace $\hat m(1 - \hat m)$ by $\theta(1-\theta)$ and $s_0 + \tilde n + 1$ by $s_0 + \tilde n = \tilde n/(1 - \lambda') = n_{\text{eff}}/(\varphi(1 - \lambda'))$ (the approximations). The ratio is $(1-\lambda')^2\bar D/n_{\text{eff}} \cdot n_{\text{eff}}/(\varphi(1-\lambda')) = (1 - \lambda')\bar D/\varphi$. For the champion, $n_{\text{eff}} = n/r$ turns the same algebra into EXP-0002's $r\bar D(1-\lambda)$. $\blacksquare$

The approximation also treats $\varphi$ as fixed, though it is computed from the same events. Measured on the replay (the across-player variance of $\hat m$ over the mean posterior variance, pooled over the 18 primary cells), the ratio is within 0.096 of $(1-\lambda')\bar D/\varphi$ at every key and checkpoint, the largest gap at W5 and W7.traps at 30 games, where $\varphi$ is most tied to the events. Checked by R4.

**Result 5 (the coverage, approximately, and the lean towards the peers).** Let $\bar m = \lambda'\mu + (1 - \lambda')\theta$, $s = \sqrt{\bar m(1 - \bar m)/(s_0 + \tilde n)}$, $\sigma = (1 - \lambda')\sqrt{\theta(1-\theta)\bar D/n_{\text{eff}}}$ and $b = \lambda'(\mu - \theta)$, at the expected counts and $\varphi$ at its cell mean. Then

```math
C \approx \Phi\Big(\frac{zs - b}{\sigma}\Big) - \Phi\Big(\frac{-zs - b}{\sigma}\Big), \quad
P(U < \theta) \approx \Phi\Big(\frac{-zs - b}{\sigma}\Big), \quad P(L > \theta) \approx 1 - \Phi\Big(\frac{zs - b}{\sigma}\Big),
```

with $z = z_{0.9}$. At $\theta = \mu$ ($\rho = 1$, peer keys), $C \approx 2\Phi\big(z\sqrt{\varphi/(\bar D(1 - \lambda'))}\big) - 1$, at least 0.8 when $\varphi \ge \bar D(1 - \lambda')$. For $\theta > \mu$, $b < 0$, so the truth lies above the range more often than below it.

*Proof.* As EXP-0002 Result 7: approximate the posterior by $N(\hat m, s^2)$ and $\hat m$ by $N(\theta + b, \sigma^2)$, with the mean from Result 3 and the variance from Result 4. $U < \theta \iff \hat m - \theta < -zs$, which has probability $\Phi((-zs - b)/\sigma)$; $L > \theta \iff \hat m - \theta > zs$; $V = 1$ is the rest. At $b = 0$, $C = 2\Phi(zs/\sigma) - 1$ with $s^2/\sigma^2 = \varphi/(\bar D(1 - \lambda'))$ by Result 4, and $x \mapsto 2\Phi(x) - 1$ is increasing and equals 0.8 at $x = z$. For $b < 0$, $(-zs - b)/\sigma > (-zs + b)/\sigma$, so $P(U < \theta) = \Phi((-zs-b)/\sigma) > \Phi((-zs+b)/\sigma) = 1 - \Phi((zs - b)/\sigma) = P(L > \theta)$. $\blacksquare$

The approximations (normal posterior and mean, expected counts, a fixed $\varphi$) are not bounded analytically. Against the replay the formula is within 0.030 of all 20 primary coverages (largest at W9b and W5 at 30 games, where $G_c$ is about 10 to 12 and $\varphi$ is most variable) and within 0.042 of the 20 high-side miss shares. The lean is in all 16 peer-key coverages: the truth lies above the challenger's range in 0.131 to 0.173 of the intervals at 30 games and below it in 0.044 to 0.061. That is what `decision.md` calls "the challenger's ranges still lean towards the peers", and Result 3 is its cause: a bias of $2/(g+2)$ of the gap, whose size in standard errors, $\lvert b\rvert/\sigma = \frac{\lambda'}{1-\lambda'}(\theta - \mu)\sqrt{n_{\text{eff}}/(\theta(1-\theta)\bar D)} = \frac{2}{g}(\theta - \mu)\sqrt{n_{\text{eff}}/(\theta(1-\theta)\bar D)}$, shrinks like $1/\sqrt g$ since $n_{\text{eff}}$ grows at most like $g$. W5, the worst cell (0.828 at 30 games), is both wide and biased, and the width dominates: its prior is flat and uncapped, $\varphi \approx 1.08$ widens it and $1 - \lambda \approx 0.85$ in Result 4; its bias towards $\tfrac12$ is about 0.34 standard errors at $\rho = 2$ and 0.11 at $\rho = 4$ and 8. Checked by R5.

**Result 6 (a credible estimate at or below the peers' rate is a range wholly above the truth).** For $\rho \le 1$, credible implies $L > \theta$. So the false-credible share of a rule is at most its share of ranges lying wholly above the truth, which an equal-tailed calibrated 80% range has at about 0.10. And by linearity the expected number of extra credible estimates in a report of the five keys, averaged over the four checkpoints and the cells, is 5 times the pooled difference: a margin of 0.01 is at most 0.05 a report on that average, one in 20. A report at one checkpoint can see more, since the pooled share averages over checkpoints.

*Proof.* At $\rho \le 1$, $\theta = \operatorname{odds}^{-1}(\rho\operatorname{odds}(\nu)) \le \nu$, since $\operatorname{odds}$ is increasing. Credible requires $L > \nu$ (M3), so $L > \nu \ge \theta$. Taking expectations of $\mathbf 1\{\text{credible}\} \le \mathbf 1\{L > \theta\}$ gives the bound. For the report: the pooled share (E4) is the equal-weighted mean of the five keys' shares, so the sum over keys of the expected credible indicators is 5 times the pooled share, by linearity of expectation. $\blacksquare$

This turns section 5's "the price of calibration" into a bound. Over the guard's scope ($\rho \le 1$) the champion's ranges lay wholly above the truth in 0.120 of intervals and the challenger's in 0.089, and the excess floor of (M3) then removes most of them (credible shares 0.025 and 0.034). At $\rho = 1$ alone the shares are 0.059 and 0.086 (the champion 0.028 for W4.missed and W7.traps, whose strong prior pinned them to the peers); the champion's larger share at $\rho = 0.5$ is its prior pulling the ranges up to the peers. The replay checks the implication on every draw of the 12 cells at $\rho \le 1$ for both rules (R6).

**Result 7 (the estimate of D, and the error control of the primary).** (a) $E[\hat D] \ge D$: the estimate is biased upwards. (b) Suppose $D > 0.07$. Then the ship condition "the 97.5% percentile interval of $D^\*$ lies at or below 0.07" holds with probability at most about 0.0125.

*Proof.* (a) $h(x) = \max_j\lvert x_j - 0.8\rvert$ is convex (a maximum of convex functions), so by Jensen's inequality $E[h(\hat C)] \ge h(E[\hat C]) = h(C) = D$, with $E[\hat C_j] = C_j$ because (E4) is a mean of unbiased cell means. (b) Let $j^\*$ have $\lvert C_{j^\*} - 0.8\rvert = D > 0.07$. In every replicate $D^\* \ge \lvert C^\*_{j^\*} - 0.8\rvert$, so the 0.9875 quantile of $D^\*$ is at least that of $\lvert C^\*_{j^\*} - 0.8\rvert$ (quantiles are monotone under pointwise order). Ship therefore implies that the one-cell percentile bound $u = q_{0.9875}(\lvert C^\*_{j^\*} - 0.8\rvert)$ is at most 0.07, below $\lvert C_{j^\*} - 0.8\rvert$. $x \mapsto \lvert x - 0.8\rvert$ is differentiable at $C_{j^\*} \ne 0.8$, and $\hat C_{j^\*}$ is the equal-weighted mean of 18 independent cell means of 200 independent bounded players each, asymptotically normal; the percentile bootstrap of a smooth function of a mean is first-order correct, so $P(u < \lvert C_{j^\*} - 0.8\rvert) \to 1 - 0.9875 = 0.0125$. $\blacksquare$

The bound is asymptotic. The R7 simulation runs the real procedure (20 coverages, one at 0.73 so $D = 0.07$ exactly, 3,600 one-interval players each, a 1,000-replicate percentile bootstrap) and finds a false-ship rate of 0.010 in 2,000 runs. (a) also means the 97.5% interval of $D^\*$ is not a confidence interval for $D$ when several cells are near the maximum: at a true $D$ of 0 the simulated $E[\hat D]$ is 0.0145, about two standard errors of one coverage. With five cells at the maximum the simulated bias is 0.0086, matching $\operatorname{sd} \times E[\max \text{ of 5 normals}] = 1.16\,\operatorname{sd}$; with all 20 at 0.80 it is 0.0145, matching the expected maximum of 20 half-normals. The error is in the conservative direction for a ship. Checked by R7.

**Result 8 (the decision rule needs no multiplicity correction).** Ship needs every one of six conditions. If any one null is true (the primary above 0.07, the challenger no better, or a guard beyond its margin), the ship probability is at most that condition's own level: about 0.0125 for the primary (Result 7), 0.0125 for $\Delta D$, and 0.10 for each guard (the upper end of a two-sided 80% interval is a one-sided 90% bound).

*Proof.* This is the intersection-union test (Berger 1982). $\{\text{ship}\} \subseteq \{\text{condition } i \text{ passes}\}$ for every $i$, so $P(\text{ship}) \le P(\text{condition } i \text{ passes})$, which is at most $\alpha_i$ under null $i$. No union bound enters, so no correction is needed; the other conditions can only lower the probability. The one-sided levels: the 97.5% interval's upper end is the $0.5 + 0.975/2 = 0.9875$ quantile, the 80% interval's the 0.9 quantile. $\blacksquare$

That is section 8's "which only makes a ship harder, so no correction applies", as a theorem. The size against the composite null is the largest of the components' levels, 0.10, not 0.0125. The profile levels are asymptotic, for the same reason as Result 7(b); R8 simulates the false-credible guard at a true difference of exactly 0.01 (9,600 players, an 80% percentile bootstrap) and it passes 0.097 of the time. The $\rho = 1$ guard has the kink of $\lvert\cdot\rvert$ near the challenger's 0.804; there $\lvert\hat C - 0.8\rvert$ is biased upwards (Jensen), which makes the guard conservative. Recall at 2 is not asymptotic: it is Newcombe's interval on 45 players, and its exact level at the margin is above 0.10 (Finding 4). Reject is a union (any guard wholly beyond its margin rejects), so its false-reject rate near the margins can reach about the sum of the levels; at the measured values every guard is far from rejecting. Checked by R8.

**Result 9 (the paired bootstrap).** Conditional on the data, the variance of a replicate of a paired difference of (E4) is exactly $\frac{1}{S^2}\sum_s\hat\sigma^2_{s,\text{diff}}/n_s$, with $\hat\sigma^2_{s,\text{diff}}$ the divisor-$n_s$ variance over the cell's players of the difference of their two shares. It is below the unpaired $\frac{1}{S^2}\sum_s(\hat\sigma^2_{s,1} + \hat\sigma^2_{s,2})/n_s$ whenever the two rules' player shares are positively correlated within cells.

*Proof.* Within cell $s$ the same $n_s$ indices are drawn for both rules, so the replicate's difference is the cell mean of $n_s$ i.i.d. draws of the player differences from their empirical distribution, with variance $\hat\sigma^2_{s,\text{diff}}/n_s$. Cells are drawn independently and enter with weight $1/S$. Last, $\hat\sigma^2_{\text{diff}} = \hat\sigma_1^2 + \hat\sigma_2^2 - 2\widehat{\operatorname{cov}}$. $\blacksquare$

For the false-credible guard the replay gives the plug-in SD 0.000278 against the bootstrap's 0.000290 (4% apart, within Monte Carlo error), and the unpaired SD would be 2.37 times larger. Pairing is why a margin of 0.01 against a difference of 0.009 is decidable with 9,600 players. Checked by R9.

**Result 10 (the Tier 1 guards).** (a) With no discordant players ($n_{10} = n_{01} = 0$), (E10) is $\pm 2z^2\lvert\tfrac12 - \hat p\rvert/(N + z^2)$: 28 of 45 on both sides gives $\pm 0.008607$. (b) With 28 reference hits of 45 and no gains, recall's guard holds at 2 losses ($-0.084$) and fails at 3 ($-0.114$). (c) At no true change, with each player's reference outcome Bernoulli($\pi$) and the candidate's outcome flipped from the reference's with probability 0.05, the guard's pass probability is an exact finite sum over the $\binom{N+3}{3}$ tables: 0.874 for recall at 2 (45 players, $\pi = 0.63$) and 0.485, 0.676, 0.820 and 0.886 for the clean alarm at 23, 40, 60 and 80 players ($\pi = 0.2$).

*Proof.* (a) $n_{10} = n_{01} = 0$ gives $\hat p_1 = \hat p_2 = \hat p$, $\hat d = 0$, and $\hat\phi = n_{11}n_{00}/(n_{11}n_{00}) = 1$, so with $A = \hat p - l$, $B = u - \hat p$ the lower half-width is $\sqrt{A^2 - 2AB + B^2} = \lvert A - B\rvert$, and the upper is the same. $A - B = 2\hat p - (l + u)$, and Wilson's centre is $(l+u)/2 = (\hat p + z^2/(2N))/(1 + z^2/N)$, which minus $\hat p$ is $z^2(\tfrac12 - \hat p)/(N + z^2)$. (b) Direct evaluation of (E10) at $(n_{11}, n_{10}, n_{01}, n_{00}) = (28 - L, 0, L, 17)$. (c) Under the flip model the four cells have probabilities $\pi(0.95)$, $(1-\pi)0.05$, $\pi(0.05)$, $(1-\pi)0.95$ and the table is multinomial; the guard is a deterministic function of the table. $\blacksquare$

The recall guard's interval at 80 games, $[-0.0086, +0.0086]$, is pure Wilson asymmetry: no player changed. `dev/power.json`'s simulated 0.8755 and 0.4883 are reproduced from their seed and agree with the exact 0.8744 and 0.4851 within Monte Carlo error; the quoted 0.68, 0.81 and 0.88 at 40, 60 and 80 players agree with the exact values within the error of 4,000 simulations. "It holds with up to 2 net losses" holds for losses without gains; with more discordant players a net loss of 2 can fail (Finding 3). Checked by R10 and N8.

**Result 11 (the secondary's intervals).** (a) The Bonferroni intervals of (E8) hold all 20 true coverages at once with probability at least about 0.95, and those of (E9) all four #13 checkpoints. (b) Pooling the #13 cell's two dispersion halves into one Wilson interval is conservative: the pooled binomial variance exceeds the true one by $n_1n_2(p_1 - p_2)^2/N \ge 0$. (c) At a coverage of 0.80 and 1,600 players, $z_4\sqrt{0.16/1600} = 0.025$.

*Proof.* (a) Union bound: $P(\text{some interval misses}) \le \sum_i P(\text{interval } i \text{ misses}) \le m \cdot 0.05/m$ for $m = 20$ or 4. (b) The pooled count is the sum of two independent binomials, with variance $n_1p_1(1-p_1) + n_2p_2(1-p_2)$; Wilson assumes $N\bar p(1 - \bar p)$, and the difference is the identity checked in R11 (a weighted Jensen gap for the concave $p(1-p)$). (c) Arithmetic. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.bonferroni`, with $A_i$ the event that interval $i$ misses. Its hypothesis $P(A_i) \le \alpha/m$ holds only approximately: (E8) is a normal interval on a bootstrap SD, and (E9) is Wilson's, both with coverage $1 - 0.05/m$ in the limit. The #13 cell's 30-game interval [0.738, 0.791] is "not within" because its lower end is 0.012 below 0.75; its upper end is 0.009 below 0.80. The other three checkpoints are within. Checked by R11 (identity; simulated pooled coverage 0.9878 against 0.9875).

**Result 12 (the churn's Jeffreys intervals).** (E12) is the equal-tailed 95% posterior interval for a binomial rate under the $\operatorname{Beta}(\tfrac12, \tfrac12)$ prior, with the end at 0 or 1 fixed when $x = 0$ or $x = n$. At 0 of 28 it is $[0, 0.085]$.

*Proof.* The prior kernel $t^{-1/2}(1-t)^{-1/2}$ times the binomial kernel $t^x(1-t)^{n-x}$ is $t^{x + 1/2 - 1}(1-t)^{n - x + 1/2 - 1}$, the $\operatorname{Beta}(x + \tfrac12, n - x + \tfrac12)$ kernel; both are densities, so the constants agree. The 2.5% and 97.5% quantiles bound 95% of its mass. $\blacksquare$

Machine-checked: `lab/lean/Rematch/BetaBinomial.lean` theorem `Rematch.beta_binomial_posterior`, with $a = b = \tfrac12 > 0$ and natural $x \le n$, which match. As a frequentist interval its exact coverage at $n = 28$ averages 0.950 over $p \in [0.01, 0.5]$ and dips to 0.890 (R12). It describes the churn; it decides nothing.

**Result 13 (the power of the rule, from the pilot).** (a) In the normal limit, with $\Delta$ the true false-credible difference and $\operatorname{se}$ its SD, the guard holds with probability $\Phi((0.01 - \Delta)/\operatorname{se} - z_{0.9})$. At the pilot's $\Delta = 0.009276$ and $\operatorname{se} = 0.000291$ that is 0.886; the other profile conditions hold with probability near 1 (the primary is $(0.07 - 0.038)/0.0056 = 5.7$ SDs inside the tolerance, 3.5 beyond what the rule's $z_{0.9875}$ needs). The pilot's replicate share, 0.872, is the same quantity computed on the replicates themselves. (b) The profile part and the Tier 1 part use disjoint data, so their pass events are independent and the whole rule's power is about $0.872 \times 0.8755 \times 0.9 = 0.687$. The 0.9 is section 6's assumption for the smoke CI rule ("about 0.9 by EXP-0001's measured false-alarm rate of the recall tolerance"), not simulated. Recall at 2 and the smoke CI rule share the smoke suite's 15 flawed players and both read recall at 2, so they are positively dependent and their product understates their joint pass rate: 0.687 is about a lower bound. (c) At 200 players a cell instead of 800, $\operatorname{se}$ doubles: the guard holds with probability 0.485 at the pilot's difference (0.009276) and 0.72 at the development's 0.0089.

*Proof.* (a) The guard holds when $\hat\Delta + z_{0.9}\operatorname{se} \le 0.01$, with $\hat\Delta \sim N(\Delta, \operatorname{se}^2)$. (b) Independent events multiply; for the dependent pair, $P(A \cap B) \ge P(A)P(B)$ when $A$ and $B$ are positively associated. (c) Each of the 12 cells' variance scales as $1/n_s$ (Result 9), so the SD scales as $\sqrt{800/200} = 2$. $\blacksquare$

The replicates' 0.872 and the normal 0.886 differ by 1.9 binomial standard errors of 2,000 replicates, within Monte Carlo error. The section 6 sensitivity sentence about the primary at 50 players a cell does not follow from this formula (Finding 1). Checked by N5 and N8.

## 5. Numbers

`math_check.py` replays the 32 registered cells (seeds `EXP-0006/players/<band>/<rho>/<disp>` and `issue13/<disp>`) through (E1) to (E3) with scipy's Beta quantiles, draws the paired bootstrap from `EXP-0006/bootstrap/paired`, and reproduces every profile number in `results.json` to six decimals (N1 to N3). It does the same for `dev/power.json` from `EXP-0006-power`, including the 0.872 ship share and the seeded Tier 1 simulations (N5, R10). The Tier 1 numbers are recomputed from the committed player rows with an independent (E10) and (E12) (N6). The rule (E11) is re-applied to the recomputed statistics (N4).

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| decision ship; every guard holds | (E11) | `decision`, `guards.holds` | ship |
| worst 0.828 (W5 at 30), D(challenger) 0.028 [0.022, 0.043] | (E6), Result 7 | `primary` | 0.028333 [0.022222, 0.042503] |
| champion 0.103 (W4.missed at 30), D 0.697; ΔD 0.668 [0.650, 0.679] | (E6) | `per_rule.champion`, `delta_D` | 0.696667; 0.668333 [0.649722, 0.679167] |
| the 40 coverages of the decision's table | (E4) | `per_rule.*.cells[].coverage` | all at 3 dp |
| challenger 20 within; champion 2 within, 13 outside; EXP-0002 2 and 12; W4.missed 0.10 | (E8) | `per_rule.*.within/outside`; EXP-0002 `primary.cells` | as quoted |
| truth above the range at 30 games 0.13 to 0.17 (champion 0.33, 0.90, 0.81, 0.27) | Result 5 | `cells[].true_rate_above` | 0.131 to 0.173; 0.33, 0.90, 0.81, 0.27 |
| $\rho = 1$: 0.874 and 0.804 (distances 0.074, 0.004), −0.070 [−0.071, −0.069] | (E7) | `guards.rho1_deviation` | 0.874292, 0.804427, −0.069865 [−0.071074, −0.068740] |
| per key at $\rho = 1$: W4.missed 0.942, W7.traps 0.931; challenger 0.787 to 0.827 | (E4) | `rho1_deviation.per_key` | as quoted |
| false credible 0.0249 to 0.0338, +0.0089 [+0.0086, +0.0093], 0.0007 to spare, one in about 110 | (E7) | `guards.false_credible` | 0.024880, 0.033792, +0.008911 [0.008552, 0.009287]; 0.000713; 1/112 |
| W4.missed 0.006 to 0.029; W7.traps 0.001 to 0.017, one in 59 against one in 740; W1 and W9b fall | (E4) | `false_credible.per_key` | 1/58.9, 1/738.6 |
| $\rho = 0.5$ coverage 0.800 (champion 0.783) | (E4) | `exploratory.rho_half_coverage` | 0.800031, 0.783396 |
| #13: 0.766 [0.738, 0.791], 0.787, 0.802, 0.786; champion 0.523, 0.639, 0.716, 0.783 | (E9) | `exploratory.issue13` | as quoted; 1,600 players each |
| #13 not fixed; W1's four within; all 20 within | Result 11 | `fixed` | as quoted |
| recall at 2: 28 of 45 both, 0 [−0.009, +0.009] | (E10), Result 10(a) | `guards.recall_at_2` | ±0.008607 |
| clean alarm 6 against 5 of 23, 0.043 [−0.017, 0.109] | (E10) | `reported.clean_alarm` | 0.043478 [−0.016791, 0.109195] |
| churn 0 [0, 0.085] at 80 of 28 hits; 1 of 22 and 1 of 30 at 45 and 60; none at 30, 80, 120 | (E12) | `exploratory.churn.by_checkpoint` | as quoted |
| misses at 80: miss_mate 9 of 9, opening_blunder 7 of 9, no_convert 1 of 9 | count | `churn.per_flaw_at_80` | as quoted |
| rank 1 changes 5 of 32, 3 of 16, 4 of 20 at 30; 1, 0, 2 at 80 | count | `tier1.players` | as quoted |
| smoke: coverage 0.65 to 0.80; recall 0.6, top-1 0.643, plan 0.6, top-2 0.927; no CI failure | `bench::results::regressions` | `tier1.scorecards.smoke` | as quoted |
| pilot coverage 0.58 to 0.78; one new clean false alarm | – | `tier1.scorecards.pilot` | as quoted |
| 4,245 moved intervals; 800 fields, 0 mismatches | count | `tier1.positive_control_moved_estimates`, `between_runs` | 1,120 + 2,233 + 892 |
| 118,400 computations; 6,720 games; 3,600, 9,600 and 1,600 players | design | `compute.actual` | $2 \cdot 4 \cdot 14{,}800$; $32 \cdot 120 + 16 \cdot 80 + 20 \cdot 80$ |
| 3,254 s at about 6.5 cores, about 6 CPU-hours | arithmetic | prose (not in `results.json`) | 5.9 |
| section 5: 0.02 resolution; one more false weakness in 20 reports | EXP-0002 Result 10; Result 6 | – | 0.0202; 0.05 |
| section 5: recall holds at 2 losses, fails at 3 | Result 10(b) | – | −0.0837, −0.1136 |
| section 6: pilot D 0.706 and 0.038 (W1 at 30, 0.762), 0.071 smaller, 0.0093 higher, SE 0.0003, ships 0.87, rejects in none | (E13) | `dev/power.json` | reproduced from `EXP-0006-power` |
| section 6: recall holds 0.88; clean alarm 0.49 (0.68, 0.81, 0.88 at 40, 60, 80) | Result 10(c) | `tier1_no_change` | exact 0.874; 0.485, 0.676, 0.820, 0.886 |
| section 6: whole rule ≈ 0.69; deviation 3's 0.37 | Result 13(b) | `ship_power_approx` | 0.687 (the 0.9 assumed; about a lower bound); $0.872 \times 0.8755 \times 0.48825 = 0.3727$ |
| section 6: clean alarm 0.68, 0.81, 0.88 at 40, 60, 80 | Result 10(c) | not in `dev/power.json`; from a simulation `power.py` does not contain | exact 0.676, 0.820, 0.886 |
| development: 180,000 computations; ΔD about 0.64 | design | `dev/dev-results.json` | 15 rules × 30 cells × 100 × 4; 0.6928 − 0.050 |
| section 6: false-credible interval about 0.001 wide; about one half at 200 a cell | Result 13(a, c) | – | 0.00075; 0.485 at the pilot's 0.009276 (0.72 at the development's 0.0089) |
| section 6: #13 half-width about 0.025 | Result 11(c) | – | 0.0250 |

> **Finding 1 (section 6, the primary's sensitivity).** Section 6 says "the primary alone needs about 50 players a cell at a true D of 0.05 against the tolerance 0.07 (a standard error of about 0.008 at 200, 0.016 at 50)". By Result 13(a)'s argument, the primary passes when $\hat D + z_{0.9875}\operatorname{se} \le 0.07$, with probability $\Phi(0.02/\operatorname{se} - 2.2414)$: 0.16 at $\operatorname{se} = 0.016$ (50 a cell) and 0.60 at 0.008 (200 a cell). 80% power at a true D of 0.05 needs $\operatorname{se} \le 0.02/(2.2414 + 0.8416) = 0.0065$, about 300 a cell at the sentence's own standard error. The pilot's measured SD of $D^\*$ at 200 a cell is 0.0056, smaller than the sentence's 0.008: it gives 0.91 at 200 and 0.33 at 50, and 80% power needs about 150 a cell. Every figure here is optimistic, since it treats $\hat D$ as unbiased and Result 7(a) biases it upwards when several cells are near the maximum. The run's sizes and its result (D = 0.028) are unaffected: the guards, not the primary, set the sizes, and the primary passed with a 97.5% upper end of 0.043.

> **Finding 2 (section 2, the clamp's widening).** Section 2 says the dispersion clamp "widens on average even without overdispersion, most at 30 games and for keys with one chance a game". The first half holds (Result 2(d)): the mean $\varphi$ falls from 30 to 120 games for W1, W4.missed and W9b. The second does not. At 30 games without dispersion the widening is largest for W9b (1.172, about 10 games with a chance) and smallest for W7.traps (1.035), the key with exactly one chance a game, whose $\varphi$ is nearly the constant $G/(G-1)$ (Result 2(c)). What sets it is the number of games with a chance, not the chances a game. It moves no number in the decision; it does change which keys the report's widening check should look at.

> **Finding 4 (section 5, the recall guard's level).** Section 5 calls recall's guard "a tripwire for gross losses, not a test of non-inferiority at 0.10", and Result 8 gives each guard a level of 0.10. Under the flip model of section 6 (reference hit rate 0.63, gain 0.05, the loss rate set so the true difference is exactly $-0.10$), the exact probability that the 80% Newcombe lower end is at least $-0.10$ is 0.149, not 0.10 (R8(b)): Newcombe's interval on 45 players is anti-conservative there. The pre-registration's "tripwire" wording already disclaims a 0.10 test, so nothing changes, but the guard's size is 0.15 under that model.

> **Finding 3 (section 5, the recall tripwire).** Section 5 says that with 45 players and 28 reference hits the recall guard "holds with up to 2 net losses and fails at 3 losses with no gains". Without gains this is exact (Result 10(b)). With gains it is not a property of the net loss: 3 lost and 1 gained holds, but only just ($-0.0999$, a near-tie that a different $z$ or rounding would flip); 4 lost and 2 gained fails ($-0.1124$), and so does a net loss of 0 at 8 lost and 8 gained ($-0.1122$), because Newcombe's interval widens with the discordant players. The run had no discordant player at 80 games.

## 6. Assumptions that can fail

- **The replay is the production code: every profile number.** Shown, not assumed: N1 to N3 reproduce every committed profile statistic, and N5 the power pilot. A few numbers in this file are not in `results.json` and rest on the replay alone: the low-side miss shares (0.044 to 0.061, and 0.120, 0.089, 0.059, 0.086), $\varphi$, $\lambda'$ and the variance ratios. They come from the same replayed intervals that reproduce every committed coverage, so the risk is small. It is tied to numpy's generator streams and to scipy's Beta quantile; a version that changes either would break N1 without the experiment being wrong.
- **The generator: Results 1 to 6.** Chances a game, peer rates and one source of dispersion (a within-game Beta at $D = 1.5$) are EXP-0002's assumptions. The challenger estimates its own dispersion, so it is tuned to a model in which $\varphi$ can see the extra variance: game-to-game variation of the rate. Variation between weeks or opponents would not show in a per-game Pearson statistic and would leave Result 4's $\bar D/\varphi$ above 1. No sensitivity varies the generator; the Tier 1 guards on engine-generated players are the only check that does not share it, and they read coverage on 15 to 30 players.
- **$\theta$ fixed: Results 3 and 5.** The cap's exact bias $2/(g+2)(\mu - \theta)$ needs a constant rate. A player who improves has a weighted rate behind the present one, which the challenger weights less than the champion did (Result 1), so drift hurts it less, but nothing here measures that.
- **The cap binds: Result 3.** It needs $\tilde n \le n_0g/2$, true here except for 3 W9b draws of 217,600. A key with more than $n_0/2$ chances a game (W1 above 150, a W9b-like key above 20) would leave the prior uncapped and $n_0$-strong; no current key is near that.
- **Normal and bootstrap approximations: Results 5, 7, 8, 11, 13.** Result 5's error is measured on the grid (0.030 at most on the primary), not bounded; it is largest where $G_c$ is small. Results 7 and 8 give asymptotic levels; R7's simulation of the real procedure gives 0.010 against 0.0125. The decision is far from the edges: the primary's upper end is 0.043 against 0.07, $\Delta D$'s lower end 0.650 against 0. The false-credible guard is not: its estimate is 0.0089 against 0.01, 3.8 of its SDs, and its upper end 0.0093 is 2.5 SDs from the margin, so its 0.10 level is the one that matters, and a small bias in the bootstrap would matter there first. EXP-0013 carries it.
- **Players independent: Results 9 to 11.** True in the simulation. In the Tier 1 run each player's outcomes come from one engine run; the paired tables assume players are independent of each other, which the bench's per-player seeds give.
- **The flip model of the Tier 1 power: Result 10(c).** The 5% discordance each way is an assumption; the run had no discordant player at 80 games among the 45, so the true discordance is likely lower and the guard's pass rate higher.
- **The two rules ran on one build.** The production `algo-2.0.0` was checked against the tested variant by the result review (`review.md` part two, concern 2), not here.

### Verification

**Blind re-derivation.** A fresh agent was given the pre-registration, `analysis.py`, `power.py`, `dev/`, `results.json`, EXP-0002's and EXP-0001's `analysis.py` and the production estimate, but not this file. It wrote its own generator and both rules (1,000 players a cell) and reproduced the committed coverages within Monte Carlo error. It re-derived five results independently:

- the cap's bound $\lambda' \le 2/(g+2)$, attained when $\tilde n/g \le n_0/2$, and the exact bias $2(\mu - \theta)/(g+2)$ (Result 3), with the same binding table;
- Kish's variance, the design effects 1.5096 and 1.516, the challenger's variance ratio $(g/(g+2))\bar D/\varphi$ (Result 4, the same as $(1-\lambda')\bar D/\varphi$), and the champion's $r\bar D(1-\lambda)$;
- the clamp's behaviour, with the same means per key and the same mechanism for one-chance games ($\varphi \approx G/(G-1)$);
- the error control of the primary (0.0125, an intersection-union test, $\hat D$ biased upwards; at a true D of 0 the bias is 0.0145) and the guards' 0.10 one-sided levels (Results 7 and 8);
- the all-concordant Newcombe half-width $z^2\lvert 2\hat p - 1\rvert/(N + z^2) = 0.008607$ and the table of losses and gains (Result 10);
- the power figures 0.886, 0.687 and 0.485 (Result 13).

Every formula agreed. It found Findings 1, 2 and 3 independently: "about 50 players a cell" is off by 3 to 6 times (its 147, 211 and 304 bracket this file's 150 and 300); W7.traps widens least and W9b most; and a net loss of 2 fails at 4 lost and 2 gained ($-0.1124$). It also noted that the reject branch is a union whose false-reject rate can reach about the sum of the levels (now in Result 8), that power.json is from an older `power.py` (the decision's deviation 3), and that Newcombe's published method 10 may use a continuity-corrected $\hat\phi$. That last point is about EXP-0001's imported `newcombe_paired`, not this experiment, and it is not settled here: this file and `math_check.py` describe the interval as the code computes it.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee and ran the script. It confirmed that the replay is an independent reproduction (nothing is imported from `analysis.py`) and that the two Lean citations match their hypotheses. It raised 29 points; these changed the file:

1. **Major.** Finding 2's mechanism was wrong: the number of games with a chance alone does not explain the ordering (W1 and W7.traps both have 30). The text now gives the two mechanisms (binary counts give $G_c/(G_c - 1)$; multi-chance games give the $\chi^2$ excess, which falls strictly in $\nu$), and R2 checks each against its prediction, as well as W7.traps's flat profile across checkpoints.
2. **Major.** Result 13(b) is now "about", with the 0.9 named as section 6's assumption and 0.687 called about a lower bound, since recall at 2 and the smoke CI rule share the smoke players.
3. **Major.** Result 8's levels are now simulated: the false-credible guard at its margin passes 0.097 of the time. The recall guard's exact level at its margin is 0.149, which is Finding 4.
4. **Major.** R7's bias check could not fail with one cell at the maximum. It now uses five cells and compares with the expected maximum of five normals, and the 0.010 and 0.0145 are asserted against closed forms.
5. **Minor.** Result 5's $\lvert b\rvert/\sigma$ was off by $(g+2)/g$ and is corrected with a sympy check. A sign typo in its proof is fixed, and W5 is now "both wide and biased".
6. **Minor.** Result 4's ratio is now measured on the replay (within 0.096). Result 6's report claim is scoped to the checkpoint average, and its comparison uses the guard's own scope ($\rho \le 1$: 0.120 and 0.089).
7. **Minor.** Result 13(c) quotes 0.485 at the pilot's unrounded difference. The 0.872 against 0.886 gap is 1.9 SE, within Monte Carlo error, and the primary's slack is 3.5 SD beyond $z_{0.9875}$. Finding 1 now gives about 150 a cell at the pilot's SD and says its figures are optimistic.
8. **Minor.** Result 2 gains (f), the use of (a), and the statement that the upper clamp never binds. Result 3 states $\tilde n > 0$. Results 7, 10 and 11 are reworded.
9. **Minor.** Checks were tightened:
   - R11 now computes exact coverage by convolution.
   - R12, R2, R9 and N8 assert their stated values at the quoted rounding.
   - The development figures are read from `dev/dev-results.json`, and the Tier 1 power figures from `dev/power.json`.
   - N4 uses the rebuilt recall table and the #13 cell checkpoint by checkpoint.
   - New checks cover `bench-hang`, `exp1-clean-0`, the positive control's parts, the budget, the 180,000 development computations and the Lean theorems' presence.
   - Section 5 says that 0.68, 0.81 and 0.88 are not reproducible from `power.py`.
10. **Not changed.** These remain unchecked, as `results.json` does not hold them: the run time, the engines and nodes, the byte-identical second run, and the smoke scorecard's equality with the `bench/baseline.json` of the time (since rebuilt for `algo-2.0.0`).
