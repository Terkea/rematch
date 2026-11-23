//! The pipeline benchmark, Tier 1 (ADR 0100, `docs/benchmark.md`): synthetic players with
//! injected flaws of known rates, run through the real analysis, enrichment, profile engine and
//! CoachingInput, then scored on diagnosis, calibration, plan targeting, data efficiency and
//! stability.

pub mod flaws;
pub mod generate;
pub mod harness;
pub mod memo;
pub mod policy;
pub mod results;
pub mod run;
pub mod suite;
