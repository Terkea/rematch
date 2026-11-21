//! Maia HTTP provider (R2 Task 24).
//!
//! Talks to `services/maia`: `GET /healthz` returns `{model}`, and
//! `POST /v1/predict {items:[{fen, eloSelf, eloOppo}]}` returns
//! `{items:[{probs:{uci:p}} | {error:"invalid_fen"}]}`, in batches of at most `batch_size`.
//!
//! Per input: `p_best = probs[best_uci]` (0 when Maia gives it no mass), and `p_ok = sum of
//! probs[m]` over the known lines `m` within [`OK_WINDOW`] of the best line. Without lines, only
//! the best move is known to be acceptable, so `p_ok = p_best`. Inputs without `best_uci`, and
//! items the service rejects, give `None`. Transport errors, timeouts, non-2xx answers and
//! malformed bodies give [`DifficultyError::Unavailable`].

use std::collections::HashMap;
use std::time::Duration;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use super::{OK_WINDOW, best_win};
use crate::types::{DifficultyError, DifficultyEstimate, DifficultyInput, DifficultyProvider};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_BATCH_SIZE: usize = 64;

#[derive(Debug)]
pub struct MaiaDifficulty {
    client: reqwest::Client,
    base: String,
    model: String,
    batch_size: usize,
}

#[derive(Deserialize)]
struct Health {
    model: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PredictItem<'a> {
    fen: &'a str,
    elo_self: Option<i32>,
    elo_oppo: Option<i32>,
}

#[derive(Serialize)]
struct PredictRequest<'a> {
    items: Vec<PredictItem<'a>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PredictResult {
    Probs { probs: HashMap<String, f64> },
    Error { error: String },
}

#[derive(Deserialize)]
struct PredictResponse {
    items: Vec<PredictResult>,
}

fn unavailable(e: impl std::fmt::Display) -> DifficultyError {
    DifficultyError::Unavailable(e.to_string())
}

impl MaiaDifficulty {
    /// Connects to the service at `url` and reads the model id from `GET /healthz`.
    pub async fn connect(
        url: &str,
        timeout: Duration,
        batch_size: usize,
    ) -> Result<Self, DifficultyError> {
        crate::install_crypto_provider();
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(unavailable)?;
        let base = url.trim_end_matches('/').to_owned();
        let health: Health = client
            .get(format!("{base}/healthz"))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(unavailable)?
            .json()
            .await
            .map_err(unavailable)?;
        if health.model.is_empty() {
            return Err(unavailable("empty model id from /healthz"));
        }
        Ok(Self {
            client,
            base,
            model: health.model,
            batch_size: batch_size.max(1),
        })
    }

    async fn predict(
        &self,
        batch: &[DifficultyInput],
    ) -> Result<Vec<PredictResult>, DifficultyError> {
        let body = PredictRequest {
            items: batch
                .iter()
                .map(|i| PredictItem {
                    fen: &i.fen,
                    elo_self: i.elo_self,
                    elo_oppo: i.elo_oppo,
                })
                .collect(),
        };
        let resp: PredictResponse = self
            .client
            .post(format!("{}/v1/predict", self.base))
            .json(&body)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(unavailable)?
            .json()
            .await
            .map_err(unavailable)?;
        if resp.items.len() != batch.len() {
            return Err(unavailable(format!(
                "maia returned {} items for {} inputs",
                resp.items.len(),
                batch.len()
            )));
        }
        Ok(resp.items)
    }

    /// `(p_best, p_ok)` from Maia's move probabilities for one input.
    pub fn score(input: &DifficultyInput, probs: &HashMap<String, f64>) -> DifficultyEstimate {
        let best_uci = input.best_uci.as_deref()?;
        let p = |uci: &str| probs.get(uci).copied().unwrap_or(0.0);
        let p_best = p(best_uci);
        let lines = input.lines.as_deref().unwrap_or_default();
        let p_ok = match best_win(lines) {
            Some(best) => lines
                .iter()
                .filter(|(_, w)| *w >= best - OK_WINDOW)
                .map(|(uci, _)| p(uci))
                .sum(),
            None => p_best,
        };
        Some((p_best, p_ok))
    }
}

impl DifficultyProvider for MaiaDifficulty {
    fn model(&self) -> &str {
        &self.model
    }

    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Result<Vec<DifficultyEstimate>, DifficultyError>> {
        Box::pin(async move {
            let mut out = Vec::with_capacity(inputs.len());
            for batch in inputs.chunks(self.batch_size) {
                let results = self.predict(batch).await?;
                out.extend(batch.iter().zip(results).map(|(input, r)| match r {
                    PredictResult::Probs { probs } => Self::score(input, &probs),
                    PredictResult::Error { error } => {
                        tracing::debug!(fen = %input.fen, %error, "maia rejected an item");
                        None
                    }
                }));
            }
            Ok(out)
        })
    }
}
