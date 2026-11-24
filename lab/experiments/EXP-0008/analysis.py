"""EXP-0008: is 300,000 nodes enough? Depth against judgements and cost (preregistration.md).

Every number here comes from the production Rust code through `rematch_lab`: the positions a game
asks the engine for (`cli lab positions`, from `evaluate_game`), the searches (`cli lab evaluate`,
the production protocol at each node count, and the deep reference), and every judgement and
accuracy (`cli lab judge`: `evaluate_game`, `build_move_records`, `game_accuracy`). Python here
samples, counts and decides; it computes no evaluation, judgement or accuracy. The one thing it
reads itself is Lichess's `[%eval]` comments, which it hands to `judge` in the shape production
stores a Lichess game's evaluations.

Stages, in the order they run (each stage's output is committed before the next one runs):

    python analysis.py sample --stage pilot            # population.csv, sample-pilot.csv
    python analysis.py pilot --out DIR                 # DIR/pilot.json
    python analysis.py power --pilot pilot/pilot.json  # power.json, figures/power.png
    python analysis.py sample --stage confirmatory --n N   # sample.csv
    python analysis.py --out DIR                       # the registered run: DIR/results.json

Only the last is the registered run; the lab check reruns it twice and compares results.json.

Every search is kept in a cache in the data directory (`exp08-evals/`, git-ignored), one JSON
line per position, keyed by the setting and the Stockfish build; a second run reads it instead of
searching again. The cache never changes a result: the run re-searches a seeded few positions of
every setting without it and stops unless they match (section 7). The budget counts the nodes the
design asks for, cached or not, so results.json is the same either way.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
import re
import sys
from collections import defaultdict
from functools import lru_cache
from pathlib import Path
from typing import Any

import numpy as np
from scipy.stats import chi2, norm
from scipy.stats import t as student_t

import rematch_lab
from rematch_lab.experiments import Budget, external_input, seed, write_results
from rematch_lab.models import DepthGame, EngineScore, GivenSearch, JudgeRequest

EXP = "EXP-0008"
HERE = Path(__file__).resolve().parent
PGN = "lichess-games/lichess_db_standard_rated_2026-10.head.pgn"
PGN_SHA256 = "ee83071caabf515c7df5ebb03b8b6246be1a887689135c9f5e0d952409e21604"
CACHE = "exp08-evals"

# Section 7: the population.
EVENT = "Rated Rapid game"
TERMINATIONS = ("Normal", "Time forfeit")
RATING_LO, RATING_HI = 800, 2200  # the mean of the two ratings
MIN_PLIES = 20
BANDS = (("800-1199", 800, 1200), ("1200-1599", 1200, 1600), ("1600-1999", 1600, 2000),
         ("2000-2199", 2000, 2200))
PILOT_GAMES = 20

# Section 3 and 7: the settings. The candidates are the production protocol at each node count
# (one thread, 16 MB, no tables); the reference is a single-thread search a hundred times
# deeper than production, with a larger hash and the Syzygy 3-4-5 tables.
SETTINGS: dict[str, dict[str, Any]] = {
    "n50k": {"nodes": 50_000},
    "n100k": {"nodes": 100_000},
    "n300k": {"nodes": 300_000},
    "n1m": {"nodes": 1_000_000},
    "n3m": {"nodes": 3_000_000},
    "ref": {"nodes": 30_000_000, "hash_mb": 128, "syzygy": True},
    # The step up's check (section 9): a reference three times deeper, on a seeded 40 games.
    "deep": {"nodes": 100_000_000, "hash_mb": 256, "syzygy": True},
}
CANDIDATES = ("n50k", "n100k", "n300k", "n1m")
# The settings whose disagreements always get the reference: the two contrasts that decide
# (section 9). 50,000 and 3 million get it on the moves they add only when their test is reached.
DECIDING = ("n100k", "n300k", "n1m")
PRODUCTION = "n300k"
LABELS = ("none", "inaccuracy", "mistake", "blunder")

# Section 5 and 9.
DELTA = 0.01
LEVEL = 0.95
# The step down's "within" end is read at 99%: on 40 games the per-game totals are skewed and the
# 95% t interval let a step down at the boundary through about 5% of the time (section 6).
LEVEL_DOWN = 0.99
# The concordant moves (every candidate gives the same label) whose reference is searched.
CONCORDANT_PILOT = 40
CONCORDANT_CONFIRMATORY = 100
RECHECK = 12
# The step down's contrast is read on a seeded 40 of the confirmatory games, the step up on all
# of them; the step up's 100-million-node check on another seeded 40 (section 7).
STEP_DOWN_GAMES = 40
DEEP_GAMES = 40
BOOT = 2000

# Section 6.
N_GRID = (40, 60, 80, 100, 120, 140, 160, 180, 200, 240, 300, 400)
POWER_SIMS = 2000
TARGET_POWER = 0.80
# Section 0: the confirmatory run may plan at most this many CPU-hours (at the pilot's rate).
CEILING_CPU_HOURS = 12.0

# Section 0: the registered run's budget, in nodes the design asks for (filled from section 6).
BUDGET: dict[str, float] = {
    # 160 games, about 10,190 positions, at 50,000 + 100,000 + 300,000 + 1 million nodes.
    "candidate_nodes": 1.5e10,
    # About 1,536 positions at 30 million: 6.45 a game for the step up's disagreements, 7.6 more
    # a game in the 40 step-down games, and two for each of the 100 concordant moves.
    "reference_nodes": 4.7e10,
    # Whichever conditional branch section 9 reaches (they exclude each other): the step up's
    # check, about 258 positions at 100 million (2.6e10), or the reference on the moves 50,000 adds
    # in the step-down games (about 5e9).
    "branch_nodes": 2.6e10,
    # 12 positions of each setting run, searched again without the cache.
    "recheck_nodes": 1.6e9,
}
CAP = 1.5

ENGINES = int(os.environ.get("EXP08_ENGINES", "10"))  # speed only; never a result


def log(msg: str) -> None:
    print(f"{EXP}: {msg}", file=sys.stderr, flush=True)


# ---- the Lichess games -----------------------------------------------------------------------

HEADER = re.compile(r'^\[(\w+) "(.*)"\]$', re.MULTILINE)
COMMENT = re.compile(r"\{([^}]*)\}")
EVAL = re.compile(r"\[%eval (#?)(-?\d+(?:\.\d+)?)\]")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def lichess_eval(comment: str) -> EngineScore | None:
    """Lichess's `[%eval]` after a ply, White's point of view: pawns with two decimals, or `#n`
    for a mate in n (negative when Black mates)."""
    m = EVAL.search(comment)
    if not m:
        return None
    if m.group(1) == "#":
        return EngineScore(mate=int(m.group(2)))
    return EngineScore(cp=round(float(m.group(2)) * 100))


def read_games() -> list[dict[str, Any]]:
    """Every game of the pinned extract, in file order."""
    path = external_input(PGN)
    if sha256_file(path) != PGN_SHA256:
        raise SystemExit(f"{path} is not the registered extract")
    text = path.read_text(encoding="utf-8")
    games = []
    for chunk in re.split(r"\n\n(?=\[Event )", text):
        chunk = chunk.strip("\n")
        if not chunk:
            continue
        head, _, moves = chunk.partition("\n\n")
        h = dict(HEADER.findall(head))
        site = h.get("Site", "")
        games.append(
            {
                "id": site.rsplit("/", 1)[-1],
                "headers": h,
                "pgn": chunk + "\n",
                "comments": COMMENT.findall(moves),
            }
        )
    return games


def clock(h: dict[str, str]) -> tuple[int | None, int | None]:
    base, _, inc = h.get("TimeControl", "").partition("+")
    try:
        return int(base), int(inc)
    except ValueError:
        return None, None


def depth_game(g: dict[str, Any], lichess: bool = False) -> DepthGame:
    base, inc = clock(g["headers"])
    return DepthGame(
        id=g["id"],
        pgn=g["pgn"],
        base_seconds=base,
        increment_seconds=inc,
        lichess_evals=[lichess_eval(c) for c in g["comments"]] if lichess else None,
    )


def band_of(rating: float) -> str:
    for name, lo, hi in BANDS:
        if lo <= rating < hi:
            return name
    raise ValueError(rating)


def positions_of(games: list[dict[str, Any]]) -> dict[str, Any]:
    """`cli lab positions` for every game, in chunks."""
    out: dict[str, Any] = {}
    for i in range(0, len(games), 500):
        resp = rematch_lab.positions([depth_game(g) for g in games[i : i + 500]])
        for gp in resp.games:
            out[gp.id] = gp
    return out


def population() -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Section 7's population: rated rapid games (not tournaments) with Lichess's evaluation on
    every ply (the final ply may lack one), a mean rating in [800, 2200), at least 20 plies, and
    an ending on the board or on time, in file order."""
    keep = []
    for g in read_games():
        h = g["headers"]
        if h.get("Event") != EVENT or h.get("Termination") not in TERMINATIONS:
            continue
        if h.get("Variant", "Standard") != "Standard" or "FEN" in h:
            continue
        try:
            w, b = int(h["WhiteElo"]), int(h["BlackElo"])
        except (KeyError, ValueError):
            continue
        mean = (w + b) / 2
        if not RATING_LO <= mean < RATING_HI:
            continue
        g["mean_elo"] = mean
        keep.append(g)
    pos = positions_of(keep)
    out = []
    for g in keep:
        gp = pos[g["id"]]
        evals = [lichess_eval(c) for c in g["comments"]]
        if gp.plies < MIN_PLIES or len(g["comments"]) != gp.plies:
            continue
        if any(e is None for e in evals[:-1]):
            continue
        g["plies"] = gp.plies
        out.append(g)
    return out, pos


