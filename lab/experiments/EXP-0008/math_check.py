"""EXP-0008: the maths, checked (ADR 0118; math.md in this folder).

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy, prefix R): the identities of math.md section 4 as `simplify(lhs - rhs) == 0`,
   or by exhaustive enumeration where the statement is about finitely many labels.
2. Simulation (numpy, fixed seeds, prefix S): games resampled from the committed per-game totals
   (the ratio, its variance and the t interval, Results 2 to 5), the chi-square factor (Result 6),
   section 6's power simulation rewritten from its text (Results 7 and 8), the fixed sequence
   (Result 9), a reference that shares the deeper candidate's errors (Result 10) and the
   stratified absolute measure (Result 11).
3. Numbers (prefix N): every number decision.md, summary.md and the preregistration quote,
   recomputed from population.csv, the samples, pilot/pilot.json, results.json and power.json.
   The per-game totals in results.json let every deciding contrast be recomputed from scratch.

The formulas are written here from math.md section 3, not imported from analysis.py.
Standard library, numpy, scipy and sympy only. About 20 s of CPU.
"""

from __future__ import annotations

import csv
import hashlib
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
PILOT = json.loads((HERE / "pilot" / "pilot.json").read_text(encoding="utf-8"))


def read_csv(name):
    with (HERE / name).open(encoding="utf-8", newline="") as fh:
        return list(csv.DictReader(fh))


POP = read_csv("population.csv")
SAMPLE = read_csv("sample.csv")
SAMPLE_PILOT = read_csv("sample-pilot.csv")
EXP = "EXP-0008"
DELTA = 0.01
LABELS = ("none", "inaccuracy", "mistake", "blunder")
STEP_DOWN_GAMES = DEEP_GAMES = 40
# Theorem names in lab/lean/Rematch/ (Multiplicity.lean) whose hypotheses match Result 9.
LEAN = {"Result 9 (the union of the two wrong moves)": "Rematch.union_bound"}

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
    return abs(quoted - value) <= 0.5 * 10.0 ** (-dp) + 1e-9


def seed(name: str) -> int:
    """rematch_lab.experiments.seed, written out: the first 4 bytes of sha256("EXP/name")."""
    return int.from_bytes(hashlib.sha256(f"{EXP}/{name}".encode()).digest()[:4], "big")


# ---- the formulas of math.md section 3 ------------------------------------------------------


def tq(level: float, n: int) -> float:
    return float(stats.t.ppf(1 - (1 - level) / 2, max(n - 1, 1)))


def ratio(e, m, level=0.95):
    """(E2) to (E4): R-hat = sum e / sum m, SE^2 = n/(n-1) sum (e - R m)^2 / (sum m)^2, t interval."""
    e, m = np.asarray(e, float), np.asarray(m, float)
    n = len(e)
    r = e.sum() / m.sum()
    se = math.sqrt(n / (n - 1) * ((e - r * m) ** 2).sum()) / m.sum()
    t = tq(level, n)
    return r, se, (r - t * se, r + t * se)


def ratio_batch(e, m, level):
    """ratio() over the rows of (reps, n) arrays."""
    n = e.shape[1]
    r = e.sum(1) / m.sum(1)
    se = np.sqrt(n / (n - 1) * ((e - r[:, None] * m) ** 2).sum(1)) / m.sum(1)
    t = tq(level, n)
    return r, se, r - t * se, r + t * se


def totals(key, src=R):
    t = src["totals"][key]
    return np.array(t["e"], float), np.array(t["m"], float)


# ---- 1. symbolic -------------------------------------------------------------------------------


