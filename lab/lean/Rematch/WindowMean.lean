import Mathlib.Probability.Moments.Variance

/-!
# The window means: the change and the level are uncorrelated

Two windows of a player's games give the measurements `q₁ = θ + e₁` and `q₂ = θ + δ + e₂`: the
player's level `θ`, the change `δ` between the windows, and the measurement noise `e₁`, `e₂`.
The lab reads the change `q₂ − q₁` and the level `(q₁ + q₂) / 2`. If the noise of the two
windows is uncorrelated and has the same variance, the change's error and the level's error are
uncorrelated, so a regression on both does not trade one for the other. With unequal variances
the covariance is `(σ₂² − σ₁²) / 2`, which is the size of the bias to check.

`θ` and `δ` are constants (one player); `e₁`, `e₂` are square-integrable random variables on a
probability space. Their means need not be zero for the covariance; the level's error is
`(q₁ + q₂) / 2 − θ − δ / 2 = (e₁ + e₂) / 2`.
-/

open MeasureTheory ProbabilityTheory

namespace Rematch

variable {Ω : Type*} {mΩ : MeasurableSpace Ω} {μ : Measure Ω} [IsProbabilityMeasure μ]

/-- **The window means, unequal variances.** With `q₁ = θ + e₁`, `q₂ = θ + δ + e₂`, `e₁` and
`e₂` square-integrable and uncorrelated,
`Cov(q₂ − q₁, (q₁ + q₂) / 2 − θ − δ / 2) = (Var e₂ − Var e₁) / 2`. -/
theorem window_mean_covariance {e₁ e₂ : Ω → ℝ} (h₁ : MemLp e₁ 2 μ) (h₂ : MemLp e₂ 2 μ)
    (hunc : cov[e₁, e₂; μ] = 0) (θ δ : ℝ) :
    cov[fun ω ↦ (θ + δ + e₂ ω) - (θ + e₁ ω),
        fun ω ↦ ((θ + e₁ ω) + (θ + δ + e₂ ω)) / 2 - θ - δ / 2; μ]
      = (Var[e₂; μ] - Var[e₁; μ]) / 2 := by
  have hD : (fun ω ↦ (θ + δ + e₂ ω) - (θ + e₁ ω)) = fun ω ↦ (e₂ - e₁) ω + δ := by
    funext ω; simp only [Pi.sub_apply]; ring
  have hM : (fun ω ↦ ((θ + e₁ ω) + (θ + δ + e₂ ω)) / 2 - θ - δ / 2)
      = fun ω ↦ (e₁ + e₂) ω / 2 := by
    funext ω; simp only [Pi.add_apply]; ring
  rw [hD, hM, covariance_add_const_left ((h₂.sub h₁).integrable (by norm_num)),
    covariance_fun_div_right, covariance_sub_left h₂ h₁ (h₁.add h₂),
    covariance_add_right h₂ h₁ h₂, covariance_add_right h₁ h₁ h₂,
    covariance_self h₂.aemeasurable, covariance_self h₁.aemeasurable,
    covariance_comm e₂ e₁, hunc]
  ring

/-- **The window means are uncorrelated**. With
`q₁ = θ + e₁`, `q₂ = θ + δ + e₂`, the noises `e₁`, `e₂` square-integrable, uncorrelated and of
equal variance `σ²`, the change `q₂ − q₁` and the level's error `(q₁ + q₂) / 2 − θ − δ / 2` have
covariance zero; with variances `σ₁²`, `σ₂²` it is `(σ₂² − σ₁²) / 2`. A lab model that reads the
change and the level from the same two windows relies on this: equal-sized windows from the same
player make the two readings' errors uncorrelated, and unequal windows leave a covariance of half
the difference in their noise variances. -/
theorem window_mean_independence {e₁ e₂ : Ω → ℝ} (h₁ : MemLp e₁ 2 μ) (h₂ : MemLp e₂ 2 μ)
    (hunc : cov[e₁, e₂; μ] = 0) (θ δ : ℝ) :
    (Var[e₁; μ] = Var[e₂; μ] →
      cov[fun ω ↦ (θ + δ + e₂ ω) - (θ + e₁ ω),
          fun ω ↦ ((θ + e₁ ω) + (θ + δ + e₂ ω)) / 2 - θ - δ / 2; μ] = 0) ∧
    (∀ σ₁ σ₂ : ℝ, Var[e₁; μ] = σ₁ ^ 2 → Var[e₂; μ] = σ₂ ^ 2 →
      cov[fun ω ↦ (θ + δ + e₂ ω) - (θ + e₁ ω),
          fun ω ↦ ((θ + e₁ ω) + (θ + δ + e₂ ω)) / 2 - θ - δ / 2; μ]
        = (σ₂ ^ 2 - σ₁ ^ 2) / 2) := by
  rw [window_mean_covariance h₁ h₂ hunc θ δ]
  refine ⟨fun h ↦ by rw [h]; ring, fun σ₁ σ₂ hσ₁ hσ₂ ↦ by rw [hσ₁, hσ₂]⟩

end Rematch
