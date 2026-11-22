//! Features, metrics and series at analysis time (R2 Task 31, TS M2 Task 24
//! `computeGameFeatures`).
//!
//! [`compute_game_features`] is pure: it turns one analysed game (main evals, move records and
//! the enrichment's raw features) into the enriched moves, the derived per-move features, the
//! player's [`PlayerGameMetrics`], the weakness [`GameObservation`] and the opening facts.
//!
//! - The book exit comes from the explorer (`book_exit`); without theory evidence it is `None`.
//! - Phases are recomputed with `detect_phases(fens, book_exit_ply.unwrap_or(20))`, so the
//!   opening phase follows the book exit (R1's records used the fixed default).
//! - The opening line is the CC0 dataset classification (`classify_line`); the platform's
//!   ECO and name are only a fallback for the stored name ([`opening_name`], T16 ruling).
//! - Inputs that are skipped anywhere in the pipeline are counted ([`FeatureCounts`]) for the
//!   `enrich-game` job metrics; nothing is substituted for them.

use std::collections::BTreeMap;

use chess_core::phase::detect_phases;
use chess_core::{Color, MoveRecord, ParsedGame, PositionEval, TimeClass};
use db::games::GameRow;
use profile_core::features::assemble_enriched_moves_counted;
use profile_core::openings::lines::classify_line;
use profile_core::openings::repertoire::{ExplorerIndex, RepertoireLine, book_exit, opening_plies};
use profile_core::types::{
    AnalysedGame, EnrichedMove, GameContext, GameObservation, MoveFeatures, PlatformId,
    PlayerGameMetrics, RawMoveFeatures,
};
use profile_core::weakness::game_metrics::compute_game_metrics_counted;
use profile_core::weakness::observe_game::{T_WIN, observe_game_counted};
use profile_core::weakness::sessions::{PreGameInput, pre_game_ratings};
use profile_core::weakness::strengths::extract_strength_plies_counted;

/// The opening phase lasts at least this many plies when there is no book exit (TS default).
pub const DEFAULT_BOOK_EXIT_PLY: u32 = 20;

/// One game's inputs.
pub struct FeatureInput<'a> {
    pub ctx: &'a GameContext,
    pub parsed: &'a ParsedGame,
    /// `plies + 1` main evals, White's view.
    pub evals: &'a [PositionEval],
    pub records: &'a [MoveRecord],
    /// One entry per record (`None` for colours not enriched).
    pub raw: &'a [Option<RawMoveFeatures>],
    /// The explorer of the player's band; `None` (or empty) gives no book exit.
    pub explorer: Option<&'a ExplorerIndex>,
}

/// Inputs left out on the way, by stage and reason (logged and stored in `job_metrics.stages`).
pub type FeatureCounts = BTreeMap<String, f64>;

/// What [`compute_game_features`] derives.
#[derive(Clone, Debug, PartialEq)]
pub struct GameFeatures {
    /// The assembled moves (both colours), with the recomputed phases.
    pub enriched: Vec<EnrichedMove>,
    /// One entry per record (default, all `None`, for a record that was skipped).
    pub derived: Vec<MoveFeatures>,
    pub metrics: PlayerGameMetrics,
    pub observation: GameObservation,
    pub book_exit_ply: Option<u32>,
    /// The player's first own deviation from theory (`book_exit`), when it was the player's.
    pub player_deviation_ply: Option<u32>,
    /// The classified line's key (R-7), `None` when no position is named.
    pub line_key: Option<String>,
    pub opening: Option<RepertoireLine>,
    pub counts: FeatureCounts,
}

fn add(counts: &mut FeatureCounts, key: &str, n: u32) {
    counts.insert(key.to_owned(), f64::from(n));
}

