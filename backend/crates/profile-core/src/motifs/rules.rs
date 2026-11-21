//! Motif rules (R2 Task 5), one function per rule (TS M2 Task 4, our own wording; ADR-R2-3).
//!
//! Every rule reads a [`LineContext`]. "Window" means plies `≤ depth + 1`; the mate rules
//! (`mate`, `back_rank_mate`, `smothered_mate`) read the mating step at the end of the whole PV.
//! Attack queries are Task 4's pseudo-legal ones (chess.js `attackers` parity) unless a rule says
//! "legal".

use chess_core::Color;
use chess_core::board::Position;

use crate::board::material::placement_pieces;
use crate::board::{LineStep, SettledPly, attackers_of, defenders_of, is_en_prise, piece_at};
use crate::board::{parse_square, piece_value, pinned_to};
use crate::types::{MotifTag, Swing};

/// A settled loss of at least this many pawns is a sacrifice; a gain of this many realises it.
const SACRIFICE_PAWNS: i32 = 2;

/// Rank value of a king for "more valuable than" comparisons (it outranks every piece).
const KING_RANK_VALUE: i32 = 1000;

/// What the rules see of one tagged line.
pub struct LineContext<'a> {
    pub pov: Color,
    /// Task 4's settled swing; the tagger only builds a context when `depth` is `Some`.
    pub swing: &'a Swing,
    /// The whole replayed PV.
    pub steps: &'a [LineStep],
    /// Number of leading steps in the window: `min(depth + 1, steps.len())`.
    pub window: usize,
    /// `settled(k)` for every pov ply `k ≤ max_plies`.
    pub settled: &'a [SettledPly],
    /// Number of leading steps the settled replay covers: `min(max(window, max_plies +
    /// settle_extra + 1), steps.len())`. A fork's completing capture may come from here
    /// (ADR-R2-3, the fork exception to the window).
    pub settled_limit: usize,
}

impl LineContext<'_> {
    fn window_steps(&self) -> &[LineStep] {
        self.steps.get(..self.window).unwrap_or(self.steps)
    }

    /// The step at index `i` if it lies inside the window.
    fn in_window(&self, i: usize) -> Option<&LineStep> {
        self.window_steps().get(i)
    }

    fn settled_steps(&self) -> &[LineStep] {
        self.steps.get(..self.settled_limit).unwrap_or(self.steps)
    }

    /// The last step of the whole PV when it is a pov checkmate.
    fn mating_step(&self) -> Option<&LineStep> {
        self.steps
            .last()
            .filter(|s| s.is_mate && s.mover == self.pov)
    }
}

fn opponent(c: Color) -> Color {
    match c {
        Color::White => Color::Black,
        Color::Black => Color::White,
    }
}

fn rank_value(letter: char) -> i32 {
    if letter == 'k' {
        KING_RANK_VALUE
    } else {
        piece_value(letter)
    }
}

fn is_slider(letter: char) -> bool {
    matches!(letter, 'b' | 'r' | 'q')
}

fn square_name(file: u8, rank: u8) -> String {
    format!("{}{}", char::from(b'a' + file), char::from(b'1' + rank))
}

/// `(file, rank)`, 0-based, of an algebraic square name.
fn coords(square: &str) -> Option<(i8, i8)> {
    let sq = parse_square(square)?;
    Some((sq.file() as i8, sq.rank() as i8))
}

/// The squares strictly between `a` and `b` when they share a rank, file or diagonal.
fn between(a: &str, b: &str) -> Vec<String> {
    let (Some((af, ar)), Some((bf, br))) = (coords(a), coords(b)) else {
        return Vec::new();
    };
    let (df, dr) = (bf - af, br - ar);
    if (df, dr) == (0, 0) || !(df == 0 || dr == 0 || df.abs() == dr.abs()) {
        return Vec::new();
    }
    let (sf, sr) = (df.signum(), dr.signum());
    let mut out = Vec::new();
    let (mut f, mut r) = (af + sf, ar + sr);
    while (f, r) != (bf, br) {
        out.push(square_name(f as u8, r as u8));
        f += sf;
        r += sr;
    }
    out
}

