<!-- rematch-header -->
**Rematch {{version}}** is a {{status}} release. Every `0.y.z` is a beta: the database schema and the settings in `env.release.example` can change between minor versions, so read the notes below before upgrading. 1.0 is reserved for the MVP ([ADR 0095](https://github.com/{{repo}}/blob/{{tag}}/docs/decisions/0095-open-source-sdlc-and-releases.md)).

### Run it

```sh
mkdir rematch && cd rematch
gh release download {{tag}} --repo {{repo}} --pattern compose.yaml --pattern env.release.example --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
cp env.release.example .env    # fill in every value marked "required"
docker compose pull && docker compose up -d --wait
```

`compose.yaml` pins each image by digest. The images are linux/amd64:

- `ghcr.io/terkea/rematch-web:{{version}}`
- `ghcr.io/terkea/rematch-worker:{{version}}` (includes **Stockfish 19**)
- `ghcr.io/terkea/rematch-ai-sidecar:{{version}}` (optional, profile `ai`)

Upgrading from an earlier release: back up the database first; the worker migrates it on start, and migrations only move forward. The steps are in [docs/RELEASING.md](https://github.com/{{repo}}/blob/{{tag}}/docs/RELEASING.md#upgrading).

### Verify the images

Each image is signed with keyless **cosign** from this repository's release workflow, and carries a build provenance attestation and an SPDX SBOM.

```sh
cosign verify ghcr.io/terkea/rematch-web:{{version}} \
  --certificate-identity-regexp '^https://github\.com/{{repo}}/\.github/workflows/release-images\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
gh attestation verify oci://ghcr.io/terkea/rematch-web:{{version}} --repo {{repo}}
```

Replace `rematch-web` with `rematch-worker` or `rematch-ai-sidecar` for the other images.

### Source

`rematch-{{version}}-source.tar.gz` is this release's source (`git archive {{tag}}`, AGPL-3.0-only). The worker image runs the unmodified official **Stockfish 19** binary (GPL-3.0-or-later); its corresponding source is attached as `stockfish-sf_19-source.tar.gz` with the network it embeds, `nn-1a298aa575a0.nnue`, and `stockfish-source.txt` gives their upstream URLs and SHA-256 sums. Each image carries `/usr/share/doc/rematch/NOTICE` and the licence texts.
<!-- /rematch-header -->
