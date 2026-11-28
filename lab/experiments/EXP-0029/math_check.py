"""EXP-0029's maths, checked (ADR 0118, math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 on any failure.

- R*: symbolic. Each derivation's two sides, subtracted, simplify to 0 with sympy; an
  inequality or limit sympy cannot settle is checked on a dense grid, and the line says so.
- S*: simulation. Each proof's model, from a fixed seed with a known truth; the derived bias,
  coverage, error rate or power must hold within 4 Monte Carlo standard errors.
- N*: numbers. Every number the registration (sections 0, 4, 6) and summary.md quote, and every
  derived number in power.json, recomputed from the committed inputs (EXP-0009's results.json,
  dev/exp09_activity.json, power.json's measured inputs) by code written apart from power.py.

The run has no results.json yet (the experiment is running). `result_numbers` is the stub for
the numbers the decision will quote; it reports PASS-skipped until results.json exists.
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import integrate
from scipy.stats import norm

HERE = Path(__file__).resolve().parent
R09 = json.loads((HERE.parent / "EXP-0009" / "results.json").read_text(encoding="utf-8"))
ACT = json.loads((HERE / "dev" / "exp09_activity.json").read_text(encoding="utf-8"))
PJ = json.loads((HERE / "power.json").read_text(encoding="utf-8"))

KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
QS = (*KEYS, "accuracy")
K2 = ("W4.allowed", "W5", "W6", "W7.traps", "W9a")
ALPHA, M, DELTA = 0.05, 8, 14.0
GAMMA = 1 - ALPHA / M
Z_G = norm.ppf(1 - (1 - GAMMA) / 2)
Z975 = norm.ppf(0.975)

FAILS: list[str] = []


def check(name: str, ok: bool, detail: str) -> None:
    print(f"{'PASS' if ok else 'FAIL'} {name}: {detail}")
    if not ok:
        FAILS.append(name)


def zero(expr: sp.Expr) -> bool:
    return sp.simplify(sp.expand(expr)) == 0


def close(a: float, b: float, tol: float) -> bool:
    return abs(a - b) <= tol


# ---- symbolic ---------------------------------------------------------------------------------


def r_fwl() -> None:
    """Result 1: the coefficient on the standardised exposure is s_x * sum(x~ y) / sum(x~^2)."""
    rng = np.random.default_rng(1)
    n = 400
    W = np.column_stack([np.ones(n), rng.normal(size=(n, 3))])
    x = W[:, 1] * 0.5 + rng.normal(size=n)
    y = 2 * x + W @ np.array([1, 0.3, -0.2, 0.1]) + rng.normal(size=n)
    sx = x.std(ddof=1)
    zc = (x - x.mean()) / sx
    X = np.column_stack([W[:, :1], zc, W[:, 1:]])
    b = np.linalg.lstsq(X, y, rcond=None)[0][1]
    xt = x - W @ np.linalg.lstsq(W, x, rcond=None)[0]
    b2 = sx * (xt @ y) / (xt @ xt)
    check("R1 Frisch-Waugh-Lovell", abs(b - b2) < 1e-10, f"|diff| {abs(b - b2):.1e}")


def r_hc3() -> None:
    """Result 2: the z element of the HC3 sandwich in scalar form; HC3 >= HC0."""
    rng = np.random.default_rng(2)
    n = 300
    X = np.column_stack([np.ones(n), rng.normal(size=(n, 4))])
    y = X @ np.ones(5) + rng.normal(size=n) * (1 + np.abs(X[:, 1]))
    XtXi = np.linalg.inv(X.T @ X)
    e = y - X @ (XtXi @ X.T @ y)
    h = np.einsum("ij,jk,ik->i", X, XtXi, X)
    V = XtXi @ (X.T * (e**2 / (1 - h) ** 2)) @ X @ XtXi
    Wm = np.delete(X, 1, axis=1)
    zt = X[:, 1] - Wm @ np.linalg.lstsq(Wm, X[:, 1], rcond=None)[0]
    scalar = (zt**2 * e**2 / (1 - h) ** 2).sum() / (zt @ zt) ** 2
    hc0 = (zt**2 * e**2).sum() / (zt @ zt) ** 2
    check("R2 HC3 scalar form", abs(V[1, 1] - scalar) < 1e-12 and scalar >= hc0,
          f"matrix {V[1, 1]:.6g}, scalar {scalar:.6g}, HC0 {hc0:.6g}")


def window_mean_algebra() -> dict[str, sp.Expr]:
    """(M2) and (M6) as covariances in the basis (T1, T2, u1, u2), tau = 1."""
    c, s1, s2, b, t1, t2 = sp.symbols("c s1 s2 b theta1 theta2", positive=True)
    S = sp.Matrix([[1, 1 - c, 0, 0], [1 - c, 1, 0, 0], [0, 0, s1, 0], [0, 0, 0, s2]])
    x = sp.Matrix([-1, 1, -1, 1])
    qbar = sp.Matrix([1, 1, 1, 1]) / 2
    q1 = sp.Matrix([1, 0, 1, 0])
    D = sp.Matrix([-1, 1, 0, 0])
    y = b * D / sp.sqrt(2 * c) + t1 * sp.Matrix([1, 0, 0, 0]) + t2 * sp.Matrix([0, 1, 0, 0])

    def cov(a: sp.Matrix, bb: sp.Matrix) -> sp.Expr:
        return (a.T * S * bb)[0, 0]

    def coef_per_sd(w: sp.Matrix) -> sp.Expr:
        vx, vw, cxw = cov(x, x), cov(w, w), cov(x, w)
        return (cov(y, x) * vw - cov(y, w) * cxw) / (vx * vw - cxw**2) * sp.sqrt(vx)

    return {"c": c, "s1": s1, "s2": s2, "b": b, "t1": t1, "t2": t2, "cov_x_qbar": cov(x, qbar),
            "wm": coef_per_sd(qbar), "f1": coef_per_sd(q1), "vx": cov(x, x),
            "vw": cov(qbar, qbar), "cyx": cov(y, x), "cyw": cov(y, qbar)}


def r_window_mean() -> None:
    """Result 5: Cov(x, qbar) = (s2 - s1)/2; with equal noise the window-mean coefficient is
    b sqrt(r) + (theta2 - theta1) c / sqrt(2(c + s)); the F1-level one is EXP-0009's Result 6."""
    a = window_mean_algebra()
    c, s1, s2, b, t1, t2 = (a[k] for k in ("c", "s1", "s2", "b", "t1", "t2"))
    s = sp.symbols("s", positive=True)
    ok1 = zero(a["cov_x_qbar"] - (s2 - s1) / 2)
    wm_eq = a["wm"].subs({s1: s, s2: s})
    want = b * sp.sqrt(c / (c + s)) + (t2 - t1) * c / sp.sqrt(2 * (c + s))
    f2 = sp.lambdify((c, s, b, t1, t2), wm_eq - want)
    ok2 = zero(sp.simplify(wm_eq - want)) or max(
        abs(f2(cc, ss, bb, a1, a2)) for cc in np.linspace(0.005, 0.5, 12)
        for ss in np.linspace(0.01, 2, 12) for bb in (0, 14) for a1 in (0, 7.3) for a2 in (0, 3)
    ) < 1e-10  # grid: sympy does not always cancel the square roots
    f1 = a["f1"].subs({s1: s, s2: s, t2: 0})
    ell = sp.sqrt(2) * s * (1 - c) / (sp.sqrt(c + s) * (2 + s - c))
    ok3 = zero(sp.simplify(f1 - (b * sp.sqrt(c / (c + s)) + t1 * ell)))
    # Relative to the minimum (b = delta): the window-mean leak is -theta1 sqrt(c/2) / delta.
    rel = sp.simplify((want.subs({b: 0, t2: 0})) / sp.sqrt(c / (c + s)))
    ok4 = zero(rel + t1 * sp.sqrt(c / 2))
    # The general (unequal-noise) formula of Result 5 (c).
    vx = 2 * c + s1 + s2
    vw = (4 - 2 * c + s1 + s2) / 4
    cxw = (s2 - s1) / 2
    cyx = b * sp.sqrt(2 * c) + (t2 - t1) * c
    cyw = (t1 + t2) * (2 - c) / 2
    gen = (cyx * vw - cyw * cxw) / (vx * vw - cxw**2) * sp.sqrt(vx)
    ok5 = zero(sp.simplify(gen - a["wm"]))
    check("R5 window-mean level", ok1 and ok2 and ok3 and ok4 and ok5,
          f"cov(x,qbar) {ok1}, equal-noise coefficient {ok2}, F1-level (EXP-0009 R6) {ok3}, "
          f"relative leak {ok4}, unequal-noise formula {ok5}")


