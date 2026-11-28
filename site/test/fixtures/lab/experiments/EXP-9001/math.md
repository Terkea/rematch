# EXP-9001: the maths

<!-- A fixture: a made-up experiment's maths, in GitHub's syntax. -->

## 1. Notation

| Symbol | Meaning | Source |
|---|---|---|
| $n$ | games in the sample | `analysis.py` `load` |
| $y_i$ | 1 if game $i$ was won | `results.json` |

A `<placeholder>` in prose stays text, and so does <word>.

## 2. Model

```math
y_i \mid p \overset{\text{iid}}{\sim} \operatorname{Bernoulli}(p), \qquad i = 1, \dots, n.
```

## 3. Estimators

The estimate is $\hat p = \frac{1}{n} \sum_i y_i$, and $0 < \hat p < 1$ once a game is won and one is lost; the pair $\langle a, b \rangle$ is ordered.

## 4. Derivations and proofs

**Result 1.** $E[\hat p] = p$.

*Proof.* By linearity,

$$
E[\hat p] = \frac{1}{n} \sum_{i=1}^{n} E[y_i] = p.
$$

$\blacksquare$ See [the template](../../../docs/experiments/MATH-TEMPLATE.md).

## 5. Numbers

`rate` = 0.53 in results.json.

## 6. Assumptions that can fail

Independent games.
