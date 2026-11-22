//! The plain-language glossary (`apps/web/src/server/profile/glossary.ts`) quotes the detector
//! thresholds in the units a player reads: win% points, pawns, moves, seconds and percent. This
//! test writes those numbers to `tests/fixtures/detector_constants.json` with
//! `UPDATE_SNAPSHOTS=1` and otherwise fails when a constant drifts from it; the web's
//! `glossary.test.ts` checks every number its copy quotes against the same file.

// `json!` with this many keys needs a deeper macro recursion.
#![recursion_limit = "256"]

use std::path::PathBuf;

use chess_core::TimeClass;
use chess_core::phase::{OPENING_MAX_PLY, OPENING_MIN_PLY};
use profile_core::bands::band_of;
use profile_core::constants::chesscom_to_lichess_offset;
use profile_core::features::derive::{THREAT_MAX_DEPTH, THREAT_MIN_GAIN};
use profile_core::features::{CRITICAL_GAP, DIFFICULTY_MIN_PLY, LINE_MAX_PLIES};
use profile_core::kpi::catalogue::SESSION_LATE_INDEX;
use profile_core::openings::repertoire::{THEORY_MIN_SHARE_PCT, THEORY_MIN_TOTAL};
use profile_core::stats::intervals::INTERVAL_LEVEL;
use profile_core::stats::priority::{MIN_GAMES, PRECISE_REL_WIDTH, WINDOW_DAYS};
use profile_core::stats::quantiles::{QuartileRef, percentile_from_quartiles};
use profile_core::stats::weights::HALF_LIFE_DAYS;
use profile_core::timeline::states::{ACTIVE_CONFIDENCE, IMPROVING_P_IMP, RESOLVE_STREAK};
use profile_core::weakness::detectors as d;
use profile_core::weakness::endgame::TB_MAX_PIECES;
use profile_core::weakness::episodes::EpisodeOpts;
use profile_core::weakness::game_metrics::{
    ENDGAME_CONVERT_MIN_ENTRY, ENDGAME_HOLD_MAX_ENTRY, MISSED_PUNISH_DW, SAVEABLE_MIN_WIN,
    TREND_FROM_PLY, TREND_TO_PLY,
};
use profile_core::weakness::observe_game::T_WIN;
use profile_core::weakness::sessions::{EST_MOVES_PER_GAME, SESSION_GAP_MINUTES};
use profile_core::weakness::tilt::TILT_MIN_INDEX;
use profile_core::weakness::time_trouble::{
    TROUBLE_FRACTION, TROUBLE_MIN_REMAINING, TROUBLE_MOVE_LIMIT,
};
use serde_json::{Value, json};

/// A fraction as a percent, rounded to two decimals (no float noise in the fixture).
fn pct(x: f64) -> f64 {
    (x * 10_000.0).round() / 100.0
}

/// The move number (both colours) of the first own move at or after `ply`: White's n-th move is
/// ply `2n - 1`, Black's ply `2n`, so plies `>= p` start at move `ceil(p / 2)`.
fn move_from_ply(ply: u32) -> u32 {
    ply.div_ceil(2)
}

/// Band edges on the Lichess-equivalent scale; each is checked against `band_of`.
const BAND_EDGES: [f64; 5] = [900.0, 1300.0, 1700.0, 2000.0, 2300.0];