def r_spearman_brown() -> None:
    """Result 6: r = 2 rho/(1 + rho); k-fold window kr/(1 + (k - 1) r); SB is increasing; the
    harmonic noise ratio gives SB's k."""
    v, k, rho, r = sp.symbols("v k rho r", positive=True)
    ok1 = zero((v / (v + 1)).subs(v, 2 * rho / (1 - rho)) - 2 * rho / (1 + rho))
    ok2 = zero((k * v / (k * v + 1)).subs(v, r / (1 - r)) - k * r / (1 + (k - 1) * r))
    ok3 = zero(sp.diff(2 * rho / (1 + rho), rho) - 2 / (1 + rho) ** 2)
    # Noise variance sigma^2 E[1/m]: a design with E09/E29 = k has v29 = k v09.
    s2, e9, e29, V = sp.symbols("sigma2 E9 E29 V", positive=True)
    r9 = V / (V + s2 * e9)
    r29 = V / (V + s2 * e29)
    ok4 = zero(sp.simplify(r29 - (k * r9 / (1 + (k - 1) * r9)).subs(k, e9 / e29)))
    check("R6 Spearman-Brown and k", ok1 and ok2 and ok3 and ok4,
          f"halves {ok1}, k-fold {ok2}, increasing {ok3}, harmonic k {ok4}")


def r_below_and_stage2() -> None:
    """Results 7, 10 and 12: the "below" event, the closed-form powers, stage 2's n."""
    d, se, zg, zp, n0, se0, n = sp.symbols("d se z_g z_p n0 se0 n", positive=True)
    # The interval beta +- zg se inside (-d, d) iff |beta| < d - zg se: the probability at 0.
    Phi = lambda t: (1 + sp.erf(t / sp.sqrt(2))) / 2  # noqa: E731
    p_inside = Phi((d - zg * se) / se) - Phi(-(d - zg * se) / se)
    ok1 = zero(sp.simplify(p_inside - (2 * Phi(d / se - zg) - 1)))
    # pi_0 >= 0.80 iff Phi(d/se - zg) >= 0.90; n* from d sqrt(n)/(se0 sqrt(n0)) = zg + z_0.90.
    sol = sp.solve(sp.Eq(d * sp.sqrt(n) / (se0 * sp.sqrt(n0)), zg + zp), n)
    ok2 = len(sol) == 1 and zero(sol[0] - ((zg + zp) * se0 * sp.sqrt(n0) / d) ** 2)
    ok3 = abs(float(norm.ppf(0.9)) - 1.2815515655) < 1e-9 and abs(Z_G - 2.7343687865) < 1e-9
    check("R7/R12 below power and stage 2", ok1 and ok2 and ok3,
          f"inside-probability {ok1}, n* {ok2}, z_gamma {Z_G:.6f} and z_0.90 {norm.ppf(0.9):.6f}")


def r_luck_share() -> None:
    """Result 8: the per-game weight identity, the aggregate share with the five-game end rating,
    the mean-share closed form, and |s_A| non-increasing in g_A (grid)."""
    phi = sp.symbols("phi", positive=True)
    N, gA, gM, gJ, kk = sp.symbols("N g_A g_M g_J k", positive=True, integer=True)
    L = 5
    wk = sum(phi ** (N - j - kk) for j in range(L)) / L
    yk = sum(phi ** (N + gA + gM + gJ - j - kk) - phi ** (N + gA - j - kk) for j in range(L)) / L
    ok1 = zero(sp.powsimp(sp.expand(yk - phi**gA * (phi ** (gM + gJ) - 1) * wk), force=True))
    # Aggregates over k = 1..N on a grid of (phi, N, g): closed forms against direct sums.
    worst = 0.0
    for ph in (0.95, 0.97, 0.98):
        abar = sum(ph ** (-j) for j in range(L)) / L
        for Nn in (10, 30, 120):
            for a_, m_, j_ in ((5, 20, 20), (20, 40, 40), (121, 121, 121)):
                w = [sum(ph ** (Nn - j - k) for j in range(L) if Nn - j >= k) / L
                     for k in range(1, Nn + 1)]
                yv = [sum(ph ** (Nn + a_ + m_ + j_ - j - k) - ph ** (Nn + a_ - j - k)
                          for j in range(L)) / L for k in range(1, Nn + 1)]
                direct = sum(yv) / sum(w)
                closed = (ph**a_ * (ph ** (m_ + j_) - 1) * abar * (1 - ph**Nn)
                          / (1 - ph**Nn * abar))
                worst = max(worst, abs(direct - closed))
    # The mean of phi^t over t = a+1..a+g.
    a, g = sp.symbols("a g", positive=True, integer=True)
    tsym = sp.symbols("t", integer=True)
    ok3 = all(abs(sum(ph**t for t in range(a_ + 1, a_ + g_ + 1)) / g_
                  - ph ** (a_ + 1) * (1 - ph**g_) / (g_ * (1 - ph))) < 1e-12
              for ph in (0.9, 0.97, 0.98) for a_ in (0, 20, 120) for g_ in (1, 20, 240))
    del a, g, tsym  # the closed form is checked on a grid (sympy returns a Piecewise)
    mono = all(abs(ph**ga * (ph ** (2 * gj) - 1)) >= abs(ph ** (ga + 1) * (ph ** (2 * gj) - 1))
               for ph in np.linspace(0.5, 0.999, 60) for ga in range(0, 200, 7)
               for gj in (5, 20, 121))
    check("R8 luck shares", ok1 and worst < 1e-12 and ok3 and mono,
          f"weight identity {ok1}, aggregate closed form max err {worst:.1e}, mean share {ok3}, "
          f"|s_A| non-increasing in g_A on a grid {mono}")


