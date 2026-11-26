"""EXP-0030: a trajectory verdict that counts the rating's memory (preregistration.md).

EXP-0015's world, again: synthetic players whose true strength we choose, game by game,
constant (the false alarms) or falling linearly by 100 or 200 points (the detections), rated by
Glicko-2 as Lichess runs it. Their rated games go through `cli lab trajectory` under three rules,
named in the request (`rule`): the champion `peak-bootstrap` (the rule every version up to
`algo-5.0.0` computes) and the two frozen candidates, `memory-halves` and `memory-before`
(`profile::coaching::trajectory::RatingRule`). Each verdict is scored against the truth.

A second world adds what the first leaves out: sessions and breaks (the deviation grows with the
days that pass), a form that lasts for weeks, a day's form and tilt after a loss. A third check
reads real series: the anonymised Lichess cohort of ADR 0114 (EXP-0009's files, by checksum),
whose rating steps and within-month change test the memory the candidates assume.

The player is the unit: one series, one account (and one production seed) per player, one
verdict per rule and length. The world is simulated here, as EXP-0015 does; the rule under test
is only ever the cli's. The cohort check is a check of the candidates' assumption, written from
section 7's formula, not a verdict anyone is told.

    python analysis.py --out DIR    # DIR/results.json and DIR/figures/
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import uuid
from datetime import UTC, datetime, timedelta
from pathlib import Path

import numpy as np
from scipy import stats

import rematch_lab as lab
from rematch_lab import stats as lstats
from rematch_lab.experiments import (
    Budget,
    BudgetExceeded,
    external_input,
    seed,
    write_results,
)
from rematch_lab.models import RatedGame, TrajectoryPlayer, TrajectoryRequest

EXP = "EXP-0030"
# The confirmatory namespace (section 7), drawn after the candidates were frozen. The
# development namespace is EXP-0015-dev (--seed-namespace EXP-0015-dev).
REGISTERED_NS = "EXP-0030"
SEED_NS = REGISTERED_NS

# ---- the rules (section 3) ----------------------------------------------------------------------
CHAMPION = "peak-bootstrap"
CANDIDATES = ["memory-halves", "memory-before"]  # in the order of preference (section 9)
RULES = [CHAMPION, *CANDIDATES]

# ---- the design (section 7) ---------------------------------------------------------------------
CHECKPOINTS = [60, 120, 250]
EXPLORE_GAMES = 30
SETTLED_PLAYERS = 2000
DECLINE_PLAYERS = 300
DECLINES = [100, 200]
ROUGH_PLAYERS = 1000
ROUGH_DECLINE = (200, 120)  # exploratory: one decline cell in the rough world
ARM_PLAYERS = 300
ELO_KS = [10, 32]
NEW_PLAYERS = 300
BURN_IN = 300
ANCHOR = datetime(2026, 1, 5, 9, 0, tzinfo=UTC)
SPACING = timedelta(hours=12)
CHUNK = 200

# The decision (section 9).
NOMINAL = 0.10
BOUND = 0.15
HIDE = 0.20
FAMILY = 12  # two candidates x six co-primary cells
ARENA_LEVEL = 1 - 0.05 / 3  # EXP-0015's arena rule
DETECT_LEVEL = 0.80
DETECT_FLOOR = 0.50
TRANSFER_LEVEL = 0.95
Z80 = 1.2815515655446004
UNSETTLED = 25.0

# The cohort (ADR 0114, EXP-0009's files): the confirmatory slice is the pseudonyms whose first
# hex digit is COHORT_SLICE; the development slice was "0".
COHORT_MONTHS = ["2026-07", "2026-08", "2026-09", "2026-10"]
COHORT_SLICE = "8"
COHORT_MIN_GAMES = 40
Q = math.log(10.0) / 400.0

# Glicko-2 as lila runs it (EXP-0015, section 7).
SCALE = 173.7178
TAU = 0.75
START_RATING = 1500.0
START_RD = 500.0
START_VOL = 0.09
MIN_RD = 45.0
MAX_RD = 500.0
MAX_VOL = 0.1
EPS = 1e-6
PERIODS_PER_DAY = 0.21436
PERIODS_PER_GAME = PERIODS_PER_DAY * 0.5
G1_START_RD = 350.0
G1_GROWTH = 10.0
G1_MIN_RD = 50.0
G1_MAX_RD = 350.0

STRENGTH_MEAN = 1500.0
STRENGTH_SD = 300.0
STRENGTH_RANGE = (700.0, 2300.0)
PAIRING_SD = 75.0
OPP_ERROR_SD = 50.0
OPP_RD = 60.0
DRAW_AT_EVEN = 0.06
ACC_MEAN = {"win": 79.0, "draw": 75.0, "loss": 69.0}
ACC_SD = 8.0
ACC_PER_ELO = 0.02
BLUNDERS = {"win": 0.7, "draw": 0.9, "loss": 1.5}
BLUNDER_PER_ELO = 0.002

# The rough world (assumptions, section 7): sessions, breaks, form and tilt.
SESSION_EXTRA_GAMES = 2.5  # a session is 1 + Poisson(2.5) games
SESSION_GAP_DAYS = 1.2  # mean gap between sessions (exponential), at least an hour
LONG_BREAK_P = 0.04  # a session is followed by a long break with this probability
LONG_BREAK_DAYS = (14.0, 90.0)
GAME_MINUTES = 20.0
FORM_SD = 40.0  # the slow form, an AR(1) over sessions
FORM_PHI = 0.8
DAY_SD = 30.0  # the day's form, fresh each session
TILT = 20.0  # after each loss in a session, for the rest of it
TILT_CAP = 60.0

SEED_NAMES = [
    "settled",
    *[f"decline/{d}/{n}" for d in DECLINES for n in CHECKPOINTS],
    "rough",
    f"rough-decline/{ROUGH_DECLINE[0]}/{ROUGH_DECLINE[1]}",
    *[f"elo/{k}" for k in ELO_KS],
    "glicko1",
    "new-account",
]
BUDGET = {
    "simulated_games": 3_144_000,
    "trajectory_players": 50_100,
    "cohort_lines": 3_955_242,
}


def r6(x: float | None) -> float | None:
    return None if x is None or not math.isfinite(x) else round(float(x), 6)


# ---- the world ----------------------------------------------------------------------------------


def g_of(phi: float) -> float:
    return 1.0 / math.sqrt(1.0 + 3.0 * phi * phi / (math.pi * math.pi))


def glicko2(
    r: float, rd: float, vol: float, r_opp: float, rd_opp: float, s: float, periods: float
) -> tuple[float, float, float]:
    """One game (Glickman 2013, the Illinois step), the deviation grown by `periods`."""
    mu, phi = (r - 1500.0) / SCALE, rd / SCALE
    mu_j, phi_j = (r_opp - 1500.0) / SCALE, rd_opp / SCALE
    g = g_of(phi_j)
    e = 1.0 / (1.0 + math.exp(-g * (mu - mu_j)))
    v = 1.0 / (g * g * e * (1.0 - e))
    delta = v * g * (s - e)
    a = math.log(vol * vol)

    def f(x: float) -> float:
        ex = math.exp(x)
        return ex * (delta * delta - phi * phi - v - ex) / (
            2.0 * (phi * phi + v + ex) ** 2
        ) - (x - a) / (TAU * TAU)

    big_a = a
    if delta * delta > phi * phi + v:
        big_b = math.log(delta * delta - phi * phi - v)
    else:
        k = 1
        while f(a - k * TAU) < 0:
            k += 1
        big_b = a - k * TAU
    fa, fb = f(big_a), f(big_b)
    while abs(big_b - big_a) > EPS:
        c = big_a + (big_a - big_b) * fa / (fb - fa)
        fc = f(c)
        if fc * fb <= 0:
            big_a, fa = big_b, fb
        else:
            fa /= 2.0
        big_b, fb = c, fc
    new_vol = min(math.exp(big_a / 2.0), MAX_VOL)
    phi_star = math.sqrt(phi * phi + periods * new_vol * new_vol)
    new_phi = 1.0 / math.sqrt(1.0 / (phi_star * phi_star) + 1.0 / v)
    new_mu = mu + new_phi * new_phi * g * (s - e)
    new_rd = min(max(new_phi * SCALE, MIN_RD), MAX_RD)
    return new_mu * SCALE + 1500.0, new_rd, new_vol


def glicko1(r: float, rd: float, r_opp: float, rd_opp: float, s: float) -> tuple[float, float]:
    rd = min(math.sqrt(rd * rd + G1_GROWTH * G1_GROWTH), G1_MAX_RD)
    g = 1.0 / math.sqrt(1.0 + 3.0 * Q * Q * rd_opp * rd_opp / (math.pi * math.pi))
    e = 1.0 / (1.0 + 10.0 ** (-g * (r - r_opp) / 400.0))
    d2 = 1.0 / (Q * Q * g * g * e * (1.0 - e))
    inv = 1.0 / (rd * rd) + 1.0 / d2
    return r + Q / inv * g * (s - e), min(max(math.sqrt(1.0 / inv), G1_MIN_RD), G1_MAX_RD)


def strength(rng: np.random.Generator) -> float:
    lo, hi = STRENGTH_RANGE
    return float(min(max(rng.normal(STRENGTH_MEAN, STRENGTH_SD), lo), hi))


class Schedule:
    """When each game is played. The plain world: every 12 hours. The rough world: sessions of
    1 + Poisson games 20 minutes apart, gaps between sessions, now and then a long break; each
    session draws its day's form, the slow form moves from session to session."""

    def __init__(self, rng: np.random.Generator, rough: bool) -> None:
        self.rng, self.rough = rng, rough
        self.t = 0.0  # days since the series began
        self.left = 0
        self.form = rng.normal(0.0, FORM_SD) if rough else 0.0
        self.day = 0.0
        self.tilt = 0.0

    def next_game(self) -> float:
        """Advances to the next game; returns the days elapsed since the previous one."""
        if not self.rough:
            self.t += 0.5
            return 0.5
        if self.left == 0:
            rng = self.rng
            gap = (
                rng.uniform(*LONG_BREAK_DAYS)
                if rng.random() < LONG_BREAK_P
                else max(rng.exponential(SESSION_GAP_DAYS), 1.0 / 24.0)
            )
            self.left = 1 + int(rng.poisson(SESSION_EXTRA_GAMES))
            self.form = FORM_PHI * self.form + rng.normal(0.0, FORM_SD * math.sqrt(1 - FORM_PHI**2))
            self.day = rng.normal(0.0, DAY_SD)
            self.tilt = 0.0
            elapsed = gap
        else:
            elapsed = GAME_MINUTES / (24.0 * 60.0)
        self.left -= 1
        self.t += elapsed
        return elapsed

    def performance(self) -> float:
        return self.form + self.day - self.tilt

    def lost(self) -> None:
        if self.rough:
            self.tilt = min(self.tilt + TILT, TILT_CAP)


