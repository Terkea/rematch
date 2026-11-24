"""EXP-0019's registered analysis on synthetic answers, and the blinding of its audit page."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import sys
from pathlib import Path

import pytest

EXP_DIR = Path(__file__).resolve().parent.parent / "experiments" / "EXP-0019"


def load(name: str):
    sys.path.insert(0, str(EXP_DIR))
    spec = importlib.util.spec_from_file_location(f"exp0019_{name}", EXP_DIR / f"{name}.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


A = load("analysis")
PIS = A.exp7_precision(json.loads(A.EXP7_RESULTS.read_text(encoding="utf-8")))


def synthetic_key(per_stratum: int = 30) -> dict:
    """A key with `per_stratum` unlabelled and 3 labelled items a tag, 6 distractors, in
    blocks of 21 (the registered layout is not needed here, only the item order)."""
    items = []
    strata = [f"U:{t}" for t in A.TAGS for _ in range(per_stratum)]
    strata += [f"L:{t}" for t in A.TAGS for _ in range(3)]
    strata += ["D:miss"] * 3 + ["D:none"] * 3
    for n, s in enumerate(strata, start=1):
        items.append({"item": n, "block": (n - 1) // 21 + 1, "puzzle_id": f"p{n}", "strata": [s],
                      "tags": [s[2:]] if s[0] in "UL" else [],
                      "themes": [s[2:]] if s[0] == "L" else ["hangingPiece"] if s == "D:miss"
                      else []})
    return {"format": A.KEY_FORMAT, "algorithm_id": "algo-test", "page_id": "abc", "items": items}


def answers_for(key: dict, rule) -> dict:
    """answers.json with each question answered by rule(item, tag)."""
    out = {}
    for it in key["items"]:
        out[str(it["item"])] = {t: rule(it, t) for t in A.TAGS}
    return {"format": A.ANSWERS_FORMAT, "page_id": key["page_id"], "answers": out}


def yes_where_tagged(it, t):
    return "yes" if t in it["tags"] else "no"


def test_every_tagged_puzzle_judged_the_motif_clears_every_tag():
    key = synthetic_key()
    items = A.done_items(key, answers_for(key, yes_where_tagged))
    for t in A.TAGS:
        d = A.decide_tag(PIS[t], A.tally(items, f"U:{t}", t), A.tally(items, f"L:{t}", t))
        assert d["decision"] == "clears", (t, d)
        assert d["estimate"] == 1.0


def test_no_unlabelled_puzzle_judged_the_motif_fails_the_tags_far_from_the_floor():
    key = synthetic_key()
    items = A.done_items(key, answers_for(key, lambda it, t: "yes" if it["strata"][0] == f"L:{t}"
                                          else "no"))
    got = {t: A.decide_tag(PIS[t], A.tally(items, f"U:{t}", t),
                           A.tally(items, f"L:{t}", t))["decision"] for t in A.TAGS}
    # pi alone is 0.53 to 0.61 for three tags: they fail. skewer's pi is 0.81: it cannot fail.
    assert got["hangingPiece"] == got["sacrifice"] == got["backRankMate"] == "fails"
    assert got["skewer"] == "inconclusive"


def test_unsure_counts_against_a_clear_and_against_a_fail():
    key = synthetic_key()
    unsure = A.done_items(key, answers_for(key, lambda it, t: "unsure" if t in it["tags"]
                                           and it["strata"][0].startswith("U") else
                                           yes_where_tagged(it, t)))
    for t in ("hangingPiece", "sacrifice", "backRankMate"):
        d = A.decide_tag(PIS[t], A.tally(unsure, f"U:{t}", t), A.tally(unsure, f"L:{t}", t))
        assert d["decision"] == "inconclusive", (t, d)


def test_the_labelled_guard_stops_a_clear():
    key = synthetic_key()

    def rule(it, t):
        if it["strata"][0] == f"L:{t}" and t == "hangingPiece":
            return "no"
        return yes_where_tagged(it, t)

    items = A.done_items(key, answers_for(key, rule))
    d = A.decide_tag(PIS["hangingPiece"], A.tally(items, "U:hangingPiece", "hangingPiece"),
                     A.tally(items, "L:hangingPiece", "hangingPiece"))
    assert d["decision"].startswith("inconclusive: the person and the theme disagree")
    assert not d["guard_passes"]


def test_items_after_the_last_answer_are_not_done_and_blanks_before_it_are_unsure():
    key = synthetic_key()
    ans = answers_for(key, yes_where_tagged)
    for n in range(41, len(key["items"]) + 1):
        del ans["answers"][str(n)]
    del ans["answers"]["3"]["skewer"]
    items = A.done_items(key, ans)
    assert len(items) == 40
    assert items[2]["answers"]["skewer"] == "unsure"


def test_answers_from_another_page_are_refused():
    key = synthetic_key()
    ans = answers_for(key, yes_where_tagged)
    ans["page_id"] = "other"
    with pytest.raises(SystemExit):
        A.done_items(key, ans)


def test_the_registered_run_writes_canonical_results(tmp_path, monkeypatch):
    key = synthetic_key()
    ans = answers_for(key, yes_where_tagged)
    paths = {}
    for name, data in (("key.json", key), ("answers.json", ans)):
        p = tmp_path / name
        p.write_text(json.dumps(data), encoding="utf-8")
        paths[name] = (hashlib.sha256(p.read_bytes()).hexdigest(), p)
    paths["results.json"] = (A.sha256_file(A.EXP7_RESULTS), A.EXP7_RESULTS)
    monkeypatch.setattr(A, "registered_inputs", lambda: paths)
    monkeypatch.setattr(A, "BLOCKS", 3)
    A.cmd_run(tmp_path / "a")
    A.cmd_run(tmp_path / "b")
    first = (tmp_path / "a" / "results.json").read_bytes()
    assert first == (tmp_path / "b" / "results.json").read_bytes()
    res = json.loads(first)
    assert res["decisions"] == {t: "clears" for t in A.TAGS}
    assert res["items_done"] == len(key["items"])
    assert res["compute"]["actual"]["items_read"] == len(key["items"])


def test_too_few_blocks_is_a_failed_result(tmp_path, monkeypatch):
    key = synthetic_key()
    ans = answers_for(key, yes_where_tagged)
    for n in range(43, len(key["items"]) + 1):  # two blocks of 21
        del ans["answers"][str(n)]
    paths = {}
    for name, data in (("key.json", key), ("answers.json", ans)):
        p = tmp_path / name
        p.write_text(json.dumps(data), encoding="utf-8")
        paths[name] = (hashlib.sha256(p.read_bytes()).hexdigest(), p)
    paths["results.json"] = (A.sha256_file(A.EXP7_RESULTS), A.EXP7_RESULTS)
    monkeypatch.setattr(A, "registered_inputs", lambda: paths)
    A.cmd_run(tmp_path)
    res = json.loads((tmp_path / "results.json").read_text(encoding="utf-8"))
    assert res["failed"] and res["blocks_done"] == 2
    assert all(v.startswith("not decided") for v in res["decisions"].values())


def test_a_changed_input_is_refused(tmp_path, monkeypatch):
    p = tmp_path / "key.json"
    p.write_text("{}", encoding="utf-8")
    monkeypatch.setattr(A, "registered_inputs", lambda: {"key.json": ("0" * 64, p)})
    with pytest.raises(SystemExit):
        A.read_input("key.json")


def test_the_rule_rarely_clears_a_tag_at_the_floor():
    import numpy as np

    rng = np.random.default_rng(1)
    for t in A.TAGS:
        r = A.simulate(PIS[t], 0.80, 30, 3, 0.0, 1.0, rng, reps=4000)
        assert r["clears"] <= A.ALPHA_TAG_ONE_SIDED * 2 + 0.003, (t, r)


PIECES = EXP_DIR.parents[2] / "packages" / "ui" / "assets" / "pieces" / "cburnett"


@pytest.mark.skipif(not PIECES.is_dir(), reason="the piece set is outside the lab image")
def test_the_page_holds_no_id_tag_theme_or_rating():
    pytest.importorskip("polars")
    M = load("make_audit")
    data = {"blocks": 1, "practice": [],
            "items": [{"n": 1, "block": 1, "fen": "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1",
                       "setup": "g8h8", "line": ["a1a8"], "mate": True}]}
    html = M.page(data, "pageid")
    assert "__DATA__" not in html and "__PIECES__" not in html
    script = html.split("const D = ", 1)[1].split(";\n", 1)[0]
    payload = json.loads(script)
    assert set(payload["items"][0]) == {"n", "block", "fen", "setup", "line", "mate"}
    assert "connect-src 'none'" in html
    assert not re.search(r"<script[^>]+src=|<link[^>]+href=|https?://(?!www\.w3\.org)", html)


def test_strata_follow_our_tags_and_the_themes():
    pytest.importorskip("polars")
    M = load("make_audit")
    assert M.stratum_of(["fork"], {"hangingPiece"}) == ["D:miss"]
    assert M.stratum_of([], set()) == ["D:none"]
    assert M.stratum_of(["sacrifice", "skewer"], {"skewer"}) == ["L:skewer", "U:sacrifice"]
