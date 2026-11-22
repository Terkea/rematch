//! Band reference aggregation (R2 Task 32, TS M2 Task 14 `aggregateBandReference`, ADR-R2-11).
//!
//! [`aggregate_band_reference`] turns the observed sides of a build's reference games into one
//! [`BandReferenceRow`] per band × time class × KPI catalogue row that has evidence:
//!
//! - **Rate rows** (weakness, motif and base rates): per-player `(k_i, n_i)` summed over the
//!   player's games in the cell. `pooled_rate = Σk / Σn`; a Beta-binomial fit
//!   (`fit_beta_binomial`) gives the per-player rate distribution, and `p25/p40/median/p75` are
//!   its quantiles (not raw per-player rates, which are noisy at 10 games). `epl_per_event =
//!   Σepl / Σk` for weakness rows (`None` without events: unknown, never 0);
//!   `opps_per_game = Σn / games with evidence`.
//! - **Mean rows** (continuous KPIs): the per-player mean of the game values, then type-7
//!   quantiles of those means. `pooled_rate`, `epl_per_event` and `opps_per_game` are `None`.
//! - `Last` rows (the rating) get no reference: a band's rating quartiles say nothing new.
//! - **`opp_slope_per_100`:** the OLS slope of the per-game value (a rate's per-game `k/n`) on
//!   `(opponent − own) / 100`. When it cannot be estimated (fewer than 2 games or no spread in
//!   the gap) it is `None` (stored NULL: no adjustment); never NaN and never a made-up 0.
//! - `n_players` counts players with evidence for the row (a rate needs `n_i > 0`);
//!   `low_sample = n_players < LOW_SAMPLE_PLAYERS`. Low-sample rows are stored but never served
//!   (Task 17).
//! - Sides whose metrics carry another `PLAYER_GAME_METRICS_VERSION` are stale and left out
//!   (counted), never read as zero evidence. Sessions are unknown for sampled games, so
//!   session-based rows get no reference.

use std::collections::BTreeMap;

use chess_core::TimeClass;
use profile_core::kpi::catalogue::{Extracted, KpiDefinition, KpiGame, KpiKind, kpi_catalogue};
use profile_core::stats::beta_binomial::fit_beta_binomial;
use profile_core::stats::quantiles::quantile;
use profile_core::types::{
    Band, BandReferenceStat, GameContext, MetricId, PLAYER_GAME_METRICS_VERSION, PlayerGameMetrics,
};
use refdata::band_reference::BandReferenceRow;

/// A row with fewer players is `low_sample` (never served).
pub const LOW_SAMPLE_PLAYERS: u32 = 10;
/// Below this sum of squared gap deviations (gaps in hundreds of points, ratings are integers,
/// so any real spread is at least 5e-5) the slope is not estimable.
pub const MIN_GAP_SPREAD: f64 = 1e-6;

/// One observed side of a reference game.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceSide {
    pub player_hash: String,
    pub band: Band,
    pub tc: TimeClass,
    pub player_rating: i32,
    pub opponent_rating: i32,
    pub metrics: PlayerGameMetrics,
    pub ctx: GameContext,
}

/// The rows and what was left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AggregateReport {
    pub rows: Vec<BandReferenceRow>,
    /// Sides with a stale metrics version.
    pub stale_sides: u32,
    /// Game contributions that were non-finite, inconsistent (`k > n`) or of the wrong kind.
    pub invalid_values: u32,
    /// Rows whose fit or quantiles failed (left out).
    pub failed_rows: u32,
}

/// See the module docs.
pub fn aggregate_band_reference(sides: &[ReferenceSide]) -> Vec<BandReferenceRow> {
    aggregate_band_reference_report(sides).rows
}

/// True when at least one weakness row is usable (not low-sample). A build without one would
/// only remove benchmarks, so it is never activated.
pub fn has_minimum_evidence(rows: &[BandReferenceRow]) -> bool {
    rows.iter()
        .any(|r| matches!(r.stat.metric, MetricId::Weakness(_)) && !r.stat.low_sample)
}

