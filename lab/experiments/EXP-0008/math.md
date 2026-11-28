# EXP-0008: the maths

Written after the decision, as part of the ADR 0118 backfill. Nothing in `preregistration.md`, `analysis.py`, `results.json` or `decision.md` was changed. Each result below is proved in prose and checked by `math_check.py` in this folder: symbolic checks and exhaustive enumerations (prefix `R`), simulations from a known truth (prefix `S`) and the recomputation of every quoted number (prefix `N`).

EXP-0008 compares **Stockfish 19** node counts by how often each one's move labels differ from a deeper search's. The production code searches, judges and scores. Python counts labels that differ, sums them per game and decides. The maths has to show four things. First, that searching the reference only where two settings disagree still gives their contrast exactly, and what a reference that is itself a search does to that contrast (Results 1 and 10). Second, what the ratio over games estimates, with its variance and interval (Results 2 to 5). Third, that section 6's power analysis and section 9's rule have the error control and power the registration states, the chi-square factor included (Results 6 to 9). Fourth, which of the registration's arguments in words hold (Results 8, 10, 11 and 13, and six Findings). Every deciding contrast is recomputed from scratch here, because `results.json` keeps each game's totals.

## 1. Notation

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $g = 1, \dots, n$ | a sampled game; $n = 160$ (step up), 40 (step down, deep check) | games | `sample.csv`, `analysis.py` `STEP_DOWN_GAMES`, `DEEP_GAMES` |
| $j = 1, \dots, M_g$ | a move (ply) of game $g$; $M_g$ its plies | moves | `sample.csv` `plies`, `results.json` `totals[*].m` |
| $S \in \{50\text{k}, 100\text{k}, 300\text{k}, 1\text{M}\}$ | a candidate: one thread, 16 MB, `go nodes N`, no tables | – | `SETTINGS`, `CANDIDATES` |
| $\rho$ | the reference: 30 million nodes, 128 MB, Syzygy 3-4-5; $\rho'$ the deep check, 100 million, 256 MB | – | `SETTINGS["ref"]`, `["deep"]` |
| $L_S(g, j)$ | the label of move $j$ (none, inaccuracy, mistake, blunder), from setting $S$'s searches of the move's two positions | – | `cli lab judge` (`backend/crates/analysis/src/evaluate.rs` `evaluate_game`, `backend/crates/chess-core/src/judgement.rs` `judge`) |
| $d_S(g, j)$ | $[L_S(g,j) \ne L_\rho(g,j)]$, 1 when $S$'s label is not the reference's | – | `contrast_totals` |
| $E_g$ | $\sum_j (d_A - d_B)(g, j)$, a contrast's total in game $g$ | moves | `contrast_totals`, `results.json` `totals[*].e` |
| $D(S)$ | the population share of moves with $d_S = 1$ | – | section 4 |
| $\theta = D(A) - D(B)$ | a contrast; $\theta_\uparrow = D(300\text{k}) - D(1\text{M})$, $\theta_\downarrow = D(100\text{k}) - D(300\text{k})$ | – | section 4 |
| $\hat R$, $\widehat{\mathrm{SE}}$ | the ratio $\sum_g E_g / \sum_g M_g$ and its linearised standard error | – | `ratio` |
| $t_{\gamma, \nu}$ | the two-sided Student quantile at level $\gamma$, $\nu = n - 1$ | – | `t_crit` |
| $\delta$ | the tolerance, 0.01 | – | `DELTA` |
| $k$ | section 6's variance factor, $\sqrt{(n_p - 1)/\chi^2_{0.20, n_p - 1}}$, $n_p = 20$ | – | `cmd_power` (`k_upper`), `power.json` `k_upper80` |
| $T(g, j)$ | the unknown true label (what an infinitely deep search would say) | – | section 2 of the preregistration |

## 2. Model

**(M1) Labels are functions of searches.** A move's label is a deterministic function of the scores of its two positions (lila's rule: a fall in winning chances of at least 0.3, 0.2 or 0.1, with the mate rules, `judgement.rs` `judge`). A node-limited search at one thread is deterministic (ADR 0104), and the run re-searched 12 positions of each of its six settings without the cache: all 72 were identical (`results.json` `rechecks`). Given the games, every $L_S$ is therefore fixed. The only randomness is which games are sampled.

**(M2) Games are independent draws.** The sampled games are treated as independent and identically distributed draws of $(E_g, M_g)$ from the population of section 7. No player appears in two sampled games. The seeded order with the player rule is close to simple random sampling (Finding 3). With 160 of 5,398 games sampled, the finite-population correction ($1 - 160/5{,}398 = 0.97$) is ignored, as section 8 does. That ignores 3% of the variance, on the cautious side.

**(M3) The estimand.** $\theta = E[E_g]/E[M_g]$, the population share of moves on which $A$ and not $B$ differs from the reference, minus the reverse. It is a ratio of means, so each move counts once, not each game.

**(M4) Section 6's power model.** Games are drawn with replacement from the pilot's 20, with each game's totals replaced by $a_g = \theta_* M_g + k(E_g - r M_g)$, where $r$ is the pilot's ratio and $\theta_*$ the scenario's contrast (Result 7). The deep check is taken to agree with the 30-million-node reference.

## 3. Estimators

- **(E1) The per-move contrast** (`contrast_totals`): $(d_A - d_B)(g, j)$, which is $0$ when $L_A = L_B$ and $[L_B = L_\rho] - [L_A = L_\rho]$ otherwise. The run stops if a discordant move has no reference.
- **(E2) The ratio** (`ratio`): $\hat R = \sum_g E_g / \sum_g M_g$.
- **(E3) Its standard error** (`ratio`):

