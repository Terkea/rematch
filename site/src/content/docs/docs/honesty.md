---
title: The honesty rules
description: How Rematch keeps every number and every sentence true to your games.
sidebar:
  order: 3
---

The most common complaint about AI chess coaching is that it invents moves and evaluations. Rematch is built around one rule from its design spec:

> Stockfish and our own statistics produce every fact. Claude only explains and prioritises those facts. Every claim in a report links to real positions from the player's games.

Everything below follows from it. These rules bind the code, the AI prompts and the interface, and a review that breaks one fails.

## Numbers

- **A count with every rate.** "Based on 38 rapid games" is printed with the rate it explains, once per section.
- **One interval convention.** Every estimate carries an 80% interval: Wilson for rates, a seeded bootstrap for means, written out in words ("80% range 3.1% to 5.6%") as well as drawn. Interval ends are rounded outward, so the stated range is never narrower than the computed one.
- **Unknown is never zero.** Below 10 games there is no number, no band and no trend, only the shortfall: "Need 9 more games". A week without games is a gap, not a zero. No page may render `NaN`, `Infinity` or `undefined`; the quality tests search every rendered page for them.
- **Better or worse only when it's clear.** A change is called better or worse only when the intervals separate. Otherwise: "No clear change". The weekly focus compares you with your goal and never says "improved". "Is it working?" waits for 30 chances on a side.
- **Fixtures are labelled.** A demo or synthetic number is never shown as yours.

Decisions: [0047](/rematch/docs/decisions/0047-honest-overview-statistics/), [ADR-WEB-12](/rematch/docs/decisions/adr-web-12-kpi-stat-honest-uncertainty/), [ADR-WEB-19](/rematch/docs/decisions/adr-web-19-one-statistics-convention-and-layout-polish/), [0089](/rematch/docs/decisions/0089-rating-trajectory-and-then-vs-now/), [0091](/rematch/docs/decisions/0091-weekly-focus-and-goal/).

## Words written by the AI

- **Facts by reference.** The model never types a move, an evaluation or a number. It writes tokens such as a position reference or a path into the computed data, and the server renders them from the stored facts.
- **Validated before it is shown.** Each paragraph is checked: every citation must exist, every token must resolve, no fact may appear as free text, and every move or square must belong to a cited position. Engine words ("mates", "winning") need engine evidence.
- **Dropped, and said so.** A report item that fails is dropped. A chat answer gets one repair attempt, then the failing part is left out and the answer says "One statement was left out because the engine could not confirm it."
- **Labelled.** Every AI text says it was written by an AI model, from the player's analysed games.
- **Honest wording survives.** The validator drops claims, not careful phrasing: "first, …" as a word of discourse and "practise mate patterns" as training advice are allowed (decision 0093).

Decisions: [0011](/rematch/docs/decisions/0011-claude-structured-outputs/), [0060](/rematch/docs/decisions/0060-sectioned-generation-and-facts-by-reference/), [0064](/rematch/docs/decisions/0064-ask-your-coach-chat/), [0071](/rematch/docs/decisions/0071-coach-text-and-states/), [0093](/rematch/docs/decisions/0093-report-validator-false-drops/).

## Colour and marks

Red means one thing: your mistake (and, in forms, an error). It is never decoration. Every mark that colour distinguishes also differs in shape, so colour never carries meaning alone: mistakes carry a `?`-family badge and an open arrowhead, good moves a `!`-family badge and a solid one. The design system is in [`packages/ui/DESIGN.md`](https://github.com/Terkea/rematch/blob/main/packages/ui/DESIGN.md).