/// See the module docs.
pub fn compute_game_features(i: FeatureInput<'_>) -> GameFeatures {
    let player = i.ctx.player_color;
    let plies = opening_plies(&i.parsed.plies);
    let (book_exit_ply, player_deviation_ply) = match i.explorer {
        Some(index) => book_exit(&plies, index, player),
        None => (None, None),
    };
    let fens: Vec<&str> = i.records.iter().map(|r| r.fen_before.as_str()).collect();
    let phases = detect_phases(&fens, book_exit_ply.unwrap_or(DEFAULT_BOOK_EXIT_PLY));
    let records: Vec<MoveRecord> = i
        .records
        .iter()
        .zip(phases)
        .map(|(r, phase)| MoveRecord { phase, ..r.clone() })
        .collect();

    let assembly = assemble_enriched_moves_counted(&records, i.evals, i.raw, i.parsed.terminal);
    let opening = classify_line(&i.parsed.plies, player);
    let line_key = opening.as_ref().map(|l| l.key.clone());
    let game = AnalysedGame {
        ctx: i.ctx.clone(),
        moves: assembly.moves,
        book_exit_ply,
        opening_line_key: line_key.clone(),
    };
    let obs = observe_game_counted(&game, T_WIN);
    let strengths = extract_strength_plies_counted(&game);
    let scan = compute_game_metrics_counted(&game, &obs.observation, strengths.plies);

    let by_ply: BTreeMap<u32, &MoveFeatures> =
        game.moves.iter().map(|m| (m.rec.ply, &m.f)).collect();
    let derived = records
        .iter()
        .map(|r| by_ply.get(&r.ply).map(|f| (*f).clone()).unwrap_or_default())
        .collect();

    let s = assembly.skipped;
    let mut counts = FeatureCounts::new();
    add(
        &mut counts,
        "skipped_moves_without_evals",
        s.moves_without_evals,
    );
    add(
        &mut counts,
        "skipped_fen_after_unplayable",
        s.fen_after_unplayable,
    );
    add(&mut counts, "skipped_raw_ply_mismatch", s.raw_ply_mismatch);
    add(
        &mut counts,
        "skipped_difficulty_rejected",
        s.difficulty_rejected,
    );
    add(
        &mut counts,
        "skipped_evals_out_of_range",
        s.evals_out_of_range,
    );
    add(
        &mut counts,
        "skipped_moves_with_invalid_fen",
        s.moves_with_invalid_fen,
    );
    add(
        &mut counts,
        "skipped_threat_fen_mismatch",
        s.threat_fen_mismatch,
    );
    add(
        &mut counts,
        "skipped_raw_beyond_records",
        s.raw_beyond_records,
    );
    add(&mut counts, "observation_skipped_moves", obs.skipped_moves);
    add(
        &mut counts,
        "observation_invalid_clocks",
        obs.invalid_clocks,
    );
    add(
        &mut counts,
        "observation_invalid_time_spent",
        obs.invalid_time_spent,
    );
    add(
        &mut counts,
        "observation_invalid_tablebase",
        obs.invalid_tablebase,
    );
    add(
        &mut counts,
        "observation_invalid_difficulty",
        obs.invalid_difficulty,
    );
    add(
        &mut counts,
        "observation_moves_without_best_line",
        obs.moves_without_best_line,
    );
    add(
        &mut counts,
        "observation_duplicate_plies",
        obs.duplicate_plies,
    );
    add(
        &mut counts,
        "observation_invalid_score",
        u32::from(obs.invalid_score),
    );
    add(
        &mut counts,
        "strengths_skipped_own_plies",
        strengths.skipped_own_plies,
    );
    add(
        &mut counts,
        "strengths_invalid_gaps",
        strengths.invalid_gaps,
    );
    add(
        &mut counts,
        "metrics_invalid_move_accuracy",
        scan.invalid_move_accuracy,
    );
    add(
        &mut counts,
        "metrics_unknown_one_move_blunders",
        scan.unknown_one_move_blunders,
    );
    add(
        &mut counts,
        "metrics_unknown_tactics_allowed",
        scan.unknown_tactics_allowed,
    );
    add(
        &mut counts,
        "metrics_invalid_observation_rows",
        scan.invalid_observation_rows,
    );

    GameFeatures {
        enriched: game.moves,
        derived,
        metrics: scan.metrics,
        observation: obs.observation,
        book_exit_ply,
        player_deviation_ply,
        line_key,
        opening,
        counts,
    }
}

/// The opening's stored `(eco, name)`: the classified line's when there is one, else the
/// platform's (T16 ruling: `games.eco`/`opening_name` stay the raw record, the profile reads the
/// classification and falls back to the platform name).
pub fn opening_name(
    line: Option<&RepertoireLine>,
    platform_eco: Option<&str>,
    platform_name: Option<&str>,
) -> (Option<String>, Option<String>) {
    match line {
        Some(l) => (Some(l.eco.clone()), Some(l.name.clone())),
        None => (
            platform_eco.map(str::to_owned),
            platform_name.map(str::to_owned),
        ),
    }
}

