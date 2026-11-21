//! Text normalisation applied before any extraction, so notation tricks cannot smuggle moves or
//! numbers. A port of m3's `validator/normalize.ts`, operation for operation (ADR-R3-2).

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use super::bounded::{m_end, scan, splice};

/// Zero-width, bidi, joiner, variation-selector, filler and other invisible code points: exactly
/// TS's `INVISIBLES`.
pub fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180B}'..='\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{E0000}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// TS's `DASH` class: `‐-―−➖﹘﹣－`.
pub fn is_dash(c: char) -> bool {
    matches!(
        c,
        '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{2796}' | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}'
    )
}

/// The figurine-to-letter map (white and black pieces, no pawns).
pub fn figurine_letter(c: char) -> Option<char> {
    Some(match c {
        '\u{2654}' | '\u{265A}' => 'K',
        '\u{2655}' | '\u{265B}' => 'Q',
        '\u{2656}' | '\u{265C}' => 'R',
        '\u{2657}' | '\u{265D}' => 'B',
        '\u{2658}' | '\u{265E}' => 'N',
        _ => return None,
    })
}

/// Removes every [`is_invisible`] code point.
pub fn strip_invisibles(s: &str) -> String {
    s.chars().filter(|c| !is_invisible(*c)).collect()
}

/// `\p{Nd}`, as JS's `u`-flag class.
fn is_nd(c: char) -> bool {
    static ND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\p{Nd}\z").expect("valid regex"));
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    let mut buf = [0u8; 4];
    ND.is_match(c.encode_utf8(&mut buf))
}

/// `(?<=\p{Nd})[    ](?=\p{Nd}{3}(?!\p{Nd}))` → `,`.
fn group_thousands(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let nd = |i: usize| chars.get(i).is_some_and(|c| is_nd(*c));
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let space = matches!(c, '\u{A0}' | '\u{202F}' | '\u{2009}' | '\u{2007}');
            if space && i > 0 && nd(i - 1) && nd(i + 1) && nd(i + 2) && nd(i + 3) && !nd(i + 4) {
                ','
            } else {
                c
            }
        })
        .collect()
}

/// Maps each char that passes `hit(prev, c, next)` to `-`; the lookarounds read the input string.
fn map_dashes(s: &str, hit: impl Fn(Option<char>, char, Option<char>) -> bool) -> String {
    let chars: Vec<char> = s.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let prev = i.checked_sub(1).and_then(|j| chars.get(j)).copied();
            if hit(prev, c, chars.get(i + 1).copied()) {
                '-'
            } else {
                c
            }
        })
        .collect()
}

/// `(?<![A-Za-z0-9])[O0]-[O0](-[O0])?(?![A-Za-z0-9])` → `O-O` or `O-O-O`.
fn castling(s: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?P<m>[O0]-[O0](?P<q>-[O0])?)(?:[^A-Za-z0-9]|$)").expect("valid regex")
    });
    let not_alnum = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric());
    let edits = scan(&RE, s, not_alnum, |c| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        let r = if c.name("q").is_some() {
            "O-O-O"
        } else {
            "O-O"
        };
        Some((end, (start, end, r.to_owned())))
    });
    splice(s, &edits)
}

/// Normalises text before extraction, in TS's order: space-grouped thousands become commas (before
/// NFKC erases the space kind), NFKC, invisibles stripped again, castling dashes, a minus before a
/// digit, figurines, then `0-0` castling.
pub fn normalize_text(s: &str) -> String {
    let t = group_thousands(&strip_invisibles(s));
    let t = strip_invisibles(&t.nfkc().collect::<String>());
    let castle = |c: Option<char>| matches!(c, Some('O' | 'o' | '0'));
    let t = map_dashes(&t, |p, c, n| castle(p) && is_dash(c) && castle(n));
    let t = map_dashes(&t, |_, c, n| {
        is_dash(c) && n.is_some_and(|n| n.is_ascii_digit())
    });
    let t: String = t.chars().map(|c| figurine_letter(c).unwrap_or(c)).collect();
    castling(&t)
}