def play(
    rng: np.random.Generator,
    truth: np.ndarray,
    observed_from: int,
    system: tuple[str, float],
    rough: bool = False,
) -> list[dict]:
    """Plays every game of `truth` (the true strength before each game) from a fresh account and
    returns the games from `observed_from` on, as EXP-0015's `play`, with the rough world's
    schedule, form and tilt when `rough`."""
    r, vol = START_RATING, START_VOL
    rd = G1_START_RD if system[0] == "glicko1" else START_RD
    base = float(truth[observed_from])
    sched = Schedule(rng, rough)
    games = []
    for t, true_now in enumerate(truth):
        elapsed = sched.next_game()
        perf = float(true_now) + sched.performance()
        r_opp = r + rng.normal(0.0, PAIRING_SD)
        t_opp = r_opp + rng.normal(0.0, OPP_ERROR_SD)
        e_true = 1.0 / (1.0 + 10.0 ** ((t_opp - perf) / 400.0))
        d = 4.0 * DRAW_AT_EVEN * e_true * (1.0 - e_true)
        u = rng.random()
        result = "win" if u < e_true - d / 2 else "draw" if u < e_true + d / 2 else "loss"
        shift = float(true_now) - base
        acc = rng.normal(ACC_MEAN[result] + ACC_PER_ELO * shift, ACC_SD)
        blunders = int(rng.poisson(BLUNDERS[result] * math.exp(-BLUNDER_PER_ELO * shift)))
        pre, pre_rd = r, rd
        s = {"win": 1.0, "draw": 0.5, "loss": 0.0}[result]
        if system[0] == "glicko2":
            periods = PERIODS_PER_DAY * elapsed if rough else PERIODS_PER_GAME
            r, rd, vol = glicko2(r, rd, vol, r_opp, OPP_RD, s, periods)
        elif system[0] == "glicko1":
            r, rd = glicko1(r, rd, r_opp, OPP_RD, s)
        else:
            r = r + system[1] * (s - 1.0 / (1.0 + 10.0 ** ((r_opp - r) / 400.0)))
        if result == "loss":
            sched.lost()
        if t >= observed_from:
            stored = r if system[0] == "glicko1" else pre
            games.append(
                {
                    "rating": int(math.floor(stored + 0.5)),
                    "pre": pre,
                    "result": result,
                    "accuracy": round(min(max(float(acc), 10.0), 99.0), 1),
                    "blunders": blunders,
                    "truth": float(true_now),
                    "rd": pre_rd,
                    "at": sched.t,
                }
            )
    return games


