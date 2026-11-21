"""API tests for services/maia with an injected fake model (no torch, no weights)."""

from __future__ import annotations

import json

import pytest
from fastapi.testclient import TestClient

from app.main import MAX_BODY_BYTES, MAX_ITEMS, create_app
from app.model import (
    DEFAULT_ELO,
    ELO_MAX,
    ELO_MIN,
    InvalidFen,
    NoLegalMoves,
    clamp_elo,
    validate_fen,
)

START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
MATE = "rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3"
ODD = "8/8/8/8/8/8/k7/6K1 w - - 0 40"  # the fake returns a bad distribution here


class FakeModel:
    model_id = "maia2-fake-0.0.0"

    def __init__(self) -> None:
        self.calls: list[tuple[str, int, int]] = []

    def predict(self, fen: str, elo_self: int, elo_oppo: int) -> dict[str, float]:
        self.calls.append((fen, elo_self, elo_oppo))
        if fen == MATE:
            raise NoLegalMoves(fen)
        if fen == ODD:
            return {"a2a3": 0.2, "a2a4": 0.2}
        # Rounded like maia2 (4 decimals): sums to 0.9999.
        return {"e2e4": 0.5, "d2d4": 0.3, "g1f3": 0.1999}


@pytest.fixture
def fake() -> FakeModel:
    return FakeModel()


@pytest.fixture
def client(fake: FakeModel) -> TestClient:
    return TestClient(create_app(fake))


def item(fen: str = START, elo_self: object = 1500, elo_oppo: object = 1500) -> dict:
    return {"fen": fen, "eloSelf": elo_self, "eloOppo": elo_oppo}


def predict(client: TestClient, items: list[dict]):
    return client.post("/v1/predict", json={"items": items})


def test_healthz_returns_model_id(client: TestClient) -> None:
    r = client.get("/healthz")
    assert r.status_code == 200
    assert r.json() == {"model": "maia2-fake-0.0.0"}


def test_predict_probabilities_are_normalised(client: TestClient) -> None:
    r = predict(client, [item(), item()])
    assert r.status_code == 200
    items = r.json()["items"]
    assert len(items) == 2
    for it in items:
        assert sum(it["probs"].values()) == pytest.approx(1.0, abs=1e-12)
        assert all(0.0 <= p <= 1.0 for p in it["probs"].values())
        assert it["probs"]["e2e4"] == pytest.approx(0.5, abs=1e-4)


def test_invalid_fen_is_a_per_item_error_and_never_reaches_the_model(
    client: TestClient, fake: FakeModel
) -> None:
    r = predict(client, [item("not a fen"), item()])
    assert r.status_code == 200
    items = r.json()["items"]
    assert items[0] == {"error": "invalid_fen"}
    assert "probs" in items[1]
    assert [c[0] for c in fake.calls] == [START]


def test_no_legal_moves_is_a_per_item_error(client: TestClient) -> None:
    r = predict(client, [item(MATE)])
    assert r.json()["items"] == [{"error": "no_legal_moves"}]


def test_bad_model_output_is_a_per_item_error(client: TestClient) -> None:
    r = predict(client, [item(ODD), item()])
    items = r.json()["items"]
    assert items[0] == {"error": "model_error"}
    assert "probs" in items[1]


@pytest.mark.parametrize(
    "fen",
    [
        "",
        "not a fen",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0",  # 5 fields
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP w KQkq - 0 1",  # 7 ranks
        "rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",  # 9 squares
        "rnbqkbnr/ppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",  # 7 squares
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNX w KQkq - 0 1",  # bad piece
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQQBNR w KQkq - 0 1",  # no white king
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKKBNR w KQkq - 0 1",  # two white kings
        "pnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",  # pawn on rank 8
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",  # side
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkqK - 0 1",  # castling
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e5 0 1",  # ep rank
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - -1 1",  # halfmove
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 0",  # fullmove
    ],
)
def test_validate_fen_rejects(fen: str) -> None:
    with pytest.raises(InvalidFen):
        validate_fen(fen)


