# EXP-0019: the maths

EXP-0019 is pre-registered and has not run: the blind audit is with the maintainer. Sections 1 to 4 are complete, as ADR 0118 asks of a registered design. They cover the estimand, the bounds, the decision rule's error control and the power. Sections 5 and 6 cover what exists now: `power.json` and the planned analysis. **The decision's numbers (the counts, the bounds, the verdict per tag) come with the run.** They will be added to section 5 then, as a dated note, without changing a proof the decision uses. `math_check.py` checks the maths, the power analysis (recomputed exactly) and `analysis.py`'s functions on synthetic answers. It never opens `data/exp19`.

Prefixes in `math_check.py`: `R` symbolic, `S` simulation or exact computation, `N` numbers.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $t$ | a tag: `hangingPiece`, `skewer`, `sacrifice`, `backRankMate` | – | `analysis.py` `TAGS` |
| $\mathcal T_t$ | the audience puzzles we tag $t$ (EXP-0007's audience, less its pilot) | puzzles | preregistration section 7 |
| $\mathcal L_t$, $\mathcal U_t$ | the part of $\mathcal T_t$ that carries Lichess's theme $t$, and the part that does not | puzzles | strata `L:<tag>`, `U:<tag>`, `make_audit.py` |
| $\pi_t = \lvert\mathcal L_t\rvert / \lvert\mathcal T_t\rvert$ | the share of tagged puzzles with the theme | share | EXP-0007 `results.json` `tags.<t>.precision.estimate` |
| $\hat\pi_t$, $n^{\text{eff}}_t$ | EXP-0007's estimate of $\pi_t$ and the smaller of its two effective sample sizes | –, puzzles | `exp7_precision` |
| $b_t$ | the share of $\mathcal U_t$ that holds the motif by section 4's definition | share | – |
| $P_t = \pi_t + (1 - \pi_t)\,b_t$ | the audited precision, the primary metric | share | `combine` |
| $n_t$ | unlabelled items judged for $t$ (30, 12, 30, 24 in one session; half at 3 blocks) | items | `n_unlabelled`, `PER_BLOCK` |
| $Y_t$, $V_t$ | of those, the answers "yes" and "unsure" (a blank in a whole block is "unsure") | items | `tally`, `done_items` |
| $M_t$ | of those, the items that truly hold the motif | items | – |
| $m_t$, $A_t$ | labelled items judged for $t$ (6, 3, 6, 3) and their "yes" count | items | `n_labelled`, `tally` |
| $g_t$, $F_t$ | items with neither our tag $t$ nor theme $t$ (93, 114, 93, 102 planned), and their "yes" count to question $t$ | items | `negatives`, `n_negatives` |
| $\alpha = 0.003125$ | one tag, one direction, one look: $0.05/(4 \cdot 2 \cdot 2)$ | – | `ALPHA_TAG_ONE_SIDED` |
| $\alpha_\pi = 0.1\alpha$, $\alpha_b = 0.9\alpha$ | the split between $\pi$ and $b$ | – | `ALPHA_PI`, `ALPHA_PART` |
| $\ell_\pi, u_\pi$ | the one-sided Wilson bounds of $\pi$ at $\alpha_\pi$, $z = \Phi^{-1}(1 - \alpha_\pi) = 3.42$ | share | `pi_bounds` |
| $\ell_b(k, n)$, $u_b(k, n)$ | the one-sided Clopper-Pearson bounds at $\alpha_b$ | share | `cp_lower`, `cp_upper` |
| $u$, $f$, $a$ | the rater's "unsure" rate, false-"yes" rate and labelled-"yes" rate (power inputs) | share | `_session` |
| $I_x(p, q)$ | the regularised incomplete beta function | – | `scipy.stats.beta.cdf` |

## 2. Model

**(M1) The estimand.** $\mathcal T_t = \mathcal L_t \sqcup \mathcal U_t$. Section 4 of the preregistration takes every puzzle in $\mathcal L_t$ as holding the motif. In $\mathcal U_t$ the motif is there or not, as section 4's definition says, and $b_t$ is the share where it is. So the share of $\mathcal T_t$ that holds the motif is $P_t$ (Result 1).

**(M2) The sample.** The frame is a simple random sample of 20,000 audience puzzles (`sample/frame`). The unlabelled items of $t$ are the first $n_t$ of a seeded random order of the frame's $\mathcal U_t$ (`sample/items`). Given the size of the frame's $\mathcal U_t$, its members are a simple random sample of the audience's $\mathcal U_t$, and the first $n_t$ of a random order of a simple random sample are a simple random sample of the audience's $\mathcal U_t$ without replacement. So $M_t$ is hypergeometric with population $\lvert\mathcal U_t\rvert$ and $b_t\lvert\mathcal U_t\rvert$ successes. The proofs use its binomial limit $M_t \sim \operatorname{Bin}(n_t, b_t)$, and `S1` checks the hypergeometric case.

**(M3) The rater.** The rater answers "unsure" on some items. On the others the analysis assumes the answer is right:

```math
\text{"yes"} \Rightarrow \text{motif}, \qquad \text{"no"} \Rightarrow \text{no motif}, \qquad
\text{so}\quad Y_t \le M_t \le Y_t + V_t .
```

The power analysis (`_session`) models "unsure" as independent of the truth at rate $u$, a false "yes" at rate $f$ on items without the motif, and "yes" on a labelled item with probability $a$. Two choices of `_session` are the power's own assumptions, not the registration's words: a labelled item is never "unsure" (section 6 says the rater is unsure on 5% of "the items"), and $b = \max(0, (P - \hat\pi)/(1 - \hat\pi))$. Result 6 and Finding 1 give the cost of the first. The bounds' coverage needs only $Y_t \le M_t \le Y_t + V_t$. A false "yes" breaks the first inequality, and that is what the guards are for (Results 5 and 8).

**(M4) $\pi$.** $\hat\pi_t$ is EXP-0007's design-weighted estimate. Its Wilson bound at $n^{\text{eff}}_t$ is taken as a valid one-sided bound at $\alpha_\pi$. That is an asymptotic approximation with thousands of effective puzzles behind it (1,373 to 6,831 by Kish).

## 3. Estimators

- **(E1) The point estimate** (`decide_tag`, `estimate`): $\hat P_t = \hat\pi_t + (1 - \hat\pi_t)\,Y_t/n_t$, with "unsure" counted as no.
- **(E2) The bounds** (`decide_tag`, `combine`):

```math
P_t^{\text{low}} = 1 - (1 - \ell_\pi)\big(1 - \ell_b(Y_t, n_t)\big), \qquad
P_t^{\text{up}} = 1 - (1 - u_\pi)\big(1 - u_b(Y_t + V_t, n_t)\big),
```

  with $\ell_b(k, n) = B^{-1}(\alpha_b;\ k, n - k + 1)$ ($0$ at $k = 0$) and $u_b(k, n) = B^{-1}(1 - \alpha_b;\ k + 1, n - k)$ ($1$ at $k = n$), where $B^{-1}$ is the Beta quantile.
- **(E3) The guards** (`decide_tag`):
  - (G1) at most one labelled item not "yes": $m_t - A_t \le 1$;
  - (G2) $\hat\pi_t A_t/m_t + (1 - \hat\pi_t) Y_t/n_t \ge 0.80$;
  - (G3) $u_b(F_t, g_t)$ at $\alpha = 0.025$ is at most 0.15.
- **(E4) The decision** (`decide_tag`): "clears" if $P^{\text{low}}_t \ge 0.80$ and G1 to G3 hold; "fails" if $P^{\text{up}}_t < 0.80$; "inconclusive" otherwise.
- **(E5) The continuation** (`follow_up`): an inconclusive tag goes on to a second session if $\hat P_t \ge 0.88$ or $\hat P_t \le 0.72$. Look 2 applies (E2) to (E4) to both sessions pooled, at the same $\alpha$.
- **(E6) The threshold** (`cmd_power`, `threshold_b`): $b^\star_t = (0.80 - \ell_\pi)/(1 - \ell_\pi)$.
- **(E7) The design** (`PER_BLOCK`, `n_labelled`, `n_negatives`): blocks of 22 (5, 5, 4 and 2 unlabelled, 3 labelled, 3 distractors), 6 blocks. $g_t$ is the session's items less the tag's unlabelled and labelled ones, less one distractor in two blocks that may carry the theme. That last term is an allowance for the power analysis, not a count: any of the 9 `D:miss` items, or another stratum's item, may carry theme $t$. The run counts the actual $g_t$ (`negatives`), and the guard's cut-off follows from it.
- **(E8) Power** (`simulate`): (E4) and (E5) on draws from (M3), 20,000 replicates a scenario, with $b = \max(0, (P - \hat\pi)/(1 - \hat\pi))$.

## 4. Derivations and proofs

**Result 1 (the decomposition).** Under (M1), the share of $\mathcal T_t$ that holds the motif is $P_t = \pi_t + (1 - \pi_t) b_t = 1 - (1 - \pi_t)(1 - b_t)$. It is non-decreasing in $\pi_t$ and in $b_t$ ($\partial P/\partial\pi = 1 - b \ge 0$, $\partial P/\partial b = 1 - \pi \ge 0$), and for $\pi_t < 1$ it is inverted by $b_t = (P_t - \pi_t)/(1 - \pi_t)$.

*Proof.* By the law of total probability over the partition $\mathcal L_t \sqcup \mathcal U_t$, the share is $\pi_t \cdot 1 + (1 - \pi_t) \cdot b_t$. Expanding, $1 - (1 - \pi)(1 - b) = \pi + b - \pi b = \pi + (1 - \pi)b$. The partial derivatives follow, and substituting $b = (P - \pi)/(1 - \pi)$ gives $\pi + (P - \pi) = P$. $\blacksquare$

Checked by `R1`. The rule cannot fail a tag whose $u_\pi \ge 0.80$, because then $P^{\text{up}} = 1 - (1 - u_\pi)(1 - u_b) \ge u_\pi \ge 0.80$. That is `skewer` ($u_\pi = 0.8434$), as section 6 says. Its "$P = 0.80$" row in `power.json` really has $b = 0$ and $P = \pi = 0.81$ (`N Result 6`).

**Result 2 (Clopper-Pearson is exact and conservative).** If $M \sim \operatorname{Bin}(n, b)$, then for every $b \in [0, 1]$, $P_b\big(\ell_b(M, n) > b\big) \le \alpha_b$ and $P_b\big(u_b(M, n) < b\big) \le \alpha_b$. For a sample without replacement, the same holds by exact computation, though not by proof, at every size the decision uses ($n$ = 6 to 60) and for strata of 100 to 20,000 puzzles.

*Proof.* The binomial tail is a regularised incomplete beta function, $P(\operatorname{Bin}(n, b) \ge k) = I_b(k, n - k + 1)$ for $1 \le k \le n$. Differentiating the right side in $b$ gives the density $b^{k-1}(1-b)^{n-k}/B(k, n-k+1)$. Differentiating the left side gives the same: the sum telescopes to $n\binom{n-1}{k-1}b^{k-1}(1-b)^{n-k}$. Both sides vanish at $b = 0$.

So $\ell_b(k, n)$ is the $b$ at which $P_b(M \ge k) = \alpha_b$. The tail $P_b(M \ge k)$ is strictly increasing in $b$, so $\ell_b(k, n) > b \iff P_b(M \ge k) < \alpha_b$. And $\ell_b(k, n)$ is increasing in $k$: at a fixed $b$, $P_b(M \ge k)$ falls as $k$ grows, so it reaches $\alpha_b$ at a larger $b$. Let $k_0 = \min\{k : \ell_b(k, n) > b\}$. Then $\{\ell_b(M, n) > b\} = \{M \ge k_0\}$, whose probability is $P_b(M \ge k_0) < \alpha_b$; it is $0$ if no such $k$ exists. The upper bound is the mirror image, via $P(\operatorname{Bin} \le k) = 1 - I_b(k + 1, n - k)$.

Without replacement, the hypergeometric tail is not ordered against the binomial one for every $k$, so the bound is checked rather than proved. For $n$ = 6, 12, 15, 24, 30, 48 and 60, strata of 100 to 20,000 puzzles, and about 200 values of $b$ each, the largest error is 0.002807 for both bounds, below $\alpha_b$. $\blacksquare$ (the binomial part; the hypergeometric part is computed, not proved).

Checked by `R2` (the identity in exact rationals with sympy, $n = 5$ and 12, every $k$, three $b$), `S1` (the binomial worst case 0.002810 over $n = 12$ to 60 and 97 values of $b$, a simulation at $n = 30$, $b = 0.6$, and the hypergeometric strata).

**Result 3 (the bound on $P$, and "unsure").** Under (M1) to (M4),

```math
P\big(P_t^{\text{low}} > P_t\big) \le \alpha_\pi + \alpha_b = \alpha, \qquad P\big(P_t^{\text{up}} < P_t\big) \le \alpha,
```

whatever the joint law of $\hat\pi_t$ and the audit, and with "unsure" placed anywhere between "yes" and "no".

*Proof.* (i) *Unsure.* By (M3), $Y_t \le M_t \le Y_t + V_t$. $\ell_b(\cdot, n)$ and $u_b(\cdot, n)$ are non-decreasing in $k$ (Result 2), so $\ell_b(Y_t, n) \le \ell_b(M_t, n)$ and $u_b(Y_t + V_t, n) \ge u_b(M_t, n)$. Both bounds are therefore at least as safe as those an exact rater would give. The two halves need different things of the rater: the lower bound needs no false "yes" ($Y_t \le M_t$), and the upper bound needs no false "no" on a true motif ($M_t \le Y_t + V_t$).

(ii) *Monotonicity.* If $\ell_\pi \le \pi_t$ and $\ell_b(M_t, n) \le b_t$, then $P^{\text{low}}_t \le 1 - (1 - \pi_t)(1 - b_t) = P_t$, by Result 1. So $\{P^{\text{low}}_t > P_t\} \subseteq \{\ell_\pi > \pi_t\} \cup \{\ell_b(M_t, n) > b_t\}$.

(iii) *The union bound.* $P(\ell_\pi > \pi_t) \le \alpha_\pi$ by (M4), and $P(\ell_b > b_t) \le \alpha_b$ by Result 2 (for the hypergeometric case, by its computed check), so the union has probability at most $\alpha_\pi + \alpha_b = \alpha$. No independence between EXP-0007 and the audit is needed. The upper bound is symmetric. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.union_bound`, for step (iii). Its hypothesis is a finite family of events each of probability at most $\varepsilon$. Here there are two events of different sizes, $\alpha_\pi$ and $\alpha_b$, so the theorem gives $2\max(\alpha_\pi, \alpha_b) = 0.005625$, not the sharper $\alpha_\pi + \alpha_b$. The sharper sum is the plain additivity bound $P(A \cup B) \le P(A) + P(B)$, which the prose proves. So the citation covers step (iii) only partly. Steps (i) and (ii) are not in Lean. Checked by `R1` (monotonicity) and `S2`. Under (M4)'s normal model of $\hat\pi$ (which `S2` assumes, so it does not test (M4)), the exact miss rate at $n = 30$, at each $b$ just below a jump of $\ell_b$, is at most 0.00065. That is a fifth of $\alpha$, because $\pi$'s bound has slack. Without $\pi$'s bound the rate is 0.0025, and with $b$'s bound set at $10\alpha_b$ it is 0.0097, above $\alpha$, so the check can fail.

**Result 4 (the family's error).** Count the tests: four tags, two directions (a false "clears" and a false "fails"), two looks, each at $\alpha = 0.003125$. Under (M1) to (M4), the probability of any wrong verdict across them is at most $16\alpha = 0.05$, under any dependence between the tags (they are judged on the same items) and between the looks (look 2 pools look 1).

*Proof.* A wrong "clears" for tag $t$ at look $j$ needs $P^{\text{low}}_{t,j} \ge 0.80 > P_t$, so it lies in $\{P^{\text{low}}_{t,j} > P_t\}$, of probability at most $\alpha$ by Result 3. A wrong "fails" needs $P^{\text{up}}_{t,j} < 0.80 \le P_t$, which lies in $\{P^{\text{up}}_{t,j} < P_t\}$. A look that never happens contributes the empty event. There are at most 16 such events, so the union bound gives at most $16\alpha = 0.05$. Within one tag the two directions cannot both be wrong, since $P_t$ is on one side of 0.80, so the realised family is at most $8\alpha = 0.025$. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.bonferroni`, whose hypothesis is a family of $m$ events each of probability at most $\alpha/m$. That matches here, with $m = 16$ and the family level 0.05, given each event's bound $\alpha = 0.05/16$. That bound comes from Result 3, which is in prose (Lean's `union_bound` gives only $2\max(\alpha_\pi, \alpha_b)$ for it), so the chain to 0.05 is machine-checked only from Result 3 on. The realised $8\alpha$ is prose only. Checked by `R4`.

