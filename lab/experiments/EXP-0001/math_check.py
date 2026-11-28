"""EXP-0001: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy): the identities of math.md section 4, each as `simplify(lhs - rhs) == 0`, and
   the inequalities on a dense grid where sympy cannot settle them.
2. Simulation (numpy, fixed seeds): each result's model with a known truth, and the
   approximations of Results 8, 15 and 16 against the committed simulation grid.
3. Numbers: every number decision.md and summary.md quote, recomputed from results.json (and
   bench/baseline.json for the installed tolerances), within the rounding it is quoted at.

Standard library, numpy, scipy and sympy only. About 40 s of CPU.
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
BASELINE_PATH = HERE.parents[2] / "bench" / "baseline.json"

FLAWS = ["hang", "miss_mate", "opening_blunder", "low_clock_blunder", "no_convert"]
K = len(FLAWS)
Z_SHIP = float(stats.norm.ppf(0.5 + 0.975 / 2))  # 2.2414
Z_GUARD = float(stats.norm.ppf(0.5 + 0.80 / 2))  # 1.2816
Z95 = 1.959964
S_RECALL = 4000
S_COVERAGE = 2000
# Theorem names in lab/lean/Rematch/ (BetaBinomial.lean).
LEAN = {"Result 1 (Beta-binomial conjugacy, posterior mean)": "Rematch.beta_binomial_posterior"}

FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"{'PASS' if ok else 'FAIL'}  {name}{': ' + detail if detail else ''}")
    if not ok:
        FAILURES.append(name)


def zero(expr) -> bool:
    return sp.simplify(expr) == 0


def near(quoted: float, value: float, dp: int) -> bool:
    """`value` rounds to `quoted` at `dp` decimals (half-up or half-even)."""
    return abs(quoted - value) <= 0.5 * 10.0 ** (-dp) + 1e-9


# ---- shared helpers (written from math.md section 3, not imported from analysis.py) ----------


def counts(g: int) -> tuple[np.ndarray, np.ndarray]:
    o = next(x for x in R["observed"]["recall_at_2"] if x["games"] == g)
    h = np.array([o["per_flaw"][f][0] for f in FLAWS], dtype=float)
    n = np.array([o["per_flaw"][f][1] for f in FLAWS], dtype=float)
    return h, n


def wilson(x, n, z):
    x = np.asarray(x, dtype=float)
    n = np.asarray(n, dtype=float)
    p = x / n
    c = (p + z * z / (2 * n)) / (1 + z * z / n)
    w = z * np.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return c - w, c + w


def newcombe(g11, g10, g01, g00, z):
    """Section 3, (E4): the paired MOVER interval; g10 = candidate hit and reference miss."""
    g11, g10, g01, g00 = (np.asarray(v, dtype=float) for v in (g11, g10, g01, g00))
    n = g11 + g10 + g01 + g00
    p1, p2 = (g11 + g10) / n, (g11 + g01) / n
    l1, u1 = wilson(g11 + g10, n, z)
    l2, u2 = wilson(g11 + g01, n, z)
    den = np.sqrt((g11 + g10) * (g01 + g00) * (g11 + g01) * (g10 + g00))
    phi = np.divide(g11 * g00 - g10 * g01, den, out=np.zeros_like(den), where=den > 0)
    d = p1 - p2
    lo = d - np.sqrt(np.maximum((p1 - l1) ** 2 - 2 * phi * (p1 - l1) * (u2 - p2) + (u2 - p2) ** 2, 0))
    hi = d + np.sqrt(np.maximum((u1 - p1) ** 2 - 2 * phi * (u1 - p1) * (p2 - l2) + (p2 - l2) ** 2, 0))
    return d, lo, hi


def rates(pibar: np.ndarray, delta: float, b: float) -> tuple[np.ndarray, np.ndarray]:
    """(E2): the gain a and the loss l that make E[D] = delta exactly."""
    with np.errstate(divide="ignore", invalid="ignore"):
        if delta >= 0:
            return (delta + b * pibar) / (1 - pibar), np.full_like(pibar, b)
        return np.full_like(pibar, b), (-delta + b * (1 - pibar)) / pibar


def posterior(rng, g: int, size: int) -> np.ndarray:
    h, n = counts(g)
    return rng.beta(h + 0.5, n - h + 0.5, size=(size, K))


def flaw_vectors(g: int) -> dict[str, np.ndarray]:
    """Each flawed player's covered flags at checkpoints up to g (a players x r array)."""
    out = {}
    for f in FLAWS:
        rows = [
            [c["covered"] for c in p["checkpoints"] if c["games"] <= g and c["covered"] is not None]
            for p in R["pilot"]["players"]
            if p["flaw"] == f
        ]
        out[f] = np.array(rows, dtype=float)
    return out


# ---- 1. symbolic -----------------------------------------------------------------------------


