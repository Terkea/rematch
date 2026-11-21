//! Text normalisation applied before any extraction, so notation tricks cannot smuggle moves or
//! numbers. A port of m3's `validator/normalize.ts`, operation for operation, plus the deliberate
//! hardening of ADR-R3-2 (fix round 1): a missed claim is a leak, so this is stricter than TS.
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

/// TS's `INVISIBLES` plus (H2) every `Default_Ignorable_Code_Point`.
static INVISIBLES: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"[\x{AD}\x{34F}\x{61C}\x{115F}\x{1160}\x{17B4}\x{17B5}\x{180B}-\x{180E}\x{200B}-\x{200F}",
        r"\x{202A}-\x{202E}\x{2060}-\x{2069}\x{3164}\x{FE00}-\x{FE0F}\x{FEFF}\x{FFA0}",
        r"\x{E0000}-\x{E007F}\x{E0100}-\x{E01EF}\p{Default_Ignorable_Code_Point}]+"
    ))
});
static MN: LazyLock<Regex> = LazyLock::new(|| re(r"\p{Mn}+"));
static ND: LazyLock<Regex> = LazyLock::new(|| re(r"\A\p{Nd}\z"));

fn char_matches(r: &Regex, c: char) -> bool {
    let mut buf = [0u8; 4];
    r.is_match(c.encode_utf8(&mut buf))
}

/// True for an invisible code point: TS's `INVISIBLES` (zero-width, bidi, joiners, variation
/// selectors, fillers, tags) and every other `Default_Ignorable_Code_Point` (H2).
pub fn is_invisible(c: char) -> bool {
    !c.is_ascii() && char_matches(&INVISIBLES, c)
}

/// Removes every [`is_invisible`] code point.
pub fn strip_invisibles(s: &str) -> String {
    INVISIBLES.replace_all(s, "").into_owned()
}

/// The dash set: TS's `DASH` (`\u{2010}-\u{2015}\u{2212}\u{2796}\u{FE58}\u{FE63}\u{FF0D}`) plus
/// U+02D7, U+2E3A, U+2E3B, U+058A and U+30FC (H5). One table for [`is_dash`] and [`dash_class`].
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

/// True for a member of [`DASH_RANGES`] (not the ASCII hyphen).
pub fn is_dash(c: char) -> bool {
    DASH_RANGES.iter().any(|(a, b)| (*a..=*b).contains(&c))
}

/// [`DASH_RANGES`] as the body of a regex character class.
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

/// `\p{Nd}`, as JS's `u`-flag class.
fn is_nd(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_digit();
    }
    char_matches(&ND, c)
}

/// H1: the ASCII digit of a `\p{Nd}` char. Unicode keeps decimal digits in contiguous runs of ten,
/// from 0 to 9, so the value is the number of `Nd` code points before it in its run, modulo 10.
fn ascii_digit(c: char) -> Option<char> {
    if c.is_ascii() || !is_nd(c) {
        return None;
    }
    let mut before = 0u32;
    let mut cp = u32::from(c);
    // The longest run (mathematical digits) is 50 code points long.
    while before < 64
        && let Some(p) = cp.checked_sub(1).and_then(char::from_u32)
        && is_nd(p)
    {
        before += 1;
        cp -= 1;
    }
    char::from_digit(before % 10, 10)
}

/// `(?<=\p{Nd})[\u{A0}\u{202F}\u{2009}\u{2007}](?=\p{Nd}{3}(?!\p{Nd}))` → `,`, with U+2006, U+2008
/// and U+200A added (H6).
fn group_thousands(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let nd = |i: usize| chars.get(i).is_some_and(|c| is_nd(*c));
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let space = matches!(
                c,
                '\u{A0}'
                    | '\u{202F}'
                    | '\u{2009}'
                    | '\u{2007}'
                    | '\u{2006}'
                    | '\u{2008}'
                    | '\u{200A}'
            );
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

/// Normalises text before extraction:
/// 1. strip invisibles;
/// 2. space-grouped thousands become commas, before NFKC erases the space kind (H6 widens the set);
/// 3. NFKC, then strip invisibles again;
/// 4. H3: drop combining marks (NFKD, drop `\p{Mn}`, NFC);
/// 5. H1: fold `\p{Nd}` to ASCII digits;
/// 6. H4: fold homoglyphs;
/// 7. then, in TS's order: castling dashes, a minus before a digit, figurines, `0-0` castling.
pub fn normalize_text(s: &str) -> String {
    let t = group_thousands(&strip_invisibles(s));
    let t = strip_invisibles(&t.nfkc().collect::<String>());
    let t = MN
        .replace_all(&t.nfkd().collect::<String>(), "")
        .nfc()
        .collect::<String>();
    let t: String = t
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c
            } else {
                ascii_digit(c).or_else(|| homoglyph(c)).unwrap_or(c)
            }
        })
        .collect();
    let castle = |c: Option<char>| matches!(c, Some('O' | 'o' | '0'));
    let t = map_dashes(&t, |p, c, n| castle(p) && is_dash(c) && castle(n));
    let t = map_dashes(&t, |_, c, n| {
        is_dash(c) && n.is_some_and(|n| n.is_ascii_digit())
    });
    let t: String = t.chars().map(|c| figurine_letter(c).unwrap_or(c)).collect();
    castling(&t)
}
