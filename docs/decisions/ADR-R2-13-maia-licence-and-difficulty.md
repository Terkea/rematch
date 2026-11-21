# ADR-R2-13. Maia licence check and difficulty providers

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Rust port of TS M2's ADR 0033 (Task 21), done as R2 Task 24; revised in fix round 1 by the owner's direction that difficulty comes only from Maia.

## Context

Move difficulty (`RawMoveFeatures.difficulty = {pBest, pOk, model}`) tells coaching whether a missed move was one a player of that rating finds. The best estimator is Maia-2, a neural network that predicts human moves per Elo. Using it in a commercial product needs both the code and the weights to grant commercial use (licensing §14). The plan made the licence check the gate: compatible gives branch A (the provider plus `services/maia`); anything non-commercial, unlicensed, AGPL or unclear gives branch B (no Maia service).

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

Runtime dependencies (installed unmodified in the `maia` image, from `services/maia/requirements.lock`): `chess` 1.10.0 is **GPL-3.0+**; `certifi` and `tqdm` are MPL-2.0 (tqdm MPL-2.0 AND MIT); `typing_extensions` and `backports.zstd` are PSF-2.0; everything else is MIT, BSD-2/3-Clause or Apache-2.0 (torch, numpy, pandas, fastapi, starlette, uvicorn, pydantic, requests, gdown, einops, opentelemetry-api, annotated-doc, …).

### Outcome: compatible, branch A

- The code is MIT. The weights carry no terms of their own; the project's only licence statement says Maia-2, the model, is MIT, and the weights are distributed through the MIT package with pinned hashes. They were trained on CC0 data. Nothing anywhere is non-commercial, AGPL or "all rights reserved".
- python-chess (GPL-3.0+) runs only inside the `maia` container: an unmodified library in a separate server process that we do not distribute, the same as Stockfish (licensing §14, TS M2 Task 21 Step 1). Our own service code (`services/maia/app`) does not import `chess`; it calls `maia2.inference` and treats its `ValueError` as `invalid_fen`. No copyleft code is in the repository.
- The weights are downloaded at image build into `/opt/maia2_models` (maia2 verifies the SHA-256) and are never committed.

Residual risk, accepted: the weights' licence rests on the README's project-wide statement rather than a licence file shipped with the checkpoints. If the authors ever publish different weight terms, unset `MAIA_URL` (every position becomes `pending_maia`, no code change) and revisit this ADR. If the image is ever distributed to third parties (it is not: private beta, ADR 0041), the GPL-3.0 obligations of python-chess and the MPL-2.0 notices apply to that image and need a fresh review.

## Decision

### Difficulty comes only from Maia (owner direction, fix round 1)

The owner ruled during Task 24's review: "we must only use maia or stockfish, no simple engine based estimates." The `spread-v1` estimator from the TS plan (difficulty from the MultiPV win% spread) is therefore deleted: there is no spread code, no fallback estimator and no selection between providers. A test (`the_only_difficulty_provider_is_maia`) scans the crate sources: `MaiaDifficulty` is the only `DifficultyProvider`, and no source mentions a spread estimator.

When Maia cannot give a value, nothing is guessed, nothing blocks, and Stockfish evaluations are unaffected. Each position gets a `DifficultyOutcome` (`enrich::difficulty`), serialised as below:

| outcome | JSON | when | later |
|---|---|---|---|
| `Ok` | `{"status":"ok","difficulty":{"pBest","pOk","model"}}` | Maia answered | `model` is always the Maia model id from `/healthz` |
| `PendingMaia` | `{"status":"pending_maia","reason":…}` | `maia_not_configured` (`MAIA_URL` unset or blank), `maia_unavailable` (connection refused, DNS, non-2xx including 5xx, `/healthz` failing, or skipped after an earlier failure in the same call), `timeout`, `invalid_response` (the whole answer, or only this item, failed validation) | `difficulty = None`; re-enriched once Maia is healthy (Task 25 hand-off) |
| `NotApplicable` | `{"status":"not_applicable","reason":…}` | `opening_ply`, `invalid_fen`, `no_legal_moves` (mate or stalemate), `no_best_move` | `difficulty = None`; final, never pending |

### Opening plies

