"""EXP-0015: is "clearly worse than your peak" a false alarm? (preregistration.md, issue #110)

Synthetic players whose true strength we choose, game by game: constant (the false alarms) or
falling linearly by 100 or 200 points (the detections). Each plays rated games against opponents
paired near their rating, and the rating follows Glicko-2 as Lichess runs it (two games a day,
the deviation grown by lila's 0.21436 rating periods a day). The rated games, with the stored
rating, the result, the accuracy and the blunders, go through the production trajectory
(`cli lab trajectory`, `rematch_lab.trajectory`: the loader's `trajectory_input`, `compute` and
`contract`, ADR 0089), and each verdict is scored against the truth. The estimand is the true
strength, the same at "then" and "now", so a `falling` or `rising` verdict for a
constant-strength player is a false alarm about strength. The player is the unit: one series,
one account (and one production seed) per player, one verdict per checkpoint.

The world (the rating systems, the opponents, the results, the accuracy and the blunders) is
simulated here, as EXP-0002 simulates its event counts; the rule under test is only ever the
cli's.

    python analysis.py --out DIR    # DIR/results.json and DIR/figures/
"""

from __future__ import annotations

import argparse
import hashlib
import math
import uuid
from datetime import UTC, datetime, timedelta
from pathlib import Path

import numpy as np
from scipy import stats

import rematch_lab as lab
from rematch_lab.experiments import Budget, BudgetExceeded, seed, write_results
from rematch_lab.models import RatedGame, TrajectoryPlayer, TrajectoryRequest

EXP = "EXP-0015"
# The seed namespace (--seed-namespace); the registered run uses the experiment id.
SEED_NS = EXP

# ---- the design (section 7) --------------------------------------------------------------------
CHECKPOINTS = [60, 120, 250]  # the primary's three sample sizes
EXPLORE_GAMES = 30  # exploratory: W = 15
SETTLED_PLAYERS = 700
DECLINE_PLAYERS = 200
DECLINES = [100, 200]
NEW_PLAYERS = 300
ELO_KS = [10, 32]
ELO_PLAYERS = 200
GLICKO1_PLAYERS = 200
HALVES = 30  # the "no peak" read: the first and the last 30 games of a series
BURN_IN = 300
ANCHOR = datetime(2026, 1, 5, 9, 0, tzinfo=UTC)
SPACING = timedelta(hours=12)
CHUNK = 200

# The decision (section 9).
NOMINAL = 0.10  # one side of an 80% two-sided interval
BOUND = 0.15  # the acceptable false-alarm rate: the nominal plus the tolerance
HIDE = 0.20  # false "falling" headlines outnumber true ones (section 5)
FAMILY = 3  # Bonferroni over the three checkpoints
DETECT_FAMILY = 6  # the detection cells
SECONDARY_FAMILY = 6  # accuracy and blunders at three checkpoints
UNSETTLED = 25.0  # a signed mean error at the first observed game past this: not settled

# Glicko-2 as lila runs it: `lila.rating.Glicko` (τ 0.75, a fresh account at 1500 with deviation
# 500 and volatility 0.09, the deviation in [45, 500], the volatility at most 0.1) and
# `RatingPeriodsPerDay(0.21436)`, the deviation grown by the elapsed periods (half a day a game).
SCALE = 173.7178
TAU = 0.75
START_RATING = 1500.0
START_RD = 500.0
START_VOL = 0.09
MIN_RD = 45.0
MAX_RD = 500.0
MAX_VOL = 0.1
EPS = 1e-6
PERIODS_PER_GAME = 0.21436 * 0.5
# A chess.com-like Glicko-1 (assumed: chess.com does not publish its constants): a fresh account
# at 1500 with deviation 350, the deviation grown by 10 a game and kept in [50, 350].
G1_START_RD = 350.0
G1_GROWTH = 10.0
G1_MIN_RD = 50.0
G1_MAX_RD = 350.0
Q = math.log(10.0) / 400.0