def symbolic() -> None:
    p, h, n, al, be = sp.symbols("p h n alpha beta", positive=True)
    y = sp.symbols("y")
    # Result 1: Fisher information of a Bernoulli, the Jeffreys prior and the posterior.
    loglik = y * sp.log(p) + (1 - y) * sp.log(1 - p)
    d2 = sp.diff(loglik, p, 2)
    fisher = -(d2.subs(y, 1) * p + d2.subs(y, 0) * (1 - p))
    check("R1 Fisher information of Bernoulli is 1/(p(1-p))", zero(fisher - 1 / (p * (1 - p))))
    kernel = p**h * (1 - p) ** (n - h) * p ** sp.Rational(-1, 2) * (1 - p) ** sp.Rational(-1, 2)
    target = p ** (h + sp.Rational(1, 2) - 1) * (1 - p) ** (n - h + sp.Rational(1, 2) - 1)
    check("R1 likelihood x Jeffreys prior is the Beta(h+1/2, n-h+1/2) kernel", zero(sp.powsimp(kernel / target, force=True) - 1))
    mean = sp.gamma(al + 1) * sp.gamma(al + be) / (sp.gamma(al) * sp.gamma(al + be + 1))
    check("R1 Beta mean B(a+1,b)/B(a,b) = a/(a+b)", zero(sp.gammasimp(mean) - al / (al + be)))
    post_mean = (al / (al + be)).subs({al: h + sp.Rational(1, 2), be: n - h + sp.Rational(1, 2)})
    check("R1 posterior mean is (h+1/2)/(n+1)", zero(post_mean - (h + sp.Rational(1, 2)) / (n + 1)))

    # Result 2: the calibration is exact, both signs, averaged over K = 5 flaws.
    pis = sp.symbols("pi1:6", positive=True)
    pib = sum(pis) / K
    dl, b, a, l = sp.symbols("delta b a l", real=True)
    mean_change = sum((1 - q) * a - q * l for q in pis) / K
    check("R2 mean change over flaws is a(1-pibar) - l*pibar", zero(mean_change - (a * (1 - pib) - l * pib)))
    a_pos = (dl + b * pib) / (1 - pib)
    check("R2 a = (delta + b pibar)/(1 - pibar) gives E[D] = delta", zero(mean_change.subs({a: a_pos, l: b}) - dl))
    l_neg = (-dl + b * (1 - pib)) / pib
    check("R2 l = (|delta| + b(1-pibar))/pibar gives E[D] = delta (delta<0)", zero(mean_change.subs({a: b, l: l_neg}) - dl))

    # Result 3: feasibility, a <= 1 iff pibar <= (1-delta)/(1+b); on a dense grid.
    D, B, PB = np.meshgrid(np.linspace(0, 0.6, 61), np.linspace(0, 0.3, 31), np.linspace(0.001, 0.999, 999))
    feas_a = (D + B * PB) / (1 - PB) <= 1 + 1e-12
    feas_r = PB <= (1 - D) / (1 + B) + 1e-12
    check("R3 a <= 1 iff pibar <= (1-delta)/(1+b) (grid of 1.9M points)", bool(np.all(feas_a == feas_r)))
    feas_l = (D + B * (1 - PB)) / PB <= 1 + 1e-12
    feas_lr = PB >= (D + B) / (1 + B) - 1e-12
    check("R3 l <= 1 iff pibar >= (|delta|+b)/(1+b) (same grid)", bool(np.all(feas_l == feas_lr)))

    # Result 4: the per-player variance and the two variance terms.
    p10, p01, p11 = sp.symbols("p10 p01 p11", nonnegative=True)
    e = p10 - p01
    var_d = (p10 * 1 + p01 * 1) - e**2
    second = p10 * 1**2 + p01 * (-1) ** 2
    check("R4 Var of a player's change is p10 + p01 - (p10 - p01)^2", zero(second - e**2 - var_d))
    pb = sp.symbols("pibar", positive=True)
    su2 = ((1 - pb) * (dl + b * pb) / (1 - pb) + b * pb) - dl**2
    check("R4 sigma_u^2 = delta + 2 b pibar - delta^2 under (E2)", zero(su2 - (dl + 2 * b * pb - dl**2)))
    check("R4 d sigma_u^2 / d pibar = 2b", zero(sp.diff(dl + 2 * b * pb - dl**2, pb) - 2 * b))
    p10f = [(1 - q) * a_pos for q in pis]
    p01f = [q * b for q in pis]
    df = [x - w for x, w in zip(p10f, p01f, strict=True)]
    ss2 = sum(x + w - dd**2 for x, w, dd in zip(p10f, p01f, df, strict=True)) / K
    su2_f = sum(p10f) / K + sum(p01f) / K - (sum(df) / K) ** 2
    varf = sum((dd - sum(df) / K) ** 2 for dd in df) / K
    check("R4 sigma_u^2 - sigma_s^2 = Var_f(delta_f) >= 0", zero(su2_f - ss2 - varf))

    # Result 5: the Wilson bounds are the roots of (phat - p)^2 = z^2 p (1-p)/n.
    ph, z, N = sp.symbols("phat z N", positive=True)
    rad = sp.sqrt(ph * (1 - ph) / N + z**2 / (4 * N**2))
    for sgn, label in ((-1, "lower"), (1, "upper")):
        bound = (ph + z**2 / (2 * N) + sgn * z * rad) / (1 + z**2 / N)
        resid = sp.expand((ph - bound) ** 2 - z**2 * bound * (1 - bound) / N)
        check(f"R5 Wilson {label} bound solves the score equation", zero(resid))
    lower = (ph + z**2 / (2 * N) - z * rad) / (1 + z**2 / N)
    lim = sp.limit(sp.sqrt(N) * (ph - lower), N, sp.oo)
    check("R5 sqrt(N)(phat - L) -> z sqrt(phat(1-phat))", zero(lim - z * sp.sqrt(ph * (1 - ph))))

    # Result 6: the phi term of Newcombe's interval is the covariance; the variance identity.
    p00 = 1 - p11 - p10 - p01
    p1, p2 = p11 + p10, p11 + p01
    q1, q2 = 1 - p1, 1 - p2
    check("R6 n01 + n00 = N(1 - p1) and n10 + n00 = N(1 - p2)", zero((p01 + p00) - q1) and zero((p10 + p00) - q2))
    cov = p11 - p1 * p2
    check("R6 Cov(X_cand, X_ref) = p11 p00 - p10 p01", zero(cov - (p11 * p00 - p10 * p01)))
    lhs = p1 * q1 + p2 * q2 - 2 * (p11 * p00 - p10 * p01)
    check("R6 p1q1 + p2q2 - 2(p11p00 - p10p01) = p10 + p01 - (p10 - p01)^2", zero(lhs - (p10 + p01 - (p10 - p01) ** 2)))

    # Result 8: solving the normal approximation for N.
    za, zb, s_u, s_s, Nn = sp.symbols("z_a z_b sigma_u sigma_s N", positive=True)
    dp = sp.Symbol("d", positive=True)
    n_star = (za * s_u + zb * s_s) ** 2 / dp**2
    arg = (dp * sp.sqrt(n_star) - za * s_u) / s_s
    check("R8 N* = (z sigma_u + z_beta sigma_s)^2 / delta^2 solves power = Phi(z_beta)", zero(arg - zb))

    # Result 9: the Monte Carlo SE of a power is at most 1/(2 sqrt(S)).
    pp = sp.symbols("pp", positive=True)
    crit = sp.solve(sp.diff(pp * (1 - pp), pp), pp)
    check("R9 p(1-p) is concave with its only stationary point at p = 1/2", crit == [sp.Rational(1, 2)] and sp.diff(pp * (1 - pp), pp, 2) == -2)
    check("R9 MC SE <= 0.0079 at S = 4000", 1 / (2 * math.sqrt(S_RECALL)) < 0.0080, f"{1 / (2 * math.sqrt(S_RECALL)):.5f}")

    # Result 10: the tolerance (k - 1/2)/n, rounded to 3 dp, flags exactly a net loss >= k, n < 1000.
    ok = True
    for nn in range(1, 1000):
        ks = np.arange(1, nn + 1)
        tol = np.round((ks - 0.5) / nn, 3)
        # flagged j are those with j/n > tol + 1e-9; the smallest is k iff (k-1)/n <= tol+1e-9 < k/n
        lo_ok = (ks - 1) / nn <= tol + 1e-9
        hi_ok = tol + 1e-9 < ks / nn
        ok &= bool(np.all(lo_ok & hi_ok))
    check("R10 drop > round((k-1/2)/n, 3) iff net loss >= k, every n < 1000 and k <= n", ok)
    tol = round((1 - 0.5) / 1000, 3)
    check("R10 the bound N < 1000 is needed: at N = 1000, k = 1 the tolerance rounds to 0.001 and one loss is not flagged",
          tol == 0.001 and not (1 / 1000 > tol + 1e-9))

    # Result 13: the resampled stratified mean's variance (a pool of 6).
    ys = sp.symbols("y1:7", real=True)
    mu = sum(ys) / 6
    var_draw = sum((v - mu) ** 2 for v in ys) / 6
    second_moment = sum(v**2 for v in ys) / 6
    check("R13 variance of one uniform draw from the pool is the ddof-0 variance", zero(second_moment - mu**2 - var_draw))
    m = sp.symbols("m", positive=True)
    s2 = sp.symbols("s1:6", positive=True)
    var_est = sum(sv / m for sv in s2) / K**2
    check("R13 Var(C) = sum_f s_f^2 / (K^2 m)", zero(var_est - sum(s2) / (25 * m)))

    # Result 14: calibration on the pooled share is exact iff it equals the mean of player means.
    g, cb, yb = sp.symbols("g cbar ybar", real=True)
    gcal = (sp.Symbol("Dc") + b * cb) / (1 - cb)
    err = gcal * (1 - yb) - b * yb - sp.Symbol("Dc")
    check("R14 calibration error is (g+b)(cbar - ybar)", zero(err - (gcal + b) * (cb - yb)))

    # Result 16: the lattice of the coverage statistic at the smoke size.
    ct = sp.nsimplify(R["decision"]["coverage_tolerance_smoke"]) * 60
    check("R16 the committed coverage tolerance lies on the 1/60 lattice (0.8 = 48/60)", sp.Rational(4, 5) * 60 == 48 and ct.is_integer, f"tolerance x 60 = {ct}")

    # Result 18: E[MSB] = sigma_w^2 + n0 sigma_b^2 needs n0 = (N - sum r^2/N)/(k-1); equal r gives r.
    r_, k_ = sp.symbols("r k", positive=True)
    n0_eq = (k_ * r_ - k_ * r_**2 / (k_ * r_)) / (k_ - 1)
    check("R18 Searle's n0 equals r for equal group sizes", zero(n0_eq - r_))
    rs = sp.symbols("r1:4", positive=True)
    Rt = sum(rs)
    sq = sum(v**2 for v in rs)
    u_part = sum(v * (1 - 2 * v / Rt + sq / Rt**2) for v in rs)
    e_part = sum(v * (1 / v - 1 / Rt) for v in rs)
    check("R18 E[SSB] = (k-1) s_w^2 + (R - sum r^2/R) s_b^2 (k = 3, unequal r)", zero(u_part - (Rt - sq / Rt)) and zero(e_part - 2))


