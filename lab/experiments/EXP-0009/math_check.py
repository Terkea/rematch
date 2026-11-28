"""EXP-0009's math_check.py (ADR 0118): checks math.md's results, not restates them.

R*: symbolic identities with sympy (and dense grids where sympy cannot settle a sign).
S*: simulations of each proof's model with a known truth, within 4 Monte Carlo SE.
N*: every quoted number, recomputed from results.json, power.json and pilot/pilot.json.
Run from this folder, in the lab image. Exits non-zero on the first failure.
"""

from __future__ import annotations

import json
import math
import re
import sys
from pathlib import Path

import numpy as np
import sympy as sp
from scipy.stats import norm

HERE = Path(__file__).resolve().parent
LEAN = {
    "Result 3": "Rematch.bonferroni (lab/lean/Rematch/Multiplicity.lean)",
    "Result 4": "Rematch.holm_fwer (lab/lean/Rematch/Multiplicity.lean)",
    "Result 10 (1)": "Rematch.luck_share, part 4 (lab/lean/Rematch/LuckShare.lean)",
}
KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
QS = (*KEYS, "accuracy")
ALPHA, M, DELTA = 0.05, 9, 14.0
GAMMA = 1 - ALPHA / M
Z_G = norm.ppf(1 - ALPHA / (2 * M))
LEVELS = {"ci80": 0.80, "ci95": 0.95, "ci97.5": 0.975, "ci99.4444": GAMMA}
FAILED: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"{'PASS' if ok else 'FAIL'} {name}{': ' + detail if detail else ''}")
    if not ok:
        FAILED.append(name)


def load(rel: str) -> dict:
    return json.loads((HERE / rel).read_text(encoding="utf-8"))


# ---- shared helpers ---------------------------------------------------------------------------


def ols_hc3(y: np.ndarray, X: np.ndarray, col: int = 1) -> tuple[float, float]:
    """OLS coefficient and HC3 standard error of column `col`, via the full sandwich."""
    XtXi = np.linalg.inv(X.T @ X)
    b = XtXi @ X.T @ y
    e = y - X @ b
    h = np.einsum("ij,jk,ik->i", X, XtXi, X)
    meat = (X * (e / (1 - h))[:, None] ** 2).T @ X
    V = XtXi @ meat @ XtXi
    return float(b[col]), float(math.sqrt(V[col, col]))


def holm(p: list[float], alpha: float = ALPHA) -> tuple[list[bool], list[float]]:
    """Holm's step-down: rejections and adjusted p-values (E5)."""
    m = len(p)
    order = sorted(range(m), key=lambda i: p[i])
    adj = [0.0] * m
    run = 0.0
    for k, i in enumerate(order):
        run = max(run, min(1.0, (m - k) * p[i]))
        adj[i] = run
    return [a <= alpha for a in adj], adj


def share(phi: float, g: int, n: int, L: int = 5) -> float:
    """Result 10 (2): the aggregate share s(g, N), closed form."""
    a = sum(phi ** -j for j in range(L)) / L
    return phi**g * a * (1 - phi**n) / (1 - phi**n * a) - 1


def share_direct(phi: float, g: int, n: int, L: int = 5) -> tuple[float, list[float]]:
    """Result 10 from the definitions: the weights of each P2 game's luck in both end ratings."""
    w, w3 = [], []
    for k in range(1, n + 1):
        w.append(sum(phi ** (n - j - k) for j in range(L) if n - j >= k) / L)
        w3.append(sum(phi ** (n + g - j - k) for j in range(L)) / L)
    per = [w3[i] / w[i] - 1 for i in range(n)]
    return (sum(w3) - sum(w)) / sum(w), per


# ---- symbolic -----------------------------------------------------------------------------------