Maia-2 is not trained on the first ten half-moves. `maia2/configs/maia2-training.yaml` sets `first_n_moves: 10`, and `maia2/main.py` `process_per_game` keeps a move only `if i >= cfg.first_n_moves` (0-based half-move index). Its opening output is unreliable. On the live service the start position gave `g1f3 0.33, b1c3 0.24, b1a3 0.076, d2d4 0.075`, with `e2e4` not in the top four. So a position whose half-move index (from the FEN: `2 × (fullmove − 1) + (black to move)`) is below `MAIA_FIRST_PLY = 10` is `not_applicable/opening_ply`. It is never sent to Maia and never marked pending. An unparsable FEN is not an opening ply; the service rejects it as `invalid_fen`.

### Runtime behaviour when Maia is down

- `select_difficulty_provider(maia_url) -> DifficultyEstimator` never fails and does not touch the network. The estimator connects on first use (`GET /healthz`, 2 s connect timeout).
- Down at start-up: every estimable position in the call is `pending_maia/maia_unavailable`, and the next call probes `/healthz` again. Maia coming back is picked up without a restart.
- Down mid-call: batches already answered keep their Maia values. The failing batch is `pending_maia` with that failure's reason (`maia_unavailable` or `timeout`). The rest of the call is marked pending with the same reason, without another request, so one outage costs at most one timeout per call. The client is then dropped, and the next call re-probes `/healthz`.
- A validation failure on one item makes only that item `pending_maia/invalid_response`. The others stand.

**Hand-off to Task 25.** Store the outcome per position, so the status and reason sit next to the Stockfish features. Re-enrich the `pending_maia` positions once Maia is healthy again; Task 25 or the reconciler decides when. `not_applicable` positions are final.

### Client (`backend/crates/enrich/src/difficulty`)

- `MaiaDifficulty::connect(url, timeout = 10 s, batch_size = 64)` uses a 2 s connect timeout (`DEFAULT_CONNECT_TIMEOUT`) and reads the model id from `GET /healthz` (`maia2-<rapid|blitz>-<version>`, e.g. `maia2-rapid-0.11.0`). Requests are `POST /v1/predict {items:[{fen, eloSelf, eloOppo}]}`, sent in batches of at most 64 (130 inputs = 3 requests).
- Per-item validation of the answer: each item is parsed on its own. A probability map is accepted only when it is non-empty, every key looks like a UCI move (`[a-h][1-8][a-h][1-8][qrbn]?`), every value is finite and in `[0, 1]`, and the values sum to `1 ± 0.01`. Anything else, including an unknown `error`, is `invalid_response` for that item. An item-count mismatch or an undecodable body fails the batch.
- Scoring: `p_best = probs[best_uci]` (0 when Maia gives it no mass). Lines are deduplicated by move, keeping the highest win%. `p_ok = Σ probs[m]` over the lines within 5 win% points of the best line (inclusive), clamped to `[p_best, 1]`; without lines, `p_ok = p_best`.
- `MaiaDifficulty` also implements the Task 1 `DifficultyProvider` trait (`None` for every non-`Ok` case, `Err(Unavailable)` for a failed call). Production code uses `DifficultyEstimator`, which keeps the reasons.

### Service (`services/maia`)

- FastAPI 0.162.4 on starlette 1.10.0, which is not affected by CVE-2025-54121 (fixed in 0.47.2) or CVE-2025-62727 (fixed in 0.49.1). Served by uvicorn, with the docs and OpenAPI endpoints off.
- The model is loaded once at start-up. `MAIA_MODEL` is `rapid` (the default) or `blitz`, and both checkpoints are in the image. `MAIA_THREADS` (default 2) sets torch's thread count, and inference is serialised by a lock.
- Input:
  - pydantic models with `extra="forbid"`, `StrictStr` FEN (≤ 100 chars) and `StrictInt | None` Elo (`"1500"`, `1500.0` and `true` are 422);
  - at most 256 items (257 → 422);
  - a pure-ASGI middleware rejects bodies over 64 KB with 413, both by `Content-Length` and while streaming chunked bodies. A full 256-item batch is about 25 KB.
- FENs are validated up front by our own rules: 6 fields, 8 ranks of 8 squares, one king per side, no pawns on the back ranks, and well-formed side, castling, en-passant and counters. Failures give `invalid_fen` without reaching the model. Mate and stalemate give `no_legal_moves`.
- The model's output is checked (finite, `[0, 1]`, sum `1 ± 0.01`) and renormalised to sum to 1, because maia2 rounds to 4 decimals. Anything else is `model_error`.
- Elo is clamped to [1000, 2100], which covers Maia-2's buckets (`<1100` … `>=2000`). An unknown Elo becomes 1500.
- `services/maia/app` does not import python-chess.

