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
- Expected cost: a person's time, about 90 minutes for one session of 132 items (at a planned 40 seconds an item, section 6), in one or two sittings, plus 5 minutes of practice and 10 of reading the definitions; at most one more session of the same size if section 9's continuation rule asks for it. No engine and no AI tokens: drawing the sample tags 20,080 puzzle solutions with `cli lab tag-motifs`, about 20 seconds in the lab container. The analysis reads two small JSON files.
- Value score: impact 3 × uncertainty 3 ÷ cost 2 = 4.5
- Cheaper alternative? No. EXP-0007's themes cannot separate our errors from their gaps, which is why the four tags are hidden. Its pilot hand check judged 19 of 26 checked disagreements to be label gaps, but it was the agent's and unblind, and the protocol keeps an LLM's judgement out of any decision (`docs/evaluation.md`). A wider check against Lichess's generator code is ruled out by ADR-R2-3. The person judges mainly the puzzles where the theme is silent (section 4), the smallest audit that answers the open question; a full human check of every tagged puzzle would need about four times the items (section 6).
- Compute budget: the registered run reads 132 audited items (`items_read`); drawing the sample (`make_audit.py`) tags 20,080 lines in 11 cli calls, recorded in the key; no engine nodes, no profile calls, no AI tokens; cap 150%

## 1. Question

When Rematch tags a puzzle's solution with one of these four motifs and Lichess's themes do not name it, is the motif there, as Rematch defines it? A club player told "you missed a hanging piece" should find, on the first move, a knight, bishop, rook or queen that nothing guarded. Rematch names every motif a line holds, not only the puzzle's main idea, so a free piece taken with mate is still a free piece (section 4). EXP-0007 measured the share of tagged audience puzzles that carry the matching theme, `π`: 0.572 for `hangingPiece`, 0.810 for `skewer`, 0.528 for `sacrifice` and 0.606 for `backRankMate` (`lab/experiments/EXP-0007/results.json`), with recall of 0.82 to 1.00 (`decision.md` there). A recall near 1 with a low `π` says our rules are wider than the themes, not that they are wrong. This experiment asks a person, blind, which it is.

## 2. Hypotheses

A measurement study: the hypotheses name the branches of section 9.

- H0 (per tag): the audited precision `P` (section 4) is below 0.80.
- H1 (per tag): `P` is at least 0.80.

Rival explanations that would give the same answers, and how each is handled:

- A lenient auditor. A person who wants the tags back can say yes too easily. The auditor is blind to our tags and to Lichess's (section 7), and every item asks all four questions whatever it was drawn for, so each tag's question is also asked on 93 to 114 items that carry neither our tag nor the theme. A tag cannot clear while the person's share of "yes" there is not shown to be at most 0.15 (section 9). Section 6 simulates a rater who says a false "yes" 5%, 10% and 20% of the time.
- A definition that drifts. The definitions are written down here and the page shows them beside every question. They are frozen with this file, and the page's id covers them (section 7).
- Labelled puzzles taken as true. `P` takes a tagged puzzle that carries the theme as the motif (section 4). If the person's reading of our definition differs from Lichess's, that is wrong, so the person also judges 6 labelled items of `hangingPiece` and `sacrifice` and 3 of the others, mixed in blind. A tag cannot clear when two or more of them get an answer other than "yes", or when the precision with them judged too, `π·a + (1 − π)·b`, is below 0.80 (section 9).
- The definitions are ours, and close to our rules. A "yes" means "the motif as Rematch means it". For `hangingPiece`, `skewer` and `backRankMate` the definitions are close to the rules in `rules.rs`, so for those tags `b` measures partly whether the rule does what its definition says; for `skewer` the definition asks more than the rule (the front piece must be attacked along the line). Where the definitions differ from Lichess's, section 4 says how.
- Solution lines are not engine lines. As in EXP-0007, the tagger reads the puzzle's solution, not **Stockfish 19**'s best line. Every claim here is about the tags of puzzle solutions; EXP-0023 measures the engine's line.
- One rater, who knows the rules. The auditor is the maintainer, who has read `rules.rs`, this design and EXP-0007's hand check. That limit is stated in `decision.md` whatever the result, and a "clears" is reported as one rater's. A second-rater subsample is registered as optional and exploratory (section 7).