/// Pieces of `color` in `fen` as `(square, lowercase letter)`.
fn pieces_of(fen: &str, color: Color) -> Vec<(String, char)> {
    placement_pieces(fen)
        .into_iter()
        .filter(|(_, _, c)| c.is_ascii_uppercase() == (color == Color::White))
        .map(|(f, r, c)| (square_name(f, r), c.to_ascii_lowercase()))
        .collect()
}

fn king_square(fen: &str, color: Color) -> Option<String> {
    pieces_of(fen, color)
        .into_iter()
        .find(|(_, c)| *c == 'k')
        .map(|(s, _)| s)
}

/// The piece of `color` on `square`, as its lowercase letter.
fn own_piece(fen: &str, square: &str, color: Color) -> Option<char> {
    piece_at(fen, square)
        .filter(|(c, _)| *c == color)
        .map(|(_, p)| p)
}

/// Some legal move in `fen` lands on `square`.
fn legal_move_to(fen: &str, square: &str) -> bool {
    let (Ok(pos), Some(sq)) = (Position::from_fen(fen), parse_square(square)) else {
        return false;
    };
    pos.board()
        .generate_moves(|moves| moves.into_iter().any(|m| m.to == sq))
}

/// `hangingPiece`: pov's first ply captures a non-pawn piece that no non-king piece defended
/// before the capture, and no LEGAL recapture on that square exists afterwards (an illegal king
/// recapture into a guarded square does not count; a mating capture has no recapture at all).
pub fn hanging_piece(ctx: &LineContext) -> bool {
    let them = opponent(ctx.pov);
    let Some(first) = ctx.in_window(0).filter(|s| s.mover == ctx.pov) else {
        return false;
    };
    let defended_by_a_piece = defenders_of(&first.fen_before, &first.to, them)
        .iter()
        .any(|sq| own_piece(&first.fen_before, sq, them) != Some('k'));
    matches!(first.captured, Some('n' | 'b' | 'r' | 'q'))
        && !defended_by_a_piece
        && !legal_move_to(&first.fen_after, &first.to)
}

/// `fork`: after a pov move, the moved piece attacks at least two enemy non-pawn pieces, each the
/// king, worth more than the mover, or undefended; the mover is not en prise on its square; and
/// the forking piece itself later captures on one of the target squares. The fork move lies in
/// the window; the completing capture may come from anywhere in the settled replay (ADR-R2-3).
pub fn fork(ctx: &LineContext) -> bool {
    let them = opponent(ctx.pov);
    let replay = ctx.settled_steps();
    ctx.window_steps().iter().enumerate().any(|(i, step)| {
        if step.mover != ctx.pov {
            return false;
        }
        let after = &step.fen_after;
        let Some(mover) = own_piece(after, &step.to, ctx.pov) else {
            return false;
        };
        if is_en_prise(after, &step.to) {
            return false;
        }
        let targets: Vec<String> = pieces_of(after, them)
            .into_iter()
            .filter(|(sq, p)| {
                *p != 'p'
                    && attackers_of(after, sq, ctx.pov).contains(&step.to)
                    && (*p == 'k'
                        || rank_value(*p) > rank_value(mover)
                        || defenders_of(after, sq, them).is_empty())
            })
            .map(|(sq, _)| sq)
            .collect();
        targets.len() >= 2
            && replay.iter().enumerate().skip(i + 1).any(|(j, later)| {
                later.mover == ctx.pov
                    && later.from == step.to
                    && later.captured.is_some()
                    && targets.contains(&later.to)
                    && {
                        // ADR-R2-3: the capture is the forker's first move after the fork, or
                        // the target has not moved since the fork.
                        let between = replay.get(i + 1..j).unwrap_or_default();
                        let untouched =
                            |sq: &str| between.iter().all(|s| s.from != sq && s.to != sq);
                        untouched(&step.to) || untouched(&later.to)
                    }
            })
    })
}