**Result 5 (the guards and the threshold).** (i) The guards remove clears and never add one: "clears" is a subset of $\{P^{\text{low}}_t \ge 0.80\}$, so Results 3 and 4 hold with them. (ii) $P^{\text{low}}_t \ge 0.80$ if and only if $\ell_b(Y_t, n_t) \ge b^\star_t = (0.80 - \ell_\pi)/(1 - \ell_\pi)$. With $\ell_\pi$ = 0.5400, 0.7710, 0.5077 and 0.5613, $b^\star$ is 0.5653, 0.1266, 0.5937 and 0.5441.

*Proof.* (i) (E4) defines "clears" as the bound condition and the three guards, a conjunction. So it is a subset of the bound condition alone, and a wrong clear under the guards is a wrong clear without them. (ii) $1 - (1 - \ell)(1 - x) \ge 0.80 \iff 1 - x \le 0.20/(1 - \ell) \iff x \ge 1 - 0.20/(1 - \ell) = (0.80 - \ell)/(1 - \ell)$, for $\ell < 1$. $\blacksquare$

Checked by `R5` and `N section 6`.

**Result 6 (the power, exactly).** Write $c_1(y, l) = \mathbf 1[P^{\text{low}}(y, n) \ge 0.8,\ m - l \le 1,\ G2(y, l)]$, $\varphi_1(r) = \mathbf 1[u_b(r, g) \le 0.15]$ for (G3), and $d_1(s) = \mathbf 1[P^{\text{up}}(s, n) < 0.8]$. Write $c_2$, $\varphi_2$ and $d_2$ for the same at the pooled sizes $(2n, 2m, 2g)$. Under (M3) each session is independent, with:

