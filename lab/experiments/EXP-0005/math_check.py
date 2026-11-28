"""EXP-0005: the maths, checked (ADR 0118; math.md in this folder).

EXP-0005 was closed unrun by its value gate, so it has no results.json. What is checked is the
argument that closed it and the design it would have run: the Jeffreys classification of the
churn, its exact power (section 6), its error rates, and the claim that no branch of section 9
could change the suites, recomputed with an independent implementation of EXP-0001's recall
simulation on EXP-0001's stored pilot.

Run from this folder in the lab image:

    python math_check.py

One line a check, PASS or FAIL; exit 1 if any fails. Standard library, numpy, scipy and sympy.
"""

from __future__ import annotations

import hashlib
import json
import math
import re
import sys
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import stats

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
E1 = json.loads((HERE.parent / "EXP-0001" / "results.json").read_text(encoding="utf-8"))
E6 = json.loads((HERE.parent / "EXP-0006" / "results.json").read_text(encoding="utf-8"))
SUITE_RS = (ROOT / "backend" / "crates" / "bench" / "src" / "suite.rs").read_text(encoding="utf-8")

FLAWS = ["hang", "miss_mate", "opening_blunder", "low_clock_blunder", "no_convert"]  # EXP-0001 order
B0 = 0.10  # EXP-0001's assumed churn
DELTA = 0.10
M_GRID = [1, 2, 3, 4, 5, 6, 8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48, 64, 80, 100]
DELTA_GRID = [0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50]
BUDGET_GAMES = 9000
CLEAN = 2
K = 5
Z_SHIP = float(stats.norm.ppf(0.9875))  # the 97.5% Newcombe interval's lower end
LEAN = {"Result 1 (Jeffreys interval: conjugacy with a = b = 1/2)": "Rematch.beta_binomial_posterior"}

FAILURES: list[str] = []


def check(name: str, ok: bool, detail: str = "") -> None:
    print(f"{'PASS' if ok else 'FAIL'}  {name}{': ' + detail if detail else ''}")
    if not ok:
        FAILURES.append(name)


def zero(expr) -> bool:
    return sp.simplify(expr) == 0


def near(quoted: float, value: float, dp: int) -> bool:
    return abs(quoted - value) <= 0.5 * 10.0 ** (-dp) + 1e-9


def jeffreys(x: int, n: int, level: float = 0.95) -> tuple[float, float]:
    """(E1): Brown, Cai and DasGupta's Jeffreys interval, ends 0 and 1 kept."""
    a = (1 - level) / 2
    lo = 0.0 if x == 0 else float(stats.beta.ppf(a, x + 0.5, n - x + 0.5))
    hi = 1.0 if x == n else float(stats.beta.ppf(1 - a, x + 0.5, n - x + 0.5))
    return lo, hi


def classify(x: int, n: int) -> str:
    """(E2)."""
    lo, hi = jeffreys(x, n)
    return "below" if hi < B0 else "above" if lo > B0 else "straddles"


def design(n: int, b: float) -> dict:
    """Result 2: the exact law of the classification at true churn b."""
    pmf = stats.binom.pmf(np.arange(n + 1), n, b)
    out = {"below": 0.0, "straddles": 0.0, "above": 0.0}
    for x in range(n + 1):
        out[classify(x, n)] += float(pmf[x])
    return out


def pilot_hits(g: int) -> tuple[np.ndarray, np.ndarray]:
    """h_f and n_f: EXP-0001's stored pilot, hits at 2 at checkpoint g, per flaw."""
    h, n = np.zeros(K), np.zeros(K)
    for p in E1["pilot"]["players"]:
        if p["flaw"] is None:
            continue
        c = next(c for c in p["checkpoints"] if c["games"] == g)
        i = FLAWS.index(p["flaw"])
        h[i] += bool(c["hit2"])
        n[i] += 1
    return h, n


def wilson(x, n, z):
    p = x / n
    den = 1 + z * z / n
    ctr = (p + z * z / (2 * n)) / den
    h = z * np.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return ctr - h, ctr + h


