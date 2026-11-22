//! Cached probe over `position_tb` (R2 Task 23, ADR-R2-12).

use std::sync::Arc;

use futures::future::BoxFuture;
use profile_core::types::Wdl;
use sqlx::PgPool;

use super::covered_fen;
use crate::types::TablebaseProbe;

/// `position_tb.source` for answers from the local 3-4-5 set.
pub const TB_SOURCE: &str = "syzygy5";

/// Reads `position_tb` by `fen_key` and writes every `Some` answer of the inner probe.
/// Uncovered positions (see [`covered_fen`]) touch neither the database nor the inner probe.
/// Database errors are logged and fall through to the inner probe.
pub struct CachedTbProbe {
    inner: Arc<dyn TablebaseProbe>,
    pool: PgPool,
}

impl CachedTbProbe {
    pub fn new(inner: Arc<dyn TablebaseProbe>, pool: PgPool) -> Self {
        Self { inner, pool }
    }

    async fn probe_fen(&self, fen: &str) -> Option<Wdl> {
        let canon = covered_fen(fen, self.inner.max_pieces())?;
        let key = chess_core::fen::fen_key(&canon);
        match sqlx::query_scalar::<_, i16>("SELECT wdl FROM position_tb WHERE fen_key = $1")
            .bind(&key)
            .fetch_optional(&self.pool)
            .await
        {
            Ok(Some(w)) => return Wdl::try_from(w).ok(),
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "position_tb read failed"),
        }
        let wdl = self.inner.probe(&canon).await?;
        if let Err(err) = sqlx::query(
            "INSERT INTO position_tb (fen_key, wdl, source) VALUES ($1, $2, $3) \
             ON CONFLICT (fen_key) DO NOTHING",
        )
        .bind(&key)
        .bind(i16::from(wdl))
        .bind(TB_SOURCE)
        .execute(&self.pool)
        .await
        {
            tracing::warn!(%err, "position_tb write failed");
        }
        Some(wdl)
    }
}

impl TablebaseProbe for CachedTbProbe {
    fn max_pieces(&self) -> u32 {
        self.inner.max_pieces()
    }

    fn probe<'a>(&'a self, fen: &'a str) -> BoxFuture<'a, Option<Wdl>> {
        Box::pin(self.probe_fen(fen))
    }
}
