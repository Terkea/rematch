//! The CC0 lichess-org/chess-openings dataset (R2 Task 16, TS M2 Task 12).
//!
//! The five source tables (`eco`, `name`, `pgn`) are embedded with `include_str!` and replayed
//! once, on first use, with [`Position::play_san`]. Each row names the position its `pgn` ends
//! in, keyed by [`fen_key`]: piece placement, side to move, castling and the en-passant square
//! only when a capture is legal, which is the dataset's EPD. Keying by position rather than by
//! move order is how the dataset is meant to be used: a game that reaches a named position by
//! another move order gets the same name. Provenance, checksums and the licence (CC0-1.0) are
//! in `data/chess-openings/SOURCE`. Only the data is used, never the repository's scripts.

use chess_core::board::Position;
use chess_core::fen::fen_key;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::OnceLock;

/// ECO code and full name of a named position, e.g. `C50` / `Italian Game`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OpeningName {
    pub eco: String,
    pub name: String,
}

/// The vendored tables as `(file name, contents)`, in file order.
pub const VENDORED_TSVS: [(&str, &str); 5] = [
    ("a.tsv", include_str!("../../data/chess-openings/a.tsv")),
    ("b.tsv", include_str!("../../data/chess-openings/b.tsv")),
    ("c.tsv", include_str!("../../data/chess-openings/c.tsv")),
    ("d.tsv", include_str!("../../data/chess-openings/d.tsv")),
    ("e.tsv", include_str!("../../data/chess-openings/e.tsv")),
];

const HEADER: &str = "eco\tname\tpgn";

/// A table without the dataset's shape. The vendored tables never produce one
/// (`tests/openings.rs` replays all of them).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DatasetError {
    #[error("{file}: the first line is not the header \"eco<TAB>name<TAB>pgn\"")]
    Header { file: String },
    #[error("{file}:{line}: expected three non-empty tab-separated fields")]
    Row { file: String, line: usize },
    #[error("{file}:{line}: illegal move {san:?}")]
    IllegalMove {
        file: String,
        line: usize,
        san: String,
    },
}

/// Every named position of the vendored dataset by `fen_key`, built on first use.
pub fn openings() -> &'static HashMap<String, OpeningName> {
    static OPENINGS: OnceLock<HashMap<String, OpeningName>> = OnceLock::new();
    OPENINGS.get_or_init(|| match build_openings(&VENDORED_TSVS) {
        Ok(map) => map,
        Err(e) => panic!("vendored chess-openings data is invalid: {e}"),
    })
}

/// Replays every row of `tables` (`(file name, contents)`) and maps the final position's
/// `fen_key` to its name. When several rows end in the same position, the longest name wins
/// (ties: the smaller name, then the smaller ECO code), so the result does not depend on row
/// order. Blank lines are skipped; `\r\n` line ends are accepted.
pub fn build_openings(
    tables: &[(&str, &str)],
) -> Result<HashMap<String, OpeningName>, DatasetError> {
    let mut replay = Replay::new();
    let mut map: HashMap<String, OpeningName> = HashMap::new();
    for &(file, text) in tables {
        let mut lines = text.lines().enumerate();
        match lines.next() {
            Some((_, header)) if header.trim_start_matches('\u{feff}').trim_end() == HEADER => {}
            _ => {
                return Err(DatasetError::Header {
                    file: file.to_owned(),
                });
            }
        }
        for (index, line) in lines {
            if line.trim().is_empty() {
                continue;
            }
            let line_no = index + 1;
            let row_error = || DatasetError::Row {
                file: file.to_owned(),
                line: line_no,
            };
            let mut fields = line.split('\t').map(str::trim);
            let (Some(eco), Some(name), Some(pgn), None) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                return Err(row_error());
            };
            if eco.is_empty() || name.is_empty() || pgn.is_empty() {
                return Err(row_error());
            }
            let key = replay
                .key_after(pgn)
                .map_err(|san| DatasetError::IllegalMove {
                    file: file.to_owned(),
                    line: line_no,
                    san: san.to_owned(),
                })?;
            let named = OpeningName {
                eco: eco.to_owned(),
                name: name.to_owned(),
            };
            match map.get_mut(&key) {
                Some(kept) if rank(&named) < rank(kept) => *kept = named,
                Some(_) => {}
                None => {
                    map.insert(key, named);
                }
            }
        }
    }
    Ok(map)
}

/// Duplicate order: longest name first, then the smaller name, then the smaller ECO code.
fn rank(o: &OpeningName) -> (Reverse<usize>, &str, &str) {
    (Reverse(o.name.chars().count()), &o.name, &o.eco)
}

/// Replays SAN sequences over a shared move tree, so each distinct prefix is played once
/// (the tables share most of their prefixes).
struct Replay<'a> {
    positions: Vec<Position>,
    children: HashMap<(usize, &'a str), usize>,
}

impl<'a> Replay<'a> {
    fn new() -> Self {
        Replay {
            positions: vec![Position::start()],
            children: HashMap::new(),
        }
    }

    /// `fen_key` of the position after `pgn` (move numbers such as `1.` or `12...` are
    /// skipped); on an illegal move, returns that SAN token.
    fn key_after(&mut self, pgn: &'a str) -> Result<String, &'a str> {
        let mut node = 0;
        for token in pgn.split_whitespace() {
            let san = strip_move_number(token);
            if san.is_empty() {
                continue;
            }
            node = match self.children.get(&(node, san)) {
                Some(&child) => child,
                None => {
                    let mut next = self.positions.get(node).ok_or(san)?.clone();
                    next.play_san(san).map_err(|_| san)?;
                    self.positions.push(next);
                    let child = self.positions.len() - 1;
                    self.children.insert((node, san), child);
                    child
                }
            };
        }
        let end = self.positions.get(node).ok_or("")?;
        Ok(fen_key(&end.fen()))
    }
}

/// `"1."` and `"12..."` become empty, `"1.e4"` becomes `"e4"`, anything else is unchanged.
fn strip_move_number(token: &str) -> &str {
    let rest = token.trim_start_matches(|c: char| c.is_ascii_digit());
    if rest.len() < token.len() && rest.starts_with('.') {
        rest.trim_start_matches('.')
    } else {
        token
    }
}
