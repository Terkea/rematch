# The algorithm lab (ADR 0106): Python 3.12 and uv, the locked analysis packages, the Rust `cli`
# and Stockfish binaries taken unchanged from a Rematch build, and the web's rules as one Node
# bundle (ADR 0115). No Rust is compiled here and nothing in this image runs in the product.
#
# The binaries come from the build context named `rematch`, the web's sources from `repo`:
#   - compose (`docker compose --profile lab build lab`) passes the `rust-worker` image and the
#     repository;
#   - by hand: docker build --build-context rematch=docker-image://<rust-worker image>
#     --build-context repo=. lab, with `rematch` also a directory holding usr/local/bin/cli and
#     usr/local/bin/stockfish (a lane build).
#
# Stages:
#   base  Python and uv only: regenerate the lock with
#         docker build --target base -t rematch-lab-base lab
#         docker run --rm -v "$PWD/lab:/lab" -w /lab rematch-lab-base uv lock
#   webbridge  Node and the locked web dependencies (pnpm, the committed lockfile): bundles
#         apps/web/scripts/lab-bridge.ts, the weekly focus and trainer rules, into one file.
#   lab   (default) the frozen environment, the binaries, Node with that bundle and the lab
#         sources; CMD runs pytest.
ARG PYTHON_IMAGE=python:3.12-slim@sha256:7a8b475003c4fe15a2cd4e55e5cfc2f3560bdc9333d624f24cdd6d4340fd7a17
ARG NODE_IMAGE=node:22.23.5-slim@sha256:e9f565d32859f0cbfb52125eafe8be39390f24dac302dc3cdf566e8ba3ab21ee
ARG UV_IMAGE=ghcr.io/astral-sh/uv:0.13.8@sha256:5cc1805e2426484abca4ad46ea783bab281fdb225fd82d8ce6d66a81254bac85

FROM ${UV_IMAGE} AS uv

FROM ${NODE_IMAGE} AS webbridge
RUN npm i -g pnpm@12.21.0
WORKDIR /repo
COPY --from=repo package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY --from=repo algorithm/manifest.json algorithm/manifest.json
COPY --from=repo packages packages
COPY --from=repo apps/web apps/web
# --frozen-lockfile: the versions the app runs; --ignore-scripts: nothing is built but the bundle.
RUN pnpm install --frozen-lockfile --ignore-scripts --filter "@ci/web..." \
    && pnpm --filter @ci/web lab-bridge:build /out/lab-bridge.mjs

FROM ${PYTHON_IMAGE} AS base
COPY --from=uv /uv /uvx /usr/local/bin/
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    UV_PYTHON_DOWNLOADS=never \
    UV_PYTHON=/usr/local/bin/python3.12 \
    UV_LINK_MODE=copy \
    UV_PROJECT_ENVIRONMENT=/opt/venv

FROM base AS lab
# git: the experiment check reads the order of commits (registered before results, ADR 0106).
RUN apt-get update \
    && apt-get install -y --no-install-recommends git libstdc++6 \
    && rm -rf /var/lib/apt/lists/* \
    && git config --system --add safe.directory '*'
WORKDIR /lab
COPY pyproject.toml uv.lock .python-version ./
# --frozen: the lock is the source of truth; a pyproject that disagrees with it fails the build.
RUN uv sync --frozen --no-install-project --group dev && uv pip check --python /opt/venv/bin/python
COPY --from=rematch --chmod=0755 /usr/local/bin/cli /usr/local/bin/cli
COPY --from=rematch --chmod=0755 /usr/local/bin/stockfish /usr/local/bin/stockfish
COPY --from=webbridge /usr/local/bin/node /usr/local/bin/node
COPY --from=webbridge /out/lab-bridge.mjs /usr/local/lib/rematch/lab-bridge.mjs
COPY . .
ENV PATH=/opt/venv/bin:$PATH \
    REMATCH_CLI=/usr/local/bin/cli \
    STOCKFISH_PATH=/usr/local/bin/stockfish \
    REMATCH_WEB_BRIDGE="node /usr/local/lib/rematch/lab-bridge.mjs" \
    PYTHONPATH=/lab \
    MPLBACKEND=Agg
# Matplotlib needs a writable config dir; nothing else is written outside /lab.
ENV MPLCONFIGDIR=/tmp/matplotlib
CMD ["pytest"]
