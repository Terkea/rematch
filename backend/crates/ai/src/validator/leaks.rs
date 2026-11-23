//! Deliberately aggressive leak detector (ADR-AI-8). It runs on token-stripped text; anything it
//! flags is a chess fact, number or date written as free text. False positives are acceptable (the
//! model can repair by using a reference token); false negatives are not.
//!
//! A port of m3's `validator/leaks.ts` ([`NormMode::TsExact`]: TS's canonical text, `CONFUSABLES`
//! table, `DASH` class and detectors, operation for operation), united with a hardened path
//! ([`NormMode::Hardened`]) that reuses the T5 hardening (ADR 0058, Task 6):
//! - the hardened invisible set (H2), and combining marks dropped (H3);
//! - every [`HOMOGLYPHS`] entry folded to ASCII (H4, a superset of `CONFUSABLES`);
//! - the hardened dash set (H5) in the SAN and castling classes.
//!
//! Folding can glue a neighbour onto a leak (`fιveé` would read `fivee`), so the hardened path is
//! origin-aware: a char folded from non-ASCII is written in ASCII **uppercase**, while every
//! original ASCII letter is lowercased. The detectors match case-insensitively, but their
//! boundaries (`[a-z]`, `[a-z0-9]`) stay case-sensitive, so a folded neighbour never counts as a
//! letter. [`find_leaks`] returns the TS path's leaks unchanged, then each hardened leak (its text
//! lowercased) not already reported, deduplicated by `(kind, text)`. It can never lose a TS leak.
//!
//! Every lookaround uses a form of the ADR 0058 table, named at the pattern.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use super::bounded::{JS_SPACE, m_end, prev_char, scan};
use super::normalize::{DASH_RANGES, NormMode, TS_DASHES, homoglyph, strip_invisibles_with};

#[cfg(test)]
use super::normalize::HOMOGLYPHS;

/// What a leak looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeakKind {
    Digit,
    Percent,
    NumberWord,
    Square,
    San,
    Castling,
    Figurine,
    Date,
}

impl LeakKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LeakKind::Digit => "digit",
            LeakKind::Percent => "percent",
            LeakKind::NumberWord => "number_word",
            LeakKind::Square => "square",
            LeakKind::San => "san",
            LeakKind::Castling => "castling",
            LeakKind::Figurine => "figurine",
            LeakKind::Date => "date",
        }
    }
}

/// One flagged piece of free text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Leak {
    pub kind: LeakKind,
    pub text: String,
}

/// TS's `CONFUSABLES`: Cyrillic and Greek lookalikes folded to Latin, only on the TS path.
const CONFUSABLES: [(char, char); 30] = [
    ('\u{430}', 'a'),
    ('\u{435}', 'e'),
    ('\u{43E}', 'o'),
    ('\u{440}', 'p'),
    ('\u{441}', 'c'),
    ('\u{445}', 'x'),
    ('\u{456}', 'i'),
    ('\u{455}', 's'),
    ('\u{4BB}', 'h'),
    ('\u{432}', 'b'),
    ('\u{3BF}', 'o'),
    ('\u{3B1}', 'a'),
    ('\u{3B5}', 'e'),
    ('\u{3C1}', 'p'),
    ('\u{391}', 'A'),
    ('\u{392}', 'B'),
    ('\u{395}', 'E'),
    ('\u{39A}', 'K'),
    ('\u{39D}', 'N'),
    ('\u{39F}', 'O'),
    ('\u{3A1}', 'P'),
    ('\u{410}', 'A'),
    ('\u{412}', 'B'),
    ('\u{415}', 'E'),
    ('\u{41A}', 'K'),
    ('\u{41D}', 'H'),
    ('\u{41E}', 'O'),
    ('\u{420}', 'P'),
    ('\u{421}', 'C'),
    ('\u{425}', 'X'),
];

/// TS's `NUMBER_WORDS`, in TS's order; [`number_words`] sorts them by length as TS does.
const NUMBER_WORDS: [&str; 76] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fourty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
    "dozen",
    "couple",
    "pair",
    "once",
    "twice",
    "thrice",
    "double",
    "doubled",
    "triple",
    "tripled",
    "quadruple",
    "half",
    "halves",
    "third",
    "quarter",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "eleventh",
    "twelfth",
    "thirteenth",
    "fourteenth",
    "fifteenth",
    "sixteenth",
    "seventeenth",
    "eighteenth",
    "nineteenth",
    "twentieth",
    "thirtieth",
    "fortieth",
    "fiftieth",
    "sixtieth",
    "seventieth",
    "eightieth",
    "ninetieth",
    "hundredth",
    "thousandth",
    "first",
    "second",
];
/// TS appends these two after `second`.
const PERCENT_WORDS: [&str; 2] = ["percent", "percentage"];