- $(Y, V) \sim \operatorname{Mult}(n;\ (1-u)(b + (1-b)f),\ u)$;
- $A \sim \operatorname{Bin}(m, a)$;
- $F \sim \operatorname{Bin}(g, (1-u)f)$.

Then

```math
\begin{aligned}
P(\text{clears}) &= \sum_{y, l, r} p(y)\,p(l)\,p(r)\, c_1(y, l)\,\varphi_1(r) \\
&\quad + \sum_{\substack{y, v, l, r:\ \text{go on}}} p(y, v)\,p(l)\,p(r)\ \sum_{y', l', r'} p(y')\,p(l')\,p(r')\ c_2(y + y', l + l')\,\varphi_2(r + r'),
\end{aligned}
```

where "go on" is $\neg(c_1 \varphi_1) \wedge \neg d_1(y + v) \wedge (\hat P(y) \ge 0.88 \vee \hat P(y) \le 0.72)$. $P(\text{fails})$ has the same form with $d_1(y + v)$ and $d_2(y + v + y' + v')$. Every number in `power.json` agrees with this to within 4.5 Monte Carlo SE: 280 values of the main tables, 108 of the sensitivity, 48 of the lenient raters and 120 of `n_curve`. 4.5 is a family-level threshold: the largest $\lvert z \rvert$ of 556 independent normals is above it with probability about 0.004. The referee's own computation agrees with `exact_power` to $3 \times 10^{-15}$, and its rerun of `analysis.py power` regenerates `power.json` exactly.

