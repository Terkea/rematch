//! Shared eval cache keyed by `(fen_key, protocol)`. Evals are stored from White's point of view.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use chess_core::{EngineScore, PositionEval};
use futures::future::BoxFuture;
use sqlx::{PgPool, Row};

/// Rows per query or insert.
pub const CACHE_CHUNK: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalSource {
    Stockfish,
    Lichess,
}

impl EvalSource {
    pub fn as_str(self) -> &'static str {
        match self {
            EvalSource::Stockfish => "stockfish",
            EvalSource::Lichess => "lichess",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvalCacheEntry {
    pub fen_key: String,
    pub protocol: String,
    /// White point of view.
    pub eval: PositionEval,
    pub source: EvalSource,
}

pub trait EvalCache: Send + Sync {
    fn get_many<'a>(
        &'a self,
        fen_keys: &'a [String],
        protocol: &'a str,
    ) -> BoxFuture<'a, Result<HashMap<String, PositionEval>, sqlx::Error>>;

    fn put_many<'a>(
        &'a self,
        entries: &'a [EvalCacheEntry],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>>;
}

/// In-memory cache; the first write for a key wins.
#[derive(Default)]
pub struct MemoryEvalCache {
    rows: Mutex<HashMap<(String, String), PositionEval>>,
}

impl EvalCache for MemoryEvalCache {
    fn get_many<'a>(
        &'a self,
        fen_keys: &'a [String],
        protocol: &'a str,
    ) -> BoxFuture<'a, Result<HashMap<String, PositionEval>, sqlx::Error>> {
        Box::pin(async move {
            let rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
            let mut out = HashMap::new();
            for k in fen_keys {
                if let Some(hit) = rows.get(&(protocol.to_string(), k.clone())) {
                    out.insert(k.clone(), hit.clone());
                }
            }
            Ok(out)
        })
    }

    fn put_many<'a>(
        &'a self,
        entries: &'a [EvalCacheEntry],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            let mut rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
            for e in entries {
                rows.entry((e.protocol.clone(), e.fen_key.clone()))
                    .or_insert_with(|| e.eval.clone());
            }
            Ok(())
        })
    }
}

pub struct DbEvalCache {
    pool: PgPool,
}

impl DbEvalCache {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const INSERT_SQL: &str = "INSERT INTO position_evals \
    (fen_key, protocol, score_cp, score_mate, best_uci, pv, source) \
    SELECT fen_key, protocol, score_cp, score_mate, best_uci, \
           string_to_array(pv, ' '), source::eval_source \
    FROM UNNEST($1::text[], $2::text[], $3::int[], $4::int[], $5::text[], $6::text[], $7::text[]) \
         AS t(fen_key, protocol, score_cp, score_mate, best_uci, pv, source) \
    ON CONFLICT DO NOTHING";

impl EvalCache for DbEvalCache {
    fn get_many<'a>(
        &'a self,
        fen_keys: &'a [String],
        protocol: &'a str,
    ) -> BoxFuture<'a, Result<HashMap<String, PositionEval>, sqlx::Error>> {
        Box::pin(async move {
            let mut out = HashMap::new();
            let mut seen = HashSet::new();
            let unique: Vec<&String> = fen_keys.iter().filter(|k| seen.insert(*k)).collect();
            for chunk in unique.chunks(CACHE_CHUNK) {
                let keys: Vec<&str> = chunk.iter().map(|k| k.as_str()).collect();
                let rows = sqlx::query(
                    "SELECT fen_key, score_cp, score_mate, best_uci, pv FROM position_evals \
                     WHERE protocol = $1 AND fen_key = ANY($2)",
                )
                .bind(protocol)
                .bind(&keys)
                .fetch_all(&self.pool)
                .await?;
                for r in rows {
                    let cp: Option<i32> = r.try_get("score_cp")?;
                    let mate: Option<i32> = r.try_get("score_mate")?;
                    let score = match (mate, cp) {
                        (Some(m), _) => EngineScore::Mate(m),
                        (None, Some(c)) => EngineScore::Cp(c),
                        (None, None) => continue,
                    };
                    out.insert(
                        r.try_get::<String, _>("fen_key")?,
                        PositionEval {
                            score,
                            best_uci: r.try_get("best_uci")?,
                            pv: r.try_get("pv")?,
                        },
                    );
                }
            }
            Ok(out)
        })
    }

    fn put_many<'a>(
        &'a self,
        entries: &'a [EvalCacheEntry],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            for chunk in entries.chunks(CACHE_CHUNK) {
                let fen: Vec<&str> = chunk.iter().map(|e| e.fen_key.as_str()).collect();
                let protocol: Vec<&str> = chunk.iter().map(|e| e.protocol.as_str()).collect();
                let cp: Vec<Option<i32>> = chunk
                    .iter()
                    .map(|e| match e.eval.score {
                        EngineScore::Cp(c) => Some(c),
                        EngineScore::Mate(_) => None,
                    })
                    .collect();
                let mate: Vec<Option<i32>> = chunk
                    .iter()
                    .map(|e| match e.eval.score {
                        EngineScore::Mate(m) => Some(m),
                        EngineScore::Cp(_) => None,
                    })
                    .collect();
                let best: Vec<Option<&str>> =
                    chunk.iter().map(|e| e.eval.best_uci.as_deref()).collect();
                let pv: Vec<String> = chunk.iter().map(|e| e.eval.pv.join(" ")).collect();
                let source: Vec<&str> = chunk.iter().map(|e| e.source.as_str()).collect();
                sqlx::query(INSERT_SQL)
                    .bind(&fen)
                    .bind(&protocol)
                    .bind(&cp)
                    .bind(&mate)
                    .bind(&best)
                    .bind(&pv)
                    .bind(&source)
                    .execute(&self.pool)
                    .await?;
            }
            Ok(())
        })
    }
}