def symbolic() -> None:
    # Result 1: d_A - d_B over the 4^3 label triples (A, B, reference).
    ok = True
    for a, b, r in itertools.product(LABELS, repeat=3):
        diff = int(a != r) - int(b != r)
        ok &= (diff == 0) if a == b else (diff == int(b == r) - int(a == r) and diff in (-1, 0, 1))
        if a != b:
            ok &= diff != -1 or a == r  # -1 (A gains) only when A is the reference's label
    check("R Result 1: d_A - d_B = 0 when A and B agree, else [B = ref] - [A = ref] in {-1, 0, 1} (all 64 label triples)", ok)
    # Result 2: the residuals of the ratio sum to zero; the SE form; the delta method.
    n = sp.symbols("n", positive=True, integer=True)
    es = sp.symbols("e1:6")
    ms = sp.symbols("m1:6", positive=True)
    rh = sum(es) / sum(ms)
    ok = zero(sum(e - rh * m for e, m in zip(es, ms)))
    resid = [e - rh * m for e, m in zip(es, ms)]
    nn = 5
    lhs = sp.Rational(nn, nn - 1) * sum(x**2 for x in resid) / sum(ms) ** 2
    mbar = sum(ms) / nn
    s2 = sum(x**2 for x in resid) / (nn - 1)
    ok &= zero(lhs - s2 / (nn * mbar**2))
    check("R Result 2: sum (e_g - R-hat m_g) = 0 exactly, and n/(n-1) sum r_g^2/(sum m)^2 = s_r^2/(n m-bar^2) (n = 5, symbolic)", ok)
    mu_e, mu_m, s_ee, s_mm, s_em = sp.symbols("mu_e mu_m s_ee s_mm s_em", positive=True)
    grad = sp.Matrix([1 / mu_m, -mu_e / mu_m**2])
    cov = sp.Matrix([[s_ee, s_em], [s_em, s_mm]])
    delta = (grad.T * cov * grad)[0] / n
    Rr = mu_e / mu_m
    want = (s_ee - 2 * Rr * s_em + Rr**2 * s_mm) / (n * mu_m**2)
    check("R Result 2: the delta method's variance of e-bar/m-bar is Var(e - R m)/(n mu_m^2)", zero(delta - want))
    # Result 3: the bias identity E[R-hat] - R = -Cov(R-hat, m-bar)/mu_m given E[R-hat m-bar] = mu_e.
    ER, ERM = sp.symbols("E_R E_Rm")
    expr = (ER - Rr) - (-(ERM - ER * mu_m) / mu_m)
    check("R Result 3 (definitional; S Result 3 tests the premise): E[R-hat] - R = -Cov(R-hat, m-bar)/mu_m", zero(expr.subs(ERM, mu_e)))
    # Result 4: the lower end above delta <=> (R-hat - delta)/SE > t; at R = delta the one-sided
    # error of a 1 - a two-sided t interval is a/2 when (R-hat - R)/SE ~ t_{n-1}.
    ok = all(near(a / 2, float(stats.t.sf(tq(1 - a, k), k - 1)), 12) for a in (0.05, 0.01, 0.2) for k in (20, 40, 160))
    check("R Result 4 (definitional: the quantile's definition): P(T_{n-1} > t_{1-a/2, n-1}) = a/2 for a = 0.05, 0.01, 0.20 and n = 20, 40, 160", ok)
    # Result 6: the chi-square factor k = sqrt((n-1)/chi2_{0.20, n-1}) and its coverage 0.80.
    k20 = math.sqrt(19 / stats.chi2.ppf(0.20, 19))
    cover = float(stats.chi2.sf(stats.chi2.ppf(0.20, 19), 19))
    sig, s, q = sp.symbols("sigma s q", positive=True)
    # sigma <= k s  <=>  (n-1) s^2 / sigma^2 >= q, with k^2 = (n-1)/q
    kk = sp.sqrt((n - 1) / q)
    ok = zero(((n - 1) * s**2 / (kk * s) ** 2) - q) and near(1.17697, k20, 5) and near(0.80, cover, 12)
    ok &= near(PJ["k_upper80"], k20, 12)
    check("R Result 6: sigma <= k s iff (n-1)s^2/sigma^2 >= chi2_{0.20}; k = 1.17697 at n = 20 = power.json k_upper80; coverage 0.80", ok, f"k {k20:.5f}")
    # Result 7: the simulation's shift keeps the ratio at the target and scales the residuals by k.
    th, kv = sp.symbols("theta k", positive=True)
    rp = sum(es) / sum(ms)
    a_ = [th * m + kv * (e - rp * m) for e, m in zip(es, ms)]
    ok = zero(sum(a_) / sum(ms) - th)
    ok &= all(zero((ai - th * m) - kv * (e - rp * m)) for ai, e, m in zip(a_, es, ms))
    check("R Result 7: a_g = theta m_g + k (e_g - r m_g) has sum a / sum m = theta and residuals k (e_g - r m_g) (n = 5)", ok)
    # Result 10: the proximity identity over the 4^4 (A, B, Ref, T) label tuples.
    ok = True
    for a, b, rf, t in itertools.product(LABELS, repeat=4):
        lhs_ = (int(a != rf) - int(b != rf)) - (int(a != t) - int(b != t))
        rhs_ = (int(b == rf) - int(b == t)) - (int(a == rf) - int(a == t))
        ok &= lhs_ == rhs_
    check("R Result 10: (d_A - d_B)_ref - (d_A - d_B)_truth = ([B=ref] - [B=T]) - ([A=ref] - [A=T]) (all 256 label tuples)", ok)
    # Result 11: the absolute difference of two settings equals their exact contrast when every
    # unreferenced move is one where they agree (a sampled concordant move adds 0 to both).
    ys1, ys2, w = sp.symbols("y1 y2 w", nonnegative=True)
    check("R Result 11 (definitional): w (y_A - y_B) = 0 on a move where A and B agree (y_A = y_B), whatever w",
          zero((w * ys1 - w * ys2).subs(ys2, ys1)))
    # Result 12: the cost model is linear in n with the slope and intercept of power.json.
    P = PJ
    rate = P["cpu_hours_per_position"]
    slope = P["positions_per_game"] * sum(rate[s] for s in ("n50k", "n100k", "n300k", "n1m")) + P["reference_positions_per_game_step_up"] * rate["ref"]
    icpt = STEP_DOWN_GAMES * P["reference_positions_per_game_step_down_extra"] * rate["ref"] + 2 * 100 * rate["ref"]
    ok = all(abs(P["planned_cpu_hours"][str(nv)] - (slope * nv + icpt)) < 1e-9 for nv in (40, 60, 80, 100, 120, 140, 160, 180, 200, 240, 300, 400))
    ok &= abs(P["branch_cpu_hours"] - DEEP_GAMES * P["reference_positions_per_game_step_up"] * rate["deep"]) < 1e-9
    check("R Result 12: planned CPU-hours = slope n + intercept at all 12 grid sizes; branch = 40 x 6.45 x the 100M rate", ok,
          f"slope {slope:.5f} h a game, intercept {icpt:.3f} h")


# ---- 2. simulation -----------------------------------------------------------------------------


def simulation() -> None:
    sim_ratio()
    sim_chi2()
    sim_power()
    sim_sequence()
    sim_proximity()
    sim_absolute()


def sim_ratio(seed_: int = 20261128, reps: int = 40_000) -> None:
    """Games drawn with replacement from the 160 committed (e, m) totals of the step up: a
    superpopulation whose ratio is known exactly."""
    rng = np.random.default_rng(seed_)
    e0, m0 = totals("n300k-n1m")
    truth = e0.sum() / m0.sum()
    for n in (40, 160):
        idx = rng.integers(0, len(e0), size=(reps, n))
        e, m = e0[idx], m0[idx]
        r, se, lo, hi = ratio_batch(e, m, 0.95)
        mbar = m.mean(1)
        # Result 3: E[R-hat m-bar] = mu_e and |bias| <= SD(R-hat) CV(m-bar).
        rm = r * mbar
        ok = abs(rm.mean() - e0.mean()) < 4 * rm.std(ddof=1) / math.sqrt(reps)
        bias = r.mean() - truth
        hr = r.std(ddof=1) * mbar.std(ddof=1) / m0.mean()
        ok &= abs(bias) <= hr + 4 * r.std(ddof=1) / math.sqrt(reps)
        check(f"S Result 3 (n = {n}; the identity part is definitional): E[R-hat m-bar] = mu_e within 4 SE; |bias| {abs(bias):.6f} <= SD x CV {hr:.6f}", bool(ok))
        # Result 2: E[SE^2] against Var(R-hat), first order (within 10%).
        rv = float((se**2).mean() / r.var(ddof=1))
        check(f"S Result 2 (n = {n}): E[SE^2]/Var(R-hat) = {rv:.3f}, within 0.90 to 1.10", 0.90 < rv < 1.10)
        # Result 5: the t interval's coverage and one-sided misses on these skewed totals.
        cov = float(np.mean((lo <= truth) & (truth <= hi)))
        lo_miss = float(np.mean(lo > truth))
        hi_miss = float(np.mean(hi < truth))
        want = {40: (0.946, 0.0205, 0.034), 160: (0.949, 0.0208, 0.030)}[n]
        check(f"S Result 5 (n = {n}): 95% t coverage {cov:.4f}, lower-end miss {lo_miss:.4f} < 0.025 < upper-end miss {hi_miss:.4f} (quoted {want})",
              near(want[0], cov, 3) and near(want[1], lo_miss, 4) and near(want[2], hi_miss, 3) and lo_miss < 0.025 < hi_miss)