# ---- 2. simulation ---------------------------------------------------------------------------


def sim_suite(rng, pi: np.ndarray, m: int, delta: float, b: float, tol: float | None = None) -> dict:
    """Section 2 (M2) and section 3 (E3 to E6), from the formulas; pi is S x K."""
    S = pi.shape[0]
    pibar = pi.mean(1)
    a, l = rates(pibar, delta, b)
    feas = np.isfinite(a) & np.isfinite(l) & (a <= 1) & (l <= 1)
    a, l = np.clip(np.nan_to_num(a), 0, 1), np.clip(np.nan_to_num(l), 0, 1)
    x = rng.random((S, K, m)) < pi[:, :, None]
    u = rng.random((S, K, m))
    y = np.where(x, u >= l[:, None, None], u < a[:, None, None])
    x, y = x.reshape(S, -1), y.reshape(S, -1)
    g11, g10, g01, g00 = ((x & y).sum(1), (~x & y).sum(1), (x & ~y).sum(1), (~x & ~y).sum(1))
    _, lo, _ = newcombe(g11, g10, g01, g00, Z_SHIP)
    _, _, hi = newcombe(g11, g10, g01, g00, Z_GUARD)
    net_loss = g01 - g10
    out = {
        "D": (g10 - g01) / (K * m),
        "ship": (lo > 0) & feas,
        "guard": (hi < 0) & feas,
        "feasible": feas,
    }
    if tol is not None:
        out["ci"] = (net_loss / (K * m) > tol + 1e-9) & feas
    return out


def approx_power(pi: np.ndarray, m: int, delta: float, b: float, z: float = Z_SHIP) -> float:
    """Result 8: E_post[ 1{feasible} Phi((delta sqrt(N) - z sigma_u)/sigma_s) ]."""
    pibar = pi.mean(1)
    a, _ = rates(pibar, delta, b)
    feas = a <= 1
    p10f = (1 - pi) * a[:, None]
    p01f = pi * b
    df = p10f - p01f
    su = np.sqrt(np.maximum(delta + 2 * b * pibar - delta**2, 1e-12))
    ss = np.sqrt(np.maximum((p10f + p01f).mean(1) - (df**2).mean(1), 1e-12))
    N = K * m
    return float(np.where(feas, stats.norm.cdf((delta * math.sqrt(N) - z * su) / ss), 0).mean())


def ci_rule_exact(pi: np.ndarray, m: int, delta: float, b: float, k: int) -> float:
    """Result 12: P(net loss >= k), the exact law of a sum of independent {-1, 0, 1} changes,
    averaged over posterior draws; an infeasible draw counts 0."""
    pibar = pi.mean(1)
    a, l = rates(pibar, delta, b)
    vals = np.zeros(pi.shape[0])
    N = K * m
    for s in range(pi.shape[0]):
        if not (a[s] <= 1 and l[s] <= 1):
            continue
        dist = np.array([1.0])  # law of the net loss, offset N
        for f in range(K):
            pl, pg = pi[s, f] * l[s], (1 - pi[s, f]) * a[s]
            step = np.array([pg, 1 - pl - pg, pl])  # net loss -1, 0, +1
            per = np.array([1.0])
            for _ in range(m):
                per = np.convolve(per, step)
            dist = np.convolve(dist, per)
        vals[s] = dist[N + k :].sum()
    return vals


def mc_close(a_val: float, b_val: float, n_a: int, n_b: int | None = None) -> bool:
    """Two Monte Carlo proportions agree within 4 combined standard errors."""
    pbar = min(max((a_val + b_val) / 2, 1e-3), 1 - 1e-3)
    se = math.sqrt(pbar * (1 - pbar) / n_a + (pbar * (1 - pbar) / n_b if n_b else 0.0))
    return abs(a_val - b_val) <= 4 * se


