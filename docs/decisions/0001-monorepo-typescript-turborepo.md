# 0001. Monorepo in TypeScript with pnpm workspaces and Turborepo

Date: 2026-11-21
Status: Accepted

## Context

The web app, background workers and shared chess logic need to share types and code.

## Decision

One pnpm + Turborepo monorepo: apps/web, apps/worker, packages/{db,chess-core,ai,ui}, and services/maia (Python).

## Alternatives considered

Separate repos; a Python backend for everything.

## Consequences

Types are shared end to end and there is one CI pipeline. Maia needs a small Python service.

See docs/superpowers/specs/2026-11-21-chess-improver-design.md.
