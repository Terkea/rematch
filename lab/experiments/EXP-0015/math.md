# EXP-0015: the maths

I wrote this after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `power.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder. There are three kinds of check: symbolic or exhaustive (prefix `R`), simulation from a known truth (prefix `S`), and recomputation of every quoted number (prefix `N`).

EXP-0015 measures one production rule in a simulated world, so the maths has to do four things:

- pin down exactly what the rule computes (the windows, the bootstrap);
- show why it fails for a rating with memory, and by how much;
- turn the registration's words about the mechanism into theorems: the 60-game cell carries memory alone, and false "rising" equals false "falling" there;
- give the decision rule's error control and the power behind the 700 players.

The rule measured is `algo-4.0.1`: `backend/crates/profile/src/coaching/trajectory.rs` at `ef0789b9` (functions `compute`, `compare_means`, `bootstrap_mean`, `at`, `canonical`, `data_seed`, `pre_game_ratings`, `trajectory_input`), reached through `cli lab trajectory`.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $n$ | rated games read, $n \in \{30, 60, 120, 250\}$ | games | `analysis.py` `CHECKPOINTS`, `EXPLORE_GAMES` |
| $r_t$, $t = 0, \dots, n-1$ | the stored pre-game rating of game $t$, an integer | points | `analysis.py` `play` (`rating`), `trajectory.rs` `trajectory_input` |
| $\theta$ | the player's true strength, constant for the false-alarm populations | points | `analysis.py` `Runner.simulate` |
| $e_t = r_t - \theta$ | the rating's error | points | `analysis.py` `run_all` (`err`, from the unrounded `pre`) |
| $W = \min(30, \lfloor n/2 \rfloor)$ | the window | games | `TRAJECTORY_WINDOW`, `compute` (`size`) |
| $i^\ast$ | the index of the first maximum of $r_0, \dots, r_{n-1}$ | – | `compute` (`peak_idx`, strict `>`) |
| $A = [a_0, a_0 + W)$, $B = [n - W, n)$ | "then" and "now" | games | `compute` (`then_start`, `now_start`) |
| $\bar r_A, \bar r_B$; $D = \bar r_B - \bar r_A$ | window means and their difference | points | `compare_means` (`vb - va`); `results.json` `diagnostics.interval.*.diff_*` |
| $R = 2000$ | bootstrap resamples | – | `RESAMPLES` |
| $d^\ast_{(0)} \le \dots \le d^\ast_{(R-1)}$ | the sorted resampled differences | points | `compare_means` (`diffs`) |
| $[\ell^\ast, u^\ast] = [d^\ast_{(199)}, d^\ast_{(1800)}]$ | the rule's 80% interval: `at(q)` is $d^\ast_{(\lfloor qR \rfloor)}$, at $q$ = `tail` $= (1 - 0.8)/2$, which is $0.09999999999999998$ in f64, so $\lfloor qR \rfloor = 199$, and at $1 - q$, giving 1800 | points | `at`, `compare_means`, `CONFIDENCE` |
| $h = (u^\ast - \ell^\ast)/2$ | its half-width | points | `analysis.py` `Runner.trajectories` (`half_width`) |
| $\sigma^2$, $v_W$ | $\operatorname{Var}(e_t)$, and the variance of a $W$-game mean of $e$ | points$^2$ | `diagnostics.settled_error_sd`, `settled_window_mean_sd` |
| $\varphi$ | the AR(1) coefficient of the error (M3) | – | section 2 |
| $S(m)$ | $\sum_{i,j=0}^{m-1} \varphi^{\lvert i-j \rvert}$ | – | (E3) |
| $N$ | players in a cell: 700 settled, 200 a decline cell or other arm, 300 new accounts | players | `SETTLED_PLAYERS`, `DECLINE_PLAYERS`, `ELO_PLAYERS`, `GLICKO1_PLAYERS`, `NEW_PLAYERS` |
| $k$, $\hat p = k/N$ | players in a cell told the verdict, and the share | players | `results.json` `*.k`, `*.share` |
| $\ell(k), u(k)$ | the ends of Wilson's interval at level $\gamma$ | share | `analysis.py` `wilson`, `power.py` `wilson` |
| $z_\gamma = \Phi^{-1}(\tfrac12 + \tfrac\gamma2)$ | $z_{0.80} = 1.2816$, $z_{1-0.05/3} = 2.3940$ | – | `stats.norm.ppf` |
| $p_g$ | a cell's true false-alarm rate at $g$ games | share | – |
| $b$, $d$, $f$ | the share of players truly declining, the detection rate, the false-alarm rate | share | preregistration section 5 |

The population is the simulated players of `analysis.py`. Each has one series of burn-in plus observed games and one account id, so one production seed at every checkpoint.

## 2. Model

**(M1) The world** (`analysis.py` `play`, `glicko2`, `glicko1`, `strength`). This is the registered data-generating process, as written in section 7 of the preregistration:

- $\theta \sim N(1500, 300)$, clipped to $[700, 2300]$;
- a fresh account at 1500;
- the opponent's rating $o_t = r_t + N(0, 75)$, and their strength $o_t + N(0, 50)$;
- the result from the true expected score $\pi_t = 1/(1 + 10^{(o^\ast_t - \theta)/400})$, with draws of probability $4 \cdot 0.06\, \pi_t (1 - \pi_t)$;
- Glicko-2 with lila's constants;
- the stored rating $\lfloor r + \tfrac12 \rfloor$ of the pre-game rating.

The players are independent.

**(M2) The bootstrap's randomness.** `compare_means` seeds its generator from a hash of the sorted values (`data_seed`), and it indexes the sorted copy (`canonical`). The proofs model the resample indices as i.i.d. uniform on $\{0, \dots, W-1\}$, independent of the data. Under that model the bootstrap's law depends on the window's values only as a multiset. The hash is a deterministic stand-in for this randomness, and `S3` checks the consequences on the real percentile rule. Result 6's exact equality depends on this model, that is on the hash behaving like a random oracle.

**(M3) The memory model** (used in Results 3 to 7). The settled error is a stationary Gaussian AR(1):

```math
e_{t+1} = \varphi\, e_t + \varepsilon_t, \qquad \varepsilon_t \overset{\text{iid}}{\sim} N\big(0, \sigma^2 (1 - \varphi^2)\big),
\qquad \operatorname{Cov}(e_t, e_{t+h}) = \sigma^2 \varphi^{\lvert h \rvert},
```

with $\sigma = 30.74$ (the measured `settled_error_sd`) and $\varphi = 0.984$. EXP-0030's math.md (Results 1 and 2) derives this form from an Elo-type update to first order, with $1 - \varphi = qK\,\pi(1-\pi)$. At Glicko-2's floor deviation of 45, one step gives $K = 11.42$. Averaging $\pi(1-\pi)$ over the opponents' spread $N(0, \sqrt{75^2 + 50^2})$ gives 0.235, so $\varphi = 0.985$ (`N (M3)` derives both). The value 0.984 is not fitted to the false-alarm rates. It is checked against EXP-0015's own lag-1 statistic, the per-series Pearson correlation of $(e_t, e_{t+1})$ averaged over series. The model gives 0.96553, against the measured 0.965505, within 0.04 SE (`S1 (M3)`). Every other prediction in sections 4 and 5 is out of sample.

**(M4) Counts.** In a cell of $N$ players, $k \sim \operatorname{Bin}(N, p)$ exactly, since the players are i.i.d. under (M1). The three checkpoints are read from the same players, so $(k_{60}, k_{120}, k_{250})$ are dependent in a way the design does not fix. `power.py` draws them independently.

## 3. Estimators

- **(E1) The windows** (`compute`). $W = \min(30, \lfloor n/2 \rfloor)$ and "now" is $B = [n - W, n)$. If $i^\ast < n - W$ (basis `peak`), "then" starts at $a_0 = \max\big(0, \min(i^\ast - \lfloor W/2 \rfloor,\ n - 2W)\big)$; otherwise $a_0 = 0$ (basis `start`). The current rating is the last stored one (`trajectory_input`, `current`): the pre-game rating of the last game with `chesscom: false`, and the post-game rating with `chesscom: true`. In the second case, if the current rating exceeds every $r_t$, it becomes the peak and the basis is `start` (`compute`). The `change` is the current rating minus the peak with basis `peak`, and minus $\bar r_A$ with basis `start`.
- **(E2) The verdict** (`compare_means`, higher is better). Draw $R$ resamples $d^\ast_j = \bar r^\ast_{B,j} - \bar r^\ast_{A,j}$, each window resampled with replacement. The verdict is `falling` ("worse") when $u^\ast = d^\ast_{(1800)} < 0$, `rising` ("better") when $\ell^\ast = d^\ast_{(199)} > 0$, and `flat` otherwise. The accuracy and blunders rows use the same function on those values (`period_rows`), with blunders scored lower-is-better.
- **(E3) The normal stand-in** (used in the proofs and in `math_check.py` `rule`). $\hat s^2 = \hat\sigma^2_A / W + \hat\sigma^2_B / W$ with the plug-in $\hat\sigma^2_X = \frac1W \sum_{i \in X}(r_i - \bar r_X)^2$. Then `falling` is $D + z_{0.80}\hat s < 0$ and `rising` is $D - z_{0.80}\hat s > 0$. The block sum is

```math
S(m) = \frac{m (1 + \varphi)}{1 - \varphi} - \frac{2 \varphi (1 - \varphi^m)}{(1 - \varphi)^2}.
```

- **(E4) The calibration** (`analysis.py` `calibration`). `honest_half_width` $= 1.2815516 \times \operatorname{SD}(D)$ across players. `half_width_mean` is the mean of $h$.
- **(E5) Wilson's interval and the reads** (`analysis.py` `wilson`, `classify`, `decide`):

```math
\ell, u = \frac{\hat p + \frac{z^2}{2N} \mp z \sqrt{\frac{\hat p (1 - \hat p)}{N} + \frac{z^2}{4N^2}}}{1 + \frac{z^2}{N}}.
```

  At $\gamma = 1 - 0.05/3$, a cell reads "above the hide line" if $\ell > 0.20$, "above the bound" if $0.15 < \ell \le 0.20$, "within the bound" if $u \le 0.15$, and "undecided" otherwise. The decision is "hide" if any primary cell is above the hide line, "change" if any is above the bound, "keep" if all three are within, and "need more data" otherwise. The confirmatory secondaries use the same reads, at $1 - 0.05/3$ for rising and $1 - 0.05/6$ for the six accuracy and blunders cells.
- **(E6) Power** (`power.py`): the decision rule on $\operatorname{Bin}(N, p)$ counts, three independent cells, 20,000 draws a point.
- **(E7) The world's diagnostics** (`run_all`): `settled_error_sd` is the SD of all $700 \times 250$ errors; `settled_window_mean_sd[w]` is the SD of the means of non-overlapping $w$-game windows; `settled_error_lag1` is the mean per-series lag-1 Pearson correlation.

## 4. Derivations and proofs

**Result 1 (the windows).** For every $n \ge 20$ and every peak position:

- (i) $0 \le a_0$ and $a_0 + W \le n - W$, so "then" ends at or before "now" begins;
- (ii) the windows are $[0, W)$ and $[W, 2W)$ whatever the peak if and only if $n = 2W$, that is $n$ even and $n \le 60$; so at 30 and 60 games, and in the no-peak reads, which send 60 games, the peak cannot move them;
- (iii) with basis `peak`, the peak game lies inside "then";
- (iv) with basis `peak`, the `change` is at most 0, so "peaked at $X$ and is $Y$ now ($-N$)" never shows a positive $N$.

*Proof.* (i) If $i^\ast \ge n - W$, then $a_0 = 0$ and $W \le n - W$ because $W \le n/2$. Otherwise $a_0 \le n - 2W$ by the `min`, and $a_0 \ge 0$ by the `max`.

(ii) At $n = 2W$, $n - 2W = 0$, so $a_0 = \max(0, \min(\cdot, 0)) = 0$ in both branches. If $n > 2W$, a peak at $i^\ast = n - W - 1$ gives $a_0 = \min(n - W - 1 - \lfloor W/2 \rfloor, n - 2W) > 0$, so the window moves. For $n \le 61$, $n = 2W$ holds exactly for even $n$, and for $n > 61$, $W = 30 < n/2$.

(iii) With basis `peak`, $i^\ast < n - W$. There are three cases. If $a_0 = i^\ast - \lfloor W/2 \rfloor$, then $a_0 \le i^\ast < a_0 + W$. If the window is clamped at 0, then $i^\ast < \lfloor W/2 \rfloor < W$. If it is clamped at $n - 2W$, then $a_0 = n - 2W \le i^\ast < n - W = a_0 + W$.

(iv) `compute` sets the peak to $\max(c, \max_t r_t)$, where $c$ is the current rating: it takes the current rating instead whenever $c > \max_t r_t$, and then the basis is `start`. So with basis `peak` the peak is $\max_t r_t \ge c$, and the `change` $= c - \text{peak} \le 0$. With `chesscom: false`, as in the primary, $c = r_{n-1}$ is one of the $r_t$, so the basis is decided by $i^\ast$ alone, as (E1) says. $\blacksquare$

Checked by `R1`, exhaustively for every $n$ from 20 to 400 and every peak position. Part (iv) needs no data. It is why the mean `change` when falling is negative (−39, −50, −59): 287 of the 310 at 60 games have basis `peak`.

**Result 2 (what the bootstrap estimates).** Given the window, a resampled mean has variance $\hat\sigma^2_X / W$, with $\hat\sigma^2_X = \frac1W\sum(r_i - \bar r_X)^2$. For a window whose position does not depend on the ratings (the fixed halves of Result 1 (ii)), and a series whose errors have a common mean $\mu$ and variance $\sigma^2$ in it,

```math
E\big[\hat\sigma^2_X\big] = \sigma^2 - v_W, \qquad E\big[\operatorname{Var}^\ast(D^\ast)\big] = \frac{2(\sigma^2 - v_W)}{W},
```

against the true $\operatorname{Var}(D) = 2v_W - 2\operatorname{Cov}(\bar e_A, \bar e_B)$. The predicted mean half-width is approximately $z_{0.80}\sqrt{2(\sigma^2 - v_W)/W}$, and it overstates the mean of $h$ (Jensen). With the measured $\sigma = 30.743$, $v_{30}^{1/2} = 28.520$ and $v_{15}^{1/2} = 29.595$, the prediction is 3.80 points for $W = 30$, against the measured 3.69 (60 games, fixed halves), and 3.89 for $W = 15$, against 3.73 (30 games).

*Proof.* Resampling draws $W$ indices i.i.d. uniform (M2). Each draw has mean $\bar r_X$ and variance $\hat\sigma^2_X$ under the empirical law, so the mean of $W$ of them has variance $\hat\sigma^2_X/W$. $\theta$ cancels in deviations, so $\hat\sigma^2_X = \frac1W\sum(e_i - \bar e)^2 = \frac1W\sum e_i^2 - \bar e^2$. Taking expectations gives $(\sigma^2 + \mu^2) - (v_W + \mu^2) = \sigma^2 - v_W$. The two windows are resampled independently, so their bootstrap variances add, and each has the same expectation. The percentile interval is close to $\pm z_{0.80}$ times the bootstrap SD, because the mean of 30 resampled values is close to normal. $E[h] \approx z\,E\sqrt{\operatorname{Var}^\ast} \le z\sqrt{E \operatorname{Var}^\ast}$ by Jensen's inequality ($\sqrt{\cdot}$ is concave). Integer rounding adds about $1/12$ to $\hat\sigma^2_X$, which is negligible here. $\blacksquare$

Checked by `R2` (the variance by enumerating all $3^3$ and $4^4$ resamples, exactly) and `N Result 2`. In the AR(1) of (M3), the normal stand-in's mean half-width is 3.72 at 60 games and 3.80 at 30 (`S2`), so Jensen accounts for a factor of about 0.98. The rest of the gap to the measured 3.69 and 3.73 (1% to 2%) is the percentile interval against the normal one, and the model against the world. At 120 and 250 games "then" is placed on the peak, a window chosen by the ratings, so this result does not apply there as stated. The measured half-widths there (3.73) are the same in practice, and `S2` reproduces them. With $\sigma^2$ and $v_W$ measured, this result needs no memory model. It says the rule's interval measures only the spread inside a window, $\sigma^2 - v_W = 132$ points$^2$, out of $\sigma^2 = 945$.

**Result 3 (the memory model's variances).** Under (M3):

```math
v_W = \frac{\sigma^2 S(W)}{W^2}, \qquad
E\big[\hat\sigma^2_X\big] = \sigma^2\Big(1 - \frac{S(W)}{W^2}\Big), \qquad
\operatorname{Var}(D)\big|_{A = [0, W),\, B = [W, 2W)} = \frac{\sigma^2}{W^2}\big(2 S(W) - 2C\big),
```

with $C = \varphi (1 - \varphi^W)^2 / (1 - \varphi)^2$. The effective sample size of a window is $n_{\text{eff}} = \sigma^2 / v_W = W^2 / S(W)$. At $\varphi = 0.984$ and $W = 30$: $S(30) = 771.0$, $n_{\text{eff}} = 1.17$, $v_{30}^{1/2} = 28.45$ (measured 28.52, $n_{\text{eff}} = 1.16$), and $\operatorname{SD}(D) = 20.76$ at 60 games (measured 20.63).

*Proof.* There are $m$ pairs $(i, j)$ at distance 0 and $2(m - k)$ at distance $k \ge 1$, so $S(m) = m + 2\sum_{k=1}^{m-1}(m - k)\varphi^k$. Summing the arithmetic-geometric series gives $\sum_{k=1}^{m-1}(m-k)\varphi^k = \frac{m\varphi}{1-\varphi} - \frac{\varphi(1-\varphi^m)}{(1-\varphi)^2}$, and $m + \frac{2m\varphi}{1-\varphi} = \frac{m(1+\varphi)}{1-\varphi}$ (EXP-0030's math.md, Result 5, gives each step).

Then $v_W = \operatorname{Var}(\frac1W \mathbf 1^\top e) = \frac{1}{W^2}\mathbf 1^\top \Gamma \mathbf 1 = \sigma^2 S(W)/W^2$, with $\Gamma_{ij} = \sigma^2 \varphi^{\lvert i-j \rvert}$. Result 2 then gives $E\hat\sigma^2_X = \sigma^2 - v_W$.

For the difference, $D = w^\top e$ with $w = (-\frac1W \mathbf 1_W, \frac1W \mathbf 1_W)$, so

```math
\operatorname{Var}(D) = w^\top \Gamma w = \frac{\sigma^2}{W^2}\Big(2S(W) - 2\sum_{i<W \le j<2W}\varphi^{j-i}\Big).
```

The cross sum factorises: $j - i = (j - W) + 1 + (W - 1 - i)$, so it is $\varphi \cdot \big(\sum_{u<W}\varphi^u\big)^2 = C$. $\blacksquare$

Checked by `R3`: the closed form against the double sum for $m = 1$ to 30, exactly in $\varphi$; $\operatorname{tr}(H\Gamma)/W$ and $w^\top\Gamma w$ exactly for $W = 4, 6$. `S1` simulates 20,000 AR(1) players and gets 28.448 against 28.451. On rounded ratings it gets $\operatorname{Var}(D)$ within 4 SE of the closed form plus $1/(6W)$ for the rounding. This is the decision's "a window of 30 ratings holds about one independent game's worth of information, and the bootstrap counts 30".

**Result 4 (the honest interval).** If $D \sim N(\mu, \tau^2)$, the interval $D \pm c$ covers $\mu$ with probability $0.80$ if and only if $c = z_{0.80}\tau = 1.2816\,\tau$. So `honest_half_width` is the half-width an 80% interval would need. The rule's half-width is smaller by a factor of 7.2, 11.5 and 12.7 at 60, 120 and 250 games, and 5.5 at 30.

*Proof.* $P(\lvert D - \mu \rvert \le c) = P(\lvert Z \rvert \le c/\tau) = \operatorname{erf}(c / (\tau\sqrt2))$. This is strictly increasing in $c$, and equals 0.8 at $c/\tau = \Phi^{-1}(0.9)$. The ratios are `honest_half_width` / `half_width_mean` from `results.json`. $\blacksquare$

Checked by `R4` and `N Result 4`. At 120 and 250 games $D$ is off-centre ($E D = -9.4$ and $-23.4$), so an interval centred on $D$ also misses 0 through bias. Width and bias are separate failures, as the decision says, and Result 7 reproduces both.

**Result 5 (the false-alarm rate from memory alone, a lower bound).** Suppose $D \sim N(0, \tau^2)$ and the half-width $h$ is independent of $D$, as at fixed halves in a reflection-symmetric world. Then

```math
P(\text{falling}) = E\,\Phi(-h/\tau) \ \ge\ \Phi(-E h/\tau).
```

At 60 games, $\Phi(-3.69/20.63) = 0.429$, against the measured 0.443. At 30 games the bound is $\Phi(-3.73/16.15) = 0.409$, against 0.411.

*Proof.* `falling` is $D < -h$. Conditioning on $h$ gives $\Phi(-h/\tau)$, and $E$ over $h$ gives the equality. $\frac{d^2}{dx^2}\Phi(-x) = x\phi(x) \ge 0$ for $x \ge 0$, so $\Phi(-x)$ is convex there. Jensen's inequality then gives the bound. $\blacksquare$

Checked by `R5` (the second derivative) and `S2 Result 5`. The independence of $h$ and $D$ is an approximation, since both come from the same 60 ratings. The bound is the registration's "memory" mechanism in one line: an interval of $\pm 3.7$ on a difference whose SD is 21 calls a verdict about 86% of the time ($2 \times 0.43$). The measured share with a verdict either way is 88%.

**Result 6 (at fixed halves false "rising" equals false "falling", exactly).** Under (M1) and (M2), when the windows do not depend on the data (Result 1 (ii): 30 and 60 games, and the no-peak reads),

```math
P(\text{rising}) = P(\text{falling}).
```

The same holds for the Elo arms and for the chess.com-like Glicko-1 arm with `chesscom: true`.

*Proof.* (a) *The world is symmetric.* Consider the map $\rho : x \mapsto 3000 - x$ on every rating and strength, together with $s \mapsto 1 - s$ on results and $u \mapsto 1 - u$ on the result's uniform draw. Each step of (M1) commutes with it:

- the strength law $N(1500, 300)$ clipped to $[700, 2300]$ is symmetric about 1500, and so is the start at 1500;
- the opponent's offsets $N(0, 75)$ and $N(0, 50)$ are symmetric;
- the expected score satisfies $E(-x) = 1 - E(x)$, the draw probability $0.24\,\pi(1-\pi)$ is unchanged, and with $u' = 1 - u$ a win of the reflected player is a loss of the original;
- Glicko-2 maps $\mu \mapsto -\mu$, $s - E \mapsto -(s - E)$, so $\Delta \mapsto -\Delta$, while $v$, $\Delta^2$ and so the volatility and deviation are unchanged; Glicko-1 and Elo behave in the same way;
- rounding $\lfloor x + \tfrac12 \rfloor$ commutes with $\rho$ except at half-integers, an event of probability 0.

So the law of the stored series is invariant under $\rho$. For the chess.com arm, `pre_game_ratings` shifts the stored ratings by one game, which commutes with $\rho$.

(b) *The verdict.* With the windows fixed, reflecting the values negates every resampled difference, if the same resample indices are used: $d^\ast_j \mapsto -d^\ast_j$. Under (M2) the indices' law does not depend on the values, so this coupling is legitimate. Sorting gives $\operatorname{sort}(-d^\ast)_{(i)} = -d^\ast_{(R-1-i)}$. The interval's indices are 199 and 1800, and $199 = R - 1 - 1800$, so the reflected interval is exactly $[-d^\ast_{(1800)}, -d^\ast_{(199)}]$, the original's negated. The reflected player is `rising` iff $-d^\ast_{(1800)} > 0$, that is iff the original's $d^\ast_{(1800)} < 0$, which is the original's `falling`. The two events coincide pathwise, and the same holds with the roles swapped. By (a), the reflected player has the same law as the original, so $P(\text{rising}) = P(\text{falling})$. $\blacksquare$

The symmetry of the indices is an accident of floating point that matters here: `tail` is $(1 - 0.8)/2 = 0.09999999999999998$ in f64, so $\lfloor 2000 \cdot \text{tail} \rfloor = 199$, not 200, while $\lfloor 2000(1 - \text{tail}) \rfloor = 1800$. A first draft of this file used 200 and derived a one-order-statistic gap. The referee caught it, and `R6` now asserts the index pair from the arithmetic the Rust uses.

Checked by:

- `R6`: the index pair (199, 1800) from the f64 arithmetic, and the mirrored verdicts on 2,000 integer-valued vectors with ties and zeros; `analysis.glicko2` and `analysis.glicko1` commuting with $\rho$ on 3,000 random states; the result rule in sympy; the strength range; the rounding;
- `S3`: the real percentile rule with the coupling: every one of 1,500 AR(1) players' reflected verdicts is the mirror of the original's;
- `N Result 6`: the committed counts are 288 against 292 at 30 games, 310 against 307 at 60, and 312 against 315 and 314 against 343 in the no-peak reads (exact binomial $p$ = 0.90, 0.94, 0.94, 0.27).

This proves the registration's section 2 statement: "the 60-game cell carries memory alone, with false 'rising' about as common as false 'falling'". It also says what breaks the symmetry at 120 and 250 games. $\rho$ maps the first maximum to the first minimum, so the peak-centred window is not equivariant. Selection is exactly the asymmetric part. Rival explanations such as a burn-in drift would also break (a), and the diagnostics rule them out (signed error at the start −2.5).

**Result 7 (the memory model reproduces the run; approximate).** Model (M3), with its two constants fixed in section 2 and the production windows (E1) with the normal stand-in (E3), gives these values:

| Games | Falling | Rising | Mean $D$ | SD of $D$ | Mean $h$ |
|---|---|---|---|---|---|
| 30 | 0.414 | 0.412 | 0.1 | 16.1 | 3.80 |
| 60 | 0.431 | 0.435 | 0.2 | 20.7 | 3.72 |
| 120 | 0.626 | 0.294 | −9.4 | 33.9 | 3.75 |
| 250 | 0.778 | 0.170 | −24.4 | 38.6 | 3.75 |

The run measured falling 0.411, 0.443, 0.621, 0.784 and rising 0.417, 0.439, 0.300, 0.171, with $D$ means of 0.2, 0.5, −9.4, −23.4 and SDs of 16.2, 20.6, 33.5, 37.0. Every pair is within 4 combined SE, and every half-width within 4%. The no-peak reads agree too: 0.456 and 0.460 falling, against 0.446 and 0.449.

*Proof.* Not a proof: a measured check of the model on 20,000 simulated players. Results 2, 3 and 5 explain the 30- and 60-game columns analytically. The 120- and 250-game columns depend on the law of the first maximum of an AR(1) and on where it falls, which has no closed form here. $\blacksquare$ (approximate; the agreement is measured, not proved).

Checked by `S1` and `S2`: 24 comparisons with the run (falling and rising, $D$'s mean and SD, and the half-width at four lengths, and the two no-peak reads), and the table above against its own rounding. This is the strongest evidence for the decision's "Why" section. A Gaussian AR(1) with the run's own error SD and memory gives the same false alarms without any Glicko-2 detail. So the failure is a property of any rating with $\varphi \approx 0.98$, read by an i.i.d. bootstrap over a selected window.

**Result 8 (the reads are cut-offs on $k$).** At $\gamma = 1 - 0.05/3$ and $N = 700$, $\ell(k)$ and $u(k)$ are strictly increasing in $k$. So "within the bound" is $k \le 82$, "above the bound" is $k \ge 128$, and "above the hide line" is $k \ge 166$. The primary counts 310, 435 and 549 are all at or above 166, so the decision is "hide" (section 9, outcome 1).

*Proof.* Write $p = \hat p$ and $s(p) = \sqrt{p(1-p)/N + z^2/4N^2}$. Then $\frac{d}{dp}(p \mp z s) = 1 \mp z(1 - 2p)/(2Ns)$. This is positive iff $z^2(1-2p)^2 < 4N^2s^2 = 4Np(1-p) + z^2$. Since $(1-2p)^2 = 1 - 4p(1-p)$, the left side is $z^2 - 4z^2 p(1-p)$, which is below the right side for $0 < p < 1$. The cut-offs are then the first and last $k$ that cross each line. The values at the edges are $u(82) = 0.1494$, $u(83) = 0.1510$, $\ell(127) = 0.1492$, $\ell(128) = 0.1505$, $\ell(165) = 0.1996$ and $\ell(166) = 0.2009$. $\blacksquare$

Checked by `R8` (every $k$ at $N = 200$ and 700) and `N` (every committed interval recomputed from $k$ to $10^{-6}$, and every committed read recomputed from the cut-offs). The blind re-derivation gave the same proof and the same cut-offs.

**Result 9 (the decision rule's error control).** Under (M4):

- (i) The three Bonferroni intervals cover their true rates simultaneously with probability at least $1 - 3 \cdot (0.05/3) = 0.95$, to the accuracy of Wilson's coverage, under any dependence between cells.
- (ii) Each read is one-sided. Its exact size per cell at the boundary rate is:
  - "hide" when $p = 0.20$: $P(\operatorname{Bin}(700, 0.20) \ge 166) = 0.00894$;
  - "change or hide" ($k \ge 128$, above the bound or above the hide line) when $p = 0.15$: 0.00992;
  - "within" when $p = 0.15$: $P(\operatorname{Bin}(700, 0.15) \le 82) = 0.00722$.

  The nominal value is $0.05/6 = 0.00833$.
- (iii) The probability of deciding "hide" when every $p_g \le 0.20$ is at most $3 \times 0.00894 = 0.0268$, and of "change or hide" when every $p_g \le 0.15$ at most 0.0298. The probability of "keep" when any $p_g > 0.15$ is at most 0.00722, whatever the dependence.

*Proof.*

(i) A cell's interval misses with probability about $0.05/3$. The union bound over three events gives at most 0.05.

(ii) By Result 8, each read is an event $\{k \ge c\}$ or $\{k \le c\}$. $P_p(k \ge c)$ is non-decreasing in $p$ by the coupling $k = \sum_i \mathbf 1[U_i < p]$, so its supremum over $p \le 0.20$ is its value at 0.20, computed exactly from the binomial distribution.

(iii) "Hide" happens iff some cell has $k_g \ge 166$. By the union bound, its probability is at most $\sum_g P(k_g \ge 166) \le 3 \times 0.00894$. "Keep" needs every cell within, so if cell $g$ has $p_g > 0.15$, $P(\text{keep}) \le P(k_g \le 82) \le 0.00722$. This is Berger's intersection-union argument, and it needs no assumption about dependence. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.union_bound` for (iii). Its hypothesis is a finite family of events, each of probability at most $\varepsilon$, which matches here: three events and $\varepsilon = 0.00894$ (or 0.00992). Also `Rematch.bonferroni` for (i), whose hypothesis is that each event has probability at most $\alpha/m$. Wilson's interval meets that only approximately: its exact non-coverage at $N = 700$ is $1 - 0.9829$ at $p = 0.15$ and $1 - 0.9841$ at $0.20$, against the nominal $0.0167$. So (i) is approximate where (iii) is exact. Steps (ii) and the intersection-union are not in Lean. Checked by `R8`, `S5 Result 9` (400,000 draws at $p = 0.20$: 0.00912, within 4 SE of 0.00894) and `N Result 9`.