def account_id(pid: str) -> str:
    return str(uuid.UUID(bytes=hashlib.sha256(f"{SEED_NS}/account/{pid}".encode()).digest()[:16]))


def request_player(pid: str, games: list[dict], *, chesscom: bool = False) -> TrajectoryPlayer:
    acct = account_id(pid)
    return TrajectoryPlayer(
        id=pid,
        seed=f"trajectory:{acct}:rapid",
        chesscom=chesscom,
        games=[
            RatedGame(
                id=f"{acct}-{t:04d}",
                played_at=ANCHOR + timedelta(days=g["at"]) if "at" in g else ANCHOR + SPACING * t,
                rating=g["rating"],
                result=g["result"],
                analysed=True,
                accuracy=g["accuracy"],
                blunders=g["blunders"],
            )
            for t, g in enumerate(games)
        ],
    )


# ---- the runs -----------------------------------------------------------------------------------


class Runner:
    def __init__(self, budget: Budget) -> None:
        self.budget = budget
        self.algorithm_id: str | None = None

    def trajectories(self, players: list[TrajectoryPlayer]) -> dict[str, list[dict]]:
        """Every player under every rule, in order."""
        out: dict[str, list[dict]] = {}
        for rule in RULES:
            rows: list[dict] = []
            for i in range(0, len(players), CHUNK):
                batch = players[i : i + CHUNK]
                self.budget.spend("trajectory_players", len(batch))
                resp = lab.trajectory(TrajectoryRequest(players=batch, rule=rule))
                if self.algorithm_id is None:
                    self.algorithm_id = resp.algorithm_id
                elif resp.algorithm_id != self.algorithm_id:
                    raise SystemExit("the algorithm_id changed during the run")
                for p, q in zip(batch, resp.players, strict=True):
                    if p.id != q.id:
                        raise SystemExit("trajectory responses out of order")
                    t = q.trajectory
                    if not t.sufficient or t.then_vs_now is None:
                        raise SystemExit(f"{p.id}: no trajectory at {len(p.games)} games")
                    tv = t.then_vs_now
                    lo, hi = tv.rating.diff_low, tv.rating.diff_high
                    rows.append(
                        {
                            "kind": t.kind,
                            "basis": tv.then.basis,
                            "then_games": tv.then.games,
                            "accuracy": tv.accuracy.verdict,
                            "blunders": tv.blunders_per_game.verdict,
                            "change": t.change,
                            "diff": tv.rating.diff,
                            "half_width": (hi - lo) / 2 if lo is not None and hi is not None else None,
                        }
                    )
            out[rule] = rows
        return out

    def simulate(
        self,
        name: str,
        players: int,
        games: int,
        burn_in: int,
        decline: float,
        system: tuple[str, float],
        rough: bool = False,
    ) -> list[list[dict]]:
        rng = np.random.default_rng(seed(SEED_NS, name))
        series = []
        for _ in range(players):
            t0 = strength(rng)
            truth = np.full(burn_in + games, t0)
            if decline:
                truth[burn_in:] = t0 - decline * np.arange(games) / (games - 1)
            self.budget.spend("simulated_games", burn_in + games)
            series.append(play(rng, truth, burn_in, system, rough))
        return series


