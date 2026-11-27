"""EXP-0029's registered analysis on a synthetic cohort with a known truth, and its rule on
hand-built inputs.

On the synthetic cohort: an injected effect is recovered as outcome 1 on its own key; a null
with the control linked reads outcome 2 at the fixed seed; without the control link it reads
outcome 3; two runs are byte-identical; the budget cap stops the run; EXP-0009's hash range is
excluded; and the sizing step reads the players files only. Holm caps the chance that any null
key reads "precedes climbing" at 5%, not at zero: a multi-seed check holds the false-positive
count within a binomial bound."""

from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path

import pytest
from scipy.stats import binom

import rematch_lab
from rematch_lab.experiments import BudgetExceeded

sys.path.insert(0, str(Path(__file__).resolve().parent))
import exp0029_synthetic as S  # noqa: E402

EXP_DIR = Path(__file__).resolve().parent.parent / "experiments" / "EXP-0029"
spec = importlib.util.spec_from_file_location("exp0029_analysis", EXP_DIR / "analysis.py")
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
CAP = (144, 120)
PLAYERS = 1000
IN_EXP0009_RANGE = PLAYERS // 10  # the generator puts every tenth player in [0, 0.3)


def cohort(tmp: Path, monkeypatch: pytest.MonkeyPatch, **kw) -> None:
    truth = S.build(tmp, **kw)
    monkeypatch.setenv("REMATCH_DATA_DIR", str(tmp))
    monkeypatch.setattr(rematch_lab, "observe", S.fake_observe(truth))
    monkeypatch.setattr(rematch_lab, "evaluate", S.fake_evaluate)
    monkeypatch.setattr(rematch_lab, "algorithm", lambda *_: {"id": "algo-synthetic"})


def run(tmp: Path, name: str = "out") -> dict:
    A.run(tmp / name, 0.6, CAP)
    return json.loads((tmp / name / "results.json").read_text(encoding="utf-8"))


# ---- the synthetic cohort ----------------------------------------------------------------------


def test_an_injected_effect_is_recovered_on_its_own_key(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=15.0, beta_control=15.0)
    r = run(tmp_path)
    d = r["decision"]
    assert d["outcome"] == 1
    assert d["climbing"] == ["W7.traps"]
    (c,) = d["candidates"]
    assert c["key"] == "W7.traps" and c["p"] <= c["p_holm"] < A.ALPHA
    assert d["follow_up"]
    assert d["positive_control_holds"] is True
    assert r["f1level"]["W7.traps"]["estimate"] is not None
    rel = r["primary"]["W7.traps"]["reliability"]
    assert 0.01 <= rel["lower"] <= rel["point"] <= 1
    # The opponent gap varies, so the families that use it are fitted on a full-rank design.
    for fam in ("primary", "adjusted", "contemporaneous", "level"):
        assert r[fam]["W7.traps"]["rank_deficient"] is False


def test_a_null_with_the_control_linked_reads_outcome_2(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=0.0, beta_control=15.0)
    r = run(tmp_path)
    d = r["decision"]
    assert d["outcome"] == 2
    assert d["climbing"] == [] and d["candidates"] == []
    assert d["below_minimum"] == list(A.OUTCOME2_KEYS)
    assert r["primary"]["accuracy"]["holds"] is True


def test_without_a_control_link_the_result_is_need_more_data(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=0.0, beta_control=0.0)
    r = run(tmp_path)
    assert r["decision"]["positive_control_holds"] is False
    assert r["decision"]["outcome"] == 3


def test_two_runs_are_byte_identical_and_exclude_exp0009s_range(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch, beta_key=15.0)
    first = run(tmp_path, "a")
    run(tmp_path, "b")
    assert (tmp_path / "a" / "results.json").read_bytes() == \
        (tmp_path / "b" / "results.json").read_bytes()
    assert first["design"]["slice"] == [0.3, 0.9]
    # Every synthetic player passes the history rule, so exactly those in [0.3, 0.9) are left.
    assert first["flow"]["history"] == PLAYERS - IN_EXP0009_RANGE
    rows, stay_rows, _ = A.build_table(0.3, 0.9, None, *CAP)
    assert rows and all(0.3 <= A.E9.fraction(r["h"]) < 0.9 for r in rows + stay_rows)
    assert first["compute"]["actual"]["observe_games"] == first["games"]["games_read"] - \
        first["games"]["games_not_standard"]


def test_the_budget_cap_stops_the_run(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch)
    monkeypatch.setattr(A, "BUDGET", {"observe_games": 1_000, "engine_nodes": 1_000_000})
    with pytest.raises(BudgetExceeded):
        A.run(tmp_path / "out", 0.6, CAP)
    assert not (tmp_path / "out" / "results.json").exists()