The family's one-sided error is therefore about 0.03, not 0.05. The registration's "Bonferroni across the three cells" is right as simultaneous two-sided coverage, and conservative as a test. At $p = 0.15$ a single cell's size, 0.00992, is slightly above the nominal 0.00833. This is Wilson's usual discreteness, and the family stays below 0.03. None of this touches the decision: the smallest primary count, 310, is far above 166, and $P(\operatorname{Bin}(700, 0.20) \ge 310) < 10^{-25}$.

**Result 10 (power, exactly, and what dependence does).** Let $F_p$ be the $\operatorname{Bin}(N, p)$ CDF and $(c_k, c_b, c_h)$ the cut-offs of Result 8. With three independent cells,

```math
P(\text{keep}) = F_p(c_k)^3, \quad P(\text{hide}) = 1 - F_p(c_h - 1)^3, \quad P(\text{change}) = F_p(c_h - 1)^3 - F_p(c_b - 1)^3,
```

and with one cell at $p$ and two at 0.10, $P(\text{change or hide}) = 1 - F_p(c_b - 1)F_{0.10}(c_b - 1)^2$. Every point of `power.json` is within 4 Monte Carlo SE of these values: the largest $\lvert z \rvert$ is 3.00 over 357 values, about what the largest of 357 normal deviates gives.

