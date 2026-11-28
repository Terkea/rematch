"""EXP-0007: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy, prefix R): the identities of math.md section 4, each as
   `simplify(lhs - rhs) == 0`, and the inequalities on a dense grid where sympy cannot settle
   them (the line says so).
2. Simulation (numpy, fixed seeds, prefix S): a finite population sampled by stratified simple
   random sampling without replacement (Results 1 to 5 and 8), the Wilson rule on a known
   effective size (Results 10 and 11), and the power simulation of preregistration section 6,
   rewritten here from the text of section 6, against power.json (Result 14).
3. Numbers (prefix N): every number decision.md, summary.md and the preregistration quote,
   recomputed from strata.csv, sample.csv, pilot/pilot.json, results.json and power.json.

The formulas are written here from math.md section 3, not imported from analysis.py.
Standard library, numpy, scipy and sympy only. About 15 s of CPU.
"""

from __future__ import annotations

import csv
import json
import math
import sys
from collections import Counter
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import stats

HERE = Path(__file__).resolve().parent
R = json.loads((HERE / "results.json").read_text(encoding="utf-8"))
PJ = json.loads((HERE / "power.json").read_text(encoding="utf-8"))
PILOT = json.loads((HERE / "pilot" / "pilot.json").read_text(encoding="utf-8"))
with (HERE / "strata.csv").open(encoding="utf-8", newline="") as fh:
    STRATA = {(r["stratum"], r["band"]): {k: int(v) for k, v in r.items() if k not in ("stratum", "band")}
              for r in csv.DictReader(fh)}
with (HERE / "sample.csv").open(encoding="utf-8", newline="") as fh:
    SAMPLE = list(csv.DictReader(fh))

AUDIENCE = ("A1", "A2", "A3", "A4")
SCORED = ("hangingPiece", "fork", "pin", "skewer", "discoveredAttack", "deflection", "sacrifice",
          "promotion", "mate", "mateIn1", "mateIn2", "mateIn3", "mateIn4Plus", "backRankMate",
          "smotheredMate")
MATE_LENGTH = ("mate", "mateIn1", "mateIn2", "mateIn3", "mateIn4Plus")
LABEL_LIMITED = ("hangingPiece", "fork", "pin", "skewer", "sacrifice", "backRankMate")
FAMILY = 30
ALPHA = 0.05
Z_DEC = float(stats.norm.ppf(1 - ALPHA / (2 * FAMILY)))
Z80, Z95 = float(stats.norm.ppf(0.90)), float(stats.norm.ppf(0.975))
C_PREC, C_REC = 0.80, 0.50
# Theorem names in lab/lean/Rematch/ (Multiplicity.lean) whose hypotheses match Result 12.
LEAN = {"Result 12 (Bonferroni over the 30 intervals)": "Rematch.union_bound, Rematch.bonferroni"}

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


# ---- the formulas of math.md section 3, written here -----------------------------------------


def alloc(n_pop: int, m: int, f: float) -> int:
    """(E1) n_h = min(N_h, max(m, ceil(f N_h)))."""
    return min(n_pop, max(m, math.ceil(f * n_pop)))


def wilson(p, n, z):
    """(E6) Wilson's interval at proportion p and size n, clipped to [0, 1]."""
    p, n = np.asarray(p, float), np.asarray(n, float)
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * np.sqrt(np.maximum(p * (1 - p), 0) / n + z * z / (4 * n * n)) / d
    return np.maximum(0.0, c - h), np.minimum(1.0, c + h)


def cell_s2(n11, n10, n, r):
    """(E4) the sample variance of d = y - r x in a cell of n sampled units, n11 with x = y = 1
    and n10 with x = 1, y = 0 (d = 1 - r, -r, 0)."""
    tot = n11 * (1 - r) - n10 * r
    sq = n11 * (1 - r) ** 2 + n10 * r * r
    return np.where(n > 1, (sq - tot * tot / np.maximum(n, 1)) / np.maximum(n - 1, 1), 0.0)


def ratio_from_counts(n11, n10, n, pop):
    """(E2) to (E5) from per-cell counts, arrays of shape (reps, cells): R-hat, the linearised
    variance, its effective size, and Kish's effective size."""
    w = pop / n
    big_y = (w * n11).sum(-1)
    big_x = (w * (n11 + n10)).sum(-1)
    r = np.where(big_x > 0, big_y / np.where(big_x > 0, big_x, 1), 0.0)
    s2 = cell_s2(n11, n10, n, r[..., None])
    fpc = 1 - n / pop
    var = (pop**2 * fpc * s2 / n).sum(-1) / np.where(big_x > 0, big_x, 1) ** 2
    kish = np.where(big_x > 0, big_x**2 / np.maximum((w * w * (n11 + n10)).sum(-1), 1e-300), 0.0)
    lin = np.where((var > 0) & (r > 0) & (r < 1), r * (1 - r) / np.maximum(var, 1e-300), kish)
    return r, var, lin, kish, big_x, big_y


# ---- 1. symbolic -------------------------------------------------------------------------------


