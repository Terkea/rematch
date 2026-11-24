"""EXP-0007: the motif tagger against the Lichess puzzle themes (preregistration.md).

The tags come from the production Rust code: every puzzle goes through `cli lab tag-motifs`, which
runs `derive_move_features` with the puzzle's solution as the mover's best line, as the enrichment
stores it (`move_features.best_line`). Python here samples, counts and decides; it computes no tag.

Stages, in the order they ran (each stage's output is committed before the next one runs):

    python analysis.py sample --stage pilot          # strata.csv, sample-pilot.csv
    python analysis.py pilot --out DIR               # DIR/pilot.json, DIR/hand-check.md
    python analysis.py power --pilot pilot.json      # power.json, figures/power.png
    python analysis.py sample --stage confirmatory --m M   # sample.csv
    python analysis.py --out DIR                     # the registered run: DIR/results.json

Only the last is the registered run; the lab check reruns it twice and compares results.json.
The snapshot is an external input (inputs.sha256), read from the data directory.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np
import polars as pl
from scipy.stats import norm

import rematch_lab
from rematch_lab.experiments import Budget, external_input, seed, write_results
from rematch_lab.models import MotifLine

EXP = "EXP-0007"
HERE = Path(__file__).resolve().parent
SNAPSHOT = "lichess-puzzles/lichess_db_puzzle.csv"
SNAPSHOT_SHA256 = "8ce98180bcb7cb60f3f51f12afdbe6a4fd294d8aaf4f1410e8c25e85ac78fdc9"

# Section 7: the population.
MIN_PLAYS = 1000
MIN_POPULARITY = 50
BANDS = (("A1", 0, 1000), ("A2", 1000, 1400), ("A3", 1400, 1700), ("A4", 1700, 2000),
         ("S", 2000, 2400))
AUDIENCE = ("A1", "A2", "A3", "A4")

# Section 4: our tag -> the Lichess themes that count as its truth.
TRUTH: dict[str, tuple[str, ...]] = {
    "hangingPiece": ("hangingPiece",),
    "fork": ("fork",),
    "pin": ("pin",),
    "skewer": ("skewer",),
    "discoveredAttack": ("discoveredAttack", "discoveredCheck", "doubleCheck"),
    "deflection": ("deflection",),
    "sacrifice": ("sacrifice",),
    "promotion": ("promotion", "underPromotion"),
    "mate": ("mate",),
    "mateIn1": ("mateIn1",),
    "mateIn2": ("mateIn2",),
    "mateIn3": ("mateIn3",),
    "mateIn4Plus": ("mateIn4", "mateIn5"),
    "backRankMate": ("backRankMate",),
    "smotheredMate": ("smotheredMate",),
}
SCORED = tuple(TRUTH)
UNSCORED_TAGS = ("matingNet",)
# Lichess themes that describe the length, phase, evaluation or source, not a motif.
NOT_MOTIFS = frozenset({
    "advantage", "crushing", "equality", "mate", "oneMove", "short", "long", "veryLong",
    "opening", "middlegame", "endgame", "rookEndgame", "bishopEndgame", "pawnEndgame",
    "knightEndgame", "queenEndgame", "queenRookEndgame", "master", "masterVsMaster", "superGM",
    "mateIn1", "mateIn2", "mateIn3", "mateIn4", "mateIn5",
})

# Section 9: the floors and the family.
PRECISION_FLOOR = 0.80
RECALL_FLOOR = 0.50
FAMILY = 2 * len(SCORED)  # precision and recall of each scored tag
Z_DECISION = float(norm.ppf(1 - 0.05 / (2 * FAMILY)))
Z80 = float(norm.ppf(0.90))
Z95 = float(norm.ppf(0.975))

# Section 6: the power analysis.
PILOT_M = 25
# The confirmatory allocation: n = min(N, max(m, ceil(f * N))) in a cell of N puzzles, a floor
# of m for the rare cells and a fraction f of the large ones, which bounds the weights.
M_GRID = (50, 100, 150, 200, 300)
F_GRID = (0.0, 0.005, 0.01, 0.02, 0.03, 0.05)
POWER_SIMS = 2000
TARGET_POWER = 0.80
SCENARIO_PRECISION = PRECISION_FLOOR + 0.10
SCENARIO_PRECISION_LOW = PRECISION_FLOOR - 0.10
SCENARIO_RECALL = RECALL_FLOOR + 0.15
BATCH = 2000

# Section 0: the compute budget of the registered run, the size of sample.csv (section 6).
BUDGET = {"lines": 23817, "cli_calls": 12}
CAP = 1.5


# ---- the population and the sample -----------------------------------------------------------


def band_of(rating: int) -> str | None:
    for name, lo, hi in BANDS:
        if lo <= rating < hi:
            return name
    return None


def labels_of(themes: list[str]) -> list[str]:
    have = set(themes)
    return [t for t in SCORED if have.intersection(TRUTH[t])]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_population() -> pl.DataFrame:
    """The filtered population: every puzzle with at least MIN_PLAYS plays, a popularity of at
    least MIN_POPULARITY and a rating in a band, sorted by id."""
    path = external_input(SNAPSHOT)
    if sha256_file(path) != SNAPSHOT_SHA256:
        raise SystemExit(f"{path} is not the registered snapshot")
    df = (
        pl.scan_csv(path, infer_schema_length=0)
        .select("PuzzleId", "FEN", "Moves", "Rating", "Popularity", "NbPlays", "Themes")
        .with_columns(pl.col("Rating", "Popularity", "NbPlays").cast(pl.Int64))
        .filter((pl.col("NbPlays") >= MIN_PLAYS) & (pl.col("Popularity") >= MIN_POPULARITY))
        .filter(pl.col("Rating") < BANDS[-1][2])
        .collect()
        .sort("PuzzleId")
    )
    themes = [t.split(" ") for t in df["Themes"].to_list()]
    labels = [labels_of(t) for t in themes]
    # The stratum: the puzzle's rarest scored label in the population, or "none".
    freq: dict[str, int] = defaultdict(int)
    for ls in labels:
        for t in ls:
            freq[t] += 1
    order = {t: (freq.get(t, 0), t) for t in SCORED}
    strata = [min(ls, key=lambda t: order[t]) if ls else "none" for ls in labels]
    return df.with_columns(
        pl.Series("band", [band_of(r) for r in df["Rating"].to_list()], dtype=pl.Utf8),
        pl.Series("themes", themes, dtype=pl.List(pl.Utf8)),
        pl.Series("labels", labels, dtype=pl.List(pl.Utf8)),
        pl.Series("stratum", strata, dtype=pl.Utf8),
    )


def cells(df: pl.DataFrame) -> dict[tuple[str, str], list[str]]:
    out: dict[tuple[str, str], list[str]] = defaultdict(list)
    for pid, s, b in df.select("PuzzleId", "stratum", "band").iter_rows():
        out[(s, b)].append(pid)
    return dict(sorted(out.items()))


def allocation(n_pop: int, m: int, f: float) -> int:
    return min(n_pop, max(m, math.ceil(f * n_pop)))


def draw(by_cell: dict[tuple[str, str], list[str]], m: int, f: float, name: str,
         exclude: set[str]) -> dict[tuple[str, str], list[str]]:
    """allocation(N, m, f) puzzles per cell, from one seeded generator walking the cells in
    sorted order."""
    rng = np.random.default_rng(seed(EXP, name))
    out = {}
    for key, ids in by_cell.items():
        pool = [i for i in ids if i not in exclude]
        order = rng.permutation(len(pool))
        out[key] = sorted(pool[i] for i in order[: allocation(len(pool), m, f)])
    return out


def read_sample(name: str) -> dict[str, tuple[str, str]]:
    with (HERE / name).open(encoding="utf-8", newline="") as f:
        return {r["puzzle_id"]: (r["stratum"], r["band"]) for r in csv.DictReader(f)}


def write_sample(name: str, sample: dict[tuple[str, str], list[str]]) -> None:
    with (HERE / name).open("w", encoding="utf-8", newline="\n") as f:
        f.write("puzzle_id,stratum,band\n")
        for (s, b), ids in sample.items():
            for i in ids:
                f.write(f"{i},{s},{b}\n")


def read_strata() -> dict[tuple[str, str], dict[str, int]]:
    with (HERE / "strata.csv").open(encoding="utf-8", newline="") as f:
        return {(r["stratum"], r["band"]): {k: int(v) for k, v in r.items()
                                            if k not in ("stratum", "band")}
                for r in csv.DictReader(f)}


def cmd_sample(stage: str, m: int | None, f: float | None) -> None:
    df = load_population()
    by_cell = cells(df)
    if stage == "pilot":
        pilot = draw(by_cell, PILOT_M, 0.0, "sample/pilot", set())
        write_sample("sample-pilot.csv", pilot)
        label_counts: dict[tuple[str, str], dict[str, int]] = defaultdict(lambda: defaultdict(int))
        for s, b, labels in df.select("stratum", "band", "labels").iter_rows():
            for t in labels:
                label_counts[(s, b)][t] += 1
        with (HERE / "strata.csv").open("w", encoding="utf-8", newline="\n") as f:
            f.write("stratum,band,population,pilot," + ",".join(f"label_{t}" for t in SCORED) + "\n")
            for key, ids in by_cell.items():
                row = [key[0], key[1], str(len(ids)), str(len(pilot[key]))]
                row += [str(label_counts[key].get(t, 0)) for t in SCORED]
                f.write(",".join(row) + "\n")
        print(f"population {df.height}, {len(by_cell)} cells, pilot {sum(map(len, pilot.values()))}")
    else:
        assert m is not None and f is not None, "--m and --f set the confirmatory sample"
        pilot_ids = set(read_sample("sample-pilot.csv"))
        sample = draw(by_cell, m, f, "sample/confirmatory", pilot_ids)
        write_sample("sample.csv", sample)
        print(f"confirmatory sample {sum(map(len, sample.values()))} at m = {m}, f = {f}")


# ---- tagging through the bridge -------------------------------------------------------------


def tag(df: pl.DataFrame, budget: Budget | None) -> dict[str, dict]:
    """Our tags for each puzzle: the position after the puzzle's first move (the opponent's), the
    rest of the solution as the mover's best line."""
    lines = []
    for pid, fen, moves in df.select("PuzzleId", "FEN", "Moves").iter_rows():
        mv = moves.split(" ")
        lines.append(MotifLine(id=pid, fen=fen, setup=mv[:1], pv=mv[1:]))
    out: dict[str, dict] = {}
    for i in range(0, len(lines), BATCH):
        chunk = lines[i : i + BATCH]
        if budget is not None:
            budget.spend("lines", len(chunk))
            budget.spend("cli_calls", 1)
        resp = rematch_lab.tag_motifs(chunk)
        for line in resp.lines:
            out[line.id] = {
                "algorithm_id": resp.algorithm_id,
                "fen": line.fen,
                "tags": None if line.best is None else list(line.best.tags),
            }
    return out