def sim_chi2(seed_: int = 6, reps: int = 200_000) -> None:
    rng = np.random.default_rng(seed_)
    n = 20
    k = math.sqrt((n - 1) / stats.chi2.ppf(0.20, n - 1))
    x = rng.normal(0, 1, size=(reps, n))
    s = x.std(1, ddof=1)
    cov = float(np.mean(1.0 <= k * s))
    check("S Result 6: with normal residuals, P(sigma <= k s) = 0.80 within 4 SE (n = 20)", abs(cov - 0.80) < 4 * math.sqrt(0.16 / reps), f"{cov:.4f}")
    # The pilot's own residuals (skewed, few games): resample 20 of the 20, the bound's coverage of
    # the resampling population's SD. Reported against 0.80; a heavy tail lowers it.
    e, m = totals("n100k-n300k", PILOT)
    res = e - e.sum() / m.sum() * m
    sd0 = res.std(ddof=0)
    idx = rng.integers(0, n, size=(reps, n))
    sb = res[idx].std(1, ddof=1)
    cov_p = float(np.mean(sd0 <= k * sb))
    e2, m2 = totals("n300k-n1m", PILOT)
    res2 = e2 - e2.sum() / m2.sum() * m2
    cov_q = float(np.mean(res2.std(ddof=0) <= k * res2[idx].std(1, ddof=1)))
    check("S Result 6: resampling the pilot's own residuals, the 80% factor covers 0.65 (step down) and 0.81 (step up), not 0.80",
          near(0.65, cov_p, 2) and near(0.81, cov_q, 2), f"step down {cov_p:.3f}, step up {cov_q:.3f}")


def decide(c100, g1m, deep):
    """(E5) without the 50,000 branch, which never changes the outcome reached."""
    if g1m[2] > DELTA:
        return "300k falls short" if deep[2] > DELTA else "need more data"
    if g1m[3] < DELTA and c100[5] < DELTA:
        return "100k is enough"
    if g1m[3] < DELTA and c100[2] > DELTA:
        return "300k is right"
    return "need more data"


def run_rule(e100, e1m, m, n, x, y, k, rng, sims):
    """Section 6's simulation, written from its text: n games with replacement from the pilot's,
    totals shifted to the targets and residuals scaled by k; the step down on 40 of them, the step
    up on all, its deep check on another 40 (agreeing with the 30M reference)."""
    r100 = e100.sum() / m.sum()
    r1m = e1m.sum() / m.sum()
    a = x * m + k * (e100 - r100 * m)
    b = y * m + k * (e1m - r1m * m)
    idx = rng.integers(0, len(m), size=(sims, n))
    kd = min(STEP_DOWN_GAMES, n)
    down = np.take_along_axis(idx, np.argsort(rng.random((sims, n)), 1)[:, :kd], 1)
    deep = np.take_along_axis(idx, np.argsort(rng.random((sims, n)), 1)[:, :kd], 1)
    r_, se, lo, hi = ratio_batch(a[down], m[down], 0.95)
    _, _, _, hi99 = ratio_batch(a[down], m[down], 0.99)
    g = ratio_batch(b[idx], m[idx], 0.95)
    dp = ratio_batch(b[deep], m[deep], 0.95)
    out = {}
    for i in range(sims):
        c100 = (None, None, lo[i], hi[i], None, hi99[i])
        o = decide(c100, (g[0][i], g[1][i], g[2][i], g[3][i]), (dp[0][i], dp[1][i], dp[2][i], dp[3][i]))
        out[o] = out.get(o, 0) + 1
    return {kk: v / sims for kk, v in out.items()}


def sim_power(seed_: int = 8, sims: int = 4000) -> None:
    rng = np.random.default_rng(seed_)
    e100, m = totals("n100k-n300k", PILOT)
    e1m, _ = totals("n300k-n1m", PILOT)
    k = PJ["k_upper80"]
    scen = {"100k is enough": (0.005, 0.005), "300k falls short": (0.005, 0.015), "300k is right": (0.015, 0.005),
            "boundary 100k": (0.01, 0.0), "boundary 1M": (0.0, 0.01)}
    target = {"100k is enough": "100k is enough", "300k falls short": "300k falls short", "300k is right": "300k is right",
              "boundary 100k": "100k is enough", "boundary 1M": "300k falls short"}
    worst = 0.0
    where = ""
    for var, kv in (("pilot", 1.0), ("upper80", k)):
        for n in (40, 160):
            row = next(r for r in PJ["table"] if r["variance"] == var and r["n"] == n)
            for sname, (x, y) in scen.items():
                got = run_rule(e100, e1m, m, n, x, y, kv, rng, sims).get(target[sname], 0.0)
                want = row[sname].get(target[sname], 0.0)
                z = abs(got - want) / math.sqrt(max(want * (1 - want), 1e-3) * (1 / sims + 1 / 2000))
                if z > worst:
                    worst, where = z, f"{var} n={n} {sname}"
    check("S Result 7: section 6 rewritten from its text reproduces power.json's outcome rates (2 variances x n = 40, 160 x 5 scenarios) within 4 combined SE",
          worst < 4.0, f"worst {where} at {worst:.2f} SE")
    # Result 8: P(300k falls short) is at most P(the deep check passes on 40 games), whatever n.
    a = 0.015 * m + k * (e1m - e1m.sum() / m.sum() * m)
    idx = rng.integers(0, len(m), size=(20000, 40))
    _, _, lo, _ = ratio_batch(a[idx], m[idx], 0.95)
    bound = float(np.mean(lo > DELTA))
    fs = [r["300k falls short"].get("300k falls short", 0.0) for r in PJ["table"] if r["variance"] == "upper80"]
    ok = all(f <= bound + 4 * math.sqrt(bound * (1 - bound) / 2000) for f in fs)
    a100 = 0.005 * m + k * (e100 - e100.sum() / m.sum() * m)
    _, _, _, hi99 = ratio_batch(a100[idx], m[idx], 0.99)
    bound2 = float(np.mean(hi99 < DELTA))
    en = [r["100k is enough"].get("100k is enough", 0.0) for r in PJ["table"] if r["variance"] == "upper80"]
    ok &= all(f <= bound2 + 4 * math.sqrt(max(bound2 * (1 - bound2), 1e-3) / 2000) for f in en)
    check("S Result 8: at every grid size, P(falls short) <= P(deep check passes on 40) and P(100k enough) <= P(step down within on 40) (upper-bound variance)",
          ok, f"bounds {bound:.3f} and {bound2:.3f}; table max {max(fs):.3f} and {max(en):.3f}")


