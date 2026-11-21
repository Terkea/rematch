//! Free-text extractors for moves, numbers and dates. A port of m3's `validator/extract.ts`,
//! operation for operation. Regex lookarounds follow ADR-R3-2 deviation 2 (see [`super::bounded`]);
//! JS `\d` is `[0-9]`, JS `\s` is [`JS_SPACE`], JS `\b` is `(?-u:\b)`, and case-insensitive parts
//! are scoped `(?i:…)` groups over ASCII words (NFKC has already folded U+212A and U+017F, the
//! only non-ASCII chars Unicode case folding would add).
//!
//! In R3 the model never writes free-text facts (ADR-AI-8): these extractors mine evidence strings
//! and run as defence in depth after the leak check (deviation 4).

use std::sync::LazyLock;

use regex::{Captures, NoExpand, Regex};

use std::collections::HashMap;
use std::hash::Hash;

use super::bounded::{
    JS_SPACE, PH, Utf16Cursor, count_scanned, is_js_space, m_end, mask, next_char, prev_char, scan,
    splice, window_before,
};
use super::normalize::{NormMode, dash_class, normalize_with};

/// The union of the TS-exact and the hardened results (ADR-R3-2, fix round 2): the TS-path list
/// unchanged, then each hardened result that the TS path did not also find (multiset difference,
/// keyed by `key`), in hardened order. Linear in the number of results.
fn union_by<T, K: Hash + Eq>(ts: Vec<T>, hardened: Vec<T>, key: impl Fn(&T) -> K) -> Vec<T> {
    let mut left: HashMap<K, usize> = HashMap::new();
    for t in &ts {
        *left.entry(key(t)).or_default() += 1;
    }
    let mut out = ts;
    for h in hardened {
        match left.get_mut(&key(&h)) {
            Some(n) if *n > 0 => *n -= 1,
            _ => out.push(h),
        }
    }
    out
}

/// The kind of an extracted number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberKind {
    Plain,
    Ratio,
}

/// A number found in free text.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedNumber {
    pub value: f64,
    pub decimals: u32,
    pub percent: bool,
    pub hedged: bool,
    pub kind: NumberKind,
    /// True for forms such as "1,5" that could be a decimal comma or a list; the validator must
    /// treat it as unverifiable.
    pub ambiguous: bool,
}

/// A date found in free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtractedDate {
    pub year: Option<i32>,
    pub month: u32,
    pub day: u32,
}

/// Moves found in free text: SAN and UCI-like (long algebraic is read as UCI).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractedMoves {
    pub san: Vec<String>,
    pub uci: Vec<String>,
}

const SQ: &str = "[a-h][1-8]";
/// The folded `(?![A-Za-z0-9])` and `(?![A-Za-z])`.
const NA: &str = "(?:[^A-Za-z0-9]|$)";
const NA_ALPHA: &str = "(?:[^A-Za-z]|$)";

fn not_alnum(c: Option<char>) -> bool {
    !c.is_some_and(|c| c.is_ascii_alphanumeric())
}

fn not_alpha(c: Option<char>) -> bool {
    !c.is_some_and(|c| c.is_ascii_alphabetic())
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid regex")
}

macro_rules! lazy_re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| re(&$pat));
    };
}

fn s_class() -> String {
    format!("[{JS_SPACE}]")
}

// ---- moves ----

// Long algebraic: e2-e4, Ng1-f3, e2xd5, Qd1–h5, e7-e8=Q. The TS-exact path keeps TS's
// separator class `[-\u{2010}-\u{2015}\u{2212}x]`; the hardened path uses the shared dash set
// (H5, ADR-R3-2).
lazy_re!(
    LONG_RE_TS,
    format!(
        "(?P<m>[KQRBN]?(?P<from>{SQ})[-\u{2010}-\u{2015}\u{2212}x](?P<to>{SQ})(?:=?\\(?(?P<p>[QRBNqrbn])\\)?)?){NA}"
    )
);
lazy_re!(
    LONG_RE,
    format!(
        "(?P<m>[KQRBN]?(?P<from>{SQ})[-{dashes}x](?P<to>{SQ})(?:=?\\(?(?P<p>[QRBNqrbn])\\)?)?){NA}",
        dashes = dash_class()
    )
);
lazy_re!(
    UCI_RE,
    format!("(?P<m>(?P<from>{SQ})(?P<to>{SQ})(?P<p>[qrbn])?){NA}")
);
lazy_re!(
    SAN_RE,
    format!(
        "(?P<m>(?:O-O-O|O-O)|[KQRBN](?:{SQ}|[a-h]|[1-8])?x?{SQ}|[a-h]x{SQ}(?:=?\\(?[QRBN]\\)?)?|[a-h][18]=?\\(?[QRBN]\\)?|(?P<bare>{SQ})){NA}"
    )
);
lazy_re!(
    MOVE_NO_BEFORE,
    format!(
        "(?:[0-9]+{s}*\\.(?:{s}*\\.{{1,3}})?|\\.{{3}}){s}*$",
        s = s_class()
    )
);
lazy_re!(
    PLAY_BEFORE,
    format!(
        "(?-u:\\b)(?i:play|played|playing|push|pushed|advance|advanced){s}+(?:(?i:to){s}+)?$",
        s = s_class()
    )
);
/// A member of TS's `GAP_AFTER_MOVE` class, `[\s().,;:\-+#!?…]`.
fn is_gap(c: char) -> bool {
    is_js_space(c)
        || matches!(
            c,
            '(' | ')' | '.' | ',' | ';' | ':' | '-' | '+' | '#' | '!' | '?' | '\u{2026}'
        )
}
lazy_re!(GAP_BEFORE_MOVE, format!("^{}*$", s_class()));
lazy_re!(PROMO_SAN, "^([a-h](?:x[a-h])?[18])=?([QRBN])$");

