//! Text normalisation applied before any extraction, so notation tricks cannot smuggle moves or
//! numbers. A port of m3's `validator/normalize.ts`, operation for operation ([`NormMode::TsExact`]),
//! plus the deliberate hardening of ADR 0058 ([`NormMode::Hardened`]): a missed claim is a leak.
//!
//! Hardening alone can also hide a claim: folding a neighbour to ASCII glues it on (`Nf3é` becomes
//! `Nf3e`, which no boundary check accepts). So the extractors run both modes and return the union
//! (fix round 2), which is a superset of TS by construction.
//!
//! Hardening beyond TS (each is a recorded deviation, H1–H6):
//! - H1: every `\p{Nd}` digit is folded to ASCII after NFKC (`٣` is `3`);
//! - H2: the whole `\p{Default_Ignorable_Code_Point}` set is stripped, not just TS's list;
//! - H3: combining marks are removed (NFKD, drop `\p{Mn}`, recompose with NFC);
//! - H4: Cyrillic, Greek and Armenian homoglyphs of chess letters and digits fold to ASCII
//!   ([`HOMOGLYPHS`]);
//! - H5: the dash set adds U+02D7, U+2E3A, U+2E3B, U+058A and U+30FC ([`DASH_RANGES`], shared with
//!   the long-algebraic pattern);
//! - H6: U+2006, U+2008 and U+200A also group thousands.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use super::bounded::{m_end, scan, splice};

fn re(p: &str) -> Regex {
    Regex::new(p).expect("valid regex")
}

/// Which normalisation to run. Extraction runs both and returns the union (ADR 0058, fix round 2):
/// folding a non-ASCII neighbour to ASCII (H1, H3, H4) can glue it onto a move or date and hide
/// it from the boundary checks (`Nf3é` becomes `Nf3e`), so the TS-exact path must run too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormMode {
    /// TS's `normalizeText`, step for step, with TS's sets.
    TsExact,
    /// TS's steps plus H1–H6.
    Hardened,
}

/// The TS-exact steps only: [`NormMode::TsExact`].
pub fn normalize_ts(s: &str) -> String {
    normalize_with(s, NormMode::TsExact)
}

/// TS's `INVISIBLES`, exactly.
static TS_INVISIBLES: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"[\x{AD}\x{34F}\x{61C}\x{115F}\x{1160}\x{17B4}\x{17B5}\x{180B}-\x{180E}\x{200B}-\x{200F}",
        r"\x{202A}-\x{202E}\x{2060}-\x{2069}\x{3164}\x{FE00}-\x{FE0F}\x{FEFF}\x{FFA0}",
        r"\x{E0000}-\x{E007F}\x{E0100}-\x{E01EF}]+"
    ))
});
/// TS's `INVISIBLES` plus (H2) every `Default_Ignorable_Code_Point`.
static INVISIBLES: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"[\x{AD}\x{34F}\x{61C}\x{115F}\x{1160}\x{17B4}\x{17B5}\x{180B}-\x{180E}\x{200B}-\x{200F}",
        r"\x{202A}-\x{202E}\x{2060}-\x{2069}\x{3164}\x{FE00}-\x{FE0F}\x{FEFF}\x{FFA0}",
        r"\x{E0000}-\x{E007F}\x{E0100}-\x{E01EF}\p{Default_Ignorable_Code_Point}]+"
    ))
});
static MN: LazyLock<Regex> = LazyLock::new(|| re(r"\p{Mn}+"));

fn char_matches(r: &Regex, c: char) -> bool {
    let mut buf = [0u8; 4];
    r.is_match(c.encode_utf8(&mut buf))
}

/// True for an invisible code point: TS's `INVISIBLES` (zero-width, bidi, joiners, variation
/// selectors, fillers, tags) and every other `Default_Ignorable_Code_Point` (H2).
pub fn is_invisible(c: char) -> bool {
    !c.is_ascii() && char_matches(&INVISIBLES, c)
}

/// Removes every [`is_invisible`] code point (the hardened set).
pub fn strip_invisibles(s: &str) -> String {
    strip_invisibles_with(s, NormMode::Hardened)
}

/// Removes TS's `INVISIBLES` ([`NormMode::TsExact`]) or the hardened set.
pub fn strip_invisibles_with(s: &str, mode: NormMode) -> String {
    let r = match mode {
        NormMode::TsExact => &TS_INVISIBLES,
        NormMode::Hardened => &INVISIBLES,
    };
    r.replace_all(s, "").into_owned()
}