# The population and the opponents.
STRENGTH_MEAN = 1500.0
STRENGTH_SD = 300.0
STRENGTH_RANGE = (700.0, 2300.0)
PAIRING_SD = 75.0  # the opponent's rating around the player's
OPP_ERROR_SD = 50.0  # the opponent's true strength around their rating
OPP_RD = 60.0
DRAW_AT_EVEN = 0.06  # P(draw) = 4 * DRAW_AT_EVEN * E (1 - E)

# Accuracy and blunders per game, by result, shifted by the change in true strength. Assumptions,
# not measured: they shape the secondaries only.
ACC_MEAN = {"win": 79.0, "draw": 75.0, "loss": 69.0}
ACC_SD = 8.0
ACC_PER_ELO = 0.02
BLUNDERS = {"win": 0.7, "draw": 0.9, "loss": 1.5}
BLUNDER_PER_ELO = 0.002

SEED_NAMES = [
    "settled",
    *[f"decline/{d}/{n}" for d in DECLINES for n in CHECKPOINTS],
    "new-account",
    *[f"elo/{k}" for k in ELO_KS],
    "glicko1",
]
BUDGET = {"simulated_games": 1_322_000, "trajectory_players": 8_100}


def r6(x: float | None) -> float | None:
    return None if x is None or not math.isfinite(x) else round(float(x), 6)


# ---- the world --------------------------------------------------------------------------------


def g_of(phi: float) -> float:
    return 1.0 / math.sqrt(1.0 + 3.0 * phi * phi / (math.pi * math.pi))


def glicko2(
    r: float, rd: float, vol: float, r_opp: float, rd_opp: float, s: float
) -> tuple[float, float, float]:
    """One game (Glickman 2013, the Illinois step for the volatility), the deviation grown by
    the elapsed rating periods."""
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
    phi_star = math.sqrt(phi * phi + PERIODS_PER_GAME * new_vol * new_vol)
    new_phi = 1.0 / math.sqrt(1.0 / (phi_star * phi_star) + 1.0 / v)
    new_mu = mu + new_phi * new_phi * g * (s - e)
    new_rd = min(max(new_phi * SCALE, MIN_RD), MAX_RD)
    return new_mu * SCALE + 1500.0, new_rd, new_vol


def glicko1(r: float, rd: float, r_opp: float, rd_opp: float, s: float) -> tuple[float, float]:
    """One game of Glicko-1 (Glickman 1999), the deviation grown first."""
    rd = min(math.sqrt(rd * rd + G1_GROWTH * G1_GROWTH), G1_MAX_RD)
    g = 1.0 / math.sqrt(1.0 + 3.0 * Q * Q * rd_opp * rd_opp / (math.pi * math.pi))
    e = 1.0 / (1.0 + 10.0 ** (-g * (r - r_opp) / 400.0))
    d2 = 1.0 / (Q * Q * g * g * e * (1.0 - e))
    inv = 1.0 / (rd * rd) + 1.0 / d2
    return r + Q / inv * g * (s - e), min(max(math.sqrt(1.0 / inv), G1_MIN_RD), G1_MAX_RD)


def strength(rng: np.random.Generator) -> float:
    lo, hi = STRENGTH_RANGE
    return float(min(max(rng.normal(STRENGTH_MEAN, STRENGTH_SD), lo), hi))


def play(
    rng: np.random.Generator,
    truth: np.ndarray,
    observed_from: int,
    system: tuple[str, float],
) -> list[dict]:
    """Plays every game of `truth` (the true strength before each game) from a fresh account;
    returns the games from `observed_from` on, with the stored rating: the pre-game rating, or for
    the chess.com-like Glicko-1 the rating after the game, as chess.com stores it."""
    r, vol = START_RATING, START_VOL
    rd = G1_START_RD if system[0] == "glicko1" else START_RD
    base = float(truth[observed_from])
    games = []
    for t, true_now in enumerate(truth):
        r_opp = r + rng.normal(0.0, PAIRING_SD)
        t_opp = r_opp + rng.normal(0.0, OPP_ERROR_SD)
        e_true = 1.0 / (1.0 + 10.0 ** ((t_opp - true_now) / 400.0))
        d = 4.0 * DRAW_AT_EVEN * e_true * (1.0 - e_true)
        u = rng.random()
        result = "win" if u < e_true - d / 2 else "draw" if u < e_true + d / 2 else "loss"
        shift = float(true_now) - base
        acc = rng.normal(ACC_MEAN[result] + ACC_PER_ELO * shift, ACC_SD)
        blunders = int(rng.poisson(BLUNDERS[result] * math.exp(-BLUNDER_PER_ELO * shift)))
        pre, pre_rd = r, rd
        s = {"win": 1.0, "draw": 0.5, "loss": 0.0}[result]
        if system[0] == "glicko2":
            r, rd, vol = glicko2(r, rd, vol, r_opp, OPP_RD, s)
        elif system[0] == "glicko1":
            r, rd = glicko1(r, rd, r_opp, OPP_RD, s)
        else:
            r = r + system[1] * (s - 1.0 / (1.0 + 10.0 ** ((r_opp - r) / 400.0)))
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
                    "step": abs(r - pre),
                }
            )
    return games