Under any dependence, with $P_1 = F_p(c_k)$, $P(\text{keep}) \in [\max(0, 1 - 3(1 - P_1)),\ P_1]$, and a read that needs one cell lies in $[\max_g P_g,\ \min(1, \sum_g P_g)]$. If the cells' counts are associated (positively dependent), "keep" is at least $P_1^3$ and a one-cell read at most its independent value.

*Proof.* The exact formulas follow from independence and Result 8: "keep" is three events $\{k_g \le c_k\}$, and "hide" is the complement of three events $\{k_g < c_h\}$. "Change" is "some $k_g \ge c_b$ and none $\ge c_h$", which is $P(\text{all} < c_h) - P(\text{all} < c_b)$.

The bounds are Fréchet's: $P(\bigcap E_g) \le \min P(E_g)$, and $P(\bigcap E_g) = 1 - P(\bigcup E_g^c) \ge 1 - \sum(1 - P(E_g))$. Similarly $\max_g P(F_g) \le P(\bigcup F_g) \le \sum P(F_g)$.

For associated counts, the events $\{k_g \le c\}$ are all decreasing in the counts, so Harris's inequality gives $P(\bigcap) \ge \prod P$. For a one-cell read, $P(\bigcup\{k_g \ge c\}) = 1 - P(\bigcap\{k_g < c\}) \le 1 - \prod P(k_g < c)$. $\blacksquare$

