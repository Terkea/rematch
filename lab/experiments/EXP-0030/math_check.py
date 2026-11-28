"""EXP-0030: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy): the identities of math.md section 4, each as `simplify(lhs - rhs) == 0`,
   and the inequalities on a dense grid where sympy cannot settle them (the line says so).
2. Simulation (numpy, fixed seeds): each result's model with a known truth: a Gaussian AR(1)
   with known parameters (Results 3, 5 to 7, 10 and Finding 1), the Elo world the memory model
   linearises (Results 1, 2, 8 and 11), the cohort's z with added change (Result 15), the lag-1
   statistic (Result 16) and the decision rule on multinomial counts (Result 12).
3. Numbers: every number decision.md and summary.md quote, recomputed from results.json and
   power.json, within the rounding it is quoted at.

Standard library, numpy, scipy and sympy only. About 20 s of CPU.
"""

from __future__ import annotations

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

Q = math.log(10.0) / 400.0
Z80 = float(stats.norm.ppf(0.9))  # 1.2816
LEVEL = 1 - 0.025 / 2
Z_DEC = float(stats.norm.ppf(1 - (1 - LEVEL) / 2))  # 2.4977
BOUND, HIDE = 0.15, 0.20
W = 30
CHECKS = [60, 120, 250]
# Theorem names in lab/lean/Rematch/ (Multiplicity.lean). LuckShare.lean is not used: math.md
# section 4, the remark after Result 6, says why.
LEAN = {"Result 12 (the union over two candidates and two looks)": "Rematch.union_bound"}

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
    """`value` rounds to `quoted` at `dp` decimals (half-up or half-even)."""
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


def block(m, phi):
    """(E3) S(m) = sum over i, j in a block of m of phi^|i - j|."""
    return m * (1 + phi) / (1 - phi) - 2 * phi * (1 - phi**m) / (1 - phi) ** 2


def cross(ma, mb, gap, phi):
    """(E4) C, with gap = b0 - a1 + 1 the smallest lag between the windows."""
    return phi**gap * (1 - phi**ma) * (1 - phi**mb) / (1 - phi) ** 2


def params(s):
    """(E2) a, phi, sigma^2 from the RMS step s."""
    a = np.minimum(Q * np.asarray(s, dtype=float) / 2, 1.0)
    return a, np.clip(1 - a, 0.0, 1 - 1e-12), np.asarray(s, dtype=float) ** 2 / (a * (2 - a))


def diff_var(sigma2, phi, a_win, b_win):
    """(E5) Var(mean B - mean A) for A = [a0, a1), B = [b0, b1)."""
    (a0, a1), (b0, b1) = a_win, b_win
    ma, mb = a1 - a0, b1 - b0
    return sigma2 * (
        block(ma, phi) / ma**2 + block(mb, phi) / mb**2 - 2 * cross(ma, mb, b0 - a1 + 1, phi) / (ma * mb)
    )


