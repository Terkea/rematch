//! Enrichment storage (R2 Task 25): the MultiPV and threat-probe caches, keyed by
//! `fen_key` under their own protocol ids. Never touches `position_evals`.

use std::collections::HashMap;
use std::sync::Mutex;

use chess_core::EngineScore;
use futures::future::BoxFuture;
use profile_core::types::MultipvLine;
use sqlx::PgPool;
use sqlx::types::Json;

use crate::protocol::{MULTIPV_PROTOCOL, THREAT_PROTOCOL};

/// A cached threat probe: the opponent's score and PV on the null-move position.
pub type ThreatEntry = (EngineScore, Vec<String>);

pub trait EnrichmentStore: Send + Sync {
    fn get_multipv<'a>(
        &'a self,
        keys: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, Vec<MultipvLine>>, sqlx::Error>>;

    /// Stores new entries; an existing key is kept (first write wins).
    fn put_multipv<'a>(
        &'a self,
        entries: &'a [(String, Vec<MultipvLine>)],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>>;

    fn get_threat<'a>(
        &'a self,
        keys: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, ThreatEntry>, sqlx::Error>>;

    /// Stores new entries; an existing key is kept (first write wins).
    fn put_threat<'a>(
        &'a self,
        entries: &'a [(String, ThreatEntry)],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>>;
}

/// `position_threats` columns: fen_key, score_cp, score_mate, pv.
type ThreatRow = (String, Option<i32>, Option<i32>, Vec<String>);

/// `position_multipv` / `position_threats`.
#[derive(Clone)]
pub struct DbEnrichmentStore {
    pool: PgPool,
}

impl DbEnrichmentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn split_score(s: EngineScore) -> (Option<i32>, Option<i32>) {
    match s {
        EngineScore::Cp(cp) => (Some(cp), None),
        EngineScore::Mate(m) => (None, Some(m)),
    }
}

impl EnrichmentStore for DbEnrichmentStore {
    fn get_multipv<'a>(
        &'a self,
        keys: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, Vec<MultipvLine>>, sqlx::Error>> {
        Box::pin(async move {
            if keys.is_empty() {
                return Ok(HashMap::new());
            }
            let rows: Vec<(String, Json<Vec<MultipvLine>>)> = sqlx::query_as(
                "SELECT fen_key, lines FROM position_multipv \
                 WHERE protocol = $1 AND fen_key = ANY($2)",
            )
            .bind(MULTIPV_PROTOCOL)
            .bind(keys)
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|(k, Json(l))| (k, l)).collect())
        })
    }

    fn put_multipv<'a>(
        &'a self,
        entries: &'a [(String, Vec<MultipvLine>)],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            if entries.is_empty() {
                return Ok(());
            }
            let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
            let lines: Vec<serde_json::Value> = entries
                .iter()
                .map(|(_, l)| serde_json::to_value(l).map_err(|e| sqlx::Error::Encode(e.into())))
                .collect::<Result<_, _>>()?;
            sqlx::query(
                "INSERT INTO position_multipv (fen_key, protocol, lines) \
                 SELECT k, $1, l FROM UNNEST($2::text[], $3::jsonb[]) AS t(k, l) \
                 ON CONFLICT (fen_key, protocol) DO NOTHING",
            )
            .bind(MULTIPV_PROTOCOL)
            .bind(&keys)
            .bind(&lines)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn get_threat<'a>(
        &'a self,
        keys: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, ThreatEntry>, sqlx::Error>> {
        Box::pin(async move {
            if keys.is_empty() {
                return Ok(HashMap::new());
            }
            let rows: Vec<ThreatRow> = sqlx::query_as(
                "SELECT fen_key, score_cp, score_mate, pv FROM position_threats \
                 WHERE protocol = $1 AND fen_key = ANY($2)",
            )
            .bind(THREAT_PROTOCOL)
            .bind(keys)
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .filter_map(|(k, cp, mate, pv)| {
                    let score = match (cp, mate) {
                        (Some(cp), None) => EngineScore::Cp(cp),
                        (None, Some(m)) => EngineScore::Mate(m),
                        // A malformed row is a cache miss: probe again.
                        _ => return None,
                    };
                    Some((k, (score, pv)))
                })
                .collect())
        })
    }

    fn put_threat<'a>(
        &'a self,
        entries: &'a [(String, ThreatEntry)],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            if entries.is_empty() {
                return Ok(());
            }
            let keys: Vec<&str> = entries.iter().map(|(k, _)| k.as_str()).collect();
            let (cps, mates): (Vec<Option<i32>>, Vec<Option<i32>>) =
                entries.iter().map(|(_, (s, _))| split_score(*s)).unzip();
            // text[][] cannot hold ragged PVs, so each PV travels as a jsonb array.
            let pvs: Vec<serde_json::Value> = entries
                .iter()
                .map(|(_, (_, pv))| serde_json::Value::from(pv.clone()))
                .collect();
            sqlx::query(
                "INSERT INTO position_threats (fen_key, protocol, score_cp, score_mate, pv) \
                 SELECT k, $1, cp, mate, ARRAY(SELECT jsonb_array_elements_text(pv)) \
                 FROM UNNEST($2::text[], $3::int4[], $4::int4[], $5::jsonb[]) AS t(k, cp, mate, pv) \
                 ON CONFLICT (fen_key, protocol) DO NOTHING",
            )
            .bind(THREAT_PROTOCOL)
            .bind(&keys)
            .bind(&cps)
            .bind(&mates)
            .bind(&pvs)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }
}

/// In-process store for tests and one-off runs.
#[derive(Default)]
pub struct MemoryEnrichmentStore {
    multipv: Mutex<HashMap<String, Vec<MultipvLine>>>,
    threat: Mutex<HashMap<String, ThreatEntry>>,
}

fn pick<V: Clone>(map: &Mutex<HashMap<String, V>>, keys: &[String]) -> HashMap<String, V> {
    let map = map.lock().unwrap_or_else(|e| e.into_inner());
    keys.iter()
        .filter_map(|k| map.get(k).map(|v| (k.clone(), v.clone())))
        .collect()
}

fn insert<V: Clone>(map: &Mutex<HashMap<String, V>>, entries: &[(String, V)]) {
    let mut map = map.lock().unwrap_or_else(|e| e.into_inner());
    for (k, v) in entries {
        map.entry(k.clone()).or_insert_with(|| v.clone());
    }
}

impl EnrichmentStore for MemoryEnrichmentStore {
    fn get_multipv<'a>(
        &'a self,
        keys: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, Vec<MultipvLine>>, sqlx::Error>> {
        Box::pin(async move { Ok(pick(&self.multipv, keys)) })
    }

    fn put_multipv<'a>(
        &'a self,
        entries: &'a [(String, Vec<MultipvLine>)],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            insert(&self.multipv, entries);
            Ok(())
        })
    }

    fn get_threat<'a>(
        &'a self,
        keys: &'a [String],
    ) -> BoxFuture<'a, Result<HashMap<String, ThreatEntry>, sqlx::Error>> {
        Box::pin(async move { Ok(pick(&self.threat, keys)) })
    }

    fn put_threat<'a>(
        &'a self,
        entries: &'a [(String, ThreatEntry)],
    ) -> BoxFuture<'a, Result<(), sqlx::Error>> {
        Box::pin(async move {
            insert(&self.threat, entries);
            Ok(())
        })
    }
}
