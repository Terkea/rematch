import Mathlib.Analysis.SpecialFunctions.Pow.Real

/-!
# The rating-memory luck share (EXP-0009, EXP-0029)

EXP-0029's preregistration, section 4, models a rating's memory: after each game a rating keeps
a share `φ` of its error (`0 < φ < 1`; EXP-0015 measured 0.97). Of the luck held in a rating at
the end of the exposure window F2,

* EXP-0009's outcome `R_end(next month) − R_end(F2)` carries the share `φ ^ g − 1`, with `g` the
  rapid games of the next month;
* EXP-0029's gap-month outcome carries `φ ^ g_A * (φ ^ (2 * g_J) − 1)`, with `g_A` April's and
  `g_J` June's rapid games (the formula of section 4, matched exactly).

The memory ratio of section 4 divides the mean of the second by the mean of the first.
-/

namespace Rematch

/-- EXP-0029's luck share `φ ^ g_A * (φ ^ (2 * g_J) − 1)` (preregistration section 4). -/
noncomputable def luckShare (φ : ℝ) (gA gJ : ℕ) : ℝ := φ ^ gA * (φ ^ (2 * gJ) - 1)

/-- EXP-0009's luck share `φ ^ g − 1` (preregistration of EXP-0029, section 4). -/
noncomputable def exp09Share (φ : ℝ) (g : ℕ) : ℝ := φ ^ g - 1

/-- EXP-0009's share lies in `(−1, 0]`: the outcome removes some of the luck at F2's end (or none,
with no games), and never more than all of it. -/
theorem exp09Share_mem {φ : ℝ} (h0 : 0 < φ) (h1 : φ < 1) (g : ℕ) :
    -1 < exp09Share φ g ∧ exp09Share φ g ≤ 0 := by
  have hp : 0 < φ ^ g := pow_pos h0 g
  have hl : φ ^ g ≤ 1 := pow_le_one₀ h0.le h1.le
  unfold exp09Share
  constructor <;> linarith

/-- EXP-0029's share lies in `(−1, 0]`. -/
theorem luckShare_mem {φ : ℝ} (h0 : 0 < φ) (h1 : φ < 1) (gA gJ : ℕ) :
    -1 < luckShare φ gA gJ ∧ luckShare φ gA gJ ≤ 0 := by
  have ha0 : 0 < φ ^ gA := pow_pos h0 gA
  have ha1 : φ ^ gA ≤ 1 := pow_le_one₀ h0.le h1.le
  have hj0 : 0 < φ ^ (2 * gJ) := pow_pos h0 _
  have hj1 : φ ^ (2 * gJ) ≤ 1 := pow_le_one₀ h0.le h1.le
  unfold luckShare
  constructor <;> nlinarith

/-- The size of EXP-0029's share never grows with April's games: each game in the gap month
forgets a further share `1 − φ` of the luck. -/
theorem abs_luckShare_antitone {φ : ℝ} (h0 : 0 < φ) (h1 : φ < 1) (gJ : ℕ) :
    Antitone fun gA => |luckShare φ gA gJ| := by
  intro a b hab
  have hj1 : φ ^ (2 * gJ) ≤ 1 := pow_le_one₀ h0.le h1.le
  have hab' : φ ^ b ≤ φ ^ a := pow_le_pow_of_le_one h0.le h1.le hab
  simp only [luckShare, abs_mul, abs_of_pos (pow_pos h0 _), abs_of_nonpos (sub_nonpos.2 hj1)]
  exact mul_le_mul_of_nonneg_right hab' (by linarith)

/-- EXP-0029's share is zero exactly when June has no games. -/
theorem luckShare_eq_zero_iff {φ : ℝ} (h0 : 0 < φ) (h1 : φ < 1) (gA gJ : ℕ) :
    luckShare φ gA gJ = 0 ↔ gJ = 0 := by
  unfold luckShare
  rw [mul_eq_zero, sub_eq_zero]
  constructor
  · rintro (h | h)
    · exact absurd h (pow_pos h0 gA).ne'
    · by_contra hne
      have : φ ^ (2 * gJ) < 1 := pow_lt_one₀ h0.le h1 (by omega)
      exact this.ne h
  · rintro rfl
    right
    simp

/-- **The luck shares of EXP-0009 and EXP-0029.** For a rating that keeps a share `0 < φ < 1` of
its error after each game:

1. EXP-0029's share `s = φ ^ g_A * (φ ^ (2 g_J) − 1)` satisfies `−1 < s ≤ 0`;
2. `|s|` is non-increasing in April's games `g_A`, so a busier gap month never leaves more luck;
3. `s = 0` exactly when `g_J = 0`;
4. EXP-0009's share `φ ^ g − 1` also lies in `(−1, 0]`.

The lab relies on (1) and (4) to read the memory ratio as a fraction of luck, and on (2) for the
claim that players who play the gap month carry less of it. -/
theorem luck_share {φ : ℝ} (h0 : 0 < φ) (h1 : φ < 1) :
    (∀ gA gJ : ℕ, -1 < luckShare φ gA gJ ∧ luckShare φ gA gJ ≤ 0) ∧
    (∀ gJ : ℕ, Antitone fun gA => |luckShare φ gA gJ|) ∧
    (∀ gA gJ : ℕ, luckShare φ gA gJ = 0 ↔ gJ = 0) ∧
    (∀ g : ℕ, -1 < exp09Share φ g ∧ exp09Share φ g ≤ 0) :=
  ⟨luckShare_mem h0 h1, abs_luckShare_antitone h0 h1, luckShare_eq_zero_iff h0 h1,
    exp09Share_mem h0 h1⟩

end Rematch
