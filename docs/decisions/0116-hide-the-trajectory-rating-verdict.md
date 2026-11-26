# 0116. Hide the trajectory's rating verdict until a rule clears EXP-0015

Date: 2026-11-26. Status: accepted (lane `hidetraj`, branch `feat/hide-trajectory-verdict`, issue #136). Builds on ADRs 0089, 0104, 0105, 0109, 0112 (the precedent: one versioned switch, still computed, cut at the edges) and 0115. Ships as `algo-5.0.0`.

## Context

EXP-0015 sent 700 simulated players whose strength never changed, rated by Glicko-2 with lila's constants, through the production trajectory (`cli lab trajectory`, `lab/experiments/EXP-0015/decision.md`). It told 44%, 62% and 78% of them that their rating was falling at 60, 120 and 250 rated games (Bonferroni intervals [0.399, 0.488], [0.577, 0.664] and [0.745, 0.819]) against a bound of 0.15, and 44% and 30% that it was rising at 60 and 120 games. A rating carries over from game to game (lag-1 correlation 0.97), so the bootstrap of 30 consecutive ratings gives an 80% interval of the difference about 4 points each side where an honest one is 26 to 47, and at 120 and 250 games the "then" window sits on a selected peak. The accuracy and blunders rows stayed within the bound. Its decision: hide the rating's verdict until EXP-0030 clears a rule.

Until #136 the verdict reached the player and the coach in five places:

- the Progress headline and the Overview's "Where you are" card (`apps/web/src/app/(app)/app/_trajectory/view.ts`): "up from", "down from", "about where it was … no clear change", and "… since the games around it";
- the then | now table's rating row: "Worse" or "Better", the difference's 80% range and each window's range;
- the CoachingInput's `trajectory.kind` and `thenVsNow.rating` (its verdict and every interval);
- the report prompt (`report/report.v4.md`: "With `kind` `flat`, say the rating has not clearly moved"; "each measure's `verdict` decides its words") and the chat prompt (`chat/system.v6.md`);
- every CoachingInput stored before the change, which the report job, the batch planner and the chat read back.

The weekly review's input (`weekly-input-v1`) has no trajectory, so no weekly summary states it.

## Decision

1. **One switch, `coaching_input::trajectory::RATING_VERDICT_SHOWN`, `false`.** `RATING_VERDICT_SOURCE` is `"EXP-0015"`. It lives in `coaching-input`, which the profile and the coach both depend on, like ADR 0112's allowlist. `backend/crates/coaching-input/tests/trajectory.rs` reads EXP-0015's `results.json` and fails unless the switch is the opposite of `primary.decision == "hide the verdict until fixed"`.
2. **It is an algorithm component.** The manifest names it (`profile.trajectory_rating_verdict`, `{source, shown}`), so `scripts/algorithm/check.mjs` requires a major bump whenever it changes. Turning it on after EXP-0030 is `true` in one line, the test's source moving to EXP-0030's results, and a new major version.
3. **Computed, never exposed.** `profile::coaching::trajectory::compute` and `contract` are unchanged and still give `kind`, the rating's verdict and its intervals; `cli lab trajectory` returns them, so EXP-0015's arena and EXP-0030 keep measuring them, and the web's `computeTrajectory` stays in parity with Rust (`backend/fixtures/trajectory/cases.json`). Everything a player or the coach reads is cut by `coaching_input::trajectory::player_visible`:
   - `kind` is null; the rating row's verdict is the new `ChangeVerdict::Withheld` (`"withheld"`), and its `diffLow`, `diffHigh` and each window's `low` and `high` are null. Its means, its n and `diff` stay, as do `peak`, `current`, `change` and `months`. A row below the minimum keeps `not_enough_games`.
   - The builder applies it to the input it writes. `ai::input::parse::parse_for_coach` applies it to every stored input the report job, the batch planner and the chat read, so an input stored before `algo-5.0.0` reaches the coach cut as well.
   - The validator accepts a null `kind` on a sufficient trajectory (a stored one still carries it), accepts `withheld` only on the rating row, and with it requires `diff` and refuses `diffLow` and `diffHigh`. The committed schema is regenerated; the coaching input stays `coaching-input-v3`, because a stored v3 input still parses, and the change is versioned by the algorithm.
   - The report prompt is `report/report.v5.md` and the chat's `chat/system.v7.md`: the peak, the current rating and the change are numbers, the rating's verdict is `withheld`, and the coach never says the rating is rising, falling, recovering, declining, better or worse. v4 and v6 stay for stored hashes.
   - The web reads the switch from `algorithm/manifest.json` (`RATING_VERDICT_SHOWN` in `view.ts`). The headline states facts in one form whatever the rule computed: "Your rapid rating peaked at 1,080 in February 2026 and is 816 now (−264).", or, with the first games as "then", "Your blitz rating is 1,120 now; it averaged 858 over your first 30 rated games (+262)." with the peak added when it is higher than now. The table's rating row shows each window's average and n, and "Not called" with the difference ("Difference −229 points"), in plain weight; its "What is this?" says why, with EXP-0015's numbers.
4. **Nothing else is hidden.** Accuracy, blunders per game, score and the top weaknesses keep their verdicts in the table, the "Since then" sentence and the coach's input. The rating chart, its peak mark and the shaded windows stay.

## Options considered

1. **Stop computing the verdict.** Rejected: EXP-0015's arena runs per pull request on `kind`, and EXP-0030 needs the champion's verdict to compare with.
2. **A null verdict on the rating row instead of `withheld`.** Rejected: the prompts tell the coach that each `verdict` decides its words, and a null reads as missing data; `withheld` names what happened, and the validator can confine it to the rating row.
3. **Keep the rating's ranges.** Rejected: the windows' and the difference's ranges are the same bootstrap that is six to thirteen times too narrow, and the coach is taught that separated ranges mean a difference ("When the ranges overlap, say there is no clear difference yet").
4. **A new coaching-input version (`coaching-input-v4`).** Rejected: no field is added or removed, a stored v3 input must still parse for the chat, and the algorithm's major version already names the change.
5. **Hide it only in the web.** Rejected: the coach reads the CoachingInput and stored inputs, as ADR 0112 found for the motifs.

## Consequences

- The release arena against `algo-4.0.1` runs every live experiment; the lab path is unchanged, so EXP-0015 measures `algo-5.0.0` exactly as `algo-4.0.1`. The result is in `algorithm/CHANGELOG.md` and `lab/leaderboard.json`.
- The replay goldens change only in their header: none of the replay's fixture accounts has 20 rated games in a time class, so every replayed trajectory is insufficient and carries no verdict. The profile's DB test (`profile/tests/coaching_input.rs`) checks the cut on a built input.
- A player sees the change on the next page load; the coach on the next report or chat turn, stored inputs included. A report or chat answer written before the upgrade keeps its text, which may say the rating was falling: its words are the coach's, cited from tokens, and nothing re-reads them to cut a sentence. The next report replaces it. Unlike ADR 0112, whose hidden rows had their own keys to skip, the rating's verdict has no stored row of its own to filter.
- The report job and the chat hash the cut input, so every stored input's `input_hash` moves once after the upgrade, and a chat thread's next turn sees a new input.
- The validator keeps the cut whole: a withheld rating has no `kind` and no window ranges, and a sufficient trajectory without `kind` has a withheld rating (or one below the minimum). An input stored before the upgrade, with `kind` and a called rating, still validates.
- The "peaked at … (−N)" headline stays as a fact. EXP-0015 notes that it compares a maximum with one draw, so it is negative for a stable player; EXP-0030 reconsiders it with the rule.
