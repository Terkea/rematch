# ADR-R2-22. Syzygy 3-4-5 tablebases: pinned, verified, never committed

Date: 2026-11-21
Status: Accepted. Placeholder number; the controller renumbers R2 ADRs before merging. Builds on ADR-R2-9 (data directory) and feeds ADR-R2-12 (Task 23).

## Context

Endgame analysis needs the 3-4-5 piece Syzygy tables (WDL `.rtbw`, DTZ `.rtbz`). They are about 984 MB in 290 files, too large to commit and fixed in content.

## Decision

- **Source:** `https://tablebase.lichess.ovh/tables/standard/`, directories `3-4-5-wdl/` and `3-4-5-dtz/`. The `.../3-4-5` path in the original brief does not exist (404).
- **Checksums:** taken from the mirror's own `sha256` listing (`/tables/standard/sha256`, dated 2024-06-23, `sha256sum` format). The 290 lines for the files in those two directories are pinned in `backend/crates/lichess-db/data/syzygy-345.sha256`. No trust-on-first-use hashing was needed.
- **Fetch:** each file streams to `<name>.part` while hashed; renamed only on a match; the `.part` is deleted on a mismatch, HTTP error, timeout, size overrun or failed rename. Stale `.part` files are removed at the start. Files already present with a matching hash are skipped (no request). Redirects are followed, which is safe because the SHA-256 is checked before the rename.
- **Bounds:** 64 MiB per file (the largest table is 32,367,120 bytes, `KBNvKP.rtbz`; the cap is 2x, rounded) and 600 s per file.
- **Requests** carry only the fixed User-Agent `chess-improver/0.1 (+https://github.com/chess-improver)` (ADR 0043; the same string as `platforms`). No request from this crate carries an email address.
- **Storage:** `$DATA_DIR/syzygy` (git-ignored `data/`). Tablebase files are never committed; only the manifest is.

## Licence and redistribution

The tables are computed data and the mirror asserts no restrictive licence; they are freely redistributed (see `docs/research/legal-and-business.md`, Syzygy row). The generator `syzygy1/tb` is GPL-2.0, which does not apply to the data and is not used here. The mirror is Lichess's public tablebase server (https://tablebase.lichess.ovh/). We do not redistribute the files: each deployment downloads them itself.

## Consequences

- A tampered or truncated file can never reach `data/syzygy`.
- The manifest is only as trustworthy as the mirror's 2024 listing; a changed mirror file fails with `ChecksumMismatch` and needs a deliberate manifest update.
