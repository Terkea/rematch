# EXP-0030 in plain words

**Bottom line:** the new "falling" rule told about 1 in 10 steady players their rating was falling, against up to 8 in 10 before, and it shipped as algo-6.0.0; the price is that real falls are caught less often.

## What we asked

Can "your rating is falling" be honest? The rule compares your first 30 games with your last 30, and its range counts how a rating carries its error from one game to the next, worked out from your own rating steps. Nothing is compared with your peak.

## Why it matters to someone learning

You should hear "falling" only when your rating has moved more than it wanders on its own.

## What we found

On 2,000 fresh made-up players whose strength never changed, it said "falling" to 10.3%, 11.7% and 10.5% at 60, 120 and 250 games, and "rising" to 8.4% to 9.5%, all inside our bound of 15%. It catches a 200-point fall 72%, 84% and 99% of the time, a 100-point fall 36% to 72%. With breaks, streaks and tilt it runs at 13% to 14%. On 16,445 real Lichess players over a month, 36% were flagged either way against the model's 20%: real ratings wander, or really change, more than the model allows.

## What changed in Rematch

algo-6.0.0 (ADR 0117): the verdict is back, the headline compares the averages, and the peak is a plain fact, never the change.

## How this helps us improve Rematch

It replaced the trajectory rule and re-runs on every release; EXP-0015 re-runs on every pull request that changes the version and records the trade-off: a real 100-point fall is now caught 0.523 of the time, against 0.887 for the old rule, which said "falling" to up to 78% of steady players. A 100-point fall at 60 games is caught one time in three, so if a fifth of players were falling that much, about half the "falling" headlines would still be false.

## What we still don't know

The false-alarm rate on real players. The maths audit showed our "between the nominal rate and about twice it" is not a proven bound, only an illustration; EXP-0032 measures it on real series. New accounts look worse (17% to 18% at 250 games) and long histories are untested; that is EXP-0031.
