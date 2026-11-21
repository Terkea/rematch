# 0018. Age gate and privacy defaults

Date: 2026-11-21
Status: Accepted

## Context

Many chess players are children, and COPPA's amended rule and the UK Children's Code apply.

## Decision

Block under-13s. Protective defaults for 13-17s. Write a DPIA. Review streaks and notifications for nudge techniques. Support data export and deletion, and encrypt OAuth tokens.

## Alternatives considered

Ignore it; a full COPPA parental-consent flow.

## Consequences

Under-13s are excluded at launch.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