## 3. Algorithm versions compared

| Role | algorithm_id | What differs |
|---|---|---|
| Measured | `algo-3.0.0+554036a3b27c` | – |

From `cli algo show` on the lab image built from main at 4d464624. The tagger (`backend/crates/profile-core/src/motifs/`, called through `derive_move_features` with `LINE_MAX_PLIES` 8) is the one EXP-0007 measured on `algo-1.1.0+fa5e2186e1dd`: ADR 0112 hides tags at the profile and leaves the tagger and `cli lab tag-motifs` unchanged, and the release arena found no difference on any of EXP-0007's metrics. `π` is therefore read from EXP-0007's results; the key records the `algorithm_id` of the tagging, and a sample that spans more than one stops.

## 4. Primary metric and ground truth

### The definitions

Written as Rematch intends each tag, for a player who knows how the pieces move. The page shows the one-line question and, on request, the paragraph, word for word as here. A position can show several motifs at once; each question is answered on its own.

- **Hanging piece.** The first move of the line (the side to move's first move) captures a knight, bishop, rook or queen that no enemy piece was guarding, and after the capture the opponent has no legal way to take back on that square. A king counts as a guard only if it can legally take back. A pawn does not count, and nor does a piece that was guarded, even too few times (winning a piece attacked more often than it is defended is another idea). It still counts when the capture also gives check or mate. Page question: "Does the first move of the line take a knight, bishop, rook or queen that nothing guarded, so it cannot be taken back?" Lichess's description: "A tactic involving an opponent piece being undefended or insufficiently defended and free to capture." The differences: Lichess counts a piece defended too few times, and we do not; Lichess's theme names the puzzle's idea, while Rematch names every motif in the line, so a mate that starts by taking a loose rook carries our tag; and ours reads only the first move.
- **Skewer.** A bishop, rook or queen attacks a valuable enemy piece along a line (often the king, with check). That piece moves out of the way, and the same attacker then captures the piece that stood behind it on the same line. It is the reverse of a pin: in a pin the piece in front is the less valuable one and cannot move. A piece that leaves the line for another reason, and is not attacked along it, does not make a skewer. Page question: "Does a bishop, rook or queen attack a valuable piece, make it move off the line, and then take the piece behind it?" Lichess's description: "A motif involving a high value piece being attacked, moving out the way, and allowing a lower value piece behind it to be captured or attacked, the inverse of a pin." The difference: Lichess also counts a piece behind that is only attacked; we need it captured in the line. Our rule (`rules.rs`, `skewer`) does not check that the front piece was attacked along the line; the definition does, so a "no" there is our error.
- **Sacrifice.** Count material as pawn 1, knight 3, bishop 3, rook 5, queen 9, from the position on the board (after the opponent's shaded move). At some point before the side to move has won anything back, once the captures of that moment have finished, it is at least two points down: a piece given for nothing, a rook for a knight or bishop, the queen for a rook. By the end of the line it is at least two points up on the starting count, or it has given mate. A trade in which the material comes straight back is not a sacrifice, and neither is a line in which the opponent promotes a pawn while the side to move is down. Page question: "Does the side to move give up at least two pawns' worth that it does not win straight back, and still end the line two or more up, or with mate?" Lichess's description: "A tactic involving giving up material in the short-term, to gain an advantage again after a forced sequence of moves." The difference: ours has a size, two points down and two up (`SACRIFICE_PAWNS` 2), and Lichess's has none.
- **Back-rank mate.** The line ends in checkmate by a rook or a queen on the mated king's back rank (rank 1 for White, rank 8 for Black), with the king standing on that rank. The king cannot step forward off it because every square in front of it holds one of its own pieces, usually its pawns. A mate on the back rank where an enemy piece, not the king's own, covers a square in front of it does not count. Page question: "Does the line end in mate by a rook or queen on the king's own back rank, the king boxed in by its own pieces in front of it?" Lichess's description: "Checkmate the king on the home rank, when it is trapped there by its own pieces." The difference: none in the words; ours names the mating piece (a rook or a queen) and asks that every forward square hold the king's own piece. EXP-0007's critic asked whose definition the product wants: ours, as written here.

Lichess's descriptions are quoted from its published puzzle themes (CC0 data, many thanks to Lichess and its players); no code from its generator is used (ADR-R2-3).

### The metric

- Primary metric (exactly one): the audited precision of each tag, `P = π + (1 − π)·b`: of the audience puzzles we tag with it, the share that are the motif, by the theme where the theme names it, and by the person where it does not. `π` is EXP-0007's precision against the themes (its estimate, with the smaller of its linearised and Kish effective sample sizes); `b` is the share of the audited tagged puzzles without the theme that the person judges to be the motif ("unsure" counted as no for clearing, as yes for failing; section 9). It departs from the issue's "a person's verdict" for the labelled share: a puzzle both our rule and Lichess's generator call the motif is taken as the motif, the same ground truth EXP-0007 trusted four tags on, because judging that share too would take about four times the items (section 6). The labelled items the person does judge guard it (section 9).
- Ground truth: a person's verdict, blind to our tags and to Lichess's themes, against the definitions above, where Lichess's theme is silent; Lichess's theme where it names the motif. The person is the maintainer, a club-level player.
- Secondary metrics, all exploratory:
  - `P` with "unsure" left out of `b`;
  - `P` with the labelled items judged too, `π·a + (1 − π)·b`, where `a` is the share of the tag's labelled items judged "yes" (it also guards a clear, section 9);
  - per tag, the share of "yes" on distractors carrying that theme (motifs we miss) and on distractors carrying none of the four themes;
  - the share of the frame's tagged puzzles that carry the theme, against EXP-0007's `π` (a diagnostic of the frame);
  - the time per item, from the page (in `decision.md`, never in `results.json`);
  - the second rater's agreement, if one is run (section 7).

## 5. The minimum effect that matters

- Minimum effect: 0.10 from the floor of 0.80, as in EXP-0007 (section 5 there): a tag whose true precision is 0.90 should be shown, one at 0.70 kept hidden.
- Why this size: the floor is the product rule of EXP-0007. A precision of 0.80 is one named motif in five that is not there. Section 6 shows that this audit cannot reach 80% power at 0.90 inside a person's 90 minutes, and registers the precision it can reach instead.

## 6. Power analysis

- Method: a simulation of the section 9 rule, both looks and every guard included (`analysis.py power`, `power.json`), 20,000 replicates a scenario, seed `power/simulation`.
- Inputs: `π` and its effective sample size per tag from EXP-0007's `results.json`; the true `P` from 0.70 to 0.98, with `b = (P − π)/(1 − π)`; the person answers "unsure" on 5% of the items whatever the truth, and otherwise answers right (sensitivity: 0%, 10% and 20% unsure; a false "yes" on 5%, 10% and 20% of the items without the motif); labelled items are judged "yes" with probability 1 (sensitivity: 0.95 and 0.85); the items with neither our tag nor the theme are taken to lack the motif; one session of 6 blocks, or the 3 blocks of the minimum, then the continuation rule; two-sided Bonferroni over the four tags and the two looks. The error claims hold for the rater modelled; a rater whose errors depend on the tag in a way the guards cannot see is not covered.
- The threshold. With `π` at its lower bound, `b` must be shown to be at least 0.565 (`hangingPiece`), 0.127 (`skewer`), 0.594 (`sacrifice`) or 0.544 (`backRankMate`) for a tag to clear.
- Result: one session of 132 items: 30 unlabelled `hangingPiece`, 30 `sacrifice`, 24 `backRankMate` and 12 `skewer` puzzles, 18 labelled (6 each for `hangingPiece` and `sacrifice`, 3 each for the others) and 18 distractors, in 6 blocks of 22 (section 7). The probability that a tag clears, by its true `P`, with the second session when the rule calls for one (the first look alone in brackets):

| Tag | P = 0.80 | P = 0.85 | P = 0.90 | P = 0.95 | P = 0.98 |
|---|---|---|---|---|---|
| `hangingPiece` | 0.0003 | 0.02 | 0.38 (0.13) | 0.95 (0.65) | 1.00 (0.94) |
| `skewer` | 0 | 0.03 | 0.66 (0.47) | 0.99 (0.96) | 1.00 (1.00) |
| `sacrifice` | 0.0004 | 0.02 | 0.29 (0.09) | 0.90 (0.52) | 1.00 (0.87) |
| `backRankMate` | 0 | 0.01 | 0.31 (0.05) | 0.92 (0.40) | 1.00 (0.80) |

  The probability that a tag fails at a true `P` of 0.70 is 0.34 (`hangingPiece`), 0.37 (`sacrifice`) and 0.20 (`backRankMate`); `skewer` cannot fail, since `π` alone is 0.81. At `P` = 0.80 a wrong "fails" comes in at most 0.0006 of the replicates and a wrong "clears" in at most 0.0004 (0.0006 at the minimum of 3 blocks), against the 0.003125 each direction of each tag is allowed. A rater with 10% false "yes" makes a wrong clear at 0.80 in at most 0.0011 of the replicates, and the false-"yes" guard stops nearly every clear of a rater at 20%.
- What is affordable: the minimum effect is not. A tag at a true 0.90 clears with 0.29 to 0.66 probability, and 80% would take more than 80 unlabelled items a tag at each look (`n_curve` in `power.json`: 0.53 to 0.59 at 80 items), more than 300 items and four hours of a person's time. The audit is powered for a tag at 0.95: it clears with probability 0.90 to 0.99 after at most two sessions, and with 0.40 to 0.96 after one. A tag between 0.80 and 0.90 will most likely stay hidden, which is the cautious direction the product already takes.
- Sensitivity (`power.json`): "unsure" matters most. With no "unsure" the chance to clear at 0.90 rises to 0.52 to 0.73, and with 20% it falls to 0.01 to 0.44 (0.14 to 0.93 at 0.95). Labelled items judged "yes" with probability 0.85 instead of 1 lower the chance to clear at 0.95 to 0.70 to 0.78 (no "unsure"), through the guards. Stopping at the minimum of 3 blocks gives 0.41 to 0.91 at 0.95.
- The 40 seconds an item is a planning figure, not a measurement; the page records the time per item and `decision.md` reports it.

## 7. Design

- Unit of analysis: the puzzle. Each comes from a different game (EXP-0007, section 7), so no two units share one. The four tags are judged on the same items, and each tag's estimate of `b` uses only the items drawn for its `U:<tag>` stratum.
- Population: EXP-0007's audience, the puzzles of the snapshot played at least 1,000 times, with a popularity of at least 50 and a rating below 2000, less EXP-0007's pilot (1,874 puzzles, development data, which its agent hand-checked).
- The frame: a simple random sample of 20,000 audience puzzles (seed `sample/frame`), tagged through `cli lab tag-motifs` as EXP-0007 does (the position after the puzzle's first move, the rest of the solution as the mover's line). A puzzle the production code cannot play is excluded and listed in the key. Each frame puzzle falls in a stratum per tag we give it: `U:<tag>` (our tag, not Lichess's theme), `L:<tag>` (both); or, with none of our four tags, `D:miss` (one of the four themes) or `D:none` (none). A puzzle we give two of the four tags is in two strata.
- The items: from each stratum, the first puzzles of a seeded random order (seed `sample/items`), a simple random sample of that stratum of the frame, and so of the audience. A block holds 5 `U:hangingPiece`, 5 `U:sacrifice`, 4 `U:backRankMate`, 2 `U:skewer`, 3 labelled items (`L:hangingPiece` and `L:sacrifice` in every block, then `L:backRankMate` in odd blocks and `L:skewer` in even ones) and 3 distractors (odd blocks 2 `D:miss` and 1 `D:none`, even blocks 1 and 2): 22 items, 6 blocks, 132 items. A puzzle drawn for two strata is shown once and counts in both. Within a block the order is seeded (seed `sample/order`), so the strata are interleaved. The allocation gives `backRankMate` four a block because its threshold is the lowest of the three hard tags, and `skewer` two because its threshold is low (section 6).
- Practice: three puzzles from EXP-0007's pilot that its hand check did not judge, drawn in a seeded order (seed `sample/practice`) from 80 candidates: the first with none of our four tags and none of the four themes, the first with one of our tags and not its theme, and the first with both. They come first on the page and are never analysed.
- Seeds: `sample/frame`, `sample/items`, `sample/order`, `sample/practice`, `power/simulation` (`rematch_lab.experiments.seed`); no other source of randomness.
- What the person sees: a page (`make_audit.py` writes `data/exp19/audit.html`, git-ignored) that runs in a browser with no network (a Content-Security-Policy forbids every connection, and it loads no script, font or image from outside). For each item: the board after the opponent's last move (the puzzle's first move, shaded), oriented for the side to move; the solution played out move by move, both sides, in long algebraic notation (`Rf7×f1`), a check marked `+` and the final mate `#` (from the replay, EXP-0007's trusted mate check); the four questions in a fixed order, each with yes, no and unsure, and its definition a click away. The page saves every answer in the browser (`localStorage`), logs each one with its time, and downloads `answers.json`; it shows the item number ("12 of 132") and the block, and the minutes spent.
- Binding: the page's id is the sha256 of everything it shows (the items, the questions, the definitions) and of its template; `answers.json` names it, and the analysis refuses answers from another page. The key records the page's id and sha256. `make_audit.py`'s sha256 is in section 10.
- Blinding: the page holds the positions and the lines only. No puzzle id, tag, theme, rating or stratum is in it, and the items' order is seeded and interleaved. Every item asks all four questions, so the questions reveal nothing about why an item is there; on most items three of the four are questions on a motif we did not tag. 18 of 132 items carry none of our four tags, and 9 of those carry one of the four themes, so a "yes" can be right on an item we did not tag. The key (`data/exp19/sealed/key.json`, which maps items to ids, strata and tags) stays out of git and off the auditor's screen until the audit is done; `sample.csv` in git holds the drawn ids only, sorted. The person does not look a position up anywhere, on Lichess or in an engine.
- The limits of the blinding: the auditor is the maintainer, who knows this design (most items carry one of our four tags), knows the rules well enough to work out by hand whether one fires, and has seen EXP-0007's hand check. The distractors, the four questions per item, the false-"yes" and labelled guards and the written definitions limit what that knowledge can steer; they do not remove it. That is said in `decision.md` whatever the result.
- Length and stopping: the session is all 6 blocks, in one sitting or more, with breaks only between blocks. The person may stop for good before block 6 only for a reason written down before the next block would have opened, and kept with `answers.json`. Only whole blocks are analysed, and fewer than 3 is a failed result. The person sees their own answers, so an early stop is not independent of them; the written reason and the fixed length keep that room small. Nobody compares the answers with the key before the session ends: there is no interim look.
- The follow-up session: at most one, registered as its own experiment (the next free number) with `Follows: EXP-0019` if section 9's continuation rule calls for it. It keeps the same mix and all four questions: the continuing tags' unlabelled items are the next 6 blocks' worth of those strata in the same seeded orders, and the other tags' places are filled with the next puzzles of their own strata (judged, not analysed for the decision), with labelled items and distractors as here. The same definitions, and the rule of section 9 on both sessions pooled at the same level. Its error is already paid for here (two looks in the Bonferroni family).
- Inter-rater: one rater, a stated limit. Optional and exploratory: a second club-level player judges blocks 1 and 2 (44 items) on the same page, blind, before the key is committed to git; Cohen's kappa per question with its 95% interval is reported in `decision.md`. It never changes a decision.
- Development and confirmatory data: nothing of the frame has been judged. The definitions, the thresholds and the allocation were set from EXP-0007's published numbers and its pilot (development data, excluded from the frame). No answer exists before the run.
- Held fixed: the snapshot, the filters, the cli of section 3, the page as generated (its id and sha256 are in the key).
- Data: the Lichess puzzle snapshot of EXP-0007 (`external:lichess-puzzles/lichess_db_puzzle.csv`, sha256 `8ce98180bcb7cb60f3f51f12afdbe6a4fd294d8aaf4f1410e8c25e85ac78fdc9`, its section 7); EXP-0007's `results.json` (for `π`), `sample-pilot.csv` (excluded) and `pilot/hand-check.md` (its judged puzzles are not practice items); the key and the answers. `inputs.sha256` names each with its sha256: the key as an external input from the commit that draws the sample, the answers as an external input whose line holds a placeholder until the audit is done. Once the audit is done, `answers.json` (the last one downloaded) and `key.json` are committed to this folder (the key's sha256 unchanged) and `inputs.sha256` points at them, so the analysis reruns in CI without the snapshot. No real chess.com player's games.
- Leakage controls: the pilot puzzles are not in the frame; the auditor never sees the key; the answers are bound to the page by its id.
- Reuse: EXP-0007's snapshot, its population filter, its precision `π` and its effective sample sizes (`results.json`, sha256 in `inputs.sha256`), and its pilot as the source of practice items; no engine evaluation is needed.

## 8. Analysis plan

- Model or test: no model. For each tag, `b` from the items drawn for `U:<tag>`: `k` "yes" of `n`, "unsure" `u`. The bounds are one-sided at `α` = 0.05 / (4 tags × 2 looks × 2 directions) = 0.003125 each, split by the union bound between `π` (0.1 of it, a Wilson bound at EXP-0007's effective sample size) and `b` (0.9 of it, an exact Clopper-Pearson bound): the lower bound of `P` is `1 − (1 − π_low)(1 − b_low(k, n))`, and the upper bound `1 − (1 − π_up)(1 − b_up(k + u, n))`. `P` increases in both `π` and `b`, so each bound holds with at least `1 − α`.
- The guards: the labelled items' answers other than "yes" (count); `π·a + (1 − π)·b` with `a` the labelled items' share of "yes" ("unsure" as no; a point estimate); the false-"yes" share, the "yes" answers to the tag's question on the items with neither our tag nor its theme, with its one-sided 97.5% Clopper-Pearson upper bound.
- Effect size: `P`'s point estimate (`π + (1 − π)·k/n`) and the bounds above; the bounds of `π` and `b` apart.
- Multiple comparisons: Bonferroni over the four tags, both directions and both looks (the follow-up's). The guards only ever stop a clear, so they add no false clear. The secondary metrics are exploratory and uncorrected.
- Diagnostics: no fitted model. Section 6's simulation reports the error rates of the rule; the frame's share of labelled puzzles is set beside `π`.
- Exclusions and missing data: only whole blocks are analysed, a block being whole when every item in it lies at or before the last item with any answer; a question left blank there counts as "unsure". Fewer than 3 whole blocks is a failed result, reported as one, with no decision. The three practice items are never analysed. A frame puzzle the code cannot play is excluded before the draw.

## 9. Decision rule

Measurement, per tag. It departs from ADR 0106's default (a 95% interval inside a band): each tag's verdict is acted on separately, so the intervals are Bonferroni over the four tags, both directions and both looks, as EXP-0007's were over its tags; and the rule has three branches, because "shown below" and "not shown either way" lead to different actions (a rule fix, against staying hidden).

- Clears: the lower bound of `P` ("unsure" as no) is at least 0.80, and all three guards hold: at most one of the tag's labelled items got an answer other than "yes"; `π·a + (1 − π)·b` is at least 0.80; and the upper bound of the false-"yes" share is at most 0.15. The tag is named to players again, in a new major algorithm version with its ADR.
- Fails: the upper bound of `P` ("unsure" as yes) is below 0.80. The tag stays hidden and its rule is fixed under its own experiment, filed under "Next".
- Inconclusive: anything else, including a tag whose bound clears while a guard does not (the result names the guard). The tag stays hidden.

### Stopping rules

- Early stopping: none inside a session (section 7: a fixed length, an early stop only with a written reason). Look 1 is this session; look 2 is the follow-up's, if any.
- Futility and continuation: after look 1, an inconclusive tag continues to a second session only if its estimate (`π + (1 − π)·k/n`, "unsure" as no) is at least 0.88, where a clear is within reach, or at most 0.72, where a fail is. Otherwise it stays hidden with no further audit, and a later fix of its rule is the way back. If no tag continues, the experiment ends at look 1. After look 2 there is no third.
- Budget: the run aborts at the cap of section 0; an aborted run is a failed result, reported as one.

## 10. Analysis code

- Entry point: `lab/experiments/EXP-0019/analysis.py` (`python analysis.py --out DIR` writes `DIR/results.json`, with the `algorithm_id` the key was tagged on; `python analysis.py power` wrote `power.json` before the run). `make_audit.py` draws the sample and writes the page; its sha256 is `4a6fc142926efc76ebfb2e82f2bad6e485bbb149f6ba683b6d7030af8ea18f73`.
- Analysis code sha256: `f33359c98fc38f2a5a5c1d5c2c212e132dc5faf5ea69301a4f40d7edd90d4208`

## 11. Deviations

Each made on 2026-11-24, after the registration commit (481e9c2e) and before any answer existed, by the lane, in the order of the commits; `review.md` part one lists them against the critic's points.

1. `make_audit.py` filters the snapshot as it reads it (memory), before the first draw; the audience rows and their order are the same (the critic checked).
2. The page starts each item after the opponent's move, as section 7 said, where the first page started before it; the page was rebuilt.
3. The key moved to `data/exp19/sealed/`, apart from the page.
4. After the critic's review, before the auditor saw any item: section 1 no longer contradicts the hanging-piece definition, and the definitions of hanging piece and sacrifice were made exact (the first move; the king as a guard; the material count, its starting point and the promotion exception); a false-"yes" guard and the guard on `π·a + (1 − π)·b` were added; the labelled items went from 3 to 6 for `hangingPiece` and `sacrifice` (blocks of 22, 132 items); the session is a fixed 6 blocks, with only whole blocks analysed; the practice items are pilot puzzles the hand check did not judge; the page's id covers the questions, the definitions and the template, and the page logs every answer; the follow-up keeps the mix. The power analysis was rerun, the sample redrawn from the same seeds (the frame is the same) and the page rebuilt. The first page was never opened by the auditor.

- 2026-11-28, from the maths audit (ADR 0118), before the run; `analysis.py` stays as frozen (`math.md`, Findings):
  1. `follow_up` gives a second session only to a tag whose decision is exactly "inconclusive". A tag whose bound clears but a guard fails gets none, while `power.py`'s simulation gives it one. The main table loses nothing; for such a tag up to 0.21 of the chance to clear is lost. The registered code is what runs, and the decision reports any tag this applies to.
  2. Small rounding slips in section 6, listed in `math.md`.
