"""EXP-0002: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy): the identities of math.md section 4, each as `simplify(lhs - rhs) == 0`,
   and the inequalities and limits symbolically or on a dense grid (the line says which).
2. Simulation (numpy, fixed seeds): each result's model with a known truth, and the replay of
   the registered draws through the formulas of math.md section 3 (written here from section 3,
   not imported from analysis.py), which must give the committed coverages exactly.
3. Numbers: every number decision.md and summary.md quote, recomputed from results.json and the
   replay, within the rounding it is quoted at.

Standard library, numpy, scipy and sympy only. About 30 s of CPU.
"""

from __future__ import annotations

import hashlib
import json
import math
import sys
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import stats

HERE = Path(__file__).resolve().parent
R = json.loads((HERE / "results.json").read_text(encoding="utf-8"))
EXP1 = HERE.parent / "EXP-0001" / "results.json"

# ---- the design, from math.md section 1 (preregistration section 7) -------------------------
# key: (chance law, mean chances a game E[c], m for the dispersion or None, B2 peers' rate,
#       prior strength n0, peer prior?)
KEYS = {
    "W1": (("poisson", 53.0), 53.0, 53.0, 0.004, 300.0, True),
    "W4.missed": (("poisson", 1.0), 1.0, None, 0.30, 40.0, True),
    "W7.traps": (("one",), 1.0, None, 0.10, 40.0, True),
    "W9b": (("zip", 1 / 3, 31.8), 31.8 / 3, 31.8, 0.02, 40.0, True),
    "W5": (("bernoulli", 0.4), 0.4, None, 0.15, 2.0, False),
}
BANDS = {"B1": 1.25, "B2": 1.0, "B3": 0.8}
RHOS = [0.5, 1.0, 2.0, 4.0, 8.0]
PRIMARY = (2.0, 4.0, 8.0)
DISPS = ["none", "de1.5"]
RECS = ["spread", "compressed"]
CHECKPOINTS = [30, 60, 120, 250]
G = 250
NPL = 200
D_EFF = 1.5
HALF_LIFE = 60.0
WINDOW = 183.0
NOMINAL = 0.8
BAND = (0.75, 0.85)
EPL_PEER, EPL_W5 = 0.3, 1.0
MIN_EXCESS = 0.02
ISSUE13 = {"theta": 0.023, "mu": 0.0008}
B_BOOT = 2000
Z90 = float(stats.norm.ppf(0.9))  # 1.2816, the 80% central quantile
Z95 = 1.959964
LEAN = {
    "Result 1 (Beta-binomial conjugacy, unit weights)": "Rematch.beta_binomial_posterior",
    "Result 12 (Bonferroni intervals for 20)": "Rematch.bonferroni",
    "Result 14 (Holm over 21 margins)": "Rematch.holm_fwer",
}

FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"{'PASS' if ok else 'FAIL'}  {name}{': ' + detail if detail else ''}")
    if not ok:
        FAILURES.append(name)


def zero(expr) -> bool:
    return sp.simplify(expr) == 0


def near(quoted: float, value: float, dp: int) -> bool:
    """`value` rounds to `quoted` at `dp` decimals."""
    return abs(quoted - value) <= 0.5 * 10.0 ** (-dp) + 1e-9


def odds_scale(p: float, f: float) -> float:
    o = p / (1 - p) * f
    return o / (1 + o)


def prior_of(key: str, band: str) -> tuple[float, float]:
    """(mu, n0) of the posterior: the peer row's pooled rate and n0, or W5's flat Beta(1, 1)."""
    _, _, _, mu2, n0, peer = KEYS[key]
    return (odds_scale(mu2, BANDS[band]), n0) if peer else (0.5, 2.0)


def median_of(key: str, band: str) -> float:
    _, _, _, mu2, _, peer = KEYS[key]
    return odds_scale(mu2, BANDS[band]) if peer else mu2


def theta_of(key: str, band: str, rho: float) -> float:
    return odds_scale(median_of(key, band), rho)


def ages(rec: str, g: int) -> np.ndarray:
    """Ages in days of games 1..g at the checkpoint g (oldest first)."""
    j = np.arange(g)[::-1].astype(float)
    return j * 0.5 + 1 / 24 if rec == "spread" else j / G + 1 / 24


def weights(rec: str, g: int) -> np.ndarray:
    return 0.5 ** (ages(rec, g) / HALF_LIFE)


# ---- the replay: the registered draws through math.md section 3 ------------------------------


def seed(name: str) -> int:
    return int.from_bytes(hashlib.sha256(f"EXP-0002/{name}".encode()).digest()[:4], "big")


def draw_chances(rng, spec, n):
    kind = spec[0]
    if kind == "poisson":
        return rng.poisson(spec[1], n)
    if kind == "one":
        return np.ones(n, dtype=np.int64)
    if kind == "zip":
        reach = rng.random(n) < spec[1]
        return np.where(reach, rng.poisson(spec[2], n), 0)
    return (rng.random(n) < spec[1]).astype(np.int64)


def draw_events(rng, c, theta, m, disp):
    if disp == "de1.5" and m is not None:
        kappa = (m - 1) / (D_EFF - 1) - 1
        return rng.binomial(c, rng.beta(theta * kappa, (1 - theta) * kappa, c.size))
    return rng.binomial(c, theta)


def score(c, e, theta, mu, n0, median, epl, rec):
    """Every checkpoint of one cell: (E1)-(E5) of math.md section 3."""
    out = {}
    for g in CHECKPOINTS:
        w = weights(rec, g)
        k = (e[:, :g] * w).sum(1)
        n = (c[:, :g] * w).sum(1)
        a = n0 * mu + k
        b = n0 * (1 - mu) + n - k
        mean = a / (a + b)
        q10 = stats.beta.ppf(0.1, a, b)
        q90 = stats.beta.ppf(0.9, a, b)
        low, high = np.minimum(q10, mean), np.maximum(q90, mean)
        k_unw = e[:, :g].sum(1)
        n_unw = c[:, :g].sum(1)
        excess = np.maximum(0.0, mean - median) * (n_unw / g) * epl
        out[g] = {
            "a": a, "b": b, "k": k, "n": n, "mean": mean, "low": low, "high": high,
            "widened": (q10 > mean) | (q90 < mean),
            "covered": (low <= theta) & (theta <= high),
            "pit": stats.beta.cdf(theta, a, b),
            "credible": (k_unw > 0) & (low > median) & (excess >= MIN_EXCESS),
            "k_unw": k_unw, "theta": theta,
        }
    return out


def replay():
    cells = {}
    for band in BANDS:
        for rho in RHOS:
            for disp in DISPS:
                rng = np.random.default_rng(seed(f"players/{band}/{rho}/{disp}"))
                for key, (spec, _, m, _, _, peer) in KEYS.items():
                    theta = theta_of(key, band, rho)
                    c = draw_chances(rng, spec, NPL * G).reshape(NPL, G)
                    e = draw_events(rng, c.ravel(), theta, m, disp).reshape(c.shape)
                    mu, n0 = prior_of(key, band)
                    med = median_of(key, band)
                    for rec in RECS:
                        sc = score(c, e, theta, mu, n0, med, EPL_PEER if peer else EPL_W5, rec)
                        for g, v in sc.items():
                            cells[(key, band, rho, disp, rec, g)] = v
    issue = {}
    for disp in DISPS:
        rng = np.random.default_rng(seed(f"issue13/{disp}"))
        spec, _, m, _, n0, _ = KEYS["W1"]
        c = draw_chances(rng, spec, NPL * G).reshape(NPL, G)
        e = draw_events(rng, c.ravel(), ISSUE13["theta"], m, disp).reshape(c.shape)
        sc = score(c, e, ISSUE13["theta"], ISSUE13["mu"], n0, ISSUE13["mu"], EPL_PEER, "spread")
        for g, v in sc.items():
            issue[(disp, g)] = v
    return cells, issue


def wilson(x, n, z=Z95):
    p = x / n
    den = 1 + z * z / n
    ctr = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return ctr - h, ctr + h


