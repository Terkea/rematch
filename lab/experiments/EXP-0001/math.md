# EXP-0001: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic checks (prefix `R`), simulations from a known truth, and recomputation of every quoted number (prefix `N`).

EXP-0001 is a sizing study. It tests nothing; it computes a simulated power under an assumed model. So the maths has to show three things: that the simulation implements the model the pre-registration states, that the quantities it reports mean what the decision says they mean, and that the shortcuts the decision takes in words (the calibration, the tolerances, the feasibility rule) hold, or by how much they fail.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $K = 5$ | flaws (`hang`, `miss_mate`, `opening_blunder`, `low_clock_blunder`, `no_convert`) | flaws | `analysis.py` `FLAWS` |
| $f$ | a flaw, $f = 1, \dots, K$ | – | – |
| $m$ | players per flaw in a simulated suite | players | `M_GRID`, `at_budget[].players_per_flaw` |
| $N = Km$ | flawed players in a suite (the $n$ of `analysis.py`) | players | `flawed_players` |
| $G$ | games per player, $G \in \{30, 45, 60, 80, 120\}$ | games | `CHECKPOINTS` |
| $n_f = 6$ | pilot players of flaw $f$ | players | `SEEDS_PER_FLAW` |
| $h_f$ | pilot players of flaw $f$ whose target is in the top 2 at $G$ | players | `observed.recall_at_2[].per_flaw` |
| $\pi_f$ | probability a flaw-$f$ player's target is in the top 2 at $G$ (reference version) | probability | drawn in `simulate_recall` |
| $\bar\pi = \frac1K \sum_f \pi_f$ | the mean hit probability | probability | `pibar` |
| $\delta$ | the true change in recall at 2, candidate minus reference | recall | `DELTA` = 0.1, `DELTA_GRID` |
| $b$ | churn: the share of reference hits the candidate loses ($\delta \ge 0$), or gains among misses ($\delta < 0$) | probability | `CHURN`, `PRIMARY_CHURN` = 0.10 |
| $a$, $\ell$ | the candidate's gain rate (miss to hit) and loss rate (hit to miss) | probability | `gain`, `lose` |
| $X_i$, $Y_i$ | 1 if player $i$'s target is in the top 2 under the reference, the candidate | – | `ref`, `cand` |
| $n_{11}, n_{10}, n_{01}, n_{00}$ | players with $(Y, X) = (1,1), (1,0), (0,1), (0,0)$: $n_{10}$ are gains, $n_{01}$ losses | players | `simulate_recall` |
| $p_{jk} = E[n_{jk}]/N$, $p_1 = p_{11} + p_{10}$, $p_2 = p_{11} + p_{01}$, $q_i = 1 - p_i$ | cell and marginal probabilities | probability | – |
| $\hat D = (n_{10} - n_{01})/N$ | the paired change in recall at 2 | recall | `mean_difference` |
| $L = n_{01} - n_{10}$ | the net number of lost detections | players | `drop` $= L/N$ |
| $z_\gamma = \Phi^{-1}(\tfrac12 + \tfrac\gamma2)$ | the normal quantile of a two-sided level $\gamma$; $z_{0.975} = 2.2414$, $z_{0.80} = 1.2816$ | – | `z_of` |
| $S$ | Monte Carlo simulations: 4,000 (recall), 2,000 (coverage) | simulations | `SIMS_RECALL`, `SIMS_COVERAGE` |
| $k^\*$, $\tau$ | the guard's smallest flagged net loss, and the recall tolerance $(k^\* - \tfrac12)/N$ rounded to 3 decimals | players, recall | `guard_k_lost`, `recall_tolerance` |
| $H$ | reference hits used to set $k^\*$: $\operatorname{round}(\text{pilot recall at } G \times N)$ | players | `reference_hits` |
| $r_j$ | checkpoint intervals of player $j$ up to $G$ (4 to 80 games, 5 to 120) | intervals | `coverage_vectors` |
| $v_{jt}$ | 1 if player $j$'s $t$-th target interval holds the true rate | – | `checkpoints[].covered` |
| $c_j = \frac{1}{r_j}\sum_t v_{jt}$ | player $j$'s coverage | share | – |
| $\hat C = \frac1N \sum_j c_j$, $\bar c = \sum_j r_j c_j / \sum_j r_j$ | mean of player coverages; pooled share | share | `simulate_coverage` `est`, `cbar` |
| $s_f^2$ | the variance (divisor $n_f$) of the pilot's $c_j$ within flaw $f$ | share$^2$ | – |
| $\Delta_c = 0.05$, $g$ | coverage change; coverage gain rate | share, probability | `DELTA_COVERAGE`, `gain` |

The population of interest is the synthetic players the bench generator produces at the smoke rates (`bench::flaws`, rating 1500, `bench::suite::RATING`). The pilot is a sample of six per flaw from it.

## 2. Model

**(M1) The pilot.** Given $\pi_f$, the pilot's six players of flaw $f$ are independent: $h_f \mid \pi_f \sim \operatorname{Binomial}(n_f, \pi_f)$. The prior is Jeffreys', $\pi_f \sim \operatorname{Beta}(\tfrac12, \tfrac12)$, independently over $f$. A flaw's six players come from distinct seeds of a deterministic pipeline (`pilot_suite`), so the independence is over seeds, not over runs.

**(M2) A simulated paired comparison** (`simulate_recall`). Draw $\pi_f$ from the posterior of (M1) independently for each $f$. Given $\pi$, for each flaw $f$ and player $j = 1, \dots, m$, independently:

```math
X_{fj} \sim \operatorname{Bernoulli}(\pi_f), \qquad
P(Y_{fj} = 0 \mid X_{fj} = 1) = \ell, \qquad P(Y_{fj} = 1 \mid X_{fj} = 0) = a,
```

with the rates set from $\bar\pi$ by

```math
\delta \ge 0:\ \ \ell = b,\ \ a = \frac{\delta + b\bar\pi}{1 - \bar\pi}; \qquad
\delta < 0:\ \ a = b,\ \ \ell = \frac{|\delta| + b(1 - \bar\pi)}{\bar\pi}.
```

A draw with $a > 1$ or $\ell > 1$ is *infeasible*; it counts as neither shipped nor flagged (`feasible`).

**(M3) The deployed CI rule** (`bench::results::regressions`, `backend/crates/bench/src/results.rs`). The same seeds are run under both versions, so the stored baseline and the new run are the reference and candidate of one paired comparison. Recall at 2 fails when `now < was - tol - 1e-9`, that is when $L/N > \tau + 10^{-9}$. Coverage fails when $|c' - 0.8| > |c - 0.8| + \tau_c + 10^{-9}$.

**(M4) Coverage resampling** (`simulate_coverage`). A suite of one version is $m$ players drawn with replacement, independently, from each flaw's six pilot players; a player brings its whole vector $(v_{jt})_t$. The pilot's players are treated as the population.

**(M5) The candidate's coverage** (`simulate_coverage` and the tolerance block of `main`). Given a resampled suite, the candidate turns a covered interval into a miss with probability $b$ and a miss into a covered interval with probability $g = (\Delta_c + b\bar c)/(1 - \bar c)$ ($\Delta_c = 0$ for the tolerance). *Interval flips*: independently for each interval. *Player flips*: two uniform draws per player, so all of a player's hits are lost together and all of its misses gained together.

## 3. Estimators

