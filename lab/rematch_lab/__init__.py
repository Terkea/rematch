"""Rematch's algorithm lab (ADR 0106): Python that drives the Rust algorithm, never a copy of it.

    import rematch_lab as lab
    facts = lab.analyse(players, nodes=12_000)   # games -> facts (database and Stockfish)
    profiles = lab.profile(facts)                # facts -> rates with 80% intervals
    ranking = lab.rank(profiles)                 # rates -> ranked weaknesses, focuses, drills
    run = lab.bench("smoke", out_dir="out")      # the Tier 1 benchmark (ADR 0100)

Every response carries `schema_version` and the `algorithm_id` that produced it.
"""

from . import models
from .bridge import SCHEMA_VERSION, LabError, analyse, bench, call_raw, check, profile, rank

__all__ = [
    "SCHEMA_VERSION",
    "LabError",
    "analyse",
    "bench",
    "call_raw",
    "check",
    "models",
    "profile",
    "rank",
]
