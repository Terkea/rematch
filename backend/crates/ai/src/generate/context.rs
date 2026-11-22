//! The prompt context of a report (Task 13) and the canonical JSON it is sent and hashed in.
//!
//! [`canonical_json`] is byte-identical to the TS reference `canonicalJson` (object keys sorted
//! by UTF-16 code units, as `Array.prototype.sort` does; every primitive as `JSON.stringify`
//! writes it, so an integral double prints `48`, not `48.0`). [`input_hash`] is its sha256, so a
//! Rust-written and a TS-written report of the same input carry the same `input_hash`
//! (`tests/prompts.rs` checks the three fixtures against a TS-generated golden).

use serde_json::{Map, Number, Value, json};
use sha2::{Digest, Sha256};

use crate::input::types::CoachingInput;
use crate::output::drills::DrillType;
use crate::validator::evidence::{build_evidence, list_date_paths, list_number_paths};
use crate::validator::focus::{GENERAL_FOCUS, select_focuses};
use crate::validator::language::{confidence_phrase, evidence_window_token, likelihood_phrase};

/// JS `Number.prototype.toString` (ECMA-262 Number::toString, radix 10) for a finite double.
/// Rust's `{:e}` gives the same shortest round-trip digits; only the layout differs.
fn js_number(x: f64) -> String {
    if x == 0.0 {
        return "0".to_owned(); // -0 too
    }
    if !x.is_finite() {
        return "null".to_owned(); // JSON.stringify; a serde_json Value never holds one
    }
    let e = format!("{:e}", x.abs());
    let (mantissa, exp) = e.split_once('e').expect("LowerExp has an exponent");
    let exp: i32 = exp.parse().expect("LowerExp exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let sign = if n - 1 < 0 { '-' } else { '+' };
        let e = (n - 1).abs();
        if k == 1 {
            format!("{digits}e{sign}{e}")
        } else {
            format!("{}.{}e{sign}{e}", &digits[..1], &digits[1..])
        }
    };
    if x < 0.0 { format!("-{body}") } else { body }
}

fn number(n: &Number) -> String {
    if n.is_u64() || n.is_i64() {
        n.to_string()
    } else {
        js_number(n.as_f64().expect("a finite JSON number"))
    }
}

fn write(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number(n)),
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("a string serialises")),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            // Sorted explicitly (UTF-16 order, as JS), never relying on the map's order.
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("a key serialises"));
                out.push(':');
                write(&m[k], out);
            }
            out.push('}');
        }
    }
}

/// Keys sorted recursively, no whitespace, numbers and strings as JS `JSON.stringify` writes
/// them.
pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write(v, &mut out);
    out
}

/// [`canonical_json`] with every `<` written as `<` (the same JSON value), for embedding in
/// a tagged prompt block: text inside the input can never close `</coaching_input>`. Only the
/// prompt uses it; hashes use [`canonical_json`].
pub fn canonical_json_escaped(v: &Value) -> String {
    canonical_json(v).replace('<', "\\u003c")
}

/// The sha256 (lowercase hex) of the input's canonical JSON.
pub fn input_hash(input: &CoachingInput) -> String {
    hex::encode(Sha256::digest(canonical_json(&input.to_json()).as_bytes()))
}

/// The drills a focus may use (as Task 8's `validate_section` allows them).
fn allowed_drills(input: &CoachingInput, focus: &str) -> Vec<DrillType> {
    if focus == GENERAL_FOCUS {
        return vec![DrillType::OwnGameAnalysis];
    }
    let variant = input
        .weaknesses
        .iter()
        .find(|w| w.id == focus)
        .and_then(|w| w.variant.as_deref());
    crate::output::drills::drills_for(focus, variant)
}

/// What the report prompts read: the input without `accountId`, plus
/// - `focuses`: the computed focuses (`["general"]` when none qualifies);
/// - `evidenceWindow`: the evidence window in token form;
/// - on each weakness, `likelihoodPhrase` (null without `pImp`) and `confidencePhrase`;
/// - `allowedDrills`: per focus, the drill types it may use;
/// - `refs`: the token paths (`num`, `date`, sorted) and the opening `lines`.
pub fn build_prompt_context(input: &CoachingInput, is_first_report: bool) -> Value {
    let mut ctx = match input.to_json() {
        Value::Object(m) => m,
        _ => unreachable!("CoachingInput serialises to an object"),
    };
    ctx.remove("accountId");

    if let Some(Value::Array(ws)) = ctx.get_mut("weaknesses") {
        for (w, v) in input.weaknesses.iter().zip(ws.iter_mut()) {
            if let Value::Object(o) = v {
                o.insert("likelihoodPhrase".into(), json!(likelihood_phrase(w.p_imp)));
                o.insert(
                    "confidencePhrase".into(),
                    json!(confidence_phrase(w.confidence)),
                );
            }
        }
    }

    let mut focuses = select_focuses(input, is_first_report);
    if focuses.is_empty() {
        focuses.push(GENERAL_FOCUS.to_owned());
    }
    let mut drills = Map::new();
    for f in &focuses {
        let list: Vec<&str> = allowed_drills(input, f)
            .into_iter()
            .map(DrillType::as_str)
            .collect();
        drills.insert(f.clone(), json!(list));
    }

    let ev = build_evidence(input);
    let lines: Vec<&str> = input.openings.iter().map(|o| o.line_key.as_str()).collect();
    ctx.insert("focuses".into(), json!(focuses));
    ctx.insert(
        "evidenceWindow".into(),
        json!(evidence_window_token(input.time_class)),
    );
    ctx.insert("allowedDrills".into(), Value::Object(drills));
    ctx.insert(
        "refs".into(),
        json!({
            "num": list_number_paths(&ev),
            "date": list_date_paths(&ev),
            "lines": lines,
        }),
    );
    Value::Object(ctx)
}

#[cfg(test)]
mod tests {
    use super::js_number;

    #[test]
    fn js_number_layouts() {
        for (x, s) in [
            (48.0, "48"),
            (-0.0, "0"),
            (0.1, "0.1"),
            (1e-7, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (1e-6, "0.000001"),
            (1e21, "1e+21"),
            (1.2e21, "1.2e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (-2.5, "-2.5"),
            (0.13999999999999999, "0.13999999999999999"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
        ] {
            assert_eq!(js_number(x), s, "{x:e}");
        }
    }
}