*Proof.* Look 1 is a function of the four counts, which are independent by (M3), so its probabilities are sums over their product law. Since $c_1$ does not depend on $v$, the first sum marginalises $v$. Look 2 happens on the event "go on", which is a function of look 1's counts. Given look 1, the second session's counts are independent of the first's and have the same law, and look 2 is a function of the sums. The two parts are disjoint, because look 2 happens only when look 1 neither cleared nor failed, so their probabilities add. Clears and fails are disjoint at each look, since $P^{\text{low}} \le P^{\text{up}}$. $\blacksquare$

Checked by `S3`, which computes the sums term by term (`exact_power`). The power analysis's continuation is `simulate`'s, which continues every tag that neither cleared nor failed. The registered analysis code does not do the same (Finding 1). The largest $\lvert z \rvert$ over the main tables is 3.98, at the 3-block minimum for `sacrifice` at 0.85 (`follow_up_share` 0.2222 against an exact 0.2107). It is next to 3.67 for `backRankMate` at 0.90 in the same 3-block table. A rerun of `analysis.simulate` at 400,000 replicates gives 0.2108 (`follow_up_share`) and 0.0592 (`clears`; committed 0.06515), against the exact 0.2107 and 0.0590. So the exact formula and the simulator agree, and the two committed values are Monte Carlo deviations. Neither is quoted in the registration or the summary. Every quoted number is within its rounding of `power.json`'s Monte Carlo value (section 5). Against the exact values a few roundings differ (Finding 2 (c)). If labelled items are "unsure" at the same 5% as the others (labelled "yes" at $a = 0.95$), a clear at 0.95 has probability 0.84 to 0.93, against the quoted 0.90 to 0.99, and 0.27 to 0.55 at 0.90, against 0.29 to 0.66.

