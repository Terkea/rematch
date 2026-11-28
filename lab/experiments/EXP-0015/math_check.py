"""EXP-0015: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy, and exhaustive enumeration where the statement is about integers): the
   window algebra of `compute` (Result 1), the bootstrap's variance (Result 2), the block sum
   and the variance of a difference of windows (Result 3), the honest interval and Jensen's
   step (Results 4 and 5), the order-statistic identity and the world's reflection symmetry
   (Result 6), the Wilson cut-offs (Result 8), the dependence bounds (Result 10) and the hide
   line's algebra (Result 11).
2. Simulation (numpy, fixed seeds): a Gaussian AR(1) rating with known parameters, read by the
   production window rule (Results 3 to 7), the production percentile bootstrap on it and on
   i.i.d. values (Results 6 and 12), and the decision rule on binomial counts, independent and
   dependent (Results 9 and 10).
3. Numbers: every number decision.md and summary.md quote (and the power figures of the
   preregistration's section 6), recomputed from results.json and power.json, within the
   rounding it is quoted at.

Standard library, numpy, scipy and sympy only; `analysis` (this folder) is imported only for
its world functions `glicko2` and `glicko1`, whose symmetry Result 6 needs. About 30 s of CPU.
"""

from __future__ import annotations

import itertools
import json
import math
import sys
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import stats

HERE = Path(__file__).resolve().parent
R = json.loads((HERE / "results.json").read_text(encoding="utf-8"))
PJ = json.loads((HERE / "power.json").read_text(encoding="utf-8"))

Z80 = float(stats.norm.ppf(0.9))  # 1.2816
LEVEL = 1 - 0.05 / 3
BOUND, HIDE, NOMINAL = 0.15, 0.20, 0.10
W = 30
CHECKS = [60, 120, 250]
READS = [30, 60, 120, 250]
RESAMPLES = 2000
PHI, SIGMA = 0.984, 30.74  # the AR(1) of section 2 (M3)
LEAN = {
    "Result 9 (the union over the three cells)": "Rematch.union_bound",
    "Result 9 (simultaneous coverage at 1 - 0.05/3)": "Rematch.bonferroni",
}

FAILURES: list[str] = []
COUNT = [0]


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"{'PASS' if ok else 'FAIL'}  {name}{': ' + detail if detail else ''}")
    COUNT[0] += 1
    if not ok:
        FAILURES.append(name)


def zero(expr) -> bool:
    return sp.simplify(expr) == 0


def near(quoted: float, value: float, dp: int) -> bool:
    """`value` rounds to `quoted` at `dp` decimals."""
    return abs(quoted - value) <= 0.5 * 10.0 ** (-dp) + 1e-9


# ---- the formulas of math.md section 3, written here, not imported from analysis.py ---------


def wilson(k, n, level):
    z = stats.norm.ppf(1 - (1 - level) / 2)
    k = np.asarray(k, dtype=float)
    p = k / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * np.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return np.maximum(0.0, c - h), np.minimum(1.0, c + h)


def cuts(n: int, level: float = LEVEL) -> tuple[int, int, int]:
    """(c_keep, c_bound, c_hide): the largest k with u <= 0.15, the smallest with l > 0.15, the
    smallest with l > 0.20 (E5)."""
    k = np.arange(n + 1)
    lo, hi = wilson(k, n, level)
    return int(k[hi <= BOUND].max()), int(k[lo > BOUND].min()), int(k[lo > HIDE].min())


