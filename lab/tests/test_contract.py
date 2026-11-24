"""The contract between rematch_lab and `cli lab` (ADR 0106).

1. The pydantic models mirror the JSON Schema field for field.
2. The committed fixture through `rematch_lab.profile` and `rematch_lab.rank` gives the committed
   golden files: byte for byte from the cli, value for value through the typed wrappers.
3. Bad input is refused by the schema before it reaches the cli.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import rematch_lab
from rematch_lab import models
from rematch_lab.bridge import call_raw, schema

LAB = Path(__file__).resolve().parent.parent
FIXTURE = LAB / "fixtures" / "profile-request.json"
GOLDEN = LAB / "fixtures" / "golden"
PLACEHOLDER = "ALGORITHM_ID"


def with_placeholder(text: str, algorithm_id: str) -> str:
    return text.replace(f'"algorithm_id": "{algorithm_id}"', f'"algorithm_id": "{PLACEHOLDER}"')


def wire_names(model: type) -> tuple[set[str], set[str]]:
    props, required = set(), set()
    for name, field in model.model_fields.items():
        wire = field.alias or name
        props.add(wire)
        if field.is_required():
            required.add(wire)
    return props, required


@pytest.mark.parametrize("name", sorted(models.OBJECTS))
def test_models_mirror_the_schema(name: str) -> None:
    definition = schema()["$defs"][name]
    props, required = wire_names(models.OBJECTS[name])
    assert props == set(definition["properties"]), f"{name}: properties differ"
    schema_required = set(definition.get("required", []))
    # A request model defaults schema_version to lab-v1; the wire still requires it.
    assert required <= schema_required, f"{name}: the model requires more than the schema"
    assert schema_required - required <= {"schema_version"}, f"{name}: required fields differ"


def test_the_cli_output_is_the_golden_file_byte_for_byte() -> None:
    request = FIXTURE.read_text(encoding="utf-8")
    profile = call_raw(["lab", "profile"], request)
    algorithm_id = json.loads(profile)["algorithm_id"]
    rematch_lab.check("ProfileResponse", json.loads(profile))
    assert with_placeholder(profile, algorithm_id) == (GOLDEN / "profile-response.json").read_text(
        encoding="utf-8"
    )
    rank = call_raw(["lab", "rank"], profile)
    rematch_lab.check("RankResponse", json.loads(rank))
    assert with_placeholder(rank, algorithm_id) == (GOLDEN / "rank-response.json").read_text(
        encoding="utf-8"
    )


def test_the_typed_wrappers_give_the_golden_values() -> None:
    facts = models.ProfileRequest.model_validate_json(FIXTURE.read_text(encoding="utf-8"))
    profiles = rematch_lab.profile(facts)
    ranking = rematch_lab.rank(profiles)
    assert ranking.schema_version == rematch_lab.SCHEMA_VERSION
    assert ranking.algorithm_id == profiles.algorithm_id

    def golden(name: str) -> dict:
        doc = json.loads((GOLDEN / name).read_text(encoding="utf-8"))
        doc["algorithm_id"] = ranking.algorithm_id
        return doc

    assert json.loads(profiles.model_dump_json(by_alias=True)) == golden("profile-response.json")
    assert json.loads(ranking.model_dump_json(by_alias=True)) == golden("rank-response.json")

    hangs, clean = ranking.players
    assert [w.id for w in hangs.ranked][:1] == ["W1"]
    assert hangs.focuses == ["W1"]
    assert clean.ranked == [] and clean.focuses == ["general"]


def test_responses_carry_the_build_manifest_id() -> None:
    manifest = rematch_lab.algorithm()
    assert manifest["id"].startswith("algo-")
    facts = models.ProfileRequest.model_validate_json(FIXTURE.read_text(encoding="utf-8"))
    assert rematch_lab.profile(facts).algorithm_id == manifest["id"]


def test_the_same_request_gives_the_same_ranking() -> None:
    facts = models.ProfileRequest.model_validate_json(FIXTURE.read_text(encoding="utf-8"))
    a = rematch_lab.rank(rematch_lab.profile(facts)).model_dump_json(by_alias=True)
    b = rematch_lab.rank(rematch_lab.profile(facts)).model_dump_json(by_alias=True)
    assert a == b


def test_a_bad_request_is_refused_before_the_cli() -> None:
    bad = json.loads(FIXTURE.read_text(encoding="utf-8"))
    bad["players"][0]["band"] = "B9"
    with pytest.raises(rematch_lab.LabError, match="band"):
        rematch_lab.check("ProfileRequest", bad)
    with pytest.raises(ValueError):
        models.PlayerFacts.model_validate({**bad["players"][1], "serie": {}})


def test_the_cli_refuses_an_unknown_schema_version() -> None:
    with pytest.raises(rematch_lab.LabError, match="lab-v0"):
        call_raw(["lab", "profile"], '{"schema_version": "lab-v0", "players": []}')


def test_algo_variant_selects_the_prior_rule() -> None:
    """`variant` reaches the cli as ALGO_VARIANT (EXP-0006): the champion is production, a dev
    rule moves the intervals, and an unknown name is refused."""
    request = models.ProfileRequest.model_validate_json(FIXTURE.read_text(encoding="utf-8"))
    base = rematch_lab.profile(request)
    assert rematch_lab.profile(request, variant="champion") == base
    lighter = rematch_lab.profile(request, variant="dev:games=1,ess")
    assert lighter.algorithm_id == base.algorithm_id
    intervals = lambda r: [e.posterior for p in r.players for e in p.estimates]  # noqa: E731
    assert intervals(lighter) != intervals(base)
    with pytest.raises(rematch_lab.LabError):
        rematch_lab.profile(request, variant="lighter")


TAG_FIXTURE = LAB / "fixtures" / "tag-motifs-request.json"


def test_tag_motifs_gives_the_golden_file() -> None:
    request = TAG_FIXTURE.read_text(encoding="utf-8")
    out = call_raw(["lab", "tag-motifs"], request)
    algorithm_id = json.loads(out)["algorithm_id"]
    rematch_lab.check("TagMotifsResponse", json.loads(out))
    assert with_placeholder(out, algorithm_id) == (GOLDEN / "tag-motifs-response.json").read_text(
        encoding="utf-8"
    )
    typed = rematch_lab.tag_motifs(models.TagMotifsRequest.model_validate_json(request))
    golden = json.loads((GOLDEN / "tag-motifs-response.json").read_text(encoding="utf-8"))
    golden["algorithm_id"] = typed.algorithm_id
    assert json.loads(typed.model_dump_json(by_alias=True)) == golden
    by_id = {line.id: line for line in typed.lines}
    assert by_id["puzzle-00008"].pov == "white"
    assert "smotheredMate" in by_id["smothered"].best.tags
    assert by_id["bad-fen"].best is None and by_id["bad-fen"].fen is None


def test_a_bad_tag_request_is_refused_before_the_cli() -> None:
    with pytest.raises(rematch_lab.LabError, match="moves"):
        rematch_lab.check(
            "TagMotifsRequest",
            {"schema_version": "lab-v1", "lines": [{"id": "x", "fen": "f", "pv": [], "moves": []}]},
        )


# ---- engine depth (EXP-0008) -----------------------------------------------------------------

OPEN_GAME = '[Event "t"]\n[Result "*"]\n\n1. e4 e5 2. Nf3 Nc6 3. Bc4 Nd4 *\n'


def test_positions_and_judge_round_trip() -> None:
    games = [models.DepthGame(id="open", pgn=OPEN_GAME)]
    asked = rematch_lab.positions(games)
    g = asked.games[0]
    assert (g.plies, g.terminal, len(g.fens)) == (6, False, 7)
    assert g.sequence == g.fens
    # Level everywhere except after 3...Nd4, where White (to move) is +4: a blunder by Black.
    searches = [
        models.GivenSearch(fen=f, score=models.EngineScore(cp=400 if i == 6 else 10))
        for i, f in enumerate(g.fens)
    ]
    judged = rematch_lab.judge(models.JudgeRequest(games=games, searches=searches))
    labels = [m.judgement for m in judged.games[0].moves]
    assert labels == [None, None, None, None, None, "blunder"]
    lichess = models.DepthGame(id="open", pgn=OPEN_GAME, lichess_evals=[models.EngineScore(cp=20)] * 6)
    only_start = [searches[0]]
    judged = rematch_lab.judge(models.JudgeRequest(games=[lichess], searches=only_start))
    assert judged.games[0].lichess_used == 6


def test_observe_counts_the_eval_only_keys_from_lichess_evaluations() -> None:
    clocked = (
        '[Event "t"]\n[Result "*"]\n\n1. e4 { [%clk 0:10:00] } e5 { [%clk 0:10:00] } '
        "2. Nf3 { [%clk 0:09:58] } Nc6 { [%clk 0:09:57] } 3. Bc4 { [%clk 0:09:55] } "
        "Nd4 { [%clk 0:09:50] } *\n"
    )
    evals = [models.EngineScore(cp=20)] * 5 + [models.EngineScore(cp=400)]
    start = rematch_lab.positions([models.DepthGame(id="s", pgn=OPEN_GAME)]).games[0].fens[0]
    req = models.ObserveRequest(
        games=[
            models.ObserveGame(
                id="open", pgn=clocked, base_seconds=600, increment_seconds=0,
                lichess_evals=evals, sides=["black"],
            ),
            models.ObserveGame(
                id="gap", pgn=clocked, base_seconds=600, increment_seconds=0,
                lichess_evals=[None] * 6, sides=["white"],
            ),
        ],
        searches=[models.GivenSearch(fen=start, score=models.EngineScore(cp=20))],
    )
    out = rematch_lab.observe(req)
    black = out.games[0].sides[0]
    assert (black.blunders, black.keys["W7.traps"].events) == (1, 1)
    assert "W1" not in black.keys
    assert out.games[1].error is not None and out.games[1].sides == []


def test_an_engine_score_is_exactly_one_of_cp_or_mate() -> None:
    assert json.loads(models.EngineScore(mate=-3).model_dump_json()) == {"mate": -3}
    with pytest.raises(ValueError):
        models.EngineScore(cp=1, mate=2)
    with pytest.raises(ValueError):
        models.EngineScore()
