# EXP-0019: a blind human audit of the motif tags the Lichess themes cannot judge

- Status: pre-registered
- Issue: #117
- Author: the maintainer, with Claude (lane `exp/0019-audit`)
- Kind: measurement (one version)
- Follows: none (EXP-0007's "hidden pending a human audit" branch filed it; that branch is not "need more data")
- Registered: 2026-11-24. The registration is the first commit that adds this file
  (`git log --diff-filter=A -- preregistration.md`), so it is not written here.

## 0. Value case

- Decision it informs: whether `hangingPiece`, `skewer`, `sacrifice` and `backRankMate`, which `algo-3.0.0` stops naming to players until they clear (ADR 0112, `coaching_input::motifs::PLAYER_VISIBLE_MOTIFS`), are named again, have their rule fixed, or stay hidden. `pin` is out of scope: its recall (0.20) needs a rule fix whatever an audit finds (EXP-0021).
- If the result is that a tag's audited precision is shown to be at least 0.80, we will add it back to `PLAYER_VISIBLE_MOTIFS` (a major algorithm version with its ADR, the test's source moving to EXP-0019); if it is shown below 0.80, its rule is fixed under its own experiment; if neither, it stays hidden, and the continuation rule of section 9 says whether a second session is worth a person's time.
- Expected cost: a person's time, about 85 minutes for one session of 126 items (at a planned 40 seconds an item, section 6), plus 5 minutes of practice and 10 of reading the definitions; at most one more session of the same size if section 9's continuation rule asks for it. No engine and no AI tokens: drawing the sample tags 20,003 puzzle solutions with `cli lab tag-motifs`, about a minute in the lab container. The analysis reads two small JSON files.
- Value score: impact 3 × uncertainty 3 ÷ cost 2 = 4.5
- Cheaper alternative? No. EXP-0007's themes cannot separate our errors from their gaps, which is why the four tags are hidden. Its pilot hand check judged 19 of 26 checked disagreements to be label gaps, but it was the agent's and unblind, and the protocol keeps an LLM's judgement out of any decision (`docs/evaluation.md`). A wider check against Lichess's generator code is ruled out by ADR-R2-3. The person judges only the puzzles where the theme is silent (section 4), which is the smallest audit that answers the open question; the full human check of every tagged puzzle would need about four times the items (section 6).
- Compute budget: the registered run reads 126 audited items (`items_read`); drawing the sample (`make_audit.py`) tags 20,003 lines in 11 cli calls, recorded in the key; no engine nodes, no profile calls, no AI tokens; cap 150%

## 1. Question

When Rematch tags a puzzle's solution with one of these four motifs and Lichess's themes do not name it, is the motif there? A club player told "you missed a hanging piece" should find a piece left loose on the board, not a back-rank mate that happened to take a rook. EXP-0007 measured the share of tagged audience puzzles that carry the matching theme, `π`: 0.572 for `hangingPiece`, 0.810 for `skewer`, 0.528 for `sacrifice` and 0.606 for `backRankMate` (`lab/experiments/EXP-0007/results.json`), with recall of 0.82 to 1.00 (`decision.md` there). A recall near 1 with a low `π` says our rules are wider than the themes, not that they are wrong. This experiment asks a person, blind, which it is.

## 2. Hypotheses

A measurement study: the hypotheses name the branches of section 9.

- H0 (per tag): the audited precision `P` (section 4) is below 0.80.
- H1 (per tag): `P` is at least 0.80.

Rival explanations that would give the same answers, and how each is handled:

- A lenient auditor. A person who wants the tags back can say yes too easily. The auditor is blind to our tags and to Lichess's (section 7), every item asks all four questions whatever it was drawn for, and 18 distractors carry none of our four tags. The share of "yes" on distractors without any of the four themes is reported (section 4); a high share there reads as leniency, not as a tag that is right.
- A definition that drifts. The definitions are written down here, before the sample is drawn, and the page shows them beside every question. They are frozen with this file.
- Labelled puzzles taken as true. `P` takes a tagged puzzle that carries the theme as the motif (section 4), the same ground truth EXP-0007 trusted four tags on. If the person's reading of our definition differs from Lichess's, that is wrong. Each tag has three labelled items, mixed in blind, and a tag whose labelled items get two or more answers other than "yes" cannot clear (section 9). The precision with the labelled items judged too is reported beside it.
- The definitions are ours. A "yes" means "the motif as Rematch means it", not "the motif as Lichess means it". Where the two differ, section 4 says how.
- Solution lines are not engine lines. As in EXP-0007, the tagger reads the puzzle's solution, not **Stockfish 19**'s best line. Every claim here is about the tags of puzzle solutions; EXP-0023 measures the engine's line.
- One rater. A single person's verdicts carry their own blind spots. The limit is stated, and a second-rater subsample is registered as optional and exploratory (section 7).

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-3.0.0+554036a3b27c` | – |

From `cli algo show` on the lab image built from main at 4d464624. The tagger (`backend/crates/profile-core/src/motifs/`, called through `derive_move_features` with `LINE_MAX_PLIES` 8) is the one EXP-0007 measured on `algo-1.1.0+fa5e2186e1dd`: ADR 0112 hides tags at the profile and leaves the tagger and `cli lab tag-motifs` unchanged, and the release arena found no difference on any of EXP-0007's metrics. `π` is therefore read from EXP-0007's results; the key records the `algorithm_id` of the tagging, and a sample that spans more than one stops.

## 4. Primary metric and ground truth

### The definitions

Written as Rematch intends each tag, for a player who knows how the pieces move. The page shows the one-line question and, on request, the paragraph. A position can show several motifs at once; each question is answered on its own.

- **Hanging piece.** Early in the line (usually its first move) a knight, bishop, rook or queen is captured that no other enemy piece was guarding, and the opponent cannot take back on that square. The point is a piece left loose and taken for free. A pawn does not count, and nor does a piece that was guarded (even too few times: winning it by attacking it more often than it is defended is another idea). It still counts when the capture also gives check or mate. Page question: "Does the line win a knight, bishop, rook or queen that was left unguarded, so it is taken for nothing?" Lichess's description: "A tactic involving an opponent piece being undefended or insufficiently defended and free to capture." The difference: Lichess counts a piece defended too few times, and we do not; Lichess names the idea of the puzzle, while we count a free piece wherever the line takes one, mate or not.
- **Skewer.** A bishop, rook or queen attacks a valuable enemy piece along a line (often the king, with check). That piece moves out of the way, and the same attacker then captures the piece that stood behind it on the same line. It is the reverse of a pin: in a pin the piece in front is the less valuable one and cannot move. A piece that leaves the line for another reason, and is not attacked along it, does not make a skewer. Page question: "Does a bishop, rook or queen attack a valuable piece, make it move off the line, and then take the piece behind it?" Lichess's description: "A motif involving a high value piece being attacked, moving out the way, and allowing a lower value piece behind it to be captured or attacked, the inverse of a pin." The difference: Lichess also counts a piece behind that is only attacked; we need it captured in the line. Our rule (`rules.rs`, `skewer`) does not check that the front piece was attacked along the line; the definition does, so a "no" there is our error.
- **Sacrifice.** The side to move deliberately gives up material it does not get straight back: at some point in the line it is at least two pawns' worth down (a knight or bishop for nothing, a rook for a minor piece, the queen for a rook). By the end of the line it has won more than it gave, at least two pawns' worth up overall, or it has given mate. A trade in which the material comes straight back is not a sacrifice. Page question: "Does the side to move give up material on purpose (two pawns' worth or more) and get more back, or mate, by the end of the line?" Lichess's description: "A tactic involving giving up material in the short-term, to gain an advantage again after a forced sequence of moves." The difference: ours has a size, two pawns' worth down and two up (`SACRIFICE_PAWNS` 2), and Lichess's has none.
- **Back-rank mate.** The line ends in checkmate by a rook or a queen on the mated king's back rank (rank 1 for White, rank 8 for Black), with the king standing on that rank. The king cannot step forward off it because every square in front of it holds one of its own pieces, usually its pawns. A mate on the back rank where an enemy piece, not the king's own, covers a square in front of it does not count. Page question: "Does the line end in mate by a rook or queen on the king's own back rank, the king boxed in by its own pieces in front of it?" Lichess's description: "Checkmate the king on the home rank, when it is trapped there by its own pieces." The difference: none in the words; ours names the mating piece (a rook or a queen) and asks that every forward square hold the king's own piece. EXP-0007's critic asked whose definition the product wants: ours, as written here.

Lichess's descriptions are quoted from its published puzzle themes (CC0 data, many thanks to Lichess and its players); no code from its generator is used (ADR-R2-3).

### The metric

- Primary metric (exactly one): the audited precision of each tag, `P = π + (1 − π)·b`: of the audience puzzles we tag with it, the share that are the motif, by the theme where the theme names it, and by the person where it does not. `π` is EXP-0007's precision against the themes (its estimate, with the smaller of its linearised and Kish effective sample sizes); `b` is the share of the audited tagged puzzles without the theme that the person judges to be the motif ("unsure" counted as no for clearing, as yes for failing; section 9).
- Ground truth: a person's verdict, blind to our tags and to Lichess's themes, against the definitions above, where Lichess's theme is silent; Lichess's theme where it names the motif. The person is the maintainer, a club-level player.
- Secondary metrics, all exploratory:
  - `P` with "unsure" left out of `b`;
  - `P` with the labelled items judged too, `π·a + (1 − π)·b`, where `a` is the share of the tag's labelled items judged "yes";
  - per tag, the share of "yes" on distractors carrying that theme (motifs we miss) and on distractors carrying none of the four themes (the auditor's leniency);
  - the time per item, from the page (in `decision.md`, never in `results.json`);
  - the second rater's agreement, if one is run (section 7).

## 5. The minimum effect that matters

- Minimum effect: 0.10 from the floor of 0.80, as in EXP-0007 (section 5 there): a tag whose true precision is 0.90 should be shown, one at 0.70 kept hidden.
- Why this size: the floor is the product rule of EXP-0007. A precision of 0.80 is one named motif in five that is not there. Section 6 shows that this audit cannot reach 80% power at 0.90 inside a person's 90 minutes, and registers the precision it can reach instead.

## 6. Power analysis

- Method: a simulation of the section 9 rule, both looks included (`analysis.py power`, `power.json`), 20,000 replicates a scenario, seed `power/simulation`.
- Inputs: `π` and its effective sample size per tag from EXP-0007's `results.json`; the true `P` from 0.70 to 0.98, with `b = (P − π)/(1 − π)`; the person answers "unsure" on 5% of the unlabelled items whatever the truth, and otherwise answers right; labelled items are judged "yes" with probability 1 (sensitivity: 0.95 and 0.85); one session of 6 blocks, or the 3 blocks of the minimum, then the continuation rule; two-sided Bonferroni over the four tags and the two looks.
- The threshold. With `π` at its lower bound, `b` must be shown to be at least 0.565 (`hangingPiece`), 0.127 (`skewer`), 0.594 (`sacrifice`) or 0.544 (`backRankMate`) for a tag to clear.
- Result: one session of 126 items, of which 30 unlabelled `hangingPiece`, 30 `sacrifice`, 24 `backRankMate` and 12 `skewer` puzzles, 12 labelled (3 a tag) and 18 distractors, in 6 blocks of 21 (section 7). The probability that a tag clears, by its true `P`, with the second session when the rule calls for one (the first look alone in brackets):

| Tag | P = 0.80 | P = 0.85 | P = 0.90 | P = 0.95 | P = 0.98 |
|---|---|---|---|---|---|
| `hangingPiece` | 0.0003 | 0.02 | 0.38 (0.13) | 0.95 (0.65) | 1.00 (0.94) |
| `skewer` | 0 | 0.03 | 0.67 (0.47) | 0.99 (0.96) | 1.00 (1.00) |
| `sacrifice` | 0.0005 | 0.02 | 0.30 (0.09) | 0.90 (0.52) | 1.00 (0.87) |
| `backRankMate` | 0.0001 | 0.01 | 0.30 (0.05) | 0.93 (0.40) | 1.00 (0.80) |

  The probability that a tag fails at a true `P` of 0.70 is 0.34 (`hangingPiece`), 0.38 (`sacrifice`) and 0.20 (`backRankMate`); `skewer` cannot fail, since `π` alone is 0.81. At `P` = 0.80 a wrong "fails" comes in at most 0.0007 of the replicates, and a wrong "clears" in at most 0.0005, against the 0.003125 each direction of each tag is allowed.
- What is affordable: the minimum effect is not. A tag at a true 0.90 clears with 0.30 to 0.67 probability, and 80% would take more than 80 unlabelled items a tag at each look (`n_curve` in `power.json`: 0.52 to 0.58 at 80 items), more than 300 items and four hours of a person's time. The audit is powered for a tag at 0.95: it clears with probability 0.90 to 0.99 after at most two sessions, and with 0.40 to 0.96 after one. A tag between 0.80 and 0.90 will most likely stay hidden, which is the cautious direction the product already takes.
- Sensitivity (`power.json`): "unsure" matters most. With no "unsure" the chance to clear at 0.90 rises to 0.52 to 0.73, and with 20% it falls to 0.01 to 0.45 (0.14 to 0.93 at 0.95). Labelled items judged "yes" with probability 0.85 instead of 1 lower the chance to clear at 0.95 by at most 0.12. Stopping at the minimum of 3 blocks gives 0.41 to 0.92 at 0.95.
- The 40 seconds an item is a planning figure, not a measurement; the page records the time per item and `decision.md` reports it.

## 7. Design

- Unit of analysis: the puzzle. Each comes from a different game (EXP-0007, section 7), so no two units share one. The four tags are judged on the same items, and each tag's estimate uses only the items drawn for that tag's strata.
- Population: EXP-0007's audience, the puzzles of the snapshot played at least 1,000 times, with a popularity of at least 50 and a rating below 2000, less EXP-0007's pilot (1,874 puzzles, development data, which its agent hand-checked).
- The frame: a simple random sample of 20,000 audience puzzles (seed `sample/frame`), tagged through `cli lab tag-motifs` as EXP-0007 does (the position after the puzzle's first move, the rest of the solution as the mover's line). A puzzle the production code cannot play is excluded and listed in the key. Each frame puzzle falls in a stratum per tag we give it: `U:<tag>` (our tag, not Lichess's theme), `L:<tag>` (both); or, with none of our four tags, `D:miss` (one of the four themes) or `D:none` (none). A puzzle we give two of the four tags is in two strata.
- The items: from each stratum, the first puzzles of a seeded random order (seed `sample/items`), a simple random sample of that stratum of the frame, and so of the audience. A block holds 5 `U:hangingPiece`, 5 `U:sacrifice`, 4 `U:backRankMate`, 2 `U:skewer`, 2 labelled items (odd blocks `L:hangingPiece` and `L:sacrifice`, even blocks `L:backRankMate` and `L:skewer`) and 3 distractors (odd blocks 2 `D:miss` and 1 `D:none`, even blocks 1 and 2): 21 items, 6 blocks, 126 items. A puzzle drawn for two strata is shown once and counts in both. Within a block the order is seeded (seed `sample/order`), so the strata are interleaved and every prefix of a stratum's items is a random sample of it. The allocation gives `backRankMate` four a block because its threshold is the lowest of the three hard tags, and `skewer` two because its threshold is low (section 6).
- Seeds: `sample/frame`, `sample/items`, `sample/order`, `power/simulation` (`rematch_lab.experiments.seed`); no other source of randomness.
- What the person sees: a page (`make_audit.py` writes `data/exp19/audit.html`, git-ignored) that runs in a browser with no network (a Content-Security-Policy forbids every connection, and it loads no script, font or image from outside). For each item: the board after the opponent's last move (the puzzle's first move, shaded), oriented for the side to move; the solution played out move by move, both sides, in long algebraic notation (`Rf7×f1`), a check marked `+` and the final mate `#` (from the replay, EXP-0007's trusted mate check); the four questions in a fixed order, each with yes, no and unsure, and its definition a click away. Three practice items from EXP-0007's pilot come first and are never analysed. The page saves every answer in the browser (`localStorage`) and downloads `answers.json`; it shows the item number ("12 of 126") and the block, and the minutes spent.
- Blinding: the page holds the positions and the lines only. No puzzle id, tag, theme, rating or stratum is in it, and the items' order is seeded and interleaved. Every item asks all four questions, so the questions reveal nothing about why an item is there; on most items three of the four are distractor questions. 18 of 126 items carry none of our four tags, and 9 of those carry one of the four themes, so a "yes" can be right on an item we did not tag. The key (`data/exp19/key.json`, which maps items to ids, strata and tags) stays out of git and off the auditor's screen until the audit is done; `sample.csv` in git holds the drawn ids only, sorted. The person does not look a position up anywhere, on Lichess or in an engine.
- The limits of the blinding: the auditor is the maintainer, who knows this design (most items carry one of our four tags), knows the rules, and has seen EXP-0007's hand check. The distractors, the four questions per item and the written definitions limit what that knowledge can steer; they do not remove it. That is said in `decision.md` whatever the result.
- The stopping rule: the person works through the items in order and may stop after any whole block once past 90 minutes, or earlier when tired, and come back later in the same browser; at least 3 blocks make an audit. Because the person never sees the key, a stop depends on time and fatigue, not on the answers, so it spends no error. Nobody looks at the answers against the key before the person stops: there is no interim look inside a session.
- The follow-up session: at most one, registered as its own experiment (the next free number) with `Follows: EXP-0019` if section 9's continuation rule calls for it, for the continuing tags only, with the next 6 blocks of those tags' strata in the same seeded orders (and distractors and labelled items drawn on from theirs, the same mix), the same definitions, and the rule of section 9 on both sessions pooled at the same level. Its error is already paid for here (two looks in the Bonferroni family).
- Inter-rater: one rater, a stated limit. Optional and exploratory: a second club-level player judges blocks 1 and 2 (42 items) on the same page, blind, before the key is committed to git; Cohen's kappa per question with its 95% interval is reported in `decision.md`. It never changes a decision.
- Development and confirmatory data: nothing of the frame has been seen. The definitions, the thresholds and the allocation were set from EXP-0007's published numbers and its pilot (development data, excluded from the frame). No answer exists before the run.
- Held fixed: the snapshot, the filters, the cli of section 3, the page as generated (its `page_id` and sha256 are in the key, and `answers.json` must name the `page_id`).
- Data: the Lichess puzzle snapshot of EXP-0007 (`external:lichess-puzzles/lichess_db_puzzle.csv`, sha256 `8ce98180bcb7cb60f3f51f12afdbe6a4fd294d8aaf4f1410e8c25e85ac78fdc9`, its section 7); EXP-0007's `results.json` (for `π`) and `sample-pilot.csv` (excluded); the key and the answers. `inputs.sha256` names each with its sha256: the key as an external input from the commit that draws the sample, the answers as an external input whose line holds a placeholder until the audit is done. Once the audit is done, `answers.json` and `key.json` are committed to this folder (the key's sha256 unchanged) and `inputs.sha256` points at them, so the analysis reruns in CI without the snapshot. No real chess.com player's games.
- Leakage controls: the pilot puzzles are not in the frame; the auditor never sees the key; the answers are bound to the page by its `page_id`.
- Reuse: EXP-0007's snapshot, its population filter, its precision `π` and its effective sample sizes (`results.json`, sha256 in `inputs.sha256`), and its pilot as practice items; no engine evaluation is needed.