def simulation() -> None:
    rng = np.random.default_rng(20261128)

    # Result 1: conjugacy, with a known prior. p ~ Beta(1/2,1/2), h ~ Bin(6, p).
    p = rng.beta(0.5, 0.5, 400_000)
    h = rng.binomial(6, p)
    worst = 0.0
    ok = True
    for hh in range(7):
        sel = p[h == hh]
        want = (hh + 0.5) / 7
        se = sel.std(ddof=1) / math.sqrt(sel.size)
        worst = max(worst, abs(sel.mean() - want) / se)
        ok &= abs(sel.mean() - want) < 4 * se
        a_, b_ = hh + 0.5, 6 - hh + 0.5
        vwant = a_ * b_ / ((a_ + b_) ** 2 * (a_ + b_ + 1))
        ok &= abs(sel.var(ddof=1) / vwant - 1) < 4 * math.sqrt(2 / (sel.size - 1)) + 0.02
    check("R1 E[p | h] = (h+1/2)/7 for h = 0..6 (simulated prior)", ok, f"worst {worst:.2f} SE")

    # Results 2 and 4 at a known pi (the posterior means at 80 games), m = 22.
    hv, nv = counts(80)
    pi0 = (hv + 0.5) / (nv + 1)
    for delta in (0.1, -0.1, 0.0):
        S = 20_000
        out = sim_suite(rng, np.tile(pi0, (S, 1)), 22, delta, 0.1)
        D = out["D"]
        pibar = pi0.mean()
        a, l = rates(np.array([pibar]), delta, 0.1)
        p10f, p01f = (1 - pi0) * a[0], pi0 * l[0]
        ss2 = float(np.mean(p10f + p01f - (p10f - p01f) ** 2))
        se = D.std(ddof=1) / math.sqrt(S)
        ok_mean = abs(D.mean() - delta) < 4 * se
        ok_var = abs(D.var(ddof=1) / (ss2 / 110) - 1) < 4 * math.sqrt(2 / (S - 1))
        check(f"R2/R4 E[D] = {delta:+.1f} and Var[D] = sigma_s^2/N at known pi", ok_mean and ok_var,
              f"mean {D.mean():+.4f}, var ratio {D.var(ddof=1) / (ss2 / 110):.4f}")

    # Result 3: the infeasible share, pooled over the 20 grid cells of each (G, b), vs the posterior.
    for g in (80, 120):
        pis = posterior(rng, g, 400_000).mean(1)
        for b in (0.0, 0.05, 0.1):
            want = float(np.mean(pis > (1 - 0.1) / (1 + b)))
            cells = [x["infeasible"] for x in R["power_recall"] if x["games"] == g and x["churn"] == b]
            got = float(np.mean(cells))
            n_js = S_RECALL * len(cells)
            se = math.sqrt(max(want * (1 - want), 1e-6) / n_js + max(want * (1 - want), 1e-6) / 400_000)
            check(f"R3 infeasible share at {g} games, churn {b}", abs(got - want) <= 4 * se, f"JSON {got:.5f}, P(pibar > (1-d)/(1+b)) {want:.5f}")

    # Result 3, corollary: ship(delta) <= P(feasible), and the curve's fall at large delta.
    bound_ok = True
    worst_txt = ""
    for row in R["at_budget"]:
        pis = posterior(rng, row["games"], 400_000).mean(1)
        for c in row["curve"]:
            d = c["delta"]
            pf_up = float(np.mean(pis <= (1 - d) / 1.1))
            pf_dn = float(np.mean(pis >= (d + 0.1) / 1.1))
            se_up = math.sqrt(max(pf_up * (1 - pf_up), 1e-4) / S_RECALL)
            se_dn = math.sqrt(max(pf_dn * (1 - pf_dn), 1e-4) / S_RECALL)
            bound_ok &= c["ship"] <= pf_up + 4 * se_up
            bound_ok &= c["ci_rule_flags_drop"] <= pf_dn + 4 * se_dn
            bound_ok &= c["guard_flags_drop"] <= pf_dn + 4 * se_dn
            if row["suite"] == "full" and row["games"] == 80 and d == 0.5:
                worst_txt = f"full 80, +0.5: ship {c['ship']} <= {pf_up:.4f}; drop 0.5 flagged {c['ci_rule_flags_drop']} vs P(feasible) {pf_dn:.4f}"
                bound_ok &= abs(c["ci_rule_flags_drop"] - pf_dn) <= 4 * se_dn
    check("R3 every ship and flag rate is at most its feasibility probability", bound_ok, worst_txt)

    # Results 6 and 7: the one-sided error of the Newcombe rules at delta = 0, known pi.
    g0 = 0.1 * pi0.mean() / (1 - pi0.mean())
    d0 = (1 - pi0) * g0 - pi0 * 0.1
    ratio0 = math.sqrt(2 * 0.1 * pi0.mean() / float(np.mean((1 - pi0) * g0 + pi0 * 0.1 - d0**2)))
    want_g = float(stats.norm.cdf(-Z_GUARD * ratio0))
    want_s = float(stats.norm.cdf(-Z_SHIP * ratio0))
    m, chunks, S = 100, 10, 20_000
    gsum = ssum = 0
    for _ in range(chunks):
        out = sim_suite(rng, np.tile(pi0, (S, 1)), m, 0.0, 0.1)
        gsum += int(out["guard"].sum())
        ssum += int(out["ship"].sum())
    T = chunks * S
    gfa, sfa = gsum / T, ssum / T
    se_g, se_s = math.sqrt(gfa * (1 - gfa) / T), math.sqrt(sfa * (1 - sfa) / T)
    check(f"R7 at delta 0, N = 500: guard within 4 SE + 0.002 of Phi(-z r) = {want_g:.4f}, and below 0.10 by more than 4 SE",
          abs(gfa - want_g) <= 4 * se_g + 0.002 and gfa < 0.10 - 4 * se_g, f"guard {gfa:.4f} +- {se_g:.4f}, r = s_u/s_s = {ratio0:.4f}")
    check("R7 at delta 0, N = 500: ship below the nominal 0.0125", sfa < 0.0125 - 2 * se_s, f"ship {sfa:.4f} +- {se_s:.4f} (limit {want_s:.4f})")
    # coverage of the two-sided 80% interval at a true change of +0.1, known pi
    S = 40_000
    out_cov = None
    rng2 = np.random.default_rng(7)
    pibar = pi0.mean()
    a, _ = rates(np.array([pibar]), 0.1, 0.1)
    x = rng2.random((S, K, 22)) < pi0[None, :, None]
    u = rng2.random((S, K, 22))
    y = np.where(x, u >= 0.1, u < a[0])
    x, y = x.reshape(S, -1), y.reshape(S, -1)
    _, lo, hi = newcombe((x & y).sum(1), (~x & y).sum(1), (x & ~y).sum(1), (~x & ~y).sum(1), Z_GUARD)
    out_cov = float(np.mean((lo <= 0.1) & (0.1 <= hi)))
    p10f, p01f = (1 - pi0) * a[0], pi0 * 0.1
    ss = math.sqrt(float(np.mean(p10f + p01f - (p10f - p01f) ** 2)))
    su = math.sqrt(0.1 + 2 * 0.1 * pibar - 0.01)
    want = 2 * stats.norm.cdf(Z_GUARD * su / ss) - 1
    check("R6 the 80% Newcombe interval covers a true +0.1 at 2 Phi(z s_u/s_s) - 1 (N = 110)", abs(out_cov - want) <= 0.015, f"{out_cov:.4f} vs {want:.4f} (s_u/s_s {su / ss:.4f})")

    # Result 8: the normal approximation against the committed grid, every cell with m >= 10.
    post = {g: posterior(rng, g, 20_000) for g in (30, 45, 60, 80, 120)}
    errs = {0.0: [], 0.05: [], 0.1: []}
    for row in R["power_recall"]:
        if row["players_per_flaw"] < 10:
            continue
        ap = approx_power(post[row["games"]], row["players_per_flaw"], 0.1, row["churn"])
        errs[row["churn"]].append(ap - row["ship"])
    for b, bound in ((0.05, 0.05), (0.1, 0.035)):
        worst = max(abs(e) for e in errs[b])
        check(f"R8 normal approximation within {bound} of the simulated assurance, churn {b}, m >= 10", worst <= bound, f"worst {worst:.4f}")
    e0 = np.array(errs[0.0])
    check("R8 at churn 0 the approximation is conservative (never above the simulation by 0.02)", bool(e0.max() <= 0.02), f"error from {e0.min():+.4f} to {e0.max():+.4f}")
    ap40 = approx_power(post[80], 40, 0.1, 0.1)
    ap22 = approx_power(post[80], 22, 0.1, 0.1)
    check("R8 at 80 games, churn 0.1: m = 40 and m = 22", abs(ap40 - 0.82575) < 0.02 and abs(ap22 - 0.52475) < 0.02, f"{ap40:.4f}, {ap22:.4f}")
    pm = float(((hv + 0.5) / (nv + 1)).mean())
    su2 = 0.1 + 2 * 0.1 * pm - 0.01
    n_star = (Z_SHIP + stats.norm.ppf(0.8)) ** 2 * su2 / 0.01
    check("R8 plug-in N* at the posterior mean (80 games, b 0.1) is about 202, m about 40.5", abs(n_star - 202.3) < 0.5, f"N* {n_star:.1f}, m {n_star / 5:.2f}")

    # Result 7 on the committed runs, and an independent re-simulation of quoted cells.
    for row in R["at_budget"]:
        fa = row["guard_false_alarm_delta_0"]
        se = math.sqrt(0.1 * 0.9 / S_RECALL)
        pis = post[row["games"]]
        pb = pis.mean(1)
        g0 = 0.1 * pb / (1 - pb)
        dfl = (1 - pis) * g0[:, None] - pis * 0.1
        r = np.sqrt(2 * 0.1 * pb / ((1 - pis) * g0[:, None] + pis * 0.1 - dfl**2).mean(1))
        lim = float(stats.norm.cdf(-Z_GUARD * r).mean())
        ok = fa <= 0.10 + 4 * se
        if row["suite"] == "full":
            ok &= abs(fa - lim) <= 4 * se
        check(f"R7 committed guard false alarm at {row['suite']} {row['games']} <= 0.10 (+4 SE), near the limit for full suites", bool(ok), f"{fa} vs limit {lim:.4f}")
    rng3 = np.random.default_rng(91)
    cells = [
        ("smoke ship +0.1", 80, 3, 0.1, None, "ship", R["at_budget"][0]["ship_power_delta_0_1"]),
        ("full-80 ship +0.1", 80, 22, 0.1, None, "ship", R["at_budget"][1]["ship_power_delta_0_1"]),
        ("full-120 ship +0.1", 120, 14, 0.1, None, "ship", R["at_budget"][2]["ship_power_delta_0_1"]),
        ("grid 80/0.1/40 ship", 80, 40, 0.1, None, "ship", 0.82575),
        ("smoke guard at 0", 80, 3, 0.0, None, "guard", R["at_budget"][0]["guard_false_alarm_delta_0"]),
    ]
    for name, g, m, d, tol, key, want in cells:
        S = 20_000
        got = float(sim_suite(rng3, posterior(rng3, g, S), m, d, 0.1, tol)[key].mean())
        check(f"re-simulated {name}", mc_close(got, want, S, S_RECALL), f"{got:.4f} vs JSON {want}")

    # Result 12: the CI rule exactly (law of the net loss), against the committed simulation.
    rng4 = np.random.default_rng(12)
    for row, d, key, want in (
        (R["at_budget"][0], 0.0, "fa", R["at_budget"][0]["ci_rule_false_alarm_delta_0"]),
        (R["at_budget"][0], -0.1, "drop 0.1", R["at_budget"][0]["curve"][1]["ci_rule_flags_drop"]),
        (R["at_budget"][0], -0.2, "drop 0.2", R["at_budget"][0]["curve"][3]["ci_rule_flags_drop"]),
        (R["at_budget"][1], 0.0, "fa", R["at_budget"][1]["ci_rule_false_alarm_delta_0"]),
        (R["at_budget"][2], 0.0, "fa", R["at_budget"][2]["ci_rule_false_alarm_delta_0"]),
    ):
        draws = 3000 if row["players_per_flaw"] <= 3 else 800
        vals = ci_rule_exact(posterior(rng4, row["games"], draws), row["players_per_flaw"], d, 0.1, row["guard_k_lost"])
        val = float(vals.mean())
        se = math.sqrt(vals.var(ddof=1) / draws + want * (1 - want) / S_RECALL)
        check(f"R12 CI rule {key} at {row['suite']} {row['games']}, exact", abs(val - want) <= 4 * se, f"{val:.4f} vs JSON {want} (4 SE {4 * se:.4f})")

    # Result 13: the coverage half-width, exact, against the committed resampling.
    for g in (80, 120):
        v = flaw_vectors(g)
        s2 = sum(float(np.var(v[f].mean(1))) for f in FLAWS)
        worst = 0.0
        for row in R["coverage_power"]:
            if row["games_up_to"] != g:
                continue
            want = Z95 * math.sqrt(s2 / (K * K * row["players_per_flaw"]))
            worst = max(worst, abs(row["half_width_95"] / want - 1))
        check(f"R13 half_width_95 = 1.96 sqrt(sum s_f^2/(25 m)), checkpoints to {g}", worst < 4 * math.sqrt(1 / (2 * (S_COVERAGE - 1))), f"worst relative error {worst:.4f}")

    # Result 14: every flawed player has a flag at every checkpoint, so the calibration is exact.
    sizes = {int(v.shape[1]) for g in (80, 120) for v in flaw_vectors(g).values()}
    full_rows = all(c["covered"] is not None for p in R["pilot"]["players"] if p["flaw"] for c in p["checkpoints"])
    check("R14 equal interval counts per player (4 to 80 games, 5 to 120)", full_rows and sizes == {4, 5}, f"sizes {sorted(sizes)}")

    # Result 15: the paired coverage power, normal approximation vs the committed grid.
    worst = 0.0
    for g in (80, 120):
        v = flaw_vectors(g)
        cf = {f: v[f].mean(1) for f in FLAWS}
        call = np.concatenate([cf[f] for f in FLAWS])
        r_ = v[FLAWS[0]].shape[1]
        b, Dc = 0.1, 0.05
        gain = (Dc + b * call.mean()) / (1 - call.mean())
        for row in R["coverage_power"]:
            if row["games_up_to"] != g or row["players_per_flaw"] < 8:
                continue
            N = K * row["players_per_flaw"]
            t = stats.t.ppf(0.9875, N - 1)
            for kind, key in (("i", "power_interval_flips"), ("p", "power_player_flips")):
                def parts(c):
                    mu = gain - (gain + b) * c
                    w = ((1 - c) * gain * (1 - gain) + c * b * (1 - b)) / r_ if kind == "i" else (1 - c) ** 2 * gain * (1 - gain) + c**2 * b * (1 - b)
                    return mu, w
                mu, w = parts(call)
                s_u = math.sqrt(w.mean() + mu.var())
                s_s = math.sqrt(np.mean([parts(cf[f])[1].mean() + parts(cf[f])[0].var() for f in FLAWS]))
                ap = float(stats.norm.cdf((Dc * math.sqrt(N) - t * s_u) / s_s))
                worst = max(worst, abs(ap - row[key]))
    check("R15 coverage power approximation within 0.05 of the grid, m >= 8", worst <= 0.05, f"worst {worst:.4f}")

    # Result 16: the coverage tolerance's null law at the smoke size, 1,000,000 suites.
    v = flaw_vectors(80)
    rng5 = np.random.default_rng(16)
    m = R["budgets"]["smoke_players_per_flaw"]
    tally = np.zeros(121, dtype=np.int64)  # d * 60 + 60
    total = 0
    b = 0.1
    for _ in range(10):
        S = 100_000
        idx = rng5.integers(0, 6, size=(S, K, m))
        pl = np.stack([v[f][idx[:, i, :]] for i, f in enumerate(FLAWS)], 1).reshape(S, K * m, 4)
        cbar = pl.mean((1, 2))
        gain = b * cbar / (1 - cbar)
        ug = rng5.random((S, K * m, 1))
        ul = rng5.random((S, K * m, 1))
        fl = np.where(pl == 1, ul >= b, ug < gain[:, None, None])
        d60 = np.rint((np.abs(fl.mean((1, 2)) - 0.8) - np.abs(cbar - 0.8)) * 60).astype(int)
        tally += np.bincount(d60 + 60, minlength=121)
        total += S
    cdf = np.cumsum(tally) / total
    f3, f4 = cdf[63], cdf[64]
    se = math.sqrt(f3 * (1 - f3) / total)
    check("R16 P(growth <= 3/60) is below 0.8 (Finding 1)", f3 < 0.8 - 3 * se, f"F(3/60) = {f3:.4f} +- {se:.4f}, F(4/60) = {f4:.4f}")
    check("R16 churn alone exceeds the installed 0.05 about 20% of the time", abs((1 - f3) - 0.20) < 0.005, f"P(growth > 0.05) = {1 - f3:.4f}")
    # The chance a 2,000-suite run's interpolated 80th percentile rounds to 0.050: x(1600) and x(1601) both 3/60.
    p_050 = float(stats.binom.sf(1600, 2000, f3))
    check("R16 a 2,000-suite run returns 0.050 with probability about 0.4", 0.3 < p_050 < 0.5, f"{p_050:.3f}")

    # Result 16, Finding 2: at the installed baseline (coverage 0.8) the rule is |c' - 0.8| > 0.05.
    base_cov = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))["metrics"]["coverage"]
    rng7 = np.random.default_rng(162)
    S = 200_000
    g08 = b * base_cov / (1 - base_cov)
    fas = {}
    for mix, vec in (("3 x 1.0 + 12 x 0.75", [4] * 3 + [3] * 12), ("12 x 1.0 + 3 x 0.0", [4] * 12 + [0] * 3)):
        hits = np.array(vec)[None, :]
        for kind in ("interval", "player"):
            if kind == "interval":
                kept = rng7.binomial(hits, 1 - b, size=(S, 15))
                gained = rng7.binomial(4 - hits, g08, size=(S, 15))
            else:
                kept = hits * (rng7.random((S, 15)) >= b)
                gained = (4 - hits) * (rng7.random((S, 15)) < g08)
            c2 = (kept + gained).sum(1) / 60
            fas[(mix, kind)] = float(np.mean(np.abs(c2 - 0.8) > np.abs(base_cov - 0.8) + 0.05 + 1e-9))
    ok = base_cov == 0.8 and all(v > 0.3 for k, v in fas.items() if k[1] == "player") and all(0.15 < v < 0.25 for k, v in fas.items() if k[1] == "interval")
    check("R16 at the installed baseline 0.8, the registered player-flip model fails coverage > 30% of null runs (Finding 2)", ok,
          ", ".join(f"{k[0]} {k[1]}: {v:.3f}" for k, v in fas.items()))

    # Result 17: the cluster bootstrap of coverage against its normal approximation.
    for g, row in ((80, R["observed"]["coverage"][3]), (120, R["observed"]["coverage"][4])):
        vv = flaw_vectors(g)
        s2 = sum(float(np.var(vv[f].mean(1))) for f in FLAWS)
        se = math.sqrt(s2 / (K * K * 6))
        half = (row["ci95_cluster_bootstrap"][1] - row["ci95_cluster_bootstrap"][0]) / 2
        check(f"R17 bootstrap half-width at {g} is 1.96 x the stratified SE within 0.006", abs(half - Z95 * se) <= 0.006, f"{half:.4f} vs {Z95 * se:.4f}")

    # Result 18: the ANOVA expectations, one-way random effects with known variances.
    rng6 = np.random.default_rng(18)
    k_, r_, sb2, sw2 = 30, 4, 0.03, 0.07
    S = 20_000
    y = rng6.normal(0, math.sqrt(sb2), (S, k_, 1)) + rng6.normal(0, math.sqrt(sw2), (S, k_, r_))
    gm = y.mean((1, 2), keepdims=True)
    msb = (r_ * ((y.mean(2, keepdims=True) - gm) ** 2).sum((1, 2))) / (k_ - 1)
    msw = ((y - y.mean(2, keepdims=True)) ** 2).sum((1, 2)) / (k_ * r_ - k_)
    ok = abs(msb.mean() / (sw2 + r_ * sb2) - 1) < 4 * msb.std() / math.sqrt(S) / (sw2 + r_ * sb2)
    ok &= abs(msw.mean() / sw2 - 1) < 4 * msw.std() / math.sqrt(S) / sw2
    check("R18 E[MSB] = sigma_w^2 + n0 sigma_b^2 and E[MSW] = sigma_w^2", bool(ok), f"{msb.mean():.4f} vs {sw2 + r_ * sb2:.4f}")


