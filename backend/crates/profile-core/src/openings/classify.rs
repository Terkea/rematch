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
use chess_core::board::Position;
use chess_core::fen::fen_key;
use sha2::{Digest, Sha256};

pub use coaching_input::LineSide;

/// The brief's name for the contract's [`LineSide`]: one type, `chosen` / `faced`.
pub type LineKind = LineSide;

/// Maximum slug length in a line key, hash suffix included (R-7).
pub const LINE_SLUG_MAX_CHARS: usize = 40;

/// Hex digits of `sha256(full slug)` appended to a cut slug.
pub const LINE_SLUG_HASH_CHARS: usize = 6;

/// The deepest named position a game reached, and where its line was entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Classified {
    pub eco: String,
    pub name: String,
    pub fen_key: String,
    /// 1-based ply of the move that reached the deepest named position.
    pub ply: u32,
    /// The side that played the deepest ply. This is not the line's entry mover: use
    /// [`Classified::entry_mover`] with [`line_kind`].
    pub mover: Color,
    /// 1-based ply of the first named position whose [`line_family`] equals the deepest
    /// name's family: the move that entered the line (`5...a6` for any Najdorf).
    pub entry_ply: u32,
    /// The side that played [`Classified::entry_ply`].
    pub entry_mover: Color,
}

/// The deepest named position among `plies` (`(fen_after, mover)` per ply, in game order), or
/// `None` when no ply reaches one (the start position is not named).
///
/// Each FEN is normalised through `Position::from_fen(..).fen()` before keying, so clocks are
/// ignored and an en-passant square is kept only when a capture is legal, whatever the source
/// printed (an unparsable FEN is keyed as given). Plies are numbered from 1 in slice order: for
/// a game from a custom start, ply 1 is the first move after that start.
pub fn classify_opening(plies: &[(String, Color)]) -> Option<Classified> {
    let book = openings();
    let named: Vec<Option<(String, &super::dataset::OpeningName)>> = plies
        .iter()
        .map(|(fen, _)| {
            let key = normalised_key(fen);
            book.get(&key).map(|o| (key, o))
        })
        .collect();
    let deepest = named.iter().rposition(Option::is_some)?;
    let (key, o) = named.get(deepest)?.as_ref()?;
    let family = line_family(&o.name);
    let entry = named
        .iter()
        .position(|n| {
            n.as_ref()
                .is_some_and(|(_, e)| line_family(&e.name) == family)
        })
        .unwrap_or(deepest);
    let ply_of = |i: usize| u32::try_from(i + 1).unwrap_or(u32::MAX);
    let mover_of = |i: usize| plies.get(i).map_or(Color::White, |(_, m)| *m);
    Some(Classified {
        eco: o.eco.clone(),
        name: o.name.clone(),
        fen_key: key.clone(),
        ply: ply_of(deepest),
        mover: mover_of(deepest),
        entry_ply: ply_of(entry),
        entry_mover: mover_of(entry),
    })
}

fn normalised_key(fen: &str) -> String {
    match Position::from_fen(fen) {
        Ok(p) => fen_key(&p.fen()),
        Err(_) => fen_key(fen),
    }
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

/// `Chosen` when the player made the move that entered the line
/// ([`Classified::entry_mover`]), else `Faced`.
pub fn line_kind(entry_mover: Color, player: Color) -> LineSide {
    if entry_mover == player {
        LineSide::Chosen
    } else {
        LineSide::Faced
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
