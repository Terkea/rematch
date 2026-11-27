# The algorithm lab

The lab is where we test Rematch's algorithm the way a scientist would: a question and its answer are written down before anything runs, the real Rust algorithm produces the numbers, and every result, negative ones included, stays in this repository. Python here designs, sizes and analyses the experiments; it never computes a recommendation itself, and nothing in `lab/` runs in the product (ADR 0106).

In a nutshell -

1. **One implementation.** `rematch_lab` calls the `cli` binary built from `backend/`, through a versioned JSON interface (`cli lab analyse|profile|rank|...`, schema `lab-v1`). The few rules that live in the web app (the weekly focus, the trainer's sources) are called the same way, through a Node step that runs the app's own TypeScript (ADR 0115). A finding that should change the product is made in Rust as a new algorithm version (ADR 0104), and the lab re-runs to confirm it.
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
| `rematch_lab.tag_motifs(lines)` | `cli lab tag-motifs` | positions, setup moves and lines | each line's `best` analysis (the swing and all sixteen motif tags, the six the product hides included, ADR 0112) from the production `derive_move_features` (EXP-0007) |
| `rematch_lab.trajectory(players)` | `cli lab trajectory` | rated games (the stored rating, the result, the analysis), the top weaknesses, a seed, and optionally the rating `rule` (`TrajectoryRequest.rule`: `peak-bootstrap`, `memory-halves` or `memory-before`, EXP-0030; none runs production's) | the coach's `trajectory` (ADR 0089) before its player-visible cut: the peak, the "then" and "now" windows and each verdict; `kind == "falling"` is "worse than your peak" (EXP-0015), which the product withholds since `algo-5.0.0` (ADR 0116) |
| `rematch_lab.timeline(players)` | `cli lab timeline` | series points (UUID game ids), the peer benchmark and the evaluation baselines | each series' state, `P_imp` and the `weakness_state_history` rows of the build's `evaluate_all`, `tl-v1` (EXP-0014) |
| `rematch_lab.focus_target(items)` | `lab-bridge focus-target` | a weakness's rate and the peer median the app would offer | the weekly target, `chooseTarget` of `apps/web/src/server/focus/goal.ts` (EXP-0011) |
| `rematch_lab.focus_progress(items)` | `lab-bridge focus-progress` | events, chances and games of a focus, its target | the verdict and the outcome, `focusProgress` of `goal.ts` (EXP-0011) |
| `rematch_lab.trainer_sources(candidates)` | `lab-bridge trainer-sources` | a player's positions with their sources | whether each makes a drill, its source and whether it is held out: `keepCandidate`, `primarySource` and `isHeldOut` of `apps/web/src/server/train/model.ts` (EXP-0012) |

`cli lab ...` is the Rust `cli`. `lab-bridge ...` is `$REMATCH_WEB_BRIDGE`: in the lab image, Node running `/usr/local/lib/rematch/lab-bridge.mjs`, a bundle of `apps/web/scripts/lab-bridge.ts` built from the committed lockfile (ADR 0115); from a checkout, `pnpm --filter @ci/web lab-bridge <step>`. It calls the exported functions of `goal.ts` and `model.ts` and nothing else: no database, no Next.js, no clock. A test (`tests/test_lab_v2.py`) replaces both binaries with a stand-in and checks that every wrapper returns the stand-in's answer, so the Python never computes a rule itself.

`profile`, `rank` and `bench` take `variant=` (`ALGO_VARIANT`, ADR 0111): `profile` and `rank` compute every estimate under that prior rule (`champion`, the production rule; `challenger`, EXP-0006's, production since `algo-2.0.0`; `algo-1`, the rule of 1.x; or `dev:<options>`), and `bench` runs it side by side with production on the same games, in a second account. A call without one runs production's rule.

Every response carries `schema_version` and the `algorithm_id` that produced it. It is the build's manifest id (ADR 0105), and `rematch_lab.algorithm()` returns the whole manifest (`cli algo show --json`). The schema is `schemas/lab-v1.schema.json`. The envelope and the lab's own objects are snake_case, and the algorithm's own types (`WeaknessEstimate`, `BandReferenceStat`, `RankedWeakness`, the CoachingInput's `Trajectory`, the web's `Target` and `Progress`) keep the camelCase the product stores and the app returns.

`analyse` runs the production `analyse-game` handler with the profile hook and the production `build-profile`, in a throwaway database next to `DATABASE_URL`'s, and checks itself: the profile of the facts it returns must equal the `weakness_estimates` rows the build stored (`checks[].profile_matches_build`). `profile`, `rank`, `tag-motifs`, `trajectory`, `timeline` and the Node steps are pure: the same request gives the same bytes, and any clock or seed is in the request. `rank` keeps a weakness that has no stored exemplar position, where the CoachingInput builder drops it; that is the one known difference.

## Running it

The image takes `cli` and Stockfish from a Rematch build, and bundles the web's rules with Node from the repository (the `repo` build context); it compiles no Rust.

```sh
# With the stack's rust-worker image (release build):
docker compose --profile lab build lab
docker compose --profile lab run --rm lab                                    # the contract tests
docker compose --profile lab run --rm lab python -m rematch_lab.experiments check --history
docker compose --profile lab run --rm lab python -m rematch_lab.arena --scope release --dry-run

# With a cli from a Rust lane (what CI does, .github/workflows/lab.yml):
docker build --build-context rematch=lab-ctx --build-context repo=. -t rematch-lab lab   # lab-ctx/usr/local/bin/{cli,stockfish}
docker run --rm rematch-lab pytest
```

Regenerate the lock only in the image, then check it: `docker build --target base -t rematch-lab-base lab`, then `docker run --rm -v "$PWD/lab:/lab" -w /lab rematch-lab-base uv lock`, then `node scripts/check-pypi-age.mjs`. Move `exclude-newer` in `pyproject.toml` forward with it, to seven days before the day you lock.

## Layout

- `rematch_lab/`: the bridge (`bridge.py`), the typed messages (`models.py`), the experiment rules (`experiments.py`), the champion/challenger arena (`arena.py`) and its interval rules (`stats.py`).
- `leaderboard.json`: every version's record on every live experiment, written by the arena (canonical JSON, never edited by hand); `overrides.json`: the ADR-recorded emergency overrides, empty unless one is in force; `source-reviews.json`: the files under an experiment's `SOURCE_PATHS` whose exact content an ADR found outside its measured path (ADR 0109 Amendment 1).
- `schemas/lab-v1.schema.json`: every message of the bridge.
- `fixtures/`: the contract fixtures and their golden outputs. `node fixtures/make-profile-request.mjs` and `node fixtures/make-rules-requests.mjs` rebuild the requests; `REPLAY_UPDATE=1` on the Rust test `backend/crates/cli/tests/lab.rs` rebuilds the `cli lab` goldens, and on the web test `apps/web/test/lab-bridge.test.ts` the Node step's, after a deliberate algorithm change.
- `experiments/EXP-NNNN/`: one folder per experiment ([the layout](experiments/README.md)).
- `findings.md`: the running log of every decided experiment.
- `tests/`: the contract tests and the experiment rules' tests.

## Credits

The methods follow **K-Dense**'s Scientific Agent Skills, nine of which are vendored, pinned and read in full in `.claude/skills/science/` (`SOURCE.md`). The statistics are **PyMC**, **ArviZ**, **statsmodels**, **SciPy**, **pingouin** and **scikit-learn**, the tables **polars**, the figures **Matplotlib**, and the types **pydantic**.
