//! Opening explorer storage (R2 Task 17): per-build move counts by position (`fen_key`), band
//! and time class, written in batches by the dump build (Task 20) and read from the active
//! build only. Semantics of TS M2 Task 11 (`queryExplorer`, `loadExplorerIndex`, `inTheory`).

use std::collections::{BTreeMap, HashMap};

use chess_core::TimeClass;
use profile_core::types::Band;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::builds::{RefdataError, Result, ensure_building};

/// Rows per `INSERT … SELECT FROM UNNEST` statement in [`flush_explorer`].
pub const FLUSH_CHUNK: usize = 5_000;
/// Positions per query in [`load_explorer_index`].
pub const INDEX_BATCH: usize = 1_000;
/// One explorer move, the theory rule and its constants live in `profile-core` (pure, used by the
/// repertoire), so there is one copy of each (T18 review M1).
pub use profile_core::openings::repertoire::{
    ExplorerMove, THEORY_MIN_SHARE_PCT, THEORY_MIN_TOTAL, in_theory,
};

/// One explorer row key: the move `uci` played from position `fen_key` in `band` × `time_class`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExplorerKey {
    pub fen_key: String,
    pub band: Band,
    pub time_class: TimeClass,
    pub uci: String,
}

/// Game results after a move (White wins, draws, Black wins).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExplorerCounts {
    pub white: i64,
    pub draws: i64,
    pub black: i64,
}

/// One stored move row ([`ExplorerMove`] has no sqlx derive: `profile-core` has no sqlx).
#[derive(FromRow)]
struct MoveRow {
    uci: String,
    white: i64,
    draws: i64,
    black: i64,
    total: i64,
}

impl From<MoveRow> for ExplorerMove {
    fn from(r: MoveRow) -> Self {
        ExplorerMove {
            uci: r.uci,
            white: r.white,
            draws: r.draws,
            black: r.black,
            total: r.total,
        }
    }
}

/// The active explorer build's moves of one position, by total desc then uci.
/// Parameters: `$1` fen_key, `$2` band, `$3` time class (both as text).
pub const QUERY_EXPLORER_SQL: &str = "SELECT uci, white::int8 AS white, draws::int8 AS draws, \
       black::int8 AS black, (white::int8 + draws + black) AS total \
     FROM opening_explorer \
     WHERE build_id = (SELECT id FROM reference_builds \
                       WHERE kind = 'opening_explorer' AND status = 'active') \
       AND fen_key = $1 AND band = $2::band AND time_class = $3::time_class \
     ORDER BY total DESC, uci";

/// The active explorer build's moves of a batch of positions.
/// Parameters: `$1` fen_keys (`text[]`), `$2` band, `$3` time class.
pub const EXPLORER_INDEX_SQL: &str = "SELECT fen_key, uci, white::int8 AS white, \
       draws::int8 AS draws, black::int8 AS black, (white::int8 + draws + black) AS total \
     FROM opening_explorer \
     WHERE build_id = (SELECT id FROM reference_builds \
                       WHERE kind = 'opening_explorer' AND status = 'active') \
       AND fen_key = ANY($1::text[]) AND band = $2::band AND time_class = $3::time_class";

