# 0019. Hosting: Vercel for the web app, a dedicated-CPU host for the workers

Date: 2026-11-21
Status: Accepted

## Context

The workers need sustained CPU, and the web app needs edge rendering and SSR.

## Decision

Vercel for apps/web. The workers, Redis, Postgres and Maia run on Hetzner dedicated-vCPU servers in Docker (the cheapest sustained CPU), with Fly.io as the alternative. The final choice and spend need owner approval before anything is deployed.

## Alternatives considered

Everything on Fly.io; everything on AWS.

## Consequences

Two providers to operate. chess.com API access must be tested from the chosen worker IPs.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
