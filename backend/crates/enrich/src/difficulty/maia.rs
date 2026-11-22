//! Maia HTTP client (R2 Task 24, ADR-R2-13).
//!
//! Talks to `services/maia`: `GET /healthz` returns `{model}`, and
//! `POST /v1/predict {items:[{fen, eloSelf, eloOppo}]}` returns
//! `{items:[{probs:{uci:p}} | {error:"invalid_fen"|"no_legal_moves"}]}`, in batches of at most
//! `batch_size`.
//!
//! Each item is validated on its own. A probability map is accepted only when it is non-empty,
//! every key looks like a UCI move, every value is finite and in `[0, 1]`, and the values sum to
//! `1 ± 0.01`. Anything else makes that item [`MaiaItem::Invalid`]; the other items stand.
//!
//! Scoring: `p_best = probs[best_uci]` (0 when Maia gives it no mass); `p_ok = sum of probs[m]`
//! over the deduplicated lines within [`OK_WINDOW`] of the best line, clamped to `[p_best, 1]`.
//! Without lines, `p_ok = p_best`.

use std::collections::HashMap;
use std::time::Duration;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    NotApplicableReason, OK_WINDOW, PendingReason, best_win, dedupe_lines, is_maia_opening_ply,
};
use crate::types::{DifficultyError, DifficultyEstimate, DifficultyInput, DifficultyProvider};

/// Per-request timeout. 30 s (coordinator ruling): a 64-item batch took about 5.4 s on CPU with two
/// threads, so a loaded host must not time a 32-item batch out into `pending_maia`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Items per predict request (the service accepts up to 256).
pub const DEFAULT_BATCH_SIZE: usize = 32;
/// Accepted deviation of an item's probability sum from 1.
pub const SUM_TOLERANCE: f64 = 0.01;

#[derive(Debug)]
pub struct MaiaDifficulty {
    client: reqwest::Client,
    base: String,
    model: String,
    batch_size: usize,
}

/// One item of a predict answer, after validation.
#[derive(Clone, Debug, PartialEq)]
pub enum MaiaItem {
    Probs(HashMap<String, f64>),
    /// The service says Maia cannot evaluate the position.
    Rejected(NotApplicableReason),
    /// The item failed validation (the reason is for logs).
    Invalid(String),
}

/// A failed predict call: no item of the batch has an answer.
#[derive(thiserror::Error, Debug)]
pub enum MaiaCallError {
    #[error("maia request timed out: {0}")]
    Timeout(String),
    #[error("maia unavailable: {0}")]
    Unavailable(String),
    #[error("maia answer invalid: {0}")]
    InvalidResponse(String),
}

impl MaiaCallError {
    pub fn pending_reason(&self) -> PendingReason {
        match self {
            Self::Timeout(_) => PendingReason::Timeout,
            Self::Unavailable(_) => PendingReason::MaiaUnavailable,
            // The whole answer was unusable: not a per-position answer.
            Self::InvalidResponse(_) => PendingReason::BadResponse,
        }
    }

    fn from_reqwest(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            Self::Timeout(e.to_string())
        } else if e.is_decode() {
            Self::InvalidResponse(e.to_string())
        } else {
            Self::Unavailable(e.to_string())
        }
    }
}

impl From<MaiaCallError> for DifficultyError {
    fn from(e: MaiaCallError) -> Self {
        DifficultyError::Unavailable(e.to_string())
    }
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

/// Items stay raw JSON so one malformed item cannot fail the whole batch.
#[derive(Deserialize)]
struct PredictResponse {
    items: Vec<Value>,
}

fn looks_like_uci(m: &str) -> bool {
    let b = m.as_bytes();
    let square = |f: u8, r: u8| (b'a'..=b'h').contains(&f) && (b'1'..=b'8').contains(&r);
    match b.len() {
        4 => square(b[0], b[1]) && square(b[2], b[3]),
        5 => square(b[0], b[1]) && square(b[2], b[3]) && b"qrbn".contains(&b[4]),
        _ => false,
    }
}

/// Validates one answer item.
pub fn parse_item(item: &Value) -> MaiaItem {
    if let Some(error) = item.get("error") {
        return match error.as_str() {
            Some("invalid_fen") => MaiaItem::Rejected(NotApplicableReason::InvalidFen),
            Some("no_legal_moves") => MaiaItem::Rejected(NotApplicableReason::NoLegalMoves),
            _ => MaiaItem::Invalid(format!("unknown error {error}")),
        };
    }
    let Some(map) = item.get("probs").and_then(Value::as_object) else {
        return MaiaItem::Invalid("no probs".into());
    };
    if map.is_empty() {
        return MaiaItem::Invalid("empty probs".into());
    }
    let mut probs = HashMap::with_capacity(map.len());
    let mut sum = 0.0;
    for (uci, p) in map {
        let Some(p) = p.as_f64() else {
            return MaiaItem::Invalid(format!("{uci}: not a number"));
        };
        if !p.is_finite() || !(0.0..=1.0).contains(&p) {
            return MaiaItem::Invalid(format!("{uci}: {p} outside [0, 1]"));
        }
        if !looks_like_uci(uci) {
            return MaiaItem::Invalid(format!("{uci}: not a UCI move"));
        }
        sum += p;
        probs.insert(uci.clone(), p);
    }
    if (sum - 1.0).abs() > SUM_TOLERANCE {
        return MaiaItem::Invalid(format!("probabilities sum to {sum}"));
    }
    MaiaItem::Probs(probs)
}

impl MaiaDifficulty {
    /// Connects to the service at `url` and reads the model id from `GET /healthz`. Connecting
    /// is bounded by [`DEFAULT_CONNECT_TIMEOUT`] (or `timeout`, if shorter); each request by
    /// `timeout`.
    pub async fn connect(
        url: &str,
        timeout: Duration,
        batch_size: usize,
    ) -> Result<Self, MaiaCallError> {
        crate::install_crypto_provider();
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT.min(timeout))
            .build()
            .map_err(|e| MaiaCallError::Unavailable(e.to_string()))?;
        let base = url.trim_end_matches('/').to_owned();
        let health: Health = client
            .get(format!("{base}/healthz"))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(MaiaCallError::from_reqwest)?
            .json()
            .await
            .map_err(MaiaCallError::from_reqwest)?;
        // The exact Maia id rule shared with the feature layer (T6 review): anything else, an
        // empty id included, is not Maia and never tags a difficulty.
        if !profile_core::features::is_maia_model_id(&health.model) {
            return Err(MaiaCallError::InvalidResponse(format!(
                "model id {:?} from /healthz is not a Maia model id",
                health.model
            )));
        }
        Ok(Self {
            client,
            base,
            model: health.model,
            batch_size: batch_size.max(1),
        })
    }

