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

/// Knots (and a value and a knot) closer than this, relative to the reference's scale
/// `max(|p25|, |p40|, |median|, |p75|)`, count as tied.
pub const TIE_REL_TOL: f64 = 1e-9;

/// Band percentile of `value` (TS M2 `percentileFromQuartiles`): piecewise linear through
/// `(p25, 25)`, `(p40, 40)`, `(median, 50)`, `(p75, 75)`; below `p25` it extends the p25-p40
/// slope, above `p75` the median-p75 slope. The result is clamped to `[1, 99]`, and a
/// lower-is-better metric gives `100 - raw` (so 99 is always "better than almost everyone").
///
/// Ties are resolved so that float noise in the reference cannot change the answer (ADR-R2-5):
/// - knots within `TIE_REL_TOL * scale` of each other merge into one point at their mean,
///   carrying the mid-rank of their percentiles (`p25 = p40 = 0` gives one point `(0, 32.5)`);
/// - a value within the same tolerance of a point takes that point's percentile exactly;
/// - otherwise the polyline runs through the merged points, and beyond the ends it extends the
///   nearest non-flat segment (merged points are distinct, so no slope is infinite);
/// - when all four knots merge there is no slope: the point gets 50, values below it 25 and
///   values above it 75 (the knot bounds).
///
/// For finite input the result is monotone in `value` and in `[1, 99]`; it is continuous except
/// for steps of at most `slope * tolerance` at the snap edges, and, for a reference whose
/// quartiles are all exactly 0 (no scale), the step at 0. NaN if the value or a knot is NaN.
pub fn percentile_from_quartiles(value: f64, r: &QuartileRef, higher_is_better: bool) -> f64 {
    let raw = raw_percentile(value, r).clamp(1.0, 99.0);
    if higher_is_better { raw } else { 100.0 - raw }
}

fn raw_percentile(v: f64, r: &QuartileRef) -> f64 {
    let mut xs = [r.p25, r.p40, r.median, r.p75];
    if v.is_nan() || xs.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    // Quantiles from data are ordered already; sorting makes unsorted float noise harmless.
    xs.sort_by(f64::total_cmp);
    let scale = xs.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
    let tol = TIE_REL_TOL * scale;
    // Merge knots within `tol` of their group's first knot into (mean x, mid-rank) points.
    let mut points = [(0.0_f64, 0.0_f64); 4];
    let mut m = 0;
    let mut i = 0;
    while i < xs.len() {
        let mut j = i;
        while j + 1 < xs.len() && xs[j + 1] - xs[i] <= tol {
            j += 1;
        }
        let x = xs[i..=j].iter().sum::<f64>() / (j - i + 1) as f64;
        points[m] = (x, 0.5 * (KNOT_PERCENTILES[i] + KNOT_PERCENTILES[j]));
        m += 1;
        i = j + 1;
    }
    let points = &points[..m];
    if let Some(&(_, y)) = points.iter().find(|&&(x, _)| (v - x).abs() <= tol) {
        return y;
    }
    if let [(x, _)] = points {
        return if v < *x {
            KNOT_PERCENTILES[0]
        } else {
            KNOT_PERCENTILES[3]
        };
    }
    // The segment containing v, or the end segment it extends.
    let seg = (0..m - 1).find(|&s| v <= points[s + 1].0).unwrap_or(m - 2);
    let ((x0, y0), (x1, y1)) = (points[seg], points[seg + 1]);
    y0 + (v - x0) / (x1 - x0) * (y1 - y0)
}