def newcombe_lo(n11, n10, n01, n00, z):
    """The lower end of Newcombe's method 10 for p1 - p2 (p1 the candidate)."""
    n = n11 + n10 + n01 + n00
    p1, p2 = (n11 + n10) / n, (n11 + n01) / n
    l1, _ = wilson(n11 + n10, n, z)
    _, u2 = wilson(n11 + n01, n, z)
    den = np.sqrt((n11 + n10) * (n01 + n00) * (n11 + n01) * (n10 + n00))
    phi = np.divide(n11 * n00 - n10 * n01, den, out=np.zeros_like(den), where=den > 0)
    return p1 - p2 - np.sqrt(np.maximum((p1 - l1) ** 2 - 2 * phi * (p1 - l1) * (u2 - p2) + (u2 - p2) ** 2, 0))


def power(rng, g: int, m: int, delta: float, b: float, sims: int) -> float:
    """Result 4: EXP-0001's model (its math.md (M1)-(M2), (E1)-(E5)), written here independently.
    Each simulated suite draws pi_f ~ Beta(h_f + 1/2, n_f - h_f + 1/2), m reference outcomes a flaw
    from Bernoulli(pi_f), loses a hit with probability b and gains a miss with probability
    a = (delta + b pibar)/(1 - pibar); ship when the 97.5% Newcombe lower end is above 0 and a <= 1."""
    h, n = pilot_hits(g)
    pi = rng.beta(h + 0.5, n - h + 0.5, size=(sims, K))
    pibar = pi.mean(1)
    a = (delta + b * pibar) / (1 - pibar)
    feasible = a <= 1
    ref = rng.random((sims, K, m)) < pi[:, :, None]
    u = rng.random((sims, K, m))
    cand = np.where(ref, u >= b, u < np.minimum(a, 1)[:, None, None])
    ref, cand = ref.reshape(sims, -1).astype(float), cand.reshape(sims, -1).astype(float)
    n11 = (ref * cand).sum(1)
    n10 = ((1 - ref) * cand).sum(1)
    n01 = (ref * (1 - cand)).sum(1)
    n00 = ((1 - ref) * (1 - cand)).sum(1)
    return float(((newcombe_lo(n11, n10, n01, n00, Z_SHIP) > 0) & feasible).mean())


def approx_power(rng, g: int, m: int, delta: float, b: float, draws: int = 20_000) -> float:
    """EXP-0001's Result 8: E_pi[Phi((delta sqrt N - z sigma_u)/sigma_s)], infeasible draws 0."""
    h, n = pilot_hits(g)
    pi = rng.beta(h + 0.5, n - h + 0.5, size=(draws, K))
    pibar = pi.mean(1)
    a = (delta + b * pibar) / (1 - pibar)
    dfl = (1 - pi) * a[:, None] - pi * b
    su2 = delta + 2 * b * pibar - delta**2
    ss2 = su2 - ((dfl - delta) ** 2).mean(1)
    nn = K * m
    p = stats.norm.cdf((delta * math.sqrt(nn) - Z_SHIP * np.sqrt(su2)) / np.sqrt(ss2))
    return float(np.where(a <= 1, p, 0.0).mean())


def required_m(curve: dict[int, float]) -> int | None:
    """EXP-0001's (E7): the smallest grid m from which the power stays at least 0.8."""
    ms = sorted(curve)
    for i, m in enumerate(ms):
        if all(curve[x] >= 0.8 for x in ms[i:]):
            return m
    return None


# ---- 1. symbolic and exact --------------------------------------------------------------------


