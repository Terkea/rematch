# 0026. Workspace packages resolve source via a development condition

Date: 2026-11-21
Status: Accepted

## Context

`@ci/chess-core` and `@ci/db` exported only their built `dist` output, so lint, typecheck and tests failed on a fresh checkout until every package had been built first. That is a hidden prebuild step and a source of stale-build bugs.

## Decision

Each workspace package's `exports["."]` lists a `development` condition first (`{ types, default }`, both `./src/index.ts`), then `types: ./dist/index.d.ts` and `default: ./dist/index.js`. `tsconfig.base.json` sets `customConditions: ["development"]`; each `tsconfig.build.json` resets it to `[]`, so production builds resolve declarations and runtime from `dist` and never pull foreign source outside `rootDir`. Builds run in dependency order via turbo's `^build`. Vitest resolves `development` by default, and the worker `dev` script runs tsx with `--conditions=development`. `test` and `typecheck` do not depend on `^build`, so a missing prebuild cannot be hidden.

## Alternatives considered

TypeScript project references with a prebuild in every script (slower, easy to forget); path aliases in tsconfig (duplicated in every tool).

## Consequences

No prebuild is needed for development or CI checks. `types` points at source, so consumers typecheck against the live code.

See ADR 0022.