### Supply chain and image

- `requirements.lock` (43 runtime pins) and `requirements-test.lock` (6 test-only pins) carry one `--hash=sha256:` per pin. That is the exact wheel for linux x86_64 CPython 3.12, `torch==2.8.0+cpu` from the PyTorch CPU index included.
- They are installed with `pip install --require-hashes --no-deps --only-binary=:all:`, then `pip check`. With every file hash-pinned, the extra PyTorch index cannot substitute a package.
- Every pin was at least 7 days old on 2026-11-21. Resolver picks that were too new were replaced: certifi 2026.10.23, filelock 4.2.10, soupsieve 4.1, annotated-doc 0.0.6.
- Base image `python:3.12-slim@sha256:7a8b475003c4fe15a2cd4e55e5cfc2f3560bdc9333d624f24cdd6d4340fd7a17`.
- Multi-stage build:
  - `deps` stage: venv and weights;
  - `test` stage: deps plus pytest and httpx plus `tests/`;
  - `runtime` stage, the compose target: the venv, the weights and `app/` only, with no pytest, httpx or tests.
- `/srv` is root-owned and not writable by the `maia` user (uid 10001). Compose also runs the container `read_only: true` with a tmpfs `/tmp` (`HOME=/tmp`).
- The weights are downloaded at image build into `/opt/maia2_models`, and maia2 verifies their SHA-256. They are never committed.

### Network and ports

- Compose service `maia` (profile `maia`, build target `runtime`) is attached only to `maia_internal`, a network with `internal: true`. It has no egress, which was verified with `pypi.org` (DNS fails) and `1.1.1.1` / `8.8.8.8` (network unreachable). `rust-dev` joins `maia_internal` as well as `default`; the `rust-worker` (lane I) must join it when it lands, with `MAIA_URL: ${MAIA_URL:-}`.
- `docker-compose.yml` publishes no host port for `maia`. The opt-in overlay `docker-compose.ports.yml` adds `127.0.0.1:58080:8080`, the same pattern as Postgres and Redis. Docker cannot publish a port from an internal-only network, so with the overlay `maia` also joins `default`, which has egress, for the main dev stack only. Parallel and CI projects (`COMPOSE_FILE=docker-compose.yml`) keep it internal-only.

## Consequences

- Positions without Maia have no difficulty. Coaching must treat `pending_maia` and `not_applicable` as "unknown", never as easy or hard. The first ten half-moves of every game have no difficulty.
- Observed on the live service (CPU, 2 threads):
  - a 64-item batch took about 5.4 s, inside the 10 s timeout but not by much;
  - the recapture in `maia_live_contract` scored `p(f1d1) = 1.0`;
  - castling (`e1g1`) went from 0.18 at Elo 1000 to 0.08 at Elo 2100 (L1 distance 0.91 between the two distributions).
  A busy service could hit the timeout; those batches become `pending_maia/timeout` and are retried, but raising `MAIA_THREADS` or lowering the batch size is the knob if that happens often.
- The `maia` image is large: about 2.6 GB, of which CPU torch and its dependencies are about 1.07 GB and the two checkpoints 559 MB. Its build depends on Google Drive being reachable.
- `maia_live_contract` (`enrich/tests/difficulty.rs`) runs when `MAIA_URL` is set; with `IN_DOCKER=1` an unreachable Maia then fails it. It checks:
  - the model id;
  - a middlegame recapture as the top move with p ≥ 0.5;
  - finite, in-range probabilities summing to 1 ± 1e-3;
  - keys that are legal canonical UCI for the side to move (black to move, `e1g1`, `e8g8`, `e7e8q`, checked with `chess-core`);
  - `None` for a garbage FEN, mate and stalemate;
  - a 64-item batch inside the timeout;
  - that Elo changes the distribution.
  To run it: `COMPOSE_FILE=docker-compose.yml docker compose -p <ci> --profile maia up -d --wait maia`, then `... run --rm -e MAIA_URL=http://maia:8080 rust-dev cargo test --locked -p enrich --test difficulty maia_live_contract`.
