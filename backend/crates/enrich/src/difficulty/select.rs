//! The production difficulty entry point (R2 Task 24, ADR-R2-13).
//!
//! [`DifficultyEstimator`] is the only way production code gets a difficulty, and Maia is its
//! only source. It connects lazily: when Maia is down at start-up, or goes down later, every
//! affected position is `pending_maia` and the next call probes `/healthz` again, so the worker
//! never blocks on Maia and picks it up once it is healthy.

use std::sync::Arc;
use std::time::Duration;

use profile_core::types::Difficulty;
use tokio::sync::Mutex;

use super::maia::{DEFAULT_BATCH_SIZE, DEFAULT_TIMEOUT, MaiaDifficulty, MaiaItem};
use super::{
    DifficultyOutcome, NotApplicableReason, PendingReason, PlyIndexError, half_move_index,
    is_maia_opening_ply,
};
use crate::types::DifficultyInput;

pub struct DifficultyEstimator {
    url: Option<String>,
    timeout: Duration,
    batch_size: usize,
    maia: Mutex<Option<Arc<MaiaDifficulty>>>,
}

/// The estimator for `MAIA_URL` (`None` or empty = not configured, so every estimable position
/// is `pending_maia`). It never fails and never blocks start-up: Maia is probed on first use.
pub fn select_difficulty_provider(maia_url: Option<&str>) -> DifficultyEstimator {
    DifficultyEstimator::new(maia_url, DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE)
}

impl DifficultyEstimator {
    pub fn new(maia_url: Option<&str>, timeout: Duration, batch_size: usize) -> Self {
        Self {
            url: maia_url
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .map(str::to_owned),
            timeout,
            batch_size,
            maia: Mutex::new(None),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.url.is_some()
    }

    /// The connected client, connecting first when needed.
    async fn client(&self) -> Result<Arc<MaiaDifficulty>, PendingReason> {
        let Some(url) = &self.url else {
            return Err(PendingReason::MaiaNotConfigured);
        };
        let mut guard = self.maia.lock().await;
        if let Some(m) = guard.as_ref() {
            return Ok(Arc::clone(m));
        }
        match MaiaDifficulty::connect(url, self.timeout, self.batch_size).await {
            Ok(m) => {
                tracing::info!(model = m.model_id(), "maia connected");
                let m = Arc::new(m);
                *guard = Some(Arc::clone(&m));
                Ok(m)
            }
            Err(e) => {
                tracing::warn!(error = %e, "maia unavailable; difficulty pending");
                Err(e.pending_reason())
            }
        }
    }

    /// Forgets the client after a failed call, so the next call re-probes `/healthz`.
    async fn reset(&self) {
        *self.maia.lock().await = None;
    }

    /// One outcome per input, in order. Never fails.
    pub async fn estimate(&self, inputs: &[DifficultyInput]) -> Vec<DifficultyOutcome> {
        let mut out: Vec<Option<DifficultyOutcome>> = inputs
            .iter()
            .map(|i| {
                let reason = if half_move_index(&i.fen) == Err(PlyIndexError::Overflow) {
                    NotApplicableReason::InvalidFen
                } else if is_maia_opening_ply(&i.fen) {
                    NotApplicableReason::OpeningPly
                } else if i.best_uci.is_none() {
                    NotApplicableReason::NoBestMove
                } else {
                    return None;
                };
                Some(DifficultyOutcome::NotApplicable { reason })
            })
            .collect();
        let todo: Vec<usize> = (0..inputs.len()).filter(|&i| out[i].is_none()).collect();
        if !todo.is_empty() {
            match self.client().await {
                Err(reason) => {
                    for &i in &todo {
                        out[i] = Some(DifficultyOutcome::PendingMaia { reason });
                    }
                }
                Ok(maia) => self.estimate_with(&maia, inputs, &todo, &mut out).await,
            }
        }
        out.into_iter()
            .map(|o| o.expect("every input has an outcome"))
            .collect()
    }

    async fn estimate_with(
        &self,
        maia: &MaiaDifficulty,
        inputs: &[DifficultyInput],
        todo: &[usize],
        out: &mut [Option<DifficultyOutcome>],
    ) {
        let mut failed: Option<PendingReason> = None;
        for idx in todo.chunks(maia.batch_size()) {
            if let Some(reason) = failed {
                // Maia failed earlier in this call: do not wait on it again; keep that reason.
                for &i in idx {
                    out[i] = Some(DifficultyOutcome::PendingMaia { reason });
                }
                continue;
            }
            let batch: Vec<&DifficultyInput> = idx.iter().map(|&i| &inputs[i]).collect();
            match maia.predict_batch(&batch).await {
                Ok(items) => {
                    for (&i, item) in idx.iter().zip(items) {
                        out[i] = Some(outcome(maia, &inputs[i], item));
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "maia call failed; difficulty pending");
                    let reason = e.pending_reason();
                    for &i in idx {
                        out[i] = Some(DifficultyOutcome::PendingMaia { reason });
                    }
                    failed = Some(reason);
                }
            }
        }
        if failed.is_some() {
            self.reset().await;
        }
    }
}

fn outcome(maia: &MaiaDifficulty, input: &DifficultyInput, item: MaiaItem) -> DifficultyOutcome {
    match item {
        MaiaItem::Probs(probs) => {
            let best = input.best_uci.as_deref().unwrap_or_default();
            let lines = input.lines.as_deref().unwrap_or_default();
            let (p_best, p_ok) = MaiaDifficulty::score(best, lines, &probs);
            DifficultyOutcome::Ok {
                difficulty: Difficulty {
                    p_best,
                    p_ok,
                    model: maia.model_id().to_owned(),
                },
            }
        }
        MaiaItem::Rejected(reason) => DifficultyOutcome::NotApplicable { reason },
        MaiaItem::Invalid(why) => {
            tracing::warn!(fen = %input.fen, %why, "maia item failed validation; difficulty pending");
            DifficultyOutcome::PendingMaia {
                reason: PendingReason::InvalidResponse,
            }
        }
    }
}