def dilution(phi: float, G: float, r1: float, k: float = 3.0, kg: float = 3.0) -> float:
    rk = k * r1 / (1 + (k - 1) * r1)
    return (1 + phi**G + phi ** (2 * G)) / kg * math.sqrt(rk / r1)


def r_dilution() -> None:
    """Result 9: D = (1 + phi^G + phi^2G)/k_g * sqrt(r_k / r_1); with phi^G -> 0 and k = k_g = 3
    it is 1/sqrt(3(1 + 2 r_1)) <= 1/sqrt(3), with equality only as r_1 -> 0."""
    r1, f = sp.symbols("r1 f", positive=True)
    k = 3
    rk = k * r1 / (1 + (k - 1) * r1)
    D = (1 + f + f**2) / k * sp.sqrt(rk / r1)
    ok1 = zero(sp.simplify(D.subs(f, 0) - 1 / sp.sqrt(3 * (1 + 2 * r1))))
    ok2 = sp.limit(D.subs(f, 0), r1, 0) == 1 / sp.sqrt(3)
    ok3 = all(dilution(0.97, 1e6, r) <= 1 / math.sqrt(3) for r in np.linspace(1e-4, 1, 200))
    check("R9 dilution", ok1 and ok2 and ok3,
          f"forgotten-months form {ok1}, limit 1/sqrt(3) {ok2}, bound on a grid {ok3}")


# ---- simulation -------------------------------------------------------------------------------


def s_coverage() -> None:
    """Results 2 and 3: HC3 normal intervals cover at 95% and 99.375% under heteroskedasticity."""
    rng = np.random.default_rng(20261128)
    reps, n = 3000, 1500
    hit95 = hitg = 0
    for _ in range(reps):
        X = np.column_stack([np.ones(n), rng.normal(size=(n, 3))])
        y = 0.3 * X[:, 1] + X[:, 2] + rng.normal(size=n) * (0.5 + np.abs(X[:, 1]))
        XtXi = np.linalg.inv(X.T @ X)
        bhat = XtXi @ X.T @ y
        e = y - X @ bhat
        h = np.einsum("ij,jk,ik->i", X, XtXi, X)
        V = XtXi @ (X.T * (e**2 / (1 - h) ** 2)) @ X @ XtXi
        t = abs(bhat[1] - 0.3) / math.sqrt(V[1, 1])
        hit95 += t <= Z975
        hitg += t <= Z_G
    c95, cg = hit95 / reps, hitg / reps
    ok = (abs(c95 - 0.95) < 4 * math.sqrt(0.95 * 0.05 / reps)
          and abs(cg - GAMMA) < 4 * math.sqrt(GAMMA * (1 - GAMMA) / reps) + 1 / reps)
    check("S1 HC3 coverage", ok, f"95%: {c95:.4f}, 99.375%: {cg:.4f}, {reps} reps of n {n}")


def holm_reject(p: np.ndarray, alpha: float = ALPHA) -> np.ndarray:
    """Holm by adjusted p-values (written apart from power.py's cumulative product)."""
    m = p.shape[1]
    order = np.argsort(p, axis=1)
    ps = np.take_along_axis(p, order, axis=1)
    adj = np.maximum.accumulate(np.minimum(1, (m - np.arange(m)) * ps), axis=1)
    rej = np.zeros_like(p, dtype=bool)
    np.put_along_axis(rej, order, adj <= alpha, axis=1)
    return rej


def s_holm() -> None:
    """Result 4: Holm's FWER at 0.05 over eight correlated nulls; with negative means the chance
    of any "precedes climbing" stays below the union bound 8 * 0.025; the control's one-sided
    false-hold rate at beta = 0 is 0.025."""
    rng = np.random.default_rng(4)
    sims = 200_000
    C = np.full((M, M), 0.3) + 0.7 * np.eye(M)
    Z = rng.standard_normal((sims, M)) @ np.linalg.cholesky(C).T
    p = 2 * norm.sf(np.abs(Z))
    fwer = holm_reject(p).any(axis=1).mean()
    Zb = Z - np.array([0.2, 0.25, 0.45, 0.4, 0.3, 0.25, 0.05, 0.03]) * 1.0  # biased down
    climb = (holm_reject(2 * norm.sf(np.abs(Zb))) & (Zb > 0)).any(axis=1).mean()
    ctrl = (Z[:, 0] > Z975).mean()
    se = math.sqrt(0.05 * 0.95 / sims)
    ok = fwer <= 0.05 + 4 * se and climb <= 0.2 and abs(ctrl - 0.025) < 4 * math.sqrt(
        0.025 * 0.975 / sims)
    check("S2 Holm and the control", ok,
          f"FWER {fwer:.4f} (<= 0.05), climbing under negative means {climb:.4f}, "
          f"control false hold {ctrl:.4f} (0.025)")


def s_window_mean() -> None:
    """Result 5 on simulated players: the window-mean and F1-level coefficients per observed SD
    match the formulas, with equal and with unequal noise."""
    rng = np.random.default_rng(5)
    n = 400_000
    a = window_mean_algebra()
    worst = 0.0
    for c, s1, s2, b, t1, t2 in ((0.03, 0.15, 0.15, 14.0, 7.3, 0.0),
                                 (0.05, 0.2, 0.2, 0.0, 4.0, 2.0),
                                 (0.03, 0.10, 0.25, 6.0, 7.3, 0.0)):
        T1 = rng.standard_normal(n)
        T2 = (1 - c) * T1 + math.sqrt(1 - (1 - c) ** 2) * rng.standard_normal(n)
        q1 = T1 + math.sqrt(s1) * rng.standard_normal(n)
        q2 = T2 + math.sqrt(s2) * rng.standard_normal(n)
        y = b * (T2 - T1) / math.sqrt(2 * c) + t1 * T1 + t2 * T2 + 10 * rng.standard_normal(n)
        x = q2 - q1
        z = (x - x.mean()) / x.std()
        subs = {a["c"]: c, a["s1"]: s1, a["s2"]: s2, a["b"]: b, a["t1"]: t1, a["t2"]: t2}
        for w, key in (((q1 + q2) / 2, "wm"), (q1, "f1")):
            X = np.column_stack([np.ones(n), z, w])
            bh = np.linalg.lstsq(X, y, rcond=None)[0]
            e = y - X @ bh
            XtXi = np.linalg.inv(X.T @ X)
            se = math.sqrt((XtXi @ (X.T * e**2) @ X @ XtXi)[1, 1])
            want = float(a[key].subs(subs))
            worst = max(worst, abs(bh[1] - want) / se)
    check("S3 window-mean leak", worst < 4, f"largest |estimate - formula| {worst:.2f} SE")