# ---- the analysis (section 8) -------------------------------------------------------------------


def wilson(k: int, n: int, level: float) -> list:
    z = stats.norm.ppf(1 - (1 - level) / 2)
    p = k / n
    den = 1 + z * z / n
    c = (p + z * z / (2 * n)) / den
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / den
    return [r6(max(0.0, c - h)), r6(min(1.0, c + h))]


def share(flags: list[bool], family: int = 1) -> dict:
    k, n = sum(flags), len(flags)
    return {
        "n": n,
        "k": k,
        "share": r6(k / n),
        "ci95": wilson(k, n, 0.95),
        "ci80": wilson(k, n, 0.80),
        "ci_bonferroni": wilson(k, n, 1 - 0.05 / family) if family > 1 else None,
    }


def classify(lo: float, hi: float) -> str:
    if lo > HIDE:
        return "above the hide line"
    if lo > BOUND:
        return "above the bound"
    if hi <= BOUND:
        return "within the bound"
    return "undecided"


def classified(flags: list[bool], family: int = FAMILY) -> dict:
    cell = share(flags, family)
    cell["read"] = classify(*cell["ci_bonferroni"])
    return cell


def bits(flags: list[bool]) -> str:
    return "".join("1" if f else "0" for f in flags)


def settle(series: list[list[dict]]) -> dict:
    first = np.array([s[0]["pre"] - s[0]["truth"] for s in series])
    mean = float(first.mean())
    return {
        "mean_error_at_start": r6(mean),
        "abs_error_at_start": r6(float(np.abs(first).mean())),
        "unsettled": abs(mean) > UNSETTLED,
    }


