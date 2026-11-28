import Rematch

/-!
# The axiom check

`lake build` fails if any of the lab's theorems depends on an axiom beyond mathlib's standard
three (`propext`, `Classical.choice`, `Quot.sound`). A `sorry` shows up as `sorryAx` and fails
it too.
-/

open Lean Elab Command

/-- `#assert_standard_axioms n` fails unless `n` uses only `propext`, `Classical.choice` and
`Quot.sound`, and logs the axioms it does use. -/
elab "#assert_standard_axioms " id:ident : command => do
  let n ← liftCoreM <| realizeGlobalConstNoOverloadWithInfo id
  let axs ← collectAxioms n
  let allowed := [``propext, ``Classical.choice, ``Quot.sound]
  let bad := axs.filter (fun a => !allowed.contains a)
  if bad.isEmpty then
    logInfo m!"{n}: standard axioms only {axs.toList}"
  else
    throwError m!"{n} depends on non-standard axioms {bad.toList}"

#print axioms Rematch.holm_fwer
#print axioms Rematch.bonferroni
#print axioms Rematch.beta_binomial_posterior
#print axioms Rematch.luck_share
#print axioms Rematch.window_mean_independence

#assert_standard_axioms Rematch.holm_rejects_true_null
#assert_standard_axioms Rematch.holm_fwer
#assert_standard_axioms Rematch.union_bound
#assert_standard_axioms Rematch.bonferroni
#assert_standard_axioms Rematch.bonferroni_fwer
#assert_standard_axioms Rematch.beta_add_one_left
#assert_standard_axioms Rematch.integral_betaPDFReal
#assert_standard_axioms Rematch.integral_id_mul_betaPDFReal
#assert_standard_axioms Rematch.beta_binomial_posterior
#assert_standard_axioms Rematch.luck_share
#assert_standard_axioms Rematch.window_mean_covariance
#assert_standard_axioms Rematch.window_mean_independence
