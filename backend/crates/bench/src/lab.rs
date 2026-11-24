//! The lab bridge (ADR 0106): the Rust algorithm as three JSON steps, so the Python lab (`lab/`)
//! drives the production code instead of a copy of it. `cli lab analyse|profile|rank` reads a
//! request on stdin and writes the response on stdout; the JSON Schemas are `lab/schemas/`.
//!
//! 1. [`analyse`] (games to facts): the games go through the production `analyse-game` handler
//!    with the profile hook and the production `build-profile` (the bench harness, ADR 0100) in
//!    a throwaway database, and the facts the profile is built from are read back: each binomial
//!    weakness key's series points, the games in the window, the stored peer benchmark, the
//!    evaluation baselines (ADR 0101), the band and the timeline's state per key.
//! 2. [`profile`] (facts to rates): `profile::estimates::compute_estimates`, the function the
//!    build calls, gives each player one estimate per binomial key with its 80% interval.
//! 3. [`rank`] (rates to a recommendation): `rank_weaknesses`, the ranking floor and the contract
//!    mapping (`ranked_weakness`) up to `LIMITS.weaknesses`, the coach's computed focuses and the
//!    drills of each, exactly as the CoachingInput builder and the report context compute them.
//! 4. [`tag_motifs`] (lines to motif tags, EXP-0007): each line goes through the production
//!    `derive_move_features` as a move's best line, which calls the motif tagger (`tag_line`), and
//!    the stored `best` analysis comes back. Pure: no engine, no database.
//!
//! What the lab does not reproduce, by design: the builder drops a ranked weakness that has no
//! stored exemplar position (it cannot be cited); `rank` has no positions, so it keeps it.
//! [`analyse`] checks its own facts: it runs [`profile`] on them and compares the result with the
//! `weakness_estimates` rows the build stored (`checks`).
//!
//! Every step is deterministic: no wall clock (the build's clock is the request's `now`), no
//! unseeded randomness, keys in sorted order. The `algorithm_id` is passed in by the caller.
//!
//! JSON names: the envelope and the lab's own objects are snake_case. The algorithm's own types
//! keep their serialisation (`WeaknessEstimate`, `BandReferenceStat` and the CoachingInput's
//! `RankedWeakness` are camelCase), so the lab reads exactly what the product stores.

use std::collections::BTreeMap;
use std::path::PathBuf;

use coaching_input::{LIMITS, RankedWeakness};
use profile::coaching::build_coaching_input_report;
use profile::coaching::mapping::ranked_weakness;
use profile::estimates::{compute_estimates, games_in_window, model_baselines};
use profile::load::{load_analysed_games, load_series_points};
use profile::peer_benchmark::load_account_benchmark;
use profile_core::stats::priority::{WeaknessEstimate, rank_weaknesses};
use profile_core::timeline::series::SeriesPoint;
use profile_core::types::{
    Band, BandReference, BandReferenceStat, SeriesKey, WeaknessKey, WeaknessState,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::generate::GeneratedGame;
use crate::harness::{Harness, TIME_CLASS, insert_account, insert_game_with_clock, set_now};
use crate::run::{Throwaway, memo_engines};

/// The version of the request and response shapes (`lab/schemas/*.schema.json`).
pub const SCHEMA_VERSION: &str = "lab-v1";
/// Engines `analyse` starts when the request does not say.
pub const DEFAULT_ENGINES: usize = 2;

fn check_version(v: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        v == SCHEMA_VERSION,
        "schema_version {v:?} is not supported (this cli speaks {SCHEMA_VERSION})"
    );
    Ok(())
}

// ---- facts ----------------------------------------------------------------------------------

/// One game's events and opportunities of one weakness key (a `weakness_series_point`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabPoint {
    #[serde(with = "time::serde::rfc3339")]
    pub played_at: OffsetDateTime,
    pub events: u32,
    pub opportunities: u32,
}

/// The timeline's view of one key at the build (`weakness_estimates.state` and `p_imp`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabOutcome {
    pub state: WeaknessState,
    #[serde(default)]
    pub p_imp: Option<f64>,
}

