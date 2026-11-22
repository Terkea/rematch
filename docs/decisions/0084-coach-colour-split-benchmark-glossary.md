# 0084. The coach reads the colour split, names the peer benchmark and uses the glossary

Date: 2026-11-22. Status: accepted (next round, task T4). Builds on ADR 0080 and 0081 (the peer
benchmark), ADR 0082 (the colour split's profile rows), ADR-FIX-D-1 (numbers keep their units)
and ADR-R2-2 (the CoachingInput contract).

## Context

T1 stores every profile row per side (both, White, Black), and the peer benchmark compares with
the opponents the player has faced. The coach (report and chat) read only the both rows, never
said where White and Black differ, and had no words for the comparison group. The web has a
plain-language glossary (`glossary.ts`) the prompts did not see.

## Decision

1. **coaching-input-v2.** `InputVersion` gains `V2` (`"coaching-input-v2"`), which the builder
   writes; v1 still parses (stored report-run inputs) and serialises byte for byte. v2 adds:
   - `byColor: { white, black }`, required in v2. Each colour has `games` (its most recent
     analysed games, at most 50, the KPI `last50` window), W-D-L, `score` (points per game with
     an 80% Wilson interval), `accuracy` and `blundersPerGame` (the colour's `last50` snapshot
     rows and their intervals), and up to 5 `weaknesses` (the colour's own estimates against its
     own benchmark rows: compared ones with `priority > 0` by priority, then uncompared ones by
     rate; never below `MIN_GAMES` (30) colour games, the minimum evidence or a single event).
     Each weakness carries `peerRate`, `peerPercentile` and `peerPlayers` only when the colour
     has its own peers (a both benchmark is never applied to a colour).
   - `benchmark`: the stored benchmark band (`source`, `ratingLow`, `ratingHigh`, `players`,
     `games`), present when the account has served both rows.
   The minimum evidence holds per colour: below `MIN_COLOR_GAMES` (10) games the colour, and
   below a metric's own `min_n` the metric, is `sufficient: false` with null numbers and
   `needMore`. `validate` rejects a 0 (or any number) where the evidence is insufficient.
   Both v2 keys are optional on the wire (`x-optional` in the schema transform), so v1 inputs
   stay valid; v1 may not carry them. The zod export stays v1's: the parity tests compare the
   schema's v1 shape (`v1_shape`). The committed schema is renamed
   `schema/coaching-input.schema.json`.
2. **Citable with units.** The evidence index keys a colour's weakness rows by series key
   (`byColor.white.weaknesses.W4_missed.rate`), marks scores and weakness rates as rates
   (`pct0`/`pct1`) and accuracies as percent. A null number has no path, so it cannot be cited.
3. **Prompts.** `report/report` v2 and `report/section-overview` v2 teach the colour split (only
   from cited numbers of both colours, a difference only when the ranges separate, nothing for
   an insufficient colour) and the benchmark's words: "players you've faced on chess.com, rated
   about X to Y", with the bias sentence, never "players at your level". `chat/system` v3 does the
   same and drops "do not compare the player with anyone" for "only with the named group". The
   chat context carries `byColor` and `benchmark` (the "how do I do as Black?" answer needs no
   tool call). The prompt version labels stay `report@1` and `chat@1`, as for Fix D; the set
   hashes record the files.
4. **The glossary.** `glossaryForCoach()` cuts every glossary entry (weaknesses, KPIs, areas,
   terms, states, opening verdicts) to its `name` and `meaning`: about 14 KB where the full
   entries are about 71 KB, sent with each of the five section calls and every chat turn. It is
   embedded as the generated prompt file `core/glossary.v<N>.md`, appended to system[1] of the
   report and the chat. `apps/web/test/profile/glossary.test.ts` fails when the latest file
   differs from `coachGlossaryPrompt()`; with `UPDATE_SNAPSHOTS=1` it writes the next version,
   which `prompts.rs` then registers (a prompt never changes in place).
5. **Set versions are what is sent.** The report request read every prompt at version 1, so
   `core/tokens` v2 (Fix D) was recorded in the set hash but never sent. The request now reads
   each prompt at its version in `REPORT_PROMPT_SET`.
6. **The report.** The computed report carries `byColor` and `benchmark` (absent for a v1 input).
   The web adds a "White and Black" section: both colours side by side with n, a dash and the
   shortfall below the minimum, a comparison sentence only when the ranges separate, each
   colour's weakness rows against that colour's own peers and the bias sentence. The overview's
   strengths (already `select_strengths`) link to the Strengths page.

## Consequences

- Report and chat prompts grow by about 4,000 tokens (the glossary), cached as part of system[1].
- Stored reports from v1 inputs show no colour section until the next build.
- A colour with fewer than 30 games has no weakness rows, and one without its own peers has only
  uncompared ones.