@pytest.mark.parametrize(
    "fen",
    [
        START,
        MATE,
        "r1bqk2r/ppp2ppp/2np1n2/2b1p3/2B1P3/2PP1N2/PP1N1PPP/R1BQK2R b KQkq - 1 6",
        "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3",
        "8/4P3/8/8/8/8/k7/6K1 w - - 0 50",
    ],
)
def test_validate_fen_accepts(fen: str) -> None:
    validate_fen(fen)


def test_257_items_is_422(client: TestClient) -> None:
    assert MAX_ITEMS == 256
    assert predict(client, [item()] * 257).status_code == 422


def test_256_items_is_accepted(client: TestClient) -> None:
    r = predict(client, [item()] * 256)
    assert r.status_code == 200
    assert len(r.json()["items"]) == 256


def test_elo_is_clamped_and_defaulted(client: TestClient, fake: FakeModel) -> None:
    r = predict(client, [item(elo_self=100, elo_oppo=3200), item(elo_self=None, elo_oppo=None)])
    assert r.status_code == 200
    assert fake.calls == [(START, ELO_MIN, ELO_MAX), (START, DEFAULT_ELO, DEFAULT_ELO)]


def test_clamp_elo_bounds() -> None:
    assert clamp_elo(None) == DEFAULT_ELO
    assert clamp_elo(ELO_MIN - 1) == ELO_MIN
    assert clamp_elo(ELO_MAX + 1) == ELO_MAX
    assert clamp_elo(1500) == 1500


def test_missing_items_is_422(client: TestClient) -> None:
    assert client.post("/v1/predict", json={}).status_code == 422


@pytest.mark.parametrize("elo", ["1500", 1500.0, 1500.5, True])
def test_elo_must_be_a_strict_int(client: TestClient, elo: object) -> None:
    assert predict(client, [item(elo_self=elo)]).status_code == 422


def test_fen_must_be_a_string(client: TestClient) -> None:
    assert predict(client, [item(fen=123)]).status_code == 422  # type: ignore[arg-type]


def test_extra_fields_are_rejected(client: TestClient) -> None:
    extra_item = {**item(), "depth": 3}
    assert predict(client, [extra_item]).status_code == 422
    r = client.post("/v1/predict", json={"items": [item()], "debug": True})
    assert r.status_code == 422


def test_body_over_64_kb_is_413(client: TestClient, fake: FakeModel) -> None:
    assert MAX_BODY_BYTES == 64 * 1024
    body = json.dumps({"items": [item()], "pad": "x" * MAX_BODY_BYTES}).encode()
    r = client.post(
        "/v1/predict", content=body, headers={"content-type": "application/json"}
    )
    assert r.status_code == 413
    assert fake.calls == []


def test_chunked_body_over_64_kb_is_413(client: TestClient, fake: FakeModel) -> None:
    body = json.dumps({"items": [item()], "pad": "x" * MAX_BODY_BYTES}).encode()

    def chunks():
        for i in range(0, len(body), 4096):
            yield body[i : i + 4096]

    r = client.post(
        "/v1/predict", content=chunks(), headers={"content-type": "application/json"}
    )
    assert r.status_code == 413
    assert fake.calls == []


def test_full_256_item_batch_fits_the_body_limit(client: TestClient) -> None:
    long_fen = "r1bqk2r/ppp2ppp/2np1n2/2b1p3/2B1P3/2PP1N2/PP1N1PPP/R1BQK2R b KQkq - 100 100"
    body = json.dumps({"items": [item(long_fen, 2000, 2000)] * MAX_ITEMS})
    assert len(body) < MAX_BODY_BYTES
    r = client.post(
        "/v1/predict", content=body, headers={"content-type": "application/json"}
    )
    assert r.status_code == 200
