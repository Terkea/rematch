//! Pure profile-engine logic: features, weaknesses W1-W12, statistics, timeline, KPIs and
//! openings. No sqlx, no I/O; every stochastic function takes a seed (ADR-R2-1).

pub mod bands;
pub mod board;
pub mod constants;
pub mod features;
pub mod kpi;
pub mod motifs;
pub mod openings;
pub mod stats;
pub mod timeline;
pub mod types;
pub mod weakness;
