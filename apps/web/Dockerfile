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