def account_id(pid: str) -> str:
    return str(uuid.UUID(bytes=hashlib.sha256(f"{SEED_NS}/account/{pid}".encode()).digest()[:16]))


def request_player(
    pid: str, games: list[dict], *, chesscom: bool = False, index: list[int] | None = None
) -> TrajectoryPlayer:
    """The player's rated games; `index` is each game's position in the player's whole series
    (its id and its time), so a subset keeps the ids and dates the full series gives it."""
    acct = account_id(pid)
    index = index if index is not None else list(range(len(games)))
    return TrajectoryPlayer(
        id=pid,
        seed=f"trajectory:{acct}:rapid",
        chesscom=chesscom,
        games=[
            RatedGame(
                id=f"{acct}-{index[t]:04d}",
                played_at=ANCHOR + SPACING * index[t],
                rating=g["rating"],
                result=g["result"],
                analysed=True,
                accuracy=g["accuracy"],
                blunders=g["blunders"],
            )
            for t, g in enumerate(games)
        ],
    )


# ---- the runs ---------------------------------------------------------------------------------


class Runner:
    def __init__(self, budget: Budget) -> None:
        self.budget = budget
        self.algorithm_id: str | None = None

    def trajectories(self, players: list[TrajectoryPlayer]) -> list[dict]:
        out: list[dict] = []
        for i in range(0, len(players), CHUNK):
            batch = players[i : i + CHUNK]
            self.budget.spend("trajectory_players", len(batch))
            resp = lab.trajectory(TrajectoryRequest(players=batch))
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
                out.append(
                    {
                        "kind": t.kind,
                        "basis": tv.then.basis,
                        "rating": tv.rating.verdict,
                        "accuracy": tv.accuracy.verdict,
                        "blunders": tv.blunders_per_game.verdict,
                        "score": tv.score.verdict,
                        "change": t.change,
                        "diff": tv.rating.diff,
                        "half_width": (hi - lo) / 2 if lo is not None and hi is not None else None,
                    }
                )
        return out

    def simulate(
        self,
        name: str,
        players: int,
        games: int,
        burn_in: int,
        decline: float,
        system: tuple[str, float],
    ) -> list[list[dict]]:
        rng = np.random.default_rng(seed(SEED_NS, name))
        series = []
        for _ in range(players):
            t0 = strength(rng)
            truth = np.full(burn_in + games, t0)
            if decline:
                truth[burn_in:] = t0 - decline * np.arange(games) / (games - 1)
            self.budget.spend("simulated_games", burn_in + games)
            series.append(play(rng, truth, burn_in, system))
        return series


# ---- the analysis (section 8) ------------------------------------------------------------------


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


def classified(flags: list[bool], family: int) -> dict:
    cell = share(flags, family)
    cell["read"] = classify(*cell["ci_bonferroni"])
    return cell


def decide(cells: list[dict]) -> str:
    reads = [c["read"] for c in cells]
    if "above the hide line" in reads:
        return "hide the verdict until fixed"
    if "above the bound" in reads:
        return "change the threshold"
    if all(r == "within the bound" for r in reads):
        return "keep the threshold"
    return "need more data"


def bits(flags: list[bool]) -> str:
    return "".join("1" if f else "0" for f in flags)


