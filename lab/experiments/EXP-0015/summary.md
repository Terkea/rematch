---
id: EXP-0015
title: Is "clearly worse than your peak" a false alarm?
status: decided
algorithm: algo-4.0.1+fcda2762eeeb
issue: 110
updated: 2026-11-26
live: true
rerun: per-pr
run_cpu_hours: 0.03
run_minutes: 2
---

- Hide the verdict until fixed. Of 700 simulated players whose strength never changed, rated by Glicko-2 as on Lichess, the production trajectory told 44%, 62% and 78% that their rating was falling at 60, 120 and 250 rated games (95% 41 to 48, 58 to 66, 75 to 81), against an acceptable 15%; it told 44% and 30% that it was rising at 60 and 120 games.
- Why: the rule's 80% interval of the rating difference is about 4 points each side where an honest one would be 26 to 47. A rating remembers its last games (game-to-game correlation 0.97), and the bootstrap counts 30 consecutive ratings as 30 independent ones; at 120 and 250 games the "then" window also sits on a selected peak.
- Players whose strength really fell are told so 74% to 100% of the time, but that is inflated by the false alarms: if a fifth of players were declining, about three "falling" headlines in four would be false.
- The accuracy and blunders rows of then against now stayed within the bound, in a world where those values have no memory; they keep their verdicts.
- Next: hide the rating's verdict (the headline's "falling" and "rising", the table's "Worse" and "Better", the coach's `kind`) in its own lane, and EXP-0030, a rule that counts the rating's memory and drops the selected peak.