Checked by `R10` (the Fréchet inequalities on a grid, and $1 - (1-P)^3 \ge P$ in sympy) and `S5 Result 10`. Under a Gaussian copula with correlation 0.5, which is an associated law, "keep" at 0.10 is 0.849 against the independent 0.830, and "hide" at 0.25 in all three is 0.971 against 0.991. `N Result 10` and the `N prereg section 6` rows check every power the registration quotes.

> **Finding 1 (the dependence premise of section 6 and `power.py`).** Section 6 says the three cells "are positively correlated, which makes 'keep' more likely than simulated; the simulation is the harder case". `power.py`'s docstring adds "and the same for the other reads, which need one cell". Two things are off.
>
> 1. The committed per-player flags show no positive correlation. The phi coefficients between the 60-, 120- and 250-game verdicts are 0.008, 0.020 and −0.058, each within $2/\sqrt{700}$ of 0. P(all three) is 0.217 against 0.216 under independence. The windows at different lengths are mostly different games, further apart than the rating's memory ($1/(1 - \varphi) \approx 62$ games), so sharing players does not make the verdicts share noise. The independent simulation is close to the truth for this rule.
> 2. Under positive dependence, a read that needs one cell becomes *less* likely than simulated, not equally hard (Result 10). The registration's figures stand as follows. "Keep 0.83 at 0.10" lies in $[0.819, 0.940]$ under any dependence, so 700 players reach 0.80 whatever the dependence. "0.88 when 0.20 in one cell" and "1.00 when 0.30 in one cell" are one-cell powers, also free of dependence. But "hide when 0.25 in all three: 0.99" is 0.991 under independence, and its dependence-free floor is a single cell's 0.796.
>
> This changes no decision: the observed counts are nowhere near any cut-off. It matters for EXP-0030 and any challenger sized the same way, whose cells may be more dependent. Reported to the coordinator.

