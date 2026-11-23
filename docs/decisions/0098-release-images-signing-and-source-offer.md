# 0098. Release images on GHCR, signed, with the GPL source attached

- Status: accepted
- Date: 2026-11-23
- Lane: `ci/release-artifacts`

## Context

ADR 0095 made a release a tag, a changelog and a GitHub Release, but nothing a self-hoster can run: every service was built from source with `docker compose up --build`, which needs about 16 GB of Docker memory and a Rust release build. The worker image also redistributes the **Stockfish 19** binary, which is GPL-3.0-or-later, so publishing it obliges us to offer its corresponding source with every copy.

Two facts shaped the design:

- A Release that release-please publishes with the workflow's `GITHUB_TOKEN` starts no `release: published` workflow (GitHub's rule against recursive runs). A workflow listening only for that event would never run on an automatic release.
- Merging the release PR pushes the release commit to `main`, so `CI` runs the full gate on exactly the tagged commit, at the same time as `Release`.

## Decision

1. **Registry and images.** Three images go to GHCR under the owner's namespace: `ghcr.io/terkea/rematch-web` (`apps/web/Dockerfile`, target `runtime`), `ghcr.io/terkea/rematch-worker` (`backend/Dockerfile`, target `worker`) and `ghcr.io/terkea/rematch-ai-sidecar` (`services/ai-sidecar/Dockerfile`, target `runtime`). The worker image is named for its role, not for the Compose service `rust-worker`. Postgres and Redis are the upstream images, pinned by digest in `deploy/compose.yaml`. Maia (opt-in, ADR-R2-13) is not published; its image carries torch and is built from source with the overlay. GHCR is free for public images, needs no stored secret (the job's `GITHUB_TOKEN` with `packages: write`), and links each package to the repository through `org.opencontainers.image.source`.
2. **linux/amd64 only.** `backend/Dockerfile` downloads `stockfish-linux-x86-64-universal.tar.gz` against a pinned checksum, so the worker cannot be built for arm64 without a second pinned asset (`stockfish-linux-arm64-universal.tar.gz` exists upstream), and a Rust release build under QEMU takes hours on a hosted runner. Arm64 waits for a native arm64 runner and that second pin.
3. **Tags.** `imageTags` in `scripts/release/release-lib.mjs`: `X.Y.Z`, `X.Y` and `beta` below 1.0; `X.Y.Z`, `X.Y`, `X` and `latest` from 1.0; a prerelease such as `0.5.0-rc.1` only its own tag. No `latest` exists before 1.0, so nobody pulls a beta by accident. The released `compose.yaml` pins `X.Y.Z@sha256:…`, so it never follows a moved tag.
4. **The workflow.** `.github/workflows/release-images.yml`, with `permissions: {}` at the top and each job asking for its own:
   - triggers: `workflow_call` from `release.yml` when release-please reports `release_created` (the automatic path); `release: published` for a Release a person publishes or re-publishes; `workflow_dispatch` as a dry run on any ref, which builds, smoke-tests and renders the assets and pushes, signs and attaches nothing;
   - `gate` (`contents: read`, `checks: read`): re-runs `test-versions.mjs`, `test-settings.mjs` and `test-release.mjs` on the tag, checks the tag against `package.json` and `deploy/compose.yaml`, then waits for CI on the tag's SHA;
   - `images` (`contents: read`, `packages: write`, `id-token: write`, `attestations: write`): builds into the runner's Docker, runs `scripts/release/smoke.sh`, then pushes the same builds (every layer a cache hit, buildx cache in the GHA backend per image) with `sbom: true` and `provenance: mode=max`, attests them with `actions/attest-build-provenance` and signs them with keyless `cosign sign`;
   - `assets` (`contents: read`) and `attach` (`contents: write`, publish only): the assets below, uploaded with `--clobber` so a re-run replaces them.
   Every action is pinned by commit SHA, each release at least 7 days old when pinned: checkout v7.0.1, setup-node v7.1.0, setup-buildx-action v4.4.1, login-action v4.8.0, build-push-action v7.4.0, attest-build-provenance v4.2.2, cosign-installer v4.1.3, upload-artifact v7.0.2, download-artifact v8.0.2.
