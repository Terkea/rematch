---
id: EXP-0019
title: A blind human audit of the motif tags the Lichess themes cannot judge
status: pre-registered
algorithm: algo-3.0.0+554036a3b27c
issue: 117
updated: 2026-11-24
---

- The question: when Rematch calls a puzzle a hanging piece, a skewer, a sacrifice or a back-rank mate and Lichess's themes do not, is the motif there? `algo-3.0.0` hides these four tags until they clear (ADR 0112).
- A person (the maintainer, a club-level player) judges 132 puzzle solutions against definitions written down first, blind to our tags and to Lichess's, with distractors mixed in: about 90 minutes, on a page that runs with no network.
- The theme decides where it names the motif and the person where it is silent. A tag clears when that audited precision is shown to be at least 0.80 (Bonferroni bounds over the four tags) and the guards hold: the person agrees with the theme on the labelled puzzles and rarely says yes where neither we nor Lichess see the motif.
- Powered for a tag at 0.95, which clears with probability 0.90 to 0.99 after at most two sessions. A tag at 0.90 clears only 0.29 to 0.66 of the time, so most tags between 0.80 and 0.90 would stay hidden.
- One rater who knows the rules: any clear is reported as that, with a second rater's check optional.