def test_the_run_refuses_a_cli_other_than_the_pinned_one(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch)
    with pytest.raises(SystemExit, match="not the requested"):
        A.run(tmp_path / "out", 0.6, CAP, "algo-6.0.0+f40a2b27677a")


def test_the_sizing_step_reads_the_players_files_only(tmp_path, monkeypatch):
    cohort(tmp_path, monkeypatch)

    def no_games(*_, **__):
        raise AssertionError("the sizing step must not observe a game")

    monkeypatch.setattr(rematch_lab, "observe", no_games)
    for m in (*S.F1, *S.F2):
        (tmp_path / "exp29-cohort" / m / "games.jsonl").unlink()
    m = A.sizing_measures(7.0)
    assert set(m["caps"]) == {"144/120", "96/80", "72/60"}
    assert m["counts"]["with_outcome"] > 0
    assert m["density"] < m["density_observed"] * A.POWER.PLAYERS_OBSERVED_0009 / A.WITH_OUTCOME_0009
    assert m["april_share"]["0.98"] <= 0
    results0009 = json.loads(A.POWER.EXP0009.read_text(encoding="utf-8"))
    d = A.POWER.decide_size(results0009, {k: v for k, v in m.items() if k != "counts"})
    assert "run" in d


def test_null_keys_read_climbing_within_holms_family_wise_bound(tmp_path, monkeypatch):
    """20 null cohorts of 400 players: the number in which any key reads "precedes climbing"
    must not exceed what a 5% family-wise rate allows (binomial, one-sided 0.001)."""
    seeds = range(101, 121)
    bound = int(binom.ppf(0.999, len(seeds), A.ALPHA))
    hits = 0
    for seed in seeds:
        root = tmp_path / f"s{seed}"
        cohort(root, monkeypatch, players=400, seed=seed, beta_key=0.0, beta_control=15.0)
        rows, _, _ = A.build_table(0.3, 0.9, None, *CAP)
        fam = A.family(rows, "primary")
        hits += any(fam[q]["reads"] == "precedes climbing" for q in A.KEYS)
    assert hits <= bound


# ---- the rule on hand-built inputs -------------------------------------------------------------


def fits(ps: dict[str, float], estimates: dict[str, float] | None = None,
         below: tuple[str, ...] = ()) -> dict:
    """Fits as `fit` returns them: a key in `below` has a narrow interval inside its minimum."""
    out = {}
    for q in A.QUANTITIES:
        p = ps.get(q, 0.5)
        if p is None:
            out[q] = {"n": 0, "estimate": None}
            continue
        est = (estimates or {}).get(q, 1.0)
        half = 0.5 if q in below else 20.0
        out[q] = {"n": 1000, "estimate": est, "se": 1.0, "p": p, "minimum_observed": 5.0,
                  f"ci{A.SIMULTANEOUS * 100:g}": [est - half, est + half]}
    return out


def test_holm_rejects_a_small_p_among_eight_and_not_a_borderline_one():
    f = A.read_family(fits({"W5": 0.002}))
    assert f["W5"]["reads"] == "precedes climbing"
    assert f["W5"]["p_holm"] == pytest.approx(0.016)
    f = A.read_family(fits({"W5": 0.01}))
    assert f["W5"]["reads"] != "precedes climbing"  # 0.01 > 0.05 / 8


def test_a_key_that_cannot_be_estimated_still_counts_in_holms_eight():
    f = A.read_family(fits({"W5": 0.0065, "W9b": None}))
    assert f["W9b"]["reads"] == "not estimable"
    assert f["W5"]["p_holm"] == pytest.approx(0.052)
    assert f["W5"]["reads"] != "precedes climbing"


def test_decide_follows_section_9():
    five = A.OUTCOME2_KEYS
    control = {"accuracy": 1e-6}
    primary = A.read_family(fits(control, below=five))
    assert A.decide(primary, primary)["outcome"] == 2
    # One of the five undetermined: need more data.
    primary = A.read_family(fits(control, below=five[1:]))
    assert A.decide(primary, primary)["outcome"] == 3
    # W4.missed precedes falling, the five below, the control holds: still outcome 2.
    primary = A.read_family(fits({**control, "W4.missed": 1e-6}, {"W4.missed": -5.0}, five))
    assert primary["W4.missed"]["reads"] == "precedes falling"
    assert A.decide(primary, primary)["outcome"] == 2
    # A key that climbs only in the F1-level model is reported, never a candidate.
    f1level = A.read_family(fits({**control, "W6": 1e-6}, below=five))
    d = A.decide(A.read_family(fits(control, below=five)), f1level)
    assert d["outcome"] == 2 and d["f1level_only_climbing"] == ["W6"] and d["candidates"] == []
    # No control: need more data, whatever the keys read.
    primary = A.read_family(fits({}, below=five))
    assert primary["accuracy"]["holds"] is False
    assert A.decide(primary, primary)["outcome"] == 3
