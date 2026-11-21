//! Provider selection (R2 Task 24).

use std::sync::Arc;

use super::maia::{DEFAULT_BATCH_SIZE, DEFAULT_TIMEOUT, MaiaDifficulty};
use super::spread::SpreadDifficulty;
use crate::types::DifficultyProvider;

/// Maia when `maia_url` is set (non-empty) and its `/healthz` answers, otherwise the spread
/// provider. Never fails: an unreachable Maia is logged and replaced by the fallback.
pub async fn select_difficulty_provider(maia_url: Option<&str>) -> Arc<dyn DifficultyProvider> {
    let Some(url) = maia_url.map(str::trim).filter(|u| !u.is_empty()) else {
        return Arc::new(SpreadDifficulty);
    };
    match MaiaDifficulty::connect(url, DEFAULT_TIMEOUT, DEFAULT_BATCH_SIZE).await {
        Ok(maia) => {
            tracing::info!(model = maia.model(), "difficulty provider: maia");
            Arc::new(maia)
        }
        Err(e) => {
            tracing::warn!(error = %e, "maia unavailable; difficulty provider: spread-v1");
            Arc::new(SpreadDifficulty)
        }
    }
}