5. **The release gate waits for CI on the tagged commit** instead of re-running the full gate. The fast checks run again on the tag (seconds); the slow ones (`e2e`, `e2e-fakes`, `rust`, the schema checks) already run on that exact commit because merging the release PR pushed it to `main`. `release.mjs ci-gate` polls the commit's check runs for every required check in `rulesets/main.json` that `ci.yml` reports, takes the newest run of each (a re-run supersedes a failure), and passes only on `success`; `cancelled`, `skipped` and `failure` fail it. Re-running the full gate inside the release would double an hour of CI for no new information, and a gate that only reads a status from `main`'s branch head could be satisfied by a different commit. To keep a later merge from cancelling the run a release waits for, `ci.yml`'s concurrency group is the commit SHA for pushes and the ref for pull requests.
6. **Smoke test before push.** `scripts/release/smoke.sh` runs `deploy/compose.yaml` itself against the freshly built images (`REMATCH_IMAGE_PREFIX` and `REMATCH_VERSION` point it at local tags) in the project `ci-release`, with no host ports (`scripts/release/smoke.compose.yaml`), and `down -v` on exit. It requires `/api/health` to answer `{"ok":true,"db":true,"redis":true}`, `worker --version` to print `worker X.Y.Z` (a flag added to the worker for this; any other argument exits 2), the worker to apply every `db/migrations` file and keep running, the licence files in each image, and the sidecar to turn healthy after `ai-sidecar-role`.
7. **Signing and supply chain.** Keyless cosign, so there is no signing key to store or leak; the certificate binds each signature to `release-images.yml` in this repository through the job's OIDC token. GitHub build provenance is pushed to the registry and is checked with `gh attestation verify oci://… --repo Terkea/rematch`. BuildKit's SPDX SBOM sits in each image index and a copy is attached to the Release. `SHA256SUMS` is not signed: it checks a download is intact, and the trust comes from the digests in `compose.yaml`, which `cosign verify` checks.
8. **GPL source offer.** Each Release attaches `stockfish-sf_19-source.tar.gz` (the `sf_19` tag archive) and `nn-1a298aa575a0.nnue` (the network the binary embeds, from `EvalFileDefaultName` in that tag's `src/evaluate.h`), both downloaded and checked against `deploy/stockfish-source.txt` at release time; a changed upstream file fails the release rather than attaching something that is not the binary's source. The source travels with the binary instead of a written offer, because an attached file outlives a URL we do not control. Our own source is `rematch-X.Y.Z-source.tar.gz` (`git archive` of the tag), attached to the same Release as the images built from it. Each image carries `/usr/share/doc/rematch/LICENSE` and `NOTICE`; the worker also keeps Stockfish's `Copying.txt` and `AUTHORS` from the release archive in `/usr/share/doc/stockfish/`.
9. **Release assets.** `deploy/compose.yaml` pulls the images and never builds; release-please bumps its three image lines (`x-release-please-version`, a fourth `extra-files` entry) and `test-versions.mjs` fails when they differ from the root version. The attached `compose.yaml` is that file rendered with digests (`release.mjs render-compose`). `env.release.example` lists every setting, the required ones enforced by `${VAR:?}`.
10. **Release notes.** release-please writes the changelog; `attach` puts `.github/release-notes/header.md` (the beta status, how to run it, how to verify it, the source offer) above it between `<!-- rematch-header -->` markers, so a re-run replaces the header instead of stacking it. release-please has no option for a release-body header, so this is a post-step.
11. **Migrations on upgrade.** The worker applies `db/migrations` (sqlx) at every start before it takes a job, so pulling a newer worker image and starting it is the migration. Migrations only move forward; the upgrade steps in `docs/RELEASING.md` take a `pg_dump` first, and going back means restoring it.
12. **actionlint** 1.7.12, pinned by image digest, lints every workflow in the `ts` job, with `test-release.mjs` beside it. Neither is a new required check, so the ruleset is unchanged.

## Alternatives

- Docker Hub: a second account and a stored token, with pull limits for anonymous users.
- `release: published` only: never fires for release-please's own Releases. A personal access token for release-please would fix that at the price of a stored secret with write access, which ADR 0095 rejected.
- Re-running the whole CI in the release workflow: duplicates about an hour of e2e and Rust per release and proves nothing the push to `main` did not.
- A cosign key pair: a long-lived secret in a public repository; keyless signing ties each signature to the workflow instead.
- A written offer for the Stockfish source (GPL-3.0 section 6(b)): it obliges us to answer requests for three years; attaching the source (section 6(a) and (d)) does not.
- Multi-arch now: hours of QEMU per release, and an unverified arm64 Stockfish asset; see decision 2.

## Consequences

- A release takes CI's duration plus about 40 minutes (the Rust release build with a cold cache), and the `Release` concurrency group holds later `Release` runs until it ends.
- After the first release, each GHCR package must be made public once on its settings page.
- A cancelled or failed CI run on the release commit stops the images until it is re-run; `docs/RELEASING.md` lists the cases and the commands.
- Bumping Stockfish means a new `backend/Dockerfile` pin and new lines in `deploy/stockfish-source.txt`; `test-release.mjs` checks the tag in the Dockerfile URL matches the source manifest.
- The release workflows now include `release-images.yml` with write permissions (`packages`, `contents` for `attach`); ADR 0095's "the only workflow that writes" now reads "the release workflows".
