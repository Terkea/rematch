//! A Lichess game's weakness observations from Lichess's own evaluations (EXP-0009):
//! `cli lab observe`.
//!
//! Each game goes through the production path of a Lichess game with a server analysis, with no
//! engine and no database: `evaluate_game` with `lichess_evals` (Lichess's `[%eval]` after every
//! ply, put where production puts them) and the given searches for the positions Lichess does not
//! score (the start position), then `build_move_records` and `game_accuracy`, then the production
//! feature path `profile::analyse_features::compute_game_features` with no enrichment and no
//! opening explorer (`book_exit` falls back to `DEFAULT_BOOK_EXIT_PLY`, as production does without
//! an explorer), then `observe_game`.
//!
//! Only the keys whose detectors read nothing but the evaluations, the clocks, the board and the
//! result are returned ([`EVAL_ONLY_KEYS`]). The others need the enrichment (MultiPV lines, the
//! refutation and threat probes, Maia, the tablebase probe), which this path does not run. Their
//! detectors treat a missing input as unknown, but not always as no opportunity: W1 counts a
//! non-decisive move as an opportunity even without a refutation line, so without the enrichment
//! it would see opportunities and never an event. Returning them would be wrong, not just thin.
//!
//! A game that cannot be observed (an illegal PGN, a position neither Lichess nor the given
//! searches score, an evaluation list whose length is not the number of plies) gets an `error`
//! and no sides; the batch goes on.
//!
//! None of this changes the product: the manifest's components are only called.

use std::collections::{BTreeMap, HashMap};

use analysis::eval_cache::MemoryEvalCache;
use analysis::evaluate::{EvaluateDeps, evaluate_game};
use chess_core::game_accuracy::game_accuracy;
use chess_core::move_record::build_move_records;
use chess_core::pgn::parse_pgn;
use chess_core::score::win_percent;
use chess_core::time_control::classify_time_control;
use chess_core::{ClockInfo, Color, EngineScore, Judgement};
use engine::{AnalyseOptions, Analyser, AnalysisLine, AnalysisResult, EngineError};
use futures::FutureExt;
use futures::future::BoxFuture;
use profile::analyse_features::{FeatureInput, compute_game_features, player_score};
use profile_core::types::{GameContext, PlatformId, WeaknessKey};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::depth::GivenSearch;
use crate::lab::SCHEMA_VERSION;

/// The rankable keys whose detectors read only the evaluations, the clocks, the board and the
/// result (`profile_core::weakness::detectors`): W4 (mate scores before and after the move), W5
/// and W6 (winning and losing episodes, the result), W7 (decisive errors by ply 20, the win% at
/// the end of the opening), W9a and W9b (the clocks). Every other rankable key needs an
/// enrichment input.
pub const EVAL_ONLY_KEYS: [WeaknessKey; 8] = [
    WeaknessKey::W4Missed,
    WeaknessKey::W4Allowed,
    WeaknessKey::W5,
    WeaknessKey::W6,
    WeaknessKey::W7Traps,
    WeaknessKey::W7Prep,
    WeaknessKey::W9a,
    WeaknessKey::W9b,
];

/// One game of an `observe` request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveGame {
    pub id: String,
    pub pgn: String,
    pub base_seconds: i32,
    pub increment_seconds: i32,
    /// Lichess's evaluation after each ply, White's point of view (`games.lichess_evals`).
    pub lichess_evals: Vec<Option<EngineScore>>,
    /// The sides to observe.
    pub sides: Vec<Color>,
    #[serde(default)]
    pub white_rating: Option<i32>,
    #[serde(default)]
    pub black_rating: Option<i32>,
    #[serde(default)]
    pub termination: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveRequest {
    pub schema_version: String,
    pub games: Vec<ObserveGame>,
    /// The positions Lichess does not score (the start position), by FEN.
    pub searches: Vec<GivenSearch>,
}

/// One key's evidence in one game, as `observe_game` counts it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyCount {
    pub events: u32,
    pub opportunities: u32,
}

/// One side of one game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservedSide {
    pub color: Color,
    /// `game_accuracy` of the side, as the `analyse-game` job stores it; null when not finite.
    pub accuracy: Option<f64>,
    /// The side's moves with a move record.
    pub moves: u32,
    pub inaccuracies: u32,
    pub mistakes: u32,
    pub blunders: u32,
    /// The keys of [`EVAL_ONLY_KEYS`] with at least one opportunity, by their wire name.
    pub keys: BTreeMap<String, KeyCount>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObservedGame {
    pub id: String,
    pub error: Option<String>,
    pub sides: Vec<ObservedSide>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ObserveResponse {
    pub schema_version: String,
    pub algorithm_id: String,
    pub games: Vec<ObservedGame>,
}

/// The given searches as an engine; a position it was not given is an error.
struct Given<'a> {
    by_fen: &'a HashMap<&'a str, &'a GivenSearch>,
}