- **(E1) The posterior draw** (`simulate_recall`, `rng.beta(h + 0.5, n - h + 0.5)`): $\pi_f \sim \operatorname{Beta}(h_f + \tfrac12, n_f - h_f + \tfrac12)$.
- **(E2) The rates** (`simulate_recall`, `gain` and `lose`): (M2)'s formulas.
- **(E3) The Wilson interval** (`wilson`): for $x$ successes of $n$, $\hat p = x/n$,

```math
\frac{\hat p + \frac{z^2}{2n} \pm z\sqrt{\frac{\hat p(1-\hat p)}{n} + \frac{z^2}{4n^2}}}{1 + \frac{z^2}{n}}.
```

- **(E4) Newcombe's method 10** (`newcombe_paired`): with $(l_i, u_i)$ the Wilson interval of $\hat p_i$ and $\hat\phi = (n_{11}n_{00} - n_{10}n_{01})/\sqrt{(n_{11}+n_{10})(n_{01}+n_{00})(n_{11}+n_{01})(n_{10}+n_{00})}$ (0 when the root is 0),

```math
\text{lo} = \hat D - \sqrt{(\hat p_1 - l_1)^2 - 2\hat\phi(\hat p_1 - l_1)(u_2 - \hat p_2) + (u_2 - \hat p_2)^2},
\quad
\text{hi} = \hat D + \sqrt{(u_1 - \hat p_1)^2 - 2\hat\phi(u_1 - \hat p_1)(\hat p_2 - l_2) + (\hat p_2 - l_2)^2}.
```

