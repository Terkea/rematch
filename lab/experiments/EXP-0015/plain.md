# EXP-0015 in plain words

**Bottom line:** the old rule told 44% to 78% of steady players their rating was falling, so we hid it until a better rule existed.

## What we asked

If Rematch says your rating is clearly worse than at your peak, is that true? We made 700 players whose strength never changed, rated them as Lichess does, and asked the rule.

## Why it matters to someone learning

Telling a steady player they're getting worse is discouraging, and it sends them looking for a problem that isn't there.

## What we found

It told 44%, 62% and 78% of them their rating was falling at 60, 120 and 250 games, against the 15% we'd accept. Two reasons. A rating remembers its last games (one game's rating and the next correlate at 0.97), and the rule treated 30 ratings in a row as 30 independent ones, so its range was about 4 points each side where an honest one is 26 to 47. And comparing with your peak compares with the top of the noise. It caught 74% to 100% of real falls, but mostly because it said "falling" to most steady players too: if a fifth of players were declining, between about two in three and three in four "falling" headlines would be false.

## What changed in Rematch

algo-5.0.0 hid the rating's verdict from the headline, the table and the coach. Accuracy and blunders kept theirs. EXP-0030 brought it back with a new rule in algo-6.0.0.

## How this helps us improve Rematch

It tests the trajectory verdict, and it re-runs on every pull request that changes the version. On algo-6.0.0 it shows the trade-off: false "falling" dropped from 0.443 to 0.094, 0.621 to 0.110 and 0.784 to 0.081, but a real 100-point fall is now caught 0.523 of the time instead of 0.887, and a 200-point fall 0.848 instead of 0.978. The old rate was inflated by false alarms; once those are taken off, it is about 0.27 before and 0.43 after.

## What we still don't know

Real players: the world is simulated, with no breaks, streaks or tilt. Accuracy and blunders passed only because the simulation gives them no memory; real accuracy has form, and that needs checking on real games.