struct Hit {
    at: usize,
    text: String,
}

fn normalise_san_text(s: &str) -> String {
    let stripped: String = s.chars().filter(|c| !matches!(c, '(' | ')')).collect();
    match PROMO_SAN.captures(&stripped) {
        Some(c) => format!("{}={}", &c[1], &c[2]),
        None => stripped,
    }
}

/// One long-algebraic or UCI pass: the hits and the text with each hit masked.
fn uci_pass(t: &str, re: &Regex, lower: bool) -> (Vec<Hit>, String) {
    let mut cur = Utf16Cursor::new(t);
    let found = scan(re, t, not_alnum, |c| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        let p = c.name("p").map_or(String::new(), |p| {
            if lower {
                p.as_str().to_lowercase()
            } else {
                p.as_str().to_owned()
            }
        });
        let text = format!("{}{}{p}", &c["from"], &c["to"]);
        Some((end, (start, end, text)))
    });
    let ranges: Vec<(usize, usize)> = found.iter().map(|(a, b, _)| (*a, *b)).collect();
    let hits = found
        .into_iter()
        .map(|(a, _, text)| Hit {
            at: cur.at(a),
            text,
        })
        .collect();
    (hits, mask(t, &ranges))
}

/// Finds SAN and UCI-like moves and returns text with each hit masked by a placeholder.
fn scan_moves(t: &str, mode: NormMode) -> (Vec<Hit>, Vec<Hit>, String) {
    let long: &Regex = match mode {
        NormMode::TsExact => &LONG_RE_TS,
        NormMode::Hardened => &LONG_RE,
    };
    let (mut uci, work) = uci_pass(t, long, true);
    let (uci2, work) = uci_pass(&work, &UCI_RE, false);
    uci.extend(uci2);

    struct Cand {
        start: usize,
        end: usize,
        bare: bool,
    }
    let cands = scan(&SAN_RE, &work, not_alnum, |c| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        Some((
            end,
            Cand {
                start,
                end,
                bare: c.name("bare").is_some(),
            },
        ))
    });
    let mut cur = Utf16Cursor::new(&work);
    let mut san = Vec::new();
    let mut ranges = Vec::new();
    let mut last_end: Option<usize> = None;
    // TS tests `GAP_AFTER_MOVE` (`^[gap]*$`) on `work.slice(lastEnd, c.at)`, which rescans the
    // gap for every rejected bare square: quadratic. Instead, walk `work` once and keep the
    // position of the last non-gap char before the cursor. The slice is all gap iff that
    // position is before `last_end`.
    let mut cursor = 0;
    let mut last_non_gap: Option<usize> = None;
    for (i, c) in cands.iter().enumerate() {
        if c.start > cursor {
            count_scanned(c.start - cursor);
            for (off, ch) in work[cursor..c.start].char_indices() {
                if !is_gap(ch) {
                    last_non_gap = Some(cursor + off);
                }
            }
            cursor = c.start;
        }
        let gap_after_move = last_end.is_some_and(|e| last_non_gap.is_none_or(|p| p < e));
        let accept = !c.bare || {
            let before = window_before(&work, c.start, 16);
            count_scanned(before.len());
            MOVE_NO_BEFORE.is_match(before)
                || PLAY_BEFORE.is_match(before)
                || gap_after_move
                || cands.get(i + 1).is_some_and(|next| {
                    count_scanned(next.start - c.end);
                    !next.bare && GAP_BEFORE_MOVE.is_match(&work[c.end..next.start])
                })
        };
        if accept {
            san.push(Hit {
                at: cur.at(c.start),
                text: normalise_san_text(&work[c.start..c.end]),
            });
            ranges.push((c.start, c.end));
            last_end = Some(c.end);
        }
    }
    let masked = mask(&work, &ranges);
    (san, uci, masked)
}

/// SAN and UCI-like moves under one normalisation, each list in text order.
fn moves_with(text: &str, mode: NormMode) -> ExtractedMoves {
    let (mut san, mut uci, _) = scan_moves(&normalize_with(text, mode), mode);
    san.sort_by_key(|h| h.at);
    uci.sort_by_key(|h| h.at);
    ExtractedMoves {
        san: san.into_iter().map(|h| h.text).collect(),
        uci: uci.into_iter().map(|h| h.text).collect(),
    }
}

