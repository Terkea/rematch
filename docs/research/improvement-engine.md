# Improvement Engine: research notes and proposed spec

Status: research draft, 2026-11-21
Scope: what evidence says about chess improvement, what game signals separate rating bands, how coaches diagnose, how to classify moves and detect motifs from engine output, and how to build a measure → diagnose → prescribe → practise → re-measure → adapt loop. The last section is an implementable spec.

How to read the evidence labels:

- **Strong**: peer-reviewed, large sample, replicated or reanalysed.
- **Moderate**: peer-reviewed but correlational or single-study, or very large observational datasets with obvious confounds.
- **Weak**: blog or vendor analysis, small or self-selected samples, unclear methods.
- **Expert consensus**: what strong coaches broadly agree on, without controlled data behind it.

The overall honest conclusion: there is **no randomized controlled trial** showing that any specific chess training method causes rating gain in adults. What we have is correlational evidence that structured, effortful, individualised study ("deliberate practice") predicts skill; strong general learning-science evidence (spacing, retrieval practice, feedback, appropriate difficulty); and a lot of coach consensus. The product should therefore promise a *rigorous process with honest measurement*, not an outcome.

---

## 1. What actually improves chess rating

### 1.1 Deliberate practice and serious study (Moderate to Strong for correlation; no causal trials)

