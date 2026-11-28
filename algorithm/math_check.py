"""Checks algorithm/MATH.md (ADR 0118): the maths of the algorithm, against the build.

Run in the lab image, from the repository root or from algorithm/:

    docker run --rm -v "$PWD:/repo" -w /repo/algorithm --entrypoint python rematch-lab:main math_check.py

Five kinds of check, each named by the result it covers (A1 ... in MATH.md):

1. constants: every constant MATH.md uses, read from the Rust and TypeScript sources by regex
   and from algorithm/manifest.json (the build's manifest, which backend/crates/algo/tests/
   manifest.rs holds equal to `cli algo manifest`). When the `cli` of the lab image is on PATH
   and is this build (`cli algo show --json` has the manifest's id), its components must equal
   the committed manifest's. No algorithm/manifest-*.json files exist in the repository, so none
   are written: the constants the manifest does not carry come from the sources.
2. symbolic: each derivation's two sides, subtracted, simplify to 0 with sympy; a bound sympy
   cannot settle is checked on a dense grid, and the line says so.
3. simulation: each probabilistic claim, simulated from a fixed seed with a known truth, within
   4 Monte Carlo standard errors.
4. build: the production code's own outputs, the lab's golden responses (lab/fixtures/golden,
   which the cli and Node tests regenerate from the build), recomputed from MATH.md's formulas.
5. numbers: every number MATH.md and the README's worked example quote.

Exit status: 0 when every check passes, 1 otherwise (every failure is printed).
"""

from __future__ import annotations

import itertools
import json
import math
import re
import shutil
import subprocess
import sys
from datetime import datetime
from fractions import Fraction
from pathlib import Path

import numpy as np
import sympy as sp
from scipy import stats

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
FAILURES: list[str] = []
PASSES = 0


def check(name: str, ok: bool, detail: str = "") -> None:
    global PASSES
    if ok:
        PASSES += 1
        print(f"ok   {name}{': ' + detail if detail else ''}")
    else:
        FAILURES.append(f"{name}: {detail}")
        print(f"FAIL {name}: {detail}")


def close(a: float, b: float, rel: float = 1e-9, abs_: float = 1e-12) -> bool:
    return math.isclose(a, b, rel_tol=rel, abs_tol=abs_)