/// Extracts SAN and UCI-like moves: the TS-exact moves in text order, then any extra moves the
/// hardened normalisation finds (ADR-R3-2 union).
pub fn extract_moves(text: &str) -> ExtractedMoves {
    let ts = moves_with(text, NormMode::TsExact);
    let hard = moves_with(text, NormMode::Hardened);
    ExtractedMoves {
        san: union_by(ts.san, hard.san, String::clone),
        uci: union_by(ts.uci, hard.uci, String::clone),
    }
}

// ---- dates ----

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const MON_SRC: &str = "(?i:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|June?|July?|Aug(?:ust)?|Sept?(?:ember)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)";

lazy_re!(ISO_RE, "([0-9]{4})-([0-9]{2})-([0-9]{2})");
lazy_re!(
    DMY_RE,
    format!(
        "(?P<d>[0-9]{{1,2}})(?i:st|nd|rd|th)?{s}+(?P<mon>{MON_SRC})(?-u:\\b)\\.?",
        s = s_class()
    )
);
lazy_re!(
    MDY_RE,
    format!("(?P<mon>{MON_SRC})(?-u:\\b)\\.?{s}+", s = s_class())
);
lazy_re!(
    YEAR_RE,
    format!("^,?{s}+(?P<y>(?:19|20)[0-9]{{2}})", s = s_class())
);

fn month_index(name: &str) -> Option<u32> {
    if name.to_lowercase() == "may" && name != "May" {
        return None; // "you may 5 times"
    }
    let key = name.get(..3)?.to_lowercase();
    let i = MONTHS.iter().position(|m| *m == key)?;
    u32::try_from(i + 1).ok()
}

