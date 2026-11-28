# EXP-0030: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic checks (prefix `R`), simulations from a known truth (prefix `S`) and recomputation of every quoted number (prefix `N`).

EXP-0030 compares rules, and the rule it ships carries a model: a rating is a mean-reverting series around the player's strength, and its parameters come from the player's own rating steps. So the maths has to show four things. First, that the model follows from an Elo-type update, and how far its plug-in estimates are off. Second, that the closed forms in the Rust and the cohort check are the variance they claim to be. Third, that the decision rule controls its errors at the level the registration states, and what its power is. Fourth, that the arguments the registration makes in words hold: the champion's bootstrap is too narrow by the stated factor, the two candidates coincide at 60 games, the cohort's two-sided share is an upper bound, and the lag-1 bias is about $-1/4$.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $n$ | rated games read, $n \in \{30, 60, 120, 250\}$ | games | `analysis.py` `CHECKPOINTS`, `EXPLORE_GAMES` |
| $r_t$, $t = 0, \dots, n - 1$ | the pre-game rating of game $t$, in played order | rating points | `trajectory.rs` `trajectory_input`, `compute_with` |
| $\theta$ | the player's true strength, constant for the false-alarm populations | rating points | `analysis.py` `Runner.simulate` |
| $e_t = r_t - \theta$ | the rating's error | rating points | – |
| $d_t = r_{t+1} - r_t$ | a rating step, $t = 0, \dots, n - 2$ | rating points | `Memory::of` |
| $s$ | the RMS step, $s = \sqrt{\frac{1}{n-1}\sum_t d_t^2}$ | rating points | `Memory::of` (`rms`), `analysis.py` `model_var` (`rms`) |
| $q = \ln 10 / 400 = 0.005756$ | the rating scale | 1/point | `RATING_SCALE`, `analysis.py` `Q` |
| $K$ | an Elo-type update's step size | points | – |
| $o_t$ | the opponent's rating | points | `analysis.py` `play` (`r_opp`) |
| $E(x) = 1/(1 + 10^{-x/400})$ | the expected score at a rating difference $x$ | – | – |
| $\pi_t$ | the true expected score of game $t$ | – | `play` (`e_true`) |
| $v$ | the conditional variance of a game's result $s_t \in \{0, \tfrac12, 1\}$ | – | – |
| $a = 1 - \varphi$ | the reversion a game; $\varphi$ the share of error kept | – | `Memory::of` (`a`, `phi`) |
| $\sigma^2$ | the stationary variance of $e_t$ | points$^2$ | `Memory::of` (`sigma2`) |
| $\hat a, \hat\varphi, \hat\sigma^2$ | the plug-in values (E2) | – | `Memory::of` |
| $W = \min(30, \lfloor n/2 \rfloor)$ | the window | games | `TRAJECTORY_WINDOW`, `compute_with` (`size`) |
| $A = [a_0, a_1)$, $B = [b_0, b_1)$ | "then" and "now", $b_0 \ge a_1$; $m_A$, $m_B$ their lengths | games | `compute_with` |
| $\bar r_A$, $\bar r_B$; $D = \bar r_B - \bar r_A$ | window means; the difference | points | `memory_compare` (`diff`) |
| $S(m)$, $C$ | the block sum and the cross term (E3), (E4) | – | `Memory::block`, `diff_var`; `analysis.py` `within`, `model_var` |
| $V$ | $\operatorname{Var}(D)$ under the model (E5) | points$^2$ | `Memory::diff_var` |
| $z_{0.80} = 1.2816$ | $\Phi^{-1}(0.9)$ | – | `Z80` |
| $z_\star = 2.4977$ | $\Phi^{-1}(1 - 0.0125/2)$, the decision level $1 - 0.025/2$ | – | `DECISION_LEVEL` |
| $N$ | players in a cell: 2,000 settled, 300 a decline cell, 1,000 rough or chess.com-like | players | `SETTLED_PLAYERS`, `DECLINE_PLAYERS`, `ROUGH_PLAYERS`, `GLICKO1_PLAYERS` |
| $k$ | players in a cell told the side | players | `results.json` `candidates.*.primary[].k` |
| $\ell(k), u(k)$ | the Wilson interval's ends | share | `analysis.py` `wilson` |
| $p_f$, $p_r$ | a cell's true falling and rising rates | share | – |
| $z = D/\sqrt{V}$ | the cohort's standardised difference | – | `cohort_reads` |

The population is the simulated players of `analysis.py`, one series each. The cohort is the `8` slice of EXP-0009's anonymised Lichess players (ADR 0114), one month a pseudonym.

## 2. Model

**(M1) The Elo-type update.** For a player of constant strength $\theta$,

```math
r_{t+1} = r_t + K\,(s_t - E(r_t - o_t)),
\qquad E[s_t \mid \mathcal F_t] = \pi_t, \qquad \operatorname{Var}(s_t \mid \mathcal F_t) = v_t,
```

with $\mathcal F_t$ the history up to the pairing of game $t$. In the plain world, $\pi_t = E(\theta - o^\ast_t)$, where $o_t \sim N(r_t, 75)$ and $o^\ast_t \sim N(o_t, 50)$ is the opponent's strength (`play`). Draws have probability $4 \times 0.06\,\pi(1 - \pi)$, so $v_t = \pi_t(1 - \pi_t) - 0.06\,\pi_t(1 - \pi_t)$. Glicko-2 at lila's floor deviation (45) is of this form with an effective $K = 173.72\, g\, \phi'^2 \approx 11.4$ points (lila's constants, the starting volatility, $E = \tfrac12$; check `N (M1)`). The chess.com-like Glicko-1 world and the rough world depart from it, and those are the guards (section 6).

**(M2) The memory model** (preregistration section 2, `Memory`). The error is a stationary AR(1):

```math
e_{t+1} = \varphi\, e_t + \varepsilon_t, \qquad \varphi = 1 - a, \ 0 < a < 1,
\qquad E[\varepsilon_t \mid \mathcal F_t] = 0, \ E[\varepsilon_t^2] = \sigma_\varepsilon^2 = K^2 v,
```

with $a = qK\,p(1-p)$, the $\varepsilon_t$ uncorrelated, and $e_0$ drawn from the stationary law. Result 1 derives (M2) from (M1) to first order. For the error control the results use (M2G): (M2) with Gaussian $\varepsilon_t$.

**(M3) The plug-in assumption** (preregistration section 2). $p(1 - p) = v = \tfrac14$ and $s^2 = K^2 v$. Result 4 shows that the second part holds only to $O(a)$, and section 6 says what the first costs.

**(M4) Counts.** Players are independent. In a cell of $N$ players, the falling and rising counts at one length are $\operatorname{Multinomial}(N; p_f, p_r, 1 - p_f - p_r)$. A player cannot be told both. The three lengths are read from the same players, so they are dependent in an unknown way. `power.py` draws them independently.

**(M5) The cohort.** For a real player-month, $D = D_0 + \Delta$. Here $D_0$ is the rating noise of (M2), and $\Delta$ is the real change of the player's strength between the two windows, independent of $D_0$. So $z = Z_0 + \delta$ with $Z_0 = D_0/\sqrt V$ and $\delta = \Delta/\sqrt V$.

## 3. Estimators

- **(E1) The windows** (`compute_with`). `memory-halves`: $A = [0, W)$, $B = [n - W, n)$. `memory-before`: $A = [0, n - W)$, the same $B$. The champion `peak-bootstrap`: $B$ as above. $A$ has length $W$ and starts at $\max(0, \min(i^\ast - \lfloor W/2 \rfloor, n - 2W))$ when the peak $i^\ast$ lies before $B$, and at 0 otherwise.
- **(E2) The plug-in** (`Memory::of`; `analysis.py` `model_var`): $\hat a = \min(qs/2, 1)$, $\hat\varphi = \min(1 - \hat a, 1 - 10^{-12})$, $\hat\sigma^2 = s^2 / (\hat a (2 - \hat a))$. No verdict when $s = 0$.
- **(E3) The block sum** (`Memory::block`, `within`):

