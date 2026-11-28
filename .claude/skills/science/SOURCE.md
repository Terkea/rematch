# Vendored science skills

Ten skills from **K-Dense** Scientific Agent Skills, pinned to one upstream commit and read in full before they were copied. They guide the experiments in `lab/` (ADR 0106); the lab itself drives the Rust algorithm and never re-implements it.

- Source: https://github.com/K-Dense-AI/scientific-agent-skills
- Commit: `c21d13b3cc2894680229ba9959a53281acb9200e` (committed 2026-11-23T09:18:26Z)
- Vendored: 2026-11-23, lane `science`, branch `feat/science-lab`
- Repository licence: MIT, Copyright (c) 2025 K-Dense Inc. (`LICENSE-K-Dense.md`, copied unchanged)
- Paper: Kassis, T., Agarwal, V., He, Y., Patel, D., & Brueckner, A. M. (2026). Scientific Agent Skills: A Library of Procedural Knowledge for Research Agents. arXiv:2609.00065.

| Skill | Version | Licence declared by the skill | What we use it for |
|---|---|---|---|
| `statistical-analysis` | 2.0 | MIT | test choice, assumption checks, effect sizes with intervals; it rejects post-hoc power |
| `statistical-power` | 1.3 | MIT | a-priori sample sizes and Monte Carlo power for clustered designs (EXP-0001) |
| `experimental-design` | 1.5 | MIT | randomisation, blocking, pseudoreplication |
| `pymc` | 2.0 | Apache-2.0 | hierarchical models, R-hat and ESS, PSIS-LOO (EXP-0003) |
| `statsmodels` | 1.5 | BSD-3-Clause | GLMs, mixed models, cluster-robust errors (EXP-0004) |
| `scikit-learn` | 1.5 | BSD-3-Clause | grouped and chronological splits, leakage control (EXP-0004) |
| `scientific-critical-thinking` | 1.5 | MIT | the critic's review of a pre-registration and its result |
| `peer-review` | 2.4 | MIT | the same review, written as a structured report |
| `hypothesis-generation` | 2.4 | MIT | H0 and H1, rival explanations, discriminating predictions |
| `sympy` | 1.5 | BSD-3-Clause (SymPy's) | symbolic checks of every derivation in an experiment's `math.md` (ADR 0118); vendored 2026-11-28 from the same commit, read in full by a fresh reviewer |

The licence column is the skill's own `license:` field. The pymc, statsmodels, scikit-learn and sympy skills declare the licence of the library they document; the skill text is K-Dense's work under the repository's MIT licence. Each skill keeps its front matter unchanged apart from the `compatibility` line.

## What was read

Every `SKILL.md`, every file under `scripts/` and `assets/`, and a scan of every file under `references/` for install commands, network calls, credentials and external services. The upstream `tests/` folder was not vendored.

- The Python scripts import only the standard library, NumPy, pandas, SciPy, statsmodels, Matplotlib, scikit-learn, ArviZ, PyMC and pydoe. None opens a socket, calls an HTTP client, reads an environment variable or a `.env` file, or starts a subprocess. They write only to paths their caller names (figures, CSV summaries, the review scaffolds).
- The scikit-learn examples load the bundled Iris and breast-cancer datasets, which ship inside the library; nothing is downloaded.

## What was changed

Each changed `SKILL.md` starts with a "Rematch vendoring note" that lists its changes.

1. **Install steps removed.** Every `uv venv`, `uv pip install` and `uv run --with` block in a `SKILL.md` is replaced by a pointer to the lab container, which pins the packages (`lab/pyproject.toml`, `lab/uv.lock`). In the scikit-learn references the install lines are commented out.
2. **No network citation fetch.** Each skill told the agent to fetch the arXiv record before citing the paper; that paragraph now says to cite it as written.
3. **No remote service.** `scientific-critical-thinking` offered figures through the scientific-schematics skill, which sends the prompt to OpenRouter and needs `OPENROUTER_API_KEY`. That step is removed; figures are drawn with Matplotlib.
4. **The `compatibility` line** says the skill runs in the lab container, followed by the upstream text.

No skill was dropped: none depends on a remote service for its main use. Two scripts need packages the lab does not pin yet, and say so in their notes: `experimental-design/scripts/doe_designs.py` (pydoe) and the survival extensions of `statistical-power` (lifelines).

## Updating

The monthly sweep compares this commit with upstream `main`. An update is a pull request that bumps the commit here, re-reads every changed file, and re-applies the four changes above.
