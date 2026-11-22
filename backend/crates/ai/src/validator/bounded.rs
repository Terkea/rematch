//! Bounded matching: the lookaround emulation of ADR 0058 deviation 2.
//!
//! The `regex` crate has no lookarounds, but it never backtracks catastrophically: every search is
//! linear in the haystack. A TS lookbehind becomes a `before_ok` check on the char before a match.
//! When it fails, the search restarts one char after the rejected start, skipping every position
//! whose before-check also fails, so a long run of rejected starts costs one pass. A TS lookahead at
//! the end of a pattern is either a post-check (`after_ok`), when the match end is forced, or is
//! folded into the pattern as a consumed `(?:[^…]|$)` after a named group `m`, which keeps the JS
//! backtracking order exactly (leftmost-first semantics); the search then resumes at the end of `m`.
//!
//! The helpers below also keep TS's UTF-16 arithmetic where it is observable: window sizes count
//! UTF-16 units and masks write one placeholder per unit.

use regex::{Captures, Regex};

/// The placeholder that masks removed tokens (never alphanumeric, never whitespace).
pub(crate) const PH: char = '\u{1}';

/// The members of JS's `\s` (WhiteSpace and LineTerminator), as a regex class body. Rust's `\s`
/// differs: it adds U+0085 and lacks U+FEFF.
pub(crate) const JS_SPACE: &str = "\\t\\n\\x0B\\x0C\\r \\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}\\x{FEFF}";

/// True for a member of JS's `\s` (and of `String.prototype.trim`).
pub(crate) fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

#[cfg(test)]
thread_local! {
    static SCANNED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts bytes read by a regex search or a per-candidate check, for the deterministic linearity
/// test (unit tests only; a no-op otherwise). The model: a search from `pos` that ends at a match
/// end `e` (or at the text end) reads `e - pos` bytes.
#[inline]
pub(crate) fn count_scanned(n: usize) {
    #[cfg(test)]
    SCANNED.with(|c| c.set(c.get() + n));
    #[cfg(not(test))]
    let _ = n;
}

/// Returns and resets this thread's scanned-byte count.
#[cfg(test)]
pub(crate) fn take_scanned() -> usize {
    SCANNED.with(|c| c.replace(0))
}

/// The char that ends at byte `at`, if any.
pub(crate) fn prev_char(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

/// The char that starts at byte `at`, if any.
pub(crate) fn next_char(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

fn next_boundary(text: &str, at: usize) -> usize {
    at + next_char(text, at).map_or(1, char::len_utf8)
}

/// Finds the matches of `re` in `text` whose preceding char passes `before_ok` and whose following
/// char passes `after_ok` (`None` is the start or end of the text). Returns byte spans of the
/// whole match.
///
/// A rejected match is retried from its start plus one char. For `before_ok` this is exactly what a
/// JS engine does with a lookbehind. `after_ok` is a post-check on the leftmost-first match, so it
/// equals a JS lookahead **only when the pattern's match end is fixed** (no optional, repeated or
/// alternative parts that change the length). A JS engine would backtrack to a shorter match
/// instead: `[a-c]+(?!\d)` matches "ab" in "abc1", but here "abc" is rejected outright. Every
/// variable-length pattern must fold its lookahead into the regex as `(?P<m>X)(?:[^C]|$)` and use
/// [`scan`] with [`m_end`] (ADR 0058).
pub fn bounded_matches(
    re: &Regex,
    text: &str,
    before_ok: impl Fn(Option<char>) -> bool,
    after_ok: impl Fn(Option<char>) -> bool,
) -> Vec<(usize, usize)> {
    scan(re, text, before_ok, |c| {
        let m = c.get(0)?;
        after_ok(next_char(text, m.end())).then_some((m.end(), (m.start(), m.end())))
    })
}

/// The general scan behind [`bounded_matches`]. `check` sees each candidate whose before-check
/// passed and returns `Some((end, value))` to accept it (the next search starts at `end`) or `None`
/// to reject it (the next search starts one char after its start). The candidate's start is the
/// start of the whole match, so a pattern must not consume anything before its logical start.
pub(crate) fn scan<'t, T>(
    re: &Regex,
    text: &'t str,
    before_ok: impl Fn(Option<char>) -> bool,
    mut check: impl FnMut(&Captures<'t>) -> Option<(usize, T)>,
) -> Vec<T> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= text.len() {
        let Some(caps) = re.captures_at(text, pos) else {
            count_scanned(text.len() - pos);
            break;
        };
        // The search read at least from `pos` to the end of the match.
        count_scanned(caps.get(0).map_or(0, |m| m.end()) - pos);
        let Some(start) = caps.get(0).map(|m| m.start()) else {
            break;
        };
        if before_ok(prev_char(text, start))
            && let Some((end, v)) = check(&caps)
        {
            out.push(v);
            pos = if end > start {
                end
            } else {
                next_boundary(text, start)
            };
            continue;
        }
        if start >= text.len() {
            break;
        }
        let mut p = next_boundary(text, start);
        while p < text.len() && !before_ok(prev_char(text, p)) {
            p = next_boundary(text, p);
        }
        count_scanned(p - start);
        pos = p;
    }
    out
}

/// The end of the named group `m` (the logical match when a lookahead is folded in), else of the
/// whole match.
pub(crate) fn m_end(c: &Captures<'_>) -> usize {
    c.name("m").or_else(|| c.get(0)).map_or(0, |m| m.end())
}

/// The slice of at most `units` UTF-16 units that ends at byte `end` (JS `s.slice(i - n, i)`; a
/// surrogate pair cut in half by JS is dropped, which no window pattern can tell apart).
pub(crate) fn window_before(s: &str, end: usize, units: usize) -> &str {
    let mut used = 0;
    let mut start = end;
    for c in s[..end].chars().rev() {
        let w = c.len_utf16();
        if used + w > units {
            break;
        }
        used += w;
        start -= c.len_utf8();
    }
    &s[start..end]
}

/// Replaces every char inside the byte ranges with one [`PH`] per UTF-16 unit, as TS's
/// `PH.repeat(m.length)` does. Ranges must be sorted and disjoint.
pub(crate) fn mask(s: &str, ranges: &[(usize, usize)]) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for &(a, b) in ranges {
        out.push_str(&s[last..a]);
        for c in s[a..b].chars() {
            for _ in 0..c.len_utf16() {
                out.push(PH);
            }
        }
        last = b;
    }
    out.push_str(&s[last..]);
    out
}

/// Replaces each sorted, disjoint byte range with its replacement.
pub(crate) fn splice(s: &str, edits: &[(usize, usize, String)]) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for (a, b, r) in edits {
        out.push_str(&s[last..*a]);
        out.push_str(r);
        last = *b;
    }
    out.push_str(&s[last..]);
    out
}

/// Converts byte offsets into UTF-16 offsets (TS string indexes) in amortised linear time when the
/// offsets are queried in increasing order (it restarts on a smaller one).
pub(crate) struct Utf16Cursor<'s> {
    s: &'s str,
    byte: usize,
    unit: usize,
}

impl<'s> Utf16Cursor<'s> {
    pub(crate) fn new(s: &'s str) -> Self {
        Self {
            s,
            byte: 0,
            unit: 0,
        }
    }

    pub(crate) fn at(&mut self, byte: usize) -> usize {
        if byte < self.byte {
            self.byte = 0;
            self.unit = 0;
        }
        self.unit += self.s[self.byte..byte]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>();
        self.byte = byte;
        self.unit
    }
}
