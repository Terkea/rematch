//! Opening classification (R2 Task 16, TS M2 Task 12).
//!
//! A game is classified by the positions it reaches, not by its move order: the name is the
//! one of the deepest ply whose `fen_key` is in [`openings`], so transpositions get the same
//! name. Callers use it instead of the opening names Lichess and chess.com report (controller
//! ruling), so games from both platforms are named from one dataset.
//!
//! Line keys (R-7) are `"{color}:{slug}"`, where the slug is the lowercase ASCII form of
//! [`line_family`] (diacritics folded, apostrophes dropped, other runs of non-alphanumerics
//! joined by `-`). A slug longer than [`LINE_SLUG_MAX_CHARS`] is cut at a `-` boundary and gets
//! `-` plus the first 6 hex digits of `sha256(full slug)`, keeping the whole slug within
//! [`LINE_SLUG_MAX_CHARS`]. So a line key has at most 46 characters and the series key
//! `opening:{line_key}` at most 54, inside the contract's 64.

use super::dataset::openings;
use chess_core::Color;
use chess_core::fen::fen_key;
use sha2::{Digest, Sha256};

/// Maximum slug length in a line key, hash suffix included (R-7).
pub const LINE_SLUG_MAX_CHARS: usize = 40;

/// Hex digits of `sha256(full slug)` appended to a cut slug.
pub const LINE_SLUG_HASH_CHARS: usize = 6;

/// The deepest named position a game reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classified {
    pub eco: String,
    pub name: String,
    pub fen_key: String,
    /// 1-based ply of the move that reached the position.
    pub ply: u32,
    /// The side that played that move (the line's entry mover, see [`line_kind`]).
    pub mover: Color,
}

/// Whether the player steered into the line (`Chosen`) or the opponent did (`Faced`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Chosen,
    Faced,
}

impl LineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LineKind::Chosen => "chosen",
            LineKind::Faced => "faced",
        }
    }
}

/// The deepest named position among `plies` (`(fen_after, mover)` per ply, in game order), or
/// `None` when no ply reaches one (the start position is not named). Clocks in the FENs are
/// ignored.
pub fn classify_opening(plies: &[(String, Color)]) -> Option<Classified> {
    let book = openings();
    plies
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, (fen, mover))| {
            let key = fen_key(fen);
            let named = book.get(&key)?;
            Some(Classified {
                eco: named.eco.clone(),
                name: named.name.clone(),
                fen_key: key,
                ply: u32::try_from(index + 1).unwrap_or(u32::MAX),
                mover: *mover,
            })
        })
}

/// The line a name belongs to: the text before the first `,`
/// (`"Sicilian Defense: Najdorf Variation, English Attack"` → `"Sicilian Defense: Najdorf
/// Variation"`).
pub fn line_family(name: &str) -> &str {
    name.split_once(',')
        .map_or(name, |(family, _)| family)
        .trim_end()
}

/// Stable key of the line `name` belongs to, for the player's `color` (R-7).
pub fn line_key(color: Color, name: &str) -> String {
    format!("{}:{}", color.as_str(), capped(&slug(line_family(name))))
}

/// `Chosen` when the player made the move that entered the line, else `Faced`.
pub fn line_kind(entry_mover: Color, player: Color) -> LineKind {
    if entry_mover == player {
        LineKind::Chosen
    } else {
        LineKind::Faced
    }
}

/// Lowercase ASCII slug: letters and digits kept (diacritics folded), apostrophes dropped,
/// every other run joined by one `-`, no leading or trailing `-`.
fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    let mut buf = [0u8; 4];
    for c in text.chars().flat_map(char::to_lowercase) {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}') {
            continue;
        }
        let ascii = if c.is_ascii_alphanumeric() {
            Some(&*c.encode_utf8(&mut buf))
        } else {
            fold(c)
        };
        match ascii {
            Some(ascii) => {
                if gap && !out.is_empty() {
                    out.push('-');
                }
                gap = false;
                out.push_str(ascii);
            }
            None => gap = true,
        }
    }
    if out.is_empty() {
        out.push_str("unnamed");
    }
    out
}

/// ASCII form of a lowercase Latin letter with a diacritic, or `None` (a separator).
fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'ł' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
        'œ' => "oe",
        'ř' => "r",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// Cuts an ASCII slug longer than [`LINE_SLUG_MAX_CHARS`] at a `-` boundary and appends
/// `-` and [`LINE_SLUG_HASH_CHARS`] hex digits of `sha256(slug)`.
fn capped(slug: &str) -> String {
    if slug.len() <= LINE_SLUG_MAX_CHARS {
        return slug.to_owned();
    }
    let budget = LINE_SLUG_MAX_CHARS - 1 - LINE_SLUG_HASH_CHARS;
    let head = slug.get(..budget).unwrap_or(slug);
    let cut = if slug.as_bytes().get(budget) == Some(&b'-') {
        head
    } else {
        head.rsplit_once('-').map_or(head, |(kept, _)| kept)
    };
    let digest = hex::encode(Sha256::digest(slug.as_bytes()));
    let suffix = digest.get(..LINE_SLUG_HASH_CHARS).unwrap_or(&digest);
    format!("{}-{suffix}", cut.trim_end_matches('-'))
}
