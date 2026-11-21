//! Board wrapper over `cozy_chess` that reproduces chess.js 1.4.0 behaviour
//! byte for byte: FEN (including the en-passant rule), SAN (disambiguation,
//! suffixes, permissive parsing) and UCI (castling as the king's two-square
//! step). See ADR 0037.

use crate::Color;
use cozy_chess as cc;
use cozy_chess::{Board, File, Piece, Rank, Square};
use regex::Regex;
use std::sync::OnceLock;

/// A FEN string chess.js would reject (or that this wrapper cannot represent).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid FEN: {0}")]
pub struct FenError(pub String);

/// A SAN token that matches no legal move.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("illegal move: {0}")]
pub struct IllegalMove(pub String);

/// Errors from [`Position::play_uci`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChessError {
    #[error("invalid UCI move: {0}")]
    InvalidUci(String),
    #[error("illegal move: {0}")]
    Illegal(String),
}

/// A move that was played: chess.js SAN (with check/mate suffix) and UCI
/// (castling is the king's from and to, e.g. `e1g1`; promotion is lowercase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedMove {
    pub san: String,
    pub uci: String,
}

/// A legal move in chess.js terms.
#[derive(Clone, Copy, Debug)]
struct LegalMove {
    /// The move in cozy-chess form (castling is king-takes-own-rook).
    cozy: cc::Move,
    from: Square,
    /// Destination as chess.js reports it (king destination for castling).
    to: Square,
    piece: Piece,
    promotion: Option<Piece>,
    capture: bool,
    /// `Some(true)` king side, `Some(false)` queen side.
    castle: Option<bool>,
}

/// A chess position with the extra state chess.js carries.
#[derive(Clone, Debug)]
pub struct Position {
    board: Board,
    halfmove: u32,
    fullmove: u32,
    /// File of chess.js's hashed en-passant square: set when a pawn has just
    /// double-pushed next to an enemy pawn, whether or not the capture is
    /// legal. Used only by `repetition_key`.
    hash_ep: Option<File>,
}

fn color_of(c: cc::Color) -> Color {
    match c {
        cc::Color::White => Color::White,
        cc::Color::Black => Color::Black,
    }
}

fn piece_char(p: Piece) -> char {
    match p {
        Piece::Pawn => 'p',
        Piece::Knight => 'n',
        Piece::Bishop => 'b',
        Piece::Rook => 'r',
        Piece::Queen => 'q',
        Piece::King => 'k',
    }
}

fn file_char(f: File) -> char {
    (b'a' + f as u8) as char
}

fn rank_char(r: Rank) -> char {
    (b'1' + r as u8) as char
}

fn sq_str(s: Square) -> String {
    format!("{}{}", file_char(s.file()), rank_char(s.rank()))
}

fn parse_square(s: &str) -> Option<Square> {
    let b = s.as_bytes();
    if b.len() != 2 || !(b'a'..=b'h').contains(&b[0]) || !(b'1'..=b'8').contains(&b[1]) {
        return None;
    }
    Some(Square::new(
        File::index((b[0] - b'a') as usize),
        Rank::index((b[1] - b'1') as usize),
    ))
}

fn parse_promotion(c: char) -> Option<Piece> {
    match c.to_ascii_lowercase() {
        'q' => Some(Piece::Queen),
        'r' => Some(Piece::Rook),
        'b' => Some(Piece::Bishop),
        'n' => Some(Piece::Knight),
        _ => None,
    }
}

/// chess.js scan order: a8, b8 .. h8, a7 .. h1.
fn scan_index(s: Square) -> usize {
    (7 - s.rank() as usize) * 8 + s.file() as usize
}

/// chess.js `strippedSan`: drop the first `=`, then a trailing `[+#]?[?!]*`.
fn stripped_san(s: &str) -> String {
    let s = s.replacen('=', "", 1);
    let t = s.trim_end_matches(['?', '!']);
    let t = t.strip_suffix(['+', '#']).unwrap_or(t);
    t.to_string()
}

fn re_coords() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"([pnbrqkPNBRQK])?([a-h][1-8])x?-?([a-h][1-8])([qrbnQRBN])?").expect("regex")
    })
}

