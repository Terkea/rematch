//! Band reference storage (R2 Task 17): per build, band × time class × metric statistics
//! (`BandReferenceStat`), written by the finaliser (Task 32) and read from the active build.
//!
//! [`load_band_reference`] falls back **per metric** (ADR-R2-23, review ruling): each metric is
//! served from the nearest band, in the same time class, whose row for that metric is not
//! `low_sample`, at most [`MAX_FALLBACK_BANDS`] bands away; otherwise that metric is absent. A
//! low-sample row is never served. Equally near bands resolve to the lower one (unknown players
//! are more likely beginners). The result records the band each metric came from, so callers can
//! say "B2 benchmark" honestly. Rows with a metric this version does not know are ignored, so a
//! cell holding only unknown metrics falls back like an empty one. Statistics are stored as
//! `real`, so values read back are `f32` precision.

use std::collections::BTreeMap;

use chess_core::TimeClass;
use profile_core::types::{Band, BandReference, BandReferenceStat, MetricId};
use sqlx::{FromRow, PgConnection, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::builds::{RefdataError, Result, ensure_building};

/// A metric is served from at most this many bands away from the requested one.
pub const MAX_FALLBACK_BANDS: usize = 2;

/// The active build's usable (not low-sample) rows of a time class in a set of bands.
/// Parameters: `$1` bands (`band[]` as text), `$2` time class.
pub const BAND_REFERENCE_SQL: &str = "SELECT build_id, band::text AS band, metric, \
       median::float8 AS median, p25::float8 AS p25, p40::float8 AS p40, p75::float8 AS p75, \
       pooled_rate::float8 AS pooled_rate, epl_per_event::float8 AS epl_per_event, \
       opps_per_game::float8 AS opps_per_game, opp_slope_per_100::float8 AS opp_slope_per_100, \
       n_players, n_games, low_sample \
     FROM band_reference \
     WHERE build_id = (SELECT id FROM reference_builds \
                       WHERE kind = 'band_reference' AND status = 'active') \
       AND band = ANY($1::band[]) AND time_class = $2::time_class AND NOT low_sample";

/// One metric of one band × time class cell.
#[derive(Clone, Debug, PartialEq)]
pub struct BandReferenceRow {
    pub band: Band,
    pub time_class: TimeClass,
    pub stat: BandReferenceStat,
}

/// Upserts a `building` build's band reference rows inside the caller's transaction (the
/// build's share lock holds until it commits; a rewritten metric replaces its row).
pub async fn write_band_reference(
    tx: &mut Transaction<'_, Postgres>,
    build_id: Uuid,
    rows: &[BandReferenceRow],
) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let conn: &mut PgConnection = tx;
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
struct RefRow {
    build_id: Uuid,
    band: String,
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

/// One metric's statistics and the band they were served from.
#[derive(Clone, Debug, PartialEq)]
pub struct ServedStat {
    pub band: Band,
    pub stat: BandReferenceStat,
}

/// The active band reference for one requested band × time class.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedBandReference {
    pub build_id: Uuid,
    pub requested: Band,
    pub time_class: TimeClass,
    /// Only metrics with a usable row within [`MAX_FALLBACK_BANDS`].
    pub stats: BTreeMap<MetricId, ServedStat>,
}

impl LoadedBandReference {
    /// The statistics alone, as the pure profile code takes them.
    pub fn reference(&self) -> BandReference {
        self.stats
            .iter()
            .map(|(m, s)| (*m, s.stat.clone()))
            .collect()
    }

    /// The band a metric was served from, if it was served.
    pub fn served_from(&self, metric: MetricId) -> Option<Band> {
        self.stats.get(&metric).map(|s| s.band)
    }

    /// True when the metric came from another band than the requested one.
    pub fn is_fallback(&self, metric: MetricId) -> bool {
        self.served_from(metric)
            .is_some_and(|b| b != self.requested)
    }
}

fn band_pos(b: Band) -> usize {
    Band::ALL.iter().position(|x| *x == b).unwrap_or(0)
}

/// The active band reference of `band` × `tc` with the per-metric fallback of the module doc.
/// `None` without an active build or when no metric has a usable row within reach. One
/// statement, so one snapshot.
pub async fn load_band_reference(
    pool: &PgPool,
    band: Band,
    tc: TimeClass,
) -> Result<Option<LoadedBandReference>> {
    let at = band_pos(band);
    let bands: Vec<&str> = Band::ALL
        .iter()
        .filter(|b| band_pos(**b).abs_diff(at) <= MAX_FALLBACK_BANDS)
        .map(|b| b.as_str())
        .collect();
    let rows: Vec<RefRow> = sqlx::query_as(BAND_REFERENCE_SQL)
        .bind(&bands)
        .bind(tc.as_str())
        .fetch_all(pool)
        .await?;
    let Some(build_id) = rows.first().map(|r| r.build_id) else {
        return Ok(None);
    };
    // Best row per metric by (distance, band position): nearest, lower band on ties.
    let mut best: BTreeMap<MetricId, ((usize, usize), ServedStat)> = BTreeMap::new();
    for r in rows {
        let Ok(metric) = r.metric.parse::<MetricId>() else {
            tracing::warn!(metric = %r.metric, %build_id, "ignoring unknown band reference metric");
            continue;
        };
        let row_band: Band = r
            .band
            .parse()
            .map_err(|_| RefdataError::Decode(format!("band {:?}", r.band)))?;
        let key = (band_pos(row_band).abs_diff(at), band_pos(row_band));
        if best.get(&metric).is_some_and(|(k, _)| *k <= key) {
            continue;
        }
        let count = |n: i32| {
            u32::try_from(n).map_err(|_| RefdataError::Decode(format!("negative count {n}")))
        };
        let stat = BandReferenceStat {
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
        };
        best.insert(
            metric,
            (
                key,
                ServedStat {
                    band: row_band,
                    stat,
                },
            ),
        );
    }
    if best.is_empty() {
        return Ok(None);
    }
    Ok(Some(LoadedBandReference {
        build_id,
        requested: band,
        time_class: tc,
        stats: best.into_iter().map(|(m, (_, s))| (m, s)).collect(),
    }))
}