```math
S(m) = \frac{m (1 + \varphi)}{1 - \varphi} - \frac{2 \varphi (1 - \varphi^m)}{(1 - \varphi)^2}.
```

- **(E4) The cross term** (`Memory::diff_var`, `model_var`): $C = \varphi^{\,b_0 - a_1 + 1} (1 - \varphi^{m_A})(1 - \varphi^{m_B}) / (1 - \varphi)^2$.
- **(E5) The variance of the difference:**

```math
V = \sigma^2 \left( \frac{S(m_A)}{m_A^2} + \frac{S(m_B)}{m_B^2} - \frac{2C}{m_A m_B} \right),
```

evaluated at $(\hat\sigma^2, \hat\varphi)$. Each window's own interval is $\bar r \pm z_{0.80}\, \hat\sigma \sqrt{S(m)}/m$.
- **(E6) The verdict** (`memory_compare`): `falling` when $D + z_{0.80}\sqrt{\hat V} < 0$, `rising` when $D - z_{0.80}\sqrt{\hat V} > 0$.
- **(E7) Wilson's interval** (`analysis.py` `wilson`, at level $\gamma$ with $z = \Phi^{-1}(\tfrac12 + \tfrac\gamma2)$):

```math
\ell, u = \frac{\hat p + \frac{z^2}{2N} \mp z \sqrt{\frac{\hat p (1 - \hat p)}{N} + \frac{z^2}{4N^2}}}{1 + \frac{z^2}{N}}, \qquad \hat p = k/N.
```

  A co-primary cell is "within the bound" when $u \le 0.15$ at $\gamma = 1 - 0.025/2$ (`classify`).
- **(E8) The arena rule** (`lstats.newcombe_paired`): the paired change in false `falling`, candidate minus champion, with Newcombe's method 10 at $1 - 0.05/3$. It is "better" when the upper end is below 0.
- **(E9) The guards.** Detection: $\ell \ge 0.50$ at 80%, a cell of 300. A world (rough, chess.com-like): every one of its six cells has $\hat p \le 0.15$ and $u_{80} \le 0.20$ (`guarded`). Transfer: the lower 95% end of the cohort's two-sided share $|z| > z_{0.80}$ is at most 0.40.
- **(E10) The cohort's $z$** (`cohort_reads`): $D$ is the mean pre-game rating of the month's last five rapid games minus that of its first five. $s$ is the RMS of those ten games' own steps (after minus before), and $V$ is (E5) with $A = [0, 5)$, $B = [n_g - 5, n_g)$, $n_g$ the month's rapid games.
- **(E11) Lag-1** (`lag1`): the Pearson correlation of the pooled pairs $(x_t, x_{t+1})$ within the five-step blocks, centred on the pooled means.
- **(E12) Power** (`power.py`): the probability that all six cells are within, with each length's counts drawn as in (M4) and the lengths independent, from 20,000 draws.

## 4. Derivations and proofs

**Result 1 (the update is an AR(1) to first order).** Under (M1) with constant $\theta$, $e_{t+1} = (1 - a_t) e_t + K\varepsilon_t + R_t$. Here $a_t = qK\,E(x_t)(1 - E(x_t))$ at $x_t = r_t - o_t$, $\varepsilon_t = s_t - \pi_t$ is a martingale difference with conditional variance $v_t$, and the remainder is bounded by $|R_t| \le K q^2 e_t^2 / (12\sqrt3)$, up to the opponent's own error. With $p(1-p)$ replaced by its mean, this is (M2).

*Proof.* Write $s_t - E(r_t - o_t) = (s_t - \pi_t) + (\pi_t - E(r_t - o_t))$. Take the opponent's strength at its rating for the moment, so that $\pi_t = E(\theta - o_t) = E(x_t - e_t)$. Taylor's theorem with the Lagrange remainder gives $E(x_t - e_t) = E(x_t) - E'(x_t) e_t + \tfrac12 E''(\xi) e_t^2$ for some $\xi$ between $x_t - e_t$ and $x_t$. Differentiating, $E'(x) = q E(1 - E)$ and $E''(x) = q^2 E(1 - E)(1 - 2E)$. On $[0, 1]$, $|u(1-u)(1-2u)|$ has its maximum $1/(6\sqrt3)$ at $u = \tfrac12 \pm \tfrac{1}{2\sqrt3}$: the derivative $1 - 6u + 6u^2$ vanishes there, and the value is $\tfrac16 \cdot \tfrac{1}{\sqrt3}$. So $|\tfrac12 E''(\xi) e_t^2| \le q^2 e_t^2/(12\sqrt3)$. Then

```math
e_{t+1} = e_t + K(s_t - \pi_t) - K q E(x_t)(1 - E(x_t))\, e_t + K \cdot \tfrac12 E''(\xi) e_t^2
        = (1 - a_t) e_t + K \varepsilon_t + R_t .
```

Next, $E[\varepsilon_t \mid \mathcal F_t] = \pi_t - \pi_t = 0$ and $E[\varepsilon_t^2 \mid \mathcal F_t] = v_t$, by (M1). In the plain world the opponent's strength differs from its rating by $\eta_t \sim N(0, 50)$, independent of the past. Linearised, it adds a further innovation $-KqE(1-E)\eta_t$ of variance $(qK\,p(1-p) \cdot 50)^2 \approx 0.64$ points$^2$, about 2% of $K^2 v$. It is still a martingale difference, so (M2) holds with $\sigma_\varepsilon^2 = K^2 v + 0.64$. Relative to the linear term, $|R_t| / (a_t |e_t|) \le q|e_t| / (12\sqrt3\, p(1-p))$, which is $0.0011\,|e_t|$ at $p(1 - p) = \tfrac14$: about 3% at $|e_t| = \sigma \approx 31$. $\blacksquare$

The linearisation is therefore approximate, with a relative error of about 3% at one $\sigma$. S2 measures it in the Elo world. The error SD is 31.34, against 30.52 from $K^2 v$ alone (2.7%, `S2 Result 2`) and 30.85 with the opponent's innovation (1.6%, `S2 Result 1`). The remaining 1.6% is the curvature remainder plus the variation of $a_t$. Checked by `R1` (symbolic: the slope, the curvature, its maximum).

**Result 2 (stationary variance and autocovariance).** Under (M2), $\sigma^2 = K^2 v/(1 - \varphi^2) = K^2 v/(a(2 - a))$ and $\operatorname{Cov}(e_t, e_{t+h}) = \sigma^2 \varphi^{|h|}$.

*Proof.* $e_t$ is $\mathcal F_t$-measurable and $E[\varepsilon_t \mid \mathcal F_t] = 0$, so $E[e_t \varepsilon_t] = 0$. Squaring $e_{t+1} = \varphi e_t + \varepsilon_t$ and taking expectations gives $E e_{t+1}^2 = \varphi^2 E e_t^2 + \sigma_\varepsilon^2$. Stationarity sets both second moments to $\sigma^2$, so $\sigma^2 (1 - \varphi^2) = K^2 v$. Then $1 - \varphi^2 = (1 - \varphi)(1 + \varphi) = a(2 - a)$, and $0 < a < 1$ makes it positive. For $h \ge 1$, iterating gives $e_{t+h} = \varphi^h e_t + \sum_{j=0}^{h-1} \varphi^{h-1-j} \varepsilon_{t+j}$. Each $\varepsilon_{t+j}$ is uncorrelated with $e_t$, because $E[e_t \varepsilon_{t+j}] = E[e_t E[\varepsilon_{t+j} \mid \mathcal F_{t+j}]] = 0$. So $\operatorname{Cov}(e_t, e_{t+h}) = \varphi^h \sigma^2$, and the case $h < 0$ follows by symmetry. The mean is zero, since $E e_{t+1} = \varphi E e_t$ and $\varphi < 1$. $\blacksquare$ Checked by `R2` (symbolic) and `S1` (the variances of window differences, which depend on the whole autocovariance).

