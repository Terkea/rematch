# Releasing Rematch

A release is cut by merging the release PR; the `Release` workflow then tags `vX.Y.Z`, publishes the GitHub Release, and calls `Release images`, which pushes the signed images to GHCR and attaches the source and deployment files. The decisions behind it are [ADR 0095](decisions/0095-open-source-sdlc-and-releases.md) (versions and release-please) and [ADR 0098](decisions/0098-release-images-signing-and-source-offer.md) (images, tags, signing and the GPL source offer).

## What a release contains

Images, linux/amd64 only (the worker's **Stockfish 19** asset and checksum in `backend/Dockerfile` are the x86-64 build):

| Image | From | Notes |
| --- | --- | --- |
| `ghcr.io/terkea/rematch-web` | `apps/web/Dockerfile`, target `runtime` | Next.js standalone server on port 3100 |
| `ghcr.io/terkea/rematch-worker` | `backend/Dockerfile`, target `worker` | `worker`, `cli`, `lichess-db` and Stockfish 19 |
| `ghcr.io/terkea/rematch-ai-sidecar` | `services/ai-sidecar/Dockerfile`, target `runtime` | optional, Compose profile `ai` |

Tags, from `imageTags` in `scripts/release/release-lib.mjs`:

- below 1.0: `0.4.2`, `0.4` and `beta`. There is no `latest` before 1.0;
- from 1.0: `1.2.3`, `1.2`, `1` and `latest`;
- a prerelease such as `v0.5.0-rc.1` gets only `0.5.0-rc.1`, so it never moves `beta` or `latest`.

Release assets, listed in `SHA256SUMS`:

- `compose.yaml`: `deploy/compose.yaml` with each Rematch image pinned as `0.4.2@sha256:…`, the digest the workflow pushed;
- `env.release.example`: every setting, the required ones marked;
- `rematch-0.4.2-source.tar.gz`: `git archive` of the tagged commit;
- `stockfish-sf_19-source.tar.gz` and `nn-1a298aa575a0.nnue`: the corresponding source of the Stockfish binary in the worker image and the network it embeds, with `stockfish-source.txt` giving their upstream URLs and SHA-256 sums;
- `rematch-<image>-0.4.2.spdx.json`: the SPDX SBOM of each image.

Each image carries `/usr/share/doc/rematch/LICENSE` and `NOTICE`; the worker also carries Stockfish's `Copying.txt` and `AUTHORS` in `/usr/share/doc/stockfish/`.

## Cutting a release

1. Check that the commits listed in the open release PR (opened by release-please, with the next version and its `CHANGELOG.md` entry) passed CI on `main`.
2. Merge the release PR with the admin bypass. GitHub starts no workflows on a PR opened with `GITHUB_TOKEN`, so its required checks never report (ADR 0095).
3. The merge pushes to `main`, which starts `CI` and `Release` on the same commit. release-please tags `v0.4.2` and publishes the Release, marked prerelease while the major is 0.
4. `Release` calls `Release images` with the tag:
   - `gate` re-runs `test-versions.mjs`, `test-settings.mjs` and `test-release.mjs` on the tag, checks that the tag equals the version in `package.json`, `Cargo.toml` and `deploy/compose.yaml`, then waits (up to 150 minutes) for CI on that commit. It passes only when every required check in `.github/settings/rulesets/main.json` that `ci.yml` reports (`ts`, `e2e`, `e2e-fakes`, `rust`, `site`, `baseline-schema`, `schema-drift`) succeeded on that exact SHA;
   - `images` builds the three images, runs `scripts/release/smoke.sh` against them, then pushes, attests and signs them;
   - `assets` builds the files above; `attach` uploads them and puts the header from `.github/release-notes/header.md` above release-please's notes.
5. After the first release only: remove `release-as` from `release-please-config.json` (`test-versions.mjs` warns until you do), and on each package's GHCR settings page set the visibility to public. A package pushed for the first time is private.

A dry run builds and smoke-tests everything from any ref without pushing, signing or attaching:

```sh
gh workflow run release-images.yml --ref main -f ref=main
```

The assets of a dry run are kept as the run's `release-assets` artifact for 30 days.

## The smoke test

`scripts/release/smoke.sh <image-prefix> <version> [--with-ai]` runs `deploy/compose.yaml` against images already in the local Docker, in the compose project `ci-release` (or `$SMOKE_PROJECT`) with no host ports, and removes it with `down -v` when it ends, pass or fail. It fails unless:

1. `/api/health` answers `200 {"ok":true,"db":true,"redis":true}`;
2. `worker --version` prints `worker <version>`;
3. the worker applies every file in `db/migrations` and is still running five seconds later;
4. `/usr/share/doc/rematch/NOTICE` and `LICENSE` are in each image, and Stockfish's `Copying.txt` in the worker;
5. with `--with-ai`, `ai-sidecar-role` grants the role LOGIN and the sidecar turns healthy (in development mode, with no credential).

To run it locally on images you built yourself:

```sh
docker buildx build --platform linux/amd64 -f apps/web/Dockerfile --target runtime -t rematch-local-web:0.4.2 --load .
docker buildx build --platform linux/amd64 -f backend/Dockerfile --target worker -t rematch-local-worker:0.4.2 --load .
sh scripts/release/smoke.sh rematch-local 0.4.2
```

## When something fails

- `gate` fails with `CI did not pass on <sha>: ts: failure`. The tagged commit is broken. Fix it on `main` with a `fix:` commit; release-please opens the next release PR (0.4.3). Do not move the tag.
- `gate` fails with `rust: cancelled`. A CI run on that commit was cancelled. Pushes to `main` get one concurrency group per commit, so a later merge no longer cancels it, but a person can still cancel a run by hand. Re-run the failed CI jobs on the commit (`gh run rerun <ci-run-id> --failed`), then re-run the failed `Release` jobs.
- `gate` fails with `still running after 150 minutes`. Wait for CI to finish, then re-run the failed `Release` jobs.
- `images` or `attach` fails (a runner, GHCR or network error). Re-run the failed jobs of the `Release` run: `gh run rerun <release-run-id> --failed`. Pushes are idempotent and `gh release upload --clobber` replaces assets. Re-runs are possible for 30 days; after that, start a fresh run by re-publishing the Release by hand, which a person's token turns into a `release: published` event:

  ```sh
  gh release edit v0.4.2 --draft=true && gh release edit v0.4.2 --draft=false --prerelease
  ```

- `fetch-stockfish-source` fails with `SHA-256 … pins …`. GitHub changed the bytes of the `sf_19` archive (it regenerates archive tarballs rarely, but has done so). Download it, compare the unpacked tree with the old one (`diff -r`), and if only the compression differs, update the sum in `deploy/stockfish-source.txt` in a `fix(release):` PR. Never skip the check: the attached file must be the source of the binary we ship.

## A hotfix

There are no release branches while the project is a beta. A hotfix is an ordinary `fix:` PR to `main`; release-please opens a release PR with the next patch (0.4.2 to 0.4.3), and it is merged as above. If `main` already holds unreleased work that must not ship, revert that work on `main` first (`revert:` commits), release, then restore it.

## Yanking a bad release

A release that corrupts data or cannot start is yanked, never deleted: the tag and images stay, so anyone who pulled them can still check what they ran.

1. Put the reason at the top of the Release notes, above the existing text:

   ```sh
   { echo "Yanked: <reason>. Use 0.4.3."; echo; gh release view v0.4.2 --json body --jq .body; } > yanked.md
   gh release edit v0.4.2 --notes-file yanked.md
   ```

2. Move the `beta` tag back to the last good version, signed images included:

   ```sh
   docker buildx imagetools create -t ghcr.io/terkea/rematch-web:beta ghcr.io/terkea/rematch-web:0.4.1
   ```

   and the same for `rematch-worker` and `rematch-ai-sidecar`. The `0.4` tag moves only when a fixed 0.4.3 is pushed.
3. Ship the fix as 0.4.3 as soon as possible. If the bad release ran a migration, the fix must leave that migrated schema working: migrations only move forward.

Delete a package version on GHCR only if it contains a secret or personal data, and say so in the Release notes.

## How 1.0 is decided

1.0.0 is the MVP. The maintainer decides it in its own ADR, which names what the MVP is and the evidence that this release meets it. In the same PR: `"release-as": "1.0.0"` and `"prerelease": false` in `release-please-config.json` (`test-versions.mjs` requires `prerelease` to be false from major 1). No commit message ever reaches 1.0, because `bump-minor-pre-major` turns a breaking change into a minor bump while the major is 0. From 1.0 the images also get `1` and `latest`.

## Verifying a release

Each image is signed with keyless **cosign** by `.github/workflows/release-images.yml` in this repository, and has a GitHub build provenance attestation:

```sh
cosign verify ghcr.io/terkea/rematch-worker:0.4.2 \
  --certificate-identity-regexp '^https://github\.com/Terkea/rematch/\.github/workflows/release-images\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
gh attestation verify oci://ghcr.io/terkea/rematch-worker:0.4.2 --repo Terkea/rematch
```

The SBOM is also in the registry:

```sh
docker buildx imagetools inspect ghcr.io/terkea/rematch-worker:0.4.2 --format '{{ json .SBOM }}'
```

`SHA256SUMS` checks that the downloaded files are intact (`sha256sum --check SHA256SUMS`). It is not signed; the trust comes from the digests in `compose.yaml`, which `cosign verify` checks against this repository's workflow.

## Running a release

```sh
mkdir rematch && cd rematch
gh release download v0.4.2 --repo Terkea/rematch --pattern compose.yaml --pattern env.release.example --pattern SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
cp env.release.example .env      # POSTGRES_PASSWORD, BETTER_AUTH_SECRET, BETTER_AUTH_URL, SMTP_URL
docker compose pull
docker compose up -d --wait
```

The web app listens on `127.0.0.1:3100` (`WEB_PORT`); put a TLS reverse proxy in front of it and list the proxy in `TRUSTED_PROXIES`. Sign-up is invite-only: add your address to `INVITE_ALLOWLIST` before the first sign-in. Running a release needs Compose 2.20 or newer (`depends_on.required`); development needs 2.24 (CONTRIBUTING.md).

With the AI coach: set `COMPOSE_PROFILES=ai`, `AI_SIDECAR_URL=http://ai-sidecar:7310`, `AI_SIDECAR_TOKEN`, `ANTHROPIC_API_KEY` and `AI_SIDECAR_DB_PASSWORD` in `.env`, run `docker compose up -d`, then once `docker compose run --rm ai-sidecar-role`. Without it every number is still computed; the worker logs `ai: disabled (AI_SIDECAR_URL not set)`.

### Syzygy tables

The 3-4-5 piece tables (984 MB, ADR-R2-22) are optional. The worker image fetches and verifies them:

```sh
mkdir -p data/syzygy
docker compose run --rm --no-deps --user "$(id -u):$(id -g)" -v "$PWD/data:/out" \
  --entrypoint lichess-db worker syzygy --dest /out/syzygy
docker compose restart worker
```

## Upgrading

The worker applies `db/migrations` with **sqlx** every time it starts, before it takes a job (`backend/crates/worker/src/start.rs`), so the first start of a newer worker image migrates the database. Migrations only move forward; going back to an older release means restoring the backup taken before the upgrade.

1. Read the new Release's notes. While the version is 0.y.z, a minor bump (0.4 to 0.5) can change settings; compare `env.release.example` with your `.env`.
2. Back up the database:

   ```sh
   docker compose exec -T postgres pg_dump -U postgres -Fc rematch > rematch-$(date +%Y%m%d).dump
   ```

3. Download the new `compose.yaml` and `env.release.example` (see "Running a release") over the old ones, and add any new settings to `.env`.
4. Pull and restart:

   ```sh
   docker compose pull
   docker compose up -d --wait
   docker compose logs worker | grep -iE 'migrat|error'
   ```

5. If the worker does not start, keep it stopped, restore the backup into a fresh database and return to the old `compose.yaml`:

   ```sh
   docker compose stop worker web
   docker compose exec -T postgres dropdb -U postgres rematch
   docker compose exec -T postgres createdb -U postgres rematch
   docker compose exec -T postgres pg_restore -U postgres -d rematch < rematch-YYYYMMDD.dump
   ```

Then open an issue with the worker's log.
