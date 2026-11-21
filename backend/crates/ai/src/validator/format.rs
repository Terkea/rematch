//! Token rendering: a port of m3's `formatNumber` and `resolveToken` (`validator/evidence.ts`),
//! plus the R3 `{{eng:}}` tokens.
//!
//! JS number semantics that matter here: `Math.round` rounds half towards +inf; `String(n)` and
//! `toFixed` never print `-0`; `Number.isInteger`. Rust's shortest `{}` for `f64` equals JS
//! `String(n)` for every value this can print (integers below 1e21 and values rounded to two
//! decimals); JS switches to exponent form at 1e21, which no evidence number reaches.

use super::evidence::{EngineLine, Evidence};
use super::tokens::{NumFormat, PvSide, TokenBody};
use crate::input::types::Score;

/// JS `Math.round`: the nearest integer, ties towards +inf. `x - floor(x)` is exact, so unlike
/// `(x + 0.5).floor()` this is also right for 0.49999999999999994 and for odd integers >= 2^52.
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// JS `Math.sign` (0 for 0, unlike `f64::signum`).
fn js_sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// TS `round(v, d)`: `Math.round((v + EPSILON * sign(v)) * 10^d) / 10^d`.
fn round(v: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    js_round((v + f64::EPSILON * js_sign(v)) * f) / f
}

/// -0 becomes 0, as JS `String` and `toFixed` print it.
fn no_neg_zero(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x }
}

fn to_fixed(x: f64, d: usize) -> String {
    format!("{:.d$}", no_neg_zero(x))
}

fn js_string(x: f64) -> String {
    format!("{}", no_neg_zero(x))
}

/// TS `group`: `/\B(?=(\d{3})+(?!\d))/g` → `,` (commas every three digits of a digit run).
fn group(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            out.push(char::from(b[i]));
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        // Inside a run, `\B` holds between digits and the lookahead counts the digits left in
        // the run. TS only groups integer strings.
        let run = &s[start..i];
        for (k, c) in run.chars().enumerate() {
            if k > 0 && (run.len() - k).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
    }
    out
}

/// TS `formatNumber`. `Pct0`/`Pct1` treat the value as a fraction (0.31 renders as `31%`).
pub fn format_number(value: f64, fmt: Option<NumFormat>) -> String {
    match fmt {
        Some(NumFormat::Int) => group(&js_string(js_round(value))),
        Some(NumFormat::Pct0) => format!("{}%", to_fixed(round(value * 100.0, 0), 0)),
        Some(NumFormat::Pct1) => format!("{}%", to_fixed(round(value * 100.0, 1), 1)),
        Some(NumFormat::Dec1) => to_fixed(round(value, 1), 1),
        Some(NumFormat::Dec2) => to_fixed(round(value, 2), 2),
        Some(NumFormat::Signed1) => {
            let r = round(value, 1);
            if r == 0.0 {
                "0.0".to_owned()
            } else {
                format!(
                    "{}{}",
                    if r > 0.0 { "+" } else { "-" },
                    to_fixed(r.abs(), 1)
                )
            }
        }
        None => {
            if value.is_finite() && value.fract() == 0.0 {
                group(&js_string(value))
            } else {
                js_string(round(value, 2))
            }
        }
    }
}

/// An engine line's evaluation, from White's point of view: centipawns as `signed1` pawns
/// (`+1.3`), `mate in n` when White mates, `mated in n` when White is mated, and `checkmate`
/// for a mate score of 0 (the side to move is already mated).
pub fn eval_text(line: &EngineLine) -> String {
    match line.score {
        Score::Cp(c) => format_number(f64::from(c) / 100.0, Some(NumFormat::Signed1)),
        Score::Mate(0) => "checkmate".to_owned(),
        Score::Mate(m) if m > 0 => format!("mate in {m}"),
        Score::Mate(m) => format!("mated in {}", m.unsigned_abs()),
    }
}

fn nth<T>(xs: &[T], n: u32) -> Option<&T> {
    xs.get(usize::try_from(n.checked_sub(1)?).ok()?)
}

/// Renders a token against the evidence, or `None` when it does not resolve. `{{offtopic}}`
/// never resolves here: the chat engine handles it.
pub fn resolve_token(ev: &Evidence, t: &TokenBody) -> Option<String> {
    let pos = |g: &str, p: u32| ev.positions.get(&format!("{g}#{p}"));
    match t {
        TokenBody::Move { game_id, ply } => Some(pos(game_id, *ply)?.played.as_ref()?.san.clone()),
        TokenBody::Best { game_id, ply } => Some(pos(game_id, *ply)?.best.as_ref()?.san.clone()),
        TokenBody::Pv {
            game_id,
            ply,
            side,
            n,
        } => {
            let p = pos(game_id, *ply)?;
            let line = match side {
                PvSide::Best => &p.best_pv_san,
                PvSide::Refutation => &p.refutation_pv_san,
            };
            nth(line, *n).cloned()
        }
        TokenBody::Line { line_key, n } => {
            let moves = ev.line_moves.get(line_key)?;
            match n {
                None => Some(moves.join(" ")),
                Some(n) => nth(moves, *n).cloned(),
            }
        }
        TokenBody::Num { path, fmt } => Some(format_number(*ev.number_paths.get(path)?, *fmt)),
        TokenBody::Date { path } => ev.date_paths.get(path).cloned(),
        TokenBody::Sq { square } => Some(square.clone()),
        TokenBody::Eng { eval_id, line, n } => {
            let e = ev.engine_evals.get(eval_id)?;
            match (line, n) {
                (None, None) if !e.tried_san.is_empty() => Some(e.tried_san.join(" ")),
                (Some(l), None) => Some(eval_text(nth(&e.lines, *l)?)),
                (Some(l), Some(n)) => nth(&nth(&e.lines, *l)?.san, *n).cloned(),
                _ => None,
            }
        }
        TokenBody::Offtopic => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_round_matches_math_round() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(-12.5), -12.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        assert_eq!(js_round(4503599627370497.0), 4503599627370497.0);
        assert_eq!(js_round(-0.4), 0.0);
    }

    #[test]
    fn group_inserts_commas_in_digit_runs() {
        assert_eq!(group("0"), "0");
        assert_eq!(group("999"), "999");
        assert_eq!(group("1000"), "1,000");
        assert_eq!(group("-1234567"), "-1,234,567");
    }
}