def symbolic() -> None:
    N, n = sp.symbols("N n", positive=True, integer=True)
    f = n / N
    # Result 1: the inclusion probability of SRSWOR, C(N-1, n-1)/C(N, n) = n/N.
    incl = sp.binomial(N - 1, n - 1) / sp.binomial(N, n)
    check("R Result 1: P(unit in an SRSWOR sample) = C(N-1,n-1)/C(N,n) = n/N",
          zero(sp.combsimp(incl) - n / N))
    # Result 2: the joint inclusion and the covariance of two indicators.
    joint = sp.binomial(N - 2, n - 2) / sp.binomial(N, n)
    pij = n * (n - 1) / (N * (N - 1))
    cov = pij - (n / N) ** 2
    ok = zero(sp.combsimp(joint) - pij) and zero(cov + f * (1 - f) / (N - 1))
    check("R Result 2: pi_ij = n(n-1)/(N(N-1)) and Cov(I_i, I_j) = -f(1-f)/(N-1)", ok)
    # Result 2: Var(sum_i I_i u_i) for a population of N = 5, exactly, against the formula.
    us = sp.symbols("u1:6")
    Nn, nn = 5, 2
    tot = sum(us)
    S2 = sum((u - tot / Nn) ** 2 for u in us) / (Nn - 1)
    from itertools import combinations
    samples = list(combinations(range(Nn), nn))
    est = [sp.Rational(Nn, nn) * sum(us[i] for i in s) for s in samples]
    mean_est = sum(est) / len(samples)
    var_est = sum((e - mean_est) ** 2 for e in est) / len(samples)
    want = sp.Integer(Nn) ** 2 * (1 - sp.Rational(nn, Nn)) * S2 / nn
    s2_samples = [sum((us[i] - sum(us[j] for j in s) / nn) ** 2 for i in s) / (nn - 1) for s in samples]
    ok = zero(mean_est - tot) and zero(var_est - want) and zero(sum(s2_samples) / len(samples) - S2)
    check("R Results 1, 2: N=5, n=2, all 10 samples: E[HT] = U, Var = N^2(1-f)S^2/n, E[s^2] = S^2 (exact in u)", ok)
    # Result 3: E[R X] = Y gives the bias identity; the Hartley-Ross bound is Cauchy-Schwarz.
    Rh, Xh, Rr, X = sp.symbols("Rhat Xhat R X")
    ERX, ER, EX, cv = sp.symbols("E_RX E_R E_X Cov")
    # Cov(R, X) = E[RX] - E[R]E[X], E[RX] = Y = R X, E[X] = X  =>  E[R] - R = -Cov/X.
    expr = (ER - Rr) - (-(ERX - ER * EX) / X)
    check("R Result 3 (definitional; S Result 3 tests the premise): E[R-hat] - R = -Cov(R-hat, X-hat)/X when E[R-hat X-hat] = R X and E[X-hat] = X",
          zero(expr.subs({ERX: Rr * X, EX: X})))
    # Result 4: R-hat - R = D-hat / X-hat exactly, and the remainder of the first-order term.
    Yh, Dh = sp.symbols("Yhat Dhat")
    ok = zero((Yh / Xh - Rr) - (Yh - Rr * Xh) / Xh)
    ok &= zero(((Yh - Rr * Xh) / Xh - (Yh - Rr * Xh) / X) - (-(Yh - Rr * Xh) * (Xh - X) / (X * Xh)))
    check("R Result 4: R-hat - R = D-hat/X-hat; D-hat/X-hat - D-hat/X = -D-hat (X-hat - X)/(X X-hat)", ok)
    # Result 4 (E4): the three-valued d's sample variance, closed form against the definition.
    n11, n10, n00 = sp.symbols("n11 n10 n00", positive=True)
    r = sp.symbols("r")
    nh = n11 + n10 + n00
    mean = (n11 * (1 - r) - n10 * r) / nh
    direct = (n11 * (1 - r - mean) ** 2 + n10 * (-r - mean) ** 2 + n00 * mean**2) / (nh - 1)
    closed = (n11 * (1 - r) ** 2 + n10 * r**2 - (n11 * (1 - r) - n10 * r) ** 2 / nh) / (nh - 1)
    check("R Result 4: s_h^2 of d in {1-r, -r, 0} equals the closed form of (E4) (analysis.py neff_np)",
          zero(direct - closed))
    # Result 5: when every tagged unit of a cell is a true positive (n10 = 0).
    x_bar, hh = sp.symbols("xbar h", positive=True)
    closed_h = (n11 * (1 - r) ** 2 + n10 * r**2 - (n11 * (1 - r) - n10 * r) ** 2 / hh) / (hh - 1)
    s2_tp = closed_h.subs({n10: 0, n11: x_bar * hh})
    want = hh / (hh - 1) * x_bar * (1 - x_bar) * (1 - r) ** 2
    check("R Result 5: with n10 = 0, s_h^2 = n_h/(n_h-1) xbar (1-xbar)(1-r)^2 (zero iff xbar in {0,1} or r = 1)",
          zero(s2_tp - want))
    # Result 6 (illustrative case): unit-level data, x = 1 throughout, every cell's sample share
    # of y equal to p. Kish's p(1-p)/n_K from the units' weights, against (E4) from the units' d,
    # with the factors (1 - f_h) and n_h/(n_h - 1) taken out. Exact rationals.
    ok = True
    rng0 = np.random.default_rng(6)
    for _ in range(20):
        H = int(rng0.integers(2, 6))
        den = int(rng0.integers(2, 6))
        p = sp.Rational(int(rng0.integers(1, den)), den)
        cells_ = []
        for _h in range(H):
            nh = den * int(rng0.integers(1, 5))
            Nh = nh + int(rng0.integers(0, 400))
            ys = [1] * int(p * nh) + [0] * (nh - int(p * nh))
            cells_.append((Nh, ys))
        ws = [sp.Rational(Nh, len(ys)) for Nh, ys in cells_ for _y in ys]
        kish = sum(ws) ** 2 / sum(w * w for w in ws)
        Xt = sum(Nh for Nh, _ in cells_)
        lin = 0
        for Nh, ys in cells_:
            nh = len(ys)
            d = [y - p for y in ys]
            s2 = sum((v - sum(d) / nh) ** 2 for v in d) / (nh - 1)
            lin += sp.Integer(Nh) ** 2 * (s2 * sp.Rational(nh - 1, nh)) / nh  # (E4) with both factors removed
        ok &= zero(p * (1 - p) / kish - lin / Xt**2)
    check("R Result 6: 20 random unit-level samples (x = 1, equal cell shares): p(1-p)/n_K = (E4) without (1 - f_h) and n_h/(n_h - 1), exactly", ok)
    # Result 6: Kish's n_K <= n (Cauchy-Schwarz), on a grid of random weights.
    rng = np.random.default_rng(7)
    worst = 0.0
    for _ in range(2000):
        w = rng.exponential(1, size=rng.integers(1, 50)) ** rng.uniform(0, 4)
        worst = max(worst, w.sum() ** 2 / (w * w).sum() / len(w))
    check("R Result 6: (sum w)^2 / sum w^2 <= n, 2,000 random weight vectors (grid)", worst <= 1 + 1e-12, f"max ratio {worst:.6f}")
    # Result 7: the Wilson bounds are the roots of (p-hat - p)^2 = z^2 p (1 - p)/n.
    ph, z, nn_ = sp.symbols("phat z n", positive=True)
    pp = sp.symbols("p")
    roots = sp.solve(sp.Eq((ph - pp) ** 2, z**2 * pp * (1 - pp) / nn_), pp)
    d = 1 + z**2 / nn_
    c = (ph + z**2 / (2 * nn_)) / d
    h = z * sp.sqrt(ph * (1 - ph) / nn_ + z**2 / (4 * nn_**2)) / d
    ok = len(roots) == 2 and all(any(zero(rt - cand) for cand in (c - h, c + h)) for rt in roots)
    # At p-hat = 1 the lower bound is n/(n + z^2); the centre minus p-hat is z^2 (1/2 - p-hat)/(n + z^2).
    ok &= zero((c - h).subs(ph, 1) - nn_ / (nn_ + z**2))
    ok &= zero(c - ph - z**2 * (sp.Rational(1, 2) - ph) / (nn_ + z**2))
    check("R Result 7: Wilson's ends solve the score equation; lower end n/(n+z^2) at p-hat = 1; centre shift z^2(1/2 - p-hat)/(n+z^2)", ok)
    # Result 7: the half-width over z sqrt(p(1-p)/n) tends to 1 (n -> oo), for fixed p-hat in (0,1).
    lim = sp.limit((h / (z * sp.sqrt(ph * (1 - ph) / nn_))).subs({ph: sp.Rational(3, 10), z: 3}), nn_, sp.oo)
    check("R Result 7: Wilson half-width / Wald half-width -> 1 as n -> oo", lim == 1)
    # Result 9: scaling the false-positive rates by k = TP (1/pi - 1) / FP gives precision pi.
    TPm, FPm, pi_ = sp.symbols("TP FP pi", positive=True)
    k = TPm * (1 / pi_ - 1) / FPm
    check("R Result 9: TP / (TP + k FP) = pi with k = TP(1/pi - 1)/FP (section 6's scenarios, uncapped)",
          zero(TPm / (TPm + k * FPm) - pi_))
    # Result 10: L >= c  <=>  p-hat >= c + z sqrt(c(1-c)/n), on a dense grid of p-hat, n, c, z.
    bad = 0
    for cc in (0.5, 0.8):
        for zz in (Z80, Z95, Z_DEC):
            for nv in (10.0, 50.0, 228.0, 1000.0, 5127.2):
                pg = np.linspace(0, 1, 20001)
                lo, _ = wilson(pg, nv, zz)
                thr = cc + zz * math.sqrt(cc * (1 - cc) / nv)
                # away from the threshold itself (floating point at equality), the two must agree
                far = np.abs(pg - thr) > 1e-9
                bad += int(((lo >= cc) != (pg >= thr))[far].sum())
    check("R Result 10: Wilson lower end >= c iff p-hat >= c + z sqrt(c(1-c)/n) (grid of 20,001 x 30)", bad == 0, f"{bad} mismatches")
    # Result 10: the lower end rises with p-hat (monotone), on the same grid.
    mono = True
    for nv in (10.0, 228.0, 5000.0):
        lo, hi = wilson(np.linspace(0, 1, 20001), nv, Z_DEC)
        mono &= bool(np.all(np.diff(lo) >= -1e-15) and np.all(np.diff(hi) >= -1e-15))
    check("R Result 10: both Wilson ends are non-decreasing in p-hat (grid)", mono)
    # Result 11: the power formula and the sample size that gives 80%.
    c1, p1, za, zb, ne = sp.symbols("c p1 z_a z_b n_e", positive=True)
    n_req = ((za * sp.sqrt(c1 * (1 - c1)) + zb * sp.sqrt(p1 * (1 - p1))) / (p1 - c1)) ** 2
    arg = ((p1 - c1) * sp.sqrt(ne) - za * sp.sqrt(c1 * (1 - c1))) / sp.sqrt(p1 * (1 - p1))
    check("R Result 11: at n_e = n_req the power argument equals z_b", zero(sp.simplify(arg.subs(ne, n_req) - zb).subs({c1: sp.Rational(4, 5), p1: sp.Rational(9, 10)})) and zero(sp.simplify(arg.subs(ne, n_req) - zb).subs({c1: sp.Rational(1, 2), p1: sp.Rational(13, 20)})))
    # Result 12: the Bonferroni z.
    check("R Result 12: z = Phi^-1(1 - 0.05/60) = 3.1440", near(3.144, Z_DEC, 3) and near(3.14398, Z_DEC, 5), f"{Z_DEC:.5f}")
    # Result 13: the weight bound w <= 1/f, for every N up to 300,000 at the chosen design and others.
    ok = True
    for m, ff in ((50, 0.03), (300, 0.02), (50, 0.005)):
        Nv = np.arange(1, 300_001)
        nv = np.minimum(Nv, np.maximum(m, np.ceil(ff * Nv)))
        ok &= bool(np.all(Nv / nv <= 1 / ff + 1e-9)) and bool(np.all(nv >= np.minimum(Nv, m)))
    check("R Result 13: N/n <= 1/f and n >= min(N, m) for every N <= 300,000 (three designs, exhaustive)", ok)
    # Result 15: precision against labels <= precision against truth + label-error share.
    pT, pl, pe = sp.symbols("P_T P_l P_err", nonnegative=True)
    # P(l | x) = P(l & T | x) + P(l & not T | x) <= P(T | x) + P(err | x)
    check("R Result 15 (definitional): P(l|x) <= P(T|x) + P(l,~T|x); with no label error P(l|x) <= P(T|x), sometimes strictly (10^5 joint laws)",
          _label_bound_grid())
    # Result 14: with the estimated SD fixed, coverage 2 Phi(z sigma_hat / sigma) - 1 falls as sigma grows.
    sig_hat, sig = sp.symbols("sigma_hat sigma", positive=True)
    cov_expr = sp.erf(z * sig_hat / (sig * sp.sqrt(2)))
    dcov = sp.simplify(sp.diff(cov_expr, sig))
    want_d = -sp.sqrt(2) * z * sig_hat * sp.exp(-(z * sig_hat) ** 2 / (2 * sig**2)) / (sp.sqrt(sp.pi) * sig**2)
    check("R Result 14: d/d sigma [2 Phi(z sigma_hat/sigma) - 1] = -sqrt(2/pi) z sigma_hat exp(-z^2 sigma_hat^2/(2 sigma^2))/sigma^2 < 0",
          zero(dcov - want_d))


