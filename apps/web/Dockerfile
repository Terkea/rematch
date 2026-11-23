FROM node:22-bookworm-slim AS base
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && npm i -g pnpm@12
ENV npm_config_store_dir=/pnpm-store
WORKDIR /app

FROM base AS dev
# The repo is bind-mounted at /app; dependencies are installed into volumes by the `deps` service.
COPY apps/worker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["pnpm", "--filter", "@ci/web", "dev"]

# Production image. Nothing here is used by Compose's dev stack and no secrets are baked in:
# all configuration arrives as environment variables at run time.
FROM base AS build
COPY . .
RUN pnpm install --frozen-lockfile \
 && pnpm turbo run build --filter @ci/web...

FROM node:22-bookworm-slim AS runtime
ENV NODE_ENV=production \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3100 \
    HOSTNAME=0.0.0.0
WORKDIR /app
COPY --from=build --chown=node:node /app/apps/web/.next/standalone ./
COPY --from=build --chown=node:node /app/apps/web/.next/static ./apps/web/.next/static
# The manifest icons (public/icon-*.png); standalone output leaves public/ out.
COPY --from=build --chown=node:node /app/apps/web/public ./apps/web/public
# The licence and third-party notices (ADR 0098), readable in every published image.
COPY LICENSE NOTICE /usr/share/doc/rematch/
USER node
EXPOSE 3100
CMD ["node", "apps/web/server.js"]