def symbolic() -> None:
    # R1: FWL with a standardised column = s_x times the raw slope (exact on a random design).
    rng = np.random.default_rng(1)
    n = 400
    W = np.column_stack([np.ones(n), rng.normal(size=(n, 3))])
    x = rng.normal(size=n) + W[:, 1]
    y = 2 * x + W @ np.array([1, 0.5, -1, 2]) + rng.normal(size=n) * (1 + abs(x))
    z = (x - x.mean()) / x.std(ddof=1)
    b_std = np.linalg.lstsq(np.column_stack([np.ones(n), z, W[:, 1:]]), y, rcond=None)[0][1]
    xt = x - W @ np.linalg.lstsq(W, x, rcond=None)[0]
    check("R1 FWL: coefficient per observed SD = s_x * sum(x~ y)/sum(x~^2)",
          abs(b_std - x.std(ddof=1) * (xt @ y) / (xt @ xt)) < 1e-9)
    # R2: HC3's z element equals the scalar form, and HC3 >= HC0.
    X = np.column_stack([np.ones(n), z, W[:, 1:]])
    b, se3 = ols_hc3(y, X)
    XtXi = np.linalg.inv(X.T @ X)
    e = y - X @ (XtXi @ X.T @ y)
    h = np.einsum("ij,jk,ik->i", X, XtXi, X)
    zt = z - W @ np.linalg.lstsq(W, z, rcond=None)[0]
    scalar = math.sqrt(np.sum(zt**2 * e**2 / (1 - h) ** 2) / (zt @ zt) ** 2)
    hc0 = math.sqrt(np.sum(zt**2 * e**2) / (zt @ zt) ** 2)
    check("R2 HC3 scalar form equals the sandwich element; HC3 >= HC0",
          abs(scalar - se3) < 1e-10 and se3 >= hc0, f"{se3:.5f} vs HC0 {hc0:.5f}")
    # R3: the simultaneous quantile.
    check("R3 z_gamma = Phi^-1(1 - 0.05/18) = 2.77292", abs(Z_G - 2.77292) < 5e-6, f"{Z_G:.6f}")
    # R4: Result 6, both identities.
    b_, th, c, s = sp.symbols("b theta c s", positive=True)
    var_x, var_q1, cov_xq = 2 * c + 2 * s, 1 + s, -(c + s)
    cov_yx = b_ * sp.sqrt(2 * c) - th * c
    cov_yq = -b_ * sp.sqrt(c / 2) + th
    var_xt = var_x - cov_xq**2 / var_q1
    cov_yxt = cov_yx - cov_xq / var_q1 * cov_yq
    beta = sp.sqrt(var_x) * cov_yxt / var_xt
    want = b_ * sp.sqrt(c / (c + s)) + th * sp.sqrt(2) * s * (1 - c) / (sp.sqrt(c + s) * (2 + s - c))
    check("R4 Result 6: beta = b sqrt(r_D) + theta l, with q1 as covariate",
          sp.simplify(beta - want) == 0 and sp.simplify(var_xt - (c + s) * (2 + s - c) / (1 + s)) == 0)
    # Covariances of Result 6 from the model's parts (tau = 1).
    r1 = 1 - c
    check("R4b Result 6's covariances from (M2)",
          sp.simplify((r1 - 1 - s) - cov_xq) == 0
          and sp.simplify(b_ * (2 * c) / sp.sqrt(2 * c) + th * (r1 - 1) - cov_yx) == 0
          and sp.simplify(b_ * (r1 - 1) / sp.sqrt(2 * c) + th - cov_yq) == 0)
    # R5: Spearman-Brown, its delta-method SE, lengthening.
    rho, v, k, nn = sp.symbols("rho v k n", positive=True)
    rho_v = v / (v + 2)
    r_v = v / (v + 1)
    ok_sb = sp.simplify(r_v - (2 * rho_v / (1 + rho_v))) == 0
    se_r = sp.diff(2 * rho / (1 + rho), rho) * (1 - rho**2) / sp.sqrt(nn - 3)
    ok_se = sp.simplify(se_r - 2 * (1 - rho) / ((1 + rho) * sp.sqrt(nn - 3))) == 0
    rr = sp.symbols("r", positive=True)
    rk = (k * v / (k * v + 1)).subs(v, rr / (1 - rr))
    ok_k = sp.simplify(rk - k * rr / (1 + (k - 1) * rr)) == 0
    check("R5 Spearman-Brown 2rho/(1+rho), se(r) = 2(1-rho)/((1+rho)sqrt(n-3)), k-fold",
          ok_sb and ok_se and ok_k)
    # R6: power formula inversion (Result 9) and Bonferroni within Holm (Result 8).
    d, se_p, n_p, n = sp.symbols("d se_p n_p n", positive=True)
    zs = sp.symbols("Z", positive=True)
    nstar = (zs * se_p * sp.sqrt(n_p) / d) ** 2
    check("R6 Result 9: d/(se_p sqrt(n_p/n*)) = z_gamma + z_0.8",
          sp.simplify(d / (se_p * sp.sqrt(n_p / nstar)) - zs) == 0)
    rngp = np.random.default_rng(6)
    ok = True
    for _ in range(20000):
        p = list(rngp.uniform(0, 0.02, 9) ** rngp.uniform(0.5, 3))
        rej, _ = holm(p)
        ok &= all(rej[i] for i in range(9) if p[i] <= ALPHA / M)
    check("R6b every Bonferroni rejection is a Holm rejection (20,000 random families)", ok)
    # R7: Result 10, closed form against the definitions, Lean share before the last five, sign.
    phi_s, g_s, N_s = sp.symbols("phi g N", positive=True)
    a_s = sum(phi_s ** -j for j in range(5)) / 5
    num = sum(phi_s ** (g_s - j) for j in range(5)) / 5 * (1 - phi_s**N_s) / (1 - phi_s)
    den = sum((1 - phi_s ** (N_s - j)) for j in range(5)) / 5 / (1 - phi_s)
    ok_sym = sp.simplify(num / den - 1 - (phi_s**g_s * a_s * (1 - phi_s**N_s)
                                           / (1 - phi_s**N_s * a_s) - 1)) == 0
    ok_direct, ok_before = True, True
    for phi in (0.95, 0.97, 0.98, 0.995):
        for g in (5, 12, 40, 121):
            for nN in (10, 37, 150):
                agg, per = share_direct(phi, g, nN)
                ok_direct &= abs(agg - share(phi, g, nN)) < 1e-12
                ok_before &= all(abs(per[i] - (phi**g - 1)) < 1e-12 for i in range(nN - 4))
    check("R7 Result 10: s(g,N) closed form = definitions; share is phi^g - 1 before the last five",
          ok_sym and ok_direct and ok_before)
    ok_lim, ok_sign = True, True
    for phi in np.linspace(0.5, 0.999, 120):
        a = sum(phi ** -j for j in range(5)) / 5
        for g in range(4, 400, 3):
            t = phi**g * a  # the limit is t - 1: in (-1, 0] iff 0 < t <= 1
            ok_lim &= 0 < t <= 1 + 1e-15
            for nN in (5, 10, 20, 60, 200):
                lhs = phi**g * a + phi**nN * a * (1 - phi**g)
                if abs(lhs - 1) < 1e-12:
                    continue  # on the boundary, float rounding decides; nothing to test
                ok_sign &= (share(phi, g, nN) <= 0) == (lhs <= 1)
    g9 = min(g for g in range(0, 60) if share(0.97, g, 10) <= 0)
    g5 = min(g for g in range(0, 60) if share(0.97, g, 20) <= 0)
    check("R7b limit in (-1, 0] for g >= 4; finite sign condition; N=10 needs g>=9, N=20 g>=5",
          ok_lim and ok_sign and g9 == 9 and g5 == 5, "dense grid, phi 0.5 to 0.999")
    # R8: Result 12 (a), the omitted-variable identity in a sample.
    rng = np.random.default_rng(8)
    n = 3000
    W = np.column_stack([np.ones(n), rng.normal(size=(n, 4))])
    x = rng.normal(size=n)
    G = np.column_stack([0.6 * x + rng.normal(size=n), rng.normal(size=n) - 0.2 * x])
    y = 0.3 * x - 0.8 * G[:, 0] + 0.1 * G[:, 1] + rng.normal(size=n)
    XP = np.column_stack([W[:, :1], x, W[:, 1:]])
    XA = np.column_stack([XP, G])
    bP = np.linalg.lstsq(XP, y, rcond=None)[0]
    bA = np.linalg.lstsq(XA, y, rcond=None)[0]
    Dl = np.linalg.lstsq(XP, G, rcond=None)[0]
    check("R8 Result 12 (a): b_P - b_A = sum_k gamma_k delta_k (exact in the sample)",
          abs((bP[1] - bA[1]) - Dl[1] @ bA[-2:]) < 1e-10)
    # R9: Result 12 (b), the population algebra.
    a_, bb, beta_, kc, lam, Vc, VL = sp.symbols("a b beta kappa_c lambda V_c V_L", real=True)
    # Population moments from (M6): Var x = 1, D2 = (a + b) x + c_perp + L_perp.
    covyx = beta_ + kc * a_ + lam * bb
    var_d = (a_ + bb) ** 2 + Vc + VL
    covyd = covyx * (a_ + bb) + kc * Vc + lam * VL
    bx, bd = sp.symbols("bx bd")
    sol = sp.solve([sp.Eq(bx + bd * (a_ + bb), covyx), sp.Eq(bx * (a_ + bb) + bd * var_d, covyd)],
                   [bx, bd], dict=True)[0]
    rhoL = VL / (Vc + VL)
    check("R9 Result 12 (b): the normal equations give gamma and beta_A - beta as stated",
          sp.simplify(sol[bd] - (kc * (1 - rhoL) + lam * rhoL)) == 0
          and sp.simplify(sol[bx] - beta_ - (lam - kc) * (bb * (1 - rhoL) - rhoL * a_)) == 0
          and sp.simplify((covyx - sol[bx]) - sol[bd] * (a_ + bb)) == 0)