**Result 11 (the hide line, and when the decision would flip).** Let a share $b$ of players be truly declining and caught with probability $d$, and let the others be told `falling` with probability $f$. The share of `falling` headlines that are false is $(1-b)f / ((1-b)f + bd)$. It exceeds $\tfrac12$ iff $f > f^\ast = bd/(1-b)$, equivalently iff $b < b^\ast = f/(f + d)$.

*Proof.* By Bayes' rule over the two kinds of player: the expected false headlines are $(1-b)f$ and the true ones $bd$. Solving $(1-b)f = bd$ for $f$ and for $b$ gives $f^\ast$ and $b^\ast$, and both inequalities are strict monotone rearrangements. $\blacksquare$

Checked by `R11` and `N`:

- the registration's 0.20, 0.09 and 0.34;
- the decision's break-even of 0.18 to 0.25 ($b = 0.2$, $d$ from 0.735 to 1.0);
- the decision's 28%, 37% and 43%, which are $b^\ast$ with $f$ at its Bonferroni lower end and $d = 1$, the case most favourable to the rule;
- likelihood ratios $d/f$ of 1.25 to 2.11;
- the share of false headlines at $b = 0.2$, which is 0.65 to 0.76 (0.71 to 0.76 at a 100-point fall).

The blind re-derivation used the point $f$ and the measured $d$ and got $b^\ast$ from 0.32 to 0.44. That is the same formula on less conservative inputs, and it gives the same conclusion.