def player_shares(cells, pred):
    """Per stratum (band, rho, disp, rec), each player's share of covered intervals kept by
    `pred`, strata in analysis order (sorted by their text)."""
    acc: dict[tuple, list[np.ndarray]] = {}
    for (key, band, rho, disp, rec, g), v in cells.items():
        if pred(key, band, rho, disp, rec, g):
            acc.setdefault((band, rho, disp, rec), []).append(v["covered"].astype(float))
    return [np.mean(acc[s], axis=0) for s in sorted(acc, key=str)]


def boot(rng, strata):
    out = np.empty(B_BOOT)
    for b in range(B_BOOT):
        out[b] = np.mean([y[rng.integers(0, y.size, y.size)].mean() for y in strata])
    return out


def strat_var(strata):
    """Result 11: the exact bootstrap variance, (1/C^2) sum sigma_c^2 / n_c (divisor n_c)."""
    return sum(y.var() / y.size for y in strata) / len(strata) ** 2


# ---- 1. symbolic ------------------------------------------------------------------------------


def symbolic() -> None:
    th, a, b, k, n, n0, mu, w = sp.symbols("theta alpha beta k n n0 mu w", positive=True)

    # Result 1: prior kernel times (weighted) likelihood kernel is the Beta(a + k, b + n - k)
    # kernel, for real k and n; the Beta mean via Gamma(x + 1) = x Gamma(x).
    lhs = th ** (a - 1) * (1 - th) ** (b - 1) * th**k * (1 - th) ** (n - k)
    rhs = th ** (a + k - 1) * (1 - th) ** (b + n - k - 1)
    check("R1 conjugate kernel (real k, n)", zero(sp.powsimp(lhs / rhs, force=True) - 1))
    mean = sp.gamma(a + 1) * sp.gamma(a + b) / (sp.gamma(a) * sp.gamma(a + b + 1))
    check("R1 Beta mean a/(a+b)", zero(sp.gammasimp(mean) - a / (a + b)))
    # the weighted likelihood: prod (th^e (1-th)^(c-e))^w_t has exponents sum w e, sum w (c - e)
    e1, e2, c1, c2, w1, w2 = sp.symbols("e1 e2 c1 c2 w1 w2", positive=True)
    prod = (th**e1 * (1 - th) ** (c1 - e1)) ** w1 * (th**e2 * (1 - th) ** (c2 - e2)) ** w2
    kk, nn = w1 * e1 + w2 * e2, w1 * c1 + w2 * c2
    check(
        "R1 tempered likelihood has exponents (k, n - k)",
        zero(sp.expand_log(sp.log(prod) - sp.log(th**kk * (1 - th) ** (nn - kk)), force=True)),
    )
    # W5: (mu, n0) = (1/2, 2) is Beta(1, 1)
    check("R1 W5 (mu, n0) = (1/2, 2) gives shapes (1, 1) (arithmetic)", 2 * sp.Rational(1, 2) == 1 and 2 * (1 - sp.Rational(1, 2)) == 1)

    # Result 3: the posterior mean is lambda mu + (1 - lambda) k/n; E[m | c] - theta.
    lam = n0 / (n0 + n)
    m = (n0 * mu + k) / (n0 + n)
    check("R3 mean = lam mu + (1-lam) k/n", zero(m - (lam * mu + (1 - lam) * k / n)))
    m_given = (n0 * mu + th * n) / (n0 + n)  # E[k | c] = theta n
    check("R3 E[m|c] - theta = lam (mu - theta)", zero(m_given - th - lam * (mu - th)))
    check("R3 E[m|c]/theta = 1 - lam (1 - mu/theta)",
          zero(m_given / th - (1 - lam * (1 - mu / th))))

    # Result 4: spread-schedule sums. w_j = q0 r^j; r_g = sum w^2 / sum w = q0 (1 + r^g)/(1 + r).
    q0, r, gs = sp.symbols("q0 r g", positive=True)
    s1 = q0 * (1 - r**gs) / (1 - r)
    s2 = q0**2 * (1 - r ** (2 * gs)) / (1 - r**2)
    check("R4 sum w^2 / sum w = q0 (1 + r^g)/(1 + r)", zero(s2 / s1 - q0 * (1 + r**gs) / (1 + r)))
    for g in (30, 250):
        j = sp.symbols("j", integer=True, nonnegative=True)
        qv, rv = sp.Integer(2) ** sp.Rational(-1, 1440), sp.Integer(2) ** sp.Rational(-1, 120)
        direct = sum(float(qv * rv**jj) for jj in range(g))
        check(f"R4 geometric sum at g={g}",
              abs(direct - float(s1.subs({q0: qv, r: rv, gs: g}))) < 1e-9)
        _ = j
    # ages: j/2 + 1/24 days at half-life 60 -> exponent j/120 + 1/1440
    jj = sp.symbols("j", nonnegative=True)
    check("R4 age exponent", zero((jj / 2 + sp.Rational(1, 24)) / 60 - (jj / 120 + sp.Rational(1, 1440))))
    limit = sp.limit(s1.subs({q0: 2 ** sp.Rational(-1, 1440), r: 2 ** sp.Rational(-1, 120)}), gs, sp.oo)
    check("R4 infinite-history limit 173.5", abs(float(limit) - 173.54) < 0.005, f"{float(limit):.2f}")
    s366 = float(s1.subs({q0: 2 ** sp.Rational(-1, 1440), r: 2 ** sp.Rational(-1, 120), gs: 366}))
    check("R4 window limit (366 games) 152.6", abs(s366 - 152.6) < 0.05, f"{s366:.2f}")
    check("R4 window holds 366 games, not 367", 365 / 2 + 1 / 24 <= WINDOW < 366 / 2 + 1 / 24)
    rlim = float(sp.limit((q0 * (1 + r**gs) / (1 + r)).subs(
        {q0: 2 ** sp.Rational(-1, 1440), r: 2 ** sp.Rational(-1, 120)}), gs, sp.oo))
    check("R4 r_g -> q0/(1+r) = 0.501", abs(rlim - 0.5012) < 5e-4, f"{rlim:.4f}")
    # lambda grows when weights shrink n: d lam / d n < 0
    check("R4 d lambda / d n < 0", sp.simplify(sp.diff(lam, n)) == -n0 / (n0 + n) ** 2)
    # w^2 <= w on [0, 1] (dense grid and the factorisation w - w^2 = w (1 - w))
    wg = np.linspace(0, 1, 100_001)
    check("R4 w^2 <= w on [0,1] (identity w - w^2 = w(1-w), sign on a grid)",
          zero(w - w**2 - w * (1 - w)) and bool(np.all(wg - wg**2 >= 0)))
    check("R4 lambda convex in n (Jensen step)", sp.simplify(sp.diff(lam, n, 2) - 2 * n0 / (n0 + n) ** 3) == 0)
    sw = [float(s1.subs({q0: 2 ** sp.Rational(-1, 1440), r: 2 ** sp.Rational(-1, 120), gs: gg})) for gg in (45, 46)]
    check("R4 n0 = 40 weighs as the first 46 spread games", sw[0] < 40 <= sw[1], f"{sw[0]:.2f}, {sw[1]:.2f}")

    # Result 5: the Beta-binomial game. tg ~ Beta(theta kappa, (1-theta) kappa), e | tg ~ Bin(c, tg).
    kap, c, Dd, mm = sp.symbols("kappa c D m", positive=True)
    e_tg = th
    var_tg = th * (1 - th) / (kap + 1)
    e_tg2 = var_tg + e_tg**2
    var_e = c * (e_tg - e_tg2) + c**2 * var_tg  # law of total variance
    check("R5 Var(e|c) = c th(1-th)(1 + (c-1)/(kappa+1))",
          zero(var_e - c * th * (1 - th) * (1 + (c - 1) / (kap + 1))))
    # Beta variance: ab/((a+b)^2 (a+b+1)) with a = th kappa, b = (1-th) kappa
    av, bv = th * kap, (1 - th) * kap
    check("R5 Beta(th k, (1-th) k) variance", zero(av * bv / ((av + bv) ** 2 * (av + bv + 1)) - var_tg))
    kap_reg = (mm - 1) / (Dd - 1) - 1
    check("R5 kappa = (m-1)/(D-1) - 1 gives design effect D at c = m",
          zero((1 + (mm - 1) / (kap_reg + 1)) - Dd))
    # Poisson(m) chances: E[c d]/E[c] = 1 + E[c(c-1)]/(E[c](kappa+1)), E[c(c-1)] = m^2
    de_pois = 1 + mm**2 / (mm * (kap_reg + 1))
    check("R5 pooled design effect 1 + (D-1) m/(m-1)", zero(de_pois - (1 + (Dd - 1) * mm / (mm - 1))))
    d1 = float(de_pois.subs({mm: 53, Dd: sp.Rational(3, 2)}))
    d9 = float(de_pois.subs({mm: sp.Rational(318, 10), Dd: sp.Rational(3, 2)}))
    check("R5 W1 1.510, W9b 1.516", near(1.510, d1, 3) and near(1.516, d9, 3), f"{d1:.4f}, {d9:.4f}")
    check("R5 kappa W1 103, W9b 60.6",
          zero(kap_reg.subs({mm: 53, Dd: sp.Rational(3, 2)}) - 103)
          and zero(kap_reg.subs({mm: sp.Rational(318, 10), Dd: sp.Rational(3, 2)}) - sp.Rational(606, 10)))

    # Result 6: posterior variance m(1-m)/(N+1); sd ratio
    N = a + b
    check("R6 Beta variance = m(1-m)/(N+1)",
          zero(a * b / (N**2 * (N + 1)) - (a / N) * (1 - a / N) / (N + 1)))
    S2 = sp.symbols("S2", positive=True)
    s_post2 = th * (1 - th) / (n0 + n)
    sig2 = th * (1 - th) * S2 / (n0 + n) ** 2
    check("R6 posterior var / sampling var = (n0+n)/S2", zero(s_post2 / sig2 - (n0 + n) / S2))
    rr, DD = sp.symbols("r D", positive=True)
    check("R6 with S2 = r D n: ratio = 1/(r D (1 - lam))",
          zero((n0 + n) / (rr * DD * n) - 1 / (rr * DD * (1 - lam))))
    # (1 - lam) sd(p_hat) = sqrt(th(1-th) S2)/(n0 + n)
    check("R6 (1-lam)^2 Var(p_hat) = th(1-th) S2/(n0+n)^2",
          zero((1 - lam) ** 2 * th * (1 - th) * S2 / n**2 - sig2))

    # Result 7: coverage at theta = mu, 2 Phi(z/sqrt(v)) - 1 > 0.8 iff v < 1 (monotone in v)
    v = sp.symbols("v", positive=True)
    cov = sp.erf(sp.Float(Z90) / sp.sqrt(v) / sp.sqrt(2))  # 2 Phi(x) - 1 = erf(x / sqrt 2)
    check("R7 d coverage / d v < 0", bool(sp.simplify(sp.diff(cov, v)).is_negative))
    check("R7 coverage(v = 1) = 0.8", abs(float(cov.subs(v, 1)) - 0.8) < 1e-12)
    vals = [float(cov.subs(v, x)) for x in np.linspace(0.05, 3, 600)]
    check("R7 monotone on a grid of 600", all(x > y for x, y in zip(vals, vals[1:], strict=False)))

    # Result 10: Bhatia-Davis, X in [0, 1]: Var X <= mu (1 - mu); x(1-x) concave (Jensen)
    x = sp.symbols("x")
    xg = np.linspace(0, 1, 100_001)
    check("R10 X^2 <= X on [0,1] (identity and sign on a grid)",
          zero(x - x**2 - x * (1 - x)) and bool(np.all(xg >= xg**2)))
    check("R10 x(1-x) concave", sp.diff(x * (1 - x), x, 2) == -2)

    # Result 15: Wilson is the root pair of (p_hat - p)^2 = z^2 p (1-p)/n
    p, ph, zz = sp.symbols("p phat z", positive=True)
    den = 1 + zz**2 / n
    ctr = (ph + zz**2 / (2 * n)) / den
    hh = zz * sp.sqrt(ph * (1 - ph) / n + zz**2 / (4 * n**2)) / den
    quad = (ph - p) ** 2 - zz**2 * p * (1 - p) / n
    check("R15 Wilson bounds solve the score equation",
          zero(sp.expand(quad.subs(p, ctr + hh))) and zero(sp.expand(quad.subs(p, ctr - hh))))

    # Result 16: odds scaling stays in (0, 1); W4.missed at B1, rho 8
    pp, ff = sp.symbols("p f", positive=True)
    o = pp / (1 - pp) * ff
    check("R16 odds(scaled) = f odds(p)", zero((o / (1 + o)) / (1 - o / (1 + o)) - ff * pp / (1 - pp)))
    t = odds_scale(odds_scale(0.30, 1.25), 8.0)
    check("R16 W4.missed B1 rho 8 is 0.811", near(0.811, t, 3), f"{t:.4f}")
    ch = [odds_scale(0.004, 1.25) / 0.004 - 1, odds_scale(0.004, 0.8) / 0.004 - 1,
          odds_scale(0.30, 1.25) / 0.30 - 1, odds_scale(0.30, 0.8) / 0.30 - 1]
    check("R16 band moves +25%/-20% (small p), +16%/-15% (0.30)",
          near(0.25, ch[0], 2) and near(-0.20, ch[1], 2) and near(0.16, ch[2], 2) and near(-0.15, ch[3], 2),
          f"{[round(x_, 3) for x_ in ch]}")
    worst = max(theta_of(k, b_, r_) for k in KEYS for b_ in BANDS for r_ in RHOS)
    check("R16 every true rate below 1", worst < 1, f"max {worst:.4f}")

    # Result 2: F(mean) in [0.1, 0.9] on a dense grid of shapes >= 0.2 (so no widening)
    aa = np.exp(np.linspace(np.log(0.2), np.log(2e5), 260))
    A, Bm = np.meshgrid(aa, aa)
    Fm = stats.beta.cdf(A / (A + Bm), A, Bm)
    check("R2 F(mean) in [0.1, 0.9] for shapes in [0.2, 2e5] (grid 260^2)",
          bool(np.all((Fm > 0.1) & (Fm < 0.9))), f"range [{Fm.min():.3f}, {Fm.max():.3f}]")
    check("R2 small-shape counterexample Beta(0.02, 40) widens",
          stats.beta.ppf(0.9, 0.02, 40) < 0.02 / 40.02)