def s_fisher() -> None:
    """Result 6: the Fisher-z lower bound covers r at about 97.5% (bivariate normal halves)."""
    rng = np.random.default_rng(6)
    reps, n = 4000, 2000
    hits = 0
    r_true = 0.16
    rho = r_true / (2 - r_true)
    for _ in range(reps):
        a = rng.standard_normal(n)
        bb = rho * a + math.sqrt(1 - rho**2) * rng.standard_normal(n)
        rh = np.corrcoef(a, bb)[0, 1]
        lo = math.tanh(math.atanh(rh) - Z975 / math.sqrt(n - 3))
        rl = max(0.01, 2 * lo / (1 + lo)) if lo > 0 else 0.01
        hits += rl <= r_true
    cov = hits / reps
    check("S4 Fisher-z lower bound", abs(cov - 0.975) < 4 * math.sqrt(0.975 * 0.025 / reps),
          f"P(r_L <= r) {cov:.4f} (0.975), n {n}")


def s_luck() -> None:
    """Result 8: an AR(1) rating with five-game end ratings; the regression of the outcome on the
    luck of F2's games gives the aggregate share, and on the early luck the exact share."""
    rng = np.random.default_rng(8)
    phi, Nn, gA, gM, gJ, L = 0.97, 40, 20, 25, 25, 5
    n = 60_000
    xi = rng.standard_normal((n, Nn))  # F2's lucks; later games' luck is independent noise
    later = rng.standard_normal((n, gA + gM + gJ))
    allx = np.concatenate([xi, later], axis=1)
    T = allx.shape[1]
    e = np.zeros((n, T))
    prev = np.zeros(n)
    for t in range(T):
        prev = phi * prev + allx[:, t]
        e[:, t] = prev
    end = lambda t_end: e[:, t_end - L:t_end].mean(axis=1)  # noqa: E731
    rF2, rA, rJ = end(Nn), end(Nn + gA), end(T)
    y = rJ - rA
    abar = sum(phi ** (-j) for j in range(L)) / L
    want_agg = phi**gA * (phi ** (gM + gJ) - 1) * abar * (1 - phi**Nn) / (1 - phi**Nn * abar)
    lam = xi.sum(axis=1)  # equally lucky games: the luck of the month
    w = np.array([sum(phi ** (Nn - j - k) for j in range(L) if Nn - j >= k) / L
                  for k in range(1, Nn + 1)])
    # Share of the F2 luck held in R_end(F2): Cov(y, lam)/Cov(rF2, lam).
    agg = np.cov(y, lam)[0, 1] / np.cov(rF2, lam)[0, 1]
    early = xi[:, 0]
    exact = np.cov(y, early)[0, 1] / np.cov(rF2, early)[0, 1]
    want_exact = phi**gA * (phi ** (gM + gJ) - 1)
    se_agg = np.std(y) / (np.cov(rF2, lam)[0, 1] / np.std(lam)) / math.sqrt(n)
    se_ex = np.std(y) / (w[0]) / math.sqrt(n)
    ok = abs(agg - want_agg) < 4 * se_agg and abs(exact - want_exact) < 4 * se_ex
    check("S5 luck share", ok, f"aggregate {agg:.4f} vs {want_agg:.4f}, early game "
          f"{exact:.4f} vs {want_exact:.4f}, phi {phi}")


def s_dilution() -> None:
    """Result 9: the covariance of the standardised three-month exposure with F2's end-rating
    luck, over the one-month exposure's, is (1 + phi^G + phi^2G)/3 * sqrt(Var x1 / Var x3).
    Players in chunks, so only three numbers a player are kept."""
    rng = np.random.default_rng(9)
    worst = 0.0
    for phi, G in ((0.97, 20), (0.98, 40), (0.97, 121)):
        V = 0.03  # true-change variance; per-game noise variance 1
        wts = (phi ** (3 * G - 1 - np.arange(3 * G))).astype(np.float32)  # end rating, L = 1
        x3s, x1s, lams = [], [], []
        for _ in range(10):
            n = 10_000
            u = rng.standard_normal((n, 3 * G), dtype=np.float32)  # F2's per-game noise
            xi = u + 0.2 * rng.standard_normal((n, 3 * G), dtype=np.float32)  # luck shares it
            Dv = math.sqrt(V) * rng.standard_normal(n)
            x3s.append(Dv + u.mean(axis=1) - rng.standard_normal(n) / math.sqrt(3 * G))
            x1s.append(Dv + u[:, 2 * G:].mean(axis=1) - rng.standard_normal(n) / math.sqrt(G))
            lams.append(xi @ wts)
        x3, x1, lam = (np.concatenate(v).astype(float) for v in (x3s, x1s, lams))
        ratio = (np.cov(x3, lam)[0, 1] / x3.std()) / (np.cov(x1, lam)[0, 1] / x1.std())
        r1, r3 = V / (V + 2 / G), V / (V + 2 / (3 * G))
        want = (1 + phi**G + phi ** (2 * G)) / 3 * math.sqrt(r3 / r1)
        worst = max(worst, abs(ratio / want - 1))
    check("S6 dilution", worst < 0.05, f"largest relative error {worst:.3f} (n 100,000)")


# ---- the planning model, re-implemented -------------------------------------------------------


def sb(r: float, k: float) -> float:
    return k * r / (1 + (k - 1) * r)


def r_lower_of(r0: float, n0: float) -> float:
    rho = r0 / (2 - r0)
    lo = math.tanh(math.atanh(rho) - Z975 / math.sqrt(n0 - 3))
    return max(0.01, sb(lo, 2.0))


def plan_inputs(w: float, density: float, k: float, sd_ratio: float, truth: str) -> dict:
    out = {}
    for q in QS:
        f = R09["primary"][q]
        n0, r0, se0 = float(f["n"]), float(f["reliability"]), float(f["se"])
        n = n0 / 0.3 * w * density
        base = r0 if truth == "point" else r_lower_of(r0, n0)
        out[q] = {"n": n, "se": se0 * math.sqrt(n0 / n) * sd_ratio, "r": sb(base, k)}
    return out


def shift() -> dict[str, float]:
    return {q: R09["primary"][q]["estimate"] - R09["adjusted"][q]["estimate"] for q in QS}