**Result 12 (the accuracy and blunders rows: the same bootstrap on values without memory).** If a row's values are i.i.d. normal within and across the two windows, each side's false verdict has probability approximately

```math
P\Big(T_{2W-2} > z_{0.80}\sqrt{\tfrac{W-1}{W}}\Big) = 0.106 \qquad (W = 30),
```

slightly above the nominal 0.10. The six committed rows (0.080 to 0.114) are within 3 binomial SE of 0.106. This holds at fixed halves. On the peak window (120 and 250 games) "then" is chosen where the rating is high, and so where the player is overrated and loses more often. Its accuracy is therefore lower, and "worse" becomes less likely and "better" more likely.

*Proof.* The bootstrap variance $\hat\sigma^2/W$ uses the plug-in divisor $W$, which is $(W-1)/W$ times the unbiased one. So the rule's threshold is $z\sqrt{(W-1)/W}$ unbiased SEs. The difference divided by its estimated SE, pooled over $2W - 2$ degrees of freedom, is approximately $t_{2W-2}$. $\blacksquare$ (approximate: the percentile interval is not exactly a $t$ interval).

Checked by `S4`: on 3,000 i.i.d. normal pairs with the real percentile rule, 0.112 against 0.106, within 4 SE. `N Result 12` gives $\lvert z \rvert \le 2.26$ for the six rows. In (M1) accuracy depends only on the result, and the result carries the rating's memory only through $\pi_t$, whose SD from the error is about $q\sigma/4 \approx 0.045$. At fixed halves that adds less than 0.005 to the rate (a variance share under 2%). `S5 Result 12` puts accuracy through the production windows in the AR(1) world, with results driven by the error. "Worse" and "better" are 0.110 and 0.110 at 60 games, 0.100 and 0.117 at 120, and 0.092 and 0.137 at 250, with a mean accuracy difference of +0.31 at 250. So the rows are a working internal control of the mechanism at fixed halves, as the decision says: the same bootstrap is about right when the values do not remember. At 120 and 250 games the selection moves "worse" down by about 0.01 to 0.02, the direction that keeps the reads "within the bound", and "better" up by about 0.02 to 0.03. The decision's "within the bound" reads stand, and the control is weaker there than it says.

**Result 13 (the bookkeeping).** The simulated games are

```math
700 \cdot 550 + 2 \cdot 200\,(360 + 420 + 550) + 300 \cdot 250 + 2 \cdot 200 \cdot 550 + 200 \cdot 550 = 1{,}322{,}000,
```

and the trajectories are $700 \cdot 4 + 700 \cdot 2 + 6 \cdot 200 + 300 \cdot 3 + 3 \cdot 200 \cdot 3 = 8{,}100$, both equal to `compute.actual` and to the budget.

*Proof.* Each population plays burn-in plus observed games (`Runner.simulate`): settled 300 + 250; a decline cell 300 + $g$; new accounts 250; each other arm 550. The settled players are read at four lengths and twice more for the no-peak read. A decline cell is read once, new accounts three times, and each arm three times. $\blacksquare$ Checked by `N Result 13`.

> **Finding 2 (roundings and descriptions in decision.md).** None of these changes the decision.
>
> - (a) The primary's Bonferroni lower ends are quoted as "0.40, 0.58 and 0.75". The third is 0.7448, which rounds to 0.74; the table in the same file shows 0.745.
> - (b) "The rule's is six to thirteen times too narrow": the ratio of the honest half-width to the rule's is 7.2, 11.5 and 12.7 at 60, 120 and 250 games, so "seven to thirteen". The 6 matches neither: it is 5.5 at 30 games, and 5.6 is the ratio of SD (not half-width) to half-width at 60.
> - (c) Withdrawn after review. "Peak selection ... adds about 0.2 to 0.3 to 'falling'" is right at the one decimal it is quoted at: the primary minus the no-peak read is 0.176 at 120 games and 0.336 at 250.
> - (d) "About three in four 'falling' headlines would be false" at $b = 0.2$: it is 0.65 to 0.76 over the six detection cells, and 0.71 to 0.76 at a 100-point fall.

> **Finding 3 (the Glicko-1 asymmetry at 60 games is chance).** The decision says Glicko-1's 60-game read (0.365 falling, 0.495 rising) "may be noise or the loader's pre-game derivation under `chesscom: true`, and is to be checked there before any chess.com claim". By Result 6, at 60 games the windows are fixed. Glicko-1 commutes with the reflection (`R6`), and so does `pre_game_ratings`: it shifts the stored ratings by one game and keeps the first. So under (M2) the loader cannot make false "rising" and false "falling" differ at all: they are equal exactly. Conditional on the 172 players with a verdict, 73 against 99 has an exact two-sided binomial $p$ of 0.056, an unremarkable value among the dozens of uncorrected exploratory reads. The open check in `decision.md` can be closed as chance for the 60-game cell. A chess.com claim still needs real chess.com constants. Reported to the coordinator.

## 5. Numbers