/// The number words sorted by length, longest first (a stable sort, like V8's).
fn number_words() -> String {
    let mut w: Vec<&str> = NUMBER_WORDS.iter().chain(&PERCENT_WORDS).copied().collect();
    w.sort_by_key(|s| std::cmp::Reverse(s.len()));
    w.join("|")
}

const MONTH: &str = "(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sept?(?:ember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)";

/// The dash class body: TS's `DASH` (`-` and the first [`TS_DASHES`] ranges), or the hardened
/// [`DASH_RANGES`] (H5).
fn dash(mode: NormMode) -> String {
    let ranges = match mode {
        NormMode::TsExact => &DASH_RANGES[..TS_DASHES],
        NormMode::Hardened => &DASH_RANGES[..],
    };
    let mut out = String::from("\\-");
    for (a, b) in ranges {
        out.push_str(&format!("\\x{{{:X}}}", u32::from(*a)));
        if a != b {
            out.push_str(&format!("-\\x{{{:X}}}", u32::from(*b)));
        }
    }
    out
}

/// Which text a detector reads: the canonical text in its own case, or lowercased.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Src {
    Cased,
    Lower,
}

/// How a detector checks the chars around a candidate.
#[derive(Clone, Copy)]
enum Bound {
    /// No lookarounds.
    None,
    /// A whole-pattern lookbehind `(?<![a-z])` (`alnum`: `[a-z0-9]`), plus a folded trailing
    /// lookahead in group `m`.
    Around { alnum: bool },
    /// No lookbehind; a folded trailing lookahead in group `m`.
    After,
    /// The date alternation: alternatives `a` and `c` carry their own `(?<![a-z])`.
    Date,
}

struct Detector {
    kind: LeakKind,
    re: Regex,
    src: Src,
    bound: Bound,
}

