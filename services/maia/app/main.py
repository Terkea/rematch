"""FastAPI app: `GET /healthz` and `POST /v1/predict` (contract in ADR-R2-13).

    GET  /healthz     -> {"model": "maia2-<type>-<version>"}
    POST /v1/predict  {"items": [{"fen", "eloSelf", "eloOppo"}]}  (at most 256 items, 64 KB)
                      -> {"items": [{"probs": {uci: p}}
                                    | {"error": "invalid_fen" | "no_legal_moves" | "model_error"}]}

Probabilities are validated (finite, in [0, 1], sum 1 +- 0.01) and renormalised to sum to 1
(maia2 rounds them to 4 decimals). Any other model output, or any unexpected exception for one
item, becomes `model_error` for that item only; the rest of the batch is answered.
"""

from __future__ import annotations

import logging
import math
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request
from pydantic import BaseModel, ConfigDict, Field, StrictInt, StrictStr

from .model import InvalidFen, Maia2Model, MoveModel, NoLegalMoves, clamp_elo, validate_fen

log = logging.getLogger("maia")

MAX_ITEMS = 256
MAX_BODY_BYTES = 64 * 1024
SUM_TOLERANCE = 0.01


class PredictItem(BaseModel):
    model_config = ConfigDict(extra="forbid")

    fen: StrictStr = Field(max_length=100)
    eloSelf: StrictInt | None = None  # noqa: N815 (wire name)
    eloOppo: StrictInt | None = None  # noqa: N815 (wire name)


class PredictRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")

    items: list[PredictItem] = Field(max_length=MAX_ITEMS)


class BodyLimit:
    """Pure ASGI middleware: 413 for a request body over `limit` bytes, whether the size is
    declared in Content-Length or only seen while streaming (chunked)."""

    def __init__(self, app, limit: int) -> None:
        self.app = app
        self.limit = limit

    async def __call__(self, scope, receive, send):
        if scope["type"] != "http":
            return await self.app(scope, receive, send)
        for name, value in scope.get("headers", []):
            if name == b"content-length":
                try:
                    declared = int(value)
                except ValueError:
                    return await self._reject(send, 400, b"bad content-length")
                if declared > self.limit:
                    return await self._reject(send, 413, b"request body too large")

        # Read the body up front, counting bytes, then replay it to the app.
        chunks: list[bytes] = []
        size = 0
        more = True
        while more:
            message = await receive()
            if message["type"] == "http.disconnect":
                return
            body = message.get("body", b"")
            size += len(body)
            if size > self.limit:
                return await self._reject(send, 413, b"request body too large")
            chunks.append(body)
            more = message.get("more_body", False)
        replay = [{"type": "http.request", "body": b"".join(chunks), "more_body": False}]

        async def replay_receive():
            if replay:
                return replay.pop()
            return await receive()

        return await self.app(scope, replay_receive, send)

    @staticmethod
    async def _reject(send, status: int, text: bytes) -> None:
        await send(
            {
                "type": "http.response.start",
                "status": status,
                "headers": [(b"content-type", b"text/plain"), (b"connection", b"close")],
            }
        )
        await send({"type": "http.response.body", "body": text})


def normalise(probs: dict[str, float]) -> dict[str, float] | None:
    """The validated, renormalised distribution, or None when it is unusable."""
    if not probs:
        return None
    values = list(probs.values())
    if not all(isinstance(p, float) and math.isfinite(p) and 0.0 <= p <= 1.0 for p in values):
        return None
    total = math.fsum(values)
    if abs(total - 1.0) > SUM_TOLERANCE:
        return None
    return {uci: p / total for uci, p in probs.items()}


def create_app(model: MoveModel | None = None) -> FastAPI:
    """Builds the app. Without `model`, the real Maia-2 model is loaded once at startup."""

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.model = model if model is not None else Maia2Model.load()
        yield

    app = FastAPI(title="maia", lifespan=lifespan, docs_url=None, redoc_url=None, openapi_url=None)
    if model is not None:
        app.state.model = model

    @app.get("/healthz")
    def healthz(request: Request) -> dict:
        return {"model": request.app.state.model.model_id}

    # A sync handler: FastAPI runs it in its thread pool, so inference never blocks the loop.
    @app.post("/v1/predict")
    def predict(body: PredictRequest, request: Request) -> dict:
        m: MoveModel = request.app.state.model
        out: list[dict] = []
        for it in body.items:
            try:
                validate_fen(it.fen)
                probs = normalise(m.predict(it.fen, clamp_elo(it.eloSelf), clamp_elo(it.eloOppo)))
            except InvalidFen:
                out.append({"error": "invalid_fen"})
                continue
            except NoLegalMoves:
                out.append({"error": "no_legal_moves"})
                continue
            except Exception:
                # One poison position must not fail the batch: log it, report model_error for
                # this item only (the Rust client treats it as pending_maia/invalid_response).
                log.exception("maia2 failed on one item", extra={"fen": it.fen})
                out.append({"error": "model_error"})
                continue
            out.append({"probs": probs} if probs is not None else {"error": "model_error"})
        return {"items": out}

    app.add_middleware(BodyLimit, limit=MAX_BODY_BYTES)
    return app


app = create_app()
