//! The web ports the Beta posterior (`apps/web/src/server/stats/posterior.ts`) for its period
//! comparisons. This test writes a deterministic case grid of `posterior`, Beta quantiles,
//! `prob_above`, `p_improvement`, `p_regression` and the special functions to
//! `tests/fixtures/posterior_cases.json` with `UPDATE_SNAPSHOTS=1`, and otherwise fails when this
//! crate's numbers drift from it. The web's `posterior.test.ts` checks every case to 1e-9.

use std::path::PathBuf;

use profile_core::stats::beta::beta_quantile;
use profile_core::stats::posterior::{
    BetaPosterior, P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED, REGRESSION_FACTOR, p_improvement,
    p_regression, posterior, prob_above,
};
use profile_core::stats::special::{lgamma, ln_beta, reg_inc_beta};
use serde_json::{Value, json};

/// `(k, n)` pairs: empty, no events, all events, sparse, fractional (recency-weighted), large.
const COUNTS: [(f64, f64); 9] = [
    (0.0, 0.0),
    (0.0, 10.0),
    (3.0, 10.0),
    (10.0, 10.0),
    (1.0, 50.0),
    (12.5, 40.25),
    (200.0, 1000.0),
    (5.0, 5000.0),
    (40.0, 120.0),
];
const MUS: [f64; 5] = [0.0, 0.02, 0.3, 0.5, 0.97];
const N0S: [f64; 4] = [0.0, 2.0, 20.0, 150.0];

const QUANTILE_PS: [f64; 6] = [0.001, 0.1, 0.25, 0.5, 0.9, 0.999];
const SHAPES: [(f64, f64); 8] = [
    (0.02, 40.0),
    (0.03, 1.0),
    (0.5, 0.5),
    (1.0, 1.0),
    (2.0, 5.0),
    (30.0, 70.0),
    (1500.0, 8500.0),
    (12.75, 0.4),
];

/// Comparison pairs `(current, baseline)` as `(k, n, mu, n0)`.
type Side = (f64, f64, f64, f64);
const PAIRS: [(Side, Side); 7] = [
    ((3.0, 40.0, 0.1, 20.0), (8.0, 40.0, 0.1, 20.0)),
    ((8.0, 40.0, 0.1, 20.0), (3.0, 40.0, 0.1, 20.0)),
    ((10.0, 20.0, 0.5, 2.0), (10.0, 20.0, 0.5, 2.0)),
    ((0.0, 10.0, 0.05, 20.0), (4.0, 12.0, 0.05, 20.0)),
    ((120.0, 400.0, 0.3, 150.0), (150.0, 420.0, 0.3, 150.0)),
    ((1.0, 50.0, 0.02, 20.0), (2.5, 61.5, 0.02, 20.0)),
    ((0.0, 0.0, 0.0, 0.0), (5.0, 9.0, 0.5, 0.0)),
];

fn side(s: Side) -> (Value, BetaPosterior) {
    let (k, n, mu, n0) = s;
    (
        json!({ "k": k, "n": n, "mu": mu, "n0": n0 }),
        posterior(k, n, mu, n0),
    )
}