fn detectors(mode: NormMode) -> Vec<Detector> {
    let s = format!("[{JS_SPACE}]");
    let d = dash(mode);
    // The hardened text writes folded chars in uppercase: the bodies match case-insensitively and
    // the boundary classes stay case-sensitive.
    let ci = if mode == NormMode::Hardened {
        "(?i)"
    } else {
        ""
    };
    // Folded lookahead (ADR 0058, third row): `(?P<m>X)(?:[^C]|$)`.
    let not_az = "(?:(?-i:[^a-z])|$)";
    let not_az09 = "(?:(?-i:[^a-z0-9])|$)";
    let words = number_words();
    let det = |kind, p: String, src, bound| Detector {
        kind,
        re: Regex::new(&format!("{ci}{p}")).expect("valid detector"),
        src,
        bound,
    };
    vec![
        det(LeakKind::Digit, r"\p{N}+".into(), Src::Cased, Bound::None),
        det(
            LeakKind::Percent,
            "[%\u{2030}\u{66A}]".into(),
            Src::Cased,
            Bound::None,
        ),
        // `(?<![a-z])(?:…)(?![a-z])`: lookbehind as `before_ok`, lookahead folded.
        det(
            LeakKind::NumberWord,
            format!(
                "(?P<m>(?:{words})(?:s|es)?|(?:twen|thir|for|fif|six|seven|eigh|nine)ties|per{s}+cent){not_az}"
            ),
            Src::Lower,
            Bound::Around { alnum: false },
        ),
        det(
            LeakKind::Square,
            "[a-h][1-8]".into(),
            Src::Lower,
            Bound::None,
        ),
        det(
            LeakKind::San,
            format!("[a-h][1-8][x{d}][a-h][1-8]"),
            Src::Lower,
            Bound::None,
        ),
        // `[a-h][18]\s*[=(]?\s*[qrbn]\)?(?![a-z])`: lookahead folded.
        det(
            LeakKind::San,
            format!(r"(?P<m>[a-h][18]{s}*[=(]?{s}*[qrbn]\)?){not_az}"),
            Src::Lower,
            Bound::After,
        ),
        // `(?<![a-z])e\.?\s?p\.(?![a-z])`.
        det(
            LeakKind::San,
            format!(r"(?P<m>e\.?{s}?p\.){not_az}"),
            Src::Lower,
            Bound::Around { alnum: false },
        ),
        // `(?<![a-z0-9])[o0]\s*[DASH]+\s*[o0](?:\s*[DASH]+\s*[o0])?(?![a-z0-9])`.
        det(
            LeakKind::Castling,
            format!("(?P<m>[o0]{s}*[{d}]+{s}*[o0](?:{s}*[{d}]+{s}*[o0])?){not_az09}"),
            Src::Lower,
            Bound::Around { alnum: true },
        ),
        det(
            LeakKind::Figurine,
            r"[\x{2654}-\x{265F}\x{1FA00}-\x{1FA6F}]".into(),
            Src::Cased,
            Bound::None,
        ),
        // Alternatives `a`, `t` and `c` have a lookbehind (checked per alternative); `t`, `b` and
        // `c` a folded lookahead. JS `\d` and `\w` are ASCII. TS's `the\b` (after `e`, a word
        // char) is the folded lookahead `(?:(?-i:[^a-z0-9_])|$)` outside group `t`, not
        // `(?-u:\b)`: in the hardened text a folded char is an uppercase ASCII marker and must
        // count as a non-word char (T6 review probe `jaո theé`). TS's `(?:[0-9]|the\b)` is split
        // into `a` and `t`; the char after the greedy `\s+` decides between them, so
        // leftmost-first gives the same match.
        det(
            LeakKind::Date,
            format!(
                "(?P<a>{MONTH}\\.?{s}+[0-9])\
                 |(?P<t>{MONTH}\\.?{s}+the)(?:(?-i:[^a-z0-9_])|$)\
                 |(?P<b>[0-9]{{1,2}}(?:st|nd|rd|th)?{s}+(?:of{s}+)?{MONTH}){not_az}\
                 |(?P<c>[A-Za-z0-9_]+{s}+of{s}+{MONTH}){not_az}\
                 |(?P<d>[0-9]{{1,2}}{s}*[/.]{s}*[0-9]{{1,2}})\
                 |(?P<e>[0-9]{{4}}{s}*-{s}*[0-9]{{2}}{s}*-{s}*[0-9]{{2}})"
            ),
            Src::Lower,
            Bound::Date,
        ),
    ]
}

static TS_DETECTORS: LazyLock<Vec<Detector>> = LazyLock::new(|| detectors(NormMode::TsExact));
static HARDENED_DETECTORS: LazyLock<Vec<Detector>> =
    LazyLock::new(|| detectors(NormMode::Hardened));
static MN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\p{Mn}$").expect("valid regex"));

fn is_mn(c: char) -> bool {
    let mut buf = [0u8; 4];
    !c.is_ascii() && MN.is_match(c.encode_utf8(&mut buf))
}

fn nfkc_stripped(s: &str, mode: NormMode) -> String {
    let t: String = strip_invisibles_with(s, mode).nfkc().collect();
    strip_invisibles_with(&t, mode)
}

/// TS's `canonical`: invisibles, NFKC, invisibles, then `CONFUSABLES` on `[Α-ωЀ-ӿ]`.
fn canonical_ts(s: &str) -> String {
    nfkc_stripped(s, NormMode::TsExact)
        .chars()
        .map(|c| {
            if ('\u{391}'..='\u{3C9}').contains(&c) || ('\u{400}'..='\u{4FF}').contains(&c) {
                CONFUSABLES
                    .iter()
                    .find(|(k, _)| *k == c)
                    .map_or(c, |(_, v)| *v)
            } else {
                c
            }
        })
        .collect()
}

/// The hardened, origin-aware canonical text: H2 invisibles, NFKC, then per char: ASCII is
/// lowercased; a lone combining mark is dropped (H3); a homoglyph (H4), or a char whose canonical
/// decomposition without marks folds to ASCII (`é`, `ẃ`, `ё`), becomes that ASCII in uppercase;
/// anything else is lowercased.
fn canonical_hardened(s: &str) -> String {
    let t = nfkc_stripped(s, NormMode::Hardened);
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
        } else if is_mn(c) {
        } else if let Some(a) = homoglyph(c) {
            out.push(a.to_ascii_uppercase());
        } else {
            let d: String = c
                .nfkd()
                .filter(|x| !is_mn(*x))
                .map(|x| homoglyph(x).unwrap_or(x))
                .collect();
            if !d.is_empty() && d.is_ascii() {
                out.push_str(&d.to_ascii_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
        }
    }
    out
}

