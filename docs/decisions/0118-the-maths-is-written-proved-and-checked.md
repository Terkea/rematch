# 0118. The maths is written, proved and checked

Date: 2026-11-28. Status: accepted (lane `mathtool`, branch `feat/math-tooling`, the owner-approved plan "the maths of every experiment and algorithm, written out and checked"). Builds on ADRs 0096, 0104, 0105, 0106, 0107 and 0109.

## Context

The lab never showed its maths. Formulas sat in prose as unicode (`R = Σ w·y / Σ w·x`, `σ² = s² / (a (2 − a))`), with no derivations and no proofs, and neither GitHub nor the site rendered any TeX. A reader could not check where a standard error, a power figure or a bias term came from, and nothing checked that the formula in the prose was the one `analysis.py` computed. The owner asked for the maths of every experiment and every algorithm version, written out with proofs, shown on GitHub and on the site, and checked as part of the benchmark.

On the same day a separate bug surfaced: `python -m rematch_lab.experiments check` ran `analysis.py` twice whenever an experiment's external inputs were on the machine, even when no `results.json` was committed. That started EXP-0029's confirmatory run before its sizing commit.

## Decision

1. **Every experiment has `math.md`**, a file apart from `preregistration.md` (whose sections 0 to 10 freeze with the results), with six fixed headings from `docs/experiments/MATH-TEMPLATE.md`:
   1. Notation: every symbol, its unit and its source;
   2. Model: the data-generating assumptions;
   3. Estimators: each one as a formula, with the `analysis.py` function that computes it;
   4. Derivations and proofs: numbered results, each with a full proof;
   5. Numbers: every number the decision quotes, recomputed, with its `results.json` or `power.json` key;
   6. Assumptions that can fail, and the sensitivity that covers each.

   Sections 1 to 4 are committed before the freeze tag (`exp-nnnn-freeze`) and before any result; sections 5 and 6 with the decision.
