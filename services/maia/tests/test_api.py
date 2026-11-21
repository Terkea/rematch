"""API tests for services/maia with an injected fake model (no torch, no weights)."""

from __future__ import annotations

import pytest
from fastapi.testclient import TestClient

from app.main import MAX_ITEMS, create_app
from app.model import DEFAULT_ELO, ELO_MAX, ELO_MIN, InvalidFen, clamp_elo

START = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"


class FakeModel:
    model_id = "maia2-fake-0.0.0"

    def __init__(self) -> None:
        self.calls: list[tuple[str, int, int]] = []

    def predict(self, fen: str, elo_self: int, elo_oppo: int) -> dict[str, float]:
        self.calls.append((fen, elo_self, elo_oppo))
        if len(fen.split(" ")) != 6:
            raise InvalidFen(fen)
        return {"e2e4": 0.5, "d2d4": 0.3, "g1f3": 0.2}


@pytest.fixture
def fake() -> FakeModel:
    return FakeModel()


@pytest.fixture
def client(fake: FakeModel) -> TestClient:
    return TestClient(create_app(fake))


def item(fen: str = START, elo_self: int | None = 1500, elo_oppo: int | None = 1500) -> dict:
    return {"fen": fen, "eloSelf": elo_self, "eloOppo": elo_oppo}


def test_healthz_returns_model_id(client: TestClient) -> None:
    r = client.get("/healthz")
    assert r.status_code == 200
    assert r.json() == {"model": "maia2-fake-0.0.0"}


def test_predict_probabilities_sum_to_one(client: TestClient) -> None:
    r = client.post("/v1/predict", json={"items": [item(), item()]})
    assert r.status_code == 200
    items = r.json()["items"]
    assert len(items) == 2
    for it in items:
        assert sum(it["probs"].values()) == pytest.approx(1.0)
        assert it["probs"]["e2e4"] == pytest.approx(0.5)


def test_invalid_fen_is_a_per_item_error(client: TestClient) -> None:
    r = client.post("/v1/predict", json={"items": [item("not a fen"), item()]})
    assert r.status_code == 200
    items = r.json()["items"]
    assert items[0] == {"error": "invalid_fen"}
    assert "probs" in items[1]


def test_257_items_is_422(client: TestClient) -> None:
    assert MAX_ITEMS == 256
    r = client.post("/v1/predict", json={"items": [item()] * 257})
    assert r.status_code == 422


def test_256_items_is_accepted(client: TestClient) -> None:
    r = client.post("/v1/predict", json={"items": [item()] * 256})
    assert r.status_code == 200
    assert len(r.json()["items"]) == 256


def test_elo_is_clamped_and_defaulted(client: TestClient, fake: FakeModel) -> None:
    r = client.post(
        "/v1/predict",
        json={"items": [item(elo_self=100, elo_oppo=3200), item(elo_self=None, elo_oppo=None)]},
    )
    assert r.status_code == 200
    assert fake.calls == [(START, ELO_MIN, ELO_MAX), (START, DEFAULT_ELO, DEFAULT_ELO)]


def test_clamp_elo_bounds() -> None:
    assert clamp_elo(None) == DEFAULT_ELO
    assert clamp_elo(ELO_MIN - 1) == ELO_MIN
    assert clamp_elo(ELO_MAX + 1) == ELO_MAX
    assert clamp_elo(1500) == 1500


def test_missing_items_is_422(client: TestClient) -> None:
    assert client.post("/v1/predict", json={}).status_code == 422
