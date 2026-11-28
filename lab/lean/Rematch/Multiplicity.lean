import Mathlib.MeasureTheory.Measure.Basic

/-!
# Multiple testing: Bonferroni and Holm

A finite family of hypotheses indexed by `ι`, with p-values `p i : Ω → ℝ` on a measure space
`(Ω, P)`. The true nulls are a finset `I0`. A p-value is *valid* under its null when
`P {p i ≤ t} ≤ t` for every `t ≥ 0`; nothing is assumed about how the p-values depend on each
other.

Holm's step-down procedure is usually stated on the sorted p-values: reject `H_(1), H_(2), …`
while `p_(k) ≤ α / (m − k + 1)`. Here it is stated without sorting: `H_i` is rejected when every
hypothesis `j` with `p j ≤ p i` passes `p j ≤ α / #{l | p j ≤ p l}`. The count `#{l | p j ≤ p l}`
is `m − k + 1` for the first of a run of tied p-values at rank `k`, the strictest threshold of
the run, so the two descriptions reject the same hypotheses.
-/

open MeasureTheory Finset

namespace Rematch

variable {ι Ω : Type*}

/-- Holm's step-down procedure at level `α`, without sorting: `H_i` is rejected when every
hypothesis whose p-value is at most `p i` clears its threshold `α / #{l | p j ≤ p l}`. -/
def holmRejects [Fintype ι] (α : ℝ) (p : ι → ℝ) (i : ι) : Prop :=
  ∀ j, p j ≤ p i → p j ≤ α / (univ.filter fun l => p j ≤ p l).card

/-- **The deterministic core of Holm's proof.** If Holm rejects a true null, then the smallest
p-value among the `m₀` true nulls is at most `α / m₀`: the first true null to be reached in the
sorted order is tested against a threshold no larger than `α / m₀`, because at least the `m₀`
true nulls are still in play. -/
theorem holm_rejects_true_null [Fintype ι] {α : ℝ} (hα : 0 ≤ α) (p : ι → ℝ) (I0 : Finset ι)
    {i : ι} (hi : i ∈ I0) (hrej : holmRejects α p i) :
    ∃ j ∈ I0, p j ≤ α / I0.card := by
  obtain ⟨j, hjI, hjmin⟩ := I0.exists_min_image p ⟨i, hi⟩
  refine ⟨j, hjI, ?_⟩
  have hj := hrej j (hjmin i hi)
  have hsub : I0 ⊆ univ.filter fun l => p j ≤ p l := by
    intro l hl
    simp [hjmin l hl]
  have hcard : (I0.card : ℝ) ≤ (univ.filter fun l => p j ≤ p l).card := by
    exact_mod_cast card_le_card hsub
  have hpos : (0 : ℝ) < I0.card := by exact_mod_cast card_pos.2 ⟨i, hi⟩
  exact hj.trans (div_le_div_of_nonneg_left hα hpos hcard)

/-- **The union bound for a family of events, each of probability at most `ε`.** The
probability that any of `n` events happens is at most `n ε`. No independence is needed. -/
theorem union_bound {_ : MeasurableSpace Ω} (P : Measure Ω) (I : Finset ι) (A : ι → Set Ω)
    {ε : ℝ} (hA : ∀ i ∈ I, P (A i) ≤ ENNReal.ofReal ε) :
    P (⋃ i ∈ I, A i) ≤ ENNReal.ofReal (I.card * ε) := by
  calc P (⋃ i ∈ I, A i) ≤ ∑ i ∈ I, P (A i) := measure_biUnion_finset_le I A
    _ ≤ ∑ _i ∈ I, ENNReal.ofReal ε := sum_le_sum hA
    _ = ENNReal.ofReal (I.card * ε) := by
      rw [sum_const, nsmul_eq_mul, ENNReal.ofReal_mul (Nat.cast_nonneg _), ENNReal.ofReal_natCast]

