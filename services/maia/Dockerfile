# Maia-2 difficulty service (ADR-R2-13). CPU only. Three stages:
#   deps     hash-checked install into /opt/venv, plus the weights (downloaded at build time,
#            SHA-256-verified by maia2, never in the repository)
#   test     deps + pytest/httpx + tests:  docker build --target test -t <tag> services/maia
#   runtime  (default) the venv, the weights and app/ only: no pytest, httpx or tests
ARG PYTHON_IMAGE=python:3.12-slim@sha256:7a8b475003c4fe15a2cd4e55e5cfc2f3560bdc9333d624f24cdd6d4340fd7a17

FROM ${PYTHON_IMAGE} AS deps
ENV PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PATH=/opt/venv/bin:$PATH
RUN python -m venv /opt/venv
COPY requirements.lock /tmp/requirements.lock
# Every file is hash-pinned, so the extra index (needed only for torch==2.8.0+cpu) cannot
# substitute any package.
RUN pip install --require-hashes --no-deps --only-binary=:all: \
        --index-url https://pypi.org/simple \
        --extra-index-url https://download.pytorch.org/whl/cpu \
        -r /tmp/requirements.lock \
    && pip check
# Both released models (rapid, blitz), so MAIA_MODEL can switch without a rebuild.
RUN python -c "from maia2 import model; [model.from_pretrained(type=t, device='cpu', save_root='/opt/maia2_models') for t in ('rapid', 'blitz')]" \
    && chmod -R a+rX,go-w /opt/maia2_models

FROM deps AS test
COPY requirements-test.lock /tmp/requirements-test.lock
RUN pip install --require-hashes --no-deps --only-binary=:all: \
        --index-url https://pypi.org/simple -r /tmp/requirements-test.lock \
    && pip check
WORKDIR /srv
COPY pyproject.toml ./
COPY app ./app
COPY tests ./tests
CMD ["pytest", "-q"]

FROM ${PYTHON_IMAGE} AS runtime
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PATH=/opt/venv/bin:$PATH \
    HOME=/tmp \
    MAIA_MODEL_DIR=/opt/maia2_models \
    MAIA_MODEL=rapid
COPY --from=deps /opt/venv /opt/venv
COPY --from=deps /opt/maia2_models /opt/maia2_models
WORKDIR /srv
# /srv stays root-owned and read-only for the service user.
COPY app ./app
RUN chmod -R a+rX,go-w /srv \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin maia
USER maia
EXPOSE 8080
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8080", "--workers", "1", "--no-server-header"]