def rule(inp: dict, effects: dict, bias: dict, rng: np.random.Generator, sims: int) -> dict:
    """Section 9 under (M7), written from the registration's text."""
    m = len(QS)
    se = np.array([inp[q]["se"] for q in QS])
    r = np.array([inp[q]["r"] for q in QS])
    n = np.array([inp[q]["n"] for q in QS])
    mu = np.array([effects.get(q, 0) * math.sqrt(inp[q]["r"]) + bias.get(q, 0) for q in QS])
    C = np.full((m, m), 0.3) + 0.7 * np.eye(m)
    bh = mu + (rng.standard_normal((sims, m)) @ np.linalg.cholesky(C).T) * se
    zr = np.arctanh(r / (2 - r)) + rng.standard_normal((sims, m)) / np.sqrt(n - 3)
    lo = np.tanh(zr - Z975 / np.sqrt(n - 3))
    rl = np.maximum(0.01, 2 * lo / (1 + lo))
    d = DELTA * np.sqrt(rl)
    below = (bh - Z_G * se > -d) & (bh + Z_G * se < d)
    p = 2 * norm.sf(np.abs(bh / se))
    ki = [QS.index(q) for q in KEYS]
    climb = holm_reject(p[:, ki]) & (bh[:, ki] > 0)
    ctrl = (p[:, -1] < ALPHA) & (bh[:, -1] > 0)
    o1 = climb.any(axis=1)
    o2 = ~o1 & below[:, [QS.index(q) for q in K2]].all(axis=1) & ctrl
    return {"outcome1": o1.mean(), "outcome2": o2.mean(), "outcome3": 1 - o1.mean() - o2.mean(),
            "control_holds": ctrl.mean(), "climb": dict(zip(KEYS, climb.mean(axis=0))),
            "below": dict(zip(QS, below.mean(axis=0)))}


def mc_close(mine: float, theirs: float, n1: int, n2: int) -> bool:
    pbar = min(max((mine + theirs) / 2, 1 / n1), 1 - 1 / n1)
    return abs(mine - theirs) <= 4 * math.sqrt(pbar * (1 - pbar) * (1 / n1 + 1 / n2)) + 5e-5


def s_rule() -> None:
    """Result 10: an independent simulation of section 9 reproduces power.json's outcome
    probabilities in all 24 cells, the plan's and the sizing's target, within 4 combined Monte
    Carlo SE; and the control's power equals its closed form Phi(mu/se - 1.96)."""
    rng = np.random.default_rng(10)
    sim = PJ["simulation"]
    k = ACT["window_proxy"]["E_inv_m_exp0009_cap40"] / ACT["window_proxy"]["caps"]["144/120"][
        "E_inv_min_kept"]
    sdr = sd_ratio()["planned"]
    ratio = max(ACT["g0"][f"E_share_A_proxy_phi{p}"] / ACT["g0"][f"E_share_exp0009_phi{p}"]
                for p in ("0.97", "0.98"))
    bad, closed_bad, cells = [], [], 0
    for truth in ("point", "lower"):
        inp = plan_inputs(0.6, 0.7, k, sdr, truth)
        for mult in (1.0, 1.5, 2.0):
            bias = {q: s * ratio / math.sqrt(3) * mult for q, s in shift().items()}
            for ce in (5.0, 7.0, 10.0, "dev"):
                if ce == "dev":
                    eff = {q: R09["adjusted"][q]["estimate"]
                           / math.sqrt(R09["primary"][q]["reliability"]) for q in KEYS}
                    eff["accuracy"] = 7.0
                    key = f"truth={truth},bias_x{mult:g},dev_control7"
                else:
                    eff = {"accuracy": ce}
                    key = f"truth={truth},bias_x{mult:g},null_control{ce:g}"
                got = rule(inp, eff, bias, rng, 20_000)
                want = sim["outcomes_at_w"][key]
                cells += 1
                for o in ("outcome1", "outcome2", "outcome3", "control_holds"):
                    if not mc_close(got[o], want[o], 20_000, 20_000):
                        bad.append(f"{key}:{o} {got[o]:.4f}/{want[o]}")
                a = inp["accuracy"]
                mu = eff["accuracy"] * math.sqrt(a["r"]) + bias["accuracy"]
                cf = norm.cdf(mu / a["se"] - Z975)
                if not mc_close(cf, want["control_holds"], 10**9, 20_000):
                    closed_bad.append(key)
    check("S7a the rule, 24 cells", not bad, f"{cells} cells x 4 probabilities; off: {bad[:3]}")
    check("S7b control closed form", not closed_bad,
          f"Phi(mu/se - 1.96) within 4 MC SE of power.json in {cells - len(closed_bad)}/{cells}")
    # The gate by memory ratio: the control's holding probability at each ratio, closed form.
    inp = plan_inputs(0.6, 0.7, k, sdr, "point")
    a = inp["accuracy"]
    gbad = []
    for rr, v in sim["gate_by_memory_ratio"].items():
        mu = 7 * math.sqrt(a["r"]) + shift()["accuracy"] * float(rr) / math.sqrt(3)
        cf = norm.cdf(mu / a["se"] - Z975)
        if not mc_close(cf, v["control_holds"], 10**9, 20_000):
            gbad.append(rr)
        if v["ok"] != (cf >= 0.8 and True):
            gbad.append(f"ok@{rr}")
    check("S7c gate by memory ratio", not gbad,
          f"control holds {norm.cdf(7 * math.sqrt(a['r']) / a['se'] - Z975 + shift()['accuracy'] * 0.6 / math.sqrt(3) / a['se']):.4f} at 0.6; off: {gbad}")
    # The targets of the plan's and the sizing's chosen size.
    tbad = []
    for label, tried, inp, rat in (
            ("plan", sim["sizing"]["tried"][0], inp, ratio),
            ("sizing", PJ["sizing"]["decision"]["tried"][0],
             plan_inputs(0.6, PJ["sizing"]["measured"]["density"],
                         PJ["sizing"]["decision"]["k"], PJ["sizing"]["measured"]["sd_ratio"],
                         "point"),
             max(PJ["sizing"]["decision"]["memory_ratio"].values()))):
        bias = {q: s * rat / math.sqrt(3) for q, s in shift().items()}
        null = rule(inp, {}, bias, rng, 20_000)
        for q in K2:
            if not mc_close(null["below"][q], tried["below"][q], 20_000, 20_000):
                tbad.append(f"{label}:below:{q}")
        for q in KEYS:
            one = rule(inp, {q: DELTA}, bias, rng, 5_000)["climb"][q]
            if not mc_close(one, tried["single"][q], 5_000, 5_000):
                tbad.append(f"{label}:single:{q} {one:.4f}/{tried['single'][q]}")
            # Bonferroni-to-unadjusted bounds of Result 10 (b).
            mu = DELTA * math.sqrt(inp[q]["r"]) + bias[q]
            lo_b = norm.cdf(mu / inp[q]["se"] - norm.ppf(1 - ALPHA / 16))
            hi_b = norm.cdf(mu / inp[q]["se"] - Z975)
            if not (lo_b - 4 * math.sqrt(lo_b * (1 - lo_b) / 5000) - 1e-4 <= tried["single"][q]
                    <= hi_b + 4 * math.sqrt(hi_b * (1 - hi_b) / 5000) + 1e-4):
                tbad.append(f"{label}:bounds:{q}")
        a = inp["accuracy"]
        cf = norm.cdf((7 * math.sqrt(a["r"]) + bias["accuracy"]) / a["se"] - Z975)
        if not mc_close(cf, tried["control_holds"], 10**9, 20_000):
            tbad.append(f"{label}:control")
    check("S7d targets at the chosen size", not tbad, f"plan and sizing; off: {tbad}")


