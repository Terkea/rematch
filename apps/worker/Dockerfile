FROM node:26-bookworm-slim AS base
# Stockfish 19, generic x86-64 Linux build (runs on any x86-64 CPU).
# Asset: stockfish-linux-x86-64-universal.tar.gz (release sf_19)
ARG STOCKFISH_ASSET=stockfish-linux-x86-64-universal.tar.gz
ARG STOCKFISH_SHA256=9defc0d4e55d49c65a6d042f3e571a39fcea499ade6dbe741b53b8c65e03611f
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && curl -fsSL -o /tmp/sf.tar.gz \
      "https://github.com/official-stockfish/Stockfish/releases/download/sf_19/${STOCKFISH_ASSET}" \
 && echo "${STOCKFISH_SHA256}  /tmp/sf.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/sf.tar.gz -C /tmp \
 && install -m 0755 /tmp/stockfish/stockfish-linux-x86-64-universal /usr/local/bin/stockfish \
 && rm -rf /tmp/sf.tar.gz /tmp/stockfish
RUN npm i -g pnpm@12
ENV STOCKFISH_PATH=/usr/local/bin/stockfish \
    npm_config_store_dir=/pnpm-store
WORKDIR /app

FROM base AS dev
# The repo is bind-mounted at /app; dependencies are installed into volumes.
COPY apps/worker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh
ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["pnpm", "--filter", "@ci/worker", "dev"]