def calibration(ts: list[dict]) -> dict:
    diffs = np.array([t["diff"] for t in ts])
    half = np.array([t["half_width"] for t in ts if t["half_width"] is not None])
    sd = float(diffs.std())
    return {
        "diff_mean": r6(float(diffs.mean())),
        "diff_sd": r6(sd),
        "half_width_mean": r6(float(half.mean())) if len(half) else None,
        "honest_half_width": r6(Z80 * sd),
    }


def steps_of(ratings: list[float]) -> np.ndarray:
    return np.diff(np.asarray(ratings, dtype=float))


def step_lag1(blocks: list[np.ndarray]) -> float | None:
    """The pooled lag-1 correlation of consecutive rating steps, each block centred on its own
    mean: about 0 when results carry no memory (an Elo rating's own mean reversion makes it
    slightly negative), above 0 with streaks."""
    xs, ys = [], []
    for b in blocks:
        if len(b) < 3:
            continue
        c = b - b.mean()
        xs.extend(c[:-1])
        ys.extend(c[1:])
    if len(xs) < 3:
        return None
    return r6(float(np.corrcoef(xs, ys)[0, 1]))


def world_steps(series: list[list[dict]]) -> dict:
    blocks = [steps_of([g["rating"] for g in s]) for s in series]
    rms = [float(np.sqrt(np.mean(b * b))) for b in blocks if len(b)]
    return {"step_rms_median": r6(float(np.median(rms))), "step_lag1": step_lag1(blocks)}


# ---- the memory model's check on real series (section 7) ----------------------------------------


def within(m: int, phi: float) -> float:
    """sum over i, j in a block of m consecutive games of phi^|i-j|."""
    if phi <= 0.0:
        return float(m)
    return m * (1 + phi) / (1 - phi) - 2 * phi * (1 - phi**m) / (1 - phi) ** 2


def model_var(a0: int, a1: int, b0: int, b1: int, rms: float) -> float | None:
    """Section 7's variance of mean(b) - mean(a) for a stationary rating whose RMS step is
    `rms`: sigma^2 = rms / (q (1 - a/2)), phi = 1 - a, a = q rms / 2."""
    if rms <= 0.0:
        return None
    a = Q * rms / 2.0
    phi = min(max(1.0 - a, 0.0), 1.0 - 1e-12)
    s2 = rms / (Q * (1.0 - a / 2.0))
    ma, mb = a1 - a0, b1 - b0
    sa = (1 - phi**ma) / (1 - phi)
    sb = (1 - phi**mb) / (1 - phi)
    cross = phi ** (b0 - a1 + 1) * sa * sb
    return s2 * (within(ma, phi) / ma**2 + within(mb, phi) / mb**2 - 2 * cross / (ma * mb))


def cohort_check(budget: Budget) -> dict:
    """Each pseudonym of the slice once, in its first month with at least COHORT_MIN_GAMES rapid
    games: the mean pre-game rating of its last five rapid games against its first five, in
    units of the model's standard deviation, from the RMS of those ten games' own steps."""
    seen: set[str] = set()
    zs: list[float] = []
    blocks: list[np.ndarray] = []
    rms_all: list[float] = []
    games_all: list[int] = []
    for month in COHORT_MONTHS:
        with open(external_input(f"exp09-cohort/{month}/players.jsonl"), encoding="utf-8") as f:
            for line in f:
                budget.spend("cohort_lines", 1)
                p = json.loads(line)
                h = p["h"]
                if h[0] != COHORT_SLICE or h in seen or p["rapid_games"] < COHORT_MIN_GAMES:
                    continue
                first, last = p["first"], p["last"]
                if len(first) != 5 or len(last) != 5:
                    continue
                seen.add(h)
                n = p["rapid_games"]
                steps = np.array([g[3] - g[2] for g in first + last], dtype=float)
                rms = float(np.sqrt(np.mean(steps * steps)))
                v = model_var(0, 5, n - 5, n, rms)
                if v is None or v <= 0:
                    continue
                diff = np.mean([g[2] for g in last]) - np.mean([g[2] for g in first])
                zs.append(float(diff / math.sqrt(v)))
                blocks.append(steps[:5])
                blocks.append(steps[5:])
                rms_all.append(rms)
                games_all.append(n)
    z = np.array(zs)
    falling = [bool(x < -Z80) for x in z]
    rising = [bool(x > Z80) for x in z]
    fall = share(falling)
    fall["ci_transfer"] = wilson(sum(falling), len(falling), TRANSFER_LEVEL)
    return {
        "players": len(z),
        "falling": fall,
        "rising": share(rising),
        "mean_z": r6(float(z.mean())),
        "mean_z2": r6(float(np.mean(z * z))),
        "median_z2": r6(float(np.median(z * z))),
        "step_rms_median": r6(float(np.median(rms_all))),
        "step_lag1": step_lag1(blocks),
        "games_median": int(np.median(games_all)),
        "guard": "passes" if fall["ci_transfer"][0] <= BOUND else "fails",
    }