# ---- simulation ---------------------------------------------------------------------------------


def simulation() -> None:
    # S1: OLS + HC3 intervals under skewed, heteroskedastic errors (Results 1 to 3).
    rng = np.random.default_rng(20261128)
    n, reps, beta = 1500, 2000, 0.7
    cover95 = cover_g = 0
    ests = []
    for _ in range(reps):
        W = rng.normal(size=(n, 4))
        x = 0.5 * W[:, 0] + rng.normal(size=n)
        e = (rng.exponential(size=n) - 1) * (0.5 + np.abs(x) + W[:, 1] ** 2)
        sx = x.std(ddof=1)
        y = beta / sx * x + W @ np.array([1.0, -0.5, 0.2, 0.0]) + e
        z = (x - x.mean()) / sx
        b, se = ols_hc3(y, np.column_stack([np.ones(n), z, W]))
        ests.append(b)
        cover95 += abs(b - beta) <= 1.959964 * se
        cover_g += abs(b - beta) <= Z_G * se
    ests = np.array(ests)
    bias_ok = abs(ests.mean() - beta) < 4 * ests.std(ddof=1) / math.sqrt(reps) + 0.01
    c95, cg = cover95 / reps, cover_g / reps
    check("S1 OLS+HC3: unbiased, 95% and 99.44% coverage nominal",
          bias_ok and abs(c95 - 0.95) < 4 * math.sqrt(0.95 * 0.05 / reps)
          and abs(cg - GAMMA) < 4 * math.sqrt(GAMMA * (1 - GAMMA) / reps) + 0.002,
          f"mean {ests.mean():.4f} (truth {beta}), cover {c95:.4f}, {cg:.4f}")
    # S2: Holm's FWER at 9 correlated z's, all nulls true and a mix (Result 4).
    rng = np.random.default_rng(2)
    reps = 40000
    for label, corr in (("rho 0.6", 0.6), ("rho 0", 0.0)):
        C = np.full((9, 9), corr) + np.eye(9) * (1 - corr)
        Z = rng.multivariate_normal(np.zeros(9), C, size=reps)
        P = 2 * norm.sf(np.abs(Z))
        fwer = np.mean([any(holm(list(p))[0]) for p in P])
        check(f"S2 Holm FWER <= 0.05, global null, {label}",
              fwer <= ALPHA + 4 * math.sqrt(ALPHA * (1 - ALPHA) / reps), f"{fwer:.4f}")
    mu = np.array([0, 0, 0, 0, 0, -0.5, -0.5, 3, -4.0])
    Z = rng.multivariate_normal(mu, np.full((9, 9), 0.3) + np.eye(9) * 0.7, size=reps)
    P = 2 * norm.sf(np.abs(Z))
    nulls = mu == 0
    fwer = np.mean([any(np.array(holm(list(p))[0]) & nulls) for p in P])
    neg = mu < 0
    wrong = np.mean([any(np.array(holm(list(p))[0]) & neg & (z > 0)) for p, z in zip(P, Z)])
    se_w = math.sqrt(3 * ALPHA / 2 / reps)
    check("S2b Holm FWER over the true nulls of a mix; wrong-sign reads <= m_J alpha/2 (m_J = 3)",
          fwer <= ALPHA + 4 * math.sqrt(ALPHA * (1 - ALPHA) / reps)
          and 0 < wrong <= 3 * ALPHA / 2 + 4 * se_w,
          f"FWER {fwer:.4f}, wrong sign {wrong:.5f}")
    # S3: Result 6 by simulation with known b, theta.
    rng = np.random.default_rng(3)
    s, c, bt, th, n = 0.4, 0.03, 10.0, 4.0, 400_000
    T1 = rng.normal(size=n)
    T2 = (1 - c) * T1 + math.sqrt(1 - (1 - c) ** 2) * rng.normal(size=n)
    q1 = T1 + math.sqrt(s) * rng.normal(size=n)
    q2 = T2 + math.sqrt(s) * rng.normal(size=n)
    D = T2 - T1
    y = bt * D / math.sqrt(2 * c) + th * T1 + 20 * rng.normal(size=n)
    x = q2 - q1
    z = (x - x.mean()) / x.std(ddof=1)
    b, se = ols_hc3(y, np.column_stack([np.ones(n), z, q1]))
    ell = math.sqrt(2) * s * (1 - c) / (math.sqrt(c + s) * (2 + s - c))
    want = bt * math.sqrt(c / (c + s)) + th * ell
    check("S3 Result 6: coefficient = b sqrt(r_D) + theta l", abs(b - want) < 4 * se,
          f"{b:.3f} vs {want:.3f} (se {se:.3f})")
    # S4: Spearman-Brown SE at the run's size (Result 7, Finding 3).
    rng = np.random.default_rng(4)
    n, reps, rho = 16204, 400, 0.0247
    rh, rr = [], []
    for _ in range(reps):
        t = rng.normal(size=n)
        a = math.sqrt(rho) * t + math.sqrt(1 - rho) * rng.normal(size=n)
        b2 = math.sqrt(rho) * t + math.sqrt(1 - rho) * rng.normal(size=n)
        r_ = np.corrcoef(a, b2)[0, 1]
        rh.append(r_)
        rr.append(2 * r_ / (1 + r_))
    sd_rho, sd_r = float(np.std(rh, ddof=1)), float(np.std(rr, ddof=1))
    want_r = 2 * (1 - rho) / ((1 + rho) * math.sqrt(n - 3))
    want_rho = (1 - rho**2) / math.sqrt(n - 3)
    tol = 4 * math.sqrt(1 / (2 * (reps - 1)))
    check("S4 SD of r-hat ~ 0.015 and of rho-hat ~ 0.008 at n = 16,204",
          abs(sd_r / want_r - 1) < tol and abs(sd_rho / want_rho - 1) < tol
          and abs(np.mean(rr) - 2 * rho / (1 + rho)) < 4 * sd_r / math.sqrt(reps),
          f"{sd_r:.4f} vs {want_r:.4f}; {sd_rho:.4f} vs {want_rho:.4f}")
    # S5: the power formulas against a Holm family of nine correlated estimates (Result 8).
    rng = np.random.default_rng(5)
    reps, se_ = 40000, 1.0
    d = np.array([3.3, 3.0, 4.0, 2.6, 3.6, 2.0, 5.1, 4.5, 3.6])
    for label, mu in (("at d", d), ("at 0", np.zeros(9))):
        Zs = rng.multivariate_normal(mu, np.full((9, 9), 0.2) + np.eye(9) * 0.8, size=reps)
        P = 2 * norm.sf(np.abs(Zs))
        R = np.array([holm(list(p))[0] for p in P])
        if label == "at d":
            sim = np.mean(R & (Zs > 0), axis=0)
            form = norm.cdf(d / se_ - Z_G)
            ok = np.all(sim >= form - 4 * np.sqrt(form * (1 - form) / reps))
            check("S5 Holm 'climbing' power >= Phi(d/se - z_gamma) for each of nine", bool(ok),
                  f"min margin {np.min(sim - form):.4f}")
        else:
            inside = (np.abs(Zs) < d - Z_G * se_) & ~R
            sim = np.mean(inside, axis=0)
            form = np.maximum(0, 2 * norm.cdf((d - Z_G * se_) / se_) - 1)
            ok = np.all((sim <= form + 4 * np.sqrt(form * (1 - form) / reps + 1e-12))
                        & (sim >= form - ALPHA - 4 * np.sqrt(0.25 / reps)))
            check("S5b 'below' power within [formula - alpha, formula]", bool(ok),
                  f"max gap {np.max(form - sim):.4f}")
    # S6: the luck share and its bias, simulated on the AR(1) rating (Results 10, 11).
    rng = np.random.default_rng(7)
    # A light month (N = 10, g = 6): s(g, N) > 0 while phi^g - 1 and phi^g a - 1 are < 0, so
    # the three candidate shares predict different signs.
    phi, N, g, n = 0.97, 10, 6, 1_000_000
    xi = rng.normal(size=(n, N + g))
    e = np.zeros((n, N + g))
    e_prev = rng.normal(size=n) * math.sqrt(1 / (1 - phi**2))
    for t in range(N + g):
        e_prev = phi * e_prev + xi[:, t]
        e[:, t] = e_prev
    r_end2 = e[:, N - 5:N].mean(axis=1)
    r_end3 = e[:, N + g - 5:N + g].mean(axis=1)
    yl = r_end3 - r_end2
    # The exposure shares each P2 game's luck equally (kappa > 0), plus its own noise.
    x = xi[:, :N].mean(axis=1) + 0.5 * rng.normal(size=n)
    slope = np.cov(x, yl)[0, 1] / np.var(x, ddof=1)
    # The predicted slope: kappa * sum_k c_k / Var(x), with kappa = Var(xi)/N.
    w_sum = (1 - phi**N * sum(phi ** -j for j in range(5)) / 5) / (1 - phi)
    pred = share(phi, g, N) * w_sum * (1 / N) / np.var(x, ddof=1)
    se_slope = np.std(yl) / (np.std(x) * math.sqrt(n))
    a_bar = sum(phi ** -j for j in range(5)) / 5
    alts = [sh * w_sum / N / np.var(x, ddof=1) for sh in (phi**g - 1, phi**g * a_bar - 1)]
    check("S6 Results 10-11: luck slope = kappa s(g,N) sum w / Var x; Lean and limit shares rejected",
          abs(slope - pred) < 4 * se_slope and all(abs(slope - v) > 4 * se_slope for v in alts)
          and slope > 0,
          f"{slope:.4f} vs {pred:.4f} (se {se_slope:.4f}); alternatives {alts[0]:.4f}, {alts[1]:.4f}")
    # A typical month (N = 60, g = 20): the bias is negative, as predicted.
    phi, N, g, n = 0.97, 60, 20, 200_000
    xi = rng.normal(size=(n, N + g))
    e_prev = rng.normal(size=n) * math.sqrt(1 / (1 - phi**2))
    e = np.zeros((n, N + g))
    for t in range(N + g):
        e_prev = phi * e_prev + xi[:, t]
        e[:, t] = e_prev
    yl = e[:, N + g - 5:N + g].mean(axis=1) - e[:, N - 5:N].mean(axis=1)
    x = xi[:, :N].mean(axis=1) + 0.5 * rng.normal(size=n)
    slope = np.cov(x, yl)[0, 1] / np.var(x, ddof=1)
    w_sum = (1 - phi**N * sum(phi ** -j for j in range(5)) / 5) / (1 - phi)
    pred = share(phi, g, N) * w_sum / N / np.var(x, ddof=1)
    se_slope = np.std(yl) / (np.std(x) * math.sqrt(n))
    check("S6b typical month: the luck slope is negative and as predicted",
          abs(slope - pred) < 4 * se_slope and slope < 0, f"{slope:.4f} vs {pred:.4f}")
    # S7: Result 12 by simulation: no shared luck (b = 0), yet the shift is lambda rho a < 0.
    rng = np.random.default_rng(9)
    n = 400_000
    a_, b_, beta_, kc, lam, Vc, VL = 0.8, 0.0, 1.0, 0.0, -0.95, 1.0, 1.5
    x = rng.normal(size=n)
    cc = a_ * x + math.sqrt(Vc) * rng.normal(size=n)
    Lw = b_ * x + math.sqrt(VL) * rng.normal(size=n)
    y = beta_ * x + kc * cc + lam * Lw + rng.normal(size=n)
    bP, seP = ols_hc3(y, np.column_stack([np.ones(n), x]))
    bA, seA = ols_hc3(y, np.column_stack([np.ones(n), x, cc + Lw]))
    rhoL = VL / (Vc + VL)
    want_shift = lam * rhoL * (a_ + b_)
    check("S7 Result 12: primary unbiased, shift = lambda rho_L a, adjusted biased up",
          abs(bP - beta_) < 4 * seP and abs((bP - bA) - want_shift) < 4 * (seP + seA)
          and bA - beta_ > 0, f"b_P {bP:.3f}, shift {bP - bA:.3f} vs {want_shift:.3f}")
    # A general case: continued catch-up and shared luck, every term of Result 12 (b).
    a_, b_, kc = 0.6, 0.4, 0.3
    cc = a_ * x + math.sqrt(Vc) * rng.normal(size=n)
    Lw = b_ * x + math.sqrt(VL) * rng.normal(size=n)
    y = beta_ * x + kc * cc + lam * Lw + rng.normal(size=n)
    bP, seP = ols_hc3(y, np.column_stack([np.ones(n), x]))
    XA = np.column_stack([np.ones(n), x, cc + Lw])
    bA, seA = ols_hc3(y, XA)
    gA = np.linalg.lstsq(XA, y, rcond=None)[0][2]
    gam = kc * (1 - rhoL) + lam * rhoL
    okg = (abs(bP - (beta_ + kc * a_ + lam * b_)) < 4 * seP and abs(gA - gam) < 0.01
           and abs(bA - beta_ - (lam - kc) * (b_ * (1 - rhoL) - rhoL * a_)) < 4 * seA)
    check("S7b Result 12 with kappa_c = 0.3, b = 0.4: beta_P, gamma and beta_A as derived", okg,
          f"gamma {gA:.3f} vs {gam:.3f}")


