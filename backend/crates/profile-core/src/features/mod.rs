//! Per-move features and enriched-move assembly (R2 Task 6, rules exactly TS M2 Task 5).
//!
//! Honesty rules on top of TS M2:
//! - A feature that cannot be computed is `None`; nothing is substituted. An empty or illegal PV
//!   gives no line analysis (`None`), never a "quiet" line with gain 0, so Lichess evals without
//!   a PV cannot silently count as "no tactic".
//! - Move difficulty comes only from Maia (owner override): it is `None` when Maia was down, at
//!   plies `≤ 10`, or when the stored value is not a finite Maia probability.
//! - Maia is recognised by its exact model id ([`is_maia_model_id`]), shared with T24.
//! - Invalid inputs are skipped and counted ([`SkipCounts`]); no NaN or infinity is emitted.

pub mod assemble;
pub mod derive;

pub use assemble::{
    Assembly, MAX_EVAL_CP, MAX_EVAL_MATE, SkipCounts, assemble_enriched_moves,
    assemble_enriched_moves_counted, eval_in_range,
};
pub use derive::{
    CRITICAL_GAP, DIFFICULTY_EPSILON, DIFFICULTY_MIN_PLY, InvalidDifficulty, LINE_MAX_PLIES,
    MAIA_MODEL_ID_PATTERN, PLAYABLE_WINDOW, THREAT_MAX_DEPTH, THREAT_MIN_GAIN,
    derive_move_features, first_capture_square, is_maia_model_id, maia_difficulty,
    threat_probe_matches,
};