2. **GitHub's maths syntax, one source for both places.** `$…$` inline; display maths in a ` ```math ` fence (preferred: it renders reliably on GitHub and nothing in it is read as Markdown) or between `$$` lines.
3. **`math_check.py` checks it**, from `lab/templates/math_check.py`, in the lab image. Four local checks:
   1. symbolic: each derivation's two sides, subtracted, simplify to 0 with sympy (a dense numeric grid where sympy cannot settle it);
   2. simulation: each proof's model, simulated from a fixed seed with a known truth, reproduces the derived bias, variance, coverage, power and error rates within the simulation's standard error, which catches a proof that is right about the wrong model;
   3. numbers: every number in `decision.md`, `summary.md` and the README, recomputed from the formulas and the committed `results.json` and `power.json`;
   4. a fresh critic, given the experiment's files but not `math.md`, re-derives the key results blind, and `review.md` compares the two.

   The shared lemmas (Holm's family-wise error control, Beta-binomial conjugacy, the rating-memory luck share, the window-mean noise independence, Bonferroni or Benjamini where used) get machine-checked proofs in Lean 4 with mathlib in `lab/lean/`, built by `lake build` in a pinned container; a `math.md` names the theorem beside its prose proof.
4. **The lab check enforces it.** `rematch_lab.experiments` reports `[math]` problems: no `math.md`, a missing heading, an empty section that is due, no `math_check.py`, or one that does not exit 0 (it runs in the experiment folder, with a 30-minute limit). An experiment that is frozen, running or has results owes sections 1 to 4; one with a `decision.md` owes all six. `REMATCH_MATH_RULE=warn`, the default, prints them as warnings; `fail` makes them problems. The switch to `fail` comes in its own commit once every experiment (EXP-0001 to EXP-0030) and `algorithm/MATH.md` are backfilled, so CI's lab check and the arena then fail without the maths.
5. **The check reruns only committed results.** `check` runs `analysis.py` only when `results.json` is committed (tracked by git, or present outside a git checkout); otherwise it prints "not rerun: no results committed". A regression test holds it.
6. **sympy 1.14.0** (with mpmath 1.3.0) joins the lab, pinned exactly, older than 7 days, under the lock's existing `exclude-newer` (`scripts/check-pypi-age.mjs`).
7. **The site renders the same maths.** `remark-math` 6.0.0 and `rehype-katex` 7.0.1 with `katex` 0.16.47, pinned exactly and checked by `scripts/check-npm-age.mjs`, run in Astro's `unified()` processor (`site/scripts/math-config.mjs`). A small remark plugin (`site/scripts/remark-math-fence.mjs`) turns ` ```math ` fences into display maths, so they never depend on the code highlighter. `escapeProse` and the link rewriter skip `$…$`, `$$…$$` and ` ```math ` spans, and escape a dollar left in prose, since a price pair such as "$9.99/month and $71.99/year" is not maths on GitHub either. Each experiment's `math.md` becomes `/docs/experiments/exp-nnnn/math/`, linked from the experiment page, and `algorithm/MATH.md` becomes `/docs/algorithm/math/` once it exists.
8. **KaTeX's fonts are an exception to "our typefaces only"** (DESIGN.md, Typography). Literata and Schibsted Grotesk have no maths glyphs; setting TeX in them would draw wrong symbols and spacing. The exception covers the site's maths only. KaTeX's stylesheet and fonts are bundled from `node_modules` by the site build, never fetched from a CDN (ADR 0096), and `site/src/styles/math.css` fits them to the page with tokens only: formulas inherit `ink`, display formulas scroll sideways on a phone, and a formula KaTeX cannot parse shows its source in `ink-soft`, never pen red.
9. **The scientists' skills do the maths.** The vendored science skills gain a "do the maths" note (`statistical-analysis`, `sympy`) and the critic's maths checklist: check every proof, re-derive the key results blind, run `math_check.py` (`scientific-critical-thinking`, `peer-review`). These are Rematch's own additions, marked in each skill's vendoring note and in `.claude/skills/science/SOURCE.md`.
10. **Third-party skills are vendored after review only.** The `sympy` skill from K-Dense's Scientific Agent Skills is vendored at the pinned commit (merged 2026-11-28, `SOURCE.md`). `statistical-power`, `statistical-analysis`, `hypothesis-generation`, `peer-review` and `scientific-critical-thinking` were vendored with ADR 0106; re-vetting them against the newer upstream is pending in another lane. Their scripts run only in the lab container, never on the host, and anything that review finds doubtful is left out and named in an amendment here.

## Options considered

1. **Maths inside `preregistration.md`.** Rejected: the freeze locks sections 0 to 10, so the numbers of section 5, which need the results, could not be added without a deviation, and a correction to a proof would read as outcome switching.
2. **`$$` blocks only.** Rejected as the recommendation: GitHub reads Markdown inside `$$` in some places (an underscore pair can become emphasis), and a ` ```math ` fence is never parsed. Both render on the site.
3. **MathJax, or KaTeX from a CDN.** Rejected: MathJax renders in the browser and is heavier; a CDN breaks ADR 0096's "no third-party requests". KaTeX renders at build time to HTML and CSS.
4. **A Lean proof for every result.** Rejected for now: most results are short algebra that sympy and a simulation check well. Lean is kept for the lemmas many experiments share, where one machine-checked proof serves them all.
5. **Fail at once.** Rejected: every committed experiment would fail CI until backfilled. Warn first, backfill, then fail.

## Consequences

- An experiment's maths is public, checked by a script in CI, and its numbers cannot drift from `results.json` unnoticed.
- The lab image grows by sympy and mpmath (about 7 MB). The site ships KaTeX's CSS (about 23 KB) on every docs page; its fonts load only where a formula uses them.
- Writing an experiment costs more: the proofs and `math_check.py` before the freeze. That cost is the point: a power figure or an interval nobody can derive is not evidence.
- The site build now reads `$` as maths. Prose prices are escaped by the content sync; a hand-written page under `site/src/content` must write a price as `\$`.

## Amendment 1 (2026-11-28): the Lean project

Decision 3's shared lemmas are proved in `lab/lean/` (branch `feat/lean-lemmas`). The owner chose machine-checked proofs over prose for them.

1. **Pins.** Lean `v4.36.0` and mathlib's tag `v4.36.0` (commit `fc4f59e5`), both from 2026-11-16, with mathlib's own dependency commits in `lake-manifest.json`. elan is v4.3.0 (2026-11-17), checked by sha256, and `debian:bookworm-slim` is pinned by digest. Each was at least 7 days old when pinned. An upgrade moves to a newer dated tag under the same rule.
2. **Theorems.** `Rematch.holm_fwer`, `Rematch.bonferroni` (with `bonferroni_fwer` and `union_bound`), `Rematch.beta_binomial_posterior`, `Rematch.luck_share` and `Rematch.window_mean_independence`. `lab/lean/README.md` states each one in words. Holm is stated over a measure, with valid p-values and no dependence assumption. It is defined without sorting, and the README explains why that matches the sorted procedure. Beta-binomial uses mathlib's `beta` and `betaPDFReal`. The window means use mathlib's `covariance` and `variance`. Benjamini-Hochberg is not proved yet: no committed experiment uses it.
3. **The check.** `lake build` also builds `RematchTest/Axioms.lean`. That file fails the build if any theorem depends on an axiom other than `propext`, `Classical.choice` and `Quot.sound`, so a `sorry` fails it too. `docker build -t rematch-lean:local lab/lean` runs the whole thing. `.github/workflows/lean.yml` runs it on changes to `lab/lean/**` and every Sunday. It uses a read-only token, actions pinned by SHA and a cache of `.lake/packages` keyed on the manifest.
4. **Citation.** A `math.md` cites a theorem as "Machine-checked: `lab/lean/Rematch/<File>.lean` theorem `Rematch.<name>`", beside its prose proof, and only when the theorem's hypotheses match the experiment's model.
