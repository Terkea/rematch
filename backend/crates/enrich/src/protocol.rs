//! Enrichment protocol ids (R2 Task 25, ADR-R2-14).
//!
//! Enrichment is versioned apart from the main analysis (`ANALYSIS_PROTOCOL`, MultiPV 1), so
//! its results never replace a `position_evals` row. Bumping [`ENRICHMENT_PROTOCOL`] re-enriches
//! games from the `position_multipv` / `position_threats` caches without new main evals.

/// Stored with every `move_features` row and on `games.enrichment_protocol`.
pub const ENRICHMENT_PROTOCOL: &str = "enr-v1";
/// `position_multipv.protocol`: Stockfish 19, 300k nodes, MultiPV 3.
pub const MULTIPV_PROTOCOL: &str = "sf19-n300k-mpv3-v1";
/// `position_threats.protocol`: Stockfish 19, 50k nodes on the null-move position.
pub const THREAT_PROTOCOL: &str = "sf19-n50k-null-v1";
/// Nodes per threat probe.
pub const THREAT_NODES: u64 = 50_000;
/// Lines per MultiPV probe.
pub const MULTIPV: u32 = 3;
/// At most this many critical plies get a MultiPV probe per game.
pub const MAX_CRITICAL_PLIES: usize = 30;