def windows(rule: str, n: int):
    """(E1) the windows of each candidate."""
    w = min(W, n // 2)
    then = (0, w) if rule == "memory-halves" else (0, n - w)
    return then, (n - w, n)


def newcombe_paired(n11, n10, n01, n00, level):
    """(E8) Newcombe's method 10, written from Newcombe (1998)."""
    n = n11 + n10 + n01 + n00
    p1, p2 = (n11 + n10) / n, (n11 + n01) / n
    l1, u1 = (float(x) for x in wilson(n11 + n10, n, level))
    l2, u2 = (float(x) for x in wilson(n11 + n01, n, level))
    den = math.sqrt((n11 + n10) * (n01 + n00) * (n11 + n01) * (n10 + n00))
    ph = (n11 * n00 - n10 * n01) / den if den > 0 else 0.0
    d = p1 - p2
    lo = d - math.sqrt(max((p1 - l1) ** 2 - 2 * ph * (p1 - l1) * (u2 - p2) + (u2 - p2) ** 2, 0))
    hi = d + math.sqrt(max((u1 - p1) ** 2 - 2 * ph * (u1 - p1) * (p2 - l2) + (p2 - l2) ** 2, 0))
    return d, lo, hi


def cut(n, level, bound):
    """Result 12: the largest k whose Wilson upper end at `level` is at most `bound`."""
    k = np.arange(n + 1)
    _, hi = wilson(k, n, level)
    return int(k[hi <= bound].max())


def both_within(n, p, c):
    """P(K1 <= c, K2 <= c) for (K1, K2, rest) ~ Multinomial(n; p, p, 1 - 2p), exactly."""
    k1 = np.arange(c + 1)
    return float((stats.binom.pmf(k1, n, p) * stats.binom.cdf(c, n - k1, p / (1 - p))).sum())


# ---- 1. symbolic ------------------------------------------------------------------------------


def symbolic() -> None:
    x, q, K, v, a, s, phi, c, d = sp.symbols("x q K v a s phi c delta", positive=True)
    # R1: the logistic's slope and the bound on its curvature.
    E = 1 / (1 + 10 ** (-x / 400))
    qq = sp.log(10) / 400
    check("R1 dE/dx = q E (1 - E)", zero(sp.diff(E, x) - qq * E * (1 - E)))
    check("R1 d2E/dx2 = q^2 E(1-E)(1-2E)", zero(sp.diff(E, x, 2) - qq**2 * E * (1 - E) * (1 - 2 * E)))
    u = sp.symbols("u")
    g = u * (1 - u) * (1 - 2 * u)
    crit = [r for r in sp.solve(sp.diff(g, u), u) if r.is_real and 0 <= r <= 1]
    gmax = max(sp.nsimplify(abs(g.subs(u, r))) for r in crit)
    grid = np.linspace(0, 1, 100_001)
    check(
        "R1 max |u(1-u)(1-2u)| = 1/(6 sqrt 3)",
        zero(gmax - 1 / (6 * sp.sqrt(3)))
        and abs(np.max(np.abs(grid * (1 - grid) * (1 - 2 * grid))) - float(1 / (6 * sp.sqrt(3)))) < 1e-9,
    )
    # R2: the stationary variance.
    p_ = 1 - a
    sig2 = sp.symbols("sigma2", positive=True)
    sol = sp.solve(sp.Eq(sig2, p_**2 * sig2 + K**2 * v), sig2)[0]
    check("R2 sigma^2 = K^2 v / (a (2 - a))", zero(sol - K**2 * v / (a * (2 - a))))
    # R3: the step's second moment, E d^2 = a^2 sigma^2 + K^2 v = 2 a sigma^2 = K^2 v 2/(2 - a).
    ed2 = a**2 * sol + K**2 * v
    check("R3 E d^2 = 2 a sigma^2", zero(ed2 - 2 * a * sol))
    check("R3 E d^2 = K^2 v * 2/(2 - a)", zero(ed2 - K**2 * v * 2 / (2 - a)))
    # R4: the plug-in under p(1 - p) = v = 1/4: a = q K / 4, s^2 = E d^2.
    a_true = q * K / 4
    s_true = sp.sqrt(ed2.subs({v: sp.Rational(1, 4), a: a_true}))
    a_hat = q * s_true / 2
    ratio_a = sp.simplify(a_hat / a_true)
    check("R4 a_hat / a = sqrt(2 / (2 - a))", zero(ratio_a - sp.sqrt(2 / (2 - a_true))))
    r = sp.sqrt(2 / (2 - a))
    sig_ratio = sp.simplify((s_true**2 / (a_hat * (2 - a_hat))) / (K**2 / 4 / (a_true * (2 - a_true))))
    check(
        "R4 sigma_hat^2 / sigma^2 = 2 / (r (2 - a r))",
        zero(sig_ratio.subs(K, 4 * a / q) - (2 / (r * (2 - a * r)))),
    )
    ser_a = sp.series(r, a, 0, 2).removeO()
    ser_s = sp.series(2 / (r * (2 - a * r)), a, 0, 2).removeO()
    check("R4 both ratios are 1 + a/4 + O(a^2)", zero(ser_a - (1 + a / 4)) and zero(ser_s - (1 + a / 4)))
    ag = np.linspace(1e-6, 0.1, 10_001)
    rg = np.sqrt(2 / (2 - ag))
    ok = np.all(rg - 1 <= ag / 4 + ag**2 / 4) and np.all(rg - 1 >= ag / 4)
    sg = 2 / (rg * (2 - ag * rg))
    ok &= np.all(sg - 1 <= ag / 4 + ag**2 / 2) and np.all(sg - 1 >= ag / 4)
    check("R4 a/4 <= ratio - 1 <= a/4 + a^2/2 on (0, 0.1] (grid)", bool(ok))
    # R4: the two forms of sigma^2 agree when a < 1, and a < 1 iff s < 800 / ln 10.
    a_s = q * s / 2
    check("R4 s^2/(a(2-a)) = s/(q(1 - a/2)) at a = q s/2", zero(s**2 / (a_s * (2 - a_s)) - s / (q * (1 - a_s / 2))))
    check("R4 a < 1 iff s < 2/q = 800/ln 10 = 347.4", zero(2 / qq - 800 / sp.log(10)) and near(347.4, 2 / Q, 1))
    # R5: the block sum.
    ph = sp.symbols("phi")
    ok = True
    for m in range(1, 41):
        direct = sum(ph ** abs(i - j) for i in range(m) for j in range(m))
        closed = m * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m) / (1 - ph) ** 2
        ok &= zero(sp.cancel(direct - closed))
    check("R5 S(m) closed form = direct double sum, m = 1..40 (exact in phi)", ok)
    m_ = sp.symbols("m", positive=True, integer=True)
    k_ = sp.symbols("k", positive=True, integer=True)
    sym_sum = m_ + 2 * sp.summation((m_ - k_) * ph**k_, (k_, 1, m_ - 1))
    closed_m = m_ * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m_) / (1 - ph) ** 2
    ok_sym = all(
        abs(complex(sp.N((sym_sum - closed_m).subs({m_: mm, ph: pv})))) < 1e-9
        for mm in (2, 7, 30)
        for pv in (sp.Rational(1, 3), sp.Rational(983, 1000))
    )
    check("R5 S(m) from sympy's symbolic summation over k (spot values)", ok_sym)
    check("R5 S(m) -> m as phi -> 0", zero(sp.limit(closed_m.subs(m_, 30), ph, 0) - 30))
    check("R5 S(m) -> m^2 as phi -> 1", zero(sp.limit(closed_m.subs(m_, 30), ph, 1) - 900))
    # R6: the cross term.
    ok = True
    for ma, mb, a0gap in [(3, 4, 0), (5, 5, 3), (2, 6, 7), (30, 30, 0)][:3]:
        a1 = ma
        b0 = a1 + a0gap
        direct = sum(ph ** (j - i) for i in range(0, a1) for j in range(b0, b0 + mb))
        closed = ph ** (b0 - a1 + 1) * (1 - ph**ma) * (1 - ph**mb) / (1 - ph) ** 2
        ok &= zero(sp.cancel(direct - closed))
    check("R6 C closed form = direct sum, exponent b0 - a1 + 1 (3 layouts, exact)", ok)
    # R7: Var(mean B - mean A) = w' Gamma w, exactly, for a small layout.
    ok = True
    for ma, mb, gap in [(3, 4, 0), (4, 2, 2)]:
        n = ma + gap + mb
        wv = sp.Matrix([-sp.Rational(1, ma)] * ma + [0] * gap + [sp.Rational(1, mb)] * mb)
        G = sp.Matrix(n, n, lambda i, j: ph ** abs(i - j))
        quad = (wv.T * G * wv)[0]
        a1, b0 = ma, ma + gap
        f = (
            (ma * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**ma) / (1 - ph) ** 2) / ma**2
            + (mb * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**mb) / (1 - ph) ** 2) / mb**2
            - 2 * ph ** (b0 - a1 + 1) * (1 - ph**ma) * (1 - ph**mb) / ((1 - ph) ** 2 * ma * mb)
        )
        ok &= zero(sp.cancel(quad - f))
        # R8: the variogram form, sum w = 0.
        V = sp.Matrix(n, n, lambda i, j: 2 * (1 - ph ** abs(i - j)))
        ok &= zero(sp.cancel(quad + (wv.T * V * wv)[0] / 2))
    check("R7 (E5) = w' Gamma w; R8 = -1/2 w' V w (exact, 2 layouts)", ok)
    kk = sp.symbols("k", positive=True)
    lim = sp.limit((1 - (1 - a) ** kk) / (a * (2 - a)), a, 0)
    check("R8 (1 - (1-a)^k)/(a(2-a)) -> k/2 as a -> 0 (random-walk limit)", zero(lim - kk / 2))
    # R9: the windows.
    ok = True
    for n in range(2, 401):
        same = windows("memory-halves", n) == windows("memory-before", n)
        ok &= same == (n % 2 == 0 and n <= 60)
    check("R9 the candidates' windows coincide iff n is even and n <= 60 (n = 2..400)", ok)
    # R10: the expected plug-in within-window variance, trace(H Gamma) = W - S(W)/W.
    for m in (5, 8):
        H = sp.eye(m) - sp.ones(m, m) / m
        G = sp.Matrix(m, m, lambda i, j: ph ** abs(i - j))
        Sm = m * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m) / (1 - ph) ** 2
        check(f"R10 E sum (x - xbar)^2 = sigma^2 (m - S(m)/m), m = {m}", zero(sp.cancel((H * G).trace() - (m - Sm / m))))
    # R12: the Wilson upper end rises with k; the binomial cdf falls with p.
    ok = True
    for n in (300, 1000, 2000):
        _, hi = wilson(np.arange(n + 1), n, LEVEL)
        ok &= bool(np.all(np.diff(hi) > 0))
        _, hi8 = wilson(np.arange(n + 1), n, 0.80)
        lo8, _ = wilson(np.arange(n + 1), n, 0.80)
        ok &= bool(np.all(np.diff(hi8) > 0) and np.all(np.diff(lo8[1:]) > 0))
    pg = np.linspace(0.15, 0.30, 301)
    ok &= bool(np.all(np.diff(stats.binom.cdf(260, 2000, pg)) < 0))
    check("R12 Wilson ends increase in k (n = 300, 1000, 2000); P(K <= c) falls in p (grid)", ok)
    # R15: the two-sided share rises with |delta|; E z^2 = 1 + E delta^2.
    f2 = sp.Rational(1, 2) * (sp.erfc((c + d) / sp.sqrt(2)) + sp.erfc((c - d) / sp.sqrt(2)))
    dens = lambda t: sp.exp(-(t**2) / 2) / sp.sqrt(2 * sp.pi)  # noqa: E731
    check("R15 d/d delta P(|Z + delta| > c) = phi(c - delta) - phi(c + delta)", zero(sp.diff(f2, d) - (dens(c - d) - dens(c + d))))
    check("R15 phi(c - delta)/phi(c + delta) = exp(2 c delta) >= 1", zero(sp.simplify(dens(c - d) / dens(c + d)) - sp.exp(2 * c * d)))
    from sympy import stats as sst

    mu, tau = sp.symbols("mu tau", positive=True)
    Zs, Ds = sst.Normal("Z", 0, 1), sst.Normal("D", mu, tau)
    check("R15 E (Z + D)^2 = 1 + E D^2 (sympy.stats, Z independent of D)",
          zero(sst.E((Zs + Ds) ** 2) - (1 + mu**2 + tau**2)))
    # R16: the lag-1 statistic's bias with each block of m = 5 steps centred on its own mean.
    m = 5
    H = sp.eye(m) - sp.ones(m, m) / m  # Cov of the centred block, in units of sigma^2
    num = sum(H[t, t + 1] for t in range(m - 1))
    den_pairs = sum(H[t, t] for t in range(m - 1))
    den_all = sum(H[t, t] for t in range(m))
    check("R16 per-block centring: ratio of expectations = -1/(m-1) = -1/4", zero(num / den_pairs + sp.Rational(1, 4)))
    check("R16 with all m squares in the denominator it is -1/m = -1/5", zero(num / den_all + sp.Rational(1, 5)))
    # R16b: Corr(d_t, d_{t+1}) = -a/2 under (M2).
    sg2 = sp.symbols("sigma2", positive=True)
    se2 = a * (2 - a) * sg2
    cov = a**2 * (1 - a) * sg2 - a * se2  # E[(-a e + eps)(-a(1-a) e - a eps + eps')]
    check("R16b Cov(d_t, d_t+1) = -a^2 sigma^2 and Corr = -a/2", zero(cov + a**2 * sg2) and zero(cov / (2 * a * sg2) + a / 2))
    # R17: the share of false 'falling' headlines.
    f, det, pi = sp.symbols("f d pi", positive=True)
    share = (1 - pi) * f / ((1 - pi) * f + pi * det)
    check("R17 false headlines equal true ones iff f = pi d/(1 - pi): 0.20 at pi = 0.2, d = 0.8",
          zero(sp.solve(sp.Eq(share, sp.Rational(1, 2)), f)[0] - pi * det / (1 - pi))
          and zero((pi * det / (1 - pi)).subs({pi: sp.Rational(1, 5), det: sp.Rational(4, 5)}) - sp.Rational(1, 5)))


