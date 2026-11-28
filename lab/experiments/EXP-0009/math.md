# EXP-0009: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic identities and dense grids (prefix `R`), simulations from a known truth (prefix `S`) and the recomputation of every quoted number from `results.json`, `power.json` and `pilot/pilot.json` (prefix `N`).

EXP-0009 regresses next month's rating change on a standardised fall in each of nine rates, one ordinary least squares fit per rate, with HC3 standard errors, Holm across the nine, and a minimum effect scaled by each exposure's split-half reliability. Its decision found a bias: the outcome starts from a rating built from the same games as the exposure. The maths has to show four things. First, what each coefficient, standard error and interval is, and the error control of Holm and of the "below the minimum effect" read (Results 1 to 5). Second, that the reliability scaling is right, including with the P1 level as a covariate, and what the reliability's own error is (Results 6 and 7). Third, that the power analysis and the stage 2 price follow from their parts (Results 8 and 9). Fourth, the luck-sharing bias as theorems: the share of the luck the outcome carries, the sign it gives every coefficient, and what the primary-minus-adjusted shift does and does not measure (Results 10 to 12). EXP-0029 builds on the fourth part, so its Findings are at the top of section 4.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $i = 1, \dots, n_q$ | a player in quantity $q$'s model (one row each) | players | `analysis.py` `design`, `results.json` `primary.<q>.n` |
| $q \in \mathcal Q$ | the nine quantities: W4.missed, W4.allowed, W5, W6, W7.traps, W7.prep, W9a, W9b, accuracy; $m = 9$ | – | `QUANTITIES` |
| $o_q$ | the orientation: $+1$ for accuracy, $-1$ for a rate (lower is better) | – | `oriented`, `HIGHER_IS_BETTER` |
| $q_{i1}, q_{i2}$ | player $i$'s value of $q$ in P1 (2026-08) and P2 (2026-09): events over opportunities summed over the month's observed games (missing below 5), or the mean game accuracy | rate | `month_measures` |
| $x_i$ | the oriented improvement $o_q (q_{i2} - q_{i1})$ | rate | `design` |
| $z_i$ | the standardised improvement $(x_i - \bar x)/s_x$, $s_x$ the sample SD with divisor $n - 1$ | SD | `design` |
| $R_t$ | the player's Lichess rapid rating after their $t$-th rapid game | Elo | the cohort's players files |
| $R_{\text{end}}(P), R_{\text{start}}(P)$ | the mean of the ratings after the month's last $L = 5$ rapid games, and before its first 5 | Elo | `rating_end`, `rating_start`, `ENDS` |
| $y_i$ | the primary outcome $R_{\text{end}}(P3) - R_{\text{end}}(P2)$ | Elo | `design` |
| $D_{2,i}$ | September's own change $R_{\text{end}}(P2) - R_{\text{end}}(P1)$, added in the adjusted model | Elo | `design` (`adjusted`) |
| $W_i$ | the 12 covariates of section 8 (the P1 level, ratings, activity, shares, opportunities), with a constant | mixed | `base_covariates` |
| $X$ | the design matrix $[1, z, W]$, $n_q \times p$ | – | `design` |
| $\hat\beta_q$, $\widehat{\mathrm{se}}_q$, $p_q$ | the coefficient on $z$, its HC3 standard error, its two-sided normal p-value | Elo per observed SD | `fit` |
| $\beta_q$ | the population projection coefficient of $y$ on $z$ given $W$ (the estimand) | Elo per observed SD | section 2 |
| $h_{ii}$, $\hat e_i$ | the leverage (diagonal of $X(X^\top X)^{-1}X^\top$) and the OLS residual | –, Elo | statsmodels `HC3` |
| $\rho_q$, $r_q$ | the correlation of the change on odd and on even games by game key; $r_q = \max(0.01, 2\rho_q/(1+\rho_q))$, $0.01$ when $\rho_q \le 0$ | – | `reliability`, `RELIABILITY_FLOOR` |
| $\delta$, $d_q$ | the minimum effect, 14 Elo per true SD; $d_q = \delta\sqrt{r_q}$ per observed SD | Elo | `DELTA`, `fit` (`delta_observed`) |
| $\alpha$, $\gamma$, $z_\gamma$ | 0.05; the simultaneous level $1 - \alpha/m = 0.99444$; $z_\gamma = \Phi^{-1}(1 - (1-\gamma)/2) = 2.7729$ | – | `ALPHA`, `SIMULTANEOUS` |
| $n_p$, $\widehat{\mathrm{se}}_p$, $r_p$ | the pilot's players, standard error and reliability per quantity (hash fraction $[0.9, 1)$, width $w_p = 0.1$) | – | `pilot/pilot.json` `primary` |
| $\varphi$ | the rating's memory: the share of its error kept after one game ($0 < \varphi < 1$; EXP-0015 measured 0.97) | – | EXP-0029 section 4 |
| $S_t$, $e_t$, $\xi_t$ | the player's skill, the rating's error $R_t - S_t$, the luck of game $t$ | Elo | section 2 (M4) |
| $g_i$, $N_i$ | the player's rapid games in P3 and in P2 | games | `results.json` `flow`, EXP-0029's `dev/exp09_activity.json` |
| $\bar a$ | $\frac{1}{L}\sum_{j=0}^{L-1}\varphi^{-j}$, the averaging factor of a five-game end rating | – | Result 10 |
| $T_{it}$, $u_{it}$, $\tau^2$, $\sigma^2$ | the true value of $q$ in month $t$, its measurement noise, their variances | rate$^2$ | section 2 (M2) |

## 2. Model

**(M1) Players are independent draws.** The rows $(y_i, z_i, W_i)$ of each model are independent and identically distributed draws from the population of section 7 (the hash order is independent of everything measured), with finite fourth moments. Games of two cohort players against each other break this slightly (decision, "games with an opponent in the other slice"); within the confirmatory slice it is ignored. The estimand $\beta_q$ is the population linear projection coefficient:

```math
(\beta_0, \beta_q, \beta_W) = \arg\min_b E\big[(y - b_0 - b_q z - b_W^\top W)^2\big].
```

No causal or linear-truth assumption is needed for $\beta_q$ to be well defined; the reads are statements about it.

**(M2) Measured rates are true values plus noise.** $q_{it} = T_{it} + u_{it}$ for $t = 1, 2$, with $u_{i1}, u_{i2}$ mean zero, uncorrelated with each other and with $(T_{i1}, T_{i2})$, $\operatorname{Var} u_{it} = \sigma^2$, $\operatorname{Var} T_{it} = \tau^2$ (stationary), $\operatorname{Corr}(T_{i1}, T_{i2}) = 1 - c$. The true change is $D_i = o_q (T_{i2} - T_{i1})$, with variance $2\tau^2 c$.

**(M3) The halves are parallel.** The odd and even games by game key give two measures of the same true change, each with noise variance $2\sigma^2$ (half the games), uncorrelated with each other. The month's measure is their average. This holds exactly for a mean of independent per-game values with equal halves and approximately for a pooled rate.

**(M4) The rating's memory.** Between rapid games the rating's error decays geometrically and takes each game's luck:

```math
R_t = S_t + e_t, \qquad e_t = \varphi\, e_{t-1} + \xi_t, \qquad 0 < \varphi < 1,
```

with $\xi_t$ mean zero and independent of the past. This is a linearisation of Lichess's Glicko-2 update around a settled rating deviation; it is the model of EXP-0029 section 4 and of `lab/lean/Rematch/LuckShare.lean`.