/// A `games.time_class` value.
pub fn parse_time_class(s: &str) -> Option<TimeClass> {
    Some(match s {
        "bullet" => TimeClass::Bullet,
        "blitz" => TimeClass::Blitz,
        "rapid" => TimeClass::Rapid,
        "classical" => TimeClass::Classical,
        "daily" => TimeClass::Daily,
        _ => return None,
    })
}

/// A `color` value.
pub fn parse_color(s: &str) -> Option<Color> {
    match s {
        "white" => Some(Color::White),
        "black" => Some(Color::Black),
        _ => None,
    }
}

/// The player-POV score of a result (`None` for `*` or anything else).
pub fn player_score(result: &str, player: Color) -> Option<f64> {
    let white = match result {
        "1-0" => 1.0,
        "0-1" => 0.0,
        "1/2-1/2" => 0.5,
        _ => return None,
    };
    Some(match player {
        Color::White => white,
        Color::Black => 1.0 - white,
    })
}

/// The game's context from the player's point of view, with `accuracy` the player's game
/// accuracy. `Err` names a column that does not parse.
/// The player's stored rating in their previous rated game of the same time class (`$1` the
/// game id): the input of [`pre_game_ratings`] (ADR-FIX-C-1). Served by
/// `games_account_played_idx`; a NULL rating there reads as `None`.
pub const PREVIOUS_OWN_RATING_SQL: &str = "SELECT CASE p.player_color WHEN 'white' THEN p.white_rating ELSE p.black_rating END FROM games g JOIN games p ON p.account_id = g.account_id AND p.time_class = g.time_class WHERE g.id = $1 AND p.rated AND (p.played_at, p.id) < (g.played_at, g.id) ORDER BY p.played_at DESC, p.id DESC LIMIT 1";

/// See [`PREVIOUS_OWN_RATING_SQL`].
pub async fn previous_own_rating(
    pool: &sqlx::PgPool,
    game_id: uuid::Uuid,
) -> Result<Option<i32>, sqlx::Error> {
    let r: Option<Option<i32>> = sqlx::query_scalar(PREVIOUS_OWN_RATING_SQL)
        .bind(game_id)
        .fetch_optional(pool)
        .await?;
    Ok(r.flatten())
}

/// The game's context from the player's point of view. Its ratings are the pre-game ones
/// ([`pre_game_ratings`], from `previous_own_rating`), so the expected score is the one predicted
/// before the game.
pub fn game_context(
    game: &GameRow,
    accuracy: Option<f64>,
    protocol: &str,
    previous_own_rating: Option<i32>,
) -> Result<GameContext, String> {
    let player = parse_color(&game.player_color)
        .ok_or_else(|| format!("player_color {:?}", game.player_color))?;
    let time_class = parse_time_class(&game.time_class)
        .ok_or_else(|| format!("time_class {:?}", game.time_class))?;
    let platform: PlatformId = game
        .platform
        .parse()
        .map_err(|_| format!("platform {:?}", game.platform))?;
    let (own, opp) = match player {
        Color::White => (game.white_rating, game.black_rating),
        Color::Black => (game.black_rating, game.white_rating),
    };
    let (own, opp) = pre_game_ratings(
        platform,
        PreGameInput {
            rated: game.rated,
            own_rating: own,
            opponent_rating: opp,
            previous_own_rating,
        },
    );
    let secs = |v: Option<i32>| v.and_then(|s| u32::try_from(s).ok());
    Ok(GameContext {
        game_id: game.id,
        platform_game_id: game.platform_game_id.clone(),
        platform,
        played_at: game.played_at,
        time_class,
        base_seconds: secs(game.base_seconds),
        increment_seconds: secs(game.increment_seconds),
        rated: game.rated,
        player_color: player,
        player_rating: own,
        opponent_rating: opp,
        result: game.result.clone(),
        score: player_score(&game.result, player),
        termination: game.termination.clone(),
        accuracy,
        protocol: protocol.to_owned(),
    })
}
