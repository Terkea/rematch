# syntax=docker/dockerfile:1
# Claude Agent SDK sidecar (ADR-R3-6). Build context: the repository root.
# bookworm (glibc), so the SDK's linux-x64 `claude` binary resolves from its optionalDependencies.

FROM node:22-bookworm-slim AS build
RUN npm i -g pnpm@12.21.0
WORKDIR /src
# The workspace manifests first, so the install layer is cached until a dependency changes.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY apps/web/package.json apps/web/
COPY apps/worker/package.json apps/worker/
COPY packages/chess-core/package.json packages/chess-core/
COPY packages/db/package.json packages/db/
COPY packages/ui/package.json packages/ui/
COPY services/ai-sidecar/package.json services/ai-sidecar/
# Optional dependencies are kept: they carry the native `claude` binary. Never pass --no-optional.
RUN pnpm install --frozen-lockfile --filter @ci/ai-sidecar...
COPY services/ai-sidecar services/ai-sidecar
RUN pnpm --filter @ci/ai-sidecar build \
 && pnpm --filter @ci/ai-sidecar deploy --prod --legacy /out

FROM node:22-bookworm-slim AS runtime
WORKDIR /app
COPY --from=build --chown=node:node /out /app
# workDir and configDir live here. Compose mounts a tmpfs over it: the CLI's disk is scratch, and
# durable chat transcripts are in Postgres through the session store.
RUN mkdir -p /var/lib/ai-sidecar \
 && chown node:node /var/lib/ai-sidecar \
 && chmod 0700 /var/lib/ai-sidecar
USER node
EXPOSE 7310
HEALTHCHECK --interval=10s --timeout=5s --start-period=10s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:7310/health').then(r=>process.exit(r.ok?0:1))"
CMD ["node", "dist/index.js"]
