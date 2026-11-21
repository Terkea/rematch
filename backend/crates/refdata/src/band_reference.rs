//! Band reference storage (R2 Task 17): per build, band × time class × metric statistics
//! (`BandReferenceStat`), written by the finaliser (Task 32) and read from the active build.
//!
//! [`load_band_reference`] follows TS M2 Task 14's `loadBandReference`: a low-sample cell falls
//! back to the nearest band with the same time class that is not low-sample, else `None`. A
//! cell is low-sample when it has no rows or every row in it is `low_sample`; a usable cell is
//! returned whole, and its rows keep their own flags. Equally near bands resolve to the lower
//! one (unknown players are more likely beginners). Statistics are stored as `real`, so values
//! read back are `f32` precision.

use chess_core::TimeClass;
use profile_core::types::{Band, BandReference, BandReferenceStat, MetricId};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::builds::{BuildKind, RefdataError, Result, ensure_building};

/// One cell's rows of a build. Parameters: `$1` build id, `$2` band, `$3` time class.
pub const BAND_CELL_SQL: &str = "SELECT metric, median::float8 AS median, p25::float8 AS p25, \
       p40::float8 AS p40, p75::float8 AS p75, pooled_rate::float8 AS pooled_rate, \
       epl_per_event::float8 AS epl_per_event, opps_per_game::float8 AS opps_per_game, \
       opp_slope_per_100::float8 AS opp_slope_per_100, n_players, n_games, low_sample \
     FROM band_reference \
     WHERE build_id = $1 AND band = $2::band AND time_class = $3::time_class";

/// One metric of one band × time class cell.
#[derive(Clone, Debug, PartialEq)]
pub struct BandReferenceRow {
    pub band: Band,
    pub time_class: TimeClass,
    pub stat: BandReferenceStat,
}

