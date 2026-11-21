//! Validation of a typed `CoachingInput`: R2's `CoachingInput::validate` (every zod bound and
//! refinement, with zod's paths and messages) plus the checks R2 lacks (see `extra_checks`).
//! There is no second validator here.

pub use coaching_input::Issue as CoachingInputIssue;

use super::types::CoachingInput;

/// Every issue found: R2's, then the `ai`-side extras. Empty means valid.
pub fn validate(input: &CoachingInput) -> Vec<CoachingInputIssue> {
    let mut issues = input.validate().err().unwrap_or_default();
    issues.extend(extra_checks(input));
    issues
}

/// Checks the zod schema has and R2's `validate()` does not. Audited against `schema.ts`: R2
/// covers every refinement (the one gap, unknown keys on the flattened `ChangeExample`, is a
/// raw-JSON check in `parse.rs`), so this is empty. A check that truly belongs in R2 is reported
/// to the controller, not added here.
fn extra_checks(_input: &CoachingInput) -> Vec<CoachingInputIssue> {
    Vec::new()
}
