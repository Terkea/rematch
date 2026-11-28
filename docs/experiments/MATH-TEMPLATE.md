# EXP-NNNN: the maths

<!--
The maths of one experiment (ADR 0118). Copy this file to lab/experiments/EXP-NNNN/math.md and
lab/templates/math_check.py to lab/experiments/EXP-NNNN/math_check.py. Keep the six headings
exactly as they are: `python -m rematch_lab.experiments check` looks for them by their text
(`MATH_SECTIONS` in lab/rematch_lab/experiments.py).

When each section is due:
- sections 1 to 4 (notation, model, estimators, proofs) before the freeze, in a commit before the
  `exp-nnnn-freeze` tag and before any result, so the maths is fixed with the analysis it
  describes;
- sections 5 and 6 (numbers, assumptions) with the decision, once results.json exists.
math.md is a separate file from preregistration.md: the freeze check locks sections 0 to 10 of
the pre-registration, and a correction to the maths after the results is a dated note at the
end of section 5 or 6, never a silent edit of a proof the decision used.

How to write the maths. One source renders on GitHub and on the site (remark-math and KaTeX):
- inline maths between single dollars: $\hat p = \bar y$;
- display maths in a fenced block whose language is `math` (shown below). Prefer it to $$...$$:
  GitHub renders it reliably, and nothing in it is read as Markdown;
- no HTML and no unicode maths in place of TeX: write `\sigma^2`, not σ².

Every result is numbered (Result 1, Result 2, ...), stated in one sentence and a formula, and
followed by a full proof that ends with $\blacksquare$. A proof that relies on a standard result
names it (the law of total variance, Holm 1979) and says why its conditions hold here. Where a
shared lemma has a Lean proof in lab/lean/, the result names the theorem
(`Rematch.Holm.fwer_le_alpha`), and the prose proof stays.

How math_check.py verifies it, in the lab image (docs/evaluation.md):
1. Symbolic: each derivation's two sides, subtracted, simplify to 0 with sympy. An identity,
   bound or limit that sympy cannot settle is checked on a dense numeric grid instead, and the
   script says which.
2. Simulation: each proof's model, simulated from a fixed seed with a known truth. The bias,
   variance, interval coverage, power and error rates that come out must match the derived
   formulas within the simulation's own standard error (4 SE). This catches a proof that is
   right about the wrong model.
3. Numbers: every number in decision.md, summary.md and the README, recomputed from the formulas
   of section 4 and the committed results.json and power.json, within the stated rounding.
4. Lean: a result with a theorem in lab/lean/ names it; `lake build` proves it.
A fifth check is done by people: a fresh critic, given the experiment's files but not math.md,
derives the key results blind, and review.md compares the two.
-->

## 1. Notation

<!--
Every symbol, its unit and where it comes from (the file, the column, the function). A table:

| Symbol | Meaning | Unit | Source |
|---|---|---|---|
| $n$ | rated games in the window | games | `analysis.py` `load_games` |
| $y_i$ | 1 if game $i$ was won, else 0 | — | `results.json` `games[].won` |

Index sets, the population and the sample are named here, so no later section introduces a
symbol it does not define.
-->

## 2. Model

<!--
The data-generating assumptions the proofs rely on, as maths: the binomial rate, the Beta prior,
the rating's memory $\varphi^g$, the regression with HC3 errors. Say what is fixed and what is
random, and which draws are independent. Example:

```math
y_i \mid p \overset{\text{iid}}{\sim} \operatorname{Bernoulli}(p), \qquad i = 1, \dots, n.
```
-->

## 3. Estimators

<!--
Each registered estimator, test statistic and interval as a formula, with the function in
analysis.py (or power.py) that computes it: "`analysis.py` `win_rate`". The formula here and the
code must agree term by term; math_check.py's numbers check is where they meet. Example:

```math
\hat p = \frac{1}{n} \sum_{i=1}^{n} y_i, \qquad
\hat p \pm z_{0.975} \sqrt{\frac{\hat p (1 - \hat p)}{n}}.
```
-->

## 4. Derivations and proofs

<!--
Each result numbered, stated, then proved in full. Typical results: unbiasedness or the bias
term; the variance and its standard error; the power and the minimum detectable effect, built
from their parts; the coverage of each interval; the family-wise error rate of the correction
(Holm); the decision rule's error rates. Example:

**Result 1.** $\hat p$ is unbiased: $E[\hat p] = p$.

*Proof.* By linearity of expectation, $E[\hat p] = \frac{1}{n} \sum_i E[y_i]$, and
$E[y_i] = 1 \cdot p + 0 \cdot (1 - p) = p$ under section 2. $\blacksquare$

**Result 2.** $\operatorname{Var}[\hat p] = p (1 - p) / n$.

*Proof.* The $y_i$ are independent, so their variances add:

```math
\operatorname{Var}[\hat p] = \frac{1}{n^2} \sum_{i=1}^{n} \operatorname{Var}[y_i]
  = \frac{n \, p (1 - p)}{n^2} = \frac{p (1 - p)}{n}.
```

$\blacksquare$ Checked by `math_check.py` `symbolic` and `simulation`.
-->

## 5. Numbers

<!--
Due with the decision. Each number the decision, the summary or the README quotes, recomputed
from the formulas of section 4, with the key it has in results.json or power.json:

| Quoted as | Formula | Key | Value |
|---|---|---|---|
| win rate 0.53 | Result 1 | `results.json` `rate` | 0.53 |
| 95% interval 0.481 to 0.579 | section 3 | `results.json` `interval` | [0.4811, 0.5789] |

math_check.py `numbers` asserts every row.
-->

## 6. Assumptions that can fail

<!--
Due with the decision. For each proof: the assumptions it needs (independence of games, equal
variances, a stationary rating, no truncation), what breaks if one fails, how large the error
can be, and which registered sensitivity analysis or guard covers it. An assumption no
sensitivity covers is named as such, not left out.
-->
