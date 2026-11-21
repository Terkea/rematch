//! Clean-room motif tagger (R2 Task 5, ADR-R2-3).
//!
//! Our own rules, written from TS M2 Task 4's wording and the published description in
//! improvement-engine section 4.4. No code from `cook.py` (AGPL) or any other copyleft tagger.

pub mod rules;
pub mod tag_line;

pub use tag_line::{MAX_REPLAY_PLIES, SETTLE_EXTRA, tag_line};