**Result 7 (a lenient rater).** If the rater says "yes" falsely at rate $f$, the audit estimates $b' = b + (1 - b)f$, not $b$. $P^{\text{low}}$ then bounds $1 - (1 - \pi)(1 - b')$, which is above $P$ by $(1 - \pi)(1 - b)f$. Section 9's protection is (G3), not the bound. At $P = 0.80$ a wrong clear comes in at most 0.0011 of the replicates at $f = 0.10$ (`power.json` `lenient_rater`), still below $\alpha$. At $f = 0.20$, clears at a true 0.90 fall to at most 0.0011.

*Proof.* Under (M3) with a false "yes", an item without the motif is answered "yes" with probability $(1-u)f$, so $E[Y/n] = (1-u)(b + (1-b)f) = (1-u)b'$. Result 3's step (i) then fails, because $Y \le M$ no longer holds. The bound covers $b'$ in place of $b$, and $1 - (1 - \pi)(1 - b') - P = (1 - \pi)(b' - b) = (1 - \pi)(1 - b) f$. The rates are Result 6's sums with $f > 0$. $\blacksquare$

Checked by `S3` (all 48 lenient-rater values, exactly) and `N section 6`. The error claim at $f = 0.10$ is about the rater modelled. A rater whose false "yes" falls only on $\mathcal U_t$ and not on the negatives would not be seen by (G3). The preregistration says so (section 6: "a rater whose errors depend on the tag in a way the guards cannot see is not covered").

**Result 8 (the false-yes guard's operating points).** (G3) passes if and only if $F_t \le 7, 9, 7, 8$ of $g_t = 93, 114, 93, 102$ (`hangingPiece`, `skewer`, `sacrifice`, `backRankMate`). With $u = 0.05$, it always passes for an exact rater ($f = 0$). At $f = 0.05$, 0.10 and 0.20 it passes with probability 0.925 to 0.955, 0.332 to 0.359, and 0.001 to 0.002.

*Proof.* $u_b(r, g)$ at $\alpha = 0.025$ is increasing in $r$ (Result 2), so (G3) is $\{F \le r^\star\}$ with $r^\star = \max\{r : u_b(r, g) \le 0.15\}$. Under (M3) a negative item, taken to lack the motif, is answered "yes" with probability $(1-u)f$, so $P(G3) = P(\operatorname{Bin}(g, 0.95 f) \le r^\star)$. $\blacksquare$

Checked by `N Result 8` (the cut-offs and both neighbours, and the pass rates). The guard is strict. A rater at 5% false "yes" is stopped from a clear about one time in 13 for `hangingPiece` and `sacrifice`, one in 19 for `backRankMate` and one in 22 for `skewer`, whatever the truth; at 10%, about two times in three. The negatives are "taken to lack the motif" (section 6), so a negative that truly holds it counts against an honest rater. This is the cautious direction.

**Result 9 (the analysis code computes these formulas).** On 300 synthetic keys and answer files (random strata, random answers including blanks, and sessions of 1 to 6 blocks with the last block partly done):

- `done_items` keeps exactly the whole blocks;
- `tally` and `negatives` equal the counts taken here from the raw answers, with a blank in a whole block counted as "unsure";
- `decide_tag`'s bounds and decision equal (E2) to (E4) as written in `math_check.py`.

All "yes" on 30 `hangingPiece` items clears; all "no" fails; 10 false "yes" of 93 negatives turns the clear into "inconclusive", with the guard named.

*Proof.* Not a proof: a test of the code against the formulas on data with a known answer. $\blacksquare$ Checked by `S4`, which also runs `decide_tag` and `follow_up` over every state of a grid: 6 and 3 blocks, every $Y$, several $V$, every $A$, and $F$ at and around the guard's cut-off. That is 11,292 states, 200 clears and 1,220 bound-clears blocked by a guard, so a wrong guard constant or continuation threshold would fail it. The synthetic keys are built in memory; no file under `data/exp19` is read.

**Result 10 (the design's arithmetic).** The block mix is $5 + 5 + 4 + 2 + 3 + 3 = 22$ and $6 \times 22 = 132$ items, 88 minutes at 40 seconds. The unlabelled items are 30, 12, 30 and 24, the labelled 6, 3, 6 and 3, and the planned negatives $132 - n_t - m_t - 3$ = 93, 114, 93 and 102.

*Proof.* The labelled turn goes to `backRankMate` in odd blocks and `skewer` in even ones, $\lceil 6/2 \rceil = \lfloor 6/2 \rfloor = 3$. The negatives subtract, from all 132 items, the tag's own unlabelled and labelled items and the $\lfloor 6/2 \rfloor = 3$ distractors that may carry its theme (E7). $\blacksquare$

Checked by `R10` and `N section 0 and 6`. The actual negatives depend on which items carry two of our tags. The key records them, and the analysis counts them (`negatives`), so the planned counts are approximate.

> **Finding 1 (the continuation rule in `analysis.py` and in its power simulation differ).** Section 9 continues "an inconclusive tag", and section 9 calls "inconclusive" anything that neither clears nor fails, "including a tag whose bound clears while a guard does not". `simulate` follows that: `go_on = ~clears1 & ~fails1 & (...)`. But `follow_up`, which the registered run uses, tests `tag["decision"] == "inconclusive"` by string equality. For a tag whose bound clears and a guard fails, `decide_tag` returns `"inconclusive: the bound clears, the guards do not (...)"`, so `follow_up` returns False, and that tag gets no second session.
>
> `S4 Finding 1` shows it on a synthetic tag (29 "yes" of 30, 3 of 6 labelled not "yes", estimate 0.986). The cost, computed exactly with the code's rule:
>
> - none in the main tables, where the labelled items are all "yes" and there are no false "yes", so a bound that clears always comes with its guards;
> - up to 0.168 of the chance to clear in the sensitivity rows with labelled "yes" at 0.85, at a true 0.95 (`skewer`, no "unsure"). The registration's "Labelled items judged 'yes' with probability 0.85 instead of 1 lower the chance to clear at 0.95 to 0.70 to 0.78" would read 0.61 to 0.73 under the code;
> - up to 0.211 in the lenient-rater rows (`skewer` at 0.90, 10% false "yes"), and 0.024 at 5%;
> - in the main scenario too, once labelled items can be "unsure" like the others ($a = 0.95$, the power's unstated assumption of (M3)): `skewer` at 0.95 falls from 0.930 to 0.846, and the four tags' range from 0.84–0.93 to 0.84–0.89.
>
> The error control is unaffected, since dropping a continuation can only remove clears and fails. Neither the registration nor the code may change now, so this is for the coordinator. The registered run will follow `analysis.py` as frozen, and `decision.md` should say which rule it applied to any tag stopped by a guard. Reported to the coordinator.

> **Finding 2 (small misstatements in section 6).** (a) and (b) are about the rule's own error rates, and both stay far below $\alpha$. (c) is about roundings.
>
> - (a) "0.0006 at the minimum of 3 blocks" is `power.json`'s Monte Carlo value of 0.00055, which rounds to 0.0006 or 0.0005. The exact rate is 0.00074 (`hangingPiece`), which would be quoted as 0.0007.
> - (b) For `skewer` the rows labelled $P = 0.70$, 0.75 and 0.80 have $b = 0$, so their true $P$ is $\pi = 0.81$ (Result 1). A clear there is not a wrong clear. The lenient-rater figure "at most 0.0011" is right because its maximum is `hangingPiece`'s, not `skewer`'s.
> - (c) The registration's numbers are `power.json`'s Monte Carlo values, quoted at their rounding, and they match it. Against the exact values (Result 6) some round differently. Quoted against exact: `hangingPiece` at 0.90, 0.38 (0.13) against 0.3731 (0.1362); `skewer` at 0.90, 0.66 against 0.6671; `backRankMate` at 0.95, 0.92 against 0.9256; at 0.98, look 1, 0.80 against 0.7949; at 0.80, `sacrifice` 0.0004 against 0.00029 and `backRankMate` 0 against 0.00009; "0.29 to 0.66" against 0.29 to 0.67; "0.70 to 0.78" against 0.70 to 0.79; `n_curve`'s "0.53 to 0.59" against 0.53 to 0.58. None changes a claim's direction.

## 5. Numbers

The decision's numbers do not exist yet: there are no answers, so no counts, bounds or verdicts. When the audit is done and `results.json` is committed, the counts $Y_t, V_t, n_t$, $A_t$, $F_t$, $g_t$ and each tag's $P^{\text{low}}_t$, $P^{\text{up}}_t$ and guards will be recomputed here from (E1) to (E4), with their `results.json` keys, and `math_check.py` will assert them.

What exists now, recomputed by `math_check.py` from `power.json` and EXP-0007's `results.json`:

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| $\pi$ 0.572, 0.810, 0.528, 0.606 (section 1) | – | EXP-0007 `tags.<t>.precision.estimate` | 0.5717, 0.8098, 0.5284, 0.6059 |
| $\pi$'s bounds at $\alpha_\pi$ | (M4), Wilson | EXP-0007 `estimate`, `n_eff_kish` | [0.5400, 0.6029], [0.7710, 0.8434], [0.5077, 0.5490], [0.5613, 0.6488] |
| thresholds on $b$ 0.565, 0.127, 0.594, 0.544 | Result 5 | `threshold_b` | 0.5653, 0.1266, 0.5937, 0.5441 |
| 132 items, about 90 minutes (88 at 40 s) | Result 10 | `design` | 132, 88.0 |
| 93 to 114 items with neither tag nor theme | Result 10 | `design.negatives` | 93, 114, 93, 102 |
| the clear table (0.0003 to 1.00, look 1 in brackets) | Result 6 | `main."all blocks".<t>.<P>.clears`, `look1_clears` | every cell within its rounding, and within 4 SE of the exact value |
| fails at 0.70: 0.34, 0.37, 0.20; `skewer` cannot fail | Results 1, 6 | `main."all blocks".<t>."0.7".fails` | 0.3389, 0.3719, 0.2013; $\hat\pi_{\text{skewer}} = 0.81$ |
| at 0.80: wrong fail at most 0.0006, wrong clear at most 0.0004 (0.0006 at 3 blocks) | Result 6 | `main.*.<t>."0.8"` | 0.00065, 0.0004, 0.00055 (exact: 0.00046, 0.00029, 0.00074) |
| 10% false yes: wrong clear at most 0.0011 | Result 7 | `lenient_rater."false yes 0.1"` | 0.00105 |
| at 0.90: 0.29 to 0.66; at 0.95: 0.90 to 0.99, one look 0.40 to 0.96 | Result 6 | `main` | as quoted |
| 80 items: 0.53 to 0.59 at 0.90 | Result 6 | `n_curve.<t>."80"."0.9"` | as quoted (the three hard tags) |
| no unsure 0.52 to 0.73; 20% unsure 0.01 to 0.44 (0.14 to 0.93 at 0.95); labelled 0.85: 0.70 to 0.78 | Result 6 | `sensitivity` | as quoted |
| 3 blocks: 0.41 to 0.91 at 0.95 | Result 6 | `main."minimum blocks"` | as quoted |
| summary: 0.90 to 0.99 at 0.95, 0.29 to 0.66 at 0.90 | Result 6 | `main` | as quoted |

One rounding to note. The 3-block wrong clear's worst case in `power.json` is 0.00055, and the registration quotes 0.0006. The exact value for that cell (`hangingPiece`) is 0.00074, which is still below $\alpha$, so the claim "against the 0.003125" holds.

Not recomputed: "a full human check of every tagged puzzle would need about four times the items" (section 0). It is a planning statement with no committed computation behind it. The 20 seconds of tagging and the time per item will be reported by the run.

## 6. Assumptions that can fail

This section covers the planned analysis. What the run shows about each assumption (the share of "unsure", the false-"yes" count, the labelled answers) is added with the decision.

- **The labelled share is the motif (M1): Result 1.** $P$ takes every puzzle in $\mathcal L_t$ as holding the motif. If the rater's reading of section 4's definition differs from Lichess's, $P$ is too high by $\pi_t$ times the labelled share that fails the definition. Guards G1 and G2 cover it: no clear with two or more labelled items not "yes", or with $\pi a + (1-\pi) b < 0.80$. With 3 to 6 labelled items they catch a gross difference only. At $a = 0.85$, G1 stops a clear with probability $1 - P(\operatorname{Bin}(6, 0.85) \ge 5) = 0.22$ for `hangingPiece` and `sacrifice`, and $1 - P(\operatorname{Bin}(3, 0.85) \ge 2) = 0.06$ for the others. That is weak protection for an assumption that $P$ rests on.
- **The rater is right when not unsure (M3): Results 3 and 7.** A false "no" lowers both bounds, which errs towards "hidden". A false "yes" raises them, and only G3 guards it (Result 8). Correlated errors, such as a rater more lenient on $\mathcal U_t$ than on the negatives, are not covered, as section 6 of the registration says. There is one rater, and the optional second-rater kappa never decides.
- **"Unsure" independent of the truth: Result 6 only.** The bounds do not need it (Result 3 (i)); the power does. If "unsure" falls mostly on true motifs, the chance to clear drops, which is the cautious direction.
- **$\pi$'s Wilson bound (M4): Result 3.** It is asymptotic. The share of $\alpha$ given to $\pi$ is a tenth, and its effective sample is in the thousands. An error in $\pi$'s coverage moves the family error by a fraction of $\alpha_\pi$ per test. The $\pi$ events are shared across looks, and only one direction per tag can be wrong, so the total effect is at most $4\alpha_\pi = 0.00125$ per doubling of $\pi$'s miss rate.
- **The frame and the audience (M2): Result 2.** The items are a sample without replacement from the audience's $\mathcal U_t$. `S1` checks the bound's size on strata of 100 to 5,000. The frame's share of labelled puzzles (`pi_in_frame`) is a registered diagnostic against EXP-0007's $\pi$. A large gap would mean the frame and EXP-0007's sample differ, and it will be reported with the decision.
- **The tagger at the version measured: section 3 of the registration.** $\pi$ comes from EXP-0007 on `algo-1.1.0`, the audit's tags from `algo-3.0.0`. The registration argues the tagger is unchanged, and the key records the `algorithm_id` of the tagging. A sample that spans two versions stops the run.
- **Whole blocks and the early stop: (E4).** Only whole blocks are analysed. An early stop is allowed only with a written reason, because the rater sees their own answers. The bounds assume the number of blocks does not depend on the answers. The fixed length and the written reason keep that dependence small, and nothing in the maths bounds it.
- **The continuation rule: Results 4 and 6.** Look 2 is paid for in the family (two looks). The power assumes the follow-up has the same size and mix, as section 7 registers.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py`, `power.json` and EXP-0007's precision entries, but not this file or `math_check.py`, and no access to `data/exp19`. It derived independently:

- the estimand and its three assumptions (Result 1);
- the union-bound split, Clopper-Pearson's exactness, the "unsure" argument with its two different rater assumptions, and $\pi$'s bounds and the thresholds on $b$, all four to four decimals (Results 2, 3 and 5);
- the family bound of $16\alpha$, with no dependence assumption, the observation that it is $8\alpha$ in effect, and that the guards cannot add a clear (Results 4 and 5);
- the exact power with the follow-up: 0.3731 and 0.2928 at 0.90 and 0.9465 and 0.9004 at 0.95 for `hangingPiece` and `sacrifice`, and wrong clears and fails at 0.80 that match `exact_power` to four significant figures, including 0.00074 at 3 blocks (Result 6);
- the false-yes guard's cut-offs 7, 9 and 8, with the same pass rates (Result 8).

Every formula and number agreed. On the hypergeometric case it cited Hoeffding's convex-order result, where this file checks the bound numerically on strata of 100 to 5,000 (Result 2). It found Finding 1, which the first draft of this file did not have: the draft's `exact_power` had copied `simulate`'s continuation, and so agreed with `power.json` while missing that the run's code differs. `S4 Finding 1` now tests `follow_up` itself, and `exact_power(code_rule=True)` measures the cost. It also raised Finding 2 (a) and (b), the weak labelled guard (section 6 now gives 0.06 for three items), and the rater assumption behind the upper bound, which Result 3 now states.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee. It wrote its own exact computation of the section 9 rule from the registration's text, which agrees with `exact_power` to $3 \times 10^{-15}$ under both continuation rules. It ran `decide_tag` and `follow_up` over 34,029 states with no mismatch, regenerated `power.json` exactly from `analysis.py power` in a scratch folder, and ran mutated copies of `analysis.py` through this checker. It found Results 1 to 8 and 10 and Findings 1 and 2 correct, and sections 5 and 6 free of claimed results. Its points, all accepted:

1. **Major.** `exact_power` shares `simulate`'s unstated choice that labelled items are never "unsure". (M3) now names it as the power's assumption. Result 6 gives its effect (0.84 to 0.93 at 0.95), and Finding 1 its cost under the code's rule.
2. **Major.** The quoted numbers match `power.json`'s Monte Carlo values, not always the exact ones. Finding 2 (c) lists the differences, and `N Finding 2 (c)` asserts the exact values.
3. **Major.** The random sessions of `S4` never cleared, so mutated guards and continuation thresholds passed. `S4` now checks every state of a grid with 200 clears and 1,220 guard-blocked bounds.
4. **Major.** Finding 1 understated the cost. It now covers the lenient rows (up to 0.211) and the "unsure" on labelled items.
5. **Minor.** The Lean citation for Result 3 is now marked partial, and Result 4 says where the machine-checked chain starts.
6. **Minor.** `S2` could not fail. It is now exact at the worst $b$, and shown to fail with a broken bound.
7. **Minor.** `R2` compared scipy with itself. It now uses exact rationals.
8. **Minor.** The hypergeometric check now covers $n$ = 6 to 60, both bounds and strata up to 20,000, and Result 2 calls it computed, not proved.
9. **Minor.** `skewer` cannot fail because $u_\pi \ge 0.80$, not because $\hat\pi > 0.80$. Fixed in Result 1 and its check.
10. **Minor.** "One time in thirteen" holds for two tags. Result 8 now gives all four, and the pass-rate ranges are asserted.
11. **Minor.** Unchecked quotes are now checked: the recalls 0.82 to 1.00 and `pin`'s 0.20, the 20,000 replicates, and the 352 items and 3.9 hours behind "more than 300 items and four hours". The sample-drawing figures of section 0 (20,080 solutions, 11 calls, 20 s) and the pilot's "19 of 26" are in no committed file and stay unchecked. `summary.md` quotes only section 6's figures, which are checked.
12. **Minor.** Results 3 and 4 now assume (M1) as well.
13. **Minor.** The $\pi$ bullet of section 6 now gives $4\alpha_\pi$.
14. **Minor.** The negatives' distractor term is named as an allowance in (E7).
15. **Minor.** `S3`'s tolerance is now a stated family-level 4.5, and Result 6 names the key of its 0.0592.
16. **Minor.** Labels and the monotonicity step of Result 2 are fixed.

After these changes `math_check.py` runs 48 checks, all PASS, and the lab check with `REMATCH_MATH_RULE=fail` passes.