```math
\widehat{\mathrm{SE}}^2 = \frac{n}{n-1}\,\frac{\sum_g (E_g - \hat R M_g)^2}{\left(\sum_g M_g\right)^2}.
```

- **(E4) The interval:** $\hat R \pm t_{\gamma, n-1}\widehat{\mathrm{SE}}$ at $\gamma$ = 0.80, 0.95, 0.99.
- **(E5) The rule** (`decide`). Let $\ell_\uparrow$, $u_\uparrow$ be the 95% ends of $\theta_\uparrow$ on all games, $\ell'_\uparrow$ the lower 95% end against $\rho'$ on the deep 40, and $\ell_\downarrow$, $u_\downarrow$ and $u^{99}_\downarrow$ the 95% and 99% ends of $\theta_\downarrow$ on the step-down 40. The outcomes, in order:
  1. if $\ell_\uparrow > \delta$: *300k falls short* if also $\ell'_\uparrow > \delta$, else *need more data*;
  2. if $u_\uparrow < \delta$ and $u^{99}_\downarrow < \delta$: *100k is enough* (50,000 if its own 99% upper end is also below $\delta$);
  3. if $u_\uparrow < \delta$ and $\ell_\downarrow > \delta$: *300k is right*;
  4. otherwise *need more data*.
- **(E6) The absolute measures** (`evaluate_stage`, `stratified`). $\hat D_{\text{abs}}(S) = \sum_{(g,j)} w_{gj}\,d_S(g, j) / \sum_g M_g$. Here $w = 1$ on each of the 945 discordant moves and $w = 9{,}651/100$ on each of the 100 moves sampled from the 9,651 others. The bootstrap over games is seeded.
- **(E7) The variance factor** (`cmd_power`): $k = \sqrt{19/\chi^2_{0.20, 19}} = 1.17697$.
- **(E8) The cost** (`cmd_power`): planned CPU-hours $= n\,(63.7 \sum_S c_S + 6.45\,c_\rho) + 40 \times 7.6\,c_\rho + 200\,c_\rho$, with $c_S$ the pilot's CPU-hours a position.

## 4. Derivations and proofs

**Result 1 (the contrast is exact on the discordant moves).** For any two settings $A$, $B$ and any reference, $(d_A - d_B)(g, j) = 0$ whenever $L_A(g, j) = L_B(g, j)$. Otherwise it equals $[L_B = L_\rho] - [L_A = L_\rho] \in \{-1, 0, 1\}$. So $E_g$, and with it $\hat R$, (E3) and every interval, depends on the reference only through the moves where $A$ and $B$ disagree. Searching the reference there and nowhere else loses nothing.

*Proof.* If $L_A = L_B$, then $[L_A \ne L_\rho] = [L_B \ne L_\rho]$, and the difference is 0. If $L_A \ne L_B$, at most one of them equals $L_\rho$. With $[x \ne y] = 1 - [x = y]$, $d_A - d_B = [L_B = L_\rho] - [L_A = L_\rho]$, which is 1, $-1$ or 0. By (M1) a move's label depends only on its two positions, so the reference searched on a discordant move's two positions gives that move's reference label exactly (`labels_on`). $\blacksquare$ Checked by `R Result 1` (all 64 label triples) and by the run's own guard. "Exact" here means no sampling of moves within the sampled games: the only sampling is of games.