/// [`aggregate_band_reference`] with its skip counts. Rows are ordered by band, time class and
/// catalogue order.
pub fn aggregate_band_reference_report(sides: &[ReferenceSide]) -> AggregateReport {
    let mut report = AggregateReport::default();
    let mut cells: BTreeMap<(usize, &'static str), (Band, TimeClass, Vec<&ReferenceSide>)> =
        BTreeMap::new();
    for s in sides {
        if s.metrics.version != PLAYER_GAME_METRICS_VERSION {
            report.stale_sides += 1;
            continue;
        }
        let pos = Band::ALL.iter().position(|b| *b == s.band).unwrap_or(0);
        cells
            .entry((pos, s.tc.as_str()))
            .or_insert_with(|| (s.band, s.tc, Vec::new()))
            .2
            .push(s);
    }
    for (band, tc, cell) in cells.into_values() {
        for def in kpi_catalogue() {
            if def.kind == KpiKind::Last {
                continue;
            }
            if let Some(stat) = aggregate_row(def, &cell, Weighting::Games, &mut report) {
                report.rows.push(BandReferenceRow {
                    band,
                    time_class: tc,
                    stat,
                });
            }
        }
    }
    report
}

/// How a cell's games are pooled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Weighting {
    /// Every game counts once (the Lichess band reference: sampled players, a few games each).
    Games,
    /// Every player counts once (the peer benchmark, ADR 0080): each player's sums are divided
    /// by their games, so one frequent opponent cannot dominate the pooled rate, the EPL per
    /// event, the opportunities per game or the slope (one point per player). The Beta-binomial
    /// fit and the mean quantiles already take one value per player.
    Players,
}

#[derive(Default)]
struct PlayerAcc {
    k: f64,
    n: f64,
    epl: f64,
    sum: f64,
    values: u32,
    /// Games with evidence for the row, and the sum of their rating gaps.
    games: u32,
    gap: f64,
}

impl PlayerAcc {
    /// The per-game share of the player's sums (`Players` weighting).
    fn per_game(&self, x: f64) -> f64 {
        if self.games == 0 {
            0.0
        } else {
            x / f64::from(self.games)
        }
    }
}

