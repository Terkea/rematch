//! Enrichment traits and inputs (final in R2 Task 1).

use futures::future::BoxFuture;
use profile_core::types::Wdl;

/// Syzygy WDL probe (Task 23).
pub trait TablebaseProbe: Send + Sync {
    /// Largest piece count (both kings included) the probe can answer.
    fn max_pieces(&self) -> u32;

    /// WDL from the side-to-move's point of view, or `None` when not available.
    fn probe<'a>(&'a self, fen: &'a str) -> BoxFuture<'a, Option<Wdl>>;
}

/// One position to estimate move difficulty for (Task 24).
#[derive(Clone, Debug, PartialEq)]
pub struct DifficultyInput {
    pub fen: String,
    pub elo_self: Option<i32>,
    pub elo_oppo: Option<i32>,
    /// MultiPV lines as (uci, mover win%).
    pub lines: Option<Vec<(String, f64)>>,
    pub best_uci: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum DifficultyError {
    #[error("difficulty service unavailable: {0}")]
    Unavailable(String),
}

/// `(p_best, p_ok)` for one input, or `None` when the provider cannot estimate it.
pub type DifficultyEstimate = Option<(f64, f64)>;

/// Estimates `(p_best, p_ok)` per input: the probability that a player of the band plays the best
/// move, and an acceptable move (Task 24).
pub trait DifficultyProvider: Send + Sync {
    /// Model id recorded with each estimate.
    fn model(&self) -> &str;

    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Result<Vec<DifficultyEstimate>, DifficultyError>>;
}
