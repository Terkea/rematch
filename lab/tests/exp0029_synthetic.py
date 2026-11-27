"""A synthetic cohort for EXP-0029's analysis: `lichess-db cohort`'s files for the nine months
(players.jsonl, meta.json and, for the six feature months, games.jsonl) in the layout of
`data/exp09-cohort`, and a stand-in for `cli lab observe` that draws each game's weakness counts
and accuracy from the player's true rate in that window.

The truth is known: every player's rates change from F1 to F2 by a drawn amount, and the rating
change from the end of April to the end of June is `beta_key` per SD of the improvement in
`key`, plus `beta_control` per SD of the improvement in accuracy, plus noise. Nothing here reads a
real month.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import numpy as np

KEYS = ("W4.missed", "W4.allowed", "W5", "W6", "W7.traps", "W7.prep", "W9a", "W9b")
H = "2025-09"
F1 = ("2025-10", "2025-11", "2025-12")
F2 = ("2026-01", "2026-02", "2026-03")
G = "2026-04"
J = "2026-06"
MONTHS = (H, *F1, *F2, G, J)
SALT = hashlib.sha256(b"EXP-0009/cohort/salt").hexdigest()
D_SD = 0.04  # the true change in a rate, F1 to F2
ACC_SD = 2.5  # the true change in accuracy
PGN = ('[Event "Rated Rapid game"]\n[White "?"]\n[Black "?"]\n[Result "1-0"]\n'
       '[TimeControl "600+0"]\n\n1. e4 { [%eval 0.2] [%clk 0:10:00] } 1-0\n')


def pseudonym(rng: np.random.Generator, lo: float, hi: float) -> str:
    frac = int(rng.uniform(lo, hi) * 2**64)
    return f"{frac:016x}" + "".join(f"{x:02x}" for x in rng.integers(0, 256, 8))


def ends(t0: int, gid: str, start: float, end: float) -> dict[str, Any]:
    return {"first": [[t0 + i, f"{gid}f{i}", start, start] for i in range(5)],
            "last": [[t0 + 900_000 + i, f"{gid}l{i}", end, end] for i in range(5)]}


def build(root: Path, *, players: int = 1000, seed: int = 1, key: str = "W7.traps",
          beta_key: float = 0.0, beta_control: float = 15.0, noise: float = 30.0
          ) -> dict[str, Any]:
    """Writes the cohort under `root/exp29-cohort/` and returns the truth the fake observe needs
    (each game's player, window and true rates)."""
    rng = np.random.default_rng(seed)
    base = root / "exp29-cohort"
    lines: dict[str, list[str]] = {m: [] for m in MONTHS}
    games: dict[str, list[str]] = {m: [] for m in (*F1, *F2)}
    truth: dict[str, Any] = {}
    counter = 0
    for i in range(players):
        lo, hi = (0.0, 0.3) if i % 10 == 0 else (0.3, 0.9)  # a tenth in EXP-0009's range
        h = pseudonym(rng, lo, hi)
        p1 = {k: float(rng.uniform(0.08, 0.35)) for k in KEYS}
        d = {k: float(rng.normal(0, D_SD)) for k in KEYS}
        p2 = {k: float(np.clip(p1[k] + d[k], 0.01, 0.9)) for k in KEYS}
        a1 = float(rng.normal(75, 6))
        da = float(rng.normal(0, ACC_SD))
        rates = {"F1": (p1, a1), "F2": (p2, a1 + da)}
        rh0 = float(rng.uniform(900, 1850))
        rh1 = rh0 + float(rng.normal(0, 15))
        r1 = rh1 + float(rng.normal(0, 20))
        r2 = r1 + float(rng.normal(0, 20))
        rg = r2 + float(rng.normal(0, 15))
        y = (beta_key * (-d[key] / D_SD) + beta_control * (da / ACC_SD)
             + float(rng.normal(0, noise)))
        rj = rg + y
        quiet_f2 = i % 13 == 0  # not eligible: too few analysed games in F2
        left = i % 11 == 0  # no June games: attrition
        month_end = {H: (rh0, rh1), F1[0]: (rh1, rh1 + (r1 - rh1) / 3),
                     F1[1]: (rh1 + (r1 - rh1) / 3, rh1 + 2 * (r1 - rh1) / 3),
                     F1[2]: (rh1 + 2 * (r1 - rh1) / 3, r1),
                     F2[0]: (r1, r1 + (r2 - r1) / 3), F2[1]: (r1 + (r2 - r1) / 3,
                                                             r1 + 2 * (r2 - r1) / 3),
                     F2[2]: (r1 + 2 * (r2 - r1) / 3, r2), G: (r2, rg), J: (rg, rj)}
        for mi, m in enumerate(MONTHS):
            if m == J and left:
                continue
            if m in F1 or m in F2:
                ev = 3 if (quiet_f2 and m in F2) else int(rng.integers(12, 28))
            else:
                ev = 0
            rapid = ev + int(rng.integers(5, 25)) if m != H else int(rng.integers(20, 60))
            if m == G:
                rapid = int(rng.integers(5, 90))
            start, end = month_end[m]
            rec = {"h": h, "rapid_games": rapid, "eval_games": ev, "bullet": 0,
                   "blitz": int(rng.integers(0, 10)), "classical": 0,
                   **ends(mi * 10_000_000, f"{i}{m}", round(start, 1), round(end, 1))}
            lines[m].append(json.dumps(rec, separators=(",", ":")))  # "h" first, as the cohort writes
            w = "F1" if m in F1 else "F2" if m in F2 else None
            for _ in range(ev if w else 0):
                gid = f"g{counter:07d}"
                counter += 1
                white = bool(rng.integers(0, 2))
                opp = pseudonym(rng, 0.95, 1.0)
                g = {"id": gid, "utc": mi * 10_000_000 + counter, "base_seconds": 600,
                     "increment_seconds": 0, "white": h if white else opp,
                     "black": opp if white else h, "white_elo": int(r1), "black_elo": int(r1),
                     "white_rating_diff": 0, "black_rating_diff": 0, "result": "1-0",
                     "termination": "Normal", "pgn": PGN}
                games[m].append(json.dumps(g, sort_keys=True))
                truth[gid] = {"color": "white" if white else "black", "rates": rates[w]}
    for m in MONTHS:
        out = base / m
        out.mkdir(parents=True, exist_ok=True)
        lines[m].sort()
        (out / "players.jsonl").write_text("\n".join(lines[m]) + "\n", encoding="utf-8")
        (out / "meta.json").write_text(json.dumps({
            "month": m, "url": f"synthetic:{m}", "sha256": "0" * 64, "compressed_bytes": 0,
            "games_seen": len(games.get(m, [])), "games_kept": len(games.get(m, [])),
            "players": len(lines[m]), "eval_games": len(games.get(m, [])),
            "player_rate": 0.9, "salt": SALT}), encoding="utf-8")
        if m in games:
            (out / "games.jsonl").write_text("\n".join(games[m]) + "\n", encoding="utf-8")
    return truth


def fake_observe(truth: dict[str, Any]):
    """A stand-in for `rematch_lab.observe`: per game, the followed side's counts drawn from its
    window's true rates by a generator seeded on the game id (pure, order-free)."""

    def observe(req: Any) -> Any:
        out = []
        for g in req.games:
            gid = g.id.split(":", 1)[1]
            t = truth[gid]
            rng = np.random.default_rng(int(hashlib.sha256(gid.encode()).hexdigest()[:8], 16))
            if rng.random() < 0.01:
                out.append(SimpleNamespace(id=g.id, error="synthetic failure", sides=[]))
                continue
            rates, acc = t["rates"]
            keys = {}
            for k in KEYS:
                opps = int(rng.integers(1, 5))
                keys[k] = SimpleNamespace(events=int(rng.binomial(opps, rates[k])),
                                          opportunities=opps)
            side = SimpleNamespace(color=t["color"], accuracy=float(rng.normal(acc, 8)),
                                   moves=40, blunders=int(rng.binomial(40, 0.03)), keys=keys)
            out.append(SimpleNamespace(id=g.id, error=None, sides=[side]))
        return SimpleNamespace(algorithm_id="algo-synthetic", games=out)

    return observe


def fake_evaluate(*_: Any, **__: Any) -> Any:
    from rematch_lab.models import EngineScore

    s = SimpleNamespace(fen="startpos", score=EngineScore(cp=20), best_uci="e2e4")
    return SimpleNamespace(searches=[s])