pub(crate) fn aggregate_row(
    def: &KpiDefinition,
    cell: &[&ReferenceSide],
    weighting: Weighting,
    report: &mut AggregateReport,
) -> Option<BandReferenceStat> {
    let mut players: BTreeMap<&str, PlayerAcc> = BTreeMap::new();
    let mut slope_points: Vec<(f64, f64)> = Vec::new();
    let mut games = 0u32;
    for s in cell {
        let g = KpiGame {
            ctx: &s.ctx,
            metrics: &s.metrics,
            session: None,
        };
        let Some(x) = (def.extract)(&g) else {
            continue;
        };
        let gap = f64::from(s.opponent_rating - s.player_rating) / 100.0;
        match (def.kind, x) {
            (KpiKind::Mean, Extracted::Value(v)) if v.is_finite() => {
                let acc = players.entry(&s.player_hash).or_default();
                acc.sum += v;
                acc.values += 1;
                acc.games += 1;
                acc.gap += gap;
                slope_points.push((gap, v));
            }
            (KpiKind::Rate, Extracted::Counts(k, n))
                if k.is_finite() && n.is_finite() && k >= 0.0 && k <= n =>
            {
                let epl = match def.metric {
                    MetricId::Weakness(key) => s.metrics.weakness.get(&key).map_or(0.0, |o| o.epl),
                    _ => 0.0,
                };
                if !(epl.is_finite() && epl >= 0.0) {
                    report.invalid_values += 1;
                    continue;
                }
                let acc = players.entry(&s.player_hash).or_default();
                acc.k += k;
                acc.n += n;
                acc.epl += epl;
                acc.games += 1;
                acc.gap += gap;
                if n > 0.0 {
                    slope_points.push((gap, k / n));
                }
            }
            _ => {
                report.invalid_values += 1;
                continue;
            }
        }
        games += 1;
    }
    if weighting == Weighting::Players {
        // One point per player: their mean gap against their mean value (a rate's k / n).
        slope_points = players
            .values()
            .filter_map(|p| match def.kind {
                KpiKind::Mean if p.values > 0 => {
                    Some((p.per_game(p.gap), p.sum / f64::from(p.values)))
                }
                KpiKind::Rate if p.n > 0.0 => Some((p.per_game(p.gap), p.k / p.n)),
                _ => None,
            })
            .collect();
    }
    let slope = ols_slope(&slope_points);
    let stat = match def.kind {
        KpiKind::Rate => rate_stat(def.metric, &players, games, slope, weighting),
        KpiKind::Mean => mean_stat(def.metric, &players, games, slope),
        KpiKind::Last => None,
    };
    if stat.is_none() && !players.is_empty() && def.kind == KpiKind::Rate {
        // Players with counts but no row: either no opportunities at all (no evidence) or a fit
        // failure, counted by rate_stat's caller below.
        let any_opps = players.values().any(|p| p.n > 0.0);
        if any_opps {
            report.failed_rows += 1;
        }
    }
    stat
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn rate_stat(
    metric: MetricId,
    players: &BTreeMap<&str, PlayerAcc>,
    games: u32,
    slope: Option<f64>,
    weighting: Weighting,
) -> Option<BandReferenceStat> {
    let with: Vec<&PlayerAcc> = players.values().filter(|p| p.n > 0.0).collect();
    if with.is_empty() || games == 0 {
        return None;
    }
    let pairs: Vec<(f64, f64)> = with.iter().map(|p| (p.k, p.n)).collect();
    let dist = fit_beta_binomial(&pairs).ok()?.rates().ok()?;
    let q = |p: f64| dist.quantile(p).ok().filter(|x| x.is_finite());
    let (sum_k, sum_n, sum_epl) = match weighting {
        Weighting::Games => players.values().fold((0.0, 0.0, 0.0), |(k, n, e), p| {
            (k + p.k, n + p.n, e + p.epl)
        }),
        Weighting::Players => players.values().fold((0.0, 0.0, 0.0), |(k, n, e), p| {
            (
                k + p.per_game(p.k),
                n + p.per_game(p.n),
                e + p.per_game(p.epl),
            )
        }),
    };
    let epl_per_event = match metric {
        MetricId::Weakness(_) if sum_k > 0.0 => Some(sum_epl / sum_k),
        _ => None,
    };
    let opps_per_game = match weighting {
        Weighting::Games => sum_n / f64::from(games),
        Weighting::Players => {
            let units = players.values().filter(|p| p.games > 0).count();
            if units == 0 {
                return None;
            }
            sum_n / units as f64
        }
    };
    let n_players = count(with.len());
    Some(BandReferenceStat {
        metric,
        median: q(0.5)?,
        p25: q(0.25)?,
        p40: q(0.4)?,
        p75: q(0.75)?,
        pooled_rate: Some(sum_k / sum_n),
        epl_per_event,
        opps_per_game: Some(opps_per_game),
        opp_slope_per_100: slope,
        n_players,
        n_games: games,
        low_sample: n_players < LOW_SAMPLE_PLAYERS,
    })
}

fn mean_stat(
    metric: MetricId,
    players: &BTreeMap<&str, PlayerAcc>,
    games: u32,
    slope: Option<f64>,
) -> Option<BandReferenceStat> {
    let mut means: Vec<f64> = players
        .values()
        .filter(|p| p.values > 0)
        .map(|p| p.sum / f64::from(p.values))
        .filter(|m| m.is_finite())
        .collect();
    if means.is_empty() {
        return None;
    }
    means.sort_by(f64::total_cmp);
    let n_players = count(means.len());
    Some(BandReferenceStat {
        metric,
        median: quantile(&means, 0.5),
        p25: quantile(&means, 0.25),
        p40: quantile(&means, 0.4),
        p75: quantile(&means, 0.75),
        pooled_rate: None,
        epl_per_event: None,
        opps_per_game: None,
        opp_slope_per_100: slope,
        n_players,
        n_games: games,
        low_sample: n_players < LOW_SAMPLE_PLAYERS,
    })
}

/// The OLS slope of `y` on `x`; `None` with fewer than 2 points or no spread in `x`.
pub fn ols_slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let my = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (sxx, sxy) = points.iter().fold((0.0, 0.0), |(sxx, sxy), &(x, y)| {
        (sxx + (x - mx) * (x - mx), sxy + (x - mx) * (y - my))
    });
    if sxx.is_nan() || sxx <= MIN_GAP_SPREAD {
        return None;
    }
    let s = sxy / sxx;
    s.is_finite().then_some(s)
}