/// Adds counts to a `building` build's rows (`ON CONFLICT … DO UPDATE SET white = white +
/// excluded.white, …`), in one transaction and chunks of [`FLUSH_CHUNK`]. Repeated keys in
/// `rows` are summed first, and rows are written in key order so concurrent flushes lock rows
/// in the same order. Negative counts are rejected.
///
/// Not retry-safe: counts are added, so re-flushing a chunk after an error that may have
/// committed double-counts it. The caller (Task 20) must make each chunk idempotent, or fail the
/// build and start over.
pub async fn flush_explorer(
    pool: &PgPool,
    build_id: Uuid,
    rows: &[(ExplorerKey, ExplorerCounts)],
) -> Result<()> {
    let mut merged: BTreeMap<(&str, &str, &str, &str), ExplorerCounts> = BTreeMap::new();
    for (k, c) in rows {
        if c.white < 0 || c.draws < 0 || c.black < 0 {
            return Err(RefdataError::OutOfRange(format!(
                "negative explorer count for {} {}",
                k.fen_key, k.uci
            )));
        }
        let e = merged
            .entry((
                k.fen_key.as_str(),
                k.band.as_str(),
                k.time_class.as_str(),
                k.uci.as_str(),
            ))
            .or_default();
        e.white += c.white;
        e.draws += c.draws;
        e.black += c.black;
    }
    let mut tx = pool.begin().await?;
    ensure_building(&mut tx, build_id).await?;
    let merged: Vec<_> = merged.into_iter().collect();
    for chunk in merged.chunks(FLUSH_CHUNK) {
        let mut fens = Vec::with_capacity(chunk.len());
        let mut bands = Vec::with_capacity(chunk.len());
        let mut tcs = Vec::with_capacity(chunk.len());
        let mut ucis = Vec::with_capacity(chunk.len());
        let mut whites = Vec::with_capacity(chunk.len());
        let mut draws = Vec::with_capacity(chunk.len());
        let mut blacks = Vec::with_capacity(chunk.len());
        for ((fen, band, tc, uci), c) in chunk {
            fens.push(*fen);
            bands.push(*band);
            tcs.push(*tc);
            ucis.push(*uci);
            whites.push(c.white);
            draws.push(c.draws);
            blacks.push(c.black);
        }
        sqlx::query(
            "INSERT INTO opening_explorer \
               (build_id, fen_key, band, time_class, uci, white, draws, black) \
             SELECT $1, f, b::band, t::time_class, u, w::int4, d::int4, k::int4 \
             FROM UNNEST($2::text[], $3::text[], $4::text[], $5::text[], \
                         $6::int8[], $7::int8[], $8::int8[]) AS x(f, b, t, u, w, d, k) \
             ON CONFLICT (build_id, fen_key, band, time_class, uci) DO UPDATE SET \
               white = opening_explorer.white + excluded.white, \
               draws = opening_explorer.draws + excluded.draws, \
               black = opening_explorer.black + excluded.black",
        )
        .bind(build_id)
        .bind(&fens)
        .bind(&bands)
        .bind(&tcs)
        .bind(&ucis)
        .bind(&whites)
        .bind(&draws)
        .bind(&blacks)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Deletes a `building` build's rows whose total is below `min_row_count`; returns how many.
pub async fn prune_explorer(pool: &PgPool, build_id: Uuid, min_row_count: i64) -> Result<u64> {
    let mut tx = pool.begin().await?;
    ensure_building(&mut tx, build_id).await?;
    let n = sqlx::query(
        "DELETE FROM opening_explorer \
         WHERE build_id = $1 AND (white::int8 + draws + black) < $2",
    )
    .bind(build_id)
    .bind(min_row_count)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(n)
}

/// The active build's moves of one position (empty without an active build), total desc.
pub async fn query_explorer(
    pool: &PgPool,
    fen_key: &str,
    band: Band,
    tc: TimeClass,
) -> Result<Vec<ExplorerMove>> {
    let rows: Vec<MoveRow> = sqlx::query_as(QUERY_EXPLORER_SQL)
        .bind(fen_key)
        .bind(band.as_str())
        .bind(tc.as_str())
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(ExplorerMove::from).collect())
}

#[derive(FromRow)]
struct IndexRow {
    fen_key: String,
    #[sqlx(flatten)]
    mv: MoveRow,
}

/// The active build's moves for each known position among `fen_keys` (deduplicated, queried in
/// batches of [`INDEX_BATCH`]); unknown positions are absent. Each list is total desc, then uci.
pub async fn load_explorer_index(
    pool: &PgPool,
    fen_keys: &[String],
    band: Band,
    tc: TimeClass,
) -> Result<HashMap<String, Vec<ExplorerMove>>> {
    let mut keys: Vec<&str> = fen_keys.iter().map(String::as_str).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut index: HashMap<String, Vec<ExplorerMove>> = HashMap::new();
    if keys.is_empty() {
        return Ok(index);
    }
    // One snapshot for every batch, so a concurrent activation cannot mix two builds.
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    for batch in keys.chunks(INDEX_BATCH) {
        let rows: Vec<IndexRow> = sqlx::query_as(EXPLORER_INDEX_SQL)
            .bind(batch)
            .bind(band.as_str())
            .bind(tc.as_str())
            .fetch_all(&mut *tx)
            .await?;
        for r in rows {
            index.entry(r.fen_key).or_default().push(r.mv.into());
        }
    }
    tx.commit().await?;
    for moves in index.values_mut() {
        moves.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.uci.cmp(&b.uci)));
    }
    Ok(index)
}
