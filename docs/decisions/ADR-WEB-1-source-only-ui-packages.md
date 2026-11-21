# ADR-WEB-1. Source-only UI packages and the web dependency set

Date: 2026-11-21
Status: Accepted

## Context

`apps/web` (Next 16) and `packages/ui` (shared components) are consumed only inside this monorepo, and Next compiles them with `transpilePackages`. ADR 0026 gives every library package a `development` export condition that points at source and a `default` that points at a built `dist`. A UI package built to `dist` would add a build step to every component edit and would break Tailwind class scanning, which needs to see the source.

## Decision

`@ci/ui` exports source only: `"./*": { "development": "./src/*/index.ts", "default": "./src/*/index.ts" }`, plus `"./tokens.css"`. Each component folder owns an `index.ts`, so no two lanes share a barrel file. `@ci/web` is an application and is never imported. Both packages are the one recorded exception to ADR 0026's `dist` default, and neither has a `build` script of its own for UI (the web app builds with `next build`).

Relative imports inside `apps/web` and `packages/ui` are extensionless, because both use `moduleResolution: "Bundler"`. `@ci/db` and `@ci/chess-core` keep their NodeNext `.js` imports; `next dev` runs on webpack with `resolve.extensionAlias` so those resolve to `.ts` (see ADR-WEB-2).

All dependencies for this milestone are installed in one commit, so later lanes never touch `pnpm-lock.yaml`.

## Versions and licences (npm view, 2026-11-21)

| Package | Version | Licence |
|---|---|---|
| next | 16.4.6 | MIT |
| react, react-dom | 19.3.1 | MIT |
| tailwindcss, @tailwindcss/postcss | 4.3.5 | MIT |
| motion | 14.14.0 | MIT |
| better-auth | 1.7.27 | MIT |
| drizzle-orm | 0.45.4 | Apache-2.0 |
| ioredis | 6.0.0 | MIT |
| @tanstack/react-query | 5.108.2 | MIT |
| react-chessboard | 5.12.1 | MIT |
| chess.js | 1.4.0 | BSD-2-Clause |
| @visx/visx | 4.0.0 | MIT |
| @number-flow/react | 0.6.2 | MIT |
| sonner | 2.0.8 | MIT |
| vaul | 1.1.2 | MIT |
| cmdk | 1.1.1 | MIT |
| next-themes | 0.4.6 | MIT |
| @phosphor-icons/react | 2.1.10 | MIT |
| shadcn (CLI) | 4.23.0 | MIT |
| @base-ui/react | 1.9.0 | MIT |
| nodemailer | 10.6.1 | MIT-0 |
| @types/nodemailer | 8.0.2 | MIT |
| @playwright/test | 1.65.1 | Apache-2.0 |
| @playwright/mcp | 0.0.86 | Apache-2.0 |
| @testing-library/react | 16.3.3 | MIT |
| @testing-library/jest-dom | 8.0.0 | MIT |
| @testing-library/user-event | 14.6.13 | MIT |
| jsdom | 31.0.1 | MIT |
| @vitejs/plugin-react | 6.3.0 | MIT |
| tailwind-merge | 3.7.1 | MIT |
| clsx | 2.1.1 | MIT |
| class-variance-authority | 0.7.1 | Apache-2.0 |
| tw-animate-css | 1.4.0 | MIT |
| zod | 4.6.7 | MIT |
| tsx | 4.23.15 | MIT |
| @axe-core/playwright | 4.14.0 | **MPL-2.0 (not installed)** |

`@axe-core/playwright` is MPL-2.0, which is outside the allowed MIT, ISC, BSD and Apache set. It is not installed. The decision on accepting it (it is a dev-only e2e dependency, never shipped) is with the controller. MIT-0 (nodemailer) is the MIT licence without the attribution clause and is treated as permissive.

## Alternatives considered

Building `@ci/ui` to `dist` (slower edit loop, breaks Tailwind source scanning); a single barrel `index.ts` (merge conflicts between lanes).

## Consequences

Component edits show up immediately in the dev server. Extensionless relative imports are required in `apps/web` and `packages/ui`. Any package outside the allowed licence set stops the task.
