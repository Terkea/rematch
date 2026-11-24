"""Rematch's algorithm lab (ADR 0106): Python that drives the Rust algorithm, never a copy of it.

    import rematch_lab as lab
    facts = lab.analyse(players, nodes=12_000)   # games -> facts (database and Stockfish)
    profiles = lab.profile(facts)                # facts -> rates with 80% intervals
    ranking = lab.rank(profiles)                 # rates -> ranked weaknesses, focuses, drills
    tagged = lab.tag_motifs(lines)               # lines -> motif tags (EXP-0007)
    asked = lab.positions(games)                 # games -> the positions the engine is asked for
    found = lab.evaluate(fens, nodes=300_000)    # positions -> Stockfish searches (EXP-0008)
    judged = lab.judge(request)                  # games and searches -> judgements, accuracy
    run = lab.bench("smoke", out_dir="out")      # the Tier 1 benchmark (ADR 0100)
    manifest = lab.algorithm()                   # this build's manifest (ADR 0105)

Every response carries `schema_version` and the `algorithm_id` that produced it.
"""

from . import models
from .bridge import (
    SCHEMA_VERSION,
    LabError,
    algorithm,
    algorithm_diff,
    analyse,
    bench,
    call_raw,
    check,
    evaluate,
    judge,
    positions,
    profile,
    rank,
    tag_motifs,
)

__all__ = [
    "SCHEMA_VERSION",
    "LabError",
    "algorithm",
    "algorithm_diff",
    "analyse",
    "bench",
    "call_raw",
    "check",
    "evaluate",
    "judge",
    "models",
    "positions",
    "profile",
    "rank",
    "tag_motifs",
]