    /// The model id from `/healthz`, e.g. `maia2-rapid-0.11.0`.
    pub fn model_id(&self) -> &str {
        &self.model
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// One predict request for at most `batch_size` inputs (no opening-ply filtering).
    pub async fn predict_batch(
        &self,
        batch: &[&DifficultyInput],
    ) -> Result<Vec<MaiaItem>, MaiaCallError> {
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
            .map_err(MaiaCallError::from_reqwest)?
            .json()
            .await
            .map_err(MaiaCallError::from_reqwest)?;
        if resp.items.len() != batch.len() {
            return Err(MaiaCallError::InvalidResponse(format!(
                "maia returned {} items for {} inputs",
                resp.items.len(),
                batch.len()
            )));
        }
        Ok(resp.items.iter().map(parse_item).collect())
    }

    /// Validated move probabilities per input, batched; `None` for items Maia rejected or that
    /// failed validation. Sends every input, opening plies included.
    pub async fn probabilities(
        &self,
        inputs: &[DifficultyInput],
    ) -> Result<Vec<Option<HashMap<String, f64>>>, MaiaCallError> {
        let mut out = Vec::with_capacity(inputs.len());
        let refs: Vec<&DifficultyInput> = inputs.iter().collect();
        for batch in refs.chunks(self.batch_size) {
            out.extend(
                self.predict_batch(batch)
                    .await?
                    .into_iter()
                    .map(|item| match item {
                        MaiaItem::Probs(p) => Some(p),
                        _ => None,
                    }),
            );
        }
        Ok(out)
    }

    /// `(p_best, p_ok)` from Maia's move probabilities for one input with a best move.
    pub fn score(
        best_uci: &str,
        lines: &[(String, f64)],
        probs: &HashMap<String, f64>,
    ) -> (f64, f64) {
        let p = |uci: &str| probs.get(uci).copied().unwrap_or(0.0);
        let p_best = p(best_uci);
        let lines = dedupe_lines(lines);
        let p_ok = match best_win(&lines) {
            Some(best) => lines
                .iter()
                .filter(|(_, w)| *w >= best - OK_WINDOW)
                .map(|(uci, _)| p(uci))
                .sum(),
            None => p_best,
        };
        (p_best, p_ok.clamp(p_best, 1.0))
    }
}

/// The Task 1 trait: `None` for opening plies, inputs without a best move and items Maia
/// rejected or that failed validation; a failed call is `Err`. Production code uses
/// [`super::select::DifficultyEstimator`], which also says why an estimate is missing.
impl DifficultyProvider for MaiaDifficulty {
    fn model(&self) -> &str {
        &self.model
    }

    fn estimate<'a>(
        &'a self,
        inputs: &'a [DifficultyInput],
    ) -> BoxFuture<'a, Result<Vec<DifficultyEstimate>, DifficultyError>> {
        Box::pin(async move {
            let mut out: Vec<DifficultyEstimate> = vec![None; inputs.len()];
            let todo: Vec<usize> = (0..inputs.len())
                .filter(|&i| inputs[i].best_uci.is_some() && !is_maia_opening_ply(&inputs[i].fen))
                .collect();
            for idx in todo.chunks(self.batch_size) {
                let batch: Vec<&DifficultyInput> = idx.iter().map(|&i| &inputs[i]).collect();
                let items = self.predict_batch(&batch).await?;
                for (&i, item) in idx.iter().zip(items) {
                    if let (MaiaItem::Probs(probs), Some(best)) = (item, &inputs[i].best_uci) {
                        let lines = inputs[i].lines.as_deref().unwrap_or_default();
                        out[i] = Some(Self::score(best, lines, &probs));
                    }
                }
            }
            Ok(out)
        })
    }
}