/// The dash set: TS's `DASH` (`\u{2010}-\u{2015}\u{2212}\u{2796}\u{FE58}\u{FE63}\u{FF0D}`, the
/// first [`TS_DASHES`] entries) plus U+02D7, U+2E3A, U+2E3B, U+058A and U+30FC (H5). One table for
/// [`is_dash`] and [`dash_class`].
pub const DASH_RANGES: [(char, char); 11] = [
    ('\u{2010}', '\u{2015}'),
    ('\u{2212}', '\u{2212}'),
    ('\u{2796}', '\u{2796}'),
    ('\u{FE58}', '\u{FE58}'),
    ('\u{FE63}', '\u{FE63}'),
    ('\u{FF0D}', '\u{FF0D}'),
    ('\u{02D7}', '\u{02D7}'),
    ('\u{2E3A}', '\u{2E3A}'),
    ('\u{2E3B}', '\u{2E3B}'),
    ('\u{058A}', '\u{058A}'),
    ('\u{30FC}', '\u{30FC}'),
];
/// The number of [`DASH_RANGES`] entries that form TS's `DASH`.
pub const TS_DASHES: usize = 6;

fn dash_ranges(mode: NormMode) -> &'static [(char, char)] {
    match mode {
        NormMode::TsExact => &DASH_RANGES[..TS_DASHES],
        NormMode::Hardened => &DASH_RANGES,
    }
}

/// True for a member of the hardened dash set (not the ASCII hyphen).
pub fn is_dash(c: char) -> bool {
    is_dash_with(c, NormMode::Hardened)
}

/// True for a member of TS's `DASH` ([`NormMode::TsExact`]) or the hardened set.
pub fn is_dash_with(c: char, mode: NormMode) -> bool {
    dash_ranges(mode)
        .iter()
        .any(|(a, b)| (*a..=*b).contains(&c))
}

/// The hardened [`DASH_RANGES`] as the body of a regex character class.
pub fn dash_class() -> String {
    DASH_RANGES
        .iter()
        .map(|(a, b)| {
            if a == b {
                format!("\\x{{{:X}}}", u32::from(*a))
            } else {
                format!("\\x{{{:X}}}-\\x{{{:X}}}", u32::from(*a), u32::from(*b))
            }
        })
        .collect()
}

/// H4: Cyrillic, Greek and Armenian lookalikes of chess letters (files, pieces, `O`, `x`) and
/// digits, folded to ASCII. A superset of TS `leaks.ts`'s `CONFUSABLES`; one table for the
/// normaliser and the leak detector.
pub const HOMOGLYPHS: [(char, char); 60] = [
    // Cyrillic lowercase
    ('\u{430}', 'a'), // а
    ('\u{432}', 'b'), // в
    ('\u{431}', '6'), // б
    ('\u{441}', 'c'), // с
    ('\u{501}', 'd'), // ԁ
    ('\u{435}', 'e'), // е
    ('\u{4BB}', 'h'), // һ
    ('\u{456}', 'i'), // і
    ('\u{458}', 'j'), // ј
    ('\u{43E}', 'o'), // о
    ('\u{440}', 'p'), // р
    ('\u{51B}', 'q'), // ԛ
    ('\u{455}', 's'), // ѕ
    ('\u{445}', 'x'), // х
    ('\u{443}', 'y'), // у
    // Cyrillic uppercase
    ('\u{410}', 'A'), // А
    ('\u{412}', 'B'), // В
    ('\u{421}', 'C'), // С
    ('\u{415}', 'E'), // Е
    ('\u{41D}', 'H'), // Н
    ('\u{406}', 'I'), // І
    ('\u{408}', 'J'), // Ј
    ('\u{41A}', 'K'), // К
    ('\u{41C}', 'M'), // М
    ('\u{41E}', 'O'), // О
    ('\u{420}', 'P'), // Р
    ('\u{405}', 'S'), // Ѕ
    ('\u{422}', 'T'), // Т
    ('\u{425}', 'X'), // Х
    ('\u{417}', '3'), // З
    // Greek lowercase
    ('\u{3B1}', 'a'), // α
    ('\u{3B5}', 'e'), // ε
    ('\u{3B9}', 'i'), // ι
    ('\u{3BA}', 'k'), // κ
    ('\u{3BD}', 'v'), // ν
    ('\u{3BF}', 'o'), // ο
    ('\u{3C1}', 'p'), // ρ
    ('\u{3C7}', 'x'), // χ
    // Greek uppercase
    ('\u{391}', 'A'), // Α
    ('\u{392}', 'B'), // Β
    ('\u{395}', 'E'), // Ε
    ('\u{396}', 'Z'), // Ζ
    ('\u{397}', 'H'), // Η
    ('\u{399}', 'I'), // Ι
    ('\u{39A}', 'K'), // Κ
    ('\u{39C}', 'M'), // Μ
    ('\u{39D}', 'N'), // Ν
    ('\u{39F}', 'O'), // Ο
    ('\u{3A1}', 'P'), // Ρ
    ('\u{3A4}', 'T'), // Τ
    ('\u{3A5}', 'Y'), // Υ
    ('\u{3A7}', 'X'), // Χ
    // Armenian
    ('\u{570}', 'h'), // հ
    ('\u{578}', 'n'), // ո
    ('\u{57D}', 'u'), // ս
    ('\u{585}', 'o'), // օ
    ('\u{555}', 'O'), // Օ
    // Latin lookalikes outside ASCII
    ('\u{261}', 'g'), // ɡ
    ('\u{251}', 'a'), // ɑ
    ('\u{131}', 'i'), // ı
];

