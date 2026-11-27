# Progress

<!--
Add an entry when a release ships, an experiment is decided, or a milestone closes.
Format: `## YYYY-MM-DD: title`, then 2 to 4 hyphen bullets with real numbers and links. Newest first.
The site builds this file into /docs/progress/ and shows the newest entry on the landing page.
-->

See also the [experiments](/rematch/docs/experiments/) and the [roadmap](/rematch/docs/roadmap/).

## 2026-11-27: algo-6.0.0 calls the rating's direction again, with a range that counts its memory

- The trajectory compares your first 30 rated games with your last 30 and says whether your rating moved by more than its own wander, from a rule that counts how a rating carries its error from one game to the next ([ADR 0117](decisions/0117-a-trajectory-verdict-that-counts-the-ratings-memory.md), [EXP-0030](https://github.com/Terkea/rematch/blob/main/lab/experiments/EXP-0030/decision.md)). The headline compares the two averages and states the peak as a plain fact: "Your rapid rating averaged 1,492 over your last 30 games, down from 1,560 over your first 30 (−68). It is 1,488 now. Your highest was 1,610, in April 2026."
- On 2,000 fresh simulated players whose strength never changed it said "falling" to 10% to 12% and "rising" to 8% to 10% at 60, 120 and 250 rated games, against the old rule's 46% to 79% "falling", and it caught a 200-point fall 72% to 99% of the time. With breaks, streaks and tilt it ran at 13% to 14%.
- On real Lichess players over a month, 36% were flagged either way against the model's 20%: real ratings wander, or really change, more than the model allows, so the rate on real players is not yet known. EXP-0031 (long histories and new accounts) and EXP-0032 (real series at the product's windows) are next.
- An input the coach stored before this version keeps its verdict withheld: the trajectory now names the rule that computed it, and only the cleared rule's verdict is stated.

## 2026-11-26: algo-5.0.0 stops calling the rating rising or falling

- The trajectory no longer tells a player or the coach that their rating is falling, rising, clearly worse or clearly better than at their peak. The headline states the peak, the current rating and the change as numbers ("Your rapid rating peaked at 1,080 in February 2026 and is 816 now (−264)."), and the then | now table shows both windows' average ratings and the difference, marked "Not called" ([ADR 0116](decisions/0116-hide-the-trajectory-rating-verdict.md), #136).
- That is EXP-0015's decision: the rule told 44% to 78% of simulated players whose strength never changed that their rating was falling. Accuracy, blunders per game, score and the top weaknesses keep their "better" and "worse", and the coach's inputs stored before the upgrade are cut the same way when it reads them. A report written before the upgrade keeps its words until the next one replaces it.
- The rule still runs behind the switch, so the lab and EXP-0030 measure it exactly as before; turning the verdict back on for a rule that passes is one line and a major version.
- The release arena against `algo-4.0.1` found no clear difference on any metric of the six live experiments (EXP-0001, EXP-0002, EXP-0007, EXP-0008, EXP-0009 and EXP-0015): nothing the lab measures moved, which is the point.

## 2026-11-26: EXP-0015, the trajectory's "falling" is mostly a false alarm

- We simulated 700 players whose strength never changes, rated game by game by Glicko-2 with Lichess's own constants, and sent each history through the production trajectory ([EXP-0015](https://github.com/Terkea/rematch/blob/main/lab/experiments/EXP-0015/decision.md), #110). It told 44% of them their rating was falling at 60 rated games, 62% at 120 and 78% at 250, against the 15% we registered as acceptable, and told 44% and 30% at 60 and 120 games that it was rising.
- The cause is in the statistics, not the chess: the rule's 80% range for the change is about 4 points each side where an honest one would be 26 to 47. A rating remembers its last games (a correlation of 0.97 from one game to the next), and the bootstrap treats 30 consecutive ratings as 30 independent ones; at 120 and 250 games the earlier window also sits on a selected peak.
- So the rating's verdict is to be hidden until a fixed rule passes the same test: the headline keeps the peak, the current rating and the change as facts, without "falling", "rising", "Worse" or "Better", and the coach stops being told the rating is falling. The accuracy and blunders comparisons stayed within the bound and keep their verdicts. The fix is EXP-0030; the run took 98 seconds with no engine.

## 2026-11-26: EXP-0009, no data-driven teaching order yet

- We followed 16,204 anonymised Lichess rapid players rated below 2000 from July to October 2026, by a salted hash of their name with nothing else kept ([ADR 0114](decisions/0114-following-anonymised-lichess-players-across-months.md)), and measured eight of Rematch's weaknesses, accuracy and the blunder rate from the evaluations Lichess already published, with no engine time ([EXP-0009](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0009)).
- No weakness whose rate fell from August to September came before a rise in October's rating, and neither did accuracy, the check that the design could see such a link. The likeliest reason is the design itself: September's rating, where the outcome starts, comes from the same games as September's rates, so October's drift back from September's luck pulls every estimate down. A month of 10 to 40 analysed games also measures a player's change with a reliability of only 0.02 to 0.13.
- So the teaching order stays as the research notes set it, and we say plainly that it is not data-driven yet. The one follow-up the rule allows starts the outcome away from the measured games and uses three-month windows on other months (EXP-0029); measuring the other nine weaknesses with our own analysis would cost about 4,560 CPU-hours at this design and does not run.

## 2026-11-24: algo-4.0.0 analyses every position at 1 million nodes

- Every position is now searched at 1 million **Stockfish 19** nodes instead of 300,000, the count EXP-0008 chose: 7.45% of move labels change, mostly fewer inaccuracies, and game accuracy falls by 0.42 of a point on average ([ADR 0113](decisions/0113-analyse-at-1-million-nodes.md), #128).
- The engine costs 39.1 CPU-seconds a game instead of 11.8, so a 300-game import is about 3.3 CPU-hours: about 49 minutes on 4 engines and 25 on 8. Games analysed before keep their 300,000-node labels until EXP-0027 says whether they are worth redoing.
- The release arena against `algo-3.0.0`: EXP-0008 better (the version ships the count its rule chooses), and no clear difference on EXP-0001 (recall at 2 0.633 on both sides), EXP-0002 (worst coverage 0.835) or EXP-0007 (the same 9 of 15 tags trusted), 2.8 CPU-hours in all.

## 2026-11-24: EXP-0008, 300,000 nodes falls short of 1 million

- We searched every position of 160 rated rapid Lichess games (CC0) at 50,000 to 1 million nodes and compared each move's label with **Stockfish 19** at 30 million. 1 million nodes disagrees with it on 2.5 moves in 100 fewer than today's 300,000 (95% 2.0 to 3.0), and on 2.2 fewer against 100 million nodes on 40 of the games, past the registered tolerance of 1 in 100 ([EXP-0008](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0008)).
- For a player, 7.4% of move labels change, mostly fewer inaccuracies, and game accuracy falls by 0.4 of a point on average. The engine costs 39 CPU-seconds a game instead of 12: a 300-game import goes from about 15 to 49 minutes on 4 engines.
- The deep reference is not the truth either: on the moves where 300,000 and 1 million disagree, 12% of its labels changed again between 30 and 100 million nodes, so the result says 1 million agrees more with deeper searches, not that its labels are exact.
- Next: ship 1 million nodes as a major algorithm version, test searching deeper only where the cheaper searches disagree (EXP-0028, about 22 CPU-seconds a game), and measure whether players' top weaknesses move (EXP-0027).

## 2026-11-24: algo-3.0.0 stops naming the six motifs EXP-0007 did not clear

- Hanging piece, pin, skewer, deflection, sacrifice and back-rank mate are no longer named anywhere a player or the coach reads: no "Missed pins" row, no timeline change, no tag on a coached position and no glossary entry. The moves behind them still count as missed tactics or blunders, with their eval swing and the best move ([ADR 0112](decisions/0112-hide-the-motifs-exp-0007-did-not-clear.md), #122).
- Fork, discovered attack, promotion, smothered mate and the mates in 1 to 4+ stay named; EXP-0007 found each right at least 97% of the time. Mating net has no Lichess theme to be scored against and stays as it was.
- The tagger still computes all sixteen tags, so the arena measured `algo-3.0.0` on EXP-0007 exactly as before: the same 9 of 15 tags trusted and every precision unchanged, with no clear difference on EXP-0001 or EXP-0002 either. The replay fixtures lose 112 of 435 series points and 234 of 2,964 KPI rows, all of them the six tags.
- Next: the six come back one by one, each with an experiment behind it: the blind audit (EXP-0019), then the deflection and pin fixes (EXP-0020, EXP-0021).

## 2026-11-24: EXP-0006 ships algo-2.0.0, the 80% ranges calibrated

- On fresh synthetic players well above their peers, the 80% ranges now hold the true rate about 80% of the time: all 20 weakness-by-sample-size cells are inside 75–85% (the worst 83%), where the old rule's range for missed mates at 30 games held the truth 10% of the time. The peers' prior is worth at most two of the player's own games, and recency-weighted games count at their effective size, divided by how much they vary from game to game. The synthetic players' spread and peer rates are assumptions; real players come with Tier 2 ([EXP-0006](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0006), [ADR 0111](decisions/0111-prior-rule-variants-and-the-80-percent-ranges.md)).
- The cost is measured: about one more credible estimate in 110 for players at their peers' level (a credible estimate still needs the excess floor before it ranks), inside the 1-in-100 margin registered first with little to spare, and most for missed mates and early traps. The benchmark finds the same flaws (28 of 45 in both versions), and its smoke coverage rises from 65% to 80%.
- EXP-0005 (the churn between versions) was closed unrun: no result it could give would have changed the benchmark's size. Its measurement ran inside EXP-0006 instead: the new rule lost none of 28 correct top-2 weaknesses at 80 games, though rank 1 changed for 12 of 68 players at 30 games.
- Next: #13's last case, a hanging-piece player far above peers who rarely hang anything, is covered 77% at 30 games, below 80% (EXP-0025); what the extra false weaknesses mean for clean players is EXP-0013.

## 2026-11-24: EXP-0007, the motif tags against the Lichess puzzle themes

- We ran the real Rust tagger on 54,987 Lichess puzzles (CC0, many thanks to Lichess and its players), each solution standing in for the engine's best line, and scored it against Lichess's themes. Fork is right 98% of the time and finds 72% of the labelled forks; discovered attack 99% and 86%; promotion and smothered mate match almost exactly ([EXP-0007](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0007)).
- Six tags can't be shown to be right four times in five: deflection (56%, mostly attraction) is ours to fix; skewer (81%, not shown to clear 80%), back-rank mate, hanging piece, sacrifice and pin wait for a blind human audit, since the themes don't name every motif in a puzzle. Pin also finds only 20% of the labelled pins.
- Next: stop naming those six to players until they clear, the audit (EXP-0019), fixes for deflection and pin (EXP-0020, EXP-0021), motifs after the first capture (EXP-0022), and the engine's own line (EXP-0023).

## 2026-11-24: Champion and challenger

- Decided experiments no longer retire. Each one is live and re-runs against every new algorithm version on its registered seeds; a challenger ships only if it beats the current champion without losing ground on the guarded measures ([ADR 0109](decisions/0109-champion-and-challenger.md)).
- EXP-0002 re-runs on every pull request that bumps the algorithm (0.44 CPU-hours, no engine). EXP-0001 re-runs on each release (2.44 CPU-hours with Stockfish), and the release images wait for it.
- The Experiments page has a leaderboard: one row per algorithm version, one column per live experiment, with intervals. Today it has one row, `algo-1.1.0`, the champion.
- Known weaknesses, written down: every version reuses the same seeds, so a challenger could be tuned to them, and there's no correction yet for comparing many measures at once.

## 2026-11-24: The first two experiments are decided

- EXP-0001 sized the benchmark with a power analysis on the real Rust pipeline. Reliably seeing a 0.1 gain in "finds the flaw in its top 2" needs 40 synthetic players per flaw; the weekly budget allows 22, which reliably sees 0.15. The suites were resized to match (smoke 1,280 games, full 8,960), and 10 of the pilot's 11 misses are missed mates and early traps, so that's where gains have to come from ([EXP-0001](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0001)).
- EXP-0002 checked whether the 80% ranges hold 80% of the time. For players well above their peers, 12 of 20 weakness-by-sample-size cells fall outside 75–85%, worst for missed mates at 30 games (10%). The ranges are pulled towards the peers, so they miss high ([EXP-0002](https://github.com/Terkea/rematch/tree/main/lab/experiments/EXP-0002), [#13](https://github.com/Terkea/rematch/issues/13)).
- Both were pre-registered, reviewed by an independent critic before and after the run, and re-run to byte-identical results. Next: EXP-0005 measures how much the rankings move between two real versions, and EXP-0006 tests a fix for the priors.

## 2026-11-23: The algorithm science lab and algo-1.1.0

- Every change to the recommendation algorithm now needs a pre-registered experiment: the question, H0 and H1, the primary metric, the minimum effect, the sample size and the decision rule are committed before anything runs ([ADR 0106](decisions/0106-algorithm-science-lab.md), [evaluation](evaluation.md#how-we-do-science-on-the-algorithm)).
- The lab in `lab/` drives the real Rust algorithm from Python (`cli lab analyse|profile|rank`), so no second implementation exists. Lab CI checks the commit order in git, the frozen sections, the input hashes and that two runs give byte-identical results.
- Nine vetted scientific skills are vendored and pinned, and the first four experiments are planned (EXP-0001 to EXP-0004).
- `algo-1.1.0` is the first version bump through the new guard: a refactor in the language layer with no output change, replay goldens byte-identical ([changelog](https://github.com/Terkea/rematch/blob/main/algorithm/CHANGELOG.md)).

## 2026-11-23: Tier 1 benchmark, and the ranking fix

- The Tier 1 benchmark runs the production pipeline on synthetic players with injected flaws of known rates, five flaws and one clean control, Stockfish at 12,000 nodes ([ADR 0100](decisions/0100-pipeline-benchmark-synthetic-players.md), [benchmark](benchmark.md)). Its first scorecard found two of the five flaws in the top 2, and the clean control had a rank-1 weakness.
- [ADR 0101](decisions/0101-weakness-ranking-floor-and-baselines.md) ranks a weakness only when its 80% range lies above the benchmark and it costs at least 0.02 points a game, and adds peer baselines for hanging pieces (W1) and thrown-away wins (W5), which had none and so were never ranked.
- Before and after: flaw in the top 2 from 40% to 60% (the target is 80%), the clean control with a rank-1 weakness from 1 of 1 to 0 of 1, mean games to a top-2 detection from 55 to 45.
- Still failing, and published: the 80% ranges hold the true rate 60% of the time (12 of 20 intervals), down from 80% and below the 65% threshold, and missed mates are measured but not ranked.

## 2026-11-23: A versioned recommendation algorithm

- Every recommendation now names the rules that produced it: `algo-<version>+<hash>`, from `algorithm/manifest.json`, which lists about thirty constants, protocols, prompt hashes and model ids ([ADR 0105](decisions/0105-algorithm-manifest-stamping-and-replay.md)). The first version is `algo-1.0.0`.
- A replay suite imports 19 fixture games and runs analysis, the profile and the coaching input twice; both runs must equal the committed goldens byte for byte.
- A CI guard fails a pull request that changes a component without the version bump and a changelog entry in `algorithm/CHANGELOG.md`, and a ban test fails any unseeded randomness or clock read in the algorithm's crates.

## 2026-11-23: Factual and deterministic, evaluation first

- [ADR 0104](decisions/0104-factual-and-deterministic-four-layers.md) splits the product into four layers (facts, statistics, recommendations, language), each with its own ground truth: **Stockfish**, **Syzygy** tablebases, the Lichess puzzle database and lila's accuracy formula ([evaluation](evaluation.md)).
- The roadmap was rewritten evaluation-first: measure what the algorithm does before changing it.
- Stockfish now gets `Threads 1` and `Hash 16` explicitly, instead of relying on defaults that happen to equal them; the evaluations are identical with and without.

## 2026-11-23: Open source as Rematch

- The repository went public as Rematch under the AGPL-3.0, with a README, community files, a contributing guide and a release process ([ADR 0096](decisions/0096-public-face-readme-and-site.md)).
- This site went live on GitHub Pages at terkea.github.io/rematch, built from `site/` on every push to `main`.
- `main` is protected by a ruleset: changes arrive by pull request with the required checks passing, and the repository settings live in `.github/settings` as code ([ADR 0095](decisions/0095-open-source-sdlc-and-releases.md)).

## 2026-11-23: Self-hosting in one script

- `scripts/setup.sh` merges `.env`, checks Docker, memory, disk and ports, starts the services the app needs and waits for the worker's migrations; `doctor` re-checks everything and explains each failure ([ADR 0097](decisions/0097-self-hosting-setup-and-demo-data.md)).
- A demo player (`pnpm seed:demo`) and the data downloads (Syzygy tables, the Lichess reference) are part of the setup, so a fresh clone is not an empty app.
- Written after a clean-clone run on Windows 11 found six snags, among them `nvm use` ignoring `.nvmrc` and a missing shared cargo volume stopping `docker compose up`.