fn re_loose() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"([pnbrqkPNBRQK])?([a-h]?[1-8]?)x?-?([a-h][1-8])([qrbnQRBN])?").expect("regex")
    })
}

fn re_two_squares() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[a-h]\d.*[a-h]\d").expect("regex"))
}

/// chess.js `inferPieceType`: `Some(Some(c))` filters by piece letter,
/// `Some(None)` means no filter.
fn infer_piece_type(san: &str) -> Option<char> {
    let first = san.chars().next()?;
    if ('a'..='h').contains(&first) {
        if re_two_squares().is_match(san) {
            return None;
        }
        return Some('p');
    }
    let lower = first.to_ascii_lowercase();
    Some(if lower == 'o' { 'k' } else { lower })
}

impl Position {
    /// The standard starting position.
    pub fn start() -> Position {
        Position {
            board: Board::startpos(),
            halfmove: 0,
            fullmove: 1,
            hash_ep: None,
        }
    }

    /// Parse a six-field FEN. The halfmove clock and move number are kept as
    /// given (chess.js does not cap them).
    pub fn from_fen(fen: &str) -> Result<Position, FenError> {
        let fields: Vec<&str> = fen.split_whitespace().collect();
        if fields.len() != 6 {
            return Err(FenError(format!("expected 6 fields in {fen:?}")));
        }
        let halfmove: u32 = fields[4]
            .parse()
            .map_err(|_| FenError(format!("bad halfmove clock {:?}", fields[4])))?;
        let fullmove: u32 = fields[5]
            .parse()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| FenError(format!("bad move number {:?}", fields[5])))?;
        let normalised = format!(
            "{} {} {} {} 0 1",
            fields[0], fields[1], fields[2], fields[3]
        );
        let board = Board::from_fen(&normalised, false).map_err(|e| FenError(format!("{e}")))?;
        let hash_ep = board.en_passant();
        Ok(Position {
            board,
            halfmove,
            fullmove,
            hash_ep,
        })
    }

    fn placement(&self) -> String {
        let mut out = String::new();
        for rank in (0..8).rev() {
            let mut empty = 0;
            for file in 0..8 {
                let sq = Square::new(File::index(file), Rank::index(rank));
                match (self.board.piece_on(sq), self.board.color_on(sq)) {
                    (Some(p), Some(c)) => {
                        if empty > 0 {
                            out.push_str(&empty.to_string());
                            empty = 0;
                        }
                        let ch = piece_char(p);
                        out.push(if c == cc::Color::White {
                            ch.to_ascii_uppercase()
                        } else {
                            ch
                        });
                    }
                    _ => empty += 1,
                }
            }
            if empty > 0 {
                out.push_str(&empty.to_string());
            }
            if rank > 0 {
                out.push('/');
            }
        }
        out
    }

    fn side_char(&self) -> char {
        match self.board.side_to_move() {
            cc::Color::White => 'w',
            cc::Color::Black => 'b',
        }
    }

    fn castling_str(&self) -> String {
        let mut out = String::new();
        let w = self.board.castle_rights(cc::Color::White);
        let b = self.board.castle_rights(cc::Color::Black);
        if w.short.is_some() {
            out.push('K');
        }
        if w.long.is_some() {
            out.push('Q');
        }
        if b.short.is_some() {
            out.push('k');
        }
        if b.long.is_some() {
            out.push('q');
        }
        if out.is_empty() {
            out.push('-');
        }
        out
    }

    /// The ep square chess.js prints: only when a legal en-passant capture exists.
    fn printed_ep(&self) -> Option<Square> {
        let file = self.board.en_passant()?;
        let rank = match self.board.side_to_move() {
            cc::Color::White => Rank::Sixth,
            cc::Color::Black => Rank::Third,
        };
        let target = Square::new(file, rank);
        let legal = self
            .legal_moves()
            .iter()
            .any(|m| m.piece == Piece::Pawn && m.to == target && m.from.file() != target.file());
        legal.then_some(target)
    }

    /// FEN exactly as chess.js prints it.
    pub fn fen(&self) -> String {
        let ep = self.printed_ep().map_or_else(|| "-".to_string(), sq_str);
        format!(
            "{} {} {} {} {} {}",
            self.placement(),
            self.side_char(),
            self.castling_str(),
            ep,
            self.halfmove,
            self.fullmove
        )
    }

    /// Placement, side, castling and chess.js's hashed ep file: the key chess.js
    /// uses for threefold repetition.
    pub fn repetition_key(&self) -> String {
        let ep = self.hash_ep.map_or('-', file_char);
        format!(
            "{} {} {} {}",
            self.placement(),
            self.side_char(),
            self.castling_str(),
            ep
        )
    }

    pub fn side_to_move(&self) -> Color {
        color_of(self.board.side_to_move())
    }

    pub fn halfmove_clock(&self) -> u32 {
        self.halfmove
    }

    fn legal_moves(&self) -> Vec<LegalMove> {
        let us = self.board.side_to_move();
        let mut out = Vec::new();
        self.board.generate_moves(|pm| {
            for mv in pm {
                let piece = pm.piece;
                let castle_target = piece == Piece::King && self.board.color_on(mv.to) == Some(us);
                let (to, castle) = if castle_target {
                    let short = mv.to.file() > mv.from.file();
                    let file = if short { File::G } else { File::C };
                    (Square::new(file, mv.from.rank()), Some(short))
                } else {
                    (mv.to, None)
                };
                let ep = piece == Piece::Pawn
                    && mv.from.file() != mv.to.file()
                    && self.board.piece_on(mv.to).is_none();
                let capture = !castle_target && (ep || self.board.piece_on(mv.to).is_some());
                out.push(LegalMove {
                    cozy: mv,
                    from: mv.from,
                    to,
                    piece,
                    promotion: mv.promotion,
                    capture,
                    castle,
                });
            }
            false
        });
        out.sort_by_key(|m| (scan_index(m.from), scan_index(m.to)));
        out
    }

    fn has_legal_move(&self) -> bool {
        self.board.generate_moves(|_| true)
    }

    pub fn is_checkmate(&self) -> bool {
        !self.board.checkers().is_empty() && !self.has_legal_move()
    }

    pub fn is_stalemate(&self) -> bool {
        self.board.checkers().is_empty() && !self.has_legal_move()
    }

    /// chess.js `isInsufficientMaterial`: K v K; K+B or K+N v K; or only kings
    /// and bishops with every bishop on one square colour.
    pub fn is_insufficient_material(&self) -> bool {
        let total = self.board.occupied().len() as usize;
        if total == 2 {
            return true;
        }
        let bishops = self.board.pieces(Piece::Bishop);
        let knights = self.board.pieces(Piece::Knight).len();
        if total == 3 && (bishops.len() == 1 || knights == 1) {
            return true;
        }
        if total == bishops.len() as usize + 2 {
            let colours: Vec<usize> = bishops
                .iter()
                .map(|s| (s.file() as usize + s.rank() as usize) % 2)
                .collect();
            let sum: usize = colours.iter().sum();
            return sum == 0 || sum == colours.len();
        }
        false
    }

    fn disambiguator(m: &LegalMove, all: &[LegalMove]) -> String {
        let mut ambiguities = 0;
        let mut same_rank = 0;
        let mut same_file = 0;
        for o in all {
            if o.piece == m.piece && o.from != m.from && o.to == m.to {
                ambiguities += 1;
                if o.from.rank() == m.from.rank() {
                    same_rank += 1;
                }
                if o.from.file() == m.from.file() {
                    same_file += 1;
                }
            }
        }
        if ambiguities == 0 {
            String::new()
        } else if same_rank > 0 && same_file > 0 {
            sq_str(m.from)
        } else if same_file > 0 {
            rank_char(m.from.rank()).to_string()
        } else {
            file_char(m.from.file()).to_string()
        }
    }

    /// SAN without the check or mate suffix.
    fn base_san(m: &LegalMove, all: &[LegalMove]) -> String {
        match m.castle {
            Some(true) => return "O-O".to_string(),
            Some(false) => return "O-O-O".to_string(),
            None => {}
        }
        let mut out = String::new();
        if m.piece != Piece::Pawn {
            out.push(piece_char(m.piece).to_ascii_uppercase());
            out.push_str(&Self::disambiguator(m, all));
        }
        if m.capture {
            if m.piece == Piece::Pawn {
                out.push(file_char(m.from.file()));
            }
            out.push('x');
        }
        out.push_str(&sq_str(m.to));
        if let Some(p) = m.promotion {
            out.push('=');
            out.push(piece_char(p).to_ascii_uppercase());
        }
        out
    }

    fn next_board(&self, m: &LegalMove) -> Board {
        let mut next = self.board.clone();
        next.play_unchecked(m.cozy);
        next
    }

    fn full_san(&self, m: &LegalMove, all: &[LegalMove]) -> String {
        let mut san = Self::base_san(m, all);
        let after = self.next_board(m);
        if !after.checkers().is_empty() {
            let mate = !after.generate_moves(|_| true);
            san.push(if mate { '#' } else { '+' });
        }
        san
    }

    fn uci_of(m: &LegalMove) -> String {
        format!(
            "{}{}{}",
            sq_str(m.from),
            sq_str(m.to),
            m.promotion
                .map(piece_char)
                .map_or(String::new(), |c| c.to_string())
        )
    }

    /// Apply a legal move, updating the clocks and chess.js's hashed ep file.
    fn apply(&mut self, m: &LegalMove) {
        let us = self.board.side_to_move();
        self.board.play_unchecked(m.cozy);
        if m.piece == Piece::Pawn || m.capture {
            self.halfmove = 0;
        } else {
            self.halfmove += 1;
        }
        if us == cc::Color::Black {
            self.fullmove += 1;
        }
        self.hash_ep = None;
        if m.piece == Piece::Pawn && (m.from.rank() as i8 - m.to.rank() as i8).abs() == 2 {
            let them = self.board.side_to_move();
            let enemy_pawns = self.board.colored_pieces(them, Piece::Pawn);
            let f = m.to.file() as i8;
            let adjacent = [f - 1, f + 1].into_iter().any(|nf| {
                (0..8).contains(&nf)
                    && enemy_pawns.has(Square::new(File::index(nf as usize), m.to.rank()))
            });
            if adjacent {
                self.hash_ep = Some(m.to.file());
            }
        }
    }

    fn find_san(&self, token: &str, all: &[LegalMove]) -> Option<LegalMove> {
        let mut clean = stripped_san(token);
        if clean == "0-0" {
            clean = "O-O".to_string();
        } else if clean == "0-0-0" {
            clean = "O-O-O".to_string();
        }
        let piece_type = infer_piece_type(&clean);
        let candidates: Vec<LegalMove> = all
            .iter()
            .filter(|m| match piece_type {
                Some(c) => piece_char(m.piece) == c,
                None => true,
            })
            .copied()
            .collect();
        // Strict pass.
        for m in &candidates {
            if clean == stripped_san(&Self::base_san(m, &candidates)) {
                return Some(*m);
            }
        }
        // Permissive pass.
        let caps = re_coords()
            .captures(&clean)
            .or_else(|| re_loose().captures(&clean))?;
        let piece = caps.get(1).map(|m| m.as_str().to_ascii_lowercase());
        let from = caps.get(2).map_or("", |m| m.as_str());
        let to = caps.get(3)?.as_str();
        let promotion = caps
            .get(4)
            .and_then(|m| m.as_str().chars().next())
            .and_then(parse_promotion);
        let overly = from.len() == 1;
        let filter: Option<char> = match &piece {
            Some(p) => p.chars().next(),
            None => piece_type,
        };
        let pool: Vec<LegalMove> = all
            .iter()
            .filter(|m| match filter {
                Some(c) => piece_char(m.piece) == c,
                None => true,
            })
            .copied()
            .collect();
        let piece_ok = |m: &LegalMove| match &piece {
            Some(p) => p.starts_with(piece_char(m.piece)),
            None => true,
        };
        let promo_ok = |m: &LegalMove| promotion.is_none() || promotion == m.promotion;
        for m in &pool {
            if from.is_empty() {
                let san = stripped_san(&Self::base_san(m, &pool)).replacen('x', "", 1);
                if clean == san {
                    return Some(*m);
                }
            } else if piece_ok(m) && sq_str(m.from) == from && sq_str(m.to) == to && promo_ok(m) {
                return Some(*m);
            } else if overly {
                let sq = sq_str(m.from);
                let (a, b) = (&sq[0..1], &sq[1..2]);
                if piece_ok(m) && sq_str(m.to) == to && (from == a || from == b) && promo_ok(m) {
                    return Some(*m);
                }
            }
        }
        None
    }

    /// Play a SAN token with chess.js's permissive parser. The position is
    /// unchanged on error.
    pub fn play_san(&mut self, token: &str) -> Result<PlayedMove, IllegalMove> {
        let all = self.legal_moves();
        let m = self
            .find_san(token, &all)
            .ok_or_else(|| IllegalMove(token.to_string()))?;
        let san = self.full_san(&m, &all);
        let uci = Self::uci_of(&m);
        self.apply(&m);
        Ok(PlayedMove { san, uci })
    }

    /// Apply a UCI move to a copy of this position. Castling may be given as
    /// the king's two-square step (`e1g1`, chess.js) or king-takes-rook
    /// (`e1h1`, some engines). A promotion needs its suffix.
    pub fn play_uci(&self, uci: &str) -> Result<Position, ChessError> {
        let bad = || ChessError::InvalidUci(uci.to_string());
        if !uci.is_ascii() || !(4..=5).contains(&uci.len()) {
            return Err(bad());
        }
        let from = parse_square(&uci[0..2]).ok_or_else(bad)?;
        let to = parse_square(&uci[2..4]).ok_or_else(bad)?;
        let promotion = match uci.chars().nth(4) {
            None => None,
            Some(c) => Some(parse_promotion(c).ok_or_else(bad)?),
        };
        let all = self.legal_moves();
        let found = all
            .iter()
            .find(|m| m.from == from && m.to == to && m.promotion == promotion)
            .or_else(|| {
                all.iter().find(|m| {
                    m.castle.is_some() && promotion.is_none() && m.from == from && m.cozy.to == to
                })
            })
            .ok_or_else(|| ChessError::Illegal(uci.to_string()))?;
        let mut next = self.clone();
        next.apply(found);
        Ok(next)
    }
}