/// The ASCII fold of a [`HOMOGLYPHS`] member.
pub fn homoglyph(c: char) -> Option<char> {
    HOMOGLYPHS.iter().find(|(h, _)| *h == c).map(|(_, a)| *a)
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

/// The code point of the `0` of every run of ten `\p{Nd}` digits, sorted. Generated from Node 22.22
/// (ICU 77.1, Unicode 16.0) by testing `/\p{Nd}/u` on every code point; adjacent runs (the
/// mathematical digits at U+1D7CE, and U+116D0/U+116DA) are listed per ten.
const ND_ZEROS: [u32; 76] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x10D40, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
    0x11650, 0x116C0, 0x116D0, 0x116DA, 0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50,
    0x11DA0, 0x11F50, 0x16130, 0x16A60, 0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8,
    0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// The decimal value of a `\p{Nd}` char, from [`ND_ZEROS`].
fn nd_value(c: char) -> Option<u32> {
    let cp = u32::from(c);
    let i = ND_ZEROS.partition_point(|z| *z <= cp).checked_sub(1)?;
    let off = cp - ND_ZEROS[i];
    (off < 10).then_some(off)
}

/// `\p{Nd}`, as JS's `u`-flag class.
fn is_nd(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    nd_value(c).is_some()
}

/// H1: the ASCII digit of a non-ASCII `\p{Nd}` char.
fn ascii_digit(c: char) -> Option<char> {
    if c.is_ascii() {
        return None;
    }
    char::from_digit(nd_value(c)?, 10)
}

/// `(?<=\p{Nd})[\u{A0}\u{202F}\u{2009}\u{2007}](?=\p{Nd}{3}(?!\p{Nd}))` → `,`. The hardened mode
/// adds U+2006, U+2008 and U+200A (H6).
fn group_thousands(s: &str, mode: NormMode) -> String {
    let chars: Vec<char> = s.chars().collect();
    let nd = |i: usize| chars.get(i).is_some_and(|c| is_nd(*c));
    let hardened = mode == NormMode::Hardened;
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let space = matches!(c, '\u{A0}' | '\u{202F}' | '\u{2009}' | '\u{2007}')
                || (hardened && matches!(c, '\u{2006}' | '\u{2008}' | '\u{200A}'));
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

/// `(?<![A-Za-z0-9])[O0]-[O0](-[O0])?(?![A-Za-z0-9])` → `O-O` or `O-O-O`. The trailing lookahead is
/// folded into the pattern, so JS's backtracking order is kept (`0-0-0x` gives `O-O-0x`).
fn castling(s: &str) -> String {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| re(r"(?P<m>[O0]-[O0](?P<q>-[O0])?)(?:[^A-Za-z0-9]|$)"));
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

/// Normalises text before extraction with the hardened pipeline ([`NormMode::Hardened`]).
pub fn normalize_text(s: &str) -> String {
    normalize_with(s, NormMode::Hardened)
}

/// Normalises text before extraction. Both modes run TS's steps with that mode's sets:
/// 1. strip invisibles;
/// 2. space-grouped thousands become commas, before NFKC erases the space kind (H6 widens the set);
/// 3. NFKC, then strip invisibles again;
/// 4. hardened only: H3 drops combining marks (NFKD, drop `\p{Mn}`, NFC), then H1 folds `\p{Nd}`
///    to ASCII and H4 folds homoglyphs;
/// 5. castling dashes, a minus before a digit, figurines, `0-0` castling (H5 widens the dashes).
pub fn normalize_with(s: &str, mode: NormMode) -> String {
    let t = group_thousands(&strip_invisibles_with(s, mode), mode);
    let mut t = strip_invisibles_with(&t.nfkc().collect::<String>(), mode);
    if mode == NormMode::Hardened {
        let d = MN
            .replace_all(&t.nfkd().collect::<String>(), "")
            .nfc()
            .collect::<String>();
        t = d
            .chars()
            .map(|c| {
                if c.is_ascii() {
                    c
                } else {
                    ascii_digit(c).or_else(|| homoglyph(c)).unwrap_or(c)
                }
            })
            .collect();
    }
    let castle = |c: Option<char>| matches!(c, Some('O' | 'o' | '0'));
    let dash = |c: char| is_dash_with(c, mode);
    let t = map_dashes(&t, |p, c, n| castle(p) && dash(c) && castle(n));
    let t = map_dashes(&t, |_, c, n| {
        dash(c) && n.is_some_and(|n| n.is_ascii_digit())
    });
    let t: String = t.chars().map(|c| figurine_letter(c).unwrap_or(c)).collect();
    castling(&t)
}
