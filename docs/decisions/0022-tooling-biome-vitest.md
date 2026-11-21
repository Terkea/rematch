# 0022. Biome for lint and format, Vitest for tests

Date: 2026-11-21
Status: Accepted

## Context

The monorepo needs fast, low-configuration lint, format and test tooling that works with ESM TypeScript.

## Decision

Biome (2.x) handles lint and format in one tool, configured in the root `biome.json`. Vitest (5.x) runs tests. Each package has its own `test`, `lint` and `typecheck` scripts, orchestrated by Turborepo (2.11). A root `vitest.config.ts` uses `test.projects` (the `vitest.workspace.ts` file is no longer the mechanism in Vitest 5) so editors and a root-level `vitest` run see every package. TypeScript is pinned to 5.x (`~5.9`); pnpm 12 is installed with `npm i -g pnpm@12` because the current corepack cannot run pnpm 12.

## Alternatives considered

ESLint with Prettier; Jest.

## Consequences

One config for lint and format, and fast runs. Biome has fewer plugins than ESLint, so rules without a Biome equivalent are not enforced.

See ADR 0001.