# ---- 2. simulation ----------------------------------------------------------------------------


def ar1(rng, n_players, n_games, phi, sigma):
    e = np.empty((n_players, n_games))
    e[:, 0] = rng.normal(0, sigma, n_players)
    sd = sigma * math.sqrt(1 - phi * phi)
    for t in range(1, n_games):
        e[:, t] = phi * e[:, t - 1] + rng.normal(0, sd, n_players)
    return e


def simulation() -> None:
    # S1: a Gaussian AR(1) with known phi and sigma (Results 5 to 8, 11, Finding 1).
    rng = np.random.default_rng(20261128)
    N, phi, sigma = 20_000, 0.984, 31.0
    e = ar1(rng, N, 250, phi, sigma)
    for rule in ("memory-halves", "memory-before"):
        for n in CHECKS:
            A, B = windows(rule, n)
            diff = e[:, B[0] : B[1]].mean(1) - e[:, A[0] : A[1]].mean(1)
            want = diff_var(sigma**2, phi, A, B)
            got = diff.var(ddof=1)
            se = want * math.sqrt(2 / (N - 1))
            h = Z80 * math.sqrt(want)
            fall, rise = np.mean(diff < -h), np.mean(diff > h)
            se_p = math.sqrt(0.09 / N)
            check(
                f"S1 {rule} {n}: Var(diff) and the 0.10 shares with known parameters",
                abs(got - want) < 4 * se and abs(fall - 0.10) < 4 * se_p and abs(rise - 0.10) < 4 * se_p,
                f"var {got:.1f} vs {want:.1f}, falling {fall:.4f}, rising {rise:.4f}",
            )
    # Result 3: E d^2 = 2 a sigma^2 in the AR(1).
    per = np.mean(np.diff(e, axis=1) ** 2, axis=1)  # one mean per independent player
    d2, se = per.mean(), per.std(ddof=1) / math.sqrt(N)
    want = 2 * (1 - phi) * sigma**2
    noise_only = (1 - phi) * (1 + phi) * sigma**2  # K^2 v: what (M3) takes s^2 to be
    check("S1 Result 3: mean d^2 = 2 a sigma^2 (and not K^2 v)", abs(d2 - want) < 4 * se and abs(d2 - noise_only) > 4 * se,
          f"{d2:.3f} vs {want:.3f} (K^2 v {noise_only:.3f}, SE {se:.3f})")
    # Result 16b: the step's lag-1 is -a/2.
    dd_ = np.diff(e, axis=1)
    lag = float(np.corrcoef(dd_[:, :-1].ravel(), dd_[:, 1:].ravel())[0, 1])
    check("S1 Result 16b: step lag-1 = -a/2", abs(lag + (1 - phi) / 2) < 4 / math.sqrt(dd_[:, :-1].size),
          f"{lag:.4f} vs {-(1 - phi) / 2:.4f}")
    # Result 10: the expected plug-in within-window variance.
    x = e[:, :W]
    ss = ((x - x.mean(1, keepdims=True)) ** 2).sum(1)
    want = sigma**2 * (W - block(W, phi) / W)
    check("S1 Result 10: E sum (x - xbar)^2 = sigma^2 (W - S/W)", abs(ss.mean() - want) < 4 * ss.std() / math.sqrt(N),
          f"{ss.mean():.0f} vs {want:.0f}")
    # Finding 1: the lag-1 estimate from 250 games, centred on the series' mean.
    # EXP-0015's estimator (its analysis.py: np.corrcoef(row[:-1], row[1:]) per series, then the mean).
    xa = e[:, :-1] - e[:, :-1].mean(1, keepdims=True)
    xb = e[:, 1:] - e[:, 1:].mean(1, keepdims=True)
    r1 = (xa * xb).sum(1) / np.sqrt((xa**2).sum(1) * (xb**2).sum(1))
    bias = r1.mean() - phi
    se = r1.std() / math.sqrt(N)
    exp15 = json.loads((HERE.parent / "EXP-0015" / "results.json").read_text(encoding="utf-8"))["diagnostics"]["settled_error_lag1"]
    check("S1 Finding 1: EXP-0015's lag-1 estimator at phi = 0.984, n = 250: mean 0.9655 = EXP-0015's measured value; bias -0.0185, not -0.015",
          abs(r1.mean() - exp15) < 0.002 and abs(bias + 0.0185) < 0.001 and abs(bias + 0.015) > 4 * se,
          f"mean {r1.mean():.4f} (EXP-0015 {exp15}), bias {bias:.4f} (SE {se:.4f})")
    xc = e - e.mean(1, keepdims=True)
    r_ratio = ((xc[:, 1:] * xc[:, :-1]).sum(1) / (xc**2).sum(1)).mean()
    check("S1 Finding 1: the ratio form sum(x_t x_t+1)/sum(x_t^2) would give 0.960", near(0.960, r_ratio, 3), f"{r_ratio:.4f}")

    # S2: the Elo world the model linearises (Results 1 to 4, 9): constant strength, K = 11.6,
    # opponents N(r, 75), their strength N(their rating, 50), draws 0.24 E(1 - E), ratings
    # stored rounded, 300 burn-in games. The rule is applied with (E1) to (E6).
    rng = np.random.default_rng(30)
    N, K, burn, G = 20_000, 11.6, 300, 250
    r = np.full(N, 1500.0)
    obs = np.empty((N, G))
    pq, vv = [], []
    for t in range(burn + G):
        ro = r + rng.normal(0, 75, N)
        to = ro + rng.normal(0, 50, N)
        et = 1 / (1 + 10 ** ((to - 1500.0) / 400))
        dr = 0.24 * et * (1 - et)
        uu = rng.random(N)
        res = np.where(uu < et - dr / 2, 1.0, np.where(uu < et + dr / 2, 0.5, 0.0))
        er = 1 / (1 + 10 ** ((ro - r) / 400))
        if t >= burn:
            obs[:, t - burn] = np.floor(r + 0.5)
            pq.append(np.mean(er * (1 - er)))
            vv.append(np.mean(et * (1 - et) - dr / 4))
        r = r + K * (res - er)
    pqm, vm = float(np.mean(pq)), float(np.mean(vv))
    a_lin = Q * K * pqm
    sig_lin = math.sqrt(K * K * vm / (a_lin * (2 - a_lin)))
    err_sd = float((obs - 1500).std())
    extra = (Q * K * pqm * 50) ** 2  # the opponent's strength error, linearised: an extra innovation
    sig_lin2 = math.sqrt((K * K * vm + extra) / (a_lin * (2 - a_lin)))
    check("S2 Result 1: with the opponent-error innovation the linearised sigma is within 2% of the Elo world",
          abs(err_sd / sig_lin2 - 1) < 0.02, f"{err_sd:.2f} vs {sig_lin2:.2f} (extra variance {extra:.2f} of {K * K * vm:.2f})")
    check("S2 Result 2: the error SD of the Elo world is the linearised sigma within 4%",
          abs(err_sd / sig_lin - 1) < 0.04, f"{err_sd:.2f} vs {sig_lin:.2f} (E p(1-p) {pqm:.4f}, v {vm:.4f})")
    for rule in ("memory-halves", "memory-before"):
        for n in CHECKS:
            xx = obs[:, :n]
            s = np.sqrt((np.diff(xx, axis=1) ** 2).mean(1))
            _, ph_, s2 = params(s)
            A, B = windows(rule, n)
            diff = xx[:, B[0] :].mean(1) - xx[:, A[0] : A[1]].mean(1)
            h = Z80 * np.sqrt(diff_var(s2, ph_, A, B))
            fall, rise = float(np.mean(diff < -h)), float(np.mean(diff > h))
            hw, honest = float(h.mean()), Z80 * float(diff.std())
            se_p = math.sqrt(0.09 / N)
            check(
                f"S2 Result 11, {rule} {n}: the plug-in rule in the Elo world, shares within 0.01 of 0.10, "
                "half-width within 1.3% of honest",
                abs(fall - 0.10) <= 0.01 and abs(rise - 0.10) <= 0.01 and abs(hw / honest - 1) <= 0.013,
                f"falling {fall:.4f}, rising {rise:.4f}, half-width {hw:.2f} vs {honest:.2f}",
            )
    # Result 8's elasticity of the variance in a at fixed s (Gaussian approximation of the share).
    for n in CHECKS:
        A, B = windows("memory-halves", n)
        s0 = 5.45
        a0, _, _ = params(s0)

        def var_at(aa):
            return diff_var(s0**2 / (aa * (2 - aa)), 1 - aa, A, B)

        el = (math.log(var_at(a0 * 1.001)) - math.log(var_at(a0 / 1.001))) / (2 * math.log(1.001))
        check(f"S2 Result 8: elasticity of Var in a at fixed s, {n} games", near({60: -0.31, 120: -0.64, 250: -1.01}[n], el, 2), f"{el:.3f}")

    # S3: the cohort's z with independent real change (Result 15).
    rng = np.random.default_rng(15)
    N = 400_000
    z0 = rng.normal(0, 1, N)
    dl = rng.normal(0.4, 0.8, N)
    z = z0 + dl
    two0, two = np.mean(np.abs(z0) > Z80), np.mean(np.abs(z) > Z80)
    check("S3 Result 15: two-sided share with change >= without; one-sided falling drops below 0.10",
          two > two0 and two > 0.2 and np.mean(z < -Z80) < 0.10, f"{two0:.4f} -> {two:.4f}, falling {np.mean(z < -Z80):.4f}")
    check("S3 Result 15: E z^2 = 1 + E delta^2", abs(np.mean(z * z) - (1 + 0.4**2 + 0.8**2)) < 4 * np.std(z * z) / math.sqrt(N))

    # S4: the lag-1 statistic on iid five-step blocks (Result 16).
    rng = np.random.default_rng(16)
    b = rng.normal(0, 6, (200_000, 5))
    bc = b - b.mean(1, keepdims=True)
    per_block = float(np.corrcoef(bc[:, :-1].ravel(), bc[:, 1:].ravel())[0, 1])
    pooled = float(np.corrcoef(b[:, :-1].ravel(), b[:, 1:].ravel())[0, 1])
    se = 1 / math.sqrt(b[:, :-1].size)
    check("S4 Result 16: per-block centring -1/4, pooled 0", abs(per_block + 0.25) < 4 * se * 1.5 and abs(pooled) < 4 * se,
          f"{per_block:.4f}, {pooled:.4f}")

    # S5: the decision rule's size on counts (Result 12): one cell at 0.15, the others at 0.10.
    rng = np.random.default_rng(12)
    S = 400_000
    c = cut(2000, LEVEL, BOUND)
    clear = np.ones(S, dtype=bool)
    for g in range(3):
        p_f = 0.15 if g == 1 else 0.10
        k = rng.multinomial(2000, [p_f, 0.10, 1 - p_f - 0.10], size=S)
        clear &= (k[:, 0] <= c) & (k[:, 1] <= c)
    size = float(stats.binom.cdf(c, 2000, 0.15))
    check("S5 Result 12: P(clear) with one cell at 0.15 is at most the exact cell size",
          clear.mean() <= size + 4 * math.sqrt(size / S), f"{clear.mean():.5f} <= {size:.5f}")