def settle(series: list[list[dict]]) -> dict:
    """The signed and absolute error of the rating at the first observed game, and the flag."""
    first = np.array([s[0]["pre"] - s[0]["truth"] for s in series])
    mean = float(first.mean())
    return {
        "mean_error_at_start": r6(mean),
        "abs_error_at_start": r6(float(np.abs(first).mean())),
        "unsettled": abs(mean) > UNSETTLED,
    }


def calibration(ts: list[dict]) -> dict:
    """The rule's 80% interval of the rating difference against the spread of the difference
    across players of constant strength (an honest interval's half-width is about 1.28 SD)."""
    diffs = np.array([t["diff"] for t in ts])
    half = np.array([t["half_width"] for t in ts if t["half_width"] is not None])
    sd = float(diffs.std())
    return {
        "diff_mean": r6(float(diffs.mean())),
        "diff_sd": r6(sd),
        "half_width_mean": r6(float(half.mean())),
        "honest_half_width": r6(1.2815516 * sd),
    }


def run_all(run: Runner) -> dict:
    glicko = ("glicko2", 0.0)
    reads = [EXPLORE_GAMES, *CHECKPOINTS]

    # Settled players of constant strength: one series of 250 observed games after the burn-in,
    # read at 30, 60, 120 and 250 games (a player with g rated games).
    settled = run.simulate("settled", SETTLED_PLAYERS, max(CHECKPOINTS), BURN_IN, 0.0, glicko)
    flat = {
        g: run.trajectories(
            [request_player(f"settled-{i:04d}", s[:g]) for i, s in enumerate(settled)]
        )
        for g in reads
    }

    primary_cells = [
        {"games": g, **classified([t["kind"] == "falling" for t in flat[g]], FAMILY)}
        for g in CHECKPOINTS
    ]
    primary = {
        "metric": "share of constant-strength players told their rating is falling",
        "bound": BOUND,
        "hide": HIDE,
        "nominal": NOMINAL,
        "cells": primary_cells,
        "decision": decide(primary_cells),
    }

    # Confirmatory secondaries: false "rising" (the same verdict the other way), and accuracy
    # and blunders per game "worse", read like the primary.
    rising = [
        {"games": g, **classified([t["kind"] == "rising" for t in flat[g]], FAMILY)}
        for g in CHECKPOINTS
    ]
    secondary = [
        {
            "measure": m,
            "games": g,
            **classified([t[m] == "worse" for t in flat[g]], SECONDARY_FAMILY),
        }
        for m in ("accuracy", "blunders")
        for g in CHECKPOINTS
    ]

    by_basis = []
    for g in reads:
        row: dict = {"games": g}
        for basis in ("peak", "start"):
            ts = [t for t in flat[g] if t["basis"] == basis]
            row[basis] = {
                "players": len(ts),
                "falling": sum(t["kind"] == "falling" for t in ts),
                "rising": sum(t["kind"] == "rising" for t in ts),
            }
        by_basis.append(row)

    # Exploratory: the "no peak" read, the production rule on the first and last 30 games of the
    # same series (at 60 games the windows are exactly those halves, so the peak cannot move them).
    halves = []
    for g in CHECKPOINTS[1:]:
        idx = [*range(HALVES), *range(g - HALVES, g)]
        ts = run.trajectories(
            [
                request_player(f"settled-{i:04d}", [s[j] for j in idx], index=idx)
                for i, s in enumerate(settled)
            ]
        )
        halves.append(
            {
                "games": g,
                "falling": share([t["kind"] == "falling" for t in ts]),
                "rising": share([t["kind"] == "rising" for t in ts]),
            }
        )

    exploratory = {
        "games_30": share([t["kind"] == "falling" for t in flat[EXPLORE_GAMES]]),
        "rising_30": share([t["kind"] == "rising" for t in flat[EXPLORE_GAMES]]),
        "first_and_last_30": halves,
        "score_worse": [
            {"games": g, **share([t["score"] == "worse" for t in flat[g]])} for g in reads
        ],
        "any_worse": [
            {
                "games": g,
                **share(
                    [
                        "worse" in (t["rating"], t["accuracy"], t["blunders"], t["score"])
                        for t in flat[g]
                    ]
                ),
            }
            for g in reads
        ],
        "mean_change_when_falling": [
            {
                "games": g,
                "change": r6(
                    float(np.mean([t["change"] for t in flat[g] if t["kind"] == "falling"]))
                    if any(t["kind"] == "falling" for t in flat[g])
                    else None
                ),
            }
            for g in reads
        ],
    }

    # Detection: separate series per decline and length (the decline spans the observed games).
    detection = []
    detect_flags: dict[str, str] = {}
    decline_settle = {}
    for d in DECLINES:
        for g in CHECKPOINTS:
            series = run.simulate(f"decline/{d}/{g}", DECLINE_PLAYERS, g, BURN_IN, float(d), glicko)
            ts = run.trajectories(
                [request_player(f"decline-{d}-{g}-{i:04d}", s) for i, s in enumerate(series)]
            )
            flags = [t["kind"] == "falling" for t in ts]
            fall = [s[0]["rating"] - s[-1]["rating"] for s in series]
            detection.append(
                {
                    "decline": d,
                    "games": g,
                    **share(flags, DETECT_FAMILY),
                    "mean_rating_fall": r6(float(np.mean(fall))),
                }
            )
            detect_flags[f"{d}/{g}"] = bits(flags)
            decline_settle[f"{d}/{g}"] = settle(series)

    # Exploratory: new accounts (no burn-in: the series starts at 1500, deviation 500).
    new = run.simulate("new-account", NEW_PLAYERS, max(CHECKPOINTS), 0, 0.0, glicko)
    new_rows = []
    for g in CHECKPOINTS:
        ts = run.trajectories([request_player(f"new-{i:04d}", s[:g]) for i, s in enumerate(new)])
        pairs = list(zip(ts, new, strict=True))
        below = [t["kind"] == "falling" for t, s in pairs if s[0]["truth"] < 1500]
        above = [t["kind"] == "falling" for t, s in pairs if s[0]["truth"] >= 1500]
        new_rows.append(
            {
                "games": g,
                "all": share([t["kind"] == "falling" for t in ts]),
                "below_1500": share(below) if below else None,
                "at_or_above_1500": share(above) if above else None,
            }
        )

    # Exploratory sensitivity: a plain Elo rating with K 10 and 32, and a chess.com-like Glicko-1
    # whose stored rating is the one after the game (`chesscom: true`).
    other = []
    arms = [(f"elo/{k}", ELO_PLAYERS, ("elo", float(k)), False) for k in ELO_KS]
    arms.append(("glicko1", GLICKO1_PLAYERS, ("glicko1", 0.0), True))
    for name, players, system, chesscom in arms:
        series = run.simulate(name, players, max(CHECKPOINTS), BURN_IN, 0.0, system)
        row = {"arm": name, "settle": settle(series), "cells": []}
        for g in CHECKPOINTS:
            ts = run.trajectories(
                [
                    request_player(f"{name.replace('/', '-')}-{i:04d}", s[:g], chesscom=chesscom)
                    for i, s in enumerate(series)
                ]
            )
            row["cells"].append(
                {
                    "games": g,
                    "falling": share([t["kind"] == "falling" for t in ts]),
                    "rising": share([t["kind"] == "rising" for t in ts]),
                }
            )
        other.append(row)

    # Diagnostics of the world: how far the rating sits from the truth, its memory, its step,
    # and the rule's interval against the real spread of the difference.
    err = np.array([[g["pre"] - g["truth"] for g in s] for s in settled])
    lag1 = [float(np.corrcoef(row[:-1], row[1:])[0, 1]) for row in err if np.std(row) > 0]
    diagnostics = {
        "settled": settle(settled),
        "decline": decline_settle,
        "settled_error_mean": r6(float(err.mean())),
        "settled_error_sd": r6(float(err.std())),
        "settled_error_lag1": r6(float(np.mean(lag1))),
        "settled_rd_mean": r6(float(np.mean([g["rd"] for s in settled for g in s]))),
        "settled_step_mean": r6(float(np.mean([g["step"] for s in settled for g in s]))),
        "settled_window_mean_sd": {
            str(w): r6(
                float(
                    np.std(
                        [
                            np.mean(row[i : i + w])
                            for row in err
                            for i in range(0, err.shape[1] - w + 1, w)
                        ]
                    )
                )
            )
            for w in (15, 30)
        },
        "interval": {str(g): calibration(flat[g]) for g in reads},
    }

    return {
        "primary": primary,
        "secondary": {"rising": rising, "then_now_rows": secondary},
        "by_basis": by_basis,
        "detection": detection,
        "exploratory": {**exploratory, "new_account": new_rows, "other_ratings": other},
        "diagnostics": diagnostics,
        "flags": {
            "false_alarm": {
                str(g): bits([t["kind"] == "falling" for t in flat[g]]) for g in CHECKPOINTS
            },
            "detection": detect_flags,
        },
    }


