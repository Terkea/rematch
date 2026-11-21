# Maia-2 difficulty service (ADR-R2-13). CPU only.
# The weights are downloaded at build time into the image (/opt/maia2_models), never into the
# repository. maia2 verifies their SHA-256 before use.
FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PIP_NO_CACHE_DIR=1 \
    PIP_DISABLE_PIP_VERSION_CHECK=1 \
    MAIA_MODEL_DIR=/opt/maia2_models \
    MAIA_MODEL=rapid

WORKDIR /srv

COPY requirements.lock ./
# torch alone comes from the PyTorch CPU index (no CUDA); everything else only from PyPI.
RUN grep '^torch==' requirements.lock > torch.lock \
    && grep -v '^torch==' requirements.lock > pypi.lock \
    && pip install --root-user-action=ignore --no-deps \
        --index-url https://download.pytorch.org/whl/cpu -r torch.lock \
    && pip install --root-user-action=ignore --no-deps -r pypi.lock \
    && pip check \
    && rm torch.lock pypi.lock

# Both released models (rapid, blitz), so MAIA_MODEL can switch without a rebuild.
RUN python -c "from maia2 import model; [model.from_pretrained(type=t, device='cpu', save_root='/opt/maia2_models') for t in ('rapid', 'blitz')]" \
    && chmod -R a+rX /opt/maia2_models

COPY pyproject.toml ./
COPY app ./app
COPY tests ./tests

RUN useradd --system --uid 10001 maia && chown -R maia /srv
USER maia

EXPOSE 8080
CMD ["uvicorn", "app.main:app", "--host", "0.0.0.0", "--port", "8080", "--workers", "1"]
