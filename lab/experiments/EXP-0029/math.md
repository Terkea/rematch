# EXP-0029: the maths

Written while the confirmatory run is going, before any result exists (ADR 0118). Nothing in `preregistration.md`, `analysis.py`, `power.py` or `power.json` was changed. Sections 1 to 4 are complete. Section 5 recomputes every number committed so far (the registration, `power.json` with its `sizing` block, and `summary.md`); the result numbers are added with the decision. Section 6 lists the assumptions, including deviation 2 (the size of the luck bias is assumed, not measured).

Each result is proved in prose and checked by `math_check.py` in this folder, in the lab image: symbolic identities and dense grids (prefix `R`), simulations from a known truth (prefix `S`) and the recomputation of the committed numbers by code written apart from `power.py` (prefix `N`). `result_numbers` is the stub for the decision's numbers; it reports "PASS-skipped" until `results.json` exists.

EXP-0029 regresses the rating change from the end of April to the end of June 2026 on a standardised improvement in each of nine quantities from one three-month window (F1, October to December 2025) to the next (F2, January to March 2026), one ordinary least squares fit per quantity with HC3 errors. It reads the eight keys under Holm, tests the control (accuracy) on its own, and reads "below the minimum effect" against $14\sqrt{r_L}$, with $r_L$ a Fisher-$z$ lower bound of the exposure's reliability. Its design rests on four arguments in words: the gap month forgets most of the luck the exposure shares with the baseline; adjusting for the window-mean level leaks little of a stable level; the registered bias scales EXP-0009's shift by a memory ratio and $1/\sqrt3$; and the planned power and the sizing step follow from EXP-0009's results. Results 5, 8 and 9 turn those into theorems, and the Findings say where they hold only approximately.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $i = 1, \dots, n_q$ | a player in quantity $q$'s model, one row each | players | `analysis.py` `design` |
| $q \in \mathcal Q$ | the nine quantities: the eight keys $\mathcal K$ (W4.missed, W4.allowed, W5, W6, W7.traps, W7.prep, W9a, W9b) and the control $c$ = accuracy | – | `KEYS`, `QUANTITIES`, `CONTROL` |
| $\mathcal K_2$ | outcome 2's five keys: W4.allowed, W5, W6, W7.traps, W9a | – | `OUTCOME2_KEYS` |
| $m = 8$, $\alpha = 0.05$ | the size of the Holm family and its level | – | `KEYS`, `ALPHA` |
| $\gamma = 1 - \alpha/m = 0.99375$, $z_\gamma = 2.7344$ | the simultaneous level and its normal quantile, $\Phi^{-1}(1 - (1-\gamma)/2)$ | – | `SIMULTANEOUS` |
| $\delta = 14$ | the minimum effect | Elo per true SD of the improvement | `DELTA` |
| $o_q \in \{-1, +1\}$ | orientation: $-1$ for a rate (lower is better), $+1$ for accuracy | – | `E9.oriented` |
| $q_{i1}, q_{i2}$ | $q$ over the observed games of F1 and of F2 (events over opportunities summed over the window, or mean accuracy) | rate or accuracy points | `E9.month_measures` |
| $x_i = o_q(q_{i2} - q_{i1})$ | the oriented improvement | as $q$ | `design`, `change` |
| $z_i = (x_i - \bar x)/s_x$ | the standardised improvement, $s_x$ the sample SD | observed SD | `design` |
| $\bar q_i = o_q(q_{i1} + q_{i2})/2$ | the oriented window-mean level | as $q$ | `level_of` |
| $W_i$ | the other eleven terms of `base_terms` after the level (ratings, game counts, shares, opportunities) | mixed | `base_terms` |
| $y_i = R_{\text{end}}(\text{June}) - R_{\text{end}}(\text{April})$ | the outcome | Elo | `design`, `r_end` |
| $R_{\text{end}}$ | the mean rating after a month's (or window's) last $L = 5$ rapid games | Elo | `r_end`, `window_end` |
| $\beta_q$, $\hat\beta_q$, $\widehat{\mathrm{se}}_q$ | the population coefficient of $z$, its OLS estimate and HC3 standard error | Elo per observed SD | `fit` |
| $h_{ii}$, $\hat e_i$ | leverage and residual of the fit | – | `fit` |
| $p_q$, $\tilde p_q$ | the two-sided normal p-value and its Holm adjustment | – | `fit`, `read_family` |
| $\hat\rho_q$, $n'_q$ | the correlation over players of the improvement on odd and on even games by game key, and the number of players with both halves | –, players | `reliability` |
| $\hat r_q$, $r_{L,q}$ | the Spearman-Brown point reliability and its 95% lower end by Fisher's $z$, floored at 0.01 | – | `reliability` |
| $d_q = \delta\sqrt{r_{L,q}}$ | the minimum per observed SD | Elo per observed SD | `fit`, `minimum_observed` |
| $\varphi$ | the rating's memory per game: 0.97 (EXP-0015) or 0.98 | – | `power.py` `PHI` |
| $g_A, g_M, g_J$ | rapid games in April, May and June 2026 ($g_M$ is not streamed; $g_J$ stands in) | games | `sizing_measures` |
| $G$, $N$ | rapid games in one month of F2, and in the whole of F2 | games | – |
| $s_0(g) = \varphi^g - 1$ | EXP-0009's luck share at $g$ games in its outcome month | – | `luck_share("exp0009")` |
| $s_A = \varphi^{g_A}(\varphi^{2g_J} - 1)$ | this design's luck share | – | `luck_share("A")`, `sizing_measures` |
| $\bar a = \frac1L\sum_{j=0}^{L-1}\varphi^{-j}$ | the five-game end rating's factor: 1.0638 at $\varphi = 0.97$, 1.0417 at 0.98 | – | EXP-0009 `math.md` Result 10 |
| $M_\varphi = E[s_A]/E[s_0]$ | the memory ratio; $M = \max_\varphi M_\varphi$ is the worse | – | `memory_ratio`, `worse_ratio` |
| $\Delta_q$ | EXP-0009's primary minus adjusted estimate | Elo per observed SD | `luck_shift`, `power.json` `luck_shift_0009` |
| $D$ | the window's dilution of the shared luck, registered as $1/\sqrt3$ | – | `PLAN["dilution"]` |
| $B_q = \Delta_q M D \mu$ | the registered expected bias, $\mu \in \{1, 1.5, 2\}$ the sensitivity multiplier | Elo per observed SD | `biases` |
| $n_q^{9}, \mathrm{se}_q^{9}, r_q^{9}$ | EXP-0009's primary $n$, HC3 SE and reliability | players, Elo, – | EXP-0009 `results.json` `primary` |
| $w$, $\varpi$, $\eta$ | the slice width, the density (players per unit of hash over EXP-0009's), the outcome SD ratio | –, –, – | `inputs`, `decide_size` |
| $k$ | Spearman-Brown's factor $E_{9}[1/m]/E[1/\min(m, \text{kept})]$ | – | `k_of` |
| $m_i$ | a window's analysed rapid games | games | `sizing_measures` |
| $\rho_\beta = 0.3$ | the planned correlation between the nine coefficients | – | `PLAN` |

Every probability without a subscript is over the population of section 7 of the registration (anonymised Lichess rapid players below 2000, active for six months, in the hash range $[0.3, 0.9)$), and every expectation over players drawn from it.

## 2. Model

**(M1) Players are independent draws.** The rows $(y_i, z_i, \bar q_i, W_i)$ of each model are independent and identically distributed draws from the population, with finite fourth moments and $E[XX^\top]$ invertible for $X = [1, z, \bar q, W]$. The hash order is independent of everything measured. Games between two players of the slice break this slightly; the run counts them (`games_between_two_observed_players`). The estimand is the population linear projection coefficient:

```math
(\beta_0, \beta_q, \beta_{\bar q}, \beta_W) = \arg\min_b E\big[(y - b_0 - b_q z - b_{\bar q}\bar q - b_W^\top W)^2\big].
```

No causal or linear-truth assumption is needed for $\beta_q$ to exist; the reads are statements about it.

**(M2) Measured quantities are true values plus noise.** Oriented, $q_{it} = T_{it} + u_{it}$ for $t = 1, 2$, with $u_{i1}, u_{i2}$ mean zero, uncorrelated with each other and with $(T_{i1}, T_{i2})$, $\operatorname{Var}u_{it} = \sigma_t^2$. The true levels are stationary, $\operatorname{Var}T_{it} = \tau^2$, with $\operatorname{Corr}(T_{i1}, T_{i2}) = 1 - c$. The true change is $D_i = T_{i2} - T_{i1}$, $\operatorname{Var}D = 2c\tau^2$. Write $s_t = \sigma_t^2/\tau^2$, and $s$ when $s_1 = s_2$. The change's reliability is $r_\Delta = \operatorname{Var}D/\operatorname{Var}x$.

**(M3) The halves are parallel.** The odd and the even games of each window, by game key, give two measures of $D$ whose noises are uncorrelated, each with twice the full window's noise variance. A window's noise variance is $\sigma_g^2/m$ for $m$ games with the same per-game variance $\sigma_g^2$, in both experiments.

**(M4) The rating's memory.** Between a player's rapid games the rating's error decays geometrically and takes each game's luck:

```math
R_t = S_t + e_t, \qquad e_t = \varphi e_{t-1} + \xi_t, \qquad 0 < \varphi < 1,
```

with $\xi_t$ mean zero and independent of the past (EXP-0009 (M4); `lab/lean/Rematch/LuckShare.lean`). The game counts are independent of the lucks given the covariates.

**(M5) The luck the exposure shares.** Each observed game $k$ of F2 has $\operatorname{Cov}(\xi_k, \tilde x) = \kappa_k$, the same sign for every game (a game won by luck tends to have fewer weakness events and higher accuracy), where $\tilde x$ is $x$ residualised on the other regressors. For Result 9 the months of F2 have equal counts $G$, each observed game enters $q_2$ with the same weight, and $\kappa_k = \kappa$ for every game. Games after F2 are independent of $x$.

**(M6) A stable level in the outcome.** For Result 5, with every variable oriented,

```math
y = b\,\frac{D}{\operatorname{SD}(D)} + \theta_1\frac{T_1}{\tau} + \theta_2\frac{T_2}{\tau} + \varepsilon,
```

with $\varepsilon$ uncorrelated with $(T_1, T_2, u_1, u_2)$. $b$ is the effect per true SD of the change (the estimand of section 2's $H_1$), and $\theta_1, \theta_2$ the effects of the two windows' true levels. EXP-0009's level family measured a level effect on the earlier window.

**(M7) The planning model** (`power.py` `simulate`), with one difference from the run's rule (Finding 4). Per replicate, the nine estimates are jointly normal,

```math
\hat\beta_q \sim N\big(\mu_q, \mathrm{se}_q^2\big), \quad \operatorname{Corr}(\hat\beta_q, \hat\beta_{q'}) = \rho_\beta, \qquad
\mu_q = b_q\sqrt{r_q} + B_q,
```

and, independently of them, each quantity's $\operatorname{atanh}\hat\rho_q \sim N(\operatorname{atanh}\rho_q, 1/(n_q - 3))$ with $\rho_q = r_q/(2 - r_q)$. The run uses $n'_q \le n_q$, the players with both halves, so the planned $r_L$ is slightly high. The standard errors are treated as known, $\widehat{\mathrm{se}}_q = \mathrm{se}_q$. The inputs are

```math
n_q = \frac{n_q^{9}}{0.3}\,w\,\varpi, \qquad
\mathrm{se}_q = \mathrm{se}_q^{9}\sqrt{\frac{n_q^{9}}{n_q}}\;\eta, \qquad
r_q = \frac{k\,r^{\ast}_q}{1 + (k-1)\,r^{\ast}_q},
```

with $r^\ast_q = r_q^{9}$ (the point truth) or EXP-0009's own Fisher-$z$ lower bound at $n_q^9$ (the lower truth).

## 3. Estimators

- **(E1) The coefficient** (`fit`, statsmodels `OLS`): $\hat\beta_q$, the coefficient of $z$ in the least squares fit of $y$ on $X = [1, z, \bar q, W]$, $p = 14$ columns in the primary (`design`, `base_terms`).
- **(E2) The HC3 standard error** (`fit`, `cov_type="HC3"`):

```math
\widehat V_{\text{HC3}} = (X^\top X)^{-1}\Big(\sum_i \frac{\hat e_i^2}{(1-h_{ii})^2}X_iX_i^\top\Big)(X^\top X)^{-1}, \qquad
\widehat{\mathrm{se}}_q = \sqrt{[\widehat V_{\text{HC3}}]_{zz}}.
```

- **(E3) The intervals and the p-value** (`fit`): $\hat\beta_q \pm z_\ell\,\widehat{\mathrm{se}}_q$, $z_\ell = \Phi^{-1}(1 - (1-\ell)/2)$, for $\ell \in \{0.80, 0.95, 0.975, \gamma\}$ (keys `ci80`, `ci95`, `ci97.5`, `ci99.375`); $p_q = 2\{1 - \Phi(|\hat\beta_q|/\widehat{\mathrm{se}}_q)\}$.
- **(E4) Holm** (`read_family`, statsmodels `multipletests(method="holm")`), over the eight keys, a key that cannot be estimated entering with $p = 1$: with $p_{(1)} \le \dots \le p_{(m)}$, reject $H_{(1)}, \dots, H_{(k)}$ for the largest $k$ with $p_{(j)} \le \alpha/(m - j + 1)$ for every $j \le k$; $\tilde p_{(k)} = \max_{j \le k}\min\{1, (m - j + 1)p_{(j)}\}$.
- **(E5) The control** (`read_family`): holds when $p_c < 0.05$ and $\hat\beta_c > 0$.
- **(E6) The reliability** (`reliability`): $\hat\rho_q$ over the $n'_q$ players with both halves;

```math
\hat r_q = \max\Big(0.01, \frac{2\hat\rho_q}{1 + \hat\rho_q}\Big), \qquad
\rho_L = \tanh\Big(\operatorname{atanh}\hat\rho_q - \frac{1.96}{\sqrt{n'_q - 3}}\Big), \qquad
r_{L,q} = \min\Big(\hat r_q, \max\Big(0.01, \frac{2\rho_L}{1 + \rho_L}\Big)\Big),
```

  with $r_{L,q} = 0.01$ when $\rho_L \le 0$; $d_q = \delta\sqrt{r_{L,q}}$; the disattenuated effects $\hat\beta_q/\sqrt{\hat r_q}$ and $\hat\beta_q/\sqrt{r_{L,q}}$.
- **(E7) The reads and the outcome** (`read_family`, `decide`): "precedes climbing" if Holm rejects and $\hat\beta_q > 0$; "precedes falling" if Holm rejects and $\hat\beta_q < 0$; "below the minimum effect" if the $\gamma$-interval lies inside $(-d_q, d_q)$; otherwise "undetermined". Outcome 1 if any key climbs; else outcome 2 if all of $\mathcal K_2$ read "below" and the control holds; else outcome 3.
- **(E8) The luck share and the memory ratio** (`sizing_measures`, `memory_ratio`): $\hat E[s_A] = \frac1{n_O}\sum_{i \in O}\varphi^{g_{A,i}}(\varphi^{2g_{J,i}} - 1)$ over the $n_O$ eligible players with an outcome; $M_\varphi = \hat E[s_A]/\hat E_9[s_0]$, the denominator EXP-0009's mean of $\varphi^{g} - 1$ over its 16,206 players' October games (`dev/exp09_activity.json`, `E_share_exp0009_phi*`).
- **(E9) The registered bias** (`biases`): $B_q = \Delta_q\, M\, D\, \mu$ with $\Delta_q = \hat\beta^{9,\text{primary}}_q - \hat\beta^{9,\text{adjusted}}_q$, $M = \max(M_{0.97}, M_{0.98})$, $D = 1/\sqrt3$.
- **(E10) Spearman-Brown's $k$ and the projected games** (`k_of`, `decide_size`): $k = E_9[1/m]/\hat E[1/\min(m, \text{kept})]$ with $E_9[1/m] = 0.04807$ (EXP-0009's months capped at 40); games $= \frac{17{,}138}{0.3}\,w\,\varpi_{\text{obs}}\cdot 2\,\hat E[\min(m, \text{read})]$.
- **(E11) The costs** (`costs`): CPU-hours $= h_s + 2\,\text{games}/10^6 \times 1.5/0.877072$, wall hours $= 2\,\text{games}/10^6 \times 0.8/0.877072 + 8$, with $h_s$ the streaming's CPU-hours.
- **(E12) The sizing decision** (`decide_size`): for each cap (read, kept) in the order (144, 120), (96, 80), (72, 60), the largest $w \in \{0.1, \dots, 0.6\}$ whose games $\le 5{,}550{,}000$, CPU-hours $\le 25$ and wall hours $\le 24$; at it, the target (E13); the first cap that passes is the design, and if none does, the kill rule.
- **(E13) The target** (`target`), each probability estimated by (M7) with 20,000 replicates (5,000 for each single key): every key of $\mathcal K_2$ reads "below" with probability $\ge 0.80$ at $b = 0$; each alone at $b = \delta$ reads "precedes climbing" with probability $\ge 0.80$; the control holds with probability $\ge 0.80$ at $b_c = 7$; all with $B_q$ at $\mu = 1$.
- **(E14) Stage 2's price** (`stage2`): $n^\ast_q = \big((z_\gamma + z_{0.90})\,\mathrm{se}_q\sqrt{n_q}/(\delta\sqrt{r_q})\big)^2$ for each key at the point truth, the median of the eight ($(n^\ast_{(4)} + n^\ast_{(5)})/2$), times 240 games at 39.1 CPU-seconds.

## 4. Derivations and proofs

> **Finding 1 (the level leak's registered sizes mix two level effects).** Result 5 proves section 2's claims in sign: adjusting for the window mean leaves a leak $-(\theta_1 - \theta_2)c/\sqrt{2(c+s)}$ per observed SD, negative when the level effect sits on the earlier window, while the F1-level model's leak is $+\theta_1\ell$. Their ratio $\ell/\{c/\sqrt{2(c+s)}\}$ does not depend on $\theta$. Under (M2), with EXP-0009's reliabilities through $k = 2.64$, it is 4.8 for accuracy and 17 for W7.prep. The registered sizes "+0.8 to +1.0 Elo per observed SD for accuracy and W7.prep" and "−6% of the minimum, against +33%" cannot both hold for one $\theta$. At half of each key's level effect per true SD ($\theta_1 = 3.66$ for accuracy, $6.22/\sqrt{0.722}/2$, and 3.68 for W7.prep), the F1-level leak is $+0.83$ for accuracy and $+1.05$ for W7.prep, as registered, and the window-mean leak is $-3.1\%$ of the minimum. At the whole effect ($\theta_1 = 7.33$) the window-mean leak is $-6.2\%$, as registered, and the F1-level leak is $+29.8\%$. The registered $+33\%$ is closest to the one-month value at $\theta_1 = 3.66$, which is 35%. Either way the primary's leak is small and negative, and the F1-level model's is positive and larger (4.8 times for accuracy), so nothing in the design or the rule changes. Checked by `N11`.

> **Finding 2 (the dilution $1/\sqrt3$ assumes F2's first two months have been forgotten).** Result 9 shows that the shared luck per observed SD of a three-month exposure is $D = \frac{1 + \varphi^G + \varphi^{2G}}{3}\sqrt{r_k/r_1}$ times a one-month exposure's, under (M5) with equal months of $G$ games. Here $r_k$ is Spearman-Brown at the registration's $k$, 2.64 at the plan and 2.85 at the sizing. When $\varphi^G \to 0$, $D = \frac13\sqrt{k/(1 + (k-1)r_1)}$, below the registered $1/\sqrt3$ for any $k \le 3$. So $1/\sqrt3$ is the "less favourable end" only when the luck of January and February is gone from F2's end rating. With accuracy's $r_1 = 0.067$, $\sqrt3\,D$ is as follows:
>
> | Rapid games a month | φ 0.97, k 2.64 | φ 0.98, k 2.64 | φ 0.97, k 2.85 | φ 0.98, k 2.85 |
> |---|---|---|---|---|
> | 121 (EXP-0009's median) | 0.91 | 0.97 | 0.94 | 1.01 |
> | 40 | 1.23 | 1.46 | 1.27 | 1.51 |
> | 20 | 1.64 | 1.88 | 1.69 | 1.94 |
>
> For light players, the registered bias understates the model's by up to these factors. EXP-0009's Finding 2 (the five-game end rating) adds a factor of 1.09 at 40 games and 1.15 at 20 (Result 8 (2)). At 40 games a month, the $\times1.5$ sensitivity covers φ 0.97 and the $\times2$ sensitivity covers both. At 20 games a month, the product reaches about 1.9 to 2.2, at or beyond $\times2$. The memory ratio weights those players most, because their shares are largest. This is a further reason, beside deviation 2, to read the bias's size as assumed. Checked by `R9`, `S6` and `N12`.

> **Finding 3 (one quoted range leaves out a cell).** Section 0 and `summary.md` say that at the lower-bound truth "outcome 3 is 0.82 to 0.93 at every bias". Over the controls of 5 and 7 Elo per true SD that is right (0.8206 to 0.9268). With the control at 10, as section 0's sentence lists, the lowest cell is 0.8087 (`power.json` `truth=lower,bias_x1,null_control10`), so the range is 0.81 to 0.93. The difference is 0.011 and changes nothing. Checked by `N10`.

> **Finding 4 (the planned outcome probabilities let a Holm-rejected key read "below"; the run does not).** In `analysis.py` `read_family`, a key that Holm rejects reads "precedes climbing" or "precedes falling", never "below the minimum effect". So a key of $\mathcal K_2$ rejected with $\hat\beta < 0$ blocks outcome 2. `power.py` `simulate` computes `below` without that exclusion, and stage 1 of this file's `S7a` copied it. Under the registered bias a falling rejection is not rare: at $b = 0$ each key of $\mathcal K_2$ has $P(p \le 0.05)$ of 0.050 to 0.061 at $\times1$ and up to 0.096 at $\times2$. Simulated under `analysis.py`'s reading (40,000 replicates, point truth), outcome 2 at the plan is:
>
> | Cell | `power.json` | analysis.py's reading |
> |---|---|---|
> | ×1, control 5 | 0.8460 | 0.8277 |
> | ×1, control 7 | 0.9816 | 0.9498 |
> | ×1, control 10 | 0.9869 | 0.9565 |
> | ×2, control 5 | 0.6909 | 0.6665 |
>
> Averaged over the three controls at $\times1$, outcome 2 is about 0.91, not 0.94, and outcome 3 about 0.075, not 0.05 (section 0; `summary.md`, "about 94%" and "about 5%"). The sizing decision is unaffected: under the same reading its target still passes at the chosen size, with every tested probability at least 0.99. The decision rule is `analysis.py`'s, so the run reads correctly. Only the planned probabilities of outcomes 2 and 3 are optimistic, by 0.02 to 0.03. Checked by `N13`.

> **Finding 5 (section 2's luck-bias figures use stage 1's ratio).** Section 2 says the residual luck bias "for accuracy [is] −1.5 Elo per observed SD at 20 rapid games in April, −0.7 at 40". Those figures divide this design's share by EXP-0009's share at the same game count (−0.383/−0.456 at 20 games, φ 0.97), stage 1's ratio, which review N1 replaced. Under the registered ratio (E8, E9), with EXP-0009's measured mean share as the denominator, a player at 20 and at 40 April games has −0.84 and −0.81 at the worse φ. That is smaller than quoted, so the quoted figures overstate the bias. Section 4's registered bias (−0.31 at the plan, −0.30 at the sizing) is correct. Checked by `N14`.

**Result 1 (the coefficient is a partial regression in observed SD units).** $\hat\beta_q = s_x\sum_i\tilde x_i y_i/\sum_i\tilde x_i^2$, with $\tilde x$ the residual of $x$ on $[1, \bar q, W]$. Under (M1) it converges in probability to $\beta_q$. If $E[y \mid X]$ is linear in $X$, $E[\hat\beta_q \mid X] = \beta_q$.

*Proof.* `design` replaces the first column by $z = (x - \bar x)/s_x$. By the Frisch-Waugh-Lovell theorem, the coefficient of $z$ in the fit on $[1, z, \bar q, W]$ is $\sum_i\tilde z_i y_i/\sum_i\tilde z_i^2$, with $\tilde z$ the residual of $z$ on $[1, \bar q, W]$. Residualising is linear and the constant absorbs $\bar x$, so $\tilde z = \tilde x/s_x$, and the coefficient is $(\sum_i\tilde x_iy_i/s_x)/(\sum_i\tilde x_i^2/s_x^2) = s_x\sum_i\tilde x_iy_i/\sum_i\tilde x_i^2$. Consistency: $\hat b = (X^\top X/n)^{-1}(X^\top y/n) \to E[XX^\top]^{-1}E[Xy]$ by the law of large numbers and continuity, and $s_x \to \operatorname{SD}(x)$. Unbiasedness: if $E[y \mid X] = Xb$ then $E[\hat b \mid X] = (X^\top X)^{-1}X^\top Xb = b$, conditionally on the design, which fixes $z$ and so $s_x$. $\blacksquare$ Checked by `R1`.

**Result 2 (the HC3 variance in scalar form, and its consistency).** The $z$ element of (E2) is

```math
\widehat{\mathrm{se}}_q^2 = \frac{\sum_i\tilde z_i^2\,\hat e_i^2/(1-h_{ii})^2}{\big(\sum_i\tilde z_i^2\big)^2},
```

at least the HC0 value, and $n_q\widehat{\mathrm{se}}_q^2$ converges to the sandwich variance $E[\tilde z^2e^2]/E[\tilde z^2]^2$ under (M1), whatever the heteroskedasticity.

*Proof.* With $A = (X^\top X)^{-1}X^\top$, $\hat b = Ay$, and by Frisch-Waugh-Lovell the row of $A$ for $z$ is $\tilde z^\top/\tilde z^\top\tilde z$. So $[\widehat V_{\text{HC3}}]_{zz} = \sum_iA_{zi}^2\hat e_i^2/(1-h_{ii})^2$, which is the display. Since $0 \le h_{ii} < 1$, each weight $(1-h_{ii})^{-2} \ge 1$, so HC3 is at least HC0 term by term. HC0 is White's (1980) estimator, consistent under (M1) with finite fourth moments. Under (M1), $\sum_ih_{ii} = p = 14$ and $\max_ih_{ii} \to 0$ in probability (for iid rows with finite second moments $\max_i\lVert X_i\rVert^2/n \to 0$), so $\max_i|(1-h_{ii})^{-2} - 1| \to 0$ and HC3 over HC0 tends to 1. $\blacksquare$ At the plan the smallest $n_q$ is W9b's 2,932 (every other quantity has more than 13,000), so $\bar h = p/n_q \le 0.0048$; at the sizing's density it is about 2,471, $\bar h \le 0.0057$. Checked by `R2` and `S1`.

**Result 3 (the intervals cover, and the eight simultaneous ones cover together).** Each interval of (E3) covers $\beta_q$ with probability tending to $\ell$. The eight $\gamma$-intervals of the keys cover all eight $\beta_q$ together with probability at least $1 - \alpha = 0.95$ in the limit, under any dependence between the fits.

*Proof.* By Result 1 and the central limit theorem, $\sqrt{n_q}(\hat\beta_q - \beta_q) \to N(0, V)$, and by Result 2 $n_q\widehat{\mathrm{se}}_q^2 \to V$. By Slutsky's theorem $t_q = (\hat\beta_q - \beta_q)/\widehat{\mathrm{se}}_q \to N(0, 1)$, so $P(|t_q| \le z_\ell) \to 2\Phi(z_\ell) - 1 = \ell$. Let $A_q$ be the event that key $q$'s $\gamma$-interval misses $\beta_q$; $P(A_q) \to 1 - \gamma = \alpha/m$. By the union bound, $P(\bigcup_{q \in \mathcal K}A_q) \le \sum_qP(A_q) \to m\cdot\alpha/m = \alpha$, with no assumption on how the $A_q$ depend on each other. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.bonferroni`. Its hypothesis, $P(A_q) \le \alpha/m$ for each event, holds here only in the limit, so the Lean bound applies to the limiting experiment; `S1` measures the finite-sample coverage with heteroskedastic, heavy-tailed ($t_5$) errors and the primary's $p = 14$: 0.9500 at 95% and 0.9929 at 99.375%, $n = 800$, 20,000 replicates, which separates 0.99375 from 0.99. The Lean files are on main, which this branch predates. Checked by `R7` ($z_\gamma = 2.734369$) and `S1`.

**Result 4 (Holm's error control, the control's test, and wrong-sign reads).** (a) With p-values valid under their point nulls, Holm (E4) rejects at least one true null among the eight keys with probability at most $\alpha = 0.05$, under any dependence; a key that cannot be estimated enters with $p = 1$, which is valid, so the bound holds with the family always eight. Under section 2's $H_0$ with no bias ($\beta_q = 0$ for every key), outcome 1 has probability at most 0.05. (b) For one key with $\beta_q \le 0$, "precedes climbing" has probability at most $\alpha/2 = 0.025$ in the limit; across the keys with $\beta_q \le 0$, at most $0.025$ times their number. (c) If $\beta_c \le 0$, the control holds with probability at most 0.025 in the limit.

*Proof.* (a) Let $I_0$ be the true nulls, $m_0 = |I_0|$. Suppose Holm rejects some $i \in I_0$. Let $j$ be the first true null in Holm's sorted order (ties broken as the procedure breaks them). It comes no later than $i$, so Holm reached $j$ and rejected it, at its position $k$ in the sorted order, against $\alpha/(m - k + 1)$. No true null precedes $j$, so at least $m_0$ hypotheses remain at step $k$: $m - k + 1 \ge m_0$ and $p_j \le \alpha/m_0$. The event therefore lies inside $\bigcup_{j \in I_0}\{p_j \le \alpha/m_0\}$, whose probability is at most $m_0\cdot\alpha/m_0 = \alpha$ by the union bound and validity. A constant $p = 1$ satisfies $P(p \le t) \le t$ for $t \in [0, 1)$, and $P(p \le t) = 1 \le t$ for $t \ge 1$. Outcome 1 needs a rejection, so under $H_0$ its chance is at most the FWER. (b) Holm's largest threshold is $\alpha$, so a rejection needs $p_q \le \alpha$; with $\hat\beta_q > 0$ that is $\hat\beta_q/\widehat{\mathrm{se}}_q \ge z_{0.975}$. With $\beta_q \le 0$, $\hat\beta_q/\widehat{\mathrm{se}}_q = t_q + \beta_q/\widehat{\mathrm{se}}_q \le t_q$, so the probability is at most $P(t_q \ge z_{0.975}) \to 0.025$. The union bound gives the rest. (c) is (b) for the control's own test at 0.05. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.holm_fwer` (with `Rematch.holm_rejects_true_null` for the deterministic step). Its hypotheses are valid p-values for the true nulls and point nulls; here $p_q$ is an asymptotic HC3 $z$-test, valid only in the limit. Parts (b) and (c) are not in Lean. Lean's `holmRejects` uses a tie-aware threshold no larger than statsmodels' for tied p-values, so it bounds a subset of statsmodels' rejections; the only ties here are keys that cannot be estimated, at $p = 1$, which are never rejected. Under the registered bias every key's $\beta_q$ is negative at $b_q = 0$, so (a)'s point-null bound does not apply as stated. (b) does, and gives at most $8 \times 0.025 = 0.2$ for outcome 1 under no link. The planning simulation gives 0.013 (`power.json`), because a negative mean makes a positive rejection rarer. Checked by `S2` (the FWER, 0.0451 over eight null keys at $\rho = 0.3$; Holm by adjusted p-values against a direct step-down; a single null key's climbing rate, 0.0031).

**Result 5 (the window-mean level).** Under (M2) and (M6), with $\tau = 1$:

(a) $\operatorname{Cov}(x, \bar q) = (\sigma_2^2 - \sigma_1^2)/2$. With equal noise the change and the window mean are uncorrelated.

(b) With equal noise ($s_1 = s_2 = s$), the population coefficient on the standardised $x$, in the regression of $y$ on $x$ and $\bar q$, is

```math
\beta = b\sqrt{r_\Delta} - (\theta_1 - \theta_2)\frac{c}{\sqrt{2(c + s)}}, \qquad r_\Delta = \frac{c}{c+s}.
```

  Relative to the minimum $\delta\sqrt{r_\Delta}$, the leak is $-(\theta_1 - \theta_2)\sqrt{c/2}/\delta$, which does not depend on the noise. The F1-level model instead gives $b\sqrt{r_\Delta} + \theta_1\ell$ with $\ell = \sqrt2\,s(1-c)/\{\sqrt{c+s}(2 + s - c)\}$ (EXP-0009 `math.md` Result 6, at $\theta_2 = 0$).

(c) With unequal noise,

```math
\beta = \sqrt{V_x}\;\frac{C_{yx}V_{\bar q} - C_{y\bar q}C_{x\bar q}}{V_xV_{\bar q} - C_{x\bar q}^2},
```

  with $V_x = 2c + s_1 + s_2$, $V_{\bar q} = (4 - 2c + s_1 + s_2)/4$, $C_{x\bar q} = (s_2 - s_1)/2$, $C_{yx} = b\sqrt{2c} + (\theta_2 - \theta_1)c$ and $C_{y\bar q} = (\theta_1 + \theta_2)(2 - c)/2$. The level effect $\theta_1 + \theta_2$ then leaks in through the noise imbalance $C_{x\bar q}$, which the inverse-variance-weighted sensitivity targets.

*Proof.* (a) $x = T_2 - T_1 + u_2 - u_1$ and $\bar q = \frac12(T_1 + T_2 + u_1 + u_2)$. By bilinearity, with the noises uncorrelated with each other and with $T$:

```math
\operatorname{Cov}(x, \bar q) = \tfrac12\big(\operatorname{Var}T_2 - \operatorname{Var}T_1\big) + \tfrac12\big(\operatorname{Var}u_2 - \operatorname{Var}u_1\big)
  = 0 + \tfrac12(\sigma_2^2 - \sigma_1^2),
```

  the cross terms $\operatorname{Cov}(T_2, T_1) - \operatorname{Cov}(T_1, T_2)$ and $\operatorname{Cov}(u_2,u_1) - \operatorname{Cov}(u_1,u_2)$ cancelling, and $\operatorname{Var}T_1 = \operatorname{Var}T_2$ by stationarity.

(c) The moments: $V_x = \operatorname{Var}D + s_1 + s_2 = 2c + s_1 + s_2$. $V_{\bar q} = \frac14(\operatorname{Var}T_1 + \operatorname{Var}T_2 + 2\operatorname{Cov}(T_1, T_2) + s_1 + s_2) = \frac14(2 + 2(1-c) + s_1 + s_2)$. $C_{x\bar q}$ is (a). For $y$: $\operatorname{Cov}(D, x) = \operatorname{Var}D = 2c$, so the $b$ term gives $b\cdot2c/\sqrt{2c} = b\sqrt{2c}$; $\operatorname{Cov}(T_1, x) = \operatorname{Cov}(T_1, T_2) - \operatorname{Var}T_1 = -c$ and $\operatorname{Cov}(T_2, x) = c$, so $C_{yx} = b\sqrt{2c} + (\theta_2 - \theta_1)c$. $\operatorname{Cov}(D, \bar q) = \frac12(\operatorname{Var}T_2 - \operatorname{Var}T_1) = 0$ and $\operatorname{Cov}(T_t, \bar q) = \frac12(1 + 1 - c)$ for each $t$, so $C_{y\bar q} = (\theta_1 + \theta_2)(2 - c)/2$. The coefficient of $x$ in the population regression on $(x, \bar q)$ is, by the two-regressor normal equations (Cramer's rule), $(C_{yx}V_{\bar q} - C_{y\bar q}C_{x\bar q})/(V_xV_{\bar q} - C_{x\bar q}^2)$; per observed SD it is multiplied by $\operatorname{SD}(x) = \sqrt{V_x}$.

(b) With $s_1 = s_2 = s$, $C_{x\bar q} = 0$, so $\beta = \sqrt{V_x}\,C_{yx}/V_x = C_{yx}/\sqrt{V_x}$:

```math
\beta = \frac{b\sqrt{2c} + (\theta_2 - \theta_1)c}{\sqrt{2(c + s)}} = b\sqrt{\frac{c}{c+s}} - (\theta_1 - \theta_2)\frac{c}{\sqrt{2(c+s)}}.
```

  Dividing the leak by $\delta\sqrt{c/(c+s)}$ gives $-(\theta_1 - \theta_2)\frac{c}{\sqrt{2(c+s)}}\sqrt{\frac{c+s}{c}}/\delta = -(\theta_1 - \theta_2)\sqrt{c/2}/\delta$. The F1-level formula is EXP-0009's Result 6, whose proof is the same calculation with $q_1$ in place of $\bar q$. $\blacksquare$

So section 2's "its noise is uncorrelated with the change's noise when the two windows are equally noisy" is (a), and "the remaining leak is small and negative" is (b) when the level effect sits on F1 ($\theta_1 > \theta_2$). If instead the more recent level matters more ($\theta_2 > \theta_1$) the leak is positive, of the same small size. The numbers are in Finding 1. The model leaves out the other covariates; with them, the formulas hold for the variables residualised on $W$, if (M2) and (M6) hold after residualising.

Machine-checked: `lab/lean/Rematch/WindowMean.lean` theorem `Rematch.window_mean_independence`, for the noise part of (a): with $e_1, e_2$ square-integrable and uncorrelated, $\operatorname{Cov}(q_2 - q_1, \bar q - \theta - \delta/2) = (\operatorname{Var}e_2 - \operatorname{Var}e_1)/2$, and 0 with equal variances. The Lean statement fixes one player's $\theta$ and $\delta$; the across-player part, $\operatorname{Cov}(T_2 - T_1, (T_1 + T_2)/2) = 0$ under stationarity, and parts (b) and (c) are proved here only. Checked by `R5` (sympy on the covariance algebra; the equal-noise identity on a dense grid where sympy cannot cancel the square roots), `S3` (400,000 simulated players, three settings, within 1.3 SE) and `N11`.

**Result 6 (Spearman-Brown, the Fisher-$z$ lower bound, and the harmonic $k$).** (a) Under (M3), $r = 2\rho/(1 + \rho)$, with $\rho$ the halves' correlation, and a window whose noise variance is $1/k$ times as large has reliability $kr/(1 + (k-1)r)$. (b) If $\operatorname{atanh}\hat\rho$ is asymptotically $N(\operatorname{atanh}\rho, 1/(n' - 3))$, then for a true $r \ge 0.01$, $P(r_L \le r) \to 0.975$. (c) Under (M3), with a window's noise $\sigma_g^2/m$, the design-to-design noise ratio is $k = E_9[1/m]/E[1/m]$, the harmonic ratio, not the ratio of mean game counts.

*Proof.* (a) Each half measures $D$ with noise variance $2\sigma^2$, the window's $\sigma^2$. With $v = \operatorname{Var}D/\sigma^2$, $\rho = v/(v + 2)$ and $r = v/(v + 1)$. Then $v = 2\rho/(1 - \rho)$ and $r = 2\rho/(1+\rho)$. With noise $\sigma^2/k$, $r_k = kv/(kv + 1)$; with $v = r/(1 - r)$ this is $kr/(1 + (k-1)r)$. (b) $f(\rho) = 2\rho/(1 + \rho)$ has $f'(\rho) = 2/(1+\rho)^2 > 0$, so it is increasing, and so are $\tanh$ and $\max(0.01, \cdot)$. The event $\rho_L \le \rho$ is $\operatorname{atanh}\hat\rho - 1.96/\sqrt{n'-3} \le \operatorname{atanh}\rho$, whose probability tends to $\Phi(1.96) = 0.975$. On it, $\max(0.01, f(\rho_L)) \le \max(0.01, f(\rho)) = r$ for $r \ge 0.01$. The outer $\min(\hat r, \cdot)$ changes nothing, since $\rho_L < \hat\rho$ gives $f(\rho_L) < f(\hat\rho)$. So $P(r_L \le r) \ge P(\rho_L \le \rho) \to 0.975$. When $\rho_L \le 0$, $r_L = 0.01 \le r$. (c) The population noise variance of a window measure is $E[\sigma_g^2/m] = \sigma_g^2E[1/m]$, so $v$ scales by $E_9[1/m]/E[1/m] = k$, and (a) applies. $\blacksquare$

The registration's $k = 0.0481/0.0182 = 2.64$ is (c) applied to the capped counts. The Fisher-$z$ variance $1/(n'-3)$ is exact only for bivariate normal halves; the halves are changes in means of many games, close to normal by the central limit theorem. Checked by `R6` and `S4` (coverage 0.9772 against 0.975, $n' = 2{,}000$, 4,000 replicates).

**Result 7 (a "below the minimum effect" read and its error).** Let $\beta_q = b_q\sqrt{r_q}$ (no bias). If $|b_q| \ge \delta$, key $q$ reads "below" with probability at most $P(A_q) + P(r_{L,q} > r_q) \to 0.00625 + 0.025 = 0.03125$. Outcome 2's claim, $|b_q| < \delta$ for every key of $\mathcal K_2$, is false with probability at most $5 \times 0.00625 + 5 \times 0.025 = 0.156$ in the limit, under any dependence, and at most 0.03125 when only one key is truly above the minimum.

*Proof.* On $\{r_{L,q} \le r_q\}$, $d_q = \delta\sqrt{r_{L,q}} \le \delta\sqrt{r_q} \le |b_q|\sqrt{r_q} = |\beta_q|$. A "below" read needs the $\gamma$-interval inside $(-d_q, d_q)$, so if the interval covers $\beta_q$, $|\beta_q| < d_q$, a contradiction. So a wrong read needs $A_q$ (the interval misses $\beta_q$) or $r_{L,q} > r_q$. By Result 3, $P(A_q) \to \alpha/8 = 0.00625$; by Result 6 (b), $P(r_{L,q} > r_q) \to 0.025$. For the family: a false claim needs a wrong read at some false key, inside $\bigcup_{q \in \mathcal K_2}(A_q \cup \{r_{L,q} > r_q\})$, of probability at most $5 \times 0.00625 + 5 \times 0.025$ by Results 3 and 6 and the union bound. A Holm rejection only removes "below" reads (E7), so the bound holds for the rule as `read_family` applies it. $\blacksquare$

With a negative bias $B_q$ and $b_q > 0$, $\beta_q = b_q\sqrt{r_q} + B_q$ is pulled towards 0, and the proof's step $|\beta_q| \ge d_q$ can fail. Result 7 then does not bound the error; Result 13 says what does. Checked by `R7`.

**Result 8 (the luck shares).** Under (M4), let F2's rapid games be $t = 1, \dots, N$, April's the next $g_A$, May's $g_M$ and June's $g_J$, all at least $L = 5$. The luck $\xi_k$ of an F2 game has weight $w_k$ in $R_{\text{end}}(\text{F2})$ and weight $w_k^y$ in $y$, where:

1. for $k \le N - L + 1$, exactly $w_k^y = \varphi^{g_A}(\varphi^{g_M + g_J} - 1)\,w_k$, which with $g_M = g_J$ is $s_A\,w_k$;
2. summed over all of F2's games (equally lucky games), the outcome carries the share

```math
\frac{\sum_k w^y_k}{\sum_k w_k} = \varphi^{g_A}\big(\varphi^{g_M + g_J} - 1\big)\,\frac{\bar a(1 - \varphi^N)}{1 - \varphi^N\bar a} \ \xrightarrow{N \to \infty}\ \bar a\,\varphi^{g_A}\big(\varphi^{g_M + g_J} - 1\big);
```

3. $s_A \in (-1, 0]$; $|s_A|$ is non-increasing in $g_A$; $s_A = 0$ exactly when $g_J = 0$; and EXP-0009's $s_0 = \varphi^g - 1 \in (-1, 0]$;
4. with $g$ games in each of April, May and June, the end-rating outcomes carry $\varphi^{3g} - \varphi^g$ (design A, the gap month) and $\varphi^{3g} - \varphi^{2g}$ (design B, two gap months), and the month-mean outcome, the mean over May and June minus the mean over April, carries $\bar m(g, 2g) - \bar m(0, g)$ with $\bar m(a, n) = \frac1n\sum_{t=a+1}^{a+n}\varphi^t = \varphi^{a+1}(1 - \varphi^n)/\{n(1 - \varphi)\}$, and EXP-0009's month mean $\bar m(0, g) - 1$.

*Proof.* Iterating (M4), $\xi_k$ enters $e_t$ with coefficient $\varphi^{t-k}$ for $t \ge k$ and 0 before. $R_{\text{end}}$ of a period ending at game $T$ is $\frac1L\sum_{j=0}^{L-1}R_{T-j}$. So $w_k = \frac1L\sum_{j:\,N-j\ge k}\varphi^{N-j-k}$. April's and June's last five games all come after $k$, because $g_A, g_J \ge L$, so

```math
w_k^y = \frac1L\sum_{j=0}^{L-1}\Big(\varphi^{N + g_A + g_M + g_J - j - k} - \varphi^{N + g_A - j - k}\Big)
  = \varphi^{g_A}\big(\varphi^{g_M + g_J} - 1\big)\cdot\frac1L\sum_{j=0}^{L-1}\varphi^{N - j - k}.
```

  (1) For $k \le N - L + 1$ every $j$ counts in $w_k$, so the last factor is $w_k$. (2) $\sum_{k=1}^N\frac1L\sum_j\varphi^{N-j-k} = \bar a\sum_{k=1}^N\varphi^{N-k} = \bar a(1-\varphi^N)/(1-\varphi)$, and $\sum_kw_k = \frac1L\sum_j\sum_{k=1}^{N-j}\varphi^{N-j-k} = \frac1L\sum_j\frac{1-\varphi^{N-j}}{1-\varphi} = \frac{1 - \varphi^N\bar a}{1-\varphi}$; their ratio times the common factor is the display, and $\varphi^N \to 0$ gives the limit. (3) $0 < \varphi^{g_A} \le 1$ and $0 < \varphi^{2g_J} \le 1$, so $s_A$ is a product of a number in $(0, 1]$ and one in $(-1, 0]$, which lies in $(-1, 0]$. $|s_A| = \varphi^{g_A}(1 - \varphi^{2g_J})$ is a non-negative constant times $\varphi^{g_A}$, which decreases in $g_A$. $s_A = 0$ iff $\varphi^{2g_J} = 1$ iff $g_J = 0$. The bound on $s_0$ is the same argument. (4) With equal $g$, (1) gives $\varphi^g(\varphi^{2g} - 1) = \varphi^{3g} - \varphi^g$. For design B the baseline is May's end, $\varphi^{2g}$, and the outcome June's, $\varphi^{3g}$. A month mean of ratings after games $a + 1, \dots, a + n$ weights the luck by the mean of $\varphi^t$ over them, and the geometric sum gives $\bar m$. EXP-0009's month-mean baseline is $R_{\text{end}}(\text{P2})$, weight 1. $\blacksquare$

Machine-checked: `lab/lean/Rematch/LuckShare.lean` theorem `Rematch.luck_share`, which is part (3) exactly: `luckShare` is $\varphi^{g_A}(\varphi^{2g_J} - 1)$ and `exp09Share` is $\varphi^g - 1$, for $0 < \varphi < 1$. Parts (1), (2) and (4), the weight identities that make these the shares, are proved here only. Part (2) is EXP-0009's Finding 2 carried over: the aggregate share is $\bar a$ times the registered one in the limit, and with EXP-0009's denominator built the same way, $\varphi^g\bar a - 1$, the registered memory ratio is low by the factor $\bar a(1 - \varphi^g)/(1 - \bar a\varphi^g)$, 1.066 and 1.046 at 121 games, 1.15 and 1.14 at 20 (EXP-0009 `math.md`, Finding 2; deviation 2). Checked by `R8` (sympy on the weights; the aggregate and the mean on grids), `S5` (100,000 simulated players: the early game's share $-0.4217$ against $-0.4252$, within 4 SE of 0.011, which separates it from $\bar a$ times it, a gap of 0.027; the aggregate $-0.549$ against $-0.551$) and `N6`.

**Result 9 (the memory ratio, the registered bias, and the dilution).** Under (M1), (M4) and (M5), take a one-month exposure (EXP-0009's) and this design's three-month exposure, with the same per-game noise, the same true-change variance and the same per-game covariance $\kappa = \operatorname{Cov}(\xi_k, u_k)$ between a game's luck and its contribution $u_k$ to the measure. Then:

(a) the shared-luck bias of a primary coefficient is $B = \operatorname{SD}(x)\,E[\tilde x\Lambda]/E[\tilde x^2]$, $\Lambda = \sum_kw^y_k\xi_k$. With the counts fixed and a one-game end rating ($L = 1$), $E[\tilde x\Lambda] = s\,K$, with $s$ the player's share ($s_A$ here, $s_0$ in EXP-0009) and $K = \sum_k w_k\operatorname{Cov}(\xi_k, \tilde x)$ the covariance before the outcome's share. If $K/E[\tilde x^2]$ is the same for every player, the bias averaged over players is $E[s]K\operatorname{SD}(x)/E[\tilde x^2]$, and the ratio of this design's to EXP-0009's is $M_\varphi \cdot D$, with $D$ the ratio of $K\operatorname{SD}(x)/E[\tilde x^2]$ between the designs. With $L = 5$ the ratio is $M_\varphi$ times Result 8 (2)'s factor, 1.05 to 1.15;

(b) the dilution, with F2's months of $G$ games each and every observed game of F2 equally weighted in $q_2$, is

```math
D = \frac{\operatorname{Cov}(x_3/\operatorname{SD}x_3,\ \Lambda)}{\operatorname{Cov}(x_1/\operatorname{SD}x_1,\ \Lambda)} = \frac{1 + \varphi^G + \varphi^{2G}}{3}\sqrt{\frac{r_k}{r_1}}, \qquad r_k = \frac{k\,r_1}{1 + (k-1)r_1},
```

  and when $\varphi^G \to 0$, $D = \frac13\sqrt{k/(1 + (k-1)r_1)}$, which is below $1/\sqrt3$ for $1 \le k \le 3$, and equals $1/\sqrt{3(1 + 2r_1)}$ at $k = 3$;

(c) so $B_q = \Delta_qMD$ is the bias of this design only if EXP-0009's shift $\Delta_q$ was its luck bias, (a)'s condition holds, and $D = 1/\sqrt3$. None of the three is identified by data before the run.

*Proof.* (a) By Result 1 in population form, $\beta_q$ is linear in $y$, so the part from $\Lambda$ is $\operatorname{SD}(x)E[\tilde x\Lambda]/E[\tilde x^2]$. Conditioning on the counts, $E[\tilde x\Lambda] = \sum_kw^y_k\operatorname{Cov}(\xi_k, \tilde x)$. With $L = 1$ every game of F2 comes before its end rating, so Result 8 (1) gives $w^y_k = s\,w_k$ for every $k$, and the sum is $s\,K$. If $K/E[\tilde x^2]$ does not vary over players, averaging gives $E[s]\,K\operatorname{SD}(x)/E[\tilde x^2]$. Dividing this design's by EXP-0009's gives $E[s_A]/E[s_0] = M_\varphi$ times the ratio of the per-design constants, which is $D$. With $L = 5$, the last four games of F2 have $w^y_k \ne s\,w_k$; in aggregate, Result 8 (2) gives the factor. (b) A game of F2 enters $q_2$ with weight $1/(3G)$ here and $1/G$ in EXP-0009's month, so with $\operatorname{Cov}(\xi_k, q_2) = \kappa/(3G)$ and $\kappa/G$, $\operatorname{Cov}(x_3, \Lambda) = \frac{\kappa}{3G}\sum_{k=1}^{3G}w^y_k$ and $\operatorname{Cov}(x_1, \Lambda) = \frac{\kappa}{G}\sum_{k=2G+1}^{3G}w^y_k$. With $w^y_k \propto \varphi^{3G-k}$ (luck at the end rating, $L = 1$), $\sum_{k=1}^{3G}\varphi^{3G-k} = (1 + \varphi^G + \varphi^{2G})\sum_{k=2G+1}^{3G}\varphi^{3G-k}$, because each game of the first month is $\varphi^{2G}$ times the game at the same place in the last month, and each game of the second month is $\varphi^G$ times. So the covariance ratio is $(1 + \varphi^G + \varphi^{2G})/3$. Standardising divides by $\operatorname{SD}(x)$; with $\operatorname{Var}D$ fixed, $r = \operatorname{Var}D/\operatorname{Var}x$, so $\operatorname{SD}(x_1)/\operatorname{SD}(x_3) = \sqrt{r_k/r_1}$, with $r_k$ from Result 6 (a). When $\varphi^G \to 0$, $D = \frac13\sqrt{k/(1 + (k-1)r_1)}$, and since $k/(1 + (k-1)r_1) \le k \le 3$, $D \le 1/\sqrt3$, with equality only at $k = 3$, $r_1 \to 0$. (c) $B_q$ is $\Delta_q$ times (a)'s ratio with $D$ set to $1/\sqrt3$; each step needs its condition. $\blacksquare$

Part (c) is where deviation 2 sits. EXP-0009's `math.md` (Result 12, Finding 1) proves that $\Delta_q = \gamma(a + b)$ while the luck bias is $\lambda b$: catch-up and luck in September's change move the shift even with no shared luck. So $\Delta_q$ is not a measurement of the luck bias, and $B_q$ inherits that. (a)'s condition fails if lighter players share more luck per game. (b) is Finding 2. The sign is robust: under (M5), $s_A \le 0$ (Result 8 (3)) and $\kappa \ge 0$, so the bias is $\le 0$ whatever its size. Checked by `R9` (sympy on the forgotten-months forms, a grid on the bound), `S6` and `N12`. `S6` simulates (M5)'s geometric-weight model, so it checks the derivation's algebra, not the model: 100,000 players at $(\varphi, G) = (0.97, 20)$, $(0.98, 40)$ and $(0.97, 121)$ come within 1% of the formula, where $1/\sqrt3$ is off by about 50% at the lightest setting.

**Result 10 (the planning model's closed forms).** Under (M7):

(a) the control holds with probability exactly $\Phi(\mu_c/\mathrm{se}_c - z_{0.975})$, where $\mu_c = b_c\sqrt{r_c} + B_c$;

(b) a key alone at $b_q = \delta$, the others at $b = 0$, reads "precedes climbing" with probability between $\Phi(\mu_q/\mathrm{se}_q - z_{1-\alpha/16})$ and $\Phi(\mu_q/\mathrm{se}_q - z_{0.975})$;

(c) a key at $b_q = 0$ has the $\gamma$-interval inside $(-d_q, d_q)$ with probability $E\big[\{\Phi((d_q - B_q)/\mathrm{se}_q - z_\gamma) - \Phi((-d_q - B_q)/\mathrm{se}_q + z_\gamma)\}_+\big]$, the expectation over $r_{L,q}$. In the run, the read "below" also needs no Holm rejection (E7), so this is high by at most $P(p_q \le \alpha) = \Phi(-z_{0.975} - B_q/\mathrm{se}_q) + \Phi(-z_{0.975} + B_q/\mathrm{se}_q)$, which is $\alpha$ at $B_q = 0$ and larger under a bias (Finding 4);

(d) each probability in `power.json` is a mean of $S = 20{,}000$ (or 5,000) indicators, with Monte Carlo standard error $\sqrt{\pi(1-\pi)/S}$, at most 0.0035 (or 0.0071).

*Proof.* (a) "Holds" is $p_c < 0.05$ and $\hat\beta_c > 0$, that is $\hat\beta_c/\mathrm{se}_c > z_{0.975}$; with $\hat\beta_c \sim N(\mu_c, \mathrm{se}_c^2)$, $P = 1 - \Phi(z_{0.975} - \mu_c/\mathrm{se}_c) = \Phi(\mu_c/\mathrm{se}_c - z_{0.975})$. (b) If $p_q \le \alpha/8$, every hypothesis sorted before $q$ also has $p \le \alpha/8 \le \alpha/(m - j + 1)$, so Holm rejects $q$; so "climbing" contains $\{\hat\beta_q/\mathrm{se}_q > z_{1-\alpha/16}\}$. Any Holm rejection needs $p_q \le \alpha$, so "climbing" lies inside $\{\hat\beta_q/\mathrm{se}_q > z_{0.975}\}$. Both probabilities follow as in (a). (c) Given $r_{L,q}$, the interval $\hat\beta_q \pm z_\gamma\mathrm{se}_q$ lies inside $(-d_q, d_q)$ iff $-d_q + z_\gamma\mathrm{se}_q < \hat\beta_q < d_q - z_\gamma\mathrm{se}_q$, an empty set if $d_q \le z_\gamma\mathrm{se}_q$; with $\hat\beta_q \sim N(B_q, \mathrm{se}_q^2)$ independent of $r_{L,q}$ by (M7), the probability is the bracket, and the tower rule gives the expectation. The read "below" is this event minus its intersection with a Holm rejection. Any rejection needs $p_q \le \alpha$, that is $|\hat\beta_q|/\mathrm{se}_q \ge z_{0.975}$, and with $\hat\beta_q \sim N(B_q, \mathrm{se}_q^2)$ that has the stated probability. (d) The variance of a mean of $S$ Bernoulli($\pi$) draws, at most $1/(4S)$. $\blacksquare$

With $B_q = 0$, (c) is EXP-0009's $\max\{0, 2\Phi(d/\mathrm{se} - z_\gamma) - 1\}$ at fixed $d$. These forms check `power.py`'s simulation from outside: `S7b` and `S7c` compare (a) with every `control_holds` in `power.json` (24 cells, the gate by memory ratio, both targets), and `S7d` checks (b)'s bounds on every single-key power. `S7a` re-implements (M7) apart from `power.py`, with Holm by adjusted p-values instead of `power.py`'s cumulative product, and reproduces all 96 probabilities of the 24 cells (four each) within 4 combined Monte Carlo SE. `S7d` does the same for both targets' "below" and single-key probabilities, and `S7c` for the gate's verdict at each memory ratio. These reproduce `power.py`'s reading of "below"; `N13` runs the same code with `analysis.py`'s reading (Finding 4).

**Result 11 (the sizing decision is a function of the measured nuisance inputs only).** (a) For each cap, the projected games, CPU-hours and wall hours (E10, E11) increase linearly in $w$, so the set of grid points that fit is $\{w \le w^\ast\}$, and `decide_size`'s descending loop returns its largest element. (b) The decision reads only $\varpi$, $\varpi_{\text{obs}}$, the two moments of $m$ per cap, $\hat E[s_A]$ at two $\varphi$, $\eta$ and $h_s$, none of which involves an exposure, and a fixed seed; so it is a blinded re-estimate of nuisance parameters, and it cannot depend on any link between an exposure and $y$. (c) The density counts are consistent: $\varpi = n_O/0.6/(16{,}204/0.3)$ scales EXP-0009's players with an outcome (the $n$ that (M7) scales), and $\varpi_{\text{obs}} = n_{F1 \cap F2}/0.6/(17{,}138/0.3)$ scales its observed players (the games).

*Proof.* (a) games $= C_1w$ with $C_1 = 2 \cdot \frac{17{,}138}{0.3}\varpi_{\text{obs}}\hat E[\min(m, \text{read})] > 0$; CPU-hours $= h_s + C_2w$ and wall hours $= 8 + C_3w$ with $C_2, C_3 > 0$. Each cap is a constraint $f(w) \le c$ with $f$ increasing, so it holds on an initial segment of the grid, and so does their intersection. A descending scan stops at the first, hence largest, fitting point. (b) By inspection of `sizing_measures` and `decide_size`: the inputs are counts and game totals from the players files and the SD of $y$, with no exposure computed. The target's probabilities are functions of (M7)'s inputs and the seed `seed("EXP-0029", "sizing")`. (c) $[0.3, 0.9)$ has width 0.6 and EXP-0009's slice 0.3; the ratios are per unit of hash. $\blacksquare$

The target is tested with Monte Carlo error (Result 10 (d)); at the chosen size every tested probability is at least 0.9985 against a threshold of 0.80. At the threshold a probability's standard error is 0.0028 with 20,000 replicates and 0.0057 with 5,000, so the margin is at least 35 standard errors, and the decision is not sensitive to the seed. Under `analysis.py`'s reading (Finding 4) the smallest is 0.99. Checked by `N7` and `S7d`.

**Result 12 (stage 2's sample size).** At the point truth with no bias, a key reaches 0.80 in both closed-form powers, $\pi_\uparrow = \Phi(\delta\sqrt{r_q}/\mathrm{se}_q - z_\gamma)$ and $\pi_0 = 2\Phi(\delta\sqrt{r_q}/\mathrm{se}_q - z_\gamma) - 1$, exactly when $n \ge n^\ast_q = \big((z_\gamma + z_{0.90})\mathrm{se}_q\sqrt{n_q}/(\delta\sqrt{r_q})\big)^2$.

*Proof.* $\pi_0 \ge 0.80$ iff $\Phi(\cdot) \ge 0.90$, which implies $\pi_\uparrow \ge 0.90 \ge 0.80$, so $\pi_0$ binds. $\Phi$ is strictly increasing, so $\Phi(t) \ge 0.90$ iff $t \ge z_{0.90} = 1.2816$. With $\mathrm{se}(n) = \mathrm{se}_q\sqrt{n_q/n}$ (Result 2: the HC3 variance scales as $1/n$), $\delta\sqrt{r_q}\sqrt n/(\mathrm{se}_q\sqrt{n_q}) \ge z_\gamma + z_{0.90}$ iff $n \ge n^\ast_q$. $\blacksquare$ This is why `stage2` adds $\Phi^{-1}(0.90)$, not $\Phi^{-1}(0.80)$: the registration's "0.80 in both closed-form powers" is the $\pi_0$ condition. The median of the eight is 5,126.7 players. Checked by `R7` and `N8`.

**Result 13 (the decision rule's error rates).** In the limit:

(a) with no key linked ($b_q = 0$ for every key) and a bias $B_q \le 0$, outcome 1 has probability at most $8 \times 0.025 = 0.20$, and at most $\alpha = 0.05$ when $B_q = 0$;

(b) with no bias, outcome 2 is wrong (some key of $\mathcal K_2$ has $|b_q| \ge \delta$) with probability at most 0.156, and at most 0.031 when one key is above the minimum;

(c) with a bias, (b) holds for keys with $b_q > 0$ only above $\delta + |B_q|/\sqrt{r_q}$: outcome 2 then says the link is below $\delta$ when it is only shown below $\delta + |B_q|/\sqrt{r_q}$. For a key at exactly $b_q = \delta$, the chance its interval misses on the low side grows from $\alpha/16 = 0.003$ to $\Phi(-z_\gamma + |B_q|/\mathrm{se}_q)$. The control limits this: if the same scale of bias applies to the control, then $\mu_c = b_c\sqrt{r_c} + B_c$ falls with it, and the control holds with probability $\Phi(\mu_c/\mathrm{se}_c - z_{0.975})$ (Result 10 (a)), so a large bias makes outcome 3 likelier, not outcome 2.

*Proof.* (a) Outcome 1 needs some key to climb; Result 4 (b) for each key with $\beta_q = B_q \le 0$, and the union bound; with $B_q = 0$, Result 4 (a). (b) Outcome 2 needs every key of $\mathcal K_2$ to read "below"; Result 7. (c) In Result 7's proof, $|\beta_q| = b_q\sqrt{r_q} - |B_q| \ge d_q$ holds when $b_q \ge \delta + |B_q|/\sqrt{r_q}$ and $r_{L,q} \le r_q$; the rest is the same. The control's statement is Result 10 (a) with $B_c$ in $\mu_c$. $\blacksquare$

Part (c)'s guard is an argument, not a bound: it needs the keys' and the control's biases to move together, which the registered bias model ($B_q = \Delta_qMD$ with common $M$ and $D$) assumes and no data before the run checks. At the plan the registered biases are $|B_q|/\sqrt{r_q} \le 0.57$ Elo per true SD for every key of $\mathcal K_2$ (W6 the largest), against $\delta = 14$ (`N7`). If the true bias were EXP-0009's whole shift ($\Delta_q$, 5.9 times the registered $B_q$), that low-side miss would be 0.07 to 0.19 for W4.allowed, W5, W6 and W7.traps (the blind re-derivation's figure); deviation 2 says the size is not known.

## 5. Numbers

The result numbers (each family's estimates, intervals, Holm p-values, $\hat r$ and $r_L$, the reads and the outcome) are added with the decision, once `results.json` exists; `math_check.py` `result_numbers` is their stub. Below, every number committed so far, recomputed by `math_check.py` from EXP-0009's `results.json`, `dev/exp09_activity.json` and `power.json`'s inputs, by code written apart from `power.py`. "Rounded" means the quoted digits agree with the value.

| Quoted as (where) | Formula | Key | Value | Check |
|---|---|---|---|---|
| reliabilities W7.prep 0.019 … W9a 0.131 (section 6) | EXP-0009 | `EXP-0009 primary.<q>.reliability` | 0.0193 … 0.1312 | `N1` |
| bias shift accuracy −3.16 … W9a −0.23 (section 4) | (E9) $\Delta_q$ | `luck_shift_0009` | −3.1585 … −0.2270 | `N2` |
| error ratio 1.34, range 0.94 to 1.71 (section 6) | upper quartile of the level/primary SE | `sd_ratio_from_exp0009.planned` | 1.3415 (W7.prep), 0.9421 to 1.7072 | `N3` |
| $k$ = 0.0481/0.0182 = 2.64; 2.50, 2.31 (section 6, review N3) | Result 6 (c) | `simulation.sizing.k` | 2.6353, 2.5029, 2.3061 | `N4` |
| games per window 70.4, 62.7, 55.7; median 58, p10 33; 1.40 × the players | `dev/exp09_activity.json` | `window_proxy` | 70.36, 62.74, 55.72 | `N4` |
| 16,206 players, median 121, p10 26, 6.8% below 20; shares −0.87, −0.81 | (E8) denominators | `g0` | as quoted | `N5` |
| pre-check 86,819, 18,968, 17,891; median 118, p25 56, p75 213; 82.8% ≥ 40, 90.2% ≥ 25 | – | `precheck` | as quoted | `N5` |
| luck carried −0.46 to −0.84; −0.38 to −0.16; −0.37 to −0.27; −0.38 vs −0.46, −0.37 vs −0.33; −0.03 and −0.09 vs −0.97 and −0.91; month means −0.36 to −0.46; design B about a third (section 4) | Result 8 (4) | `memory[]` | every row, within $5\times10^{-5}$ | `N6` |
| memory ratio 0.11 (φ 0.97), 0.17 (φ 0.98) at the plan | (E8) | `simulation.sizing.memory_ratio` | 0.1092, 0.1700 | `N6` |
| memory ratio at the sizing | (E8) | `sizing.decision.memory_ratio` | 0.1048, 0.1651 | `N6` |
| accuracy's bias −0.31 (section 4), and every $B_q$ | (E9) | `simulation.sizing.bias`, `sizing.decision.bias` | −0.3100; −0.3011 at the sizing | `N7` |
| 1.7 CPU-hours and 0.91 wall hours a million games | (E11) | `CPU_HOURS_PER_MILLION` | 1.710, 0.912 | `N7` |
| about 24,000 players, about 22,700 with an outcome, 3.38 million games, 18.5 CPU-hours, about 14 hours (section 6, summary) | (M7), (E10), (E11) | `simulation.players_at_w`, `costs_at_w` | 23,993.2; 22,685.6; 3,376,086; 18.548; 14.159 | `N7` |
| caps 5,550,000 games, 25 CPU-hours, 24 hours: the plan fits | (E12) | – | yes | `N7` |
| density 0.59 and 0.6958; $k$ 2.8498; 3.62 million games, 15.28 CPU-hours, 14.61 hours | Result 11 (c), (E10), (E11) | `sizing.measured`, `sizing.decision.tried[0]` | 0.5900, 0.6958, 2.844 (from the rounded $E[1/m]$), 3,622,020, 15.276, 14.607 | `N7` |
| summary: 19,120 players, w 0.6, 144/120, about 3.6 million games, 15.3 CPU-hours, 14.6 hours, 2.9 CPU-hours streamed, control 0.998 | as above | `sizing` | as quoted | `N7`, `S7d` |
| stage 2: about 5,130 players, 13,400 CPU-hours, 670 × 20, 2.9 × 4,560 | Result 12, (E14) | `stage2` | 5,126.7; 13,363.7; 668.2; 2.93 | `N8` |
| outcome 1 about 0.013; outcome 2 0.85, 0.98, 0.99; outcome 3 0.14, 0.006, < 0.001; averaged 0.01, 0.94, 0.05 (section 0; summary 94% and 5%) | (M7) | `outcomes_at_w` point, ×1 | 0.0135/0.0129/0.0129; 0.846/0.9816/0.9869; 0.1405/0.0055/0.0002; 0.0131/0.9382/0.0487 | `N9`, `S7a` |
| ×1.5: 0.22, 0.013, 0.001; ×2: 0.30, 0.03, 0.001; "up to 30%" (summary) | (M7) | point, ×1.5 and ×2 | 0.2153/0.0132/0.0006; 0.3005/0.0276/0.0011 | `N9`, `S7a` |
| lower bounds: outcome 3 0.82 to 0.93 | (M7) | lower, every bias and control | 0.8206 to 0.9268 at controls 5 and 7; 0.8087 at 10 (Finding 3) | `N10`, `S7a` |
| dev effects: outcome 1 1.00, or 0.85 to 0.90 at the lower bounds | (M7) | `*,dev_control7` | 0.9976 to 0.9989; 0.8491 to 0.8999 | `N9`, `S7a` |
| control holds 0.86, 0.99, 1.00 at 5, 7, 10 | Result 10 (a) | point, ×1 | 0.8595, 0.9946, 1.0 | `N9`, `S7b` |
| plan target: the five read "below" 1.00, single 1.00; W4.missed 0.97, W7.prep 0.998, W9b 0.43; control 0.99 | (E13) | `simulation.sizing.tried[0]` | 0.9999 to 1.0; 0.9734, 0.998, 0.4312; 0.9938 | `N9`, `S7d` |
| the gate closes between 0.6 and 0.8; control 0.86 and 0.68 there | Result 10 (a) | `gate_by_memory_ratio` | ok at 0.6, not at 0.8; 0.8646, 0.6756 | `N9`, `S7c` |
| | Finding 1: θ 7.33 and 7.35; leaks +0.83 and +1.05; −6.2% and −3.1%; +29.8%; one-month 35%; ratios 4.8 and 17 | Result 5 | EXP-0009 `level`, `primary` | 7.326, 7.351, 0.833, 1.048, −6.18%, −3.09%, 29.76%, 35.2%, 4.82, 17.3 | `N11` |
| Finding 2's table | Result 9 (b) | `simulation.sizing.k`, `sizing.decision.k` | 0.912 … 1.942 | `N12` |
| Finding 4: outcome 2 0.83, 0.95, 0.96, 0.67; averaged 0.91 and 0.075; $P(p \le 0.05)$ 0.050 to 0.061, 0.096 | (M7) with (E7)'s order | – | 0.8277, 0.9498, 0.9565, 0.6665; 0.9113, 0.0751 | `N13` |
| Finding 5 and section 2: −1.5 and −0.7 (stage 1's ratio); −0.84 and −0.81 (E9) | (E8), (E9) | `memory`, `g0` | −1.53, −0.70; −0.84, −0.81 | `N14` |
| $\bar a$ 1.0638, 1.0417; factors 1.066, 1.046, 1.15, 1.14 (Result 8) | Result 8 (2) | – | as quoted | `N14` |
| section 4: +3.0 at 0.067, about 11.7 per true SD; level +6.2, +5.9, +4.4; 7.3 per true SD | EXP-0009 | `adjusted`, `level` | 3.035, 11.69; 6.22, 5.89, 4.44; 7.33 | `N14` |
| section 5: the reliability's SE about 0.015; section 6: 57,127 and 53.8 | Result 6 (EXP-0009 Finding 3) | `rating_change_sd` | 0.0149; 57,126.7; 53.79 | `N14` |

Not recomputable from committed files: the stage 1 power figures that the registration quotes for design B, 0.54 against 0.65 at $w = 0.5$ (section 4; `power.json` keeps only the chosen design's simulation); the critic's interpolation between the lower-bound and point truths, outcome 3 about 0.30, 0.06 and 0.01 (section 0, `review.md` R3-4, from a re-run that was not committed); review B3's stage 1 biases (−1.53, −1.02, −0.70), superseded by N1; and the streaming's measured 263 GB and 54 GB on disk (`summary.md`, from the stream logs, which are not committed).

## 6. Assumptions that can fail

- **Independent players (M1)**, for Results 1 to 4. Games between two players of the slice give two rows a shared game; the run counts them, and the standard errors ignore them. Not covered by a sensitivity; the count bounds its size.
- **Asymptotic normality**, for Results 3, 4, 7 and the p-values. With $n_q$ in the thousands and $p = 14$, HC3 intervals are close to nominal (`S1`); the outcome's heavy tails (rating changes) are the main risk. No sensitivity.
- **Stationary true levels and equal noise (M2)**, for Result 5 (a) and (b). Unequal noise between F1 and F2 (more opportunities in one window) makes the change and the window mean correlated by $(\sigma_2^2 - \sigma_1^2)/2$ (Result 5 (c)). Covered by the inverse-variance-weighted sensitivity (`ivw`) and by the log-opportunities covariates; the F1-level model is reported beside it, and a key that climbs only there is a level artefact.
- **Where the level effect sits (M6)**, for the sign of the primary's leak. If the later window's level matters more, the leak is positive, of the size $\theta\sqrt{c/2}/\delta$ of the minimum (Finding 1: about 3% to 6% for accuracy). Not covered beyond the F1-level comparison.
- **Parallel halves and per-game noise (M3)**, for Result 6. Spearman-Brown over three-month windows assumes the true-change variance stays fixed as the windows grow (registration, section 5); if it grows, the reliability is underestimated and the minimum is conservative. The harmonic $k$ assumes opportunities are proportional to games. The Fisher-$z$ bound assumes near-normal halves. The read uses the run's own $r_L$, so a wrong $k$ changes the planned power, not the read's validity.
- **The rating's memory (M4)**, for Results 8 and 9. Glicko-2's update is not exactly AR(1), and $\varphi$ is 0.97 (EXP-0015's simulation) or 0.98 (an established account); the gate uses the worse. The five-game end rating makes the true ratio $\bar a(1-\varphi^g)/(1 - \bar a\varphi^g)$ times the registered, 1.05 to 1.15 (Result 8 (2), deviation 2). June's games stand in for May's ($g_M = g_J$). Covered in scale by the $\times1.5$ and $\times2$ sensitivities.
- **The shared luck (M5) and the bias's size (Result 9 (c)).** This is deviation 2 in section 11 of the registration: the registered bias $B_q = \Delta_qMD$ is assumed, not measured. EXP-0009's shift $\Delta_q$ measures $\gamma(a + b)$, not the luck bias $\lambda b$ (EXP-0009 `math.md`, Result 12 and Finding 1), so it can overstate or understate the bias by an unknown factor. The memory ratio assumes the same shared covariance per unit of share for every player (Result 9 (a)), and the dilution assumes F2's first two months are forgotten (Finding 2). The decision rule subtracts no bias, so a wrong size cannot produce a candidate. It moves the "below" reads (Result 13 (c)) and the control together, and a larger bias makes outcome 3 likelier. The $\times1.5$ and $\times2$ sensitivities cover only part of the range. The decision must read outcome 2 with this limit.
- **The planning model (M7)**, for Result 10 and every probability in `power.json`. It reads "below" without Holm's priority, which overstates outcome 2 by 0.02 to 0.03 (Finding 4). The nine estimates are assumed jointly normal with an equicorrelation of 0.3 (review round 2: 0 and 0.6 change nothing), the reliability estimate independent of $\hat\beta$, and the standard errors known and scaled from EXP-0009 by $\sqrt{n}$ and $\eta$. The sizing step replaced $\varpi$, $\eta$, the game moments and $E[s_A]$ by measured values; the reliabilities and the shifts are still EXP-0009's. The lower-bound truth is the sensitivity for the reliabilities.
- **The sizing's blindness (Result 11 (b))** holds by construction: the inputs are nuisance parameters from the players files. The remainder of $[0.3, 0.9)$ has had its outcome spread read, so it is not a holdout.
- **Attrition and survivorship.** The claim is for players active for six months with an outcome. The inverse-probability-of-staying weights are a sensitivity and rest on the staying models' terms; nothing proves they remove the selection.

### Verification

- **Blind re-derivation.** A fresh agent was given the registration, `analysis.py`, `power.py`, `power.json`, `review.md` and the inputs, but no `math.md`. It derived five results on its own: the luck shares, exact per early game and in aggregate with the factor $\bar a$, with the same correction factors as Result 8 (2) (1.151, 1.137 at 20 games); the window-mean and F1-level coefficients, the same as Result 5, including the point that the leak's sign depends on which window carries the level; the dilution $(1 + \varphi^G + \varphi^{2G})/\sqrt3$ in the noise-dominated limit, as Result 9 (b) has it; Holm's control and the "below" read's error through $r_L$; and stage 2's $z_\gamma + z_{0.90}$ with 5,127 players and 13,365 CPU-hours (`power.json`, 5,126.7 and 13,363.7, the gap from rounding 1.3415). All five agree with this file. It added the dilution at 20 games a month (now in Finding 2's table) and the size of the low-side miss if the true bias were EXP-0009's whole shift (now in Result 13).
- **Referee.** A second fresh agent checked every proof and the script and found 8 problems, all fixed. (1) `power.py`'s simulation, and this file's re-implementation of it, let a key that Holm rejects read "below", which `analysis.py` does not: Finding 4, with `N13` and a `faithful` option in `rule`; Result 10 (c)'s bound is now the exact $P(p_q \le \alpha)$. (2) A sign error in Result 13 (c), now $\delta + |B_q|/\sqrt{r_q}$. (3) Result 9 mixed a per-game and a per-measure covariance, and its part (a) assumed what (b) computes; restated with $\kappa$ per game, $L = 1$, and the $\bar a$ factor for $L = 5$. (4) Finding 2, `R9` and `N12` used $k = 3$; they now use the measured 2.64 and 2.85. (5) Section 2's −1.5 and −0.7 were unchecked and use stage 1's ratio: Finding 5, `N14`. (6) Unchecked quoted numbers ($\bar a$, the correction factors, section 4's development readings, the reliability's SE, 57,127 and 53.8, Finding 1's −3.1%, 35%, 4.8 and 17) are now in `N11` and `N14`, and the critic's 0.30, 0.06 and 0.01 are listed as not recomputable. (7) Checks that could not fail were replaced: `S1` now runs 20,000 replicates with $t_5$ errors and $p = 14$, which separates 0.99375 from 0.99; `S2` compares Holm with a direct step-down instead of testing a bound it could not miss; `S5` now separates the exact share from $\bar a$ times it; `S7c` tests the gate's whole verdict; the tautologies in `R8` and `R9` are gone; `S6` is described as checking the algebra of its model. (8) Minor: Result 1's conditioning, Result 4's tie order and Lean's tie-aware threshold, Result 6's no-op minimum, (M7)'s $n_q$ against $n'_q$, Result 7's bound (0.156, not 0.175), Result 11's margin (35 SE, not 70), Finding 1's W7.prep figures.