def then_start(n: int, pk: int) -> tuple[int, int, bool]:
    """(E1) from `compute`: the window, the "then" start and whether the basis is the peak."""
    size = min(W, n // 2)
    now = n - size
    earlier = pk < now
    start = max(min(pk - size // 2, now - size), 0) if earlier else 0
    return size, start, earlier


def block(m: int, phi: float) -> float:
    """(E3) S(m) = sum_{i,j<m} phi^|i-j|."""
    return m * (1 + phi) / (1 - phi) - 2 * phi * (1 - phi**m) / (1 - phi) ** 2


def cross(ma: int, mb: int, gap: int, phi: float) -> float:
    """Sum over A x B of phi^(j-i), the smallest lag `gap`."""
    return phi**gap * (1 - phi**ma) * (1 - phi**mb) / (1 - phi) ** 2


def ar1(rng, n_players: int, n: int, phi: float = PHI, sigma: float = SIGMA) -> np.ndarray:
    e = np.empty((n_players, n))
    e[:, 0] = rng.normal(0.0, sigma, n_players)
    inn = sigma * math.sqrt(1 - phi * phi)
    for t in range(1, n):
        e[:, t] = phi * e[:, t - 1] + rng.normal(0.0, inn, n_players)
    return e


def rule(x: np.ndarray):
    """The production windows (E1) and the normal stand-in for the bootstrap (E3): the
    difference, its bootstrap SD (plug-in variances over W) and the basis."""
    n = x.shape[1]
    size = min(W, n // 2)
    now = n - size
    pk = np.argmax(x, axis=1)  # the first maximum, as `compute` (strict >)
    earlier = pk < now
    ts = np.where(earlier, np.clip(pk - size // 2, 0, now - size), 0)
    a = np.take_along_axis(x, ts[:, None] + np.arange(size), 1)
    b = x[:, now:]
    d = b.mean(1) - a.mean(1)
    return d, np.sqrt(a.var(1) / size + b.var(1) / size), earlier


def boot_sorted(rng, a: np.ndarray, b: np.ndarray, resamples: int = RESAMPLES) -> np.ndarray:
    """The sorted resampled differences of `compare_means` for one player (indices uniform)."""
    m, k = len(a), len(b)
    db = b[rng.integers(0, k, (resamples, k))].mean(1)
    da = a[rng.integers(0, m, (resamples, m))].mean(1)
    return np.sort(db - da)


def at(sorted_: np.ndarray, q: float) -> float:
    """`at` in trajectory.rs: the floor(q R) order statistic."""
    i = min(max(int(math.floor(q * len(sorted_))), 0), len(sorted_) - 1)
    return float(sorted_[i])


# `compare_means`: tail = (1 - CONFIDENCE)/2 in f64, the interval [at(tail), at(1 - tail)].
TAIL = (1.0 - 0.8) / 2.0
LO_Q, HI_Q = TAIL, 1.0 - TAIL


def verdict(sorted_: np.ndarray) -> str:
    """(E2): falling when the upper end is below 0, rising when the lower end is above 0."""
    if at(sorted_, HI_Q) < 0:
        return "falling"
    if at(sorted_, LO_Q) > 0:
        return "rising"
    return "flat"


# ---- 1. symbolic -------------------------------------------------------------------------------


def symbolic() -> None:
    # Result 1: the windows, every n from 20 to 400 and every peak position.
    ok_fixed = ok_in = ok_len = True
    for n in range(20, 401):
        for pk in range(n):
            size, s, earlier = then_start(n, pk)
            now = n - size
            ok_len &= s >= 0 and s + size <= now
            if n == 2 * size:
                ok_fixed &= s == 0
            if earlier:
                ok_in &= s <= pk < s + size
    check("R1 'then' lies in [0, n - W - W] and ends at or before 'now' (n = 20..400, every peak)", ok_len)
    check("R1 at n = 2W (every even n <= 60) 'then' is [0, W) whatever the peak", ok_fixed)
    check("R1 with basis 'peak' the peak game lies inside 'then' (n = 20..400)", ok_in)
    fixed_ns = [n for n in range(20, 401) if all(then_start(n, pk)[1] == 0 for pk in range(n))]
    check("R1 the windows cannot move iff n = 2W: n in {20, 22, ..., 60}", fixed_ns == list(range(20, 61, 2)))

    # Result 2: the resampled mean's variance, by enumerating every resample (W = 3 and 4).
    for m in (3, 4):
        xs = sp.symbols(f"x0:{m}")
        means = [sum(xs[i] for i in idx) / m for idx in itertools.product(range(m), repeat=m)]
        mu = sum(means) / len(means)
        var = sp.expand(sum((v - mu) ** 2 for v in means) / len(means))
        xbar = sum(xs) / m
        plug = sum((x - xbar) ** 2 for x in xs) / m
        check(f"R2 Var*(resampled mean) = (1/W) * (1/W) sum (x - xbar)^2, exact over all {m}^{m} resamples",
              zero(var - plug / m) and zero(mu - xbar))
    es = sp.symbols("e0:5")
    eb = sum(es) / 5
    check("R2 (1/W) sum (e - ebar)^2 = (1/W) sum e^2 - ebar^2", zero(sum((e - eb) ** 2 for e in es) / 5 - (sum(e * e for e in es) / 5 - eb**2)))

    # Result 3: S(m) closed form, the AR(1) bootstrap expectation and the fixed-halves variance.
    ph = sp.symbols("phi", positive=True)
    ok = True
    for m in range(1, 31):
        direct = sum(ph ** abs(i - j) for i in range(m) for j in range(m))
        closed = m * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m) / (1 - ph) ** 2
        ok &= sp.cancel(direct - closed) == 0
    check("R3 S(m) closed form = the double sum, m = 1..30 (exact in phi)", ok)
    for m in (4, 6):
        G = sp.Matrix(m, m, lambda i, j: ph ** abs(i - j))
        H = sp.eye(m) - sp.ones(m, m) / m
        Sm = m * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m) / (1 - ph) ** 2
        check(f"R3 E[(1/W) sum (e - ebar)^2] / sigma^2 = 1 - S(W)/W^2 under the AR(1), W = {m}",
              sp.cancel((H * G).trace() / m - (1 - Sm / m**2)) == 0)
        G2 = sp.Matrix(2 * m, 2 * m, lambda i, j: ph ** abs(i - j))
        w = sp.Matrix([-sp.Rational(1, m)] * m + [sp.Rational(1, m)] * m)
        Cm = ph * (1 - ph**m) ** 2 / (1 - ph) ** 2
        check(f"R3 Var(D) at fixed halves = sigma^2 (2 S(W) - 2 C)/W^2 with C's lag 1, W = {m}",
              sp.cancel((w.T * G2 * w)[0] - (2 * Sm - 2 * Cm) / m**2) == 0)

    # Result 4: the honest interval. P(|Z| <= z) = erf(z / sqrt 2), solved for 0.8 by sympy.
    zz = sp.symbols("z", positive=True)
    root = sp.nsolve(sp.erf(zz / sp.sqrt(2)) - sp.Rational(4, 5), zz, 1)
    check("R4 erf(z/sqrt 2) = 0.8 solves to z = 1.2815516, the constant analysis.calibration uses",
          abs(float(root) - 1.2815516) < 1e-7, f"{float(root):.9f}")

    # Result 5: Phi(-x) is convex on x >= 0, so E Phi(-h/tau) >= Phi(-E h / tau).
    x = sp.symbols("x", nonnegative=True)
    Phi = (1 + sp.erf(-x / sp.sqrt(2))) / 2
    check("R5 d^2/dx^2 Phi(-x) = x phi(x) >= 0 on x >= 0", zero(sp.diff(Phi, x, 2) - x * sp.exp(-x**2 / 2) / sp.sqrt(2 * sp.pi)))

    # Result 6 (i): the order statistics under negation, with ties and zeros.
    rng = np.random.default_rng(6)
    idx = (math.floor(LO_Q * RESAMPLES), math.floor(HI_Q * RESAMPLES))
    check("R6 the rule's order statistics: tail = (1 - 0.8)/2 = 0.09999999999999998 in f64, so indices (199, 1800) = (R - 1 - 1800, 1800)",
          idx == (199, 1800) and idx[0] == RESAMPLES - 1 - idx[1], str(idx))
    ok_neg = ok_event = True
    for _ in range(2000):
        d = rng.integers(-3, 4, RESAMPLES).astype(float) / 30.0
        s = np.sort(d)
        sn = np.sort(-d)
        ok_neg &= bool(np.all(sn == -s[::-1]))
        ok_event &= (verdict(sn) == "rising") == (verdict(s) == "falling")
        ok_event &= (verdict(sn) == "falling") == (verdict(s) == "rising")
    check("R6 sorted(-d)[i] = -sorted(d)[R-1-i] (2,000 integer-valued vectors with ties and zeros)", ok_neg)
    check("R6 so the reflected player's 'rising' is exactly the original's 'falling', and back, pathwise", ok_event)

    # Result 6 (ii): the world is symmetric under r -> 3000 - r, s -> 1 - s (analysis.py's code).
    if str(HERE) not in sys.path:
        sys.path.insert(0, str(HERE))  # the lab check may run this script in isolated mode
    import analysis  # the world's own functions

    g = np.random.default_rng(66)
    ok2 = ok1 = True
    for _ in range(3000):
        r, ro = g.uniform(600, 2400, 2)
        rd = g.uniform(45, 500)
        vol = g.uniform(0.05, 0.1)
        s = float(g.choice([0.0, 0.5, 1.0]))
        a = analysis.glicko2(r, rd, vol, ro, 60.0, s)
        b = analysis.glicko2(3000 - r, rd, vol, 3000 - ro, 60.0, 1 - s)
        ok2 &= abs(a[0] - (3000 - b[0])) < 1e-6 and abs(a[1] - b[1]) < 1e-9 and abs(a[2] - b[2]) < 1e-9
        rd1 = g.uniform(50, 350)
        c = analysis.glicko1(r, rd1, ro, 60.0, s)
        d_ = analysis.glicko1(3000 - r, rd1, 3000 - ro, 60.0, 1 - s)
        ok1 &= abs(c[0] - (3000 - d_[0])) < 1e-9 and abs(c[1] - d_[1]) < 1e-12
    check("R6 analysis.glicko2 commutes with the reflection (3,000 random states)", ok2)
    check("R6 analysis.glicko1 commutes with the reflection (3,000 random states)", ok1)
    e_, dd, u = sp.symbols("E d u", positive=True)
    # play(): win when u < E - d/2, draw when u < E + d/2. With E' = 1 - E and u' = 1 - u, a win
    # of the reflected player is a loss of the original, and d(E) = 4 c E (1 - E) is symmetric.
    c06 = sp.Rational(6, 100)
    dE = 4 * c06 * e_ * (1 - e_)
    ok_res = zero(dE.subs(e_, 1 - e_) - dE)
    ok_res &= zero(((1 - e_) - dE / 2) - (1 - (e_ + dE / 2)))  # P'(win) = P(loss)
    ok_res &= zero(1 / (1 + 10 ** (-(-dd) / 400)) - (1 - 1 / (1 + 10 ** (-dd / 400))))
    check("R6 the result rule: E(-x) = 1 - E(x), draws symmetric, a reflected win is a loss", ok_res)
    lo_, hi_ = analysis.STRENGTH_RANGE
    check("R6 the strengths N(1500, 300) clipped to [700, 2300] and the start 1500 are symmetric about 1500",
          analysis.STRENGTH_MEAN == 1500 and lo_ + hi_ == 3000 and analysis.START_RATING == 1500)
    xr = g.uniform(500, 2500, 100_000)
    rounded = np.floor(xr + 0.5)
    check("R6 rounding floor(x + 1/2) commutes with x -> 3000 - x off the half-integers",
          bool(np.all(3000 - np.floor(3000 - xr + 0.5) == rounded)))

    # Result 8: Wilson's ends increase in k, and P(Bin <= c) falls in p.
    ok = True
    for n in (200, 700):
        lo, hi = wilson(np.arange(n + 1), n, LEVEL)
        ok &= bool(np.all(np.diff(lo) >= 0) and np.all(np.diff(hi) > 0))
    ps = np.linspace(0.001, 0.999, 999)
    ok &= bool(np.all(np.diff(stats.binom.cdf(82, 700, ps)) <= 1e-15))
    check("R8 Wilson ends increase in k (n = 200, 700); P_p(K <= c) is non-increasing in p (grid)", ok)
    check("R8 the cut-offs at 700 players: keep k <= 82, above the bound k >= 128, above the hide line k >= 166",
          cuts(700) == (82, 128, 166), str(cuts(700)))

    # Result 10: Frechet bounds for three events of probability P.
    P = sp.symbols("P", positive=True)
    # Frechet's bounds on random dependent joint laws of three events (Dirichlet over 8 cells).
    gr = np.random.default_rng(10)
    cells = np.array(list(itertools.product((0, 1), repeat=3)))
    ok = True
    for _ in range(20000):
        w = gr.dirichlet(np.full(8, gr.uniform(0.05, 2.0)))
        pe = [(w * cells[:, j]).sum() for j in range(3)]
        p_all, p_any = w[7], 1 - w[0]
        ok &= max(0.0, sum(pe) - 2) - 1e-12 <= p_all <= min(pe) + 1e-12
        ok &= max(pe) - 1e-12 <= p_any <= min(1.0, sum(pe)) + 1e-12
    check("R10 Frechet: max(0, sum P - 2) <= P(all) <= min P and max P <= P(any) <= min(1, sum P), 20,000 random joint laws", ok)
    check("R10 1 - (1 - P)^3 >= P (the 'any cell' read never falls below one cell's power)",
          zero(sp.expand(1 - (1 - P) ** 3 - P - (P * (1 - P) * (2 - P)))))

    # Result 11: the hide line. False headlines outnumber true ones iff (1 - b) f > b d.
    b, d, f = sp.symbols("b d f", positive=True)
    fstar = sp.solve(sp.Eq((1 - b) * f, b * d), f)[0]
    bstar = sp.solve(sp.Eq((1 - b) * f, b * d), b)[0]
    check("R11 break-even f* = b d / (1 - b) and b* = f / (f + d)", zero(fstar - b * d / (1 - b)) and zero(bstar - f / (f + d)))
    share_false = (1 - b) * f / ((1 - b) * f + b * d)
    check("R11 the share of false headlines is 1/2 exactly at f = f*", zero(share_false.subs(f, fstar) - sp.Rational(1, 2)))


# ---- 2. simulation -----------------------------------------------------------------------------


def simulation() -> None:
    diag = R["diagnostics"]
    rng = np.random.default_rng(20261128)
    n_sim = 20_000
    e = ar1(rng, n_sim, max(CHECKS))
    # S1 (M3): phi = 0.984 reproduces EXP-0015's own lag-1 statistic (the Pearson correlation of
    # (e_t, e_t+1) within each 250-game series, averaged).
    xa, xb = e[:, :-1], e[:, 1:]
    xa_c = xa - xa.mean(1, keepdims=True)
    xb_c = xb - xb.mean(1, keepdims=True)
    lag = (xa_c * xb_c).sum(1) / np.sqrt((xa_c**2).sum(1) * (xb_c**2).sum(1))
    se_700 = lag.std(ddof=1) / math.sqrt(700)
    check("S1 (M3) the AR(1) at phi = 0.984 gives EXP-0015's lag-1 statistic 0.9655 within 4 SE of a 700-player mean",
          abs(lag.mean() - diag["settled_error_lag1"]) < 4 * se_700, f"{lag.mean():.5f} vs {diag['settled_error_lag1']}, SE {se_700:.5f}")
    sd_all = e.std()
    check("S1 (M3) the stationary error SD is sigma = 30.74 (within 1%)", abs(sd_all / SIGMA - 1) < 0.01, f"{sd_all:.2f}")
    # Result 3: the 30-game window mean's SD, simulated against sigma sqrt(S(30))/30, and the model
    # against the measured 28.52.
    wm = np.concatenate([e[:, i : i + 30].mean(1) for i in range(0, 240, 30)])
    want = SIGMA * math.sqrt(block(30, PHI)) / 30
    se = wm.std() / math.sqrt(2 * len(wm) / 8)  # 8 windows a series are correlated: a loose SE
    check("S1 Result 3: SD of a 30-game window mean = sigma sqrt(S(30))/30 = 28.45 (simulated)",
          abs(wm.std() - want) < 4 * se, f"{wm.std():.3f} vs {want:.3f}")
    check("S1 Result 3: the model's 28.45 against the measured 28.52 (within 1%)",
          abs(want / diag["settled_window_mean_sd"]["30"] - 1) < 0.01)
    # Result 4 (iii): Var(D) at fixed halves (60 games) from the closed form.
    vfix = SIGMA**2 * (2 * block(30, PHI) - 2 * cross(30, 30, 1, PHI)) / 900
    r60 = np.floor(1500 + e[:, :60] + 0.5)
    d60 = r60[:, 30:].mean(1) - r60[:, :30].mean(1)
    se_v = d60.var() * math.sqrt(2 / (n_sim - 1))
    check("S1 Result 3: Var(D) at 60 games = sigma^2 (2 S - 2 C)/W^2 (rounded ratings, +1/(6W) for the rounding)",
          abs(d60.var() - (vfix + 1 / (6 * 30))) < 4 * se_v, f"SD {d60.std():.2f} vs {math.sqrt(vfix):.2f}")
    # Results 5 and 7: the production windows with the bootstrap's normal stand-in, against the run.
    r = np.floor(1500 + e + 0.5)
    table7 = {30: (0.414, 0.412, 0.1, 16.1, 3.80), 60: (0.431, 0.435, 0.2, 20.7, 3.72),
              120: (0.626, 0.294, -9.4, 33.9, 3.75), 250: (0.778, 0.170, -24.4, 38.6, 3.75)}
    for g in READS:
        d, s, _ = rule(r[:, :g])
        fall, rise = float(np.mean(d + Z80 * s < 0)), float(np.mean(d - Z80 * s > 0))
        q = table7[g]
        check(f"S2 Result 7's table at {g} games as quoted in math.md",
              near(q[0], fall, 3) and near(q[1], rise, 3) and near(q[2], float(d.mean()), 1) and near(q[3], float(d.std()), 1)
              and near(q[4], float((Z80 * s).mean()), 2))
        if g == 30:
            mf, mr = R["exploratory"]["games_30"]["share"], R["exploratory"]["rising_30"]["share"]
        else:
            mf = next(c["share"] for c in R["primary"]["cells"] if c["games"] == g)
            mr = next(c["share"] for c in R["secondary"]["rising"] if c["games"] == g)
        sef = math.sqrt(mf * (1 - mf) / 700 + fall * (1 - fall) / n_sim)
        ser = math.sqrt(mr * (1 - mr) / 700 + rise * (1 - rise) / n_sim)
        iv = diag["interval"][str(g)]
        se_m = iv["diff_sd"] / math.sqrt(700)
        se_s = iv["diff_sd"] / math.sqrt(2 * 700)
        check(f"S2 Result 7 at {g} games: the AR(1) model's falling {fall:.3f} and rising {rise:.3f} vs the run's {mf:.3f}, {mr:.3f} (4 SE)",
              abs(fall - mf) < 4 * sef and abs(rise - mr) < 4 * ser)
        check(f"S2 Result 7 at {g} games: mean D {d.mean():.1f}, SD {d.std():.1f} vs {iv['diff_mean']:.1f}, {iv['diff_sd']:.1f} (4 SE)",
              abs(d.mean() - iv["diff_mean"]) < 4 * se_m and abs(d.std() - iv["diff_sd"]) < 4 * se_s)
        hw = float((Z80 * s).mean())
        check(f"S2 Result 2 at {g} games: the model's half-width {hw:.2f} within 4% of the run's {iv['half_width_mean']:.2f}",
              abs(hw / iv["half_width_mean"] - 1) < 0.04)
        if g in (30, 60):
            tau = d.std()
            hs = Z80 * s
            lower = float(stats.norm.cdf(-hs.mean() / tau))
            mean_phi = float(stats.norm.cdf(-hs / tau).mean())
            sef_ = math.sqrt(fall * (1 - fall) / n_sim)
            check(f"S2 Result 5 at {g} games: Phi(-E h/tau) {lower:.4f} <= E Phi(-h/tau) {mean_phi:.4f} (Jensen), and E Phi(-h/tau) = the falling share {fall:.4f} within 4 SE",
                  lower <= mean_phi and abs(mean_phi - fall) < 4 * sef_)
    # The no-peak read: fixed halves at 120 and 250, symmetric (Result 6).
    for k, g in enumerate((120, 250)):
        x = np.concatenate([r[:, :30], r[:, g - 30 : g]], 1)
        d, s, _ = rule(x)
        fall, rise = float(np.mean(d + Z80 * s < 0)), float(np.mean(d - Z80 * s > 0))
        h = R["exploratory"]["first_and_last_30"][k]
        mf, mr = h["falling"]["share"], h["rising"]["share"]
        sef = math.sqrt(mf * (1 - mf) / 700 + fall * (1 - fall) / n_sim)
        ser = math.sqrt(mr * (1 - mr) / 700 + rise * (1 - rise) / n_sim)
        check(f"S2 Result 7's no-peak falling {fall:.3f} as quoted (0.456, 0.460)", near((0.456, 0.460)[k], fall, 3))
        check(f"S2 Result 7 no-peak read at {g}: model falling {fall:.3f}, rising {rise:.3f} vs {mf:.3f}, {mr:.3f} (4 SE)",
              abs(fall - mf) < 4 * sef and abs(rise - mr) < 4 * ser)

    # S3 Result 6: the production percentile bootstrap at 60 games, with the reflection coupling.
    g2 = np.random.default_rng(606)
    m = 1500
    x60 = r[:m, :60]
    fall = rise = 0
    ok_path = True
    for i in range(m):
        a, b = x60[i, :30], x60[i, 30:]
        ia = g2.integers(0, 30, (RESAMPLES, 30))
        ib = g2.integers(0, 30, (RESAMPLES, 30))
        dd = b[ib].mean(1) - a[ia].mean(1)
        s = np.sort(dd)
        # The reflected player, same indices: values 3000 - r, so every difference negates.
        dr = (3000 - b)[ib].mean(1) - (3000 - a)[ia].mean(1)
        sr = np.sort(dr)
        ok_path &= bool(np.allclose(dr, -dd))
        v, vr = verdict(s), verdict(sr)
        fall += v == "falling"
        rise += v == "rising"
        ok_path &= (vr == "rising") == (v == "falling") and (vr == "falling") == (v == "rising")
    check("S3 Result 6: with the rule's indices (199, 1800), every reflected player's verdict mirrors the original's (1,500 players, 2,000 resamples)", ok_path)
    pf, pr = fall / m, rise / m
    se = math.sqrt((pf + pr) / m)
    check("S3 Result 6: falling and rising equal at 60 games within 4 SE (the real bootstrap)", abs(pf - pr) < 4 * se, f"{pf:.3f}, {pr:.3f}")

    # S5' Result 12: accuracy through the production windows, with results driven by the error.
    g5 = np.random.default_rng(1213)
    n_acc = 20_000
    ea = e[:n_acc]
    xo = ea + g5.normal(0, 75, ea.shape) + g5.normal(0, 50, ea.shape)  # opponent's strength - theta
    pi_ = 1 / (1 + 10 ** (xo / 400))
    dr_ = 4 * 0.06 * pi_ * (1 - pi_)
    uu = g5.random(ea.shape)
    res = np.where(uu < pi_ - dr_ / 2, 0, np.where(uu < pi_ + dr_ / 2, 1, 2))  # win, draw, loss
    acc = np.clip(g5.normal(np.array([79.0, 75.0, 69.0])[res], 8.0), 10, 99).round(1)
    ra = r[:n_acc]
    shares = {}
    for g in CHECKS:
        x = ra[:, :g]
        size, now = W, g - W
        pk = np.argmax(x, axis=1)
        ts = np.where(pk < now, np.clip(pk - size // 2, 0, now - size), 0)
        aa = np.take_along_axis(acc[:, :g], ts[:, None] + np.arange(size), 1)
        bb = acc[:, now:g]
        dd = bb.mean(1) - aa.mean(1)
        ss = np.sqrt(aa.var(1) / size + bb.var(1) / size)
        shares[g] = (float(np.mean(dd + Z80 * ss < 0)), float(np.mean(dd - Z80 * ss > 0)), float(dd.mean()))
    se_a = math.sqrt(0.11 * 0.89 / n_acc)
    w60, b60, _ = shares[60]
    w250, b250, m250 = shares[250]
    check(f"S5 Result 12: accuracy 'worse' and 'better' at fixed halves (60) are equal and near 0.106: {w60:.3f}, {b60:.3f}",
          abs(w60 - b60) < 4 * math.sqrt(2) * se_a and abs(w60 - 0.106) < 0.01)
    check(f"S5 Result 12: on the peak window the selection lowers 'worse' and raises 'better' at 250 games: {w250:.3f}, {b250:.3f}, mean D {m250:+.2f}",
          b250 - w250 > 0.02 and w250 < w60 and m250 > 0, "; ".join(f"{g}: {v[0]:.3f}/{v[1]:.3f}" for g, v in shares.items()))
    del xo, pi_, dr_, uu, res, acc

    # S4 Result 12: the bootstrap on values independent from game to game.
    g3 = np.random.default_rng(1212)
    m = 3000
    f = 0
    for _ in range(m):
        a, b = g3.normal(0, 1, 30), g3.normal(0, 1, 30)
        s = boot_sorted(g3, a, b)
        f += verdict(s) == "falling"
    pf = f / m
    want = float(stats.t.sf(Z80 * math.sqrt(29 / 30), 58))
    check("S4 Result 12: i.i.d. values, the bootstrap says 'worse' per side about P(T_58 > z sqrt(29/30)) = 0.106",
          abs(pf - want) < 4 * math.sqrt(want * (1 - want) / m), f"{pf:.4f} vs {want:.4f}")

    # S5 Results 9 and 10: the decision rule on counts. Exact sizes, then a dependent model.
    g4 = np.random.default_rng(99)
    c_keep, c_bound, c_hide = cuts(700)
    reps = 400_000
    k = g4.binomial(700, HIDE, reps)
    lo, _ = wilson(k, 700, LEVEL)
    exact = float(stats.binom.sf(c_hide - 1, 700, HIDE))
    est = float(np.mean(lo > HIDE))
    check("S5 Result 9: P(a cell reads above the hide line | p = 0.20) = 0.00894 (400,000 draws)",
          abs(est - exact) < 4 * math.sqrt(exact * (1 - exact) / reps), f"{est:.5f} vs {exact:.5f}")
    # Positive dependence by a Gaussian copula (an associated law), 700 players, three cells.
    sims = 20_000
    rho = 0.5
    for p, what in ((0.10, "keep"), (0.25, "hide")):
        zc = g4.normal(size=(sims, 700, 1))
        zi = g4.normal(size=(sims, 700, 3))
        lat = math.sqrt(rho) * zc + math.sqrt(1 - rho) * zi
        kk = (lat < stats.norm.ppf(p)).sum(1)
        lo, hi = wilson(kk, 700, LEVEL)
        if what == "keep":
            dep = float(np.mean(np.all(hi <= BOUND, 1)))
            ind = float(stats.binom.cdf(c_keep, 700, p) ** 3)
            single = float(stats.binom.cdf(c_keep, 700, p))
            check("S5 Result 10: under positive dependence 'keep' at 0.10 rises above the independent 0.830 (and stays <= one cell's)",
                  ind < dep <= single + 4 * math.sqrt(single * (1 - single) / sims), f"{dep:.4f}")
        else:
            dep = float(np.mean(np.any(lo > HIDE, 1)))
            ind = float(1 - stats.binom.cdf(c_hide - 1, 700, p) ** 3)
            single = float(stats.binom.sf(c_hide - 1, 700, p))
            check("S5 Result 10: under positive dependence 'hide' at 0.25 in all three falls below the independent 0.991 (not below one cell's 0.796)",
                  single - 4 * math.sqrt(single * (1 - single) / sims) <= dep < ind, f"{dep:.4f}")
        del zc, zi, lat


# ---- 3. numbers --------------------------------------------------------------------------------


def numbers() -> None:
    P = {c["games"]: c for c in R["primary"]["cells"]}
    RS = {c["games"]: c for c in R["secondary"]["rising"]}
    SEC = {(c["measure"], c["games"]): c for c in R["secondary"]["then_now_rows"]}
    DET = {(c["decline"], c["games"]): c for c in R["detection"]}
    ex = R["exploratory"]
    diag = R["diagnostics"]

    # Every committed Wilson interval, from its k and n.
    ok = True
    cells = list(P.values()) + list(RS.values()) + list(SEC.values()) + list(DET.values())
    fams = [3] * 6 + [6] * 6 + [6] * 6
    for c, fam in zip(cells, fams, strict=True):
        for key, lev in (("ci95", 0.95), ("ci80", 0.80), ("ci_bonferroni", 1 - 0.05 / fam)):
            lo, hi = wilson(c["k"], c["n"], lev)
            ok &= abs(float(lo) - c[key][0]) < 1e-6 and abs(float(hi) - c[key][1]) < 1e-6
        ok &= abs(c["k"] / c["n"] - c["share"]) < 1e-6
    check("N every primary, rising, accuracy, blunders and detection interval recomputed from k and n (to 1e-6)", ok)

    # The reads and the decision (Result 8): k against the cut-offs.
    c_keep, c_bound, c_hide = cuts(700)

    def read(k: int, n: int, fam: int) -> str:
        ck, cb, ch = cuts(n, 1 - 0.05 / fam)
        return "above the hide line" if k >= ch else "above the bound" if k >= cb else "within the bound" if k <= ck else "undecided"

    ok = all(read(c["k"], 700, 3) == c["read"] for c in list(P.values()) + list(RS.values()))
    ok &= all(read(c["k"], 700, 6) == c["read"] for c in SEC.values())
    ok &= R["primary"]["decision"] == "hide the verdict until fixed" and all(P[g]["k"] >= c_hide for g in CHECKS)
    check("N the reads from the cut-offs: primary all above the hide line (310, 435, 549 >= 166); rising 307, 210 above, 120 undecided; the six rows within",
          ok)
    los = [P[g]["ci_bonferroni"][0] for g in CHECKS]
    check("N decision: Bonferroni lower ends 0.40, 0.58 and 0.7448 against 0.20 (Finding 2: quoted 0.75, which rounds to 0.74)",
          all(near(q, v, 2) for q, v in zip([0.40, 0.58, 0.74], los, strict=True)) and not near(0.75, los[2], 2) and min(los) > HIDE,
          ", ".join(f"{v:.4f}" for v in los))
    check("N decision/summary: 44%, 62%, 78%; 310, 435, 549 of 700; 0.443, 0.621, 0.784",
          all(near(q, P[g]["share"], 3) for q, g in zip([0.443, 0.621, 0.784], CHECKS, strict=True))
          and [P[g]["k"] for g in CHECKS] == [310, 435, 549]
          and all(near(q, 100 * P[g]["share"], 0) for q, g in zip([44, 62, 78], CHECKS, strict=True)))
    q95 = [[0.406, 0.480], [0.585, 0.657], [0.752, 0.813]]
    qbf = [[0.399, 0.488], [0.577, 0.664], [0.745, 0.819]]
    ok = all(near(a, b, 3) for g, q in zip(CHECKS, q95, strict=True) for a, b in zip(q, P[g]["ci95"], strict=True))
    ok &= all(near(a, b, 3) for g, q in zip(CHECKS, qbf, strict=True) for a, b in zip(q, P[g]["ci_bonferroni"], strict=True))
    ok &= all(near(a, 100 * b, 0) for g, q in zip(CHECKS, [[41, 48], [58, 66], [75, 81]], strict=True) for a, b in zip(q, P[g]["ci95"], strict=True))
    check("N the primary table's 95% and Bonferroni intervals, and the summary's 41 to 48, 58 to 66, 75 to 81", ok)
    check("N decision: 'four to eight times' the nominal 0.10 (4.4 to 7.8)", near(4, P[60]["share"] / NOMINAL, 0) and near(8, P[250]["share"] / NOMINAL, 0))
    ch = {c["games"]: c["change"] for c in ex["mean_change_when_falling"]}
    check("N decision: mean change when falling -39, -50, -59", all(near(q, ch[g], 0) for q, g in zip([-39, -50, -59], CHECKS, strict=True)))

    # Result 11: the hide line and the break-even shares.
    check("N prereg section 5: b d/(1 - b) = 0.20, 0.09, 0.34 at b = 0.2, 0.1, 0.3 (d = 0.8)",
          near(0.20, 0.2 * 0.8 / 0.8, 2) and near(0.09, 0.1 * 0.8 / 0.9, 2) and near(0.34, 0.3 * 0.8 / 0.7, 2))
    dets = [c["share"] for c in R["detection"]]
    fmin, fmax = 0.25 * min(dets), 0.25 * max(dets)
    check("N decision: detection 0.735 to 1.0 gives a break-even 0.18 to 0.25 at b = 0.2, below every lower end",
          near(0.735, min(dets), 3) and max(dets) == 1.0 and near(0.18, fmin, 2) and near(0.25, fmax, 2) and min(los) > fmax)
    bst = [lo / (lo + 1.0) for lo in los]
    check("N decision: b* = l/(l + 1) = 28%, 37%, 43% (Bonferroni lower end, detection 1)",
          all(near(q, 100 * v, 0) for q, v in zip([28, 37, 43], bst, strict=True)), ", ".join(f"{v:.4f}" for v in bst))
    lrs = [DET[d, g]["share"] / P[g]["share"] for d in (100, 200) for g in CHECKS]
    check("N decision: likelihood ratios 1.25 to 2.1", near(1.25, min(lrs), 2) and near(2.1, max(lrs), 1), f"{min(lrs):.3f} to {max(lrs):.3f}")
    fs = [0.8 * P[g]["share"] / (0.8 * P[g]["share"] + 0.2 * DET[d, g]["share"]) for d in (100, 200) for g in CHECKS]
    fs100 = fs[:3]
    check("N decision/summary: 'about three in four' false at b = 0.2 is 0.65 to 0.76 (0.71 to 0.76 at a 100-point fall)",
          near(0.65, min(fs), 2) and near(0.76, max(fs), 2) and near(0.71, min(fs100), 2), f"{min(fs):.4f} to {max(fs):.4f}")

    # Results 2 to 4: the interval.
    iv = diag["interval"]
    hws = [iv[str(g)]["half_width_mean"] for g in READS]
    check("N decision: the rule's mean half-width is 3.7 at every length", all(near(3.7, h, 1) for h in hws))
    check("N decision: difference SD 21, 34, 37; honest half-widths 26 to 47",
          all(near(q, iv[str(g)]["diff_sd"], 0) for q, g in zip([21, 34, 37], CHECKS, strict=True))
          and near(26, iv["60"]["honest_half_width"], 0) and near(47, iv["250"]["honest_half_width"], 0))
    check("N Result 4: honest_half_width = 1.2815516 x diff_sd at every length",
          all(abs(iv[str(g)]["honest_half_width"] - 1.2815516 * iv[str(g)]["diff_sd"]) < 1e-5 for g in READS))
    ratios = [iv[str(g)]["honest_half_width"] / iv[str(g)]["half_width_mean"] for g in CHECKS]
    check("N Finding 2: the rule is 7.2 to 12.7 times too narrow at 60 to 250 games (quoted 'six to thirteen'); 5.5 at 30",
          near(7.2, ratios[0], 1) and near(12.7, ratios[2], 1) and near(5.5, iv["30"]["honest_half_width"] / iv["30"]["half_width_mean"], 1),
          ", ".join(f"{v:.2f}" for v in ratios))
    se, w30 = diag["settled_error_sd"], diag["settled_window_mean_sd"]["30"]
    w15 = diag["settled_window_mean_sd"]["15"]
    pred = Z80 * math.sqrt(2 * (se**2 - w30**2) / 30)
    pred15 = Z80 * math.sqrt(2 * (se**2 - w15**2) / 15)
    hw30 = [iv[str(g)]["half_width_mean"] for g in CHECKS]
    check("N Result 2: predicted half-width z sqrt(2 (sigma^2 - v_W)/W) = 3.80 (W = 30) and 3.89 (W = 15), above the measured 3.69 to 3.73 and 3.73 (Jensen)",
          near(3.80, pred, 2) and near(3.89, pred15, 2) and all(h < pred for h in hw30) and iv["30"]["half_width_mean"] < pred15,
          f"{pred:.3f}, {pred15:.3f}")
    check("N Result 2: sigma^2 - v_30 = 132 of sigma^2 = 945 points^2", near(132, se**2 - w30**2, 0) and near(945, se**2, 0))
    neff = (se / w30) ** 2
    check("N decision: 'about one independent game's worth': n_eff = sigma^2/Var(ebar_30) = 1.16 (AR(1): 900/S(30) = 1.17)",
          near(1.16, neff, 2) and near(1.17, 900 / block(30, PHI), 2), f"{neff:.3f}")
    check("N decision: step about 5 a game, error SD 31, correlation 0.97, window SD 29, RD at 45",
          near(5, diag["settled_step_mean"], 0) and near(31, se, 0) and near(0.97, diag["settled_error_lag1"], 2)
          and near(29, w30, 0) and near(45, diag["settled_rd_mean"], 0))
    check("N decision: the difference is off-centre at -9 and -23 points (120, 250)", near(-9, iv["120"]["diff_mean"], 0) and near(-23, iv["250"]["diff_mean"], 0))
    # (M3): K from one Glicko-2 step at lila's floor, E[pi(1 - pi)] over the opponents' spread.
    scale = 173.7178
    phi_j = 60.0 / scale
    gj = 1 / math.sqrt(1 + 3 * phi_j**2 / math.pi**2)
    phs2 = (45.0 / scale) ** 2 + 0.21436 * 0.5 * 0.09**2
    k_eff = scale * gj / (1 / phs2 + gj * gj * 0.25)
    xs = np.linspace(-8, 8, 20001) * math.hypot(75.0, 50.0)
    wts = stats.norm.pdf(xs, 0, math.hypot(75.0, 50.0))
    ee = 1 / (1 + 10 ** (xs / 400))
    pq = float(np.sum(wts * ee * (1 - ee)) / np.sum(wts))
    phi_from_k = 1 - math.log(10) / 400 * k_eff * pq
    check("N (M3): K = 11.4 from one Glicko-2 step at RD 45, E[pi(1 - pi)] = 0.235, so 1 - q K E[pi(1 - pi)] = 0.985 (the model's 0.984)",
          near(11.4, k_eff, 1) and near(0.235, pq, 3) and abs(phi_from_k - PHI) < 0.001, f"K {k_eff:.3f}, pq {pq:.4f}, phi {phi_from_k:.4f}")

    # Result 6 and the mechanism split.
    h = {x["games"]: x for x in ex["first_and_last_30"]}
    check("N decision: no-peak 0.446 [0.409, 0.483] and 0.450 at 120; 0.449 [0.412, 0.486] and 0.490 at 250",
          near(0.446, h[120]["falling"]["share"], 3) and near(0.450, h[120]["rising"]["share"], 3)
          and near(0.449, h[250]["falling"]["share"], 3) and near(0.490, h[250]["rising"]["share"], 3)
          and all(near(a, b, 3) for a, b in zip([0.409, 0.483], h[120]["falling"]["ci95"], strict=True))
          and all(near(a, b, 3) for a, b in zip([0.412, 0.486], h[250]["falling"]["ci95"], strict=True)))
    fixed = [P[60]["share"], h[120]["falling"]["share"], h[250]["falling"]["share"]]
    check("N decision: memory alone 'about 0.45 at every length' (0.443, 0.446, 0.449) and 'more than twice the hide line'",
          all(abs(v - 0.45) < 0.01 for v in fixed) and min(fixed) > 2 * HIDE)
    either = [P[60]["share"] + RS[60]["share"], h[120]["falling"]["share"] + h[120]["rising"]["share"], h[250]["falling"]["share"] + h[250]["rising"]["share"]]
    check("N decision: 'about nine in ten get a verdict either way' (0.88, 0.90, 0.94)", all(0.85 <= v <= 0.95 for v in either), ", ".join(f"{v:.3f}" for v in either))
    add = [P[120]["share"] - h[120]["falling"]["share"], P[250]["share"] - h[250]["falling"]["share"]]
    check("N decision: the peak adds 'about 0.2 to 0.3' to 'falling' (0.176 and 0.336, one decimal)", near(0.2, add[0], 1) and near(0.3, add[1], 1),
          f"{add[0]:.3f}, {add[1]:.3f}")
    # Result 6: falling against rising at fixed halves, as a test of equality given the total.
    pairs = [(P[60]["k"], RS[60]["k"]), (h[120]["falling"]["k"], h[120]["rising"]["k"]), (h[250]["falling"]["k"], h[250]["rising"]["k"]),
             (ex["games_30"]["k"], ex["rising_30"]["k"])]
    pv = [float(stats.binomtest(a, a + b, 0.5).pvalue) for a, b in pairs]
    check("N Result 6: at fixed halves falling and rising do not differ (exact binomial p > 0.05 at 30, 60, no-peak 120, 250)", min(pv) > 0.05,
          ", ".join(f"{v:.2f}" for v in pv))
    g1 = next(a for a in ex["other_ratings"] if a["arm"] == "glicko1")["cells"][0]
    kf, kr = g1["falling"]["k"], g1["rising"]["k"]
    pg1 = float(stats.binomtest(kf, kf + kr, 0.5).pvalue)
    check("N Finding 3: Glicko-1 at 60 games, 73 falling against 99 rising: exact two-sided p = 0.056 under Result 6's equality",
          (kf, kr) == (73, 99) and near(0.056, pg1, 3), f"{pg1:.4f}")

    # The confirmatory secondaries.
    check("N decision: rising 0.439 [0.394, 0.484], 0.300 [0.260, 0.343], 0.171 [0.140, 0.208]",
          all(near(q, RS[g]["share"], 3) for q, g in zip([0.439, 0.300, 0.171], CHECKS, strict=True))
          and all(near(a, b, 3) for g, q in zip(CHECKS, [[0.394, 0.484], [0.260, 0.343], [0.140, 0.208]], strict=True) for a, b in zip(q, RS[g]["ci_bonferroni"], strict=True)))
    check("N summary: rising 44% and 30% at 60 and 120", near(44, 100 * RS[60]["share"], 0) and near(30, 100 * RS[120]["share"], 0))
    acc = [SEC["accuracy", g]["share"] for g in CHECKS]
    bl = [SEC["blunders", g]["share"] for g in CHECKS]
    check("N decision: accuracy 0.113, 0.080, 0.110; blunders 0.114, 0.087, 0.086; the 60-game blunders upper end 0.14988 clears by 0.0001",
          all(near(q, v, 3) for q, v in zip([0.113, 0.080, 0.110, 0.114, 0.087, 0.086], acc + bl, strict=True))
          and near(0.14988, SEC["blunders", 60]["ci_bonferroni"][1], 5) and near(0.0001, BOUND - SEC["blunders", 60]["ci_bonferroni"][1], 4))
    want = float(stats.t.sf(Z80 * math.sqrt(29 / 30), 58))
    zs = [(v * 700 - want * 700) / math.sqrt(700 * want * (1 - want)) for v in acc + bl]
    check("N Result 12: the six rows against the i.i.d. prediction 0.106: every |z| below 3", max(abs(z) for z in zs) < 3, ", ".join(f"{z:.2f}" for z in zs))
    q_det = {(100, 60): (0.735, [0.670, 0.791]), (100, 120): (0.945, [0.904, 0.969]), (100, 250): (0.980, [0.950, 0.992]),
             (200, 60): (0.935, [0.892, 0.962]), (200, 120): (1.000, [0.981, 1.000]), (200, 250): (1.000, [0.981, 1.000])}
    ok = all(near(s, DET[k]["share"], 3) and all(near(a, b, 3) for a, b in zip(ci, DET[k]["ci95"], strict=True)) for k, (s, ci) in q_det.items())
    check("N decision: the detection table and the summary's '74% to 100%'", ok and near(74, 100 * min(dets), 0))

    # Exploratory.
    check("N decision: 30 games 0.411 falling, 0.417 rising", near(0.411, ex["games_30"]["share"], 3) and near(0.417, ex["rising_30"]["share"], 3))
    sw = [x["share"] for x in ex["score_worse"]]
    aw = {x["games"]: x["share"] for x in ex["any_worse"]}
    check("N decision: score 'worse' 0.05 to 0.10; any row 'worse' 0.60, 0.69, 0.83",
          0.05 <= min(sw) and max(sw) <= 0.10 and all(near(q, aw[g], 2) for q, g in zip([0.60, 0.69, 0.83], CHECKS, strict=True)))
    na = {x["games"]: x for x in ex["new_account"]}
    check("N decision: new accounts 0.43, 0.53, 0.60; 0.58 below and 0.26 at or above 1500 at 60",
          all(near(q, na[g]["all"]["share"], 2) for q, g in zip([0.43, 0.53, 0.60], CHECKS, strict=True))
          and near(0.58, na[60]["below_1500"]["share"], 2) and near(0.26, na[60]["at_or_above_1500"]["share"], 2))
    arms = {a["arm"]: a for a in ex["other_ratings"]}
    q_arms = {"elo/10": ([0.42, 0.58, 0.77], 2), "elo/32": ([0.415, 0.65, 0.785], 3), "glicko1": ([0.365, 0.625, 0.815], 3)}
    ok = all(near(q, c["falling"]["share"], dp) for arm, (qs, dp) in q_arms.items() for q, c in zip(qs, arms[arm]["cells"], strict=True))
    ok &= all(near(a, b, 3) for q, c in zip([[0.301, 0.434], [0.556, 0.689], [0.755, 0.863]], arms["glicko1"]["cells"], strict=True) for a, b in zip(q, c["falling"]["ci95"], strict=True))
    ok &= near(0.495, arms["glicko1"]["cells"][0]["rising"]["share"], 3)
    check("N decision: Elo K = 10 and 32 and Glicko-1 falling shares, Glicko-1's intervals and its 0.495 rising", ok)
    dec = [v["mean_error_at_start"] for v in diag["decline"].values()]
    oth = [a["settle"]["mean_error_at_start"] for a in ex["other_ratings"]]
    check("N decision: burn-in -2.5 settled, within +-3.1 for the declines, -3.9 to +4.5 for the arms, none flagged",
          near(-2.5, diag["settled"]["mean_error_at_start"], 1) and max(abs(v) for v in dec) <= 3.1 and near(3.1, max(abs(v) for v in dec), 1)
          and near(-3.9, min(oth), 1) and near(4.5, max(oth), 1)
          and not any(v["unsettled"] for v in diag["decline"].values()) and not diag["settled"]["unsettled"])

    # Result 13: the budget, from the design.
    sim = 700 * 550 + 200 * sum(300 + g for g in CHECKS) * 2 + 300 * 250 + 2 * 200 * 550 + 200 * 550
    traj = 700 * 4 + 700 * 2 + 2 * 3 * 200 + 300 * 3 + 3 * 200 * 3
    act = R["compute"]["actual"]
    check("N Result 13: 1,322,000 simulated games and 8,100 trajectories, from the design and in compute.actual",
          sim == 1_322_000 == act["simulated_games"] and traj == 8_100 == act["trajectory_players"])
    check("N decision/summary: 98 s is 0.03 CPU-hours and 2 minutes; 0.10718 rating periods a game",
          near(0.03, 98 / 3600, 2) and near(2, 98 / 60, 0) and abs(R["design"]["glicko2"]["periods_per_game"] - 0.21436 / 2) < 1e-12)

    # Results 9 and 10: power.json against the exact formulas, and the preregistration's figures.
    worst = 0.0
    for row in PJ["grid"]:
        n, p = row["players"], row["true_rate_all_cells"]
        ck, cb, chh = cuts(n)
        keep = stats.binom.cdf(ck, n, p) ** 3
        hide = 1 - stats.binom.cdf(chh - 1, n, p) ** 3
        chg = 1 - stats.binom.cdf(cb - 1, n, p) ** 3 - hide
        for sim_v, ex_v in ((row["keep"], keep), (row["change"], chg), (row["hide"], hide)):
            sd = math.sqrt(max(ex_v * (1 - ex_v), 1e-9) / PJ["sims"])
            worst = max(worst, abs(sim_v - ex_v) / sd if ex_v * (1 - ex_v) > 1e-6 else abs(sim_v - ex_v) * 1e4)
    for row in PJ["one_cell"]:
        n, p = row["players"], row["one_cell_rate"]
        ck, cb, chh = cuts(n)
        ch_ = 1 - stats.binom.cdf(cb - 1, n, p) * stats.binom.cdf(cb - 1, n, 0.10) ** 2
        hd = 1 - stats.binom.cdf(chh - 1, n, p) * stats.binom.cdf(chh - 1, n, 0.10) ** 2
        for sim_v, ex_v in ((row["change_or_hide"], ch_), (row["hide"], hd)):
            sd = math.sqrt(max(ex_v * (1 - ex_v), 1e-9) / PJ["sims"])
            worst = max(worst, abs(sim_v - ex_v) / sd if ex_v * (1 - ex_v) > 1e-6 else abs(sim_v - ex_v) * 1e4)
    n_vals = 3 * len(PJ["grid"]) + 2 * len(PJ["one_cell"])
    check("N Result 10: every power.json point within 4 Monte Carlo SE of the exact independent-cells formula (357 values, largest |z| 3.00)",
          worst < 4 and n_vals == 357 and near(3.00, worst, 2), f"{n_vals} values, largest |z| {worst:.2f}")
    hw = {n: float((wilson(0.1 * int(n), int(n), LEVEL)[1] - wilson(0.1 * int(n), int(n), LEVEL)[0]) / 2) for n in PJ["half_width_at_0_10"]}
    check("N power.json half_width_at_0_10 recomputed; 0.027 at 700", all(near(PJ["half_width_at_0_10"][n], v, 4) for n, v in hw.items()) and near(0.027, hw["700"], 3))

    def keep(n, p):
        return float(stats.binom.cdf(cuts(n)[0], n, p))

    k700, k600 = keep(700, 0.10) ** 3, keep(600, 0.10) ** 3
    check("N prereg section 6: keep 0.83 at 700 (0.73 at 600) and 0.08 at 0.12", near(0.83, k700, 2) and near(0.73, k600, 2) and near(0.08, keep(700, 0.12) ** 3, 2))
    one20 = float(stats.binom.sf(cuts(700)[1] - 1, 700, 0.20))
    hide30 = float(stats.binom.sf(cuts(700)[2] - 1, 700, 0.30))
    h25_1 = float(stats.binom.sf(cuts(700)[2] - 1, 700, 0.25))
    check("N prereg section 6: change-or-hide 0.88 (one cell at 0.20), hide 1.00 (one cell at 0.30), hide 0.99 (0.25 in all three)",
          near(0.88, 1 - (1 - one20) * stats.binom.cdf(cuts(700)[1] - 1, 700, 0.1) ** 2, 2) and near(1.00, hide30, 2) and near(0.99, 1 - (1 - h25_1) ** 3, 2))
    smallest = next(n for n in range(200, 801, 100)
                    if keep(n, 0.10) ** 3 >= 0.80
                    and 1 - (1 - stats.binom.sf(cuts(n)[1] - 1, n, 0.20)) * stats.binom.cdf(cuts(n)[1] - 1, n, 0.1) ** 2 >= 0.80)
    check("N prereg section 6: 700 is the smallest of 200..800 where every power reaches 0.80", smallest == 700)
    h200 = 1 - stats.binom.cdf(cuts(200)[2] - 1, 200, 0.30) ** 3
    check("N prereg section 6: 200 players decide 'hide' with power 1.00 at 0.30 (0.996, independent cells)", near(1.00, h200, 2), f"{h200:.4f}")
    k = np.arange(201)
    lo, hi = wilson(k, 200, 1 - 0.05 / 6)
    zw = float(stats.norm.ppf(1 - 0.05 / 12))
    check("N prereg section 6: the detection half-width is at most 0.093 (Wilson max 0.0917; Wald at 1/2 0.0933)",
          float(((hi - lo) / 2).max()) <= 0.093 and near(0.093, zw * math.sqrt(0.25 / 200), 3))
    p1 = keep(700, 0.10)
    check("N Finding 1: 'keep' at 0.10 lies in [0.819, 0.940] under any dependence of the three cells (Frechet)",
          near(0.819, 1 - 3 * (1 - p1), 3) and near(0.940, p1, 3), f"P1 = {p1:.4f}")
    check("N Finding 1: 'hide' at 0.25 in all three is 0.991 independent but only >= 0.796 under any dependence", near(0.991, 1 - (1 - h25_1) ** 3, 3) and near(0.796, h25_1, 3))
    # Result 9: the exact one-sided sizes and the family bounds.
    sz_h = float(stats.binom.sf(c_hide - 1, 700, HIDE))
    sz_b = float(stats.binom.sf(c_bound - 1, 700, BOUND))
    sz_k = float(stats.binom.cdf(c_keep, 700, BOUND))
    check("N Result 9: exact sizes per cell 0.00894 (hide | 0.20), 0.00992 (above the bound | 0.15), 0.00722 (keep | 0.15); nominal 0.00833",
          near(0.00894, sz_h, 5) and near(0.00992, sz_b, 5) and near(0.00722, sz_k, 5))
    check("N Result 9: family bounds 3 x 0.00894 = 0.0268 and 3 x 0.00992 = 0.0298, both below 0.05", near(0.0268, 3 * sz_h, 4) and near(0.0298, 3 * sz_b, 4))
    cov = []
    for p in (0.15, 0.20):
        kk = np.arange(701)
        lo, hi = wilson(kk, 700, LEVEL)
        cov.append(float(stats.binom.pmf(kk, 700, p)[(lo <= p) & (p <= hi)].sum()))
    check("N Result 9: Wilson's exact coverage at 700 players is 0.9829 (p = 0.15) and 0.9841 (p = 0.20), nominal 0.9833",
          near(0.9829, cov[0], 4) and near(0.9841, cov[1], 4), ", ".join(f"{v:.5f}" for v in cov))
    ends = [float(wilson(82, 700, LEVEL)[1]), float(wilson(83, 700, LEVEL)[1]), float(wilson(127, 700, LEVEL)[0]),
            float(wilson(128, 700, LEVEL)[0]), float(wilson(165, 700, LEVEL)[0]), float(wilson(166, 700, LEVEL)[0])]
    check("N Result 8: the edge values u(82) 0.1494, u(83) 0.1510, l(127) 0.1492, l(128) 0.1505, l(165) 0.1996, l(166) 0.2009",
          all(near(q, v, 4) for q, v in zip([0.1494, 0.1510, 0.1492, 0.1505, 0.1996, 0.2009], ends, strict=True)))
    vfix = SIGMA**2 * (2 * block(30, PHI) - 2 * cross(30, 30, 1, PHI)) / 900
    check("N Result 3: S(30) = 771.0 and SD(D) = 20.76 at 60 games (phi 0.984, sigma 30.74)",
          near(771.0, block(30, PHI), 1) and near(20.76, math.sqrt(vfix), 2))
    b60 = float(stats.norm.cdf(-iv["60"]["half_width_mean"] / iv["60"]["diff_sd"]))
    b30 = float(stats.norm.cdf(-iv["30"]["half_width_mean"] / iv["30"]["diff_sd"]))
    check("N Result 5: the bounds 0.429 (60 games) and 0.409 (30) below the measured 0.443 and 0.411; 2 x 0.429 = 86% against the measured 88%",
          near(0.429, b60, 3) and near(0.409, b30, 3) and b60 < P[60]["share"] and b30 < ex["games_30"]["share"]
          and near(86, 200 * b60, 0) and near(88, 100 * (P[60]["share"] + RS[60]["share"]), 0))
    hh = {x["games"]: x for x in ex["first_and_last_30"]}
    cnt = [(ex["games_30"]["k"], ex["rising_30"]["k"]), (P[60]["k"], RS[60]["k"]),
           (hh[120]["falling"]["k"], hh[120]["rising"]["k"]), (hh[250]["falling"]["k"], hh[250]["rising"]["k"])]
    check("N Result 6: the counts 288/292, 310/307, 312/315, 314/343", cnt == [(288, 292), (310, 307), (312, 315), (314, 343)])
    check("N Finding 2: SD/half-width at 60 games is 5.6, not a half-width ratio", near(5.6, iv["60"]["diff_sd"] / iv["60"]["half_width_mean"], 1))
    kk = np.arange(201)
    lw, hw_ = wilson(kk, 200, 1 - 0.05 / 6)
    check("N section 5: the largest Bonferroni(6) Wilson half-width at 200 players is 0.0917", near(0.0917, float(((hw_ - lw) / 2).max()), 4))
    rd = diag["settled_rd_mean"]
    check("N decision: 'the settled deviation sits at lila's floor of 45 throughout': RD >= 45 always, and its mean 45.0003 leaves an average excess of 0.0003",
          0 <= rd - 45 < 0.001, f"{rd}")
    v_mean = SIGMA**2 * block(250, PHI) / 250**2
    z_err = diag["settled_error_mean"] / math.sqrt(v_mean / 700)
    check("N section 6: the settled mean error -1.75 is z = -2.5 under the AR(1) (symmetric world, mean 0): within 3",
          abs(z_err) < 3 and near(-2.5, z_err, 1), f"z {z_err:.2f}")
    check("N Result 9: P(Bin(700, 0.20) >= 310) < 1e-25", float(stats.binom.sf(309, 700, 0.20)) < 1e-25)
    bb = {x["games"]: x for x in R["by_basis"]}
    check("N Result 1 (iv): 287 of the 310 falling at 60 games have basis 'peak'",
          bb[60]["peak"]["falling"] == 287 and bb[60]["peak"]["falling"] + bb[60]["start"]["falling"] == 310)
    fl = R["flags"]["false_alarm"]
    X = np.array([[int(ch_) for ch_ in fl[str(g)]] for g in CHECKS])
    cc = np.corrcoef(X)
    rr = [cc[0, 1], cc[0, 2], cc[1, 2]]
    p_all = float(np.mean(X.all(0)))
    p_ind = float(np.prod(X.mean(1)))
    check("N Finding 1: P(all three) 0.217 against 0.216 under independence", near(0.217, p_all, 3) and near(0.216, p_ind, 3), f"{p_all:.4f}, {p_ind:.4f}")
    check("N Finding 1: the per-player flags at 60, 120, 250 are nearly uncorrelated (0.008, 0.020, -0.058; each |r| < 2/sqrt(700))",
          all(abs(v) < 2 / math.sqrt(700) for v in rr) and [int(s) for s in X.sum(1)] == [310, 435, 549], ", ".join(f"{v:.3f}" for v in rr))


if __name__ == "__main__":
    symbolic()
    simulation()
    numbers()
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem} (lab/lean/)")
    if FAILURES:
        print(f"{len(FAILURES)} FAIL: {', '.join(FAILURES)}")
        sys.exit(1)
    print(f"{COUNT[0]} PASS, 0 FAIL")