def records(df: pl.DataFrame, tagged: dict[str, dict], weights: dict[tuple[str, str], float]):
    """One record per sampled puzzle: its cell, weight, Lichess labels and themes, our tags."""
    recs, excluded = [], []
    for pid, s, b, labels, themes in df.select("PuzzleId", "stratum", "band", "labels",
                                               "themes").iter_rows():
        t = tagged[pid]
        if t["tags"] is None:
            excluded.append(pid)
            continue
        recs.append({"id": pid, "stratum": s, "band": b, "w": weights[(s, b)],
                     "labels": set(labels), "themes": set(themes), "tags": set(t["tags"])})
    return recs, excluded


# ---- estimates ------------------------------------------------------------------------------


def wilson(p: float, n: float, z: float) -> tuple[float, float]:
    if n <= 0:
        return (0.0, 1.0)
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return (max(0.0, c - h), min(1.0, c + h))


def weighted_share(ws: list[float], ys: list[bool]) -> dict:
    """A weighted share with Wilson intervals at Kish's effective sample size."""
    sw = sum(ws)
    if sw == 0:
        return {"estimate": None, "n": 0, "n_eff": 0.0, "ci80": None, "ci95": None,
                "ci_decision": None}
    p = sum(w for w, y in zip(ws, ys, strict=True) if y) / sw
    n_eff = sw * sw / sum(w * w for w in ws)
    r = lambda x: round(x, 4)  # noqa: E731
    ci = lambda z: [r(v) for v in wilson(p, n_eff, z)]  # noqa: E731
    return {"estimate": r(p), "n": len(ws), "n_eff": round(n_eff, 1), "ci80": ci(Z80),
            "ci95": ci(Z95), "ci_decision": ci(Z_DECISION)}