fn constants() -> Value {
    for edge in BAND_EDGES {
        assert_ne!(band_of(edge - 0.5), band_of(edge), "band edge {edge}");
        assert_eq!(band_of(edge), band_of(edge + 99.0), "band edge {edge}");
    }
    let episodes = EpisodeOpts::default();
    assert_eq!(episodes.t_win, T_WIN, "W5 and the KPI share T_WIN");
    let tail = (1.0 - INTERVAL_LEVEL) / 2.0;
    let q = QuartileRef {
        p25: 0.1,
        p40: 0.2,
        median: 0.3,
        p75: 0.4,
    };
    json!({
        // Win% scale (0-100, from the mover's side) and move judgements.
        "decisiveDropWinPct": d::DECISIVE_DW,
        "inaccuracyDropWinPct": d::INACCURACY_DW,
        "openPositionMinWinPct": 50.0 - d::ELIGIBLE_RADIUS,
        "openPositionMaxWinPct": 50.0 + d::ELIGIBLE_RADIUS,
        "tacticMinGainPawns": d::TACTIC_MIN_GAIN,
        "hungMaterialMaxPlies": d::W1_MAX_DEPTH,
        "threatMinGainPawns": THREAT_MIN_GAIN,
        "threatMaxPlies": THREAT_MAX_DEPTH,
        "shallowTacticMaxPlies": d::W3_SHALLOW_MAX_DEPTH,
        "deepTacticMinPlies": d::W3_SHALLOW_MAX_DEPTH + 1,
        "lineMaxPlies": LINE_MAX_PLIES,
        "missedMateMaxMoves": d::W4_MAX_MATE,
        "trapMaxMoves": d::TRAP_MAX_PLY / 2,
        "openingWorseMaxWinPct": d::PREP_MAX_WIN,
        "openingMinMoves": OPENING_MIN_PLY / 2,
        "openingMaxMoves": OPENING_MAX_PLY / 2,
        "theoryMinGames": THEORY_MIN_TOTAL,
        "theoryMinSharePct": THEORY_MIN_SHARE_PCT,
        "lowClockSeconds": d::LOW_CLOCK_SECONDS,
        "rushPctOfMedian": pct(d::RUSH_FRACTION),
        "timeSpreadMinMoves": d::W9D_MIN_MOVES,
        "quietMaxPawns": f64::from(d::QUIET_MAX_CP) / 100.0,
        "quietMaxCentipawns": d::QUIET_MAX_CP,
        "criticalGapWinPct": CRITICAL_GAP,
        "easyMinPct": pct(d::EASY_MIN_P_OK),
        "difficultyFromMove": move_from_ply(DIFFICULTY_MIN_PLY),
        "tablebaseMaxPieces": TB_MAX_PIECES,
        // Winning and losing episodes (W5, W6 and their KPIs).
        "winningMinWinPct": T_WIN,
        "losingMaxWinPct": episodes.t_lose,
        "episodeMinMoves": episodes.min_moves,
        "winningFromMove": move_from_ply(episodes.min_ply),
        // Time trouble (W9a).
        "troubleClockPctOfBase": pct(TROUBLE_FRACTION),
        "troubleMoveLimit": TROUBLE_MOVE_LIMIT,
        "troubleMinMovesLeft": TROUBLE_MIN_REMAINING,
        // Sessions and tilt (W12 and the mindset KPIs).
        "sessionGapMinutes": SESSION_GAP_MINUTES,
        "estMovesPerGame": EST_MOVES_PER_GAME,
        "tiltFromGame": TILT_MIN_INDEX,
        "lateSessionFromGame": SESSION_LATE_INDEX,
        // Per-game metrics.
        "missedPunishDropWinPct": MISSED_PUNISH_DW,
        "trendFromMove": TREND_FROM_PLY / 2,
        "trendToMove": TREND_TO_PLY / 2,
        "endgameWinningEntryWinPct": ENDGAME_CONVERT_MIN_ENTRY,
        "endgameWorseEntryWinPct": ENDGAME_HOLD_MAX_ENTRY,
        "saveableMinWinPct": SAVEABLE_MIN_WIN,
        // Estimates, ranking and states.
        "windowDays": WINDOW_DAYS,
        "halfLifeDays": HALF_LIFE_DAYS,
        "minGames": MIN_GAMES,
        "intervalPct": pct(INTERVAL_LEVEL),
        "intervalLowPct": pct(tail),
        "intervalHighPct": pct(1.0 - tail),
        "preciseWidthPctOfMedian": pct(PRECISE_REL_WIDTH),
        "bandMedianPercentile": percentile_from_quartiles(q.median, &q, true),
        "activeConfidencePct": pct(ACTIVE_CONFIDENCE),
        "improvingChancePct": pct(IMPROVING_P_IMP),
        "resolveStreakGames": RESOLVE_STREAK,
        // A resolve needs the rate below the band reference's p40 knot.
        "resolveBelowPercentile": percentile_from_quartiles(q.p40, &q, true),
        // Rating bands.
        "bandEdges": BAND_EDGES,
        "chesscomOffsetBlitz": chesscom_to_lichess_offset(TimeClass::Blitz),
        "chesscomOffsetRapid": chesscom_to_lichess_offset(TimeClass::Rapid),
        "chesscomOffsetClassical": chesscom_to_lichess_offset(TimeClass::Classical),
    })
}

#[test]
fn detector_constants_match_the_fixture() {
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&constants()).expect("constants serialise")
    );
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/detector_constants.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create fixture dir");
        std::fs::write(&path, &text).expect("write fixture");
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (regenerate it with UPDATE_SNAPSHOTS=1)",
            path.display()
        )
    });
    assert_eq!(
        stored.replace("\r\n", "\n"),
        text,
        "a detector constant changed; regenerate the fixture with UPDATE_SNAPSHOTS=1 and update \
         the glossary copy in apps/web/src/server/profile/glossary.ts"
    );
}
