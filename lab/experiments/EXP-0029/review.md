# EXP-0029: review

## Part one: the pre-registration, before any new month is read

The critic was a fresh agent (Claude, no part in the design). It read the stage 1 registration at d37dce69 (`preregistration.md`, `power.py`, `power.json`, `summary.md`, `inputs.sha256`, the placeholder `analysis.py`), EXP-0009's registration, decision, review and results, issue #131, the template, ADR 0114 and `trainability`. It re-ran `power.py`'s functions read-only in the lab image and downloaded nothing. Its verdict was not ready: 4 blocking, 8 should-fix and 7 minor concerns.

Nothing has been streamed, and no month from 2025-09 to 2026-06 has been read, before or after this review. Every fix below uses only EXP-0009's committed results.

### Blocking

1. **B1. The power took the lower bound once, but the run takes it again.** Stage 1 planned at the lower bound and then read the minimum at that same value. The run, though, computes its own lower bound below whatever the truth is. Under the assumption the text stated, two keys reached the target, not six, and the closed form ignored the correlation between keys and the joint requirement on the control. Done: `power.py` now simulates section 9 end to end. It draws the run's own split-half correlation at its n, takes the lower bound and the floor, draws correlated coefficients (ρ = 0.3; 0 and 0.6 as the sensitivity), applies Holm across the eight keys, the control's own test and the three outcomes. The plan assumes EXP-0009's point reliability through Spearman-Brown, with the run's lower bound as the only penalty, and the lower bound as the truth is reported as a sensitivity. Outcome 2's list is cut, from the simulation and before any data, to W4.allowed, W5, W6, W7.traps and W9a. W7.prep (0.52 to 0.77) and W4.missed (0.58 to 0.79) are out with W9b, and section 9 says outcome 2 covers none of the three (sections 6 and 9).
2. **B2. The primary leaked the true level, a positive bias.** With only q₁ adjusted, q₂ still carries a stable level, which EXP-0009 showed predicts later climbing. Done: the primary adjusts for the window-mean level q̄ = (q₁ + q₂)/2, and the F1-level model is a sensitivity. Outcome 1 is read on the primary, and a key that reads "precedes climbing" only in the F1-level model is called a level artefact. Section 2 no longer says the bias works "never for it": it names the level leak and its size, about +0.8 to +1.0 per observed SD for accuracy and W7.prep by the critic's estimate (sections 2, 8 and 9).
3. **B3. The control's power ignored the residual luck bias, so outcome 3 was likely and unstated.** Done: the expected bias is registered for each quantity. It is EXP-0009's primary-minus-adjusted shift, scaled by the memory ratio at April's games and by 1/√3 for the window's dilution: for accuracy −1.53 at 20 games, −1.02 at 30 and −0.70 at 40. It enters the simulation, and the sizing step requires the control to hold 0.80 at 7 Elo per true SD after it. The review's three options were weighed:
   - (a) a two-month gap was simulated as design B. It removes about two thirds of the luck at 20 games, but with one outcome month a lasting link has half the Elo, and the control's power is lower (0.54 against 0.65), so it is not taken;
   - (b) reading the control on the adjusted model would not vouch for the keys' model under the same bias, so it decides nothing;
   - (c) the outcome probabilities are stated in section 0.

   The result: at the plan's 20 games in April the run would close unrun at the sizing step. It goes ahead at about 25 or more, and there, with no key truly linked, outcome 3 is about 0.29 (under 0.01 to 0.66 over a control of 10 to 5 Elo per true SD). If the reliabilities sit at EXP-0009's lower bounds, outcome 3 is about 0.73. The value gate is applied in section 0. A real link is found almost always, a false candidate comes up less than 2% of the time, and the worst case costs the streaming alone, so the experiment proceeds to stage 2 with the sizing step as its stop. The case for closing it now is set out in the lane report.
4. **B4. The kill rule's budget test was ambiguous, and good news could kill the run.** Done: the sizing step is a function, `power.py --sizing` (`decide_size`), tested on a fixture (`lab/tests/test_exp0029_sizing.py`). It tests the games against the 150% cap (5,550,000), and it projects CPU-hours and wall time from the measured streaming and EXP-0009's rates. Before it kills, it falls back to smaller per-window caps in a fixed order: (144, 120), then (96, 80), then (72, 60). It has no free parameter on the day (sections 0 and 6).

### Should-fix

1. **S1. Outcome 2's statement claimed more than the estimand.** Done: it now says "did not come before further climbing … once the rating had a month to catch up". It also says the catch-up already in April's rating reads zero, and that three keys are not covered (section 9).
2. **S2. The analysis code would have been written after the data was on disk.** Done: stage 2 writes `analysis.py` against a synthetic fixture, a critic reviews it, and its sha256 is committed in section 10, all before the first byte is streamed. The wall-time cap now runs from the sizing commit (sections 0, 7 and 10).
3. **S3. R_end of a window and R_start were undefined for many players.** Done: section 4 defines a month's and a window's end and start ratings, and what happens with fewer than five games in a month.
4. **S4. A mean-games ratio overstated the Spearman-Brown gain.** Done: k is the harmonic-mean ratio E_0009[1/m] / E_0029[1/m] of the capped game counts. EXP-0009's side comes from its own players files, whose checksums are in its `inputs.sha256` (section 6).
5. **S5. With no history month, new accounts entered in October.** Done: the September 2025 players file is streamed (about 30 GB and an hour, no games). The activity rule and the history covariates come from it, as July's did in EXP-0009. This adds one month to issue #131's range, still disjoint from EXP-0009's (sections 2, 7 and 8).
6. **S6. Selection on F2 activity is selection on the exposure window.** Done: the flow from F1 to F2 is reported by F1 level, rating and activity. Staying into F2 is modelled and enters the inverse-probability weights. The population is named as players active for six months (sections 1, 2, 7 and 8).
7. **S7. The minimum effect is not in EXP-0009's unit.** Done: "unchanged, so the follow-up asks the same question" is gone. Section 5 says the true change is now between three-month means, that Spearman-Brown assumes a fixed true-change variance, and which way that errs.
8. **S8. `summary.md` overclaimed the fix.** Done: it says the gap helps only for players active in April, states the run's condition on that, and gives the likely results.

### Minor

- M1 (Holm): the control is out of the keys' Holm family and is tested on its own. The simultaneous level is 1 − 0.05/8 (section 8).
- M2 (windows): O is May and June, read from the end of April (section 4).
- M3 (version): the sizing commit pins the algorithm id (section 3).
- M4 (planned edits): section 10 lists the only edits planned after stage 2's review.
- M5 (leakage): the run counts the games between two players of the slice as well (section 7).
- M6 (Docker share): if the share is lost, the run stops and the fix is recorded as a deviation; the cohort is never copied (section 0).
- M7 (holdout): the unused part of [0.3, 0.9) is said not to be a holdout (section 6).
