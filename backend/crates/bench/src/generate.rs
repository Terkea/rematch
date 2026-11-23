//! The synthetic-game generator: two engine-driven players, one of them optionally flawed, on a
//! simulated rapid clock, written out as chess.com PGN.
//!
//! - Every move of both sides comes from a [`MemoAnalyser`] search of the position
//!   ([`GEN_MULTIPV`] lines). The clean choice is [`base_choice`]; the flawed player's injector
//!   replaces it when its coin comes up and the engine finds a move that does what it wants
//!   ([`find_goal_move`]).
//! - The clock: 10 minutes, no increment. Each side gets a pace for the game in `[0.7, 2.3)`,
//!   and a move spends `max(0.5, clock * pace / 25 * u)` seconds with `u` in `[0.5, 1.5)`. About
//!   a third of the games reach a clock under 30 seconds. A side whose clock runs out loses on
//!   time.
//! - A side resigns after two own moves in a row at -10 pawns or a mate against it, from ply 30.
//!   A game still running at ply [`MAX_PLIES`] is agreed drawn.
//! - Everything is drawn from one seeded mulberry32 stream per game, and the searches are
//!   deterministic, so the same seed gives the same game.

use std::collections::HashMap;
use std::sync::Arc;

use chess_core::board::Position;
use chess_core::{Color, EngineScore};
use cozy_chess::Piece;
use profile_core::stats::rng::Rng;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::flaws::{Flaw, FlawGame, Goal, INJECTED_DROP, MoveCtx, SPOILED_W, Tally, Unit};
use crate::memo::{GEN_MULTIPV, MemoAnalyser};
use crate::policy::{Candidate, base_choice, shuffle, uniform};

/// The simulated time control: 10 minutes, no increment (chess.com rapid).
pub const BASE_SECONDS: f64 = 600.0;
pub const MAX_PLIES: u32 = 200;
/// Resign threshold, in centipawns from the side to move (`score_to_cp` clamps at 1000).
pub const RESIGN_CP: i32 = -1000;
pub const RESIGN_FROM_PLY: u32 = 30;
/// Candidate moves the blunder search tries before giving up.
pub const FIND_TRIES: usize = 14;