def symbolic() -> None:
    t, x, n = sp.symbols("t x n", positive=True)
    kern = t ** sp.Rational(-1, 2) * (1 - t) ** sp.Rational(-1, 2) * t**x * (1 - t) ** (n - x)
    post = t ** (x + sp.Rational(1, 2) - 1) * (1 - t) ** (n - x + sp.Rational(1, 2) - 1)
    check("R1(a) Beta(1/2,1/2) prior x binomial kernel = Beta(x+1/2, n-x+1/2) kernel", zero(sp.powsimp(kern / post, force=True) - 1))
    # (b) the likelihood ratio of Beta(x+3/2, n-x-1/2) to Beta(x+1/2, n-x+1/2) is t/(1-t) times a constant
    ratio = sp.log(post.subs(x, x + 1) / post)
    check("R1(b) d/dt log density ratio (x+1 against x) = 1/t + 1/(1-t) > 0",
          zero(sp.diff(sp.expand_log(ratio, force=True), t) - (1 / t + 1 / (1 - t))))
    ends = [jeffreys(k, 28) for k in range(29)]
    check("R1(b) both ends rise strictly in x at n = 28 (exact quantiles)",
          all(ends[k + 1][0] > ends[k][0] and ends[k + 1][1] > ends[k][1] for k in range(28)))
    cls = [classify(k, 28) for k in range(29)]
    first_above = cls.index("above")
    check("R1(c) n = 28: below iff x = 0 (upper 0.0851), above iff x >= 7",
          cls[0] == "below" and all(c != "below" for c in cls[1:]) and first_above == 7
          and all(c == "above" for c in cls[7:]) and near(0.0851, ends[0][1], 4),
          f"upper(0) {ends[0][1]:.6f}, upper(1) {ends[1][1]:.4f}, lower(6) {ends[6][0]:.4f}, lower(7) {ends[7][0]:.4f}")

    # Result 2: the binomial is stochastically increasing in b
    b = sp.symbols("b", positive=True)
    nn, kk = 28, 7
    tail = sum(sp.binomial(nn, j) * b**j * (1 - b) ** (nn - j) for j in range(kk, nn + 1))
    check("R2 d/db P(X >= 7) = 28 C(27, 6) b^6 (1-b)^21 (n = 28)",
          zero(sp.expand(sp.diff(tail, b) - nn * sp.binomial(nn - 1, kk - 1) * b ** (kk - 1) * (1 - b) ** (nn - kk))))
    table = {0.0: (1.00, 0, 0), 0.02: (0.57, 0.43, 0.00), 0.05: (0.24, 0.76, 0.00), 0.10: (0.05, 0.93, 0.02),
             0.20: (0.00, 0.68, 0.32), 0.30: (0.00, 0.22, 0.78)}
    ok = True
    for bv, (qb, qs, qa) in table.items():
        d = design(28, bv)
        ok &= near(qb, d["below"], 2) and near(qs, d["straddles"], 2) and near(qa, d["above"], 2)
        ok &= abs(d["below"] - (1 - bv) ** 28) < 1e-12
    check("R2 section 6's table: 18 classification probabilities at n = 28, and P(below) = (1-b)^28", ok)
    check("R3 at b = 0.10 'below' is 0.0523 = 0.9^28 (above the nominal 0.025); 'above' 0.0179",
          near(0.0523, 0.9**28, 4) and near(0.0179, design(28, 0.10)["above"], 4), f"{design(28, 0.10)['above']:.5f}")
    worst_above = max(design(28, bv)["above"] for bv in np.linspace(0, 0.10, 101))
    worst_below = max(design(28, bv)["below"] for bv in np.linspace(0.10, 1, 181))
    check("R3 the errors peak at the boundary b = 0.10 (grids on either side)",
          abs(worst_above - design(28, 0.10)["above"]) < 1e-12 and abs(worst_below - 0.9**28) < 1e-12)

    # Result 3: the other sizes
    check("R3 pilot alone (n = 19): no loss gives an upper end of 0.1223 (Finding 3: quoted 0.124), so 'below' is impossible",
          near(0.1223, jeffreys(0, 19)[1], 4) and not near(0.124, jeffreys(0, 19)[1], 3) and design(19, 0.0)["below"] == 0.0,
          f"{jeffreys(0, 19)[1]:.5f}")
    nmin = next(k for k in range(1, 200) if jeffreys(0, k)[1] < B0)
    check("R3 the smallest n with a possible 'below' is 24", nmin == 24, f"{nmin}: {jeffreys(0, 24)[1]:.4f}")
    check("R3 n = 40: one loss straddles; P(below) at b = 0.02 is 0.45 against 0.57 at n = 28",
          classify(1, 40) == "straddles" and classify(0, 40) == "below"
          and near(0.45, design(40, 0.02)["below"], 2) and near(0.57, design(28, 0.02)["below"], 2))