Every number that `decision.md` and `summary.md` quote, and the power figures of the preregistration's section 6, recomputed by `math_check.py` `numbers` (rows `N ...`) from `results.json` and `power.json`.

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| 310, 435, 549 of 700; 0.443, 0.621, 0.784; 44%, 62%, 78% | share | `primary.cells[].k`, `share` | 0.442857, 0.621429, 0.784286 |
| 95% [0.406, 0.480], [0.585, 0.657], [0.752, 0.813]; summary 41 to 48, 58 to 66, 75 to 81 | (E5) | `primary.cells[].ci95` | all 18 primary intervals to $10^{-6}$ |
| Bonferroni [0.399, 0.488], [0.577, 0.664], [0.745, 0.819]; lower ends "0.40, 0.58 and 0.75" | (E5) | `ci_bonferroni` | 0.3986, 0.5767, **0.7448** (Finding 2a) |
| every cell above the hide line; hide | Result 8 | `read`, `decision` | $k \ge 166$ in all three |
| four to eight times the nominal | ratio | `share` / 0.10 | 4.43 to 7.84 |
| change −39, −50, −59 | mean; Result 1 (iv) | `exploratory.mean_change_when_falling` | −39.02, −49.96, −58.57 |
| break-even 0.18 to 0.25; 28%, 37%, 43%; up to about 0.28 | Result 11 | `detection[].share`, `ci_bonferroni[0]` | 0.184 to 0.25; 0.2850, 0.3658, 0.4269 |
| half-width 3.7 at every length | (E4) | `diagnostics.interval.*.half_width_mean` | 3.69 to 3.73; Result 2 predicts 3.80 |
| SD 21, 34, 37; honest 26 to 47 | Result 4 | `diff_sd`, `honest_half_width` | 20.63, 33.53, 37.02; 26.43 to 47.44 |
| six to thirteen times too narrow | Result 4 | ratio | **7.2, 11.5, 12.7** (Finding 2b) |
| about 5 points a game; error SD 31; correlation 0.97; window SD 29; deviation 45 | (E7) | `settled_step_mean`, `settled_error_sd`, `settled_error_lag1`, `settled_window_mean_sd.30`, `settled_rd_mean` | 5.21, 30.74, 0.9655, 28.52, 45.00 |
| about one independent game's worth | Result 3 | $\sigma^2 / v_{30}$ | 1.16 (AR(1): 1.17) |
| off-centre −9 and −23 | (E4) | `interval.120/250.diff_mean` | −9.37, −23.38 |
| about 0.45 at every length; 0.443 and 0.439 | Results 5, 6 | `primary`, `secondary.rising`, `first_and_last_30` | 0.443, 0.446, 0.449; 0.4386 |
| no-peak 0.446 [0.409, 0.483], 0.450; 0.449 [0.412, 0.486], 0.490 | (E5) | `exploratory.first_and_last_30` | as quoted |
| nine in ten get a verdict either way | sum | as above | 0.881, 0.896, 0.939 |
| peak adds about 0.2 to 0.3 | difference | as above | 0.176, 0.336 (right at one decimal) |
| more than twice the hide line | – | – | 0.443 > 0.40 |
| rising 0.439 [0.394, 0.484], 0.300 [0.260, 0.343], 0.171 [0.140, 0.208]; summary 44%, 30% | (E5) | `secondary.rising` | as quoted; reads from the cut-offs |
| accuracy 0.113, 0.080, 0.110; blunders 0.114, 0.087, 0.086; 0.14988, clears by 0.0001 | (E5), Result 12 | `secondary.then_now_rows` | as quoted; 0.00012 |
| detection table; 0.735 to 1.0; summary 74% to 100% | (E5) | `detection` | all 18 intervals to $10^{-6}$ |
| likelihood ratios 1.25 to 2.1 | Result 11 | `detection`, `primary` | 1.250 to 2.111 |
| about three in four false | Result 11 | as above | **0.65 to 0.76** (Finding 2d) |
| 30 games 0.411 and 0.417 | share | `exploratory.games_30`, `rising_30` | 0.4114, 0.4171 |
| score worse 0.05 to 0.10; any row worse 0.60, 0.69, 0.83 | share | `score_worse`, `any_worse` | 0.053 to 0.096; 0.597, 0.687, 0.831 |
| new accounts 0.43, 0.53, 0.60; 0.58 and 0.26 | share | `new_account` | 0.433, 0.533, 0.597; 0.583, 0.255 |
| Elo K = 10: 0.42, 0.58, 0.77; K = 32: 0.415, 0.65, 0.785; Glicko-1 0.365 [0.301, 0.434], 0.625 [0.556, 0.689], 0.815 [0.755, 0.863]; 0.495 | share, (E5) | `other_ratings` | as quoted; Finding 3 |
| burn-in −2.5; ±3.1; −3.9 to +4.5 | (E7) | `diagnostics.settled`, `decline`, `other_ratings[].settle` | −2.46; max 3.07; −3.86 to 4.45 |
| 1,322,000 games, 8,100 trajectories | Result 13 | `compute.actual` | as quoted |
| 98 s, 0.03 CPU-hours, `run_minutes: 2` | arithmetic | prose | 0.027 h, 1.6 min |
| prereg: b d/(1−b) = 0.20, 0.09, 0.34 | Result 11 | – | 0.200, 0.089, 0.343 |
| prereg: keep 0.83 (600: 0.73), 0.08 at 0.12 | Result 10 | `power.json` `grid` | exact 0.8298, 0.7292, 0.0831 |
| prereg: 0.88 (one cell 0.20), 1.00 (one cell 0.30), 0.99 (0.25 in all three), 200 decide hide at 1.00 from 0.30 | Result 10 | `grid`, `one_cell` | 0.8820, 0.9999, 0.9915, 0.9961 (Finding 1) |
| prereg: 700 the smallest of 200 to 800 | Result 10 | `grid`, `one_cell` | 700 |
| prereg: half-width 0.027 at 0.10; detection half-width at most 0.093 | (E5) | `half_width_at_0_10` | 0.0272; Wilson max 0.0917 (the 0.093 is the Wald bound, $z\sqrt{0.25/200}$) |

Not recomputed, because they are not in a committed file: the reviewer's approximation (0.37 to 0.82), the timing namespace's 5 µs a game and 13 ms a trajectory, and the 98 s wall time beyond its arithmetic.

## 6. Assumptions that can fail