# ---- numbers ------------------------------------------------------------------------------------


def r2(x: float) -> float:
    return round(x + 0.0, 2)


def numbers() -> None:
    r = load("results.json")
    pw = load("power.json")
    pl = load("pilot/pilot.json")
    pr = r["primary"]
    # N1: every interval, minimum, p and disattenuated value of every family is its formula.
    ok = True
    for fam in ("primary", "adjusted", "contemporaneous", "level"):
        for q in QS:
            f = r[fam][q]
            for key, lev in LEVELS.items():
                z = norm.ppf(1 - (1 - lev) / 2)
                ok &= all(abs(u - v) < 2.5e-6 for u, v in
                          zip(f[key], (f["estimate"] - z * f["se"], f["estimate"] + z * f["se"])))
            # reliability is rounded to 6 decimals: propagate its 5e-7 through each formula.
            rel = f["reliability"]
            ok &= abs(f["delta_observed"] - DELTA * math.sqrt(rel)) < 7 / math.sqrt(rel) * 5e-7 + 1e-6
            tol = abs(f["estimate"]) / math.sqrt(rel) * (2.5e-7 / rel) + 5e-7 / math.sqrt(rel) + 1e-6
            ok &= abs(f["disattenuated"] - f["estimate"] / math.sqrt(rel)) < tol
            ok &= abs(f["p"] - 2 * norm.sf(abs(f["estimate"]) / f["se"])) < 1e-5
    check("N1 every CI = est +- z se, minimum = 14 sqrt(r), p two-sided normal (4 families)", ok)
    # N2: the decision table, as quoted.
    table = {  # n, est, ci95, sim, rel, minimum, reads
        "W4.allowed": (16204, -1.18, (-2.23, -0.14), (-2.66, 0.29), 0.051, 3.17, "below the minimum effect"),
        "W4.missed": (9653, 0.75, (-0.76, 2.26), (-1.39, 2.88), 0.035, 2.64, "undetermined"),
        "W5": (10416, -2.94, (-4.34, -1.54), (-4.91, -0.96), 0.087, 4.13, "precedes falling"),
        "W6": (12099, -1.03, (-2.19, 0.12), (-2.66, 0.60), 0.056, 3.32, "below the minimum effect"),
        "W7.prep": (16204, 0.66, (-0.27, 1.59), (-0.66, 1.97), 0.019, 1.95, "undetermined"),
        "W7.traps": (16204, 0.04, (-0.92, 0.99), (-1.31, 1.39), 0.048, 3.08, "below the minimum effect"),
        "W9a": (16204, -0.02, (-0.84, 0.80), (-1.18, 1.14), 0.131, 5.07, "below the minimum effect"),
        "W9b": (2094, -0.32, (-3.25, 2.60), (-4.47, 3.82), 0.040, 2.79, "undetermined"),
        "accuracy": (16204, -0.12, (-1.09, 0.84), (-1.49, 1.24), 0.067, 3.63, "below the minimum effect"),
    }
    ok = True
    for q, (n, est, c95, cs, rel, mn, reads) in table.items():
        f = pr[q]
        ok &= f["n"] == n and r2(f["estimate"]) == est and f["reads"] == reads
        ok &= tuple(r2(v) for v in f["ci95"]) == c95 and tuple(r2(v) for v in f["ci99.4444"]) == cs
        ok &= round(f["reliability"], 3) == rel and r2(f["delta_observed"]) == mn
    check("N2 the decision's primary table (9 rows, 7 columns)", ok)
    # N3: Holm and the reads recomputed from the p-values; the decision.
    for fam in ("primary", "adjusted", "contemporaneous", "level"):
        ps = [r[fam][q]["p"] for q in QS]
        rej, adj = holm(ps)
        ok = True
        for q, rj, a in zip(QS, rej, adj):
            f = r[fam][q]
            # p is rounded to 6 decimals: compare adjusted p within (m - j + 1) * 5e-7.
            ok &= abs(a - f["p_holm"]) < 9 * 5e-7 + 1e-9 and rj == f["holm_reject"]
            lo, hi = f["ci99.4444"]
            d = f["delta_observed"]
            want = ("precedes climbing" if rj and f["estimate"] > 0 else "precedes falling" if rj
                    else "below the minimum effect" if -d < lo and hi < d else "undetermined")
            ok &= want == f["reads"]
        check(f"N3 Holm and the reads recomputed, {fam}", ok)
    dec = r["decision"]
    below = [q for q in KEYS if pr[q]["reads"] == "below the minimum effect"]
    check("N3b outcome 3: no key climbs, 4 below (6 needed), control fails; W5 Holm p 0.0003",
          dec["outcome"] == "need more data" and dec["climbing"] == [] and len(below) == 4
          and sorted(below) == sorted(dec["below_minimum"]) and not dec["positive_control_holds"]
          and round(pr["W5"]["p_holm"], 4) == 0.0003
          and abs(9 * pr["W5"]["p"] - pr["W5"]["p_holm"]) < 9 * 5e-7 + 1e-9)
    acc = pr["accuracy"]
    check("N3c accuracy 99.44% [-1.49, 1.24] against +-3.63; summary -0.1 [-1.1, 0.8]",
          [r2(v) for v in acc["ci99.4444"]] == [-1.49, 1.24] and r2(acc["delta_observed"]) == 3.63
          and round(acc["estimate"], 1) == -0.1 and [round(v, 1) for v in acc["ci95"]] == [-1.1, 0.8])
    # N4: adjusted, contemporaneous, level.
    ad = r["adjusted"]
    ok = (r2(ad["W5"]["estimate"]) == -0.45 and [r2(v) for v in ad["W5"]["ci95"]] == [-1.83, 0.92])
    for q, est, ci in (("accuracy", 3.0, (2.1, 4.0)), ("W7.prep", 1.9, (1.0, 2.8)),
                       ("W7.traps", 1.5, (0.6, 2.5))):
        f = ad[q]
        ok &= round(f["estimate"], 1) == est and tuple(round(v, 1) for v in f["ci95"]) == ci
        ok &= f["reads"] == "precedes climbing"
    ok &= [q for q in QS if ad[q]["reads"] == "precedes climbing"] == ["W7.traps", "W7.prep", "accuracy"]
    shift = {q: pr[q]["estimate"] - ad[q]["estimate"] for q in QS}
    ok &= round(shift["accuracy"], 2) == -3.16 and round(shift["W5"], 2) == -2.48
    ok &= all(v < 0 for v in shift.values())
    check("N4 adjusted: W5 -0.45, accuracy +3.0, W7.prep +1.9, W7.traps +1.5; shifts all < 0", ok)
    co = r["contemporaneous"]
    want = {"accuracy": 14.1, "W5": 12.5, "W6": 8.5, "W7.traps": 7.0, "W4.allowed": 6.1,
            "W7.prep": 5.6, "W4.missed": 5.2, "W9a": 0.8}
    ok = all(round(co[q]["estimate"], 1) == v for q, v in want.items())
    ok &= [round(v, 1) for v in co["accuracy"]["ci95"]] == [13.1, 15.2]
    ok &= [r2(v) for v in co["W9a"]["ci95"]] == [-0.01, 1.65]
    check("N4b contemporaneous estimates as quoted", ok)
    lv = r["level"]
    want = {"accuracy": (6.2, 4.6, 7.9), "W7.prep": (5.9, 4.6, 7.1), "W7.traps": (4.4, 2.9, 6.0),
            "W4.missed": (2.5, 1.0, 4.1), "W5": (2.1, 0.5, 3.6)}
    ok = all((round(lv[q]["estimate"], 1), *[round(v, 1) for v in lv[q]["ci95"]]) == w
             and lv[q]["holm_reject"] for q, w in want.items())
    ok &= all(lv[q]["reads"] == "below the minimum effect" for q in ("W4.allowed", "W6", "W9a"))
    rels = sorted(lv[q]["reliability"] for q in QS)
    ok &= round(rels[2], 2) == 0.36 and round(rels[-1], 2) == 0.79
    ok &= round(lv["W4.missed"]["reliability"], 2) == 0.17 and round(lv["W9b"]["reliability"], 2) == 0.12
    ok &= [round(lv[q]["estimate"]) for q in ("accuracy", "W7.traps", "W7.prep")] == [6, 4, 6]
    check("N4c level family as quoted; reliabilities 0.36 to 0.79 for seven of nine", ok)
    # N5: reliabilities, Result 5's thresholds and Result 7's SE (Finding 3).
    rel = [pr[q]["reliability"] for q in QS]
    ok = round(min(rel), 2) == 0.02 and round(max(rel), 2) == 0.13
    wp = pr["W7.prep"]
    ok &= r2(wp["ci99.4444"][1]) == 1.97 and r2(wp["delta_observed"]) == 1.95
    ok &= abs((wp["ci99.4444"][1] - wp["delta_observed"]) - 0.0274) < 5e-4
    rstar = {q: (max(abs(v) for v in pr[q]["ci99.4444"]) / DELTA) ** 2 for q in QS}
    want = {"W4.allowed": 0.0361, "W6": 0.0362, "W7.traps": 0.0098, "W9a": 0.0071,
            "accuracy": 0.0113, "W7.prep": 0.0199}
    ok &= all(round(rstar[q], 4) == v for q, v in want.items())
    ok &= all(pr[q]["reliability"] > rstar[q] for q in below) and wp["reliability"] < rstar["W7.prep"]
    check("N5 reliabilities 0.02-0.13; W7.prep missed by 0.027; r* of each 'below' read", ok)
    ses = {}
    for q in QS:
        rr = pr[q]["reliability"]
        rho = rr / (2 - rr)
        ses[q] = (2 * (1 - rho) / ((1 + rho) * math.sqrt(pr[q]["n"] - 3)),
                  (1 - rho**2) / math.sqrt(pr[q]["n"] - 3))
    big = [ses[q][0] for q in QS if pr[q]["n"] >= 9653]
    wt = pr["W7.traps"]
    lo_, hi_ = wt["reliability"] - 1.96 * ses["W7.traps"][0], wt["reliability"] + 1.96 * ses["W7.traps"][0]
    ok = round(min(big), 4) == 0.0137 and round(max(big), 4) == 0.0196 and round(ses["W9b"][0], 3) == 0.042
    ok &= round(ses["W7.traps"][1], 3) == 0.008 and round(ses["W7.traps"][0], 3) == 0.015
    ok &= round(lo_, 2) == 0.02 and round(hi_, 2) == 0.08
    ok &= round(DELTA * math.sqrt(lo_)) == 2 and round(DELTA * math.sqrt(hi_)) == 4
    ok &= abs((ses["W4.allowed"][0] and (pr["W4.allowed"]["reliability"] - rstar["W4.allowed"])
               / ses["W4.allowed"][0]) - 1.02) < 0.01
    ok &= round((pr["W6"]["reliability"] - rstar["W6"]) / ses["W6"][0], 2) == 1.16
    ok &= round((wt["reliability"] - rstar["W7.traps"]) / ses["W7.traps"][0], 1) == 2.6
    ok &= round((acc["reliability"] - rstar["accuracy"]) / ses["accuracy"][0], 1) == 3.8
    check("N5b se(r) 0.0137-0.0196 (W9b 0.042), se(rho) 0.008; W7.traps [0.02, 0.08], minimum 2 to 4",
          ok, f"W7.traps se(r) {ses['W7.traps'][0]:.4f}, se(rho) {ses['W7.traps'][1]:.4f}")
    check("N5c Spearman-Brown: 3x the games lifts 0.05 to 0.136 ('about 0.14')",
          round(3 * 0.05 / (1 + 2 * 0.05), 3) == 0.136)
    # Result 6's leak factor from the run's reliabilities.
    ells = {}
    for q in ("accuracy", "W7.prep"):
        rL, rD = lv[q]["reliability"], pr[q]["reliability"]
        s_ = (1 - rL) / rL
        c_ = s_ * rD / (1 - rD)
        ells[q] = math.sqrt(2) * s_ * (1 - c_) / (math.sqrt(c_ + s_) * (2 + s_ - c_))
    theta = lv["accuracy"]["estimate"] / math.sqrt(lv["accuracy"]["reliability"]) / 2
    check("N5d Result 6: l = 0.35 (accuracy), 0.41 (W7.prep); illustration +1.3",
          round(ells["accuracy"], 2) == 0.35 and round(ells["W7.prep"], 2) == 0.41
          and round(theta, 2) == 3.66 and round(theta * ells["accuracy"], 1) == 1.3)
    # N6: attrition, bands, exploratory.
    at = r["attrition"]
    fl = r["flow"]
    ok = round(fl["with_outcome"] / fl["observed_games"], 3) == 0.946
    ok &= round(fl["p3_active"] / fl["audience"], 3) == 0.946
    ok &= max(abs(at[q]["per_sd"]) for q in QS) < 0.006
    ok &= r2(at["W5"]["ipw_primary"]["estimate"]) == -2.85
    ok &= [r2(v) for v in at["W5"]["ipw_primary"]["ci95"]] == [-4.23, -1.47]
    ok &= r2(at["accuracy"]["ipw_primary"]["estimate"]) == 0.15
    ok &= [r2(v) for v in at["accuracy"]["ipw_primary"]["ci95"]] == [-0.86, 1.16]
    bb = r["by_band"]
    w5 = [bb[b]["W5"]["estimate"] for b in ("B1", "B2", "B3")]
    ok &= round(max(w5), 1) == -2.4 and round(min(w5), 1) == -3.8
    t0 = bb["B0"]["W7.traps"]
    ok &= round(t0["estimate"], 1) == 6.3 and [round(v, 1) for v in t0["ci95"]] == [0.8, 11.8]
    ok &= bb["B0"]["players"] == 554
    br = r["exploratory"]["blunder_rate"]
    ok &= r2(br["estimate"]) == 0.97 and [r2(v) for v in br["ci95"]] == [-0.02, 1.96]
    check("N6 attrition 94.6%, |per SD| < 0.6 points, IPW; bands; blunder rate", ok)
    # N7: flow, games, compute, stage 2.
    ok = (fl["p0_history"], fl["feature_months"], fl["audience"], fl["observed_games"],
          fl["with_outcome"], fl["p3_active"]) == (86819, 18968, 17138, 17136, 16204, 16206)
    ok &= fl["by_band"] == {"B0": 554, "B1": 4009, "B2": 7514, "B3": 5059}
    gm = r["games"]
    ok &= (gm["games_read"], gm["games_observed"], gm["games_failed"],
           gm["games_with_an_opponent_in_the_other_slice"]) == (877072, 876947, 125, 91366)
    seen = sum(r["months"][k]["games_seen"] for k in ("P0", "P1", "P2", "P3"))
    ok &= round(seen / 1e6, 1) == 366.2 and round(r["rating_change_sd"], 1) == 53.8
    cp = r["compute"]
    ok &= round(100 * cp["actual"]["observe_games"] / cp["budget"]["observe_games"]) == 73
    ok &= r["players"] == 17136 and r["design"]["slice"] == [0.0, 0.3]
    ok &= cp["actual"]["engine_nodes"] == cp["budget"]["engine_nodes"] == 1_000_000
    src = (HERE / "analysis.py").read_text(encoding="utf-8")
    const = {k: float(v) for k, v in re.findall(r"^([A-Z0-9_]+) = ([0-9.]+)\b", src, re.M)}
    ok &= (const["MIN_EVAL_GAMES"], const["MAX_EVAL_GAMES"], const["STAGE2_MAX_CPU_HOURS"],
           const["STAGE2_CPU_SECONDS_PER_GAME"], const["DELTA"]) == (10, 40, 20, 39.1, 14)
    ok &= r["stage2"]["max_cpu_hours"] == const["STAGE2_MAX_CPU_HOURS"]
    check("N7 cohort flow, games, 366.2 million, SD 53.8, 73% and 100% of the budget; constants", ok)
    # N8: the pilot.
    pf = pl["flow"]
    ok = (pf["p0_history"], pf["feature_months"], pf["audience"], pf["observed_games"],
          pf["with_outcome"]) == (28721, 6365, 5820, 5818, 5472)
    ok &= pl["games"]["games_read"] == 305043 and pl["games"]["games_failed"] == 54
    ok &= round(3 * pl["games"]["games_read"], -3) == 915000
    want = {"W4.missed": 0.01, "W7.prep": 0.026, "W4.allowed": 0.044, "W7.traps": 0.047,
            "accuracy": 0.071, "W9a": 0.10, "W9b": 0.12, "W5": 0.14, "W6": 0.15}
    ok &= all(round(pl["primary"][q]["reliability"], 3 if v < 0.1 else 2) == v for q, v in want.items())
    check("N8 the pilot: flow, 305,043 games, 54 failed, ~915,000, reliabilities", ok)
    # N9: the power grid and q recomputed from the pilot (E8), stage 2 (E9).
    width = 0.1
    ok, first = True, None
    for qg in (0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9):
        cell = pw["q_grid"][f"{qg:g}"]
        at_t = 0
        for q, f in pl["primary"].items():
            n = f["n"] * qg / width
            se = f["se"] * math.sqrt(f["n"] / n)
            d = f["delta_observed"]
            up = norm.cdf(d / se - Z_G)
            sm = max(0.0, 2 * norm.cdf((d - Z_G * se) / se) - 1)
            got = cell["per_quantity"][q]
            ok &= abs(got["n"] - n) < 1e-6 and abs(got["se"] - se) < 2e-6
            ok &= abs(got["power_climbing"] - up) < 2e-6 and abs(got["power_small"] - sm) < 2e-6
            if q in KEYS and min(up, sm) >= 0.8:
                at_t += 1
            if q == "accuracy":
                ctrl = up >= 0.8
        ok &= cell["keys_at_target"] == at_t and cell["control_at_target"] == ctrl
        if first is None and at_t >= 6 and ctrl:
            first = qg
    g3 = pw["q_grid"]["0.3"]["per_quantity"]
    ok &= first == 0.3 and pw["q"] == 0.3 and pw["reaches_target"]
    ok &= (round(g3["W9b"]["power_climbing"], 2), round(g3["W9b"]["power_small"], 2)) == (0.75, 0.50)
    ok &= (round(g3["W4.missed"]["power_climbing"], 2), round(g3["W4.missed"]["power_small"], 2)) == (0.15, 0.0)
    ok &= round(g3["accuracy"]["power_climbing"], 2) == 1.0
    need = sorted(((Z_G + norm.ppf(0.8)) * f["se"] * math.sqrt(f["n"]) / f["delta_observed"]) ** 2
                  for q, f in pl["primary"].items() if q in KEYS)
    n2 = need[len(need) // 2]
    cpu = n2 * 80 * 39.1 / 3600
    st = pw["stage2"]
    ok &= abs(n2 - st["players_for_median_key"]) < 1e-3 and abs(cpu - st["cpu_hours"]) < 1e-3
    ok &= round(n2, -1) == 5240 and round(cpu, -1) == 4560 and round(cpu / 20) == 228
    ok &= r["stage2"]["runs"] is False
    per_key = {q: ((Z_G + norm.ppf(0.8)) * f["se"] * math.sqrt(f["n"]) / f["delta_observed"]) ** 2
               for q, f in pl["primary"].items() if q in KEYS}
    ok &= abs(per_key["W7.traps"] - n2) < 1e-9
    passed = [q for q in KEYS if min(g3[q]["power_climbing"], g3[q]["power_small"]) >= 0.8]
    lows = {q: min(g3[q]["power_climbing"], g3[q]["power_small"]) for q in passed}
    ok &= len(passed) == 6 and min(lows, key=lows.get) == "W7.prep"
    ok &= round(min(lows.values()), 3) == 0.966 and min(lows.values()) - ALPHA > 0.8
    check("N9 power grid recomputed from the pilot; q = 0.3; stage 2 5,240 players, 4,560 CPU-h, 228x",
          ok, f"n* {n2:.1f}, {cpu:.1f} CPU-hours")
    # Finding 2's numbers.
    a97 = sum(0.97 ** -j for j in range(5)) / 5
    a98 = sum(0.98 ** -j for j in range(5)) / 5
    ok = round(a97, 4) == 1.0638 and round(a98, 4) == 1.0417
    ok &= round(a97 * 0.97**20 - 1, 2) == -0.42 and round(0.97**20 - 1, 2) == -0.46
    ok &= round(a97 * 0.97**121 - 1, 3) == -0.973 and round(0.97**121 - 1, 3) == -0.975
    fac = {(phi, g): a * (1 - phi**g) / (1 - a * phi**g)
           for phi, a in ((0.97, a97), (0.98, a98)) for g in (20, 121)}
    ok &= round(fac[(0.97, 121)], 3) == 1.066 and round(fac[(0.98, 121)], 3) == 1.046
    ok &= round(fac[(0.97, 20)], 2) == 1.15 and round(fac[(0.98, 20)], 2) == 1.14
    pN = 0.97**100 * a97
    ok &= round(pN / (1 - pN), 3) == 0.053
    check("N10 Finding 2: a-bar, shares, ratio factor 1.066/1.046 at 121, 1.15/1.14 at 20; 0.053", ok)


if __name__ == "__main__":
    symbolic()
    simulation()
    numbers()
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem}")
    print(f"{'FAILED: ' + ', '.join(FAILED) if FAILED else 'all checks passed'}")
    sys.exit(1 if FAILED else 0)