fn valid_day(m: u32, d: u32) -> bool {
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// The optional `(?:,?\s+((?:19|20)\d{2})(?![A-Za-z0-9]))?` tail at byte `q`.
fn year_after(t: &str, q: usize) -> Option<(usize, i32)> {
    let c = YEAR_RE.captures(&t[q..])?;
    let y = c.name("y")?;
    let end = q + y.end();
    if !not_alnum(next_char(t, end)) {
        return None;
    }
    Some((end, y.as_str().parse().ok()?))
}

/// The `(\d{1,2})(?:st|nd|rd|th)?(?![\d:])` part of MDY at byte `q`, in backtracking order.
fn mdy_day(t: &str, q: usize) -> Option<(usize, u32)> {
    let run = t[q..].bytes().take_while(u8::is_ascii_digit).count();
    let blocked = |at: usize| next_char(t, at).is_some_and(|c| c.is_ascii_digit() || c == ':');
    for dl in (1..=run.min(2)).rev() {
        let p = q + dl;
        let day: u32 = t[q..p].parse().ok()?;
        let suffix = t
            .get(p..p + 2)
            .is_some_and(|s| matches!(s.to_ascii_lowercase().as_str(), "st" | "nd" | "rd" | "th"));
        if suffix && !blocked(p + 2) {
            return Some((p + 2, day));
        }
        if !blocked(p) {
            return Some((p, day));
        }
    }
    None
}

/// One date pass: each structural match is consumed; only valid dates are recorded and masked.
fn date_pass(
    t: &str,
    re: &Regex,
    before_ok: impl Fn(Option<char>) -> bool,
    parse: impl Fn(&Captures<'_>) -> Option<(usize, Option<ExtractedDate>)>,
    found: &mut Vec<(usize, ExtractedDate)>,
) -> String {
    let hits = scan(re, t, before_ok, |c| {
        let start = c.get(0)?.start();
        let (end, d) = parse(c)?;
        Some((end, (start, end, d)))
    });
    let mut cur = Utf16Cursor::new(t);
    let mut ranges = Vec::new();
    for (a, b, d) in hits {
        if let Some(d) = d {
            found.push((cur.at(a), d));
            ranges.push((a, b));
        }
    }
    mask(t, &ranges)
}

/// Finds dates; returns them and the text with each date replaced by placeholders.
fn scan_dates(t: &str) -> (Vec<ExtractedDate>, String) {
    let mut found = Vec::new();
    let rest = date_pass(
        t,
        &ISO_RE,
        |c| !c.is_some_and(|c| c.is_ascii_digit() || c == '-'),
        |c| {
            let end = c.get(0)?.end();
            if next_char(t, end).is_some_and(|c| c.is_ascii_digit()) {
                return None;
            }
            let (y, mo, d) = (c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?);
            let ok = valid_day(mo, d).then_some(ExtractedDate {
                year: Some(y),
                month: mo,
                day: d,
            });
            Some((end, ok))
        },
        &mut found,
    );
    let r = rest.as_str();
    let rest = date_pass(
        r,
        &DMY_RE,
        not_alnum,
        |c| {
            let main_end = c.get(0)?.end();
            let (end, year) =
                year_after(r, main_end).map_or((main_end, None), |(e, y)| (e, Some(y)));
            let day: u32 = c["d"].parse().ok()?;
            let ok = month_index(&c["mon"])
                .filter(|m| valid_day(*m, day))
                .map(|month| ExtractedDate { year, month, day });
            Some((end, ok))
        },
        &mut found,
    );
    let r = rest.as_str();
    let rest = date_pass(
        r,
        &MDY_RE,
        not_alnum,
        |c| {
            let (day_end, day) = mdy_day(r, c.get(0)?.end())?;
            let (end, year) = year_after(r, day_end).map_or((day_end, None), |(e, y)| (e, Some(y)));
            let ok = month_index(&c["mon"])
                .filter(|m| valid_day(*m, day))
                .map(|month| ExtractedDate { year, month, day });
            Some((end, ok))
        },
        &mut found,
    );
    found.sort_by_key(|f| f.0);
    (found.into_iter().map(|f| f.1).collect(), rest)
}

/// Extracts dates: the TS-exact dates in text order, then any extra dates the hardened
/// normalisation finds (ADR-R3-2 union).
pub fn extract_dates(text: &str) -> Vec<ExtractedDate> {
    let ts = scan_dates(&normalize_with(text, NormMode::TsExact)).0;
    let hard = scan_dates(&normalize_with(text, NormMode::Hardened)).0;
    union_by(ts, hard, |d| *d)
}

// ---- numbers ----

const ONES: [(&str, f64); 20] = [
    ("zero", 0.0),
    ("one", 1.0),
    ("two", 2.0),
    ("three", 3.0),
    ("four", 4.0),
    ("five", 5.0),
    ("six", 6.0),
    ("seven", 7.0),
    ("eight", 8.0),
    ("nine", 9.0),
    ("ten", 10.0),
    ("eleven", 11.0),
    ("twelve", 12.0),
    ("thirteen", 13.0),
    ("fourteen", 14.0),
    ("fifteen", 15.0),
    ("sixteen", 16.0),
    ("seventeen", 17.0),
    ("eighteen", 18.0),
    ("nineteen", 19.0),
];
const TENS: [(&str, f64); 9] = [
    ("twenty", 20.0),
    ("thirty", 30.0),
    ("forty", 40.0),
    ("fourty", 40.0),
    ("fifty", 50.0),
    ("sixty", 60.0),
    ("seventy", 70.0),
    ("eighty", 80.0),
    ("ninety", 90.0),
];
const UNIT_WORDS: [&str; 20] = [
    "game", "move", "piece", "blunder", "mistake", "time", "position", "week", "day", "point",
    "opening", "month", "session", "pawn", "error", "hour", "minute", "second", "year", "percent",
];
const FRACTION_DEN: [(&str, f64); 9] = [
    ("third", 3.0),
    ("quarter", 4.0),
    ("fourth", 4.0),
    ("fifth", 5.0),
    ("sixth", 6.0),
    ("seventh", 7.0),
    ("eighth", 8.0),
    ("ninth", 9.0),
    ("tenth", 10.0),
];

fn lookup(table: &[(&str, f64)], w: &str) -> Option<f64> {
    table.iter().find(|(k, _)| *k == w).map(|(_, v)| *v)
}

fn is_unit(w: &str) -> bool {
    UNIT_WORDS
        .iter()
        .any(|u| w == *u || w.strip_suffix('s') == Some(u))
}

/// TS's `WORD_ALT`: ones, tens, "hundred", "thousand", stably sorted by length, longest first.
fn word_alt() -> String {
    let mut words: Vec<&str> = ONES
        .iter()
        .chain(TENS.iter())
        .map(|(k, _)| *k)
        .chain(["hundred", "thousand"])
        .collect();
    words.sort_by_key(|w| std::cmp::Reverse(w.len()));
    words.join("|")
}

fn ones_alt() -> String {
    ONES.iter().map(|(k, _)| *k).collect::<Vec<_>>().join("|")
}

lazy_re!(CITATION_RE, r"\[\[g:[^\]]*\]\]");
lazy_re!(
    MOVE_NO_PREFIX,
    format!(
        "(?P<m>[0-9]+{s}*\\.(?:{s}*\\.{{1,3}})?{s}*)\u{1}",
        s = s_class()
    )
);
lazy_re!(
    PER_CENT,
    format!("(?-u:\\b)(?i:per){}+(?i:cent)(?-u:\\b)", s_class())
);
lazy_re!(WEAKNESS_ID, format!("(?P<m>W[0-9]{{1,2}}[a-d]?){NA}"));
lazy_re!(BAND_ID, format!("(?P<m>B[0-5]){NA}"));
lazy_re!(
    GAME_RESULT,
    format!("(?P<m>1-0|0-1|1(?:/|\u{2044})2-1(?:/|\u{2044})2){NA}")
);
lazy_re!(ORDINAL, format!("(?P<m>[0-9]+(?i:st|nd|rd|th)){NA_ALPHA}"));
lazy_re!(
    NUM_RE,
    "(?P<sign>[-+]?)(?:(?P<i1>[0-9]{1,3}(?:,[0-9]{3})+)(?:\\.(?P<f1>[0-9]+)|[^0-9]|$)|(?P<i2>[0-9]+,[0-9]+|[0-9]+)(?:\\.(?P<f2>[0-9]+))?|\\.(?P<f3>[0-9]+))"
);
lazy_re!(AMBIGUOUS_INT, "^[0-9]+,[0-9]+$");
lazy_re!(THOUSANDS_INT, "^[0-9]{1,3}(?:,[0-9]{3})+$");
lazy_re!(TIMES_AFTER, format!("^{}*(?i:times)(?-u:\\b)", s_class()));
lazy_re!(
    PERCENT_AFTER,
    format!("^{}*(?:%|(?i:percent)(?-u:\\b))", s_class())
);
lazy_re!(
    HEDGE_RE,
    format!(
        "(?:(?i:about|around|roughly|nearly|almost|over|under|approximately|approx\\.?|more than|less than|at least|at most)|~|\u{223C}){}*$",
        s_class()
    )
);
lazy_re!(
    FRAC_RE,
    format!(
        "(?P<m>(?P<q>(?i:a|an|{ones}))[{JS_SPACE}-]+(?P<den>(?i:{dens}))(?i:s)?){NA_ALPHA}",
        ones = ones_alt(),
        dens = FRACTION_DEN
            .iter()
            .map(|(k, _)| *k)
            .collect::<Vec<_>>()
            .join("|")
    )
);
lazy_re!(
    DOZEN_RE,
    format!(
        "(?P<m>(?P<q>(?i:a|an|{ones})){s}+(?i:dozen)){NA_ALPHA}",
        ones = ones_alt(),
        s = s_class()
    )
);
lazy_re!(
    RUN_RE,
    format!(
        "(?P<m>(?:(?i:a){s}+)?(?i:{w})(?:(?:{s}*-{s}*|{s}+)(?:(?i:and){s}+)?(?i:{w}))*){NA_ALPHA}",
        w = word_alt(),
        s = s_class()
    )
);
lazy_re!(WORD_RE, format!("(?i:{})", word_alt()));
lazy_re!(ARTICLE_RE, format!("^(?i:a){}", s_class()));
lazy_re!(TOKS_RE, format!("^(?:{}+[A-Za-z]+){{1,2}}", s_class()));
lazy_re!(TWICE_RE, format!("(?P<m>(?i:twice)){NA}"));
lazy_re!(DOUBLE_RE, "(?i:double)");
lazy_re!(HALF_RE, "(?i:half)");
lazy_re!(CHECK_AFTER, format!("^{}+(?i:check)", s_class()));
lazy_re!(
    QUANT_HALF_NEXT,
    format!(
        "^{}+(?i:of|as|a|an|the|your|my|our|their|these|those|all|again|more|less|fewer|over)(?-u:\\b)",
        s_class()
    )
);
lazy_re!(
    QUANT_HALF_UNIT,
    format!(
        "^(?:{s}+[A-Za-z]+){{0,1}}{s}+(?i:games?|moves?|percent|points?)(?-u:\\b)",
        s = s_class()
    )
);
lazy_re!(
    NONQ_HALF_PREV,
    format!(
        "(?-u:\\b)(?i:first|second|other|latter|former|back|top|bottom|upper|lower|opening|closing|final|next|last){}+$",
        s_class()
    )
);

fn hedged_before(t: &str, idx: usize) -> bool {
    HEDGE_RE.is_match(window_before(t, idx, 20))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    Ones,
    Tens,
    Hundred,
    Thousand,
}

/// Evaluates a run of number words into one or more numbers (splitting at invalid sequences).
fn eval_word_run(words: &[String], article: bool) -> Vec<f64> {
    struct St {
        out: Vec<f64>,
        total: f64,
        cur: f64,
        last: Option<Last>,
        any: bool,
    }
    impl St {
        fn flush(&mut self) {
            if self.any {
                self.out.push(self.total + self.cur);
            }
            self.total = 0.0;
            self.cur = 0.0;
            self.last = None;
            self.any = false;
        }
    }
    let mut st = St {
        out: Vec::new(),
        total: 0.0,
        cur: 0.0,
        last: None,
        any: false,
    };
    for (i, w) in words.iter().enumerate() {
        let art = article && i == 0;
        if let Some(v) = lookup(&ONES, w) {
            let after_tens = st.last == Some(Last::Tens) && (1.0..=9.0).contains(&v);
            let fresh =
                st.cur % 100.0 == 0.0 && st.last != Some(Last::Ones) && st.last != Some(Last::Tens);
            if after_tens || fresh {
                st.cur += v;
            } else {
                st.flush();
                st.cur = v;
            }
            st.last = Some(Last::Ones);
            st.any = true;
        } else if let Some(v) = lookup(&TENS, w) {
            if st.cur % 100.0 != 0.0 || st.last == Some(Last::Ones) || st.last == Some(Last::Tens) {
                st.flush();
            }
            st.cur += v;
            st.last = Some(Last::Tens);
            st.any = true;
        } else if w == "hundred" {
            if st.any && st.cur > 0.0 && st.cur < 100.0 {
                st.cur *= 100.0;
            } else if art || !st.any {
                st.flush();
                st.cur = 100.0;
            }
            st.last = Some(Last::Hundred);
            st.any = true;
        } else if w == "thousand" {
            st.total += (if st.cur == 0.0 { 1.0 } else { st.cur }) * 1000.0;
            st.cur = 0.0;
            st.last = Some(Last::Thousand);
            st.any = true;
        }
    }
    st.flush();
    st.out
}

/// Replaces each bounded match's `m` group with one placeholder (TS `replace(re, PH)`).
fn replace_with_ph(t: &str, re: &Regex, before_ok: impl Fn(Option<char>) -> bool) -> String {
    let edits = scan(re, t, before_ok, |c| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        Some((end, (start, end, PH.to_string())))
    });
    splice(t, &edits)
}

fn number(value: f64, hedged: bool) -> ExtractedNumber {
    ExtractedNumber {
        value,
        decimals: 0,
        percent: false,
        hedged,
        kind: NumberKind::Plain,
        ambiguous: false,
    }
}

fn len_u32(s: &str) -> u32 {
    u32::try_from(s.len()).unwrap_or(u32::MAX)
}

/// Digit numbers: `(?<![A-Za-z0-9])([-+]?)(?:(\d{1,3}(?:,\d{3})+(?!\d)|\d+,\d+|\d+)(?:\.(\d+))?|\.(\d+))`.
fn digit_numbers(t: &str, found: &mut Vec<(usize, ExtractedNumber)>) {
    let hits = scan(&NUM_RE, t, not_alnum, |c| {
        let start = c.get(0)?.start();
        let end = ["sign", "i1", "f1", "i2", "f2", "f3"]
            .iter()
            .filter_map(|g| c.name(g).map(|m| m.end()))
            .max()?;
        let int_raw = c
            .name("i1")
            .or_else(|| c.name("i2"))
            .map_or("", |m| m.as_str());
        let frac = ["f1", "f2", "f3"]
            .iter()
            .find_map(|g| c.name(g))
            .map_or("", |m| m.as_str());
        let sign = c.name("sign").map_or("", |m| m.as_str());
        Some((end, (start, end, sign, int_raw, frac)))
    });
    let mut cur = Utf16Cursor::new(t);
    for (start, end, sign, int_raw, frac) in hits {
        let sign = if !sign.is_empty() && prev_char(t, start) == Some('%') {
            "" // "50%-60%" is a range
        } else {
            sign
        };
        let ambiguous = AMBIGUOUS_INT.is_match(int_raw) && !THOUSANDS_INT.is_match(int_raw);
        let mut decimals = len_u32(frac);
        let mut value: f64 = if ambiguous {
            let (a, b) = int_raw.split_once(',').unwrap_or((int_raw, ""));
            decimals = len_u32(b) + len_u32(frac);
            format!("{a}.{b}{frac}").parse().unwrap_or(f64::NAN)
        } else {
            let digits: String = int_raw.chars().filter(|c| *c != ',').collect();
            let digits = if digits.is_empty() {
                "0".to_owned()
            } else {
                digits
            };
            let s = if frac.is_empty() {
                digits
            } else {
                format!("{digits}.{frac}")
            };
            s.parse().unwrap_or(f64::NAN)
        };
        if sign == "-" {
            value = -value;
        }
        let after = &t[end..];
        let mut it = after.chars();
        let ratio = (matches!(it.next(), Some('x' | 'X' | '\u{D7}')) && not_alpha(it.next()))
            || TIMES_AFTER.is_match(after);
        found.push((
            cur.at(start),
            ExtractedNumber {
                decimals,
                percent: PERCENT_AFTER.is_match(after),
                kind: if ratio {
                    NumberKind::Ratio
                } else {
                    NumberKind::Plain
                },
                ambiguous,
                ..number(value, hedged_before(t, start))
            },
        ));
    }
}

fn quantity(q: &str) -> f64 {
    let q = q.to_lowercase();
    if q == "a" || q == "an" {
        1.0
    } else {
        lookup(&ONES, &q).unwrap_or(f64::NAN)
    }
}

/// Extracts numbers (digits, fractions, dozens, counted number words, ratio words): the TS-exact
/// numbers in text order, then any extra numbers the hardened normalisation finds (ADR-R3-2
/// union).
pub fn extract_numbers(text: &str) -> Vec<ExtractedNumber> {
    let ts = numbers_with(text, NormMode::TsExact);
    let hard = numbers_with(text, NormMode::Hardened);
    union_by(ts, hard, |n| {
        // `-0` and `0` are equal numbers, so they share a key.
        let v = if n.value == 0.0 { 0.0 } else { n.value };
        (
            v.to_bits(),
            n.decimals,
            n.percent,
            n.hedged,
            n.kind == NumberKind::Ratio,
            n.ambiguous,
        )
    })
}

/// Numbers under one normalisation, in text order.
fn numbers_with(text: &str, mode: NormMode) -> Vec<ExtractedNumber> {
    let t = normalize_with(text, mode);
    let t = CITATION_RE.replace_all(&t, NoExpand("\u{1}")).into_owned();
    let t = scan_dates(&t).1;
    let t = scan_moves(&t, mode).2;
    // move-number prefixes directly before a removed move
    let t = replace_with_ph(&t, &MOVE_NO_PREFIX, not_alnum);
    let t = PER_CENT.replace_all(&t, NoExpand("percent")).into_owned();
    // weakness ids, band ids, game results, ordinals
    let t = replace_with_ph(&t, &WEAKNESS_ID, not_alnum);
    let t = replace_with_ph(&t, &BAND_ID, not_alnum);
    let t = replace_with_ph(&t, &GAME_RESULT, not_alnum);
    let t = replace_with_ph(&t, &ORDINAL, not_alnum);

    let mut found: Vec<(usize, ExtractedNumber)> = Vec::new();
    digit_numbers(&t, &mut found);

    // fractions ("a third", "two thirds", "three-quarters") and dozens
    let mut mask_ranges: Vec<(usize, usize)> = Vec::new();
    let fracs = scan(&FRAC_RE, &t, not_alpha, |c| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        let den = lookup(&FRACTION_DEN, &c["den"].to_lowercase()).unwrap_or(f64::NAN);
        Some((end, (start, end, quantity(&c["q"]) / den)))
    });
    let mut cur = Utf16Cursor::new(&t);
    for (start, end, v) in fracs {
        let n = ExtractedNumber {
            kind: NumberKind::Ratio,
            decimals: 2,
            ..number(v, hedged_before(&t, start))
        };
        found.push((cur.at(start), n));
        mask_ranges.push((start, end));
    }
    let dozens = scan(&DOZEN_RE, &t, not_alpha, |c| {
        let start = c.get(0)?.start();
        let end = m_end(c);
        Some((end, (start, end, quantity(&c["q"]) * 12.0)))
    });
    let mut cur = Utf16Cursor::new(&t);
    for (start, end, v) in dozens {
        found.push((cur.at(start), number(v, hedged_before(&t, start))));
        mask_ranges.push((start, end));
    }
    let t = if mask_ranges.is_empty() {
        t
    } else {
        // TS masks char by char, so overlapping ranges are fine; merge them for `mask`.
        mask_ranges.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (a, b) in mask_ranges {
            match merged.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => merged.push((a, b)),
            }
        }
        mask(&t, &merged)
    };

    // number words (with a counted unit within two tokens, not followed by "of")
    let runs = scan(&RUN_RE, &t, not_alpha, |c| {
        let start = c.get(0)?.start();
        Some((m_end(c), (start, m_end(c))))
    });
    let mut cur = Utf16Cursor::new(&t);
    for (at, end) in runs {
        let m = &t[at..end];
        let lower = m.to_lowercase();
        let words: Vec<String> = WORD_RE
            .find_iter(&lower)
            .map(|w| w.as_str().to_lowercase())
            .collect();
        let article = ARTICLE_RE.is_match(m);
        let Some(toks) = TOKS_RE.find(&t[end..]) else {
            continue;
        };
        let toks: Vec<String> = toks
            .as_str()
            .split(is_js_space)
            .filter(|x| !x.is_empty())
            .map(str::to_lowercase)
            .collect();
        if toks.first().is_none_or(|x| x == "of") {
            continue;
        }
        if !toks.iter().take(2).any(|x| is_unit(x)) {
            continue;
        }
        let vals = eval_word_run(&words, article);
        let Some(&value) = vals.last() else {
            continue;
        };
        let is_pct = toks.iter().take(2).any(|x| x == "percent");
        let ratio = toks[0] == "times" && (3.0..=10.0).contains(&value);
        found.push((
            cur.at(at),
            ExtractedNumber {
                percent: is_pct,
                kind: if ratio {
                    NumberKind::Ratio
                } else {
                    NumberKind::Plain
                },
                ..number(value, hedged_before(&t, at))
            },
        ));
    }

    // ratio words
    let not_alnum_dash = |c: Option<char>| not_alnum(c) && c != Some('-');
    let twice = scan(&TWICE_RE, &t, not_alnum, |c| {
        Some((m_end(c), (c.get(0)?.start(), m_end(c))))
    });
    let double = scan(&DOUBLE_RE, &t, not_alnum, |c| {
        let m = c.get(0)?;
        let ok = not_alnum_dash(next_char(&t, m.end())) && !CHECK_AFTER.is_match(&t[m.end()..]);
        ok.then_some((m.end(), (m.start(), m.end())))
    });
    let half = scan(&HALF_RE, &t, not_alnum, |c| {
        let m = c.get(0)?;
        not_alnum_dash(next_char(&t, m.end())).then_some((m.end(), (m.start(), m.end())))
    });
    for (hits, value) in [(twice, 2.0), (double, 2.0), (half, 0.5)] {
        let mut cur = Utf16Cursor::new(&t);
        for (at, end) in hits {
            if value == 0.5 {
                let next = &t[end..];
                let quant_next = QUANT_HALF_NEXT.is_match(next) || QUANT_HALF_UNIT.is_match(next);
                if !quant_next || NONQ_HALF_PREV.is_match(window_before(&t, at, 12)) {
                    continue;
                }
            }
            found.push((
                cur.at(at),
                ExtractedNumber {
                    kind: NumberKind::Ratio,
                    decimals: if value % 1.0 != 0.0 { 1 } else { 0 },
                    ..number(value, hedged_before(&t, at))
                },
            ));
        }
    }

    found.sort_by_key(|f| f.0);
    found.into_iter().map(|f| f.1).collect()
}