# ---- 3. numbers ------------------------------------------------------------------------------


def numbers() -> None:
    cands = R["candidates"]
    # Every co-primary cell: the three Wilson intervals and the read.
    ok = True
    for name, cnd in cands.items():
        for cell in cnd["primary"]:
            k, n = cell["k"], cell["n"]
            for key, lev in (("ci95", 0.95), ("ci80", 0.80), ("ci_decision", LEVEL)):
                lo, hi = wilson(k, n, lev)
                ok &= abs(float(lo) - cell[key][0]) < 1e-6 and abs(float(hi) - cell[key][1]) < 1e-6
            lo, hi = cell["ci_decision"]
            read = ("above the hide line" if lo > HIDE else "above the bound" if lo > BOUND
                    else "within the bound" if hi <= BOUND else "undecided")
            ok &= read == cell["read"] and abs(k / n - cell["share"]) < 1e-6
    check("N co-primary cells: 24 Wilson intervals at 95%, 80%, 1 - 0.025/2 and the reads", ok)
    # Detection, rough and chess.com-like cells and their guards.
    ok = True
    for cnd in cands.values():
        for cell in cnd["detection"] + cnd["rough"] + cnd["chesscom"]:
            for key, lev in (("ci95", 0.95), ("ci80", 0.80)):
                lo, hi = wilson(cell["k"], cell["n"], lev)
                ok &= abs(float(lo) - cell[key][0]) < 1e-6 and abs(float(hi) - cell[key][1]) < 1e-6
        for cell in cnd["detection"]:
            if cell["decline"] == 200:
                ok &= (cell["guard"] == "passes") == (cell["ci80"][0] >= 0.50)
        for cell in cnd["rough"] + cnd["chesscom"]:
            ok &= (cell["guard"] == "fails") == (cell["share"] > BOUND or cell["ci80"][1] > HIDE)
    check("N detection, rough and chess.com-like intervals and guards recomputed", ok)
    # The arena rule from the committed flags.
    ok = True
    fl = R["flags"]
    details = []
    for name, cnd in cands.items():
        for g in CHECKS:
            ch = [b == "1" for b in fl["peak-bootstrap"]["falling"][str(g)]]
            cl = [b == "1" for b in fl[name]["falling"][str(g)]]
            n11 = sum(a and b for a, b in zip(ch, cl))
            n10 = sum((not a) and b for a, b in zip(ch, cl))
            n01 = sum(a and (not b) for a, b in zip(ch, cl))
            n00 = len(ch) - n11 - n10 - n01
            d, lo, hi = newcombe_paired(n11, n10, n01, n00, 1 - 0.05 / 3)
            ar = cnd["arena"][str(g)]
            ok &= abs(d - ar["effect"]) < 1e-6 and abs(lo - ar["interval"][0]) < 1e-6 and abs(hi - ar["interval"][1]) < 1e-6
            ok &= hi < 0 and ar["verdict"] == "better"
            if name == "memory-halves":
                details.append(f"{d:.2f} [{lo:.2f}, {hi:.2f}]")
            ok &= sum(cl) == next(c["k"] for c in cnd["primary"] if c["games"] == g and c["side"] == "falling")
    check("N arena: Newcombe (method 10) at 1 - 0.05/3 from the flags", ok, "; ".join(details))
    quoted = [(-0.35, -0.38, -0.33), (-0.52, -0.55, -0.50), (-0.69, -0.71, -0.66)]
    ok = all(
        near(qd, cands["memory-halves"]["arena"][str(g)]["effect"], 2)
        and near(ql, cands["memory-halves"]["arena"][str(g)]["interval"][0], 2)
        and near(qh, cands["memory-halves"]["arena"][str(g)]["interval"][1], 2)
        for g, (qd, ql, qh) in zip(CHECKS, quoted)
    )
    check("N quoted arena changes -0.35, -0.52, -0.69 and intervals", ok)
    eff = [cands["memory-halves"]["arena"][str(g)]["effect"] for g in CHECKS]
    check("N 'beats the champion by 0.35 to 0.69'", near(0.35, -max(eff), 2) and near(0.69, -min(eff), 2))
    # The candidates' 60-game cells are identical (Result 9), in the flags too.
    same = all(fl["memory-halves"][s]["60"] == fl["memory-before"][s]["60"] for s in ("falling", "rising"))
    g30 = R["exploratory"]["games_30"]
    same &= g30["memory-halves"] == g30["memory-before"]
    check("N Result 9: the two candidates' 60- and 30-game verdicts are identical player by player", same)
    # The co-primary table, as quoted.
    tab = {
        "memory-halves": {60: (0.103, 0.087, 0.121, 0.089, 0.074, 0.106), 120: (0.117, 0.100, 0.136, 0.084, 0.070, 0.101),
                          250: (0.105, 0.089, 0.123, 0.095, 0.079, 0.112)},
        "memory-before": {120: (0.109, 0.092, 0.127, 0.088, 0.073, 0.105), 250: (0.101, 0.085, 0.119, 0.101, 0.085, 0.119)},
    }
    ok = True
    for name, rows in tab.items():
        for g, (fs, fl_, fh, rs, rl, rh) in rows.items():
            cf = next(c for c in cands[name]["primary"] if c["games"] == g and c["side"] == "falling")
            cr = next(c for c in cands[name]["primary"] if c["games"] == g and c["side"] == "rising")
            ok &= near(fs, cf["share"], 3) and near(fl_, cf["ci_decision"][0], 3) and near(fh, cf["ci_decision"][1], 3)
            ok &= near(rs, cr["share"], 3) and near(rl, cr["ci_decision"][0], 3) and near(rh, cr["ci_decision"][1], 3)
    worst = max(c["ci_decision"][1] for c in cands["memory-halves"]["primary"])
    ok &= near(0.136, worst, 3)
    ch = R["champion"]
    ok &= all(near(q, c["share"], 3) for q, c in zip((0.458, 0.640, 0.795), ch["falling"]))
    ok &= all(near(q, c["share"], 3) for q, c in zip((0.411, 0.294, 0.164), ch["rising"]))
    ok &= near(0.7945, R["diagnostics"]["positive_control_falling_250"], 4) and R["diagnostics"]["positive_control_falling_250"] > 0.5
    check("N co-primary table, worst upper end 0.136, champion cells, positive control 0.79 > 0.5", ok)
    summ = [c["share"] * 100 for c in cands["memory-halves"]["primary"]]
    ok = all(near(q, v, 1) for q, v in zip((10.3, 8.9, 11.7, 8.4, 10.5, 9.5), summ))
    ok &= near(46, ch["falling"][0]["share"] * 100, 0) and near(79, ch["falling"][2]["share"] * 100, 0)
    check("N summary: 10.3%, 11.7%, 10.5% falling; 8.9%, 8.4%, 9.5% rising; champion 46% to 79%", ok)
    # Calibration (Result 7's prediction from the median RMS step).
    diag = R["diagnostics"]
    iv = diag["interval"]
    ok = all(near(q, iv["memory-halves"][str(g)]["half_width_mean"], 1) for q, g in zip((26.5, 43.8, 50.9), CHECKS))
    ok &= all(near(q, iv["memory-halves"][str(g)]["honest_half_width"], 1) for q, g in zip((26.3, 44.6, 50.9), CHECKS))
    ok &= all(near(3.7, iv["peak-bootstrap"][str(g)]["half_width_mean"], 1) for g in CHECKS)
    ok &= all(abs(iv[n][str(g)]["honest_half_width"] - Z80 * iv[n][str(g)]["diff_sd"]) < 1e-5 for n in iv for g in CHECKS)
    check("N calibration: 26.5, 43.8, 50.9 against 26.3, 44.6, 50.9; champion 3.7", ok)
    s_med = diag["steps"]["settled"]["step_rms_median"]
    a_, ph_, s2_ = params(s_med)
    ok = abs(math.sqrt(float(s2_)) / diag["settled_error_sd"] - 1) < 0.01
    pred = []
    for rule in ("memory-halves", "memory-before"):
        for g in CHECKS:
            A, B = windows(rule, g)
            h = Z80 * math.sqrt(float(diff_var(s2_, ph_, A, B)))
            pred.append(h)
            ok &= abs(h / iv[rule][str(g)]["half_width_mean"] - 1) < 0.01
    W_ = W
    boot = Z80 * math.sqrt(2 * float(s2_) * (W_ - block(W_, float(ph_)) / W_) / W_**2)
    ok &= all(abs(boot / iv["peak-bootstrap"][str(g)]["half_width_mean"] - 1) < 0.06 for g in CHECKS)
    check("N Results 7, 10: predicted from the median step 5.45: sigma 30.9 vs 30.94; half-widths within 1%; "
          "bootstrap within 6%", ok, f"sigma {math.sqrt(float(s2_)):.2f}, {', '.join(f'{x:.2f}' for x in pred)}, boot {boot:.2f}")
    ratio = (W_**2 - block(W_, float(ph_))) / (W_ * block(W_, float(ph_)))
    check("N Result 10: bootstrap variance / true variance of a 30-game mean, 0.0055 and 13.5", near(0.0055, ratio, 4) and near(13.5, 1 / math.sqrt(ratio), 1),
          f"{ratio:.4f}, the SE {1 / math.sqrt(ratio):.1f} times too small")
    # The 120-game asymmetry and the mean differences.
    hv = cands["memory-halves"]["primary"]
    ok = next(c["k"] for c in hv if c["games"] == 120 and c["side"] == "falling") == 234
    ok &= next(c["k"] for c in hv if c["games"] == 120 and c["side"] == "rising") == 168
    ok &= near(-2.3, iv["memory-halves"]["120"]["diff_mean"], 1)
    mc = R["exploratory"]["mean_change"]["memory-halves"]
    ok &= all(near(q, m["when_falling"], 0) for q, m in zip((-37, -62, -70), mc))
    check("N 234 against 168 at 120 games, mean difference -2.3; mean change when falling -37, -62, -70", ok)
    # Detection, as quoted.
    det = {(c["decline"], c["games"]): c for c in cands["memory-halves"]["detection"]}
    detb = {(c["decline"], c["games"]): c for c in cands["memory-before"]["detection"]}
    ok = near(0.72, det[200, 60]["share"], 2) and near(0.68, det[200, 60]["ci80"][0], 2) and near(0.75, det[200, 60]["ci80"][1], 2)
    ok &= near(0.84, det[200, 120]["share"], 2) and near(0.81, det[200, 120]["ci80"][0], 2) and near(0.87, det[200, 120]["ci80"][1], 2)
    ok &= near(0.99, det[200, 250]["share"], 2) and near(0.98, det[200, 250]["ci80"][0], 2)
    ok &= all(near(q, det[100, g]["share"], 2) for q, g in zip((0.36, 0.50, 0.72), CHECKS))
    ok &= all(near(q, detb[200, g]["share"], 2) for q, g in zip((0.72, 0.88, 0.94), CHECKS))
    ok &= all(near(q, det[200, g]["false_alarm_same_length"], 3) for q, g in zip((0.103, 0.117, 0.105), CHECKS))
    check("N detection table (Finding 4: the 250-game upper end is 0.995, quoted 0.99)", ok,
          f"upper end at 250 games {det[200, 250]['ci80'][1]:.4f}")
    check("N Finding 4: the quoted 0.99 upper end does not round from 0.9951", not near(0.99, det[200, 250]["ci80"][1], 2))
    # Result 17: the headline arithmetic.
    f60 = det[200, 60]["false_alarm_same_length"]
    h200 = 0.8 * f60 / (0.8 * f60 + 0.2 * det[200, 60]["share"])
    h100 = 0.8 * f60 / (0.8 * f60 + 0.2 * det[100, 60]["share"])
    pre37 = 0.8 * 0.10 / (0.8 * 0.10 + 0.2 * 0.67)
    pre47 = 0.8 * 0.15 / (0.8 * 0.15 + 0.2 * 0.67)
    ok = near(37, pre37 * 100, 0) and near(47, pre47 * 100, 0) and near(53, h100 * 100, 0)
    ok &= near(0.08, 0.8 * f60, 2) and near(0.14, 0.2 * det[200, 60]["share"], 2)
    check("N Result 17: preregistration's 37% and 47%; decision's 53%, 0.08 against 0.14", ok,
          f"{pre37:.3f}, {pre47:.3f}, {h100:.3f}")
    check("N the decision's 'about 37%' at 60 games: 0.0824 / (0.0824 + 0.1433) = 36.5%",
          near(37, h200 * 100, 0), f"{h200:.4f}")
    # The rough and chess.com-like worlds.
    rough = [c["share"] for c in cands["memory-halves"]["rough"]]
    ok = all(near(q, v, 3) for q, v in zip((0.125, 0.137, 0.131, 0.141, 0.131, 0.144), rough))
    ok &= near(0.159, max(c["ci80"][1] for c in cands["memory-halves"]["rough"]), 3)
    ok &= near(1.3, min(rough) / 0.10, 1) and near(1.4, max(rough) / 0.10, 1)
    ok &= near(13, min(rough) * 100, 0) and near(14, max(rough) * 100, 0)
    g1 = [c["share"] for c in cands["memory-halves"]["chesscom"]]
    ok &= near(0.086, min(g1), 3) and near(0.107, max(g1), 3) and near(9, min(g1) * 100, 0) and near(11, max(g1) * 100, 0)
    ok &= near(0.141, max(c["share"] for c in cands["memory-before"]["rough"]), 3)
    check("N rough world 0.125 to 0.144, max 80% upper 0.159; chess.com-like 0.086 to 0.107", ok)
    # The cohort.
    co = R["cohort"]
    st = co["settled"]
    ok = near(0.356, st["two_sided"]["share"], 3) and near(0.349, st["two_sided"]["ci_transfer"][0], 3)
    ok &= near(0.363, st["two_sided"]["ci_transfer"][1], 3) and st["two_sided"]["ci_transfer"][0] <= 0.40
    lo, hi = wilson(st["two_sided"]["k"], st["n"] if "n" in st else st["players"], 0.95)
    ok &= abs(float(lo) - st["two_sided"]["ci_transfer"][0]) < 1e-6
    ok &= near(0.113, st["falling"]["share"], 3) and near(0.243, st["rising"]["share"], 3)
    ok &= near(2.37, st["mean_z2"], 2) and near(3.10, co["settled_pooled_rms"]["mean_z2"], 2)
    ok &= near(1.5, math.sqrt(st["mean_z2"]), 1) and st["players"] == 16445 and co["games_median"] == 74
    ok &= near(0.036, co["step_lag1"], 3) and near(0.029, co["sign_lag1"], 3)
    ok &= near(-0.008, R["diagnostics"]["steps"]["settled"]["step_lag1"], 3)
    ok &= near(0.020, R["diagnostics"]["steps"]["rough"]["step_lag1"], 3)
    ok &= near(-0.026, R["diagnostics"]["steps"]["glicko1"]["step_lag1"], 3)
    ok &= st["falling"]["k"] + st["rising"]["k"] == st["two_sided"]["k"]
    check("N cohort: 0.356 [0.349, 0.363], 0.113 and 0.243, z^2 2.37 and 3.10, 16,445, lag-1 0.036", ok)
    # Exploratory numbers.
    ex = R["exploratory"]
    ok = near(0.094, ex["games_30"]["memory-halves"]["falling"]["share"], 3)
    ok &= near(0.090, ex["games_30"]["memory-halves"]["rising"]["share"], 3)
    ok &= near(0.41, ex["games_30"]["peak-bootstrap"]["falling"]["share"], 2) and near(0.41, ex["games_30"]["peak-bootstrap"]["rising"]["share"], 2)
    tn = [x["share"] for x in ex["then_now_rows"]["memory-halves"]]
    ok &= near(0.087, min(tn), 3) and near(0.107, max(tn), 3)
    ok &= near(0.81, ex["rough_decline"]["memory-halves"]["share"], 2)
    elo = [c[r][s]["share"] for arm in ex["other_ratings"] for c in arm["cells"] for r in ["memory-halves"] for s in ("falling", "rising")]
    ok &= near(0.07, min(elo), 2) and near(0.14, max(elo), 2)
    new = [c["memory-halves"][s]["share"] for c in ex["new_account"] for s in ("falling", "rising")]
    ok &= near(0.09, min(new), 2) and near(0.18, max(new), 2)
    ok &= near(0.177, ex["new_account"][2]["memory-halves"]["falling"]["share"], 3)
    ok &= near(0.167, ex["new_account"][2]["memory-halves"]["rising"]["share"], 3)
    ok &= all(near(0.03, ex["new_account"][2]["memory-before"][s]["share"], 2) for s in ("falling", "rising"))
    ok &= near(0.11, ex["other_ratings"][1]["cells"][2]["memory-before"]["falling"]["share"], 2)
    form = [c[r][s]["share"] for row in ex["rough_form"] for c in row["cells"] for r in ["memory-halves"] for s in ("falling", "rising")]
    ok &= near(0.09, min(form), 2) and near(0.12, max(form), 2)
    check("N exploratory: 30 games, rows, rough decline 0.81, Elo 0.07 to 0.14, new accounts, form SD", ok)
    # Burn-in and deviations.
    ok = near(1.0, diag["settled"]["mean_error_at_start"], 1) and near(2.0, diag["glicko1"]["mean_error_at_start"], 1)
    ok &= near(-13.7, diag["rough"]["mean_error_at_start"], 1)
    dec = [v["mean_error_at_start"] for v in diag["decline"].values()]
    ok &= near(-3.3, min(dec), 1) and near(4.3, max(dec), 1)
    ok &= near(45, diag["settled_rd_mean"], 0) and near(49.8, diag["rough_rd_mean"], 1) and near(218, diag["rough_days_median"], 0)
    check("N burn-in errors +1.0, -3.3 to +4.3, +2.0, -13.7; deviations 45 and 49.8; 218 days", ok)
    # The decision, recomputed from section 9.
    ok = True
    for name, cnd in cands.items():
        cl = cnd["checks"]
        ok &= cl["primary_within"] == all(c["read"] == "within the bound" for c in cnd["primary"])
        ok &= cl["transfer_guard"] == (co["settled"]["two_sided"]["ci_transfer"][0] <= 0.40)
        ok &= cnd["clears"] == all(cl.values())
    first = next(c for c in ("memory-halves", "memory-before") if cands[c]["clears"])
    ok &= R["decision"] == f"ship {first}" == "ship memory-halves"
    check("N the decision: both clear, the fixed order ships memory-halves", ok)
    # The budget (section 0), recomputed from the design.
    sim = (2000 * 550 + 2 * 300 * (360 + 420 + 550) + 1000 * 550 + 1000 * 550 + 300 * 420 + 2 * 500 * 550
           + 300 * 250 + 2 * 300 * 550)
    traj = 3 * (2000 * 4 + 6 * 300 + 1000 * 3 + 1000 * 3 + 300 + 2 * 500 * 3 + 2 * 300 * 3 + 300 * 3)
    comp = R["compute"]["actual"]
    ok = sim == comp["simulated_games"] == 4_079_000 and traj == comp["trajectory_players"] == 65_400
    ok &= comp["cohort_lines"] == 3_955_242 and near(0.24, 858 / 3600, 2)
    check("N budget: 4,079,000 games and 65,400 trajectories from the design; 858 s is 0.24 CPU-hours", ok)
    check("N summary run_minutes 15: 858 s is 14.3 minutes (rounded up; Finding 4)", near(14, 858 / 60, 0))
    # The beacon round and the namespace.
    genesis, freeze = 1692803367, 1795739570
    rnd = math.ceil((freeze + 600 - genesis) / 3) + 1
    ns = (HERE / "namespace.txt").read_text(encoding="utf-8").strip()
    ok = rnd == 34312269 and genesis + (rnd - 1) * 3 == 1795740171 and ns == "EXP-0030-fb2f150c7574"
    ok &= R["design"]["seed_namespace"] == ns
    check("N drand round 34312269 at 1795740171; namespace EXP-0030-fb2f150c7574", ok)
    # power.json against the exact power of Result 13.
    ok = True
    worst = 0.0
    for row in PJ["primary"]:
        n, p = row["players"], row["true_rate_each_cell"]
        ex_ = both_within(n, p, cut(n, LEVEL, BOUND)) ** 3
        se = math.sqrt(max(ex_ * (1 - ex_), 1e-12) / PJ["sims"])
        z = abs(row["all_six_within"] - ex_) / se if ex_ * (1 - ex_) > 1e-9 else 0.0
        ok &= z < 4 or abs(row["all_six_within"] - ex_) < 1e-4
        worst = max(worst, z)
    for row in PJ["detection_guard"]:
        n, dd = row["players_a_length"], row["true_detection_each_length"]
        lo, _ = wilson(np.arange(n + 1), n, 0.80)
        cd = int(np.arange(n + 1)[lo >= 0.5].min())
        ex_ = stats.binom.sf(cd - 1, n, dd) ** 3
        ok &= abs(row["guard_passes"] - ex_) < 4 * math.sqrt(max(ex_ * (1 - ex_), 1e-12) / PJ["sims"]) + 1e-4
    for row in PJ["world_guard"]:
        n, rr = row["players"], row["true_rate_each_cell"]
        k = np.arange(n + 1)
        _, hi = wilson(k, n, 0.80)
        cw = int(k[(k / n <= BOUND) & (hi <= HIDE)].max())
        ex_ = both_within(n, rr, cw) ** 3
        ok &= abs(row["guard_passes"] - ex_) < 4 * math.sqrt(max(ex_ * (1 - ex_), 1e-12) / PJ["sims"]) + 1e-4
    check("N power.json: every grid point within 4 SE of the exact power (Result 13)", ok, f"largest |z| {worst:.2f}")

    def pw(n, p):
        return next(r["all_six_within"] for r in PJ["primary"] if r["players"] == n and r["true_rate_each_cell"] == p)

    ok = near(0.99, pw(2000, 0.11), 2) and near(0.89, pw(1500, 0.11), 2) and near(0.25, pw(1500, 0.12), 2)
    ok &= near(0.60, pw(2000, 0.12), 2) and near(1.00, pw(2000, 0.10), 2) and near(0.92, pw(1000, 0.10), 2)
    ok &= near(0.015, pw(2000, 0.13), 3)
    ok &= min(n for n in (700, 1000, 1500, 2000, 2500) if pw(n, 0.11) >= 0.80) == 1500
    dg = {r["true_detection_each_length"]: r["guard_passes"] for r in PJ["detection_guard"]}
    wg = {r["true_rate_each_cell"]: r["guard_passes"] for r in PJ["world_guard"]}
    ok &= near(0.96, dg[0.60], 2) and near(0.29, dg[0.55], 2)
    ok &= near(0.99, wg[0.12], 2) and near(0.84, wg[0.13], 2) and near(0.015, wg[0.15], 3) and wg[0.18] == 0
    check("N section 6: 0.99, 0.89, 0.25, 0.60, 1.00, 0.92, 0.015; guards 0.96, 0.29, 0.99, 0.84, 0.015, 0", ok)
    # Finding 3: the family-wise level.
    c2000 = cut(2000, LEVEL, BOUND)
    size = float(stats.binom.cdf(c2000, 2000, 0.15))
    fwer = 4 * size
    check("N Result 12: c = 260 at n = 2000; exact cell size 0.0060; family <= 4 x 0.0060 = 0.024 (Finding 3)",
          c2000 == 260 and abs(size - 0.005975) < 5e-6 and fwer < 0.025, f"size {size:.6f}, family {fwer:.4f}")
    c_one = cut(2000, 1 - 2 * 0.0125, BOUND)  # a one-sided test at 0.0125 per look
    check("N Finding 3: a one-sided reading at 0.0125 would use z = 2.24 and c = 264",
          c_one == 264 and near(2.24, float(stats.norm.ppf(1 - 0.0125)), 2), f"c = {c_one}")
    sizes = {n: float(stats.binom.cdf(cut(n, LEVEL, BOUND), n, 0.15)) for n in range(1000, 5001, 10)}
    worst_n = max(sizes, key=sizes.get)
    check("N Result 12: sup over a follow-up's N in 1,000..5,000 of the cell size; family <= 2 x 0.0060 + 2 x sup",
          sizes[worst_n] < 0.0075 and 2 * size + 2 * sizes[worst_n] < 0.027,
          f"sup {sizes[worst_n]:.5f} at N = {worst_n}, family {2 * size + 2 * sizes[worst_n]:.4f}")
    q11 = both_within(2000, 0.11, c2000)
    check("N Result 13: Frechet lower bound at 0.11 is 0.986 whatever the dependence between lengths",
          near(0.986, 1 - 3 * (1 - q11), 3), f"{1 - 3 * (1 - q11):.4f}")
    # Result 13's exact values and bounds; Result 14's cut-offs and rates.
    ex = {(n, p): both_within(n, p, cut(n, LEVEL, BOUND)) for n in (1000, 1500, 2000) for p in (0.10, 0.11, 0.12, 0.13)}
    ok = near(0.9954, ex[2000, 0.11], 4) and near(0.9862, ex[2000, 0.11] ** 3, 4) and near(0.599, ex[2000, 0.12] ** 3, 3)
    ok &= near(0.0143, ex[2000, 0.13] ** 3, 4) and near(0.8895, ex[1500, 0.11] ** 3, 4) and near(0.2472, ex[1500, 0.12] ** 3, 4)
    ok &= near(0.9218, ex[1000, 0.10] ** 3, 4) and near(0.53, 1 - 3 * (1 - ex[2000, 0.12]), 2) and near(0.84, ex[2000, 0.12], 2)
    check("N Result 13: exact 0.9954, 0.9862, 0.599, 0.0143, 0.8895, 0.2472, 0.9218; bounds 0.53 to 0.84 at 0.12", ok)
    lo300, _ = wilson(np.arange(301), 300, 0.80)
    cd = int(np.arange(301)[lo300 >= 0.5].min())
    k1000 = np.arange(1001)
    _, hi1000 = wilson(k1000, 1000, 0.80)
    cw = int(k1000[(k1000 / 1000 <= BOUND) & (hi1000 <= HIDE)].max())
    ok = cd == 162 and cw == 150 and int(k1000[hi1000 <= HIDE].max()) > 150
    ok &= near(0.955, stats.binom.sf(161, 300, 0.60) ** 3, 3) and near(0.285, stats.binom.sf(161, 300, 0.55) ** 3, 3)
    ok &= all(near(qv, both_within(1000, pv, 150) ** 3, 3 if qv > 0.1 else 4) for qv, pv in ((0.988, 0.12), (0.838, 0.13), (0.0145, 0.15)))
    check("N Result 14: cut-offs 162 of 300 and 150 of 1,000 (the share binds); 0.955, 0.285, 0.988, 0.838, 0.0145", ok)
    # Result 9: the three rules' mean difference at 60 games.
    check("N Result 9: diff_mean -1.257 at 60 games under all three rules",
          all(R["diagnostics"]["interval"][rr]["60"]["diff_mean"] == -1.257 for rr in ("peak-bootstrap", "memory-halves", "memory-before")))
    # Result 16b: the model's step lag-1 -a/2 against the plain world's.
    a_pl = float(params(R["diagnostics"]["steps"]["settled"]["step_rms_median"])[0])
    check("N Result 16b: -a/2 = -0.008 against the plain world's step lag-1 -0.0079",
          near(-0.008, -a_pl / 2, 3) and abs(-a_pl / 2 - R["diagnostics"]["steps"]["settled"]["step_lag1"]) < 0.002,
          f"{-a_pl / 2:.4f}")
    # (M1): Glicko-2's effective K at the floor (lila's constants, the starting volatility, E = 1/2).
    scale, ph0 = 173.7178, 45.0 / 173.7178
    gj = 1 / math.sqrt(1 + 3 * (60.0 / scale) ** 2 / math.pi**2)
    vv_ = 1 / (gj * gj * 0.25)
    phs2 = ph0**2 + 0.21436 * 0.5 * 0.09**2
    keff = scale * gj / (1 / phs2 + 1 / vv_)
    check("N (M1): Glicko-2's effective K = SCALE g phi'^2 at the floor is about 11.4", near(11.4, keff, 1), f"{keff:.2f}")
    # Finding 5: what the decision's "about twice" needs.
    kappa = math.sqrt(R["cohort"]["settled"]["mean_z2"])
    two = 2 * stats.norm.cdf(-Z80 / kappa)
    check("N Finding 5: a uniform variance inflation of 2.37 gives a two-sided 0.41 (about twice 0.20), 0.20 a side",
          near(0.41, two, 2) and near(0.20, two / 2, 2), f"{two:.4f}")
    # Finding 4: the summary's "17%" and the decision's "74 games apart".
    na = R["exploratory"]["new_account"][2]["memory-halves"]
    worst_new = max(na["falling"]["share"], na["rising"]["share"])
    check("N Finding 4: the summary's '17% at 250 games from a new account' is 17.7%, which rounds to 18%",
          near(18, worst_new * 100, 0) and not near(17, worst_new * 100, 0), f"{worst_new:.4f}")
    # Decision prose: "one time in three", "95% intervals about +-0.04", the 0.14 Elo cells.
    ok = near(1 / 3, det[100, 60]["share"], 1)
    arms = {arm["arm"]: arm["cells"] for arm in R["exploratory"]["other_ratings"]}
    ok &= near(0.14, arms["elo/10"][1]["memory-halves"]["rising"]["share"], 2)
    ok &= near(0.14, arms["elo/32"][2]["memory-halves"]["falling"]["share"], 2)
    hws = [float(np.subtract(*wilson(k_, 300, 0.95)[::-1])) / 2 for k_ in (20, 30, 42)]
    ok &= all(0.025 <= h <= 0.045 for h in hws)
    check("N decision: 'one time in three' 0.363; Elo 0.14 at K = 10 rising 120 and K = 32 falling 250; 95% half-widths about 0.03 to 0.04",
          ok, ", ".join(f"{h:.3f}" for h in hws))
    # Finding 2: the preregistration's sigma and phi at K = 11.6.
    a_k = Q * 11.6 / 4
    s_k = math.sqrt((11.6 / 2) ** 2 / (a_k * (2 - a_k)))
    check("N Finding 2: at K = 11.6 under p(1-p) = var(s) = 1/4, phi = 0.983 and sigma = 31.9 (quoted 0.984, 31)",
          near(0.983, 1 - a_k, 3) and near(31.9, s_k, 1), f"phi {1 - a_k:.4f}, sigma {s_k:.2f}")


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