- **The bootstrap's randomness (M2): Results 2 and 6.** The resample indices come from a hash of the sorted values, not from independent uniforms. Result 6's exact equality holds for the modelled uniforms. For the hash it holds in law only if the hash behaves like a random oracle. A hash correlated with the reflection could in principle shift the balance. The committed counts at fixed halves (four reads, all $p \ge 0.27$) show no such shift. No registered sensitivity covers it beyond that.
- **The reflection symmetry of the world: Result 6.** It needs the strength law to be symmetric about the starting 1500. It also predicts a mean error of 0. The measured `settled_error_mean` is −1.75, which is $z = -2.5$ against the AR(1)'s SE for a 700-player mean of 250-game means (`N section 6`). That is probably chance among the diagnostics read; no registered check covers it. New accounts break it in the transient, because a player below 1500 truly falls while the rating settles. That is why the new-account rates are not read as false alarms (decision, Exploratory). The burn-in diagnostics guard the settled populations, with every signed start error within ±4.5 against the 25-point line.
- **The AR(1) (M3): Results 3, 5 and 7.** The Glicko-2 world is AR(1) only to first order (EXP-0030's math.md, Result 1). Its innovations are discrete and its deviation sits at the floor of 45. Result 7 measures the cost: every share is within 4 SE and every half-width within 4%. Real ratings with breaks, streaks and tilt have more memory than (M3) and than the world, which the decision states as the main transfer limit. The Elo K = 10 and K = 32 arms and the Glicko-1 arm (exploratory) show the result is robust to the step size within this world. No arm has breaks; EXP-0030 adds one.
- **Jensen and normality of the percentile interval: Results 2 and 5.** The predicted half-width 3.80 is an upper bound on the mean of about 3.7, and the lower bound 0.429 on the false-alarm rate is a bound, not an estimate. Both assume the resampled mean is nearly normal, which 30 values give. `S2` and `S3` use the real percentile rule or its stand-in and agree with the run.
- **Independence of $h$ and $D$: Result 5.** Both are functions of the same ratings. The bound is approximate, and `S2` checks it at 60 games (0.429 against a simulated 0.431).
- **Wilson's coverage: Result 9 (i).** It is asymptotic. At 700 players the exact two-sided coverage at 0.15 and 0.20 is 0.983 to 0.984, against the nominal 0.983, and one tail's size at 0.15 is 0.0099, against 0.0083. Result 9 (ii) and (iii) use exact binomial sizes instead.
- **Independence of the cells: Result 10, `power.py`.** It is exact only for the detection cells, which are separate populations. For the primary, Finding 1 says what the data show (no positive correlation) and gives the dependence-free bounds. The sizing to 700 survives any dependence.
- **Players independent (M4): Results 8 to 10.** True by construction: one series per player, drawn in sequence from one seeded generator, with no shared state between players.
- **The accuracy and blunders model: Result 12.** The values are memoryless apart from the result, by assumption with no measured source. Real accuracy has form, so these rows' "within the bound" is not evidence about real players. The decision says so, and asks for a lag-1 check on a real series first.
- **The declining share $b$: Result 11.** $b = 0.2$ is an assumption. Result 11 shows the decision survives up to $b = 0.28$ at every length, even granting perfect detection.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py`, `power.py`, `power.json`, `results.json` and `trajectory.rs` at `ef0789b9`, but not this file, `decision.md`, `summary.md` or `review.md`. It derived five results independently:

- the Wilson monotonicity proof and the cut-offs 82, 128 and 166 (Result 8);
- the exact sizes 0.008942, 0.009918 and 0.007216, with the family bounds, and the point that the reads are one-sided (Result 9);
- the exact power formulas, agreeing with `power.json`, and the Fréchet bounds [0.819, 0.940] and the 0.796 floor (Result 10);
- the bootstrap's $E\hat\sigma^2 = \sigma^2 - v_W$, the predicted 3.798 and 3.894, $n_{\text{eff}}$ = 1.162 and 1.079, $S(30) = 771.0$, and $\operatorname{SD}(D) = 20.77$ at 60 games (Results 2 and 3);
- the reflection symmetry (Result 6); it also derived a one-order-statistic gap of about $1.2 \times 10^{-4}$ favouring "rising", from the same misreading of the lower index as my first draft (see the referee's point 1);
- $b^\ast = f/(f+d)$ (Result 11).

Every formula and number agreed with this file. Independently, it found:

- Finding 1: the flags' correlations of 0.008, 0.020 and −0.059, and that the docstring's "the same for the other reads" is wrong in general;
- the one-sided family error of about 0.025 to 0.030 in Result 9;
- that the Glicko-1 60-game asymmetry is chance under the same symmetry, which is Finding 3.

The one difference was in inputs, not in maths. It computed $b^\ast$ from the point $f$ and the measured $d$ (0.32 to 0.44). The decision's 28%, 37% and 43% use the Bonferroni lower end and $d = 1$. Result 11 now names both. It also measured a Jensen factor of 0.962 in an AR(1). The first draft cited it, and the referee showed it does not reconcile 3.80 with the observed 3.7 (it gives 3.65); Result 2 now uses the factor from `S2`.

Four of `math_check.py`'s number checks failed on the first run. Each one was a quoted rounding I had taken on trust, and in each case I corrected the text and the assertion rather than widening a tolerance:

- the 250-game lower end is 0.7448, not 0.75 (Finding 2a);
- "three in four" runs from 0.6549, not 0.66 (Finding 2d);
- "about 0.45" is 0.443 to 0.449, now asserted as within 0.01;
- the independent "hide at 0.25" is 0.9915, quoted as 0.991.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee. It checked Results 1 to 13 step by step, found the proofs of Results 3, 4, 5, 8, 9 (ii) and (iii), 10, 11 and 13 correct, and the Lean citations matching their use. It made one major point and ten minor ones, all accepted:

1. **Major.** The rule's lower percentile index is 199, not 200: `tail = (1.0 - 0.8) / 2.0` is 0.09999999999999998 in f64. The interval is exactly symmetric, so Result 6 is an exact equality, not equality up to one order statistic. The checker's `at(s, 0.1)` had restated my misreading instead of the Rust's arithmetic. Result 6, (E2), the notation and Finding 3 are rewritten. `R6` now asserts the index pair from the f64 arithmetic, and `S3` asserts the mirrored verdict for every player. The conclusions are stronger for it.
2. **Minor.** Result 6's numeric bound had sat inside an exact statement. It is gone with point 1.
3. **Minor.** Result 12's "adds less than 0.005" held only at fixed halves. Peak selection shifts the accuracy rows at 120 and 250 games. Result 12 now says so, with the sizes from a new `S5 Result 12`.
4. **Minor.** Result 2 compared the $W = 30$ prediction with the 30-game read ($W = 15$), and cited a Jensen factor that does not close the gap. It now gives per-window predictions (3.80 and 3.89), takes the factor from `S2`, and is restricted to windows not chosen by the ratings.
5. **Minor.** Finding 2 (c) was not a misstatement at the quoted one decimal. It is withdrawn, and its check now asserts the quote at one decimal.
6. **Minor.** Result 9 (ii)'s 0.00992 is "change or hide", not "above the bound" alone. Relabelled.
7. **Minor.** `N (M3)` had asserted its inputs against themselves. It now derives $K = 11.42$ from one Glicko-2 step and $E[\pi(1 - \pi)] = 0.235$ from the opponents' spread. (M3) quotes both.
8. **Minor.** Result 1 (iv)'s reason ("the current rating is one of the $r_t$") is false with `chesscom: true`. The proof now argues from peak $= \max(c, \max_t r_t)$, and (E1) states the chess.com case.
9. **Minor.** Checks that could not fail or restated the code are fixed: `R4` now solves for $z$ with sympy, `R10` tests Fréchet's bounds on 20,000 random dependent joint laws, `S2 Result 5` compares $E\,\Phi(-h/\tau)$ with the simulated share directly, the always-passing `S3` band check is gone, the no-peak rising comparison uses its own SE, and the redundant `R1` flag is removed.
10. **Minor.** Quoted numbers that nothing asserted are now asserted: Result 8's edge values, $S(30)$ and $\operatorname{SD}(D)$, Result 5's bounds and the 86% and 88%, Result 6's counts, Result 7's table, the 357 values and largest $\lvert z \rvert$ of 3.00, P(all three) 0.217 against 0.216, the 5.6, the Wilson maximum 0.0917, and the deviation's mean excess over its floor.
11. **Observation.** The settled mean error of −1.75 is $z = -2.5$ under the symmetric model. Section 6 now mentions it.

After these changes `math_check.py` runs 127 checks, all PASS, and the lab check with `REMATCH_MATH_RULE=fail` passes.
