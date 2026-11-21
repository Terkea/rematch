//! Quantiles (R2 Task 11): type-7 sample quantiles and band percentiles from reference
//! quartiles.

/// Sample quantile of an ascending slice, Hyndman-Fan type 7 (the R and NumPy default):
/// `h = (n - 1) p`, linear between the order statistics around `h`. `p` is clamped to `[0, 1]`;
/// NaN for an empty slice or a NaN `p`.
pub fn quantile(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    if n == 0 || p.is_nan() {
        return f64::NAN;
    }
    let h = (n - 1) as f64 * p.clamp(0.0, 1.0);
    let lo = (h.floor() as usize).min(n - 1);
    let frac = h - lo as f64;
    let hi = (lo + 1).min(n - 1);
    if frac == 0.0 {
        sorted[lo]
    } else {
        sorted[lo] + frac * (sorted[hi] - sorted[lo])
    }
}

/// Reference quantiles of a metric in a rating band (`BandReferenceStat` carries them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuartileRef {
    pub p25: f64,
    pub p40: f64,
    pub median: f64,
    pub p75: f64,
}

/// Percentiles at the knots `p25`, `p40`, `median`, `p75`.
const KNOT_PERCENTILES: [f64; 4] = [25.0, 40.0, 50.0, 75.0];

/// Band percentile of `value` (TS M2 `percentileFromQuartiles`): piecewise linear through
/// `(p25, 25)`, `(p40, 40)`, `(median, 50)`, `(p75, 75)`; below `p25` it extends the p25-p40
/// slope, above `p75` the median-p75 slope. The result is clamped to `[1, 99]`, and a
/// lower-is-better metric gives `100 - raw` (so 99 is always "better than almost everyone").
///
/// Degenerate references stay finite. A value equal to several tied knots (for example
/// `p25 = p40 = 0` for a rare event) gets the middle of their percentiles; outside a flat end
/// segment the slope is infinite and the result saturates at 1 or 99. NaN for a NaN value.
pub fn percentile_from_quartiles(value: f64, r: &QuartileRef, higher_is_better: bool) -> f64 {
    let raw = raw_percentile(value, r).clamp(1.0, 99.0);
    if higher_is_better { raw } else { 100.0 - raw }
}

fn raw_percentile(v: f64, r: &QuartileRef) -> f64 {
    if v.is_nan() {
        return f64::NAN;
    }
    // Quantiles from data are ordered already; sorting only makes float noise harmless.
    let mut xs = [r.p25, r.p40, r.median, r.p75];
    xs.sort_by(f64::total_cmp);
    let ys = KNOT_PERCENTILES;
    let line = |i: usize| -> f64 {
        let (x0, x1, y0, y1) = (xs[i], xs[i + 1], ys[i], ys[i + 1]);
        if x1 > x0 {
            y0 + (v - x0) / (x1 - x0) * (y1 - y0)
        } else if v < x0 {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        }
    };
    if v < xs[0] {
        return line(0);
    }
    if v > xs[3] {
        return line(2);
    }
    if let Some(first) = xs.iter().position(|&x| x == v) {
        let last = xs.iter().rposition(|&x| x == v).unwrap_or(first);
        return 0.5 * (ys[first] + ys[last]);
    }
    (0..3)
        .find(|&i| xs[i] < v && v < xs[i + 1])
        .map_or(f64::NAN, line)
}