def precision_recall(recs: list[dict], t: str, bands: tuple[str, ...]) -> dict:
    dom = [r for r in recs if r["band"] in bands]
    tagged = [r for r in dom if t in r["tags"]]
    labelled = [r for r in dom if t in r["labels"]]
    return {
        "precision": weighted_share([r["w"] for r in tagged], [t in r["labels"] for r in tagged]),
        "recall": weighted_share([r["w"] for r in labelled], [t in r["tags"] for r in labelled]),
    }


def decide(pr: dict) -> str:
    p, r = pr["precision"]["ci_decision"], pr["recall"]["ci_decision"]
    if p is None or p[0] < PRECISION_FLOOR:
        return "hidden until fixed"
    if r is not None and r[0] >= RECALL_FLOOR:
        return "trusted"
    return "needs fixing"


def confusion(recs: list[dict]) -> dict:
    """For each of our tags, the weighted share of its puzzles carrying each Lichess motif theme;
    for each Lichess motif theme, the weighted share of its puzzles carrying each of our tags."""
    dom = [r for r in recs if r["band"] in AUDIENCE]
    themes = sorted({th for r in dom for th in r["themes"]} - NOT_MOTIFS)
    ours = list(SCORED) + list(UNSCORED_TAGS)
    by_tag, by_theme = {}, {}
    for t in ours:
        rows = [r for r in dom if t in r["tags"]]
        sw = sum(r["w"] for r in rows)
        if sw:
            shares = {th: round(sum(r["w"] for r in rows if th in r["themes"]) / sw, 4)
                      for th in themes}
            by_tag[t] = {"n": len(rows), "themes": {k: v for k, v in shares.items() if v >= 0.01}}
    for th in themes:
        rows = [r for r in dom if th in r["themes"]]
        sw = sum(r["w"] for r in rows)
        if sw:
            shares = {t: round(sum(r["w"] for r in rows if t in r["tags"]) / sw, 4) for t in ours}
            by_theme[th] = {"n": len(rows), "tags": {k: v for k, v in shares.items() if v >= 0.01}}
    return {"by_tag": by_tag, "by_theme": by_theme}