/// Everything the profile of one player is computed from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerFacts {
    pub id: String,
    /// The build's clock: ages and the 183-day window are measured from it.
    #[serde(with = "time::serde::rfc3339")]
    pub now: OffsetDateTime,
    pub band: Band,
    /// Analysed games at most 183 days before `now` (`MIN_GAMES` is 30).
    pub games_in_window: u32,
    /// Series points per binomial weakness key, oldest first.
    #[serde(default)]
    pub series: BTreeMap<WeaknessKey, Vec<LabPoint>>,
    /// The peer benchmark's served rows (`null`: no benchmark, nothing is compared).
    #[serde(default)]
    pub reference: Option<BandReference>,
    /// Evaluation baselines (ADR 0101) for keys the benchmark lacks (W5).
    #[serde(default)]
    pub model_reference: BTreeMap<WeaknessKey, BandReferenceStat>,
    /// The timeline state per key; a key without one is `not_an_issue`, as in the builder.
    #[serde(default)]
    pub outcomes: BTreeMap<WeaknessKey, LabOutcome>,
}

// ---- profile --------------------------------------------------------------------------------

/// `cli lab profile` input: facts (an `analyse` response is one).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileRequest {
    pub schema_version: String,
    /// The algorithm that produced the facts, when they came from `analyse` (informational).
    #[serde(default)]
    pub algorithm_id: Option<String>,
    pub players: Vec<PlayerFacts>,
}

/// One player's estimates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerProfile {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub now: OffsetDateTime,
    pub band: Band,
    pub games_in_window: u32,
    /// One per binomial key, in key order (`WeaknessEstimate`, camelCase).
    pub estimates: Vec<WeaknessEstimate>,
    #[serde(default)]
    pub outcomes: BTreeMap<WeaknessKey, LabOutcome>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub players: Vec<PlayerProfile>,
}

fn series_of(f: &PlayerFacts) -> anyhow::Result<BTreeMap<SeriesKey, Vec<SeriesPoint>>> {
    let mut out = BTreeMap::new();
    for (key, points) in &f.series {
        anyhow::ensure!(
            key.is_binomial(),
            "player {}: {} is not a binomial weakness key",
            f.id,
            key.as_str()
        );
        let pts = points
            .iter()
            .map(|p| {
                anyhow::ensure!(
                    p.events <= p.opportunities,
                    "player {}: {} has more events than opportunities",
                    f.id,
                    key.as_str()
                );
                Ok(SeriesPoint {
                    game_id: Uuid::nil(),
                    played_at: p.played_at,
                    events: p.events,
                    opportunities: p.opportunities,
                    protocol: String::new(),
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        out.insert(SeriesKey::Weakness(*key), pts);
    }
    Ok(out)
}

/// The estimates of one player's facts (`compute_estimates`, the build's own function).
pub fn profile_player(f: &PlayerFacts) -> anyhow::Result<PlayerProfile> {
    let series = series_of(f)?;
    let estimates = compute_estimates(
        &series,
        f.games_in_window,
        f.band,
        f.reference.as_ref(),
        &f.model_reference,
        f.now,
    );
    Ok(PlayerProfile {
        id: f.id.clone(),
        now: f.now,
        band: f.band,
        games_in_window: f.games_in_window,
        estimates,
        outcomes: f.outcomes.clone(),
    })
}

/// `cli lab profile`.
pub fn profile(req: &ProfileRequest, algorithm_id: &str) -> anyhow::Result<ProfileResponse> {
    check_version(&req.schema_version)?;
    Ok(ProfileResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        players: req
            .players
            .iter()
            .map(profile_player)
            .collect::<anyhow::Result<_>>()?,
    })
}

// ---- rank -----------------------------------------------------------------------------------

/// `cli lab rank` input: profiles (a `profile` response is one).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RankRequest {
    pub schema_version: String,
    #[serde(default)]
    pub algorithm_id: Option<String>,
    /// The first report allows 2 focuses, later ones 3 (`select_focuses`).
    #[serde(default)]
    pub first_report: bool,
    pub players: Vec<PlayerProfile>,
}

/// One player's recommendation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerRanking {
    pub id: String,
    /// The CoachingInput's ranked weaknesses (camelCase), with no exemplar positions.
    pub ranked: Vec<RankedWeakness>,
    /// The drills of each ranked weakness, by its key (`W3.shallow` style).
    pub drills: BTreeMap<String, Vec<String>>,
    /// The coach's computed focuses; `["general"]` when none qualifies.
    pub focuses: Vec<String>,
    /// The drill types each focus may use.
    pub focus_drills: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RankResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub players: Vec<PlayerRanking>,
}

fn drill_names(d: Vec<ai::output::drills::DrillType>) -> Vec<String> {
    d.into_iter().map(|d| d.as_str().to_owned()).collect()
}

/// The recommendation of one profile.
pub fn rank_player(p: &PlayerProfile, first_report: bool) -> PlayerRanking {
    let mut ranked: Vec<RankedWeakness> = Vec::new();
    for e in rank_weaknesses(p.estimates.clone()) {
        if ranked.len() >= LIMITS.weaknesses {
            break;
        }
        let (state, p_imp) = p
            .outcomes
            .get(&e.key)
            .map_or((WeaknessState::NotAnIssue, None), |o| (o.state, o.p_imp));
        let rank = u32::try_from(ranked.len() + 1).unwrap_or(u32::MAX);
        if let Some(w) = ranked_weakness(&e, state, p_imp, rank, Vec::new()) {
            ranked.push(w);
        }
    }
    let drills = ranked
        .iter()
        .map(|w| {
            let key = match &w.variant {
                Some(v) => format!("{}.{v}", w.id),
                None => w.id.clone(),
            };
            let list = ai::output::drills::drills_for(&w.id, w.variant.as_deref());
            (key, drill_names(list))
        })
        .collect();
    let mut focuses = ai::validator::focus::select_focuses_of(&ranked, first_report);
    if focuses.is_empty() {
        focuses.push(ai::validator::focus::GENERAL_FOCUS.to_owned());
    }
    let focus_drills = focuses
        .iter()
        .map(|f| {
            let list = ai::generate::context::allowed_drills_of(&ranked, f);
            (f.clone(), drill_names(list))
        })
        .collect();
    PlayerRanking {
        id: p.id.clone(),
        ranked,
        drills,
        focuses,
        focus_drills,
    }
}

/// `cli lab rank`.
pub fn rank(req: &RankRequest, algorithm_id: &str) -> anyhow::Result<RankResponse> {
    check_version(&req.schema_version)?;
    Ok(RankResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        players: req
            .players
            .iter()
            .map(|p| rank_player(p, req.first_report))
            .collect(),
    })
}

