"""Model interface and the real Maia-2 backend.

The API depends only on the `MoveModel` protocol, so tests inject a fake and never import torch.
`maia2` (MIT) and its dependencies are imported lazily, inside `Maia2Model.load`.
"""

from __future__ import annotations

import os
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


class InvalidFen(ValueError):
    """The position cannot be evaluated (malformed FEN, or no legal moves)."""


class MoveModel(Protocol):
    model_id: str

    def predict(self, fen: str, elo_self: int, elo_oppo: int) -> dict[str, float]:
        """Move probabilities keyed by UCI. Raises `InvalidFen`."""
        ...


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
        except (ValueError, IndexError, KeyError, AssertionError) as e:
            raise InvalidFen(str(e)) from e
        return {uci: float(p) for uci, p in probs.items()}