# ---- numbers ----------------------------------------------------------------------------------


def sd_ratio() -> dict:
    rat = {q: R09["level"][q]["se"] / R09["primary"][q]["se"] for q in QS}
    xs = sorted(rat.values())
    pos = 0.75 * (len(xs) - 1)  # 6.0: the upper quartile is the seventh of nine, exactly
    return {"per": rat, "planned": xs[int(pos)], "median": xs[4]}


def n_inputs() -> None:
    """EXP-0009's inputs as section 6 quotes them, and power.json's derived inputs."""
    rel = {q: round(R09["primary"][q]["reliability"], 3) for q in QS}
    want = {"W7.prep": 0.019, "W4.missed": 0.035, "W9b": 0.040, "W7.traps": 0.048,
            "W4.allowed": 0.051, "W6": 0.056, "accuracy": 0.067, "W5": 0.087, "W9a": 0.131}
    check("N1 reliabilities (section 6)", rel == want, f"{rel}")
    sh = shift()
    q4 = {"accuracy": -3.16, "W5": -2.48, "W6": -2.14, "W7.traps": -1.50, "W4.allowed": -1.30,
          "W7.prep": -1.22, "W4.missed": -0.90, "W9b": -0.46, "W9a": -0.23}
    ok = all(round(sh[q], 2) == v for q, v in q4.items()) and all(
        close(sh[q], PJ["luck_shift_0009"][q], 5e-5) for q in QS)
    check("N2 the shift (section 4, luck_shift_0009)", ok, f"accuracy {sh['accuracy']:.4f}")
    sr = sd_ratio()
    lo, hi = min(sr["per"].values()), max(sr["per"].values())
    ok = (close(sr["planned"], PJ["sd_ratio_from_exp0009"]["planned"], 5e-5)
          and close(sr["median"], PJ["sd_ratio_from_exp0009"]["median"], 5e-5)
          and round(sr["planned"], 2) == 1.34 and (round(lo, 2), round(hi, 2)) == (0.94, 1.71)
          and all(close(sr["per"][q], PJ["sd_ratio_from_exp0009"]["per_quantity"][q], 5e-5)
                  for q in QS))
    check("N3 error ratio 1.34, range 0.94 to 1.71", ok, f"planned {sr['planned']:.4f}")
    wp = ACT["window_proxy"]
    ks = {c: wp["E_inv_m_exp0009_cap40"] / wp["caps"][c]["E_inv_min_kept"]
          for c in ("144/120", "96/80", "72/60")}
    ok = ([round(ks[c], 2) for c in ks] == [2.64, 2.50, 2.31]
          and close(ks["144/120"], PJ["simulation"]["sizing"]["k"], 5e-5)
          and round(wp["E_inv_m_exp0009_cap40"], 4) == 0.0481
          and round(wp["caps"]["144/120"]["E_inv_min_kept"], 4) == 0.0182
          and [round(wp["caps"][c]["E_min_read"], 1) for c in ks] == [70.4, 62.7, 55.7]
          and wp["analysed"]["median"] == 58 and wp["analysed"]["p10"] == 33
          and round(wp["players_per_unit_hash"] / wp["exp0009_players_per_unit_hash"], 2) == 1.40)
    check("N4 k 2.64/2.50/2.31, games 70.4/62.7/55.7", ok, f"k {ks['144/120']:.4f}")
    g0 = ACT["g0"]
    oc = g0["october_rapid"]
    ok = (g0["players"] == 16206 and oc["median"] == 121 and oc["p10"] == 26
          and round(oc["share_below_20"] * 100, 1) == 6.8
          and round(g0["E_share_exp0009_phi0.97"], 2) == -0.87
          and round(g0["E_share_exp0009_phi0.98"], 2) == -0.81)
    pc = ACT["precheck"]
    po = pc["october_rapid"]
    ok2 = (pc["july_ge20"] == 86819 and pc["chain"] == 18968 and pc["oct_ge5"] == 17891
           and po["median"] == 118 and po["p25"] == 56 and po["p75"] == 213
           and round((1 - po["share_below_40"]) * 100, 1) == 82.8
           and round((1 - po["share_below_25"]) * 100, 1) == 90.2)
    check("N5 activity (sections 4 and 6)", ok and ok2, "16,206 players, median 121; precheck")


def n_memory() -> None:
    """Section 4's luck shares from Result 8 (power.json `memory`) and the memory ratios."""
    bad = []
    for row in PJ["memory"]:
        ph, g = row["phi"], row["rapid_games_per_month"]
        ms = lambda a, n: ph ** (a + 1) * (1 - ph**n) / (n * (1 - ph))  # noqa: E731
        want = {"exp0009": ph**g - 1, "gap_end": ph ** (3 * g) - ph**g,
                "gap2_end": ph ** (3 * g) - ph ** (2 * g), "left_after_gap": ph**g,
                "mean_after": ms(g, 2 * g) - ms(0, g), "mean_next": ms(0, g) - 1}
        bad += [f"{ph}/{g}/{k}" for k, v in want.items() if not close(v, row[k], 5e-5)]
    t = {(r["phi"], r["rapid_games_per_month"]): r for r in PJ["memory"]}
    quoted = [
        (t[0.97, 20]["exp0009"], -0.46), (t[0.97, 60]["exp0009"], -0.84),
        (t[0.97, 20]["gap_end"], -0.38), (t[0.97, 60]["gap_end"], -0.16),
        (t[0.98, 20]["gap_end"], -0.37), (t[0.98, 60]["gap_end"], -0.27),
        (t[0.98, 20]["exp0009"], -0.33), (t[0.97, 120]["gap_end"], -0.03),
        (t[0.98, 120]["gap_end"], -0.09), (t[0.97, 120]["exp0009"], -0.97),
        (t[0.98, 120]["exp0009"], -0.91)]
    bad += [f"{a}!={b}" for a, b in quoted if round(a, 2) != b]
    mean_after = [t[p, g]["mean_after"] for p in (0.97, 0.98) for g in (20, 40, 60)]
    if (round(min(mean_after), 2), round(max(mean_after), 2)) != (-0.46, -0.36):
        bad.append("mean_after range")
    if round(t[0.97, 20]["gap2_end"] / t[0.97, 20]["exp0009"], 1) != 0.3:
        bad.append("design B a third")
    g0 = ACT["g0"]
    ratios = {p: g0[f"E_share_A_proxy_phi{p}"] / g0[f"E_share_exp0009_phi{p}"]
              for p in ("0.97", "0.98")}
    meas = PJ["sizing"]["measured"]["april_share"]
    sz = {p: meas[p] / g0[f"E_share_exp0009_phi{p}"] for p in ("0.97", "0.98")}
    ok_r = (all(close(ratios[p], PJ["simulation"]["sizing"]["memory_ratio"][p], 5e-5)
                for p in ratios)
            and (round(ratios["0.97"], 2), round(ratios["0.98"], 2)) == (0.11, 0.17)
            and all(close(sz[p], PJ["sizing"]["decision"]["memory_ratio"][p], 2e-4) for p in sz)
            and (round(sz["0.97"], 3), round(sz["0.98"], 3)) == (0.105, 0.165))
    if not ok_r:
        bad.append("memory ratios")
    check("N6 luck shares and memory ratios", not bad,
          f"{len(PJ['memory'])} rows; ratios plan {ratios['0.98']:.4f}, sizing {sz['0.98']:.4f}; "
          f"off: {bad}")