/// `pin`: the piece captured at the swing ply was pinned (`pinned_to`, so by a pov slider) on its
/// square in some earlier position of the line with the opponent to move.
pub fn pin(ctx: &LineContext) -> bool {
    let them = opponent(ctx.pov);
    let Some(depth) = ctx.swing.depth.and_then(|d| usize::try_from(d).ok()) else {
        return false;
    };
    let Some(swing_step) = depth
        .checked_sub(1)
        .and_then(|i| ctx.in_window(i))
        .filter(|s| s.mover == ctx.pov)
    else {
        return false;
    };
    let Some(captured) = swing_step.captured else {
        return false;
    };
    let square = &swing_step.to;
    ctx.window_steps()
        .iter()
        .take(depth - 1)
        .filter(|s| s.mover == ctx.pov)
        .map(|s| s.fen_after.as_str())
        .chain(
            ctx.steps
                .first()
                .filter(|s| s.mover == them)
                .map(|s| s.fen_before.as_str()),
        )
        .any(|fen| {
            own_piece(fen, square, them) == Some(captured) && pinned_to(fen, square).is_some()
        })
}

/// `skewer`: a pov slider captures X along a line through a square that a more valuable enemy
/// piece vacated on the previous ply.
pub fn skewer(ctx: &LineContext) -> bool {
    let window = ctx.window_steps();
    window.windows(2).any(|pair| {
        let [prev, step] = pair else {
            return false;
        };
        let (Some(captured), Some(slider)) = (
            step.captured,
            own_piece(&step.fen_before, &step.from, ctx.pov),
        ) else {
            return false;
        };
        let Some(fled) = own_piece(&prev.fen_before, &prev.from, opponent(ctx.pov)) else {
            return false;
        };
        step.mover == ctx.pov
            && prev.mover != ctx.pov
            && is_slider(slider)
            && rank_value(fled) > rank_value(captured)
            && between(&step.from, &step.to).contains(&prev.from)
    })
}

/// `discoveredAttack`: a pov move vacates a square, and a different pov slider then attacks
/// through it the enemy king (the move gives check) or an enemy piece that the slider captures two
/// plies later.
pub fn discovered_attack(ctx: &LineContext) -> bool {
    let them = opponent(ctx.pov);
    ctx.window_steps().iter().enumerate().any(|(i, step)| {
        if step.mover != ctx.pov {
            return false;
        }
        let after = &step.fen_after;
        let follow_up = ctx
            .in_window(i + 2)
            .filter(|s| s.mover == ctx.pov && s.captured.is_some());
        pieces_of(after, ctx.pov)
            .into_iter()
            .filter(|(sq, p)| *sq != step.to && is_slider(*p))
            .any(|(slider, _)| {
                let mut targets = Vec::new();
                if step.is_check {
                    targets.extend(king_square(after, them));
                }
                if let Some(next) = follow_up
                    .filter(|s| s.from == slider && own_piece(after, &s.to, them).is_some())
                {
                    targets.push(next.to.clone());
                }
                targets.iter().any(|t| {
                    between(&slider, t).contains(&step.from)
                        && attackers_of(after, t, ctx.pov).contains(&slider)
                })
            })
    })
}

/// `deflection`: a pov capture or check at ply k makes opponent piece D move at k+1 (to t): the
/// reply recaptures on the square of ply k or gives check. At k+2 pov captures on a square x ≠ t
/// that D defended before ply k+1. A voluntary reply is no deflection.
pub fn deflection(ctx: &LineContext) -> bool {
    let them = opponent(ctx.pov);
    ctx.window_steps().iter().enumerate().any(|(i, step)| {
        let (Some(reply), Some(hit)) = (ctx.in_window(i + 1), ctx.in_window(i + 2)) else {
            return false;
        };
        step.mover == ctx.pov
            && (step.captured.is_some() || step.is_check)
            && reply.mover == them
            && ((reply.captured.is_some() && reply.to == step.to) || reply.is_check)
            && hit.mover == ctx.pov
            && hit.captured.is_some()
            && hit.to != reply.to
            && defenders_of(&reply.fen_before, &hit.to, them).contains(&reply.from)
    })
}