# ---- 3. numbers ------------------------------------------------------------------------------


def numbers() -> None:
    pr = R["power_recall"]
    cell = lambda g, b, m: next(x for x in pr if x["games"] == g and x["churn"] == b and x["players_per_flaw"] == m)  # noqa: E731
    ab = {(x["suite"], x["games"]): x for x in R["at_budget"]}

    # Pilot.
    players = R["pilot"]["players"]
    flawed = [p for p in players if p["flaw"]]
    clean = [p for p in players if not p["flaw"]]
    check("N pilot: 30 flawed, 2 clean, 120 games each, 3,840 games", len(flawed) == 30 and len(clean) == 2 and all(p["games"] == 120 for p in players) and R["pilot"]["games_total"] == 3840)
    check("N 1,489 s / 3,840 games = 0.39 s a game (decision.md prose)", near(0.39, 1489 / 3840, 2))
    check("N summary run_minutes 25 = 1,489 s; run_cpu_hours 2.44 = 1,489 s at 590%", near(25, 1489 / 60, 0) and near(2.44, 1489 * 5.90 / 3600, 2))
    check("N pilot at 0.544 s a game is about 35 minutes", near(35, 3840 * 0.544 / 60, 0))

    # Section 9 budgets (R19).
    smoke_m = (1650 // 80 - 1) // K
    full = {g: (9000 // g - 2) // K for g in (80, 120)}
    check("N budgets: smoke m 3, full m 22 at 80 and 14 at 120", smoke_m == 3 == R["budgets"]["smoke_players_per_flaw"] and full == {80: 22, 120: 14} and R["budgets"]["full_players_per_flaw"] == {"80": 22, "120": 14})
    check("N totals: 1,280, 8,960, 8,640 games", (K * 3 + 1) * 80 == 1280 == ab[("smoke", 80)]["games_total"] and (K * 22 + 2) * 80 == 8960 == ab[("full", 80)]["games_total"] and (K * 14 + 2) * 120 == 8640 == ab[("full", 120)]["games_total"])
    check("N 1,650 games is about 15 minutes, 9,000 about 82, 16,160 about 147 (0.544 s)", near(15, 1650 * 0.544 / 60, 0) and near(82, 9000 * 0.544 / 60, 0) and near(147, 16160 * 0.544 / 60, 0))
    check("N 40 a flaw is 200 flawed players, 16,160 games with two clean controls", 40 * K == 200 and (200 + 2) * 80 == 16160)

    # The required table (R20: the "stays at least 0.80 for every larger m" rule).
    table = {0.0: [14, 14, 14, 14, 16], 0.05: [24, 28, 28, 28, 32], 0.1: [28, 40, 40, 40, 48]}
    ok = True
    for b, want in table.items():
        for g, w in zip((30, 45, 60, 80, 120), want, strict=True):
            curve = sorted((x for x in pr if x["games"] == g and x["churn"] == b), key=lambda x: x["players_per_flaw"])
            req = next((c["players_per_flaw"] for i, c in enumerate(curve) if all(r["ship"] >= 0.8 for r in curve[i:])), None)
            ok &= req == w == R["required_players_per_flaw"][f"{g}/{b}"]
    check("N the table of players needed (15 cells), recomputed from power_recall", ok)
    rq = R["required_players_per_flaw"]
    check("N churn moves the answer about threefold (80 and 120 games, churn 0 to 0.10)", 2.8 < rq["80/0.1"] / rq["80/0.0"] < 3.0 and rq["120/0.1"] / rq["120/0.0"] == 3, f"{rq['80/0.1']}/{rq['80/0.0']}, {rq['120/0.1']}/{rq['120/0.0']}")

    # Quoted powers and their Monte Carlo intervals (R9).
    def mc(p):
        lo, hi = wilson(p * S_RECALL, S_RECALL, Z95)
        return float(lo), float(hi)

    for label, p, q, qlo, qhi in (
        ("80/0.1/40", cell(80, 0.1, 40)["ship"], 0.83, 0.81, 0.84),
        ("80/0.1/32", cell(80, 0.1, 32)["ship"], 0.71, 0.69, 0.72),
        ("full 80", ab[("full", 80)]["ship_power_delta_0_1"], 0.52, 0.51, 0.54),
        ("full 120", ab[("full", 120)]["ship_power_delta_0_1"], 0.28, 0.27, 0.30),
        ("smoke", ab[("smoke", 80)]["ship_power_delta_0_1"], 0.07, 0.06, 0.08),
    ):
        lo, hi = mc(p)
        check(f"N power {label} {q} [{qlo}, {qhi}]", near(q, p, 2) and near(qlo, lo, 2) and near(qhi, hi, 2), f"{p} [{lo:.4f}, {hi:.4f}]")
    lo, hi = mc(cell(80, 0.1, 40)["ship"])
    check("N ship_mc_ci95 of 80/0.1/40 is the Wilson interval at S = 4,000", abs(lo - cell(80, 0.1, 40)["ship_mc_ci95"][0]) < 1e-6 and abs(hi - cell(80, 0.1, 40)["ship_mc_ci95"][1]) < 1e-6)

    inf120 = [x["infeasible"] for x in pr if x["games"] == 120 and x["churn"] == 0.1]
    check("N 0.3% to 0.75% of the 120-game draws are infeasible (churn 0.1)", near(0.003, min(inf120), 3) and abs(max(inf120) - 0.0075) < 1e-9, f"{min(inf120)} to {max(inf120)}")
    rec = {x["games"]: x for x in R["observed"]["recall_at_2"]}
    check("N recall at 2: 0.73 at 120 games, 0.63 at 80", near(0.73, rec[120]["recall"], 2) and near(0.63, rec[80]["recall"], 2))
    mm = [c for p in flawed if p["flaw"] == "miss_mate" for c in p["checkpoints"]]
    check("N W4.missed ranked for none of six players at any checkpoint", all(c["target_rank"] is None for c in mm) and len(mm) == 30)
    check("N the same ordering at churn 0 (16 at 120 > 14 at 80)", R["required_players_per_flaw"]["120/0.0"] > R["required_players_per_flaw"]["80/0.0"])

    # Budget table and MDEs.
    check("N MDE (ship): 0.15 full 80, none full 120, none smoke", ab[("full", 80)]["mde_ship"] == 0.15 and ab[("full", 120)]["mde_ship"] is None and ab[("smoke", 80)]["mde_ship"] is None)
    for key, row in ab.items():
        first = lambda k: next((c["delta"] for c in row["curve"] if c[k] >= 0.8), None)  # noqa: E731
        ok = first("ship") == row["mde_ship"] and first("guard_flags_drop") == row["mde_guard_drop"] and first("ci_rule_flags_drop") == row["mde_ci_rule_drop"]
        check(f"N MDEs of {key} are the first grid change at 0.80", ok)
    c80 = {c["delta"]: c for c in ab[("full", 80)]["curve"]}
    check("N full ship curve: 0.96 at +0.2, 0.003 at +0.5", near(0.96, c80[0.2]["ship"], 2) and near(0.003, c80[0.5]["ship"], 3))
    check("N the smallest change shipped 80% lies between 0.10 and 0.15", c80[0.1]["ship"] < 0.8 <= c80[0.15]["ship"])
    d = R["decision"]
    check("N decision: budget-bound, full 22 at 80, smoke 3 at 80", d["outcome"].startswith("budget-bound") and d["full_suite"] == {"games": 80, "players_per_flaw": 22} and d["smoke_suite"] == {"games": 80, "players_per_flaw": 3})
    rq = R["required_players_per_flaw"]
    bm = R["budgets"]["full_players_per_flaw"]
    check("N budget-bound: required 40 > 22 and 48 > 14, and 80 games has the higher power", rq["80/0.1"] > bm["80"] and rq["120/0.1"] > bm["120"] and ab[("full", 80)]["ship_power_delta_0_1"] > ab[("full", 120)]["ship_power_delta_0_1"] + 0.01)

    pf80 = {f: tuple(rec[80]["per_flaw"][f]) for f in FLAWS}
    check("N 80 games: hang 6/6, low_clock 6/6, no_convert 5/6, opening 2/6, miss_mate 0/6, 19/30", pf80 == {"hang": (6, 6), "low_clock_blunder": (6, 6), "no_convert": (5, 6), "opening_blunder": (2, 6), "miss_mate": (0, 6)} and rec[80]["hits"] == 19)
    misses = {f: 6 - pf80[f][0] for f in FLAWS}
    check("N 10 of the 11 misses at 80 games are W4.missed and W7.traps", sum(misses.values()) == 11 and misses["miss_mate"] + misses["opening_blunder"] == 10)

    # The tolerances (R10, R11).
    def kstar(n, H, z):
        for k in range(1, H + 1):
            _, _, hi = newcombe(H - k, 0, k, n - H, z)
            if float(hi) < 0:
                return k
        return None

    for key, row in ab.items():
        n = row["flawed_players"]
        H = int(round(rec[row["games"]]["hits"] / 30 * n))
        k = kstar(n, H, Z_GUARD)
        tol = round((k - 0.5) / n, 3)
        check(f"N {key}: H {H}, k {k}, tolerance {tol}", H == row["reference_hits"] and k == row["guard_k_lost"] and abs(tol - row["recall_tolerance"]) < 1e-9)
    sm = ab[("smoke", 80)]
    check("N smoke: k 2 of n 15, tolerance 0.1", sm["guard_k_lost"] == 2 and sm["flawed_players"] == 15 and sm["recall_tolerance"] == 0.1)
    check("N smoke CI rule false alarm 0.118 (12%)", near(0.118, sm["ci_rule_false_alarm_delta_0"], 3) and near(0.12, sm["ci_rule_false_alarm_delta_0"], 2))
    cs = {c["delta"]: c for c in sm["curve"]}
    check("N smoke CI rule catches a drop of 0.2 81% and 0.1 49% of the time", near(0.81, cs[0.2]["ci_rule_flags_drop"], 2) and near(0.49, cs[0.1]["ci_rule_flags_drop"], 2), f"{cs[0.2]['ci_rule_flags_drop']}, {cs[0.1]['ci_rule_flags_drop']}")
    check("N coverage tolerance 0.05", d["coverage_tolerance_smoke"] == 0.05)
    f80, f120 = ab[("full", 80)], ab[("full", 120)]
    check("N full tolerance 0.014, false alarm 0.34", near(0.014, f80["recall_tolerance"], 3) and near(0.34, f80["ci_rule_false_alarm_delta_0"], 2))
    check("N guard false alarms 0.098, 0.093, 0.090", near(0.098, sm["guard_false_alarm_delta_0"], 3) and near(0.093, f80["guard_false_alarm_delta_0"], 3) and near(0.090, f120["guard_false_alarm_delta_0"], 3))
    check("N guard MDE 0.3, 0.1, 0.1; CI rule MDE 0.05 and 0.1, false alarms 0.34 and 0.30", (sm["mde_guard_drop"], f80["mde_guard_drop"], f120["mde_guard_drop"]) == (0.3, 0.1, 0.1) and (f80["mde_ci_rule_drop"], f120["mde_ci_rule_drop"]) == (0.05, 0.1) and near(0.30, f120["ci_rule_false_alarm_delta_0"], 2))
    base = json.loads(BASELINE_PATH.read_text(encoding="utf-8"))
    check("N bench/baseline.json installs 0.1 (recall) and 0.05 (coverage)", base["tolerance"]["recall_at_2"] == 0.1 and base["tolerance"]["coverage"] == 0.05)
    t = base["tolerance"]
    check("N untouched tolerances 0.2, 0.2, 0.25 (top1_precision, plan_hit_rate, top2_agreement)", (t["top1_precision"], t["plan_hit_rate"], t["top2_agreement"]) == (0.2, 0.2, 0.25))
    suite = R["pilot"]["suite"]
    check("N pilot settings: 12,000 nodes, 16 opponents; algorithm algo-1.1.0+fa5e2186e1dd", suite["nodes"] == 12000 and suite["opponents"] == 16 and R["algorithm_id"] == "algo-1.1.0+fa5e2186e1dd")
    grid = sorted({x["players_per_flaw"] for x in R["coverage_power"]})
    check("N 20 is the coverage grid size nearest 22", min(grid, key=lambda v: abs(v - 22)) == 20, f"grid {grid}")

    # Coverage (secondaries).
    cov = {x["games_up_to"]: x for x in R["observed"]["coverage"]}
    c = cov[80]
    v80 = flaw_vectors(80)
    point = float(np.mean(np.concatenate([v80[f].mean(1) for f in FLAWS])))
    pooled = float(np.concatenate([v80[f].ravel() for f in FLAWS]).mean())
    check("N coverage to 80 games 0.575 [0.467, 0.683], 120 intervals, = pooled share", near(0.575, point, 3) and abs(point - pooled) < 1e-12 and c["intervals"] == 120 and near(0.467, c["ci95_cluster_bootstrap"][0], 3) and near(0.683, c["ci95_cluster_bootstrap"][1], 3))
    c2 = cov[120]
    check("N coverage to 120 games 0.58 [0.48, 0.67]", near(0.58, c2["coverage"], 2) and near(0.48, c2["ci95_cluster_bootstrap"][0], 2) and near(0.67, c2["ci95_cluster_bootstrap"][1], 2) and abs(c2["coverage"] - R["pilot"]["scorecard_metrics"]["coverage"]) < 1e-6)
    pf = {f: float(v80[f].mean()) for f in FLAWS}
    check("N per flaw: W1 0.42, W4.missed 0.79, W7.traps 0.38, W9b 0.63, W5 0.67", near(0.42, pf["hang"], 2) and near(0.79, pf["miss_mate"], 2) and near(0.38, pf["opening_blunder"], 2) and near(0.63, pf["low_clock_blunder"], 2) and near(0.67, pf["no_convert"], 2))

    groups = [v80[f][j] for f in FLAWS for j in range(6)]
    allv = np.concatenate(groups)
    gm = allv.mean()
    kk, NN = len(groups), allv.size
    msb = sum(gg.size * (gg.mean() - gm) ** 2 for gg in groups) / (kk - 1)
    msw = sum(((gg - gg.mean()) ** 2).sum() for gg in groups) / (NN - kk)
    sizes = np.array([gg.size for gg in groups], dtype=float)
    n0 = (sizes.sum() - (sizes**2).sum() / sizes.sum()) / (kk - 1)
    icc = (msb - msw) / (msb + (n0 - 1) * msw)
    check("N within-player ICC 0.30 (Searle n0)", near(0.30, icc, 2) and abs(icc - c["icc_within_player"]) < 1e-6, f"{icc:.6f}")

    cp = {x["players_per_flaw"]: x for x in R["coverage_power"] if x["games_up_to"] == 80}
    check("N coverage half-width 0.057 at 20, 0.091 at 8, 0.153 at 3", near(0.057, cp[20]["half_width_95"], 3) and near(0.091, cp[8]["half_width_95"], 3) and near(0.153, cp[3]["half_width_95"], 3))
    check("N coverage paired power at 20: 0.52 (interval flips), 0.23 (player flips)", near(0.52, cp[20]["power_interval_flips"], 2) and near(0.23, cp[20]["power_player_flips"], 2))

    # Observed recall with Wilson 95%.
    for g, q, lo_q, hi_q, hits in ((80, 0.63, 0.46, 0.78, 19), (120, 0.73, 0.56, 0.86, 22)):
        lo, hi = wilson(hits, 30, Z95)
        check(f"N recall at {g}: {hits} of 30, {q} [{lo_q}, {hi_q}] (Wilson)", rec[g]["hits"] == hits and near(q, hits / 30, 2) and near(lo_q, float(lo), 2) and near(hi_q, float(hi), 2) and abs(float(lo) - rec[g]["ci95"][0]) < 1e-6)

    ob = [c for p in flawed if p["flaw"] == "opening_blunder" for c in p["checkpoints"]]
    check("N opening players' rank 1 is W1 at 29 of 30 checkpoints, the other empty at 30", sum(c["rank1"] == "W1" for c in ob) == 29 and [c["games"] for c in ob if c["rank1"] != "W1"] == [30] and all(c["rank1"] is None for c in ob if c["rank1"] != "W1"))
    cr = [(p["name"], c["games"], c["rank1"]) for p in clean for c in p["checkpoints"] if c["rank1"]]
    check("N clean controls: W9a at 45 and W5 at 80, 2 of 10 checkpoints", sorted((g, k) for _, g, k in cr) == [(45, "W9a"), (80, "W5")] and len({n for n, _, _ in cr}) == 2)
    check("N every CoachingInput deterministic", all(p["deterministic"] for p in players) and R["pilot"]["scorecard_metrics"]["deterministic"])
    check("N against the thresholds: 0.63 < 0.8 and 0.575 < 0.65", rec[80]["recall"] < 0.8 and point < 0.65)


if __name__ == "__main__":
    symbolic()
    simulation()
    numbers()
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem} (lab/lean/)")
    print(f"{'FAIL' if FAILURES else 'PASS'}: {len(FAILURES)} failed")
    sys.exit(1 if FAILURES else 0)
