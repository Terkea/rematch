//! The CoachingInput builder (R2 Task 34, ADR-R2-18): what the AI coach (R3) and the web read.
//! R3 depends only on the `coaching-input` crate's types; this module fills them.

pub mod builder;
pub mod colors;
pub mod exemplars;
pub mod mapping;
pub mod strengths;
pub mod weekly;

pub use builder::{BuildError, BuildReport, build_coaching_input, build_coaching_input_report};
