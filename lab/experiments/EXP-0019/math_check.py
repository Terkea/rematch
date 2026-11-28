"""EXP-0019: the maths, checked (ADR 0118; math.md in this folder).

EXP-0019 is pre-registered and not run: the blind audit is pending. This script checks what
exists: the estimator and its bounds, the decision rule's error control, the power analysis
(`power.json`, recomputed exactly) and the analysis code's formulas on synthetic answers. It
reads no answer and no key: `data/exp19` is the auditor's and is never opened here.

Run from this folder in the lab image:

    python math_check.py

Three kinds of check, one line each, PASS or FAIL; the script exits 1 if any fails.

1. Symbolic (sympy): the decomposition of P, its monotonicity, the Clopper-Pearson identity,
   the family's alpha, the threshold on b, the design's counts.
2. Simulation and exact computation (numpy, scipy, fixed seeds): the bounds' error rates at the
   boundary, the decision rule's power computed exactly (Results 6 and 7) against power.json,
   and analysis.py's own functions on synthetic keys and answers against the formulas here.
3. Numbers: every number the preregistration's sections 0, 1, 6 and 8 quote (summary.md quotes only section 6's),
   recomputed from power.json and EXP-0007's results.json. The decision's numbers come with
   the run.

Standard library, numpy, scipy and sympy only; `analysis` (this folder) is imported only to
test its functions on synthetic data (Result 9). About 20 s of CPU.
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
PJ = json.loads((HERE / "power.json").read_text(encoding="utf-8"))
E7 = json.loads((HERE.parent / "EXP-0007" / "results.json").read_text(encoding="utf-8"))

TAGS = ("hangingPiece", "skewer", "sacrifice", "backRankMate")
FLOOR = 0.80
ALPHA_TAG = 0.05 / 4 / 2 / 2  # 0.003125
ALPHA_PI = 0.1 * ALPHA_TAG
ALPHA_B = 0.9 * ALPHA_TAG
FY_MAX, FY_ALPHA = 0.15, 0.025
GO_UP, GO_DOWN = 0.88, 0.72
U_PER = {"hangingPiece": 5, "sacrifice": 5, "backRankMate": 4, "skewer": 2}
LEAN = {
    "Result 3 (partly: two events at 2 max(alpha_pi, alpha_b), not the sum)": "Rematch.union_bound",
    "Result 4 (the family over 4 tags, 2 directions, 2 looks)": "Rematch.bonferroni",
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
    return abs(quoted - value) <= 0.5 * 10.0 ** (-dp) + 1e-9


# ---- the formulas of math.md section 3, written here, not imported from analysis.py ---------


def pi_of(t: str) -> tuple[float, float]:
    p = E7["tags"][t]["precision"]
    return p["estimate"], min(p["n_eff"], p["n_eff_kish"])


def wilson(p: float, n: float, z: float) -> tuple[float, float]:
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return max(0.0, c - h), min(1.0, c + h)


def pi_bounds(t: str) -> tuple[float, float]:
    est, n = pi_of(t)
    return wilson(est, n, float(stats.norm.isf(ALPHA_PI)))


def cp_low(k: int, n: int, a: float = ALPHA_B) -> float:
    return 0.0 if k == 0 else float(stats.beta.ppf(a, k, n - k + 1))


def cp_up(k: int, n: int, a: float = ALPHA_B) -> float:
    return 1.0 if k == n else float(stats.beta.ppf(1 - a, k + 1, n - k))


def combine(pi: float, b: float) -> float:
    return 1 - (1 - pi) * (1 - b)


def counts(blocks: int, t: str) -> tuple[int, int, int]:
    """(unlabelled, labelled, negatives) of the design (E7)."""
    nu = blocks * U_PER[t]
    nl = {"hangingPiece": blocks, "sacrifice": blocks, "backRankMate": (blocks + 1) // 2,
          "skewer": blocks // 2}[t]
    return nu, nl, blocks * 22 - nu - nl - blocks // 2


# ---- exact power (Results 6 and 7) ------------------------------------------------------------


def tables(t: str, n_u: int, n_l: int, n_neg: int):
    """Per look size: the clear indicator over (y, ly) and the fail indicator over y + unsure,
    and the false-yes guard over ny."""
    pi_lo, pi_up = pi_bounds(t)
    pi = pi_of(t)[0]
    ys = np.arange(n_u + 1)
    low = np.array([combine(pi_lo, cp_low(k, n_u)) for k in ys])
    up = np.array([combine(pi_up, cp_up(k, n_u)) for k in ys])
    lys = np.arange(n_l + 1)
    human = pi * lys[None, :] / n_l + (1 - pi) * ys[:, None] / n_u
    clear_core = (low[:, None] >= FLOOR) & ((n_l - lys)[None, :] <= 1) & (human >= FLOOR)
    fy = np.array([cp_up(k, n_neg, FY_ALPHA) for k in range(n_neg + 1)]) <= FY_MAX
    est = 1 - (1 - pi) * (1 - ys / n_u)
    return clear_core, up < FLOOR, fy, est, low >= FLOOR


def multinom(n: int, py: float, pu: float) -> np.ndarray:
    """P(Y = y, U = u) for y + u <= n."""
    out = np.zeros((n + 1, n + 1))
    for y in range(n + 1):
        pu_cond = pu / (1 - py) if py < 1 else 0.0
        out[y, : n - y + 1] = stats.binom.pmf(y, n, py) * stats.binom.pmf(np.arange(n - y + 1), n - y, pu_cond)
    return out


def exact_power(t: str, p_true: float, blocks: int, unsure: float, a_true: float, false_yes: float,
                n_u: int | None = None, n_l: int | None = None, n_neg: int | None = None,
                code_rule: bool = False) -> dict:
    """The section 9 rule with its follow-up, computed exactly: look 1, then for every look-1
    state that continues, the pooled look 2. With `code_rule`, the continuation is analysis.py's
    `follow_up` as written: a tag whose bound clears while a guard does not is not continued
    (Finding 1); otherwise it is `simulate`'s rule, which continues it."""
    pi = pi_of(t)[0]
    if n_u is None:
        n_u, n_l, n_neg = counts(blocks, t)
    b = max(0.0, (p_true - pi) / (1 - pi))
    py = (1 - unsure) * (b + (1 - b) * false_yes)
    m = multinom(n_u, py, unsure)  # (y, u)
    pl = stats.binom.pmf(np.arange(n_l + 1), n_l, a_true)
    pn = stats.binom.pmf(np.arange(n_neg + 1), n_neg, (1 - unsure) * false_yes)
    c1, f1, fy1, est1, bound1 = tables(t, n_u, n_l, n_neg)
    c2, f2, fy2, _, _ = tables(t, 2 * n_u, 2 * n_l, 2 * n_neg)
    yu = np.arange(n_u + 1)
    # Look 1.
    fails1 = np.zeros((n_u + 1, n_u + 1), dtype=bool)
    for y in yu:
        fails1[y, : n_u - y + 1] = f1[y + yu[: n_u - y + 1]]
    p_fy1 = float(pn @ fy1)
    # clears1 over (y, u, ly, ny) = c1[y, ly] & fy1[ny]; it does not depend on u.
    p_c1_y = (c1 * pl[None, :]).sum(1) * p_fy1  # P(clear1 | y), summed over ly, ny
    look1_clears = float((m.sum(1) * p_c1_y).sum())
    look1_fails = float((m * fails1).sum())
    # Look 2: the pooled counts. For a look-1 state (y1, u1, ly1, ny1) that goes on.
    my2 = multinom(n_u, py, unsure)
    # P(clear2 | y1, ly1, ny1) = sum_{y2, ly2} P(y2) P(ly2) c2[y1+y2, ly1+ly2] * P(fy2 ok | ny1).
    py2 = my2.sum(1)
    clear2_core = np.zeros((n_u + 1, n_l + 1))
    for y1 in yu:
        for l1 in range(n_l + 1):
            sub = c2[y1 : y1 + n_u + 1, l1 : l1 + n_l + 1]
            clear2_core[y1, l1] = float(py2 @ sub @ pl)
    fy2_given = np.array([float(pn @ fy2[n1 : n1 + n_neg + 1]) for n1 in range(n_neg + 1)])
    # P(fail2 | y1 + u1 = s1): sum over (y2, u2) of f2[s1 + y2 + u2].
    s_law = np.zeros(n_u + 1)
    for y in yu:
        for u in range(n_u - y + 1):
            s_law[y + u] += my2[y, u]
    fail2_given = np.array([float(s_law @ f2[s1 : s1 + n_u + 1]) for s1 in range(n_u + 1)])
    go_est = (est1 >= GO_UP) | (est1 <= GO_DOWN)
    clears = look1_clears
    fails = look1_fails
    share = 0.0
    for y in yu:
        if not go_est[y] or (code_rule and bound1[y]):
            continue
        for u in range(n_u - y + 1):
            pyu = m[y, u]
            if pyu == 0 or fails1[y, u]:
                continue
            for l1 in range(n_l + 1):
                for_ny_clear1 = c1[y, l1]
                # ny1 matters through clears1 (fy1) and look 2's guard.
                w = pyu * pl[l1]
                if for_ny_clear1:
                    p_go_ny = pn * ~fy1  # clears1 fails only through the false-yes guard
                else:
                    p_go_ny = pn
                pgo = float(p_go_ny.sum())
                share += w * pgo
                clears += w * clear2_core[y, l1] * float(p_go_ny @ fy2_given)
                fails += w * pgo * fail2_given[y + u]
    return {"look1_clears": look1_clears, "look1_fails": look1_fails, "clears": clears,
            "fails": fails, "follow_up_share": share, "b": b}


