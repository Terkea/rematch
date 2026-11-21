# ADR-R2-13. Maia licence check and difficulty providers

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Rust port of TS M2's ADR 0033 (Task 21), done as R2 Task 24.

## Context

Move difficulty (`RawMoveFeatures.difficulty = {pBest, pOk, model}`) tells coaching whether a missed move was one a player of that rating finds. The best estimator is Maia-2, a neural network that predicts human moves per Elo. Using it in a commercial product needs both the code and the weights to grant commercial use (licensing §14). The plan made the licence check the gate: compatible gives branch A (the provider plus `services/maia`); anything non-commercial, unlicensed, AGPL or unclear gives branch B (spread fallback only).

## Licence check (primary sources, 2026-11-21)

| What | Source | Finding |
|---|---|---|
| Code licence | `gh api repos/CSSLab/maia2 --jq '.license.spdx_id'` | `MIT` |
| LICENSE file | https://github.com/CSSLab/maia2/blob/main/LICENSE (raw: https://raw.githubusercontent.com/CSSLab/maia2/main/LICENSE) | Standard MIT text, "Copyright (c) 2024 CSSLab". Grants use, copy, modify, merge, publish, distribute, sublicense, sell. No field-of-use or non-commercial clause. |
| README licence section | https://github.com/CSSLab/maia2/blob/main/README.md ("## License") | "Maia-2 is released under the MIT License." The README's first paragraph defines Maia-2 as "a unified, skill-aware chess model", i.e. the statement names the model, not only the code. |
| PyPI metadata | https://pypi.org/pypi/maia2/json (version 0.11.0, uploaded 2026-07-15) | `license` field empty; classifier `License :: OSI Approved :: MIT License`. |
| Where the weights come from | `maia2/model.py` at https://github.com/CSSLab/maia2/blob/main/maia2/model.py (`_MODEL_ASSETS`) | Two released checkpoints on Google Drive, fetched by the MIT package's own `from_pretrained` and SHA-256-verified: `rapid_model.pt` (`https://drive.google.com/uc?id=1gbC1-c7c0EQOPPAVpGWubezeEW8grVwc`, sha256 `65aae846…e997`) and `blitz_model.pt` (`https://drive.google.com/uc?id=1X-Z4J3PX3MQFJoa8gRt3aL8CIH0PWoyt`, sha256 `5090d5d0…44b2`). |
| Separate weight terms | GitHub issues search (`repo:CSSLab/maia2 license`, `… commercial`), release assets (v0.9.1–v0.11.0), Hugging Face `UofTCSSLab` models | None. No separate weights licence, model card or terms exist anywhere in the project. Releases carry only the wheel and sdist. (The Maia-3 Hugging Face models have no licence tag, but we do not use Maia-3.) |
| Training data | README "Training / Data" | The Lichess standard-rated monthly database, which is CC0. |
| maiachess.com | https://www.maiachess.com/ | Returned HTTP 429 to every request on 2026-11-21. It is the separate Maia platform (frontend repo `CSSLab/maia-platform-frontend` is GPL-3.0); we use neither the site nor that code. |

Runtime dependencies (installed unmodified in the `maia` image, from `services/maia/requirements.lock`): `chess` 1.10.0 is **GPL-3.0+**; `certifi` and `tqdm` are MPL-2.0 (tqdm MPL-2.0 AND MIT); `typing_extensions` and `backports.zstd` are PSF-2.0; everything else is MIT, BSD-2/3-Clause or Apache-2.0 (torch, numpy, pandas, fastapi, starlette, uvicorn, pydantic, requests, gdown, einops, …).

### Outcome: compatible, branch A

- The code is MIT. The weights carry no terms of their own; the project's only licence statement says Maia-2, the model, is MIT, and the weights are distributed through the MIT package with pinned hashes. They were trained on CC0 data. Nothing anywhere is non-commercial, AGPL or "all rights reserved".
- python-chess (GPL-3.0+) runs only inside the `maia` container: an unmodified library in a separate server process that we do not distribute, the same as Stockfish (licensing §14, TS M2 Task 21 Step 1). Our own service code (`services/maia/app`) does not import `chess`; it calls `maia2.inference` and treats its `ValueError` as `invalid_fen`. No copyleft code is in the repository.
- The weights are downloaded at image build into `/opt/maia2_models` (maia2 verifies the SHA-256) and are never committed.