- **(E5) The ship rule** (`ship`): $\text{lo}_{0.975} > 0$ and feasible. **The guard** (`flagged`, ADR 0104): $\text{hi}_{0.80} < 0$ and feasible. **The CI rule** (`ci_flagged`): $L/N > \tau + 10^{-9}$ and feasible.
- **(E6) Assurance** (`ship` mean, `ship_power_delta_0_1`): $\hat A = \frac1S \sum_s \mathbf 1\{\text{ship in simulation } s\}$, and its 95% Monte Carlo interval is (E3) with $x = \hat A S$, $n = S$ (`mc_ci`).
- **(E7) The required size** (`required_m`): the smallest grid $m$ from which $\hat A \ge 0.80$ at every larger grid $m$.
- **(E8) The tolerance** (the `kstar` loop of `main`): $k^\*$ is the smallest $k \ge 1$ with $\text{hi}_{0.80}(n_{11} = H - k, n_{10} = 0, n_{01} = k, n_{00} = N - H) < 0$; $\tau = \operatorname{round}((k^\* - \tfrac12)/N, 3)$.
- **(E9) Coverage precision** (`simulate_coverage`, `half_width_95`): $1.959964 \times$ the standard deviation of $\hat C$ over $S$ resampled suites. **Paired coverage power** (`power_interval_flips`, `power_player_flips`): the share of suites with $\bar d - t_{0.9875, N-1}\, s_d/\sqrt N > 0$, $d_j = c'_j - c_j$.
- **(E10) The coverage tolerance** (`coverage_tolerance`): the 0.8 quantile (numpy's linear interpolation) of $|c' - 0.8| - |c - 0.8|$ over 2,000 smoke-sized suites under (M5) with player flips and $\Delta_c = 0$, rounded to 3 decimals.
- **(E11) Observed recall** (`main`, `observed.recall_at_2`): $\sum_f h_f / 30$ with its Wilson 95% interval. **Observed coverage**: $\hat C$ over the pilot, with a cluster bootstrap that resamples players within flaw (2,000 resamples, percentiles 2.5 and 97.5). **ICC** (`icc_oneway`): $(\text{MSB} - \text{MSW})/(\text{MSB} + (n_0 - 1)\text{MSW})$, $n_0 = (R - \sum_j r_j^2/R)/(k - 1)$, $R = \sum_j r_j$, over $k = 30$ players.
- **(E12) Budget sizes** (`smoke_m`, `full_budget_m`): the largest $m$ with $(Km + C)G \le B$, for $C$ clean controls and budget $B$.

## 4. Derivations and proofs

**Result 1 (Jeffreys posterior).** Under (M1), $\pi_f \mid h_f \sim \operatorname{Beta}(h_f + \tfrac12, n_f - h_f + \tfrac12)$, with posterior mean $(h_f + \tfrac12)/(n_f + 1)$. So (E1) draws from the exact posterior, and the reported power is averaged over it: it is an assurance, not a power at a point.

*Proof.* The Jeffreys prior is proportional to $\sqrt{I(p)}$. For one Bernoulli observation $y$, $\log f(y \mid p) = y\log p + (1-y)\log(1-p)$, and $\partial_p^2 \log f = -y/p^2 - (1-y)/(1-p)^2$. Taking $-E$ with $E[y] = p$ gives $I(p) = 1/p + 1/(1-p) = 1/(p(1-p))$, so the prior is $\propto p^{-1/2}(1-p)^{-1/2}$, the $\operatorname{Beta}(\tfrac12,\tfrac12)$ kernel. By Bayes' theorem the posterior is proportional to likelihood times prior:

```math
p^{h}(1-p)^{n-h} \cdot p^{-1/2}(1-p)^{-1/2} = p^{(h + 1/2) - 1}(1-p)^{(n - h + 1/2) - 1},
```

which is the $\operatorname{Beta}(h + \tfrac12, n - h + \tfrac12)$ kernel; the normalising constant is fixed because both are densities. For $\operatorname{Beta}(\alpha, \beta)$, $E[p] = B(\alpha + 1, \beta)/B(\alpha, \beta) = \frac{\Gamma(\alpha+1)\Gamma(\alpha+\beta)}{\Gamma(\alpha)\Gamma(\alpha+\beta+1)} = \frac{\alpha}{\alpha+\beta}$ by $\Gamma(x+1) = x\Gamma(x)$. With $\alpha = h + \tfrac12$, $\beta = n - h + \tfrac12$, $\alpha + \beta = n + 1$. $\blacksquare$

Machine-checked: `lab/lean/Rematch/BetaBinomial.lean` theorem `Rematch.beta_binomial_posterior`, with $a = b = \tfrac12 > 0$ and $k = h_f \le n_f$, which are its hypotheses. It proves the conjugacy and the posterior mean $(a + k)/(a + b + n)$. The Jeffreys step (the Fisher information) is not in Lean. Checked by `math_check.py` R1 (symbolic: the Fisher information, the kernel, the mean; simulation: $\pi \sim \operatorname{Beta}(\tfrac12,\tfrac12)$, $h \sim \operatorname{Bin}(6, \pi)$, the conditional mean and variance of $\pi$ for each $h$).

The prior matters here: three flaws sit at 0 of 6 or 6 of 6 at 80 games, where a flat-prior plug-in would make $\pi_f$ exactly 0 or 1 and remove all uncertainty from those flaws.

**Result 2 (the calibration is exact).** Under (M2), for every feasible draw, $E[\hat D \mid \pi] = \delta$ exactly, for $\delta \ge 0$ and $\delta < 0$, whatever the spread of the $\pi_f$.

*Proof.* For one player of flaw $f$, $Y - X = 1$ when a miss is gained, with probability $(1 - \pi_f)a$, and $-1$ when a hit is lost, with probability $\pi_f \ell$. So $E[Y - X] = (1 - \pi_f)a - \pi_f\ell$. With $m$ players in each flaw, $\hat D$ is the average of $N$ such terms, $m$ per flaw, and by linearity

```math
E[\hat D \mid \pi] = \frac1K \sum_f \big[(1 - \pi_f)a - \pi_f \ell\big] = a(1 - \bar\pi) - \ell\bar\pi,
```

because $a$ and $\ell$ do not depend on $f$. For $\delta \ge 0$ put $\ell = b$ and $a = (\delta + b\bar\pi)/(1 - \bar\pi)$: $a(1-\bar\pi) - b\bar\pi = \delta$. For $\delta < 0$ put $a = b$ and $\ell = (|\delta| + b(1-\bar\pi))/\bar\pi$: $b(1 - \bar\pi) - |\delta| - b(1 - \bar\pi) = -|\delta| = \delta$. Exactness needs equal $m$ in every flaw, which (M2) has. $\blacksquare$

Checked by R2 (symbolic, with five free $\pi_f$) and by simulation at $\pi$ fixed at the 80-game posterior means ($m = 22$, $\delta = +0.1, -0.1, 0$, 20,000 suites each).

**Result 3 (feasibility, and why the ship curve falls).** For $\delta \ge 0$, $a \le 1 \iff \bar\pi \le (1 - \delta)/(1 + b)$. For $\delta < 0$, $\ell \le 1 \iff \bar\pi \ge (|\delta| + b)/(1 + b)$. Hence, for every $m$,

```math
P(\text{ship} \mid \delta) \le P\!\left(\bar\pi \le \tfrac{1 - \delta}{1 + b}\right), \qquad
P(\text{flag a drop } |\delta|) \le P\!\left(\bar\pi \ge \tfrac{|\delta| + b}{1 + b}\right),
```

with $\bar\pi$ under the posterior of Result 1. The right-hand sides fall to 0 as $|\delta|$ grows. So any curve that rises at small $\delta$ must eventually fall: it cannot be monotone. The committed full-suite curve does rise and then fall (0.531 at +0.1, 0.957 at +0.2, 0.0025 at +0.5). Past the change where the rule almost always fires on a feasible draw, the curve sits on its bound.

*Proof.* For $\bar\pi < 1$, $(\delta + b\bar\pi)/(1 - \bar\pi) \le 1 \iff \delta + b\bar\pi \le 1 - \bar\pi \iff \bar\pi(1 + b) \le 1 - \delta$. For $\bar\pi > 0$, $(|\delta| + b(1 - \bar\pi))/\bar\pi \le 1 \iff |\delta| + b \le \bar\pi(1 + b)$. Shipping and flagging both require feasibility (E5), so their probability is at most that of the feasible event. $\blacksquare$

At $\delta = 0.1$, $b = 0.1$ the threshold is $\bar\pi \le 0.818$. Under the 120-game posterior, $P(\bar\pi > 0.818) = 0.0052$; the grid's pooled infeasible share is 0.0053. For the full suite at 80 games and $+0.5$, the bound is $P(\bar\pi \le 0.4545) = 0.0033$ against a ship rate of 0.003. A drop of 0.5 is feasible with probability 0.897 against a flag rate of 0.89. Checked by R3 (the equivalences on a grid of 1.9 million points; the infeasible share for each $(G, b)$ against 400,000 posterior draws; every curve point of `at_budget` against its bound).

**Result 4 (the variance of the paired change).** Given $\pi$, with $p_{10,f} = (1 - \pi_f)a$, $p_{01,f} = \pi_f\ell$, $\delta_f = p_{10,f} - p_{01,f}$:

```math
\operatorname{Var}[\hat D \mid \pi] = \frac{\sigma_s^2}{N}, \quad
\sigma_s^2 = \frac1K\sum_f \big(p_{10,f} + p_{01,f} - \delta_f^2\big); \qquad
\sigma_u^2 := \bar p_{10} + \bar p_{01} - \delta^2 = \sigma_s^2 + \frac1K\sum_f (\delta_f - \delta)^2 \ \ge\ \sigma_s^2.
```

For $\delta \ge 0$, $\sigma_u^2 = \delta + 2b\bar\pi - \delta^2$, which rises with $\bar\pi$ at slope $2b$.

*Proof.* A player's change $Y - X$ takes the values $1, -1, 0$ with probabilities $p_{10,f}, p_{01,f}$ and the rest, so $E[(Y-X)^2] = p_{10,f} + p_{01,f}$ and $\operatorname{Var}[Y - X] = p_{10,f} + p_{01,f} - \delta_f^2$. The $N$ players are independent given $\pi$ (M2), so the variance of their mean is the sum of their variances over $N^2$, which with $m$ per flaw is $\frac{m}{N^2}\sum_f(\cdot) = \sigma_s^2/N$. For the second identity, $\bar p_{10} + \bar p_{01} - \delta^2 - \sigma_s^2 = \frac1K\sum_f \delta_f^2 - \delta^2$, and since $\delta = \frac1K\sum_f\delta_f$ (Result 2), that is the variance of the $\delta_f$ about their mean, which is nonnegative. Last, $\bar p_{10} = (1-\bar\pi)a = \delta + b\bar\pi$ and $\bar p_{01} = b\bar\pi$, so $\sigma_u^2 = \delta + 2b\bar\pi - \delta^2$, and $\partial\sigma_u^2/\partial\bar\pi = 2b$. $\blacksquare$

$\sigma_u^2$ is what an interval that ignores the flaws estimates (Result 6). $\sigma_s^2$ is the true sampling variance under the stratified design. Checked by R4 (symbolic) and in the R2 simulation (variance ratio within its Monte Carlo SE).

*Remark: why 120 games needs more players.* At $b > 0$, a higher baseline $\bar\pi$ (0.700 at 120 games against 0.614 at 80, posterior means) gives a larger $\sigma_u^2$, because more hits are exposed to churn. That is the decision's "fewer misses to gain from" stated as a variance: $a$ grows, so $p_{10} = \delta + b\bar\pi$ grows. At $b = 0$, $\sigma_u^2 = \delta(1 - \delta)$ does not depend on $\bar\pi$, so the decision's "the same ordering holds at churn 0" (16 against 14) is not a first-order effect of the baseline. It sits on adjacent grid points. That it comes from second-order terms (the infeasible tail at 120 games, the spread of the $\delta_f$, the Wilson limits) is a conjecture, not checked here.

**Result 5 (Wilson's interval).** The bounds of (E3) are the two roots in $p$ of the score equation $(\hat p - p)^2 = z^2 p(1-p)/n$. So the interval is $\{p : |\hat p - p| \le z\sqrt{p(1-p)/n}\}$, the inversion of the score test, with asymptotic coverage $\gamma$. Also $\sqrt n(\hat p - l) \to z\sqrt{\hat p(1 - \hat p)}$, and likewise for $u - \hat p$.

*Proof.* Expand $(\hat p - p)^2 - z^2p(1-p)/n = 0$ as $p^2(1 + z^2/n) - p(2\hat p + z^2/n) + \hat p^2 = 0$. Its roots are

```math
p = \frac{2\hat p + z^2/n \pm \sqrt{(2\hat p + z^2/n)^2 - 4\hat p^2(1 + z^2/n)}}{2(1 + z^2/n)}, \qquad
(2\hat p + \tfrac{z^2}{n})^2 - 4\hat p^2(1 + \tfrac{z^2}{n}) = 4z^2\Big(\frac{\hat p(1-\hat p)}{n} + \frac{z^2}{4n^2}\Big),
```

which is (E3). The quadratic is negative between its roots, which is the set where $|\hat p - p| < z\sqrt{p(1-p)/n}$. Under $H_0: p = p_0$, $(\hat p - p_0)/\sqrt{p_0(1-p_0)/n} \to N(0,1)$ by the central limit theorem, so $P(p_0 \in \text{interval}) \to P(|Z| \le z) = \gamma$. For the limit, $\hat p - l = \big[\hat p z^2/n - z^2/(2n) + z\sqrt{\hat p(1-\hat p)/n + z^2/(4n^2)}\big]/(1 + z^2/n)$. Multiplying by $\sqrt n$, the $O(1/n)$ terms vanish and the root tends to $z\sqrt{\hat p(1-\hat p)}$. $\blacksquare$

Checked by R5 (both bounds substituted into the score equation simplify to 0; the limit by sympy).

**Result 6 (Newcombe's interval is asymptotically the paired Wald interval, and is conservative under stratification).** As $N \to \infty$ with $\pi$ fixed,

```math
\sqrt N\,(\hat D - \text{lo}) \ \xrightarrow{p}\ z\,\sigma_u, \qquad \sqrt N\,(\text{hi} - \hat D) \ \xrightarrow{p}\ z\,\sigma_u,
```

and the interval's coverage of $\delta$ under (M2) tends to $2\Phi(z\,\sigma_u/\sigma_s) - 1 \ \ge\ \gamma$.

*Proof.* By Result 5, $\sqrt N(\hat p_1 - l_1) \to z\sqrt{p_1q_1}$ and $\sqrt N(u_2 - \hat p_2) \to z\sqrt{p_2q_2}$ in probability (the sample shares converge to $p_i$ by the law of large numbers). Since $n_{01} + n_{00} = N(1 - \hat p_1)$ and $n_{10} + n_{00} = N(1 - \hat p_2)$, the denominator of $\hat\phi$ is $N^2\sqrt{\hat p_1\hat q_1\hat p_2\hat q_2}$, so $\hat\phi\sqrt{\hat p_1\hat q_1\hat p_2\hat q_2} = \hat p_{11}\hat p_{00} - \hat p_{10}\hat p_{01}$. This equals $\hat p_{11} - \hat p_1\hat p_2$, the sample covariance of $Y$ and $X$: expand $\hat p_1\hat p_2 = (\hat p_{11} + \hat p_{10})(\hat p_{11} + \hat p_{01})$ and use $\hat p_{00} = 1 - \hat p_{11} - \hat p_{10} - \hat p_{01}$. Then $N$ times the squared half-width tends to

```math
z^2\big[p_1q_1 + p_2q_2 - 2(p_{11}p_{00} - p_{10}p_{01})\big] = z^2\big[p_{10} + p_{01} - (p_{10} - p_{01})^2\big] = z^2\sigma_u^2,
```

and the middle identity is polynomial algebra (R6). The cell probabilities here are the pooled ones $\bar p_{jk}$, because the $N$ players are pooled. So the width is $z\sigma_u/\sqrt N$ to first order. Meanwhile $\hat D$ is a mean of $K$ independent within-flaw means, each of $m$ i.i.d. bounded terms, so $\sqrt N(\hat D - \delta) \to N(0, \sigma_s^2)$ by the central limit theorem (Result 4 gives the variance). By Slutsky's theorem, $P(\text{lo} \le \delta \le \text{hi}) \to P(|Z|\sigma_s \le z\sigma_u) = 2\Phi(z\sigma_u/\sigma_s) - 1$, which is at least $2\Phi(z) - 1 = \gamma$ because $\sigma_u \ge \sigma_s$ (Result 4). $\blacksquare$

At the 80-game posterior means with $b = 0.1$, $\sigma_u/\sigma_s = 1.084$ at $\delta = 0.1$ and 1.034 at $\delta = 0$. The 80% interval's coverage at $+0.1$ ($N = 110$, 40,000 suites) is 0.833 against a predicted 0.835. Checked by R6 (the identities; that simulation).

**Result 7 (the error control of the ship rule and the guard).** When the true change is 0 and $b > 0$, as $N \to \infty$ with $\pi$ fixed,

```math
P(\text{ship}) \to \Phi\!\left(-z_{0.975}\,\tfrac{\sigma_u}{\sigma_s}\right) \le 0.0125, \qquad
P(\text{guard flags}) \to \Phi\!\left(-z_{0.80}\,\tfrac{\sigma_u}{\sigma_s}\right) \le 0.10.
```

The ship rule is a one-sided test at level 1.25%, not 2.5%, and the guard is one-sided at 10%. Both are conservative by the stratification factor $\sigma_u/\sigma_s$.

*Proof.* At $\delta = 0$, $\sigma_u^2 = 2b\bar\pi > 0$, so Result 6 applies. Ship needs $\text{lo} > 0$, that is $\sqrt N\hat D > z\sigma_u + o_p(1)$. Since $\sqrt N\hat D \to N(0, \sigma_s^2)$, its probability tends to $1 - \Phi(z\sigma_u/\sigma_s)$. The guard needs $\text{hi} < 0$, the mirror image, with the same limit at its own $z$. Both are at most $\Phi(-z) = (1 - \gamma)/2$ because $\sigma_u/\sigma_s \ge 1$. (At $b = 0$ and $\delta = 0$ no player changes, $\hat D \equiv 0$, and neither rule ever fires.) $\blacksquare$

With $\sigma_u/\sigma_s = 1.034$ at the 80-game posterior means, the limits are 0.0926 (guard) and 0.0102 (ship). Simulated at that $\pi$: 0.096 and 0.010 at $N = 110$, 0.093 and 0.011 at $N = 500$. The committed guard false alarms are 0.098 (smoke), 0.0925 (full, 80 games) and 0.0895 (full, 120 games). The smoke value is nearer 0.10 because $N = 15$ is far from the limit. The section 9 decision reads one quantity, the primary assurance at churn 0.10, so there is no multiplicity to correct. Checked by R7.

**Result 8 (power and sample size).** For a feasible draw, approximately

```math
P(\text{ship} \mid \pi) \approx \Phi\!\left(\frac{\delta\sqrt N - z_{0.975}\,\sigma_u}{\sigma_s}\right), \qquad
N^\* = \frac{\big(z_{0.975}\,\sigma_u + u_{0.8}\,\sigma_s\big)^2}{\delta^2}
```

solves $P = 0.80$, where $u_{0.8} = \Phi^{-1}(0.8) = 0.8416$ is the one-sided quantile (not the two-sided $z_{0.80} = 1.2816$ of section 1). The assurance (E6) estimates $A(m) = E_{\pi \mid \text{pilot}}\big[\mathbf 1\{\text{feasible}\}\,P(\text{ship} \mid \pi)\big]$.

*Derivation.* By Result 6, ship $\iff \sqrt N\hat D > z\sigma_u + o_p(1)$. With $\sqrt N(\hat D - \delta)/\sigma_s \approx N(0,1)$, $P(\text{ship}) \approx P\big(Z > (z\sigma_u - \delta\sqrt N)/\sigma_s\big) = \Phi\big((\delta\sqrt N - z\sigma_u)/\sigma_s\big)$. Setting the argument to $u_{0.8}$ and solving: $\delta\sqrt N = z\sigma_u + u_{0.8}\sigma_s$, which squared gives $N^\*$. The assurance is the expectation of the conditional ship probability over the posterior of Result 1, with infeasible draws scored 0 (E5); the simulation's mean of i.i.d. indicators estimates it without bias. $\blacksquare$

This is an approximation, and its error is measured, not bounded analytically. Over every grid cell with $m \ge 10$, evaluated by averaging the formula over 20,000 posterior draws, it is within 0.035 of the committed assurance at churn 0.10 and within 0.05 at churn 0.05. At churn 0 it is conservative and can be far off (up to 0.14 below the simulation at 45 games, $m = 16$). With no losses the discordance is one-sided, and the Wilson-based limits are then narrower than the Wald width $z\sigma_u/\sqrt N$. So churn-0 sizes should be read from the simulation only. At 80 games and churn 0.10 the formula gives 0.817 at $m = 40$ (simulated 0.826) and 0.514 at $m = 22$ (simulated 0.525). The plug-in $N^\*$ at the posterior mean $\bar\pi = 0.614$, with $\sigma_s$ replaced by its upper bound $\sigma_u$ ($\sigma_u^2 = 0.2129$), is 202.3, or $m = 40.5$. The simulation's 40 agrees. Checked by R8 (the algebra symbolically; the formula against all 300 grid cells with $m \ge 10$).

The factor of three between churn 0 and 0.10 is $\sigma_u^2$'s dependence on $b$: from $\delta(1-\delta) = 0.09$ to $0.09 + 2b\bar\pi = 0.213$, a ratio of 2.4 at first order. The rest of the factor (40/14 = 2.9) is not explained by the formula. That it comes from the churn-0 cell's Wilson advantage, the cause of the approximation's churn-0 error, is a conjecture.

**Result 9 (Monte Carlo error).** An assurance estimated from $S$ independent simulations has standard error $\sqrt{A(1-A)/S} \le 1/(2\sqrt S) = 0.0079$ at $S = 4{,}000$. The section 6 "at most 0.008" holds.

*Proof.* Within a grid cell the $S$ simulations are successive draws from one generator (one spawned stream per cell, `SeedSequence.spawn`), treated as i.i.d., so $S\hat A \sim \operatorname{Binomial}(S, A)$. Its variance is $SA(1-A)$, so $\operatorname{Var}\hat A = A(1-A)/S$. $A(1-A)$ has derivative $1 - 2A$, zero only at $A = \tfrac12$, and second derivative $-2 < 0$, so it is concave and the stationary point is the maximum, $\tfrac14$. $\blacksquare$ The 95% Monte Carlo intervals are the Wilson intervals of Result 5 at $n = S$. Checked by R9 and the numbers.

**Result 10 (the recall tolerance flags exactly a net loss of $k$).** For $1 \le k \le N < 1000$ and the integer net loss $L$, $L/N > \operatorname{round}((k - \tfrac12)/N, 3) + 10^{-9} \iff L \ge k$.

*Proof.* Write $\tau = (k - \tfrac12)/N + e$ with $|e| \le 0.0005$ (rounding to 3 decimals). The rule is $L > k - \tfrac12 + Ne + N\cdot10^{-9}$. For $N < 1000$, $|Ne| < \tfrac12$ (at most 0.4995) and $N\cdot 10^{-9} < 10^{-6}$, so the right side lies strictly between $k - 1$ and $k$. For an integer $L$ the rule is then $L \ge k$. The bound is needed: at $N = 1000$, $k = 1$, $(k - \tfrac12)/N = 0.0005$ rounds to $0.001$, and a single loss ($L/N = 0.001$) is not above it, so the rule would need $L \ge 2$. $\blacksquare$ Checked by R10, exhaustively for every $N < 1000$ and every $k \le N$.

**Result 11 (the tolerance's $k^\*$).** (E8) sets the reference at $H = \operatorname{round}(\text{pilot recall} \times N)$ hits with $k$ of them lost and none gained, the configuration review concern 3 asked for. At every budget size $k^\* = 2$. Smoke: $N = 15$, $H = 10$ (19/30 × 15 = 9.5 exactly, which Python rounds to 10); full at 80 games: $N = 110$, $H = 70$; full at 120: $N = 70$, $H = 51$. With $k = 1$ the 80% interval's upper end is still above 0. So $\tau = 1.5/N$: 0.1, 0.014 and 0.021. This is a direct computation of (E8), with no proof beyond Result 10, recomputed with the independent (E4) of `math_check.py`.

**Result 12 (the CI rule's law, and why its false-alarm rate is high).** Given $\pi$, $L = \sum_i \lambda_i$ with independent $\lambda_i \in \{-1, 0, 1\}$, $P(\lambda_i = 1) = \pi_f\ell$, $P(\lambda_i = -1) = (1 - \pi_f)a$, so

```math
P(L \ge k \mid \pi) = \sum_{j \ge k} [t^j] \prod_f \big((1 - \pi_f)a\,t^{-1} + (1 - (1-\pi_f)a - \pi_f\ell) + \pi_f\ell\,t\big)^m ,
```

and the CI rule's flag rate is $E_\pi[\mathbf 1\{\text{feasible}\}P(L \ge k^\* \mid \pi)]$. When the true change is 0, $E[L] = 0$ and $\operatorname{Var}[L \mid \pi] = N\sigma_s^2$, so $P(L \ge 2) \approx 1 - \Phi\big(1.5/(\sigma_s\sqrt N)\big)$. With $k^\* = 2$ fixed, that tends to $\tfrac12$ as $N$ grows.

*Proof.* The probability generating function of a sum of independent variables is the product of theirs, and each player's is the trinomial shown. The coefficient of $t^j$ is $P(L = j)$. $E[L] = -N E[\hat D] = 0$ at $\delta = 0$ (Result 2), and $\operatorname{Var}[L] = N^2\operatorname{Var}[\hat D] = N\sigma_s^2$ (Result 4). The normal approximation with continuity correction gives $P(L \ge 2) = P(L > 1.5) \approx 1 - \Phi(1.5/(\sigma_s\sqrt N))$, and $1.5/(\sigma_s\sqrt N) \to 0$. $\blacksquare$

Evaluated exactly by convolution, averaged over posterior draws, it gives a false-alarm rate of 0.116 for the smoke suite (committed 0.118), 0.334 for the full suite at 80 games (0.344) and 0.303 at 120 games (0.299). For the smoke suite's drops of 0.1 and 0.2 it gives 0.486 and 0.805 (committed 0.493 and 0.805). All agree within Monte Carlo error. The full suite's 0.34 is the reason `decision.md` keeps that suite out of the gate. Result 12 makes the reason structural: a fixed net-loss tolerance under churn fails about half of all null runs once $\sigma_s\sqrt N \gg 1.5$. Checked by R12.

**Result 13 (coverage precision).** Under (M4), with $s_f^2$ the divisor-$n_f$ variance of the pilot's $c_j$ in flaw $f$,

```math
\operatorname{Var}[\hat C] = \frac{1}{K^2 m}\sum_f s_f^2, \qquad
\text{half\_width\_95} \approx 1.96\sqrt{\frac{\sum_f s_f^2}{25\,m}}.
```

*Proof.* $\hat C = \frac1K\sum_f \bar c_f$, where $\bar c_f$ is the mean of $m$ i.i.d. draws, uniform over the six pilot values of flaw $f$. One draw has mean $\mu_f = \frac16\sum c_j$ and variance $\frac16\sum(c_j - \mu_f)^2 = s_f^2$. A mean of $m$ i.i.d. draws has variance $s_f^2/m$. The $K$ flaws are drawn independently, so $\operatorname{Var}\hat C = \frac{1}{K^2}\sum_f s_f^2/m$. (E9) reports 1.96 times the sample standard deviation of 2,000 draws of $\hat C$, which estimates this square root with relative standard error about $1/\sqrt{2 \cdot 1999} = 1.6\%$. $\blacksquare$

At 80 games, $\sum_f s_f^2 = 0.4427$, so the half-width is 0.151 at $m = 3$, 0.092 at $m = 8$ and 0.058 at $m = 20$ (committed 0.153, 0.091, 0.057). Every grid cell agrees within 2.2% relative, inside 4 Monte Carlo SE. Checked by R13.

**Result 14 (the coverage calibration).** Under (M5), with either flip model, $E[\frac1N\sum_j(c'_j - c_j)] = g(1 - \hat C) - b\hat C$. Calibrating $g$ on the pooled share $\bar c$ therefore gives the intended $\Delta_c$ plus the error $(g + b)(\bar c - \hat C)$. That error is zero when every player has the same $r_j$, which holds here: every flawed player has an interval at every checkpoint, so $r_j = 4$ to 80 games and 5 to 120. Review concern 9(a) therefore has no effect on these results.

*Proof.* In both flip models each interval of player $j$ is a miss turned covered with probability $g$, or covered turned miss with probability $b$. Only the dependence between a player's intervals differs. So $E[c'_j - c_j] = g(1 - c_j) - bc_j$, and averaging over $j$ gives $g(1 - \hat C) - b\hat C$. With $g$ chosen so that $g(1 - \bar c) - b\bar c = \Delta_c$, subtracting gives $g(1-\hat C) - b\hat C - \Delta_c = (g + b)(\bar c - \hat C)$. With equal $r_j$, $\bar c = \sum r c_j/\sum r = \hat C$. $\blacksquare$ Checked by R14 (symbolic; the interval counts read from `results.json`).

**Result 15 (paired coverage power, approximately).** Write $\mu(c) = g - (g + b)c$ for a player's expected change. Its conditional variance is $w_I(c) = [(1 - c)g(1 - g) + cb(1 - b)]/r$ under interval flips and $w_P(c) = (1-c)^2g(1-g) + c^2b(1-b)$ under player flips. Then, approximately,

```math
P(\text{ship}) \approx \Phi\!\left(\frac{\Delta_c\sqrt N - t_{0.9875,N-1}\,\sigma_u}{\sigma_s}\right),\quad
\sigma_u^2 = \overline{w(c)} + \operatorname{Var}_j\mu(c_j),\quad
\sigma_s^2 = \frac1K\sum_f\big[\overline{w(c)}_f + \operatorname{Var}_{j \in f}\mu(c_j)\big].
```

*Derivation.* Given $c_j$, interval flips change $r(1 - c)$ misses and $rc$ hits independently, so $d_j = (\#\text{gained} - \#\text{lost})/r$ has variance $w_I$. Player flips change all misses together with probability $g$ and all hits together with probability $b$, two independent Bernoulli variables scaled by $1 - c$ and $c$, so the variance is $w_P$. By the law of total variance, a resampled player's $d$ has variance $E[w] + \operatorname{Var}[\mu]$ within its flaw. The $t$ statistic's standard deviation pools the flaws ($\sigma_u$), while $\bar d$ varies only within them ($\sigma_s$). The rest is Result 8's argument with $t$ for $z$. $\blacksquare$ The approximation ignores the randomness of $g$, which is recalibrated on each suite. Its measured error is at most 0.05 over the grid with $m \ge 8$ (at 20 a flaw: 0.511 against 0.520 with interval flips, 0.219 against 0.229 with player flips). Checked by R15.

**Result 16 (the coverage tolerance lies on a lattice).** At the smoke size, every suite has 15 players with 4 intervals, so $c, c' \in \{i/60\}$ and $0.8 = 48/60$. The statistic $|c' - 0.8| - |c - 0.8|$ is a multiple of $1/60$, its 0.8 quantile is one of those multiples (or an interpolation between adjacent ones), and the committed tolerance 0.05 is $3/60$.

*Proof.* $|i/60 - 48/60| = |i - 48|/60$, and a difference of two such terms is an integer over 60. $\blacksquare$

> **Finding 1 (the coverage tolerance is a Monte Carlo coin toss between 0.050 and 0.067).** Under the analysis's own null model (M5 with player flips, $\Delta_c = 0$, $b = 0.10$, 1,000,000 suites in R16), $P(\text{growth} \le 3/60) = 0.7981 \pm 0.0004$ and $P(\text{growth} \le 4/60) = 0.8666$. The population 0.8 quantile that (E10) defines is therefore $4/60 = 0.067$, not 0.05. A 2,000-suite run returns 0.050 only when at least 1,601 of its draws are at most $3/60$, which happens with probability $P(\operatorname{Bin}(2000, 0.7981) \ge 1601) = 0.41$. The committed run happened to land there. The property section 8 wanted, that churn alone exceeds the tolerance 20% of the time, holds for 0.05 to within 0.002: $P(\text{growth} > 0.05) = 0.2019$. For $4/60$ it would be 0.133. So the installed 0.05 does what the pre-registration says in words, but it is not what its formula gives at the population level. Size of the discrepancy: 0.0167 in the tolerance, 1/60 of coverage. Reported to the coordinator. No committed file is changed.

> **Finding 2 (at the installed baseline the coverage rule is two-sided, and its false-alarm rate is not 20%).** (E10) calibrates the tolerance on suites resampled from the pilot. Their coverage is about 0.575, well below 0.8, so there $|c' - 0.8| - |c - 0.8|$ is close to the one-sided $c - c'$. The installed smoke baseline has coverage exactly 0.8 (`bench/baseline.json` `metrics.coverage`). There the CI rule of (M3) becomes $|c' - 0.8| > 0.05$, a two-sided test, which fails when coverage moves either way by 4/60 or more. R16 simulates churn alone ($b = 0.10$, no true change, gain $0.1 \times 0.8/0.2 = 0.4$) on 15 players with 4 intervals each, at coverage 0.8:
> - under interval flips, the false-alarm rate is 0.188 for both test mixes (3 players at 1.0 and 12 at 0.75; 12 at 1.0 and 3 at 0);
> - under player flips, the model the tolerance was registered under, it is 0.40 and 0.71.
>
> So "churn alone exceeds it 20% of the time" holds at the pilot's coverage, not where the rule runs. Size of the discrepancy: a false-alarm rate 2 to 3.5 times the intended 0.20 under the registered dependence model. The real suite's mix of player coverages is not known here, so the true rate is not pinned down. This was found by the referee (Verification). Reported to the coordinator. No committed file is changed.

**Result 17 (the cluster bootstrap of observed coverage, approximately).** Resampling the six players within each flaw gives a bootstrap distribution of $\hat C$ with standard deviation $\sqrt{\sum_f s_f^2/(K^2 \cdot 6)}$, by Result 13 with $m = 6$. Its 2.5% and 97.5% percentiles are approximately $\hat C \pm 1.96$ times that. At 80 games the committed interval's half-width is 0.108 against $1.96 \times 0.0543 = 0.106$; at 120 games, 0.093 against 0.096. They differ by less than 0.003. The approximation ignores the skew and discreteness of a bootstrap over six players, so the error is measured, not bounded. Checked by R17.

**Result 18 (the ICC estimator).** In the one-way random-effects model $y_{jt} = \mu + u_j + e_{jt}$, with $u_j$ of variance $\sigma_b^2$ and $e_{jt}$ of variance $\sigma_w^2$, all independent: $E[\text{MSW}] = \sigma_w^2$ and $E[\text{MSB}] = \sigma_w^2 + n_0\sigma_b^2$ with Searle's $n_0$. So (E11)'s ICC is a ratio of unbiased estimators of $\sigma_b^2$ and $\sigma_b^2 + \sigma_w^2$ (each times $n_0$): consistent for $\sigma_b^2/(\sigma_b^2 + \sigma_w^2)$, not unbiased.

*Proof.* $\text{SSW} = \sum_j\sum_t(y_{jt} - \bar y_j)^2 = \sum_j\sum_t(e_{jt} - \bar e_j)^2$, with expectation $\sum_j(r_j - 1)\sigma_w^2 = (R - k)\sigma_w^2$, so MSW is unbiased. For SSB, $\bar y_j - \bar y = (u_j - \tilde u) + (\bar e_j - \bar e)$ with $\tilde u = \sum r_j u_j/R$ and $\bar e = \sum r_j\bar e_j/R$; the cross terms have mean 0. $E[(u_j - \tilde u)^2] = \sigma_b^2(1 - 2r_j/R + \sum r^2/R^2)$, and $\sum_j r_j(\cdot) = \sigma_b^2(R - \sum r^2/R)$. $E[(\bar e_j - \bar e)^2] = \sigma_w^2(1/r_j - 1/R)$, and $\sum_j r_j(\cdot) = (k - 1)\sigma_w^2$. So $E[\text{SSB}] = (k - 1)\sigma_w^2 + (R - \sum r^2/R)\sigma_b^2$, and dividing by $k - 1$ gives the claim. With equal $r_j = r$, $n_0 = r$. Then $E[\text{MSB} - \text{MSW}] = n_0\sigma_b^2$ and $E[\text{MSB} + (n_0 - 1)\text{MSW}] = n_0(\sigma_b^2 + \sigma_w^2)$, and the ratio converges by the law of large numbers and the continuous mapping theorem. $\blacksquare$ The coverage flags are binary, so the model is a working model, and the ICC (0.295 at 80 games) is descriptive. It is exploratory and decides nothing. Checked by R18 (the two sums symbolically; the expectations by simulation).

**Result 19 (budget sizes).** The largest $m$ with $(Km + C)G \le B$ is $\lfloor(\lfloor B/G\rfloor - C)/K\rfloor$, since $Km + C \le B/G \iff Km + C \le \lfloor B/G\rfloor$ for integers. Smoke: $\lfloor(20 - 1)/5\rfloor = 3$; full at 80 games: $\lfloor(112 - 2)/5\rfloor = 22$; full at 120: $\lfloor(75 - 2)/5\rfloor = 14$. $\blacksquare$

**Result 20 (the decision rule, as computed).** (E7) returns, for each $(G, b)$, the smallest grid $m$ after which no grid value falls below 0.80; review concern 8's fix is built in. At $b = 0.10$ it gives 40 at 80 games and 48 at 120, both above their budget sizes 22 and 14 (Result 19). So the section 9 branch is budget-bound, and the larger assurance at the budget (0.525 at 80 games against 0.283 at 120, a gap above 0.01) picks $G = 80$, $m = 22$. Recomputed in `numbers`.

## 5. Numbers

Every number `decision.md` and `summary.md` quote, recomputed by `math_check.py` `numbers` (rows `N ...`) from `results.json` and `bench/baseline.json`, within the rounding quoted.

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| pilot 30 flawed + 2 clean, 120 games, 3,840 games | count | `pilot.players`, `pilot.games_total` | 32 players, 3840 |
| 0.39 s a game; 25 minutes; 2.44 CPU hours | 1489/3840; 1489/60; 1489 × 5.90/3600 | prose in `decision.md` (wall time is not in `results.json`) | 0.388; 24.8; 2.440 |
| 575,567 searches, 468,209 from cache; 1,489 s; 590%; 8 engines; "under two minutes"; 0.544 s a game; commit 7409f457; 16 opponents instead of 20 (deviation 2); timeouts 180 to 300 and 90 to 300; "none of four" smoke clean checkpoints | – | not in `results.json` or `baseline.json`; the arithmetic built on 1,489 s, 590% and 0.544 s is checked, the inputs are not | – |
| 12,000 nodes, 16 opponents, `algo-1.1.0+fa5e2186e1dd` | – | `pilot.suite`, `algorithm_id` | as quoted |
| untouched tolerances 0.2, 0.2, 0.25 | – | `bench/baseline.json` `tolerance` | as quoted |
| 20 is the grid size nearest 22 | – | `coverage_power[].players_per_flaw` | grid 1 to 64 |
| players needed table (15 cells) | (E7) on the grid | `required_players_per_flaw`, `power_recall` | 14…48, all match |
| 40 a flaw: 0.83 [0.81, 0.84]; 200 players, 16,160 games | (E6), Result 9; $(200 + 2) \times 80$ | `power_recall` (80, 0.1, 40) | 0.82575 [0.8137, 0.8372] |
| 32 a flaw: 0.71 [0.69, 0.72] | (E6) | `power_recall` (80, 0.1, 32) | 0.7055 [0.6912, 0.7194] |
| recall 0.73 against 0.63 | (E11) | `observed.recall_at_2` | 22/30, 19/30 |
| 0.3% to 0.75% of 120-game draws infeasible | Result 3 | `power_recall[].infeasible` (120, 0.1) | 0.00325 to 0.0075 |
| factor of three from churn | ratio | `required_players_per_flaw` | 40/14 = 2.86, 48/16 = 3 |
| full 80: 22, 8,960, 0.52 [0.51, 0.54], MDE 0.15 | Results 19, 9; (E6) | `at_budget[1]` | 22, 8960, 0.52475 [0.5093, 0.5402], 0.15 |
| full 120: 14, 8,640, 0.28 [0.27, 0.30], none | same | `at_budget[2]` | 14, 8640, 0.28325 [0.2695, 0.2974], null |
| smoke: 3, 1,280, 0.07 [0.06, 0.08], none | same | `at_budget[0]` | 3, 1280, 0.0685 [0.0611, 0.0767], null |
| 0.96 at +0.2, 0.003 at +0.5 | Result 3 | `at_budget[1].curve` | 0.9565, 0.0025 |
| hang 6/6, low_clock 6/6, no_convert 5/6; 10 of 11 misses | count | `observed.recall_at_2[3].per_flaw` | as quoted |
| smoke tolerance 0.1 ($k = 2$, $n = 15$) | Results 10, 11 | `at_budget[0].recall_tolerance`, `guard_k_lost` | 0.1, 2 |
| smoke false alarm 0.118; drop 0.2 81%, 0.1 49% | Result 12 | `at_budget[0]` | 0.11775; 0.805, 0.49325 |
| coverage tolerance 0.05 | (E10), Result 16, Finding 1 | `decision.coverage_tolerance_smoke` | 0.05 |
| full tolerance 0.014, false alarm 0.34 | Results 11, 12 | `at_budget[1]` | 0.014, 0.34425 |
| guard false alarms 0.098, 0.093, 0.090 | Result 7 | `at_budget[].guard_false_alarm_delta_0` | 0.09825, 0.0925, 0.0895 |
| guard MDE 0.3, 0.1, 0.1; CI rule MDE 0.05, 0.1; false alarms 0.34, 0.30 | first grid crossing | `at_budget[].mde_*`, `ci_rule_false_alarm_delta_0` | as quoted |
| installed tolerances 0.1 and 0.05 | – | `bench/baseline.json` `tolerance` | 0.1, 0.05 |
| coverage to 80: 0.575 [0.467, 0.683], 120 intervals | (E11), Result 17 | `observed.coverage[3]` | 0.575 [0.4667, 0.6833], 120 |
| coverage to 120: 0.58 [0.48, 0.67] | same | `observed.coverage[4]` | 0.58 [0.48, 0.6667] |
| ICC 0.30 | Result 18 | `observed.coverage[3].icc_within_player` | 0.295139 |
| per flaw W1 0.42, W4.missed 0.79, W7.traps 0.38, W9b 0.63, W5 0.67 | mean of flags | `observed.coverage[3].per_flaw` | 0.4167, 0.7917, 0.375, 0.625, 0.6667 |
| half-width 0.057 (20), 0.091 (8), 0.153 (3) | Result 13 | `coverage_power` (80) | 0.0574, 0.0906, 0.1535 |
| paired coverage power at 20: 0.52 and 0.23 | Result 15 | `coverage_power` (80, 20) | 0.5195, 0.229 |
| recall 19/30, 0.63 [0.46, 0.78]; 22/30, 0.73 [0.56, 0.86] | Result 5 | `observed.recall_at_2` | [0.4551, 0.7813]; [0.5555, 0.8582] |
| W1 rank 1 at 29 of 30 opening checkpoints | count | `pilot.players[].checkpoints[].rank1` | 29 |
| clean: W9a at 45, W5 at 80 | count | same | as quoted |
| budgets: 1,650 games about 15 min; 9,000 about 82; 16,160 about 147; pilot about 35 | games × 0.544/60 | section 9 constants | 15.0, 81.6, 146.5, 34.8 |
| budget-bound, full 22 at 80, smoke 3 at 80 | Result 20 | `decision` | as quoted |

## 6. Assumptions that can fail

- **The churn and gain model (M2): Results 2 to 8, 12.** Every power is conditional on $b = 0.10$ and on a gain spread evenly over misses. Result 8 shows the size scales roughly with $\sigma_u^2 = \delta + 2b\bar\pi - \delta^2$, so an error in $b$ moves $N^\*$ in proportion. The registered sensitivity is the churn grid (0, 0.05, 0.10); nothing measures $b$, and EXP-0005 is the follow-up. A gain concentrated in one flaw changes $\sigma_s^2$ (Result 4 allows any $\delta_f$) and feasibility per flaw. No sensitivity covers that; review concern 6 named it.
- **Independence across players given $\pi$: Results 4, 6 to 8, 12, 13.** Seeds are distinct (`pilot_suite` refuses repeats), the pipeline is deterministic, and the players share only the opponent pool and the engine. A shared factor (for example a detector change that moves every player of a flaw together) would add a within-flaw correlation and inflate $\operatorname{Var}\hat D$ beyond $\sigma_s^2/N$. In (M2) the flaw-level variation is carried by $\pi_f$ but not by the candidate's rates. The player-by-player flips of the coverage secondary are the only bracket.
- **The pilot as the population: Results 1, 13, 15 to 17.** Result 1 carries the six-player uncertainty about $\pi_f$ into the assurance. The coverage resampling (M4) does not: it treats the pilot's six players as the whole population, and $s_f^2$ (divisor 6) understates an unbiased variance by the factor 5/6. Against a population with the unbiased variance, the half-widths of Result 13 are about $\sqrt{5/6} = 0.91$ times too small. No sensitivity covers it.
- **Asymptotics: Results 5 to 8, 15, 17.** At the smoke size ($N = 15$) the normal limits are rough. The decision uses the simulation, not the formulas, so the formulas only explain it. The measured errors are stated with each result. At churn 0, Result 8's formula is off by up to 0.14 and should not be used.
- **The checkpoint as a stand-in for a shorter player: all results at $G < 120$.** An 80-game checkpoint of a 120-game player is taken to be what an 80-game player would produce (review concern 9(d)). Not tested here.
- **Monte Carlo noise in a quantile on a lattice: Result 16.** Finding 1. A larger $S$, or reading the tolerance as the smallest lattice point whose exceedance probability is at most 0.20, would remove it.
- **The baseline the coverage rule compares with: Result 16.** The tolerance was calibrated at the pilot's coverage (0.575). The deployed comparison is with a baseline at 0.8, where the rule is two-sided (Finding 2). No registered sensitivity covers this.
- **Rounding of the tolerance: Result 10.** Holds for $N < 1000$. The suites have $N \le 110$.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py` and `results.json`, but not this file. It re-derived five results:

- the posterior and calibration, and the feasibility bound with its consequence that the curve is not monotone (Results 1 to 3);
- the variance and the normal-approximation sample size (Results 4 and 8);
- Newcombe's large-$N$ limit and the one-sided error rates (Results 6 and 7);
- the tolerance's net-loss equivalence and $k^\*$ (Results 10 and 11);
- the coverage half-width and calibration (Results 13 and 14).

Every formula agreed. The numbers agreed within Monte Carlo error:

- the infeasible shares, 1.0e-4 and 0.0053;
- the half-widths at $m = 3, 8, 20$;
- $H = 10, 70, 51$ and $k^\* = 2$;
- the stratification ratio $\sigma_u/\sigma_s = 1.034$ and the guard's limit of about 0.092.

Its $N^\* = 194$ at 80 games uses the stratified $\sigma_s$ in the power term, and Result 8's 202.3 uses the upper bound $\sigma_u$; both are stated. It found no inconsistency in the committed files. Two of its points were added here: the guard's null rate sits below 10% because of stratification (Result 7 now gives the limit $\Phi(-z\sigma_u/\sigma_s)$ instead of only the nominal rate), and the fixed-$k$ CI rule's false-alarm rate tends to $\tfrac12$ (Result 12).

While `math_check.py` was being written, two of its own checks failed on a first draft, and the maths was corrected rather than the tolerance widened:

- the 80% interval's coverage is $2\Phi(z\sigma_u/\sigma_s) - 1 = 0.835$, not 0.80 (Result 6);
- Result 8's normal approximation fails at churn 0 and is now restricted, with its error stated.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee. It confirmed that section 2 matches `analysis.py` item by item, and it re-checked Results 1, 2, 4 to 6, 10, 12 to 14 and 18. It reported eleven points, all accepted:

1. **Major.** The coverage tolerance claim does not hold at the installed baseline. Added as Finding 2, with a check in R16 and an assumption in section 6.
2. **Major.** Result 8 used $z_{0.80}$ for $\Phi^{-1}(0.8)$, which clashes with section 1. Renamed $u_{0.8}$. $N^\*$ is unchanged (202.3).
3. **Minor.** R7's additive slack was as large as the stratification effect, so the check could not detect it. R7 now uses 200,000 suites at $N = 500$. It checks the guard against $\Phi(-z\sigma_u/\sigma_s)$ with 4 SE plus 0.002, and below 0.10 by more than 4 SE, which the nominal model would fail.
4. **Minor.** R7's check of the committed guard rates is now one-sided ($\le 0.10 + 4$ SE). For the full suites it is also compared with the limit averaged over the posterior.
5. **Minor.** R12 now uses the spread of $P(L \ge k \mid \pi)$ across draws, not a Bernoulli SE at the number of draws.
6. **Minor.** R17's tolerance is down from 0.015 to 0.006, and the claim about its cause is dropped.
7. **Minor.** Checks that compared literals now read `results.json`: the lattice of the committed tolerance, the factor of three and the budget-bound comparison.
8. **Minor.** Result 3 now cites the observed rise and fall, instead of inferring non-monotonicity from the falling bound alone.
9. **Minor.** Two attributions (Result 4 remark, Result 8) are now marked as conjectures. Result 9 states concavity.
10. **Minor.** Result 9's independence wording is corrected.
11. **Minor.** The numbers not recomputed are now listed in section 5. Four that are in files were added to `numbers`: 12,000 nodes, 16 opponents, the algorithm id and the untouched tolerances.

Its perturbation tests (a stratification ratio of 1, the exact CI-rule law times 1.15, $m = 5$ in R17) no longer pass the revised checks, except for the ship half of R7, whose nominal-to-limit gap (0.0023) is within the check's resolution. That half now asserts only that the rate is below nominal.