# ---- 1. symbolic -------------------------------------------------------------------------------


def symbolic() -> None:
    pi, b, P = sp.symbols("pi b P", positive=True)
    Pexpr = pi + (1 - pi) * b
    check("R1 P = pi + (1 - pi) b = 1 - (1 - pi)(1 - b)", zero(Pexpr - (1 - (1 - pi) * (1 - b))))
    check("R1 dP/dpi = 1 - b >= 0 and dP/db = 1 - pi >= 0", zero(sp.diff(Pexpr, pi) - (1 - b)) and zero(sp.diff(Pexpr, b) - (1 - pi)))
    check("R1 b = (P - pi)/(1 - pi) inverts it", zero(Pexpr.subs(b, (P - pi) / (1 - pi)) - P))
    # Result 2: P(Bin(n, b) >= k) = I_b(k, n - k + 1), so Clopper-Pearson is exact.
    tt = sp.symbols("t", positive=True)
    ok = True
    for n in (5, 12):
        for k in range(1, n + 1):
            for bb in (sp.Rational(1, 10), sp.Rational(37, 100), sp.Rational(4, 5)):
                tail = sum(sp.binomial(n, j) * bb**j * (1 - bb) ** (n - j) for j in range(k, n + 1))
                ib = sp.factorial(n) / (sp.factorial(k - 1) * sp.factorial(n - k)) * sp.integrate(tt ** (k - 1) * (1 - tt) ** (n - k), (tt, 0, bb))
                ok &= sp.simplify(tail - ib) == 0
    check("R2 P(Bin(n, b) >= k) = I_b(k, n - k + 1), in exact rationals (n = 5, 12, every k, b = 1/10, 37/100, 4/5)", ok)
    check("R4 alpha per tag, direction and look = 0.05 / (4 x 2 x 2) = 0.003125; 0.1 and 0.9 of it",
          sp.Rational(5, 100) / 16 == sp.Rational(1, 320) and abs(ALPHA_PI - 0.0003125) < 1e-15 and abs(ALPHA_B - 0.0028125) < 1e-15)
    # Result 5: the threshold on b, and that the guards only remove clears (set inclusion).
    lo = sp.symbols("l", positive=True)
    thr = (FLOOR - lo) / (1 - lo)
    check("R5 1 - (1 - l)(1 - b) >= 0.8 iff b >= (0.8 - l)/(1 - l)", zero(combine(lo, thr) - FLOOR))
    # Design (E7): 6 blocks of 22; the counts per tag.
    ok = sum(U_PER.values()) + 3 + 3 == 22 and 6 * 22 == 132
    ok &= [counts(6, t) for t in TAGS] == [(30, 6, 93), (12, 3, 114), (30, 6, 93), (24, 3, 102)]
    check("R10 the design: 6 x 22 = 132 items; unlabelled 30, 12, 30, 24; labelled 6, 3, 6, 3; negatives 93, 114, 93, 102", ok)


