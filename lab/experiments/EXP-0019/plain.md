# EXP-0019 in plain words

**Bottom line:** not run yet. I will check, blind, whether four motif names we hid are really wrong or whether Lichess's themes just miss them.

## What we asked

When Rematch calls a puzzle a hanging piece, a skewer, a sacrifice or a back-rank mate and Lichess's themes don't, is the motif there? EXP-0007 couldn't tell our errors from gaps in the themes, so algo-3.0.0 hides these four names until they clear.

## Why it matters to someone learning

If Rematch labels a position "hanging piece", the motif should be there. A rule wider than Lichess's may still be right.

## What is being tested

I, a club-level player, judge 132 puzzle solutions against definitions written down first, blind to our tags and to Lichess's, with decoys mixed in: about 90 minutes, offline. The theme decides where it names the motif, I decide where it's silent. A tag clears when it is shown right at least 8 times in 10, I agree with the themes on the puzzles they label, and I rarely say yes where neither we nor Lichess see the motif. A tag that is really right 95% of the time clears with probability about 0.84 to 0.99 after at most two sessions; one right 90% of the time only 0.27 to 0.66, so most tags between 80% and 90% would stay hidden.

## What each outcome would mean

A tag that clears is named to players again, as a major version with its ADR. A tag shown below 80% gets its rule fixed under its own experiment. Anything in between stays hidden, and the rule says whether a second session is worth the time.

## How this helps us improve Rematch

It is how the motif tags EXP-0007 couldn't judge get back to players and the coach, or get fixed. It isn't decided, so it isn't live.

## What we still don't know

The result. One rater who knows the rules: any clear is reported as exactly that. The maths audit found the code gives no second session to a tag whose bound clears but a guard fails, which can cost it up to 0.21 of its chance to clear; the registered code runs, and the decision names any tag it hits.
