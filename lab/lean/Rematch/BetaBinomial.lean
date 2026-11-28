import Mathlib.Probability.Distributions.Beta

/-!
# Beta-binomial conjugacy

A rate `x ∈ (0, 1)` with a `Beta(a, b)` prior, and `k` successes in `n` trials. The posterior is
`Beta(a + k, b + n − k)`, and its mean is `(a + k) / (a + b + n)`.

Mathlib's `ProbabilityTheory.beta a b = Γ(a) Γ(b) / Γ(a + b)` and `betaPDFReal a b x`, the
density `x ^ (a − 1) (1 − x) ^ (b − 1) / beta a b` on `(0, 1)` (zero elsewhere), are used as
they stand. The lemmas relied on: `lintegral_betaPDF_eq_one` (the density integrates to one),
`beta_pos`, `Real.Gamma_add_one` (`Γ(s + 1) = s Γ(s)`, which gives the recurrence
`B(a + 1, b) = a / (a + b) · B(a, b)`), `Real.rpow_add`, `Real.rpow_natCast` and
`integral_eq_lintegral_of_nonneg_ae`.
-/

open MeasureTheory ProbabilityTheory Set

namespace Rematch

/-- The binomial likelihood of `k` successes in `n` trials at rate `x`. -/
noncomputable def binomialLik (n k : ℕ) (x : ℝ) : ℝ :=
  (n.choose k : ℝ) * x ^ k * (1 - x) ^ (n - k)

/-- The Beta-function recurrence `B(a + 1, b) = a / (a + b) · B(a, b)`, from `Γ(s + 1) = s Γ(s)`. -/
theorem beta_add_one_left {a b : ℝ} (ha : 0 < a) (hb : 0 < b) :
    beta (a + 1) b = a / (a + b) * beta a b := by
  have hab : 0 < a + b := add_pos ha hb
  have h1 : a + 1 + b = (a + b) + 1 := by ring
  simp only [beta]
  rw [h1, Real.Gamma_add_one ha.ne', Real.Gamma_add_one hab.ne']
  have : Real.Gamma (a + b) ≠ 0 := (Real.Gamma_pos_of_pos hab).ne'
  field_simp

/-- The Beta density integrates to one over the line (from mathlib's `lintegral_betaPDF_eq_one`). -/
theorem integral_betaPDFReal {a b : ℝ} (ha : 0 < a) (hb : 0 < b) :
    ∫ x, betaPDFReal a b x = 1 := by
  have hnn : ∀ x, 0 ≤ betaPDFReal a b x := by
    intro x
    by_cases hx : 0 < x ∧ x < 1
    · exact (betaPDFReal_pos hx.1 hx.2 ha hb).le
    · simp [betaPDFReal, hx]
  rw [integral_eq_lintegral_of_nonneg_ae (Filter.Eventually.of_forall hnn)
    (stronglyMeasurable_betaPDFReal a b).aestronglyMeasurable]
  have : ∫⁻ x, ENNReal.ofReal (betaPDFReal a b x) = ∫⁻ x, betaPDF a b x := rfl
  rw [this, lintegral_betaPDF_eq_one ha hb, ENNReal.toReal_one]

/-- `x` times the `Beta(a, b)` density is `a / (a + b)` times the `Beta(a + 1, b)` density. -/
theorem mul_betaPDFReal {a b : ℝ} (ha : 0 < a) (hb : 0 < b) (x : ℝ) :
    x * betaPDFReal a b x = a / (a + b) * betaPDFReal (a + 1) b x := by
  by_cases hx : 0 < x ∧ x < 1
  · simp only [betaPDFReal, ite_eq_left hx]
    rw [beta_add_one_left ha hb, add_sub_cancel_right]
    have hB : beta a b ≠ 0 := (beta_pos ha hb).ne'
    have hab : a + b ≠ 0 := (add_pos ha hb).ne'
    have hpow : x ^ a = x ^ (a - 1) * x := by
      rw [← Real.rpow_add_one hx.1.ne', sub_add_cancel]
    rw [hpow]
    field_simp
  · simp [betaPDFReal, hx]

/-- The mean of `Beta(a, b)` is `a / (a + b)`. -/
theorem integral_id_mul_betaPDFReal {a b : ℝ} (ha : 0 < a) (hb : 0 < b) :
    ∫ x, x * betaPDFReal a b x = a / (a + b) := by
  simp_rw [mul_betaPDFReal ha hb]
  rw [integral_const_mul, integral_betaPDFReal (by linarith) hb, mul_one]

/-- **Beta-binomial conjugacy.** For `a, b > 0` and `k ≤ n`, on `(0, 1)` the `Beta(a, b)` prior
density times the binomial likelihood of `k` successes in `n` trials equals a positive constant
`C` times the `Beta(a + k, b + n − k)` density, so the posterior is `Beta(a + k, b + n − k)`;
and the posterior mean is `(a + k) / (a + b + n)`. The constant is
`C = (n choose k) · B(a + k, b + n − k) / B(a, b)`. A lab model with a Beta prior on a rate (a
puzzle success rate, a share of games) updates by adding successes to `a` and failures to `b`,
and reports `(a + k) / (a + b + n)` as the posterior mean; this is the proof that both are
exact. -/
theorem beta_binomial_posterior {a b : ℝ} (ha : 0 < a) (hb : 0 < b) {n k : ℕ} (hk : k ≤ n) :
    (∃ C : ℝ, 0 < C ∧ ∀ x ∈ Ioo (0 : ℝ) 1,
      betaPDFReal a b x * binomialLik n k x = C * betaPDFReal (a + k) (b + (n - k : ℕ)) x) ∧
    ∫ x, x * betaPDFReal (a + k) (b + (n - k : ℕ)) x = (a + k) / (a + b + n) := by
  have ha' : 0 < a + k := by positivity
  have hb' : 0 < b + ((n - k : ℕ) : ℝ) := by positivity
  refine ⟨⟨(n.choose k : ℝ) * beta (a + k) (b + (n - k : ℕ)) / beta a b, ?_, ?_⟩, ?_⟩
  · have := Nat.choose_pos hk
    have := beta_pos ha hb
    have := beta_pos ha' hb'
    positivity
  · rintro x ⟨hx0, hx1⟩
    have hx : 0 < x ∧ x < 1 := ⟨hx0, hx1⟩
    have h1x : 0 < 1 - x := by linarith
    simp only [betaPDFReal, binomialLik, ite_eq_left hx]
    have e1 : x ^ (a + k - 1) = x ^ (a - 1) * x ^ k := by
      rw [← Real.rpow_natCast, ← Real.rpow_add hx0]; ring_nf
    have e2 : (1 - x) ^ (b + ((n - k : ℕ) : ℝ) - 1) = (1 - x) ^ (b - 1) * (1 - x) ^ (n - k) := by
      rw [← Real.rpow_natCast, ← Real.rpow_add h1x]; ring_nf
    rw [e1, e2]
    have hB : beta a b ≠ 0 := (beta_pos ha hb).ne'
    have hB' : beta (a + k) (b + (n - k : ℕ)) ≠ 0 := (beta_pos ha' hb').ne'
    field_simp
  · rw [integral_id_mul_betaPDFReal ha' hb']
    congr 1
    rw [Nat.cast_sub hk]
    ring

end Rematch
