# The algorithm lab

The lab is where we test Rematch's algorithm the way a scientist would: a question and its answer are written down before anything runs, the real Rust algorithm produces the numbers, and every result, negative ones included, stays in this repository. Python here designs, sizes and analyses the experiments; it never computes a recommendation itself, and nothing in `lab/` runs in the product (ADR 0106).

In a nutshell -

1. **One implementation.** `rematch_lab` calls the `cli` binary built from `backend/`, through a versioned JSON interface (`cli lab analyse|profile|rank`, schema `lab-v1`). A finding that should change the product is made in Rust as a new algorithm version (ADR 0104), and the lab re-runs to confirm it.
2. **Pre-registered.** A behaviour change (a threshold, the ranking, a detector, a prior) needs a pre-registered experiment: [the template](../docs/experiments/TEMPLATE.md) and [the protocol](../docs/evaluation.md#how-we-do-science-on-the-algorithm).
3. **Live.** A decided experiment stays live: `python -m rematch_lab.arena` re-runs it for the champion (the production algorithm) and each challenger on its registered seeds, compares them by the experiment's own rule, and records both in `leaderboard.json` (ADR 0109).
4. **Reproducible.** Python 3.12 and every package are pinned and locked (`pyproject.toml`, `uv.lock`), at least 7 days old (`scripts/check-pypi-age.mjs`); everything runs in the lab image, never on the host's Python; every random draw is seeded; CI reruns each experiment twice and compares `results.json` byte for byte.

## The bridge

| Call | Command | In | Out |
|---|---|---|---|
| `rematch_lab.analyse(players, nodes=…)` | `cli lab analyse` | games (chess.com rapid, importer shape) | facts: each binomial key's series points, the games in the window, the peer benchmark, the evaluation baselines, the band and the timeline states |
| `rematch_lab.profile(facts)` | `cli lab profile` | facts | one estimate per key with its 80% interval (`compute_estimates`) |
| `rematch_lab.rank(profiles)` | `cli lab rank` | estimates | the ranked weaknesses, the coach's focuses and their drills |
| `rematch_lab.bench("smoke", out_dir=…)` | `cli bench run --json` | a suite | the Tier 1 scorecard and every player's results (ADR 0100) |

Every response carries `schema_version` and the `algorithm_id` that produced it. It is the build's manifest id (ADR 0105), and `rematch_lab.algorithm()` returns the whole manifest (`cli algo show --json`). The schema is `schemas/lab-v1.schema.json`. The envelope and the lab's own objects are snake_case, and the algorithm's own types (`WeaknessEstimate`, `BandReferenceStat`, `RankedWeakness`) keep the camelCase the product stores.

`analyse` runs the production `analyse-game` handler with the profile hook and the production `build-profile`, in a throwaway database next to `DATABASE_URL`'s, and checks itself: the profile of the facts it returns must equal the `weakness_estimates` rows the build stored (`checks[].profile_matches_build`). `profile` and `rank` are pure. `rank` keeps a weakness that has no stored exemplar position, where the CoachingInput builder drops it; that is the one known difference.

## Running it

The image takes `cli` and Stockfish from a Rematch build; it compiles no Rust.

```sh
# With the stack's rust-worker image (release build):
docker compose --profile lab build lab
docker compose --profile lab run --rm lab                                    # the contract tests
docker compose --profile lab run --rm lab python -m rematch_lab.experiments check --history
docker compose --profile lab run --rm lab python -m rematch_lab.arena --scope release --dry-run

# With a cli from a Rust lane (what CI does, .github/workflows/lab.yml):
docker build --build-context rematch=lab-ctx -t rematch-lab lab   # lab-ctx/usr/local/bin/{cli,stockfish}
docker run --rm rematch-lab pytest
```

Regenerate the lock only in the image, then check it: `docker build --target base -t rematch-lab-base lab`, then `docker run --rm -v "$PWD/lab:/lab" -w /lab rematch-lab-base uv lock`, then `node scripts/check-pypi-age.mjs`. Move `exclude-newer` in `pyproject.toml` forward with it, to seven days before the day you lock.

## Layout

- `rematch_lab/`: the bridge (`bridge.py`), the typed messages (`models.py`), the experiment rules (`experiments.py`), the champion/challenger arena (`arena.py`) and its interval rules (`stats.py`).
- `leaderboard.json`: every version's record on every live experiment, written by the arena (canonical JSON, never edited by hand); `overrides.json`: the ADR-recorded emergency overrides, empty unless one is in force.
- `schemas/lab-v1.schema.json`: every message of the bridge.
- `fixtures/`: the contract fixture and its golden outputs. `node fixtures/make-profile-request.mjs` rebuilds the fixture; `REPLAY_UPDATE=1` on the Rust test `backend/crates/cli/tests/lab.rs` rebuilds the goldens after a deliberate algorithm change.
- `experiments/EXP-NNNN/`: one folder per experiment ([the layout](experiments/README.md)).
- `findings.md`: the running log of every decided experiment.
- `tests/`: the contract tests and the experiment rules' tests.

## Credits

The methods follow **K-Dense**'s Scientific Agent Skills, nine of which are vendored, pinned and read in full in `.claude/skills/science/` (`SOURCE.md`). The statistics are **PyMC**, **ArviZ**, **statsmodels**, **SciPy**, **pingouin** and **scikit-learn**, the tables **polars**, the figures **Matplotlib**, and the types **pydantic**.