# ---- 2. simulation ----------------------------------------------------------------------------


def simulation(cells) -> None:
    rng = np.random.default_rng(20261128)

    # Result 8: prior-averaged coverage is exactly 0.8 when theta ~ prior (unit weights).
    for label, mu, n0, nch in (("W4-like", 0.3, 40.0, 30), ("W1-like", 0.004, 300.0, 1590)):
        reps = 200_000
        th = rng.beta(n0 * mu, n0 * (1 - mu), reps)
        k = rng.binomial(nch, th)
        a, b = n0 * mu + k, n0 * (1 - mu) + nch - k
        cov = np.mean((stats.beta.ppf(0.1, a, b) <= th) & (th <= stats.beta.ppf(0.9, a, b)))
        se = math.sqrt(0.16 / reps)
        check(f"R8 Bayes coverage 0.8 ({label})", abs(cov - 0.8) < 4 * se, f"{cov:.4f}")

    # Result 3: E[m]/theta = 1 - E[lam](1 - mu/theta), weighted, dispersed (W1-like, spread 30).
    w = weights("spread", 30)
    reps = 40_000
    th, mu, n0, mch = 0.0154, 0.004, 300.0, 53.0
    c = rng.poisson(mch, (reps, 30))
    kap = (mch - 1) / 0.5 - 1
    e = rng.binomial(c, rng.beta(th * kap, (1 - th) * kap, c.shape))
    k, n = (e * w).sum(1), (c * w).sum(1)
    m = (n0 * mu + k) / (n0 + n)
    want = 1 - np.mean(n0 / (n0 + n)) * (1 - mu / th)
    se = (m / th).std() / math.sqrt(reps)
    check("R3 mean ratio formula (weighted, de1.5)", abs((m / th).mean() - want) < 4 * se,
          f"{(m / th).mean():.4f} vs {want:.4f}")

    # Result 5: Var(p_hat | c) = th(1-th) sum w^2 c d / n^2, with c fixed.
    cfix = rng.poisson(mch, 30)
    e = rng.binomial(cfix, rng.beta(th * kap, (1 - th) * kap, (reps, 30)))
    ph = (e * w).sum(1) / (cfix * w).sum()
    d = 1 + (cfix - 1) / (kap + 1)
    want = th * (1 - th) * (w**2 * cfix * d).sum() / (cfix * w).sum() ** 2
    rel = ph.var(ddof=1) / want - 1
    check("R5 weighted Beta-binomial variance", abs(rel) < 4 * math.sqrt(2 / (reps - 1)),
          f"rel {rel:+.4f}")
    # pooled design effect with Poisson chances: E[c d]/E[c]
    cc = rng.poisson(mch, 400_000)
    de = np.mean(cc * (1 + (cc - 1) / (kap + 1))) / np.mean(cc)
    check("R5 Poisson pooled design effect 1.510", abs(de - (1 + 0.5 * 53 / 52)) < 1e-3, f"{de:.4f}")

    # Result 7 (approximate): the normal-approximation coverage against the replay, primary.
    errs = []
    for key in KEYS:
        for g in CHECKPOINTS:
            exact = np.mean([cells[(key, b, r, d, "spread", g)]["covered"].mean()
                             for b in BANDS for r in PRIMARY for d in DISPS])
            approx = np.mean([approx_coverage(key, b, r, d, "spread", g)
                              for b in BANDS for r in PRIMARY for d in DISPS])
            errs.append((key, g, approx - exact))
    allmax = max(abs(x) for _, _, x in errs)
    check("R7 normal approximation within 0.018 of every primary coverage", allmax < 0.0185,
          f"max {allmax:.4f}")
    pairs = {("W1", 30): (0.653, 0.636), ("W1", 60): (0.715, 0.709), ("W1", 120): (0.768, 0.763),
             ("W1", 250): (0.820, 0.802), ("W4.missed", 30): (0.098, 0.100),
             ("W4.missed", 250): (0.266, 0.260), ("W5", 250): (0.902, 0.904)}
    got = {}
    for (key, g), (qa, qe) in pairs.items():
        ex = np.mean([cells[(key, b, r, d, "spread", g)]["covered"].mean()
                      for b in BANDS for r in PRIMARY for d in DISPS])
        ap = np.mean([approx_coverage(key, b, r, d, "spread", g) for b in BANDS for r in PRIMARY for d in DISPS])
        got[(key, g)] = near(qa, ap, 3) and near(qe, ex, 3)
    check("R7 the quoted approximation and replay pairs", all(got.values()),
          f"{[k for k, v in got.items() if not v]}")
    wsp, wco = weights("spread", 250), weights("compressed", 250)
    heur = [2 * stats.norm.cdf(Z90 / math.sqrt(float((x_**2).sum() / x_.sum()))) - 1 for x_ in (wsp, wco)]
    check("R7 W5 heuristic 0.897 (spread), 0.80 (compressed)", near(0.897, heur[0], 3) and near(0.80, heur[1], 2),
          f"{heur[0]:.4f}, {heur[1]:.4f}")
    # theta = mu (rho = 1): the over-coverage formula against the replay, every key, spread
    errs1 = []
    for key in KEYS:
        for g in CHECKPOINTS:
            exact = np.mean([cells[(key, b, 1.0, d, "spread", g)]["covered"].mean()
                             for b in BANDS for d in DISPS])
            approx = np.mean([approx_coverage(key, b, 1.0, d, "spread", g) for b in BANDS for d in DISPS])
            errs1.append(abs(approx - exact))
    check("R7 rho = 1 approximation within 0.038", max(errs1) < 0.0385, f"max {max(errs1):.3f}")

    # Result 9: unit weights, c = 1 a game: exact coverage as a binomial sum vs simulation.
    mu, n0, th, g = 0.10, 40.0, odds_scale(0.10, 4.0), 30
    kk = np.arange(g + 1)
    a, b = n0 * mu + kk, n0 * (1 - mu) + g - kk
    ins = (stats.beta.ppf(0.1, a, b) <= th) & (th <= stats.beta.ppf(0.9, a, b))
    exact = float((stats.binom.pmf(kk, g, th) * ins).sum())
    xs = rng.binomial(g, th, 200_000)
    aa_, bb_ = n0 * mu + xs, n0 * (1 - mu) + g - xs
    mm_ = aa_ / (aa_ + bb_)
    lo_ = np.minimum(stats.beta.ppf(0.1, aa_, bb_), mm_)
    hi_ = np.maximum(stats.beta.ppf(0.9, aa_, bb_), mm_)
    sim = np.mean((lo_ <= th) & (th <= hi_))
    check("R9 binomial-sum coverage (W7-like, g 30)", abs(sim - exact) < 4 * math.sqrt(exact * (1 - exact) / 200_000),
          f"exact {exact:.4f}, sim {sim:.4f}")
    check("R9 W7.traps rho 4, 30 games: C = 0.100", near(0.100, exact, 3))

    # Result 11: the stratified bootstrap variance is (1/C^2) sum sigma_c^2/n_c (simulated data).
    strata = [rng.binomial(1, p, 200).astype(float) for p in np.linspace(0.1, 0.9, 18)]
    bs = boot(np.random.default_rng(7), strata)
    want = strat_var(strata)
    check("R11 bootstrap variance formula", abs(bs.var(ddof=1) / want - 1) < 4 * math.sqrt(2 / (B_BOOT - 1)),
          f"ratio {bs.var(ddof=1) / want:.4f}")

    # Result 12: decision-rule error, 20 normal estimates at the band edges with known se.
    se = math.sqrt(0.16 / 3600)
    z20 = float(stats.norm.ppf(1 - 0.05 / 40))
    reps = 200_000
    truth = np.array([0.75, 0.85] * 10)
    est = truth + se * rng.standard_normal((reps, 20))
    lo, hi = est - z20 * se, est + z20 * se
    wrong_out = np.any((hi < BAND[0]) | (lo > BAND[1]), axis=1).mean()
    miss_any = np.any((truth < lo) | (truth > hi), axis=1).mean()
    check("R12 (arithmetic, normal model) P(any interval misses) <= 0.05", miss_any <= 0.05 + 4 * math.sqrt(0.05 * 0.95 / reps),
          f"{miss_any:.4f}")
    check("R12 false 'outside' <= P(miss)", wrong_out <= miss_any, f"{wrong_out:.5f}")
    truth2 = np.full(20, 0.80)
    truth2[0] = 0.74
    est = truth2 + se * rng.standard_normal((reps, 20))
    lo, hi = est - z20 * se, est + z20 * se
    false_within = np.all((lo >= BAND[0]) & (hi <= BAND[1]), axis=1).mean()
    check("R12 false 'within' (one cell at 0.74) <= 0.05/20", false_within <= 0.0025 + 4 * math.sqrt(0.0025 / reps),
          f"{false_within:.5f}")

    # Result 12 beyond the normal model: Bernoulli strata, a bootstrapped se, the real procedure.
    brng = np.random.default_rng(99)
    sims, misses = 300, 0
    ptrue = np.linspace(0.70, 0.90, 18)
    for _ in range(sims):
        strata = [(brng.random(200) < p_).astype(float) for p_ in ptrue]
        est_ = np.mean([y.mean() for y in strata])
        bs_ = np.empty(400)
        for b_ in range(400):
            bs_[b_] = np.mean([y[brng.integers(0, 200, 200)].mean() for y in strata])
        h_ = z20 * bs_.std(ddof=1)
        misses += not (est_ - h_ <= ptrue.mean() <= est_ + h_)
    check("R12 a bootstrapped Bonferroni interval misses about 0.05/20 (Bernoulli strata, 300 runs)",
          misses <= 3, f"{misses} misses (about 0.4 expected; 3 or fewer)")

    # Result 13: the power formula against simulation of the classification.
    for ctrue, want_label in ((0.78, "within"), (0.72, "outside")):
        sd = math.sqrt(ctrue * (1 - ctrue) / 3600)
        est = ctrue + sd * rng.standard_normal(400_000)
        h = z20 * sd
        sim = np.mean((est - h >= 0.75) & (est + h <= 0.85)) if want_label == "within" else np.mean(est + h < 0.75)
        f = power(ctrue, ctrue * (1 - ctrue), 3600, z20)[0 if want_label == "within" else 1]
        check(f"R13 (arithmetic, normal model) power formula at {ctrue} ({want_label})", abs(sim - f) < 4 * math.sqrt(f * (1 - f) / 400_000),
              f"formula {f:.4f}, sim {sim:.4f}")

    # Result 14: Holm as analysis.py computes it equals Lean's holmRejects, with ties; and FWER.
    def holm_adj(p):
        order = sorted(range(len(p)), key=lambda i: p[i])
        adj, run = [0.0] * len(p), 0.0
        for rank, i in enumerate(order):
            run = max(run, min(1.0, (len(p) - rank) * p[i]))
            adj[i] = run
        return adj

    def lean_rejects(p, alpha, i):
        return all(p[j] <= alpha / sum(1 for l_ in range(len(p)) if p[j] <= p[l_])
                   for j in range(len(p)) if p[j] <= p[i])

    agree = True
    for _ in range(3000):
        p = list(np.round(rng.random(21) ** 3, 3))  # rounding makes ties
        adj = holm_adj(p)
        for i in range(21):
            agree &= (adj[i] <= 0.05) == lean_rejects(p, 0.05, i)
    check("R14 Holm adjusted p <= alpha iff Lean holmRejects (3,000 vectors, ties)", agree)
    reps = 40_000
    zc = rng.standard_normal((reps, 21)) * 0.6 + 0.8 * rng.standard_normal((reps, 1))  # correlated nulls
    pv = 2 * stats.norm.sf(np.abs(zc / 1.0))
    srt = np.sort(pv, axis=1) * (21 - np.arange(21))
    rej = np.maximum.accumulate(srt, axis=1)[:, 0] <= 0.05
    check("R14 Holm FWER <= 0.05 (21 correlated true nulls)",
          rej.mean() <= 0.05 + 4 * math.sqrt(0.05 * 0.95 / reps), f"{rej.mean():.4f}")