fn run(d: &Detector, text: &str) -> Vec<String> {
    let not_az = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_lowercase());
    let not_az09 =
        |c: Option<char>| !c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let whole = |c: &Captures<'_>| {
        let m = c.get(0)?;
        Some((m.end(), m.as_str().to_owned()))
    };
    let logical = |c: &Captures<'_>| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        Some((end, text[start..end].to_owned()))
    };
    match d.bound {
        Bound::None => scan(&d.re, text, |_| true, whole),
        Bound::After => scan(&d.re, text, |_| true, logical),
        Bound::Around { alnum: false } => scan(&d.re, text, not_az, logical),
        Bound::Around { alnum: true } => scan(&d.re, text, not_az09, logical),
        Bound::Date => scan(
            &d.re,
            text,
            |_| true,
            |c| {
                let (name, g) = ["a", "t", "b", "c", "d", "e"]
                    .into_iter()
                    .find_map(|n| c.name(n).map(|g| (n, g)))?;
                // A rejected `a`, `t` or `c` cannot leave another alternative at the same start:
                // `b`, `d` and `e` need a digit where `a` and `t` need a letter, `a` and `t` share
                // the lookbehind, and `d`/`e` need `/`, `.` or `-` where `c` needs spaces and
                // "of". So retrying at start + 1 is exact.
                if matches!(name, "a" | "t" | "c") && !not_az(prev_char(text, g.start())) {
                    return None;
                }
                Some((g.end(), g.as_str().to_owned()))
            },
        ),
    }
}

fn detect(detectors: &[Detector], cased: &str, lower: &str, fold: bool) -> Vec<Leak> {
    let mut out = Vec::new();
    for d in detectors {
        let text = match d.src {
            Src::Cased => cased,
            Src::Lower => lower,
        };
        for t in run(d, text) {
            let text = if fold && d.src == Src::Lower {
                t.to_ascii_lowercase()
            } else {
                t
            };
            out.push(Leak { kind: d.kind, text });
        }
    }
    out
}

fn dedup(leaks: impl IntoIterator<Item = Leak>) -> Vec<Leak> {
    let mut seen = HashSet::new();
    leaks
        .into_iter()
        .filter(|l| seen.insert((l.kind, l.text.clone())))
        .collect()
}

/// One path of the detector. [`NormMode::TsExact`] is TS's `findLeaks` exactly; the hardened path
/// alone is not a superset of TS (use [`find_leaks`]).
pub fn find_leaks_with(stripped: &str, mode: NormMode) -> Vec<Leak> {
    match mode {
        NormMode::TsExact => {
            let c = canonical_ts(stripped);
            let lc = c.to_lowercase();
            dedup(detect(&TS_DETECTORS, &c, &lc, false))
        }
        NormMode::Hardened => {
            let h = canonical_hardened(stripped);
            dedup(detect(&HARDENED_DETECTORS, &h, &h, true))
        }
    }
}

/// Flags every free-text chess fact, number or date in token-stripped text: TS's leaks in TS's
/// detector order, then the hardened path's extras, deduplicated by `(kind, text)`.
pub fn find_leaks(stripped: &str) -> Vec<Leak> {
    let ts = find_leaks_with(stripped, NormMode::TsExact);
    let hardened = find_leaks_with(stripped, NormMode::Hardened);
    dedup(ts.into_iter().chain(hardened))
}

