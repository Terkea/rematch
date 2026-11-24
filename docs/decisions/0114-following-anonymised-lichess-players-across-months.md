# 0114. Following anonymised Lichess players across months

Date: 2026-11-24. Status: accepted (lane `exp09`, branch `exp/0009-what-helps`, issue #104). Builds on ADRs 0106 (Amendment 1), 0107 and ADR-R2-11. The privacy rules are ADR-R2-11's; this ADR applies them to a selection that spans months.

## Context

EXP-0009 asks whether anything Rematch measures relates to rating gain, which needs the same players in several monthly Lichess dumps. The band reference (ADR-R2-11) cannot provide them: it samples 20 players per band × time class cell and keeps 10 games each, so a player who climbs out of a band leaves the sample, and a fresh salt per build gives each month different pseudonyms. ADR 0107 asked for "a cohort selection keyed on the player hash alone, across months", with its own ADR under ADR-R2-11's privacy rules before any data is built. This is that ADR. No Rematch user, no account linked in the app and no chess.com player is in the cohort, and nothing is read from the app's database.

## Decision

1. **A new operator command, `lichess-db cohort`** (`backend/crates/lichess-db/src/cohort.rs`). It streams one monthly dump through the same path as `build` (`open_dump`, the band reference's filter `reference_filter`: rated games with a real clock, both ratings and `[%clk]`, no BOT, not abandoned and no rules infraction), verifies the whole-file SHA-256 against Lichess's `sha256sums.txt`, and writes three files to `--out`, only after the checksum matches. It needs no database.
2. **The selection is the hash alone.** A side is followed when `hash_fraction(sha256(salt ‖ lower(name))) < --player-rate`, the band reference's own functions (`player_hash`, `hash_fraction`). The same salt every month follows the same players. The fraction does not depend on a player's games, ratings or results, so the selection cannot pick players by how they did; any rule on activity is applied afterwards, in the experiment, where it is registered. EXP-0009 uses a rate of 1 (every player) and divides the hash range between its pilot and its confirmatory slice.
3. **What is written.** `players.jsonl`, one line per followed player with a rapid game, sorted by pseudonym: the counts of rapid games, of rapid games carrying Lichess's `[%eval]`, and of bullet, blitz and classical games, and the first and last five rapid games by time (UTC start, Lichess game id, rating before and after). With `--games`, `games.jsonl`: every rapid game with `[%eval]` a followed player played, its PGN with the `White` and `Black` tags replaced by `?` (`redact_pgn`), each followed side's pseudonym, the two ratings and rating changes, the result and the termination. `meta.json`: the month, the URL, the verified SHA-256, the byte and game counts, the salt and the rate.
4. **Pseudonyms, not names.** No name is written anywhere. The pseudonym is the first 16 bytes of the hash in hex. The files stay in the git-ignored data directory (`data/exp09-cohort/<month>/`) and are never committed; what an experiment commits is aggregates and checksums (ADR 0106 Amendment 1).
5. **The salt is public, and that is stated.** A salt that must be the same in four builds, and that makes a run reproducible from the public dumps, cannot be a secret: EXP-0009's is `sha256("EXP-0009/cohort/salt")`, written in its pre-registration. As ADR-R2-11 already says of the stored salt, anyone can hash candidate Lichess usernames and match a pseudonym; the pseudonyms group a player's games, they do not hide who played them. The Lichess game id is kept, as in the band reference, so the public game can be looked up. Nothing is kept that the CC0 dump does not already publish.
6. **Memory and cost.** The command holds a map from pseudonym to the counts and ten games' ends, about 200 bytes per followed rapid player, plus counts for every followed player; at a rate of 1, a month's dump of about 100 million games needs roughly 1 GB. It is bound by the download, about 30 GB a month at the mirror's speed.

## Options considered

1. **Extend the band reference with a fixed salt.** Rejected: its per-cell bottom-k and 10-game cap are what make a climber disappear; keying a second selection on the same code would change a production path that the band reference's finaliser depends on.
2. **Follow players by their Lichess username.** Rejected: names would be stored, against ADR-R2-11.
3. **A secret salt kept outside the repository.** Rejected: the experiment could not be reproduced from the public dumps, and the dumps are public anyway, so secrecy would protect nothing.
4. **The lab parses the dumps in Python.** Rejected: the lab image has no zstd, the streamed and verified download already exists in `lichess-db`, and the filter must be the band reference's.

## Consequences

- EXP-0009's cohort is built from four monthly dumps, streamed once each; EXP-0004 (#94) can reuse the command for month t against t+1 with its own salt.
- A future selection that follows players across months uses this command or amends this ADR.
- `cli lab observe` (EXP-0009, `backend/crates/bench/src/observe.rs`) reads the games written here; it is a lab command, outside the manifest, and calls the production feature path without changing it.
