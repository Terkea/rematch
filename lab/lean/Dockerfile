# The lab's machine-checked lemmas (ADR 0118): Lean 4 and mathlib, pinned, and
# `lake build` over lab/lean. Nothing here runs in the product.
#
#   docker build -t rematch-lean:local lab/lean
#
# Stages:
#   toolchain  Debian, elan (checksum-verified) and the Lean toolchain named in lean-toolchain.
#              CI runs this one with lab/lean mounted, so it can cache .lake/packages itself:
#              docker run --rm -v "$PWD/lab/lean:/lean" rematch-lean:toolchain \
#                sh -c "lake exe cache get && lake build"
#   deps       the packages in lake-manifest.json and mathlib's prebuilt oleans (lake exe cache get).
#   build      (default) the Rematch sources and `lake build`, which also runs the axiom check.
ARG BASE_IMAGE=debian:bookworm-slim@sha256:e67eeead736ff7bbe258ef088bff683635c561b1fcfefad0806c1df4ab49ce0e

FROM ${BASE_IMAGE} AS toolchain
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git \
 && rm -rf /var/lib/apt/lists/*
# elan v4.3.0 (released 2026-11-17).
ARG ELAN_VERSION=v4.3.0
ARG ELAN_SHA256=0c71472fa3ac34871dc1e2539993cca222d7656b42a1469f8f8c97c22f9adab2
ENV ELAN_HOME=/opt/elan
ENV PATH=/opt/elan/bin:$PATH
COPY lean-toolchain /tmp/lean-toolchain
RUN curl -fsSL -o /tmp/elan.tar.gz \
      "https://github.com/leanprover/elan/releases/download/${ELAN_VERSION}/elan-x86_64-unknown-linux-gnu.tar.gz" \
 && echo "${ELAN_SHA256}  /tmp/elan.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/elan.tar.gz -C /tmp \
 && /tmp/elan-init -y --no-modify-path --default-toolchain "$(cat /tmp/lean-toolchain)" \
 && rm -f /tmp/elan.tar.gz /tmp/elan-init \
 && chmod -R a+rX /opt/elan \
 && lean --version
# A writable home for any user CI runs the container as (mathlib's cache lives under it).
ENV HOME=/tmp
WORKDIR /lean

FROM toolchain AS deps
COPY lean-toolchain lakefile.toml lake-manifest.json ./
RUN lake exe cache get

FROM deps AS build
COPY Rematch.lean RematchTest.lean ./
COPY Rematch Rematch
COPY RematchTest RematchTest
RUN lake build
CMD ["lake", "build"]