def design() -> dict:
    return {
        "checkpoints": CHECKPOINTS,
        "explore_games": EXPLORE_GAMES,
        "settled_players": SETTLED_PLAYERS,
        "decline_players": DECLINE_PLAYERS,
        "declines": DECLINES,
        "new_players": NEW_PLAYERS,
        "elo": {"k": ELO_KS, "players": ELO_PLAYERS},
        "glicko1": {
            "players": GLICKO1_PLAYERS,
            "start_rd": G1_START_RD,
            "growth": G1_GROWTH,
            "rd": [G1_MIN_RD, G1_MAX_RD],
        },
        "halves": HALVES,
        "burn_in": BURN_IN,
        "glicko2": {
            "tau": TAU,
            "start": [START_RATING, START_RD, START_VOL],
            "rd": [MIN_RD, MAX_RD],
            "max_volatility": MAX_VOL,
            "periods_per_game": PERIODS_PER_GAME,
        },
        "world": {
            "strength": [STRENGTH_MEAN, STRENGTH_SD, *STRENGTH_RANGE],
            "pairing_sd": PAIRING_SD,
            "opponent_error_sd": OPP_ERROR_SD,
            "opponent_rd": OPP_RD,
            "draw_at_even": DRAW_AT_EVEN,
            "accuracy": ACC_MEAN,
            "accuracy_sd": ACC_SD,
            "accuracy_per_elo": ACC_PER_ELO,
            "blunders": BLUNDERS,
            "blunders_per_elo": BLUNDER_PER_ELO,
        },
        "seed_names": SEED_NAMES,
        "seed_namespace": SEED_NS,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument(
        "--algorithm",
        default=None,
        help="the algorithm_id the run must be on (default: whatever the cli is); the cli of "
        "$REMATCH_CLI is the version that runs, and a different one stops the run",
    )
    ap.add_argument(
        "--seed-namespace",
        default=EXP,
        help="the namespace of every named seed (default: the experiment id, the registered "
        "seeds); another namespace draws a fresh replication",
    )
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
        body = run_all(run)
    except BudgetExceeded as e:
        # A failed result, reported as one (section 9).
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

    fig, ax = plt.subplots(figsize=(6, 4))
    cells = res["primary"]["cells"]
    xs = [c["games"] for c in cells]
    ys = [c["share"] for c in cells]
    lo = [c["share"] - c["ci_bonferroni"][0] for c in cells]
    hi = [c["ci_bonferroni"][1] - c["share"] for c in cells]
    ax.errorbar(xs, ys, yerr=[lo, hi], marker="o", capsize=3, label="constant strength (false alarm)")
    for d in DECLINES:
        dc = [c for c in res["detection"] if c["decline"] == d]
        ax.plot(
            [c["games"] for c in dc],
            [c["share"] for c in dc],
            marker="s",
            label=f"falling {d} points (detection)",
        )
    ax.axhline(BOUND, color="grey", lw=0.8, ls="--")
    ax.axhline(HIDE, color="grey", lw=0.8, ls=":")
    ax.set_xscale("log")
    ax.set_xticks(xs, [str(x) for x in xs])
    ax.set_ylim(0, 1)
    ax.set_xlabel("rated games")
    ax.set_ylabel('share told "falling"')
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(fig_dir / "falling.png", dpi=120, metadata={"Software": None})
    plt.close(fig)


if __name__ == "__main__":
    raise SystemExit(main())