def _label_bound_grid() -> bool:
    rng = np.random.default_rng(15)
    # joint law over (l, T) given x: four cells
    q = rng.dirichlet(np.ones(4), size=100_000)  # (l&T, l&~T, ~l&T, ~l&~T)
    p_l = q[:, 0] + q[:, 1]
    p_t = q[:, 0] + q[:, 2]
    err = q[:, 1]
    ok = bool(np.all(p_l <= p_t + err + 1e-12))
    # with no label error (err = 0) precision against the labels is a lower bound, and can be strict
    q0 = q.copy()
    q0[:, 1] = 0
    q0 /= q0.sum(1, keepdims=True)
    ok &= bool(np.all(q0[:, 0] <= q0[:, 0] + q0[:, 2])) and bool(np.any(q0[:, 0] < q0[:, 0] + q0[:, 2] - 0.1))
    return ok


# ---- 2. simulation -----------------------------------------------------------------------------


def simulation() -> None:
    sim_finite_population()
    sim_wilson_rule()
    sim_power_json()


def make_population(rng):
    """A finite population of cells shaped like EXP-0007's: a few census-sized label cells, mid
    cells and a huge 'none' cell. Each cell holds counts of (x=1,y=1), (x=1,y=0), (x=0)."""
    sizes = np.array([40, 75, 160, 900, 4000, 60000])
    lab = np.array([1.0, 1.0, 0.9, 0.6, 0.3, 0.0])  # label share
    tpr = np.array([0.95, 0.7, 0.8, 0.75, 0.6, 0.0])  # P(tag | label)
    fpr = np.array([0.0, 0.05, 0.1, 0.1, 0.05, 0.003])  # P(tag | no label)
    n11 = rng.binomial(rng.binomial(sizes, lab), tpr)
    labelled = (sizes * lab).round().astype(int)
    n11 = np.minimum(n11, labelled)
    n10 = rng.binomial(sizes - labelled, fpr)
    return sizes, n11, n10