def approx_coverage(key, band, rho, disp, rec, g):
    """Result 7: Phi((z s - B)/sigma) - Phi((-z s - B)/sigma) at the expected counts."""
    _, cbar, m, _, _, _ = KEYS[key]
    mu, n0 = prior_of(key, band)
    th = theta_of(key, band, rho)
    w = weights(rec, g)
    n = cbar * w.sum()
    ecd = cbar * (1 + (D_EFF - 1) / (m - 1) * m) if (m is not None and disp == "de1.5") else cbar
    s2 = (w**2).sum() * ecd
    lam = n0 / (n0 + n)
    tb = lam * mu + (1 - lam) * th
    s = math.sqrt(tb * (1 - tb) / (n0 + n))
    sig = math.sqrt(th * (1 - th) * s2) / (n0 + n)
    bias = lam * (mu - th)
    return stats.norm.cdf((Z90 * s - bias) / sig) - stats.norm.cdf((-Z90 * s - bias) / sig)


def power(c, v, n_players, z):
    """Result 13: P(within), P(outside) for a coverage c with player variance v."""
    se = math.sqrt(v / n_players)
    h = z * se
    within = max(0.0, stats.norm.cdf((BAND[1] - h - c) / se) - stats.norm.cdf((BAND[0] + h - c) / se))
    outside = stats.norm.cdf((BAND[0] - h - c) / se) + stats.norm.sf((BAND[1] + h - c) / se)
    return within, outside


