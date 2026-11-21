# chess-improver

A chess coaching platform: analysis of your games, improvement plans and training. TypeScript monorepo (pnpm + Turborepo).

## Getting started

```sh
nvm use            # Node 22
npm i -g pnpm@12   # or corepack, if your corepack supports pnpm 12
pnpm install
pnpm build && pnpm test && pnpm lint && pnpm typecheck
```

Layout: `apps/*` (web, worker), `packages/*` (shared libraries, scope `@ci/*`), `services/*`.
Design decisions live in `docs/decisions/`; the spec is in `docs/superpowers/specs/`.