#[derive(Clone, Debug)]
pub struct GameSpec {
    pub platform_game_id: String,
    pub player_name: String,
    pub player_color: Color,
    pub player_rating: i32,
    pub opponent_name: String,
    pub opponent_rating: i32,
    pub played_at: OffsetDateTime,
    pub seed: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeneratedGame {
    pub platform_game_id: String,
    pub pgn: String,
    /// `1-0`, `0-1` or `1/2-1/2`.
    pub result: String,
    pub termination: String,
    pub plies: u32,
    pub white_name: String,
    pub black_name: String,
    pub white_rating: i32,
    pub black_rating: i32,
    pub player_color: String,
    #[serde(with = "time::serde::rfc3339")]
    pub played_at: OffsetDateTime,
    /// Injections that happened in this game (by ply).
    pub injected_plies: Vec<u32>,
}

fn other(c: Color) -> Color {
    match c {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

fn idx(c: Color) -> usize {
    match c {
        Color::White => 0,
        Color::Black => 1,
    }
}

/// The value of a piece for W1's "hung a piece" test (pawns are not pieces here).
fn is_piece(p: Piece) -> bool {
    matches!(
        p,
        Piece::Knight | Piece::Bishop | Piece::Rook | Piece::Queen
    )
}

/// Legal non-king moves of `pos` in UCI, in a stable order.
fn legal_non_king(pos: &Position) -> Vec<String> {
    let board = pos.board();
    let mut out = Vec::new();
    board.generate_moves(|pm| {
        if pm.piece != Piece::King {
            for mv in pm {
                out.push(mv.to_string());
            }
        }
        false
    });
    out.sort();
    out
}

/// A move of `pos` that achieves `goal`, or `None` when [`FIND_TRIES`] candidates found none.
/// `w_before` is the mover's win percentage; `skip` are moves never considered (the engine's
/// own lines for a blunder, the mating lines for [`Goal::AvoidMate`]).
pub async fn find_goal_move(
    memo: &MemoAnalyser,
    pos: &Position,
    w_before: f64,
    skip: &[String],
    goal: Goal,
    rng: &mut Rng,
) -> anyhow::Result<Option<String>> {
    let mut cands: Vec<String> = legal_non_king(pos)
        .into_iter()
        .filter(|m| !skip.iter().any(|s| s.get(..4) == m.get(..4)))
        .collect();
    shuffle(rng, &mut cands);
    for uci in cands.into_iter().take(FIND_TRIES) {
        let Ok(next) = pos.play_uci(&uci) else {
            continue;
        };
        if next.is_checkmate() || next.is_stalemate() {
            continue;
        }
        let r = memo.search(&next.fen(), GEN_MULTIPV).await?;
        let Some(top) = r.lines.first() else { continue };
        // `top.score` is the opponent's view.
        let w_after = 100.0 - chess_core::score::win_percent(top.score);
        let ok = match goal {
            Goal::AvoidMate => !matches!(top.score, EngineScore::Mate(m) if m < 0),
            Goal::Blunder { hang } => {
                let drop = w_before - w_after >= INJECTED_DROP;
                let not_mate = !matches!(top.score, EngineScore::Mate(_));
                let hung = !hang
                    || top.pv.first().is_some_and(|reply| {
                        reply
                            .get(2..4)
                            .and_then(|sq| sq.parse::<cozy_chess::Square>().ok())
                            .and_then(|sq| next.board().piece_on(sq))
                            .is_some_and(is_piece)
                    });
                drop && hung && (not_mate || !hang)
            }
        };
        if ok {
            return Ok(Some(uci));
        }
    }
    Ok(None)
}

fn clock_text(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    let (s, t) = (tenths / 10, tenths % 10);
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if t == 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{h}:{m:02}:{s:02}.{t}")
    }
}

/// The chess.com PGN of a game: the importer's shape, `[%clk]` after every move.
pub fn write_pgn(
    spec: &GameSpec,
    moves: &[(String, f64)],
    result: &str,
    termination: &str,
) -> String {
    let (white, black, we, be) = match spec.player_color {
        Color::White => (
            &spec.player_name,
            &spec.opponent_name,
            spec.player_rating,
            spec.opponent_rating,
        ),
        Color::Black => (
            &spec.opponent_name,
            &spec.player_name,
            spec.opponent_rating,
            spec.player_rating,
        ),
    };
    let d = spec.played_at;
    let date = format!("{}.{:02}.{:02}", d.year(), u8::from(d.month()), d.day());
    let t = format!("{:02}:{:02}:{:02}", d.hour(), d.minute(), d.second());
    let mut out = String::new();
    for (k, v) in [
        ("Event", "Live Chess".to_owned()),
        ("Site", "Chess.com".to_owned()),
        ("Date", date.clone()),
        ("Round", "-".to_owned()),
        ("White", white.clone()),
        ("Black", black.clone()),
        ("Result", result.to_owned()),
        ("WhiteElo", we.to_string()),
        ("BlackElo", be.to_string()),
        ("TimeControl", format!("{}", BASE_SECONDS as u32)),
        ("Termination", termination.to_owned()),
        ("UTCDate", date),
        ("UTCTime", t),
        (
            "Link",
            format!("https://www.chess.com/game/live/{}", spec.platform_game_id),
        ),
    ] {
        out.push_str(&format!("[{k} \"{v}\"]\n"));
    }
    out.push('\n');
    let mut text = Vec::new();
    for (i, (san, clock)) in moves.iter().enumerate() {
        let n = i / 2 + 1;
        let num = if i % 2 == 0 {
            format!("{n}.")
        } else {
            format!("{n}...")
        };
        text.push(format!("{num} {san} {{[%clk {}]}}", clock_text(*clock)));
    }
    text.push(result.to_owned());
    out.push_str(&text.join(" "));
    out.push('\n');
    out
}

/// Plays one game. Returns the game and the flaw's tally for it (default without a flaw).
pub async fn generate_game(
    memo: &MemoAnalyser,
    spec: &GameSpec,
    flaw: Option<&Arc<dyn Flaw>>,
) -> anyhow::Result<(GeneratedGame, Tally)> {
    let mut rng = Rng::new(spec.seed);
    let mut pos = Position::start();
    let mut moves: Vec<(String, f64)> = Vec::new();
    let mut seen: HashMap<String, u32> = HashMap::new();
    seen.insert(pos.repetition_key(), 1);
    let mut clocks = [BASE_SECONDS; 2];
    let pace = [uniform(&mut rng, 0.7, 2.3), uniform(&mut rng, 0.7, 2.3)];
    let mut resign_streak = [0u32; 2];
    let mut winning_run = 0u32;
    let mut tally = Tally::default();
    let mut fg: FlawGame = flaw.map(|f| f.start_game(&mut rng)).unwrap_or_default();
    let mut injected = Vec::new();
    let player = spec.player_color;
    let mut ply: u32 = 0;
    let (result, termination) = loop {
        ply += 1;
        let stm = pos.side_to_move();
        let name = |c: Color| {
            if c == player {
                spec.player_name.clone()
            } else {
                spec.opponent_name.clone()
            }
        };
        let win_token = |c: Color| match c {
            Color::White => "1-0",
            Color::Black => "0-1",
        };
        if ply > MAX_PLIES {
            break ("1/2-1/2", "Game drawn by agreement".to_owned());
        }
        let fen = pos.fen();
        let r = memo.search(&fen, GEN_MULTIPV).await?;
        let lines: Vec<_> = r.lines.iter().filter(|l| !l.pv.is_empty()).collect();
        let Some(best) = lines.first() else {
            break ("1/2-1/2", "Game drawn by agreement".to_owned());
        };
        if ply >= RESIGN_FROM_PLY && chess_core::score::score_to_cp(best.score) <= RESIGN_CP {
            resign_streak[idx(stm)] += 1;
            if resign_streak[idx(stm)] >= 2 {
                let w = other(stm);
                break (win_token(w), format!("{} won by resignation", name(w)));
            }
        } else {
            resign_streak[idx(stm)] = 0;
        }
        let cands: Vec<Candidate> = lines
            .iter()
            .map(|l| Candidate::new(l.pv[0].clone(), l.score))
            .collect();
        let w_before = cands[0].w;
        let mut choice = cands[base_choice(&cands, rng.next_f64()).unwrap_or(0)]
            .uci
            .clone();
        let clock_before = clocks[idx(stm)];
        if stm == player {
            winning_run = if ply >= 21 && w_before >= crate::flaws::WINNING_W {
                winning_run + 1
            } else {
                0
            };
        }
        if let (Some(f), true) = (flaw, stm == player) {
            let ctx = MoveCtx {
                ply,
                w_before,
                mate_in: match best.score {
                    EngineScore::Mate(n) if n > 0 => Some(n),
                    _ => None,
                },
                clock_before: Some(clock_before),
                winning_run,
            };
            let goal = match f.unit() {
                Unit::Move => {
                    if f.opportunity(&ctx, &fg) {
                        tally.opportunities += 1;
                    }
                    let mut g = None;
                    if f.armed(&ctx, &fg) {
                        tally.armed += 1;
                        if rng.next_f64() < f.rate() {
                            tally.attempts += 1;
                            g = Some(f.goal());
                        }
                    }
                    g
                }
                Unit::Game => {
                    if !fg.counted_opportunity && f.opportunity(&ctx, &fg) {
                        fg.counted_opportunity = true;
                        tally.opportunities += 1;
                    }
                    let armed = f.armed(&ctx, &fg);
                    if armed && !fg.counted_armed {
                        fg.counted_armed = true;
                        tally.armed += 1;
                    }
                    if armed && fg.fire {
                        if !fg.counted_attempt {
                            fg.counted_attempt = true;
                            tally.attempts += 1;
                        }
                        Some(f.goal())
                    } else {
                        None
                    }
                }
            };
            if let Some(goal) = goal {
                let skip: Vec<String> = match goal {
                    Goal::AvoidMate => cands
                        .iter()
                        .filter(|c| c.mates)
                        .map(|c| c.uci.clone())
                        .collect(),
                    Goal::Blunder { .. } => cands.iter().map(|c| c.uci.clone()).collect(),
                };
                let found = find_goal_move(memo, &pos, w_before, &skip, goal, &mut rng).await?;
                if let Some(uci) = found {
                    choice = uci;
                    injected.push(ply);
                    match f.unit() {
                        Unit::Move => tally.successes += 1,
                        Unit::Game => {
                            if !fg.succeeded {
                                fg.succeeded = true;
                                tally.successes += 1;
                            }
                            // W5 keeps spoiling until the win is gone.
                            fg.spoiling = f.id() == "no_convert";
                        }
                    }
                }
            }
            if fg.spoiling && w_before <= SPOILED_W {
                fg.spoiling = false;
            }
        }
        let spent = (clock_before * pace[idx(stm)] / 25.0 * uniform(&mut rng, 0.5, 1.5)).max(0.5);
        let spent = (spent * 10.0).round() / 10.0;
        if spent >= clock_before {
            let w = other(stm);
            break (win_token(w), format!("{} won on time", name(w)));
        }
        clocks[idx(stm)] = clock_before - spent;
        let played = pos
            .push_uci(&choice)
            .map_err(|e| anyhow::anyhow!("illegal generated move {choice}: {e:?}"))?;
        moves.push((played.san, clocks[idx(stm)]));
        if pos.is_checkmate() {
            break (win_token(stm), format!("{} won by checkmate", name(stm)));
        }
        if pos.is_stalemate() {
            break ("1/2-1/2", "Game drawn by stalemate".to_owned());
        }
        if pos.is_insufficient_material() {
            break ("1/2-1/2", "Game drawn by insufficient material".to_owned());
        }
        if pos.halfmove_clock() >= 100 {
            break ("1/2-1/2", "Game drawn by 50-move rule".to_owned());
        }
        let n = seen.entry(pos.repetition_key()).or_insert(0);
        *n += 1;
        if *n >= 3 {
            break ("1/2-1/2", "Game drawn by repetition".to_owned());
        }
    };
    let (white_name, black_name, white_rating, black_rating) = match player {
        Color::White => (
            spec.player_name.clone(),
            spec.opponent_name.clone(),
            spec.player_rating,
            spec.opponent_rating,
        ),
        Color::Black => (
            spec.opponent_name.clone(),
            spec.player_name.clone(),
            spec.opponent_rating,
            spec.player_rating,
        ),
    };
    let pgn = write_pgn(spec, &moves, result, &termination);
    Ok((
        GeneratedGame {
            platform_game_id: spec.platform_game_id.clone(),
            pgn,
            result: result.to_owned(),
            termination,
            plies: moves.len() as u32,
            white_name,
            black_name,
            white_rating,
            black_rating,
            player_color: player.as_str().to_owned(),
            played_at: spec.played_at,
            injected_plies: injected,
        },
        tally,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GameSpec {
        GameSpec {
            platform_game_id: "bench-1".to_owned(),
            player_name: "bench-hero".to_owned(),
            player_color: Color::Black,
            player_rating: 1500,
            opponent_name: "bench-opp".to_owned(),
            opponent_rating: 1480,
            played_at: time::macros::datetime!(2026-10-01 09:05:00 UTC),
            seed: 1,
        }
    }

    #[test]
    fn clock_text_is_chesscom_style() {
        assert_eq!(clock_text(598.1), "0:09:58.1");
        assert_eq!(clock_text(25.0), "0:00:25");
        assert_eq!(clock_text(3725.04), "1:02:05");
    }

    #[test]
    fn pgn_parses_back_with_clocks_and_headers() {
        let moves = vec![
            ("e4".to_owned(), 598.1),
            ("e5".to_owned(), 597.0),
            ("Qh5".to_owned(), 590.0),
            ("Nc6".to_owned(), 580.0),
            ("Bc4".to_owned(), 570.0),
            ("Nf6".to_owned(), 25.5),
            ("Qxf7#".to_owned(), 560.0),
        ];
        let pgn = write_pgn(&spec(), &moves, "1-0", "bench-opp won by checkmate");
        let g = chess_core::pgn::parse_pgn(&pgn).expect("parses");
        assert_eq!(g.plies.len(), 7);
        assert_eq!(g.header("White"), Some("bench-opp"));
        assert_eq!(g.header("Black"), Some("bench-hero"));
        assert_eq!(g.header("TimeControl"), Some("600"));
        assert_eq!(g.plies[5].clock_seconds, Some(25.5));
        assert_eq!(g.terminal, Some(chess_core::Terminal::Checkmate));
    }

    #[test]
    fn legal_non_king_moves_at_the_start() {
        let moves = legal_non_king(&Position::start());
        assert_eq!(moves.len(), 20);
        assert!(moves.contains(&"g1f3".to_owned()));
    }
}