# ---- everything ---------------------------------------------------------------------------------


def run_all(run: Runner, budget: Budget) -> dict:
    glicko = ("glicko2", 0.0)
    reads = [EXPLORE_GAMES, *CHECKPOINTS]

    settled = run.simulate("settled", SETTLED_PLAYERS, max(CHECKPOINTS), BURN_IN, 0.0, glicko)
    flat = {
        g: run.trajectories(
            [request_player(f"settled-{i:04d}", s[:g]) for i, s in enumerate(settled)]
        )
        for g in reads
    }

    detect: dict[str, dict[str, list[dict]]] = {}
    detect_series = {}
    for d in DECLINES:
        for g in CHECKPOINTS:
            series = run.simulate(f"decline/{d}/{g}", DECLINE_PLAYERS, g, BURN_IN, float(d), glicko)
            detect[f"{d}/{g}"] = run.trajectories(
                [request_player(f"decline-{d}-{g}-{i:04d}", s) for i, s in enumerate(series)]
            )
            detect_series[f"{d}/{g}"] = series

    rough = run.simulate("rough", ROUGH_PLAYERS, max(CHECKPOINTS), BURN_IN, 0.0, glicko, True)
    rough_t = {
        g: run.trajectories([request_player(f"rough-{i:04d}", s[:g]) for i, s in enumerate(rough)])
        for g in CHECKPOINTS
    }
    rd_dec, rd_len = ROUGH_DECLINE
    rough_dec = run.simulate(
        f"rough-decline/{rd_dec}/{rd_len}", DECLINE_PLAYERS, rd_len, BURN_IN, float(rd_dec), glicko, True
    )
    rough_dec_t = run.trajectories(
        [request_player(f"rough-decline-{i:04d}", s) for i, s in enumerate(rough_dec)]
    )

    cohort = cohort_check(budget)

    # The co-primaries, the arena rule against the champion, and the guards, per candidate.
    candidates = {}
    for c in CANDIDATES:
        cells = []
        for g in CHECKPOINTS:
            for side in ("falling", "rising"):
                cells.append({"games": g, "side": side, **classified([t["kind"] == side for t in flat[g][c]])})
        arena = {}
        for g in CHECKPOINTS:
            champ = [t["kind"] == "falling" for t in flat[g][CHAMPION]]
            chal = [t["kind"] == "falling" for t in flat[g][c]]
            n11 = sum(a and b for a, b in zip(champ, chal, strict=True))
            n10 = sum((not a) and b for a, b in zip(champ, chal, strict=True))
            n01 = sum(a and (not b) for a, b in zip(champ, chal, strict=True))
            n00 = len(champ) - n11 - n10 - n01
            eff, lo, hi = lstats.newcombe_paired(n11, n10, n01, n00, level=ARENA_LEVEL)
            arena[str(g)] = {
                "effect": r6(eff),
                "interval": [r6(lo), r6(hi)],
                "verdict": lstats.verdict(lo, hi, "lower"),
            }
        detection = []
        for g in CHECKPOINTS:
            for d in DECLINES:
                flags = [t["kind"] == "falling" for t in detect[f"{d}/{g}"][c]]
                cell = {"decline": d, "games": g, **share(flags)}
                cell["false_alarm_same_length"] = r6(
                    float(np.mean([t["kind"] == "falling" for t in flat[g][c]]))
                )
                if d == 200:
                    cell["guard"] = "passes" if cell["ci80"][0] >= DETECT_FLOOR else "fails"
                detection.append(cell)
        rough_cells = []
        for g in CHECKPOINTS:
            for side in ("falling", "rising"):
                rough_cells.append({"games": g, "side": side, **classified([t["kind"] == side for t in rough_t[g][c]])})
        checks = {
            "primary_within": all(x["read"] == "within the bound" for x in cells),
            "beats_champion": all(a["verdict"] == lstats.BETTER for a in arena.values()),
            "detection_guard": all(x.get("guard", "passes") == "passes" for x in detection),
            "rough_guard": all(x["read"] in ("within the bound", "undecided") for x in rough_cells),
            "transfer_guard": cohort["guard"] == "passes",
        }
        only_undecided = (
            not checks["primary_within"]
            and all(x["read"] in ("within the bound", "undecided") for x in cells)
            and all(v for k, v in checks.items() if k != "primary_within")
        )
        candidates[c] = {
            "primary": cells,
            "arena": arena,
            "detection": detection,
            "rough": rough_cells,
            "checks": checks,
            "clears": all(checks.values()),
            "only_undecided": only_undecided,
        }

    cleared = [c for c in CANDIDATES if candidates[c]["clears"]]
    if cleared:
        decision = f"ship {cleared[0]}"
    elif all(candidates[c]["only_undecided"] for c in CANDIDATES):
        decision = "need more data"
    else:
        decision = "keep the verdict hidden"

    # The champion on the same players, for the record (EXP-0015's numbers on fresh seeds).
    champion = {
        "falling": [{"games": g, **share([t["kind"] == "falling" for t in flat[g][CHAMPION]])} for g in CHECKPOINTS],
        "rising": [{"games": g, **share([t["kind"] == "rising" for t in flat[g][CHAMPION]])} for g in CHECKPOINTS],
        "rough_falling": [
            {"games": g, **share([t["kind"] == "falling" for t in rough_t[g][CHAMPION]])} for g in CHECKPOINTS
        ],
    }

    # Exploratory.
    exploratory: dict = {"games_30": {}, "then_now_rows": {}, "rough_decline": {}, "mean_change": {}}
    for rule in RULES:
        exploratory["games_30"][rule] = {
            side: share([t["kind"] == side for t in flat[EXPLORE_GAMES][rule]]) for side in ("falling", "rising")
        }
        exploratory["then_now_rows"][rule] = [
            {"measure": m, "games": g, **share([t[m] == "worse" for t in flat[g][rule]])}
            for m in ("accuracy", "blunders")
            for g in CHECKPOINTS
        ]
        exploratory["rough_decline"][rule] = share([t["kind"] == "falling" for t in rough_dec_t[rule]])
        exploratory["mean_change"][rule] = [
            {
                "games": g,
                "all": r6(float(np.mean([t["change"] for t in flat[g][rule]]))),
                "when_falling": r6(
                    float(np.mean([t["change"] for t in flat[g][rule] if t["kind"] == "falling"]))
                    if any(t["kind"] == "falling" for t in flat[g][rule])
                    else None
                ),
            }
            for g in CHECKPOINTS
        ]

    new = run.simulate("new-account", NEW_PLAYERS, max(CHECKPOINTS), 0, 0.0, glicko)
    arms = [(f"elo/{k}", ("elo", float(k)), False, BURN_IN) for k in ELO_KS]
    arms.append(("glicko1", ("glicko1", 0.0), True, BURN_IN))
    other = []
    for name, system, chesscom, burn in arms:
        series = run.simulate(name, ARM_PLAYERS, max(CHECKPOINTS), burn, 0.0, system)
        row: dict = {"arm": name, "settle": settle(series), "steps": world_steps(series), "cells": []}
        for g in CHECKPOINTS:
            ts = run.trajectories(
                [request_player(f"{name.replace('/', '-')}-{i:04d}", s[:g], chesscom=chesscom) for i, s in enumerate(series)]
            )
            row["cells"].append(
                {
                    "games": g,
                    **{
                        rule: {side: share([t["kind"] == side for t in ts[rule]]) for side in ("falling", "rising")}
                        for rule in RULES
                    },
                }
            )
        other.append(row)
    new_rows = []
    for g in CHECKPOINTS:
        ts = run.trajectories([request_player(f"new-{i:04d}", s[:g]) for i, s in enumerate(new)])
        new_rows.append(
            {
                "games": g,
                **{rule: {side: share([t["kind"] == side for t in ts[rule]]) for side in ("falling", "rising")} for rule in RULES},
            }
        )
    exploratory["other_ratings"] = other
    exploratory["new_account"] = new_rows

    err = np.array([[g["pre"] - g["truth"] for g in s] for s in settled])
    diagnostics = {
        "settled": settle(settled),
        "decline": {k: settle(v) for k, v in detect_series.items()},
        "rough": settle(rough),
        "settled_error_sd": r6(float(err.std())),
        "settled_rd_mean": r6(float(np.mean([g["rd"] for s in settled for g in s]))),
        "rough_rd_mean": r6(float(np.mean([g["rd"] for s in rough for g in s]))),
        "rough_days_median": r6(float(np.median([s[-1]["at"] - s[0]["at"] for s in rough]))),
        "steps": {"settled": world_steps(settled), "rough": world_steps(rough)},
        "interval": {
            rule: {str(g): calibration(flat[g][rule]) for g in CHECKPOINTS} for rule in RULES
        },
    }

    return {
        "decision": decision,
        "candidates": candidates,
        "champion": champion,
        "cohort": cohort,
        "exploratory": exploratory,
        "diagnostics": diagnostics,
        "flags": {
            rule: {
                "falling": {str(g): bits([t["kind"] == "falling" for t in flat[g][rule]]) for g in CHECKPOINTS},
                "rising": {str(g): bits([t["kind"] == "rising" for t in flat[g][rule]]) for g in CHECKPOINTS},
                "detection": {
                    k: bits([t["kind"] == "falling" for t in v[rule]]) for k, v in detect.items()
                },
                "rough_falling": {str(g): bits([t["kind"] == "falling" for t in rough_t[g][rule]]) for g in CHECKPOINTS},
                "rough_rising": {str(g): bits([t["kind"] == "rising" for t in rough_t[g][rule]]) for g in CHECKPOINTS},
            }
            for rule in RULES
        },
    }


