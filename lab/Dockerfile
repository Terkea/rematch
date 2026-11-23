# The algorithm lab (ADR 0106): Python 3.12 and uv, the locked analysis packages, and the Rust
# `cli` and Stockfish binaries taken unchanged from a Rematch build. No Rust is compiled here and
# nothing in this image runs in the product.
#
# The binaries come from the build context named `rematch`:
#   - compose (`docker compose --profile lab build lab`) passes the `rust-worker` image;
#   - by hand: docker build --build-context rematch=docker-image://<rust-worker image> lab
#     or a directory holding usr/local/bin/cli and usr/local/bin/stockfish (a lane build).
#
# Stages:
#   base  Python and uv only: regenerate the lock with
#         docker build --target base -t rematch-lab-base lab
#         docker run --rm -v "$PWD/lab:/lab" -w /lab rematch-lab-base uv lock
#   lab   (default) the frozen environment, the binaries and the lab sources; CMD runs pytest.
ARG PYTHON_IMAGE=python:3.12-slim@sha256:7a8b475003c4fe15a2cd4e55e5cfc2f3560bdc9333d624f24cdd6d4340fd7a17
ARG UV_IMAGE=ghcr.io/astral-sh/uv:0.13.8@sha256:5cc1805e2426484abca4ad46ea783bab281fdb225fd82d8ce6d66a81254bac85

FROM ${UV_IMAGE} AS uv

FROM ${PYTHON_IMAGE} AS base
COPY --from=uv /uv /uvx /usr/local/bin/
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    UV_PYTHON_DOWNLOADS=never \
    UV_PYTHON=/usr/local/bin/python3.12 \
    UV_LINK_MODE=copy \
    UV_PROJECT_ENVIRONMENT=/opt/venv

FROM base AS lab
WORKDIR /lab
COPY pyproject.toml uv.lock .python-version ./
# --frozen: the lock is the source of truth; a pyproject that disagrees with it fails the build.
RUN uv sync --frozen --no-install-project --group dev && uv pip check --python /opt/venv/bin/python
COPY --from=rematch --chmod=0755 /usr/local/bin/cli /usr/local/bin/cli
COPY --from=rematch --chmod=0755 /usr/local/bin/stockfish /usr/local/bin/stockfish
COPY . .
ENV PATH=/opt/venv/bin:$PATH \
    REMATCH_CLI=/usr/local/bin/cli \
    STOCKFISH_PATH=/usr/local/bin/stockfish \
    PYTHONPATH=/lab \
    MPLBACKEND=Agg
# Matplotlib needs a writable config dir; nothing else is written outside /lab.
ENV MPLCONFIGDIR=/tmp/matplotlib
CMD ["pytest"]