def sim_finite_population(seed: int = 20261128, reps: int = 40_000) -> None:
    rng = np.random.default_rng(seed)
    sizes, P11, P10 = make_population(rng)
    P00 = sizes - P11 - P10
    m, f = 30, 0.03
    n = np.array([alloc(int(N), m, f) for N in sizes])
    big_x, big_y = float((P11 + P10).sum()), float(P11.sum())
    r_true = big_y / big_x
    # Stratified SRSWOR: the sample's type counts are multivariate hypergeometric in each cell.
    s11 = np.empty((reps, len(sizes)))
    s10 = np.empty((reps, len(sizes)))
    for h in range(len(sizes)):
        draw = rng.multivariate_hypergeometric([int(P11[h]), int(P10[h]), int(P00[h])], int(n[h]), size=reps)
        s11[:, h], s10[:, h] = draw[:, 0], draw[:, 1]
    w = sizes / n
    yhat = (w * s11).sum(1)
    xhat = (w * (s11 + s10)).sum(1)
    # Result 1: HT totals unbiased.
    ok = abs(yhat.mean() - big_y) < 4 * yhat.std(ddof=1) / math.sqrt(reps)
    ok &= abs(xhat.mean() - big_x) < 4 * xhat.std(ddof=1) / math.sqrt(reps)
    check("S Result 1: E[Y-hat] = Y and E[X-hat] = X under stratified SRSWOR (finite population, 6 cells)", bool(ok),
          f"Y {big_y:.0f} vs {yhat.mean():.1f}, X {big_x:.0f} vs {xhat.mean():.1f}")
    # Result 2: Var of X-hat against sum N^2 (1-f) S^2 / n.
    xs_pop = []
    want = 0.0
    for h in range(len(sizes)):
        Nh = sizes[h]
        k = P11[h] + P10[h]
        S2 = k * (Nh - k) / (Nh * (Nh - 1))  # population variance of a 0/1 variable, N-1 divisor
        want += Nh**2 * (1 - n[h] / Nh) * S2 / n[h]
    dev2 = (xhat - xhat.mean()) ** 2
    se_var = dev2.std(ddof=1) / math.sqrt(reps)
    check("S Result 2: Var(X-hat) = sum N_h^2 (1 - f_h) S_h^2 / n_h within 4 SE", abs(xhat.var(ddof=1) - want) < 4 * se_var,
          f"{xhat.var(ddof=1):.1f} vs {want:.1f}")
    r, var, lin, kish, bx, by = ratio_from_counts(s11, s10, n.astype(float), sizes.astype(float))
    # Result 3: E[R X-hat] = Y (so the exact bias identity), and the Hartley-Ross bound.
    rx = r * xhat
    check("S Result 3: E[R-hat X-hat] = Y within 4 SE (the bias identity's premise)",
          abs(rx.mean() - big_y) < 4 * rx.std(ddof=1) / math.sqrt(reps))
    bias = r.mean() - r_true
    hr = r.std(ddof=1) * xhat.std(ddof=1) / big_x
    ident = -np.cov(r, xhat)[0, 1] / big_x
    se_bias = r.std(ddof=1) / math.sqrt(reps)
    check("S Result 3: |bias| <= SD(R-hat) CV(X-hat) and bias = -Cov(R-hat, X-hat)/X within 4 SE",
          abs(bias) <= hr + 4 * se_bias and abs(bias - ident) < 4 * se_bias, f"bias {bias:.5f}, bound {hr:.5f}, identity {ident:.5f}")
    # Result 4: the linearised variance against the true variance of R-hat (approximate, 10%).
    ratio = var.mean() / r.var(ddof=1)
    check("S Result 4: E[v(R-hat)] / Var(R-hat) within 0.90 to 1.10 (first-order, n about 2,900)", 0.90 < ratio < 1.10, f"{ratio:.4f}")
    # Result 6: Kish's n_K never exceeds the count of tagged units.
    cnt = (s11 + s10).sum(1)
    check("S Result 6: n_Kish <= n_x in every replicate", bool(np.all(kish <= cnt + 1e-9)), f"max n_K/n {np.max(kish / cnt):.4f}")
    # Result 5: the linearised effective size can exceed the count (census and pure cells).
    # Result 5: a hangingPiece-like population (the tag fires on every labelled puzzle, rarely on
    # the rest): the linearised n_eff then exceeds the count of tagged puzzles.
    sz = np.array([400, 3000, 60000])
    q11 = np.array([400, 2997, 0])
    q10 = np.array([0, 0, 1500])
    nq = np.array([alloc(int(v), 30, 0.03) for v in sz])
    t11 = np.empty((reps // 4, 3))
    t10 = np.empty((reps // 4, 3))
    for h in range(3):
        dd = rng.multivariate_hypergeometric([int(q11[h]), int(q10[h]), int(sz[h] - q11[h] - q10[h])], int(nq[h]), size=reps // 4)
        t11[:, h], t10[:, h] = dd[:, 0], dd[:, 1]
    _, _, lin2, kish2, _, _ = ratio_from_counts(t11, t10, nq.astype(float), sz.astype(float))
    cnt2 = (t11 + t10).sum(1)
    check("S Result 5: with pure label cells the linearised n_eff exceeds the tagged count (Kish's never does)",
          bool(np.mean(lin2 > cnt2) > 0.99 and np.all(kish2 <= cnt2 + 1e-9)),
          f"share {np.mean(lin2 > cnt2):.3f}, median n_eff/n {np.median(lin2 / cnt2):.2f}")
    # Result 8: the 95% Wilson interval at the linearised n_eff covers R about 95% (approximate).
    lo, hi = wilson(r, lin, Z95)
    cov95 = float(np.mean((lo <= r_true) & (r_true <= hi)))
    lo, hi = wilson(r, lin, Z_DEC)
    miss_lo = float(np.mean(lo > r_true))
    check("S Result 8: 95% coverage within 0.015 of 0.95, and the one-sided Bonferroni miss within 4 SE of at most 0.00083",
          abs(cov95 - 0.95) < 0.015 and miss_lo < ALPHA / 60 + 4 * math.sqrt(ALPHA / 60 / reps), f"coverage {cov95:.4f}, lower miss {miss_lo:.5f} (nominal {ALPHA / 60:.5f})")


def sim_wilson_rule(seed: int = 11, reps: int = 2_000_000) -> None:
    rng = np.random.default_rng(seed)
    # Result 10: false-show rate at a true precision of exactly c, binomial with n_e = 228 and 2,000.
    for ne in (228, 2000):
        k = rng.binomial(ne, C_PREC, size=reps)
        lo, _ = wilson(k / ne, ne, Z_DEC)
        rate = float(np.mean(lo >= C_PREC))
        nominal = ALPHA / 60
        exact = float(stats.binom.sf(math.ceil(ne * (C_PREC + Z_DEC * math.sqrt(C_PREC * (1 - C_PREC) / ne))) - 1, ne, C_PREC))
        check(f"S Result 10: false-show at true 0.80, n_e = {ne}: simulated = exact binomial tail, exact tail <= nominal 0.00083",
              abs(rate - exact) < 4 * math.sqrt(exact * (1 - exact) / reps) + 1e-7 and exact <= nominal,
              f"sim {rate:.5f}, exact {exact:.5f}, nominal {nominal:.5f}")
    # Result 11: power at n_e = 228 (precision 0.90) and 173 (recall 0.65), formula vs exact binomial.
    for c, p1, ne in ((C_PREC, 0.90, 229), (C_REC, 0.65, 174)):
        z8 = float(stats.norm.ppf(0.8))
        req = ((Z_DEC * math.sqrt(c * (1 - c)) + z8 * math.sqrt(p1 * (1 - p1))) / (p1 - c)) ** 2
        formula = float(stats.norm.cdf(((p1 - c) * math.sqrt(ne) - Z_DEC * math.sqrt(c * (1 - c))) / math.sqrt(p1 * (1 - p1))))
        kk = np.arange(ne + 1)
        lo, _ = wilson(kk / ne, ne, Z_DEC)
        exact = float(stats.binom.pmf(kk[lo >= c], ne, p1).sum())
        check(f"S Result 11: n_e = {math.ceil(req)} for 80% at {p1} vs floor {c}; exact binomial power {exact:.3f}, normal formula {formula:.3f} (quoted 0.789/0.814 and 0.803)",
              math.ceil(req) == ne and near({229: 0.789, 174: 0.814}[ne], exact, 3) and near({229: 0.803, 174: 0.803}[ne], formula, 3))


def scenario_rates(t):
    """Section 6's scenario inputs for tag t over the audience cells (rewritten from the text)."""
    cells = [k for k in STRATA if k[1] in AUDIENCE]
    pop = np.array([STRATA[k]["population"] - STRATA[k]["pilot"] for k in cells], float)
    lam = np.array([STRATA[k][f"label_{t}"] / STRATA[k]["population"] for k in cells])
    cc = [PILOT["counts"][t].get(k[0], [0, 0, 0, 0]) for k in cells]
    a = np.array([(x[1] + 0.5) / (x[0] + 1) for x in cc])
    b = np.array([(x[3] + 0.5) / (x[2] + 1) for x in cc])
    tp = float((pop * lam * a).sum())
    fp = float((pop * (1 - lam) * b).sum())
    return cells, pop, lam, a, b, tp, fp


def sim_power_json(seed: int = 4, reps: int = 4000) -> None:
    rng = np.random.default_rng(seed)
    m, f = PJ["chosen"]["m"], PJ["chosen"]["f"]
    key = f"m={m},f={f}"
    worst_dev, worst_name = 0.0, ""
    cap_binds = []
    finite_ok = True
    for t in SCORED:
        cells, pop, lam, a, b, tp_mass, fp_mass = scenario_rates(t)
        n = np.array([alloc(int(x), m, f) for x in pop], float)
        for target in (0.90, 0.80):
            bx = np.minimum(1, b * tp_mass * (1 / target - 1) / fp_mass)
            achieved = tp_mass / (tp_mass + float((pop * (1 - lam) * bx).sum()))
            if abs(achieved - target) > 1e-12:
                cap_binds.append((t, target, achieved))
            lab = rng.binomial(n.astype(int), lam, size=(reps, len(n)))
            tp = rng.binomial(lab, a)
            fp = rng.binomial(n.astype(int) - lab, bx)
            r, var, lin, kish, _, _ = ratio_from_counts(tp.astype(float), fp.astype(float), n, pop)
            lin = np.where((tp + fp).sum(1) > 0, lin, 0.0)
            lo, hi = wilson(r, np.maximum(lin, 1e-12), Z_DEC)
            lo = np.where(lin > 0, lo, 0.0)
            hi = np.where(lin > 0, hi, 1.0)
            shown = float(np.mean(lo >= C_PREC))
            stored = PJ["power"][t][f"linearised/shown_at_{target:.2f}"][key]
            dev = abs(shown - stored) / math.sqrt(max(stored * (1 - stored), 1 / 2000) * (1 / 2000 + 1 / reps))
            if dev > worst_dev:
                worst_dev, worst_name = dev, f"{t} shown_at_{target:.2f}"
            if target == 0.90:
                cov = float(np.mean((lo <= 0.90) & (0.90 <= hi)))
                stored = PJ["power"][t]["linearised/coverage_at_0.90"][key]
                dev = abs(cov - stored) / math.sqrt(max(stored * (1 - stored), 1 / 2000) * (1 / 2000 + 1 / reps))
                if dev > worst_dev:
                    worst_dev, worst_name = dev, f"{t} coverage"
                if t in ("smotheredMate", "skewer", "fork"):
                    # Result 14: one fixed finite population of the same scenario, sampled without
                    # replacement; the coverage of its own precision is at least the binomial one.
                    big = pop.astype(int)
                    L = np.minimum(np.round(big * lam).astype(int), big)
                    P11 = np.round(L * a).astype(int)
                    P10 = np.round((big - L) * bx).astype(int)
                    P00 = big - P11 - P10
                    s11 = np.empty((reps, len(n)))
                    s10 = np.empty((reps, len(n)))
                    for h in range(len(n)):
                        d = rng.multivariate_hypergeometric([P11[h], P10[h], P00[h]], int(n[h]), size=reps)
                        s11[:, h], s10[:, h] = d[:, 0], d[:, 1]
                    rf, _, linf, _, _, _ = ratio_from_counts(s11, s10, n, pop)
                    truth = P11.sum() / (P11 + P10).sum()
                    lof, hif = wilson(rf, np.maximum(linf, 1e-12), Z_DEC)
                    covf = float(np.mean((lof <= truth) & (truth <= hif)))
                    finite_ok &= covf >= cov - 4 * math.sqrt(max(cov * (1 - cov), 1 / reps) * 2 / reps)
        # The recall scenario: true-positive rates scaled (capped at 1) so recall is 0.65.
        lo_s, hi_s = 0.0, 1e6
        for _ in range(200):
            mid = (lo_s + hi_s) / 2
            rec = float((pop * lam * np.minimum(1, a * mid)).sum() / (pop * lam).sum())
            lo_s, hi_s = (mid, hi_s) if rec < 0.65 else (lo_s, mid)
        ar = np.minimum(1, a * lo_s)
        rec = float((pop * lam * ar).sum() / (pop * lam).sum())
        if abs(rec - 0.65) > 1e-6:
            cap_binds.append((t, "recall"))
        lab = rng.binomial(n.astype(int), lam, size=(reps, len(n)))
        tpr = rng.binomial(lab, ar)
        rr, _, linr, _, _, _ = ratio_from_counts(tpr.astype(float), (lab - tpr).astype(float), n, pop)
        lor, _ = wilson(rr, np.maximum(linr, 1e-12), Z_DEC)
        lor = np.where(linr > 0, lor, 0.0)
        trusted = float(np.mean(lor >= C_REC))
        stored = PJ["power"][t]["linearised/recall_trusted_at_0.65"][key]
        dev = abs(trusted - stored) / math.sqrt(max(stored * (1 - stored), 1 / 2000) * (1 / 2000 + 1 / reps))
        if dev > worst_dev:
            worst_dev, worst_name = dev, f"{t} recall_trusted"
    check("S Result 14: section 6 rewritten from its text reproduces power.json at the chosen design (15 tags x 4 rates) within 4.5 combined SE",
          worst_dev < 4.5, f"worst {worst_name} at {worst_dev:.2f} SE")
    worst_cap = max((abs(x[2] - x[1]) for x in cap_binds if x[1] != "recall"), default=0.0)
    check("S Result 9: the precision caps bind for mate only, off target by 1.6e-5 at 0.80 and 4.7e-6 at 0.90; every recall scenario is 0.65 within 1e-6",
          {x[0] for x in cap_binds} == {"mate"} and near(1.6e-5, worst_cap, 6) and all(x[1] != "recall" for x in cap_binds),
          f"{len(cap_binds)} capped, worst {worst_cap:.2e}")
    check("S Result 14: sampling one finite population without replacement covers its own precision at least as often as the binomial model covers 0.90 (3 tags)",
          finite_ok)


# ---- 3. numbers --------------------------------------------------------------------------------


def numbers() -> None:
    aud_cells = {k: v for k, v in STRATA.items() if k[1] in AUDIENCE}
    pop_all = sum(v["population"] for v in STRATA.values())
    pop_aud = sum(v["population"] for v in aud_cells.values())
    pilot = sum(v["pilot"] for v in STRATA.values())
    short = [(k, v["pilot"]) for k, v in STRATA.items() if v["pilot"] != 25]
    check("N population 1,826,817, audience 1,580,025, pilot 1,874 (25 a cell, 24 in smotheredMate/S, its whole cell)",
          pop_all == 1_826_817 and pop_aud == 1_580_025 and pilot == 1874 and short == [(("smotheredMate", "S"), 24)]
          and STRATA[("smotheredMate", "S")]["population"] == 24 and len(STRATA) == 75)

    def size(m, f, bands):
        return sum(alloc(v["population"] - v["pilot"], m, f) for k, v in STRATA.items() if k[1] in bands)

    every = AUDIENCE + ("S",)
    check("N sample 47,503 audience and 54,987 in all at m = 50, f = 0.03; 36,370 at m = 300, f = 0.02",
          size(50, 0.03, AUDIENCE) == 47_503 and size(50, 0.03, every) == 54_987 and size(300, 0.02, AUDIENCE) == 36_370
          and len(SAMPLE) == 54_987 and R["sample"]["puzzles"] == 54_987 and R["sample"]["tagged"] == 54_987)
    want = {f"{s}/{b}": alloc(v["population"] - v["pilot"], 50, 0.03) for (s, b), v in STRATA.items()
            if v["population"] - v["pilot"] > 0}
    got = Counter(f"{r['stratum']}/{r['band']}" for r in SAMPLE)
    check("N every cell of sample.csv and results.json sample.cells has n_h = min(N_h, max(50, ceil(0.03 N_h))), N_h = population - pilot (74 cells)",
          dict(got) == want == R["sample"]["cells"] and "smotheredMate/S" not in got)
    check("N budget: 28 cli calls = ceil(54,987 / 2,000), lines 54,987, actual = budget, no exclusions",
          math.ceil(54_987 / 2000) == 28 and R["compute"]["actual"] == R["compute"]["budget"] == {"cli_calls": 28, "lines": 54_987}
          and R["sample"]["excluded"] == [])
    none_max = max(v["population"] for k, v in STRATA.items() if k[0] == "none")
    check("N the largest 'none' cell holds 223,180 puzzles", none_max == 223_180)
    # Result 13: weights at f = 0, and one false positive's effect on smotheredMate's precision.
    ratios = []
    for m in (50, 100, 150, 200, 300):
        ws = [(v["population"] - v["pilot"]) / alloc(v["population"] - v["pilot"], m, 0.0) for v in aud_cells.values()]
        ratios.append(max(ws) / min(ws))
    w_none200 = (223_180 - 25) / 200
    L = sum(v["label_smotheredMate"] for v in aud_cells.values())
    drop = 1 - L / (L + w_none200)
    check("N Result 13: at f = 0 the heaviest weight is 744 to 2,010 times the lightest on the grid (1,116 at m = 200, 'about a thousand'); one false positive there moves smotheredMate's precision by 0.15",
          near(744, min(ratios), 0) and near(2010, max(ratios), 0) and near(1116, w_none200, 0) and near(0.15, drop, 2),
          f"{min(ratios):.0f} to {max(ratios):.0f}, drop {drop:.3f}")
    chosen_w = max((v["population"] - v["pilot"]) / alloc(v["population"] - v["pilot"], 50, 0.03) for v in STRATA.values() if v["population"] > v["pilot"])
    check("N Result 13: the heaviest weight at the chosen design is at most 1/f = 33.3", chosen_w <= 1 / 0.03 + 1e-9, f"{chosen_w:.2f}")
    check("N z_decision 3.144 in results.json and power.json", R["z_decision"] == 3.144 == PJ["z_decision"])
    # Intervals recomputed from (estimate, n_eff), with their rounding (estimate +-5e-5, n_eff +-0.05).
    worst = 0.0
    fallback = True
    kish_le_n = True
    for t, v in R["tags"].items():
        for dom in [("all", v)] + [(b, v["by_band"][b]) for b in v["by_band"]]:
            for metric in ("precision", "recall"):
                e = dom[1][metric]
                if e["estimate"] is None:
                    continue
                kish_le_n &= e["n_eff_kish"] <= e["n"] + 0.05
                if e["estimate"] in (0.0, 1.0):
                    fallback &= e["n_eff"] == e["n_eff_kish"]
                for key, z in (("ci80", Z80), ("ci95", Z95), ("ci_decision", Z_DEC)):
                    lows, highs = [], []
                    for de in (-5e-5, 5e-5):
                        for dn in (-0.05, 0.05):
                            pe = min(1.0, max(0.0, e["estimate"] + de))
                            lo, hi = wilson(pe, e["n_eff"] + dn, z)
                            lows.append(float(lo))
                            highs.append(float(hi))
                    err = max(0.0, min(lows) - e[key][0] - 5e-5, e[key][0] - max(lows) - 5e-5,
                              min(highs) - e[key][1] - 5e-5, e[key][1] - max(highs) - 5e-5)
                    worst = max(worst, err)
    check("N every committed interval (15 tags x 6 domains x 2 metrics x 3 levels) is Wilson at (estimate, n_eff), within rounding",
          worst == 0.0, f"worst excess {worst:.2e}")
    check("N where the estimate is 0 or 1 the effective size is Kish's (section 8's fallback)", fallback)
    check("N Result 6: n_eff_kish <= n for every committed estimate", kish_le_n)
    hp = R["tags"]["hangingPiece"]["precision"]
    check("N Result 5: hangingPiece precision n_eff 5,127 against 2,888 puzzles", near(5127, hp["n_eff"], 0) and hp["n"] == 2888)
    # Smothered mate: 1.000 [0.953, 1.000] from Kish's n_eff at p-hat = 1: n/(n + z^2).
    sm = R["tags"]["smotheredMate"]["precision"]
    check("N Result 7: smotheredMate's lower end 0.953 = n_K/(n_K + z^2) at n_K = 202.3",
          near(0.9534, sm["n_eff"] / (sm["n_eff"] + Z_DEC**2), 4), f"{sm['n_eff'] / (sm['n_eff'] + Z_DEC ** 2):.5f}")
    # Decisions recomputed from the rule of section 9 (math.md (E7)).
    ok = True
    for t, v in R["tags"].items():
        lo_p = v["precision"]["ci_decision"][0]
        lo_r = v["recall"]["ci_decision"][0]
        if lo_p < C_PREC:
            want_d = "hidden pending a human audit" if t in LABEL_LIMITED else "hidden until fixed"
        elif lo_r >= C_REC:
            want_d = "trusted"
        else:
            want_d = "needs fixing"
        ok &= v["decision"] == want_d and v["label_limited"] == (t in LABEL_LIMITED)
        ok &= v["group"] == ("mate length" if t in MATE_LENGTH else "motif")
    trusted_motifs = [t for t in R["decisions"]["trusted"] if t not in MATE_LENGTH]
    ok &= sorted(trusted_motifs) == sorted(["fork", "discoveredAttack", "promotion", "smotheredMate"])
    ok &= R["decisions"]["hidden until fixed"] == ["deflection"] and R["decisions"]["needs fixing"] == []
    ok &= sorted(R["decisions"]["hidden pending a human audit"]) == sorted(["hangingPiece", "pin", "skewer", "sacrifice", "backRankMate"])
    check("N decisions: 4 of 10 motif tags trusted, 5 mate-length tags pass, deflection hidden until fixed, 5 pending audit, none needs fixing", ok)
    # The decision table, three decimals.
    table = {
        "fork": (0.976, 0.969, 0.982, 0.725, 0.709, 0.740), "discoveredAttack": (0.992, 0.985, 0.996, 0.858, 0.839, 0.875),
        "promotion": (0.998, 0.987, 1.000, 0.816, 0.779, 0.848), "smotheredMate": (1.000, 0.953, 1.000, 1.000, 0.953, 1.000),
        "skewer": (0.810, 0.778, 0.838, 0.897, 0.867, 0.920), "backRankMate": (0.606, 0.575, 0.636, 1.000, 0.989, 1.000),
        "hangingPiece": (0.572, 0.550, 0.593, 0.999, 0.992, 1.000), "deflection": (0.562, 0.527, 0.597, 0.335, 0.307, 0.363),
        "sacrifice": (0.528, 0.513, 0.544, 0.824, 0.806, 0.840), "pin": (0.402, 0.368, 0.436, 0.201, 0.181, 0.222),
        "mate": (1.000, 0.999, 1.000, 1.000, 0.999, 1.000), "mateIn1": (1.000, 0.997, 1.000, 1.000, 0.997, 1.000),
        "mateIn2": (0.999, 0.997, 1.000, 1.000, 0.999, 1.000), "mateIn3": (1.000, 0.996, 1.000, 1.000, 0.996, 1.000),
        "mateIn4Plus": (1.000, 0.978, 1.000, 1.000, 0.978, 1.000),
    }
    bad = []
    for t, q in table.items():
        p, rc = R["tags"][t]["precision"], R["tags"][t]["recall"]
        got = (p["estimate"], *p["ci_decision"], rc["estimate"], *rc["ci_decision"])
        if not all(near(a, b, 3) for a, b in zip(q, got)):
            bad.append(t)
    check("N decision.md's table: 15 tags x (precision, interval, recall, interval) at 3 dp", not bad, ", ".join(bad))
    summ = {"fork": (0.98, 0.72), "discoveredAttack": (0.99, 0.86), "promotion": (1.00, 0.82), "smotheredMate": (1.00, 1.00),
            "deflection": (0.56, 0.33), "skewer": (0.81, None), "backRankMate": (0.61, None), "hangingPiece": (0.57, None),
            "sacrifice": (0.53, None), "pin": (0.40, 0.20)}
    ok = all(near(p, R["tags"][t]["precision"]["estimate"], 2) and (rc is None or near(rc, R["tags"][t]["recall"]["estimate"], 2))
             for t, (p, rc) in summ.items())
    check("N summary.md's two-decimal precisions and recalls (10 motif tags)", ok)
    check("N decision: skewer's estimate 0.81 is above 0.80 and its lower end 0.778 below; recall 0.999 and 1.000 for hangingPiece and backRankMate",
          R["tags"]["skewer"]["precision"]["estimate"] > 0.8 > R["tags"]["skewer"]["precision"]["ci_decision"][0]
          and near(0.999, R["tags"]["hangingPiece"]["recall"]["estimate"], 3) and near(1.0, R["tags"]["backRankMate"]["recall"]["estimate"], 3))
    check("N matingNet on 19.7% of the audience puzzles", near(19.7, 100 * R["unscored"]["matingNet"]["tagged_share"], 1))
    bb = lambda t, b, m: R["tags"][t]["by_band"][b][m]["estimate"]  # noqa: E731
    ok = (near(0.86, bb("fork", "A1", "recall"), 2) and near(0.61, bb("fork", "A4", "recall"), 2) and near(0.54, bb("fork", "S", "recall"), 2)
          and near(0.95, bb("discoveredAttack", "A1", "recall"), 2) and near(0.81, bb("discoveredAttack", "A4", "recall"), 2)
          and near(0.71, bb("discoveredAttack", "S", "recall"), 2) and near(0.83, bb("promotion", "A1", "recall"), 2)
          and near(0.77, bb("promotion", "A4", "recall"), 2) and near(0.70, bb("promotion", "S", "recall"), 2)
          and near(0.90, bb("skewer", "A1", "precision"), 2) and near(0.77, bb("skewer", "A4", "precision"), 2)
          and near(0.71, bb("skewer", "S", "precision"), 2) and near(0.36, bb("sacrifice", "A1", "precision"), 2)
          and near(0.66, bb("sacrifice", "S", "precision"), 2) and R["tags"]["smotheredMate"]["by_band"]["S"]["precision"]["estimate"] is None)
    ok &= all(bb(t, b1, "recall") > bb(t, b2, "recall") for t in ("fork", "discoveredAttack", "promotion")
              for b1, b2 in (("A1", "A4"), ("A4", "S")))
    check("N by band: fork 0.86/0.61/0.54, discoveredAttack 0.95/0.81/0.71, promotion 0.83/0.77/0.70 recall; skewer 0.90/0.77/0.71, sacrifice 0.36/0.66 precision; smotheredMate S empty", ok)
    sk = [bb("skewer", b, "precision") for b in ("A1", "A2")]
    check("N decision: skewer's two easiest bands would clear 0.80 on their estimates", all(s > 0.80 for s in sk), f"{sk}")
    ct = R["confusion"]["by_tag"]
    cth = R["confusion"]["by_theme"]
    ok = (near(0.10, ct["hangingPiece"]["themes"]["defensiveMove"], 2) and near(0.04, ct["hangingPiece"]["themes"]["intermezzo"], 2)
          and near(0.24, ct["sacrifice"]["themes"]["attraction"], 2) and near(0.19, ct["sacrifice"]["themes"]["discoveredAttack"], 2)
          and near(0.14, ct["sacrifice"]["themes"]["deflection"], 2) and near(0.24, ct["pin"]["themes"]["deflection"], 2)
          and near(0.16, cth["deflection"]["tags"]["pin"], 2) and near(0.16, ct["deflection"]["themes"]["attraction"], 2)
          and near(0.13, ct["deflection"]["themes"]["fork"], 2) and near(0.33, cth["deflection"]["tags"]["deflection"], 2)
          and near(0.36, cth["deflection"]["tags"]["sacrifice"], 2))
    check("N confusion shares quoted in decision.md (hangingPiece, sacrifice, pin, deflection), 2 dp", ok)
    # Finding 1: those shares are over all puzzles we tag; over the false positives alone they lie in
    # [max(0, (s - P)/(1 - P)), s/(1 - P)] (Result 16).
    rows = [("hangingPiece", "defensiveMove"), ("deflection", "attraction"), ("deflection", "fork"),
            ("sacrifice", "attraction"), ("sacrifice", "discoveredAttack"), ("sacrifice", "deflection")]
    bounds = {}
    for t, th in rows:
        s = ct[t]["themes"][th]
        p = R["tags"][t]["precision"]["estimate"]
        bounds[(t, th)] = (max(0.0, (s - p) / (1 - p)), s / (1 - p))
    ok = near(0.229, bounds[("hangingPiece", "defensiveMove")][1], 3) and near(0.375, bounds[("deflection", "attraction")][1], 3)
    ok &= near(0.512, bounds[("sacrifice", "attraction")][1], 3)
    ok &= all(near(q, bounds[k][1], 3) for k, q in ((("sacrifice", "discoveredAttack"), 0.406), (("sacrifice", "deflection"), 0.295), (("deflection", "fork"), 0.287)))
    ok &= all(near(q, ct["hangingPiece"]["themes"][k], 3) for k, q in (("fork", 0.068), ("kingsideAttack", 0.067), ("advancedPawn", 0.053), ("intermezzo", 0.038)))
    larger = sorted(k for k, v in ct["hangingPiece"]["themes"].items() if k != "hangingPiece" and v > ct["hangingPiece"]["themes"]["intermezzo"])
    ok &= larger == ["advancedPawn", "defensiveMove", "fork", "kingsideAttack"]
    check("N Finding 1: FP-only shares up to 0.229 (hangingPiece/defensiveMove), 0.375 (deflection/attraction), 0.512 (sacrifice/attraction); fork 0.068, kingsideAttack 0.067, advancedPawn 0.053 exceed intermezzo 0.038",
          ok, "; ".join(f"{t}/{th} [{lo:.3f}, {hi:.3f}]" for (t, th), (lo, hi) in bounds.items()))
    # Hand check: 19 of 26, Wilson 95%.
    lo, hi = wilson(19 / 26, 26, Z95)
    check("N hand check: 19 of 26 = 0.73, 95% Wilson 0.54 to 0.86", near(0.73, 19 / 26, 2) and near(0.54, float(lo), 2) and near(0.86, float(hi), 2),
          f"[{float(lo):.4f}, {float(hi):.4f}]")
    # Power numbers of preregistration section 6 and review.md.
    P = PJ["power"]
    ch = f"m={PJ['chosen']['m']},f={PJ['chosen']['f']}"
    designs = PJ["designs"]
    idx = designs.index(ch)
    ok = ch == "m=50,f=0.03" and PJ["audience_puzzles"][ch] == 47_503 and near(0.84, PJ["worst"][ch], 2)
    ok &= all(PJ["worst"][d] < 0.80 for d in designs[:idx])
    ok &= designs[idx - 1] == "m=300,f=0.02" and near(0.56, PJ["worst"][designs[idx - 1]], 2)
    sm_p = P["smotheredMate"]["linearised/shown_at_0.90"][ch]
    ok &= near(0.016, 1.96 * math.sqrt(sm_p * (1 - sm_p) / PJ["sims"]), 3)
    ok &= all(near(1.0, P[t]["linearised/shown_at_0.90"][ch], 2) for t in SCORED if t != "smotheredMate")
    ok &= all(near(0.90, P[t]["linearised/recall_trusted_at_0.65"][ch], 2) or P[t]["linearised/recall_trusted_at_0.65"][ch] >= 0.90 for t in SCORED)
    check("N power: chosen m = 50, f = 0.03 is the first passing design; worst smotheredMate 0.84 (+-0.016); m = 300, f = 0.02 before it at 0.56; others 1.00; recall >= 0.90 (0.898 rounds to it)", ok)
    lin_show80 = max(P[t]["linearised/shown_at_0.80"][ch] for t in SCORED)
    ok = near(0.0015, lin_show80, 4) and all(P[t][f"linearised/shown_at_{s}"][ch] == 0 for t in SCORED for s in ("0.75", "0.70"))
    ok &= near(0.996, min(P[t]["linearised/coverage_at_0.90"][ch] for t in SCORED), 3)
    ok &= near(0.9975, min(P[t]["kish/coverage_at_0.90"][ch] for t in SCORED), 4) and near(0.001, max(P[t]["kish/shown_at_0.80"][ch] for t in SCORED), 4)
    ok &= near(0.0095, sum(P[t]["linearised/shown_at_0.80"][ch] for t in SCORED), 4) and 0.0095 <= 15 * ALPHA / 60
    check("N power: at the chosen design false-show at 0.80 at most 0.0015 (sum over tags 0.0095 <= 0.0125), none at 0.75 or 0.70; worst coverage 0.996 (Kish's 0.9975, 0.001)", ok)
    k1, k2 = "m=200,f=0.01", "m=300,f=0.01"
    ok = near(0.87, P["smotheredMate"]["kish/coverage_at_0.90"][k1], 2) and near(0.041, P["smotheredMate"]["kish/shown_at_0.80"][k1], 3)
    ok &= near(0.063, P["smotheredMate"]["kish/shown_at_0.80"][k2], 3) and near(0.0008, ALPHA / 60, 4)
    check("N power: Kish's at m = 200, f = 0.01: coverage 0.87, false-show 0.041 (0.063 at m = 300, f = 0.01); nominal one-sided 0.0008", ok)
    pos = [d for d in designs if float(d.split("f=")[1]) >= 0.005]
    zer = [d for d in designs if float(d.split("f=")[1]) == 0.0]
    cov_min = min(P[t]["linearised/coverage_at_0.90"][d] for t in SCORED for d in pos)
    fs_max = max(P[t]["linearised/shown_at_0.80"][d] for t in SCORED for d in pos)
    fs0 = [max(P[t]["linearised/shown_at_0.80"][d] for t in SCORED) for d in zer]
    pos01 = [d for d in designs if float(d.split("f=")[1]) >= 0.01]
    cov01 = min(P[t]["linearised/coverage_at_0.90"][d] for t in SCORED for d in pos01)
    low = sorted((P[t]["linearised/coverage_at_0.90"][d], t, d) for t in SCORED for d in pos
                 if P[t]["linearised/coverage_at_0.90"][d] < 0.994)
    ok = fs_max <= 0.0025 and near(0.018, min(fs0), 3) and near(0.12, max(fs0), 2)
    check("N power: linearised false-show <= 0.0025 for every f >= 0.005; f = 0 false-show 0.018 to 0.12", ok,
          f"{fs_max}, {min(fs0)} to {max(fs0)}")
    ok = cov01 >= 0.994 and cov_min == 0.9615 and [x[1:] for x in low] == [("smotheredMate", f"m={m},f=0.005") for m in (50, 100, 150, 200, 300)]
    check("N Finding 2: linearised coverage at 0.90 is >= 0.994 only for f >= 0.01; at f = 0.005 smotheredMate's is 0.9615 to 0.988 (quoted '0.994 or more for f >= 0.005')",
          ok, f"min over f >= 0.01: {cov01}; f = 0.005: {[x[0] for x in low]}")
    # Remark after Result 1: the estimand is the population less the pilot.
    shares = {}
    for t in SCORED:
        lab_aud = sum(v[f"label_{t}"] for v in aud_cells.values())
        pil = sum(c[0] for c in PILOT["counts"][t].values())  # labelled pilot puzzles, all bands (an upper bound)
        shares[t] = pil / lab_aud if lab_aud else 0.0
    # precision: the pilot's tagged puzzles (all bands, an upper bound) over themselves plus the
    # estimated tagged total X = recall L / precision (Y = recall L = precision X).
    pshare = {}
    for t in SCORED:
        pt = sum(c[1] + c[3] for c in PILOT["counts"][t].values())
        lab_aud = sum(v[f"label_{t}"] for v in aud_cells.values())
        x_est = R["tags"][t]["recall"]["estimate"] * lab_aud / R["tags"][t]["precision"]["estimate"]
        pshare[t] = pt / (pt + x_est)
    ptop = max(pshare, key=pshare.get)
    check("N remark: pilot puzzles are at most 1.9% of any tag's estimated tagged audience total (smotheredMate; skewer 0.27%)",
          near(0.019, pshare[ptop], 3) and ptop == "smotheredMate" and near(0.0027, pshare["skewer"], 4), f"largest {ptop} {pshare[ptop]:.4f}, skewer {pshare['skewer']:.4f}")
    # Finding 3: no audience cell is sampled in full; the largest fraction.
    fr = {k: alloc(v["population"] - v["pilot"], 50, 0.03) / (v["population"] - v["pilot"]) for k, v in aud_cells.items()}
    kmax = max(fr, key=fr.get)
    census = [k for k, v in STRATA.items() if v["population"] - v["pilot"] > 0 and alloc(v["population"] - v["pilot"], 50, 0.03) == v["population"] - v["pilot"]]
    check("N Finding 3: no cell with puzzles left after the pilot is sampled in full; the largest audience fraction is 50/111 = 0.45 (mateIn4Plus/A1)",
          census == [] and kmax == ("mateIn4Plus", "A1") and near(0.45, fr[kmax], 2), f"{kmax} {fr[kmax]:.3f}")
    # decision.md's band wording: precision falls with rating for skewer, pin, backRankMate.
    pb = {t: [bb(t, b, "precision") for b in ("A1", "A2", "A3", "A4", "S")] for t in ("skewer", "pin", "backRankMate")}
    ok = all(pb[t][0] > pb[t][3] for t in pb) and all(a > b for a, b in zip(pb["skewer"], pb["skewer"][1:]))
    ok &= pb["pin"][1] > pb["pin"][0] and pb["backRankMate"][4] > pb["backRankMate"][3]
    check("N by band: precision falls from A1 to A4 for skewer, pin and backRankMate; monotone only for skewer (pin A2 > A1; backRankMate S 0.58 > A4 0.44)",
          ok, "; ".join(f"{t} {[round(x, 2) for x in v]}" for t, v in pb.items()))
    # The hand check, parsed from pilot/hand-check.md: 19 label gaps of 26 checked false
    # positives, 2 to 4 a tag, and LABEL_LIMITED = the tags with at least half label gaps.
    import re
    text = (HERE / "pilot" / "hand-check.md").read_text(encoding="utf-8")
    secs = re.findall(r"^## (\w+): tagged, not labelled: \S+\n(.*?)(?=^## |\Z)", text, flags=re.S | re.M)
    per: dict[str, list[str]] = {}
    for t, body in secs:
        v = re.search(r"- Verdict: ([a-z ]+?)[.(]", body)
        per.setdefault(t, []).append(v.group(1).strip() if v else "?")
    gaps = sum(v.count("label gap") for v in per.values())
    total = sum(len(v) for v in per.values())
    limited = sorted(t for t, v in per.items() if v.count("label gap") >= len(v) / 2)
    ok = gaps == 19 and total == 26 and limited == sorted(LABEL_LIMITED)
    ok &= all(1 <= len(v) <= 4 for v in per.values())
    detail = ", ".join(f"{t} {v.count('label gap')}/{len(v)}" for t, v in per.items())
    check("N hand check (parsed): 19 label gaps of 26 checked false positives; at least half label gaps for exactly the six LABEL_LIMITED tags",
          ok, f"{gaps}/{total}; {detail}")
    # Section 6's selection: the designs ordered by audience puzzles; the first that passes.
    order = sorted(PJ["designs"], key=lambda d: (PJ["audience_puzzles"][d], d))
    first = next(d for d in order if PJ["worst"][d] >= 0.80)
    check("N power: sorted by audience puzzles, the first design whose worst tag reaches 0.80 is m = 50, f = 0.03", first == "m=50,f=0.03" == ch)
    # Finding 2: 0.0015 is 3 of 2,000; its 95% interval.
    lo3, hi3 = stats.beta.ppf(0.025, 3, 1998), stats.beta.ppf(0.975, 4, 1997)
    check("N Finding 2: 0.0015 = 3/2,000, 95% Clopper-Pearson 0.0003 to 0.0044; 0.0025 = 5/2,000 is 3 x nominal",
          near(0.0015, 3 / 2000, 4) and near(0.0003, lo3, 4) and near(0.0044, hi3, 4) and near(3.0, 0.0025 / (ALPHA / 60), 1), f"[{lo3:.4f}, {hi3:.4f}]")
    top = max(shares, key=shares.get)
    check("N remark: pilot puzzles are at most 1.9% of any tag's labelled audience puzzles (smotheredMate, an upper bound; 100 of 6,377 = 1.6% in the audience)",
          near(0.019, shares[top], 3) and top == "smotheredMate" and near(0.016, sum(STRATA[("smotheredMate", b)]["pilot"] for b in AUDIENCE) / L, 3) and sum(STRATA[("smotheredMate", b)]["pilot"] for b in AUDIENCE) == 100, f"largest {top} {shares[top]:.4f}")


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