// ---- analyse --------------------------------------------------------------------------------

/// One game, in the importer's shape: a chess.com rapid game with `[%clk]` comments.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabGame {
    pub id: String,
    pub pgn: String,
    #[serde(with = "time::serde::rfc3339")]
    pub played_at: OffsetDateTime,
    /// `white` or `black`: the side the player had.
    pub player_color: String,
    pub white_name: String,
    pub black_name: String,
    pub white_rating: i32,
    pub black_rating: i32,
    /// `1-0`, `0-1` or `1/2-1/2`.
    pub result: String,
    pub termination: String,
    pub base_seconds: i32,
    #[serde(default)]
    pub increment_seconds: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysePlayer {
    pub id: String,
    /// The build's clock; default: the latest game plus one hour (as the benchmark does).
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub now: Option<OffsetDateTime>,
    pub games: Vec<LabGame>,
}

/// `cli lab analyse` input.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyseRequest {
    pub schema_version: String,
    /// Nodes per search; default the production `ENGINE_NODES` (300,000). The in-memory cache
    /// serves every search at this count, enrichment included (ADR 0100).
    #[serde(default)]
    pub nodes: Option<u64>,
    /// Stockfish processes; default [`DEFAULT_ENGINES`]. It changes the speed, never the facts.
    #[serde(default)]
    pub engines: Option<usize>,
    pub players: Vec<AnalysePlayer>,
}

/// Whether [`profile`] on a player's facts gives the estimates the build stored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerCheck {
    pub id: String,
    pub analysed_games: u32,
    pub profile_matches_build: bool,
    /// Keys whose recomputed mean, interval, events or priority differ from the stored row.
    pub mismatches: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyseResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub nodes: u64,
    pub players: Vec<PlayerFacts>,
    pub checks: Vec<PlayerCheck>,
}

pub struct AnalyseOptions {
    pub database_url: String,
    pub stockfish: PathBuf,
}

fn generated(g: &LabGame) -> GeneratedGame {
    GeneratedGame {
        platform_game_id: g.id.clone(),
        pgn: g.pgn.clone(),
        result: g.result.clone(),
        termination: g.termination.clone(),
        plies: u32::try_from(analysis::jobs::import::count_plies(&g.pgn)).unwrap_or(0),
        white_name: g.white_name.clone(),
        black_name: g.black_name.clone(),
        white_rating: g.white_rating,
        black_rating: g.black_rating,
        player_color: g.player_color.clone(),
        played_at: g.played_at,
        injected_plies: Vec::new(),
    }
}

type StoredRow = (String, f64, f64, f64, f64, f64, String, Option<f64>);

async fn stored_rows(pool: &sqlx::PgPool, account: Uuid) -> anyhow::Result<Vec<StoredRow>> {
    Ok(sqlx::query_as(
        "SELECT series_key, k_unweighted, mean, ci_low, ci_high, priority, state::text, p_imp \
         FROM weakness_estimates WHERE account_id = $1 AND time_class = $2::time_class \
           AND side = 'both' ORDER BY series_key",
    )
    .bind(account)
    .bind(TIME_CLASS.as_str())
    .fetch_all(pool)
    .await?)
}