def design() -> dict:
    return {
        "rules": RULES,
        "checkpoints": CHECKPOINTS,
        "settled_players": SETTLED_PLAYERS,
        "decline_players": DECLINE_PLAYERS,
        "declines": DECLINES,
        "rough_players": ROUGH_PLAYERS,
        "arm_players": ARM_PLAYERS,
        "new_players": NEW_PLAYERS,
        "burn_in": BURN_IN,
        "rough": {
            "session_extra_games": SESSION_EXTRA_GAMES,
            "session_gap_days": SESSION_GAP_DAYS,
            "long_break": [LONG_BREAK_P, *LONG_BREAK_DAYS],
            "game_minutes": GAME_MINUTES,
            "form": [FORM_SD, FORM_PHI],
            "day_sd": DAY_SD,
            "tilt": [TILT, TILT_CAP],
        },
        "cohort": {"months": COHORT_MONTHS, "slice": COHORT_SLICE, "min_games": COHORT_MIN_GAMES},
        "family": FAMILY,
        "seed_names": SEED_NAMES,
        "seed_namespace": SEED_NS,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--algorithm", default=None)
    ap.add_argument("--seed-namespace", default=REGISTERED_NS)
    args = ap.parse_args()
    out_dir = Path(args.out)
    global SEED_NS
    SEED_NS = args.seed_namespace
    (out_dir / "figures").mkdir(parents=True, exist_ok=True)
    if args.algorithm is not None and lab.algorithm()["id"] != args.algorithm:
        raise SystemExit(f"the cli is not the requested {args.algorithm}")
    budget = Budget(BUDGET)
    run = Runner(budget)
    try:
        body = run_all(run, budget)
    except BudgetExceeded as e:
        write_results(
            out_dir,
            {
                "experiment": EXP,
                "algorithm_id": run.algorithm_id,
                "design": design(),
                "failed": str(e),
                "compute": budget.record(),
            },
        )
        return 1
    results = {
        "experiment": EXP,
        "algorithm_id": run.algorithm_id,
        "design": design(),
        **body,
        "compute": budget.record(),
    }
    write_results(out_dir, results)
    figures(out_dir / "figures", results)
    return 0


def figures(fig_dir: Path, res: dict) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, 2, figsize=(9, 4), sharey=True)
    for ax, side in zip(axes, ("falling", "rising"), strict=True):
        champ = res["champion"][side]
        ax.plot([c["games"] for c in champ], [c["share"] for c in champ], marker="o", label=CHAMPION)
        for c in CANDIDATES:
            cells = [x for x in res["candidates"][c]["primary"] if x["side"] == side]
            xs = [x["games"] for x in cells]
            ys = [x["share"] for x in cells]
            lo = [x["share"] - x["ci_bonferroni"][0] for x in cells]
            hi = [x["ci_bonferroni"][1] - x["share"] for x in cells]
            ax.errorbar(xs, ys, yerr=[lo, hi], marker="s", capsize=3, label=c)
        ax.axhline(BOUND, color="grey", lw=0.8, ls="--", label="bound 0.15")
        ax.set_xscale("log")
        ax.set_xticks(CHECKPOINTS, [str(x) for x in CHECKPOINTS])
        ax.set_ylim(0, 1)
        ax.set_xlabel("rated games")
        ax.set_title(f'constant strength told "{side}"')
    axes[0].set_ylabel("share of players")
    axes[0].legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(fig_dir / "false-alarms.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    raise SystemExit(main())