def sim_sequence(seed_: int = 9, reps: int = 200_000) -> None:
    """Result 9: (a) the rule (E5) with both contrasts at their boundaries: the two wrong moves are
    disjoint outcomes and their total is below 0.05; (b) a fixed sequence 100,000 then 50,000 at
    level a, with 100,000 truly within by a varying margin and 50,000 at its null, under four
    correlations: the error never exceeds a."""
    rng = np.random.default_rng(seed_)
    e100, m = totals("n100k-n300k", PILOT)
    e1m, _ = totals("n300k-n1m", PILOT)
    out = run_rule(e100, e1m, m, 160, DELTA, DELTA, PJ["k_upper80"], rng, 4000)
    wrong = out.get("100k is enough", 0.0) + out.get("300k falls short", 0.0)
    check("S Result 9: at both boundaries (n = 160, upper-bound variance) the rule's two wrong moves total at most 0.05",
          wrong <= 0.05, f"100k enough {out.get('100k is enough', 0.0):.4f} + falls short {out.get('300k falls short', 0.0):.4f}")
    a = 0.025
    zc = stats.norm.ppf(1 - a)
    worst = 0.0
    for rho in (-0.8, 0.0, 0.5, 0.95):
        for shift in (0.0, 1.0, 2.0, 4.0):
            z = rng.multivariate_normal([0, 0], [[1, rho], [rho, 1]], size=reps // 8)
            if shift == 0.0:
                err = np.mean(z[:, 0] > zc)  # 100,000 at its null: rejecting it is the error
            else:
                err = np.mean((z[:, 0] + shift > zc) & (z[:, 1] > zc))  # 50,000 at its null, tested only after 100,000
            worst = max(worst, float(err))
    check("S Result 9: the fixed sequence's error is at most 0.025 (4 correlations x 4 margins, 4 SE)",
          worst < a + 4 * math.sqrt(a / (reps // 8)), f"worst {worst:.4f}")


def sim_proximity(seed_: int = 10, moves: int = 400_000) -> None:
    """Result 10: a reference that shares more of the deeper candidate's errors inflates the step up."""
    rng = np.random.default_rng(seed_)
    T = rng.integers(0, 4, moves)

    def noisy(err):
        flip = rng.random(moves) < err
        return np.where(flip, (T + rng.integers(1, 4, moves)) % 4, T)

    A = noisy(0.12)
    B = noisy(0.09)
    # The reference copies B's label with probability 0.3 where B errs; otherwise it is T with error 0.03.
    ref = noisy(0.03)
    share = (B != T) & (rng.random(moves) < 0.3)
    ref = np.where(share, B, ref)
    c_ref = np.mean((A != ref).astype(int) - (B != ref))
    c_true = np.mean((A != T).astype(int) - (B != T))
    pi_b = np.mean(B == ref) - np.mean(B == T)
    pi_a = np.mean(A == ref) - np.mean(A == T)
    ok = abs(c_ref - c_true - (pi_b - pi_a)) < 1e-12 and c_ref > c_true
    check("S Result 10 (illustrative: the identity is algebraic and the sharing is built in): contrast against the reference = against the truth + (pi_B - pi_A)",
          bool(ok), f"against truth {c_true:.4f}, reference {c_ref:.4f}")


def sim_absolute(seed_: int = 12, reps: int = 40_000) -> None:
    """Result 11: the discordant moves in full and 100 concordant moves by SRSWOR, weighted
    N_c/100, estimate a setting's total disagreement without bias."""
    rng = np.random.default_rng(seed_)
    n_disc, n_conc = 945, 9651
    y_disc = rng.random(n_disc) < 0.5
    y_conc = rng.random(n_conc) < 0.06
    total = y_disc.sum() + y_conc.sum()
    est = np.empty(reps)
    for i in range(reps):
        pick = rng.choice(n_conc, 100, replace=False)
        est[i] = y_disc.sum() + n_conc / 100 * y_conc[pick].sum()
    se = est.std(ddof=1) / math.sqrt(reps)
    var_want = n_conc**2 * (1 - 100 / n_conc) * y_conc.var(ddof=1) / 100
    check("S Result 11: E[stratified total] = true total within 4 SE, Var = N^2 (1-f) S^2/n within 5%",
          abs(est.mean() - total) < 4 * se and abs(est.var(ddof=1) / var_want - 1) < 0.05, f"{est.mean():.1f} vs {total}")


# ---- 3. numbers --------------------------------------------------------------------------------


def accepted(pop):
    rng = np.random.default_rng(seed("sample/order"))
    order = rng.permutation(len(pop))
    seen, out, dropped, drop_at = set(), [], 0, {}
    for i in order:
        g = pop[int(i)]
        pl = {g["white"].lower(), g["black"].lower()}
        if pl & seen:
            dropped += 1
            continue
        seen |= pl
        out.append(g)
        drop_at[len(out)] = dropped
    return out, dropped, drop_at


def numbers() -> None:
    # The sample, reproduced from population.csv and the seed.
    acc, dropped, drop_at = accepted(POP)
    ok = [g["game_id"] for g in acc[:20]] == [g["game_id"] for g in SAMPLE_PILOT]
    ok &= [g["game_id"] for g in acc[20:180]] == [g["game_id"] for g in SAMPLE]
    pl = [p.lower() for g in SAMPLE_PILOT + SAMPLE for p in (g["white"], g["black"])]
    ok &= len(pl) == len(set(pl)) and len(POP) == 5398 == R["population"]
    check("N sample: the seeded order of population.csv, with the player rule, gives the pilot's 20 and the sample's 160 exactly; no player twice", ok)
    check("N Finding 3: the player rule drops 2,291 of the 5,398 games over the whole order (section 7 says 120); before the 180th kept game it has dropped "
          f"{drop_at[180]}", dropped == 2291 and drop_at[180] == 5 and len(acc) == 3107 and drop_at[798] == 119 and drop_at[799] == 121, f"{drop_at[180]} dropped by the 180th kept game")
    plies = [int(g["plies"]) for g in SAMPLE]
    ok = sum(plies) == 10596 == R["moves"] and R["games"] == 160 and R["positions"] == 9997
    ok &= sum(int(g["plies"]) for g in SAMPLE_PILOT) == 1308 == PILOT["moves"] and PILOT["positions"] == 1274
    e, m = totals("n300k-n1m")
    ok &= list(m) == [float(p) for p in plies]
    check("N 160 games, 10,596 moves, 9,997 positions (pilot 20, 1,308, 1,274); results.json's totals are in sample.csv's order", ok)
    # The subsets, reproduced from their seeds.
    down = sorted(int(i) for i in np.random.default_rng(seed("step-down/confirmatory")).choice(160, 40, replace=False))
    deep = sorted(int(i) for i in np.random.default_rng(seed("deep/confirmatory")).choice(160, 40, replace=False))
    ids = [g["game_id"] for g in SAMPLE]
    ed, md = totals("n100k-n300k")
    edp, mdp = totals("n300k-n1m-deep")
    ok = sorted(ids[i] for i in down) == R["step_down_games"] and list(md) == [m[i] for i in down] and list(mdp) == [m[i] for i in deep]
    check("N subsets: seeds step-down/confirmatory and deep/confirmatory give the 40 step-down games and the 40 deep games (moves match)", ok,
          f"deep moves {int(mdp.sum())}")
    # The deciding contrasts, recomputed from the per-game totals.
    def same(key, src, got):
        q = src["primary"][key]
        return all(abs(a - b) < 1e-12 for a, b in ((q["estimate"], got[0]), (q["se"], got[1]), (q["ci95"][0], got[2][0]), (q["ci95"][1], got[2][1])))
    ok = True
    for key in ("n300k-n1m", "n100k-n300k", "n300k-n1m-deep"):
        e_, m_ = totals(key)
        for lv, nm in ((0.80, "ci80"), (0.95, "ci95"), (0.99, "ci99")):
            r_, se_, ci = ratio(e_, m_, lv)
            ok &= abs(R["primary"][key][nm][0] - ci[0]) < 1e-12 and abs(R["primary"][key][nm][1] - ci[1]) < 1e-12
        ok &= same(key, R, ratio(e_, m_))
    for key in ("n100k-n300k", "n300k-n1m", "n300k-n1m-deep", "n50k-n300k"):
        e_, m_ = totals(key, PILOT)
        ok &= same(key, PILOT, ratio(e_, m_))
    check("N every deciding contrast (run and pilot), its SE and its 80/95/99% intervals, recomputed from the per-game totals to 1e-12", ok)
    up = ratio(*totals("n300k-n1m"))
    up80 = ratio(*totals("n300k-n1m"), 0.80)
    dp = ratio(*totals("n300k-n1m-deep"))
    dp80 = ratio(*totals("n300k-n1m-deep"), 0.80)
    dn = ratio(*totals("n100k-n300k"))
    dn99 = ratio(*totals("n100k-n300k"), 0.99)
    ok = near(0.0246, up[0], 4) and near(0.0197, up[2][0], 4) and near(0.0296, up[2][1], 4) and near(0.0214, up80[2][0], 4) and near(0.0279, up80[2][1], 4)
    ok &= near(0.0221, dp[0], 4) and near(0.0115, dp[2][0], 4) and near(0.0327, dp[2][1], 4) and near(0.0153, dp80[2][0], 4) and near(0.0289, dp80[2][1], 4)
    ok &= near(0.0122, dn[0], 4) and near(0.0014, dn[2][0], 4) and near(0.0230, dn[2][1], 4) and near(-0.0022, dn99[2][0], 4) and near(0.0267, dn99[2][1], 4)
    check("N decision: 0.0246 [0.0197, 0.0296] (80% [0.0214, 0.0279]); deep 0.0221 [0.0115, 0.0327] (80% [0.0153, 0.0289]); step down 0.0122 [0.0014, 0.0230], 99% [-0.0022, 0.0267]", ok)
    ok = near(2.5, 100 * up[0], 1) and near(2.0, 100 * up[2][0], 1) and near(3.0, 100 * up[2][1], 1) and near(2.2, 100 * dp[0], 1)
    ok &= near(3.3, 100 * dp[2][1], 1)
    check("N summary: 2.5 in 100 [2.0, 3.0]; 2.2 [., 3.3]", ok)
    check("N Finding 2: the summary's lower end 1.2 in 100 is 1.149, which rounds to 1.1 (decision.md's 0.0115 is right to 4 dp)",
          near(1.1, 100 * dp[2][0], 1) and not near(1.2, 100 * dp[2][0], 1) and near(0.0115, dp[2][0], 4), f"{100 * dp[2][0]:.4f}")
    # The decision rule, recomputed.
    outcome = "300k falls short" if up[2][0] > DELTA and dp[2][0] > DELTA else "other"
    check("N decision: rule 1 holds (both lower ends above 0.01), so 300k falls short and the setting is 1 million; the deep margin is 0.0015",
          outcome == R["decision"]["outcome"] and R["decision"]["setting"] == "n1m" and near(0.0015, dp[2][0] - DELTA, 4), f"{dp[2][0] - DELTA:.5f}")
    check("N decision: the step down is neither shown within (99% upper 0.0267 >= 0.01) nor shown worse (95% lower 0.0014 <= 0.01)",
          dn99[2][1] >= DELTA and dn[2][0] <= DELTA)
    # The same 40 deep games against the 30M reference: 0.0252, and 12% smaller against 100M.
    e30 = np.array([e[i] for i in deep])
    m30 = np.array([m[i] for i in deep])
    r30 = e30.sum() / m30.sum()
    shrink = 1 - dp[0] / r30
    q = R["primary"]["n300k-n1m-deep"]
    check("N decision: on the deep 40 games the step up is 0.0252 against 30M (2,624 moves) and 0.0221 against 100M, 12% smaller; 12.0% of the 192 moves changed label",
          near(0.0252, r30, 4) and int(m30.sum()) == 2624 and near(0.12, shrink, 2) and q["moves"] == 192 and near(12.0, 100 * q["reference_label_changed"], 1),
          f"{r30:.5f}, shrink {shrink:.4f}")
    # Result 10: the paired difference on the deep 40, (E30 - E100)/M, with (E3) and a t interval.
    pd = ratio(e30 - edp, m30)
    check("N Result 10: the step up's paired change from 30M to 100M on the deep 40 is 0.0030 (8/2,624), SE 0.0025, 95% [-0.0021, 0.0082]: not distinguishable from 0",
          int((e30 - edp).sum()) == 8 and near(0.0030, pd[0], 4) and near(0.0025, pd[1], 4) and near(-0.0021, pd[2][0], 4) and near(0.0082, pd[2][1], 4) and pd[2][0] < 0 < pd[2][1],
          f"{pd[0]:.5f} SE {pd[1]:.5f} [{pd[2][0]:.5f}, {pd[2][1]:.5f}]")
    # By band, recomputed from sample.csv's mean ratings.
    bands = (("800-1199", 800, 1200), ("1200-1599", 1200, 1600), ("1600-1999", 1600, 2000), ("2000-2199", 2000, 2200))
    quoted = {"800-1199": (0.034, 0.024, 0.044, 47), "1200-1599": (0.019, 0.010, 0.028, 59), "1600-1999": (0.024, 0.016, 0.032, 44), "2000-2199": (0.023, 0.006, 0.039, 10)}
    ok = True
    for name, lo_, hi_ in bands:
        sel = [i for i, g in enumerate(SAMPLE) if lo_ <= float(g["mean_elo"]) < hi_]
        rb = ratio(e[sel], m[sel])
        qb = quoted[name]
        ok &= len(sel) == qb[3] == R["by_band"][name]["games"] and near(qb[0], rb[0], 3) and near(qb[1], rb[2][0], 3) and near(qb[2], rb[2][1], 3)
        ok &= abs(R["by_band"][name]["n300k-n1m"]["estimate"] - rb[0]) < 1e-12 and rb[2][0] > 0
    check("N by band: 0.034 [0.024, 0.044] (47), 0.019 [0.010, 0.028] (59), 0.024 [0.016, 0.032] (44), 0.023 [0.006, 0.039] (10), all above zero, recomputed", ok)
    # Result 11: the absolute 300k minus 1M equals the exact contrast.
    ab = R["absolute_disagreement"]
    diff = ab["n300k"]["estimate"] - ab["n1m"]["estimate"]
    check("N Result 11: absolute D(300k) - D(1M) = 0.024632 = the exact contrast 261/10,596", abs(diff - up[0]) < 1e-12 and near(261 / 10596, diff, 12), f"{diff:.6f}")
    wc = R["concordant_moves"] / R["concordant_sampled"]
    check("N the 100 concordant moves each weigh 9,651/100 = 96.5; 945 discordant moves; 945 + 9,651 = 10,596",
          near(96.5, wc, 1) and R["discordant_moves"] + R["concordant_moves"] == 10596 and R["discordant_moves"] == 945)
    # The decision's other numbers.
    wp = R["with_production"]
    ok = near(7.45, 100 * wp["n1m"]["estimate"], 2) and near(6.75, 100 * wp["n1m"]["ci95"][0], 2) and near(8.14, 100 * wp["n1m"]["ci95"][1], 2)
    ok &= near(7.4, 100 * wp["n1m"]["estimate"], 1) and near(66, 10596 / 160, 0) and near(5, wp["n1m"]["estimate"] * 10596 / 160, 0)
    ok &= near(5.3, 100 * PILOT["with_production"]["n1m"]["estimate"], 1)
    check("N decision: 7.45% [6.75%, 8.14%] of labels change (summary 7.4%), about five of 66 moves; the pilot's 5.3%", ok)
    per_pg = wp["n1m"]["estimate"] * 10596 / 320
    net = up[0] * 10596 / 320
    check("N Finding 1: 7.45% of a player's 33.1 moves is 2.47 changed labels a player and game, not 0.86; 0.86 is the contrast 0.0246 x 35 moves (0.82 at this sample's 33.1)",
          near(2.47, per_pg, 2) and near(0.86, up[0] * 35, 2) and near(0.82, net, 2), f"{per_pg:.3f}, {up[0] * 35:.3f}, {net:.3f}")
    lp = R["labels_per_player_game"]
    ok = all(near(qv, lp[s][k], 2) for s, k, qv in (("n1m", "blunder", 2.59), ("n1m", "mistake", 1.51), ("n1m", "inaccuracy", 2.75),
                                                    ("n300k", "blunder", 2.56), ("n300k", "mistake", 1.46), ("n300k", "inaccuracy", 3.04)))
    check("N labels a player and game: 2.59, 1.51, 2.75 at 1M against 2.56, 1.46, 3.04 at 300k", ok)
    ac = R["accuracy"]["n1m"]
    ok = near(-0.42, ac["mean_difference"]["estimate"], 2) and near(-0.62, ac["mean_difference"]["ci95"][0], 2) and near(-0.22, ac["mean_difference"]["ci95"][1], 2)
    ok &= near(1.06, ac["mean_absolute_difference"]["estimate"], 2) and near(0.93, ac["mean_absolute_difference"]["ci95"][0], 2) and near(1.19, ac["mean_absolute_difference"]["ci95"][1], 2)
    ok &= near(0.45, ac["share_moved_one_point"]["estimate"], 2) and near(0.39, ac["share_moved_one_point"]["ci95"][0], 2) and near(0.51, ac["share_moved_one_point"]["ci95"][1], 2)
    check("N accuracy: 0.42 lower [0.22, 0.62]; 1.06 absolute [0.93, 1.19]; 45% [39%, 51%] move a point", ok)
    bm, we = R["best_move_agreement"], R["win_percent_error"]
    ok = near(12.3, 100 * ab["n300k"]["estimate"], 1) and near(7.9, 100 * ab["n300k"]["ci95"][0], 1) and near(17.5, 100 * ab["n300k"]["ci95"][1], 1)
    ok &= near(9.8, 100 * ab["n1m"]["estimate"], 1) and near(5.7, 100 * ab["n1m"]["ci95"][0], 1) and near(14.6, 100 * ab["n1m"]["ci95"][1], 1)
    ok &= near(80.7, 100 * bm["n300k"]["estimate"], 1) and near(83.8, 100 * bm["n1m"]["estimate"], 1)
    ok &= near(2.3, we["n300k"]["estimate"], 1) and near(1.9, we["n1m"]["estimate"], 1) and near(12.6, 100 * ab["n100k"]["estimate"], 1)
    ok &= near(9.9, 100 * wp["lichess"]["estimate"], 1) and near(9.1, 100 * wp["lichess"]["ci95"][0], 1) and near(10.8, 100 * wp["lichess"]["ci95"][1], 1)
    ok &= near(12.3, 100 * ab["lichess"]["estimate"], 1) and near(7.4, 100 * ab["lichess"]["ci95"][0], 1) and near(18.0, 100 * ab["lichess"]["ci95"][1], 1)
    ok &= near(10, 100 * ab["n1m"]["estimate"], 0)
    check("N exploratory: 12.3% [7.9, 17.5] and 9.8% [5.7, 14.6]; best move 80.7%, 83.8%; win% 2.3, 1.9; 100k 12.6%; Lichess 9.9% [9.1, 10.8] and 12.3% [7.4, 18.0]", ok)
    # Cost.
    pos_pg = 9997 / 160
    ok = near(62.5, pos_pg, 1) and near(3.3, 39.1 / 11.8, 1) and near(1.0, 11.8 * 300 / 3600, 1) and near(3.3, 39.1 * 300 / 3600, 1)
    ok &= near(15, 11.8 * 300 / 60 / 4, 0) and near(49, 39.1 * 300 / 60 / 4, 0)
    check("N cost: 62.5 positions a game; 39.1/11.8 = 3.3; 300 games 1.0 and 3.3 CPU-hours, 15 and 49 minutes on 4 engines (from the quoted per-game seconds)", ok)
    rate = PJ["cpu_hours_per_position"]
    ok = near(0.20, rate["n300k"] * 3600, 2) and near(21, rate["ref"] * 3600, 0) and near(77, rate["deep"] * 3600, 0)
    ok &= near(11.6, PJ["planned_cpu_hours"]["160"], 1) and PJ["planned_cpu_hours"]["180"] > 12 >= PJ["planned_cpu_hours"]["160"] and PJ["chosen_n"] == 160
    ok &= near(5.5, PJ["branch_cpu_hours"], 1) and near(17.2, PJ["planned_cpu_hours"]["160"] + PJ["branch_cpu_hours"], 1) and PJ["powered_n"] is None
    ok &= near(63.7, PJ["positions_per_game"], 1) and near(6.45, PJ["reference_positions_per_game_step_up"], 2) and near(7.6, PJ["reference_positions_per_game_step_down_extra"], 1)
    check("N section 0/6: 0.20, 21 and 77 CPU-seconds a position; 160 games plan 11.6 CPU-hours (180 over 12); branch 5.5; worst 17.2; no size powered", ok)
    act, bud = R["compute"]["actual"], R["compute"]["budget"]
    ok = act["candidate_nodes"] == 9997 * (50_000 + 100_000 + 300_000 + 1_000_000) and act["reference_nodes"] == 1797 * 30_000_000
    ok &= act["branch_nodes"] == 336 * 100_000_000 and act["recheck_nodes"] == 12 * (50_000 + 100_000 + 300_000 + 1_000_000 + 30_000_000 + 100_000_000)
    pct = {k: act[k] / bud[k] for k in act}
    ok &= near(97, 100 * pct["candidate_nodes"], 0) and near(115, 100 * pct["reference_nodes"], 0) and near(129, 100 * pct["branch_nodes"], 0) and near(99, 100 * pct["recheck_nodes"], 0)
    ok &= all(v <= R["compute"]["cap"] for v in pct.values()) and R["reference_positions"] == 1797
    ok &= all(v == {"identical": 12, "positions": 12} for v in R["rechecks"].values()) and len(R["rechecks"]) == 6
    check("N compute: nodes = positions x node counts (9,997, 1,797, 336, 12 a setting); 97%, 115%, 129%, 99% of budget, all under 150%; 72 re-searches identical", ok)
    # Section 6's power figures.
    row = lambda var, n: next(r for r in PJ["table"] if r["variance"] == var and r["n"] == n)  # noqa: E731
    up160, pi160 = row("upper80", 160), row("pilot", 160)
    ok = near(0.20, up160["300k falls short"]["300k falls short"], 2) and near(0.30, pi160["300k falls short"]["300k falls short"], 2)
    ok &= near(0.09, up160["100k is enough"]["100k is enough"], 2) and near(0.13, pi160["100k is enough"]["100k is enough"], 2)
    ok &= near(0.09, up160["300k is right"]["300k is right"], 2) and near(0.13, pi160["300k is right"]["300k is right"], 2)
    check("N section 6: at 160 games, 0.20 (0.30), 0.09 (0.13), 0.09 (0.13)", ok)
    grid = [r for r in PJ["table"] if r["n"] in (40, 60, 80, 100, 120, 140, 160, 200, 240, 300, 400)]
    b100 = [r["boundary 100k"].get("100k is enough", 0) for r in grid]
    b1m = [r["boundary 1M"].get("300k falls short", 0) for r in grid]
    ok = max(b100 + b1m) <= 0.025 and min(b100) == 0.014 and max(b100) == 0.0245
    check("N section 6: wrong moves at the boundaries at most 0.025 at every size; step down 0.014 to 0.0245 (quoted 0.015 to 0.025)", ok,
          f"step down {min(b100)} to {max(b100)}")
    big = [r["boundary 1M"].get("300k falls short", 0) for r in grid if r["n"] >= 100]
    small = {(r["variance"], r["n"]): r["boundary 1M"].get("300k falls short", 0) for r in grid if r["n"] < 100}
    at160 = {v: row(v, 160)["boundary 1M"]["300k falls short"] for v in ("pilot", "upper80")}
    check("N Finding 4: the step up's boundary rate is 0.001 to 0.0205 over the grid (quoted 0.002 to 0.004); 0.0205 and 0.016 at 40 games, 0.005 to 0.01 at 60 and 80, 0.001 to 0.0045 from 100; 0.0035 at 160 with both variances",
          min(b1m) == 0.001 and max(b1m) == 0.0205 and small == {("pilot", 40): 0.0205, ("pilot", 60): 0.01, ("pilot", 80): 0.009, ("upper80", 40): 0.016, ("upper80", 60): 0.01, ("upper80", 80): 0.005}
          and min(big) == 0.001 and max(big) == 0.0045 and at160 == {"pilot": 0.0035, "upper80": 0.0035}, f"{small}")
    pe = PILOT["primary"]
    ok = near(0.032, pe["n100k-n300k"]["estimate"], 3) and near(0.020, pe["n100k-n300k"]["ci95"][0], 3) and near(0.045, pe["n100k-n300k"]["ci95"][1], 3)
    ok &= near(0.0099, pe["n300k-n1m"]["estimate"], 4) and near(0.0001, pe["n300k-n1m"]["ci95"][0], 4) and near(0.0197, pe["n300k-n1m"]["ci95"][1], 4)
    ok &= near(0.0099, pe["n300k-n1m-deep"]["estimate"], 4) and near(0.0002, pe["n300k-n1m-deep"]["ci95"][0], 4) and near(0.0197, pe["n300k-n1m-deep"]["ci95"][1], 4)
    ok &= near(14.5, 100 * pe["n300k-n1m-deep"]["reference_label_changed"], 1) and pe["n300k-n1m-deep"]["moves"] == 69
    e13, m13 = totals("n1m-n3m", PILOT)
    e50, m50 = totals("n50k-n300k", PILOT)
    ok &= near(0.019, e13.sum() / m13.sum(), 3) and near(0.039, e50.sum() / m50.sum(), 3)
    check("N section 6/review: pilot 0.032 [0.020, 0.045], 0.0099 [0.0001, 0.0197], deep 0.0099 [0.0002, 0.0197]; 14.5% of 69; D(1M)-D(3M) 0.019; D(50k)-D(300k) 0.039", ok)
    # Result 3's sizes: CV of the plies about 0.4; CV(m-bar) = 6% of an SE at 40 games, 3% at 160.
    cv = float(np.std(m, ddof=1) / np.mean(m))
    check("N Result 3: plies' CV is 0.47; CV(m-bar) = 0.47/sqrt(n) is 7% at 40 games and 4% at 160; chi2_{0.20,19} = 13.716, k^2 = 1.3853",
          near(0.47, cv, 2) and near(0.07, cv / math.sqrt(40), 2) and near(0.04, cv / math.sqrt(160), 2)
          and near(13.716, float(stats.chi2.ppf(0.2, 19)), 3) and near(1.3853, PJ["k_upper80"] ** 2, 4), f"CV {cv:.3f}")
    ok = near(0.195, rate["n300k"] * 3600, 3) and near(21.15, rate["ref"] * 3600, 2) and near(77.33, rate["deep"] * 3600, 2)
    ok &= near(4.93, wp["n1m"]["estimate"] * 10596 / 160, 2) and near(33.1, 10596 / 320, 1)
    check("N pilot: its own searches sum to 6.52 CPU-hours (quoted 'about 6.7' over three runs)", near(6.52, sum(PILOT["cpu_hours"].values()), 2))
    plan_ref = 160 * PJ["reference_positions_per_game_step_up"] + 40 * PJ["reference_positions_per_game_step_down_extra"] + 200
    plan = PJ["planned_cpu_hours"]["160"] + PJ["branch_cpu_hours"]
    check("N Finding 5: section 0's 1,536 reference positions = 160 x 6.45 + 40 x 7.6 + 200; x 30M = 46.1 billion, budgeted 47; plan + branch = 17.18 (section 0's 17.2, but 17.1 in section 11 and decision.md)",
          near(1536, plan_ref, 0) and near(46.1e9, plan_ref * 3e7, -8) and R["compute"]["budget"]["reference_nodes"] == 4.7e10 and near(17.2, plan, 1) and not near(17.1, plan, 1),
          f"{plan_ref:.0f} positions, {plan_ref * 3e7:.3e} nodes, {plan:.3f} h")
    ok = sum(1 for ln in (HERE / "syzygy-345.sha256").read_text(encoding="utf-8").splitlines() if ln.strip()) == 290
    ok &= near(10190, 160 * PJ["positions_per_game"], -1) and near(258, 40 * PJ["reference_positions_per_game_step_up"], 0)
    ok &= near(70, PJ["planned_cpu_hours"]["160"] * 60 / 10, 0) and near(0.35, DELTA * 35, 2)
    check("N section 0/4/5: 290 table files; about 10,190 positions (160 x 63.7); about 258 deep positions; about 70 minutes on 10 engines; 0.01 x 35 moves = one game in three", ok)
    check("N rates 0.195, 21.15, 77.33 CPU-seconds; 4.93 changed labels a game; 33.1 moves a player", ok)
    # Finding 2: the step-up check's deep margin and the fixed sequence's claims.
    # (Result 9's wording is checked in symbolic and simulation.)


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