/// Upserts a `building` build's band reference rows on the caller's connection or transaction
/// (a rewritten metric replaces its row).
pub async fn write_band_reference(
    conn: &mut PgConnection,
    build_id: Uuid,
    rows: &[BandReferenceRow],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    ensure_building(conn, build_id).await?;
    let count = |n: u32, what: &str| {
        i32::try_from(n).map_err(|_| RefdataError::OutOfRange(format!("{what} {n}")))
    };
    let mut n_players = Vec::with_capacity(rows.len());
    let mut n_games = Vec::with_capacity(rows.len());
    for r in rows {
        n_players.push(count(r.stat.n_players, "n_players")?);
        n_games.push(count(r.stat.n_games, "n_games")?);
    }
    let f = |g: fn(&BandReferenceStat) -> f64| rows.iter().map(|r| g(&r.stat)).collect::<Vec<_>>();
    let o = |g: fn(&BandReferenceStat) -> Option<f64>| {
        rows.iter().map(|r| g(&r.stat)).collect::<Vec<_>>()
    };
    sqlx::query(
        "INSERT INTO band_reference (build_id, band, time_class, metric, median, p25, p40, p75, \
           pooled_rate, epl_per_event, opps_per_game, opp_slope_per_100, n_players, n_games, \
           low_sample) \
         SELECT $1, b::band, tc::time_class, m, med, q25, q40, q75, pr, epl, opg, slope, np, ng, \
           low \
         FROM UNNEST($2::text[], $3::text[], $4::text[], $5::float8[], $6::float8[], \
                     $7::float8[], $8::float8[], $9::float8[], $10::float8[], $11::float8[], \
                     $12::float8[], $13::int4[], $14::int4[], $15::bool[]) \
           AS x(b, tc, m, med, q25, q40, q75, pr, epl, opg, slope, np, ng, low) \
         ON CONFLICT (build_id, band, time_class, metric) DO UPDATE SET \
           median = excluded.median, p25 = excluded.p25, p40 = excluded.p40, \
           p75 = excluded.p75, pooled_rate = excluded.pooled_rate, \
           epl_per_event = excluded.epl_per_event, opps_per_game = excluded.opps_per_game, \
           opp_slope_per_100 = excluded.opp_slope_per_100, n_players = excluded.n_players, \
           n_games = excluded.n_games, low_sample = excluded.low_sample",
    )
    .bind(build_id)
    .bind(rows.iter().map(|r| r.band.as_str()).collect::<Vec<_>>())
    .bind(
        rows.iter()
            .map(|r| r.time_class.as_str())
            .collect::<Vec<_>>(),
    )
    .bind(
        rows.iter()
            .map(|r| r.stat.metric.to_string())
            .collect::<Vec<_>>(),
    )
    .bind(f(|s| s.median))
    .bind(f(|s| s.p25))
    .bind(f(|s| s.p40))
    .bind(f(|s| s.p75))
    .bind(o(|s| s.pooled_rate))
    .bind(o(|s| s.epl_per_event))
    .bind(o(|s| s.opps_per_game))
    .bind(f(|s| s.opp_slope_per_100))
    .bind(n_players)
    .bind(n_games)
    .bind(rows.iter().map(|r| r.stat.low_sample).collect::<Vec<_>>())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[derive(FromRow)]
struct CellRow {
    metric: String,
    median: f64,
    p25: f64,
    p40: f64,
    p75: f64,
    pooled_rate: Option<f64>,
    epl_per_event: Option<f64>,
    opps_per_game: Option<f64>,
    opp_slope_per_100: f64,
    n_players: i32,
    n_games: i32,
    low_sample: bool,
}

/// Bands in fallback order for `band`: itself, then by distance, the lower band first on ties.
fn fallback_order(band: Band) -> Vec<Band> {
    let pos = |b: Band| Band::ALL.iter().position(|x| *x == b).unwrap_or(0) as i64;
    let at = pos(band);
    let mut order = Band::ALL.to_vec();
    order.sort_by_key(|b| ((pos(*b) - at).abs(), pos(*b)));
    order
}

/// The active band-reference build and the reference of `band` × `tc`, with the low-sample
/// fallback described in the module doc. `None` without an active build or a usable cell.
/// Reads one consistent snapshot (a repeatable-read, read-only transaction), one cell query per
/// band tried. Rows whose metric this version does not know are skipped with a warning.
pub async fn load_band_reference(
    pool: &PgPool,
    band: Band,
    tc: TimeClass,
) -> Result<Option<(Uuid, BandReference)>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let build: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM reference_builds WHERE kind = $1::reference_kind AND status = 'active'",
    )
    .bind(BuildKind::BandReference.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(build) = build else {
        return Ok(None);
    };
    for b in fallback_order(band) {
        let rows: Vec<CellRow> = sqlx::query_as(BAND_CELL_SQL)
            .bind(build)
            .bind(b.as_str())
            .bind(tc.as_str())
            .fetch_all(&mut *tx)
            .await?;
        if rows.is_empty() || rows.iter().all(|r| r.low_sample) {
            continue;
        }
        let mut reference = BandReference::new();
        for r in rows {
            let Ok(metric) = r.metric.parse::<MetricId>() else {
                tracing::warn!(metric = %r.metric, %build, "skipping unknown band reference metric");
                continue;
            };
            let count = |n: i32| {
                u32::try_from(n).map_err(|_| RefdataError::Decode(format!("negative count {n}")))
            };
            reference.insert(
                metric,
                BandReferenceStat {
                    metric,
                    median: r.median,
                    p25: r.p25,
                    p40: r.p40,
                    p75: r.p75,
                    pooled_rate: r.pooled_rate,
                    epl_per_event: r.epl_per_event,
                    opps_per_game: r.opps_per_game,
                    opp_slope_per_100: r.opp_slope_per_100,
                    n_players: count(r.n_players)?,
                    n_games: count(r.n_games)?,
                    low_sample: r.low_sample,
                },
            );
        }
        tx.commit().await?;
        return Ok(Some((build, reference)));
    }
    tx.commit().await?;
    Ok(None)
}
