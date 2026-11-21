# 0016. Freemium, with Pro at $7.99/month or $59.99/year via Stripe Managed Payments

Date: 2026-11-21
Status: Accepted

## Context

Rivals charge $8-14 a month, and a solo founder selling worldwide needs sales tax handled.

## Decision

Free and Pro tiers as in section 12 of the design, in one entitlements module. Stripe Checkout, Billing and the customer portal, with Managed Payments as merchant of record. Annual is the default, with regional pricing. Paddle is the fallback.

## Alternatives considered

Plain Stripe (the owner handles tax); Paddle first.

## Consequences

An extra 3.5% fee. Tiers are config, not code.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