**Result 2 (the linearised variance is the delta method's).** The residuals $E_g - \hat R M_g$ sum to exactly 0. (E3) equals $s_r^2/(n\bar M^2)$, with $s_r^2 = \frac{1}{n-1}\sum_g (E_g - \hat R M_g)^2$ the residuals' sample variance and $\bar M$ the mean plies. Under (M2), the delta method gives $\operatorname{Var}(\hat R) \approx \operatorname{Var}(E - \theta M)/(n\,\mu_M^2)$, of which (E3) is the plug-in.

*Proof.* $\sum_g (E_g - \hat R M_g) = \sum_g E_g - \hat R\sum_g M_g = 0$ by the definition of $\hat R$. So the residuals have mean 0, and their sample variance is $\sum_g r_g^2/(n-1)$. Then $\frac{n}{n-1}\frac{\sum_g r_g^2}{(n\bar M)^2} = \frac{\sum_g r_g^2}{(n-1)\,n\,\bar M^2} = \frac{s_r^2}{n\bar M^2}$. For the delta method, $\hat R = f(\bar E, \bar M)$ with $f(x, y) = x/y$ and $\nabla f(\mu_E, \mu_M) = (1/\mu_M, -\mu_E/\mu_M^2)$. By the central limit theorem, $(\bar E, \bar M)$ has covariance $\Sigma/n$, so

```math
\operatorname{Var}(\hat R) \approx \frac{1}{n}\nabla f^\top \Sigma\, \nabla f = \frac{\sigma_{EE} - 2\theta\sigma_{EM} + \theta^2\sigma_{MM}}{n\,\mu_M^2} = \frac{\operatorname{Var}(E - \theta M)}{n\,\mu_M^2}.
```

Replace $\theta$ by $\hat R$, $\mu_M$ by $\bar M$ and the variance by $s_r^2$. $\blacksquare$ Checked by `R Result 2` (both identities, symbolically) and `S Result 2`. Resampling the 160 committed game totals, $E[\widehat{\mathrm{SE}}^2]/\operatorname{Var}(\hat R)$ is 0.984 at 40 games and 1.002 at 160.

**Result 3 (the bias of the ratio).** Under (M2), $E[\hat R] - \theta = -\operatorname{Cov}(\hat R, \bar M)/\mu_M$, and $|E[\hat R] - \theta| \le \operatorname{SD}(\hat R)\operatorname{CV}(\bar M)$.

*Proof.* $\hat R\bar M = \bar E$, so $E[\hat R\bar M] = \mu_E = \theta\mu_M$. Then $\operatorname{Cov}(\hat R, \bar M) = \theta\mu_M - E[\hat R]\mu_M$. The bound is Cauchy-Schwarz (Hartley and Ross 1954). $\blacksquare$ `R Result 3` and the first part of `S Result 3` are definitional (they restate $\hat R\bar M = \bar E$). The bound is what `S Result 3` tests: on the resampled totals, the bias is 0.000017 at 40 games against a bound of 0.00037, and 0.000009 at 160 against 0.00009. Games run to a mean of 66.2 plies with a coefficient of variation of 0.47, so $\operatorname{CV}(\bar M) \approx 0.47/\sqrt n$. The bias is at most 7% of a standard error at 40 games and 4% at 160.

**Result 4 (the floor test).** The lower end of (E4) exceeds $\delta$ if and only if $(\hat R - \delta)/\widehat{\mathrm{SE}} > t_{\gamma, n-1}$. The upper end is below $\delta$ if and only if $(\delta - \hat R)/\widehat{\mathrm{SE}} > t_{\gamma, n-1}$. If $(\hat R - \theta)/\widehat{\mathrm{SE}}$ has the $t_{n-1}$ law and $\theta = \delta$, each has probability $(1 - \gamma)/2$ exactly: 0.025 at 95% and 0.005 at 99%.

*Proof.* $\hat R - t\widehat{\mathrm{SE}} > \delta \iff \hat R - \delta > t\widehat{\mathrm{SE}}$, and $\widehat{\mathrm{SE}} > 0$. The second is symmetric. At $\theta = \delta$, $P(T_{n-1} > t_{\gamma, n-1}) = (1 - \gamma)/2$ by the definition of the quantile. $\blacksquare$ `R Result 4` only confirms the quantile's definition. The $t$ law holds only if the game residuals are normal, so it is a model, not a fact (Result 5).

**Result 5 (coverage, approximately).** Under (M2) with finite second moments, $(\hat R - \theta)/\widehat{\mathrm{SE}} \to N(0, 1)$ as $n \to \infty$, so (E4) covers $\theta$ with probability tending to $\gamma$. At 40 and 160 games of this experiment's skewed totals, the 95% interval covers 0.946 and 0.949 of the time. It misses low (lower end above $\theta$) 0.0205 and 0.0208 of the time, and high 0.034 and 0.030.

*Proof.* The central limit theorem for $(\bar E, \bar M)$, the delta method (Result 2) and Slutsky's theorem with $\widehat{\mathrm{SE}}/\operatorname{SD}(\hat R) \to 1$ give the limit; $t_{\gamma, n-1} \to z$. The finite-sample numbers come from `S Result 5`: 40,000 resamples of 40 or 160 games from the 160 committed totals of $\theta_\uparrow$, whose ratio is known. $\blacksquare$ The asymmetry matters for the rule. A wrong step up needs the lower end above the truth, and that miss is 0.021, under the nominal 0.025. A wrong step down needs the upper end below the truth, the side that misses more (0.034 at 95% here). These figures come from the step up's 160 totals at $\theta \approx 0.0246$, not from the step down's pilot residuals at its boundary. They are consistent with section 6's report that the step down's 95% rule let a boundary truth through 5% to 6% of the time, on 40 more skewed games, but they do not reproduce it: `power.json` holds no table for the 95% variant. The asymmetry is the reason the step down reads that end at 99%.

**Result 6 (the chi-square factor).** If the pilot's $n_p$ game residuals are independent $N(0, \sigma^2)$ with sample SD $s$, then $P(\sigma \le ks) = 0.80$ for $k = \sqrt{(n_p - 1)/\chi^2_{0.20, n_p - 1}}$. At $n_p = 20$, $k = 1.17697$. This is an 80% upper confidence bound on $\sigma/s$ under normality, and nothing more.

*Proof.* Under normality, $(n_p - 1)s^2/\sigma^2 \sim \chi^2_{n_p - 1}$ (Cochran's theorem). Then $\sigma \le ks \iff \sigma^2 \le \frac{(n_p - 1)s^2}{\chi^2_{0.20}} \iff \frac{(n_p - 1)s^2}{\sigma^2} \ge \chi^2_{0.20}$, whose probability is $1 - 0.20 = 0.80$. With $\chi^2_{0.20, 19} = 13.716$, $k^2 = 19/13.716 = 1.3853$. $\blacksquare$ Checked by `R Result 6` (the equivalence, $k$ against `power.json` to $10^{-12}$) and `S Result 6`: with normal residuals the coverage is 0.8000. The pilot's residuals are not normal, though. Resampling them, the factor covers the resampling population's SD only 0.65 of the time for the step down's residuals, and 0.81 for the step up's. A few games with several discordant moves give the step down a heavy right tail, and the chi-square bound assumes none. So "the 80% upper bound" is an 80% bound under normality. On the pilot's empirical distribution, resampled 20 at a time with the SD taken about each resample's own mean, it covers about 65% for the step down. That is a bootstrap of 20 points, so it is itself rough. Section 6's power is therefore less pessimistic than its label says. It does not change the conclusion, since no size was powered anyway (Result 8).

**Result 7 (what section 6's shift does).** With $r = \sum E_g/\sum M_g$ over the pilot's games and $a_g = \theta_* M_g + k(E_g - rM_g)$, the pilot population of $(a_g, M_g)$ has ratio exactly $\theta_*$ and residuals exactly $k$ times the pilot's. So each scenario has its stated contrast, and the residual SD is the pilot's times $k$.

*Proof.* $\sum a_g = \theta_*\sum M_g + k(\sum E_g - r\sum M_g) = \theta_*\sum M_g$, since the bracket is zero (Result 2). The residual is $a_g - \theta_* M_g = k(E_g - rM_g)$. $\blacksquare$ Checked by `R Result 7` and `S Result 7`. Section 6 was rewritten here from its text (draws with replacement, a step-down 40 and a deep 40 drawn from the $n$, the rule (E5)), with a different seed. It reproduces `power.json`'s outcome rates for both variances, at 40 and 160 games, in all five scenarios, within 4 combined Monte Carlo SE (worst 2.35).

**Result 8 (no size reaches 80%: a bound on the power that does not depend on $n$).** "300k falls short" needs $\ell'_\uparrow > \delta$ on 40 games, whatever $n$ is. So its probability is at most $P(\ell'_\uparrow > \delta)$, which depends only on the 40 deep games. Likewise "100k is enough" needs $u^{99}_\downarrow < \delta$ on 40 games. Under (M4) with the upper-bound variance, these bounds are 0.218 and 0.120, so no $n$ reaches 0.80. At $n = 40$ the deep 40 are the whole sample, so in the simulation the check is the same event as $\ell_\uparrow > \delta$ and adds nothing.

*Proof.* An outcome that needs an event $A$ has probability at most $P(A)$. Under (M4) the 40 deep games are a uniform draw of 40 of the $n$, each of which is a draw with replacement from the pilot's 20. So the 40 are themselves 40 independent draws from the pilot, with a law that does not depend on $n$. At $n = 40$, `rng.choice(40, 40)` is a permutation, the same games in another order, and $\hat R$ and (E3) are symmetric in the games. So $\ell'_\uparrow = \ell_\uparrow$ in the simulation, which takes the deep reference to agree with the 30-million one. $\blacksquare$ Checked by `S Result 8`. Every grid size's simulated rate is at most its bound within Monte Carlo error: the table's maxima are 0.233 against 0.218, and 0.125 against 0.120, each from 2,000 replicates. This is the registration's "the step down and the step up's check are read on 40 games whatever n is, so their power stops growing with n", as a theorem. The $n = 40$ case also explains Finding 4: at 40 games the step up's boundary error is that of one test, about 0.02, not of two.

**Result 9 (the rule's error control).** Suppose each one-sided miss has the probability Result 4 gives it: 0.025 for the lower 95% end and 0.005 for the upper 99% end. Then:

1. a wrong step up (rule 1 when $\theta_\uparrow \le \delta$) has probability at most 0.025, and the deep check can only lower it;
2. a wrong step down (rule 2 when $\theta_\downarrow \ge \delta$) has probability at most 0.005 nominally (section 6 simulates 0.014 to 0.0245);
3. either wrong move has probability at most their sum, with no assumption on their dependence;
4. testing 50,000 only after 100,000 passes (a fixed sequence) adds no error.

*Proof.* (1) $\{\ell_\uparrow > \delta\} \subseteq \{\ell_\uparrow > \theta_\uparrow\}$ when $\theta_\uparrow \le \delta$, a lower miss, of probability at most 0.025. Rule 1 needs this and $\ell'_\uparrow > \delta$, an intersection, and $P(A \cap B) \le P(A)$. (2) The same, with the upper 99% end and $\{u^{99}_\downarrow < \delta\} \subseteq \{u^{99}_\downarrow < \theta_\downarrow\}$. (3) The union bound: $P(A \cup B) \le P(A) + P(B)$. The two wrong moves are different outcomes of (E5), which are disjoint, so here the bound is an equality. With section 6's simulated rates, the sum is at most $0.025 + 0.025 = 0.05$, which is section 8's claim. For the step down, 0.025 is a simulated figure, not a guarantee. The 99% end promises 0.005, and on the pilot's skewed totals the simulation gives up to five times that. (4) Order the hypotheses 100,000 then 50,000. If 100,000's null is true, any error needs its false rejection, probability at most $\alpha$. If it is false and 50,000's is true, an error needs 50,000's false rejection, also at most $\alpha$. 50,000 is never tested unless 100,000 is rejected, so the first true null in the sequence must be rejected for any error, and the error is at most $\alpha$ in every case. $\blacksquare$

Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.union_bound`, for part (3): two events on one probability space, each of measure at most $\varepsilon = 0.025$, with no independence; its conclusion is $2\varepsilon$. The hypothesis "each at most 0.025" is what section 6 simulates, not a theorem. The fixed sequence (4) and the intersection (1) are elementary and have no Lean statement. Checked by `S Result 9`. It runs the rule (E5) with both contrasts at their boundaries at 160 games: the two wrong moves are disjoint outcomes, totalling 0.0028. It also runs a fixed sequence under four correlations and four margins for 100,000: the worst error is 0.027, within its Monte Carlo error of 0.025.

> **Finding 6 (preregistration sections 8 and 9: the follow-up's error).** Section 8 says either wrong move has "at most 0.05" chance, and rule 4 says a follow-up on new games "spends no error this one spent". A follow-up on new games, never pooled, has its own error at most $\alpha$ given that it runs, because its data are independent of the first look's. Its own error is intact in that sense. But the procedure as a whole, first look then a possible follow-up, can move away from 300,000 wrongly at either look. By the union bound its error is at most $\alpha_1 + \alpha_2$, up to $0.05 + 0.05 = 0.10$, not 0.05. This run decided at its first look, so it is unaffected. A registered follow-up should say which rate it controls.

**Result 10 (a reference that shares the deeper candidate's errors).** For every move,

```math
(d_A - d_B)_\rho - (d_A - d_B)_T = \big([L_B = L_\rho] - [L_B = T]\big) - \big([L_A = L_\rho] - [L_A = T]\big),
```

so the contrast against the reference equals the contrast against the truth plus $\pi_B - \pi_A$. Here $\pi_S = P(L_S = L_\rho) - P(L_S = T)$ is how much more often $S$ agrees with the reference than with the truth. The step up's contrast is inflated exactly when the 1-million search gains more from sharing the reference's errors than 300,000 does: $\pi_{1\text{M}} > \pi_{300\text{k}}$.

*Proof.* $[x \ne y] = 1 - [x = y]$, so $(d_A - d_B)_\rho = [L_B = L_\rho] - [L_A = L_\rho]$, and likewise against $T$. Subtract and regroup. Averaging over moves gives the population form. $\blacksquare$ Checked by `R Result 10` (all 256 label tuples). `S Result 10` only illustrates it: in a simulation where the reference copies the deeper candidate's label on 30% of that candidate's errors, the contrast is 0.077 against the reference and 0.029 against the truth. The identity is algebraic, and the inflation is built into that simulation. The registration's argument (section 2) is the sign condition $\pi_{1\text{M}} > \pi_{300\text{k}}$. It is plausible, because the reference's errors are a deeper search's, but it is not provable from the data, since $T$ is unknown. The deep check bears on it. Applied to $\rho$ and $\rho'$, the identity says the change from 30 to 100 million nodes is exactly the excess of 1 million's proximity to $\rho$ over its proximity to $\rho'$, less the same for 300,000. On the same 40 games, $\theta_\uparrow$ is 0.0252 against 30 million nodes and 0.0221 against 100 million, 12% smaller. Of the 192 discordant moves there, 12.0% changed their reference label. The paired change, $\sum_g (E^{30}_g - E^{100}_g)/\sum_g M_g = 8/2{,}624 = 0.0030$ with (E3)'s SE 0.0025 and 95% interval $[-0.0021, 0.0082]$, cannot be told from 0 on 40 games. So the data show a shrink in the direction section 2 predicted, of a size they cannot distinguish from none. The claim the decision makes, that 1 million agrees more with deeper searches, is what the identity allows. For the step up to exceed $\delta$ against the truth, $\theta_\uparrow - (\pi_{1\text{M}} - \pi_{300\text{k}}) > \delta$ would be needed, which no search can show. Checked by `N Result 10`.

**Result 11 (the absolute measures).** Under simple random sampling without replacement of the 100 concordant moves (seed `concordant/confirmatory`), $\hat D_{\text{abs}}(S)$ is unbiased for $D(S)$ over the 160 games' moves, for every setting. Whatever the sample, $\hat D_{\text{abs}}(300\text{k}) - \hat D_{\text{abs}}(1\text{M})$ equals the exact contrast $\hat R_\uparrow$.

*Proof.* The discordant moves form a census stratum with weight 1. The concordant stratum is sampled with inclusion probability $100/9{,}651$, so its weighted sum is the Horvitz-Thompson total, unbiased (EXP-0007's Result 1, applied to one stratum). The denominator $\sum_g M_g$ is fixed. On every concordant move, $L_{300\text{k}} = L_{1\text{M}}$ by the definition of the stratum, so $w(d_{300\text{k}} - d_{1\text{M}}) = 0$ there. The difference of the two absolute measures is then $\sum_{\text{discordant}}(d_{300\text{k}} - d_{1\text{M}})/\sum_g M_g = \hat R_\uparrow$. $\blacksquare$ `R Result 11` is definitional. The checks are `S Result 11` (unbiasedness and the stratum's variance $N^2(1 - f)S^2/n$) and `N Result 11`: $0.12284 - 0.09820 = 0.024632 = 261/10{,}596$, to $10^{-12}$. This is the review's "which confirms the code". The bootstrap intervals of (E6) resample games, while the concordant moves were sampled across games, so those intervals are approximate. They are exploratory and decide nothing.

**Result 12 (the cost and the ceiling).** The planned CPU-hours are linear in $n$, with slope 0.05424 hours a game and intercept 2.961 hours at the pilot's rates. The largest grid size within 12 hours is 160 (11.64; 180 needs 12.72). The step up's branch adds $40 \times 6.45 \times c_{\rho'} = 5.54$ hours, so the worst case is 17.18.

*Proof.* (E8) is affine in $n$: the candidates and the step up's reference positions grow with every game, while the step down's 40 games and the 200 concordant positions are fixed. Evaluate at the grid. $\blacksquare$ Checked by `R Result 12` (all 12 grid sizes against `power.json` `planned_cpu_hours`, to $10^{-9}$) and the `N` cost checks. The rates are 0.195, 21.15 and 77.33 CPU-seconds a position at 300,000, 30 million and 100 million nodes, the registration's 0.20, 21 and 77.

**Result 13 (the node budget).** Each unit counts the distinct positions searched times the setting's nodes, cached or not: 9,997 positions times 1.45 million candidate nodes ($1.4496 \times 10^{10}$, 97% of the budget), 1,797 times 30 million ($5.391 \times 10^{10}$, 115%), 336 times 100 million ($3.36 \times 10^{10}$, 129%), and 12 positions of each of six settings ($1.5774 \times 10^{9}$, 99%). Every unit is under the 150% cap.

*Proof.* `searches` spends `nodes × len(distinct FENs)` before searching, and `recheck` spends `nodes × 12`. The products are exact integers. $\blacksquare$ Checked by `N compute`.

> **Finding 1 (decision.md: "0.86 a player and game").** `decision.md` says that at 1 million nodes "7.45% of moves change their label, 95% [6.75%, 8.14%], about five of a game's 66 moves, 0.86 a player and game". Five a game is right ($0.0745 \times 66.2 = 4.93$). But a player makes 33.1 moves a game, so 7.45% of them is 2.47 changed labels per player and game, not 0.86. 0.86 matches $\hat R_\uparrow \times 35 = 0.0246 \times 35 = 0.862$, which is likely its origin: the net gain in agreement with the reference, at section 5's "about 35 moves" for a player's game. At this sample's 33.1 moves a player it is 0.82. `review.md`'s "the gain is about 0.86 labels a player and game" uses it in that sense. The decision is not affected. The sentence mixes the two quantities: labels that change (2.47 a player and game) and net agreement gained (0.82, or 0.86 at 35 moves). Checked by `N Finding 1`.

> **Finding 2 (summary.md: the deep check's lower end).** `summary.md` gives the deep check as "2.2 fewer [1.2, 3.3]" moves in 100. Its lower end is 0.011487 (`results.json` `primary["n300k-n1m-deep"].ci95[0]`), 1.149 in 100, which rounds to 1.1. `decision.md`'s 0.0115 is right. The margin above the tolerance is 0.0015 (0.15 in 100), so a rounding that adds 0.05 overstates it by a third. Checked by `N Finding 2`.

> **Finding 3 (preregistration section 7: "120 of 5,398").** Section 7 says the player rule "drops a few games, 120 of 5,398 in the population's order". Reproducing the seeded order from `population.csv` (seed `sample/order`, `math_check.py` `accepted`, which gives the pilot's 20 and the sample's 160 exactly), the rule drops 2,291 of the 5,398 over the whole order, and keeps 3,107. Before the 180th kept game, the last one sampled, it has dropped 5 of 185. No prefix of the order drops exactly 120 (the count passes from 119 to 121 at the 799th kept game). The approximation section 7 needs holds better than stated: the 180 sampled games are the first 180 of 185 in a uniform random order. The 5 that were skipped shared a player with an earlier game. Checked by `N sample` and `N Finding 3` (2,291; 5 by the 180th; 3,107 kept; 119 then 121 at the 799th).

> **Finding 4 (preregistration section 6: the step up's wrong-move rate).** Section 6 says the rate of a wrong move at the boundaries is "0.002 to 0.004 for the step up, which needs both references". In `power.json` (`table[*]["boundary 1M"]["300k falls short"]`) it is 0.001 to 0.0205 over the grid: 0.0205 and 0.016 at 40 games, 0.005 to 0.01 at 60 and 80, and 0.001 to 0.0045 from 100 games on. At 40 games the deep 40 are the whole sample (Result 8), so the second reference adds nothing in the simulation. The headline "at most 0.025 for both steps at every size" holds. The step down's range is 0.014 to 0.0245 (quoted 0.015 to 0.025, within rounding). At the run's 160 games the step up's rate is 0.0035 with both variances, so the run is unaffected. Checked by `N Finding 4`.

> **Finding 5 (sections 0 and 11, decision.md: two budget figures).** Section 0's plan of about 1,536 reference positions is $160 \times 6.45 + 40 \times 7.6 + 200 = 1{,}536$ (`power.json`). At 30 million nodes that is $4.61 \times 10^{10}$, 46 billion, but `BUDGET` registers 47 billion. The cap is generous by 2% on that unit, which the run did not need (it spent 115%). Section 0 gives the worst case as 17.2 CPU-hours (11.64 + 5.54 = 17.18). Section 11, `decision.md` and `review.md` give it as 17.1. Neither changes anything. Checked by `N Finding 5`.

## 5. Numbers

Every number below is checked by `math_check.py` `numbers`. The deciding contrasts are recomputed from the per-game totals in `results.json` `totals`, which are in `sample.csv`'s order. The sample itself is recomputed from `population.csv` and the seed. The per-game CPU seconds (4.0, 11.8, 39.1), the 19.5 CPU-hours and the pilot's "about 6.7 CPU-hours over three runs" come from the cache's timings, which are not committed. They are checked only for their arithmetic. The pilot's own searches in `pilot/pilot.json` `cpu_hours` sum to 6.52. Section 0's 47 billion reference nodes is Finding 5. Not checkable from the committed files: the 133 and 4 minutes and 0.35 CPU-hours of the runs; the post-hoc hybrid's 15.9%, 22 seconds, 0.0147 [0.0111, 0.0183] and 0.0099 [0.0059, 0.0139] (computed from the cache, not in `results.json`); `review.md`'s "3 million still disagreed with the reference on about 7%", "84 of 361 reference positions" and the critic's 0.0271; and section 0's "the 50,000 branch costs about 1" CPU-hour. The checkable small figures are checked: 290 table files, about 10,190 positions ($160 \times 63.7$), about 258 deep positions ($40 \times 6.45$), about 70 minutes on 10 engines (69.8), and section 5's "one game in three" ($0.01 \times 35 = 0.35$).

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| pilot 20 games, sample 160, no player twice | `accepted` | `sample-pilot.csv`, `sample.csv` | reproduced exactly |
| 160 games, 10,596 moves, 9,997 positions | sums | `results.json` `games`, `moves`, `positions` | as quoted |
| the step-down 40 and the deep 40 | seeds `step-down/confirmatory`, `deep/confirmatory` | `step_down_games`, `totals[*].m` | reproduced |
| step up 0.0246 [0.0197, 0.0296], 80% [0.0214, 0.0279] | (E2) to (E4) | `totals["n300k-n1m"]` | 261/10,596 = 0.024632 |
| deep 0.0221 [0.0115, 0.0327], 80% [0.0153, 0.0289] | same | `totals["n300k-n1m-deep"]` | 58/2,624 = 0.022104 |
| step down 0.0122 [0.0014, 0.0230], 99% [−0.0022, 0.0267] | same | `totals["n100k-n300k"]` | 34/2,781 = 0.012226 |
| every contrast's SE and 80/95/99% ends, run and pilot | (E3), (E4) | `primary` | all to $10^{-12}$ |
| summary: 2.5 [2.0, 3.0]; 2.2 [1.2, 3.3] | ×100 | same | 1.2 is 1.149 (Finding 2) |
| rule 1 holds; deep margin 0.0015 | (E5) | `decision` | 0.00149 |
| step down neither within nor worse | (E5) | – | 99% upper 0.0267 ≥ 0.01; 95% lower 0.0014 ≤ 0.01 |
| on the deep 40: 0.0252 against 30M, 12% smaller against 100M; 12.0% of 192 changed | Result 10 | `totals`, `primary["n300k-n1m-deep"]` | 0.02515; 0.121; 0.1198 |
| by band 0.034, 0.019, 0.024, 0.023 with intervals and games 47, 59, 44, 10 | (E2) to (E4) per band of `sample.csv` `mean_elo` | `by_band` | all at 3 dp |
| absolute 300k − 1M = exact contrast | Result 11 | `absolute_disagreement` | 0.024632 |
| 96.5 per concordant move; 945 discordant | (E6) | `concordant_moves`, `discordant_moves` | 96.51; 945 |
| 7.45% [6.75%, 8.14%] change; summary's 7.4%; five of 66; pilot's 5.3% | (E2) on `with_production` | `with_production.n1m` | 0.07446; 4.93 a game; 0.0528 |
| 0.86 a player and game | – | – | Finding 1 |
| 2.59, 1.51, 2.75 against 2.56, 1.46, 3.04 | counts | `labels_per_player_game` | as quoted |
| accuracy −0.42 [−0.62, −0.22]; 1.06 [0.93, 1.19]; 45% [39%, 51%] | means | `accuracy.n1m` | as quoted |
| 12.3% [7.9, 17.5], 9.8% [5.7, 14.6]; best move 80.7%, 83.8%; win% 2.3, 1.9; 100k 12.6%; Lichess 9.9% [9.1, 10.8], 12.3% [7.4, 18.0]; 1M about 10% | (E6) | `absolute_disagreement`, `best_move_agreement`, `win_percent_error`, `with_production.lichess` | as quoted |
| 62.5 positions a game; 3.3 times; 15 and 49 minutes for 300 games on 4 engines | arithmetic on the quoted seconds | – | 62.48; 3.31; 14.8 and 48.9 |
| 0.20, 21, 77 CPU-seconds a position | `power.json` `cpu_hours_per_position` × 3,600 | – | 0.195, 21.15, 77.33 |
| plan 11.6 CPU-hours at 160, branch 5.5, worst 17.2; no size powered | Result 12 | `planned_cpu_hours`, `branch_cpu_hours`, `powered_n` | 11.64; 5.54; 17.18; null |
| nodes 97%, 115%, 129%, 99% of budget | Result 13 | `compute` | as quoted |
| 72 re-searches identical | – | `rechecks` | 6 × 12 |
| power at 160: 0.20 (0.30), 0.09 (0.13), 0.09 (0.13) | (E5) under (M4) | `power.json` `table` | as quoted |
| boundary rates ≤ 0.025; step down 0.015 to 0.025; step up 0.002 to 0.004 | same | `table[*]["boundary …"]` | 0.014 to 0.0245; **0.001 to 0.0205** (Finding 4) |
| $k = 1.18$ | Result 6 | `k_upper80` | 1.17697 |
| pilot: 0.032 [0.020, 0.045]; 0.0099 [0.0001, 0.0197]; deep 0.0099 [0.0002, 0.0197]; 14.5% of 69; D(1M) − D(3M) 0.019; D(50k) − D(300k) 0.039 | (E2) to (E4) | `pilot/pilot.json` | as quoted |
| 120 of 5,398 dropped | `accepted` | `population.csv` | **2,291** over the order, 5 before the 180th kept (Finding 3) |

## 6. Assumptions that can fail

- **Determinism of the searches** (M1). Results 1 and 11 need a label to be a function of the positions. The 72 identical re-searches cover it for this build. A different Stockfish build or engine crate changes the cache key, and the run searches again.
- **Independent games** (M2). No shared player, but games share openings, so they are slightly dependent. The sample is close to a uniform random draw (Finding 3). Positive dependence would make (E3) too small. Nothing measures it.
- **Normal residuals.** The $t$ interval (Results 4 and 5) and the chi-square factor (Result 6) assume them. The game totals are skewed. Resampling the committed totals, the 95% interval's lower miss is 0.021 and its upper miss 0.030 to 0.034. The chi-square factor covers 0.65 of the time, not 0.80, for the step down's pilot residuals. The step down's 99% reading is the guard for the side that misses more. The step up, which decided, is read on the side that misses less.
- **The reference is not the truth** (Result 10). The step up's sign against the truth needs $\pi_{1\text{M}} - \pi_{300\text{k}} < \theta_\uparrow - \delta$, which cannot be checked. The deep check is the only guard: on the same 40 games the contrast shrank 12%, and its lower end is 0.0015 above the tolerance. A reference deeper again could move it below. EXP-0028 measures convergence.
- **The power model** (M4). It holds the residual spread fixed as the scenario's contrast changes, though the spread grows with the share of discordant moves. It draws the deep 40 and the step-down 40 independently, so they can overlap, as in the run. It resamples 20 pilot games, takes the deep check to agree with the reference, and uses a factor that is not 80% for these residuals. The run's contrasts fell outside the pilot's intervals (section 11). The power figures are of the model, and no decision used them except the ceiling's choice of $n$.
- **The estimand.** $\theta$ is over Lichess rapid games with a server analysis, rated 800 to 2199. Nothing here transfers it to chess.com games or to the ranked weaknesses (EXP-0027).
- **The exploratory absolute measures** (Result 11). They are unbiased, but for 50,000, 100,000 and Lichess they rest on 100 sampled moves, and their bootstrap ignores that the moves were sampled across games. They decide nothing.
- **The follow-up** (Finding 6). Its error is its own, and the two looks' errors add, up to 0.10.

### Verification

**Blind re-derivation.** A fresh agent was given `preregistration.md`, `analysis.py`, `results.json`, `power.json`, the pilot, `population.csv` and the samples, but not this file, `decision.md`, `summary.md` or `review.md`. It re-derived five results: the per-move identity and the reference-bias identity (Results 1 and 10); the ratio's bias identity, Hartley-Ross bound and delta-method variance (Results 2 and 3); the chi-square factor, $k = 1.17697$, and what the power simulation's shift does (Results 6 and 7); the error control, with the fixed sequence and the power bound that does not depend on $n$ (Results 8 and 9), simulating the bounds at 0.218 and 0.112 to 0.120; and the absolute measures, with $0.12284 - 0.09820 = 0.024632$ (Result 11). Every formula agreed. It recomputed the three contrasts to every digit, reproduced the sample from the seed, and found Findings 3 and 4 independently. Three changes came from it. The bias size in Result 3 uses the plies' CV of 0.47, not the draft's 0.4. Result 9 now says the two wrong moves are disjoint outcomes, and that the step down's 0.025 is simulated, not guaranteed. The pilot's 6.52 CPU-hours and the 47-billion budget figure are now in section 5.

`math_check.py`'s first run failed four checks, and the maths was corrected, not the tolerances. The "120 of 5,398" did not reproduce (Finding 3). The summary's 1.2 is 1.149 (Finding 2). The step up's boundary range is wider than quoted (Finding 4). The chi-square factor's coverage on the pilot's own residuals is 0.65, not the 0.70 the draft guessed.

**Referee.** A second fresh agent reviewed this file and `math_check.py` as a hostile referee and made 14 points. All were accepted:

1. **Major.** Finding 4 misquoted 60 and 80 games (0.005 to 0.01, not 0.009 to 0.01). Every value it quotes, including 0.0035 at 160, is now asserted.
2. **Major.** Result 5 claimed to explain section 6's 5% to 6%. It now says "consistent with", and states that `power.json` holds no 95% table.
3. **Major.** `S Result 5` now asserts the quoted misses and the asymmetry, lower miss < 0.025 < upper miss.
4. **Major.** The 12% shrink had no uncertainty. Result 10 now gives the paired change, 0.0030, SE 0.0025, 95% $[-0.0021, 0.0082]$, which cannot be told from 0. `N Result 10` checks it.
5. **Major.** The 47 billion is now Finding 5, not "rounded up".
6. **Minor.** "17.1 with the branch" (sections 11, `decision.md`, `review.md`) against 17.18 is in Finding 5.
7. **Minor.** The checkable small numbers are now checked: 290 tables, 10,190, 258, about 70 minutes, one game in three. The rest are listed in section 5 as not checkable from the committed files.
8. **Minor.** `N Finding 3` asserts 5, 3,107 and the 119-to-121 step, not "not 120".
9. **Minor.** Definitional checks are labelled as such (`R Results 3, 4, 11`, part of `S Result 3`, `S Result 10`). `S Result 9` now runs the rule (E5) itself at both boundaries, and a fixed sequence with varying margins.
10. **Minor.** Result 6's 65% is described as a 20-point bootstrap on the pilot's empirical distribution.
11. **Minor.** Result 10's last sentence now says what exceeding $\delta$ against the truth needs.
12. **Minor.** Finding 1 says 0.86 "matches" the contrast times 35, the likely origin.
13. **Minor.** The remark on the follow-up is now Finding 6: the procedure's error is up to 0.10 over two looks.
14. **Minor.** Text fixes: (E5) in the docstring, the comment in `R Result 1`, the mean of 66.2 plies, and 21.15 CPU-seconds.

After these changes `math_check.py` runs 56 checks, all PASS.
