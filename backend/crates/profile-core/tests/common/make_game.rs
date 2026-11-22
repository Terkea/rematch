//! `make_game()`: the TS M2 Task 7 detector fixture (R2 Task 8).
//!
//! 60 plies: the player is White with 30 eligible moves (odd plies 1..59), Black plays the even
//! plies. Every move has ΔW 0 at W 50 / cp 0 and neutral features: quiet best and refutation lines
//! (gain 0, no depth), no threat, not critical, no MultiPV gap, no `in_top2`, no difficulty, no
//! tablebase. Blitz 300+0 with clocks (own clock 300 - 5k before own move k, 5 s per move).
//! Plies 1-20 are the opening, the rest is the middlegame. The game is a draw.

use chess_core::{Color, EngineScore, Judgement, MoveRecord, Phase, TimeClass};
use profile_core::types::{
    AnalysedGame, Difficulty, EnrichedMove, GameContext, LineAnalysis, MotifTag, MoveFeatures,
    PlatformId, RawMoveFeatures, Swing,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
pub const PLAYER_MOVES: usize = 30;
pub const PLIES: usize = 2 * PLAYER_MOVES;

/// A quiet line: no swing, no tags.
pub fn quiet_line() -> LineAnalysis {
    line(0.0, None, None, &[])
}

pub fn line(
    gain: f64,
    depth: Option<u32>,
    mate_in: Option<u32>,
    tags: &[MotifTag],
) -> LineAnalysis {
    LineAnalysis {
        swing: Swing {
            gain,
            depth,
            mate_in,
        },
        tags: tags.to_vec(),
    }
}

pub fn maia(p_ok: f64) -> Difficulty {
    Difficulty {
        p_best: p_ok.min(0.5),
        p_ok,
        model: "maia2-blitz-1.0".into(),
    }
}

fn neutral_move(i: usize) -> EnrichedMove {
    let ply = i as u32 + 1;
    let color = if i.is_multiple_of(2) {
        Color::White
    } else {
        Color::Black
    };
    let own_index = (i / 2) as f64;
    let phase = if ply <= 20 {
        Phase::Opening
    } else {
        Phase::Middlegame
    };
    EnrichedMove {
        rec: MoveRecord {
            ply,
            color,
            san: "Nf3".into(),
            uci: "g1f3".into(),
            fen_before: START.into(),
            best_uci: Some("g1f3".into()),
            win_before: 50.0,
            win_after: 50.0,
            delta_w: 0.0,
            accuracy: 100.0,
            judgement: None,
            phase,
            clock_before: Some(300.0 - 5.0 * own_index),
            time_spent: Some(5.0),
            decided: false,
        },
        fen_after: START.into(),
        score_before: EngineScore::Cp(0),
        score_after: EngineScore::Cp(0),
        raw: Some(RawMoveFeatures {
            ply,
            multipv: None,
            best_pv: vec!["g1f3".into()],
            refutation_pv: vec!["g8f6".into()],
            threat: None,
            tb_wdl_before: None,
            tb_wdl_after: None,
            difficulty: None,
        }),
        f: MoveFeatures {
            is_critical: Some(false),
            multipv_gap: None,
            playable: None,
            in_top2: None,
            best: Some(quiet_line()),
            refutation: Some(quiet_line()),
            threat: None,
            threat_existed: Some(false),
            threat_addressed: Some(true),
        },
    }
}

pub fn make_game() -> AnalysedGame {
    AnalysedGame {
        ctx: GameContext {
            game_id: Uuid::from_u128(0x8),
            platform_game_id: "abcd1234".into(),
            platform: PlatformId::Lichess,
            played_at: OffsetDateTime::UNIX_EPOCH,
            time_class: TimeClass::Blitz,
            base_seconds: Some(300),
            increment_seconds: Some(0),
            rated: true,
            player_color: Color::White,
            player_rating: Some(1500),
            opponent_rating: Some(1500),
            result: "1/2-1/2".into(),
            score: Some(0.5),
            termination: None,
            accuracy: None,
            protocol: "sf19-n300k-v1".into(),
        },
        moves: (0..PLIES).map(neutral_move).collect(),
        book_exit_ply: Some(8),
        opening_line_key: Some("white:italian-game".into()),
    }
}

/// Index into `game.moves` of the player's `k`-th move (0-based); its ply is `2k + 1`.
pub fn own(k: usize) -> usize {
    2 * k
}

/// The index whose ply is `ply`.
pub fn at_ply(ply: u32) -> usize {
    ply as usize - 1
}

/// Applies `f` to `game.moves[i]`.
pub fn with_move(mut g: AnalysedGame, i: usize, f: impl FnOnce(&mut EnrichedMove)) -> AnalysedGame {
    f(&mut g.moves[i]);
    g
}

/// Sets the mover's W before and after (ΔW, accuracy-free) and keeps `decided` consistent.
pub fn set_w(m: &mut EnrichedMove, before: f64, after: f64) {
    m.rec.win_before = before;
    m.rec.win_after = after;
    m.rec.delta_w = (before - after).max(0.0);
    m.rec.decided = (before - 50.0).abs() >= 40.0;
    m.rec.judgement = match m.rec.delta_w {
        d if d >= 15.0 => Some(Judgement::Blunder),
        d if d >= 10.0 => Some(Judgement::Mistake),
        d if d >= 5.0 => Some(Judgement::Inaccuracy),
        _ => None,
    };
}

/// A loss of `dw` W% from W 50.
pub fn lose(m: &mut EnrichedMove, dw: f64) {
    set_w(m, 50.0, 50.0 - dw);
}

/// Removes every clock reading.
pub fn without_clocks(mut g: AnalysedGame) -> AnalysedGame {
    for m in &mut g.moves {
        m.rec.clock_before = None;
        m.rec.time_spent = None;
    }
    g
}

/// Sets the game result from the player's (White's) point of view.
pub fn with_score(mut g: AnalysedGame, score: f64) -> AnalysedGame {
    g.ctx.score = Some(score);
    g.ctx.result = match score {
        1.0 => "1-0",
        0.0 => "0-1",
        _ => "1/2-1/2",
    }
    .into();
    g
}
