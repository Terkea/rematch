"""EXP-0006: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy): the identities of math.md section 4, each as `simplify(lhs - rhs) == 0`,
   and the inequalities and limits symbolically or on a dense grid (the line says which).
2. Simulation (numpy, fixed seeds): each result's model with a known truth; and the replay of the
   registered draws (seed namespace EXP-0006) through both rules of math.md section 3, written
   here from section 3 and not imported from analysis.py, which must give the committed
   coverages, credible shares and bootstrap intervals.
3. Numbers: every number decision.md and summary.md quote, recomputed from results.json,
   dev/power.json and the replay, within the rounding it is quoted at.

Standard library, numpy, scipy and sympy only.
"""

from __future__ import annotations

import hashlib
import itertools
import json
import math
import sys
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import integrate, stats
from scipy.special import gammaln

HERE = Path(__file__).resolve().parent
R = json.loads((HERE / "results.json").read_text(encoding="utf-8"))
P = json.loads((HERE / "dev" / "power.json").read_text(encoding="utf-8"))
EXP2 = json.loads((HERE.parent / "EXP-0002" / "results.json").read_text(encoding="utf-8"))

# ---- the design, from math.md section 1 (EXP-0002's generator, preregistration section 7) ----
# key: (chance law, mean chances a game, m for the dispersion or None, B2 peers' rate, n0, peer?)
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
CHECKPOINTS = [30, 60, 120, 250]
G = 250
D_EFF = 1.5
HALF_LIFE = 60.0
NOMINAL = 0.8
BAND = (0.75, 0.85)
TOL = 0.07
EPL_PEER, EPL_W5 = 0.3, 1.0
MIN_EXCESS = 0.02
MAX_DISP = 10.0
PRIOR_GAMES = 2.0
ISSUE13 = {"theta": 0.023, "mu": 0.0008}
B_BOOT = 2000
RULES = ("champion", "challenger")
Z90 = float(stats.norm.ppf(0.9))  # 1.2816
Z9875 = float(stats.norm.ppf(0.9875))  # 2.2414
Z20 = float(stats.norm.ppf(1 - 0.05 / 40))  # 3.0233
Z4 = float(stats.norm.ppf(1 - 0.05 / 8))  # 2.4977
LEAN = {
    "Result 12 (Jeffreys interval, conjugacy with a = b = 1/2)": "Rematch.beta_binomial_posterior",
    "Result 11 (Bonferroni intervals for 20 and for 4)": "Rematch.bonferroni",
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


def median_of(key: str, band: str) -> float:
    _, _, _, mu2, _, peer = KEYS[key]
    return odds_scale(mu2, BANDS[band]) if peer else mu2


def theta_of(key: str, band: str, rho) -> float:
    return ISSUE13["theta"] if rho == "issue13" else odds_scale(median_of(key, band), rho)


def weights(g: int) -> np.ndarray:
    """Spread schedule: ages j/2 + 1/24 days (j games after), oldest first."""
    j = np.arange(g)[::-1].astype(float)
    return 0.5 ** ((j * 0.5 + 1 / 24) / HALF_LIFE)


def seed(name: str, ns: str = "EXP-0006") -> int:
    """`rematch_lab.experiments.seed`: the first 4 bytes of sha256("<ns>/<name>"), big-endian."""
    return int.from_bytes(hashlib.sha256(f"{ns}/{name}".encode()).digest()[:4], "big")


def players_of(rho) -> int:
    return 800 if rho in (0.5, 1.0, "issue13") else 200


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


def counts(c, e, g, rule):
    """(k, n) of (E1) (champion) or (E2) (challenger), and the parts of (E2)."""
    w = weights(g)
    cg, eg = c[:, :g].astype(float), e[:, :g].astype(float)
    k = (eg * w).sum(1)
    n = (cg * w).sum(1)
    if rule == "champion":
        return k, n, {}
    s2 = (cg * w**2).sum(1)
    with np.errstate(invalid="ignore", divide="ignore"):
        p = np.clip(np.where(n > 0, k / n, 0.0), 0, 1)
        neff = np.where(s2 > 0, n * n / np.where(s2 > 0, s2, 1), n)
        has = cg > 0
        games = has.sum(1)
        var = p * (1 - p)
        terms = np.where(has, (eg - cg * p[:, None]) ** 2 / np.where(has, cg, 1), 0.0).sum(1)
        chi2 = np.where(var > 0, terms / np.where(var > 0, var, 1), 0.0)
        phi = np.where((games >= 2) & (var > 0), chi2 / np.maximum(games - 1, 1), 1.0)
    phi = np.clip(phi, 1.0, MAX_DISP)
    nt = np.where((n > 0) & (s2 > 0), neff / phi, n)
    kt = np.where((n > 0) & (s2 > 0), p * nt, k)
    return kt, nt, {"neff": neff, "phi": phi, "p": p}


def score(c, e, theta, mu, n0, median, epl, peer, rule):
    """Every checkpoint of one cell under one rule: math.md section 3."""
    out = {}
    for g in CHECKPOINTS:
        k, n, parts = counts(c, e, g, rule)
        s0 = np.minimum(n0, PRIOR_GAMES * n / g) if (rule == "challenger" and peer) else np.full_like(n, n0)
        a = np.where(s0 * mu + k > 0, s0 * mu + k, 1e-3)
        b = np.where(s0 * (1 - mu) + n - k > 0, s0 * (1 - mu) + n - k, 1e-3)
        mean = a / (a + b)
        low = np.minimum(stats.beta.ppf(0.1, a, b), mean)
        high = np.maximum(stats.beta.ppf(0.9, a, b), mean)
        k_unw = e[:, :g].sum(1)
        excess = np.maximum(0.0, mean - median) * (c[:, :g].sum(1) / g) * epl
        out[g] = {
            "covered": ((low <= theta) & (theta <= high)).astype(float),
            "credible": ((k_unw > 0) & (low > median) & (excess >= MIN_EXCESS)).astype(float),
            "high_miss": (high < theta).astype(float),
            "low_miss": (low > theta).astype(float),
            "width": (high - low) / theta,
            "mean": mean, "lam": s0 / (s0 + n), "n": n, **parts,
        }
    return out


def plan() -> list[tuple]:
    return [(b, r, d) for b in BANDS for r in RHOS for d in DISPS] + [("B2", "issue13", d) for d in DISPS]


def replay(ns: str = "EXP-0006"):
    """data[rule][(key, g, cell)] = per-player dict of math.md section 3, for all 32 cells."""
    data = {v: {} for v in RULES}
    for cell in plan():
        band, rho, disp = cell
        npl = players_of(rho)
        if rho == "issue13":
            rng = np.random.default_rng(seed(f"issue13/{disp}", ns))
            todo = {"W1": KEYS["W1"]}
        else:
            rng = np.random.default_rng(seed(f"players/{band}/{rho}/{disp}", ns))
            todo = KEYS
        for key, (spec, _, m, _, n0, peer) in todo.items():
            theta = theta_of(key, band, rho)
            c = draw_chances(rng, spec, npl * G).reshape(npl, G)
            e = draw_events(rng, c.ravel(), theta, m, disp).reshape(c.shape)
            if rho == "issue13":
                mu = med = ISSUE13["mu"]
            else:
                med = median_of(key, band)
                mu = med if peer else 0.5
            for v in RULES:
                sc = score(c, e, theta, mu, n0, med, EPL_PEER if peer else EPL_W5, peer, v)
                for g, val in sc.items():
                    data[v][(key, g, cell)] = val
    return data


def resamples(rng) -> dict:
    """`analysis.py`'s paired draw: one (B, n) index array per cell, in plan order."""
    return {c: rng.integers(0, players_of(c[1]), size=(B_BOOT, players_of(c[1]))) for c in plan()}


def stat(store: dict, name: str, keys_g, cells, idx=None):
    """(E4): the equal-weighted mean over cells of the mean over players of each player's share."""
    per_cell = []
    for c in cells:
        player = np.mean([store[(k, g, c)][name] for k, g in keys_g], axis=0)
        per_cell.append(player[idx[c]].mean(axis=-1) if idx is not None else player.mean())
    return np.mean(per_cell, axis=0)


def q(boot, level):
    a = (1 - level) / 2
    return [float(np.quantile(boot, a)), float(np.quantile(boot, 1 - a))]


def cells_of(rhos):
    return [c for c in plan() if c[1] in rhos]


def profile_stats(data, idx) -> dict:
    """Every profile statistic of the rule, with its bootstrap replicates: math.md (E4)-(E8)."""
    prim = cells_of(PRIMARY)
    allkg = [(k, g) for k in KEYS for g in CHECKPOINTS]
    out = {}
    for v in RULES:
        cov, boots, cells = [], [], []
        for k in KEYS:
            for g in CHECKPOINTS:
                est = float(stat(data[v], "covered", [(k, g)], prim))
                bs = stat(data[v], "covered", [(k, g)], prim, idx)
                se = float(bs.std(ddof=1))
                cells.append({"key": k, "games": g, "coverage": est, "ci95": q(bs, 0.95),
                              "ci_bonferroni": [est - Z20 * se, est + Z20 * se], "se": se,
                              "true_rate_above": float(stat(data[v], "high_miss", [(k, g)], prim)),
                              "true_rate_below": float(stat(data[v], "low_miss", [(k, g)], prim)),
                              "median_width_ratio": float(np.median(np.concatenate(
                                  [data[v][(k, g, c)]["width"] for c in prim])))})
                cov.append(est)
                boots.append(bs)
        dev = np.abs(np.array(cov) - NOMINAL)
        dboot = np.abs(np.array(boots) - NOMINAL).max(axis=0)
        out[v] = {"cells": cells, "D": float(dev.max()), "dboot": dboot, "worst": cells[int(dev.argmax())],
                  "boots": np.array(boots)}
    rho1, low = cells_of([1.0]), cells_of([0.5, 1.0])
    g1 = {v: (float(stat(data[v], "covered", allkg, rho1)), stat(data[v], "covered", allkg, rho1, idx)) for v in RULES}
    fc = {v: (float(stat(data[v], "credible", allkg, low)), stat(data[v], "credible", allkg, low, idx)) for v in RULES}
    out["rho1"] = g1
    out["rho1_boot"] = np.abs(g1["challenger"][1] - NOMINAL) - np.abs(g1["champion"][1] - NOMINAL)
    out["fc"] = fc
    out["fc_boot"] = fc["challenger"][1] - fc["champion"][1]
    out["delta_boot"] = out["champion"]["dboot"] - out["challenger"]["dboot"]
    return out


# ---- shared formulas --------------------------------------------------------------------------


def wilson(x, n, z):
    x, n = np.asarray(x, dtype=float), np.asarray(n, dtype=float)
    p = x / n
    den = 1 + z * z / n
    ctr = (p + z * z / (2 * n)) / den
    h = z * np.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return ctr - h, ctr + h


def newcombe(n11, n10, n01, n00, z):
    """(E10), Newcombe (1998) method 10, written from math.md (not imported)."""
    n11, n10, n01, n00 = (np.asarray(v, dtype=float) for v in (n11, n10, n01, n00))
    n = n11 + n10 + n01 + n00
    p1, p2 = (n11 + n10) / n, (n11 + n01) / n
    l1, u1 = wilson(n11 + n10, n, z)
    l2, u2 = wilson(n11 + n01, n, z)
    den = np.sqrt((n11 + n10) * (n01 + n00) * (n11 + n01) * (n10 + n00))
    with np.errstate(invalid="ignore", divide="ignore"):
        phi = np.where(den > 0, (n11 * n00 - n10 * n01) / np.where(den > 0, den, 1.0), 0.0)
    d = p1 - p2
    lo = d - np.sqrt(np.maximum((p1 - l1) ** 2 - 2 * phi * (p1 - l1) * (u2 - p2) + (u2 - p2) ** 2, 0))
    hi = d + np.sqrt(np.maximum((u1 - p1) ** 2 - 2 * phi * (u1 - p1) * (p2 - l2) + (p2 - l2) ** 2, 0))
    return d, lo, hi


def jeffreys(x, n, level=0.95):
    a = (1 - level) / 2
    lo = 0.0 if x == 0 else float(stats.beta.ppf(a, x + 0.5, n - x + 0.5))
    hi = 1.0 if x == n else float(stats.beta.ppf(1 - a, x + 0.5, n - x + 0.5))
    return lo, hi


def tables(n):
    """Every 2 x 2 table (n11, n10, n01, n00) with n players."""
    t = np.array([(i, j, k, n - i - j - k) for i in range(n + 1) for j in range(n + 1 - i)
                  for k in range(n + 1 - i - j)], dtype=float)
    return t.T


def exact_guard(n, base, flip, worse_is_down, margin, z=Z90):
    """Result 10(c): the exact probability that the guard holds (and that it lies wholly beyond
    its margin) at no true change, summing the multinomial law over every table."""
    p = np.array([base * (1 - flip), (1 - base) * flip, base * flip, (1 - base) * (1 - flip)])
    n11, n10, n01, n00 = tables(n)
    lp = (gammaln(n + 1) - gammaln(n11 + 1) - gammaln(n10 + 1) - gammaln(n01 + 1) - gammaln(n00 + 1)
          + n11 * np.log(p[0]) + n10 * np.log(p[1]) + n01 * np.log(p[2]) + n00 * np.log(p[3]))
    pr = np.exp(lp)
    _, lo, hi = newcombe(n11, n10, n01, n00, z)
    if worse_is_down:
        return float((pr * (lo >= -margin)).sum()), float((pr * (hi < -margin)).sum()), float(pr.sum())
    return float((pr * (hi <= margin)).sum()), float((pr * (lo > margin)).sum()), float(pr.sum())


def dbar(key: str, disp: str) -> float:
    """Result 2(e): the w^2 c-weighted design effect E[c d]/E[c] (EXP-0002 Result 5)."""
    _, _, m, _, _, _ = KEYS[key]
    return 1 + (D_EFF - 1) * m / (m - 1) if (disp == "de1.5" and m is not None) else 1.0


def approx(key, band, rho, disp, g, phi):
    """Result 5: the normal approximation of the challenger's coverage and of the two miss shares,
    at the expected counts, for a given dispersion factor phi."""
    _, cbar, _, _, n0, peer = KEYS[key]
    th = theta_of(key, band, rho)
    med = median_of(key, band)
    mu = med if peer else 0.5
    w = weights(g)
    neff = cbar * w.sum() ** 2 / (w**2).sum()
    nt = neff / phi
    s0 = min(n0, PRIOR_GAMES * nt / g) if peer else n0
    lam = s0 / (s0 + nt)
    mbar = lam * mu + (1 - lam) * th
    s = math.sqrt(mbar * (1 - mbar) / (s0 + nt))
    sig = (1 - lam) * math.sqrt(th * (1 - th) * dbar(key, disp) / neff)
    b = lam * (mu - th)
    hi = stats.norm.cdf((Z90 * s - b) / sig)
    lo = stats.norm.cdf((-Z90 * s - b) / sig)
    return hi - lo, lo, 1 - hi  # coverage, truth above the range, truth below it


# ---- 1. symbolic ------------------------------------------------------------------------------


def symbolic() -> None:
    th, mu, g, nt, s, lam, Db, phi, z, nn = sp.symbols(
        "theta mu g ntilde s lambda Dbar phi z N", positive=True)
    w1, w2, w3, c1, c2, c3, e1, e2, e3, p = sp.symbols("w1 w2 w3 c1 c2 c3 e1 e2 e3 p", positive=True)
    W, C, E = [w1, w2, w3], [c1, c2, c3], [e1, e2, e3]

    # Result 1: Kish. Var(sum w e | c) = sum w^2 c th(1-th) for independent binomials.
    nw = sum(wi * ci for wi, ci in zip(W, C, strict=True))
    s2 = sum(wi**2 * ci for wi, ci in zip(W, C, strict=True))
    var_p = sum(wi**2 * ci * th * (1 - th) for wi, ci in zip(W, C, strict=True)) / nw**2
    check("R1(a) Var(p_hat | c) = th(1-th)/n_eff with n_eff = n^2/S2", zero(var_p - th * (1 - th) / (nw**2 / s2)))
    check("R1(b) n_eff - n = n(n - S2)/S2 and n - S2 = sum w(1-w)c",
          zero(nw**2 / s2 - nw - nw * (nw - s2) / s2)
          and zero(nw - s2 - sum(wi * (1 - wi) * ci for wi, ci in zip(W, C, strict=True))))
    lagrange = sum(C) * s2 - nw**2 - sum(C[i] * C[j] * (W[i] - W[j]) ** 2 for i in range(3) for j in range(i + 1, 3))
    check("R1(b) (sum c)(sum w^2 c) - (sum w c)^2 = sum_{s<t} c_s c_t (w_s - w_t)^2 (Lagrange)", zero(sp.expand(lagrange)))
    rng = np.random.default_rng(1)
    wg = rng.random((20000, 30))
    cg = rng.poisson(2.0, (20000, 30)).astype(float) + 1.0
    ne = (wg * cg).sum(1) ** 2 / (wg**2 * cg).sum(1)
    check("R1(b) n <= n_eff <= sum c on 20,000 random weight and chance vectors (grid)",
          bool(np.all(ne >= (wg * cg).sum(1) * (1 - 1e-12)) and np.all(ne <= cg.sum(1) * (1 + 1e-12))))
    neg = [weights(gg).sum() ** 2 / (weights(gg) ** 2).sum() for gg in CHECKPOINTS]
    check("R1(c) spread n_eff for one chance a game: 29.9, 59.4, 115.4, 214.0",
          all(near(qv, v, 1) for qv, v in zip((29.9, 59.4, 115.4, 214.0), neg, strict=True)),
          ", ".join(f"{v:.2f}" for v in neg))
    sws = [weights(gg).sum() for gg in CHECKPOINTS]
    check("R1(c) sum w = 27.6, 50.8, 86.8, 132.6 (EXP-0002 Result 4)",
          all(near(qv, v, 1) for qv, v in zip((27.6, 50.8, 86.8, 132.6), sws, strict=True)))
    rgs = [(weights(gg) ** 2).sum() / weights(gg).sum() for gg in CHECKPOINTS]
    check("R4 n_eff = n / r_g with r_g = S2/n, so the weight concentration cancels",
          zero(nw / (s2 / nw) - nw**2 / s2)
          and all(abs(sw / rg - v) < 1e-9 for sw, rg, v in zip(sws, rgs, neg, strict=True)))

    # Result 2: the Pearson statistic. (a) decomposition around ebar = sum e / sum c, at any p.
    Nc = sum(C)
    eb = sum(E) / Nc
    lhs = sum((ei - ci * p) ** 2 / ci for ei, ci in zip(E, C, strict=True))
    rhs = sum((ei - ci * eb) ** 2 / ci for ei, ci in zip(E, C, strict=True)) + Nc * (eb - p) ** 2
    check("R2(a) sum (e - c p)^2/c = sum (e - c ebar)^2/c + N_c (ebar - p)^2", zero(sp.together(lhs - rhs)))
    # (b) E[(e - c th)^2]/(c th(1-th)) = d for the Beta-binomial game
    kap, cc = sp.symbols("kappa c", positive=True)
    var_tg = th * (1 - th) / (kap + 1)
    var_e = cc * (th - (var_tg + th**2)) + cc**2 * var_tg
    check("R2(b) E[(e - c th)^2]/(c th(1-th)) = 1 + (c-1)/(kappa+1)",
          zero(var_e / (cc * th * (1 - th)) - (1 + (cc - 1) / (kap + 1))))
    # (c) one chance a game, binary e
    ebs = sp.symbols("ebar", positive=True)
    check("R2(c) ebar - 2 p ebar + p^2 = p(1-p) + (ebar - p)(1 - 2p)",
          zero(ebs - 2 * p * ebs + p**2 - (p * (1 - p) + (ebs - p) * (1 - 2 * p))))
    ok = True
    for bits in itertools.product((0, 1), repeat=6):
        e_ = np.array(bits, dtype=float)
        for pv in (0.07, 0.3, 0.55):
            direct = ((e_ - pv) ** 2).sum() / (pv * (1 - pv)) / 5
            formula = 6 / 5 * (1 + (e_.mean() - pv) * (1 - 2 * pv) / (pv * (1 - pv)))
            ok &= abs(direct - formula) < 1e-12
    check("R2(c) phi_raw = G/(G-1) [1 + (ebar-p)(1-2p)/(p(1-p))] on all 64 binary series of 6, 3 rates", ok)
    # (d) x f_nu(x) = nu f_{nu+2}(x) for the chi-square density, hence the clamp's mean
    x, nu = sp.symbols("x nu", positive=True)

    def f(v):
        return x ** (v / 2 - 1) * sp.exp(-x / 2) / (2 ** (v / 2) * sp.gamma(v / 2))

    check("R2(d) x f_nu(x) = nu f_{nu+2}(x)", zero(sp.gammasimp(x * f(nu) / f(nu + 2)) - nu))
    for v in (9, 29):
        closed = 1 + stats.chi2.sf(v, v + 2) - stats.chi2.sf(v, v)
        num = 1 + integrate.quad(lambda t, v=v: (t / v - 1) * stats.chi2.pdf(t, v), v, np.inf)[0]
        check(f"R2(d) E[max(1, X/nu)] = 1 + P(chi2_(nu+2) > nu) - P(chi2_nu > nu) at nu = {v} (quadrature)",
              abs(closed - num) < 1e-8, f"{closed:.4f}")
    # (e) the game-mean of d for Poisson(m) chances, against the w^2 c-weighted Dbar
    for key, mval in (("W1", 53.0), ("W9b", 31.8)):
        gam1 = (mval - 1) / (D_EFF - 1)
        dgame = 1 + (mval / (1 - math.exp(-mval)) - 1) / gam1
        check(f"R2(e) {key}: mean d over games with a chance is 1.500 < Dbar = {dbar(key, 'de1.5'):.4f}",
              abs(dgame - 1.5) < 1e-6 and dbar(key, "de1.5") > dgame, f"{dgame:.6f}")

    # Result 3: the cap. lambda'(s) = s/(s + nt) rises in s and is 2/(g+2) at s = 2 nt/g.
    lam_s = s / (s + nt)
    check("R3 d lambda'/ds = nt/(s+nt)^2 > 0", zero(sp.diff(lam_s, s) - nt / (s + nt) ** 2))
    check("R3 lambda' at the cap s = 2 nt/g is 2/(g+2)", zero(lam_s.subs(s, 2 * nt / g) - 2 / (g + 2)))
    ph = sp.symbols("phat", positive=True)
    m_ = (s * mu + ph * nt) / (s + nt)
    check("R3 mean = lambda' mu + (1 - lambda') p_hat (k~ = p_hat nt)", zero(m_ - (lam_s * mu + (1 - lam_s) * ph)))
    check("R3 E[m | c]/theta = 1 - lambda'(1 - mu/theta)", zero(m_.subs(ph, th) / th - (1 - lam_s * (1 - mu / th))))
    vals = [2 / (gg + 2) for gg in CHECKPOINTS]
    check("R3 2/(g+2) = 0.0625, 0.0323, 0.0164, 0.0079",
          all(near(qv, v, 4) for qv, v in zip((0.0625, 0.0323, 0.0164, 0.0079), vals, strict=True)))
    t3 = 1 - (2 / 32) * (1 - 0.3 / odds_scale(0.3, 4.0))
    check("R3 B2 W4.missed rho 4 at 30 games: E[m]/theta = 0.967 (champion 0.69, EXP-0002)", near(0.967, t3, 3), f"{t3:.4f}")

    # Result 4: sampling variance over posterior variance, s0 + nt = nt/(1 - lambda'), nt = n_eff/phi
    neff = sp.symbols("n_eff", positive=True)
    s_post2 = th * (1 - th) / ((neff / phi) / (1 - lam))
    sig2 = (1 - lam) ** 2 * th * (1 - th) * Db / neff
    check("R4 Var(m | c) / posterior variance = (1 - lambda') Dbar / phi", zero(sig2 / s_post2 - (1 - lam) * Db / phi))
    a_, b_ = sp.symbols("a b", positive=True)
    check("R4 Beta variance = m(1-m)/(a+b+1)",
          zero(a_ * b_ / ((a_ + b_) ** 2 * (a_ + b_ + 1)) - (a_ / (a_ + b_)) * (1 - a_ / (a_ + b_)) / (a_ + b_ + 1)))
    check("R4 s0 + nt = nt/(1 - lambda') at lambda' = s0/(s0 + nt)", zero((s + nt) - nt / (1 - lam_s)))

    # Result 5: at b = 0 the coverage 2 Phi(z/sqrt(v)) - 1 falls in v and is 0.8 at v = 1
    v = sp.symbols("v", positive=True)
    cov = sp.erf(sp.Float(Z90) / sp.sqrt(v) / sp.sqrt(2))
    check("R5 d coverage/dv < 0 and coverage(v = 1) = 0.8",
          bool(sp.simplify(sp.diff(cov, v)).is_negative) and abs(float(cov.subs(v, 1)) - 0.8) < 1e-12)

    # Result 7: |x - 0.8| is convex (a grid of midpoints), so is a max of convex functions
    xs = np.linspace(0, 1, 2001)
    hx = np.abs(xs - 0.8)
    check("R7 |x - 0.8| midpoint-convex on a grid of 2001", bool(np.all(hx[1:-1] <= (hx[:-2] + hx[2:]) / 2 + 1e-15)))

    # Result 8: one-sided levels of the two-sided intervals
    check("R8 the 97.5% interval's upper end is the 0.9875 quantile, the 80% interval's the 0.9",
          near(0.9875, 0.5 + 0.975 / 2, 6) and near(0.9, 0.5 + 0.8 / 2, 6))

    # Result 10(a): with n10 = n01 = 0 the interval is +-|(p - l) - (u - p)| = +-2 z^2 |1/2 - p|/(N + z^2)
    den = 1 + z**2 / nn
    ctr = (p + z**2 / (2 * nn)) / den
    check("R10(a) Wilson centre - p = z^2 (1/2 - p)/(N + z^2)", zero(ctr - p - z**2 * (sp.Rational(1, 2) - p) / (nn + z**2)))
    A, B = sp.symbols("A B", positive=True)
    check("R10(a) with phi = 1, A^2 - 2AB + B^2 = (A - B)^2", zero(A**2 - 2 * A * B + B**2 - (A - B) ** 2))
    hw = 2 * Z90**2 * abs(0.5 - 28 / 45) / (45 + Z90**2)
    _, lo, hi = newcombe(28, 0, 0, 17, Z90)
    check("R10(a) 28 of 45 on both sides: half-width 0.008607, closed form = (E10)",
          abs(hw - 0.008607) < 5e-7 and abs(float(hi) - hw) < 1e-12 and abs(float(lo) + hw) < 1e-12, f"{hw:.6f}")

    # Result 11: pooling two halves; the Bonferroni quantiles
    n1, n2, p1, p2 = sp.symbols("n1 n2 p1 p2", positive=True)
    pbar = (n1 * p1 + n2 * p2) / (n1 + n2)
    gap = (n1 + n2) * pbar * (1 - pbar) - (n1 * p1 * (1 - p1) + n2 * p2 * (1 - p2))
    check("R11 N pbar(1-pbar) - sum n_i p_i(1-p_i) = n1 n2 (p1-p2)^2/N", zero(gap - n1 * n2 * (p1 - p2) ** 2 / (n1 + n2)))
    check("R11 z for 4 is 2.4977, z for 20 is 3.0233", near(2.4977, Z4, 4) and near(3.0233, Z20, 4))
    hw13 = Z4 * math.sqrt(0.16 / 1600)
    check("R11 #13 half-width at 0.80 and 1,600 players about 0.025", near(0.025, hw13, 3), f"{hw13:.4f}")

    # Result 12: Jeffreys is the posterior of Beta(1/2, 1/2) and the binomial kernel
    t_ = sp.symbols("t", positive=True)
    xk, nk = sp.symbols("x n", positive=True)
    kern = t_ ** (sp.Rational(-1, 2)) * (1 - t_) ** (sp.Rational(-1, 2)) * t_**xk * (1 - t_) ** (nk - xk)
    post = t_ ** (xk + sp.Rational(1, 2) - 1) * (1 - t_) ** (nk - xk + sp.Rational(1, 2) - 1)
    check("R12 Beta(1/2,1/2) prior x binomial kernel = Beta(x+1/2, n-x+1/2) kernel",
          zero(sp.powsimp(kern / post, force=True) - 1))
    up = stats.beta.ppf(0.975, 0.5, 28.5)
    check("R12 x = 0 of 28: [0, 0.085]", near(0.085, up, 3), f"{up:.6f}")


# ---- 2. simulation ----------------------------------------------------------------------------


def simulation(data, ps) -> None:
    rng = np.random.default_rng(20261128)
    reps = 20_000

    # Result 1: Var(p_hat | c) = th(1-th) Dbar_c / n_eff for a fixed chance vector (W1-like).
    g = 30
    w = weights(g)
    c = rng.poisson(53.0, g)
    th = 0.02
    neff = (w * c).sum() ** 2 / (w**2 * c).sum()
    for disp in DISPS:
        e = draw_events(rng, np.tile(c, reps), th, 53.0, disp).reshape(reps, g)
        ph = (e * w).sum(1) / (w * c).sum()
        d_t = 1 + (c - 1) / ((53.0 - 1) / (D_EFF - 1)) if disp == "de1.5" else np.ones(g)
        want = th * (1 - th) * (w**2 * c * d_t).sum() / (w**2 * c).sum() / neff
        ratio = ph.var(ddof=1) / want
        check(f"R1 simulated Var(p_hat) = th(1-th) Dbar_c/n_eff ({disp})",
              abs(ratio - 1) < 4 * math.sqrt(2 / (reps - 1)), f"ratio {ratio:.4f}")
        check(f"R1 simulated E[p_hat] = theta ({disp})",
              abs(ph.mean() - th) < 4 * ph.std(ddof=1) / math.sqrt(reps), f"{ph.mean():.5f}")
        # Result 2(b): at the known theta the Pearson terms average d_t
        terms = ((e - c * th) ** 2 / (c * th * (1 - th))).mean(1)
        se = terms.std(ddof=1) / math.sqrt(reps)
        check(f"R2(b) simulated mean Pearson term at the true rate = mean d_t ({disp})",
              abs(terms.mean() - d_t.mean()) < 4 * se, f"{terms.mean():.4f} against {d_t.mean():.4f}")

    # Result 2(d), (e) and the clamp: the replay's mean phi at rho = 1, every band and dispersion.
    rho1 = cells_of([1.0])
    mean_phi = {(k, g_): float(np.mean([data["challenger"][(k, g_, c_)]["phi"].mean() for c_ in rho1 if c_[2] == "none"]))
                for k in KEYS for g_ in CHECKPOINTS}
    chi = 1 + stats.chi2.sf(29, 31) - stats.chi2.sf(29, 29)
    check("R2(d) W1 at 30 games, no dispersion: replay E[phi] within 0.02 of the chi-square value 1.104 (approximate)",
          abs(mean_phi[("W1", 30)] - chi) < 0.02, f"{mean_phi[('W1', 30)]:.4f} against {chi:.4f}")
    check("R2(c) W7.traps at 30 games: replay E[phi] within 0.005 of 30/29 = 1.0345",
          abs(mean_phi[("W7.traps", 30)] - 30 / 29) < 0.005, f"{mean_phi[('W7.traps', 30)]:.4f}")
    at30 = {k: mean_phi[(k, 30)] for k in KEYS}
    order = sorted(at30, key=at30.get, reverse=True)
    check("R2 Finding 2: at 30 games the clamp widens W9b most and W7.traps least",
          order[0] == "W9b" and order[-1] == "W7.traps", ", ".join(f"{k} {at30[k]:.3f}" for k in order))
    check("R2 the mean phi falls from 30 to 120 games for W1, W4.missed, W9b",
          all(mean_phi[(k, 30)] > mean_phi[(k, 60)] > mean_phi[(k, 120)] for k in ("W1", "W4.missed", "W9b")))
    dphi = [float(np.mean([data["challenger"][(k, g_, c_)]["phi"].mean() for c_ in cells_of(PRIMARY) if c_[2] == "de1.5"]))
            for k in ("W1", "W9b") for g_ in CHECKPOINTS]
    check("R2(e) with de1.5, W1 and W9b mean phi within 0.06 of 1.5 at every checkpoint (primary cells)",
          all(abs(x - 1.5) < 0.06 for x in dphi), ", ".join(f"{x:.3f}" for x in dphi))

    # Result 3: the cap binds on every peer-key draw, and E[m] = lambda' mu + (1 - lambda') theta.
    worst, zmax, ncell, draws, free, sumc_ok = 0.0, 0.0, 0, 0, {}, True
    for (k, g_, c_), val in data["challenger"].items():
        if not KEYS[k][5]:
            continue
        worst = max(worst, float(np.max(val["lam"] - 2 / (g_ + 2))))
        unbound = np.abs(val["lam"] - 2 / (g_ + 2)) > 1e-12
        draws += val["lam"].size
        if unbound.any():
            free[(k, g_)] = free.get((k, g_), 0) + int(unbound.sum())
        sumc_ok &= bool(np.all(val["n"][unbound] * 0 + val["neff"][unbound] / val["phi"][unbound] > KEYS[k][4] * g_ / 2))
        band, rho, _ = c_
        th_ = theta_of(k, band, rho)
        mu_ = ISSUE13["mu"] if rho == "issue13" else median_of(k, band)
        lam_ = 2 / (g_ + 2)
        ncell += 1
        want = lam_ * mu_ + (1 - lam_) * th_
        zmax = max(zmax, abs(val["mean"].mean() - want) / (val["mean"].std(ddof=1) / math.sqrt(val["mean"].size)))
    check("R3 replay: lambda' <= 2/(g+2) on every peer-key draw, with equality unless n~ > n0 g/2",
          worst < 1e-12 and sumc_ok, f"{sum(free.values())} of {draws} draws unbound: {free}")
    check("R3 replay: cell means of m match lambda' mu + (1-lambda') theta (max |z| < 4.5)",
          zmax < 4.5, f"max |z| {zmax:.2f} over {ncell} key x checkpoint x cell means")

    # Result 5: the approximation against the replayed coverages and miss shares (challenger).
    prim = cells_of(PRIMARY)
    errs, errs_hi = [], []
    for cell in ps["challenger"]["cells"]:
        k, g_ = cell["key"], cell["games"]
        ap = [approx(k, *c_, g_, float(data["challenger"][(k, g_, c_)]["phi"].mean())) for c_ in prim]
        errs.append(abs(np.mean([a[0] for a in ap]) - cell["coverage"]))
        errs_hi.append(abs(np.mean([a[1] for a in ap]) - cell["true_rate_above"]))
    check("R5 approximation within 0.030 of all 20 primary coverages", max(errs) <= 0.0305, f"max {max(errs):.4f}")
    check("R5 approximation within 0.042 of all 20 high-side miss shares", max(errs_hi) <= 0.0425, f"max {max(errs_hi):.4f}")
    below = [c_["true_rate_below"] for c_ in ps["challenger"]["cells"] if c_["games"] == 30 and c_["key"] != "W5"]
    check("R5 at 30 games the truth lies below the challenger's range in 0.044 to 0.061", near(0.044, min(below), 3) and near(0.061, max(below), 3),
          ", ".join(f"{x:.3f}" for x in below))
    lam30 = {(v, k): float(np.mean([data[v][(k, 30, c_)]["lam"].mean() for c_ in prim])) for v in RULES for k in KEYS}
    check("R3 prior shares at 30 games: champion 0.17, 0.59, 0.59, 0.13; challenger 0.0625; W5 0.16 under both",
          all(near(qv, lam30[("champion", k)], 2) for qv, k in zip((0.17, 0.59, 0.59, 0.13), ("W1", "W4.missed", "W7.traps", "W9b"), strict=True))
          and all(abs(lam30[("challenger", k)] - 0.0625) < 1e-4 for k in ("W1", "W4.missed", "W7.traps", "W9b"))
          and near(0.16, lam30[("champion", "W5")], 2) and near(0.16, lam30[("challenger", "W5")], 2))
    lean = all(c_["true_rate_above"] > c_["true_rate_below"] for c_ in ps["challenger"]["cells"] if c_["key"] != "W5")
    check("R5 the lean: truth above the range more often than below in all 16 peer-key coverages", lean)

    # Result 6: credible implies the range lies wholly above the truth, at rho <= 1.
    ok = True
    for v in RULES:
        for (k, g_, c_), val in data[v].items():
            if c_[1] in (0.5, 1.0):
                ok &= bool(np.all(val["credible"] <= val["low_miss"]))
    check("R6 replay: every credible estimate at rho <= 1 has its range wholly above the truth (both rules)", ok)
    allkg6 = [(k, g_) for k in KEYS for g_ in CHECKPOINTS]
    lm = {v: float(stat(data[v], "low_miss", allkg6, cells_of([1.0]))) for v in RULES}
    lmk = {k: float(stat(data["champion"], "low_miss", [(k, g_) for g_ in CHECKPOINTS], cells_of([1.0]))) for k in ("W4.missed", "W7.traps")}
    check("R6 ranges wholly above the truth at rho = 1: champion 0.059 (W4.missed, W7.traps 0.028), challenger 0.086",
          near(0.059, lm["champion"], 3) and near(0.086, lm["challenger"], 3) and all(near(0.028, x, 3) for x in lmk.values()))

    # Result 7: the ship test on D. 20 coverages, one at 0.73 (D = 0.07 exactly), 19 at 0.80;
    # 3,600 one-interval players a coverage; percentile bootstrap of the max (binomial resampling,
    # exact for 0/1 players).
    rng7 = np.random.default_rng(7)
    sims, bb, npl = 2000, 1000, 3600
    truth = np.array([0.73] + [0.80] * 19)
    ships, dhat = 0, []
    for _ in range(sims):
        chat = rng7.binomial(npl, truth) / npl
        star = rng7.binomial(npl, np.tile(chat, (bb, 1))) / npl
        dstar = np.abs(star - 0.8).max(1)
        dhat.append(np.abs(chat - 0.8).max())
        ships += np.quantile(dstar, 0.9875) <= TOL
    rate = ships / sims
    check("R7 simulated false-ship rate at D = 0.07 is at most 0.0125 + 4 SE", rate <= 0.0125 + 4 * math.sqrt(0.0125 * 0.9875 / sims),
          f"{rate:.4f}")
    dhat = np.array(dhat)
    check("R7 simulated E[D_hat] >= D (upward bias of the max)", dhat.mean() >= 0.07 - 4 * dhat.std(ddof=1) / math.sqrt(sims),
          f"{dhat.mean():.5f}")
    truth2 = np.full(20, 0.80)
    dh2 = np.abs(rng7.binomial(npl, np.tile(truth2, (4000, 1))) / npl - 0.8).max(1)
    check("R7 at a true D of 0, E[D_hat] is about 2.2 SE above it (bias of the max of 20)",
          dh2.mean() > 4 * math.sqrt(0.16 / npl) / 2, f"E[D_hat] {dh2.mean():.4f}, SE {math.sqrt(0.16 / npl):.4f}")

    # Result 9: the paired stratified bootstrap variance of the false-credible difference is the
    # plug-in (1/S^2) sum var_s(diff)/n_s; the unpaired one is larger.
    low = cells_of([0.5, 1.0])
    allkg = [(k, g_) for k in KEYS for g_ in CHECKPOINTS]
    plug, unpaired = 0.0, 0.0
    for c_ in low:
        a = np.mean([data["challenger"][(k, g_, c_)]["credible"] for k, g_ in allkg], axis=0)
        b = np.mean([data["champion"][(k, g_, c_)]["credible"] for k, g_ in allkg], axis=0)
        plug += (a - b).var() / a.size
        unpaired += (a.var() + b.var()) / a.size
    plug, unpaired = plug / len(low) ** 2, unpaired / len(low) ** 2
    bs = float(ps["fc_boot"].std(ddof=1))
    check("R9 paired bootstrap SD of the false-credible difference = plug-in SD (within 4 MC SE)",
          abs(bs / math.sqrt(plug) - 1) < 4 / math.sqrt(2 * (B_BOOT - 1)), f"{bs:.6f} against {math.sqrt(plug):.6f}")
    check("R9 plug-in SD 0.000278, bootstrap 0.000290; pairing narrows it 2.37 times",
          near(0.000278, math.sqrt(plug), 6) and near(0.000290, bs, 6) and near(2.37, math.sqrt(unpaired / plug), 2),
          f"{math.sqrt(unpaired / plug):.3f}")

    # Result 10(c): power.json's Tier 1 simulations, reproduced from their seed, against the exact law.
    trng = np.random.default_rng(seed("tier1", "EXP-0006-power"))
    for n, base, down, key_h, key_b in ((45, 0.63, True, "recall_holds", "recall_beyond"),
                                        (23, 0.2, False, "clean_alarm_holds", "clean_alarm_beyond")):
        ref = trng.random((4000, n)) < base
        flip = trng.random((4000, n)) < 0.05
        cand = np.where(flip, ~ref, ref)
        t = [(ref & cand).sum(1), (~ref & cand).sum(1), (ref & ~cand).sum(1), (~ref & ~cand).sum(1)]
        _, lo, hi = newcombe(*t, Z90)
        hold = float((lo >= -0.10).mean() if down else (hi <= 0.10).mean())
        beyond = float((hi < -0.10).mean() if down else (lo > 0.10).mean())
        ex_h, ex_b, tot = exact_guard(n, base, 0.05, down, 0.10)
        check(f"R10(c) power.json {key_h} {P['tier1_no_change'][key_h]} reproduced from its seed",
              round(hold, 4) == P["tier1_no_change"][key_h] and round(beyond, 4) == P["tier1_no_change"][key_b])
        se = math.sqrt(ex_h * (1 - ex_h) / 4000)
        check(f"R10(c) exact law {ex_h:.4f} agrees with the simulation within 4 MC SE ({n} players)",
              abs(ex_h - hold) < 4 * se and abs(tot - 1) < 1e-9, f"beyond exactly {ex_b:.5f}")

    # Result 11: Wilson with Bonferroni z for 4 over two pooled halves of 800 covers at least 1 - 0.05/4.
    rng11 = np.random.default_rng(11)
    x = rng11.binomial(800, 0.74, 40_000) + rng11.binomial(800, 0.79, 40_000)
    lo, hi = wilson(x, 1600, Z4)
    cov = float(np.mean((lo <= 0.765) & (0.765 <= hi)))
    check("R11 pooled-halves Wilson coverage >= 0.9875 - 4 SE (conservative)",
          cov >= 0.9875 - 4 * math.sqrt(0.9875 * 0.0125 / 40_000), f"{cov:.4f}")

    # Result 12: the exact coverage of the Jeffreys 95% interval at n = 28, over p in [0.01, 0.5].
    grid = np.linspace(0.01, 0.5, 491)
    xs = np.arange(29)
    ivs = np.array([jeffreys(int(x_), 28) for x_ in xs])
    covs = [float((stats.binom.pmf(xs, 28, p_) * ((ivs[:, 0] <= p_) & (p_ <= ivs[:, 1]))).sum()) for p_ in grid]
    check("R12 Jeffreys at n = 28: mean exact coverage over p in [0.01, 0.5] within 0.01 of 0.95",
          abs(np.mean(covs) - 0.95) < 0.01, f"mean {np.mean(covs):.4f}, min {min(covs):.4f}")


# ---- 3. numbers -------------------------------------------------------------------------------


def classify(lo, hi):
    if BAND[0] <= lo and hi <= BAND[1]:
        return "within the band"
    if hi < BAND[0] or lo > BAND[1]:
        return "outside the band"
    return "need more data"


def same(a, b, tol=1.5e-6) -> bool:
    """A recomputed value against its committed six-decimal value."""
    if isinstance(b, (list, tuple)):
        return len(a) == len(b) and all(same(x, y, tol) for x, y in zip(a, b, strict=True))
    return abs(float(a) - float(b)) <= tol


def decide(ps, recall_lo, recall_hi, smoke_failures):
    """(E11): `analysis.py` `decide`, rewritten from preregistration section 9."""
    p_lo, p_hi = q(ps["challenger"]["dboot"], 0.975)
    d_lo = q(ps["delta_boot"], 0.975)[0]
    r_lo, r_hi = q(ps["rho1_boot"], 0.8)
    f_lo, f_hi = q(ps["fc_boot"], 0.8)
    holds = {"rho1_deviation": r_hi <= 0.05, "false_credible": f_hi <= 0.01,
             "recall_at_2": recall_lo >= -0.10, "smoke_ci_rule": not smoke_failures}
    beyond = {"rho1_deviation": r_lo > 0.05, "false_credible": f_lo > 0.01,
              "recall_at_2": recall_hi < -0.10, "smoke_ci_rule": False}
    if p_hi <= TOL and d_lo > 0 and all(holds.values()):
        return "ship", holds, beyond
    if p_lo > TOL or any(beyond.values()):
        return "reject", holds, beyond
    return "need more data", holds, beyond


def tier1_table(players, g, field, pred):
    t = {"n11": 0, "n10": 0, "n01": 0, "n00": 0}
    for p_ in players:
        if not pred(p_):
            continue
        a = next((r for r in p_["reference"] if r["games"] == g), None)
        b = next((r for r in p_["candidate"] if r["games"] == g), None)
        if a is None or b is None:
            continue
        t[{(True, True): "n11", (False, True): "n10", (True, False): "n01", (False, False): "n00"}[(bool(a[field]), bool(b[field]))]] += 1
    return t


def numbers(data, ps) -> None:
    rp = R["per_rule"]
    # N1: every committed cell of both rules, from the replay and the paired bootstrap.
    for v in RULES:
        bad = 0
        for mine, theirs in zip(ps[v]["cells"], rp[v]["cells"], strict=True):
            bad += not (mine["key"] == theirs["key"] and mine["games"] == theirs["games"]
                        and same(mine["coverage"], theirs["coverage"]) and same(mine["ci95"], theirs["ci95"])
                        and same(mine["ci_bonferroni"], theirs["ci_bonferroni"])
                        and classify(*mine["ci_bonferroni"]) == theirs["classification"]
                        and same(mine["true_rate_above"], theirs["true_rate_above"])
                        and same(mine["median_width_ratio"], theirs["median_width_ratio"]))
        within = sum(classify(*c_["ci_bonferroni"]) == "within the band" for c_ in ps[v]["cells"])
        outside = sum(classify(*c_["ci_bonferroni"]) == "outside the band" for c_ in ps[v]["cells"])
        ok = (bad == 0 and same(ps[v]["D"], rp[v]["D"]) and same(q(ps[v]["dboot"], 0.95), rp[v]["D_ci95"])
              and same(q(ps[v]["dboot"], 0.975), rp[v]["D_ci97_5"]) and within == rp[v]["within"]
              and outside == rp[v]["outside"] and ps[v]["worst"]["key"] == rp[v]["worst"]["key"]
              and ps[v]["worst"]["games"] == rp[v]["worst"]["games"])
        check(f"N1 {v}: 20 cells (coverage, 95% and Bonferroni intervals, class, high miss, width), D and its intervals",
              ok, f"{bad} cell mismatches; {within} within, {outside} outside")
    pr, dd = R["primary"], R["delta_D"]
    ch = ps["challenger"]
    check("N2 primary D(challenger) 0.028333, 80/95/97.5% intervals",
          same(ch["D"], pr["D"]) and same(q(ch["dboot"], 0.8), pr["ci80"]) and same(q(ch["dboot"], 0.95), pr["ci95"])
          and same(q(ch["dboot"], 0.975), pr["ci97_5"]))
    check("N2 Delta D 0.668333 and its 80/95/97.5% intervals",
          same(ps["champion"]["D"] - ch["D"], dd["delta_D"]) and same(q(ps["delta_boot"], 0.8), dd["ci80"])
          and same(q(ps["delta_boot"], 0.95), dd["ci95"]) and same(q(ps["delta_boot"], 0.975), dd["ci97_5"]))
    gr = R["guards"]
    r1 = gr["rho1_deviation"]
    g1 = ps["rho1"]
    rho1 = cells_of([1.0])
    low = cells_of([0.5, 1.0])
    pk_ok = all(same(stat(data[v], "covered", [(k, g_) for g_ in CHECKPOINTS], rho1), r1["per_key"][k][v])
                for k in KEYS for v in RULES)
    check("N3 rho = 1 guard: 0.874292, 0.804427, worse by -0.069865 [-0.071074, -0.06874], per key",
          same(g1["champion"][0], r1["champion"]) and same(g1["challenger"][0], r1["challenger"])
          and same(abs(g1["challenger"][0] - 0.8) - abs(g1["champion"][0] - 0.8), r1["worse_by"])
          and same(q(ps["rho1_boot"], 0.8), r1["ci80"]) and pk_ok)
    fcg = gr["false_credible"]
    fc = ps["fc"]
    pk_ok = all(same(stat(data[v], "credible", [(k, g_) for g_ in CHECKPOINTS], low), fcg["per_key"][k][v])
                for k in KEYS for v in RULES)
    check("N3 false-credible guard: 0.02488, 0.033792, +0.008911 [0.008552, 0.009287], per key",
          same(fc["champion"][0], fcg["champion"]) and same(fc["challenger"][0], fcg["challenger"])
          and same(fc["challenger"][0] - fc["champion"][0], fcg["worse_by"]) and same(q(ps["fc_boot"], 0.8), fcg["ci80"])
          and pk_ok)
    allkg = [(k, g_) for k in KEYS for g_ in CHECKPOINTS]
    half = R["exploratory"]["rho_half_coverage"]
    check("N3 rho = 0.5 coverage 0.783396 and 0.800031",
          all(same(stat(data[v], "covered", allkg, cells_of([0.5])), half[v]) for v in RULES))
    ok = True
    for row in R["exploratory"]["issue13"]:
        for v in RULES:
            arr = np.concatenate([data[v][("W1", row["games"], c_)]["covered"] for c_ in cells_of(["issue13"])])
            lo, hi = wilson(arr.sum(), arr.size, Z4)
            ok &= same(arr.mean(), row[v]["coverage"]) and same([lo, hi], row[v]["ci_bonferroni_wilson"]) and arr.size == row[v]["players"] == 1600
    check("N3 #13 cell: 8 coverages and Wilson intervals (Bonferroni z for 4), 1,600 players each", ok)

    # N6: Tier 1, from the committed player rows.
    pl = R["tier1"]["players"]

    def flawed(p_):
        return p_["flaw"] is not None and p_["suite"] in ("pilot", "smoke")

    def clean(p_):
        return p_["flaw"] is None

    rec = tier1_table(pl, 80, "hit2", flawed)
    d, lo, hi = newcombe(rec["n11"], rec["n10"], rec["n01"], rec["n00"], Z90)
    rg = gr["recall_at_2"]
    check("N6 recall at 2: 28 of 45 both sides, 0 [-0.008607, 0.008607]",
          rec == rg["table"] and sum(rec.values()) == 45 and rec["n11"] + rec["n01"] == 28
          and same(d, rg["difference"]) and same([lo, hi], rg["ci80_difference"]))
    for p_ in pl:
        for side in ("reference", "candidate"):
            for c_ in p_[side]:
                c_["alarm"] = c_["rank1"] is not None
    cl = tier1_table(pl, 80, "alarm", clean)
    d, clo, chi = newcombe(cl["n11"], cl["n10"], cl["n01"], cl["n00"], Z90)
    ca = R["reported"]["clean_alarm"]
    check("N6 clean alarm: 6 against 5 of 23, 0.043 [-0.017, 0.109]",
          cl == ca["table"] and sum(cl.values()) == 23 and cl["n11"] + cl["n10"] == 6 and cl["n11"] + cl["n01"] == 5
          and same(d, ca["difference"]) and same([clo, chi], ca["ci80_difference"])
          and near(0.043, d, 3) and near(-0.017, clo, 3) and near(0.109, chi, 3))
    ok = True
    for row in R["exploratory"]["churn"]["by_checkpoint"]:
        t = tier1_table(pl, row["games"], "hit2", flawed)
        hits, misses = t["n11"] + t["n01"], t["n10"] + t["n00"]
        ok &= (t == row["table"] and hits == row["reference_hits"] and t["n01"] == row["lost"] and t["n10"] == row["gained"]
               and same(jeffreys(t["n01"], hits), row["churn_ci95_jeffreys"]) and same(jeffreys(t["n10"], misses), row["gain_ci95_jeffreys"]))
    by = {r_["games"]: r_ for r_ in R["exploratory"]["churn"]["by_checkpoint"]}
    check("N6 churn by checkpoint with Jeffreys intervals; 28 hits, 0 [0, 0.085] at 80; one lost of 22 and of 30 at 45 and 60",
          ok and by[80]["reference_hits"] == 28 and by[80]["lost"] == 0 and near(0.085, by[80]["churn_ci95_jeffreys"][1], 3)
          and (by[45]["lost"], by[45]["reference_hits"], by[60]["lost"], by[60]["reference_hits"]) == (1, 22, 1, 30)
          and all(by[g_]["lost"] == by[g_]["gained"] == 0 for g_ in (30, 80, 120)) and by[45]["gained"] == by[60]["gained"] == 0)
    pf = R["exploratory"]["churn"]["per_flaw_at_80"]
    check("N6 the 17 misses at 80: miss_mate 9 of 9, opening_blunder 7 of 9, no_convert 1 of 9, under both rules",
          (pf["miss_mate"]["n00"], pf["opening_blunder"]["n00"], pf["no_convert"]["n00"]) == (9, 7, 1)
          and sum(f_["n00"] for f_ in pf.values()) == 17 and all(f_["n01"] == f_["n10"] == 0 for f_ in pf.values())
          and all(sum(f_.values()) == 9 for f_ in pf.values()))
    rc = {(r_["suite"], r_["games"]): r_ for r_ in R["exploratory"]["churn"]["rank_churn"]}
    mine = {}
    for s_ in ("pilot", "smoke", "clean"):
        for g_ in (30, 80):
            pairs = [(next(r_ for r_ in p_["reference"] if r_["games"] == g_), next(r_ for r_ in p_["candidate"] if r_["games"] == g_))
                     for p_ in pl if p_["suite"] == s_]
            mine[(s_, g_)] = (len(pairs), sum(a["rank1"] != b["rank1"] for a, b in pairs))
    check("N6 rank 1 changes: 5 of 32, 3 of 16, 4 of 20 at 30 games; 1, 0, 2 at 80",
          [mine[(s_, 30)] for s_ in ("pilot", "smoke", "clean")] == [(32, 5), (16, 3), (20, 4)]
          and [mine[(s_, 80)][1] for s_ in ("pilot", "smoke", "clean")] == [1, 0, 2]
          and all(rc[k_]["rank1_changed"] == mine[k_][1] for k_ in mine))
    sc = R["tier1"]["scorecards"]
    sm, smv = sc["smoke"]["scorecard"], sc["smoke"]["variant_scorecard"]
    fails = []
    for name, tol in (("recall_at_2", 0.1), ("top1_precision", 0.2), ("plan_hit_rate", 0.2), ("top2_agreement", 0.25)):
        if smv[name] < sm[name] - tol - 1e-9:
            fails.append(name)
    if abs(smv["coverage"] - 0.8) > abs(sm["coverage"] - 0.8) + 0.05 + 1e-9:
        fails.append("coverage")
    if smv["clean_top1_false_alarms"] > sm["clean_top1_false_alarms"] or (sm["deterministic"] and not smv["deterministic"]):
        fails.append("clean or determinism")
    check("N6 smoke CI rule: no regression; coverage 0.65 to 0.80, recall 0.6, top-1 0.643, plan 0.6, top-2 0.927 both",
          not fails and not gr["smoke_ci_rule"]["failures"] and (sm["coverage"], smv["coverage"]) == (0.65, 0.8)
          and sm["recall_at_2"] == smv["recall_at_2"] == 0.6 and near(0.643, sm["top1_precision"], 3)
          and sm["top1_precision"] == smv["top1_precision"] and sm["plan_hit_rate"] == smv["plan_hit_rate"] == 0.6
          and near(0.927, sm["top2_agreement"], 3) and sm["top2_agreement"] == smv["top2_agreement"]
          and sm["clean_top1_false_alarms"] == smv["clean_top1_false_alarms"] == 0)
    pil = sc["pilot"]
    check("N6 pilot coverage 0.58 to 0.78; one new clean false alarm in the pilot (0 to 1)",
          (pil["scorecard"]["coverage"], pil["variant_scorecard"]["coverage"]) == (0.58, 0.78)
          and (pil["scorecard"]["clean_top1_false_alarms"], pil["variant_scorecard"]["clean_top1_false_alarms"]) == (0, 1))
    pc = R["tier1"]["positive_control_moved_estimates"]
    check("N6 positive control: 4,245 moved intervals; 800 pilot fields, 0 mismatches",
          sum(pc.values()) == 4245 and R["tier1"]["between_runs"] == {"mismatches": 0, "pilot_fields_compared": 800})

    # N4: the decision rule, re-applied to the recomputed statistics.
    dec, holds, beyond = decide(ps, float(lo_rec := newcombe(28, 0, 0, 17, Z90)[1]), float(newcombe(28, 0, 0, 17, Z90)[2]), fails)
    check("N4 the section 9 rule gives ship; every guard holds, none wholly beyond",
          dec == "ship" == R["decision"] and holds == gr["holds"] and beyond == gr["wholly_beyond"], f"recall lo {lo_rec:.6f}")
    fx = R["fixed"]
    w1 = [c_ for c_ in ch["cells"] if c_["key"] == "W1"]
    i13 = all(BAND[0] <= wilson(np.concatenate([data["challenger"][("W1", g_, c_)]["covered"] for c_ in cells_of(["issue13"])]).sum(), 1600, Z4)[0]
              and wilson(np.concatenate([data["challenger"][("W1", g_, c_)]["covered"] for c_ in cells_of(["issue13"])]).sum(), 1600, Z4)[1] <= BAND[1]
              for g_ in CHECKPOINTS)
    check("N4 #13: W1's four within, the #13 cell not within at 30 games, all 20 within, so not fixed",
          all(classify(*c_["ci_bonferroni"]) == "within the band" for c_ in w1) == fx["w1_within_band"] is True
          and i13 == fx["issue13_cell_within_band"] is False and fx["all_20_within_band"] is True
          and fx["bench_hang_rank1_at_80"] == "W1" and fx["fixes_13"] is False)

    # N7: the numbers decision.md and summary.md quote, at their rounding.
    cells = {(v, c_["key"], c_["games"]): c_["coverage"] for v in RULES for c_ in ps[v]["cells"]}
    table = {
        "W1": ((0.645, 0.721, 0.764, 0.814), (0.786, 0.804, 0.793, 0.794)),
        "W4.missed": ((0.103, 0.161, 0.209, 0.273), (0.812, 0.808, 0.824, 0.817)),
        "W7.traps": ((0.191, 0.276, 0.357, 0.419), (0.791, 0.779, 0.786, 0.808)),
        "W9b": ((0.697, 0.755, 0.784, 0.842), (0.783, 0.795, 0.798, 0.810)),
        "W5": ((0.832, 0.837, 0.869, 0.899), (0.828, 0.811, 0.815, 0.805)),
    }
    ok = all(near(qv, cells[(v, k, g_)], 3) for k, rows in table.items() for v, row in zip(RULES, rows, strict=True)
             for qv, g_ in zip(row, CHECKPOINTS, strict=True))
    check("N7 the decision's table of 40 coverages at 3 dp", ok)
    check("N7 worst challenger 0.828 (W5 at 30), D 0.028 [0.022, 0.043]; champion 0.103 (W4.missed at 30), D 0.697; Delta 0.668 [0.650, 0.679]",
          ch["worst"]["key"] == "W5" and ch["worst"]["games"] == 30 and near(0.828, ch["worst"]["coverage"], 3)
          and near(0.028, ch["D"], 3) and near(0.022, pr["ci97_5"][0], 3) and near(0.043, pr["ci97_5"][1], 3)
          and ps["champion"]["worst"]["key"] == "W4.missed" and near(0.103, ps["champion"]["worst"]["coverage"], 3)
          and near(0.697, ps["champion"]["D"], 3) and near(0.668, dd["delta_D"], 3)
          and near(0.650, dd["ci97_5"][0], 3) and near(0.679, dd["ci97_5"][1], 3))
    check("N7 champion 2 within and 13 outside; challenger 20 of 20 within",
          (rp["champion"]["within"], rp["champion"]["outside"], rp["challenger"]["within"]) == (2, 13, 20))
    e2 = EXP2["primary"]["cells"]
    e2w4 = next(c_ for c_ in e2 if c_["key"] == "W4.missed" and c_["games"] == 30)
    check("N7 EXP-0002 on its seeds: 2 within, 12 outside; W4.missed at 30 games 0.10",
          sum(c_["classification"] == "within the band" for c_ in e2) == 2
          and sum(c_["classification"] == "outside the band" for c_ in e2) == 12 and near(0.10, e2w4["coverage"], 2))
    hi_ch = [cells_hm for cells_hm in (c_["true_rate_above"] for c_ in ch["cells"] if c_["games"] == 30 and c_["key"] != "W5")]
    hi_cm = [c_["true_rate_above"] for c_ in ps["champion"]["cells"] if c_["games"] == 30 and c_["key"] != "W5"]
    check("N7 truth above the range at 30 games: challenger 0.13 to 0.17; champion 0.33, 0.90, 0.81, 0.27",
          near(0.13, min(hi_ch), 2) and near(0.17, max(hi_ch), 2)
          and all(near(qv, v, 2) for qv, v in zip((0.33, 0.90, 0.81, 0.27), hi_cm, strict=True)),
          ", ".join(f"{x:.3f}" for x in hi_ch))
    check("N7 rho = 1: 0.874 and 0.804, distances 0.074 and 0.004; worse by -0.070 [-0.071, -0.069]",
          near(0.874, r1["champion"], 3) and near(0.804, r1["challenger"], 3) and near(0.074, abs(r1["champion"] - 0.8), 3)
          and near(0.004, abs(r1["challenger"] - 0.8), 3) and near(-0.070, r1["worse_by"], 3)
          and near(-0.071, r1["ci80"][0], 3) and near(-0.069, r1["ci80"][1], 3))
    pkr = r1["per_key"]
    check("N7 per key at rho = 1: champion W4.missed 0.942, W7.traps 0.931; challenger 0.787 to 0.827",
          near(0.942, pkr["W4.missed"]["champion"], 3) and near(0.931, pkr["W7.traps"]["champion"], 3)
          and near(0.787, min(x["challenger"] for x in pkr.values()), 3) and near(0.827, max(x["challenger"] for x in pkr.values()), 3))
    pkf = fcg["per_key"]
    check("N7 false credible 0.0249 to 0.0338, +0.0089 [+0.0086, +0.0093], 0.0007 to spare, one in about 110",
          near(0.0249, fcg["champion"], 4) and near(0.0338, fcg["challenger"], 4) and near(0.0089, fcg["worse_by"], 4)
          and near(0.0086, fcg["ci80"][0], 4) and near(0.0093, fcg["ci80"][1], 4) and near(0.0007, 0.01 - fcg["ci80"][1], 4)
          and abs(1 / fcg["worse_by"] - 110) < 5, f"1/{1 / fcg['worse_by']:.1f}")
    check("N7 per key: W4.missed 0.006 to 0.029, W7.traps 0.001 to 0.017 (one in 59 against one in 740); W1 and W9b fall",
          near(0.006, pkf["W4.missed"]["champion"], 3) and near(0.029, pkf["W4.missed"]["challenger"], 3)
          and near(0.001, pkf["W7.traps"]["champion"], 3) and near(0.017, pkf["W7.traps"]["challenger"], 3)
          and round(1 / pkf["W7.traps"]["challenger"]) == 59 and abs(1 / pkf["W7.traps"]["champion"] - 740) < 5
          and pkf["W1"]["challenger"] < pkf["W1"]["champion"] and pkf["W9b"]["challenger"] < pkf["W9b"]["champion"],
          f"1/{1 / pkf['W7.traps']['challenger']:.1f}, 1/{1 / pkf['W7.traps']['champion']:.1f}")
    check("N7 rho = 0.5 coverage 0.800 (champion 0.783)", near(0.800, half["challenger"], 3) and near(0.783, half["champion"], 3))
    i13r = R["exploratory"]["issue13"]
    check("N7 #13 cell: challenger 0.766 [0.738, 0.791], 0.787, 0.802, 0.786; champion 0.523, 0.639, 0.716, 0.783",
          all(near(qv, r_["challenger"]["coverage"], 3) for qv, r_ in zip((0.766, 0.787, 0.802, 0.786), i13r, strict=True))
          and all(near(qv, r_["champion"]["coverage"], 3) for qv, r_ in zip((0.523, 0.639, 0.716, 0.783), i13r, strict=True))
          and near(0.738, i13r[0]["challenger"]["ci_bonferroni_wilson"][0], 3) and near(0.791, i13r[0]["challenger"]["ci_bonferroni_wilson"][1], 3))
    check("N7 sizes: 3,600 players a primary coverage, 9,600 at rho <= 1, 1,600 in #13; 118,400 computations; 6,720 games; 45 flawed and 23 clean",
          18 * 200 == 3600 and 12 * 800 == 9600 and 2 * 800 == 1600
          and 2 * 4 * (18 * 200 + 12 * 800 + 2 * 800) == 118_400 == R["compute"]["actual"]["profile_calls"]
          and 32 * 120 + 16 * 80 + 20 * 80 == 6720 == R["compute"]["actual"]["games"]
          and sum(1 for p_ in pl if flawed(p_)) == 45 and sum(1 for p_ in pl if clean(p_)) == 23)
    check("N7 3,254 s at about 6.5 cores is about 6 CPU-hours (prose, not in results.json)", near(6, 3254 * 6.5 / 3600, 0))
    check("N7 algorithm algo-1.1.1+42d749f7c50b", R["algorithm_id"] == "algo-1.1.1+42d749f7c50b")

    # N8: section 5 and 6 claims.
    check("N8 section 5: 0.02 is EXP-0002's Bonferroni half-width at 3,600 players, 3.0233 sqrt(0.16/3600)",
          near(0.02, Z20 * math.sqrt(0.16 / 3600), 3))
    check("N8 section 5: five keys at most 0.01 each is at most 0.05 extra credible estimates a report (one in 20)",
          near(0.05, 5 * 0.01, 6))
    holds_l = {(L, Gn): float(newcombe(28 - L, Gn, L, 17 - Gn, Z90)[1]) >= -0.10 for L in range(6) for Gn in range(4) if L >= Gn}
    check("N8 section 5: recall guard holds at 2 losses, fails at 3, with no gains",
          holds_l[(2, 0)] and not holds_l[(3, 0)] and holds_l[(1, 0)] and holds_l[(0, 0)])
    check("N8 Finding 3: net loss 2 holds at 3 lost and 1 gained, fails at 4 lost and 2 gained",
          holds_l[(3, 1)] and not holds_l[(4, 2)] and not holds_l[(5, 3)])
    ex = {n: exact_guard(n, 0.2, 0.05, False, 0.10)[0] for n in (23, 40, 60, 80)}
    quoted = {23: 0.49, 40: 0.68, 60: 0.81, 80: 0.88}
    check("N8 section 6: clean-alarm guard holds 0.49, 0.68, 0.81, 0.88 at 23, 40, 60, 80 players (exact, within MC error of 4,000)",
          all(abs(ex[n] - quoted[n]) <= 0.005 + 4 * math.sqrt(ex[n] * (1 - ex[n]) / 4000) for n in ex),
          ", ".join(f"{n}: {ex[n]:.4f}" for n in ex))
    rh = exact_guard(45, 0.63, 0.05, True, 0.10)[0]
    check("N8 section 6: recall at 2 holds 0.88 at no change (exact law, within MC error of 4,000)",
          abs(rh - 0.88) <= 0.005 + 4 * math.sqrt(rh * (1 - rh) / 4000), f"{rh:.4f}")
    sdp = P["pilot"]["sd"]
    norm_power = float(stats.norm.cdf((0.01 - P["pilot"]["false_credible_worse_by"]) / sdp["fc"] - Z90))
    check("N8 Result 13: normal approximation of the profile rule's power 0.886 against the replicates' 0.872 (within 0.03)",
          abs(norm_power - P["profile_rule"]["ship"]) < 0.03, f"{norm_power:.4f}")
    check("N8 section 6: whole rule 0.87 x 0.88 x 0.9 about 0.69; deviation 3: 0.3727 = 0.872 x 0.8755 x 0.48825",
          near(0.69, 0.872 * 0.8755 * 0.9, 2) and near(0.3727, 0.872 * 0.8755 * (1953 / 4000), 4) and near(0.4883, 1953 / 4000, 4)
          and near(0.3727, P["ship_power_approx"], 4))
    check("N8 section 6: the 80% interval of the false-credible difference about 0.001 wide at the pilot's SE",
          near(0.001, 2 * Z90 * sdp["fc"], 3), f"{2 * Z90 * sdp['fc']:.5f}")
    p200 = {dlt: float(stats.norm.cdf((0.01 - dlt) / (2 * sdp["fc"]) - Z90)) for dlt in (0.0089, 0.0093)}
    check("N8 section 6: at 200 a cell the guard's power is about one half at the pilot's 0.0093 (0.47), 0.73 at 0.0089",
          near(0.47, p200[0.0093], 2) and near(0.73, p200[0.0089], 2))
    pw = {se: float(stats.norm.cdf(0.02 / se - Z9875)) for se in (0.008, 0.016, sdp["D"], 2 * sdp["D"])}
    check("N8 Finding 1: at a true D of 0.05 the primary passes 0.16 at SE 0.016 (50 a cell) and 0.60 at SE 0.008 (200)",
          near(0.16, pw[0.016], 2) and near(0.60, pw[0.008], 2), f"pilot SE {sdp['D']}: {pw[sdp['D']]:.2f}; doubled: {pw[2 * sdp['D']]:.2f}")
    nstar = 200 * (0.008 / (0.02 / (Z9875 + float(stats.norm.ppf(0.8))))) ** 2
    check("N8 Finding 1: 80% power at a true D of 0.05 needs about 300 a cell at the quoted SE", 280 < nstar < 320, f"{nstar:.0f}")
    ksd = 3.0233 * math.sqrt(0.16 / 1600)
    check("N8 section 6: the #13 cell's Bonferroni half-width about 0.025 (z for 4: 0.0250; z for 20: 0.030)",
          near(0.025, Z4 * math.sqrt(0.16 / 1600), 3), f"z20 would give {ksd:.4f}")
    check("N8 section 3: dev D 0.69 against 0.05, so Delta about 0.64",
          near(0.64, 0.6928 - 0.050, 2))


def power_numbers() -> None:
    """N5: dev/power.json, recomputed from its own namespace EXP-0006-power."""
    ns = "EXP-0006-power"
    data = replay(ns)
    idx = resamples(np.random.default_rng(seed("bootstrap/paired", ns)))
    ps = profile_stats(data, idx)
    pp = P["pilot"]
    d_ch, delta, g1, fc = ps["challenger"]["dboot"], ps["delta_boot"], ps["rho1_boot"], ps["fc_boot"]
    sd = {k: float(np.std(v, ddof=1)) for k, v in {"D": d_ch, "delta": delta, "rho1": g1, "fc": fc}.items()}
    z975 = Z9875
    ship = ((d_ch + z975 * sd["D"] <= TOL) & (delta - z975 * sd["delta"] > 0)
            & (g1 + Z90 * sd["rho1"] <= 0.05) & (fc + Z90 * sd["fc"] <= 0.01))
    reject = (d_ch - z975 * sd["D"] > TOL) | (g1 - Z90 * sd["rho1"] > 0.05) | (fc - Z90 * sd["fc"] > 0.01)
    r1 = abs(ps["rho1"]["challenger"][0] - 0.8) - abs(ps["rho1"]["champion"][0] - 0.8)
    fcw = ps["fc"]["challenger"][0] - ps["fc"]["champion"][0]
    w = ps["challenger"]["worst"]
    check("N5 power.json pilot: D 0.705556 and 0.038333 (W1 at 30, 0.761667), Delta 0.667222",
          same(ps["champion"]["D"], pp["D_champion"]) and same(ps["challenger"]["D"], pp["D_challenger"])
          and same(ps["champion"]["D"] - ps["challenger"]["D"], pp["delta_D"])
          and (w["key"], w["games"]) == ("W1", 30) and same(w["coverage"], pp["worst_challenger"]["coverage"]))
    check("N5 power.json pilot: rho = 1 worse by -0.070969, false credible +0.009276, the four SDs",
          same(r1, pp["rho1_worse_by"]) and same(fcw, pp["false_credible_worse_by"])
          and all(same(sd[k], pp["sd"][k]) for k in sd))
    check("N5 power.json: the profile rule ships in 0.872 of 2,000 replicates and rejects in 0",
          round(float(ship.mean()), 4) == P["profile_rule"]["ship"] and round(float(reject.mean()), 4) == P["profile_rule"]["reject"])
    check("N5 section 6 quotes: D 0.706 and 0.038, W1 at 30 0.762, rho = 1 0.071 smaller, false credible 0.0093 higher, SE 0.0003",
          near(0.706, pp["D_champion"], 3) and near(0.038, pp["D_challenger"], 3) and near(0.762, pp["worst_challenger"]["coverage"], 3)
          and near(-0.071, pp["rho1_worse_by"], 3) and near(0.0093, pp["false_credible_worse_by"], 4) and near(0.0003, pp["sd"]["fc"], 4))


if __name__ == "__main__":
    DATA = replay()
    IDX = resamples(np.random.default_rng(seed("bootstrap/paired")))
    PS = profile_stats(DATA, IDX)
    symbolic()
    simulation(DATA, PS)
    numbers(DATA, PS)
    del DATA, IDX, PS
    power_numbers()
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem} (lab/lean/)")
    n_fail = len(FAILURES)
    print(f"{'FAILED' if n_fail else 'OK'}: {n_fail} failure(s)")
    sys.exit(1 if n_fail else 0)
