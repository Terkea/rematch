# The lab's machine-checked lemmas

A few results carry most of the lab's experiments: Holm's and Bonferroni's error control, Beta-binomial conjugacy, the rating-memory luck share and the window-mean covariance. Each one has a prose proof in some `math.md`. Here each one also has a proof in Lean 4 with mathlib, which `lake build` checks line by line (ADR 0118, decision 3, and its Amendment 1).

## What is proved

Every theorem is in the namespace `Rematch`. There is no `sorry`, and no axiom beyond mathlib's standard three (`propext`, `Classical.choice`, `Quot.sound`). `RematchTest/Axioms.lean` asserts that during the build.

| Theorem | File | In words |
|---|---|---|
| `Rematch.holm_fwer` | `Rematch/Multiplicity.lean` | Holm's step-down procedure at level α rejects at least one true null with probability at most α, under any dependence between the p-values. "Valid" means `P(p_i ≤ t) ≤ t` for each true null and every `t ≥ 0`. |
| `Rematch.holm_rejects_true_null` | `Rematch/Multiplicity.lean` | The deterministic core of that proof. If Holm rejects a true null, the smallest of the m₀ true-null p-values is at most α / m₀. |
| `Rematch.union_bound` | `Rematch/Multiplicity.lean` | If each of n events has probability at most ε, the chance that any of them happens is at most n ε. |
| `Rematch.bonferroni` | `Rematch/Multiplicity.lean` | If each event in a family of m has probability at most α / m, the chance that any of them happens is at most α. This covers tests at α / m and simultaneous `1 − α / m` intervals (EXP-0002's family of 20). |
| `Rematch.bonferroni_fwer` | `Rematch/Multiplicity.lean` | The same bound for tests: rejecting when `p_i ≤ α / m`, with valid p-values for the true nulls, rejects a true null with probability at most α. |
| `Rematch.beta_binomial_posterior` | `Rematch/BetaBinomial.lean` | A Beta(a, b) prior density times the binomial likelihood of k successes in n trials is, on (0, 1), a positive constant times the Beta(a + k, b + n − k) density. The posterior mean is (a + k) / (a + b + n). |
| `Rematch.luck_share` | `Rematch/LuckShare.lean` | For 0 < φ < 1, EXP-0029's share `s = φ^g_A (φ^(2 g_J) − 1)` (preregistration, section 4) lies in (−1, 0]. Its size never grows with April's games g_A, and it is 0 exactly when g_J = 0. EXP-0009's share `φ^g − 1` also lies in (−1, 0]. |
| `Rematch.window_mean_independence` | `Rematch/WindowMean.lean` | Take `q₁ = θ + e₁` and `q₂ = θ + δ + e₂`, with e₁ and e₂ square-integrable and uncorrelated. If their variances are equal, the change `q₂ − q₁` and the level's error `(q₁ + q₂)/2 − θ − δ/2` have covariance 0. With variances σ₁² and σ₂², the covariance is (σ₂² − σ₁²)/2. |

The building blocks are proved and checked as well: `beta_add_one_left`, the recurrence B(a + 1, b) = a / (a + b) · B(a, b); `integral_betaPDFReal`, which says the Beta density integrates to one; `integral_id_mul_betaPDFReal`, which says the Beta mean is a / (a + b); `window_mean_covariance`; and the four parts of `luck_share`.

Some notes on the statements:

- **Holm** is stated without sorting. `H_i` is rejected when every `j` with `p_j ≤ p_i` passes `p_j ≤ α / #{l : p_j ≤ p_l}`. For the first p-value in a run of ties at rank k, that count is `m − k + 1`, the run's strictest threshold. So this rejects the same hypotheses as the sorted procedure. The probability is a measure on any measurable space, and the p-values need not be measurable, because the bound uses outer measure only.
- **Beta-binomial** uses mathlib's `ProbabilityTheory.beta` (Γ(a)Γ(b)/Γ(a+b)) and `betaPDFReal` as they stand. The mean follows from `lintegral_betaPDF_eq_one` and `Real.Gamma_add_one`.
- **Window means**: θ and δ are constants, for one player. Mean-zero noise is not needed for the covariance, so it is not assumed.

## How to build

Everything runs in Docker, as the rest of the lab does.

```sh
docker build -t rematch-lean:local lab/lean
```

The build installs elan v4.3.0 (checksum-verified) and the Lean toolchain named in `lean-toolchain`, then fetches mathlib's prebuilt oleans with `lake exe cache get` and runs `lake build`. A broken proof, a `sorry` or a new axiom fails the build. To iterate without rebuilding the image, keep `.lake` in a volume:

```sh
docker build --target toolchain -t rematch-lean:toolchain lab/lean
docker run --rm -v "$PWD/lab/lean:/lean" -v rematch-lean-dev:/lean/.lake rematch-lean:toolchain \
  sh -c "lake exe cache get && lake build"
```

CI (`.github/workflows/lean.yml`) runs the second command on every pull request that touches `lab/lean/**`, on pushes to main and every Sunday. It caches `.lake/packages` on the manifest and the toolchain.

Measured on the maintainer's machine on 2026-11-28: `docker build` took 386 s with the toolchain stage already cached (that stage takes about 90 s cold). Of that, `lake exe cache get` took 102 s for 9,117 files, `lake build` of the Rematch sources and the axiom check took 14 s, and writing the image took 267 s. `rematch-lean:local` is 15.4 GB on disk (3.6 GB compressed) and `rematch-lean:toolchain` is 4.4 GB. Most of the size is mathlib's oleans. The image is a build check, not something to ship.

## Pins

| What | Pin | Date |
|---|---|---|
| Lean | `leanprover/lean4:v4.36.0` | released 2026-11-16 |
| mathlib | tag `v4.36.0`, commit `fc4f59e577feb1a4956671bc04ac18aaaf8cd842` | 2026-11-16 |
| mathlib's dependencies | the commits in `lake-manifest.json`, as mathlib's own manifest pins them | |
| elan | v4.3.0, sha256 `0c71472f…9adab2` | released 2026-11-17 |
| Base image | `debian:bookworm-slim` by digest (in the `Dockerfile`) | |

Each one was at least 7 days old when it was pinned, the lab's rule for third-party code. To move to a newer mathlib, change `rev` in `lakefile.toml` to a dated `v4.x.0` tag that is at least 7 days old. Set `lean-toolchain` to that tag's own toolchain, run `lake update` in the toolchain image, and commit the new manifest.

## How a math.md cites a theorem

Put the citation beside the prose proof, on its own line:

> Machine-checked: `lab/lean/Rematch/Multiplicity.lean` theorem `Rematch.holm_fwer`.

Cite only a theorem listed above, and check that its hypotheses match the experiment's model. For example, `window_mean_independence` needs uncorrelated noise. If the model needs more than the theorem states, add a theorem here (with its line in `RematchTest/Axioms.lean`) before citing it.
