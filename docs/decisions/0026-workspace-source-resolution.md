# 0026. Workspace packages resolve source via a development condition

Date: 2026-11-21
Status: Accepted

## Context

`@ci/chess-core` and `@ci/db` exported only their built `dist` output, so lint, typecheck and tests failed on a fresh checkout until every package had been built first. That is a hidden prebuild step and a source of stale-build bugs.

## Decision

Each workspace package's `exports["."]` is `{ "types": "./src/index.ts", "development": "./src/index.ts", "default": "./dist/index.js" }`. `tsconfig.base.json` sets `customConditions: ["development"]`, Vitest resolves the `development` condition, and the worker `dev` script runs tsx with `--conditions=development`. Production builds still emit `dist` and consumers without the condition use it. Biome excludes `**/test/fixtures/**` so recorded API data is never reformatted.

## Alternatives considered

TypeScript project references with a prebuild in every script (slower, easy to forget); path aliases in tsconfig (duplicated in every tool).

## Consequences

No prebuild is needed for development or CI checks. `types` points at source, so consumers typecheck against the live code.

See ADR 0022.