**Result 3 (the step's second moment).** Under (M2), $E[d_t^2] = a^2 \sigma^2 + K^2 v = 2a\sigma^2 = K^2 v \cdot \frac{2}{2 - a}$.

*Proof.* With $\theta$ constant, $d_t = e_{t+1} - e_t = -a e_t + \varepsilon_t$. The cross term vanishes as in Result 2, so $E d_t^2 = a^2 \sigma^2 + K^2 v$. Substitute $K^2 v = a(2 - a)\sigma^2$ from Result 2 to get $a^2\sigma^2 + 2a\sigma^2 - a^2 \sigma^2 = 2a\sigma^2$. Substitute $\sigma^2 = K^2 v/(a(2-a))$ to get $2K^2 v/(2 - a)$. $\blacksquare$ Checked by `R3` and `S1 Result 3` (30.80 against 30.75).

**Result 4 (what the plug-in estimates).** Assume (M2) and $p(1-p) = v = \tfrac14$, so $a = qK/4$. Assume also that $s^2$ converges to $E d^2$, which it does by the ergodic theorem for the stationary AR(1). Then

```math
\frac{\hat a}{a} = \rho := \sqrt{\frac{2}{2 - a}}, \qquad \frac{\hat\sigma^2}{\sigma^2} = \frac{2}{\rho\,(2 - a\rho)},
```

and both equal $1 + a/4 + O(a^2)$. More precisely, on $0 < a \le 0.1$ both lie in $[1 + a/4,\ 1 + a/4 + a^2/2]$. At lila's floor ($a \approx 0.0167$) both are 0.42% high. The registration's equality "$s = K\sqrt{\operatorname{var}(s)}$" is therefore true only to this order. Also, $s^2/(\hat a(2 - \hat a)) = s/(q(1 - \hat a/2))$ whenever $\hat a = qs/2 < 1$, which holds if and only if $s < 2/q = 800/\ln 10 = 347.4$ points.

*Proof.* By Result 3 with $v = \tfrac14$, $s = \sqrt{E d^2} = \tfrac K2 \rho$. So $\hat a = qs/2 = \tfrac{qK}{4}\rho = a\rho$. Then $\hat\sigma^2 = s^2/(\hat a(2 - \hat a)) = \tfrac{K^2}{4}\rho^2 / (a\rho(2 - a\rho))$, while $\sigma^2 = \tfrac{K^2}{4}/(a(2 - a))$. Their ratio is $\rho(2 - a)/(2 - a\rho)$, and $\rho^2 (2 - a) = 2$ gives $\rho(2 - a) = 2/\rho$. For the expansions, $\rho = (1 - a/2)^{-1/2} = 1 + a/4 + \tfrac{3}{32}a^2 + \dots$. The second ratio is $2\rho^{-1}(2 - a\rho)^{-1}$, which expands to $1 + a/4 + O(a^2)$ (sympy, `R4`). The bounds on $(0, 0.1]$ are checked on a grid of 10,001 points. For the alternative form, put $\hat a = qs/2$: then $s^2/(\hat a(2 - \hat a)) = s^2/(\tfrac{qs}{2}(2 - \hat a)) = s/(q(1 - \hat a/2))$. Finally, $qs/2 < 1 \iff s < 2/q$. $\blacksquare$ Checked by `R4`. Section 6 adds the cost of $p(1-p) \neq v \neq \tfrac14$.

**Result 5 (the block sum).** $\sum_{i, j = 0}^{m-1} \varphi^{|i - j|} = S(m)$ of (E3), for every $m \ge 1$ and $0 \le \varphi < 1$, and $S(m) \to m$ as $\varphi \to 0$, $S(m) \to m^2$ as $\varphi \to 1$.

*Proof.* There are $m$ pairs at distance 0 and $2(m - k)$ at distance $k$, for $1 \le k \le m - 1$. So the sum is $m + 2\sum_{k=1}^{m-1}(m - k)\varphi^k$. Each $\varphi^k$ appears once for every $j$ with $k \le j \le m - 1$, that is $m - k$ times, so $\sum_{k=1}^{m-1}(m - k)\varphi^k = \sum_{j=1}^{m-1}\sum_{k=1}^{j}\varphi^k = \sum_{j=1}^{m-1} \varphi(1 - \varphi^j)/(1 - \varphi)$. Summing the geometric series in $j$ gives

```math
\sum_{k=1}^{m-1}(m - k)\varphi^k = \frac{\varphi}{1 - \varphi}\Big((m - 1) - \frac{\varphi(1 - \varphi^{m-1})}{1 - \varphi}\Big)
 = \frac{m\varphi}{1 - \varphi} - \frac{\varphi(1 - \varphi^m)}{(1 - \varphi)^2},
```

where the last step uses $\varphi(1 - \varphi) + \varphi^2(1 - \varphi^{m-1}) = \varphi(1 - \varphi^m)$. Hence the sum is $m + \tfrac{2m\varphi}{1 - \varphi} - \tfrac{2\varphi(1 - \varphi^m)}{(1 - \varphi)^2}$, and $m + \tfrac{2m\varphi}{1-\varphi} = \tfrac{m(1+\varphi)}{1-\varphi}$. The limits follow from the pair count: every term tends to $0^{|i-j|}$, or to 1. $\blacksquare$ Checked by `R5`: the closed form against the double sum, exactly in $\varphi$ for $m = 1$ to 40; against sympy's symbolic summation; and the two limits. The Rust `block` and the Python `within` are this formula term by term.

**Result 6 (the cross term).** For $A = [a_0, a_1)$, $B = [b_0, b_1)$ with $b_0 \ge a_1$, $\sum_{i \in A}\sum_{j \in B}\varphi^{j - i} = C$ of (E4). The smallest lag is $b_0 - (a_1 - 1) = b_0 - a_1 + 1$.

*Proof.* $j - i = (j - b_0) + (b_0 - a_1 + 1) + (a_1 - 1 - i)$, with $j - b_0 \in \{0, \dots, m_B - 1\}$ and $a_1 - 1 - i \in \{0, \dots, m_A - 1\}$. The double sum factorises as $\varphi^{b_0 - a_1 + 1} \sum_{u=0}^{m_A - 1}\varphi^u \sum_{w=0}^{m_B - 1}\varphi^w$, and each geometric sum is $(1 - \varphi^m)/(1 - \varphi)$. $\blacksquare$ Checked by `R6`, exactly, on three layouts, including adjacent windows ($b_0 = a_1$, lag 1). The Rust (`b.start + 1 - a.end`) and the Python (`b0 - a1 + 1`) use this exponent.

**Remark (the luck-share lemma).** `lab/lean/Rematch/LuckShare.lean` theorem `Rematch.luck_share` proves bounds on two luck shares of EXP-0009 and EXP-0029: $\varphi^g - 1 \in (-1, 0]$, and $\varphi^{g_A}(\varphi^{2g_J} - 1) \in (-1, 0]$, with its absolute value antitone in $g_A$ and zero iff $g_J = 0$. EXP-0030 uses the same memory $\varphi^h$, but as the autocovariance of a stationary AR(1) and its sums (Results 2, 5, 6). No step here needs a luck share or its bounds. The only shared fact is $0 < \varphi^h \le 1$, which is Mathlib's `pow_pos` and `pow_le_one₀`, not a Rematch theorem. So `luck_share` is not cited as covering EXP-0030. A Lean proof of Results 5 and 6 would be a new lemma.

**Result 7 (the variance of the difference, and the interval's error rates).** Under (M2), $\operatorname{Var}(\bar r_B - \bar r_A) = V$ of (E5). Under (M2G), with $(\sigma^2, \varphi)$ known, $D/\sqrt V \sim N(0, 1)$. So each of `falling` and `rising` in (E6) has probability exactly $\Phi(-z_{0.80}) = 0.10$, and the 80% interval of the difference covers 0 with probability 0.80.

*Proof.* $\theta$ cancels: $D = \bar e_B - \bar e_A = w^\top e$ with $w_i = -1/m_A$ on $A$, $1/m_B$ on $B$ and 0 elsewhere. Then $\operatorname{Var}(D) = w^\top \Gamma w$ with $\Gamma_{ij} = \sigma^2\varphi^{|i - j|}$ (Result 2):

```math
w^\top \Gamma w = \frac{\sigma^2}{m_A^2}\sum_{i, i' \in A}\varphi^{|i - i'|} + \frac{\sigma^2}{m_B^2}\sum_{j, j' \in B}\varphi^{|j - j'|} - \frac{2\sigma^2}{m_A m_B}\sum_{i \in A}\sum_{j \in B}\varphi^{j - i},
```

which is (E5) by Results 5 and 6. Under (M2G), $e$ is a Gaussian vector: $e_0$ is Gaussian, and each $e_t$ is a linear combination of $e_0$ and independent Gaussian innovations. So $D$ is Gaussian with mean 0 and variance $V$, and $P(D < -z_{0.80}\sqrt V) = \Phi(-z_{0.80}) = 0.10$, and likewise for the other side. $V \ge 0$ because $\Gamma$ is a covariance matrix. The Rust's `v.max(0.0)` only guards rounding. $\blacksquare$ Checked by `R7` (the closed form equals $w^\top\Gamma w$ exactly, in $\varphi$) and `S1`: 20,000 Gaussian AR(1) players with $\varphi = 0.984$, $\sigma = 31$. The empirical $\operatorname{Var}(D)$ matches $V$ within 4 SE at all six rule-length pairs, and the shares are 0.10 within 4 SE.

**Result 8 (the variogram form, and how much $\hat a$ matters).** With $\sum_i w_i = 0$, $V = -\tfrac12 w^\top \mathcal V w$, where $\mathcal V_{ij} = 2\sigma^2(1 - \varphi^{|i-j|})$ is the variogram $E(e_i - e_j)^2$. At the plug-in, $\hat\sigma^2(1 - \hat\varphi^h) \to s^2 h/2$ as $\hat a \to 0$. So for lags $h \ll 1/\hat a$ the variance depends on $s$ alone, like a random walk with step variance $s^2$, and $\hat a$ matters through the lags comparable to $1/\hat a \approx 64$ games. At fixed $s = 5.45$, the elasticity $\partial \ln \hat V / \partial \ln \hat a$ of `memory-halves` is $-0.31$, $-0.64$ and $-1.01$ at 60, 120 and 250 games.

*Proof.* $\mathcal V_{ij} = 2\sigma^2 - 2\Gamma_{ij}$, so $w^\top \mathcal V w = 2\sigma^2 (\sum_i w_i)^2 - 2 w^\top \Gamma w = -2V$. At the plug-in, $\hat\sigma^2(1 - \hat\varphi^h) = s^2 \frac{1 - (1 - \hat a)^h}{\hat a(2 - \hat a)}$. By l'Hôpital in $\hat a$ (or the binomial expansion $1 - (1 - \hat a)^h = h\hat a + O(\hat a^2)$), the fraction tends to $h/2$. The elasticities are numerical derivatives of (E5), by central differences at $\pm 0.1\%$. $\blacksquare$ Checked by `R7` (the variogram identity, exactly), `R8` (the limit) and `S2 Result 8` (the elasticities). A relative error $\epsilon$ in $\hat a$ at fixed $s$ therefore moves the half-width by about $\tfrac12 \times$ elasticity $\times\, \epsilon$. That is at most about $\epsilon/2$, reached at 250 games. Section 6 puts $\epsilon$ between $-2.6\%$ and $+0.4\%$, so the half-width moves by at most about 1.3%.

**Result 9 (the windows: where the rules coincide).** The candidates' windows (E1) are identical if and only if $n$ is even and $n \le 60$. At $n = 2W$, the champion's "then" is also $[0, W)$ (from reading `compute_with`). So at 60 games, and at 30, all three rules compare the same windows, and the two candidates give the same verdict for every player. The champion differs only in its interval. The gap exponent of `memory-halves` is $n - 2W + 1$: 1, 61 and 191 at 60, 120 and 250 games.

*Proof.* `memory-before`'s "then" has length $n - W$, and `memory-halves`' has length $W$. Both start at 0 and share $B$, so they are equal iff $n - W = W$. If $n \ge 60$, $W = 30$ and this means $n = 60$. If $n < 60$, $W = \lfloor n/2 \rfloor$ and $n = 2\lfloor n/2\rfloor$ iff $n$ is even. For the champion at $n = 2W$, $n - 2W = 0$, so $\max(0, \min(\cdot, 0)) = 0$, whatever the peak. $\blacksquare$ Checked by `R9` (all $n$ from 2 to 400) and `N Result 9`. The champion's part is a reading of the code, so its evidence is in the data: the committed player-by-player flags are equal at 60 games, and so are the counts at 30. All three rules also have the same `diff_mean` at 60 games, $-1.257$.

**Result 10 (the champion's bootstrap is too narrow by a factor of about 13).** Under (M2), the plug-in variance of a $W$-game window, $\tfrac1W\sum(r_i - \bar r)^2$, has expectation $\sigma^2(W - S(W)/W)/W$. The i.i.d. bootstrap therefore estimates the variance of the window mean with expectation $\sigma^2(W^2 - S(W))/W^3$, against the true $\sigma^2 S(W)/W^2$. Their ratio is

```math
\frac{W^2 - S(W)}{W\,S(W)} = 0.0055 \quad (W = 30,\ \hat\varphi = 0.9843),
```

so its standard error is $1/\sqrt{0.0055} = 13.5$ times too small. The predicted 80% half-width of the champion's difference is $z_{0.80}\sqrt{2\sigma^2(W^2 - S)/W^3} = 3.84$ points, against a measured 3.66 to 3.70.

*Proof.* $\sum_i (r_i - \bar r)^2 = e^\top H e$ with the centring matrix $H = I - \tfrac1W \mathbf 1\mathbf 1^\top$, which is idempotent. So $E[e^\top H e] = \operatorname{tr}(H\Gamma) = \operatorname{tr}\Gamma - \tfrac1W \mathbf 1^\top\Gamma\mathbf 1 = \sigma^2(W - S(W)/W)$ by Result 5. A resampled mean of $W$ draws with replacement has variance $\frac{1}{W}\cdot\frac1W\sum(r_i - \bar r)^2$ given the window. The ratio follows by dividing by $\sigma^2 S(W)/W^2$. For the difference, the two windows are resampled independently (`compare_means`), so the bootstrap variances add. The half-width uses $\sqrt{E[\cdot]}$ for $E\sqrt{\cdot}$ and a normal quantile for the 2,000-resample percentile, so it is approximate. By Jensen's inequality, $\sqrt{E[\cdot]}$ overstates the mean half-width. At 120 and 250 games the champion's "then" is also centred on the peak, which changes the spread inside it. The measured gap is 4% to 5% at every length, including 60 games, where there is no peak window. It is not separated into these causes. $\blacksquare$ Checked by `R10` ($\operatorname{tr}(H\Gamma)$ exactly for $m = 5, 8$), `S1 Result 10` (simulation) and `N Results 7, 10`. This is the registration's "about 4 points each side": its numbers come out of the model.

A related fact the copy rule of section 9 relies on: "peaked at $X$ and is $Y$ now" has $Y - X \le 0$ for every series. The peak is $\max(\max_t r_t, \text{current})$ (`compute_with`, `peak`), which is at least the current rating. Equality needs the current rating to be the peak. This holds deterministically, so it needs no check.

**Result 11 (the plug-in rule in the Elo world, approximately).** Take the Elo world of (M1) with $K = 11.6$, the plain world's pairing and draws, integer-rounded stored ratings and 300 burn-in games. Here $p(1 - p) = 0.239$ and $v = 0.220$, not $\tfrac14$. The rule (E1) to (E6) still gives falling and rising shares within 0.01 of 0.10, and half-widths within 1.3% of $z_{0.80}$ times the SD of $D$, at all six rule-length pairs.

*Proof.* Not a proof: the deviations of Results 1 and 4 and of $p(1-p), v \neq \tfrac14$ partly cancel, and the result measures what is left. The plug-in sets $\hat\sigma^2 \approx s^2/(2\hat a)$. With $s^2 \approx K^2 v$ and $\hat a = qs/2$, this gives $\hat\sigma^2 \approx K\sqrt v/q$, against the true $K^2 v/(2qK\,p(1-p))$. The ratio is $2p(1 - p)/\sqrt v = 1.02$, which is small. By Result 8, $V$ responds to $\hat a$ with an elasticity below 1. `S2` simulates 20,000 players and finds shares from 0.099 to 0.106 and half-width ratios from 0.987 to 1.004. $\blacksquare$ (approximate; the bound is measured, not proved). The committed plain world agrees: from its median step 5.45, (E2) to (E5) predict half-widths of 26.56, 43.85 and 50.93 (`memory-halves`) and 32.30 and 36.07 (`memory-before`). The measured means are 26.51, 43.79, 50.89, 32.25 and 36.03, and $\hat\sigma = 30.89$ against the measured error SD 30.94 (`N Results 7, 10`).

**Result 12 (the error control of the decision rule).** Under (M4), the probability that a candidate clears condition 1 of section 9 at one look, when some cell's true rate exceeds 0.15, is at most $\alpha_1 = P(\operatorname{Bin}(2000, 0.15) \le 260) = 0.00598$. This is the exact value. The normal approximation is the one-sided $0.0125/2 = 0.00625$. Over two candidates and at most two looks, the probability of clearing any candidate whose null is true is at most 0.024. Step (iv) states the condition on the follow-up's size.

*Proof.* (i) *The cut-off.* In (E7), $u(k)$ is strictly increasing in $k$. It is the larger root in $p$ of $(\hat p - p)^2 = z^2 p(1 - p)/N$, the score-test boundary, and that root moves right as $\hat p$ grows. The grid check `R12` confirms it on every $k$ for $N = 300$, 1,000 and 2,000. So "$u \le 0.15$" is equivalent to "$k \le c$", with $c = \max\{k : u(k) \le 0.15\} = 260$ at $N = 2000$ and $z_\star$ ($u(260) = 0.14994$, $u(261) = 0.15047$).

(ii) *The worst case.* A cell's count is $\operatorname{Bin}(2000, p)$ (a margin of (M4)). $P_p(K \le c)$ is non-increasing in $p$, by the coupling $K = \sum \mathbf 1[U_i < p]$ with uniform $U_i$. So over $p > 0.15$ its supremum is its value at 0.15, which is 0.005975.

(iii) *Intersection-union* (Berger 1982). The candidate clears only if all six cells are within. If cell $j$'s null is true, $P(\text{all within}) \le P(\text{cell } j \text{ within}) \le \alpha_1$, whatever the dependence among the cells. So the six need no correction among themselves.

(iv) *The family.* Let $A_{c,\ell}$ be the event that candidate $c$ clears at look $\ell$ though its null is true. By the union bound, $P(\bigcup A_{c,\ell}) \le \sum P(A_{c,\ell}) \le 2\alpha_1(2000) + 2\alpha_1(N_2)$, where $N_2$ is the follow-up's own size (section 9 gives it its own power analysis). The exact size $\alpha_1(N)$ is not monotone in $N$. Over $N = 1{,}000$ to 5,000 its supremum is 0.00616, at $N = 4{,}910$. So the family is at most 0.0243 for any follow-up in that range, and $4\alpha_1(2000) = 0.0239$ if the follow-up also uses 2,000. This needs no independence between the candidates, which share players, or between the looks. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.union_bound`, for step (iv). Its hypotheses are a finite family of events, each of probability at most $\varepsilon$, which matches here with four events and $\varepsilon = \max(\alpha_1(2000), \alpha_1(N_2))$. `Rematch.bonferroni` states the same for tests at $\alpha/m$. Steps (i) to (iii) are not in Lean. Checked by `R12`, `S5` (400,000 simulated runs with one cell at 0.15: the clear rate is 0.00597, at most $\alpha_1$) and `N Result 12`.

> **Finding 3 (the family-wise level is about 2.4%, not 5%).** Section 8 says "the 5% is split between the two candidates and the two looks", at $1 - 0.025/2$ a cell. But a cell is read by the upper end of a two-sided interval at that level, which is a one-sided test at $0.0125/2$. So each look spends about 0.006, and four looks spend at most 0.024 (Result 12 (iv)). The rule is conservative by a factor of about two against its own words. It does not change the decision: every cell cleared with room (the worst upper end is 0.136). A one-sided reading at the stated 5% would have used $z = \Phi^{-1}(1 - 0.0125) = 2.24$ and $c = 264$ at 2,000 players. Reported to the coordinator; no committed file is changed.

**Result 13 (power, exactly, and how much the independence of lengths matters).** Under (M4), at one length with $p_f = p_r = p$,

```math
P_1(p) = P(K_f \le c, K_r \le c) = \sum_{k=0}^{c} \binom{N}{k} p^k (1 - p)^{N - k}\, F_{\operatorname{Bin}(N - k,\ p/(1-p))}(c).
```

`power.py`'s independent lengths give $P_1^3$. Whatever the dependence between lengths, the all-six power lies in $[\max(0, 1 - 3(1 - P_1)),\ P_1]$. At $N = 2000$: $P_1^3 = 0.9862$ at $p = 0.11$ (bounds $[0.9861, 0.9954]$), 0.599 at 0.12 and 0.0143 at 0.13. Every point of `power.json` is within 4 Monte Carlo SE of $P_1^3$.

*Proof.* Given $K_f = k$, the other $N - k$ players are rising with probability $p/(1 - 2p + p) = p/(1-p)$, so $K_r \mid K_f = k \sim \operatorname{Bin}(N - k, p/(1-p))$. Summing over $k \le c$ gives $P_1$. For three events $E_g$, each of probability $P_1$: $P(\bigcap E_g) \le \min_g P(E_g) = P_1$, and $P(\bigcap E_g) = 1 - P(\bigcup E_g^c) \ge 1 - 3(1 - P_1)$ by the union bound (Fréchet). Independence gives $P_1^3$, which lies between them. $\blacksquare$ Checked by `N power.json` (largest $|z|$ 1.79 over 40 points) and `N Result 13`. At $p = 0.11$ the bounds are 0.001 apart, so section 6's "0.99" does not depend on the approximation. At 0.12 they span 0.53 to 0.84, so "0.60" does.

**Result 14 (the guards' operating characteristics).** The detection guard passes a length iff $k \ge 162$ of 300 (the smallest $k$ with $\ell_{80} \ge 0.50$). Its three-length pass rate is $P(\operatorname{Bin}(300, d) \ge 162)^3$: 0.955 at $d = 0.60$ and 0.285 at 0.55. A world guard passes a cell iff $k \le 150$ of 1,000 (here $\hat p \le 0.15$ binds before $u_{80} \le 0.20$). Its pass rate, by Result 13's formula with $c = 150$, is 0.988, 0.838, 0.0145 and 0 at true rates 0.12, 0.13, 0.15 and 0.18.

*Proof.* As Result 12 (i) for the monotone ends, and as Result 13, with the detection lengths independent: they are separate populations, so here the product is exact. For the world guard, the same players are read at three lengths, so the cube is the independent approximation of Result 13. $\blacksquare$ Checked by `N power.json` and `N section 6`.

**Result 15 (the cohort's two-sided share is an upper bound; its one-sided shares are not).** Under (M5) with $Z_0 \sim N(0,1)$ independent of $\delta$: $P(|z| > c) \ge P(|Z_0| > c) = 2\Phi(-c)$ for every $c > 0$ and every law of $\delta$, and $E z^2 = 1 + E\delta^2 \ge 1$. A one-sided share can fall below $\Phi(-c)$.

*Proof.* For fixed $\delta$, $g(\delta) = P(|Z_0 + \delta| > c) = \Phi(-c - \delta) + \Phi(-c + \delta)$. Its derivative is $\phi(c - \delta) - \phi(c + \delta)$, and $\phi(c - \delta)/\phi(c + \delta) = e^{2c\delta} \ge 1$ for $\delta \ge 0$. So $g$ is non-decreasing on $[0, \infty)$, even, and at its minimum $2\Phi(-c)$ when $\delta = 0$. Integrating over the independent $\delta$ keeps the inequality. Next, $E(Z_0 + \delta)^2 = E Z_0^2 + 2E Z_0\, E\delta + E\delta^2 = 1 + E\delta^2$. For the one-sided case, $P(Z_0 + \delta < -c) = \Phi(-c - \delta) < \Phi(-c)$ for every $\delta > 0$, so a cohort that mostly improves lowers the falling share. $\blacksquare$ Checked by `R15` (the derivative, the ratio, the moment) and `S3` (with $\delta \sim N(0.4, 0.8)$: two-sided 0.200 rises to 0.340, falling drops to 0.094). For a symmetric unimodal $Z_0$ the same holds by Anderson's theorem; for $\hat V$ estimated from ten steps, $Z_0$ is a scale mixture of normals and stays symmetric and unimodal when $s$ is independent of $D_0$, which is approximate. The cohort's 0.356 and mean $z^2$ of 2.37 are therefore readable as upper bounds on the model's false-alarm rate only under (M5)'s independence. Section 6 says when that fails.

**Result 16 (the lag-1 bias of per-block centring is $-1/4$; pooled centring removes it).** For i.i.d. steps of variance $\tau^2$ in blocks of $m = 5$, each centred on its own mean, $E[(x_t - \bar x)(x_{t'} - \bar x)] = -\tau^2/m$ for $t \neq t'$ and $\tau^2(m - 1)/m$ for $t = t'$. The ratio of expected sums over the $m - 1$ consecutive pairs is therefore $\frac{(m - 1)(-1/m)}{(m-1)(m-1)/m} = -\frac{1}{m - 1} = -\tfrac14$. It is $-1/m = -\tfrac15$ if the denominator takes all $m$ squares. Centring on the pooled mean of $2 \times 16{,}445$ blocks leaves a negligible bias, of order $1/(\text{pairs})$, provided the blocks share one mean step. If blocks have different mean steps $\mu_b$, as for players who really trend within a month, the pooled statistic gains about $+\operatorname{Var}(\mu_b)/(\operatorname{Var}(\mu_b) + \tau^2)$, a few thousandths here. So part of the cohort's 0.036 can be trend rather than streaks.

*Proof.* $\operatorname{Cov}(Hx) = \tau^2 H$, with $H = I - \tfrac1m\mathbf 1\mathbf 1^\top$, so the entries are $1 - 1/m$ on the diagonal and $-1/m$ off it. Summing over the pairs gives the ratios. With pooled centring the centre is an average over $N_b \gg 1$ blocks, so each centred cross-product has expectation $-\tau^2/(m N_b)$. $\blacksquare$ Checked by `R16` and `S4` (per-block $-0.2501$; pooled $-0.0004$). The registration's "about $-1/4$" is right for the pairwise Pearson form.

**Result 16b (the model's own step lag-1 is $-a/2$).** Under (M2), $\operatorname{Corr}(d_t, d_{t+1}) = -a/2$. At the plain world's median step this is $-0.0078$, against its measured $-0.0079$.

*Proof.* $d_t = -a e_t + \varepsilon_t$ and $d_{t+1} = -a(\varphi e_t + \varepsilon_t) + \varepsilon_{t+1}$. The cross terms are zero, as in Result 2, so $\operatorname{Cov}(d_t, d_{t+1}) = a^2\varphi\sigma^2 - a\sigma_\varepsilon^2 = a^2\sigma^2(1 - a) - a^2(2 - a)\sigma^2 = -a^2\sigma^2$. Divide by $\operatorname{Var} d_t = 2a\sigma^2$ (Result 3). $\blacksquare$ Checked by `R16b`, `S1 Result 16b` and `N Result 16b`. This is the baseline to read the real 0.036 against: a rating with no streaks has a slightly negative step correlation, so the real excess over the model is about 0.044.

**Result 17 (the share of false headlines).** If a share $\pi$ of players is falling and caught with probability $d$, and the others are told `falling` with probability $f$, the share of `falling` headlines that are false is $(1-\pi)f/((1-\pi)f + \pi d)$. It reaches $\tfrac12$ iff $f = \pi d/(1 - \pi)$, which is 0.20 at $\pi = 0.2$, $d = 0.8$: EXP-0015's hide line.

*Proof.* By Bayes' rule over the two kinds of player. Then $(1-\pi)f = \pi d$ solves the equality. $\blacksquare$ Checked by `R17` and `N Result 17`: the registration's 37% ($f = 0.10$, $d = 0.67$) and 47% ($f = 0.15$); the decision's 0.0824 against 0.1433, which is 36.5% and so "about 37%"; and 53% for 100-point falls.

**Result 18 (the bookkeeping).** The budget follows from the design. The simulated games are

```math
2000 \cdot 550 + 2 \cdot 300 (360 + 420 + 550) + 2 \cdot 1000 \cdot 550 + 300 \cdot 420 + 2 \cdot 500 \cdot 550 + 300 \cdot 250 + 2 \cdot 300 \cdot 550 = 4{,}079{,}000,
```

and the trajectories are $3 \times 21{,}800 = 65{,}400$. The beacon round is the first $r$ with $1692803367 + 3(r - 1) \ge 1795739570 + 600$: $r = 34312269$, at time 1795740171.

*Proof.* Each population plays burn-in plus observed games (`Runner.simulate`). Every request goes through three rules, and the settled players are read at four lengths, the others at one or three. For the round, solve $3(r - 1) \ge 102936803$, so $r - 1 = \lceil 34312267.67 \rceil$. $\blacksquare$ Checked by `N budget` and `N drand`.

> **Finding 1 (the lag-1 bias in section 2 is about 0.019, not 0.015, and with it the model predicts EXP-0015's 0.97 exactly).** Section 2 says EXP-0015's measured 0.97 is a lag-1 estimate from 250 games, "biased down by about 0.015". EXP-0015's estimator is the Pearson correlation of $(e_t, e_{t+1})$ within each series, averaged over series (its `analysis.py`, `np.corrcoef(row[:-1], row[1:])`). For a stationary AR(1) with $\varphi = 0.984$ and $n = 250$, its mean is 0.9655, a bias of $-0.0185$ (`S1 Finding 1`, 20,000 series, SE 0.0001). EXP-0015 measured 0.965505. So the registration's argument holds and is stronger than it says: the model at $\varphi = 0.984$ reproduces EXP-0015's number to four decimals. Only the size of the bias is off, by 0.0035. The 0.015 is close to the large-sample value $(1 + 3\varphi)/n = 0.016$, which is a poor approximation here because $n(1 - \varphi) \approx 4$. The blind re-derivation first put the bias at 0.024. It used the ratio form $\sum x_t x_{t+1} / \sum x_t^2$, which gives 0.960 (`S1 Finding 1`, second line). The referee asked for EXP-0015's own estimator, which settled it. The candidates do not use this number.

> **Finding 2 (section 2's $\sigma \approx 31$, $\varphi \approx 0.984$ at $K = 11.6$).** Under section 2's own assumption $p(1-p) = \operatorname{var}(s) = \tfrac14$, $K = 11.6$ gives $\hat a = 0.01669$, $\varphi = 0.9833$ and $\sigma = 31.9$, which round to 0.983 and 32. The quoted 31 and 0.984 match the plain world as measured ($\hat\varphi = 0.9843$ and $\hat\sigma = 30.89$ from the median step 5.45, against a measured error SD of 30.94), not the stated $K$. Prose only, and smaller than any interval.

> **Finding 4 (roundings and one description in decision.md and summary.md).** (a) The detection table quotes 0.99 [0.98, 0.99] at 250 games, but the 80% upper end is 0.9951, which rounds to 1.00. (b) `summary.md` gives `run_minutes: 15` for 858 s, which is 14.3 minutes. (c) `summary.md` says "17% at 250 games from a new account", but the worse cell, falling, is 0.1767, which rounds to 18%. Only the rising cell is 17%. (d) `decision.md` says the cohort's windows are "a median of 74 games apart". 74 is the median of the month's rapid games $n_g$ (`cohort.games_median`). The windows' starts are $n_g - 5$ apart, with $n_g - 10$ games between them, so about 69 and 64. None of these changes anything.

> **Finding 5 (the decision's "between its nominal rate and about twice it" is not a result).** `decision.md`, Transfer, says the rule's false-alarm rate on real players is "somewhere between its nominal rate and about twice it". Result 15 only gives an upper bound on the two-sided share at the cohort's five-game windows, under (M5). It says nothing about the shipped rule's 30-against-30 windows, and a one-sided share can sit on either side of nominal. "About twice" follows only from an extra assumption: that the model's variance is too small by a uniform factor $\kappa^2 = 2.37$ (the cohort's mean $z^2$) at every window and gap. Then a cell's rate is $\Phi(-1.2816/\kappa) = 0.203$ a side, 0.41 two-sided (`N Finding 5`), about twice 0.10. If instead the excess is real change (M5), the model's own rate is nearer nominal. If the model is wrong in some other way at 30-game windows, the rate can fall outside both. The sentence is a reasonable reading, not a bound. EXP-0032, which `decision.md` files for exactly this question, is where it gets settled. Reported to the coordinator.

## 5. Numbers

Every number `decision.md` and `summary.md` quote, recomputed by `math_check.py` `numbers` (rows `N ...`) from `results.json` and `power.json`, within the rounding quoted.

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| co-primary table, 12 candidate cells with intervals | (E7) at $1 - 0.025/2$ | `candidates.*.primary[]` `k`, `n`, `ci_decision` | e.g. 234/2000 = 0.117 [0.1002, 0.1362]; all 24 intervals to $10^{-6}$ |
| worst upper end 0.136 | (E7) | `memory-halves.primary` 120 falling | 0.136154 |
| every cell "within the bound" | (E7), Result 12 (i) | `read` | all $k \le 260$ |
| champion 0.458, 0.640, 0.795; 0.411, 0.294, 0.164; EXP-0015's 0.44 to 0.78 | share | `champion.falling[]`, `rising[]` | 0.4575, 0.64, 0.7945; 0.411, 0.2935, 0.164 |
| positive control above 0.5 | share | `diagnostics.positive_control_falling_250` | 0.7945 |
| half-widths 26.5, 43.8, 50.9 against 26.3, 44.6, 50.9; champion 3.7 | Results 7, 10 | `diagnostics.interval` | 26.51, 43.79, 50.89; 26.28, 44.56, 50.87; 3.66 to 3.70 |
| (implied) these half-widths are what the model predicts | Results 7, 11 | `diagnostics.steps.settled.step_rms_median` 5.4498 | 26.56, 43.85, 50.93, 32.30, 36.07; bootstrap 3.84 |
| 234 against 168; mean difference −2.3 | count | `primary` 120, `interval.memory-halves.120.diff_mean` | 234, 168, −2.285 |
| arena −0.35 [−0.38, −0.33], −0.52 [−0.55, −0.50], −0.69 [−0.71, −0.66] | (E8) from the flags | `flags`, `candidates.memory-halves.arena` | −0.3545 [−0.3799, −0.3287], −0.523 [−0.5490, −0.4955], −0.69 [−0.7135, −0.6640] |
| detection 0.72 [0.68, 0.75], 0.84 [0.81, 0.87], 0.99 [0.98, 0.99]; 0.36, 0.50, 0.72; before 0.72, 0.88, 0.94 | (E7) at 80% | `detection[]` | 0.7167 [0.682, 0.749], 0.84 [0.811, 0.865], 0.99 [0.980, **0.995**] (Finding 4); 0.363, 0.497, 0.723; 0.717, 0.877, 0.94 |
| 37% (0.08 against 0.14), 53% | Result 17 | `detection[]`, `false_alarm_same_length` | 0.365, 0.531 |
| rough 0.125, 0.131, 0.131; 0.137, 0.141, 0.144; largest 80% upper 0.159; 1.3 to 1.4 times; 13% to 14% | (E9) | `memory-halves.rough[]` | as quoted; 0.1588; 1.25 to 1.44 |
| chess.com-like 0.086 to 0.107 (9% to 11%) | (E9) | `memory-halves.chesscom[]` | 0.086 to 0.107 |
| cohort 0.356 [0.349, 0.363], 0.113, 0.243, $z^2$ 2.37 and 3.10, 16,445, "one and a half times" | (E7), (E10), Result 15 | `cohort.settled`, `settled_pooled_rms` | 0.35585 [0.34857, 0.36320], 0.1128, 0.24305, 2.374, 3.103; $\sqrt{2.374} = 1.54$ |
| passes the 0.40 line | (E9) | `cohort.settled.two_sided.ci_transfer[0]` | 0.3486 |
| lag-1 0.036 (0.029 signs); −0.008, 0.020, −0.026 | (E11), Result 16 | `cohort.step_lag1`, `diagnostics.steps.*.step_lag1` | 0.0363, 0.0292; −0.0079, 0.0200, −0.0263 |
| a median of 74 games | median | `cohort.games_median` | 74, the month's rapid games; the windows' starts are $n_g - 5$ apart (Finding 4) |
| 30 games: 0.094 and 0.090; champion 0.41 | share | `exploratory.games_30` | 0.094, 0.0895; 0.41, 0.4115 |
| accuracy and blunders 0.087 to 0.107 | share | `exploratory.then_now_rows.memory-halves` | 0.087 to 0.107 |
| rough decline 0.81 | share | `exploratory.rough_decline` | 0.8067 |
| mean change when falling −37, −62, −70 | mean | `exploratory.mean_change` | −37.17, −62.03, −70.14 |
| Elo arms 0.07 to 0.14 | share | `exploratory.other_ratings` | 0.0667 to 0.14 |
| new accounts 0.09 to 0.18, 0.177 and 0.167; before about 0.03; summary's 17% | share | `exploratory.new_account` | 0.0933 to 0.1767; 0.0267, 0.0333; 17.7% (Finding 4) |
| "one time in three"; Elo 0.14 at K = 10 rising 120 and K = 32 falling 250; 95% intervals about ±0.04 | share, (E7) | `detection`, `other_ratings` | 0.363; 0.14, 0.14; half-widths 0.029 to 0.039 |
| between nominal and about twice it | Finding 5 | `cohort.settled.mean_z2` | 0.203 a side under a uniform $\kappa^2 = 2.37$ |
| plain world lag-1 −0.008 (the model's baseline) | Result 16b | `diagnostics.steps.settled.step_lag1` | −0.0079 against $-a/2 = -0.0078$ |
| `memory-before` 0.141, 0.88, 0.11 | share | `memory-before.rough`, `detection`, `other_ratings` | 0.141, 0.8767, 0.1067 |
| slow form SD 0 or 20: 0.09 to 0.12 | share | `exploratory.rough_form` | 0.09 to 0.124 |
| burn-in +1.0, −3.3 to +4.3, +2.0, −13.7; RD 45 and 49.8; 218 days | mean | `diagnostics` | 1.049; −3.281 to 4.286; 2.018; −13.679; 45.0, 49.82; 217.5 |
| budget 4,079,000, 65,400, 3,955,242 | Result 18 | `compute.actual` | as quoted |
| 858 s, 0.24 CPU-hours, `run_minutes: 15` | arithmetic | prose (time is not in `results.json`) | 0.238 h; 14.3 min (Finding 4) |
| drand round 34312269, namespace | Result 18 | `namespace.txt`, `design.seed_namespace` | as quoted |
| power: 2,000 players; 0.99 at 0.11; 1,500: 0.89, 0.25 at 0.12 against 0.60; 1.00 and 0.92 at 0.10; 0.015 at 0.13 | Result 13 | `power.json` `primary` | 0.9864, 0.8888, 0.2506, 0.6008, 1.0, 0.9238, 0.015; exact 0.9862, 0.8895, 0.2472, 0.599, 1.0000, 0.9218, 0.0143 |
| detection guard 0.96 and 0.29; world guard 0.99, 0.84, 0.015, never | Result 14 | `power.json` | 0.9551, 0.2871; 0.9894, 0.839, 0.0148, 0 |
| value case 37% and 47% | Result 17 | prose inputs | 0.374, 0.472 |
| 347 points | Result 4 | – | 347.44 |
| σ ≈ 31, φ ≈ 0.984 at K ≈ 11.6 | Result 4 | prose | 31.9, 0.9833 (Finding 2) |

Not recomputed, because they are not in a committed file: the development run's numbers (0.166, 16,511 series, 857 s), the 0.37 of the feasibility run, EXP-0015's 0.44, 0.62, 0.78 and 98 s, and the wall-clock times.

## 6. Assumptions that can fail

- **The linearisation (M1) to (M2): Results 1, 2, 11.** The remainder is about 3% of the linear term at one $\sigma$ (Result 1). In the Elo world it moves $\sigma$ by 2.7%. That is a measured size, and the plain-world calibration (half-widths within 1% of the honest ones) shows it does not reach the verdict there.
- **$p(1-p) = v = \tfrac14$ and $s^2 = K^2 v$: Results 4, 8, 11.** Three departures. First, $s^2$ also carries the reversion, $+a/2$ relative (Result 4). Second, draws lower $v$ to $0.94\,p(1-p)$. Third, the pairing spread lowers $p(1-p)$ to 0.239. In $\hat a$ the relative error $\epsilon$ is $+0.4\%$ (the reversion alone), $-2.6\%$ (draws alone) and about $-0.6\%$ in the full plain world (S2: $\hat a = qs/2 = 0.0159$ against $qK\,p(1-p) = 0.0160$). In $\hat\sigma^2$ the errors run the other way, up to about $+3.6\%$. By Result 8 the half-width moves by at most about $|\epsilon|/2 \approx 1.3\%$. The registered guards on this are the Elo K = 10 and K = 32 arms (exploratory, 0.07 to 0.14 per cell) and the chess.com-like world (a guard, 0.086 to 0.107). A rating system whose step does not scale like $K(s - E)$ is not covered.
- **Stationarity and a settled rating: Results 2, 7.** The model takes $e_0$ from the stationary law and $a$ constant. New accounts break both: the deviation, and so $K$, shrinks over the first games. There `memory-halves` reaches 0.177 (exploratory). No confirmatory population covers it. EXP-0031 is filed for it.
- **No memory beyond the rating: Results 7, 15.** Form, streaks and tilt add correlated noise in the results, which (M2) leaves out. The rough world is the guard (0.125 to 0.144, a pass). The real step lag-1 of 0.036 is above the rough world's 0.020, so the guard is milder than real series (section 7 and the review say so).
- **Gaussian innovations: Result 7.** $\varepsilon_t$ is discrete (three results). $D$ averages 30 games of a slowly mixing series, so it is close to normal but not exactly. The Elo world (Result 11) carries the true discrete law, and its shares are within 0.01 of nominal.
- **The cohort's independence (M5): Result 15.** The two-sided share bounds the model only if real change is independent of the rating noise and does not enter $s$. Regression after a lucky run, selection on the month, or change that inflates the steps would break it. A pass at 0.40 bounds only gross failure, which is why the guard is set there. EXP-0032 is filed for the real-series test.
- **Independence of players: Results 12 to 14.** Players are separate simulated series with separate seeds, so this holds by construction. In the cohort, pseudonyms are counted once each.
- **The lengths' dependence: Result 13.** `power.py`'s independent lengths are exact only for the detection guard. For the co-primaries and the world guards, the Fréchet bounds of Result 13 bracket the truth. At the design point (0.11) they are 0.001 apart.
- **The asymptotic Wilson level and the follow-up's size: Result 12.** The exact size at 2,000 players, 0.00598, is below the nominal 0.00625. A follow-up of another size can exceed nominal slightly (0.00616 at most for 1,000 to 5,000 players; the nominal is 0.00625, and sizes outside that range are not checked). Finding 3 says the level is half the stated one.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py`, `power.py`, `power.json`, `results.json` and the Rust `trajectory.rs`, but not this file, `decision.md` or `summary.md`. It re-derived five results:

- the AR(1) from the Elo update, $\sigma^2 = K^2 v/(a(2 - a))$ and $E d^2 = 2a\sigma^2$ (Results 1 to 3);
- that the plug-in is biased by $\sqrt{2/(2 - a)}$ under (M3) (Result 4);
- $S(m)$, $C$ with the exponent $b_0 - a_1 + 1$, and $V$ (Results 5 to 7);
- the predicted half-widths and the champion's 3.84 (Results 10, 11);
- the cut-off $c = 260$, the exact size 0.00598, the intersection-union and union bound, and the exact power (Results 12 to 14);
- the two-sided bound via Anderson's theorem, and the lag-1 bias of $-1/4$ (Results 15, 16).

Every formula agreed with this file, and so did every number: 0.42% in Result 4; 26.56, 43.85, 50.93, 32.30 and 36.07; 3.84; 0.00598; 0.98615 at 0.11 and 0.59901 at 0.12; 162 of 300. Its plain-world $v$ (0.2256) differs from S2's 0.2195 because it averaged over the opponents only, not also over the player's own error. Both give a net $\hat\sigma^2$ error of about 1%. It raised Finding 1, which was not in the first draft, putting the lag-1 bias at 0.024. The referee then pointed to EXP-0015's actual estimator, which gives 0.0185 (Finding 1 as it stands). It found Findings 2 and 3 independently.

Two of `math_check.py`'s own checks failed on the first run, and the maths was corrected, not the tolerance widened:

- the elasticity at 250 games is $-1.01$, so Result 8's claimed range $(-1, 0)$ was wrong; it now states the values;
- the decision's "about 37%" is 36.5% from its own unrounded inputs, which rounds to 37%; a draft Finding that called it 36.4% (from the rounded 0.08 and 0.14) was dropped.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee. It re-derived Results 2 to 7, 9, 10, 13 and 15 to 18, the beacon round, the budget and Findings 2 to 4, and found them right. It made twelve points, all accepted:

1. **Major.** The family bound assumed the follow-up also has 2,000 players. Result 12 (iv) now gives $2\alpha_1(2000) + 2\alpha_1(N_2)$ with the supremum over 1,000 to 5,000 players, which is checked.
2. **Major.** The decision's "between its nominal rate and about twice it" was not covered. It is now Finding 5, with the assumption it needs checked.
3. **Minor.** The "74 games apart" description is in Finding 4 (d).
4. **Minor.** The summary's 17% is in Finding 4 (c), and checked.
5. **Minor.** The remark on `luck_share` now says that the share's absolute value is antitone, as the Lean statement does.
6. **Minor.** Result 1 now counts the opponent's strength error as a second innovation (2% of $K^2 v$). S2 separates it from the curvature remainder.
7. **Minor.** Result 8 and section 6 now state the error in $\hat a$ ($-2.6\%$ to $+0.4\%$), and its effect on the half-width (at most about 1.3%), consistently.
8. **Minor.** Several checks were loose or circular, and are fixed:
   - R15 now uses `sympy.stats` for $E(Z + D)^2$.
   - R9 no longer restates the champion's window formula; that part rests on the data.
   - S2 asserts the claimed 0.01 and 1.3%.
   - The elasticities and the 0.0055 and 13.5 are asserted to their quoted rounding.
   - S1 Result 3 lost its $\times 3$ slack. It uses the SE across players and also asserts that $E d^2$ differs from $K^2 v$.
9. **Minor.** Numbers that nothing asserted are now checked: Result 13's exact values and bounds, Result 14's cut-offs and rates, the $-1.257$, the effective $K$ (now 11.4, from the Glicko-2 formula, not "11.4 to 11.7"), "one time in three", the 0.14 Elo cells and the ±0.04.
10. **Minor.** Result 16 now says that pooled centring is unbiased only for blocks with one mean step. Result 16b derives the model's own step lag-1, $-a/2$, which matches the plain world's $-0.008$.
11. **Minor.** Result 10 no longer puts the 4% to 5% gap down to Jensen alone. It names both causes and says they are not separated.
12. **Minor.** Finding 1 assumed an estimator. With EXP-0015's own estimator, the bias is 0.0185, and the model reproduces EXP-0015's 0.9655. The Finding was rewritten.

After these changes `math_check.py` runs 95 checks, all PASS.
