---
title: What Rematch does
description: Every feature of Rematch, what it shows you and the decision behind it.
sidebar:
  order: 2
---

Rematch reads your chess.com games, finds the mistakes you repeat, explains them and trains you on them, then checks whether the training works. Each section below links to the decision record that defines it.

It is for anyone who knows how the pieces move, up to about 2000. Above 2000 it hasn't been tested with real players yet; if you're 2000+, [tell us where it's wrong](https://github.com/Terkea/rematch/issues/new?template=coaching_idea.yml).

## Analysis of every move

Your chess.com games are imported by username and kept in sync (every 30 minutes by default). A Rust worker analyses every move with a pool of Stockfish 19 engines under a pinned protocol, so the same position always gets the same verdict. Moves are judged by the loss of win chance, with the Lichess thresholds: an inaccuracy, a mistake or a blunder.

The game review marks the turning points the way a coach marks a scoresheet: a red pen ring and `??` on your blunders, the better move as a green arrow, and an evaluation graph on the win-chance scale.

Decisions: [0007](/rematch/docs/decisions/0007-analysis-protocol-pinned/), [0008](/rematch/docs/decisions/0008-lichess-win-percent-classification/), [0030](/rematch/docs/decisions/0030-rust-backend/), [0076](/rematch/docs/decisions/0076-recurring-account-sync/).

## Weaknesses and strengths, explained

Rematch measures each weakness per chance: loose pieces per move that had one, missed forks per fork available, blunders per move played with under 30 seconds left. It ranks them by what they cost you, says in one sentence what each means and why it matters, and compares you with players around your rating. Every rate shows its count and an 80% range; a pattern with too little evidence is an "early read". White and Black are split everywhere.

Decisions: [0079](/rematch/docs/decisions/0079-explained-stats-and-weakness-story/), [0080](/rematch/docs/decisions/0080-chesscom-peer-benchmark/), [0082](/rematch/docs/decisions/0082-colour-split-profile-rows/), [ADR-R2-4](/rematch/docs/decisions/adr-r2-4-weakness-metric-rulings/).

## The coach: reports, chat and a weekly review

With an Anthropic API key (or, for your own use, your signed-in Claude Code), Claude writes a coaching report, answers your questions in a chat and writes a weekly review. It never invents a fact. The report and chat may only cite positions, moves and numbers the app computed, through references that are resolved on the server, and every paragraph is validated before it is shown. A paragraph that fails is repaired once or dropped, and the answer says what was left out. The coach can ask Stockfish for a fresh check while it answers.

Without either, every report is computed-only: the numbers without commentary.

Decisions: [0060](/rematch/docs/decisions/0060-sectioned-generation-and-facts-by-reference/), [0062](/rematch/docs/decisions/0062-claude-agent-sdk-sidecar/), [0064](/rematch/docs/decisions/0064-ask-your-coach-chat/), [0093](/rematch/docs/decisions/0093-report-validator-false-drops/), [0094](/rematch/docs/decisions/0094-coach-knows-the-plan/).

## The missed-moments trainer

Positions from your own games where you missed a mate, a win or a short tactic, fell for a trap or blundered become drills. You play a move, say "Sure" or "A guess", then see the verdict, the engine's line as numbered arrows, the move you played in the game and a link to that moment. A right answer comes back after 1, 3, 7, 21 and then 60 days; a miss comes back tomorrow; three clean passes at least a week apart retire a position. Sessions are at most 12 positions, mixed across sources, and never name the motif before you answer.

About one position in five is held back and never drilled. "Is it working?" compares your rate per chance before and after your first drill, and the motifs you drilled with those only held-back positions have. It says "Too early to tell" until it has 30 chances on a side.

Decisions: [0090](/rematch/docs/decisions/0090-missed-moments-trainer/). Evidence: [what helps amateurs improve](/rematch/docs/research/2026-11-23-what-helps-amateurs-improve/).

## A weekly focus

One weakness at a time, with a measurable goal: get it down to a target share of your chances over your next N games (10, 20, 30 or 40). The target is a quarter fewer than your current rate, or your peers' median when that is easier to reach. The card shows games played, the rate with its range and a verdict, "On track" or "Not yet", only from 10 chances. It never says "improved": it compares with the goal.

Decisions: [0091](/rematch/docs/decisions/0091-weekly-focus-and-goal/).

## Rating trajectory

Progress opens with your rating line, the peak marked, and the games around the peak against your last games: rating, accuracy, blunders per game, score and your top three weaknesses. A measure is "clearly worse" or "clearly better" only when the evidence separates, and below 20 rated games there is no trajectory at all, just how many games it needs.

Decisions: [0089](/rematch/docs/decisions/0089-rating-trajectory-and-then-vs-now/).

## A focused overview

The overview reads top to bottom: where you are, this week's focus, one button to practise today, how you're doing by colour, then the detail.

Decisions: [0092](/rematch/docs/decisions/0092-focused-overview/).

## Also in the app

- A position explorer that opens any move the app names, with the engine line drawn on the board ([ADR-FIX-F-1](/rematch/docs/decisions/adr-fix-f-1-position-explorer/)).
- Filters by time control, colour and dates, and a period comparison ([0083](/rematch/docs/decisions/0083-filters-and-period-comparison/)).
- Original, synthesised move sounds, and pieces that move with a short, weighted animation (reduced motion respected) ([ADR-WEB-13](/rematch/docs/decisions/adr-web-13-board-pieces-and-motion/), [ADR-WEB-17](/rematch/docs/decisions/adr-web-17-original-procedural-sounds/)).
- Day and night themes, from one design system ([ADR-WEB-7](/rematch/docs/decisions/adr-web-7-design-tokens/)).
