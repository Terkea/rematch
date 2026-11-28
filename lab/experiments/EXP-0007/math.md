# EXP-0007: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic checks (prefix `R`), simulations from a known truth (prefix `S`) and the recomputation of every quoted number (prefix `N`).

EXP-0007 is a measurement of one version. The production Rust code tags each sampled puzzle's solution; Python only samples, weights, counts and decides. So all the maths is in the survey design: a stratified sample without replacement, a ratio estimator with a linearised variance, a Wilson interval read at an effective sample size, and a Bonferroni rule with floors. The maths has to show four things: what the ratio estimates and how far it is biased (Results 1 to 4); why its effective size can exceed the count, and how Kish's differs (Results 5 and 6); that the interval and the rule have the error control the registration claims, and what power the design has (Results 7 to 14); and which of the registration's arguments in words hold (Results 13, 15, 16, the remark after Result 1, and three Findings).

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $t$ | a scored tag, one of 15 | – | `analysis.py` `SCORED`, `TRUTH` |
| $i$ | a puzzle of the population | – | `load_population` |
| $h = (s, b)$ | a cell: stratum $s$ (the puzzle's rarest scored truth label, or `none`) in band $b \in \{A1, A2, A3, A4, S\}$ | – | `load_population`, `cells`, `strata.csv` |
| $N_h$ | the cell's population less its pilot puzzles | puzzles | `cmd_run` (`pops`), `strata.csv` `population` − `pilot` |
| $n_h$ | the cell's confirmatory sample, $n_h = \min(N_h, \max(m, \lceil f N_h \rceil))$ | puzzles | `allocation`, `draw`, `sample.csv` |
| $m, f$ | the allocation's floor and fraction, $m = 50$, $f = 0.03$ | puzzles, – | `power.json` `chosen` |
| $f_h = n_h/N_h$ | the cell's sampling fraction | – | – |
| $w_h = N_h/n_h$ | the weight of each sampled puzzle of cell $h$ | – | `records` (`w`) |
| $\mathcal S_h$, $\mathcal S$ | the sample of cell $h$; the union over the domain's cells | – | `sample.csv` |
| $\ell_i(t)$ | 1 if puzzle $i$ carries a Lichess theme that counts for $t$ | – | `labels_of` |
| $g_i(t)$ | 1 if our production tagger gives $i$ the tag $t$ | – | `tag` (`backend/crates/bench/src/lab.rs` `tag_motifs`, through `profile_core::features::derive::derive_move_features`) |
| $x_i, y_i$ | for precision $x = g$, $y = g\ell$; for recall $x = \ell$, $y = g\ell$. Always $y_i \le x_i$ | – | `precision_recall` |
| $X, Y$ | population totals $\sum_i x_i$, $\sum_i y_i$ over the domain (the audience A1 to A4, or one band) | puzzles | – |
| $R = Y/X$ | the population precision or recall | – | – |
| $\hat X, \hat Y, \hat R$ | the weighted totals and their ratio | puzzles, – | `ratio_share` |
| $d_i = y_i - R x_i$ | the linearised variable; $\hat d_i = y_i - \hat R x_i$ its plug-in | – | `ratio_share` (`d`) |
| $S^2_{u,h}$, $s^2_{u,h}$ | the population and the sample variance of $u$ in cell $h$, both with divisor (size − 1) | – | `ratio_share` (`s2`) |
| $v$ | the linearised variance estimate of $\hat R$ | – | `ratio_share` (`var`) |
| $n_{\text{eff}}$, $n_K$ | the linearised effective size $\hat R(1-\hat R)/v$ and Kish's $(\sum w)^2/\sum w^2$ over the units with $x = 1$ | puzzles | `ratio_share` |
| $z_\gamma$ | $\Phi^{-1}(1 - \gamma/2)$ for a two-sided level $1 - \gamma$; $z_{0.20} = 1.2816$, $z_{0.05} = 1.9600$, $z_D = \Phi^{-1}(1 - 0.05/60) = 3.1440$ | – | `Z80`, `Z95`, `Z_DECISION` |
| $c$ | a floor: $c_P = 0.80$ (precision), $c_R = 0.50$ (recall) | – | `PRECISION_FLOOR`, `RECALL_FLOOR` |
| $\lambda_h, a_h, b_h$ | the power model's label share, true-positive rate and false-positive rate of cell $h$ | – | `cmd_power` (`lam`, `a`, `b`) |

## 2. Model

**(M1) Fixed population, random sample.** The population is the 1,826,817 puzzles of the snapshot that pass the filters, with their themes and our tags. Both are fixed numbers: the tagger is deterministic, and no theme is re-drawn. The only randomness is the sample. Inference is design-based: $R$ is a property of this finite population, not of a model.

**(M2) Stratified simple random sampling without replacement.** Within each cell, $\mathcal S_h$ is a uniformly random subset of size $n_h$ of the cell's $N_h$ puzzles (`draw`: a seeded permutation, the first $n_h$). Cells are sampled independently of each other, given the fixed allocation. Strictly, one generator walks the cells in order, so the draws are deterministic given the seed. Treating them as independent uniform subsets is the usual idealisation of a good generator.

**(M3) The power model** (preregistration section 6, `cmd_power`). Each audience cell has $n_h$ sampled puzzles with $L_h \sim \operatorname{Bin}(n_h, \lambda_h)$ labelled, $T_h \mid L_h \sim \operatorname{Bin}(L_h, a_h)$ true positives and $F_h \sim \operatorname{Bin}(n_h - L_h, b_h)$ false positives, independently across cells. $\lambda_h$ is the population label share of `strata.csv`. $a$ and $b$ are the pilot's rates per stratum (pooled over bands), smoothed as $(k + 0.5)/(n + 1)$, then scaled (Result 9). This is a superpopulation model: units are drawn as if independently, not without replacement (Result 14).

## 3. Estimators

- **(E1) The allocation** (`allocation`): $n_h = \min(N_h, \max(m, \lceil fN_h\rceil))$, with $m = 50$, $f = 0.03$.
- **(E2) The weighted totals** (`ratio_share`, `records`): $\hat X = \sum_h w_h \sum_{i \in \mathcal S_h} x_i$ and $\hat Y = \sum_h w_h \sum_{i \in \mathcal S_h} y_i$, the Horvitz-Thompson totals.
- **(E3) The ratio:** $\hat R = \hat Y / \hat X$. It is undefined when $\hat X = 0$ (`estimate: null`).
- **(E4) The linearised variance** (`ratio_share`; the same in closed form in `neff_np`):

```math
v = \frac{1}{\hat X^2} \sum_h N_h^2 (1 - f_h) \frac{s^2_{\hat d, h}}{n_h}, \qquad
s^2_{\hat d, h} = \frac{n_{11}(1 - \hat R)^2 + n_{10}\hat R^2 - \big(n_{11}(1 - \hat R) - n_{10}\hat R\big)^2/n_h}{n_h - 1},
```

  where $n_{11}$ and $n_{10}$ count the cell's sampled units with $(x, y) = (1, 1)$ and $(1, 0)$. A cell with $n_h < 2$ is skipped.
- **(E5) The effective sizes:** $n_{\text{eff}} = \hat R(1 - \hat R)/v$ when $v > 0$ and $0 < \hat R < 1$; otherwise $n_{\text{eff}} = n_K = (\sum_{i: x_i = 1} w_i)^2 / \sum_{i: x_i = 1} w_i^2$.
- **(E6) Wilson's interval** (`wilson`), at proportion $\hat R$, size $n = n_{\text{eff}}$ and quantile $z$:

```math
\ell, u = \frac{\hat R + \frac{z^2}{2n} \mp z \sqrt{\frac{\hat R (1 - \hat R)}{n} + \frac{z^2}{4n^2}}}{1 + \frac{z^2}{n}}, \quad \text{clipped to } [0, 1].
```

  Three levels: $z_{0.20}$ (80%), $z_{0.05}$ (95%), and $z_D$ (Bonferroni over 30, the decision's).
- **(E7) The rule** (`decide`): with $\ell_P$, $\ell_R$ the lower ends of the $z_D$ intervals of precision and recall: if $\ell_P < 0.80$ (or precision is undefined), *hidden pending a human audit* for a tag in `LABEL_LIMITED`, *hidden until fixed* otherwise. Else *trusted* if $\ell_R \ge 0.50$, and *needs fixing* if not.
- **(E8) The confusion shares** (`confusion`): for our tag $t$ and theme $\tau$, $\hat\sigma_{t\tau} = \sum_{g_i(t) = 1} w_i [\tau \in \text{themes}_i] / \sum_{g_i(t) = 1} w_i$ over the audience sample. The denominator holds every puzzle we tag $t$, true and false positives alike.
- **(E9) The power** (`cmd_power`): under (M3), the share of 2,000 replicates in which $\ell_P \ge 0.80$ ("shown"), $\ell_R \ge 0.50$ ("recall trusted"), or the $z_D$ interval holds the scenario value ("coverage").

## 4. Derivations and proofs

**Result 1 (the weighted totals are unbiased).** Under (M2), $E[\hat X] = X$ and $E[\hat Y] = Y$.

*Proof.* Let $I_i$ be 1 if puzzle $i$ of cell $h$ is sampled. The number of samples of size $n_h$ that contain $i$ is $\binom{N_h - 1}{n_h - 1}$, out of $\binom{N_h}{n_h}$ equally likely ones. So $E[I_i] = \binom{N_h - 1}{n_h - 1}/\binom{N_h}{n_h} = n_h/N_h$. Then $\hat X = \sum_h \sum_{i \in h} w_h I_i x_i$, and by linearity $E[\hat X] = \sum_h \sum_{i \in h} \frac{N_h}{n_h}\frac{n_h}{N_h} x_i = X$. The same holds for $\hat Y$. $\blacksquare$ Checked by `R Result 1` (the binomial identity), `R Results 1, 2` (all ten samples of a population of five, exactly) and `S Result 1` (a six-cell finite population, 40,000 stratified samples).

*Remark (the estimand).* $N_h$ is the cell's population less the pilot (`cmd_run`), so $R$ is the precision or recall over the audience less its 1,874 pilot puzzles, not over all 1,580,025. The full-audience value is $R_{\text{full}} = (1 - \pi) R + \pi R_{\text{pilot}}$, with $\pi$ the pilot's share of the denominator, so $|R_{\text{full}} - R| \le \pi$. For recall, $\pi$ is at most the labelled pilot puzzles over the labelled audience. That is at most 1.9% for every tag (`smotheredMate`, an upper bound that counts the S band's pilot too; 100 of 6,377, or 1.6%, in the audience alone) and far less for the common tags. For precision, $\pi$ is at most the pilot's tagged puzzles over themselves plus the tagged total, which `results.json` does not hold. Estimated as $X = \hat R_{\text{recall}} L / \hat R_{\text{precision}}$ (since $Y$ is both), $\pi$ is again at most 1.9% (`smotheredMate`), and 0.27% for `skewer`, whose lower end is the closest to a floor (0.022 below 0.80). So no decision is likely to turn on the pilot's exclusion. The precision figure is estimated, not bounded. Checked by `N remark` (both bounds).

**Result 2 (the variance of a weighted total).** For any variable $u$, under (M2),

```math
\operatorname{Var}(\hat U) = \sum_h N_h^2 (1 - f_h) \frac{S^2_{u,h}}{n_h}, \qquad E[s^2_{u,h}] = S^2_{u,h}.
```

*Proof.* Fix a cell and drop $h$. $E[I_i I_j] = \binom{N-2}{n-2}/\binom{N}{n} = \frac{n(n-1)}{N(N-1)}$ for $i \ne j$, so $\operatorname{Cov}(I_i, I_j) = \frac{n(n-1)}{N(N-1)} - \frac{n^2}{N^2} = -\frac{f(1-f)}{N - 1}$. Also $\operatorname{Var}(I_i) = f(1 - f)$. With $\hat U = \frac Nn \sum_i I_i u_i$,

```math
\operatorname{Var}(\hat U) = \frac{N^2}{n^2} f(1-f) \Big[\sum_i u_i^2 - \frac{1}{N-1}\sum_{i \ne j} u_i u_j\Big].
```

Write $\sum_{i \ne j} u_i u_j = (\sum_i u_i)^2 - \sum_i u_i^2$. The bracket becomes $\frac{N}{N-1}\big[\sum_i u_i^2 - \frac1N(\sum_i u_i)^2\big] = N S^2_u$. So $\operatorname{Var}(\hat U) = \frac{N^2}{n^2}\cdot\frac nN\Big(1 - \frac nN\Big) N S^2_u = N^2(1 - f)S^2_u/n$. Cells are independent, so the variances add. For $s^2$: $\sum_{i \in \mathcal S}(u_i - \bar u_{\mathcal S})^2 = \sum_{i \in \mathcal S} u_i^2 - n \bar u_{\mathcal S}^2$. Its expectation is $n \cdot \frac1N\sum_i u_i^2 - n(\operatorname{Var}\bar u_{\mathcal S} + \bar U^2)$, with $\operatorname{Var}\bar u_{\mathcal S} = (1 - f)S^2_u/n$ from above. Since $\frac1N \sum u_i^2 - \bar U^2 = \frac{N-1}{N}S^2_u$, this is $n\frac{N-1}{N}S^2_u - (1 - f)S^2_u = (n - 1)S^2_u$. $\blacksquare$ Checked by `R Result 2` (the joint inclusion and the covariance), `R Results 1, 2` (exact in $u$ on all ten samples of $N = 5$, $n = 2$) and `S Result 2` (the empirical variance of $\hat X$ within 4 SE of the formula: 34,132 against 34,071).

**Result 3 (the ratio's bias: an exact identity and a bound).** $\hat R \in [0, 1]$ whenever it is defined. Set $\hat R = 0$ when $\hat X = 0$ (an event of tiny but positive probability, since no audience cell is sampled in full; `analysis.py` returns no estimate). Ignoring that event, $E[\hat R] - R = -\operatorname{Cov}(\hat R, \hat X)/X$, and

```math
|E[\hat R] - R| \le \operatorname{SD}(\hat R)\, \frac{\operatorname{SD}(\hat X)}{X}.
```

*Proof.* $0 \le y_i \le x_i$ and $w_i > 0$ give $0 \le \hat Y \le \hat X$. $\hat R \hat X = \hat Y$, so $E[\hat R \hat X] = Y = RX$ by Result 1. Then $\operatorname{Cov}(\hat R, \hat X) = E[\hat R\hat X] - E[\hat R]E[\hat X] = RX - E[\hat R]X$, which rearranges to the identity. The bound is Cauchy-Schwarz, $|\operatorname{Cov}(\hat R, \hat X)| \le \operatorname{SD}(\hat R)\operatorname{SD}(\hat X)$ (Hartley and Ross 1954). $\blacksquare$ Checked by `R Result 3` and `S Result 3`. In the simulation the bias is 0.00035, the bound 0.0061. The ratio of the bias to the standard error is at most $\operatorname{CV}(\hat X)$. As a heuristic, not derived here: $\hat X$ is a weighted count of about $n_x$ tagged puzzles, so $\operatorname{CV}(\hat X)$ is of order $1/\sqrt{n_x}$, inflated by the weights' spread and, for precision, by the false positives of the heavy `none` cells. At $n_x$ in the hundreds to thousands that is a few per cent of a standard error. `results.json` does not hold $\hat X$, so it is not measured.

**Result 4 (the linearised variance).** Exactly, $\hat R - R = \hat D/\hat X$, where $\hat D = \hat Y - R\hat X$ is the weighted total of $d_i = y_i - Rx_i$, whose population total is $D = 0$. To first order,

```math
\operatorname{Var}(\hat R) \approx \frac{\operatorname{Var}(\hat D)}{X^2} = \frac{1}{X^2}\sum_h N_h^2 (1 - f_h)\frac{S^2_{d,h}}{n_h},
```

and (E4) is this with $R$, $X$ replaced by $\hat R$, $\hat X$. The remainder is $\hat D/\hat X - \hat D/X = -\hat D(\hat X - X)/(X\hat X)$, of order $1/n$ against the $1/\sqrt n$ of the leading term. The closed form of $s^2_{\hat d, h}$ in (E4) is the sample variance of a variable taking the values $1 - \hat R$ ($n_{11}$ times), $-\hat R$ ($n_{10}$ times) and 0.

*Proof.* $\hat Y/\hat X - R = (\hat Y - R\hat X)/\hat X$, and $\hat Y - R\hat X = \sum w_i(y_i - Rx_i) = \hat D$ by linearity. $D = Y - RX = 0$. The remainder is the algebra $\frac{\hat D}{\hat X} - \frac{\hat D}{X} = \hat D \frac{X - \hat X}{X \hat X}$. $\hat D$ and $\hat X - X$ are both $O_p(N/\sqrt n)$ by Result 2, so the product over $X\hat X = O_p(N^2)$ is $O_p(1/n)$. The leading term's variance is Result 2 applied to $u = d$. Plugging in: $s^2_{\hat d, h}$ is a sample variance, so it is consistent for $S^2_{d,h}$ (Result 2, and $\hat R \to R$), and $\hat X/X \to 1$. The closed form: with $T = n_{11}(1 - \hat R) - n_{10}\hat R$ the sum and $Q = n_{11}(1 - \hat R)^2 + n_{10}\hat R^2$ the sum of squares, the sample variance is $(Q - T^2/n_h)/(n_h - 1)$. $\blacksquare$ Checked by `R Result 4` (both identities, and the closed form against the definition: so `ratio_share`'s loop and `neff_np`'s closed form compute the same thing) and `S Result 4`: in the six-cell population, $E[v]/\operatorname{Var}(\hat R) = 0.967$, inside the 0.90 to 1.10 a first-order method promises at about 2,900 sampled puzzles.

**Result 5 (when a cell adds no variance, and why $n_{\text{eff}}$ can exceed the count).** A cell adds nothing to $v$ exactly when it is a census ($n_h = N_h$) or $\hat d$ is constant on its sample: all its sampled units are true positives, or all are false positives, or none has $x = 1$. When a cell has no false positive ($n_{10} = 0$) and a share $\bar x$ of tagged units, its sample variance is $\frac{n_h}{n_h - 1}\bar x(1 - \bar x)(1 - \hat R)^2$. So $n_{\text{eff}}$ is not bounded by the count of $x = 1$ units.

*Proof.* The first part reads off (E4): the term is $N_h^2(1 - f_h)s^2/n_h$, zero iff $f_h = 1$ or $s^2 = 0$, and a sample variance is zero iff the values are equal. For the second, put $n_{10} = 0$ and $n_{11} = \bar x n_h$ in the closed form: $Q = \bar x n_h(1 - \hat R)^2$ and $T^2/n_h = \bar x^2 n_h (1 - \hat R)^2$, so $(Q - T^2/n_h)/(n_h - 1) = \frac{n_h}{n_h - 1}\bar x(1 - \bar x)(1 - \hat R)^2$. For the last claim, an example suffices. Take label cells where the tag fires on every labelled puzzle ($\bar x = 1$, no variance) and a large unlabelled cell with rare false positives. Every true positive then adds nothing, and $n_{\text{eff}}$ is set by the false positives alone. $\blacksquare$ Checked by `R Result 5` and `S Result 5`: in such a population $n_{\text{eff}} > n_x$ in more than 99% of samples, while Kish's never exceeds it.

`decision.md` explains `hangingPiece`'s $n_{\text{eff}}$ of 5,127 against 2,888 puzzles by "cells where every tagged puzzle is a true positive add no variance". By Result 5 that is exact only for a cell whose every sampled puzzle is tagged (or none is). A cell where every tagged puzzle is a true positive still adds $\bar x(1 - \bar x)(1 - \hat R)^2$ per unit. For `hangingPiece` the two plausibly coincide. Its recall is 0.999, so nearly every labelled puzzle is tagged, and in its own stratum's cells, where every puzzle carries the label, $\bar x \approx 1$. Its labelled puzzles in other strata (a rarer label wins the stratum) sit in cells that also hold unlabelled puzzles, where this does not hold. This is a conjecture: `results.json` holds no per-cell counts, so it is not checked. The decision's sentence is right as a mechanism, not as a general rule, and `N Result 5` checks only its two numbers. The decision's second reason, "fully sampled cells have no finite-population term", is Finding 3.

**Result 6 (Kish's effective size).** $n_K \le n_x$, the count of units with $x = 1$, with equality iff their weights are equal. If every unit of the domain has $x = 1$ and every cell's sample share of $y$ equals $\hat R$, then $\hat R(1 - \hat R)/n_K = \frac{1}{X^2}\sum_h N_h^2 \hat R(1 - \hat R)/n_h$. This is (E4) without the factors $(1 - f_h)$ and $n_h/(n_h - 1)$.

*Proof.* By Cauchy-Schwarz, $(\sum_i w_i \cdot 1)^2 \le (\sum_i w_i^2)(\sum_i 1^2) = n_x \sum_i w_i^2$, with equality iff $w$ is constant. For the second part, $\sum_i w_i = \sum_h n_h \frac{N_h}{n_h} = X$ and $\sum_i w_i^2 = \sum_h n_h \frac{N_h^2}{n_h^2} = \sum_h \frac{N_h^2}{n_h}$. In (E4), each cell's $\hat d$ is $1 - \hat R$ on a share $\hat R$ and $-\hat R$ on the rest, so $s^2 = \frac{n_h}{n_h - 1}\hat R(1 - \hat R)$. $\blacksquare$ Checked by `R Result 6` (symbolically, and the Cauchy-Schwarz bound on 2,000 random weight vectors), `S Result 6` and `N Result 6` (every committed $n_K \le n$). The second part's premises (every unit with $x = 1$, equal cell shares) never hold in EXP-0007, so it is illustrative: it shows what Kish's size leaves out. It leaves out three things. The first is the finite-population correction, a factor between 0.55 and 1 here (0.55 in `mateIn4Plus/A1`, the largest sampling fraction). The second is how the ratio varies between cells: precision is near 1 in the label cells and 0 in `none`. The third, for precision, is the randomness of how many false positives land in the heavy cells. Nothing here proves which way the net error goes. Section 6's simulation measures it: Kish's size under-covers for `smotheredMate` at some designs (`N power`), and the linearised one does so less (Finding 2).

**Result 7 (Wilson's interval).** The ends of (E6) are the two roots in $p$ of $(\hat R - p)^2 = z^2 p(1 - p)/n$, and the interval is $\{p : (\hat R - p)^2 \le z^2 p(1-p)/n\}$. Its centre lies $z^2(\tfrac12 - \hat R)/(n + z^2)$ from $\hat R$. At $\hat R = 1$ the lower end is $n/(n + z^2)$. As $n \to \infty$ with $\hat R \in (0, 1)$ fixed, the half-width over $z\sqrt{\hat R(1 - \hat R)/n}$ tends to 1. Both ends lie in $[0, 1]$ without the clipping.

*Proof.* Expand: $(1 + z^2/n)p^2 - (2\hat R + z^2/n)p + \hat R^2 = 0$. The quadratic formula gives $p = \frac{\hat R + z^2/(2n) \pm \sqrt{(\hat R + z^2/(2n))^2 - (1 + z^2/n)\hat R^2}}{1 + z^2/n}$. The discriminant is $z^2\hat R(1 - \hat R)/n + z^4/(4n^2)$, which gives (E6). The leading coefficient is positive, so the quadratic is $\le 0$ exactly between the roots. The centre is $\frac{\hat R + z^2/(2n)}{1 + z^2/n} = \frac{n\hat R + z^2/2}{n + z^2}$, which differs from $\hat R$ by $\frac{z^2(1/2 - \hat R)}{n + z^2}$. At $\hat R = 1$ the square root is $z^2/(2n)$, so the lower end is $\frac{1}{1 + z^2/n} = \frac{n}{n + z^2}$. The limit: the half-width is $z\sqrt{\hat R(1 - \hat R)/n + z^2/(4n^2)}/(1 + z^2/n)$, and the ratio is $\sqrt{1 + z^2/(4n\hat R(1 - \hat R))}/(1 + z^2/n) \to 1$. Finally, the quadratic is $\hat R^2 \ge 0$ at $p = 0$ and $(1 - \hat R)^2 \ge 0$ at $p = 1$, and it is negative at $p = \hat R$ (or zero at the ends), so both roots lie in $[0, 1]$. $\blacksquare$ Checked by `R Result 7` (sympy solves the equation and matches both ends, the centre shift and the end at 1, and the limit) and `N Result 7`: `smotheredMate`'s 0.953 is $202.3/(202.3 + 3.144^2)$.

**Result 8 (coverage, approximately).** As the cells' samples grow with the sampling fractions bounded away from 1 in the cells that carry the variance, $(\hat R - R)/\sqrt v \to N(0, 1)$. The interval (E6) at $n = n_{\text{eff}}$ then covers $R$ with probability tending to $1 - \gamma$. It is not exact at any finite size.

The regime is a sequence of populations and designs with a fixed number of cells, in which $n_h \to \infty$ and $N_h - n_h \to \infty$ in every cell with $S^2_{d,h} > 0$, and $\operatorname{Var}(\hat D)/\max_h w_h^2 \to \infty$. EXP-0007 has cells of 50, so it is in this regime only loosely.

*Proof.* $\hat D$ is a sum of independent cell totals, each the total of a simple random sample without replacement. Each unit's contribution is bounded by $w_h |d_i| \le w_h$, so the Lindeberg condition for the sum follows from $\operatorname{Var}(\hat D)/\max_h w_h^2 \to \infty$. The stratified central limit theorem (Bickel and Freedman 1984, building on Hájek 1960 for one cell) then gives $\hat D/\sqrt{\operatorname{Var}\hat D} \to N(0, 1)$. Krewski and Rao (1981) give the same for a ratio with its linearised variance. Result 4's remainder is $o_p(\sqrt{\operatorname{Var}(\hat R)})$, and $v/\operatorname{Var}(\hat R) \to 1$ (Result 4), so Slutsky's theorem gives the limit for $(\hat R - R)/\sqrt v$. By construction $\hat R(1 - \hat R)/n_{\text{eff}} = v$, so the Wald interval $\hat R \pm z\sqrt v$ is $\hat R \pm z\sqrt{\hat R(1 - \hat R)/n_{\text{eff}}}$. By Result 7 the Wilson interval differs from it by $O(1/n_{\text{eff}})$ at each end, while the Wald half-width is of order $1/\sqrt{n_{\text{eff}}}$. So both have the same limiting coverage. $\blacksquare$ Checked by `S Result 8`: in the six-cell population the 95% coverage is 0.9492, and the lower end at $z_D$ exceeds $R$ in 0.00013 of samples, within 4 SE of at most the nominal 0.00083. The error at EXP-0007's design, where a few heavy false positives can decide a rare tag, is measured by section 6's simulation, not bounded here (Result 14, Finding 2).

**Result 9 (section 6's scenarios).** Scaling every false-positive rate by $k = \mathrm{TP}(1/\pi - 1)/\mathrm{FP}$, with $\mathrm{TP} = \sum_h N_h\lambda_h a_h$ and $\mathrm{FP} = \sum_h N_h(1 - \lambda_h)b_h$, gives a population precision $\mathrm{TP}/(\mathrm{TP} + k\,\mathrm{FP}) = \pi$ exactly, when no scaled rate exceeds 1.

*Proof.* $\mathrm{TP} + k\,\mathrm{FP} = \mathrm{TP} + \mathrm{TP}(1/\pi - 1) = \mathrm{TP}/\pi$. $\blacksquare$ Checked by `R Result 9`. With the cap at 1 the precision can only rise. `S Result 9` computes it with the cap: the cap binds only for `mate`, whose scenario precision is then off target by $4.7 \times 10^{-6}$ at 0.90 and $1.6 \times 10^{-5}$ at 0.80 (and $2.4 \times 10^{-5}$ at 0.70, which the check does not run). The recall scenario is found by bisection to 0.65 within $10^{-6}$ for every tag.

**Result 10 (the floor test, and the false-show rate).** For $c \in (0, 1)$, the lower end of (E6) satisfies $\ell \ge c$ if and only if $\hat R \ge c + z\sqrt{c(1 - c)/n}$. Both ends are non-decreasing in $\hat R$. Under the normal approximation of Result 8, a tag whose true precision is $R \le c_P$ is shown with probability at most about $\Phi(-z_D) = 0.05/60 = 0.00083$.

*Proof.* By Result 7, $\ell \le c$ iff $c$ is in the interval or above it. For $c$ above $\hat R$, $\ell \le \hat R < c$. For $c \le \hat R$, $c$ is outside the interval (so $\ell > c$) iff $(\hat R - c)^2 > z^2 c(1 - c)/n$, that is $\hat R > c + z\sqrt{c(1-c)/n}$. Equality gives $\ell = c$, so $\ell \ge c$ iff $\hat R \ge c + z\sqrt{c(1-c)/n}$. For monotonicity, let $q(p, \hat R) = (\hat R - p)^2 - z^2p(1-p)/n$, convex in $p$. At the lower root $p_-$, $q$ crosses from positive to negative, so $\partial q/\partial p < 0$ there, and $p_- \le \hat R$ gives $\partial q/\partial \hat R = 2(\hat R - p_-) \ge 0$. By the implicit function theorem, $dp_-/d\hat R = -(\partial q/\partial \hat R)/(\partial q/\partial p) \ge 0$. At the upper root both signs flip ($\partial q/\partial p > 0$ and $\partial q/\partial\hat R \le 0$), so $dp_+/d\hat R \ge 0$ too. The roots are continuous in $\hat R$, and clipping to $[0, 1]$ by a max and a min keeps a non-decreasing function non-decreasing. For the error rate, if $R \le c$, showing needs $\ell \ge c \ge R$, a miss of $R$ from below by the $z_D$ interval. Its probability tends to $\Phi(-z_D) = 0.05/60$ by Result 8. $\blacksquare$ Checked by `R Result 10` (the equivalence on 20,001 values of $\hat R$ at 30 combinations of $c$, $z$ and $n$, and monotonicity) and `S Result 10`: at $R = 0.80$ the rate is the exact binomial tail, 0.00040 at $n_e = 228$ and 0.00066 at 2,000, against 0.00083.

**Result 11 (power and the effective size it needs).** In the normal approximation with $\operatorname{Var}(\hat R) = \pi_1(1 - \pi_1)/n_e$ at a true value $\pi_1 > c$,

```math
P(\ell \ge c) \approx \Phi\!\left(\frac{(\pi_1 - c)\sqrt{n_e} - z_D\sqrt{c(1-c)}}{\sqrt{\pi_1(1 - \pi_1)}}\right), \qquad
n_e = \left(\frac{z_D\sqrt{c(1-c)} + z_{\beta}\sqrt{\pi_1(1-\pi_1)}}{\pi_1 - c}\right)^2
```

for power $1 - \beta$. For precision ($c = 0.80$, $\pi_1 = 0.90$) at 80% power, $n_e = 228.0$, so 229 effective puzzles. For recall ($c = 0.50$, $\pi_1 = 0.65$), $n_e = 173.1$, so 174.

*Proof.* By Result 10, showing means $\hat R \ge c + z_D\sqrt{c(1 - c)/n_{\text{eff}}}$, with $n_{\text{eff}} \to n_e$ because $\hat R \to \pi_1$ and $v \to \pi_1(1-\pi_1)/n_e$. Standardise: $P(\hat R \ge c + z_D\sqrt{c(1-c)/n_e}) = \Phi\big((\pi_1 - c - z_D\sqrt{c(1-c)/n_e})/\sqrt{\pi_1(1-\pi_1)/n_e}\big)$, and multiplying through by $\sqrt{n_e}$ gives the formula. Setting the argument to $z_\beta = \Phi^{-1}(0.80) = 0.8416$ and solving for $\sqrt{n_e}$ gives $n_e$. The numbers: $(3.1440 \times 0.4 + 0.8416 \times 0.3)/0.1 = 15.10$, squared 228.0; and $(3.1440 \times 0.5 + 0.8416 \times 0.4770)/0.15 = 13.16$, squared 173.1. $\blacksquare$ Checked by `R Result 11` and `S Result 11`: the exact binomial power at 229 and 174 is 0.789 and 0.814, against the normal formula's 0.803 at both. The binomial's lattice moves the power by about 0.01 either way. This is the closed form behind section 6's simulated power. The simulation adds what the formula leaves out: the stratification, the heavy cells and the realised false positives. The registration chose its design by simulation, not by this formula, and does not quote it.

**Result 12 (Bonferroni over the 30 intervals).** With $z_D = \Phi^{-1}(1 - 0.05/60) = 3.1440$, if each of the 30 decision intervals (precision and recall of 15 tags) misses its value with probability at most $0.05/30$, the probability that any misses is at most 0.05. The rule's wrong calls (showing a tag whose precision is below 0.80, or trusting one whose recall is below 0.50) are one-sided misses. So their family-wise probability is at most $30 \times 0.05/60 = 0.025$, with no assumption on the dependence between tags. Showing a tag depends on its precision interval alone, so the probability of showing any tag whose precision is below 0.80 is at most $15 \times 0.05/60 = 0.0125$.

*Proof.* Let $A_j$ be the event that interval $j$ misses. $P(\bigcup_j A_j) \le \sum_j P(A_j) \le 30 \times 0.05/30 = 0.05$, the union bound. A wrong show of tag $t$ needs $\ell_P \ge c_P > R$, the lower half of $A_{t,P}$ (Result 10), of probability at most $0.05/60$. A wrong "trusted" needs $\ell_R \ge c_R > R$ likewise. "Trusted" needs both floors (an intersection-union rule), so it is wrong only if one of these two one-sided events happens. The union over at most 30 such events is at most $30 \times 0.05/60$, and over the 15 precision events at most $15 \times 0.05/60$. $\blacksquare$ The simulated per-tag false-show rates at a true 0.80 sum to 0.0095 at the chosen design, under that 0.0125 (`power.json`). The largest, `mate`'s 0.0015, is 3 of 2,000 replicates (95% Clopper-Pearson 0.0003 to 0.0044). That is 1.8 times the premise of $0.05/60$ per event, so the simulation neither confirms nor refutes the premise for that tag.

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorems `Rematch.union_bound` and `Rematch.bonferroni`. Their hypotheses match: a finite family of events on one probability space, each of measure at most $\varepsilon$ (`union_bound`) or $\alpha/|\iota|$ with $|\iota| = 30$ (`bonferroni`), and no independence. What the Lean statement takes as given, each event's probability bound, holds here only approximately (Result 8). The machine-checked part is the union step, not the coverage. `bonferroni_fwer`, which is stated for valid p-values, is not cited: the rule reads intervals, not p-values. Checked by `R Result 12` ($z_D = 3.14398$).

**Result 13 (the allocation bounds the weights).** For $0 < f \le 1$, every weight satisfies $w_h = N_h/n_h \le 1/f$, and $n_h \ge \min(N_h, m)$. With $f = 0$, $w_h = N_h/m$ for $N_h > m$, unbounded in $N_h$.

*Proof.* There are three cases. If $n_h = N_h$, then $w_h = 1 \le 1/f$. If $n_h = \lceil fN_h\rceil \ge fN_h$, then $w_h \le 1/f$. If $n_h = m < N_h$, the max picked $m \ge \lceil fN_h\rceil \ge fN_h$, so $w_h = N_h/m \le 1/f$. The lower bound is the max. With $f = 0$, $\lceil 0 \rceil = 0$ and $n_h = \min(N_h, m)$. $\blacksquare$ Checked by `R Result 13`, for every $N_h \le 300{,}000$ at three designs, and by `N Result 13`. At the chosen design the heaviest weight is 33.3. At $f = 0$ the heaviest weight is 744 to 2,010 times the lightest across the grid's $m$, and 1,116 at $m = 200$ (the registration's "about a thousand"). One false positive in that `none` cell would add 1,116 to `smotheredMate`'s tagged total of about 6,400. That moves its precision from 1 to 0.85: 15 points, which the registration calls "several".

**Result 14 (the power simulation is pessimistic about its own intervals, approximately).** Under (M3), within-cell draws are independent, so the simulated $\operatorname{Var}(\hat R)$ is to first order $\frac{1}{X^2}\sum_h N_h^2\frac{S^2_{d,h}}{n_h}$. The plug-in $v$ still estimates $\frac{1}{X^2}\sum_h N_h^2(1 - f_h)\frac{S^2_{d,h}}{n_h}$, which is smaller. Under the normal approximation, coverage $2\Phi(z\hat\sigma/\sigma) - 1$ falls as the true $\sigma$ rises with $\hat\sigma$ held fixed. So section 6's simulated coverage and false-show rates are, to first order, conservative for the stratified design without replacement that was run.

*Proof.* With independent draws, Result 4's linearisation holds with the variance of a total of $n_h$ independent draws, $n_h \operatorname{Var}(d) \approx n_h S^2_{d,h}$, scaled by $w_h^2 = N_h^2/n_h^2$, with no factor $(1 - f_h)$. The plug-in (E4) multiplies $s^2$, which estimates the same $S^2_{d,h}$, by $(1 - f_h) \le 1$. For the coverage, $\frac{d}{d\sigma}\operatorname{erf}\big(\frac{z\hat\sigma}{\sigma\sqrt2}\big) = -\sqrt{\tfrac2\pi}\, \frac{z\hat\sigma}{\sigma^2}e^{-z^2\hat\sigma^2/(2\sigma^2)} < 0$. $\blacksquare$ Checked by `R Result 14` (the derivative), `S Result 14` and `S Result 9`. First, section 6 rewritten here from its text reproduces `power.json`'s 60 linearised rates at the chosen design within 4.5 combined Monte Carlo SE (worst 2.0 SE). Second, for `smotheredMate`, `skewer` and `fork`, one fixed finite population sampled without replacement covers its own precision at least as often as the binomial model covers 0.90. The factor $(1 - f_h)$ is at least 0.55 in every audience cell, since no cell is sampled in full (Finding 3), so the pessimism is mild.

**Result 15 (label gaps make precision a lower bound; misses of a labelled motif are real).** Let $T_i$ be the truth (the motif is in the solution) and suppose the labels have no false positives, $\ell \le T$. Then the precision against the labels is at most the precision against the truth. In general, $P(\ell \mid g) \le P(T \mid g) + P(\ell, \neg T \mid g)$. Under $\ell \le T$, every labelled puzzle we do not tag is a true miss, so $1 - $ recall$_\ell$ is a share of real misses. Recall against the labels is not ordered against recall against the truth.

*Proof.* $P(\ell \mid g) = P(\ell, T \mid g) + P(\ell, \neg T \mid g) \le P(T \mid g) + P(\ell, \neg T \mid g)$, and the last term is 0 when $\ell \le T$. The weighted estimator estimates the population shares, so the inequality carries over to $R$. If $\ell_i = 1$ and $g_i = 0$, then $T_i = 1$ and $g_i = 0$: a miss. For the last claim, recall against the labels is $P(g \mid \ell)$ and against the truth $P(g \mid T)$. These are averages over different sets, and either can be larger: the labelled motifs may be the easy ones or the hard ones. $\blacksquare$ Checked by `R Result 15` (the inequality on $10^5$ joint laws, and a case where it is strict). This is the registration's "lower bound up to label errors in the other direction" (section 2) and the decision's "missing a labelled pin is no label gap". The second needs the labels to have no false positives. Two of the pilot's hand-checked disagreements were judged label errors, so it holds only approximately.

**Result 16 (a share among all tagged puzzles bounds the share among the false positives).** If a share $\sigma$ of the puzzles we tag $t$ carry theme $\tau$, and the precision is $P$, then the share $\sigma_{\mathrm{FP}}$ among the false positives lies in $[\max(0, (\sigma - P)/(1 - P)),\ \min(1, \sigma/(1 - P))]$, and it equals $\sigma$ only by coincidence.

*Proof.* By the law of total probability over true and false positives, $\sigma = P\sigma_{\mathrm{TP}} + (1 - P)\sigma_{\mathrm{FP}}$ with $\sigma_{\mathrm{TP}} \in [0, 1]$. Solve for $\sigma_{\mathrm{FP}}$, and put $\sigma_{\mathrm{TP}} = 0$ and $\sigma_{\mathrm{TP}} = 1$ for the two ends. $\blacksquare$ Used by Finding 1, checked by `N Finding 1`.

> **Finding 1 (decision.md: shares of all tagged puzzles quoted as shares of false positives).** `decision.md` says `sacrifice`'s "false positives carry `attraction` (0.24), `discoveredAttack` (0.19) and `deflection` (0.14)", that `deflection`'s "false positives carry `attraction` (0.16) and `fork` (0.13)", and that "almost none of `hangingPiece`'s false positives carry a motif theme (the largest are `defensiveMove`, 0.10, and `intermezzo`, 0.04)". Each number is `results.json` `confusion.by_tag[t].themes[τ]`, the share over **every** puzzle we tag $t$ (E8), of which 53%, 56% and 57% are true positives. By Result 16, the shares among the false positives alone are only bounded. They lie in $[0, 0.512]$, $[0, 0.406]$ and $[0, 0.295]$ for `sacrifice`, in $[0, 0.375]$ and $[0, 0.287]$ for `deflection`, and in $[0, 0.229]$ for `hangingPiece`'s `defensiveMove`. If the true positives rarely carry these themes, the false-positive shares are about twice the quoted ones. Separately, `hangingPiece`'s two "largest" are not the largest. `fork` (0.068), `kingsideAttack` (0.067) and `advancedPawn` (0.053) all exceed `intermezzo` (0.038), and all are motif themes by `NOT_MOTIFS`. The tags' decisions do not use these shares. Only the explanatory prose is affected: "almost none" is not shown, and the shares are lower bounds on nothing. `results.json` holds no false-positive-only shares, so they cannot be computed from it. The bounds are exact for the weighted sample shares, since $\hat R$ and $\hat\sigma_{t\tau}$ share the denominator $\sum_{g = 1} w_i$.

## 5. Numbers

Every number below is checked by `math_check.py` `numbers` against the committed files. The design numbers (sizes, cells, weights, power) are recomputed from `strata.csv`, `sample.csv` and `power.json`. The estimates and effective sizes cannot be recomputed: `sample.csv` holds no tags, and the tags come from the Rust bridge run on the 1.2 GB snapshot. They are read from `results.json`, and every interval and decision is recomputed from them.

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| population 1,826,817, audience 1,580,025 | sum of cells | `strata.csv` `population` | 1,826,817; 1,580,025 |
| pilot 1,874, 25 a cell, 24 in one | sum | `strata.csv` `pilot` | 1,874; `smotheredMate/S` 24 of 24 |
| sample 47,503 audience, 54,987 in all | (E1) at $m = 50$, $f = 0.03$ over $N_h$ = population − pilot | `power.json` `audience_puzzles`, `results.json` `sample.puzzles` | 47,503; 54,987 |
| every cell's $n_h$ | (E1) | `sample.csv`, `results.json` `sample.cells` | all 74 equal |
| largest design below, 36,370, worst 0.56 | (E1) at $m = 300$, $f = 0.02$; (E9) | `power.json` `worst` | 36,370; 0.556 |
| 28 cli calls | $\lceil 54{,}987/2{,}000\rceil$ | `results.json` `compute` | 28, actual = budget |
| a `none` cell up to 223,180 | max | `strata.csv` | 223,180 |
| $z = 3.14$ | Result 12 | `results.json` `z_decision` | 3.1440 |
| every interval (540 of them) | (E6) at `estimate`, `n_eff` | `results.json` `tags[*].{precision,recall}`, `by_band` | all within rounding |
| estimates 0 or 1 use Kish's size | (E5) | `n_eff` = `n_eff_kish` | holds |
| `smotheredMate` [0.953, 1.000] | Result 7, $n_K/(n_K + z_D^2)$ at 202.3 | `tags.smotheredMate.precision` | 0.95342 |
| `hangingPiece` $n_{\text{eff}}$ 5,127 against 2,888 | Result 5 | `tags.hangingPiece.precision` | 5,127.2; 2,888 |
| the decision table (15 tags, six numbers each) | (E3), (E6) | `tags[*]` | all at 3 dp |
| decisions: 4 of 10 motif tags trusted, … | (E7) | `decisions`, `tags[*].decision` | all recomputed |
| summary's two-decimal values | (E3) | `tags[*]` | all at 2 dp |
| `skewer` 0.81 above the floor, lower end 0.778 | (E7) | `tags.skewer.precision` | 0.8098; 0.7783 |
| `matingNet` 19.7% | weighted share | `unscored.matingNet.tagged_share` | 0.1969 |
| by band: `fork` 0.86, 0.61, 0.54; `discoveredAttack` 0.95, 0.81, 0.71; `promotion` 0.83, 0.77, 0.70 (recall); `skewer` 0.90, 0.77, 0.71; `sacrifice` 0.36, 0.66 (precision) | (E3) per band | `tags[*].by_band` | all at 2 dp |
| `skewer`'s two easiest bands clear 0.80 on their estimates | – | `by_band.A1`, `A2` | 0.900; 0.814 |
| confusion: 0.10, 0.04, 0.24, 0.19, 0.14, 0.24, 0.16, 0.16, 0.13, 0.33, 0.36 | (E8) | `confusion.by_tag`, `by_theme` | all at 2 dp (Finding 1 on their reading) |
| hand check 19 of 26, 0.73, Wilson 0.54 to 0.86 | (E6) at $z_{0.05}$ | `preregistration.md` section 7 | 0.7308; [0.5392, 0.8630] |
| worst power 0.84 (±0.016), others 1.00, recall ≥ 0.90 | (E9); $1.96\sqrt{p(1-p)/2000}$ | `power.json` `power[*]["linearised/…"]["m=50,f=0.03"]` | 0.84 ± 0.0161; recall min 0.898 (`smotheredMate`), "at least 0.90" only after rounding |
| the chosen design is the first passing one by audience puzzles | `designs` sorted by `audience_puzzles` | `power.json` | m = 50, f = 0.03 |
| hand check: 19 of 26 label gaps, 2 to 4 a tag, six label-limited tags | parsed from `pilot/hand-check.md` | – | 19/26; `LABEL_LIMITED` reproduced |
| largest sampling fraction | (E1) | `strata.csv` | 0.45 (Finding 3) |
| false-show ≤ 0.0015 at 0.80, none at 0.75 or 0.70; worst coverage 0.996; Kish's 0.9975 and 0.001 | (E9) | same | 0.0015; 0; 0.996; 0.9975; 0.001 |
| Kish's: coverage 0.87, false-show 0.041 at $m = 200$, $f = 0.01$; 0.063 at $m = 300$, $f = 0.01$; nominal 0.0008 | (E9); $0.05/60$ | `power[smotheredMate]["kish/…"]` | 0.869; 0.041; 0.0625 (0.063 rounded half up); 0.00083 |
| linearised false-show ≤ 0.0025 for $f \ge 0.005$; 0.018 to 0.12 at $f = 0$ | (E9) | `power[*]["linearised/shown_at_0.80"]` | 0.0025; 0.018 to 0.1225 |
| linearised coverage ≥ 0.994 for $f \ge 0.005$ | (E9) | `power[*]["linearised/coverage_at_0.90"]` | **0.9615** at $f = 0.005$ (Finding 2) |
| weights at $f = 0$ "about a thousand times" | Result 13 | `strata.csv` | 744 to 2,010; 1,116 at $m = 200$ |

> **Finding 2 (preregistration section 6 and `review.md`: the linearised coverage at $f = 0.005$).** Section 6 says the linearised variance "keeps the coverage at 0.994 or more and the worst tag's false-show rate at 0.0025 or less at every design with a fraction f of at least 0.005", and `review.md` says it "holds at every design with f of at least 0.005". The false-show part holds (the maximum is 0.0025). The coverage part does not. At $f = 0.005$, `smotheredMate`'s simulated coverage at 0.90 is 0.9615, 0.9715, 0.9765, 0.988 and 0.988 for $m$ = 50, 100, 150, 200 and 300 (`power.json` `power.smotheredMate["linearised/coverage_at_0.90"]`). The claim holds from $f = 0.01$ (minimum 0.994). The chosen design ($f = 0.03$) is unaffected: its worst coverage is 0.996 and its false-show rate 0.0015. Even 0.996 is below the nominal two-sided $1 - 0.05/30 = 0.9983$. The one-sided miss that the rule acts on, the false-show rate, is the quantity that matters. At the chosen design its largest value is 3 of 2,000 (0.0015, 95% interval 0.0003 to 0.0044), so the simulation cannot tell it from the nominal 0.00083. Section 6's "0.0025 or less" across designs is 5 of 2,000, three times the nominal, and reads better as "small" than as "holds". No decision changes.

> **Finding 3 (decision.md: "fully sampled cells").** `decision.md` explains effective sizes above the count partly by "fully sampled cells have no finite-population term". At the confirmatory design no cell is sampled in full. The only census cell was `smotheredMate/S`, and the pilot took all 24 of its puzzles, so none are left. The largest sampling fraction in the audience is 50 of 111, or 0.45, in `mateIn4Plus/A1` (`strata.csv`, (E1)). The finite-population factor $(1 - f_h)$ therefore lies between 0.55 and 1 in every cell, and it cannot be what lifts `hangingPiece`'s $n_{\text{eff}}$ to 1.8 times its count. Result 5's mechanism, cells where $\hat d$ is nearly constant, is the remaining explanation. No decision changes. Checked by `N Finding 3`.

Two smaller readings of `decision.md` are checked by `N by band`. Its "precision falls with rating for `skewer`, `pin` and `backRankMate`" holds from A1 to A4, but strictly only for `skewer`: `pin` rises from A1 (0.40) to A2 (0.44), and `backRankMate` rises from A4 (0.44) to S (0.58). Its "5 puzzles we tag `mateIn2` that Lichess does not label" is an unweighted count that `results.json` does not hold, so it is not checked.

## 6. Assumptions that can fail

- **(M2) uniform random subsets.** Results 1 to 4 need each cell's sample to be uniform without replacement. `draw` takes the first $n_h$ of a seeded permutation, and `N` checks that every cell has its $n_h$. One generator walking the cells makes the cells' draws dependent in principle; with a 64-bit PCG generator that is negligible. Nothing covers it beyond the idealisation.
- **The estimand is the population less the pilot** (remark after Result 1). The difference from the full audience is at most 1.9% of a recall, and much less for common tags. No sensitivity covers it, and none is needed for any decision except possibly `skewer`'s, which is hidden either way (its lower end is 0.022 below the floor).
- **Ratio bias** (Result 3). At most $\operatorname{CV}(\hat X)$ standard errors, by heuristic a few per cent. No sensitivity covers it; none is needed at these margins.
- **The linearisation and the normal limit** (Results 4 and 8). They are first-order and asymptotic. Where a rare tag's precision rests on a few heavy false positives, they can fail. Section 6's simulation at every design measures this (coverage, false-show rates), and Result 14 says that simulation is conservative for the design that ran. Finding 2 is where it fails: at $f = 0.005$, not at the chosen $f = 0.03$.
- **The fallback to Kish's size at $\hat R \in \{0, 1\}$** (E5). Kish's size ignores the between-cell structure (Result 6). For a tag at 1.000 the interval's lower end is Wilson's $n_K/(n_K + z^2)$ (Result 7), a binomial bound that is justified only under simple random sampling with equal weights. This affects `smotheredMate` and the five mate-length tags. `smotheredMate`'s lower end is 0.953, so a shift of up to 0.15 would still clear 0.80.
- **Power** (Result 11 and section 6). It rests on the pilot's smoothed rates (25 puzzles a cell), the scaling of Result 9 and the independence of (M3). The registration says so: `smotheredMate`'s sample size is set by the smoothing prior. Results 9 and 14 check the construction. Its inputs remain development data.
- **Bonferroni** (Result 12). It needs no independence. It inherits each interval's coverage, which is approximate (Result 8).
- **Label quality** (Result 15). Precision is a lower bound only if the labels have no false positives, and "a labelled miss is a real miss" needs the same. The hand check found two label errors among its puzzles, and it is not blind and not human. The registration routes label-limited tags to a human audit rather than reading precision as truth. That guard is the only coverage this assumption has.
- **Puzzle solutions are not production lines** (preregistration section 2). No result here transfers the estimates to the engine's best line. EXP-0023 is filed for that.
- **The confusion shares** (Finding 1). They describe all tagged puzzles. Nothing in the committed results gives false-positive-only shares.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py`, `results.json`, `power.json`, `strata.csv` and the pilot, but not this file, `decision.md` or `summary.md`. It re-derived five results: the ratio's exact bias identity and the Hartley-Ross bound (Result 3); the linearised variance, when a cell adds nothing, and Kish's bound $n_K \le n_x$ by Cauchy-Schwarz (Results 4 to 6); Wilson's closed form and the floor equivalence (Results 7 and 10); $z_D = 3.1440$, the per-tag false-show bound $0.05/60$ and the 15-tag union 0.0125 (Results 10 and 12); and $n_e = 228.0$ and 173.1 and the weight bound $1/f$ (Results 11 and 13). Every formula and number agreed. It found Finding 2 independently. It also noted that "every tag at least 0.90 for recall" is 0.898 for `smotheredMate`, true only after rounding. That note is now in section 5, and the 15-tag bound of 0.0125, which the first draft lacked, is in Result 12.

`math_check.py`'s first run failed seven checks, and the maths was corrected, not the tolerances. One was a weight ratio of 2,010 that the draft rounded to 2,011, and one a symbolic identity written wrongly in the script. The scenario caps bind for `mate` (by $2.4 \times 10^{-5}$ at most, Result 9). The required effective sizes round up to 229 and 174. The registration's coverage claim at $f = 0.005$ is wrong (Finding 2). A Result 5 example population that could not show $n_{\text{eff}} > n_x$ was replaced by one that does.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee and made 13 points. All were accepted:

1. **Major.** No audience cell is sampled in full; the largest fraction is 0.45. Claims about "label cells sampled in full" in Results 6 and 14 were removed. `decision.md`'s "fully sampled cells" became Finding 3, checked.
2. **Major.** Result 11 quoted the powers at 228 and 173, not at 229 and 174. It now quotes 0.789, 0.814 and 0.803, asserted to 3 dp.
3. **Major.** Result 10's monotonicity proof had a sign error. It now uses the implicit-function step at both roots, and says why clipping keeps it.
4. **Major.** Result 5's account of `hangingPiece` is marked as a conjecture: no per-cell counts are committed.
5. **Major.** Result 8 now states its asymptotic regime and the Lindeberg condition it needs, and cites the stratified CLT (Bickel and Freedman 1984; Krewski and Rao 1981).
6. **Minor.** Result 3 no longer says $\hat X > 0$ almost surely. Its "1/30 of an SE" is now labelled a heuristic.
7. **Minor.** Three symbolic checks passed by construction. `R Result 6` is now an exact check on random unit-level samples. `R Result 15` tests strictness with no label error. `R Results 3` and `15` are labelled definitional.
8. **Minor.** Loose tolerances were tightened: `S Result 8` (4 SE of nominal), `S Result 10` (at most nominal), `S Result 14` (no extra slack) and the remark's bounds (asserted to their quoted values, with the 100 derived from `strata.csv`).
9. **Minor.** Unchecked numbers are now checked or declared uncheckable. The cap sizes, Finding 1's other bounds and the four shares, and the band wording are checked (`pin` and `backRankMate` are not monotone; see the note after Finding 3). So are the hand check, parsed from `pilot/hand-check.md` (19 of 26, and `LABEL_LIMITED` reproduced), and the design order, sorted by `audience_puzzles`. The "5 puzzles" of `mateIn2` cannot be checked from `results.json`.
10. **Minor.** Result 12 and Finding 2 no longer say the false-show rate is "within twice nominal". 3 of 2,000 has a 95% interval of 0.0003 to 0.0044, and section 6's 0.0025 is three times nominal.
11. **Minor.** Result 6's second part is marked illustrative, and the list of what Kish's size ignores now has three items, matching its count.
12. **Minor.** The remark after Result 1 says "unlikely to", since the precision share is estimated.
13. **Minor.** Section 5 now says which numbers are recomputed and which are only checked for consistency.

After these changes `math_check.py` runs 69 checks, all PASS.