# ---- 3. numbers -------------------------------------------------------------------------------


def numbers(cells, issue) -> None:
    # N1: the replay reproduces every committed cell exactly (counts of 200).
    by = {(c["key"], c["band"], c["rho"], c["disp"], c["recency"], c["games"]): c for c in R["cells"]}
    bad = 0
    for kk, c in by.items():
        v = cells[kk]
        n = v["covered"].size
        ok = (
            round(c["coverage"] * n) == int(v["covered"].sum())
            and round(c["below"] * n) == int((v["high"] < v["theta"]).sum())
            and round(c["above"] * n) == int((v["low"] > v["theta"]).sum())
            and round(c["credible"] * n) == int(v["credible"].sum())
            and round(c["no_event_share"] * n) == int((v["k_unw"] == 0).sum())
            and abs(c["mean_ratio"] - float(np.mean(v["mean"] / v["theta"]))) < 2e-6
            and abs(c["median_width_ratio"] - float(np.median((v["high"] - v["low"]) / v["theta"]))) < 2e-6
            and c["ci95"] == [round(x, 6) for x in wilson(int(v["covered"].sum()), n)]
            and abs(c["theta"] - v["theta"]) < 1e-6
        )
        bad += not ok
    check("N1 replay = results.json, all 1,200 cells (coverage, sides, credible, ratios, Wilson)",
          bad == 0 and len(by) == 1200, f"{bad} differ")
    widened = sum(int(v["widened"].sum()) for v in list(cells.values()) + list(issue.values()))
    check("N1 no interval was widened to its mean, #13 included (Result 2)", widened == 0)
    a13 = min(float(v["a"].min()) for v in issue.values())
    check("N1 #13 cell: alpha at least 18.9", a13 >= 18.9, f"{a13:.2f}")
    amin = min(float(v["a"].min()) for v in cells.values())
    bmin = min(float(v["b"].min()) for v in cells.values())
    nmax = max(float(v["n"].max()) for v in cells.values())
    check("N1 shapes positive, floor and cap never bind (Result 2)",
          amin > 0.2 and bmin > 0.2 and nmax + 300 < 1e7, f"min alpha {amin:.3f}, min beta {bmin:.2f}")

    # N2: issue #13 rows.
    ok13 = True
    for row in R["issue13"]["rows"]:
        v = issue[(row["disp"], row["games"])]
        x = int(v["covered"].sum())
        ok13 &= (round(row["coverage"] * 200) == x
                 and row["ci95"] == [round(y, 6) for y in wilson(x, 200)]
                 and round(row["below"] * 200) == int((v["high"] < v["theta"]).sum())
                 and abs(row["mean_ratio"] - float(np.mean(v["mean"] / v["theta"]))) < 2e-6
                 and round(row["credible"] * 200) == int(v["credible"].sum()))
    check("N2 replay = issue13 rows", ok13)
    rows = {(r_["disp"], r_["games"]): r_ for r_ in R["issue13"]["rows"]}
    q = [rows[("none", g)]["coverage"] for g in CHECKPOINTS]
    qd = [rows[("de1.5", g)]["coverage"] for g in CHECKPOINTS]
    check("N2 #13 none 0.53, 0.65, 0.65, 0.73; de1.5 0.53, 0.615, 0.675, 0.755",
          all(near(a_, b_, 2) for a_, b_ in zip([0.53, 0.645, 0.65, 0.73], q, strict=True))
          and qd == [0.53, 0.615, 0.675, 0.755])
    lo, hi = rows[("none", 30)]["ci95"]
    lo2, hi2 = rows[("none", 250)]["ci95"]
    check("N2 #13 [0.46, 0.60] at 30, [0.66, 0.79] at 250",
          near(0.46, lo, 2) and near(0.60, hi, 2) and near(0.66, lo2, 2) and near(0.79, hi2, 2))
    check("N2 #13 every estimate credible", all(r_["credible"] == 1.0 for r_ in R["issue13"]["rows"]))
    mr = rows[("none", 30)]["mean_ratio"]
    w = weights("spread", 30)
    lam_hat = float(np.mean(300 / (300 + issue[("none", 30)]["n"])))
    pred = 1 - lam_hat * (1 - ISSUE13["mu"] / ISSUE13["theta"])
    lam_plug = 300 / (300 + 53 * w.sum())
    v30 = issue[("none", 30)]
    se_mr = float(np.std(v30["mean"] / ISSUE13["theta"], ddof=1)) / math.sqrt(NPL)
    check("N2 #13 mean 0.83 of the truth at 30 (Result 3 with E[lambda] within 4 SE, and plug-in)",
          near(0.83, mr, 2) and abs(pred - mr) < 4 * se_mr
          and abs(0.83 - (1 - lam_plug * (1 - ISSUE13["mu"] / ISSUE13["theta"]))) < 0.01,
          f"{mr:.4f} vs {pred:.4f}, plug-in lambda {lam_plug:.3f}")

    # N3: the primary, its bootstrap replayed with the registered seed.
    prng = np.random.default_rng(seed("bootstrap/primary"))
    z20 = float(stats.norm.ppf(1 - 0.05 / 40))
    check("N3 z for 20 is 3.023 (quoted 3.02)", near(R["primary"]["z_bonferroni"], z20, 6) and near(3.02, z20, 2))
    pc = {(c["key"], c["games"]): c for c in R["primary"]["cells"]}
    ok_est = ok_ci = ok_se = True
    worst_rel = 0.0
    for key in KEYS:
        for g in CHECKPOINTS:
            strata = player_shares(cells, lambda k_, b_, r_, d_, rec, gg, key=key, g=g:
                                   k_ == key and gg == g and r_ in PRIMARY and rec == "spread")
            est = float(np.mean([y.mean() for y in strata]))
            bs = boot(prng, strata)
            se = float(bs.std(ddof=1))
            c = pc[(key, g)]
            ok_est &= near(c["coverage"], est, 6)
            ok_ci &= (near(c["ci_bonferroni"][0], est - z20 * se, 6) and near(c["ci_bonferroni"][1], est + z20 * se, 6)
                      and near(c["ci95"][0], float(np.quantile(bs, 0.025)), 6)
                      and near(c["ci95"][1], float(np.quantile(bs, 0.975)), 6))
            # Result 11 on the real data: the bootstrap se against the exact formula, which for
            # one interval a player is sqrt(sum p_c(1-p_c)/n)/C from the committed cells alone.
            pcs = [by[(key, b_, r_, d_, "spread", g)]["coverage"] for b_ in BANDS for r_ in PRIMARY for d_ in DISPS]
            se_formula = math.sqrt(sum(p_ * (1 - p_) / NPL for p_ in pcs)) / len(pcs)
            worst_rel = max(worst_rel, abs(se / se_formula - 1))
            ok_se &= abs(se / se_formula - 1) < 4 / math.sqrt(2 * (B_BOOT - 1))
            ok_est &= near(c["coverage"], float(np.mean(pcs)), 6)
    check("N3 the 20 coverages = mean of 18 committed cells", ok_est)
    check("N3 the 20 Bonferroni and 95% intervals replayed exactly", ok_ci)
    check("N3 bootstrap se = Result 11's formula from the cells (4 MC SE)", ok_se, f"max rel {worst_rel:.4f}")
    classes = [c["classification"] for c in R["primary"]["cells"]]

    def classify(lo_, hi_):
        if BAND[0] <= lo_ and hi_ <= BAND[1]:
            return "within the band"
        if hi_ < BAND[0] or lo_ > BAND[1]:
            return "outside the band"
        return "need more data"

    recl = [classify(*c["ci_bonferroni"]) for c in R["primary"]["cells"]]
    check("N3 12 outside, 2 within, 6 undecided; decision outside",
          recl == classes and classes.count("outside the band") == 12 and classes.count("within the band") == 2
          and classes.count("need more data") == 6 and R["primary"]["decision"] == "outside the band"
          and R["primary"]["outside"] == 12 and R["primary"]["within"] == 2)
    table = {
        ("W1", 30): (0.64, 0.61, 0.66), ("W1", 60): (0.71, 0.69, 0.73), ("W1", 120): (0.76, 0.74, 0.78),
        ("W1", 250): (0.80, 0.78, 0.82), ("W4.missed", 30): (0.10, 0.09, 0.11),
        ("W4.missed", 60): (0.15, 0.14, 0.17), ("W4.missed", 120): (0.21, 0.20, 0.23),
        ("W4.missed", 250): (0.26, 0.24, 0.28), ("W7.traps", 30): (0.20, 0.19, 0.22),
        ("W7.traps", 60): (0.27, 0.26, 0.29), ("W7.traps", 120): (0.36, 0.34, 0.38),
        ("W7.traps", 250): (0.43, 0.41, 0.45), ("W9b", 30): (0.71, 0.69, 0.73),
        ("W9b", 60): (0.75, 0.73, 0.78), ("W9b", 120): (0.78, 0.76, 0.81), ("W9b", 250): (0.84, 0.82, 0.86),
        ("W5", 30): (0.83, 0.82, 0.85), ("W5", 60): (0.85, 0.83, 0.86), ("W5", 120): (0.86, 0.84, 0.87),
        ("W5", 250): (0.90, 0.89, 0.92),
    }
    tab_bad = [k_ for k_, (e_, l_, h_) in table.items()
               if not (near(e_, pc[k_]["coverage"], 2) and near(l_, pc[k_]["ci_bonferroni"][0], 2)
                       and near(h_, pc[k_]["ci_bonferroni"][1], 2))]
    check("N3 decision table, 20 coverages and Bonferroni intervals", not tab_bad, f"{tab_bad}")
    outside = {(c["key"], c["games"]) for c in R["primary"]["cells"] if c["classification"] == "outside the band"}
    want_out = {("W1", 30), ("W1", 60), ("W9b", 30), ("W5", 250)} | {(k_, g) for k_ in ("W4.missed", "W7.traps") for g in CHECKPOINTS}
    within = {(c["key"], c["games"]) for c in R["primary"]["cells"] if c["classification"] == "within the band"}
    sides_ok = all((pc[k_]["ci_bonferroni"][1] < 0.75) == (k_ != ("W5", 250)) for k_ in want_out)
    check("N3 the outside and within lists, and their sides", outside == want_out and within == {("W1", 250), ("W9b", 120)} and sides_ok)
    dist = []
    for c in R["primary"]["cells"]:
        if c["classification"] == "outside the band":
            se_c = (c["ci_bonferroni"][1] - c["ci_bonferroni"][0]) / (2 * z20)
            edge = BAND[0] if c["coverage"] < BAND[0] else BAND[1]
            dist.append(abs(c["coverage"] - edge) / se_c)
    hi_out = max(c["ci_bonferroni"][1] for c in R["primary"]["cells"]
                 if c["classification"] == "outside the band" and c["coverage"] < 0.75)
    check("N3 outside cells 5.5 or more SE beyond the edge; highest low-side upper bound 0.731",
          near(5.5, min(dist), 1) and round(max(dist)) == 154 and near(0.731, hi_out, 3)
          and {(c["key"], c["games"]) for c in R["primary"]["cells"]
               if near(0.731, c["ci_bonferroni"][1], 3)} == {("W1", 60), ("W9b", 30)}, f"min {min(dist):.2f}, max {max(dist):.0f}")
    check("N3 worst W4.missed at 30 games 0.10 [0.09, 0.11]",
          R["primary"]["worst"]["key"] == "W4.missed" and R["primary"]["worst"]["games"] == 30)
    pooled = player_shares(cells, lambda k_, b_, r_, d_, rec, g: r_ in PRIMARY and rec == "spread")
    pbs = boot(prng, pooled)
    pest = float(np.mean([y.mean() for y in pooled]))
    check("N3 pooled 0.572 [0.568, 0.575], 3,600 players, 72,000 intervals",
          near(R["primary"]["pooled_coverage"], pest, 6)
          and near(R["primary"]["pooled_ci95"][0], float(np.quantile(pbs, 0.025)), 6)
          and near(R["primary"]["pooled_ci95"][1], float(np.quantile(pbs, 0.975)), 6)
          and near(0.572, pest, 3) and near(0.568, R["primary"]["pooled_ci95"][0], 3)
          and near(0.575, R["primary"]["pooled_ci95"][1], 3)
          and sum(y.size for y in pooled) == 3600 == R["primary"]["players"] and R["primary"]["intervals"] == 72000)
    raw = np.mean(np.concatenate([cells[(k_, b_, r_, d_, "spread", g)]["covered"] for k_ in KEYS for b_ in BANDS
                                  for r_ in PRIMARY for d_ in DISPS for g in CHECKPOINTS]))
    check("N3 pooled = calibration at 0.8 (Result 2 identity)",
          near(R["calibration_primary"]["empirical"][7], raw, 6) and near(R["primary"]["pooled_coverage"], raw, 6))
    sides = [(cells[(k_, b_, r_, d_, "spread", g)]["high"] < theta_of(k_, b_, r_)).mean()
             for k_ in KEYS for b_ in BANDS for r_ in PRIMARY for d_ in DISPS for g in CHECKPOINTS]
    sides2 = [(cells[(k_, b_, r_, d_, "spread", g)]["low"] > theta_of(k_, b_, r_)).mean()
              for k_ in KEYS for b_ in BANDS for r_ in PRIMARY for d_ in DISPS for g in CHECKPOINTS]
    check("N3 review: truth above the interval 39.8%, below 3.0% of primary intervals",
          near(0.398, float(np.mean(sides)), 3) and near(0.030, float(np.mean(sides2)), 3),
          f"{np.mean(sides):.4f}, {np.mean(sides2):.4f}")

    # N4: the margins, replayed with the registered seed.
    mrng = np.random.default_rng(seed("bootstrap/margins"))
    z21 = float(stats.norm.ppf(1 - 0.05 / 42))
    defs = ([("key", k_) for k_ in KEYS] + [("band", b_) for b_ in BANDS] + [("games", g) for g in CHECKPOINTS]
            + [("disp", d_) for d_ in DISPS] + [("rho", r_) for r_ in RHOS] + [("recency", rec) for rec in RECS])
    idx = {"key": 0, "band": 1, "rho": 2, "disp": 3, "recency": 4, "games": 5}
    ok_m = True
    sizes = {}
    se_ratio_ok = True
    for (factor, level), m in zip(defs, R["margins"], strict=True):
        def pred(*t, factor=factor, level=level):
            if t[idx[factor]] != level:
                return False
            if factor == "rho":
                return t[4] == "spread"
            if factor == "recency":
                return t[2] in PRIMARY
            return t[2] in PRIMARY and t[4] == "spread"

        strata = player_shares(cells, pred)
        est = float(np.mean([y.mean() for y in strata]))
        bs = boot(mrng, strata)
        se = float(bs.std(ddof=1))
        zst = (est - NOMINAL) / se
        p = float(2 * stats.norm.sf(abs(zst)))
        ok_m &= (m["factor"] == factor and m["level"] == level and near(m["coverage"], est, 6)
                 and near(m["ci_bonferroni"][0], est - z21 * se, 6) and near(m["ci_bonferroni"][1], est + z21 * se, 6)
                 and near(m["ci95"][0], float(np.quantile(bs, 0.025)), 6) and near(m["p_vs_0_80"], p, 6)
                 and m["classification"] == classify(est - z21 * se, est + z21 * se))
        se_ratio_ok &= abs(se / math.sqrt(strat_var(strata)) - 1) < 4 / math.sqrt(2 * (B_BOOT - 1))
        sizes[(factor, level)] = sum(y.size for y in strata)
    check("N4 the 21 margins replayed exactly (coverage, intervals, p, class)", ok_m)
    check("N4 margin bootstrap se = Result 11 formula on replayed shares", se_ratio_ok)
    check("N4 margins hold 1,200 (band, rho), 1,800 (disp), 3,600 (key, games, recency) players",
          all(v == 1200 for (f, _), v in sizes.items() if f in ("band", "rho"))
          and all(v == 1800 for (f, _), v in sizes.items() if f == "disp")
          and all(v == 3600 for (f, _), v in sizes.items() if f in ("key", "games", "recency")))
    hw = max((m["ci_bonferroni"][1] - m["ci_bonferroni"][0]) / 2 for m in R["margins"])
    check("N4 every margin interval narrower than 0.025 a side", hw < 0.025, f"max {hw:.4f}")
    check("N4 all 21 Holm p below 0.001", all(m["p_holm"] < 0.001 for m in R["margins"]))
    check("N4 z for 21 is 3.038 (preregistration says 3.03)", near(3.038, z21, 3), f"{z21:.4f}")
    mg = {(m["factor"], m["level"]): m for m in R["margins"]}
    quoted = {
        ("key", "W1"): (0.73, 0.71, 0.74), ("key", "W4.missed"): (0.18, 0.17, 0.19),
        ("key", "W7.traps"): (0.32, 0.30, 0.33), ("key", "W9b"): (0.77, 0.76, 0.79),
        ("key", "W5"): (0.86, 0.85, 0.87), ("rho", 1.0): (0.87, 0.86, 0.88),
    }
    qbad = [k_ for k_, (e_, l_, h_) in quoted.items()
            if not (near(e_, mg[k_]["coverage"], 2) and near(l_, mg[k_]["ci_bonferroni"][0], 2)
                    and near(h_, mg[k_]["ci_bonferroni"][1], 2))]
    pts = {("rho", 0.5): 0.78, ("rho", 2.0): 0.72, ("rho", 4.0): 0.54, ("rho", 8.0): 0.46,
           ("band", "B1"): 0.56, ("band", "B2"): 0.57, ("band", "B3"): 0.58, ("games", 30): 0.50,
           ("games", 60): 0.55, ("games", 120): 0.60, ("games", 250): 0.65, ("disp", "none"): 0.59,
           ("disp", "de1.5"): 0.56}
    qbad += [k_ for k_, e_ in pts.items() if not near(e_, mg[k_]["coverage"], 2)]
    qbad += [] if (near(0.572, mg[("recency", "spread")]["coverage"], 3)
                   and near(0.566, mg[("recency", "compressed")]["coverage"], 3)) else ["recency"]
    cls = {k_: mg[k_]["classification"] for k_ in mg}
    want_cls = {k_: "outside the band" for k_ in mg}
    want_cls.update({("key", "W9b"): "within the band", ("key", "W5"): "need more data",
                     ("rho", 0.5): "within the band"})
    check("N4 decision's margin values and classes", not qbad and cls == want_cls, f"{qbad}")
    check("N4 dispersion costs about three points", near(0.03, mg[("disp", "none")]["coverage"] - mg[("disp", "de1.5")]["coverage"], 2))

    # N5: the schedule contrast (exploratory, mean of 18 cells), and rho = 1 for W4.missed, W7.traps.
    def sched(key, g, rec):
        return float(np.mean([by[(key, b_, r_, d_, rec, g)]["coverage"] for b_ in BANDS for r_ in PRIMARY for d_ in DISPS]))

    contrast = {("W1", 250): (0.80, 0.73), ("W9b", 120): (0.78, 0.73), ("W5", 250): (0.90, 0.80),
                ("W4.missed", 250): (0.26, 0.39), ("W7.traps", 250): (0.43, 0.54),
                ("W1", 120): (0.76, 0.72), ("W9b", 250): (0.84, 0.75)}
    cbad = [k_ for k_, (s_, c_) in contrast.items() if not (near(s_, sched(*k_, "spread"), 2) and near(c_, sched(*k_, "compressed"), 2))]
    check("N5 Finding 3: W5 at 250 without weighting is 0.805, quoted 0.81", near(0.805, sched("W5", 250, "compressed"), 3))
    check("N5 schedule contrast (decision and review; W5 compressed checked at 0.80)", not cbad,
          f"{cbad} {[(k_, round(sched(*k_, 'spread'), 4), round(sched(*k_, 'compressed'), 4)) for k_ in cbad]}")
    r1 = [float(np.mean([by[(k_, b_, 1.0, d_, "spread", g)]["coverage"] for b_ in BANDS for d_ in DISPS for g in CHECKPOINTS]))
          for k_ in ("W4.missed", "W7.traps")]
    check("N5 W4.missed and W7.traps together at rho 1 covered 0.93 (0.94 and 0.93 apart)",
          near(0.93, float(np.mean(r1)), 2) and near(0.94, r1[0], 2) and near(0.93, r1[1], 2), f"{r1}")
    mr = [by[("W4.missed", "B2", 4.0, d_, "spread", g)]["mean_ratio"] for d_ in DISPS for g in (30, 250)]
    th = theta_of("W4.missed", "B2", 4.0)
    check("N5 W4.missed B2 rho 4: true 0.63, mean 0.69 then 0.88 of the truth (mean of the two dispersion cells)",
          near(0.63, th, 2) and near(0.69, (mr[0] + mr[2]) / 2, 2) and near(0.88, (mr[1] + mr[3]) / 2, 2),
          f"{[round(x, 4) for x in mr]}")
    lam30 = 40 / (40 + weights("spread", 30).sum())
    lam250 = 40 / (40 + weights("spread", 250).sum())
    check("N5 plug-in Result 3: 0.69 and 0.88",
          near(0.69, 1 - lam30 * (1 - 0.3 / th), 2) and near(0.88, 1 - lam250 * (1 - 0.3 / th), 2),
          f"lambda {lam30:.3f}, {lam250:.3f}")
    check("N5 W1 300/53 about six games", near(5.7, 300 / 53, 1))

    # N6: credible, calibration.
    cr = {c["key"]: c for c in R["credible"]}
    ok_c = True
    for key in KEYS:
        rs = [(v["covered"][v["credible"]], v["high"][v["credible"]] < v["theta"])
              for (k_, _b, _r, _d, rec, _g), v in cells.items() if k_ == key and rec == "spread"]
        x = sum(int(a_.sum()) for a_, _ in rs)
        nn = sum(a_.size for a_, _ in rs)
        bl = sum(int(b_.sum()) for _, b_ in rs)
        ok_c &= (cr[key]["intervals"] == nn and near(cr[key]["coverage"], x / nn, 6)
                 and near(cr[key]["below"], bl / nn, 6)
                 and cr[key]["ci95_wilson"] == [round(y, 6) for y in wilson(x, nn)])
    check("N6 credible coverages replayed", ok_c)
    quoted_c = {"W1": 0.73, "W4.missed": 0.20, "W7.traps": 0.25, "W9b": 0.77, "W5": 0.83}
    check("N6 credible W1 0.73, W4.missed 0.20, W7.traps 0.25, W9b 0.77, W5 0.83; W4 truth above 80%",
          all(near(v_, cr[k_]["coverage"], 2) for k_, v_ in quoted_c.items()) and near(0.80, cr["W4.missed"]["below"], 2))
    check("N6 W9b's and W5's shown coverages inside the band (estimate and Wilson), W1's below",
          all(BAND[0] <= cr[k_]["coverage"] <= BAND[1] for k_ in ("W9b", "W5"))
          and all(BAND[0] <= x_ <= BAND[1] for k_ in ("W9b", "W5") for x_ in cr[k_]["ci95_wilson"])
          and cr["W1"]["coverage"] < BAND[0])
    emp = R["calibration_primary"]["empirical"]
    pits = np.concatenate([cells[(k_, b_, r_, d_, "spread", g)]["pit"] for k_ in KEYS for b_ in BANDS
                           for r_ in PRIMARY for d_ in DISPS for g in CHECKPOINTS])
    replay_emp = [float(np.mean(np.abs(pits - 0.5) <= q_ / 2)) for q_ in R["calibration_primary"]["levels"]]
    check("N6 calibration curve replayed; 50% 0.34, 80% 0.57, 95% 0.74",
          all(near(a_, b_, 6) for a_, b_ in zip(emp, replay_emp, strict=True))
          and near(0.34, emp[4], 2) and near(0.57, emp[7], 2) and near(0.74, emp[9], 2))
    calib_rho = all(near(c["empirical"][7], mg[("rho", c["rho"])]["coverage"], 6) for c in R["calibration"])
    check("N6 calibration at 0.8 = rho margin coverage for every rho (Result 2)", calib_rho)

    # N7: design arithmetic in decision.md, summary.md and preregistration section 6.
    computations = 30 * NPL * 2 * len(CHECKPOINTS) + 2 * NPL * len(CHECKPOINTS)
    check("N7 6,000 players, 49,600 computations", 30 * NPL == 6000 and computations == 49_600)
    check("N7 1,592 s is 0.44 CPU hours and 27 minutes", near(0.44, 1592 / 3600, 2) and round(1592 / 60) == 27)
    se0 = math.sqrt(0.16 / 3600)
    check("N7 se 0.0067, half-width 0.020 (z 3.02)", near(0.0067, se0, 4) and near(0.020, 3.02 * se0, 3))
    check("N7 rho margin worst half-width 0.035", near(0.035, float(stats.norm.ppf(1 - 0.05 / 42)) * math.sqrt(0.16 / 1200), 3))
    check("N7 100 players a cell: half-width 0.029", near(0.029, z20 * math.sqrt(0.16 / 1800), 3))
    wl, wh = wilson(160, 200)
    check("N7 Wilson half-width at 0.80, 200: 0.055 (quoted 'about 0.056')", near(0.055, (wh - wl) / 2, 3), f"{(wh - wl) / 2:.4f}")
    w08 = power(0.80, 0.16, 3600, z20)[0]
    w78 = power(0.78, 0.16, 3600, z20)[0]
    w82 = power(0.82, 0.16, 3600, z20)[0]
    o72 = power(0.72, 0.16, 3600, z20)[1]
    o88 = power(0.88, 0.16, 3600, z20)[1]
    check("N7 power at v = 0.16: 0.80 within > 0.99; 0.78/0.82 about 0.93; 0.72/0.88 outside > 0.9",
          w08 > 0.99 and near(0.93, w78, 2) and near(0.93, w82, 2) and o72 > 0.9 and o88 > 0.9,
          f"{w08:.5f}, {w78:.3f}, {w82:.3f}, {o72:.3f}, {o88:.3f}")
    o72b = power(0.72, 0.72 * 0.28, 3600, z20)[1]
    check("N7 Finding 1: at v = c(1-c) the 0.72 power is 0.84, below 0.9", near(0.84, o72b, 2), f"{o72b:.4f}")
    vstar = max(v_ for v_ in np.linspace(0.15, 0.21, 6001) if power(0.72, v_, 3600, z20)[1] >= 0.9)
    check("N7 Finding 1: the 0.72 claim needs a player variance below about 0.175", near(0.175, vstar, 3),
          f"{vstar:.4f}")
    vmax = max(float(v["covered"].var()) for kk, v in cells.items() if kk[2] in PRIMARY and kk[4] == "spread")
    check("N7 replayed primary strata have player variance up to 0.25 (above 0.175)", near(0.25, vmax, 2), f"{vmax:.4f}")
    w78s = power(0.78, 0.16, 1800, z20)[0]
    check("N7 Finding 2: 100 a cell, a true 0.78 passes 0.56 (quoted 'two times in three')",
          near(0.56, w78s, 2), f"{w78s:.4f}")
    t_min = float(weights("compressed", 250).min())
    check("N7 compressed weights above 0.988", t_min > 0.988, f"{t_min:.5f}")
    check("N7 spread: 250 games over 125 days, inside 183", float(ages("spread", 250).max()) < WINDOW
          and near(124.5, float(ages("spread", 250).max()) - 1 / 24, 6))
    if not EXP1.is_file():
        check("N7 EXP-0001's results.json present", False)
    else:
        e1 = json.loads(EXP1.read_text(encoding="utf-8"))
        v = e1["observed"]["coverage"][3]["per_flaw"]["miss_mate"]
        check("N7 EXP-0001's end-to-end W4.missed coverage 0.79", near(0.79, v, 2), f"{v}")
    check("N7 algorithm id", R["algorithm_id"] == "algo-1.1.0+fa5e2186e1dd")


if __name__ == "__main__":
    symbolic()
    cells_, issue_ = replay()
    simulation(cells_)
    numbers(cells_, issue_)
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem} (lab/lean/Rematch/)")
    print(f"{len(FAILURES)} FAIL" if FAILURES else "all checks PASS")
    sys.exit(1 if FAILURES else 0)