/// `sacrifice`: some pov ply in the window has `settled(k) ≤ −2`, the line still gains at least 2
/// or mates, and the opponent does not promote in the window.
pub fn sacrifice(ctx: &LineContext) -> bool {
    let down = ctx
        .settled
        .iter()
        .filter(|p| usize::try_from(p.ply).is_ok_and(|k| k <= ctx.window))
        .any(|p| p.settled <= -SACRIFICE_PAWNS);
    let pays = ctx.swing.mate_in.is_some() || ctx.swing.gain >= f64::from(SACRIFICE_PAWNS);
    let they_promote = ctx
        .window_steps()
        .iter()
        .any(|s| s.mover != ctx.pov && s.promotion.is_some());
    down && pays && !they_promote
}

/// `promotion`: a pov move promotes within the window.
pub fn promotion(ctx: &LineContext) -> bool {
    ctx.window_steps()
        .iter()
        .any(|s| s.mover == ctx.pov && s.promotion.is_some())
}

/// `mate`, `mateIn1`..`mateIn3`, `mateIn4Plus` from `mate_in`; `matingNet` when `mate_in ≥ 2`.
pub fn mate(ctx: &LineContext) -> Vec<MotifTag> {
    let Some(n) = ctx.swing.mate_in else {
        return Vec::new();
    };
    let mut tags = vec![
        MotifTag::Mate,
        match n {
            1 => MotifTag::MateIn1,
            2 => MotifTag::MateIn2,
            3 => MotifTag::MateIn3,
            _ => MotifTag::MateIn4Plus,
        },
    ];
    if n >= 2 {
        tags.push(MotifTag::MatingNet);
    }
    tags
}

/// The mated king's square and the mating piece's letter, when the mating piece gives check.
fn mate_geometry<'a>(ctx: &'a LineContext) -> Option<(&'a LineStep, String, char)> {
    let step = ctx.mating_step()?;
    let after = &step.fen_after;
    let king = king_square(after, opponent(ctx.pov))?;
    let piece = own_piece(after, &step.to, ctx.pov)?;
    attackers_of(after, &king, ctx.pov)
        .contains(&step.to)
        .then_some((step, king, piece))
}

/// The on-board squares adjacent to `square` with rank offset in `ranks`.
fn neighbours(square: &str, ranks: &[i8]) -> Vec<String> {
    let Some((f, r)) = coords(square) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dr in ranks {
        for df in -1..=1i8 {
            let (nf, nr) = (f + df, r + dr);
            if (df, *dr) != (0, 0) && (0..8).contains(&nf) && (0..8).contains(&nr) {
                out.push(square_name(nf as u8, nr as u8));
            }
        }
    }
    out
}

/// `backRankMate`: a rook or queen mates on the mated king's back rank (the king stands on it),
/// and every forward escape square of the king holds one of its own pieces.
pub fn back_rank_mate(ctx: &LineContext) -> bool {
    let Some((step, king, piece)) = mate_geometry(ctx) else {
        return false;
    };
    let them = opponent(ctx.pov);
    let (back, forward) = match them {
        Color::White => (0, 1),
        Color::Black => (7, -1),
    };
    let (Some((_, king_rank)), Some((_, piece_rank))) = (coords(&king), coords(&step.to)) else {
        return false;
    };
    let escapes = neighbours(&king, &[forward]);
    matches!(piece, 'r' | 'q')
        && king_rank == back
        && piece_rank == back
        && !escapes.is_empty()
        && escapes
            .iter()
            .all(|sq| own_piece(&step.fen_after, sq, them).is_some())
}

/// `smotheredMate`: a knight mates and every square adjacent to the king holds the king's own
/// pieces.
pub fn smothered_mate(ctx: &LineContext) -> bool {
    let Some((step, king, piece)) = mate_geometry(ctx) else {
        return false;
    };
    let them = opponent(ctx.pov);
    piece == 'n'
        && neighbours(&king, &[-1, 0, 1])
            .iter()
            .all(|sq| own_piece(&step.fen_after, sq, them).is_some())
}
