# 0024. Weakness timeline with change-point detection

Date: 2026-11-21
Status: Accepted

## Context

Players change over time. They start making new kinds of mistakes and fix old ones. The owner wants the platform to find out when a mistake starts, when it improves, and when it comes back.

## Decision

Monitor every weakness series (W1-W12, each motif, each opening line, each endgame type) continuously, not just the ones in the active plan. Each series runs Bayesian online change-point detection on per-opportunity rates, with CUSUM as an early warning. Confirmed changes record an estimated onset game and date. Each series moves through the states not_an_issue, emerging, active, improving, resolved and relapsed. Transitions are stored in weakness_state_history, and Claude explains them with likely causes (time-control mix, new openings, session length, opponent strength) plus examples from before and after the onset. See design section 12b.

## Alternatives considered

Fixed rolling windows only (slow to react, and no onset date); monitoring only the focus weaknesses (misses new problems).

## Consequences

Needs two more tables plus incremental detection after each sync. Correlated causes are shown as hypotheses, never as proof.