def n_bias_and_costs() -> None:
    """The registered bias (section 4), the sizing's costs and sizes (section 6, summary.md)."""
    sh = shift()
    bad = []
    for blk, rat in ((PJ["simulation"]["sizing"], PJ["simulation"]["sizing"]["memory_ratio"]),
                     (PJ["sizing"]["decision"], PJ["sizing"]["decision"]["memory_ratio"])):
        r = max(rat.values())
        bad += [q for q in QS if not close(sh[q] * r / math.sqrt(3), blk["bias"][q], 1e-4)]
    if round(PJ["simulation"]["sizing"]["bias"]["accuracy"], 2) != -0.31:
        bad.append("accuracy -0.31")
    cpu_m = 1.5 / 0.877072
    wall_m = 0.8 / 0.877072

    def costs(players: float, read: float, stream: float) -> tuple[float, float, float]:
        games = players * 2 * read
        return games, stream + 2 * games / 1e6 * cpu_m, 2 * games / 1e6 * wall_m + 8

    plan_players = 17138 / 0.3 * 0.6 * 0.7
    g, c, w = costs(plan_players, ACT["window_proxy"]["caps"]["144/120"]["E_min_read"], 7.0)
    ca = PJ["simulation"]["costs_at_w"]
    if not (close(g, ca["observe_games"], 1) and close(c, ca["cpu_hours"], 1e-4)
            and close(w, ca["wall_hours"], 1e-4) and close(plan_players,
                                                           PJ["simulation"]["players_at_w"], 0.05)):
        bad.append("plan costs")
    with_outcome = 16204 / 0.3 * 0.6 * 0.7
    if not (round(plan_players, -3) == 24000 and round(with_outcome, -2) == 22700
            and round(g / 1e6, 2) == 3.38 and round(c, 1) == 18.5 and round(w) == 14
            and round(cpu_m, 1) == 1.7 and round(wall_m, 2) == 0.91):
        bad.append("plan rounding")
    # Caps: 3.38 million games within 5.55 million; 18.5 within 25; 14.2 within 24 hours.
    if not (g <= 5_550_000 and c <= 25 and w <= 24 and 3_700_000 * 1.5 == 5_550_000):
        bad.append("plan fits")
    mz = PJ["sizing"]["measured"]
    dec = PJ["sizing"]["decision"]
    dens = mz["counts"]["with_outcome"] / 0.6 / (16204 / 0.3)
    dobs = mz["counts"]["f1_and_f2"] / 0.6 / (17138 / 0.3)
    k = ACT["window_proxy"]["E_inv_m_exp0009_cap40"] / mz["caps"]["144/120"]["E_inv_min_kept"]
    g2, c2, w2 = costs(17138 / 0.3 * 0.6 * dobs, mz["caps"]["144/120"]["E_min_read"],
                       mz["streaming_cpu_hours"])
    t = dec["tried"][0]
    if not (close(dens, mz["density"], 5e-5) and close(dobs, mz["density_observed"], 5e-5)
            and close(k / dec["k"], 1, 4e-3) and close(g2 / t["observe_games"], 1, 2e-4)
            and close(c2, t["cpu_hours"], 2e-3) and close(w2, t["wall_hours"], 2e-3)
            and dec["w"] == 0.6 and dec["cap"] == [144, 120] and dec["run"] is True):
        bad.append("sizing costs")
    # summary.md: 19,120 players, about 3.6 million games, 15.3 CPU-hours, 14.6 hours, 0.998,
    # 2.9 CPU-hours of streaming.
    if not (mz["counts"]["with_outcome"] == 19120 and round(g2 / 1e6, 1) == 3.6
            and round(c2, 1) == 15.3 and round(w2, 1) == 14.6
            and abs(t["control_holds"] - 0.998) <= 0.0005 + 1e-9 and round(mz["streaming_cpu_hours"], 1) == 2.9
            and t["ok"] is True):
        bad.append("summary sizing")
    # Result 13 (c): at the plan, |B_q| / sqrt(r_q) <= 0.6 Elo per true SD for outcome 2's keys.
    inp = plan_inputs(0.6, 0.7, PJ["simulation"]["sizing"]["k"], sd_ratio()["planned"], "point")
    worst_b = max(abs(PJ["simulation"]["sizing"]["bias"][q]) / math.sqrt(inp[q]["r"]) for q in K2)
    if not worst_b <= 0.6:
        bad.append("bias per true SD")
    if not round(min(inp[q]["n"] for q in QS)) == 2932:  # W9b, Result 2's smallest n
        bad.append("W9b n")
    check("N7 bias, costs and sizes", not bad,
          f"plan {g:,.0f} games {c:.3f} CPU-h {w:.3f} h; sizing {g2:,.0f} games "
          f"{c2:.3f} CPU-h {w2:.3f} h, k {k:.4f}; |B|/sqrt(r) <= {worst_b:.3f}; off: {bad}")


def n_stage2() -> None:
    """Section 6's stage 2: about 5,130 players, 13,400 CPU-hours, 670 x 20, 2.9 x 4,560."""
    k = PJ["simulation"]["sizing"]["k"]
    inp = plan_inputs(1.0, 0.7, k, sd_ratio()["planned"], "point")
    z = Z_G + norm.ppf(0.9)
    need = sorted((z * inp[q]["se"] * math.sqrt(inp[q]["n"]) / (DELTA * math.sqrt(inp[q]["r"])))
                  ** 2 for q in KEYS)
    n2 = (need[3] + need[4]) / 2
    cpu = n2 * 240 * 39.1 / 3600
    s2 = PJ["stage2"]
    ok = (close(n2 / s2["players_for_median_key"], 1, 1e-4)
          and close(cpu / s2["cpu_hours"], 1, 1e-4)
          and round(n2, -1) == 5130 and round(cpu, -2) == 13400 and round(cpu / 20, -1) == 670
          and round(cpu / 4560, 1) == 2.9)
    check("N8 stage 2", ok, f"{n2:.1f} players, {cpu:.1f} CPU-hours")