/-- **Bonferroni.** If each of the `m` events in a family has probability at most `α / m` (a test
of a true null rejecting at `α / m`, or a `1 − α / m` interval missing its value), then the
probability that any of them happens is at most `α`. This is why the lab's Bonferroni tests
control the family-wise error rate, and its Bonferroni intervals (EXP-0002's family of 20) cover
all their values together with probability at least `1 − α`, under any dependence. The first
statement is over any subfamily `I0` (the true nulls), with `m = Fintype.card ι` the whole
family. -/
theorem bonferroni [Fintype ι] {_ : MeasurableSpace Ω} (P : Measure Ω) {α : ℝ} (hα : 0 ≤ α)
    (A : ι → Set Ω) (I0 : Finset ι)
    (hA : ∀ i ∈ I0, P (A i) ≤ ENNReal.ofReal (α / Fintype.card ι)) :
    P (⋃ i ∈ I0, A i) ≤ ENNReal.ofReal α := by
  refine (union_bound P I0 A hA).trans ?_
  apply ENNReal.ofReal_le_ofReal
  rcases Nat.eq_zero_or_pos (Fintype.card ι) with h | h
  · simp [h, hα]
  · have hc : (I0.card : ℝ) ≤ Fintype.card ι := by exact_mod_cast card_le_univ I0
    have hm : (0 : ℝ) < Fintype.card ι := by exact_mod_cast h
    calc (I0.card : ℝ) * (α / Fintype.card ι) ≤ Fintype.card ι * (α / Fintype.card ι) :=
          mul_le_mul_of_nonneg_right hc (div_nonneg hα hm.le)
      _ = α := by field_simp

/-- Bonferroni for tests: rejecting each `H_i` when `p i ≤ α / m`, with valid p-values for the
true nulls `I0`, rejects at least one true null with probability at most `α`. -/
theorem bonferroni_fwer [Fintype ι] {_ : MeasurableSpace Ω} (P : Measure Ω) {α : ℝ} (hα : 0 ≤ α)
    (p : ι → Ω → ℝ) (I0 : Finset ι)
    (hvalid : ∀ i ∈ I0, ∀ t, 0 ≤ t → P {ω | p i ω ≤ t} ≤ ENNReal.ofReal t) :
    P {ω | ∃ i ∈ I0, p i ω ≤ α / Fintype.card ι} ≤ ENNReal.ofReal α := by
  have hset : {ω | ∃ i ∈ I0, p i ω ≤ α / Fintype.card ι}
      = ⋃ i ∈ I0, {ω | p i ω ≤ α / Fintype.card ι} := by
    ext ω; simp
  rw [hset]
  exact bonferroni P hα _ I0 fun i hi => hvalid i hi _ (div_nonneg hα (Nat.cast_nonneg _))

/-- **Holm controls the family-wise error rate at `α`, under any dependence.** With valid
p-values for the true nulls `I0` (`P {p i ≤ t} ≤ t`), the probability that Holm's step-down
procedure rejects at least one true null is at most `α`. The proof is the standard one: on that
event the smallest true-null p-value is at most `α / m₀` (`holm_rejects_true_null`), and the
union bound over the `m₀` true nulls gives `m₀ · α / m₀ = α`. The lab's Holm families (the keys
of EXP-0009 and EXP-0029) rely on this, with no assumption about how the keys correlate. -/
theorem holm_fwer [Fintype ι] {_ : MeasurableSpace Ω} (P : Measure Ω) {α : ℝ} (hα : 0 ≤ α)
    (p : ι → Ω → ℝ) (I0 : Finset ι)
    (hvalid : ∀ i ∈ I0, ∀ t, 0 ≤ t → P {ω | p i ω ≤ t} ≤ ENNReal.ofReal t) :
    P {ω | ∃ i ∈ I0, holmRejects α (fun j => p j ω) i} ≤ ENNReal.ofReal α := by
  have hsub : {ω | ∃ i ∈ I0, holmRejects α (fun j => p j ω) i}
      ⊆ ⋃ j ∈ I0, {ω | p j ω ≤ α / I0.card} := by
    rintro ω ⟨i, hi, hrej⟩
    obtain ⟨j, hj, hle⟩ := holm_rejects_true_null hα (fun j => p j ω) I0 hi hrej
    exact Set.mem_biUnion hj hle
  refine (measure_mono hsub).trans ?_
  refine (union_bound P I0 _
    fun j hj => hvalid j hj _ (div_nonneg hα (Nat.cast_nonneg _))).trans ?_
  apply ENNReal.ofReal_le_ofReal
  rcases Nat.eq_zero_or_pos I0.card with h | h
  · simp [h, hα]
  · have hm : (0 : ℝ) < I0.card := by exact_mod_cast h
    rw [mul_div_cancel₀ _ hm.ne']

end Rematch