# ---- 2. simulation: the value gate ------------------------------------------------------------


def exp1_seed(name: str) -> int:
    """`rematch_lab.experiments.seed` in EXP-0001's namespace."""
    return int.from_bytes(hashlib.sha256(f"EXP-0001/{name}".encode()).digest()[:4], "big")


def simulation() -> dict:
    out = {}
    rng = np.random.default_rng(20261128)
    up = jeffreys(0, 28)[1]
    # The constants are EXP-0001's.
    check("R4 the grids and budget are EXP-0001's: 20 sizes, 10 changes, 9,000 games, 22 a flaw at 80 and 14 at 120",
          sorted({r["players_per_flaw"] for r in E1["power_recall"]}) == M_GRID
          and [c["delta"] for c in E1["at_budget"][1]["curve"]] == DELTA_GRID
          and E1["budgets"]["full_games"] == BUDGET_GAMES and E1["budgets"]["full_players_per_flaw"] == {"80": 22, "120": 14})
    # Exact reproduction of EXP-0001's committed curves: this implementation, fed EXP-0001's own
    # streams (SeedSequence(seed("EXP-0001", "power/recall")).spawn, cells in its order), must give
    # its committed ship shares draw for draw.
    cells = [(g, b, m) for g in (30, 45, 60, 80, 120) for b in (0.0, 0.05, 0.10) for m in M_GRID]
    streams = np.random.SeedSequence(exp1_seed("power/recall")).spawn(len(cells))
    committed = {(r["games"], r["churn"], r["players_per_flaw"]): r["ship"] for r in E1["power_recall"]}
    bad, n_done = 0, 0
    for (g, b, m), ss in zip(cells, streams, strict=True):
        if g not in (80, 120):
            continue
        mine = power(np.random.default_rng(ss), g, m, DELTA, b, 4000)
        bad += abs(mine - committed[(g, b, m)]) > 1e-9
        n_done += 1
    check("R4 this implementation reproduces EXP-0001's committed curves exactly from its seeds (80 and 120 games, 3 churns, 20 sizes)",
          bad == 0 and n_done == 120, f"{bad} of {n_done} differ")
    mine = power(rng, 80, 22, DELTA, 0.10, 8000)
    check("R4 and EXP-0001's 22-a-flaw power at b = 0.10, 0.525, on fresh draws (0.525, within 3 SE)",
          abs(mine - E1["at_budget"][1]["ship_power_delta_0_1"]) < 3 * math.sqrt(0.25 * (1 / 8000 + 1 / 4000)) and near(0.525, mine, 3),
          f"{mine:.4f}")

    # Result 5: at the most favourable outcome's upper end, the curves at both game counts.
    curves = {g: {m: power(rng, g, m, DELTA, up, 4000) for m in M_GRID} for g in (80, 120)}
    req = {g: required_m(curves[g]) for g in curves}
    games = {g: (K * req[g] + CLEAN) * g for g in req}
    check("R5 at b = 0.0851 EXP-0001's rule needs 40 a flaw at 80 games (16,160 games) and 40 at 120 (24,240)",
          req == {80: 40, 120: 40} and games == {80: 16_160, 120: 24_240})
    check("R5 at 4,000 suites: 0.54 at 20 a flaw, 0.75 at 32, 0.85 at 40 (80 games); 0.32 at 14 a flaw and 120 games",
          near(0.54, curves[80][20], 2) and near(0.75, curves[80][32], 2) and near(0.85, curves[80][40], 2) and near(0.32, curves[120][14], 2),
          f"{curves[80][20]:.4f}, {curves[80][32]:.4f}, {curves[80][40]:.4f}, {curves[120][14]:.4f}")
    at = {m: power(rng, 80, m, DELTA, up, 8000) for m in (22, 32, 40)}
    check("R5 at 8,000 suites: 0.571, 0.749, 0.856 at 22, 32, 40; the review's 0.56, 0.76, 0.85 within 3 SE of its 2,000 draws",
          all(near(q, at[m], 3) for m, q in ((22, 0.571), (32, 0.749), (40, 0.856)))
          and all(abs(at[m] - q) <= 0.005 + 3 * math.sqrt(at[m] * (1 - at[m]) * (1 / 8000 + 1 / 2000)) for m, q in ((22, 0.56), (32, 0.76), (40, 0.85))),
          ", ".join(f"{m}: {v:.4f}" for m, v in at.items()))
    a120 = power(rng, 120, 14, DELTA, up, 8000)
    check("R5 budget-bound: neither fits 9,000 games; the budget suites 80/22 (0.571) and 120/14 (0.32) give 80 games, 22 a flaw, as suite.rs has",
          all(v > BUDGET_GAMES for v in games.values()) and (K * 22 + CLEAN) * 80 <= BUDGET_GAMES < (K * 23 + CLEAN) * 80
          and a120 < at[22] - 0.01 and re.search(r"pub const FULL_PER_FLAW: usize = 22;", SUITE_RS) is not None, f"{a120:.3f}")
    se4 = math.sqrt(0.25 / 4000)
    check("R5 the margin: 0.54 at 20 a flaw is about 33 standard errors (4,000 suites) below 0.8",
          near(33, (0.8 - curves[80][20]) / se4, -1), f"{(0.8 - curves[80][20]) / se4:.1f}")
    mde_curve = {d: power(rng, 80, 22, d, up, 4000) for d in DELTA_GRID}
    mde = next(d for d in DELTA_GRID if mde_curve[d] >= 0.8)
    check("R5 the budget suite's detectable change at b = 0.0851 is 0.15 (0.58 at 0.10, 0.88 at 0.15), as EXP-0001 registered at 0.10",
          mde == 0.15 == E1["at_budget"][1]["mde_ship"] and near(0.58, mde_curve[0.10], 2) and near(0.88, mde_curve[0.15], 2),
          f"0.10: {mde_curve[0.10]:.3f}, 0.15: {mde_curve[0.15]:.3f}")
    ap = approx_power(np.random.default_rng(4), 80, 22, DELTA, up)
    check("R4 EXP-0001's normal approximation at 22 a flaw, b = 0.0851: 0.560", near(0.560, ap, 3), f"{ap:.4f}")
    lo_above = jeffreys(7, 28)[0]
    mde7 = {d: power(np.random.default_rng(7), 80, 22, d, lo_above, 40_000) for d in (0.10, 0.15, 0.20)}
    check("R5 'above' at x = 7 (lower end 0.119): 22 a flaw ships +0.1 with 0.47; at +0.15 it sits on 0.80 (Finding 4)",
          lo_above > B0 and near(0.47, mde7[0.10], 2) and abs(mde7[0.15] - 0.80) < 0.01 and mde7[0.20] > 0.8,
          ", ".join(f"{d}: {v:.4f}" for d, v in mde7.items()))
    check("R5 'above' is rare under the expected churn: at most 0.018 for b <= 0.10 (Result 3)", design(28, 0.10)["above"] < 0.018)

    # Result 6: what would move the suites. The power of +0.1 at 80 games as the churn falls, at
    # 20 a flaw (the largest grid size within the budget) and at 22 (the budget's own size, off
    # the grid), with common random numbers and 40,000 suites a point.
    fine = [round(0.018 + 0.001 * i, 3) for i in range(15)]
    star, slope = {}, {}
    for mm in (20, 22):
        pw = {bv: power(np.random.default_rng(6), 80, mm, DELTA, bv, 40_000) for bv in fine}
        vals = [pw[bv] for bv in fine]
        check(f"R6 the assurance at {mm} a flaw does not rise as the churn rises (15 points, common random numbers)",
              all(b_ <= a_ + 1e-12 for a_, b_ in zip(vals, vals[1:], strict=False)))
        lo_b = max(bv for bv in fine if pw[bv] >= 0.8)
        hi_b = min(bv for bv in fine if pw[bv] < 0.8)
        star[mm] = lo_b + (pw[lo_b] - 0.8) / (pw[lo_b] - pw[hi_b]) * (hi_b - lo_b)
        slope[mm] = (vals[0] - vals[-1]) / (fine[-1] - fine[0])
    hits = {mm: next(k for k in range(24, 400) if jeffreys(0, k)[1] <= star[mm]) for mm in star}
    ups = [jeffreys(0, k)[1] for k in range(24, 401)]
    check("R6 J_U(0, n) falls strictly in n (24 to 400)", all(b_ < a_ for a_, b_ in zip(ups, ups[1:], strict=False)))
    check("R6 Finding 2: on EXP-0001's grid (20 a flaw) a resize needs a churn upper end below 0.0222, about 112 hits with none lost",
          abs(star[20] - 0.0222) <= 0.001 and abs(hits[20] - 112) <= 3, f"b* {star[20]:.4f}, {hits[20]} hits")
    check("R6 the review's 'near 0.03' is where 22 a flaw reaches 0.8 (0.0303, about 82 hits): an MDE change, not a resize",
          abs(star[22] - 0.0303) <= 0.001 and abs(hits[22] - 82) <= 2, f"b* {star[22]:.4f}, {hits[22]} hits")
    check("R6 slope about 0.0064 a 0.001 of churn near b*; 40,000-suite SE 0.002; so b* good to about 0.0003",
          near(6.4, slope[20], 1) and near(0.002, math.sqrt(0.16 / 40_000), 3) and near(0.0003, 0.002 / slope[20], 4),
          f"slope {slope[20]:.2f}")
    se_reg = math.sqrt(0.16 / 4000) / slope[20]
    span = [next(k for k in range(24, 400) if jeffreys(0, k)[1] <= star[20] + d * se_reg) for d in (2, -2)]
    check("R6 at the rule's own 4,000 suites the realised threshold varies by about 0.0010, about 103 to 123 hits (2 SD)",
          near(0.0010, se_reg, 4) and span == [103, 123], f"{se_reg:.5f}, {span}")
    h80, n80 = pilot_hits(80)
    full_flawed = 5 * int(re.search(r"pub const FULL_PER_FLAW: usize = (\d+);", SUITE_RS).group(1))
    full_hits = full_flawed * h80.sum() / n80.sum() + 9
    check("R6 the full suite (110) and smoke (15) at today's recall: 78.7 hits, a no-loss upper end of 0.0312, above 0.0222",
          full_flawed == 110 and near(78.7, full_hits, 1) and near(0.0312, jeffreys(0, 79)[1], 4) and jeffreys(0, 79)[1] > star[20])
    out.update(curves=curves, req=req, star=star, hits=hits)
    return out