**(M5) The luck the exposure shares.** Conditional on the player's game counts $(g, N)$, for each observed game $k$ of P2, $\operatorname{Cov}(\xi_k, \tilde x \mid g, N) = \kappa \ge 0$, the same for every observed game of the month, and 0 for P2's unobserved rapid games (at most 40 of the month's games are observed, `MAX_EVAL_GAMES`, chosen by game key and so spread over the month). The counts $(g, N)$ are independent of the lucks $\xi$ given the covariates; this is an assumption, since a lucky September may change October's activity, where $\tilde x$ is $x$ residualised on $W$: a game the player won by luck is also, on average, a game with fewer weakness events and higher accuracy. Games of P1 may have $\operatorname{Cov}(\xi_k, \tilde x) = \kappa_1 \le 0$ (luck in P1 makes $q_1$ better and the improvement smaller). Games of P3 are independent of $x$.

**(M6) September's change, for Result 12.** After residualising on $W$ and standardising $x$: $D_2 = c_2 + \Lambda$, with catch-up $c_2 = a x + c^\perp$ and luck $\Lambda = b x + \Lambda^\perp$, $c^\perp$, $\Lambda^\perp$ and $x$ uncorrelated, and $y = \beta x + \kappa_c c_2 + \lambda \Lambda + \varepsilon$ with $\varepsilon$ uncorrelated with $(x, c_2, \Lambda)$, $\lambda$ the luck share (Result 10). $\rho_\Lambda = \operatorname{Var}\Lambda^\perp/(\operatorname{Var}c^\perp + \operatorname{Var}\Lambda^\perp)$.

## 3. Estimators

- **(E1) The coefficient** (`fit`, statsmodels `OLS`): $\hat\beta_q$, the coefficient of $z$ in the least squares fit of $y$ on $X = [1, z, W]$.
- **(E2) The HC3 standard error** (`fit`, `cov_type="HC3"`):

```math
\widehat V_{\text{HC3}} = (X^\top X)^{-1} \Big( \sum_i \frac{\hat e_i^2}{(1 - h_{ii})^2}\, X_i X_i^\top \Big) (X^\top X)^{-1}, \qquad \widehat{\mathrm{se}}_q = \sqrt{[\widehat V_{\text{HC3}}]_{zz}}.
```

- **(E3) The intervals** (`fit`): $\hat\beta_q \pm z_\ell\, \widehat{\mathrm{se}}_q$ for $\ell \in \{0.80, 0.95, 0.975, \gamma\}$, $z_\ell = \Phi^{-1}(1 - (1-\ell)/2)$; keys `ci80`, `ci95`, `ci97.5`, `ci99.4444`.
- **(E4) The p-value** (`fit`): $p_q = 2\{1 - \Phi(|\hat\beta_q|/\widehat{\mathrm{se}}_q)\}$.
- **(E5) Holm** (`family`, statsmodels `multipletests(method="holm")`): with $p_{(1)} \le \dots \le p_{(m)}$, reject $H_{(1)}, \dots, H_{(k)}$ for the largest $k$ with $p_{(j)} \le \alpha/(m - j + 1)$ for every $j \le k$. The adjusted p-value is $\tilde p_{(k)} = \max_{j \le k} \min\{1, (m - j + 1) p_{(j)}\}$ (`p_holm`).
- **(E6) The reads** (`family`), in order: "precedes climbing" if Holm rejects and $\hat\beta_q > 0$; "precedes falling" if Holm rejects and $\hat\beta_q < 0$; "below the minimum effect" if the $\gamma$-interval lies inside $(-d_q, d_q)$; otherwise "undetermined". The decision (`decide`): outcome 1 if a key reads climbing; outcome 2 if none does, at least six keys read "below" and accuracy reads climbing; outcome 3 otherwise.
- **(E7) The reliability** (`reliability`): $\hat\rho_q$ the Pearson correlation over players of $x^{\text{odd}}$ and $x^{\text{even}}$, the change on each half; $r_q = 2\hat\rho_q/(1 + \hat\rho_q)$ floored at 0.01; $d_q = \delta\sqrt{r_q}$; the disattenuated effect $\hat\beta_q/\sqrt{r_q}$.
- **(E8) The power** (`power_of`, `cmd_power`): at $n$ players, $\widehat{\mathrm{se}}(n) = \widehat{\mathrm{se}}_p\sqrt{n_p/n}$;

```math
\pi_{\uparrow}(n) = \Phi\Big(\frac{d}{\widehat{\mathrm{se}}(n)} - z_\gamma\Big), \qquad
\pi_{0}(n) = \max\Big\{0,\ 2\Phi\Big(\frac{d - z_\gamma \widehat{\mathrm{se}}(n)}{\widehat{\mathrm{se}}(n)}\Big) - 1\Big\},
```

  with $d = d_p = \delta\sqrt{r_p}$. The slice $[0, q)$ has $n = n_p q / w_p$ players; $q$ is the first grid point where six keys have $\min(\pi_\uparrow, \pi_0) \ge 0.80$ and accuracy $\pi_\uparrow \ge 0.80$.
- **(E9) Stage 2** (`cmd_power`): per key $n^\ast = \big((z_\gamma + z_{0.80})\, \widehat{\mathrm{se}}_p \sqrt{n_p} / d_p\big)^2$, the fifth smallest of the eight (the upper median, `need[len(need) // 2]`), times 80 games at 39.1 CPU-seconds.

## 4. Derivations and proofs

> **Finding 1 (the luck-share argument: right in sign, but its evidence does not identify its size).** Result 11 proves the decision's claim that the shared luck biases every primary coefficient towards negative, under (M4) and (M5). Result 12 shows that the evidence the decision gives for it, the shift from the primary to the adjusted model (accuracy $-0.12$ to $+3.04$), is not a measure of that bias. Under (M6) the shift is $\gamma(a + b)$ with $\gamma = \kappa_c(1-\rho_\Lambda) + \lambda\rho_\Lambda$, while the luck bias is $\lambda b$. The shift is negative even when the exposure shares no luck at all ($b = 0$), as long as September's change carries some luck ($\rho_\Lambda > 0$), moves with the improvement through real catch-up ($a > 0$), and that catch-up does not continue strongly into October ($\kappa_c(1-\rho_\Lambda) < |\lambda|\rho_\Lambda$, for example $\kappa_c = 0$). So "the main evidence for the design flaw" (decision, "What the rest shows") is consistent with the flaw, and shows that September's change reverts on net ($\gamma < 0$), but does not separate shared luck from catch-up. In this model, "with September's change in the model, accuracy reads $+3.0$" is itself biased, by $(\lambda - \kappa_c)\{b(1-\rho_\Lambda) - \rho_\Lambda a\}$. (M6) is a stylised model: September's luck term mixes P1's and P2's luck with shares other than Result 10's, so the formulas are model-dependent, while the conclusion only needs the counterexample. The quoted shifts also contain the opponent-gap term's $\hat\gamma\hat\delta$ (Result 12 (a)), which was not stored. **For EXP-0029**, which registers its expected residual bias as this shift times its memory ratio times $1/\sqrt 3$: the shift equals the luck bias only when $\gamma(a+b) = \lambda b$ (for $\kappa_c = 0$, when $\rho_\Lambda(a+b) = b$); it can be larger or smaller, and the data of EXP-0009 cannot tell which. EXP-0029's $\times 1.5$ and $\times 2$ sensitivities cover an understatement up to a factor 2, not an unbounded one. Nothing here changes EXP-0009's outcome 3.

> **Finding 2 (the five-game end rating changes the share slightly).** The Lean share $\varphi^g - 1$ (`Rematch.exp09Share`) is exact for luck from before P2's last five games (Result 10). Averaged over a month of equally lucky games, the share is $s(g, N) = \varphi^g\bar a(1-\varphi^N)/(1 - \varphi^N\bar a) - 1$, about $\varphi^g\bar a - 1$, with $\bar a = 1.0638$ at $\varphi = 0.97$ and $1.0417$ at 0.98. At 20 games it is $-0.42$ against $-0.46$; at the median 121 it is $-0.973$ against $-0.975$. For EXP-0029's gap-month outcome built the same way, the aggregate share is $\bar a\,\varphi^{g_A}(\varphi^{g_M + g_J} - 1)$, with $g_M$ May's games (EXP-0029 and `Rematch.luckShare` put June's count for May's, $g_M + g_J = 2g_J$). Its memory ratio, registered as $\varphi^{g_A}(\varphi^{2g_J}-1)/(\varphi^g - 1)$, is then low by the factor $\bar a(1-\varphi^g)/(1 - \bar a\varphi^g)$: 1.066 at $\varphi = 0.97$ and 1.046 at 0.98 for $g = 121$, 1.15 and 1.14 for $g = 20$. That is inside its $\times 1.5$ sensitivity. For very light months the share can turn positive (at $\varphi = 0.97$ with 10 rapid games in P2, it needs $g \ge 9$ to be negative).

> **Finding 3 (the reliability's standard error is about 0.015, not 0.008).** `decision.md` and section 11 of the registration say each reliability's standard error is "near 0.008". That is the standard error of the half-correlation $\hat\rho$ at about 16,000 players. The Spearman-Brown reliability's is $2(1-\rho)/\{(1+\rho)\sqrt{n-3}\}$, 1.9 times larger: 0.0137 to 0.0196 for the eight models with 9,653 or more players, 0.042 for W9b (Result 7). These use the model's $n_q$; `reliability` uses only players with all four halves defined, a count the results do not store, so the true standard errors can only be larger. The range the decision quotes for W7.traps (0.02 to 0.08, a minimum of 2 to 4 Elo) is right, and comes from 0.015. EXP-0029 computes its lower bound by Fisher's $z$ on $\rho$ and then Spearman-Brown, which is correct. Of the four "below the minimum effect" reads, W4.allowed and W6 hold for reliabilities down to 0.0361 and 0.0362, about one standard error under their points; W7.traps and accuracy hold down to 0.0098 and 0.0113 (2.6 and 3.8 standard errors), and W9a for any reliability above its floor (Result 5).

**Result 1 (the coefficient is a partial regression in observed SD units).** $\hat\beta_q = s_x \sum_i \tilde x_i y_i / \sum_i \tilde x_i^2$, where $\tilde x$ is the residual of $x$ on $[1, W]$. It converges in probability to $\beta_q$ under (M1). If $E[y \mid z, W]$ is linear in $(z, W)$, $\hat\beta_q$ is unbiased given the design: $E[\hat\beta_q \mid X] = \beta_q$.

*Proof.* `design` replaces the first column by $z = (x - \bar x)/s_x$. By the Frisch-Waugh-Lovell theorem the coefficient on $z$ in the fit on $[1, z, W]$ is $\sum_i \tilde z_i y_i / \sum_i \tilde z_i^2$, with $\tilde z$ the residual of $z$ on $[1, W]$. Since $[1, W]$ contains the constant, the residual of $z = (x - \bar x)/s_x$ is $\tilde x / s_x$ (subtracting $\bar x$ is absorbed by the constant, and residualising is linear). So the coefficient is $(\sum_i \tilde x_i y_i / s_x)/(\sum_i \tilde x_i^2/s_x^2) = s_x \sum_i \tilde x_i y_i/\sum_i \tilde x_i^2$, the slope on raw $x$ times $s_x$. Consistency: $\hat b = (X^\top X/n)^{-1}(X^\top y/n) \to E[X X^\top]^{-1} E[X y] = b$ by the law of large numbers (finite second moments, $E[XX^\top]$ invertible) and continuity; $s_x \to \operatorname{SD}(x)$. Unbiasedness: if $E[y \mid X] = X b$, then $E[\hat b \mid X] = (X^\top X)^{-1}X^\top X b = b$, and $s_x$ is a function of $X$. $\blacksquare$ Checked by `R1` (the identity on a random design) and `S1`.

**Result 2 (the HC3 variance in scalar form, and its consistency).** The $z$ element of (E2) is

```math
\widehat{\mathrm{se}}_q^2 = \frac{\sum_i \tilde z_i^2\, \hat e_i^2/(1 - h_{ii})^2}{\big(\sum_i \tilde z_i^2\big)^2},
```

it is at least the HC0 value (the same with $(1-h_{ii})^2$ replaced by 1), and $n_q\,\widehat{\mathrm{se}}_q^2$ converges to the sandwich variance $E[\tilde z^2 e^2]/E[\tilde z^2]^2$ under (M1), whatever the form of the heteroskedasticity.

*Proof.* Write $A = (X^\top X)^{-1}X^\top$, so $\hat b = A y$. By Frisch-Waugh-Lovell, the row of $A$ for $z$ is $\tilde z^\top/\tilde z^\top\tilde z$. Then $[\widehat V_{\text{HC3}}]_{zz} = \sum_i A_{zi}^2\, \hat e_i^2/(1-h_{ii})^2 = \sum_i \tilde z_i^2 \hat e_i^2/(1-h_{ii})^2 / (\tilde z^\top \tilde z)^2$. Since $0 \le h_{ii} < 1$, each weight $(1-h_{ii})^{-2} \ge 1$, so HC3 $\ge$ HC0 term by term. Consistency: HC0 is White's (1980) estimator, consistent under (M1) with finite fourth moments. For HC3, $\sum_i h_{ii} = p$, and under (M1) $\max_i h_{ii} \le \max_i X_i^\top (X^\top X/n)^{-1}X_i/n \to 0$ in probability, because for iid rows with finite second moments $\max_i \lVert X_i\rVert^2/n \to 0$ (Chesher and Jewitt 1987 discuss HC3's small-sample behaviour), so $\max_i |(1-h_{ii})^{-2} - 1| \to 0$ and the ratio of HC3 to HC0 tends to 1. Here $p = 14$ and $n_q \ge 2{,}094$, so $\bar h = p/n_q \le 0.0067$. $\blacksquare$ Checked by `R2` and `S1`.

**Result 3 (the intervals cover; the simultaneous ones cover together).** Each interval (E3) covers $\beta_q$ with probability tending to $\ell$. The nine $\gamma$-intervals of a family cover all nine $\beta_q$ together with probability at least $1 - \alpha = 0.95$ in the limit, under any dependence between the nine fits.

*Proof.* By Result 1 and the central limit theorem, $\sqrt{n_q}(\hat\beta_q - \beta_q) \to N(0, V)$ with $V$ the sandwich of Result 2, and by Result 2 $n_q\widehat{\mathrm{se}}_q^2 \to V$. By Slutsky, $t_q = (\hat\beta_q - \beta_q)/\widehat{\mathrm{se}}_q \to N(0,1)$, so $P(|t_q| \le z_\ell) \to 2\Phi(z_\ell) - 1 = \ell$. For the family, let $A_q$ be the event that the $\gamma$-interval misses $\beta_q$; $P(A_q) \to 1 - \gamma = \alpha/m$. By the union bound, $P(\bigcup_q A_q) \le \sum_q P(A_q) \to m \cdot \alpha/m = \alpha$, with no assumption on how the $A_q$ depend on each other. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.bonferroni`. Its hypothesis, $P(A_q) \le \alpha/m$ for each event, holds here only in the limit (each interval is asymptotic), so the Lean bound applies to the limiting experiment; `S1` measures the finite-sample miss rate at $n = 1{,}500$. Checked by `R3` ($z_\gamma = 2.77292$) and `S1`.

**Result 4 (Holm's error control, and what it does not cover).** (a) With p-values valid under their point nulls $\beta_q = 0$, Holm (E5) rejects at least one true null with probability at most $\alpha = 0.05$, under any dependence. Under section 2's $H_0$ ($\beta_q = 0$ for all nine), the chance of outcome 1 is therefore at most 0.05. (b) For a single key with $\beta_q \le 0$, the chance it reads "precedes climbing" is at most $\alpha/2 = 0.025$ in the limit, whatever the others do; across $m_J$ such keys, at most $m_J\alpha/2$ by the union bound. This proof gives no tighter control of wrong-sign reads under arbitrary dependence.

*Proof.* (a) Let $I_0$ be the true nulls, $m_0 = |I_0|$. Suppose Holm rejects some $i \in I_0$. Let $j$ be the true null with the smallest p-value; $p_j \le p_i$, so Holm reached $j$ and rejected it, at the step $k$ where $j$ sits in the sorted order, against the threshold $\alpha/(m - k + 1)$. At that step at least the $m_0$ true nulls are not yet rejected (no true null precedes $j$), so $m - k + 1 \ge m_0$ and $p_j \le \alpha/m_0$. So the event is inside $\bigcup_{j \in I_0}\{p_j \le \alpha/m_0\}$, whose probability is at most $m_0 \cdot \alpha/m_0 = \alpha$ by the union bound and validity. Outcome 1 needs a rejection, so under $H_0$ its chance is at most the FWER. (b) Holm's largest threshold is $\alpha/1$, so any rejection needs $p_q \le \alpha$, and "precedes climbing" needs also $\hat\beta_q > 0$, together $\hat\beta_q/\widehat{\mathrm{se}}_q \ge z_{1-\alpha/2}$. If $\beta_q \le 0$ the probability is at most $P(t_q \ge z_{1-\alpha/2}) \to \alpha/2$ (Result 3, $t_q$ shifted down by $\beta_q/\widehat{\mathrm{se}}_q \le 0$). The union bound gives the rest. A sharper bound would need the dependence between the nine fits: once Holm rejects a key truly below zero, the thresholds loosen for the others. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.holm_fwer` (with `Rematch.holm_rejects_true_null` for the deterministic step). Its hypotheses: valid p-values for the true nulls ($P(p_i \le t) \le t$), and point nulls. Here $p_q$ is the asymptotic normal p-value of an HC3 $z$-test, valid only in the limit (Result 3); `S2` measures the finite-sample FWER. Part (b) is not in Lean. The result read no key "precedes climbing", so no decision rests on (b). Checked by `S2` and `N3` (Holm recomputed from the nine p-values).

**Result 5 (a "below the minimum effect" read is wrong with probability at most 0.05, and how far each holds).** Treat $d_q$ as fixed. On the event $C$ that all nine $\gamma$-intervals cover their $\beta_q$, every "below" read is right: $|\beta_q| < d_q$. So the chance that any of them is wrong is at most $1 - P(C) \le 0.05$ in the limit. A read with $\gamma$-interval $[\ell_q, u_q]$ stays "below" for every reliability above $r_q^\ast = (\max(|\ell_q|, |u_q|)/\delta)^2$.

*Proof.* If $[\ell_q, u_q] \subset (-d_q, d_q)$ and $\beta_q \in [\ell_q, u_q]$, then $-d_q < \ell_q \le \beta_q \le u_q < d_q$. So a wrong read needs $\beta_q \notin [\ell_q, u_q]$, an event in $C^c$, and $P(C^c) \le \alpha$ by Result 3. For the threshold: the interval lies inside $(-\delta\sqrt r, \delta\sqrt r)$ iff $\max(|\ell_q|, |u_q|) < \delta\sqrt r$ iff $r > (\max(|\ell_q|,|u_q|)/\delta)^2$. $\blacksquare$ The values are in section 5 (W4.allowed 0.0361, W6 0.0362, W7.traps 0.0098, W9a 0.0071, accuracy 0.0113; W7.prep would need 0.0199 and has 0.0193). Checked by `N5`.

**Result 6 (the attenuation is $\sqrt{r}$, also with the P1 level as a covariate; the level leaks in).** Under (M2), let $y = b\,D/\operatorname{SD}(D) + \theta\, T_1/\tau + \varepsilon$, with $\varepsilon$ uncorrelated with $(T_1, T_2, u_1, u_2)$, and all of $T$, $q$ and $D$ oriented (multiplied by $o_q$, so higher is better): $b$ is the effect per true SD of the change, $\theta$ the effect per true SD of the oriented August level. The population coefficient on the standardised $x$, in the regression of $y$ on $x$ and the oriented P1 level $o_q q_1$ (the covariate of section 8), is

```math
\beta = b\sqrt{r_\Delta} + \theta\,\ell, \qquad r_\Delta = \frac{c}{c+s}, \qquad
\ell = \frac{\sqrt 2\, s\,(1-c)}{\sqrt{c+s}\,(2 + s - c)}, \qquad s = \frac{\sigma^2}{\tau^2}.
```

So with $\theta = 0$ the registration's scaling (a true effect $b$ shows as $b\sqrt{r}$ per observed SD) is exact even with the P1 level in the model. With $\theta > 0$ the primary gains a positive leak $\theta\ell$, which the decision did not name.

*Proof.* With every variable oriented, take $\tau = 1$ (rescale). Then $\operatorname{Var} x = 2c + 2s$, $\operatorname{Var} q_1 = 1 + s$, and $\operatorname{Cov}(x, q_1) = \operatorname{Cov}(T_2 - T_1 + u_2 - u_1, T_1 + u_1) = (1 - c) - 1 - s = -(c + s)$. With $D = T_2 - T_1$, $\operatorname{SD}(D) = \sqrt{2c}$: $\operatorname{Cov}(y, x) = b\sqrt{2c} + \theta(1 - c - 1) = b\sqrt{2c} - \theta c$, and $\operatorname{Cov}(y, q_1) = b(1 - c - 1)/\sqrt{2c} + \theta = -b\sqrt{c/2} + \theta$. By Frisch-Waugh-Lovell the coefficient on $x$ is $\operatorname{Cov}(y, \tilde x)/\operatorname{Var}\tilde x$ with $\tilde x = x - \frac{\operatorname{Cov}(x,q_1)}{\operatorname{Var} q_1} q_1 = x + \frac{c+s}{1+s} q_1$:

```math
\operatorname{Var}\tilde x = 2(c+s) - \frac{(c+s)^2}{1+s} = \frac{(c+s)(2 + s - c)}{1+s},
```

```math
\operatorname{Cov}(y, \tilde x) = b\sqrt{2c} - \theta c + \frac{c+s}{1+s}\Big(\theta - b\sqrt{c/2}\Big)
= b\sqrt{\tfrac c2}\,\frac{2 + s - c}{1+s} + \theta\,\frac{s(1-c)}{1+s}.
```

(The $b$ term: $\sqrt{2c} = 2\sqrt{c/2}$, and $2 - (c+s)/(1+s) = (2 + s - c)/(1+s)$. The $\theta$ term: $-c + (c+s)/(1+s) = (s - cs)/(1+s)$.) The coefficient per observed SD multiplies by $\operatorname{SD}(x) = \sqrt{2(c+s)}$:

```math
\beta = \frac{\sqrt{2(c+s)}\,(1+s)}{(c+s)(2+s-c)}\Big[b\sqrt{\tfrac c2}\,\frac{2+s-c}{1+s} + \theta\,\frac{s(1-c)}{1+s}\Big]
= b\sqrt{\frac{c}{c+s}} + \theta\,\frac{\sqrt 2\, s(1-c)}{\sqrt{c+s}\,(2+s-c)}.
```

Finally $r_\Delta = \operatorname{Var}D/\operatorname{Var}x = 2c/(2c+2s) = c/(c+s)$. $\blacksquare$

The factor $\ell$ can be read off the run: the month's level reliability is $r_L = 1/(1+s)$ and $r_\Delta = c/(c+s)$, so $s = (1-r_L)/r_L$ and $c = s\,r_\Delta/(1-r_\Delta)$. For accuracy ($r_L = 0.722$, $r_\Delta = 0.067$) $\ell = 0.35$; for W7.prep ($0.642$, $0.019$) $\ell = 0.41$. As an illustration only, if $\theta$ were half the level family's two-month effect per true SD ($6.22/\sqrt{0.722}/2 = 3.66$ for accuracy), the leak would be $+1.3$ Elo per observed SD, against a luck bias of the opposite sign (Result 11). The illustration takes (M2) as exact and ignores the other covariates; EXP-0029's critic made the same point (its `review.md`, B2) and EXP-0029 adjusts for the window mean instead. Checked by `R4` (both identities) and `S3`.

**Result 7 (Spearman-Brown, its standard error, and longer windows).** Under (M3), the month's reliability is $r = 2\rho/(1+\rho)$, where $\rho$ is the halves' correlation. With $\hat\rho$ from $n$ players, by Fisher's $z$ and the delta method, $\operatorname{se}(\hat r) \approx 2(1-\rho)/\{(1+\rho)\sqrt{n-3}\}$, while $\operatorname{se}(\hat\rho) \approx (1-\rho^2)/\sqrt{n-3}$. A window $k$ times longer has reliability $kr/(1 + (k-1)r)$.

*Proof.* Each half measures the true change with noise variance $2\sigma^2$, so $\rho = \operatorname{Var}D/(\operatorname{Var}D + 2\sigma^2)$. Their average has noise variance $2\sigma^2/2 = \sigma^2$, so $r = \operatorname{Var}D/(\operatorname{Var}D + \sigma^2)$. Write $v = \operatorname{Var}D/\sigma^2$: $\rho = v/(v+2)$, so $v = 2\rho/(1-\rho)$, and $r = v/(v+1) = 2\rho/(1+\rho)$. Fisher's $\operatorname{atanh}\hat\rho$ has standard error $1/\sqrt{n-3}$ for bivariate normal data, and $d\tanh/dz = 1 - \rho^2$, so $\operatorname{se}(\hat\rho) \approx (1-\rho^2)/\sqrt{n-3}$. With $dr/d\rho = 2/(1+\rho)^2$, $\operatorname{se}(\hat r) \approx 2(1-\rho^2)/\{(1+\rho)^2\sqrt{n-3}\} = 2(1-\rho)/\{(1+\rho)\sqrt{n-3}\}$. The ratio of the two is $2/(1+\rho)^2 \approx 1.9$ at $\rho \approx 0.025$. For $k$ times the games, the noise variance is $\sigma^2/k$: $r_k = v/(v + 1/k) = kv/(kv+1)$, and with $v = r/(1-r)$ this is $kr/(1 + (k-1)r)$. $\blacksquare$ At $r = 0.05$, $k = 3$: $0.15/1.10 = 0.136$, the decision's "about 0.14". Checked by `R5` and `S4` (the simulated SD of $\hat r$ at $n = 16{,}204$ matches 0.015; that of $\hat\rho$ matches 0.008).

**Result 8 (the power formulas).** Take $\hat\beta \sim N(\beta, \operatorname{se}(n)^2)$ with $\operatorname{se}(n) = \operatorname{se}_p\sqrt{n_p/n}$. Then (i) the Bonferroni test at $\gamma$ rejects with $\hat\beta > 0$ with probability $\Phi(d/\operatorname{se}(n) - z_\gamma)$ at $\beta = d$, and Holm's "precedes climbing" has at least this probability; (ii) at $\beta = 0$ the $\gamma$-interval lies inside $(-d, d)$ with probability $\max\{0, 2\Phi(d/\operatorname{se}(n) - z_\gamma) - 1\}$, which overstates the chance of the read "below" by at most $P(\text{Holm rejects } q) \le \alpha$.

*Proof.* The HC3 variance is $V/n$ to first order (Result 2), so its square root scales as $n^{-1/2}$ and the pilot's $\operatorname{se}_p$ at $n_p$ gives $\operatorname{se}(n) = \operatorname{se}_p\sqrt{n_p/n}$. (i) $P(\hat\beta/\operatorname{se} > z_\gamma) = P(Z > z_\gamma - d/\operatorname{se}) = \Phi(d/\operatorname{se} - z_\gamma)$ at $\beta = d$. A Bonferroni rejection, $p_q \le \alpha/m$, is a rejection at Holm's first and strictest threshold; if $p_q \le \alpha/m$ then every hypothesis with a smaller p-value also has $p \le \alpha/m \le \alpha/(m-j+1)$, so Holm rejects $q$ too. (ii) The interval $\hat\beta \pm z_\gamma\operatorname{se}$ lies inside $(-d, d)$ iff $|\hat\beta| < d - z_\gamma\operatorname{se}$, which is empty if $d \le z_\gamma\operatorname{se}$ and otherwise has probability $2\Phi((d - z_\gamma\operatorname{se})/\operatorname{se}) - 1$ at $\beta = 0$. The read also needs no Holm rejection (E6), so its probability is this minus $P(\text{inside and rejected}) \le P(\text{rejected}) \le P(p_q \le \alpha) \to \alpha$. $\blacksquare$ So `power_of`'s docstring, "both are slightly conservative", holds for $\pi_\uparrow$ only: $\pi_0$ is an upper bound, high by at most $\alpha$. Its values at $q = 0.3$ (0.966 or more for the six keys that passed, W7.prep the lowest) stay above 0.80 after subtracting 0.05, so the choice of $q$ is unchanged (`S5b` measures the gap). Checked by `R6`, `S5` and `N9`.

**Result 9 (the stage 2 sample size).** The $n$ at which (E8)'s $\pi_\uparrow = 0.80$ is $n^\ast = ((z_\gamma + z_{0.80})\operatorname{se}_p\sqrt{n_p}/d_p)^2$.

*Proof.* $\Phi(d_p/(\operatorname{se}_p\sqrt{n_p/n}) - z_\gamma) = 0.80$ iff $d_p\sqrt n/(\operatorname{se}_p\sqrt{n_p}) = z_\gamma + z_{0.80}$ iff $n = ((z_\gamma + z_{0.80})\operatorname{se}_p\sqrt{n_p}/d_p)^2$, $\Phi$ being strictly increasing. $\blacksquare$ Stage 2 prices only $\pi_\uparrow$, not $\pi_0$; with $z_{0.80} = 0.8416$ and the pilot's inputs the upper median is W7.traps' 5,242.6 players, and $5{,}242.6 \times 80 \times 39.1/3600 = 4{,}555.2$ CPU-hours. Checked by `R6` and `N9`.

**Result 10 (the share of the luck the outcome carries).** Under (M4), let P2's rapid games be $t = 1, \dots, N$ and P3's $N+1, \dots, N+g$, with $N, g \ge L = 5$. The luck $\xi_k$ of a P2 game has weight $w_k$ in $R_{\text{end}}(P2)$ and weight $w_k + c_k$ in $R_{\text{end}}(P3)$, where:

1. for $k \le N - L + 1$ (before the last five), $c_k = (\varphi^g - 1)\,w_k$ exactly: the outcome carries the share $\varphi^g - 1 \in (-1, 0]$ of that luck;
2. summed over all of P2's games (equally lucky games), the outcome carries the share

```math
s(g, N) = \frac{\sum_k c_k}{\sum_k w_k} = \frac{\varphi^g\,\bar a\,(1 - \varphi^N)}{1 - \varphi^N \bar a} - 1 \ \xrightarrow{N \to \infty}\ \varphi^g\bar a - 1, \qquad \bar a = \frac1L\sum_{j=0}^{L-1}\varphi^{-j}.
```

The limit lies in $(-1, 0]$ for $g \ge L - 1$. For finite $N$, $s \le 0$ iff $\varphi^g\bar a + \varphi^N\bar a(1 - \varphi^g) \le 1$, which fails only when both months are light (at $\varphi = 0.97$: $N = 10$ needs $g \ge 9$, $N = 20$ needs $g \ge 5$).

*Proof.* Iterating (M4), $e_t = \varphi^{t-k}e_k + \dots$, so $\xi_k$ enters $e_t$ with coefficient $\varphi^{t-k}$ for $t \ge k$ and 0 before. $R_{\text{end}}(P2) = \frac1L\sum_{j=0}^{L-1} R_{N-j}$ and $R_{\text{end}}(P3) = \frac1L\sum_{j=0}^{L-1}R_{N+g-j}$ (the windows do not overlap since $g \ge L$). So $w_k = \frac1L\sum_{j:\,N-j \ge k}\varphi^{N-j-k}$ and $w_k + c_k = \frac1L\sum_{j=0}^{L-1}\varphi^{N+g-j-k}$ (all of P3's five come after $k$). (1) If $k \le N - L + 1$, every $j$ counts in $w_k$, so $w_k + c_k = \varphi^g w_k$ and $c_k = (\varphi^g - 1)w_k$. Since $0 < \varphi^g \le 1$, the share is in $(-1, 0]$. (2) Summing geometric series over $k = 1, \dots, N$: $\sum_k (w_k + c_k) = \frac1L\sum_j \varphi^{g-j}\sum_{k=1}^{N}\varphi^{N-k} = \varphi^g\bar a\,\frac{1-\varphi^N}{1-\varphi}$, and $\sum_k w_k = \frac1L\sum_j \sum_{k=1}^{N-j}\varphi^{N-j-k} = \frac1L\sum_j\frac{1-\varphi^{N-j}}{1-\varphi} = \frac{1 - \varphi^N\bar a}{1-\varphi}$. Their ratio minus 1 is $s(g,N)$, and $\varphi^N \to 0$ gives the limit. For $g \ge L-1$, $\varphi^g\bar a = \frac1L\sum_{j=0}^{L-1}\varphi^{g-j}$ is a mean of powers with exponents $\ge 0$, so it lies in $(0, 1]$ and the limit in $(-1, 0]$. The finite condition is $s \le 0 \iff \varphi^g\bar a(1-\varphi^N) \le 1 - \varphi^N\bar a$, rearranged. $\blacksquare$

Machine-checked: `lab/lean/Rematch/LuckShare.lean` theorem `Rematch.luck_share`, part 4 (`Rematch.exp09Share_mem`: $\varphi^g - 1 \in (-1, 0]$ for $0 < \varphi < 1$, $g \in \mathbb N$). That covers the bound in part (1), with $g$ the P3 rapid games; the weight identity $c_k = (\varphi^g - 1)w_k$ that makes $\varphi^g - 1$ the share is proved here only, and checked by `R7`. Part (2), the share averaged over a five-game end rating, is a different expression and is not in Lean (Finding 2). Checked by `R7` (sympy on the sums, a grid on the sign) and `S6`.

**Result 11 (the shared luck biases every primary coefficient towards negative).** Under (M1), (M4) and (M5), write the outcome as $y = y^\circ + \Lambda$, with $\Lambda = \sum_k c_k\xi_k$ the luck term of Result 10 and $y^\circ$ the rest. The primary's population coefficient is $\beta_q = \beta_q^\circ + B$, with

```math
B = \frac{s_x\, E\big[\tilde x\,\Lambda\big]}{E[\tilde x^2]}, \qquad
E[\tilde x\Lambda \mid g, N] = \kappa \sum_{k \in O} c_k + \kappa_1\,(\varphi^g - 1)\sum_{k \in P1} w_k,
```

with $O$ the player's observed P2 games. If the observed games are spread over the month like all of them (game-key order), $E[\sum_{k\in O}c_k] = s(g,N)\,E[\sum_{k \in O}w_k]$, so the first term has the sign of $s(g,N)$: $\le 0$ when $s(g, N) \le 0$ and $\kappa \ge 0$. The second, from P1's luck, is $\ge 0$ for $\kappa_1 \le 0$, but $\sum_{k\in P1}w_k \le \varphi^{N}\bar a/(1-\varphi)$ against $\sum_{k\in P2} w_k = (1 - \varphi^N\bar a)/(1-\varphi)$ for the month, a ratio of $\varphi^N\bar a/(1-\varphi^N\bar a)$ (0.053 at $N = 100$, $\varphi = 0.97$).

*Proof.* By Result 1, $\beta_q = s_x E[\tilde x y]/E[\tilde x^2]$ (population form), and it is linear in $y$, so the part from $\Lambda$ is $B = s_x E[\tilde x\Lambda]/E[\tilde x^2]$. Condition on $(g, N)$; by (M5) the $c_k$, which depend only on $(g, N)$ and the game's position, are then fixed. By linearity, $E[\tilde x\Lambda \mid g, N] = \sum_k c_k E[\tilde x\xi_k \mid g, N]$. Observed P2 games contribute $\kappa c_k$, unobserved ones 0. If which games are observed is independent of their position, each P2 game is observed with the same probability $\pi$, so $E[\sum_{k \in O} c_k] = \pi\sum_{k \in P2}c_k = \pi\,s(g,N)\sum_{k\in P2}w_k$ by Result 10 (2), and likewise for the $w_k$. A P1 game $k$ comes before P2's last five, so its $c_k = (\varphi^g - 1)w_k$ (Result 10 (1)), with $w_k = \varphi^{N}\cdot\varphi^{T_1 - k}\bar a$, $T_1$ P1's last game; summing over $k \le T_1$ gives at most $\varphi^N\bar a/(1-\varphi)$. P3's games have $E[\tilde x\xi_k \mid g, N] = 0$. Finally $E[\tilde x\Lambda] = \sum_{g,N}P(g,N)\,E[\tilde x\Lambda \mid g,N]$, a sum of terms each of the stated sign. $\blacksquare$

So the decision's sentence, that October's drift back from September's luck "pulls every β towards negative", is a theorem under (M5), with $\kappa \ge 0$ the assumption that carries it. Its sign is not shown by the data: the contemporaneous slopes (all positive but W9a) mix luck with catch-up (Result 12). The positive level leak of Result 6 works the other way. Checked by `S6`.

**Result 12 (what the primary-minus-adjusted shift measures).** (a) In the sample, exactly, $\hat\beta_P - \hat\beta_A = \sum_k \hat\gamma_k\hat\delta_k$, where $\hat\gamma_k$ are the adjusted model's coefficients on its two added terms ($D_2$ and the opponent-gap change) and $\hat\delta_k$ the coefficients of $z$ in the regressions of each added term on the primary's regressors. (b) Under (M6), with $D_2$ the only added term,

```math
\beta_P = \beta + \kappa_c a + \lambda b, \qquad \beta_P - \beta_A = \gamma(a + b), \qquad \gamma = \kappa_c(1 - \rho_\Lambda) + \lambda\rho_\Lambda,
```

```math
\beta_A - \beta = (\lambda - \kappa_c)\big\{b(1 - \rho_\Lambda) - \rho_\Lambda a\big\}.
```

The luck bias of the primary is $\lambda b$. The shift equals it only when $\gamma(a+b) = \lambda b$.

*Proof.* (a) Let $X_P$ be the primary's design, $X_A = [X_P, G]$ with $G$ the added columns. The adjusted fit gives $y = X_P\hat b_A + G\hat\gamma + \hat e_A$ with $X_P^\top\hat e_A = 0$. Apply $(X_P^\top X_P)^{-1}X_P^\top$ to both sides: $\hat b_P = \hat b_A + (X_P^\top X_P)^{-1}X_P^\top G\,\hat\gamma = \hat b_A + \hat\Delta\hat\gamma$, where the columns of $\hat\Delta$ are the coefficients of each added term regressed on $X_P$. The $z$ row is the claim. (b) The same identity holds in the population. Standardise $x$ ($\operatorname{Var}x = 1$). The primary: $\beta_P = \operatorname{Cov}(y, x) = \beta + \kappa_c a + \lambda b$, since $c^\perp, \Lambda^\perp, \varepsilon$ are uncorrelated with $x$. The coefficient of $D_2$ on $x$ is $\delta = a + b$. The coefficient of $D_2$ in the adjusted model is, by Frisch-Waugh-Lovell, $\operatorname{Cov}(y, \tilde D)/\operatorname{Var}\tilde D$ with $\tilde D = D_2 - \delta x = c^\perp + \Lambda^\perp$: $\operatorname{Cov}(y, \tilde D) = \kappa_c\operatorname{Var}c^\perp + \lambda\operatorname{Var}\Lambda^\perp$, so $\gamma = \kappa_c(1-\rho_\Lambda) + \lambda\rho_\Lambda$. By (a), $\beta_P - \beta_A = \gamma\delta = \gamma(a+b)$. Then $\beta_A - \beta = \kappa_c a + \lambda b - \{\kappa_c(1-\rho_\Lambda) + \lambda\rho_\Lambda\}(a+b)$; collecting, the $\kappa_c$ terms are $\kappa_c\{a - (1-\rho_\Lambda)(a+b)\} = \kappa_c\{\rho_\Lambda a - (1-\rho_\Lambda)b\}$ and the $\lambda$ terms are $\lambda\{b - \rho_\Lambda(a+b)\} = \lambda\{(1-\rho_\Lambda)b - \rho_\Lambda a\}$; together $(\lambda - \kappa_c)\{b(1-\rho_\Lambda) - \rho_\Lambda a\}$. $\blacksquare$

With $\kappa_c = 0$ (catch-up in September does not continue into October) and $b = 0$ (no shared luck), the primary is unbiased, yet the shift is $\lambda\rho_\Lambda a < 0$ and the adjusted coefficient is biased up by $-\lambda\rho_\Lambda a > 0$. That is Finding 1. Checked by `R8` (the sample identity on the committed design's shape), `R9` (the population algebra) and `S7`.

## 5. Numbers

Each quoted number recomputed by `math_check.py` `numbers` from the committed JSON (`results.json` unless named). "Rounded" means the quoted digits agree with the JSON value.

| Quoted as (where) | Formula | Key | Value |
|---|---|---|---|
| primary table: players, Elo per observed SD, 95%, simultaneous, reliability, minimum, reads (decision) | (E1), (E3), (E6), (E7) | `primary.<q>.{n, estimate, ci95, ci99.4444, reliability, delta_observed, reads}` | all 9 rows; every interval is $\hat\beta \pm z_\ell\widehat{\mathrm{se}}$ to $2 \times 10^{-6}$, every minimum $14\sqrt{r}$ to $10^{-5}$ |
| W5 "precedes falling (Holm p 0.0003)" | (E4), (E5) | `primary.W5.p_holm` | 0.000337; $9 \times 3.7\times 10^{-5}$ within $p$'s rounding |
| outcome 3; four keys below (six needed); control fails | (E6) | `decision` | `need more data`, [W4.allowed, W6, W7.traps, W9a], `false` |
| accuracy "99.44% [−1.49, 1.24] against ±3.63" | (E3), (E7) | `primary.accuracy.ci99.4444`, `delta_observed` | [−1.486, 1.239], 3.635 |
| adjusted W5 −0.45 [−1.83, 0.92]; accuracy +3.0 [2.1, 4.0], W7.prep +1.9 [1.0, 2.8], W7.traps +1.5 [0.6, 2.5] read climbing | (E1), (E3), (E6) | `adjusted.<q>` | −0.454 [−1.829, 0.921]; 3.035 [2.064, 4.006]; 1.878 [0.969, 2.787]; 1.537 [0.600, 2.474]; all three Holm-rejected, positive |
| primary minus adjusted, accuracy −3.16, W5 −2.48 (EXP-0029, from these keys) | Result 12 (a) | `primary`, `adjusted` | −3.158, −2.484 |
| contemporaneous: accuracy +14.1 [13.1, 15.2], W5 +12.5, W6 +8.5, W7.traps +7.0, W4.allowed +6.1, W7.prep +5.6, W4.missed +5.2, W9a +0.8 [−0.01, 1.65] | (E1), (E3) | `contemporaneous.<q>` | 14.124 [13.089, 15.159], 12.490, 8.493, 6.987, 6.086, 5.625, 5.166, 0.819 [−0.011, 1.650] |
| level: accuracy +6.2 [4.6, 7.9], W7.prep +5.9 [4.6, 7.1], W7.traps +4.4 [2.9, 6.0], W4.missed +2.5 [1.0, 4.1], W5 +2.1 [0.5, 3.6] (Holm); W4.allowed, W6, W9a below | (E1)–(E6) | `level.<q>` | as quoted; those five Holm-rejected; those three read "below" |
| level reliability "0.36 to 0.79 for most keys" | (E7) | `level.<q>.reliability` | 0.358 to 0.792 for seven of nine (W4.missed 0.170, W9b 0.119) |
| summary "+4 to +6 Elo per SD" (accuracy, early blunders, leaving the opening worse) | (E1) | `level.{accuracy, W7.traps, W7.prep}.estimate` | 6.22, 4.45, 5.89 |
| change reliability "0.02 to 0.13" | (E7) | `primary.<q>.reliability` | 0.0193 to 0.1312 |
| W7.prep missed "by 0.02 Elo (1.97 against 1.95)" | Result 5 | `primary.W7.prep` | 1.9736 against 1.9462: 0.027, quoted from the rounded ends |
| W7.traps "0.02 to 0.08", minimum "2 to 4" | Result 7 | `primary.W7.traps.{reliability, n}` | $0.0483 \pm 1.96 \times 0.0150$ = [0.019, 0.078]; $14\sqrt{\cdot}$ = [1.93, 3.91] |
| robustness of the "below" reads | Result 5 | `primary.<q>.ci99.4444` | $r^\ast$: W4.allowed 0.0361, W6 0.0362, W7.traps 0.0098, W9a 0.0071, accuracy 0.0113 |
| "three times the games lifts 0.05 to about 0.14" | Result 7 | – | 0.136 |
| attrition 94.6%; no change moved staying by more than 0.6 points per SD | – | `flow`, `attrition.<q>.per_sd` | 16,204/17,136 = 0.9456; max $|$per_sd$|$ = 0.0056 |
| IPW W5 −2.85 [−4.23, −1.47], accuracy 0.15 [−0.86, 1.16] | (E1), (E3) | `attrition.<q>.ipw_primary` | −2.850 [−4.232, −1.467], 0.152 [−0.856, 1.161] |
| W5 in B1 to B3 −2.4 to −3.8; W7.traps B0 +6.3 [0.8, 11.8] | (E1), (E3) | `by_band` | −3.10, −2.36, −3.77; 6.297 [0.763, 11.832] |
| blunder rate +0.97 [−0.02, 1.96] | (E1), (E3) | `exploratory.blunder_rate` | 0.970 [−0.020, 1.959] |
| summary "−0.1 Elo per SD, 95% −1.1 to 0.8" | (E1), (E3) | `primary.accuracy` | −0.123 [−1.087, 0.840] |
| flow 86,819; 18,968; 17,138; 17,136 (B0 554, B1 4,009, B2 7,514, B3 5,059); 16,204 (94.6%); 16,206 | – | `flow` | as quoted |
| 877,072 read, 876,947 observed, 125 failed; 91,366 | – | `games` | as quoted |
| 366.2 million games | sum | `months.<P>.games_seen` | 366,212,836 |
| SD of next month's change 53.8 | – | `rating_change_sd` | 53.789 |
| 73% of the budget | – | `compute` | 877,072/1,200,000 = 0.731 |
| stage 2 "about 5,240 players × 80 games, about 4,560 CPU-hours, 228 times its ceiling" | Result 9 | `power.json` `stage2` | 5,242.6; 4,555.2; 227.8 |
| q = 0.3; six keys at 0.80; W9b 0.75 and 0.50, W4.missed 0.15 and 0.00; accuracy 1.00 (registration section 6) | (E8) | `power.json` `q_grid`, recomputed from `pilot/pilot.json` | 0.1 and 0.2 fail, 0.3 passes; 0.749, 0.498; 0.155, 0.000; 1.000 |
| pilot reliabilities 0.01 … 0.15; 28,721, 6,365, 5,820, 5,818, 5,472; 305,043 games, 54 failed; "about 915,000 games" | – | `pilot/pilot.json` | as quoted; $3 \times 305{,}043 = 915{,}129$ |
| the lowest passing power at q = 0.3, 0.966 (W7.prep); the upper median is W7.traps | Results 8, 9 | `power.json` `q_grid.0.3`, `pilot/pilot.json` | 0.966; 5,242.6 |
| 1,000,000 engine nodes (100%); "10 to 40 games"; the ceiling of 20 CPU-hours; 39.1 CPU-seconds | – | `compute`, `analysis.py` constants | read from the JSON and from `analysis.py` |

Not checkable from the committed files, so not checked: the wall and CPU times (48 minutes on 8 workers, about 1.5 CPU-hours, the pilot's 18 minutes, 3.3 hours of streaming, about 118 GB at 9 to 12 MB/s, 20 GB parsed) are run metadata in `decision.md` and the summary's front matter; the median of 121 rapid games in October is from EXP-0029's `dev/exp09_activity.json`, outside this folder.

## 6. Assumptions that can fail

- **Independent players (M1)**, for Results 1 to 5. Games between two cohort players, and second accounts, make rows dependent; within the confirmatory slice no count exists (the 91,366 counts games against the pilot's hash range). A positive dependence makes the HC3 intervals too narrow. No sensitivity covers it.
- **Asymptotic normality**, for Results 3, 4 and 8. The smallest model has 2,094 players and $\bar h \le 0.007$; `S1` and `S2` show the HC3 $z$ intervals and Holm at the nominal rates at 1,500 players with skewed, heteroskedastic errors. The rating change has heavy tails (SD 53.8); a few extreme players move the HC3 variance most, which HC3's leverage weighting partly guards. No sensitivity covers it.
- **Fixed reliabilities**, for Result 5 and every "below" read. The reads treat $r_q$ as known. Its standard error is about 0.015 (Result 7, Finding 3); W4.allowed and W6 hold only about one standard error below their points. The outcome needed six reads and had four, so no plausible reliability changes it. EXP-0029 reads the lower bound.
- **Parallel halves and stationarity (M2, M3)**, for Results 6 and 7. A pooled rate's halves have unequal opportunities, and a player's true variance can differ between months. Then $\ell$ and the $\sqrt r$ scaling are approximate; with unequal noise the change and the P1 level share noise, which EXP-0029 handles with a window-mean covariate (`Rematch.window_mean_independence`). The level leak $\theta\ell$ was not covered by any EXP-0009 sensitivity; it biases the primary up.
- **The rating's memory (M4)**, for Results 10 and 11. Glicko-2's update depends on the rating deviation, so $\varphi$ varies across players and over time, and a returning or provisional account forgets faster. The sign of Result 11 needs only $s(g,N) \le 0$, which holds for any $0 < \varphi < 1$ once the months are not both light (Result 10). The size depends on $\varphi$ (EXP-0029 uses 0.97 and 0.98).
- **The luck the exposure shares (M5)**, for Result 11. $\kappa \ge 0$ is the assumption that makes the bias negative. It is very likely for W5, W6 and accuracy, which are defined partly by the games' results, and plausible for the others; it is not measured. If the analysed games bunch at the end of September, the last five games' weights matter more and the share moves towards Result 10's finite-$N$ case.
- **The linear mediator model (M6)**, for Result 12. It is the simplest model with luck and catch-up in September's change; it shows that the shift does not identify the luck bias, which needs only one counterexample, so the conclusion does not depend on (M6) being true. The adjusted model's coefficient on $D_2$ was not stored (decision), so $\gamma$ cannot be read from the run.
- **Holm's sign errors**, Result 4 (b). Only the per-key bound is proved; nothing in the run depends on it, since no key read "precedes climbing".

### Verification

- **Blind re-derivation.** A fresh agent was given the registration, `analysis.py`, the JSON, `decision.md` and `LuckShare.lean`, not this file. It derived, independently: the two power formulas and stage 2 (5,242.57 players, W7.traps as the upper median, 4,555.2 CPU-hours); Spearman-Brown and $\operatorname{se}(\hat r) \approx 0.0149$ against $\operatorname{se}(\hat\rho) \approx 0.0079$ (Finding 3); the $\sqrt{r}$ attenuation with the P1 level as a covariate, exact, and the same level-leak formula as Result 6; the per-game shares of Result 10 (with the same aggregate, for example $-0.011$ at $N = 20$, $g = 5$) and the conditions for Result 11; and the same shift decomposition as Result 12. All five agree with this file. It also showed that `power_of`'s "both slightly conservative" is wrong for the "below" power, which Result 8 now says, and its value of $z_\gamma$, 2.7729, matched `R3`, which had caught a mistyped 2.7718 here.
- **Referee.** A second fresh agent checked every proof and the script, and found nothing blocking. Changes made: Finding 1 now states the condition on $\kappa_c$ for the shift's sign, labels (M6) as stylised, and names the unmeasured opponent-gap term; Finding 2 gives the exact ratio factor (1.066 and 1.046 at 121 games, 1.15 and 1.14 at 20) instead of "about $\bar a$"; (M5) and Result 11 now condition on $(g, N)$, assume their independence from luck, and count only the observed games; Result 11's P1 ratio is 0.053; Result 6 orients $\theta$; Result 2's citation is corrected; Finding 3 notes the unstored half-count; the Lean citation of Result 10 says what the theorem covers. In the script: `R9` now solves the normal equations instead of restating $\gamma$; `S7b` tests Result 12 with $\kappa_c \ne 0$ and $b \ne 0$; `S6` now runs a light month, where Result 10's share is positive while $\varphi^g - 1$ and $\varphi^g\bar a - 1$ are negative, and fails both alternatives; `S2b` uses effects near zero so that wrong-sign reads happen; `N5b`, `N7`, `N9` and `N10` check the numbers the referee found unchecked, and section 5 lists the run metadata that cannot be checked.