impl Analyser for Given<'_> {
    fn analyse<'a>(
        &'a self,
        fen: &'a str,
        _opts: AnalyseOptions,
    ) -> BoxFuture<'a, Result<AnalysisResult, EngineError>> {
        let out = match self.by_fen.get(fen) {
            Some(s) => Ok(AnalysisResult {
                lines: vec![AnalysisLine {
                    multipv: 1,
                    score: s.score,
                    pv: s.best_uci.iter().cloned().collect(),
                    depth: 0,
                    nodes: 0,
                }],
                wall: std::time::Duration::ZERO,
                cpu: None,
            }),
            None => Err(EngineError::InvalidFen {
                fen: fen.to_owned(),
                reason: "neither Lichess nor the request scores this position".to_owned(),
            }),
        };
        futures::future::ready(out).boxed()
    }
}

async fn observe_one(g: &ObserveGame, given: &Given<'_>) -> Result<Vec<ObservedSide>, String> {
    let parsed = parse_pgn(&g.pgn).map_err(|e| e.to_string())?;
    // One evaluation (or null) per ply: a shorter or longer list would put every later
    // evaluation on the wrong position without an error.
    if g.lichess_evals.len() != parsed.plies.len() {
        return Err(format!(
            "{} evaluations for {} plies",
            g.lichess_evals.len(),
            parsed.plies.len()
        ));
    }
    let cache = MemoryEvalCache::default();
    let deps = EvaluateDeps {
        analyser: given,
        cache: &cache,
        concurrency: 1,
        on_progress: None,
        cancel: None,
        deadline: None,
    };
    let evaluated = evaluate_game(&deps, &parsed, Some(&g.lichess_evals))
        .await
        .map_err(|e| e.to_string())?;
    let (base, inc) = (
        u32::try_from(g.base_seconds).map_err(|_| "base_seconds".to_owned())?,
        u32::try_from(g.increment_seconds).map_err(|_| "increment_seconds".to_owned())?,
    );
    let clock = ClockInfo {
        base_seconds: f64::from(base),
        increment_seconds: f64::from(inc),
    };
    let evals = evaluated.evals;
    let records = build_move_records(&parsed, &evals, Some(clock)).map_err(|e| e.to_string())?;
    let wins: Vec<f64> = evals.iter().map(|e| win_percent(e.score)).collect();
    let result = parsed.header("Result").unwrap_or("*").to_owned();
    let mut sides = Vec::with_capacity(g.sides.len());
    for &color in &g.sides {
        let accuracy = Some(game_accuracy(&wins, color)).filter(|a| a.is_finite());
        let (own, opp) = match color {
            Color::White => (g.white_rating, g.black_rating),
            Color::Black => (g.black_rating, g.white_rating),
        };
        let ctx = GameContext {
            game_id: Uuid::nil(),
            platform_game_id: g.id.clone(),
            platform: PlatformId::Lichess,
            played_at: OffsetDateTime::UNIX_EPOCH,
            time_class: classify_time_control(base, inc),
            base_seconds: Some(base),
            increment_seconds: Some(inc),
            rated: true,
            player_color: color,
            player_rating: own,
            opponent_rating: opp,
            result: result.clone(),
            score: player_score(&result, color),
            termination: g.termination.clone(),
            accuracy,
            protocol: engine::ANALYSIS_PROTOCOL.to_owned(),
        };
        let features = compute_game_features(FeatureInput {
            ctx: &ctx,
            parsed: &parsed,
            evals: &evals,
            records: &records,
            raw: &[],
            explorer: None,
        });
        let own_records = records.iter().filter(|r| r.color == color);
        let (mut moves, mut inaccuracies, mut mistakes, mut blunders) = (0, 0, 0, 0);
        for r in own_records {
            moves += 1;
            match r.judgement {
                Some(Judgement::Inaccuracy) => inaccuracies += 1,
                Some(Judgement::Mistake) => mistakes += 1,
                Some(Judgement::Blunder) => blunders += 1,
                None => {}
            }
        }
        let keys = EVAL_ONLY_KEYS
            .iter()
            .filter_map(|k| {
                features.observation.binomial.get(k).map(|b| {
                    (
                        k.as_str().to_owned(),
                        KeyCount {
                            events: b.events,
                            opportunities: b.opportunities,
                        },
                    )
                })
            })
            .collect();
        sides.push(ObservedSide {
            color,
            accuracy,
            moves,
            inaccuracies,
            mistakes,
            blunders,
            keys,
        });
    }
    Ok(sides)
}

/// `cli lab observe`.
pub async fn observe(req: &ObserveRequest, algorithm_id: &str) -> anyhow::Result<ObserveResponse> {
    anyhow::ensure!(
        req.schema_version == SCHEMA_VERSION,
        "schema_version {:?} is not {SCHEMA_VERSION:?}",
        req.schema_version
    );
    let by_fen: HashMap<&str, &GivenSearch> =
        req.searches.iter().map(|s| (s.fen.as_str(), s)).collect();
    let given = Given { by_fen: &by_fen };
    let mut games = Vec::with_capacity(req.games.len());
    for g in &req.games {
        let (error, sides) = match observe_one(g, &given).await {
            Ok(sides) => (None, sides),
            Err(e) => (Some(e), vec![]),
        };
        games.push(ObservedGame {
            id: g.id.clone(),
            error,
            sides,
        });
    }
    Ok(ObserveResponse {
        schema_version: SCHEMA_VERSION.to_owned(),
        algorithm_id: algorithm_id.to_owned(),
        games,
    })
}
