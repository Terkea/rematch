"""Model interface, FEN validation and the real Maia-2 backend.

The API depends only on the `MoveModel` protocol, so tests inject a fake and never import torch.
`maia2` (MIT) and its dependencies are imported lazily, inside `Maia2Model.load`. This module
does not import python-chess: FENs are validated here by our own rules, and maia2 runs the
legal-move generation inside its own package.
"""

from __future__ import annotations

import os
import re
import threading
from importlib.metadata import version
from typing import Protocol

# Maia-2 buckets Elo as <1100, 1100-1199, ..., 1900-1999, >=2000 (maia2.utils.create_elo_dict).
# Values outside this range map to the end buckets; we clamp to it so the request is explicit.
ELO_MIN = 1000
ELO_MAX = 2100
# Used when a player's rating is unknown.
DEFAULT_ELO = 1500

MODEL_TYPES = ("rapid", "blitz")

_RANK = re.compile(r"^[pnbrqkPNBRQK1-8]+$")
_CASTLING = re.compile(r"^(-|K?Q?k?q?)$")
_EP = re.compile(r"^(-|[a-h][36])$")


class InvalidFen(ValueError):
    """The FEN is malformed."""


class NoLegalMoves(ValueError):
    """Checkmate or stalemate: there is no move to predict."""


class MoveModel(Protocol):
    model_id: str

    def predict(self, fen: str, elo_self: int, elo_oppo: int) -> dict[str, float]:
        """Move probabilities keyed by UCI. Raises `InvalidFen` or `NoLegalMoves`."""
        ...


def validate_fen(fen: str) -> None:
    """Raises `InvalidFen` unless `fen` is a well-formed six-field FEN with one king per side
    and no pawns on the back ranks. Legality beyond that is maia2's (python-chess's) job."""
    fields = fen.split(" ")
    if len(fields) != 6:
        raise InvalidFen("expected 6 fields")
    placement, side, castling, ep, halfmove, fullmove = fields
    ranks = placement.split("/")
    if len(ranks) != 8:
        raise InvalidFen("expected 8 ranks")
    for i, rank in enumerate(ranks):
        if not _RANK.match(rank):
            raise InvalidFen(f"bad rank {rank!r}")
        if sum(int(c) if c.isdigit() else 1 for c in rank) != 8:
            raise InvalidFen(f"rank {rank!r} is not 8 squares")
        if i in (0, 7) and ("p" in rank or "P" in rank):
            raise InvalidFen("pawn on a back rank")
    if placement.count("K") != 1 or placement.count("k") != 1:
        raise InvalidFen("each side needs exactly one king")
    if side not in ("w", "b"):
        raise InvalidFen("bad side to move")
    if castling == "" or not _CASTLING.match(castling):
        raise InvalidFen("bad castling field")
    if not _EP.match(ep):
        raise InvalidFen("bad en-passant field")
    if not halfmove.isdigit() or not fullmove.isdigit() or int(fullmove) < 1:
        raise InvalidFen("bad move counters")


def clamp_elo(elo: int | None) -> int:
    if elo is None:
        return DEFAULT_ELO
    return max(ELO_MIN, min(ELO_MAX, elo))


class Maia2Model:
    """The released Maia-2 model, loaded once. Inference is serialised by a lock."""

    def __init__(self, model_type: str, model, prepared) -> None:
        self.model_id = f"maia2-{model_type}-{version('maia2')}"
        self._model = model
        self._prepared = prepared
        self._lock = threading.Lock()

    @classmethod
    def load(cls, model_type: str | None = None, save_root: str | None = None) -> Maia2Model:
        model_type = model_type or os.environ.get("MAIA_MODEL", "rapid")
        if model_type not in MODEL_TYPES:
            raise ValueError(f"MAIA_MODEL must be one of {MODEL_TYPES}, got {model_type!r}")
        save_root = save_root or os.environ.get("MAIA_MODEL_DIR", "/opt/maia2_models")

        import torch
        from maia2 import inference, model

        torch.set_num_threads(int(os.environ.get("MAIA_THREADS", "2")))
        # The weights are baked into the image; this only validates their SHA-256.
        loaded = model.from_pretrained(type=model_type, device="cpu", save_root=save_root)
        return cls(model_type, loaded, inference.prepare())

    def predict(self, fen: str, elo_self: int, elo_oppo: int) -> dict[str, float]:
        from maia2 import inference

        try:
            with self._lock:
                probs, _ = inference.inference_each(
                    self._model, self._prepared, fen, elo_self, elo_oppo
                )
        except ValueError as e:
            # maia2.inference.preprocessing: "Cannot run inference on a position without legal
            # moves" for mate and stalemate; python-chess's ValueError for an illegal FEN.
            if "without legal moves" in str(e):
                raise NoLegalMoves(fen) from e
            raise InvalidFen(str(e)) from e
        except (IndexError, KeyError, AssertionError) as e:
            raise InvalidFen(str(e)) from e
        return {uci: float(p) for uci, p in probs.items()}