/// Whether a leak is an ordinal used as discourse, not a numeric fact (R2-1 (b)): "first"
/// anywhere ("First, …", "your first move", "at first"), and "second" or "third" when every use
/// of it in `stripped` opens a sentence and is followed by a comma ("Second, …"). A counted claim
/// ("first 10 games", "first ten games") keeps its own digit or number-word leak, and "a third
/// of your games" or "your second loss" stays a leak. [`find_leaks`] itself is unchanged (TS
/// parity); the claim validator applies this filter.
pub fn is_discourse_ordinal(stripped: &str, leak: &Leak) -> bool {
    if leak.kind != LeakKind::NumberWord {
        return false;
    }
    match leak.text.as_str() {
        "first" => true,
        w @ ("second" | "third") => {
            let lower = stripped.to_ascii_lowercase();
            let mut any = false;
            for (i, _) in lower.match_indices(w) {
                let before = lower[..i].trim_end_matches([' ', '\t']);
                let after = &lower[i + w.len()..];
                let opens = before.is_empty()
                    || before.ends_with(['.', '!', '?', ';', ':', '\n']);
                if !opens || !after.starts_with(',') {
                    return false;
                }
                any = true;
            }
            any
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::bounded::take_scanned;
    use super::*;

    #[test]
    fn confusables_are_a_subset_of_homoglyphs() {
        for (k, v) in CONFUSABLES {
            assert_eq!(homoglyph(k), Some(v), "{k}");
        }
        assert!(HOMOGLYPHS.len() > CONFUSABLES.len());
    }

    #[test]
    fn number_words_sort_longest_first_and_stably() {
        let w = number_words();
        let parts: Vec<&str> = w.split('|').collect();
        assert_eq!(parts.len(), 78);
        assert!(parts.windows(2).all(|p| p[0].len() >= p[1].len()));
        assert_eq!(&parts[..3], ["seventeenth", "thirteenth", "fourteenth"]);
    }

    #[test]
    fn hardened_canonical_marks_folded_chars_in_uppercase() {
        assert_eq!(
            canonical_hardened("F\u{3b9}ve\u{e9} N\u{180f}f3 \u{555}-O"),
            "fIveE nf3 O-o"
        );
        assert_eq!(canonical_hardened("tw\u{301}enty"), "tWenty");
    }

    type Shape = fn(usize) -> String;

    /// Hostile shapes for every detector: long rejected runs, long candidate words, and long
    /// space runs inside the `\s*` / `\s+` parts.
    const SHAPES: &[(&str, Shape)] = &[
        ("letters", |n| "a".repeat(n)),
        ("glued words", |n| "xten".repeat(n / 4)),
        ("word then letters", |n| format!("ten{}", "a".repeat(n))),
        ("of runs", |n| format!("x{} of", "a".repeat(n))),
        ("of month runs", |n| "ab of janx ".repeat(n / 11)),
        ("month spaces", |n| format!("jan{}x", " ".repeat(n))),
        ("day spaces", |n| format!("14{}x", " ".repeat(n))),
        ("slash spaces", |n| format!("1{}x", " ".repeat(n))),
        ("iso spaces", |n| format!("2026{}-", " ".repeat(n))),
        ("castling spaces", |n| {
            format!("o{}-{}x", " ".repeat(n / 2), " ".repeat(n / 2))
        }),
        ("castling chain", |n| "o-".repeat(n / 2)),
        ("castling glued", |n| "o-oo".repeat(n / 4)),
        ("promotion spaces", |n| format!("e8{}=x", " ".repeat(n))),
        ("e.p. runs", |n| "e.p.x".repeat(n / 5)),
        ("per spaces", |n| format!("per{}x", " ".repeat(n))),
        ("folded letters", |n| "\u{3b9}".repeat(n / 2)),
        ("folded glue", |n| "f\u{3b9}vex".repeat(n / 6)),
        ("digits", |n| "1".repeat(n)),
        ("dates", |n| "14 oct ".repeat(n / 7)),
        ("month words", |n| "january ".repeat(n / 8)),
        ("stray braces", |n| "{".repeat(n)),
        ("open tokens", |n| "{{sq:e4".repeat(n / 7)),
        ("tokens", |n| "{{sq:e4}}".repeat(n / 9)),
    ];

    #[test]
    fn scanned_bytes_are_linear_on_hostile_shapes() {
        let n = 32 * 1024;
        let mut worst = (0, "");
        for (name, shape) in SHAPES {
            let s = shape(n);
            take_scanned();
            let p = super::super::tokens::parse_tokens(&s);
            let _ = find_leaks(&p.stripped);
            let _ = find_leaks(&s);
            let used = take_scanned();
            let per = used / s.len().max(1);
            if per > worst.0 {
                worst = (per, name);
            }
            assert!(
                used <= 80 * s.len().max(1),
                "{name}: {used} bytes for {}",
                s.len()
            );
        }
        eprintln!("worst: {}·n on {}", worst.0, worst.1);
    }
}