async fn facts_of(
    pool: &sqlx::PgPool,
    id: &str,
    account: Uuid,
    now: OffsetDateTime,
) -> anyhow::Result<(PlayerFacts, PlayerCheck)> {
    let games = load_analysed_games(pool, account, TIME_CLASS)
        .await
        .map_err(|e| anyhow::anyhow!("loading games: {e}"))?;
    let points = load_series_points(pool, account, TIME_CLASS)
        .await
        .map_err(|e| anyhow::anyhow!("loading series: {e}"))?;
    let series = points
        .into_iter()
        .filter_map(|(k, pts)| match k {
            SeriesKey::Weakness(w) if w.is_binomial() => Some((
                w,
                pts.into_iter()
                    .map(|p| LabPoint {
                        played_at: p.played_at,
                        events: p.events,
                        opportunities: p.opportunities,
                    })
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    let reference = load_account_benchmark(pool, account, TIME_CLASS).await?;
    let band = build_coaching_input_report(pool, account, TIME_CLASS, now)
        .await
        .map_err(|e| anyhow::anyhow!("coaching input: {e}"))?
        .input
        .band;
    let rows = stored_rows(pool, account).await?;
    let outcomes = rows
        .iter()
        .filter_map(|r| {
            let key: WeaknessKey = r.0.parse().ok()?;
            let state: WeaknessState = r.6.parse().ok()?;
            Some((key, LabOutcome { state, p_imp: r.7 }))
        })
        .collect();
    let facts = PlayerFacts {
        id: id.to_owned(),
        now,
        band,
        games_in_window: games_in_window(games.iter().map(|(c, _)| c.played_at), now),
        series,
        reference,
        model_reference: model_baselines(&games, now),
        outcomes,
    };
    // The facts are complete only if the profile they give is the one the build stored.
    let recomputed = profile_player(&facts)?;
    let mut mismatches = Vec::new();
    for e in &recomputed.estimates {
        let row = rows.iter().find(|r| r.0 == e.key.as_str());
        let same = row.is_some_and(|r| {
            r.1 == e.k_unweighted
                && r.2 == e.posterior.mean
                && r.3 == e.posterior.interval.low
                && r.4 == e.posterior.interval.high
                && r.5 == e.priority
        });
        if !same {
            mismatches.push(e.key.as_str().to_owned());
        }
    }
    let check = PlayerCheck {
        id: id.to_owned(),
        analysed_games: u32::try_from(games.len()).unwrap_or(u32::MAX),
        profile_matches_build: mismatches.is_empty(),
        mismatches,
    };
    Ok((facts, check))
}

/// `cli lab analyse`: the games through the production analysis and build, in a throwaway
/// database next to `DATABASE_URL`'s, which is dropped afterwards. Platform chess.com and time
/// class rapid only, as the benchmark harness runs them.
pub async fn analyse(
    req: &AnalyseRequest,
    o: &AnalyseOptions,
    algorithm_id: &str,
    log: Box<dyn Fn(&str) + Send + Sync>,
) -> anyhow::Result<AnalyseResponse> {
    check_version(&req.schema_version)?;
    enrich::install_crypto_provider();
    let nodes = req.nodes.unwrap_or(engine::ENGINE_NODES);
    let engines = req.engines.unwrap_or(DEFAULT_ENGINES).max(1);
    anyhow::ensure!(nodes > 0, "nodes must be positive");
    for p in &req.players {
        anyhow::ensure!(!p.games.is_empty(), "player {} has no games", p.id);
    }
    let db = Throwaway::create(
        &o.database_url,
        u32::try_from(engines * 2 + 4).unwrap_or(16),
        &*log,
    )
    .await?;
    let outcome = async {
        let (pool, memo) = memo_engines(&o.stockfish, engines, nodes).await?;
        let harness = Harness {
            pool: db.pool.clone(),
            memo,
            engines,
            log,
        };
        profile::hook_impl::warm_openings().await;
        let deps = harness.analyse_deps();
        let mut players = Vec::new();
        let mut checks = Vec::new();
        let result = async {
            for (i, p) in req.players.iter().enumerate() {
                let account = insert_account(&db.pool, &format!("lab-{i}")).await?;
                let mut games: Vec<&LabGame> = p.games.iter().collect();
                games.sort_by(|a, b| a.played_at.cmp(&b.played_at).then(a.id.cmp(&b.id)));
                let mut ids = Vec::new();
                for g in &games {
                    let row = generated(g);
                    let (base, inc) = (g.base_seconds, g.increment_seconds);
                    ids.push(insert_game_with_clock(&db.pool, account, &row, "", base, inc).await?);
                }
                harness.analyse(&deps, &ids).await?;
                let last = games
                    .last()
                    .map(|g| g.played_at)
                    .unwrap_or(crate::suite::ANCHOR);
                let now = p.now.unwrap_or(last + time::Duration::hours(1));
                set_now(now);
                harness.build(account).await?;
                (harness.log)(&format!("{}: {} games analysed and built", p.id, ids.len()));
                let (facts, check) = facts_of(&db.pool, &p.id, account, now).await?;
                players.push(facts);
                checks.push(check);
            }
            anyhow::Ok(())
        }
        .await;
        engine::pool::EnginePool::close(&pool).await;
        result?;
        anyhow::Ok((players, checks))
    }
    .await;
    db.finish(false).await?;
    let (players, checks) = outcome?;
    Ok(AnalyseResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        nodes,
        players,
        checks,
    })
}

// ---- tag-motifs -----------------------------------------------------------------------------

/// One line to tag (`cli lab tag-motifs`, EXP-0007): a position, the moves that reach the tagged
/// position from it (`setup`, empty for none), and the line played from there (`pv`). A Lichess
/// puzzle is `fen` = its FEN, `setup` = its first move (the opponent's), `pv` = the rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotifLine {
    pub id: String,
    pub fen: String,
    #[serde(default)]
    pub setup: Vec<String>,
    pub pv: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagMotifsRequest {
    pub schema_version: String,
    pub lines: Vec<MotifLine>,
}

/// One tagged line. `fen` is the tagged position and `pov` the side to move there; both are null
/// when `fen` is invalid or `setup` cannot be played. `best` is the `best` line analysis the
/// production enrichment stores for a move played there with `pv` as the engine's best line
/// (`derive_move_features`): null exactly when production stores none (an empty or illegal line).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaggedLine {
    pub id: String,
    pub fen: Option<String>,
    pub pov: Option<chess_core::Color>,
    pub best: Option<profile_core::types::LineAnalysis>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TagMotifsResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub lines: Vec<TaggedLine>,
}

/// The position `setup` reaches from `fen`, or `None` when `fen` is invalid or a setup move is
/// illegal.
fn tagged_position(fen: &str, setup: &[String]) -> Option<String> {
    let start = chess_core::board::Position::from_fen(fen).ok()?;
    if setup.is_empty() {
        return Some(start.fen());
    }
    let steps = profile_core::board::play_line(fen, setup, None);
    if steps.len() != setup.len() {
        return None;
    }
    steps.last().map(|s| s.fen_after.clone())
}

/// One line through the production feature derivation: `pv` is the mover's best line, exactly
/// as `enrich` stores it (`RawMoveFeatures::best_pv`), so the tags are the ones the missed-motif
/// series (`motif.missed:<tag>`) and the drills read. No engine and no database.
pub fn tag_motif_line(l: &MotifLine) -> TaggedLine {
    let none = |fen: Option<String>| TaggedLine {
        id: l.id.clone(),
        fen,
        pov: None,
        best: None,
    };
    let Some(fen) = tagged_position(&l.fen, &l.setup) else {
        return none(None);
    };
    let Ok(pos) = chess_core::board::Position::from_fen(&fen) else {
        return none(None);
    };
    let pov = pos.side_to_move();
    let (uci, fen_after) = profile_core::board::play_line(&fen, &l.pv, Some(1))
        .into_iter()
        .next()
        .map(|s| (s.uci, s.fen_after))
        .unwrap_or_default();
    let raw = profile_core::types::RawMoveFeatures {
        ply: 0,
        multipv: None,
        best_pv: l.pv.clone(),
        refutation_pv: Vec::new(),
        threat: None,
        tb_wdl_before: None,
        tb_wdl_after: None,
        difficulty: None,
    };
    let features = profile_core::features::derive::derive_move_features(
        pov,
        &fen,
        &fen_after,
        &uci,
        Some(&raw),
    );
    TaggedLine {
        id: l.id.clone(),
        fen: Some(fen),
        pov: Some(pov),
        best: features.best,
    }
}

/// `cli lab tag-motifs`.
pub fn tag_motifs(req: &TagMotifsRequest, algorithm_id: &str) -> anyhow::Result<TagMotifsResponse> {
    check_version(&req.schema_version)?;
    Ok(TagMotifsResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        lines: req.lines.iter().map(tag_motif_line).collect(),
    })
}