# ---- 2. simulation and exact computation -------------------------------------------------------


def simulation() -> None:
    rng = np.random.default_rng(20261128)
    # Result 2: the Clopper-Pearson lower bound's error at the boundary, exactly and simulated.
    worst = 0.0
    for n in (12, 24, 30, 60):
        for bb in np.linspace(0.02, 0.98, 97):
            ks = np.arange(n + 1)
            lows = np.array([cp_low(int(k), n) for k in ks])
            worst = max(worst, float(stats.binom.pmf(ks, n, bb)[lows > bb].sum()))
    check("S1 Result 2: P(b_low > b) <= 0.0028125 for n = 12, 24, 30, 60 and b on a grid of 97 (exact)", worst <= ALPHA_B + 1e-12, f"worst {worst:.6f}")
    reps = 400_000
    n, bb = 30, 0.6
    k = rng.binomial(n, bb, reps)
    lows = np.array([cp_low(int(j), n) for j in range(n + 1)])[k]
    est = float(np.mean(lows > bb))
    want = float(stats.binom.pmf(np.arange(n + 1), n, bb)[np.array([cp_low(j, n) for j in range(n + 1)]) > bb].sum())
    check("S1 Result 2: simulated P(b_low > b) at n = 30, b = 0.6 matches the exact value (4 SE)",
          abs(est - want) < 4 * math.sqrt(want * (1 - want) / reps) + 1e-6, f"{est:.5f} vs {want:.5f}")
    # Result 3: the bound on P with pi random too, computed exactly under (M4)'s normal model of
    # pi's estimator, at the worst b on a fine grid (n = 30); and without pi's bound, to show the
    # check can fail.
    t = "hangingPiece"
    pi0, ne = pi_of(t)
    z = float(stats.norm.isf(ALPHA_PI))
    sd_pi = math.sqrt(pi0 * (1 - pi0) / ne)

    def miss_rate(b0: float, n: int, with_pi_bound: bool, a_b: float = ALPHA_B) -> float:
        P0 = combine(pi0, b0)
        lows = [cp_low(k, n, a_b) for k in range(n + 1)]
        pmf = stats.binom.pmf(np.arange(n + 1), n, b0)
        total = 0.0
        for k in range(n + 1):
            if lows[k] >= 1:
                continue
            need = 1 - (1 - P0) / (1 - lows[k])  # pi_low above this misses
            if with_pi_bound:
                lo_, hi_ = 0.0, 1.0  # wilson's lower end is increasing in the estimate
                for _ in range(60):
                    mid = (lo_ + hi_) / 2
                    lo_, hi_ = (mid, hi_) if wilson(mid, ne, z)[0] <= need else (lo_, mid)
                thr = hi_
            else:
                thr = need
            total += pmf[k] * float(stats.norm.sf(thr, pi0, sd_pi))
        return total

    # The worst b sit just below a jump of b_low(k, 30): there P(b_low > b) is nearly alpha_b.
    grid = [cp_low(k, 30) - 1e-9 for k in range(12, 31)]
    worst = max(miss_rate(float(b0), 30, True) for b0 in grid)
    worst_nopi = max(miss_rate(float(b0), 30, False) for b0 in grid)
    grid10 = [cp_low(k, 30, 10 * ALPHA_B) - 1e-9 for k in range(12, 31)]
    worst_broken = max(miss_rate(float(b0), 30, True, 10 * ALPHA_B) for b0 in grid10)
    check("S2 Result 3: P(P_low > P) is 0.00065 at most, below alpha = 0.003125, at b just below each jump of b_low(k, 30) (exact under (M4)); 0.0025 without pi's bound; above alpha with b's bound at 10 alpha_b",
          worst <= ALPHA_TAG and near(0.00065, worst, 5) and near(0.0025, worst_nopi, 4) and worst_broken > ALPHA_TAG,
          f"{worst:.6f}; {worst_nopi:.4f}; broken {worst_broken:.4f}")
    # Result 6: power.json's main table against the exact computation.
    worst = 0.0
    n_pts = 0
    for label, blocks in (("all blocks", 6), ("minimum blocks", 3)):
        for t in TAGS:
            for p, row in PJ["main"][label][t].items():
                ex = exact_power(t, float(p), blocks, 0.05, 1.0, 0.0)
                for key in ("look1_clears", "look1_fails", "clears", "fails", "follow_up_share"):
                    v, e = row[key], ex[key]
                    sd = math.sqrt(max(e * (1 - e), 1e-12) / PJ["reps"])
                    zz = abs(v - e) / sd if e * (1 - e) > 1e-7 else abs(v - e) * 1e5
                    worst = max(worst, zz)
                    n_pts += 1
                ok_b = near(row["b"], ex["b"], 4)
                worst = max(worst, 0 if ok_b else 99)
    # The family of 556 Monte Carlo values: the largest |z| of 556 normals exceeds 4.5 with
    # probability about 0.004, so 4.5 is the family-level threshold.
    check(f"S3 Result 6: every power.json main value within 4.5 MC SE of the exact computation ({n_pts} values; 4.5 is the threshold for the 556 values of S3)", worst < 4.5, f"largest |z| {worst:.2f}")
    worst = 0.0
    for key, table in PJ["sensitivity"].items():
        uns = float(key.split(",")[0].split()[1])
        a = float(key.split()[-1])
        for t in TAGS:
            for p, v in table[t].items():
                e = exact_power(t, float(p), 6, uns, a, 0.0)["clears"]
                sd = math.sqrt(max(e * (1 - e), 1e-12) / PJ["reps"])
                worst = max(worst, abs(v - e) / sd if e * (1 - e) > 1e-7 else abs(v - e) * 1e5)
    check("S3 Result 6: every sensitivity value (unsure 0 to 0.2, labelled yes 0.85 to 1) within 4.5 MC SE", worst < 4.5, f"largest |z| {worst:.2f}")
    worst = 0.0
    for key, table in PJ["lenient_rater"].items():
        fy = float(key.split()[-1])
        for t in TAGS:
            for p, v in table[t].items():
                e = exact_power(t, float(p), 6, 0.05, 1.0, fy)["clears"]
                sd = math.sqrt(max(e * (1 - e), 1e-12) / PJ["reps"])
                worst = max(worst, abs(v - e) / sd if e * (1 - e) > 1e-7 else abs(v - e) * 1e5)
    check("S3 Result 7: every lenient-rater value (false yes 0.05 to 0.2) within 4.5 MC SE", worst < 4.5, f"largest |z| {worst:.2f}")
    worst = 0.0
    for t in TAGS:
        for n, table in PJ["n_curve"][t].items():
            for p, v in table.items():
                e = exact_power(t, float(p), 6, 0.05, 1.0, 0.0, int(n), 6, 100)["look1_clears"]
                sd = math.sqrt(max(e * (1 - e), 1e-12) / PJ["reps"])
                worst = max(worst, abs(v - e) / sd if e * (1 - e) > 1e-7 else abs(v - e) * 1e5)
    check("S3 Result 6: every n_curve value within 4.5 MC SE of the exact look-1 power", worst < 4.5, f"largest |z| {worst:.2f}")
    # Result 2 (b): the items are a simple random sample without replacement of the U stratum.
    worst_l = worst_u = 0.0
    for n in (6, 12, 15, 24, 30, 48, 60):
        lows = np.array([cp_low(k, n) for k in range(n + 1)])
        ups = np.array([cp_up(k, n) for k in range(n + 1)])
        for big in (100, 300, 1000, 5000, 20000):
            for m_ in range(0, big + 1, max(1, big // 200)):
                bb = m_ / big
                pm = stats.hypergeom.pmf(np.arange(n + 1), big, m_, n)
                worst_l = max(worst_l, float(pm[lows > bb].sum()))
                worst_u = max(worst_u, float(pm[ups < bb].sum()))
    check("S1 Result 2: drawn without replacement (n = 6 to 60, strata of 100 to 20,000), both bounds' error <= 0.0028125 (exact, checked not proved)",
          worst_l <= ALPHA_B + 1e-12 and worst_u <= ALPHA_B + 1e-12, f"worst {worst_l:.6f}, {worst_u:.6f}")
    # Result 9: analysis.py's own functions on synthetic keys and answers.
    synthetic(rng)


def synthetic(rng: np.random.Generator) -> None:
    if str(HERE) not in sys.path:
        sys.path.insert(0, str(HERE))
    import analysis

    ok_dec = ok_tally = ok_blocks = True
    for rep in range(300):
        items = []
        idx = 0
        n_blocks = int(rng.integers(1, 7))
        for blk in range(1, 7):
            for _ in range(22):
                idx += 1
                t = TAGS[int(rng.integers(0, 4))]
                kind = rng.choice(["U", "L", "D:none"])
                strata = [f"{kind}:{t}"] if kind in ("U", "L") else ["D:none"]
                tags = [t] if kind in ("U", "L") else []
                themes = [t] if kind == "L" else []
                items.append({"item": idx, "block": blk, "strata": strata, "tags": tags, "themes": themes})
        key = {"page_id": "x", "items": items}
        last = 22 * n_blocks - int(rng.integers(0, 3))
        ans = {}
        for it in items:
            if it["item"] <= last:
                ans[str(it["item"])] = {q: str(rng.choice(["yes", "no", "unsure", "blank"])) for q in TAGS}
        answers = {"format": "exp19-answers-v1", "page_id": "x", "answers": ans}
        done = analysis.done_items(key, answers)
        blocks_whole = {b for b in range(1, 7) if 22 * b <= last}
        ok_blocks &= {it["block"] for it in done} == blocks_whole
        for t in TAGS:
            u = analysis.tally(done, f"U:{t}", t)
            lab = analysis.tally(done, f"L:{t}", t)
            neg = analysis.negatives(done, t)
            # The same counts from the raw data, here.
            def cnt(pred):
                c = {"n": 0, "yes": 0, "no": 0, "unsure": 0}
                for it in items:
                    if it["block"] in blocks_whole and pred(it):
                        a = ans.get(str(it["item"]), {}).get(t)
                        a = a if a in ("yes", "no", "unsure") else "unsure"
                        c["n"] += 1
                        c[a] += 1
                return c
            ok_tally &= u == cnt(lambda it: f"U:{t}" in it["strata"])
            ok_tally &= lab == cnt(lambda it: f"L:{t}" in it["strata"])
            ok_tally &= neg == cnt(lambda it: t not in it["tags"] and t not in it["themes"])
            if not u["n"]:
                continue
            pi = {"estimate": pi_of(t)[0], "n_eff": pi_of(t)[1]}
            got = analysis.decide_tag(pi, u, lab, neg)
            lo_pi, up_pi = pi_bounds(t)
            low = combine(lo_pi, cp_low(u["yes"], u["n"]))
            up = combine(up_pi, cp_up(u["yes"] + u["unsure"], u["n"]))
            human = (pi["estimate"] * lab["yes"] / lab["n"] + (1 - pi["estimate"]) * u["yes"] / u["n"]) if lab["n"] else None
            guards = (lab["no"] + lab["unsure"] <= 1) and human is not None and human >= FLOOR and cp_up(neg["yes"], neg["n"], FY_ALPHA) <= FY_MAX
            want = "clears" if low >= FLOOR and guards else "fails" if up < FLOOR else "inconclusive"
            ok_dec &= got["decision"].split(":")[0] == want
            ok_dec &= abs(got["bound_low"] - round(low, 4)) < 1e-9 and abs(got["bound_up"] - round(up, 4)) < 1e-9
    check("S4 Result 9: analysis.done_items keeps exactly the whole blocks (300 synthetic sessions)", ok_blocks)
    check("S4 Result 9: analysis.tally and negatives equal the counts from the raw synthetic answers", ok_tally)
    check("S4 Result 9: analysis.decide_tag's bounds and decision equal (E2) to (E4) here (random sessions)", ok_dec)
    ok_grid = True
    n_states = n_clear = n_blocked = 0
    for blocks in (6, 3):
        for t in TAGS:
            n_u, n_l, n_neg = counts(blocks, t)
            kmax = max(k for k in range(n_neg + 1) if cp_up(k, n_neg, FY_ALPHA) <= FY_MAX)
            pi = {"estimate": pi_of(t)[0], "n_eff": pi_of(t)[1]}
            lo_pi, up_pi = pi_bounds(t)
            for y in range(n_u + 1):
                for v in sorted({0, 1, 3, n_u - y}):
                    if y + v > n_u:
                        continue
                    for l in range(n_l + 1):
                        for f in sorted({0, kmax, kmax + 1, n_neg // 2}):
                            u = {"n": n_u, "yes": y, "no": n_u - y - v, "unsure": v}
                            lab = {"n": n_l, "yes": l, "no": n_l - l, "unsure": 0}
                            neg = {"n": n_neg, "yes": f, "no": n_neg - f, "unsure": 0}
                            got = analysis.decide_tag(pi, u, lab, neg)
                            low = combine(lo_pi, cp_low(y, n_u))
                            up = combine(up_pi, cp_up(y + v, n_u))
                            human = pi["estimate"] * l / n_l + (1 - pi["estimate"]) * y / n_u
                            g1, g2, g3 = n_l - l <= 1, human >= FLOOR, f <= kmax
                            if low >= FLOOR and g1 and g2 and g3:
                                want = "clears"
                            elif up < FLOOR:
                                want = "fails"
                            elif low >= FLOOR:
                                want = "inconclusive: the bound clears"
                            else:
                                want = "inconclusive"
                            ok_grid &= got["decision"] == want or (want.startswith("inconclusive:") and got["decision"].startswith(want))
                            est = round(combine(pi["estimate"], y / n_u), 4)
                            ok_grid &= analysis.follow_up(got) == (want == "inconclusive" and (est >= GO_UP or est <= GO_DOWN))
                            ok_grid &= got["guards"] == {"labelled": g1, "labelled_judged_too": g2, "false_yes": g3}
                            n_states += 1
                            n_clear += want == "clears"
                            n_blocked += want.startswith("inconclusive:")
    check(f"S4 Result 9: decide_tag, its guards and follow_up equal (E2) to (E5) on every state of a grid ({n_states} states, {n_clear} clears, {n_blocked} blocked by a guard)",
          ok_grid and n_clear > 0 and n_blocked > 0)
    # A synthetic decision at both ends: all "yes" on 30 items clears hangingPiece; all "no" fails it.
    pi = {"estimate": pi_of("hangingPiece")[0], "n_eff": pi_of("hangingPiece")[1]}
    allyes = analysis.decide_tag(pi, {"n": 30, "yes": 30, "no": 0, "unsure": 0}, {"n": 6, "yes": 6, "no": 0, "unsure": 0},
                                 {"n": 93, "yes": 0, "no": 93, "unsure": 0})
    allno = analysis.decide_tag(pi, {"n": 30, "yes": 0, "no": 30, "unsure": 0}, {"n": 6, "yes": 6, "no": 0, "unsure": 0},
                                {"n": 93, "yes": 0, "no": 93, "unsure": 0})
    lenient = analysis.decide_tag(pi, {"n": 30, "yes": 30, "no": 0, "unsure": 0}, {"n": 6, "yes": 6, "no": 0, "unsure": 0},
                                  {"n": 93, "yes": 10, "no": 83, "unsure": 0})
    # Finding 1: follow_up compares the decision string with "inconclusive" exactly.
    blocked = analysis.decide_tag(pi, {"n": 30, "yes": 29, "no": 1, "unsure": 0}, {"n": 6, "yes": 3, "no": 3, "unsure": 0},
                                  {"n": 93, "yes": 0, "no": 93, "unsure": 0})
    check("S4 Finding 1: a tag whose bound clears but whose labelled guard fails is 'inconclusive: ...', has estimate >= 0.88, and analysis.follow_up does not continue it",
          blocked["decision"].startswith("inconclusive:") and blocked["estimate"] >= 0.88
          and analysis.follow_up(blocked) is False, f"{blocked['decision']}, estimate {blocked['estimate']}")
    worst, where = 0.0, ""
    for key, table in PJ["sensitivity"].items():
        uns = float(key.split(",")[0].split()[1])
        a = float(key.split()[-1])
        for t in TAGS:
            for p_ in ("0.9", "0.95"):
                sim_rule = exact_power(t, float(p_), 6, uns, a, 0.0)["clears"]
                code = exact_power(t, float(p_), 6, uns, a, 0.0, code_rule=True)["clears"]
                if sim_rule - code > worst:
                    worst, where = sim_rule - code, f"{t} at {p_}, {key}"
    main_same = all(abs(exact_power(t, p_, 6, 0.05, 1.0, 0.0)["clears"] - exact_power(t, p_, 6, 0.05, 1.0, 0.0, code_rule=True)["clears"]) < 1e-12
                    for t in TAGS for p_ in (0.80, 0.90, 0.95))
    worst_len, where_len = 0.0, ""
    for fy in (0.05, 0.10):
        for t in TAGS:
            for p_ in (0.8, 0.9):
                d_ = exact_power(t, p_, 6, 0.05, 1.0, fy)["clears"] - exact_power(t, p_, 6, 0.05, 1.0, fy, code_rule=True)["clears"]
                if d_ > worst_len:
                    worst_len, where_len = d_, f"{t} at {p_}, false yes {fy}"
    check("S4 Finding 1: in the lenient-rater rows the code's rule loses up to 0.211 of power to clear (skewer at 0.90, 10% false yes)",
          near(0.211, worst_len, 3), f"{worst_len:.4f} ({where_len})")
    lab_u = [exact_power(t, 0.95, 6, 0.05, 0.95, 0.0)["clears"] for t in TAGS]
    lab_u_code = [exact_power(t, 0.95, 6, 0.05, 0.95, 0.0, code_rule=True)["clears"] for t in TAGS]
    lab_u90 = [exact_power(t, 0.90, 6, 0.05, 0.95, 0.0)["clears"] for t in TAGS]
    check("S4 Finding 1 and (M3): with 'unsure' on 5% of the labelled items too (a = 0.95), clears at 0.95 are 0.84 to 0.93 (0.84 to 0.89 under the code's rule; skewer 0.930 to 0.846) and 0.27 to 0.55 at 0.90",
          near(0.84, min(lab_u), 2) and near(0.93, max(lab_u), 2) and near(0.84, min(lab_u_code), 2) and near(0.89, max(lab_u_code), 2)
          and near(0.930, lab_u[1], 3) and near(0.846, lab_u_code[1], 3) and near(0.27, min(lab_u90), 2) and near(0.55, max(lab_u90), 2),
          ", ".join(f"{a_:.3f}/{b_:.3f}" for a_, b_ in zip(lab_u, lab_u_code, strict=True)))
    row = [exact_power(t, 0.95, 6, 0.0, 0.85, 0.0, code_rule=True)["clears"] for t in TAGS]
    check("S4 Finding 1: the main table is unchanged under the code's rule; the labelled-yes sensitivity loses up to 0.168 of power to clear, and section 6's 'labelled 0.85: 0.70 to 0.78 at 0.95' becomes 0.61 to 0.73",
          main_same and near(0.168, worst, 3) and near(0.61, min(row), 2) and near(0.73, max(row), 2),
          f"largest loss {worst:.4f} ({where}); code-rule row " + ", ".join(f"{v:.3f}" for v in row))
    check("S4 Result 9: all yes clears, all no fails, and 10 false yes of 93 blocks the clear (guard)",
          allyes["decision"] == "clears" and allno["decision"] == "fails" and lenient["decision"].startswith("inconclusive")
          and not lenient["guards"]["false_yes"])


# ---- 3. numbers --------------------------------------------------------------------------------


def numbers() -> None:
    thr = {t: (FLOOR - pi_bounds(t)[0]) / (1 - pi_bounds(t)[0]) for t in TAGS}
    check("N section 6: thresholds on b 0.565, 0.127, 0.594, 0.544, and power.json threshold_b",
          all(near(q, thr[t], 3) for q, t in zip([0.565, 0.127, 0.594, 0.544], TAGS, strict=True))
          and all(near(PJ["threshold_b"][t], thr[t], 4) for t in TAGS), ", ".join(f"{thr[t]:.4f}" for t in TAGS))
    pis = [pi_of(t)[0] for t in TAGS]
    check("N section 1: pi 0.572, 0.810, 0.528, 0.606 (EXP-0007)", all(near(q, v, 3) for q, v in zip([0.572, 0.810, 0.528, 0.606], pis, strict=True)))
    d = PJ["design"]
    check("N section 0 and 6: 132 items, 88 minutes at 40 s; negatives 93 to 114",
          d["items"] == 132 and d["minutes_at_40s"] == 88.0 and min(d["negatives"].values()) == 93 and max(d["negatives"].values()) == 114)
    m = PJ["main"]["all blocks"]
    m3 = PJ["main"]["minimum blocks"]
    table = {
        "hangingPiece": [0.0003, 0.02, (0.38, 0.13), (0.95, 0.65), (1.00, 0.94)],
        "skewer": [0, 0.03, (0.66, 0.47), (0.99, 0.96), (1.00, 1.00)],
        "sacrifice": [0.0004, 0.02, (0.29, 0.09), (0.90, 0.52), (1.00, 0.87)],
        "backRankMate": [0, 0.01, (0.31, 0.05), (0.92, 0.40), (1.00, 0.80)],
    }
    ok = True
    for t, row in table.items():
        ok &= near(row[0], m[t]["0.8"]["clears"], 4) and near(row[1], m[t]["0.85"]["clears"], 2)
        for q, p in zip(row[2:], ("0.9", "0.95", "0.98"), strict=True):
            ok &= near(q[0], m[t][p]["clears"], 2) and near(q[1], m[t][p]["look1_clears"], 2)
    check("N section 6: the table of clear probabilities (both looks, and look 1 in brackets)", ok)
    ok = near(0.34, m["hangingPiece"]["0.7"]["fails"], 2) and near(0.37, m["sacrifice"]["0.7"]["fails"], 2) and near(0.20, m["backRankMate"]["0.7"]["fails"], 2)
    ok &= m["skewer"]["0.7"]["fails"] == 0 and pi_bounds("skewer")[1] >= FLOOR
    check("N section 6: fails at 0.70 are 0.34, 0.37, 0.20; skewer cannot fail (u_pi = 0.8434 >= 0.80 forces P_up >= 0.80)", ok)
    wf = max(m[t]["0.8"]["fails"] for t in TAGS)
    wc = max(m[t]["0.8"]["clears"] for t in TAGS)
    wc3 = max(m3[t]["0.8"]["clears"] for t in TAGS)
    check("N section 6: at P = 0.80 a wrong fail at most 0.0006 and a wrong clear at most 0.0004 (0.0006 at 3 blocks), against 0.003125",
          near(0.0006, wf, 4) and near(0.0004, wc, 4) and near(0.0006, wc3, 4) and max(wf, wc, wc3) < ALPHA_TAG, f"{wf}, {wc}, {wc3}")
    lr = PJ["lenient_rater"]
    check("N section 6: 10% false yes gives a wrong clear at 0.80 in at most 0.0011; 20% stops nearly every clear (<= 0.0011 at 0.90)",
          near(0.0011, max(lr["false yes 0.1"][t]["0.8"] for t in TAGS), 4) and max(lr["false yes 0.2"][t]["0.9"] for t in TAGS) <= 0.0011)
    check("N section 6: a tag at 0.90 clears 0.29 to 0.66; at 0.95 0.90 to 0.99 (two looks), 0.40 to 0.96 (one)",
          near(0.29, min(m[t]["0.9"]["clears"] for t in TAGS), 2) and near(0.66, max(m[t]["0.9"]["clears"] for t in TAGS), 2)
          and near(0.90, min(m[t]["0.95"]["clears"] for t in TAGS), 2) and near(0.99, max(m[t]["0.95"]["clears"] for t in TAGS), 2)
          and near(0.40, min(m[t]["0.95"]["look1_clears"] for t in TAGS), 2) and near(0.96, max(m[t]["0.95"]["look1_clears"] for t in TAGS), 2))
    nc = PJ["n_curve"]
    hard = ("hangingPiece", "sacrifice", "backRankMate")
    check("N section 6: n_curve 0.53 to 0.59 at 80 items and 0.90 (the three hard tags)",
          near(0.53, min(nc[t]["80"]["0.9"] for t in hard), 2) and near(0.59, max(nc[t]["80"]["0.9"] for t in hard), 2))
    s = PJ["sensitivity"]
    s0 = s["unsure 0.0, labelled yes 1.0"]
    s20 = s["unsure 0.2, labelled yes 1.0"]
    s85 = s["unsure 0.0, labelled yes 0.85"]
    check("N section 6: no unsure 0.52 to 0.73 at 0.90; 20% unsure 0.01 to 0.44 at 0.90 and 0.14 to 0.93 at 0.95; labelled 0.85: 0.70 to 0.78 at 0.95",
          near(0.52, min(s0[t]["0.9"] for t in TAGS), 2) and near(0.73, max(s0[t]["0.9"] for t in TAGS), 2)
          and near(0.01, min(s20[t]["0.9"] for t in TAGS), 2) and near(0.44, max(s20[t]["0.9"] for t in TAGS), 2)
          and near(0.14, min(s20[t]["0.95"] for t in TAGS), 2) and near(0.93, max(s20[t]["0.95"] for t in TAGS), 2)
          and near(0.70, min(s85[t]["0.95"] for t in TAGS), 2) and near(0.78, max(s85[t]["0.95"] for t in TAGS), 2))
    check("N section 6: stopping at 3 blocks gives 0.41 to 0.91 at 0.95",
          near(0.41, min(m3[t]["0.95"]["clears"] for t in TAGS), 2) and near(0.91, max(m3[t]["0.95"]["clears"] for t in TAGS), 2))
    # Result 8: the false-yes guard's operating points, every tag.
    rows = []
    ok = True
    for t in TAGS:
        n_neg = counts(6, t)[2]
        kmax = max(k for k in range(n_neg + 1) if cp_up(k, n_neg, FY_ALPHA) <= FY_MAX)
        pas = [float(stats.binom.cdf(kmax, n_neg, 0.95 * f)) for f in (0.0, 0.05, 0.10, 0.20)]
        rows.append(f"{t} {kmax}/{n_neg}: " + ", ".join(f"{v:.3f}" for v in pas))
        ok &= pas[0] == 1.0 and pas[3] < 0.01 and cp_up(kmax + 1, n_neg, FY_ALPHA) > FY_MAX
        want = {"hangingPiece": 7, "sacrifice": 7, "backRankMate": 8, "skewer": 9}[t]
        ok &= kmax == want
    check("N Result 8: the false-yes guard passes iff at most 7, 9, 7, 8 yes of 93, 114, 93, 102; P(pass) at false yes 0, 5%, 10%, 20%",
          ok, "; ".join(rows))
    # Section 6's bound on pi: the Wilson bounds at alpha_pi = 0.0003125.
    pb = [pi_bounds(t) for t in TAGS]
    check("N Result 3: pi's one-sided bounds at 0.0003125 (z = 3.42)", near(3.42, float(stats.norm.isf(ALPHA_PI)), 2),
          "; ".join(f"{t} [{lo:.4f}, {up:.4f}]" for t, (lo, up) in zip(TAGS, pb, strict=True)))
    ex = {(blk, t): exact_power(t, 0.80, blk, 0.05, 1.0, 0.0) for blk in (6, 3) for t in TAGS}
    wf6 = max(ex[6, t]["fails"] for t in TAGS)
    wc6 = max(ex[6, t]["clears"] for t in TAGS)
    wc3 = max(ex[3, t]["clears"] for t in TAGS)
    check("N section 5: exact worst cases at P = 0.80: wrong fail 0.00046, wrong clear 0.00029 (6 blocks), 0.00074 at 3 blocks, all below alpha",
          near(0.00046, wf6, 5) and near(0.00029, wc6, 5) and near(0.00074, wc3, 5) and max(wf6, wc6, wc3) < ALPHA_TAG,
          f"{wf6:.6f}, {wc6:.6f}, {wc3:.6f}")
    g1 = 1 - float(stats.binom.sf(4, 6, 0.85))
    g1_3 = 1 - float(stats.binom.sf(1, 3, 0.85))
    check("N section 6: at a = 0.85, G1 stops a clear with probability 0.22 (6 labelled items) and 0.06 (3)",
          near(0.22, g1, 2) and near(0.06, g1_3, 2), f"{g1:.4f}, {g1_3:.4f}")
    check("N section 6 and Result 3: 16 alpha_pi = 0.005; Lean's union_bound gives 2 max(alpha_pi, alpha_b) = 0.005625",
          near(0.005, 16 * ALPHA_PI, 4) and near(0.005625, 2 * max(ALPHA_PI, ALPHA_B), 6))
    stop = {t: 1 - float(stats.binom.cdf({"hangingPiece": 7, "sacrifice": 7, "backRankMate": 8, "skewer": 9}[t], counts(6, t)[2], 0.95 * 0.05)) for t in TAGS}
    check("N Result 8: at 5% false yes the guard stops a clear about one time in 13 (hangingPiece, sacrifice), 19 (backRankMate) and 22 (skewer)",
          near(13, 1 / stop["hangingPiece"], 0) and near(13, 1 / stop["sacrifice"], 0) and near(19, 1 / stop["backRankMate"], 0) and near(22, 1 / stop["skewer"], 0),
          ", ".join(f"{t} {1 / v:.1f}" for t, v in stop.items()))
    passes = {f: [float(stats.binom.cdf({"hangingPiece": 7, "sacrifice": 7, "backRankMate": 8, "skewer": 9}[t], counts(6, t)[2], 0.95 * f)) for t in TAGS] for f in (0.05, 0.10, 0.20)}
    check("N Result 8: pass rates 0.925 to 0.955 (5%), 0.332 to 0.359 (10%), 0.001 to 0.002 (20%)",
          near(0.925, min(passes[0.05]), 3) and near(0.955, max(passes[0.05]), 3) and near(0.332, min(passes[0.10]), 3)
          and near(0.359, max(passes[0.10]), 3) and near(0.001, min(passes[0.20]), 3) and near(0.002, max(passes[0.20]), 3))
    # Finding 2 (c): the quotes are Monte Carlo values; their exact values.
    ex6 = {(t, p_): exact_power(t, p_, 6, 0.05, 1.0, 0.0) for t in TAGS for p_ in (0.80, 0.90, 0.95, 0.98)}
    ok = near(0.3731, ex6["hangingPiece", 0.90]["clears"], 4) and near(0.1362, ex6["hangingPiece", 0.90]["look1_clears"], 4)
    ok &= near(0.6671, ex6["skewer", 0.90]["clears"], 4) and near(0.9256, ex6["backRankMate", 0.95]["clears"], 4)
    ok &= near(0.7949, ex6["backRankMate", 0.98]["look1_clears"], 4) and near(0.00029, ex6["sacrifice", 0.80]["clears"], 5)
    ok &= near(0.00009, ex6["backRankMate", 0.80]["clears"], 5)
    ok &= near(0.67, max(ex6[t, 0.90]["clears"] for t in TAGS), 2)
    s85 = [exact_power(t, 0.95, 6, 0.0, 0.85, 0.0)["clears"] for t in TAGS]
    ok &= near(0.79, max(s85), 2) and near(0.70, min(s85), 2)
    n80 = [exact_power(t, 0.90, 6, 0.05, 1.0, 0.0, 80, 6, 100)["look1_clears"] for t in ("hangingPiece", "sacrifice", "backRankMate")]
    ok &= near(0.53, min(n80), 2) and near(0.58, max(n80), 2)
    check("N Finding 2 (c): the exact values behind the Monte Carlo quotes (0.3731, 0.1362, 0.6671, 0.9256, 0.7949, 0.00029, 0.00009; 0.29 to 0.67; 0.70 to 0.79; 0.53 to 0.58)", ok)
    rec = [E7["tags"][t]["recall"]["estimate"] for t in TAGS]
    check("N section 1 and 0: recall 0.82 to 1.00 (EXP-0007), and pin's recall 0.20",
          near(0.82, min(rec), 2) and near(1.00, max(rec), 2) and near(0.20, E7["tags"]["pin"]["recall"]["estimate"], 2))
    check("N section 6: 20,000 replicates a scenario", PJ["reps"] == 20000)
    items80 = 80 // 5 * 22
    check("N section 6: 80 unlabelled hangingPiece items a look is 16 blocks, 352 items, 3.9 hours at 40 s ('more than 300 items and four hours')",
          items80 == 352 and near(3.9, items80 * 40 / 3600, 1))
    nk = sum(len(v[t]) for v in PJ["sensitivity"].values() for t in TAGS)
    nl = sum(len(v[t]) for v in PJ["lenient_rater"].values() for t in TAGS)
    nc = sum(len(tab) for t in TAGS for tab in PJ["n_curve"][t].values())
    check("N Result 6: power.json holds 108 sensitivity, 48 lenient-rater and 120 n_curve values", (nk, nl, nc) == (108, 48, 120), f"{nk}, {nl}, {nc}")
    sk = PJ["main"]["all blocks"]["skewer"]["0.8"]
    check("N Result 6: skewer's 'P = 0.80' row has b = 0, so its true P is pi = 0.81, not 0.80", sk["b"] == 0.0 and pi_of("skewer")[0] > 0.80)

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