def n_outcomes() -> None:
    """Every outcome probability that sections 0 and 6 and summary.md quote, from power.json."""
    o = PJ["simulation"]["outcomes_at_w"]
    P = lambda b, c, t="point": o[f"truth={t},bias_x{b},null_control{c}"]  # noqa: E731
    bad = []

    def want(v: float, nd: int, q: float, label: str) -> None:
        if abs(v - q) > 0.5 * 10 ** (-nd) + 1e-9:  # q is v rounded half either way
            bad.append(f"{label} {v}")

    for c in (5, 7, 10):
        want(P(1, c)["outcome1"], 3, 0.013, f"o1@{c}")
    for c, v in ((5, 0.85), (7, 0.98), (10, 0.99)):
        want(P(1, c)["outcome2"], 2, v, f"o2@{c}")
    want(P(1, 5)["outcome3"], 2, 0.14, "o3@5")
    want(P(1, 7)["outcome3"], 3, 0.006, "o3@7")
    if not P(1, 10)["outcome3"] < 0.001:
        bad.append("o3@10")
    avg = {k: sum(P(1, c)[k] for c in (5, 7, 10)) / 3 for k in ("outcome1", "outcome2", "outcome3")}
    want(avg["outcome1"], 2, 0.01, "avg o1")
    want(avg["outcome2"], 2, 0.94, "avg o2 (summary 94%)")
    want(avg["outcome3"], 2, 0.05, "avg o3 (summary 5%)")
    for b, vals in (("1.5", (0.22, 0.013, 0.001)), ("2", (0.30, 0.03, 0.001))):
        for c, v, nd in zip((5, 7, 10), vals, (2, 3 if b == "1.5" else 2, 3), strict=True):
            want(P(b, c)["outcome3"], nd, v, f"o3 x{b}@{c}")
    low3 = [P(b, c, "lower")["outcome3"] for b in ("1", "1.5", "2") for c in (5, 7, 10)]
    low3_57 = [P(b, c, "lower")["outcome3"] for b in ("1", "1.5", "2") for c in (5, 7)]
    if (round(min(low3_57), 2), round(max(low3_57), 2)) != (0.82, 0.93):
        bad.append("lower 0.82-0.93 at controls 5 and 7")
    dev = [o[f"truth={t},bias_x{b},dev_control7"]["outcome1"]
           for t in ("point",) for b in ("1", "1.5", "2")]
    devl = [o[f"truth=lower,bias_x{b},dev_control7"]["outcome1"] for b in ("1", "1.5", "2")]
    if not (all(round(v, 2) == 1.0 for v in dev)
            and (round(min(devl), 2), round(max(devl), 2)) == (0.85, 0.90)):
        bad.append("dev effects")
    for c, v in ((5, 0.86), (7, 0.99), (10, 1.00)):
        want(P(1, c)["control_holds"], 2, v, f"control@{c}")
    tr = PJ["simulation"]["sizing"]["tried"][0]
    if not (all(round(v, 2) == 1.0 for v in tr["below"].values())
            and all(round(tr["single"][q], 2) == 1.0 for q in K2)
            and round(tr["single"]["W4.missed"], 2) == 0.97
            and round(tr["single"]["W7.prep"], 3) == 0.998
            and round(tr["single"]["W9b"], 2) == 0.43 and round(tr["control_holds"], 2) == 0.99):
        bad.append("plan target")
    g = PJ["simulation"]["gate_by_memory_ratio"]
    if not (g["0.6"]["ok"] and not g["0.8"]["ok"] and round(g["0.6"]["control_holds"], 2) == 0.86
            and round(g["0.8"]["control_holds"], 2) == 0.68):
        bad.append("gate")
    check("N9 outcome probabilities (sections 0, 6, summary)", not bad, f"off: {bad}")
    check("N10 Finding 3: 'outcome 3 is 0.82 to 0.93' spans controls 5 and 7 only",
          round(min(low3), 2) == 0.81 and round(max(low3), 2) == 0.93,
          f"all nine lower-bound cells {min(low3):.4f} to {max(low3):.4f}")


def n_findings() -> None:
    """Finding 1's numbers (Result 5 under (M2) with EXP-0009's reliabilities) and Finding 2's
    dilution table (Result 9)."""
    out = []
    for q in ("accuracy", "W7.prep"):
        rL1 = R09["level"][q]["reliability"]
        rD1 = R09["primary"][q]["reliability"]
        theta = R09["level"][q]["estimate"] / math.sqrt(rL1)
        s1 = (1 - rL1) / rL1
        c = s1 * rD1 / (1 - rD1)
        s = s1 / PJ["simulation"]["sizing"]["k"]
        ell = math.sqrt(2) * s * (1 - c) / (math.sqrt(c + s) * (2 + s - c))
        dmin = DELTA * math.sqrt(c / (c + s))
        out.append((q, theta, ell, theta / 2 * ell, -theta * math.sqrt(c / 2) / DELTA,
                    theta * ell / dmin))
    acc, prep = out
    ok = (round(acc[1], 2) == 7.33 and round(acc[3], 2) == 0.83 and round(prep[3], 2) == 1.05
          and round(acc[4], 3) == -0.062 and round(acc[5], 2) == 0.30)
    check("N11 Finding 1 (level leak)", ok,
          f"accuracy theta {acc[1]:.3f}, F1 leak at theta/2 {acc[3]:.3f}, W7.prep {prep[3]:.3f}, "
          f"window-mean leak {acc[4] * 100:.1f}% of the minimum, F1 leak {acc[5] * 100:.1f}%")
    r1 = R09["primary"]["accuracy"]["reliability"]
    tab = {(p, G): dilution(p, G, r1) * math.sqrt(3) for p in (0.97, 0.98) for G in (40, 121)}
    ok = (round(tab[0.97, 121], 2) == 0.96 and round(tab[0.98, 121], 2) == 1.03
          and round(tab[0.97, 40], 2) == 1.30 and round(tab[0.98, 40], 2) == 1.54)
    check("N12 Finding 2 (dilution x sqrt 3)", ok,
          ", ".join(f"phi {p} G {G}: {v:.3f}" for (p, G), v in tab.items()))


def result_numbers() -> None:
    """STUB, for the decision: the numbers decision.md and summary.md will quote from
    results.json (each family's estimates, intervals, Holm p, r and r_L, d_q = 14 sqrt(r_L), the
    reads and the outcome), recomputed by Results 1 to 7 and 13. Until results.json is committed
    the check is skipped and passes."""
    path = HERE / "results.json"
    if not path.is_file():
        print("PASS-skipped N-results: no results.json yet (the run is going); "
              "added with the decision")
        return
    raise SystemExit("results.json exists: write result_numbers before the decision (ADR 0118)")


if __name__ == "__main__":
    for f in (r_fwl, r_hc3, r_window_mean, r_spearman_brown, r_below_and_stage2, r_luck_share,
              r_dilution, s_coverage, s_holm, s_window_mean, s_fisher, s_luck, s_dilution,
              s_rule, n_inputs, n_memory, n_bias_and_costs, n_stage2, n_outcomes, n_findings,
              result_numbers):
        f()
    print(f"{'FAIL' if FAILS else 'PASS'}: {len(FAILS)} failed {FAILS}")
    sys.exit(1 if FAILS else 0)