# ---- the pilot and the power analysis --------------------------------------------------------


def sample_frame(name: str) -> tuple[pl.DataFrame, dict[str, tuple[str, str]]]:
    sample = read_sample(name)
    df = load_population().filter(pl.col("PuzzleId").is_in(list(sample)))
    if df.height != len(sample):
        raise SystemExit(f"{name}: {len(sample) - df.height} puzzles are not in the population")
    for pid, s, b in df.select("PuzzleId", "stratum", "band").iter_rows():
        if sample[pid] != (s, b):
            raise SystemExit(f"{name}: {pid} is in another cell than recorded")
    return df, sample


def cmd_pilot(out: Path) -> None:
    df, _ = sample_frame("sample-pilot.csv")
    strata = read_strata()
    weights = {k: v["population"] / v["pilot"] for k, v in strata.items() if v["pilot"]}
    tagged = tag(df, None)
    recs, excluded = records(df, tagged, weights)
    # Per stratum (pooled over the bands) and tag: label+/label- puzzles and how many we tagged.
    counts: dict[str, dict[str, list[int]]] = {}
    for t in SCORED:
        c: dict[str, list[int]] = defaultdict(lambda: [0, 0, 0, 0])
        for r in recs:
            k = c[r["stratum"]]
            if t in r["labels"]:
                k[0] += 1
                k[1] += t in r["tags"]
            else:
                k[2] += 1
                k[3] += t in r["tags"]
        counts[t] = dict(sorted(c.items()))
    estimates = {t: precision_recall(recs, t, AUDIENCE) for t in SCORED}
    algorithm_id = next(iter(tagged.values()))["algorithm_id"]
    out.mkdir(parents=True, exist_ok=True)
    (out / "pilot.json").write_text(json.dumps({
        "algorithm_id": algorithm_id, "puzzles": len(recs), "excluded": excluded,
        "counts": counts, "estimates": estimates,
        "note": "counts[tag][stratum] = [labelled, labelled and tagged, unlabelled, unlabelled "
                "and tagged]; development data, never confirmatory",
    }, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    hand_check(df, recs, tagged, out / "hand-check.md")
    print(f"pilot: {len(recs)} puzzles tagged, {len(excluded)} excluded")


BOARD_GLYPHS = "KQRBNPkqrbnp"


def board(fen: str) -> str:
    rows = []
    for i, rank in enumerate(fen.split(" ")[0].split("/")):
        row = ""
        for ch in rank:
            row += "." * int(ch) if ch.isdigit() else ch
        rows.append(f"{8 - i} " + " ".join(row))
    rows.append("  a b c d e f g h")
    return "\n".join(rows)


def hand_check(df: pl.DataFrame, recs: list[dict], tagged: dict, path: Path) -> None:
    """The seeded disagreements of the pilot for the hand check (section 7): up to 4 tagged but
    unlabelled and 2 labelled but untagged puzzles per scored tag."""
    rng = np.random.default_rng(seed(EXP, "pilot/hand-check"))
    moves = dict(df.select("PuzzleId", "Moves").iter_rows())
    lines = ["# EXP-0007 pilot: disagreements for the hand check", ""]
    for t in SCORED:
        fp = sorted(r["id"] for r in recs if t in r["tags"] and t not in r["labels"])
        fn = sorted(r["id"] for r in recs if t in r["labels"] and t not in r["tags"])
        for kind, ids, k in (("tagged, not labelled", fp, 4), ("labelled, not tagged", fn, 2)):
            pick = [ids[i] for i in sorted(rng.permutation(len(ids))[:k])]
            for pid in pick:
                r = next(x for x in recs if x["id"] == pid)
                fen = tagged[pid]["fen"]
                lines += [f"## {t}: {kind}: {pid}", "",
                          f"- Lichess themes: {' '.join(sorted(r['themes']))}",
                          f"- Our tags: {' '.join(sorted(r['tags'])) or '(none)'}",
                          f"- Position (to move: {fen.split(' ')[1]}): `{fen}`",
                          f"- Solution: {' '.join(moves[pid].split(' ')[1:])}",
                          "- Verdict: ", "", "```", board(fen), "```", ""]
    path.write_text("\n".join(lines), encoding="utf-8")


def wilson_np(p: np.ndarray, n: np.ndarray, z: float) -> tuple[np.ndarray, np.ndarray]:
    """wilson() over arrays; n = 0 gives (0, 1)."""
    safe = np.where(n > 0, n, 1.0)
    d = 1 + z * z / safe
    c = (p + z * z / (2 * safe)) / d
    h = z * np.sqrt(np.maximum(p * (1 - p), 0) / safe + z * z / (4 * safe * safe)) / d
    lo = np.where(n > 0, np.maximum(0.0, c - h), 0.0)
    hi = np.where(n > 0, np.minimum(1.0, c + h), 1.0)
    return lo, hi


def smoothed(k: int, n: int) -> float:
    return (k + 0.5) / (n + 1)


def design_key(m: int, f: float) -> str:
    return f"m={m},f={f}"


def cmd_power(pilot_path: Path) -> None:
    pilot = json.loads(pilot_path.read_text(encoding="utf-8"))
    strata = read_strata()
    rng = np.random.default_rng(seed(EXP, "power/simulation"))
    cells_ = [k for k in strata if k[1] in AUDIENCE]
    pop = np.array([strata[k]["population"] - strata[k]["pilot"] for k in cells_], float)

    def size(d: tuple[int, float]) -> int:
        return int(sum(allocation(int(x), *d) for x in pop))

    designs = sorted(((m, f) for m in M_GRID for f in F_GRID), key=lambda d: (size(d), d))
    table: dict[str, dict[str, dict[str, float]]] = {}
    coverage: dict[str, dict[str, float]] = {}
    for t in SCORED:
        c = pilot["counts"][t]
        cc = [c.get(k[0], [0, 0, 0, 0]) for k in cells_]
        lam = np.array([strata[k][f"label_{t}"] / strata[k]["population"] for k in cells_])
        a = np.array([smoothed(x[1], x[0]) for x in cc])
        b = np.array([smoothed(x[3], x[2]) for x in cc])
        # The scenarios: false positives scaled so that precision is 0.90; true positives scaled
        # (capped at 1) so that recall is 0.65. The pilot's structure is otherwise kept.
        tp_mass, fp_mass = float((pop * lam * a).sum()), float((pop * (1 - lam) * b).sum())
        b_s = np.minimum(1, b * (tp_mass * (1 / SCENARIO_PRECISION - 1) / fp_mass))
        b_low = np.minimum(1, b * (tp_mass * (1 / SCENARIO_PRECISION_LOW - 1) / fp_mass))
        lo, hi = 0.0, 1e6
        for _ in range(200):
            mid = (lo + hi) / 2
            rec = float((pop * lam * np.minimum(1, a * mid)).sum() / (pop * lam).sum())
            lo, hi = (mid, hi) if rec < SCENARIO_RECALL else (lo, mid)
        a_r = np.minimum(1, a * lo)
        table[t] = {"precision": {}, "recall": {}, "shown_at_0_70": {}}
        coverage[t] = {}
        for d in designs:
            key = design_key(*d)
            n = np.array([allocation(int(x), *d) for x in pop])
            w = pop / np.maximum(n, 1)
            labelled = rng.binomial(n, lam, size=(POWER_SIMS, len(cells_)))
            tp = rng.binomial(labelled, a)
            fp = rng.binomial(n - labelled, b_s)
            sw = (w * (tp + fp)).sum(1)
            p_hat = np.where(sw > 0, (w * tp).sum(1) / np.where(sw > 0, sw, 1), 0.0)
            neff = np.where(sw > 0, sw**2 / np.maximum((w * w * (tp + fp)).sum(1), 1e-12), 0.0)
            lo_p, hi_p = wilson_np(p_hat, neff, Z_DECISION)
            table[t]["precision"][key] = round(float((lo_p >= PRECISION_FLOOR).mean()), 4)
            coverage[t][key] = round(
                float(((lo_p <= SCENARIO_PRECISION) & (SCENARIO_PRECISION <= hi_p)).mean()), 4
            )
            fp_low = rng.binomial(n - labelled, b_low)
            sw_low = (w * (tp + fp_low)).sum(1)
            p_low = np.where(sw_low > 0, (w * tp).sum(1) / np.where(sw_low > 0, sw_low, 1), 0.0)
            neff_low = np.where(
                sw_low > 0, sw_low**2 / np.maximum((w * w * (tp + fp_low)).sum(1), 1e-12), 0.0
            )
            lo_low, _ = wilson_np(p_low, neff_low, Z_DECISION)
            table[t]["shown_at_0_70"][key] = round(float((lo_low >= PRECISION_FLOOR).mean()), 4)
            tp_r = rng.binomial(labelled, a_r)
            sl = (w * labelled).sum(1)
            r_hat = np.where(sl > 0, (w * tp_r).sum(1) / np.where(sl > 0, sl, 1), 0.0)
            neff_r = np.where(sl > 0, sl**2 / np.maximum((w * w * labelled).sum(1), 1e-12), 0.0)
            lo_r, _ = wilson_np(r_hat, neff_r, Z_DECISION)
            table[t]["recall"][key] = round(float((lo_r >= RECALL_FLOOR).mean()), 4)
    worst = {
        design_key(*d): min(
            min(table[t]["precision"][design_key(*d)], table[t]["recall"][design_key(*d)])
            for t in SCORED
        )
        for d in designs
    }
    # The smallest design (in audience puzzles) whose worst tag reaches the target.
    chosen = next((d for d in designs if worst[design_key(*d)] >= TARGET_POWER), None)
    result = {
        "algorithm_id": pilot["algorithm_id"],
        "designs": [design_key(*d) for d in designs],
        "audience_puzzles": {design_key(*d): size(d) for d in designs},
        "power": table,
        "coverage_at_precision_0_90": coverage,
        "worst": worst,
        "chosen": None if chosen is None else {"m": chosen[0], "f": chosen[1]},
        "sims": POWER_SIMS,
        "z_decision": round(Z_DECISION, 4),
    }
    (HERE / "power.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n",
                                     encoding="utf-8")
    figure_power(result, HERE / "figures" / "power.png")
    print(json.dumps({"chosen": result["chosen"]}))
    for d in designs:
        print(design_key(*d), size(d), worst[design_key(*d)])


def figure_power(result: dict, path: Path) -> None:
    import matplotlib.pyplot as plt

    path.parent.mkdir(parents=True, exist_ok=True)
    fig, ax = plt.subplots(1, 2, figsize=(11, 4.5), sharey=True)
    xs = [result["audience_puzzles"][d] for d in result["designs"]]
    for i, metric in enumerate(("precision", "recall")):
        for t in SCORED:
            ys = [result["power"][t][metric][d] for d in result["designs"]]
            ax[i].plot(xs, ys, ".", ms=3, label=t)
        ax[i].axhline(TARGET_POWER, color="black", ls="--", lw=0.8)
        ax[i].set_xscale("log")
        ax[i].set_xlabel("audience puzzles in the sample (one point a design)")
        ax[i].set_title(f"{metric}: P(clears the floor) when true = "
                        f"{SCENARIO_PRECISION if metric == 'precision' else SCENARIO_RECALL:.2f}")
    ax[0].set_ylabel("simulated power")
    ax[1].legend(fontsize=6, ncol=2)
    fig.tight_layout()
    fig.savefig(path, dpi=110, metadata={"Software": None})
    plt.close(fig)


# ---- the registered run ---------------------------------------------------------------------


def figure_results(results: dict, path: Path) -> None:
    import matplotlib.pyplot as plt

    path.parent.mkdir(parents=True, exist_ok=True)
    fig, ax = plt.subplots(1, 2, figsize=(11, 5.5), sharey=True)
    ys = list(range(len(SCORED)))[::-1]
    for i, metric in enumerate(("precision", "recall")):
        for y, t in zip(ys, SCORED, strict=True):
            e = results["tags"][t][metric]
            if e["estimate"] is None:
                continue
            lo, hi = e["ci_decision"]
            ax[i].plot([lo, hi], [y, y], color="0.5", lw=2)
            ax[i].plot([e["estimate"]], [y], "o", color="black", ms=4)
        ax[i].axvline(PRECISION_FLOOR if metric == "precision" else RECALL_FLOOR, color="black",
                      ls="--", lw=0.8)
        ax[i].set_xlim(0, 1)
        ax[i].set_title(f"{metric} against the Lichess themes (Bonferroni intervals)")
    ax[0].set_yticks(ys, SCORED)
    fig.tight_layout()
    fig.savefig(path, dpi=110, metadata={"Software": None})
    plt.close(fig)


def cmd_run(out: Path) -> None:
    df, sample = sample_frame("sample.csv")
    strata = read_strata()
    n_cell: dict[tuple[str, str], int] = defaultdict(int)
    for key in sample.values():
        n_cell[key] += 1
    # The confirmatory sample is drawn from the population less the pilot.
    weights = {k: (strata[k]["population"] - strata[k]["pilot"]) / n for k, n in n_cell.items()}
    budget = Budget(BUDGET, CAP)
    tagged = tag(df, budget)
    ids = {v["algorithm_id"] for v in tagged.values()}
    if len(ids) != 1:
        raise SystemExit(f"the run spans algorithm versions: {sorted(ids)}")
    recs, excluded = records(df, tagged, weights)
    tags = {}
    for t in SCORED:
        pr = precision_recall(recs, t, AUDIENCE)
        tags[t] = {**pr, "decision": decide(pr), "truth": list(TRUTH[t]),
                   "by_band": {b: precision_recall(recs, t, (b,)) for b, _, _ in BANDS}}
    results = {
        "algorithm_id": ids.pop(),
        "snapshot_sha256": SNAPSHOT_SHA256,
        "sample": {"puzzles": len(sample), "tagged": len(recs), "excluded": excluded,
                   "cells": {f"{s}/{b}": n for (s, b), n in sorted(n_cell.items())}},
        "z_decision": round(Z_DECISION, 4),
        "floors": {"precision": PRECISION_FLOOR, "recall": RECALL_FLOOR},
        "tags": tags,
        "decisions": {d: [t for t in SCORED if tags[t]["decision"] == d]
                      for d in ("trusted", "needs fixing", "hidden until fixed")},
        "unscored": {t: {"tagged_share": round(
            sum(r["w"] for r in recs if r["band"] in AUDIENCE and t in r["tags"])
            / sum(r["w"] for r in recs if r["band"] in AUDIENCE), 4)} for t in UNSCORED_TAGS},
        "confusion": confusion(recs),
        "compute": budget.record(),
    }
    write_results(out, results)
    figure_results(results, out / "figures" / "precision-recall.png")


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if args and args[0] in ("sample", "pilot", "power"):
        p = argparse.ArgumentParser(prog="analysis.py")
        p.add_argument("cmd")
        p.add_argument("--stage", choices=("pilot", "confirmatory"))
        p.add_argument("--m", type=int)
        p.add_argument("--f", type=float)
        p.add_argument("--out", type=Path)
        p.add_argument("--pilot", type=Path)
        a = p.parse_args(args)
        if a.cmd == "sample":
            cmd_sample(a.stage, a.m, a.f)
        elif a.cmd == "pilot":
            cmd_pilot(a.out)
        else:
            cmd_power(a.pilot)
        return 0
    p = argparse.ArgumentParser(prog="analysis.py")
    p.add_argument("--out", type=Path, required=True)
    cmd_run(p.parse_args(args).out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
