"""FastAPI app: `GET /healthz` and `POST /v1/predict` (contract in ADR-R2-13).

    GET  /healthz     -> {"model": "maia2-<type>-<version>"}
    POST /v1/predict  {"items": [{"fen", "eloSelf", "eloOppo"}]}  (at most 256 items)
                      -> {"items": [{"probs": {uci: p}} | {"error": "invalid_fen"}]}
"""

from __future__ import annotations

from contextlib import asynccontextmanager

from fastapi import FastAPI, Request
from pydantic import BaseModel, ConfigDict, Field

from .model import InvalidFen, Maia2Model, MoveModel, clamp_elo

MAX_ITEMS = 256


class PredictItem(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    fen: str = Field(max_length=200)
    elo_self: int | None = Field(default=None, alias="eloSelf")
    elo_oppo: int | None = Field(default=None, alias="eloOppo")


class PredictRequest(BaseModel):
    items: list[PredictItem] = Field(max_length=MAX_ITEMS)


def create_app(model: MoveModel | None = None) -> FastAPI:
    """Builds the app. Without `model`, the real Maia-2 model is loaded once at startup."""

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.model = model if model is not None else Maia2Model.load()
        yield

    app = FastAPI(title="maia", lifespan=lifespan)
    if model is not None:
        app.state.model = model

    @app.get("/healthz")
    def healthz(request: Request) -> dict:
        return {"model": request.app.state.model.model_id}

    # A sync handler: FastAPI runs it in its thread pool, so inference never blocks the loop.
    @app.post("/v1/predict")
    def predict(body: PredictRequest, request: Request) -> dict:
        m: MoveModel = request.app.state.model
        out = []
        for it in body.items:
            try:
                probs = m.predict(it.fen, clamp_elo(it.elo_self), clamp_elo(it.elo_oppo))
                out.append({"probs": probs})
            except InvalidFen:
                out.append({"error": "invalid_fen"})
        return {"items": out}

    return app


app = create_app()