def src(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def const(path: str, pattern: str) -> str:
    m = re.search(pattern, src(path), re.MULTILINE)
    if not m:
        raise AssertionError(f"{path}: no match for {pattern!r}")
    return m.group(1)


def num(path: str, pattern: str) -> float:
    return float(const(path, pattern).replace("_", ""))


# ---- 1. constants ----------------------------------------------------------------------------

SCORE = "backend/crates/chess-core/src/score.rs"
JUDGE = "backend/crates/chess-core/src/judgement.rs"
ACC = "backend/crates/chess-core/src/game_accuracy.rs"
POST = "backend/crates/profile-core/src/stats/posterior.rs"
INTERVALS = "backend/crates/profile-core/src/stats/intervals.rs"
WEIGHTS = "backend/crates/profile-core/src/stats/weights.rs"
PRIORITY = "backend/crates/profile-core/src/stats/priority.rs"
VARIANT = "backend/crates/profile-core/src/stats/variant.rs"
CONSTANTS = "backend/crates/profile-core/src/constants.rs"
OBSERVE = "backend/crates/profile-core/src/weakness/observe_game.rs"
DETECTORS = "backend/crates/profile-core/src/weakness/detectors.rs"
BOCPD = "backend/crates/profile-core/src/timeline/bocpd.rs"
SERIES = "backend/crates/profile-core/src/timeline/series.rs"
CUSUM = "backend/crates/profile-core/src/timeline/cusum.rs"
STATES = "backend/crates/profile-core/src/timeline/states.rs"
TRAJ = "backend/crates/profile/src/coaching/trajectory.rs"
GOAL = "apps/web/src/server/focus/goal.ts"
TRAIN = "apps/web/src/server/train/model.ts"
WEBSTATS = "apps/web/src/server/stats/constants.ts"

C: dict[str, float] = {}


def constants() -> dict:
    f = r"([0-9_.eE+-]+)"
    C["WIN_K"] = num(SCORE, rf"const WIN_K: f64 = {f};")
    C["CP_LIMIT"] = num(SCORE, rf"const CP_LIMIT: i32 = {f};")
    acc = re.search(
        r"let raw = ([0-9.]+) \* \(-([0-9.]+) \* \(win_before - win_after\)\)\.exp\(\)\s*- ([0-9.]+)\s*\+ ([0-9.]+);",
        src(SCORE),
    )
    assert acc, "move_accuracy's formula"
    C["ACC_A"], C["ACC_K"], C["ACC_B"], C["ACC_PLUS"] = (float(x) for x in acc.groups())
    drops = [float(x) for x in re.findall(r"drop >= ([0-9.]+)", src(JUDGE))]
    C["DROP_BLUNDER"], C["DROP_MISTAKE"], C["DROP_INACC"] = drops
    win = re.search(r"\(moves / 10\)\.clamp\((\d+), (\d+)\)", src(ACC))
    wts = re.search(r"\.clamp\(([0-9.]+), ([0-9.]+)\)\);", src(ACC))
    C["WIN_LO"], C["WIN_HI"] = (float(x) for x in win.groups())
    C["WT_LO"], C["WT_HI"] = (float(x) for x in wts.groups())
    C["SHAPE_FLOOR"] = num(POST, rf"pub const SHAPE_FLOOR: f64 = {f};")
    C["MAX_SHAPE"] = num(POST, rf"pub const MAX_SHAPE: f64 = {f};")
    C["P_IMP_DELTA_REL"] = num(POST, rf"pub const P_IMP_DELTA_REL: f64 = {f};")
    C["P_IMP_DRAWS"] = num(POST, rf"pub const P_IMP_DRAWS: usize = {f};")
    C["Z80"] = num(INTERVALS, rf"pub const Z80: f64 = {f};")
    C["LEVEL"] = num(INTERVALS, rf"pub const INTERVAL_LEVEL: f64 = {f};")
    C["HALF_LIFE"] = num(WEIGHTS, rf"pub const HALF_LIFE_DAYS: f64 = {f};")
    C["WINDOW_DAYS"] = num(PRIORITY, rf"pub const WINDOW_DAYS: f64 = {f};")
    C["MIN_GAMES"] = num(PRIORITY, rf"pub const MIN_GAMES: u32 = {f};")
    C["RANK_MIN_EXCESS"] = num(PRIORITY, rf"pub const RANK_MIN_EXCESS_PER_GAME: f64 = {f};")
    C["T_WIN"] = num(OBSERVE, rf"pub const T_WIN: f64 = {f};")
    C["DECISIVE_DW"] = num(DETECTORS, rf"pub const DECISIVE_DW: f64 = {f};")
    C["ELIGIBLE_RADIUS"] = num(DETECTORS, rf"pub const ELIGIBLE_RADIUS: f64 = {f};")
    C["MAX_DISPERSION"] = num(VARIANT, rf"pub const MAX_DISPERSION: f64 = {f};")
    ch = re.search(
        r"pub const CHALLENGER: PriorRule = PriorRule \{\s*prior_games: Some\(([0-9.]+)\),\s*effective_counts: (\w+),\s*dispersion: (\w+),",
        src(VARIANT),
    )
    C["PRIOR_GAMES"] = float(ch.group(1))
    rule = {"effective_counts": ch.group(2), "dispersion": ch.group(3)}
    champion = const(VARIANT, r"pub const CHAMPION: PriorRule = PriorRule::(\w+);")
    C["HAZARD"] = 1.0 / num(BOCPD, rf"pub const DEFAULT_HAZARD: f64 = 1\.0 / {f};")
    C["MAX_RUN"] = num(BOCPD, rf"pub const DEFAULT_MAX_RUN: usize = {f};")
    C["BOCPD_N0"] = num(SERIES, rf"pub const BOCPD_PRIOR_STRENGTH: f64 = {f};")
    C["SERIES_WINDOW"] = num(SERIES, rf"pub const SERIES_WINDOW_GAMES: usize = {f};")
    C["CONFIRM_P"] = num(SERIES, rf"pub const CONFIRM_P_CHANGE: f64 = {f};")
    C["CONFIRM_REL"] = num(SERIES, rf"pub const CONFIRM_REL_CHANGE: f64 = {f};")
    C["CUSUM_FACTOR"] = num(SERIES, rf"pub const CUSUM_FACTOR: f64 = {f};")
    C["RELAPSE_H"] = num(SERIES, rf"pub const RELAPSE_CUSUM_H: f64 = {f};")
    C["CUSUM_H"] = num(CUSUM, rf"pub const CUSUM_DEFAULT_H: f64 = {f};")
    C["ACTIVE_CONF"] = num(STATES, rf"pub const ACTIVE_CONFIDENCE: f64 = {f};")
    C["IMPROVING_P"] = num(STATES, rf"pub const IMPROVING_P_IMP: f64 = {f};")
    C["RESOLVE_STREAK"] = num(STATES, rf"pub const RESOLVE_STREAK: u32 = {f};")
    C["TRAJ_W"] = num(TRAJ, rf"pub const TRAJECTORY_WINDOW: usize = {f};")
    C["TRAJ_MIN"] = num(TRAJ, rf"pub const MIN_GAMES: usize = {f};")
    traj_rule = const(TRAJ, r"pub const PRODUCTION: RatingRule = RatingRule::(\w+);")
    C["RELATIVE_STEP"] = num(GOAL, rf"export const RELATIVE_STEP = {f};")
    C["MIN_PEER_STEP"] = num(GOAL, rf"export const MIN_PEER_STEP = {f};")
    C["MIN_VERDICT"] = num(GOAL, rf"export const MIN_VERDICT_CHANCES = {f};")
    C["WIN_BAND"] = num(TRAIN, rf"export const WIN_BAND = {f};")
    C["HOLDOUT"] = num(TRAIN, rf"export const HOLDOUT_PERCENT = {f};")
    C["TRAIN_MIN"] = num(TRAIN, rf"export const MIN_CHANCES = {f};")
    C["RETIRE"] = num(TRAIN, rf"export const RETIRE_PASSES = {f};")
    C["CLEAN_GAP"] = num(TRAIN, rf"export const CLEAN_GAP_DAYS = {f};")
    intervals = [int(x) for x in const(TRAIN, r"export const INTERVALS = \[([0-9, ]+)\]").split(",")]
    C["WEB_Z80"] = num(WEBSTATS, rf"0\.8: {f},")

    n0_block = re.search(r"pub fn n0\(k: WeaknessKey\) -> f64 \{(.*?)\n\}", src(CONSTANTS), re.S)
    n0 = {}
    for keys, value in re.findall(r"^\s*([\w| ]+) => ([0-9.]+),", n0_block.group(1), re.M):
        for k in keys.split("|"):
            n0[k.strip()] = float(value)

    m = json.loads((HERE / "manifest.json").read_text(encoding="utf-8"))
    comp = m["components"]
    check("constants: manifest version", m["version"] == "6.0.0", m["id"])
    check(
        "constants: ranking floor (manifest = priority.rs)",
        comp["recommendation"]["rank_min_excess_per_game"] == C["RANK_MIN_EXCESS"] == 0.02
        and comp["recommendation"]["min_games"] == C["MIN_GAMES"] == 30,
    )
    check(
        "constants: W5 model loss (manifest = (100 - T_WIN)/100)",
        close(comp["recommendation"]["model_loss_per_winning_episode"], (100 - C["T_WIN"]) / 100)
        and comp["profile"]["t_win"] == C["T_WIN"] == 85,
    )
    check("constants: decisive drop", comp["profile"]["decisive_dw"] == C["DECISIVE_DW"] == 15)
    check(
        "constants: trajectory rule",
        comp["profile"]["trajectory_rating_verdict"]["rule"] == "memory-halves"
        and traj_rule == "MemoryHalves"
        and comp["profile"]["trajectory_rating_verdict"]["shown"] is True,
    )
    check("constants: engine nodes", comp["facts"]["engine_nodes"] == 1_000_000)
    check(
        "constants: prior rule is EXP-0006's challenger",
        champion == "CHALLENGER"
        and C["PRIOR_GAMES"] == 2.0
        and rule == {"effective_counts": "true", "dispersion": "true"}
        and C["MAX_DISPERSION"] == 10.0,
    )
    check(
        "constants: n0 table",
        n0
        == {
            "W1": 300.0, "W2": 100.0, "W3Shallow": 60.0, "W3Deep": 60.0, "W4Missed": 40.0,
            "W4Allowed": 40.0, "W5": 30.0, "W6": 30.0, "W7Traps": 40.0, "W7Prep": 40.0,
            "W8Basics": 80.0, "W8Advanced": 80.0, "W9a": 30.0, "W9b": 40.0, "W9c": 60.0,
            "W9d": 20.0, "W10": 600.0, "W11": 300.0, "W12": 60.0,
        },
        f"{len(n0)} keys",
    )
    check(
        "constants: z_0.80 (Rust = web = Phi^-1(0.9))",
        C["Z80"] == C["WEB_Z80"] and close(C["Z80"], stats.norm.ppf(0.9), 1e-15),
    )
    check("constants: trainer intervals", intervals == [1, 3, 7, 21, 60], str(intervals))
    check(
        "constants: the rest as MATH.md states them",
        C["WIN_K"] == 0.00368208 and C["CP_LIMIT"] == 1000 and C["HALF_LIFE"] == 60
        and C["WINDOW_DAYS"] == 183 and C["LEVEL"] == 0.8 and C["HAZARD"] == 1 / 200
        and C["BOCPD_N0"] == 50 and C["CUSUM_H"] == 4 and C["RELAPSE_H"] == 10
        and C["CUSUM_FACTOR"] == 1.5 and C["CONFIRM_P"] == 0.8 and C["CONFIRM_REL"] == 0.15
        and C["ACTIVE_CONF"] == 0.7 and C["IMPROVING_P"] == 0.85 and C["TRAJ_W"] == 30
        and C["TRAJ_MIN"] == 10 and C["RELATIVE_STEP"] == 0.25 and C["MIN_PEER_STEP"] == 0.1
        and C["MIN_VERDICT"] == 10 and C["WIN_BAND"] == 5 and C["HOLDOUT"] == 20
        and C["TRAIN_MIN"] == 30 and C["RETIRE"] == 3 and C["CLEAN_GAP"] == 7
        and C["P_IMP_DELTA_REL"] == 0.15 and C["P_IMP_DRAWS"] == 10_000
        and (C["DROP_BLUNDER"], C["DROP_MISTAKE"], C["DROP_INACC"]) == (0.3, 0.2, 0.1)
        and (C["WIN_LO"], C["WIN_HI"], C["WT_LO"], C["WT_HI"]) == (2, 8, 0.5, 12),
    )
    # The build itself, when the lab image's cli is this build.
    cli = shutil.which("cli")
    if cli:
        out = subprocess.run([cli, "algo", "show", "--json"], capture_output=True, text=True)
        try:
            live = json.loads(out.stdout)
        except json.JSONDecodeError:
            live = None
        if live and live.get("id") == m["id"]:
            check("constants: cli algo show --json = manifest.json", live["components"] == comp)
        else:
            got = live.get("id") if live else out.stderr.strip()[:80]
            print(f"note constants: the image's cli is {got}, not {m['id']}; manifest.json used")
    else:
        print("note constants: no cli on PATH; manifest.json and the sources used")
    return n0


# ---- shared formulas (MATH.md Part A, as the code computes them) ------------------------------


def win_percent(cp: float) -> float:
    cp = max(-C["CP_LIMIT"], min(C["CP_LIMIT"], cp))
    return 50 + 50 * (2 / (1 + math.exp(-C["WIN_K"] * cp)) - 1)


def move_accuracy(before: float, after: float) -> float:
    if after >= before:
        return 100.0
    raw = C["ACC_A"] * math.exp(-C["ACC_K"] * (before - after)) - C["ACC_B"] + C["ACC_PLUS"]
    return min(100.0, max(0.0, raw))


def game_accuracy(w: list[float], white: bool = True) -> float | None:
    n = len(w)
    moves = n - 1
    window = min(max(moves // 10, 2), 8)
    acc, wts = [], []
    for i in range(moves):
        if (i % 2 == 0) != white:
            continue
        b, a = (w[i], w[i + 1]) if white else (100 - w[i], 100 - w[i + 1])
        acc.append(move_accuracy(b, a))
        start = min(max(i - (window - 2), 0), max(n - window, 0))
        seg = w[start : min(start + window, n)]
        wts.append(min(max(float(np.std(seg)), 0.5), 12.0))
    if not acc:
        return None
    weighted = sum(x * y for x, y in zip(acc, wts)) / sum(wts)
    harmonic = len(acc) / sum(1 / max(x, 1) for x in acc)
    return (weighted + harmonic) / 2


def beta_posterior(k: float, n: float, mu: float, n0: float) -> dict:
    a = n0 * mu + k
    b = n0 * (1 - mu) + n - k
    a = a if a > 0 else C["SHAPE_FLOOR"]
    b = b if b > 0 else C["SHAPE_FLOOR"]
    mean = a / (a + b)
    lo = min(stats.beta.ppf(0.1, a, b), mean)
    hi = max(stats.beta.ppf(0.9, a, b), mean)
    return {"alpha": a, "beta": b, "mean": mean, "low": lo, "high": hi}


def weight(age: float) -> float:
    return 0.5 ** (max(age, 0.0) / C["HALF_LIFE"])


def data_counts(points: list[tuple[float, float, float]]) -> tuple[float, float]:
    """(k~, n~) of the champion rule: Kish's effective count over the Pearson dispersion."""
    k = sum(weight(a) * e for e, o, a in points)
    n = sum(weight(a) * o for e, o, a in points)
    if n <= 0:
        return k, n
    s2 = sum(weight(a) ** 2 * o for e, o, a in points)
    rate = min(max(k / n, 0.0), 1.0)
    n_eff = n * n / s2
    var = rate * (1 - rate)
    games = [(e, o) for e, o, a in points if o > 0]
    phi = 1.0
    if len(games) >= 2 and var > 0:
        chi2 = sum((e - o * rate) ** 2 / (o * var) for e, o in games)
        phi = min(max(chi2 / (len(games) - 1), 1.0), C["MAX_DISPERSION"])
    n_eff /= phi
    return rate * n_eff, n_eff


def memory(r: list[float]) -> tuple[float, float] | None:
    steps = np.diff(np.asarray(r, dtype=float))
    rms = math.sqrt(float(np.sum(steps**2)) / len(steps))
    if not rms > 0:
        return None
    a = min(math.log(10) / 400 * rms / 2, 1.0)
    return rms * rms / (a * (2 - a)), min(max(1 - a, 0.0), 1 - 1e-12)


def block(phi: float, m: int) -> float:
    return m * (1 + phi) / (1 - phi) - 2 * phi * (1 - phi**m) / (1 - phi) ** 2


def diff_var(s2: float, phi: float, a: range, b: range) -> float:
    ma, mb = len(a), len(b)
    gap = b.start + 1 - a.stop
    cross = phi**gap * (1 - phi**ma) * (1 - phi**mb) / (1 - phi) ** 2
    return max(s2 * block(phi, ma) / ma**2 + s2 * block(phi, mb) / mb**2 - 2 * s2 * cross / (ma * mb), 0.0)


def js_round(x: float) -> float:
    return math.floor(x + 0.5)


def tenth(v: float) -> float:
    return js_round(v * 1000 + 1e-9) / 1000


def choose_target(baseline: float, peer: float | None) -> dict | None:
    if not (0 < baseline <= 1):
        return None
    relative = tenth(baseline * (1 - C["RELATIVE_STEP"]))
    p = tenth(peer) if peer is not None and 0 <= peer <= baseline * (1 - C["MIN_PEER_STEP"]) else None
    if p is not None and p > relative:
        return {"rate": p, "basis": "peer"}
    return {"rate": relative, "basis": "relative"}


def wilson(s: float, n: float, z: float) -> tuple[float, float] | None:
    if n <= 0:
        return None
    s = min(max(s, 0), n)
    p = s / n
    z2 = z * z
    den = 1 + z2 / n
    centre = (p + z2 / (2 * n)) / den
    half = z * math.sqrt(p * (1 - p) / n + z2 / (4 * n * n)) / den
    return (0.0 if s == 0 else max(0.0, centre - half), 1.0 if s == n else min(1.0, centre + half))


def fnv1a(text: str) -> int:
    h = 0x811C9DC5
    for unit in text.encode("utf-16-le")[::2]:
        h ^= unit
        h = (h * 0x01000193) & 0xFFFFFFFF
    return h


# ---- 2. symbolic -----------------------------------------------------------------------------


def symbolic() -> None:
    x, k, c, d = sp.symbols("x k c d", real=True)
    # A1: chances = tanh(kx/2), W = 100 / (1 + e^{-kx}), W(-x) = 100 - W(x).
    chances = 2 / (1 + sp.exp(-k * x)) - 1
    W = 50 + 50 * chances
    check("A1 symbolic: chances = tanh(k x / 2)", sp.simplify((chances - sp.tanh(k * x / 2)).rewrite(sp.exp)) == 0)
    check("A1 symbolic: W = 100 / (1 + e^{-k x})", sp.simplify(W - 100 / (1 + sp.exp(-k * x))) == 0)
    check("A1 symbolic: W(-x) = 100 - W(x)", sp.simplify(W.subs(x, -x) - (100 - W)) == 0)
    check("A1 symbolic: dW/dx at 0 is 25 k", sp.simplify(sp.diff(W, x).subs(x, 0) - 25 * k) == 0)
    # A2: a drop of the chances by c is a drop of W by 50 c.
    c1, c2 = sp.symbols("c1 c2", real=True)
    check("A2 symbolic: Delta W = 50 Delta chances", sp.expand((50 + 50 * c1) - (50 + 50 * c2) - 50 * (c1 - c2)) == 0)
    # A3: the accuracy curve's two breakpoints.
    A, K, B = (sp.nsimplify(C[s], rational=True) for s in ("ACC_A", "ACC_K", "ACC_B"))
    raw = A * sp.exp(-K * d) - B + 1
    d100 = sp.log(A / (99 + B)) / K
    d0 = sp.log(A / (B - 1)) / K
    check("A3 symbolic: raw(d_100) = 100, raw(d_0) = 0", sp.simplify(raw.subs(d, d100) - 100) == 0 and sp.simplify(raw.subs(d, d0)) == 0)
    check("A3 symbolic: raw is strictly decreasing", sp.simplify(sp.diff(raw, d) + A * K * sp.exp(-K * d)) == 0)
    # A4: AM >= HM, so the harmonic half lies at or below the plain mean (grid: 20,000 vectors).
    rng = np.random.default_rng(4)
    ok = True
    for _ in range(20_000):
        a = rng.uniform(0, 100, rng.integers(1, 40))
        h = len(a) / np.sum(1 / np.maximum(a, 1))
        ok &= h <= np.mean(np.maximum(a, 1)) + 1e-9 and np.min(np.maximum(a, 1)) - 1e-9 <= h
    check("A4 grid: min <= harmonic <= arithmetic mean (of max(a, 1))", bool(ok), "20,000 vectors")
    # A5: posterior mean as a mixture, and the prior's share under the cap.
    s, n, kk, mu, g, n0 = sp.symbols("s n k mu g n0", positive=True)
    mean = (s * mu + kk) / (s + n)
    lam = s / (s + n)
    check("A5 symbolic: (s mu + k)/(s + n) = lambda mu + (1 - lambda) k/n", sp.simplify(mean - (lam * mu + (1 - lam) * kk / n)) == 0)
    check("A5 symbolic: lambda at s = 2n/g is 2/(g + 2)", sp.simplify(lam.subs(s, 2 * n / g) - 2 / (g + 2)) == 0)
    check("A5 symbolic: d lambda / ds = n/(s + n)^2 > 0", sp.simplify(sp.diff(lam, s) - n / (s + n) ** 2) == 0)
    # A5: Kish's bounds, by Lagrange's identity (three games, symbolic weights and chances).
    w = sp.symbols("w1:4", positive=True)
    cc = sp.symbols("c1:4", positive=True)
    lhs = sum(cc) * sum(wi**2 * ci for wi, ci in zip(w, cc)) - sum(wi * ci for wi, ci in zip(w, cc)) ** 2
    rhs = sum(cc[i] * cc[j] * (w[i] - w[j]) ** 2 for i in range(3) for j in range(i + 1, 3))
    check("A5 symbolic: Lagrange's identity behind n_eff <= sum c", sp.expand(lhs - rhs) == 0)
    # A5: the Pearson decomposition (EXP-0006 Result 2(a)), three games.
    e = sp.symbols("e1:4", real=True)
    p = sp.symbols("p", real=True)
    ebar = sum(e) / sum(cc)
    left = sum((ei - ci * p) ** 2 / ci for ei, ci in zip(e, cc))
    right = sum((ei - ci * ebar) ** 2 / ci for ei, ci in zip(e, cc)) + sum(cc) * (ebar - p) ** 2
    check("A5 symbolic: Pearson decomposition", sp.simplify(left - right) == 0)
    # A7: W5's benchmark and excess.
    th, ee = sp.symbols("theta e", positive=True)
    check("A7 symbolic: e (theta - 0.15/e) = e theta - 0.15", sp.simplify(ee * (th - sp.Rational(15, 100) / ee) - (ee * th - sp.Rational(15, 100))) == 0)
    # A9: the BOCPD predictive is the Beta-binomial (exact integral at rational shapes).
    tt = sp.symbols("t", positive=True)
    for a_, b_, n_, k_ in [(sp.Rational(1, 2), 3, 4, 1), (2, 5, 6, 3), (sp.Rational(5, 2), sp.Rational(3, 2), 3, 0)]:
        integral = sp.integrate(sp.binomial(n_, k_) * tt**k_ * (1 - tt) ** (n_ - k_) * tt ** (a_ - 1) * (1 - tt) ** (b_ - 1), (tt, 0, 1)) / sp.beta(a_, b_)
        closed = sp.binomial(n_, k_) * sp.beta(a_ + k_, b_ + n_ - k_) / sp.beta(a_, b_)
        gap = abs(sp.N(integral - closed, 40))
        check(f"A9 symbolic: Beta-binomial predictive a={a_} b={b_} n={n_} k={k_}", gap < 1e-30, f"exact integral, |difference| {float(gap):.1e} at 40 digits")
    # A9: with no evidence, P(r < L) = 1 - (1 - H)^L.
    H, j, L = sp.symbols("H j L", positive=True)
    geo = sp.summation(H * (1 - H) ** j, (j, 0, L - 1))
    check("A9 symbolic: sum_{j<L} H (1-H)^j = 1 - (1-H)^L", sp.simplify(geo - (1 - (1 - H) ** L)) == 0)
    # A10: the CUSUM increment's mean is -n KL(p0 || p1) < 0 (sign on a grid).
    p0, p1, nn = sp.symbols("p0 p1 n", positive=True)
    inc = nn * (p0 * sp.log(p1 / p0) + (1 - p0) * sp.log((1 - p1) / (1 - p0)))
    kl = p0 * sp.log(p0 / p1) + (1 - p0) * sp.log((1 - p0) / (1 - p1))
    check("A10 symbolic: E[increment] = -n KL(p0||p1)", sp.simplify(sp.expand_log(inc + nn * kl, force=True)) == 0)
    grid = [(a_, b_) for a_ in np.linspace(0.001, 0.6, 120) for b_ in [1.5 * a_] if b_ < 1]
    check("A10 grid: KL(p0 || 1.5 p0) > 0", all(a_ * math.log(a_ / b_) + (1 - a_) * math.log((1 - a_) / (1 - b_)) > 0 for a_, b_ in grid), f"{len(grid)} points")
    # A12: the block sum and the cross term are the double sums (exact in phi; EXP-0030 R5, R6).
    ph = sp.symbols("phi", positive=True)
    ok = True
    for m in range(1, 16):
        dbl = sum(ph ** abs(i - jj) for i in range(m) for jj in range(m))
        closed = m * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m) / (1 - ph) ** 2
        ok &= sp.simplify(sp.cancel(dbl - closed)) == 0
    check("A12 symbolic: S(m) = sum phi^|i-j|, m = 1..15", bool(ok))
    ok = True
    for (a0, a1, b0, b1) in [(0, 5, 5, 9), (0, 4, 7, 12), (2, 6, 10, 13)]:
        dbl = sum(ph ** (jj - i) for i in range(a0, a1) for jj in range(b0, b1))
        closed = ph ** (b0 - a1 + 1) * (1 - ph ** (a1 - a0)) * (1 - ph ** (b1 - b0)) / (1 - ph) ** 2
        ok &= sp.simplify(sp.cancel(dbl - closed)) == 0
    check("A12 symbolic: the cross term, three layouts", bool(ok))
    # A12: V = w' Gamma w for one layout, exactly in phi.
    n_, A_, B_ = 10, range(0, 4), range(6, 10)
    wv = sp.Matrix([(-sp.Rational(1, len(A_)) if i in A_ else (sp.Rational(1, len(B_)) if i in B_ else 0)) for i in range(n_)])
    G = sp.Matrix(n_, n_, lambda i, jj: ph ** abs(i - jj))
    quad = (wv.T * G * wv)[0]
    Sf = lambda m: m * (1 + ph) / (1 - ph) - 2 * ph * (1 - ph**m) / (1 - ph) ** 2
    crossf = ph ** (B_.start - A_.stop + 1) * (1 - ph ** len(A_)) * (1 - ph ** len(B_)) / (1 - ph) ** 2
    V = Sf(len(A_)) / len(A_) ** 2 + Sf(len(B_)) / len(B_) ** 2 - 2 * crossf / (len(A_) * len(B_))
    check("A12 symbolic: V / sigma^2 = w' Gamma w", sp.simplify(sp.cancel(quad - V)) == 0)
    # A12: the stationary variance, sigma^2 = s^2/(a(2 - a)).
    a = sp.symbols("a", positive=True)
    check("A12 symbolic: 1 - (1 - a)^2 = a (2 - a)", sp.expand(1 - (1 - a) ** 2 - a * (2 - a)) == 0)


# ---- 3. simulation ---------------------------------------------------------------------------


def within(name: str, got: float, want: float, se: float, detail: str = "") -> None:
    z = (got - want) / se if se > 0 else (0.0 if got == want else math.inf)
    check(name, abs(z) < 4, f"{got:.5f} against {want:.5f} (z {z:+.2f}){'; ' + detail if detail else ''}")


def simulation() -> None:
    rng = np.random.default_rng(20261128)
    # A6: credible implies confidence >= 0.9 (every draw), and the Bayesian false-weakness bound.
    mu, n0, nchance, reps = 0.02, 300.0, 400, 200_000
    theta = rng.beta(n0 * mu, n0 * (1 - mu), reps)
    k = rng.binomial(nchance, theta)
    a, b = n0 * mu + k, n0 * (1 - mu) + nchance - k
    low = stats.beta.ppf(0.1, a, b)
    flagged = low > mu
    conf = stats.beta.sf(mu, a, b)
    check("A6 simulation: credible implies confidence >= 0.9", bool(np.all(conf[flagged] >= 0.9 - 1e-12)), f"{flagged.sum()} flagged draws")
    share = a6_share = float(np.mean(theta[flagged] <= mu))
    bound = float(np.mean(stats.beta.cdf(mu, a[flagged], b[flagged])))
    se = math.sqrt(share * (1 - share) / flagged.sum())
    check("A6 simulation: P(theta <= m | flagged) = E[P(theta <= m | data) | flagged] < 0.1", share < 0.1 and abs(share - bound) < 4 * se, f"{share:.4f}, posterior average {bound:.4f}")
    # A6, frequentist: for a fixed theta <= m, P(flag) <= P(L > theta).
    for th in (0.01, 0.02):
        kk = rng.binomial(nchance, th, 100_000)
        lo = stats.beta.ppf(0.1, n0 * mu + kk, n0 * (1 - mu) + nchance - kk)
        check(f"A6 simulation: theta {th}, P(L > m) <= P(L > theta)", np.mean(lo > mu) <= np.mean(lo > th), f"{np.mean(lo > mu):.4f} <= {np.mean(lo > th):.4f}")
    # A5: Var[p_hat | c] = theta(1 - theta)/n_eff (Kish), unit dispersion.
    c = rng.poisson(20, 60) + 1
    ages = np.arange(60)[::-1] * 2.0
    w = 0.5 ** (ages / 60)
    th = 0.1
    e = rng.binomial(c, th, size=(40_000, len(c)))
    phat = (e * w).sum(1) / (w * c).sum()
    neff = (w * c).sum() ** 2 / (w**2 * c).sum()
    v = phat.var(ddof=1)
    want = th * (1 - th) / neff
    within("A5 simulation: Var[p_hat] = theta (1 - theta) / n_eff", v / want, 1.0, math.sqrt(2 / 39_999))
    # A12: the memory verdict's error rates under a Gaussian AR(1) with known parameters.
    phi, sigma, n, W = 0.984, 31.0, 120, 30
    players = 20_000
    eps = rng.normal(0, sigma * math.sqrt(1 - phi**2), (players, n))
    r = np.empty((players, n))
    r[:, 0] = rng.normal(0, sigma, players)
    for t in range(1, n):
        r[:, t] = phi * r[:, t - 1] + eps[:, t]
    D = r[:, n - W :].mean(1) - r[:, :W].mean(1)
    V = diff_var(sigma**2, phi, range(0, W), range(n - W, n))
    within("A12 simulation: Var(D) = V", D.var(ddof=1) / V, 1.0, math.sqrt(2 / (players - 1)))
    h = C["Z80"] * math.sqrt(V)
    fall, rise = float(np.mean(D + h < 0)), float(np.mean(D - h > 0))
    within("A12 simulation: P(falling) = 0.10, known parameters", fall, 0.10, math.sqrt(0.09 / players))
    within("A12 simulation: P(rising) = 0.10, known parameters", rise, 0.10, math.sqrt(0.09 / players))
    # A12, plug-in: the same players with the plug-in memory (the shipped rule) stay near 0.10.
    plug = []
    for i in range(4_000):
        mem = memory(list(r[i] + 1500))
        hv = C["Z80"] * math.sqrt(diff_var(mem[0], mem[1], range(0, W), range(n - W, n)))
        plug.append(D[i] + hv < 0)
    pf = float(np.mean(plug))
    check("A12 simulation: plug-in falling rate under the 0.15 bound (AR(1) world)", pf < 0.15, f"{pf:.4f} of 4,000 (EXP-0030 measures the Elo worlds)")
    # A10: Page's CUSUM with known p0 has an in-control run length of at least e^h (Lorden).
    p0, nn = 0.02, 50
    p1 = C["CUSUM_FACTOR"] * p0
    we, wm = math.log(p1 / p0), math.log((1 - p1) / (1 - p0))
    runs = []
    for _ in range(3_000):
        s_, t_ = 0.0, 0
        while True:
            t_ += 1
            kk = rng.binomial(nn, p0)
            s_ = max(0.0, s_ + kk * we + (nn - kk) * wm)
            if s_ > C["CUSUM_H"] or t_ > 100_000:
                break
        runs.append(t_)
    arl = float(np.mean(runs))
    check("A10 simulation: in-control ARL >= e^h (h = 4)", arl >= math.exp(C["CUSUM_H"]), f"ARL {arl:.1f} >= {math.exp(C['CUSUM_H']):.1f} games")
    # A13: the focus verdict by luck: P(on track | rate unchanged) is the binomial cdf.
    for n_ in (10, 20):
        target = choose_target(0.2, None)["rate"]
        x = rng.binomial(n_, 0.2, 400_000)
        sim = float(np.mean(x / n_ <= target))
        exact = float(stats.binom.cdf(math.floor(target * n_ + 1e-9), n_, 0.2))
        within(f"A13 simulation: P(on track | 0.2 unchanged), {n_} chances", sim, exact, math.sqrt(exact * (1 - exact) / 400_000))
    # A14: two non-overlapping 80% Wilson ranges when nothing changed.
    for n_, p_ in ((30, 0.2), (2_000, 0.2)):
        x1, x2 = rng.binomial(n_, p_, 200_000), rng.binomial(n_, p_, 200_000)
        claims = 0
        cache: dict[int, tuple[float, float]] = {}
        for v in np.unique(np.concatenate([x1, x2])):
            cache[int(v)] = wilson(float(v), n_, C["Z80"])
        lo1 = np.array([cache[int(v)][0] for v in x1]); hi1 = np.array([cache[int(v)][1] for v in x1])
        lo2 = np.array([cache[int(v)][0] for v in x2]); hi2 = np.array([cache[int(v)][1] for v in x2])
        claims = float(np.mean((hi2 < lo1) | (lo2 > hi1)))
        exact = trainer_exact(n_, p_) if n_ <= 200 else 2 * (1 - stats.norm.cdf(C["Z80"] * math.sqrt(2)))
        within(f"A14 simulation: false claim rate, {n_} vs {n_} chances", claims, exact, math.sqrt(exact * (1 - exact) / 200_000) + (0.002 if n_ > 200 else 0))
    # A14: the holdout share over 200,000 position keys.
    keys = [f"holdout:g{i // 80}:{i % 80}" for i in range(200_000)]
    share = sum(fnv1a(s_) % 100 < C["HOLDOUT"] for s_ in keys) / len(keys)
    within("A14 simulation: holdout share", share, 0.2, math.sqrt(0.16 / len(keys)), "fnv1a over 200,000 keys")
    # A8: P_imp's Monte Carlo error at 10,000 draws is at most 0.005.
    check("A8 simulation: P_imp Monte Carlo SE <= 1/(2 sqrt(10000))", 0.5 / math.sqrt(C["P_IMP_DRAWS"]) <= 0.005 + 1e-15)
    # A4: game accuracy lies between the smallest and the largest floored move accuracy.
    ok = True
    for _ in range(2_000):
        cps = np.cumsum(rng.normal(0, 120, rng.integers(3, 90)))
        wp = [win_percent(float(x)) for x in cps]
        for white in (True, False):
            ga = game_accuracy(wp, white)
            first = 0 if white else 1
            accs = [
                move_accuracy(wp[i], wp[i + 1]) if white else move_accuracy(100 - wp[i], 100 - wp[i + 1])
                for i in range(first, len(wp) - 1, 2)
            ]
            if ga is not None:
                ok &= min(accs) - 1e-9 <= ga <= max(max(a_, 1) for a_ in accs) + 1e-9
    check("A4 simulation: min a <= game accuracy <= max max(1, a), both colours", bool(ok), "2,000 random games")
    # The numbers MATH.md quotes from this seed's runs (A6, A10, A12).
    quoted = {
        "A6 flagged share 0.038": round(a6_share, 3) == 0.038,
        "A6 posterior average 0.036": round(bound, 3) == 0.036,
        "A10 run length 864": round(arl) == 864,
        "A12 Var(D)/V 0.986": round(D.var(ddof=1) / V, 3) == 0.986,
        "A12 falling 0.0985, rising 0.1016": round(fall, 4) == 0.0985 and round(rise, 4) == 0.1016,
        "A12 plug-in falling 0.105": round(pf, 3) == 0.105,
    }
    check("simulation: the quoted values of this seed", all(quoted.values()), ", ".join(k for k, v in quoted.items() if not v) or "all")


def trainer_exact(n: int, p: float) -> float:
    pm = stats.binom.pmf(np.arange(n + 1), n, p)
    iv = [wilson(float(x), n, C["Z80"]) for x in range(n + 1)]
    tot = 0.0
    for x1 in range(n + 1):
        for x2 in range(n + 1):
            if iv[x2][1] < iv[x1][0] or iv[x2][0] > iv[x1][1]:
                tot += pm[x1] * pm[x2]
    return tot


# ---- 4. build: the golden responses, recomputed -----------------------------------------------

KEYMAP = {"W1": "W1", "W5": "W5", "W9b": "W9b", "W4.missed": "W4Missed"}


def parse_time(s: str) -> datetime:
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def build(n0: dict) -> None:
    fx = ROOT / "lab" / "fixtures"
    req = json.loads((fx / "profile-request.json").read_text(encoding="utf-8"))
    res = json.loads((fx / "golden" / "profile-response.json").read_text(encoding="utf-8"))
    tr_rows = {"B0": 0, "B1": 1, "B2": 2, "B3": 3, "B4": 4, "B5": 5}
    checked = 0
    for player, out in zip(req["players"], res["players"]):
        now = parse_time(player["now"])
        g = player["games_in_window"]
        est = {e["key"]: e for e in out["estimates"]}
        for key, pts in player["series"].items():
            points = []
            for p in pts:
                age = (now - parse_time(p["played_at"])).total_seconds() / 86_400
                if age <= C["WINDOW_DAYS"]:
                    points.append((float(p["events"]), float(p["opportunities"]), age))
            kt, nt = data_counts(points)
            e = est[key]
            ref = player["reference"].get(f"weakness:{key}")
            model = ref is None and key in player.get("model_reference", {})
            if model:
                ref = player["model_reference"][key]
                mu, s0 = 0.5, 2.0
            elif ref is not None:
                mu = ref["pooledRate"]
                s0 = min(n0[KEYMAP[key]], C["PRIOR_GAMES"] * nt / g)
            else:
                mu, s0 = 0.5, 2.0
            post = beta_posterior(kt, nt, mu, s0)
            ok = close(e["k"], kt) and close(e["n"], nt)
            ok &= all(close(e["posterior"][f], post[f]) for f in ("alpha", "beta", "mean"))
            ok &= close(e["posterior"]["interval"]["low"], post["low"], 1e-7) and close(e["posterior"]["interval"]["high"], post["high"], 1e-7)
            k_raw = sum(x[0] for x in points)
            n_raw = sum(x[1] for x in points)
            if ref is not None:
                median = ref["median"]
                conf = float(stats.beta.sf(median, post["alpha"], post["beta"])) if k_raw > 0 else 0.0
                excess = max(0.0, post["mean"] - median) * (n_raw / g) * ref["eplPerEvent"]
                credible = g >= C["MIN_GAMES"] and k_raw > 0 and post["low"] > median and excess >= C["RANK_MIN_EXCESS"]
                prio = excess * e["trainability"] * conf if credible else 0.0
                ok &= close(e["confidence"], conf, 1e-7, 1e-9) and close(e["excess"], excess) and e["credible"] == credible
                ok &= close(e["priority"], prio, 1e-7, 1e-12) and e["modelBaseline"] == model
                if credible:
                    ok &= e["confidence"] >= 0.9
            check(f"A5-A7 build: {player['id']} {key} estimate = MATH.md's formulas", ok, f"k~ {kt:.4f}, n~ {nt:.2f}, s0 {s0:.3f}")
            checked += 1
    check("A5-A7 build: estimates recomputed", checked == 5, f"{checked} estimates")
    # A12: the trajectory's rating rows under memory-halves.
    req = json.loads((fx / "trajectory-request.json").read_text(encoding="utf-8"))
    res = json.loads((fx / "golden" / "trajectory-response.json").read_text(encoding="utf-8"))
    for player, out in zip(req["players"], res["players"]):
        games = sorted(player["games"], key=lambda x: (x["played_at"], x["id"]))
        own = [float(x["rating"]) for x in games]
        # chess.com stores the rating after the game: the pre-game rating is the previous one's.
        r = [own[0]] + own[:-1] if player.get("chesscom") else own
        n = len(r)
        t = out["trajectory"]
        if n < 2 * C["TRAJ_MIN"]:
            check(f"A12 build: {player['id']} has no verdict below 20 games", t.get("sufficient") is False and t.get("kind") is None, f"{n} games")
            continue
        W = int(min(C["TRAJ_W"], n // 2))
        A, B = range(0, W), range(n - W, n)
        ma, mb = float(np.mean(r[: W])), float(np.mean(r[n - W :]))
        s2, phi = memory(r)
        h = C["Z80"] * math.sqrt(diff_var(s2, phi, A, B))
        side = lambda m, mm: (m - C["Z80"] * math.sqrt(s2 * block(phi, mm) / mm**2), m + C["Z80"] * math.sqrt(s2 * block(phi, mm) / mm**2))
        row = t["thenVsNow"]["rating"]
        verdict = "better" if ma < mb and (mb - ma) - h > 0 else ("worse" if (mb - ma) + h < 0 else "no_clear_change")
        kind = {"better": "rising", "worse": "falling", "no_clear_change": "flat"}[verdict]
        ok = close(row["then"]["value"], ma) and close(row["now"]["value"], mb)
        ok &= close(row["diff"], mb - ma) and close(row["diffLow"], mb - ma - h) and close(row["diffHigh"], mb - ma + h)
        ok &= all(close(row[s]["low"], side(m, W)[0]) and close(row[s]["high"], side(m, W)[1]) for s, m in (("then", ma), ("now", mb)))
        ok &= row["verdict"] == verdict and t["kind"] == kind and t["change"] == js_round(mb - ma)
        check(f"A12 build: {player['id']} rating row = memory-halves", ok, f"W {W}, D {mb - ma:.2f}, half-width {h:.2f}, {kind}")
    # A13: the focus target and progress.
    req = json.loads((fx / "focus-target-request.json").read_text(encoding="utf-8"))
    res = json.loads((fx / "golden" / "focus-target-response.json").read_text(encoding="utf-8"))
    ok = all(choose_target(i["baseline"], i["peer"]) == o["target"] for i, o in zip(req["items"], res["items"]))
    check("A13 build: chooseTarget on the golden items", ok, f"{len(req['items'])} items")
    req = json.loads((fx / "focus-progress-request.json").read_text(encoding="utf-8"))
    res = json.loads((fx / "golden" / "focus-progress-response.json").read_text(encoding="utf-8"))
    ok = True
    for i, o in zip(req["items"], res["items"]):
        n_, k_ = max(0, int(i["n"])), min(max(0, int(i["k"])), max(0, int(i["n"])))
        enough = n_ >= C["MIN_VERDICT"]
        below = enough and k_ / n_ <= i["target"]
        verdict = "too_early" if not enough else ("on_track" if below else "not_yet")
        done = min(i["games"], i["games_n"]) >= i["games_n"]
        outcome = None if not done else ("inconclusive" if not enough else ("met" if below else "not_met"))
        got = o["progress"] if "progress" in o else o
        ok &= got["verdict"]["kind"] == verdict and got["outcome"] == outcome
        iv = wilson(k_, n_, C["Z80"])
        if iv and got.get("interval"):
            ok &= close(got["interval"]["low"], iv[0]) and close(got["interval"]["high"], iv[1])
    check("A13 build: focusProgress on the golden items", ok, f"{len(req['items'])} items")
    # A9: the timeline rows' confidence is P(theta > median) under Beta(n0 mu + k, n0(1-mu) + n - k).
    req = json.loads((fx / "timeline-request.json").read_text(encoding="utf-8"))
    res = json.loads((fx / "golden" / "timeline-response.json").read_text(encoding="utf-8"))
    minev = {"W1": 1000.0, "W9b": 40.0}
    rows = 0
    ok = True
    for player, out in zip(req["players"], res["players"]):
        for key, s in out["series"].items():
            ref = (player.get("reference") or {}).get(f"weakness:{key}")
            if ref is None or key not in KEYMAP:
                continue
            nn0 = n0[KEYMAP[key]]
            for row in s["rows"]:
                k_, n_ = row["evidence"]["k"], row["evidence"]["n"]
                a_, b_ = nn0 * ref["pooledRate"] + k_, nn0 * (1 - ref["pooledRate"]) + n_ - k_
                want = float(stats.beta.sf(ref["median"], a_, b_)) if n_ >= minev[key] and k_ > 0 else 0.0
                ok &= close(row["evidence"]["confidence"], want, 1e-7, 1e-9)
                rows += 1
    check("A9 build: every timeline row's confidence", ok and rows > 0, f"{rows} rows")


def bocpd_exact() -> None:
    """A9: the run-length recursion equals the posterior of the product-partition model."""
    H, a0, b0 = 1 / 200, 1.5, 30.0
    obs = [(1, 20), (0, 20), (4, 20), (3, 20), (0, 20), (5, 20), (6, 20)]

    def lpred(a, b, k, n):
        return math.lgamma(n + 1) - math.lgamma(k + 1) - math.lgamma(n - k + 1) + math.lgamma(a + k) + math.lgamma(b + n - k) - math.lgamma(a + b + n) - (math.lgamma(a) + math.lgamma(b) - math.lgamma(a + b))

    # The recursion of bocpd.rs (no truncation is reached).
    runs = [(0.0, a0, b0)]
    for k, n in obs:
        nxt = [None]
        cp = -math.inf
        for lp, a, b in runs:
            j = lp + lpred(a, b, k, n)
            cp = np.logaddexp(cp, j + math.log(H))
            nxt.append((j + math.log1p(-H), a + k, b + n - k))
        nxt[0] = (cp, a0, b0)
        tot = -math.inf
        for lp, _, _ in nxt:
            tot = np.logaddexp(tot, lp)
        runs = [(lp - tot, a, b) for lp, a, b in nxt]
    post = [math.exp(lp) for lp, _, _ in runs]
    # Brute force: every set of change points after games 1..t; a change after game t gives r = 0.
    t = len(obs)
    mass = [0.0] * (t + 1)
    for cuts in itertools.product([0, 1], repeat=t):
        lp = sum(math.log(H) if c else math.log1p(-H) for c in cuts)
        start = 0
        for i in range(t):
            if cuts[i] or i == t - 1:
                seg = obs[start : i + 1]
                k_, n_ = sum(x[0] for x in seg), sum(x[1] for x in seg)
                lp += sum(lpred(a0 + sum(x[0] for x in seg[:m]), b0 + sum(x[1] - x[0] for x in seg[:m]), seg[m][0], seg[m][1]) for m in range(len(seg)))
                start = i + 1
        r = 0 if cuts[-1] else t - max([i + 1 for i in range(t - 1) if cuts[i]] or [0])
        mass[r] += math.exp(lp)
    total = sum(mass)
    brute = [m / total for m in mass]
    check("A9 exact: the BOCPD recursion = the brute-force run-length posterior", all(close(x, y, 1e-10, 1e-14) for x, y in zip(post, brute)), f"t = {t}, 2^{t} partitions")
    # With no evidence (n = 0 every game), the mass on r < 50 after 60 games is 1 - (1 - H)^50.
    runs = [0.0]
    for _ in range(60):
        runs = [math.log(H)] + [x + math.log1p(-H) for x in runs]
        tot = np.logaddexp.reduce(runs)
        runs = [x - tot for x in runs]
    pc = sum(math.exp(x) for x in runs[:50])
    check("A9 exact: no-evidence p_change = 1 - (1 - H)^50", close(pc, 1 - (1 - H) ** 50), f"{pc:.4f}")


# ---- 5. numbers ------------------------------------------------------------------------------


def numbers() -> None:
    # README's worked example: +271 -> 73.06%, -111 -> 39.92%, Delta W 33.14, accuracy 22.2.
    wb, wa = win_percent(271), win_percent(-111)
    check("numbers: README +271 -> 73.06, -111 -> 39.92", round(wb, 2) == 73.06 and round(wa, 2) == 39.92, f"{wb:.4f}, {wa:.4f}")
    check("numbers: README Delta W 33.14, accuracy 22.2", round(wb - wa, 2) == 33.14 and round(move_accuracy(wb, wa), 1) == 22.2, f"{wb - wa:.4f}, {move_accuracy(wb, wa):.4f}")
    check("numbers: W(100) = 59.10, W(300) = 75.11 (README: about 59% and 75%)", round(win_percent(100), 2) == 59.10 and round(win_percent(300), 2) == 75.11, f"{win_percent(100):.3f}, {win_percent(300):.3f}")
    # Finding 5: the epsilon matters at 0.5005, which floating point puts below the half.
    check("numbers: Finding 5, tenth(0.5005) = 0.501, 0.500 without the epsilon", tenth(0.5005) == 0.501 and js_round(0.5005 * 1000) / 1000 == 0.5)
    # A13(b) with a peer: the worst peer is 0.9 b (tenth is monotone).
    peer_bad = [float(b) for b in np.arange(1, 1_000_001) / 1_000_000 if choose_target(float(b), 0.9 * float(b))["rate"] >= b]
    check("numbers: A13 with a peer, 1,115 baselines from 0.000556 to 0.005", len(peer_bad) == 1115 and round(min(peer_bad), 6) == 0.000556 and round(max(peer_bad), 6) == 0.005, f"{len(peer_bad)}, {min(peer_bad)} to {max(peer_bad)}")
    check("numbers: A13 example b 0.0029, peer 0.0026 gives 0.003", choose_target(0.0029, 0.0026)["rate"] == 0.003)
    # A7: model_baseline's min(0.15/e, 1) and its default e = 1, in the Rust.
    mb = re.search(r"pub fn model_baseline.*?\n\}", src(PRIORITY), re.S).group(0)
    check("constants: A7 model_baseline is min(0.15/e, 1), e defaulting to 1 and capped at 1", ".map_or(1.0, |e| e.min(1.0))" in mb and "(MODEL_LOSS_PER_WINNING_EPISODE / e).min(1.0)" in mb)
    # Part B 5.0.0: EXP-0030 Result 10's ratio at phi = 0.9843.
    ratio = (30**2 - block(0.9843, 30)) / (30 * block(0.9843, 30))
    check("numbers: B 5.0.0 ratio 0.0055, factor 13.5 at phi 0.9843", round(ratio, 4) == 0.0055 and round(1 / math.sqrt(ratio), 1) == 13.5, f"{ratio:.5f}, {1 / math.sqrt(ratio):.2f}")
    d100 = math.log(C["ACC_A"] / (99 + C["ACC_B"])) / C["ACC_K"]
    d0 = math.log(C["ACC_A"] / (C["ACC_B"] - 1)) / C["ACC_K"]
    check("numbers: A3 breakpoints 0.2237 and 88.72", round(d100, 4) == 0.2237 and round(d0, 2) == 88.72, f"{d100:.6f}, {d0:.6f}")
    check("numbers: A4 one zero caps H at 100m/(m + 99) = 28.8 for 40 moves", round(100 * 40 / 139, 1) == 28.8 and close(40 / sum(1 / max(1, a) for a in [0.0] + [100.0] * 39), 100 * 40 / 139))
    check("numbers: A1 slope 0.092 points a centipawn at 0", round(25 * C["WIN_K"], 4) == 0.0921)
    check("numbers: A1 W(1000) = 97.54", round(win_percent(1000), 2) == 97.54, f"{win_percent(1000):.4f}")
    # A5: the prior's share 2/(g + 2).
    check("numbers: A5 prior share at 30, 60, 120, 250 games", [round(2 / (g + 2), 4) for g in (30, 60, 120, 250)] == [0.0625, 0.0323, 0.0164, 0.0079])
    # A9: no-evidence change mass 0.2217 and the CUSUM bounds.
    check("numbers: A9 1 - (199/200)^50 = 0.2217", round(1 - (1 - C["HAZARD"]) ** 50, 4) == 0.2217)
    check("numbers: A10 e^4 = 54.6, e^10 = 22026", round(math.exp(4), 1) == 54.6 and round(math.exp(10)) == 22026)
    # A12: z_0.8, the factor sqrt(2) z and the lag scale.
    check("numbers: A12 2/q = 347.4 points", round(2 / (math.log(10) / 400), 1) == 347.4)
    # A13: the targets that are not below their baseline, and the luck rates.
    grid = np.round(np.arange(1, 1_000_001) / 1_000_000, 6)
    bad = [float(b) for b in grid if choose_target(float(b), None)["rate"] >= b]
    check("numbers: A13 targets not below the baseline lie in (0, 0.002]", max(bad) <= 0.002 and min(bad) > 0 and abs(max(bad) - 0.002) < 1e-12, f"{len(bad)} of 10^6 baselines, largest {max(bad)}")
    above = [b for b in bad if choose_target(b, None)["rate"] > b]
    check("numbers: A13 targets above the baseline lie in [0.000667, 0.001)", min(above) >= 0.000666 and max(above) < 0.001, f"{len(above)}, from {min(above)} to {max(above)}")
    zero = [float(b) for b in grid[:1000] if choose_target(float(b), None)["rate"] == 0]
    check("numbers: A13 target 0 for baselines below 0.000667", max(zero) < 0.000667, f"largest {max(zero)}")
    on10 = stats.binom.cdf(1, 10, 0.2)
    on20 = stats.binom.cdf(3, 20, 0.2)
    check("numbers: A13 luck rates 0.376 (10 chances), 0.411 (20)", round(on10, 3) == 0.376 and round(on20, 3) == 0.411, f"{on10:.4f}, {on20:.4f}")
    # A14: the exact holdout share and the non-overlap rates.
    share = Fraction(20 * math.ceil(2**32 / 100), 2**32)
    check("numbers: A14 holdout 20 x 42,949,673 / 2^32", math.ceil(2**32 / 100) == 42_949_673 and abs(float(share) - 0.2) < 1e-9, f"{float(share):.12f}")
    lim = 2 * (1 - stats.norm.cdf(C["Z80"] * math.sqrt(2)))
    check("numbers: A14 large-sample non-overlap rate 0.070", round(lim, 3) == 0.070, f"{lim:.5f}")
    ex = trainer_exact(30, 0.2)
    check("numbers: A14 30 vs 30 at 0.2: 0.070", round(ex, 3) == 0.070, f"{ex:.5f}")
    days = 0
    sched = {"streak": 0, "clean": 0, "last": None}
    attempts = 0
    while True:
        attempts += 1
        gap = days - sched["last"] if sched["last"] is not None else 0
        clean = sched["last"] is not None and gap >= C["CLEAN_GAP"]
        sched["streak"] += 1
        sched["clean"] += 1 if clean else 0
        sched["last"] = days
        if sched["clean"] >= C["RETIRE"]:
            break
        days += [1, 3, 7, 21, 60][min(sched["streak"], 5) - 1]
    check("numbers: A14 an always-right, sure item retires on day 92, attempt 6", days == 92 and attempts == 6, f"day {days}, attempt {attempts}")
    # Part B: numbers quoted from the experiments, recomputed from their committed results.
    r30 = json.loads((ROOT / "lab/experiments/EXP-0030/results.json").read_text(encoding="utf-8"))
    prim = {(c["games"], c["side"]): c for c in r30["candidates"]["memory-halves"]["primary"]}
    zf = stats.norm.ppf(1 - 0.025 / 2 / 2)
    ups = {key: wilson(c["k"], c["n"], zf)[1] for key, c in prim.items()}
    quoted = {(60, "falling"): 0.103, (120, "falling"): 0.117, (250, "falling"): 0.105, (60, "rising"): 0.089, (120, "rising"): 0.084, (250, "rising"): 0.095}
    half_up = lambda k_, n_: math.floor(Fraction(k_ * 1000, n_) + Fraction(1, 2)) / 1000
    ok = all(half_up(prim[key]["k"], prim[key]["n"]) == v for key, v in quoted.items())
    check("numbers: B 6.0.0 false-alarm shares (EXP-0030 results.json)", ok and max(ups.values()) <= 0.15 and round(max(ups.values()), 3) == 0.136, f"worst upper end {max(ups.values()):.4f}")
    cpu = 300 * 39.1 / 3600
    check("numbers: B 4.0.0 300 games = 3.3 CPU-hours, 49 minutes on 4 engines", round(cpu, 1) == 3.3 and round(cpu / 4 * 60) == 49, f"{cpu:.3f} h")


def main() -> int:
    n0 = constants()
    symbolic()
    simulation()
    bocpd_exact()
    build(n0)
    numbers()
    print(f"\n{PASSES} passed, {len(FAILURES)} failed")
    for f in FAILURES:
        print(f"  {f}")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