/// Profile-engine additions (R2 Task 4): in-place UCI play with SAN, board access and check state.
/// [`Position::play_uci`] keeps its R1 signature, so the in-place variant is `push_uci`.
impl Position {
    /// Play a UCI move in place and return its chess.js SAN and UCI. Castling is accepted as the
    /// king's two-square step (`e1g1`) or king-takes-rook (`e1h1`) and reported as the former; a
    /// promotion needs its suffix. The position is unchanged on error.
    pub fn push_uci(&mut self, uci: &str) -> Result<PlayedMove, IllegalMove> {
        let illegal = || IllegalMove(uci.to_string());
        if !uci.is_ascii() || !(4..=5).contains(&uci.len()) {
            return Err(illegal());
        }
        let from = parse_square(&uci[0..2]).ok_or_else(illegal)?;
        let to = parse_square(&uci[2..4]).ok_or_else(illegal)?;
        let promotion = match uci.chars().nth(4) {
            None => None,
            Some(c) => Some(parse_promotion(c).ok_or_else(illegal)?),
        };
        let all = self.legal_moves();
        let found = all
            .iter()
            .find(|m| m.from == from && m.to == to && m.promotion == promotion)
            .or_else(|| {
                all.iter().find(|m| {
                    m.castle.is_some() && promotion.is_none() && m.from == from && m.cozy.to == to
                })
            })
            .copied()
            .ok_or_else(illegal)?;
        let san = self.full_san(&found, &all);
        let uci = Self::uci_of(&found);
        self.apply(&found);
        Ok(PlayedMove { san, uci })
    }

    /// The underlying cozy-chess board (for attack and piece-set queries).
    ///
    /// It is not a chess.js FEN source: its halfmove clock and move number are normalised (this
    /// wrapper keeps the real ones) and its en-passant file is cozy-chess's, not the square chess.js
    /// prints. Build FENs with [`Position::fen`].
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// Whether the side to move is in check.
    pub fn is_check(&self) -> bool {
        !self.board.checkers().is_empty()
    }
}