def write_ids(name: str, rows: list[dict[str, Any]]) -> None:
    with (HERE / name).open("w", encoding="utf-8", newline="\n") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(["game_id", "white", "black", "mean_elo", "plies"])
        for g in rows:
            h = g["headers"]
            w.writerow([g["id"], h["White"], h["Black"], f"{g['mean_elo']:g}", g["plies"]])


def read_ids(name: str) -> list[str]:
    with (HERE / name).open(encoding="utf-8") as f:
        return [r["game_id"] for r in csv.DictReader(f)]


def accepted_order(pop: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """The population in the seeded order `sample/order`, keeping a game only when neither of
    its players is in a game kept before it, so no player is in two sampled games."""
    rng = np.random.default_rng(seed(EXP, "sample/order"))
    order = rng.permutation(len(pop))
    seen: set[str] = set()
    out = []
    for i in order:
        g = pop[int(i)]
        players = {g["headers"]["White"].lower(), g["headers"]["Black"].lower()}
        if players & seen:
            continue
        seen |= players
        out.append(g)
    return out


def cmd_sample(stage: str, n: int | None) -> None:
    pop, _ = population()
    log(f"population: {len(pop)} games")
    write_ids("population.csv", pop)
    order = accepted_order(pop)
    if stage == "pilot":
        write_ids("sample-pilot.csv", order[:PILOT_GAMES])
    else:
        if n is None:
            raise SystemExit("the confirmatory sample needs --n")
        write_ids("sample.csv", order[PILOT_GAMES : PILOT_GAMES + n])


# ---- searches, with the cache ----------------------------------------------------------------


@lru_cache(maxsize=None)
def engine_id() -> str:
    """The Stockfish build and the engine crate's source digest (the UCI protocol): a cached
    search is reused only when both are the ones this cli was built with."""
    facts = rematch_lab.algorithm()["components"]["facts"]
    return f"{str(facts['stockfish']['sha256'])[:12]}-{facts['source_digest']['backend/crates/engine/src'][:12]}"


def cache_path(setting: str) -> Path:
    """One file per setting, named by everything that decides a search: the nodes, the hash, the
    tables and the engine."""
    cfg = SETTINGS[setting]
    key = f"{cfg['nodes']}n-{cfg.get('hash_mb', 16)}mb-{'tb' if cfg.get('syzygy') else 'notb'}"
    return external_input(f"{CACHE}/{setting}-{key}-{engine_id()}.jsonl")


# A copy of `backend/crates/lichess-db/data/syzygy-345.sha256`, the pinned set (an input).
SYZYGY_MANIFEST = HERE / "syzygy-345.sha256"


@lru_cache(maxsize=None)
def check_tables() -> int:
    """The Syzygy 3-4-5 tables at `SYZYGY_PATH` are the pinned set, file for file (Stockfish loads
    an empty or partial directory without a word, so the run checks it first)."""
    root = Path(os.environ.get("SYZYGY_PATH", ""))
    lines = [ln.split() for ln in SYZYGY_MANIFEST.read_text(encoding="utf-8").splitlines() if ln.strip()]
    bad = [name for want, name in lines
           if not (root / Path(name).name).is_file() or sha256_file(root / Path(name).name) != want]
    if not lines or bad:
        raise SystemExit(f"SYZYGY_PATH={root}: {len(bad)} of {len(lines)} pinned tables missing or "
                         f"different, for example {bad[:1]}")
    return len(lines)


def load_cache(setting: str) -> dict[str, dict[str, Any]]:
    path = cache_path(setting)
    out: dict[str, dict[str, Any]] = {}
    if path.is_file():
        with path.open(encoding="utf-8") as f:
            for line in f:
                if line.strip():
                    row = json.loads(line)
                    out[row["fen"]] = row
    return out


def search_row(s: Any) -> dict[str, Any]:
    return {
        "fen": s.fen,
        "score": json.loads(s.score.model_dump_json()),
        "best_uci": s.best_uci,
        "depth": s.depth,
        "nodes": s.nodes,
        "cpu_ms": s.cpu_ms,
    }


def search(setting: str, fens: list[str]) -> list[dict[str, Any]]:
    """One `cli lab evaluate` call at a setting, without the cache."""
    cfg = SETTINGS[setting]
    if cfg.get("syzygy"):
        check_tables()
    resp = rematch_lab.evaluate(
        fens,
        nodes=cfg["nodes"],
        hash_mb=cfg.get("hash_mb"),
        syzygy=cfg.get("syzygy", False),
        engines=ENGINES,
    )
    return [search_row(s) for s in resp.searches]


def searches(setting: str, fens: list[str], budget: Budget | None, unit: str | None) -> dict[str, dict[str, Any]]:
    """Every FEN at a setting: from the cache where it is there, else searched and cached. The
    budget counts the design's nodes (each distinct FEN at the setting's count) before anything
    is searched, so an over-budget plan stops before it spends."""
    want = list(dict.fromkeys(fens))
    if SETTINGS[setting].get("syzygy") and want:
        check_tables()  # every time, cached or not: a cached search must have had the tables too
    if budget is not None and unit is not None:
        budget.spend(unit, SETTINGS[setting]["nodes"] * len(want))
    cache = load_cache(setting)
    missing = [f for f in want if f not in cache]
    if missing:
        path = cache_path(setting)
        path.parent.mkdir(parents=True, exist_ok=True)
        step = 400 if SETTINGS[setting]["nodes"] <= 1_000_000 else 40
        for i in range(0, len(missing), step):
            rows = search(setting, missing[i : i + step])
            with path.open("a", encoding="utf-8", newline="\n") as f:
                for r in rows:
                    f.write(json.dumps(r, sort_keys=True) + "\n")
                    cache[r["fen"]] = r
            log(f"{setting}: searched {min(i + step, len(missing))} of {len(missing)} "
                f"({len(want) - len(missing)} cached)")
    return {f: cache[f] for f in want}


def recheck(setting: str, cached: dict[str, dict[str, Any]], budget: Budget | None) -> dict[str, Any]:
    """Section 7: a seeded few positions of the setting searched again without the cache. The
    run stops unless every one matches its cached search exactly."""
    fens = sorted(cached)
    rng = np.random.default_rng(seed(EXP, f"recheck/{setting}"))
    pick = [fens[int(i)] for i in rng.choice(len(fens), size=min(RECHECK, len(fens)), replace=False)]
    if budget is not None:
        budget.spend("recheck_nodes", SETTINGS[setting]["nodes"] * len(pick))
    fresh = {r["fen"]: r for r in search(setting, pick)}
    keys = ("score", "best_uci", "depth", "nodes")
    bad = [f for f in pick if any(fresh[f][k] != cached[f][k] for k in keys)]
    if bad:
        raise SystemExit(f"{setting}: {len(bad)} of {len(pick)} re-searched positions differ "
                         f"from the cache, for example {bad[0]}")
    return {"positions": len(pick), "identical": len(pick)}


def cpu_hours(rows: dict[str, dict[str, Any]]) -> float:
    return sum((r.get("cpu_ms") or 0.0) for r in rows.values()) / 3.6e6


# ---- judgements ------------------------------------------------------------------------------


def judge(games: list[dict[str, Any]], found: dict[str, dict[str, Any]], lichess: bool = False):
    """`cli lab judge` on the given searches; per game, the judged moves and the accuracies."""
    out = {}
    for i in range(0, len(games), 50):
        chunk = games[i : i + 50]
        need = {f for g in chunk for f in g["pos"].fens}
        req = JudgeRequest(
            games=[depth_game(g, lichess) for g in chunk],
            searches=[
                GivenSearch(fen=f, score=EngineScore(**found[f]["score"]),
                            best_uci=found[f]["best_uci"])
                for f in sorted(need) if f in found
            ],
        )
        for jg in rematch_lab.judge(req).games:
            out[jg.id] = jg
    return out


def label(m: Any) -> str:
    return m.judgement or "none"


def move_fens(gp: Any, ply: int) -> list[str]:
    """The FENs whose searches judge the move at `ply` (1-based): the positions before and after
    it, as `evaluate_game` scores them; a final position scored without a search needs none."""
    return [f for f in (gp.sequence[ply - 1], gp.sequence[ply]) if f is not None]


def labels_by_setting(games, judged: dict[str, dict]) -> dict[str, dict[str, list[str]]]:
    return {s: {g["id"]: [label(m) for m in judged[s][g["id"]].moves] for g in games}
            for s in judged}


# ---- estimates -------------------------------------------------------------------------------


@lru_cache(maxsize=None)
def t_crit(level: float, n: int) -> float:
    return float(student_t.ppf(1 - (1 - level) / 2, max(n - 1, 1)))


def ratio(e: np.ndarray, m: np.ndarray, level: float) -> dict[str, Any]:
    """The ratio Σe/Σm over games with its linearised variance and a t interval (section 8)."""
    n = len(e)
    r = float(e.sum() / m.sum())
    resid = e - r * m
    se = float(math.sqrt(n / (n - 1) * float((resid**2).sum())) / m.sum()) if n > 1 else math.inf
    out = {"estimate": r, "se": se}
    for name, lv in (("ci80", 0.80), ("ci95", 0.95), ("ci99", 0.99), ("ci_decision", level)):
        t = t_crit(lv, n)
        out[name] = [r - t * se, r + t * se]
    return out


def contrast_totals(games, labs, ref: dict[str, list[str | None]], a: str, b: str):
    """Per game, Σ (d_a − d_b) over its moves, with d_N = 1 when N's label is not the
    reference's. A move where a and b agree adds 0 whatever the reference says, so the
    reference is needed only where they differ (and is there: section 7)."""
    e, m = [], []
    for g in games:
        la, lb, lr = labs[a][g["id"]], labs[b][g["id"]], ref[g["id"]]
        tot = 0
        for x, y, r in zip(la, lb, lr, strict=True):
            if x == y:
                continue
            if r is None:
                raise SystemExit(f"{g['id']}: a discordant move has no reference")
            tot += int(x != r) - int(y != r)
        e.append(tot)
        m.append(len(la))
    return np.array(e, dtype=float), np.array(m, dtype=float)


def within(c: dict) -> bool:
    """A step down's upper end, at LEVEL_DOWN, below the tolerance."""
    return c["ci99"][1] < DELTA


def step_down_shown(c100: dict, g1m: dict) -> bool:
    return g1m["ci_decision"][1] < DELTA and within(c100)


def decide(c100: dict, c50: dict | None, g1m: dict, deep: dict | None) -> dict[str, Any]:
    """Section 9, read on the 95% intervals. The step up needs its lower end above the tolerance
    against the 30-million-node reference and, on the seeded 40 games, against the 100-million."""
    lo_g, hi_g = g1m["ci_decision"]
    lo_d, hi_d = c100["ci_decision"]
    if lo_g > DELTA:
        if deep is not None and deep["ci_decision"][0] > DELTA:
            return {"outcome": "300k falls short", "setting": "n1m"}
        return {"outcome": "need more data", "setting": PRODUCTION}
    if step_down_shown(c100, g1m):
        if c50 is not None and within(c50):
            return {"outcome": "100k is enough", "setting": "n50k"}
        return {"outcome": "100k is enough", "setting": "n100k"}
    if hi_g < DELTA and lo_d > DELTA:
        return {"outcome": "300k is right", "setting": PRODUCTION}
    return {"outcome": "need more data", "setting": PRODUCTION}


def boot_rng(name: str) -> np.random.Generator:
    return np.random.default_rng(seed(EXP, f"bootstrap/{name}"))


def stratified(games, per_game: list[tuple[float, float]], name: str) -> dict[str, Any]:
    """A share over all moves from per-game (weighted events, moves), with a seeded bootstrap
    over games (exploratory)."""
    y = np.array([p[0] for p in per_game])
    m = np.array([p[1] for p in per_game])
    est = float(y.sum() / m.sum())
    rng = boot_rng(name)
    idx = rng.integers(0, len(y), size=(BOOT, len(y)))
    reps = y[idx].sum(axis=1) / m[idx].sum(axis=1)
    return {
        "estimate": est,
        "ci80": [float(np.quantile(reps, 0.10)), float(np.quantile(reps, 0.90))],
        "ci95": [float(np.quantile(reps, 0.025)), float(np.quantile(reps, 0.975))],
    }


# ---- one stage: candidates, reference, every metric ------------------------------------------


def evaluate_stage(games, budget: Budget | None, concordant: int, stage: str,
                   pilot: bool, rechecks: bool) -> dict[str, Any]:
    """Searches the candidates on every position, then the reference on: every move where
    300,000 and 1 million disagree; in the step-down games (all of them in the pilot, a seeded 40
    in the confirmatory run) every move where 100,000, 300,000 and 1 million disagree; and
    `concordant` seeded moves of the rest. Conditional branches (section 9): the moves 50,000 adds
    in the step-down games when the step down is shown; the 100-million-node check of the step up
    on a seeded 40 games when the step up is shown. The pilot runs both branches and 3 million
    nodes, for the record."""
    all_fens = sorted({f for g in games for f in g["pos"].fens})
    found: dict[str, dict[str, dict[str, Any]]] = {}
    for s in CANDIDATES:
        found[s] = searches(s, all_fens, budget, "candidate_nodes")
    judged = {s: judge(games, found[s]) for s in CANDIDATES}
    labs = labels_by_setting(games, judged)
    by_id = {g["id"]: g for g in games}

    def subset(name: str, k: int) -> list[dict[str, Any]]:
        if pilot or k >= len(games):
            return list(games)
        rng = np.random.default_rng(seed(EXP, f"{name}/{stage}"))
        pick = sorted(int(i) for i in rng.choice(len(games), size=k, replace=False))
        return [games[i] for i in pick]

    down_games = subset("step-down", STEP_DOWN_GAMES)
    down_ids = {g["id"] for g in down_games}

    def disc_in(gs, settings: tuple[str, ...]) -> set[tuple[str, int]]:
        out = set()
        for g in gs:
            cols = [labs[s][g["id"]] for s in settings]
            for p, row in enumerate(zip(*cols, strict=True), start=1):
                if len(set(row)) > 1:
                    out.add((g["id"], p))
        return out

    def reference_for(moves: set[tuple[str, int]], unit: str, setting: str = "ref") -> None:
        have = found.get(setting, {})
        fens = sorted({f for gid, p in moves for f in move_fens(by_id[gid]["pos"], p)} - set(have))
        found[setting] = {**have, **searches(setting, fens, budget, unit)}

    # The referenced stratum: every move whose disagreement gets the reference, game by game.
    discordant = disc_in(games, ("n300k", "n1m")) | disc_in(down_games, DECIDING)
    concordant_all = sorted((g["id"], p) for g in games for p in range(1, g["plies"] + 1)
                            if (g["id"], p) not in discordant)
    rng = np.random.default_rng(seed(EXP, f"concordant/{stage}"))
    pick = rng.choice(len(concordant_all), size=min(concordant, len(concordant_all)), replace=False)
    conc_sample = {concordant_all[int(i)] for i in pick}
    found["ref"] = {}
    reference_for(discordant | conc_sample, "reference_nodes")
    deciding_positions = len(found["ref"])

    def labels_on(setting: str, gs) -> tuple[dict[str, list[str | None]], dict[str, Any]]:
        # A deep setting's label where both of a move's positions have its search, else none: the
        # hybrid search set is the deep setting's on every searched move (a move reads only its
        # two positions).
        hybrid = {**found[PRODUCTION], **found[setting]}
        jr = judge(gs, hybrid)
        out = {}
        for g in gs:
            row = []
            for p, m in enumerate(jr[g["id"]].moves, start=1):
                fens = move_fens(g["pos"], p)
                row.append(label(m) if all(f in found[setting] for f in fens) else None)
            out[g["id"]] = row
        return out, jr

    ref, jref = labels_on("ref", games)
    totals = {"n100k-n300k": contrast_totals(down_games, labs, ref, "n100k", PRODUCTION),
              "n300k-n1m": contrast_totals(games, labs, ref, PRODUCTION, "n1m")}
    c100 = ratio(*totals["n100k-n300k"], LEVEL)
    g1m = ratio(*totals["n300k-n1m"], LEVEL)
    settings: tuple[str, ...] = CANDIDATES

    c50 = None
    if step_down_shown(c100, g1m) or pilot:
        extra = disc_in(down_games, (*DECIDING, "n50k")) - discordant
        if extra:
            reference_for(extra, "branch_nodes")
            discordant |= extra
            ref, jref = labels_on("ref", games)
        totals["n50k-n300k"] = contrast_totals(down_games, labs, ref, "n50k", PRODUCTION)
        c50 = ratio(*totals["n50k-n300k"], LEVEL)

    deep = None
    if g1m["ci_decision"][0] > DELTA or pilot:
        # The step up, checked against a reference three times deeper on a seeded 40 games
        # (section 9): the contrast is exact on the moves where 300,000 and 1 million disagree.
        deep_games = subset("deep", DEEP_GAMES)
        moves = disc_in(deep_games, ("n300k", "n1m"))
        reference_for(moves, "branch_nodes", "deep")
        deep_labels, _ = labels_on("deep", deep_games)
        e, m = contrast_totals(deep_games, labs, deep_labels, PRODUCTION, "n1m")
        totals["n300k-n1m-deep"] = (e, m)
        deep = ratio(e, m, LEVEL)
        changed = sum(ref[gid][p - 1] != deep_labels[gid][p - 1] for gid, p in moves)
        deep["games"] = len(deep_games)
        deep["moves"] = len(moves)
        deep["reference_label_changed"] = changed / len(moves) if moves else 0.0

    if pilot:
        found["n3m"] = searches("n3m", all_fens, budget, "branch_nodes")
        judged["n3m"] = judge(games, found["n3m"])
        labs = labels_by_setting(games, judged)
        extra = disc_in(games, ("n1m", "n3m")) - discordant
        reference_for(extra, "branch_nodes")
        discordant |= extra
        ref, jref = labels_on("ref", games)
        totals["n1m-n3m"] = contrast_totals(games, labs, ref, "n1m", "n3m")
        settings = (*CANDIDATES, "n3m")
    decision = decide(c100, c50, g1m, deep)

    # The strata of the exploratory, absolute measures: every discordant move (weight 1) and the
    # seeded concordant moves (weight = concordant moves / sampled).
    n_conc = sum(g["plies"] for g in games) - len(discordant)
    conc_sample = {k for k in conc_sample if k not in discordant}
    w_conc = n_conc / len(conc_sample) if conc_sample else 0.0

    def weight(gid: str, p: int) -> float:
        if (gid, p) in discordant:
            return 1.0
        return w_conc if (gid, p) in conc_sample else 0.0

    lichess = judge(games, found[PRODUCTION], lichess=True)
    labs["lichess"] = {g["id"]: [label(x) for x in lichess[g["id"]].moves] for g in games}

    absolute: dict[str, Any] = {}
    best_move: dict[str, Any] = {}
    win_error: dict[str, Any] = {}
    for s in (*settings, "lichess"):
        dis, bm, we = [], [], []
        for g in games:
            gid = g["id"]
            ys = yb = yw = 0.0
            mw = 0.0
            for p in range(1, g["plies"] + 1):
                w = weight(gid, p)
                r = ref[gid][p - 1]
                if w == 0:
                    continue
                if r is None:
                    raise SystemExit(f"{gid} ply {p}: a weighted move has no reference")
                ys += w * (labs[s][gid][p - 1] != r)
                if s != "lichess":
                    rm = jref[gid].moves[p - 1]
                    sm = judged[s][gid].moves[p - 1]
                    yb += w * (sm.best_uci == rm.best_uci)
                    yw += w * abs(sm.win_before - rm.win_before)
                mw += w
            dis.append((ys, g["plies"]))
            bm.append((yb, mw))
            we.append((yw, mw))
        absolute[s] = stratified(games, dis, f"absolute/{s}")
        if s != "lichess":
            best_move[s] = stratified(games, bm, f"best-move/{s}")
            win_error[s] = stratified(games, we, f"win-error/{s}")

    # Agreement of each setting's labels with production's, over every move (no reference).
    with_production: dict[str, Any] = {}
    for s in (*settings, "lichess"):
        if s == PRODUCTION:
            continue
        e = np.array([sum(x != y for x, y in zip(labs[s][g["id"]], labs[PRODUCTION][g["id"]],
                                                   strict=True)) for g in games], dtype=float)
        mm = np.array([g["plies"] for g in games], dtype=float)
        with_production[s] = ratio(e, mm, LEVEL)

    # Accuracy, per player and game, against production's (confirmatory secondary).
    accuracy: dict[str, Any] = {}
    for s in settings:
        if s == PRODUCTION:
            continue
        diffs = []
        for g in games:
            a, b = judged[s][g["id"]], judged[PRODUCTION][g["id"]]
            diffs.append([a.accuracy_white - b.accuracy_white, a.accuracy_black - b.accuracy_black])
        d = np.array(diffs)
        n = len(d)
        def mean_ci(per_game: np.ndarray) -> dict[str, Any]:
            mu = float(per_game.mean())
            se = float(per_game.std(ddof=1) / math.sqrt(n)) if n > 1 else math.inf
            t = t_crit(0.95, n)
            return {"estimate": mu, "ci95": [mu - t * se, mu + t * se]}
        accuracy[s] = {
            "mean_difference": mean_ci(d.mean(axis=1)),
            "mean_absolute_difference": mean_ci(np.abs(d).mean(axis=1)),
            "share_moved_one_point": mean_ci((np.abs(d) >= 1.0).mean(axis=1)),
        }

    # Labels per player and game at each setting, and Lichess's (descriptive).
    counts: dict[str, Any] = {}
    for s in (*settings, "lichess"):
        tally = {k: 0 for k in LABELS}
        for g in games:
            for x in labs[s][g["id"]]:
                tally[x] += 1
        counts[s] = {k: v / (2 * len(games)) for k, v in tally.items() if k != "none"}

    # By rating band (exploratory): the primary contrast within each band.
    by_band: dict[str, Any] = {}
    for name, _, _ in BANDS:
        sub = [g for g in games if band_of(g["mean_elo"]) == name]
        if len(sub) >= 2:
            e2, m2 = contrast_totals(sub, labs, ref, PRODUCTION, "n1m")
            by_band[name] = {"games": len(sub), "n300k-n1m": ratio(e2, m2, 0.95)}
            down = [g for g in sub if g["id"] in down_ids]
            if len(down) >= 2:
                e, m = contrast_totals(down, labs, ref, "n100k", PRODUCTION)
                by_band[name]["n100k-n300k"] = {**ratio(e, m, 0.95), "games": len(down)}

    stage_rechecks = {}
    if rechecks:
        for s in sorted(found):
            stage_rechecks[s] = recheck(s, found[s], budget)
    for s in sorted(found):
        log(f"{s}: {len(found[s])} positions, {cpu_hours(found[s]):.3f} CPU-hours searched")

    return {
        **({"_found_fens": {s: sorted(v) for s, v in found.items()}} if pilot else {}),
        "games": len(games),
        "moves": int(sum(g["plies"] for g in games)),
        "positions": len(all_fens),
        "settings_run": sorted(found),
        "reference_positions": len(found["ref"]),
        # The reference positions of the deciding settings' disagreements and the concordant
        # sample, before any conditional branch (the power analysis's cost model).
        "reference_positions_deciding": deciding_positions,
        "reference_positions_step_up": len({f for gid, p in disc_in(games, ("n300k", "n1m"))
                                            for f in move_fens(by_id[gid]["pos"], p)}),
        "step_down_games": sorted(down_ids),
        "discordant_moves": len(discordant),
        "concordant_sampled": len(conc_sample),
        "concordant_moves": n_conc,
        "primary": {"n100k-n300k": c100, "n300k-n1m": g1m,
                    **({"n50k-n300k": c50} if c50 else {}),
                    **({"n300k-n1m-deep": deep} if deep else {})},
        "decision": decision,
        "totals": {k: {"e": [float(x) for x in v[0]], "m": [float(x) for x in v[1]]}
                   for k, v in totals.items()},
        "absolute_disagreement": absolute,
        "best_move_agreement": best_move,
        "win_percent_error": win_error,
        "with_production": with_production,
        "accuracy": accuracy,
        "labels_per_player_game": counts,
        "by_band": by_band,
        "nodes_per_game": {s: SETTINGS[s]["nodes"] * len(all_fens) / len(games)
                           for s in settings},
        "rechecks": stage_rechecks,
    }


def sample_games(name: str, pop_pos: tuple[list[dict[str, Any]], dict[str, Any]] | None = None):
    pop, pos = pop_pos or population()
    by_id = {g["id"]: g for g in pop}
    ids = read_ids(name)
    missing = [i for i in ids if i not in by_id]
    if missing:
        raise SystemExit(f"{name}: {len(missing)} games are not in the population")
    games = [by_id[i] for i in ids]
    for g in games:
        g["pos"] = pos[g["id"]]
    return games


def cmd_pilot(out: Path) -> None:
    games = sample_games("sample-pilot.csv")
    result = evaluate_stage(games, None, CONCORDANT_PILOT, "pilot", pilot=True, rechecks=False)
    result["algorithm_id"] = rematch_lab.algorithm()["id"]
    # The stage's own searches only (the cache can hold others).
    own = result.pop("_found_fens")
    result["cpu_hours"] = {s: cpu_hours({f: r for f, r in load_cache(s).items() if f in set(fens)})
                           for s, fens in own.items()}
    result["searched"] = {s: len(fens) for s, fens in own.items()}
    out.mkdir(parents=True, exist_ok=True)
    (out / "pilot.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n",
                                    encoding="utf-8")


# ---- the power analysis ----------------------------------------------------------------------


def simulate(e100, e1m, m, n: int, target100: float, target1m: float, k: float,
             rng: np.random.Generator, sims: int) -> dict[str, float]:
    """Section 6: the section 9 rule on n games drawn with replacement from the pilot's, each
    game's two contrast totals shifted so the population contrasts are the targets and their
    spread around the targets scaled by k (the variance inflation). The step down is read on 40
    of the n, the step up on all n and its deep check on another 40, which the simulation takes
    to agree with the 30-million-node reference (the bias the check guards against is not
    modelled, so the step up's power here is an upper bound)."""
    r100 = e100.sum() / m.sum()
    r1m = e1m.sum() / m.sum()
    a = target100 * m + k * ((e100 - r100 * m))
    b = target1m * m + k * ((e1m - r1m * m))
    out = defaultdict(int)
    for _ in range(sims):
        idx = rng.integers(0, len(m), size=n)
        down = idx[rng.choice(n, size=min(STEP_DOWN_GAMES, n), replace=False)]
        deep = idx[rng.choice(n, size=min(DEEP_GAMES, n), replace=False)]
        c100 = ratio(a[down], m[down], LEVEL)
        g1m = ratio(b[idx], m[idx], LEVEL)
        gdeep = ratio(b[deep], m[deep], LEVEL)
        # 50,000 does not change which of the four outcomes is reached.
        d = decide(c100, None, g1m, gdeep)
        out[d["outcome"]] += 1
    return {kk: v / sims for kk, v in out.items()}


def cmd_power(pilot_path: Path) -> None:
    pilot = json.loads(pilot_path.read_text(encoding="utf-8"))
    t100 = pilot["totals"]["n100k-n300k"]
    t1m = pilot["totals"]["n300k-n1m"]
    e100, e1m, m = (np.array(t100["e"]), np.array(t1m["e"]), np.array(t100["m"]))
    n_pilot = len(m)
    # The variance inflation: the 80% upper bound of a variance estimated from n_pilot games.
    k_upper = math.sqrt((n_pilot - 1) / chi2.ppf(0.20, n_pilot - 1))
    scenarios = {
        "100k is enough": (DELTA / 2, DELTA / 2),
        "300k falls short": (DELTA / 2, 1.5 * DELTA),
        "300k is right": (1.5 * DELTA, DELTA / 2),
        "boundary 100k": (DELTA, 0.0),
        "boundary 1M": (0.0, DELTA),
    }
    table = []
    for k_name, k in (("pilot", 1.0), ("upper80", k_upper)):
        for n in N_GRID:
            row = {"variance": k_name, "k": k, "n": n}
            for sname, (x, y) in scenarios.items():
                rng = np.random.default_rng(seed(EXP, f"power/simulation/{k_name}/{n}/{sname}"))
                row[sname] = simulate(e100, e1m, m, n, x, y, k, rng, POWER_SIMS)
            table.append(row)
    rate = {s: pilot["cpu_hours"][s] / pilot["searched"][s] for s in pilot["cpu_hours"]}
    per_game_positions = pilot["positions"] / pilot["games"]
    # The step up's disagreements on every game; the step down's extra ones on 40; the concordant
    # sample fixed (two positions a move at most).
    up = pilot["reference_positions_step_up"] / pilot["games"]
    down_extra = (pilot["reference_positions_deciding"] - 2 * pilot["concordant_sampled"]
                  - pilot["reference_positions_step_up"]) / pilot["games"]
    def cost(n: int) -> float:
        return (n * (per_game_positions * sum(rate[s] for s in CANDIDATES) + up * rate["ref"])
                + STEP_DOWN_GAMES * down_extra * rate["ref"]
                + 2 * CONCORDANT_CONFIRMATORY * rate["ref"])
    # The step up's check, if reached: 40 games' step-up positions at the deep setting.
    branch = DEEP_GAMES * up * rate["deep"]
    def ok(row: dict[str, Any]) -> bool:
        return all(row[s].get(s, 0) >= TARGET_POWER
                   for s in ("100k is enough", "300k falls short", "300k is right"))
    powered = next((r["n"] for r in table if r["variance"] == "upper80" and ok(r)), None)
    affordable = [n for n in N_GRID if cost(n) <= CEILING_CPU_HOURS]
    if powered is not None and cost(powered) <= CEILING_CPU_HOURS:
        chosen = powered
    else:
        chosen = max(affordable) if affordable else None
    result = {
        "pilot_games": n_pilot,
        "k_upper80": k_upper,
        "pilot_estimates": {"n100k-n300k": float(e100.sum() / m.sum()),
                            "n300k-n1m": float(e1m.sum() / m.sum())},
        "table": table,
        "powered_n": powered,
        "chosen_n": chosen,
        "cpu_hours_per_position": rate,
        "positions_per_game": per_game_positions,
        "reference_positions_per_game_step_up": up,
        "reference_positions_per_game_step_down_extra": down_extra,
        "planned_cpu_hours": {n: cost(n) for n in N_GRID},
        "branch_cpu_hours": branch,
        "ceiling_cpu_hours": CEILING_CPU_HOURS,
    }
    (HERE / "power.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n",
                                     encoding="utf-8")
    figure_power(result, HERE / "figures" / "power.png")
    log(f"chosen n: {chosen}; planned CPU-hours {cost(chosen) if chosen else 'n/a'}, branch {branch:.2f}")


def figure_power(result: dict[str, Any], path: Path) -> None:
    import matplotlib.pyplot as plt

    path.parent.mkdir(parents=True, exist_ok=True)
    fig, ax = plt.subplots(figsize=(7, 4))
    for k_name, style in (("pilot", "--"), ("upper80", "-")):
        rows = [r for r in result["table"] if r["variance"] == k_name]
        for sname in ("100k is enough", "300k falls short", "300k is right"):
            ax.plot([r["n"] for r in rows], [r[sname].get(sname, 0) for r in rows], style,
                    label=f"{sname} ({k_name} variance)")
    ax.axhline(TARGET_POWER, color="grey", lw=0.8)
    ax.set_xlabel("games")
    ax.set_ylabel("probability of the right outcome")
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(path, dpi=120)
    plt.close(fig)


# ---- the registered run ----------------------------------------------------------------------


def figure_results(stage: dict[str, Any], path: Path) -> None:
    import matplotlib.pyplot as plt

    path.parent.mkdir(parents=True, exist_ok=True)
    settings = [s for s in ("n50k", "n100k", "n300k", "n1m", "n3m")
                if s in stage["absolute_disagreement"]]
    nodes = [SETTINGS[s]["nodes"] for s in settings]
    est = [stage["absolute_disagreement"][s]["estimate"] for s in settings]
    lo = [stage["absolute_disagreement"][s]["ci95"][0] for s in settings]
    hi = [stage["absolute_disagreement"][s]["ci95"][1] for s in settings]
    fig, ax = plt.subplots(figsize=(6, 4))
    ax.errorbar(nodes, est, yerr=[np.subtract(est, lo), np.subtract(hi, est)], fmt="o-")
    ax.set_xscale("log")
    ax.set_xlabel("nodes per position")
    ax.set_ylabel("share of moves whose label differs from the reference")
    fig.tight_layout()
    fig.savefig(path, dpi=120)
    plt.close(fig)


def cmd_run(out: Path, algorithm: str | None) -> None:
    manifest = rematch_lab.algorithm()
    algorithm_id = manifest["id"]
    if algorithm is not None and algorithm_id != algorithm:
        raise SystemExit(f"the cli is {algorithm_id}, not the requested {algorithm}")
    if not BUDGET:
        raise SystemExit("the budget of section 0 is not registered")
    budget = Budget(BUDGET, CAP)
    pop_pos = population()
    games = sample_games("sample.csv", pop_pos)
    pilot = set(read_ids("sample-pilot.csv"))
    players = lambda gs: {p for g in gs for p in (g["headers"]["White"].lower(),  # noqa: E731
                                                   g["headers"]["Black"].lower())}
    pilot_games = [g for g in pop_pos[0] if g["id"] in pilot]
    if pilot & {g["id"] for g in games} or players(pilot_games) & players(games):
        raise SystemExit("the confirmatory sample shares a game or a player with the pilot")
    stage = evaluate_stage(games, budget, CONCORDANT_CONFIRMATORY, "confirmatory",
                           pilot=False, rechecks=True)
    results = {
        "experiment": EXP,
        "algorithm_id": algorithm_id,
        "seed_namespace": EXP,
        # The node count this version's analysis uses (the arena compares it with the decision).
        "production_nodes": manifest["components"]["facts"]["engine_nodes"],
        "delta": DELTA,
        "level": LEVEL,
        "settings": SETTINGS,
        "population": len(pop_pos[0]),
        **stage,
        "compute": budget.record(),
    }
    write_results(out, results)
    figure_results(stage, out / "figures" / "disagreement.png")


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] in ("sample", "pilot", "power"):
        p = argparse.ArgumentParser(prog="analysis.py")
        p.add_argument("cmd")
        p.add_argument("--stage", choices=("pilot", "confirmatory"))
        p.add_argument("--n", type=int)
        p.add_argument("--out", type=Path)
        p.add_argument("--pilot", type=Path)
        a = p.parse_args(argv)
        if a.cmd == "sample":
            cmd_sample(a.stage, a.n)
        elif a.cmd == "pilot":
            cmd_pilot(a.out)
        else:
            cmd_power(a.pilot)
        return 0
    p = argparse.ArgumentParser(prog="analysis.py")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--algorithm", help="the algorithm_id the run must be on (the arena's)")
    a = p.parse_args(argv)
    cmd_run(a.out, a.algorithm)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