- **Charness, Tuffiash, Krampe, Reingold & Vasyukova (2005)**, "The role of deliberate practice in chess expertise", *Applied Cognitive Psychology* 19:151-165 ([doi:10.1002/acp.1106](https://doi.org/10.1002/acp.1106)). Cumulative hours of *serious solitary study* (studying openings, endgames, master games, analysing own games) was the strongest activity predictor of rating; tournament play and group activities predicted less. Defined deliberate practice as "appropriately challenging tasks chosen with the goal of improving a particular skill".
- **Gobet & Campitelli (2007)**, "The role of domain-specific practice, handedness and starting age in chess", *Developmental Psychology*; and **Campitelli & Gobet (2008/2011)** ([Murdoch repository](https://researchportal.murdoch.edu.au/esploro/outputs/journalArticle/The-role-of-practice-in-chess/991005542731507891)). Practice hours correlate strongly with skill, but individual variation is huge: some players reach master in a few thousand hours, others never do with many more. Starting age matters independently (partial r ≈ -0.40 with rating after controlling for hours). Conclusion: practice is *necessary but not sufficient* ([Gobet & Campitelli, "Deliberate practice: necessary but not sufficient"](https://researchonline.lse.ac.uk/id/eprint/102854)).
- **Hambrick et al. (2014)** meta-analytic reanalysis: deliberate practice explains roughly a third of variance in chess skill (≈34% after correction for measurement error in **Burgoyne et al. 2019**, *American Journal of Psychology*, [PDF](https://englelab.gatech.edu/articles/2019/Burgoyne_et_al_2019_AJP.pdf)). Burgoyne et al. also found serious study was a stronger predictor of current and peak rating than formal instruction or tournament play, though tournament play added unique variance.
- Counterpoint: Ericsson and colleagues argue meta-analyses underestimate deliberate practice because they lump unindividualised practice in with genuinely individualised, coach-designed practice. This argument actually supports our product thesis: *individualisation* is the claimed active ingredient.
- **Southwick, Harwell, Wright, Olsen & Ogles (2026)**, "Not All Practice Is Created Equal: Longitudinal Evidence From Over 40,000 Chess Players", *Psychological Science* ([SAGE](https://journals.sagepub.com/doi/10.1177/09567976261452568), [APS](https://www.psychologicalscience.org/journals/psychological-science/09567976261452568/)). The largest longitudinal study so far, covering Chess.com activity logs over six months. As reported in summaries (the full text was not accessible to us, so **verify before quoting numbers**): "deliberate-practice-aligned" activities (game review, lessons, puzzles grouped together) gave ≈3.6× the rating gain per hour of playing games, although >90% of user time went to games. Puzzles *alone* were reportedly **not** significantly more efficient per hour than games (p ≈ .10). This is the most directly relevant evidence for our product. Read the full paper before finalising prescription weights.

**Implication:** the strongest correlational signal is for effortful, individualised study, especially **analysing one's own games** and studying with feedback. Mindless volume, whether more blitz or more easy puzzles, has the weakest support.

### 1.2 Tactics training volume (Weak to Moderate)

- No RCT exists. Observational data:
  - Disco Chess / Woodpecker-style analysis: ~1,950 users, 431k puzzles, 60k rated games, average ≈ +35 Elo over 20 weeks, smaller gains above ~1600, transfer lagging by weeks. The authors acknowledge selection and confounding ([Lichess blog](https://lichess.org/@/DiscoChessApp/blog/woodpecker-method-training-431000-puzzles-60000-rated-games-and-35-elo/yrWsFF1N)). Weak.
  - De la Maza's "Seven Circles" (*Rapid Chess Improvement*, 2002): one person's dramatic gain, many anecdotal failures, criticised by Nunn (*Secrets of Practical Chess*, 2nd ed.). Weak ([Chess.com profile of de la Maza](https://www.chess.com/article/the-michael-de-la-maza-story), [follow-up blog](https://www.chess.com/blog/Larceny/seven-circles-follow-up)). The successor Woodpecker Method (Smith & Tikkanen) is also untested.
- Expert consensus is very strong that **below ~1600, tactics (especially safety: not hanging pieces, seeing opponent threats) dominates outcomes**. This matches the data in section 2: low-rated games are decided by large one-move swings.
- Training design principle from learning science: puzzles should be **retrieval practice with feedback at the right difficulty**, mixed by theme (interleaved) rather than blocked by theme once a theme is introduced, since a theme label gives away the answer. Interleaving evidence: Taylor & Rohrer 2010, *Applied Cognitive Psychology* 24:837-848. Testing effect: Roediger & Karpicke 2006, *Psychological Science*. Both Strong in general learning research but not tested in chess specifically.

### 1.3 Puzzle rating vs game rating (Weak, but consistent)

- Lichess community analyses report correlations of roughly r ≈ 0.68-0.74 between puzzle rating and classical/rapid/blitz rating, with individual gaps of 500+ points common ([correlation blog](https://lichess.org/@/mlatb/blog/correlation-between-rapidblitz-rating-and-puzzle-rating/H39ApaeR), [forum thread](https://lichess.org/forum/general-chess-discussion/low-correlation-between-puzzle-proficiency--rating-implications-for-chess-improvement), [puzzles-matter analysis, 24k players](https://lichess.org/@/ggSayItBack/blog/do-puzzles-matter-a-data-analysis/twAyjQDX)).
- Why the gap exists: puzzles are untimed, they announce that a tactic exists (the "puzzle prompt" effect), and they skip the threat-detection and blunder-check steps that games demand.
- **Implication:** puzzle rating is a *leading indicator of pattern knowledge*, not of in-game performance. The engine must measure transfer in actual games and use formats that remove the prompt, such as "is there a tactic here, or not?" and "find the opponent's threat".

### 1.4 Spaced repetition for openings (Strong in general, Weak chess-specific)

- Spacing effect meta-analysis: **Cepeda et al. 2006**, *Psychological Bulletin* 132:354-380, 317 experiments ([PubMed](https://pubmed.ncbi.nlm.nih.gov/16719566/)). Strong.
- Chessable MoveTrainer is spaced repetition applied to move sequences. Evidence for its benefit is vendor-affiliated or anecdotal ([Matuschak note](https://notes.andymatuschak.org/zDr94hP6bG3jJYrdYy8B5hx) on its limits for conceptual knowledge). Opening memorisation has small rating value below ~1600 (see section 2: opening-phase errors are the smallest share of lost points), while recall of *ideas and typical plans* matters more at higher levels.
- Modern scheduler: **FSRS** (power-law forgetting curve, R(t) = (1 + F·t/S)^C with F = 19/81 and C = -0.5 in FSRS v4/v5; target retention ≈ 0.9) ([Anki docs](https://docs.ankiweb.net/faqs/what-spaced-repetition-algorithm), [Implementing FSRS in 100 lines](https://borretti.me/article/implementing-fsrs-in-100-lines), [Expertium](https://expertium.github.io/Algorithm.html)).
- **Implication:** use spaced repetition (a) for the player's own repertoire *where the player actually deviated or lost*, and (b) for own-mistake positions, which are highly personal and retrieval-friendly. Don't sell big opening courses to sub-1400 players.

### 1.5 Slow vs fast games (Moderate for the skill correlation; Expert consensus for training value)

- **Burns 2004**, *Psychological Science*: blitz performance accounted for up to 81% of variance in standard rating. Fast pattern recognition is a large component of skill ([APS](https://www.psychologicalscience.org/journals/psychological-science/j.0956-7976.2004.00699.x/)). This shows blitz is a valid *measure*, not that blitz is good *training*.
- Lichess community: blitz and classical rating correlate at r ≈ 0.66, while bullet correlates with classical much less (Weak; [forum](https://lichess.org/forum/game-analysis/bullet-vs-classical-rating?page=2)).
- Coach consensus (Heisman, Yusupov, Naroditsky and many others): play slower games (rapid 15+10 and above, or classical) for learning, and use the time to execute a thinking process. Then analyse them. Southwick et al. 2026 suggests games alone are low-efficiency per hour, so slow games are most useful as *material for analysis*.
- Anderson, Kleinberg & Mullainathan (2016/2017), "Assessing Human Error Against a Benchmark of Perfection" ([arXiv:1606.04956](https://arxiv.org/abs/1606.04956)): on 200M amateur games, using tablebase ground truth, **position difficulty predicts errors far better than player skill or time remaining**. Implication: weakness metrics must be **difficulty-adjusted**, or a player who reaches harder positions will look worse.

### 1.6 Learning science applied to drill design

| Principle | Evidence | Application |
|---|---|---|
| Retrieval practice and testing effect | Strong (Roediger & Karpicke 2006) | Drills are recall tasks with immediate feedback, not passive replay |
| Spacing | Strong (Cepeda 2006) | FSRS scheduling for own-mistakes and repertoire |
| Interleaving | Moderate-Strong (Taylor & Rohrer 2010) | After introducing a motif, mix motifs and include "no tactic" positions |
| Desirable difficulty / ~85% rule | Moderate theory (Wilson et al. 2019, *Nature Communications*, "The Eighty Five Percent Rule for optimal learning") | Target 70-85% puzzle success; select puzzle rating accordingly |
| Individualisation | Argued by Ericsson; supported by Charness 2005 framing | The whole weakness-model approach |
| Feedback specificity | Strong in general education literature | Explain *why* the move failed (refutation line, motif), not just "-3.2" |

---

## 2. Measurable signals that predict rating or differ by band

### 2.1 Public analyses

- **336M blitz moves (3+0), ~600k players** ([Chess.com blog, "Hunting for Context in 336 Million Blitz Moves"](https://www.chess.com/blog/Jordi641/hunting-for-context-in-336-million-blitz-moves)). Weak to Moderate: very large sample, but the source and engine are unstated. Blunder here means a loss of more than 100 cp.
  - Average cp loss: 85.6 at 800-899 vs 29.2 at 2800-2899. Blunder rate per move: ~24.6% vs ~7.3%.
  - Opening (moves 1-10) is the cleanest phase: ACPL ≈ 28.8, blunder rate 6.6%. Most loss happens in moves 11-30 and later.
  - **Blunder rate is much higher once someone has an advantage**, whether converting or defending: 1200-1299 rises from 14.7% in undecided positions to 24.5% with an advantage. 2000-2099 rises from 9.3% to 19.6%. 2800+ rises from 4.4% to 12.7%. Note that raw cp-loss inflates at large evals, which partly explains this; win%-based metrics fix it (section 4).
  - Time: every band degrades below 30 s and again below 10 s. 2000-2099 goes from 12.0% to 22.7% blunders. 800-899 goes from 21.4% to 32.4%.
- **Anderson et al. 2016**: difficulty dominates. Time pressure matters but less than difficulty.
- **Maia / Maia-2** (McIlroy-Young, Sen, Kleinberg, Anderson, KDD 2020, [arXiv:2006.01855](https://arxiv.org/abs/2006.01855); Maia-2, [arXiv:2409.20553](https://arxiv.org/abs/2409.20553)): neural nets trained on Lichess games at each rating level predict *which move a human of rating R plays* (~50%+ top-1) and can predict when a human will blunder. **Useful as a difficulty estimator**: P_maia(best move | position, rating R) tells us whether peers would find the move.
- **Chowdhary, Iacopini & Battiston 2022**, "Quantifying human performance in chess" ([arXiv:2207.07780](https://arxiv.org/abs/2207.07780)): 120M games. Experts specialise and repeat openings while beginners explore more, and hot and cold streaks exist, with cold streaks longer. Moderate.
- **Regan's Intrinsic Performance Ratings** ([Regan 2012 draft](https://cse.buffalo.edu/~regan/papers/pdf/Reg12IPRs.pdf), [Regan & Haworth 2011](https://cse.buffalo.edu/~regan/papers/pdf/ReHa11c.pdf)): scaled average error maps roughly linearly onto Elo (for example, IPR ≈ 3571 - 15413·AE in one fit). Single-game IPR has an error bar of hundreds of points, and tournament-sized samples are needed for ±50. This is a good reality check on how noisy per-game accuracy is.
- **Aimchess** (now owned by Chess.com) reports six peer-relative dimensions: tactics, endgame, advantage capitalization, resourcefulness, time management, and opening ([aimchess.com](https://aimchess.com), [review](https://www.chess.com/blog/SheldonOfOsaka/aimchess-a-review)). This is the closest existing product and validates the dimension set, but the exact formulas are unpublished. Example output: "converts winning positions 63% of time, about average".
- **Lichess Insights** offers user-pivoted stats (ACPL, accuracy, result, and time by phase, piece moved, opening, material imbalance), descriptive only, with no prescription.

### 2.2 Signals ranked by expected usefulness

| Signal | Why it matters | Band where it matters most | Noise |
|---|---|---|---|
| Large-swing blunders per game (ΔW ≥ 15-20 win% points) | Decides most sub-1600 games | <1600 | Medium; counts are Poisson-like |
| Hung-piece / missed-opponent-threat rate | The specific cause of most low-level blunders (Heisman "safety") | <1400 | Medium |
| Missed own tactics (opportunity-based) | Pattern knowledge | 800-2000 | Higher; opportunities are rarer |
| Conversion rate from winning positions (W ≥ 80-85% held ≥ 3 plies) | Huge expected-points leak at all levels | All | Per-opportunity, needs ~30+ opportunities |
| Save rate from losing positions | Resourcefulness | All | Noisy; depends on the opponent's skill |
| Blunder-rate ratio under time trouble vs ample time | Time management | Blitz/rapid players, all bands | Medium |
| Time-trouble frequency (fraction of games reaching <10% clock before move 40) | Time management habit | 1200+ | Low noise (one per game) |
| Time spent vs position criticality | Spending time where it matters | 1400+ | Medium |
| Endgame accuracy and tablebase result drops | Technique | 1400+ (basics earlier) | Exact for ≤7 pieces |
| Opening: eval at ply 20-30, early losses, repertoire deviations | Opening prep | 1600+ (traps below that) | Low per line, but fragmented across lines |
| Small-loss accumulation in quiet positions | Positional understanding | 1800+ | High; needs many games |
| Performance by game-in-session and after a loss | Tilt / fatigue | All | Medium; behavioural advice only |

---

## 3. How strong coaches diagnose and prescribe

### 3.1 Dan Heisman (sub-1800 adults): safety and thought process

- "Hope chess" vs "real chess": in *real chess*, before every move you check whether the opponent's possible forcing replies (checks, captures, threats) after your candidate beat it. In *hope chess* you play and hope ([Heisman site](https://danheisman.com/tactics-and-safety.html), *Is Your Move Safe?* (2016), CJA Best Instruction book ([publisher](https://www.mongoosepress.com/catalog/books/move_safe.html))).
- His definition: a move is *safe* if the opponent has no forcing sequence that wins material or mates in response. Tactics are "the science of piece safety".
- Diagnostic: read the player's lost games. Were losses caused by (a) hanging pieces or missing threats, (b) missed tactics, (c) time management (playing too fast or too slow), or (d) something deeper? Below ~1600 it is almost always (a) or (c). His prescription: slow games, a strict thought process ("what does the opponent's move do? What are all my checks, captures and threats? Is my candidate safe?"), and lots of *easy* tactics for pattern volume. Heisman also emphasises that playing too fast in slow games is the most common time error.
- Time management rule of thumb: use most of your clock in slow games, spending more in critical positions and less in forced or obvious ones.

### 3.2 Steps Method (Brunia & van Wijgerden): structured curriculum for beginners and juniors

- Six steps, each with a trainer manual and workbooks, developed in 1987 for children and widely used in Dutch, Belgian, German, Austrian and Swiss clubs ([Lichess blog](https://lichess.org/@/NoelStuder/blog/the-chess-step-method-explained/QCCUDwgt), [New in Chess](https://www.newinchess.com/the-complete-step-by-step-method)). Unofficially, step 6 is said to reach about 2100 strength.
- Design lessons: small concepts in strict order (piece safety → double attack → pins → mate patterns → ...), large volumes of *easy* exercises per concept, and mastery before progression. This is a mastery-learning model that maps directly onto a skill graph with mastery gates.

### 3.3 Yusupov's school (about 1000-2200): balanced curriculum

- Nine volumes in three tiers (Fundamentals / Beyond the Basics / Mastery). Each chapter mixes tactics, strategy, endgames, openings and calculation, followed by tested exercises with a points threshold for passing ([series](https://www.simonandschuster.ca/series/Yusupovs-Chess-School), [review](https://lichess.org/@/InfiniteFlash/blog/build-up-your-chess-2-beyond-the-basics--chesstempo-review/OEPayaq2)). FIDE Boleslavsky Medal.
- Design lessons: interleaving across domains, scored tests with pass thresholds, and repeating failed chapters. This supports a spiral curriculum with assessment gates.

### 3.4 De la Maza (1200-2000 adults): extreme tactics volume

- 400-day plan with repeated circuits of a fixed puzzle set at increasing speed. The underlying principle (automatise common patterns) is reasonable, but the extreme claims and exclusive focus aren't supported. Use as one possible *dose* profile for tactics-dominant weaknesses, not as a program.

### 3.5 Common coach diagnostic pattern (Expert consensus, synthesised)

1. Collect 20-50 recent slow-ish games, prioritising losses and draws from good positions.
2. For each decisive mistake, classify **why** it happened (not just what): didn't see the move, saw it but misevaluated, ran out of time, didn't consider the opponent's reply, didn't know the plan or the theory.
3. Count categories, then fix the most frequent *cheap-to-fix* category first. Safety and time come before strategy.
4. Prescribe specific materials plus a thinking-process change, then review again after 1-3 months.
5. Higher levels (2000+): opening repertoire depth, calculation of long lines, endgame theory, and strategic understanding via annotated master games. Diagnosis becomes about *types of positions* the player mishandles (structures, imbalances, attack vs defence).

The engine automates steps 1-3 at scale and makes step 4's review quantitative.

---

## 4. Win-probability move classification and motif detection

### 4.1 Lichess model (open source, reproducible)

From `lila` source ([AccuracyPercent.scala](https://github.com/lichess-org/lila/blob/master/modules/analyse/src/main/AccuracyPercent.scala), [Advice.scala](https://github.com/lichess-org/lila/blob/master/modules/tree/src/main/Advice.scala), [accuracy page](https://lichess.org/page/accuracy)):

```
WinPercent(cp) = 50 + 50 * (2 / (1 + exp(-0.00368208 * cp)) - 1)      # cp from mover's POV, clamp cp to ±1000; mate → ±1000
WinningChances(cp) = 2 / (1 + exp(-0.00368208 * cp)) - 1               # in [-1, 1]

MoveAccuracy(before, after):                                          # win% from mover's POV
  if after >= before: 100
  else: clamp( 103.1668100711649 * exp(-0.04354415386753951 * (before - after)) - 3.166924740191411 + 1, 0, 100 )
         # "+1" = uncertainty bonus for imperfect analysis

GameAccuracy(color):
  windowSize = clamp((numPositions - 1) / 10, 2, 8)
  weight_i   = clamp(stdev(WinPercent over window containing move i), 0.5, 12)   # volatility weighting
  accuracy   = ( weightedMean(moveAcc, weight) + harmonicMean(moveAcc) ) / 2

Judgement by drop in WinningChances (mover POV):
  ≥ 0.3 → Blunder      (≡ 15 win% points)
  ≥ 0.2 → Mistake      (≡ 10 win% points)
  ≥ 0.1 → Inaccuracy   (≡  5 win% points)
  Mate created against mover: Blunder unless already lost (cp < -700 → Mistake, < -999 → Inaccuracy); symmetric rules for mate lost.
```

The 0.00368208 constant was fitted to Lichess game results at ~2300+ level ([discussion](https://lichess.org/@/jk_182/blog/comparing-different-ways-to-calculate-accuracy/6aeYnbh8)). The expected score at a given eval depends on rating: weaker players convert +2 less reliably. See [jk_182, "How the evaluation and clock impact results of blitz games"](https://lichess.org/@/jk_182/blog/how-the-evaluation-and-clock-impact-results-of-blitz-games/I2kRp2sk) and [Expected score of GMs by evaluation](https://chessenginelab.substack.com/p/expected-score-of-grandmasters).

### 4.2 Chess.com "expected points" model (closed)

- Move classes by expected-points loss ([support article](https://support.chess.com/en/articles/8572705-how-are-moves-classified-what-is-a-blunder-or-brilliant-move)): Best 0.00, Excellent 0-0.02, Good 0.02-0.05, Inaccuracy 0.05-0.10, Mistake 0.10-0.20, Blunder 0.20-1.00. Special labels: Brilliant (a sound piece sacrifice with extra conditions), Great (only good move / critical), Miss (failing to punish the opponent's mistake), Book.
- Key difference: the eval→expected-points curve **depends on player rating**. A +3 position is "winning" for a 2000 player but less so for a 600 player. The formula is not published.
- CAPS/accuracy is not published either. Don't attempt to replicate chess.com numbers. Present our own metrics and explain them.

### 4.3 Recommended approach for us

- Use the Lichess WinPercent curve as the **canonical, explainable** scale, since it is open and reproducible.
- Additionally fit a **rating-conditioned expected-score model** from the Lichess open database (database.lichess.org, which includes the evaluated games subset): `E(cp, R, TC, clock) = σ(a(R,TC) · cp + b·clockFeature)`, fitted by logistic regression on (eval at ply t, rating, time control, remaining time) → game result. Use it for *conversion* thresholds per band (what "winning" means for this player).
- Always compute losses as `ΔW = max(0, W_before − W_after)` from the mover's POV. This compresses differences in won or lost positions, fixing the raw-ACPL artefact noted in 2.1.
- **Measurement consistency is critical:** pin the Stockfish version, fixed nodes per position (for example 1-2M nodes, MultiPV = 3) and NNUE net. Re-analyse baseline games if the engine changes, or else "improvement" may just be engine drift.

### 4.4 Tactical motif detection from engine PVs

Reference implementation: Lichess puzzle tagger `cook.py` ([ornicar/lichess-puzzler/tagger/cook.py](https://github.com/ornicar/lichess-puzzler/blob/master/tagger/cook.py)). It works on a solution line (PV), not just one move. Its detectors include: mateIn1-5, smothered/back-rank/anastasia/hook/arabian/boden/dovetail mate, attraction, deflection, advancedPawn, doubleCheck, quietMove, defensiveMove, sacrifice, xRayAttack, fork, hangingPiece, trappedPiece, discoveredAttack, exposedKing, skewer, interference, intermezzo, pin, attackingF2F7, clearance, promotion, underPromotion, capturingDefender, plus endgame-type and length tags. Example rules:

- **Fork**: a player move after which ≥ 2 opponent non-pawn pieces are attacked that are either more valuable than the mover or hanging, and the mover isn't en prise on its square.
- **Hanging piece**: the first move captures an undefended non-pawn piece and the opponent can't recapture with an equal or higher-value piece.
- **Skewer**: a slider captures on a line through a square vacated by a more valuable enemy piece that was forced to move.
- **Discovered attack**: a check or capture by a piece other than the one that moved, along a line just vacated.
- **Sacrifice**: material is down ≥ 2 at some point in the line without promotion.
- **Advantage tag**: final cp > 600 is crushing, > 200 is advantage.

Plan to port or vendor `cook.py` (AGPL: check licence compatibility, or reimplement from the documented rules using python-chess).

> **Note (R2):** we do not port or vendor `cook.py`. The motif tagger in `profile-core::motifs` is a clean-room implementation of our own rules, written from the descriptions above (ADR-R2-3).

For **in-game error classification** (not puzzles) we need three lines per critical move:

1. **Refutation line** (why the played move was bad): PV after the played move from the opponent's side. Tag it with cook-style detectors from the opponent's perspective. Output examples: "you allowed a fork", "you hung a piece".
2. **Missed line** (what was better): PV from the best move. Tag it from the mover's perspective. Output: "you missed a skewer / mate in 2".
3. **Threat line** (null-move probe): in the position before the mover's move, let the opponent move twice (null move) and search shallowly. If the opponent then wins ≥ 2 pawns of material or mates, a *threat existed*. If the played move didn't address it (the refutation line realises the same threat), classify as **missed opponent threat**. This is the operational version of Heisman's "what does my opponent's move threaten?".

Additional derived features:

- **Depth-to-swing**: plies in the PV until material or mate swing is realised. One to two plies is a *shallow* tactic (board vision), five or more is a *deep* one (calculation).
- **Only-move flag**: MultiPV gap between best and second-best ≥ 15 win% points means the position was *critical*. This feeds time-use analysis.
- **Difficulty**: Maia-2 probability that a player of the user's rating plays the best move (or any move with ΔW < 5). An error on an easy position (P_maia(best) > 0.6) is a stronger weakness signal than an error on a hard one.

---

## 5. Designing the adaptive loop: statistics and noise

### 5.1 How noisy is rating as an outcome?

With the Elo logistic, expected score is `E = 1/(1+10^(−D/400))`. Near E = 0.5, the slope is `dE/dR = ln(10)/400 · E(1−E) ≈ 0.00144` per rating point.

The per-game score SD is about 0.5 without draws (less with draws: `Var = w + d/4 − p²`). For a performance rating over n games:

```
SE(performance rating) ≈ 0.5 / sqrt(n) / 0.00144 ≈ 347 / sqrt(n)     (Elo points)

n =  25 → ±69      n = 100 → ±35      n = 400 → ±17
```

Detecting a +50 rating change between two independent windows (two-sided α = 0.05, power 0.8) needs `SE_diff ≤ 50/2.8`, so about **750 games per window**. Conclusion: **rating is far too noisy to evaluate a 4-8 week prescription for one player**. Online ratings (Glicko-2 with RD; Lichess treats RD > 110 as provisional, [FAQ](https://lichess.org/qa/888/what-is-glicko-2-rating-volatility)) add their own lag and noise. Use rating as a long-horizon (3-12 month) trend with confidence bands only.

**Regression to the mean warning:** users often sign up after a losing streak, and cold streaks are real and longer than hot ones (Chowdhary et al. 2022). Measuring the baseline at sign-up then guarantees fake "improvement". Use a **baseline = median rating over the prior 90 days** (or the Glicko rating 60+ games back), not the current rating.

### 5.2 More sensitive outcomes: per-opportunity weakness rates

Measure each weakness as **events per opportunity**, computed on moves, not games, with difficulty adjustment. For example:

- Blunder-safety rate = (moves with ΔW ≥ 15 caused by hanging or missed threat) / (moves in positions not already decided, |W−50| < 40).
- Conversion = (winning episodes converted to a win) / (winning episodes).

Moves within a game are correlated, so apply a design effect (DE ≈ 1.3-2; estimate it from data as `1 + (m̄ − 1)·ICC`).

Example power calculation, for a baseline rate p₀ = 0.05 per eligible move with ~30 eligible moves per game and DE = 1.5:

```
SE(p) = sqrt(p(1−p) · DE / m)          m = eligible moves in window
Detect 40% relative drop (0.05 → 0.03) at α=.05, power .8:
   need ≈ 2,800 moves per window  ≈ 90-100 games per window
Detect 50% drop: ≈ 60-65 games per window
```

That is still a lot for slow-game players. Three tactics make the loop workable:

1. **Bayesian shrinkage with a peer prior.** Each player's rate is estimated as `θ̂ = (k + n₀·μ_band)/(n + n₀)` (Beta(n₀μ, n₀(1−μ)) prior from the rating-band reference, with n₀ ≈ 100-300 opportunities). Report posterior intervals and never raw rates from small n.
2. **Sequential monitoring instead of fixed windows**: Bayesian posterior comparison updated after each analysed game, using decision thresholds (section 6.6) rather than p-values. This avoids peeking problems because it reports posterior probabilities, not significance.
3. **Leading indicators** from drills, checked against lagging in-game indicators. Leading: drill accuracy, solve time, and FSRS retention. In-game: the opportunity-based rate. Lagging: rating. Then judge each layer separately: "you are now solving fork puzzles at 85% in 20 s" (fast, certain), "you are hanging fewer pieces in games: 2.1 → 1.4 per 10 games, 87% likely to be real" (medium), "your rating trend is +40 ± 60" (slow).

### 5.3 Expected-points budget: one currency for all weaknesses

For each player, compute **expected points lost per game by category**:

```
EPL_c = (1 / G) · Σ_games Σ_moves∈c  ΔW_move / 100
```

The total EPL across categories approximates how many game-points per game "leak" from errors (it overstates actual result loss, because opponents also err, but it is consistent for *ranking*). Rough rating translation: lowering score-leak by Δs points per game ≈ `Δs / 0.00144` Elo, so 0.03 points/game ≈ +20 Elo. Use this only as an *illustrative upper bound*. Never show it as a promise.

**Excess** EPL vs peers is what the prescription targets: `Excess_c = EPL_c(player) − median EPL_c(band, TC)`.

### 5.4 Choosing the prescription

Priority for weakness c:

```
Priority_c = Excess_c (shrunk posterior mean, in points/game)
           × Trainability_c(band)        # 0..1 table, from coach consensus (section 6.4)
           × Confidence_c                # P(player rate > band median | data)
           × (1 − Fatigue_c)             # down-weight if trained ≥ 8 weeks with no transfer
```

Pick the top 1-2 weaknesses as the **focus** (about 70% of training time), plus about 30% maintenance or general (mixed tactics and review of own games). Coaches and learning research both favour a narrow focus with interleaved maintenance over trying to fix everything.

### 5.5 When to change the plan

Use the posterior probability of improvement `P_imp = P(θ_now < θ_baseline − δ_min)`, where δ_min is the smallest meaningful change (for example 15% relative). Evaluate after at least the minimum evidence (section 6.6) and at most 10 weeks.

| Drill metrics | In-game metric | Interpretation | Action |
|---|---|---|---|
| Improving | P_imp ≥ 0.85 | Working | Keep going, then taper to maintenance once the rate ≤ band P40 |
| Improving | P_imp in 0.5-0.85 | Inconclusive | Continue, collect more games |
| Improving | P_imp < 0.5 after max window | **Transfer failure** | Change modality: timed, no-prompt or "find the threat" drills, own-game positions, a thinking-process checklist in slow games, a longer time control |
| Flat | Any | Wrong difficulty or dose | Adjust puzzle difficulty to 70-85% success; check adherence (minutes done vs prescribed) |
| Low adherence | Any | Not a test of the prescription | Reduce dose, change format; don't conclude anything |

---

## 6. Proposed improvement engine spec

### 6.1 Data pipeline and measurement contract

1. **Import** games from the Lichess API and Chess.com PubAPI (PGN with clocks `%clk`). Store the platform, time control, ratings of both players, and result.
2. **Time-control classes**: bullet (< 3 min est.), blitz (3-8), rapid (8-25), classical (> 25), using estimated duration = base + 40·increment. Metrics are computed **per class**, with blitz and rapid pooled only with a TC covariate.
3. **Analysis**: pinned Stockfish version and net, fixed **1,000,000 nodes** per position, MultiPV = 3 for positions where the mover's ΔW ≥ 5 or the best/second gap needs evaluating. Store eval, PV (≥ 8 plies), and the engine version hash per position. Re-analyse in bulk on engine change.
4. **Tablebase**: Syzygy 7-man (local or the Lichess tablebase API) for every position with ≤ 7 pieces. Store WDL.
5. **Difficulty**: Maia-2 at the player's rating bucket gives `p_best` (probability of best move) and `p_ok` (probability of a move with ΔW < 5).
6. **Per-move record**: `W_before, W_after, ΔW, judgement (Lichess thresholds), phase, clock_before, time_spent, clock_fraction, is_critical (best−second ≥ 15 W%), decided (|W−50| ≥ 40), refutation_tags[], missed_tags[], threat_existed, threat_addressed, depth_to_swing, p_best, p_ok, tb_wdl_before/after`.
7. **Phase**: opening until ply 20 or until leaving the player's repertoire/book, whichever is later, capped at ply 30. Endgame when each side's non-pawn material ≤ 13 (Q=9, R=5, B=N=3) or total non-pawn material ≤ 26 with no queens. Middlegame otherwise.
8. **Band reference tables**: from the Lichess open database (evaluated subset), computed per platform-normalised band × TC class. Store median, P25, P40, P75 per metric. Recompute quarterly. Validate a sample against the 336M-move anchors (section 2.1).

Rating bands (internal, Lichess-equivalent scale; map Chess.com ratings via a configurable offset table fitted from players with accounts on both platforms):

| Band | Lichess-eq. rapid | Label |
|---|---|---|
| B0 | < 900 | Beginner |
| B1 | 900-1300 | Novice |
| B2 | 1300-1700 | Intermediate |
| B3 | 1700-2000 | Club |
| B4 | 2000-2300 | Strong club / expert |
| B5 | 2300+ | Master-level |

### 6.2 Weakness taxonomy: detection rules and metrics

All rates use Bayesian shrinkage to the band prior (n₀ given per weakness) and are reported with 80% credible intervals. "Eligible" means the position is not already decided (|W_before − 50| < 40) unless stated otherwise. **Decisive error** means ΔW ≥ 15 (Lichess blunder).

| ID | Weakness | Detection rule (per move or episode) | Metric (numerator / denominator) | Primary bands | n₀ |
|---|---|---|---|---|---|
| W1 | **Hung material** | Decisive error AND refutation PV wins ≥ 2 pawns of material within ≤ 3 plies via `hangingPiece` or capture of an undefended/under-defended piece (SEE < 0 for the moved or left piece) | events / eligible moves; also per game | B0-B2 | 300 moves |
| W2 | **Missed opponent threat** | `threat_existed` (null-move probe: opponent wins ≥ 2 pawns or mates within 4 plies) AND not `threat_addressed` AND decisive error | events / eligible moves where threat_existed | B0-B3 | 100 threat positions |
| W3 | **Missed own tactic** (by motif) | Best move improves W by ≥ 15 vs the played move, best PV realises material ≥ 2 or mate, the played move is not in top-2. Tag motif via cook-style detectors on the best PV. Split by `depth_to_swing` ≤ 3 (shallow) vs ≥ 5 (deep) | events / tactical opportunities; per motif | B1-B4 | 60 opportunities |
| W4 | **Missed mate / allowed mate** | Best line is mate in ≤ 4 and the played move isn't, or the played move allows forced mate (Lichess MateCreated/MateLost) | events / opportunities | B0-B2 | 40 |
| W5 | **Conversion failure** | Episode: player W ≥ T_win(band) (default 85; per-band from 4.3) held for ≥ 3 consecutive own moves after ply 20. Failure = game not won. Also record the move that dropped below 65 W% and its tags | failures / winning episodes; EPL in won positions | All | 30 episodes |
| W6 | **Defence / resourcefulness** | Episode: player W ≤ 15 for ≥ 3 moves. Success = draw or win. Secondary: mean ΔW per move while losing vs band | saves / losing episodes | B2+ | 30 episodes |
| W7 | **Opening losses** | ΔW losses in the opening phase; eval at end of opening (W_open_end − 50); per-ECO/line aggregation; repertoire deviation = player's move ≠ their own most frequent move in a position they reached ≥ 3 times; early traps fallen for (decisive error ≤ ply 20) | mean W at opening end; EPL_opening/game; per-line score vs expectation | Traps: B0-B2; prep: B3+ | 40 games/line |
| W8 | **Endgame technique** | Endgame-phase EPL/move vs band; **TB drops**: tb_wdl worsens (Win→Draw/Loss, Draw→Loss) on the player's move. Categorise by material signature (KPK, rook endings, minor-piece, queen) | TB drops / TB-eligible moves; endgame EPL/move | B1+ (basics B0-B1) | 80 TB positions |
| W9a | **Time trouble** | Game reaches clock < 10% of base (or < 60 s rapid / < 20 s blitz) before move 40 or game end with ≥ 10 own moves remaining | fraction of games | B2+ (rapid/blitz) | 30 games |
| W9b | **Time-pressure collapse** | Decisive error rate when clock < 30 s ÷ rate when clock > 60 s (blitz), adjusted for band ratio | ratio vs band median ratio | All fast TCs | 40 low-clock moves |
| W9c | **Rushing critical moves** | is_critical AND time_spent < 0.5 × player's median time per move AND error ≥ Mistake; or in rapid/classical, < 10% of remaining time on critical decisive errors | events / critical positions | B0-B3 (slow games) | 60 critical |
| W9d | **Time misallocation** | Spearman correlation between time_spent and criticality score (best−second gap) per game, averaged; low or negative = spends time on the wrong moves | mean ρ vs band | B2+ | 20 games |
| W10 | **Positional drift** | Mean ΔW on eligible *quiet* moves (not critical, no tactic in best PV, |eval| < 150 cp) in the middlegame; frequency of Inaccuracy-grade losses | EPL_quiet/move vs band | B3+ | 600 moves |
| W11 | **Easy-move errors (vision)** | Decisive error where `p_ok ≥ 0.8` (peers almost always avoid it) | events / eligible moves with p_ok ≥ 0.8 | B0-B2 | 300 |
| W12 | **Tilt / fatigue** (behavioural) | Score residual (actual − expected from ratings) for game k ≥ 4 in a session (gap < 15 min) and after a loss vs otherwise | residual difference with CI | All | 60 games |

Notes:

- W11 is a cross-cutting "board vision" indicator. Anderson et al. show difficulty is the main error driver, so errors on easy positions are the most diagnostic and the most trainable.
- Each detected event stores the FEN, the played move, the best move, PVs and tags. These become **own-mistake puzzles** (section 6.4).
- Overlaps: one move can be W1 and W2, or W1 and W11. Attribute EPL once, by precedence: W4 > W2 > W1 > W3 > W8 > W7 > W10. Tags still record all labels for coaching text.

### 6.3 Initial diagnosis

- **Minimum data**: 30 analysed games in one TC class gives a provisional profile (wide intervals, shown as "early read"). **100+ games** gives a full profile. Use the last 6 months, weighting recent games (half-life 60 days).
- **Score** each weakness: posterior rate, band percentile, Excess EPL/game, and Confidence = P(rate > band median).
- **Rank** by Priority_c (5.4). Select the top 1-2 focus weaknesses with Confidence ≥ 0.7. If none qualify, the focus becomes general: slow games plus own-game analysis plus mixed tactics.
- **Claude's role**: receives the structured profile (metrics, intervals, 3-5 exemplar positions per focus weakness with PVs and tags) and writes the coaching narrative and weekly plan. It must cite exemplar games and positions, must not invent evaluations or lines (only use the supplied PVs), and must state uncertainty in the given intervals. Validate output against the structured data, for example checking that every move it mentions exists in the supplied PV.

### 6.4 Prescription mapping

Trainability table (0-1) used in Priority, based on expert consensus. Tune it later from platform outcome data (6.8):

| Weakness | B0 | B1 | B2 | B3 | B4 | B5 |
|---|---|---|---|---|---|---|
| W1 Hung material | 1.0 | 1.0 | 0.9 | 0.8 | 0.7 | 0.6 |
| W2 Missed threat | 1.0 | 1.0 | 0.9 | 0.8 | 0.7 | 0.6 |
| W3 Missed tactic (shallow) | 0.8 | 1.0 | 0.9 | 0.8 | 0.6 | 0.5 |
| W3 Missed tactic (deep) | 0.2 | 0.4 | 0.7 | 0.8 | 0.8 | 0.7 |
| W4 Mates | 1.0 | 0.9 | 0.7 | 0.5 | 0.4 | 0.3 |
| W5 Conversion | 0.6 | 0.8 | 0.9 | 0.9 | 0.8 | 0.7 |
| W6 Defence | 0.2 | 0.4 | 0.6 | 0.7 | 0.7 | 0.7 |
| W7 Opening traps | 0.7 | 0.7 | 0.5 | 0.3 | 0.2 | 0.2 |
| W7 Opening prep | 0.1 | 0.2 | 0.4 | 0.7 | 0.9 | 1.0 |
| W8 Endgame basics (K+P, R vs P, Lucena/Philidor) | 0.6 | 0.8 | 0.9 | 0.8 | 0.6 | 0.5 |
| W8 Endgame advanced | 0.0 | 0.2 | 0.4 | 0.7 | 0.9 | 1.0 |
| W9 Time (a-d) | 0.5 | 0.8 | 0.9 | 0.9 | 0.8 | 0.7 |
| W10 Positional drift | 0.0 | 0.1 | 0.3 | 0.6 | 0.8 | 0.9 |
| W11 Vision | 1.0 | 1.0 | 0.8 | 0.6 | 0.5 | 0.4 |
| W12 Tilt | advice only (session caps, break rules); not a drill target |

Prescription modules per weakness. Doses are per week for a "standard" plan of about 4-5 h; scale them linearly with the user's available time.

| Weakness | Primary drills | Thinking-process / game assignment | Mastery criterion (leading indicator) |
|---|---|---|---|
| W1/W2/W11 | (a) **Own-mistake puzzles**: the position before the blunder, task "find the opponent's best reply to your move" and "find a safe move". (b) **"Is it safe?"** drills: show a candidate move and ask safe or unsafe, 50/50 mix. (c) Lichess themes `hangingPiece`, `fork`, `mateIn1`, `mateIn2`, `defensiveMove` at 80-85% target success. FSRS-scheduled own-mistakes. ~60-80% of the plan for B0-B1 | Heisman checklist before every move in rapid 15+10 (or longer): (1) what does the opponent's last move threaten? (2) my checks, captures and threats; (3) blunder-check the candidate. Play ≥ 3 slow games per week | Safe/unsafe drill ≥ 90% in < 15 s; own-mistake retention ≥ 0.9 |
| W3 shallow | Interleaved Lichess themed puzzles for the player's weakest motifs (by W3 motif tags), at 70-80% success, timed. Include 20-30% "no tactic, find a good move" positions to break the puzzle prompt | Analyse own games: before running the engine, mark where you think tactics existed | Motif puzzle success ≥ 80% at puzzle rating ≈ game rating + 100 |
| W3 deep | Longer puzzles (`long`, `veryLong`), solved fully before moving pieces (blindfold-style), annotated calculation exercises (Yusupov-style tests) | Slow games; write candidate lists in analysis | Success on ≥ 5-ply puzzles at band level |
| W4 | Mate pattern drills (`mateIn1-3`, named patterns: back rank, smothered, Anastasia, ...) | — | ≥ 90% on mateIn2 at band rating |
| W5 | **Convert-vs-engine**: start from the user's own positions where they were winning (W ≥ 85) and failed, play it out vs Stockfish (or Maia at their rating + 200) at a rapid clock. Must reach W ≥ 95 or a win. Plus "simplify when ahead" lessons | Rule: when ahead, trade pieces not pawns, remove counterplay first; spend time on the move *after* winning material | Converts ≥ 80% of drill positions |
| W6 | Defend-vs-engine from own losing positions; Lichess `defensiveMove`, `equality` puzzles | — | Holds/draws ≥ 40% of drill positions |
| W7 traps (B0-B2) | Short repertoire (1-2 openings per colour) built from the user's own games, trained with FSRS only on lines they actually reached. Opening-trap puzzles from positions where they lost early | Prefer principled openings; no new lines until the error rate drops | Line retention ≥ 0.9; no early-trap losses in the last 20 games |
| W7 prep (B3+) | Opening trainer (FSRS) for deviation points from their games (where they left book or erred), model games for the resulting structures | Post-game: check the opening against the explorer and add a card | Mean W at opening end ≥ band median |
| W8 | **Tablebase drills** vs perfect defence: K+P vs K, Lucena, Philidor, R+P vs R, Q vs P, plus the user's own TB-drop positions. Graded curriculum by band (basics first) | Play out won endgames to the end in casual games | Wins all TB-won drill positions within the 50-move rule at rapid speed |
| W9a/c/d | **Clock-pressure drills**: puzzles with a countdown per move scaled to the user's TC. "Critical or not?" classification drills (is this a position where you should think?). Time budget guidance (Heisman: use most of the clock in slow games) | Per-game time checkpoints (for example ≥ 40% of base left at move 20 in rapid). Play at a longer TC or with increment | Time-trouble frequency ≤ band P40 |
| W9b | Blitz-speed safety puzzles (Puzzle Storm-style with a safety emphasis), premove discipline | Switch to increment time controls | Low-clock blunder ratio ≤ band median |
| W10 | Annotated model games in the user's main structures (from their repertoire), "guess the move" exercises scored by ΔW, plan-identification tasks with Claude explanations | Post-game analysis: identify the plan in 3 key positions | Guess-the-move mean ΔW decreasing |
| W12 | Session rules: stop after 2 consecutive losses or 90 min, no rated play when tilted | — | Session-residual difference within CI of 0 |

Default allocation by band (fraction of training time), as a starting point that is then personalised by Priority:

| Band | Safety/vision + tactics | Own-game analysis | Endgames | Openings | Strategy / model games | Slow games played |
|---|---|---|---|---|---|---|
| B0 | 50% | 15% | 10% (basic mates) | 5% | 0% | 20% |
| B1 | 45% | 20% | 10% | 5% | 0% | 20% |
| B2 | 35% | 20% | 15% | 10% | 5% | 15% |
| B3 | 25% | 20% | 15% | 15% | 10% | 15% |
| B4 | 20% | 20% | 15% | 20% | 15% | 10% |
| B5 | 15% | 20% | 15% | 25% | 15% | 10% |

**Own-game analysis is always ≥ 15%**. This is the activity with the best correlational support (Charness 2005, Southwick 2026 "game review"). Require the player to annotate their own thoughts before the engine/Claude reveal (retrieval and self-explanation).

**Puzzle difficulty targeting** (Glicko expected score): to target success rate s, choose puzzle rating `r_p = r_user_puzzle − 400·log10(s/(1−s))`. For s = 0.75 that is r_user − 191, and for s = 0.85 it is r_user − 301. Use s = 0.8-0.85 for new motifs and safety, and 0.7 for consolidation and deep calculation. Recompute weekly from actual success.

### 6.5 Weekly cycle

1. **Daily**: FSRS due cards (own mistakes and repertoire), then focus drills, then maintenance mixed puzzles.
2. **Per new game imported**: analyse, update the posteriors for all weaknesses, generate new own-mistake cards (only for decisive errors and the top 3 errors by ΔW per game, max 5 per game), and update the adherence log.
3. **Weekly review**: Claude writes a short report from the deltas: drills done vs planned, leading-indicator changes, in-game metric posteriors, 1-2 exemplar positions from this week's games (good and bad).
4. **Plan revision check**: run 6.6.

### 6.6 Re-measurement and adaptation rules

Definitions per focus weakness c:

- **Baseline**: posterior Beta(α₀, β₀) from the diagnosis window (≥ 30 games, shrunk).
- **Current**: posterior from games since the prescription started, with the baseline prior *not* reused. Use the band prior with n₀ to avoid double-counting.
- `P_imp = P(θ_current < θ_baseline × (1 − δ_rel))`, with δ_rel = 0.15. Compute by Monte Carlo (10k draws from each Beta).
- **Minimum evidence before any verdict**: the larger of 4 weeks and the per-weakness opportunity minimum (W1/W2/W11: 1,000 eligible moves, about 30-40 games; W3: 40 opportunities; W5/W6: 15 episodes; W8: 50 TB positions; W9a: 20 games; W10: 1,500 moves).
- **Maximum window**: 10 weeks or 2× the minimum evidence, whichever comes first, then a forced decision.
- **Adherence gate**: if completed minutes < 50% of prescribed, show "not enough practice to judge" and make no verdict on efficacy. Offer a smaller plan.

Decision rules, evaluated weekly after minimum evidence:

| Condition | Verdict | Action |
|---|---|---|
| P_imp ≥ 0.85 AND current rate ≤ band P40 | **Resolved** | Move to maintenance (10% time, FSRS only). Promote the next weakness |
| P_imp ≥ 0.85, still above band P40 | **Improving** | Continue. Raise difficulty one step |
| 0.5 ≤ P_imp < 0.85 | **Inconclusive** | Continue until max window |
| P_imp < 0.5 at max window, drill mastery met | **Transfer failure** | Switch modality per 5.5 (own-game positions, no-prompt drills, slower TC, thinking-process checklist). Fatigue_c += 0.3 |
| P_imp < 0.5 at max window, drill mastery not met | **Practice not effective** | Change drill type or difficulty. Check whether the weakness was misclassified by reviewing exemplar events (Claude summarises, user confirms) |
| P(θ_current > θ_baseline × 1.15) ≥ 0.85 | **Regression** | Check for a confound (time control change, more games under fatigue, engine change). Then reassess |
| Fatigue_c ≥ 0.6 | **Park** | Drop from focus for 8 weeks and focus the next priority |

**Drift monitoring** for maintained weaknesses: an EWMA on per-game event rate, `z_t = λ·x_t + (1−λ)·z_{t−1}` with λ = 0.1. Alert when z_t exceeds the resolved level + 2·σ_EWMA, with `σ_EWMA = σ·sqrt(λ/(2−λ))`. Re-promote to focus on alert.

**Rating outcome** (long horizon only): fit a per-player linear trend on Glicko rating vs time over ≥ 12 weeks and ≥ 60 games, with a 90% interval. Compare against the band-matched expected drift of similar users who are *not* in a focus plan (from platform data, see 6.8). Never compute it from the sign-up moment, only from the 90-day median baseline (5.1).

### 6.7 Honest progress presentation

- Three tiers, always labelled with certainty:
  1. **Practice** ("Your fork-puzzle success: 62% → 84%; solve time 41 s → 19 s"). This is high certainty, but it is not the goal itself.
  2. **Your games** ("Pieces hung per 10 games: 3.1 → 1.7. 91% likely this is a real change, not luck"). Show posterior intervals as bars, and show "games until we can tell" when the evidence is insufficient.
  3. **Rating** (trend line with a shaded 90% band, a "normal fluctuation" range of ±347/√n for the last n games, and a 90-day baseline, not sign-up day).
- Show an **expected points budget**: "Where your points go", a stacked bar of EPL by category vs peers. This makes the "why this plan" visible.
- Language rules for Claude-generated text: never "guaranteed". Say "likely" or "unclear yet" mapped from P_imp bands. Always name the evidence window ("over your last 42 rapid games"). Celebrate process (adherence, drill mastery) separately from outcomes. When a plan fails, say so and say what changes.
- **Peer comparison** uses band percentiles ("you hang pieces less often than 60% of players at your level"), which is more motivating and more stable than raw numbers.

### 6.8 Platform-level validation (turning the guarantee into evidence)

Individual-level noise makes per-user proof impossible, so validate the *engine* at population level:

- Log every prescription, dose, adherence and outcome metric.
- **Within-platform experiments**: randomise secondary choices (for example interleaved vs blocked drills, s = 0.75 vs 0.85 targeting, own-mistake FSRS on vs off) with users' consent. Primary outcome: the change in the targeted in-game weakness rate at 8 weeks. Secondary: rating at 6 months. Estimate effects with mixed models (user random effects, band × TC fixed effects).
- **Observational comparison** with matching: users with high vs low adherence, matched on band, TC, baseline trend and activity. This is confounded (motivated users do everything), so report it as such.
- Re-fit the **Trainability table** from estimated effect sizes per weakness × band each quarter.
- Pre-register the analysis plan internally, and publish an annual "what worked" post. That honest evidence is a stronger marketing asset than a guarantee.

### 6.9 Implementation checklist

- [ ] Engine worker: pinned Stockfish, 1M nodes, MultiPV 3 on demand, version hash per eval
- [ ] Syzygy 7-man WDL lookup
- [ ] Maia-2 inference service (rating-bucketed `p_best`, `p_ok`)
- [ ] Motif tagger applied to the best PV, the refutation PV, and the null-move threat probe. It is a clean-room implementation of our own rules (ADR-R2-3), not a port of `cook.py`.
- [ ] Per-move feature table (6.1 #6) and episode extraction (W5, W6, W9a)
- [ ] Band reference builder from the Lichess open DB (quarterly job)
- [ ] Beta-binomial posterior service with band priors; P_imp Monte Carlo; EWMA monitor
- [ ] Priority scorer + prescription generator (tables 6.4) + puzzle selector (Lichess puzzle DB by theme and rating, own-mistake generator)
- [ ] FSRS scheduler (open-spaced-repetition library) for own-mistake and repertoire cards
- [ ] Engine and Maia play-out drills (conversion, defence, TB endgames) with Lichess or custom board UI
- [ ] Claude prompt contracts: structured JSON in, coaching text out, with validation that every move and eval it cites exists in the input
- [ ] Progress UI with three-tier certainty display
- [ ] Experiment and logging framework (6.8)

---

## Sources

Academic:

- Charness et al. 2005, The role of deliberate practice in chess expertise, *Appl. Cogn. Psychol.* — https://doi.org/10.1002/acp.1106
- Burgoyne et al. 2019, *American Journal of Psychology* — https://englelab.gatech.edu/articles/2019/Burgoyne_et_al_2019_AJP.pdf
- Gobet & Campitelli, practice in chess — https://researchportal.murdoch.edu.au/esploro/outputs/journalArticle/The-role-of-practice-in-chess/991005542731507891
- Gobet & Campitelli, Deliberate practice: necessary but not sufficient — https://researchonline.lse.ac.uk/id/eprint/102854
- Southwick et al. 2026, Not All Practice Is Created Equal, *Psychological Science* — https://journals.sagepub.com/doi/10.1177/09567976261452568 (full text not reviewed; numbers here are from summaries)
- Burns 2004, The effects of speed on skilled chess performance, *Psychological Science* — https://www.psychologicalscience.org/journals/psychological-science/j.0956-7976.2004.00699.x/
- Anderson, Kleinberg, Mullainathan 2016, Assessing Human Error Against a Benchmark of Perfection — https://arxiv.org/abs/1606.04956
- McIlroy-Young et al. 2020, Maia — https://arxiv.org/abs/2006.01855 ; Maia-2 — https://arxiv.org/abs/2409.20553
- Chowdhary, Iacopini, Battiston 2022, Quantifying human performance in chess — https://arxiv.org/abs/2207.07780
- Regan, Intrinsic Performance Ratings — https://cse.buffalo.edu/~regan/papers/pdf/Reg12IPRs.pdf ; Regan & Haworth 2011 — https://cse.buffalo.edu/~regan/papers/pdf/ReHa11c.pdf
- Cepeda et al. 2006, Distributed practice meta-analysis — https://pubmed.ncbi.nlm.nih.gov/16719566/
- Taylor & Rohrer 2010, The effects of interleaved practice, *Appl. Cogn. Psychol.* 24:837-848
- Roediger & Karpicke 2006, Test-enhanced learning, *Psychological Science* 17:249-255
- Wilson, Shenhav, Straccia, Cohen 2019, The Eighty Five Percent Rule for optimal learning, *Nature Communications* 10:4646

Platform documentation and code:

- Lichess accuracy — https://lichess.org/page/accuracy ; source https://github.com/lichess-org/lila/blob/master/modules/analyse/src/main/AccuracyPercent.scala ; judgements https://github.com/lichess-org/lila/blob/master/modules/tree/src/main/Advice.scala
- Lichess puzzle tagger — https://github.com/ornicar/lichess-puzzler/blob/master/tagger/cook.py
- Lichess Glicko-2 / provisional — https://lichess.org/qa/888/what-is-glicko-2-rating-volatility
- Chess.com move classification — https://support.chess.com/en/articles/8572705-how-are-moves-classified-what-is-a-blunder-or-brilliant-move
- FSRS — https://docs.ankiweb.net/faqs/what-spaced-repetition-algorithm ; https://borretti.me/article/implementing-fsrs-in-100-lines ; https://expertium.github.io/Algorithm.html
- Aimchess — https://aimchess.com ; review https://www.chess.com/blog/SheldonOfOsaka/aimchess-a-review

Data analyses and blogs (Weak evidence):

- 336M blitz moves — https://www.chess.com/blog/Jordi641/hunting-for-context-in-336-million-blitz-moves
- Woodpecker / Disco Chess 1,950 users — https://lichess.org/@/DiscoChessApp/blog/woodpecker-method-training-431000-puzzles-60000-rated-games-and-35-elo/yrWsFF1N
- Puzzle vs game rating — https://lichess.org/@/mlatb/blog/correlation-between-rapidblitz-rating-and-puzzle-rating/H39ApaeR ; https://lichess.org/@/ggSayItBack/blog/do-puzzles-matter-a-data-analysis/twAyjQDX ; https://lichess.org/forum/general-chess-discussion/low-correlation-between-puzzle-proficiency--rating-implications-for-chess-improvement
- Eval/clock vs results — https://lichess.org/@/jk_182/blog/how-the-evaluation-and-clock-impact-results-of-blitz-games/I2kRp2sk ; accuracy methods — https://lichess.org/@/jk_182/blog/comparing-different-ways-to-calculate-accuracy/6aeYnbh8
- GM expected score by eval — https://chessenginelab.substack.com/p/expected-score-of-grandmasters
- Chess.com/Stanford activity study (vendor-reported) — https://www.chess.com/article/view/new-study-reveals-how-to-best-improve-on-chess-com
- Bullet vs classical correlation — https://lichess.org/forum/game-analysis/bullet-vs-classical-rating?page=2

Coaching:

- Dan Heisman, tactics and safety — https://danheisman.com/tactics-and-safety.html ; *Is Your Move Safe?* — https://www.mongoosepress.com/catalog/books/move_safe.html
- Steps Method — https://lichess.org/@/NoelStuder/blog/the-chess-step-method-explained/QCCUDwgt ; https://www.newinchess.com/the-complete-step-by-step-method
- Yusupov's Chess School — https://www.simonandschuster.ca/series/Yusupovs-Chess-School
- De la Maza — https://www.chess.com/article/the-michael-de-la-maza-story ; https://www.chess.com/blog/Larceny/seven-circles-follow-up
- Matuschak on MoveTrainer — https://notes.andymatuschak.org/zDr94hP6bG3jJYrdYy8B5hx