Residual risk, accepted: the weights' licence rests on the README's project-wide statement rather than a licence file shipped with the checkpoints. If the authors ever publish different weight terms, unset `MAIA_URL` (spread fallback, no code change) and revisit this ADR. If the image is ever distributed to third parties (it is not: private beta, ADR 0041), the GPL-3.0 obligations of python-chess and the MPL-2.0 notices apply to that image and need a fresh review.

## Decision

### Providers (`backend/crates/enrich/src/difficulty`)

All implement `DifficultyProvider` from `enrich::types` and return `(p_best, p_ok)` per input. A line is acceptable when its mover win% is at least `best − 5` (`OK_WINDOW`, inclusive), where `best` is the highest win% among the input's MultiPV lines.

- **`SpreadDifficulty`** (`model = "spread-v1"`): `None` with fewer than 2 lines; `playable` = acceptable lines; `p_ok = playable / lines`, `p_best = 1 / playable`. A pure function of the engine lines, so it is always available.
- **`MaiaDifficulty::connect(url, timeout = 10 s, batch_size = 64)`**: reads the model id from `GET /healthz` (`{"model": "maia2-<type>-<version>"}`, e.g. `maia2-rapid-0.11.0`). `estimate` sends `POST /v1/predict {items:[{fen, eloSelf, eloOppo}]}` in batches of at most 64 (130 inputs = 3 requests) and reads `{items:[{probs:{uci:p}} | {error:"invalid_fen"}]}`.
  - `p_best = probs[best_uci]`, or 0 when Maia gives it no mass.
  - `p_ok = Σ probs[m]` over the input's lines `m` that are acceptable. Without lines, `p_ok = p_best` (only the best move is known to be acceptable).
  - `None` for an item the service rejects and for an input without `best_uci`.
  - Transport errors, timeouts, non-2xx answers (a 503 included), malformed bodies and an item-count mismatch are `DifficultyError::Unavailable`. Task 25 turns that into `difficulty = null` for the game (`difficulty_provider_failure_yields_null_difficulty`).
- **`select_difficulty_provider(maia_url: Option<&str>) -> Arc<dyn DifficultyProvider>`** (async, because it probes `/healthz`): Maia when the URL is set, non-empty and healthy; otherwise spread, with a warning log. It never fails, so the worker starts without Maia.

The model id is stored with every estimate, so spread and Maia estimates are never mixed silently.

### Service (`services/maia`)

- FastAPI on `python:3.12-slim`, CPU-only torch (`2.8.0+cpu`, from the PyTorch CPU index; every other package only from PyPI). The full transitive set is pinned exactly in `requirements.lock` and installed with `--no-deps`, then `pip check`. Every pin was at least 7 days old on 2026-11-21 (three newer resolver picks were replaced: `certifi 2026.10.23`, `filelock 4.2.10`, `soupsieve 4.1`).
- The model is loaded once at startup. `MAIA_MODEL` = `rapid` (default) or `blitz`; both checkpoints are in the image. `MAIA_THREADS` (default 2) sets torch's thread count; inference is serialised by a lock.
- Elo is clamped to [1000, 2100], which covers Maia-2's buckets (`<1100` … `>=2000`); an unknown Elo becomes 1500.
- At most 256 items per request (257 → 422). A bad FEN or a position without legal moves gives a per-item `{"error":"invalid_fen"}`.
- Compose: service `maia` under profile `maia`, `127.0.0.1:58080:8080`. The Rust side reads `MAIA_URL` (empty by default = spread). `rust-dev` already passes `MAIA_URL` through; the `rust-worker` service (lane I) must add `MAIA_URL: ${MAIA_URL:-}` when it lands.

## Consequences

- Without Maia, difficulty still exists (spread-v1) but is coarse: it measures how many moves keep the evaluation, not how human players see them.
- The `maia` image is large (2.6 GB: CPU torch and its dependencies 1.07 GB, the two checkpoints 559 MB) and its build depends on Google Drive being reachable.
- The real-model smoke (start position's top move in `e2e4, d2d4, g1f3, c2c4`) and `maia_live_contract` against a running `maia` were not run in Task 24 (starting the service, which publishes host port 58080, was not permitted in the lane). They are pending for the controller or lane I.
- `maia_live_contract` (in `enrich/tests/difficulty.rs`) runs only when `MAIA_URL` is set; with `IN_DOCKER=1` an unreachable Maia then fails the test.