# ---- 3. numbers -------------------------------------------------------------------------------


def numbers() -> None:
    h80, n80 = pilot_hits(80)
    sm = E6["tier1"]["scorecards"]["smoke"]["scorecard"]
    row80 = next(r for r in E6["exploratory"]["churn"]["by_checkpoint"] if r["games"] == 80)
    check("N1 n = 28: 19 pilot hits at 80 games (EXP-0001) and 9 smoke hits (recall 0.6 of 15)",
          h80.sum() == 19 and n80.sum() == 30 and round(sm["recall_at_2"] * 15) == 9)
    check("N1 EXP-0006's run, the same players: 28 reference hits at 80 games, 0 lost, [0, 0.085] (the outcome EXP-0005 would have had)",
          row80["reference_hits"] == 28 and row80["lost"] == 0 and near(0.085, row80["churn_ci95_jeffreys"][1], 3))
    check("N2 decision: no loss of 28 gives 0.0851 (summary: 0.085)", near(0.0851, jeffreys(0, 28)[1], 4) and near(0.085, jeffreys(0, 28)[1], 3))
    check("N2 Result 1: J_U(1, 28) 0.155, J_L(6, 28) 0.095, J_L(7, 28) 0.119; J_U(0, 24) 0.098",
          near(0.155, jeffreys(1, 28)[1], 3) and near(0.095, jeffreys(6, 28)[0], 3) and near(0.119, jeffreys(7, 28)[0], 3)
          and near(0.098, jeffreys(0, 24)[1], 3))
    check("N2 Result 3: 0.446 and 0.568; 0.0523 about twice 0.025",
          near(0.446, design(40, 0.02)["below"], 3) and near(0.568, design(28, 0.02)["below"], 3) and near(2.1, 0.9**28 / 0.025, 1))
    budget = 2 * len(M_GRID) * 4000 + len(DELTA_GRID) * 4000 + 3 * 4000
    check("N3 Finding 5: section 0's itemised simulated suites sum to 212,000; the stated 172,000 is analysis.py's 'below' branch without the MDE curve",
          budget == 212_000 and 2 * len(M_GRID) * 4000 + 3 * 4000 == 172_000 and 212_000 <= 1.5 * 172_000)
    check("N3 budget: 5,120 games (3,840 + 1,280); n = 40 needs the full suite's 8,960 games",
          32 * 120 + 1280 == 5120 and (110 + 2) * 80 == 8960)
    req = E1["required_players_per_flaw"]
    check("N4 section 1: the churn moved 14 to 40 a flaw at 80 games (EXP-0001)", req["80/0.0"] == 14 and req["80/0.1"] == 40)
    check("N4 Finding 1: section 5's 'halves' is 28 against 40 (0.70), or half of the churn's extra 26 players (0.54)",
          req["80/0.05"] == 28 and near(0.70, 28 / 40, 2) and near(0.54, (28 - 14) / (40 - 14), 2))
    pub = {22: E1["at_budget"][1]["ship_power_delta_0_1"],
           32: next(r["ship"] for r in E1["power_recall"] if (r["games"], r["churn"], r["players_per_flaw"]) == (80, 0.1, 32)),
           40: next(r["ship"] for r in E1["power_recall"] if (r["games"], r["churn"], r["players_per_flaw"]) == (80, 0.1, 40))}
    check("N4 Finding 3: the review's 'published' 0.51 and 0.82 at 22 and 40 are not EXP-0001's 0.525 and 0.826 (0.71 at 32 is)",
          near(0.525, pub[22], 3) and near(0.706, pub[32], 3) and near(0.826, pub[40], 3)
          and not near(0.51, pub[22], 2) and not near(0.82, pub[40], 2) and near(0.71, pub[32], 2))
    b0_120 = next(r["ship"] for r in E1["power_recall"] if (r["games"], r["churn"], r["players_per_flaw"]) == (120, 0.0, 14))
    check("N4 at 120 games the budget's 14 a flaw has 0.784 even at no churn (EXP-0001), 2.6 SE below 0.8",
          near(0.784, b0_120, 3) and near(2.6, (0.8 - b0_120) / math.sqrt(0.16 / 4000), 1))
    check("N5 decision compute: none ran (no results.json)", not (HERE / "results.json").exists())


if __name__ == "__main__":
    symbolic()
    simulation()
    numbers()
    for result, theorem in LEAN.items():
        print(f"lean: {result} is {theorem} (lab/lean/)")
    print(f"{'FAILED' if FAILURES else 'OK'}: {len(FAILURES)} failure(s)")
    sys.exit(1 if FAILURES else 0)