#[cfg(test)]
mod tests {
    use super::super::bounded::take_scanned;
    use super::super::normalize::normalize_text;
    use super::*;

    type Shape = fn(usize) -> String;

    /// Hostile shapes for every extractor. The first two are the bare-square gap shapes (an
    /// accepted move, a long gap, then many rejected bare squares) that were quadratic before
    /// fix round 1; the last ones are the fix round 2 boundary shapes.
    const SHAPES: &[(&str, Shape)] = &[
        ("move, gap, rejected squares", |n| {
            format!("Nf3{}{}", " ".repeat(n / 2), "x e4 ".repeat(n / 10))
        }),
        ("move, punctuation gap, squares", |n| {
            format!("Nf3{}{}", "(.; ".repeat(n / 8), "the e4 ".repeat(n / 14))
        }),
        ("glued digits", |n| format!("x{}", "1".repeat(n))),
        ("glued thousands", |n| format!("x{}", "1,111".repeat(n / 5))),
        ("comma digits", |n| "1,".repeat(n / 2)),
        ("glued uci", |n| "e2e4".repeat(n / 4)),
        ("dangling long", |n| "e2-".repeat(n / 3)),
        ("castling chain", |n| "O-O-".repeat(n / 4)),
        ("castling glued", |n| "0-0-0x".repeat(n / 6)),
        ("glued san", |n| "Nf3".repeat(n / 3)),
        ("spaced san", |n| "Nf3 ".repeat(n / 4)),
        ("word run", |n| format!("x{}", "one ".repeat(n / 4))),
        ("glued words", |n| "xone ".repeat(n / 5)),
        ("articles", |n| "a ".repeat(n / 2)),
        ("month day", |n| "Oct 1".repeat(n / 5)),
        ("month and spaces", |n| {
            format!("Oct{}x ", " ".repeat(16)).repeat(n / 21)
        }),
        ("day month", |n| "14 Oct ".repeat(n / 7)),
        ("iso prefix", |n| "2026-10-1".repeat(n / 9)),
        ("half", |n| "half ".repeat(n / 5)),
        ("half and spaces", |n| format!("half{}", " ".repeat(n))),
        ("double", |n| "double ".repeat(n / 7)),
        ("spaces", |n| " ".repeat(n)),
        ("nbsp digits", |n| "\u{a0}1".repeat(n / 2)),
        ("move numbers", |n| "12. ".repeat(n / 4)),
        ("weakness ids", |n| "W1".repeat(n / 2)),
        ("zero width", |n| "\u{200b}".repeat(n)),
        ("minus digits", |n| "\u{2212}0".repeat(n / 2)),
        ("combining marks", |n| "e\u{301}".repeat(n / 2)),
        ("homoglyphs", |n| "\u{435}4 ".repeat(n / 3)),
        ("accented neighbours", |n| "Nf3\u{e9} ".repeat(n / 6)),
        ("cyrillic neighbours", |n| "e2e4\u{430} ".repeat(n / 7)),
    ];

    /// Deterministic linearity: the bytes read by every regex search in `scan` (which backs
    /// `bounded_matches` and every extractor pass) and by the per-candidate checks in `scan_moves`,
    /// over both normalisation paths, stay within a constant times the input length. A quadratic
    /// rescan (such as the pre-fix gap check) reads about n^2/20 bytes on the first shape, 1600n
    /// at this n.
    #[test]
    fn scanned_bytes_are_linear_on_hostile_shapes() {
        const N: usize = 1 << 15;
        const C: usize = 120;
        let mut worst = (0, "");
        for (name, make) in SHAPES {
            let text = make(N);
            let n = text.len();
            take_scanned();
            let _ = normalize_text(&text);
            let _ = extract_moves(&text);
            let _ = extract_numbers(&text);
            let _ = extract_dates(&text);
            let scanned = take_scanned();
            assert!(
                scanned <= C * n,
                "{name}: {scanned} bytes scanned for {n} bytes of input"
            );
            if scanned / n > worst.0 {
                worst = (scanned / n, name);
            }
        }
        assert!(worst.0 > 0, "nothing counted");
        eprintln!("worst ratio: {} on {}", worst.0, worst.1);
    }
}
