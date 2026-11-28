# EXP-0005 in plain words

**Bottom line:** we closed this one before running it, because no result it could give would have changed anything.

## What we asked

How much do the rankings shuffle between two real versions of Rematch? If a new version finds your hanging pieces for one player and loses them for another, that churn is noise our benchmark has to see through. EXP-0001 guessed it at 0.10 and sized the benchmark on that guess.

## Why it matters to someone learning

The benchmark is one of the checks meant to stop a version that ranks your weaknesses worse from reaching you. Its size rests on this number.

## What we found

Before it ran, the reviewer showed that even the best possible result, none of the 28 reference hits lost, leaves the uncertainty reaching 0.085. At that value EXP-0001's own rule keeps today's 22 players per flaw. Every outcome led to the same action, so by our kill rule we didn't run it: 0 of 5,120 games spent.

## What changed in Rematch

Nothing. EXP-0006 then reported the churn of its own change on the side: at 80 games it lost none of 28 hits and gained none (one hit each at 45 and 60 games).

## How this helps us improve Rematch

It is about the benchmark, not the algorithm. Closing it saved the compute and set the rule for the next try in advance: the suites shrink only on the churn of a change to a detector or the ranking, the kind they exist to catch, measured on the full suite (EXP-0024). It is not live; nothing here re-runs per version.

## What we still don't know

The real churn of a detector or ranking change. The maths audit also found the bar is higher than the decision said: a resize needs an upper end of about 0.022, about 112 hits with none lost, not 0.03 and 83. The suites give about 79 hits today, so EXP-0024 as planned couldn't resize them even with nothing lost, and needs more players or a rule that can choose sizes off its grid before it is registered.