fn cases() -> Value {
    let mut post = Vec::new();
    let mut above = Vec::new();
    for &(k, n) in &COUNTS {
        for &mu in &MUS {
            for &n0 in &N0S {
                let p = posterior(k, n, mu, n0);
                post.push(json!({
                    "k": k, "n": n, "mu": mu, "n0": n0,
                    "alpha": p.alpha, "beta": p.beta, "mean": p.mean,
                    "low": p.interval.low, "high": p.interval.high,
                }));
                for threshold in [0.05, p.mean] {
                    above.push(json!({
                        "k": k, "n": n, "mu": mu, "n0": n0, "threshold": threshold,
                        "p": prob_above(&p, threshold),
                    }));
                }
            }
        }
    }

    let mut quantile = Vec::new();
    let mut inc = Vec::new();
    let mut lnb = Vec::new();
    for &(a, b) in &SHAPES {
        lnb.push(json!({ "a": a, "b": b, "y": ln_beta(a, b) }));
        for &p in &QUANTILE_PS {
            let q = beta_quantile(p, a, b).expect("valid shapes");
            quantile.push(json!({ "p": p, "a": a, "b": b, "q": q }));
        }
        for x in [1e-6, 0.01, 0.2, 0.5, 0.8, 0.999] {
            inc.push(json!({ "x": x, "a": a, "b": b, "y": reg_inc_beta(x, a, b) }));
        }
    }
    for &(a, b) in &[(1e5, 2e5), (15.0, 3.0), (3.0, 15.0), (2.5e6, 7.5e6)] {
        lnb.push(json!({ "a": a, "b": b, "y": ln_beta(a, b) }));
    }

    let lg: Vec<Value> = [
        -2.5, 0.1, 0.5, 1.0, 1.5, 2.5, 7.0, 9.99, 10.0, 12.3, 100.0, 1e5,
    ]
    .iter()
    .map(|&x| json!({ "x": x, "y": lgamma(x) }))
    .collect();

    let mut imp = Vec::new();
    let mut reg = Vec::new();
    for &(c, b) in &PAIRS {
        let (cj, cp) = side(c);
        let (bj, bp) = side(b);
        for (delta, draws, seed) in [
            (P_IMP_DELTA_REL, P_IMP_DRAWS, P_IMP_SEED),
            (0.0, P_IMP_DRAWS, P_IMP_SEED),
            (0.0, 2000, 42),
        ] {
            imp.push(json!({
                "current": cj, "baseline": bj, "deltaRel": delta, "draws": draws, "seed": seed,
                "p": p_improvement(&cp, &bp, delta, draws, seed),
            }));
        }
        for (factor, draws, seed) in [
            (REGRESSION_FACTOR, P_IMP_DRAWS, P_IMP_SEED),
            (1.0, P_IMP_DRAWS, P_IMP_SEED),
        ] {
            reg.push(json!({
                "current": cj, "baseline": bj, "factor": factor, "draws": draws, "seed": seed,
                "p": p_regression(&cp, &bp, factor, draws, seed),
            }));
        }
    }

    json!({
        "posterior": post,
        "probAbove": above,
        "quantile": quantile,
        "regIncBeta": inc,
        "lnBeta": lnb,
        "lgamma": lg,
        "pImprovement": imp,
        "pRegression": reg,
    })
}

/// Same shape and keys, and every number within a relative 1e-12 (libm may differ by an ulp
/// between hosts; the web's check is 1e-9).
fn assert_close(stored: &Value, fresh: &Value, path: &str) {
    match (stored, fresh) {
        (Value::Object(s), Value::Object(f)) => {
            let sk: Vec<_> = s.keys().collect();
            let fk: Vec<_> = f.keys().collect();
            assert_eq!(sk, fk, "{path}: keys differ");
            for (k, v) in s {
                assert_close(v, &f[k], &format!("{path}.{k}"));
            }
        }
        (Value::Array(s), Value::Array(f)) => {
            assert_eq!(s.len(), f.len(), "{path}: lengths differ");
            for (i, (a, b)) in s.iter().zip(f).enumerate() {
                assert_close(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Number(s), Value::Number(f)) => {
            let (a, b) = (s.as_f64().expect("f64"), f.as_f64().expect("f64"));
            assert!(
                (a - b).abs() <= 1e-12 * a.abs().max(1.0),
                "{path}: stored {a}, now {b}"
            );
        }
        _ => assert_eq!(stored, fresh, "{path}"),
    }
}

#[test]
fn posterior_cases_match_the_fixture() {
    let fresh = cases();
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/posterior_cases.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&fresh).expect("cases serialise")
        );
        std::fs::create_dir_all(path.parent().expect("fixture dir")).expect("create fixture dir");
        std::fs::write(&path, text).expect("write fixture");
    }
    let stored = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (regenerate it with UPDATE_SNAPSHOTS=1)",
            path.display()
        )
    });
    let stored: Value = serde_json::from_str(&stored).expect("fixture parses");
    assert_close(&stored, &fresh, "cases");
}