## 8. Analysis plan

- Model or test: no model. For each tag, `b` from the items drawn for `U:<tag>`: `k` "yes" of `n`, "unsure" `u`. The bounds are one-sided at `α` = 0.05 / (4 tags × 2 looks × 2 directions) = 0.003125 each, split by the union bound between `π` (0.1 of it, a Wilson bound at EXP-0007's effective sample size) and `b` (0.9 of it, an exact Clopper-Pearson bound): the lower bound of `P` is `1 − (1 − π_low)(1 − b_low(k, n))`, and the upper bound `1 − (1 − π_up)(1 − b_up(k + u, n))`. `P` increases in both `π` and `b`, so each bound holds with at least `1 − α`.
- Effect size: `P`'s point estimate (`π + (1 − π)·k/n`) and the bounds above; the bounds of `π` and `b` apart.
- Multiple comparisons: Bonferroni over the four tags, both directions and both looks (the follow-up's). The secondary metrics are exploratory and uncorrected.
- Diagnostics: no fitted model. Section 6's simulation reports the error rates of the rule.
- Exclusions and missing data: the items done are those up to the last item with any answer; a question left blank before it counts as "unsure". Fewer than 3 whole blocks done is a failed result, reported as one, with no decision. The three practice items are never analysed. A frame puzzle the code cannot play is excluded before the draw.

## 9. Decision rule

Measurement, per tag. It departs from ADR 0106's default (a 95% interval inside a band): each tag's verdict is acted on separately, so the intervals are Bonferroni over the four tags, both directions and both looks, as EXP-0007's were over its tags; and the rule has three branches, because "shown below" and "not shown either way" lead to different actions (a rule fix, against staying hidden).

- Clears: the lower bound of `P` ("unsure" as no) is at least 0.80, and at most one of the tag's labelled items got an answer other than "yes". The tag is named to players again, in a new major algorithm version with its ADR.
- Fails: the upper bound of `P` ("unsure" as yes) is below 0.80. The tag stays hidden and its rule is fixed under its own experiment, filed under "Next".
- Inconclusive: anything else, including a tag whose lower bound clears while two or more of its labelled items got an answer other than "yes" (the person and the theme disagree, so `P`'s ground truth does not hold for that tag). The tag stays hidden.

### Stopping rules

- Early stopping: none inside a session (the stopping rule of section 7 depends on time, not on the answers). Look 1 is this session; look 2 is the follow-up's, if any.
- Futility and continuation: after look 1, an inconclusive tag continues to a second session only if its estimate (`π + (1 − π)·k/n`, "unsure" as no) is at least 0.88, where a clear is within reach, or at most 0.72, where a fail is. Otherwise it stays hidden with no further audit, and a later fix of its rule is the way back. If no tag continues, the experiment ends at look 1. After look 2 there is no third.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0019/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` the key was tagged on; `python analysis.py power` wrote `power.json` before registration). `make_audit.py` draws the sample and writes the page; it is committed with this file, before it runs.
- Analysis code sha256: `c24a698030fc07580cee97d55308540bc47451907a4a81d3086a04002628f796`

## 11. Deviations

None.
