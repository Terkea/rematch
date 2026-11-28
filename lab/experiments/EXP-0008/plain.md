# EXP-0008 in plain words

**Bottom line:** 300,000 nodes a move wasn't enough; 1 million agrees noticeably more with a far deeper search, so Rematch now analyses at 1 million, at about three times the engine time.

## What we asked

How hard should Stockfish think on each of your moves before Rematch calls it a blunder, a mistake or an inaccuracy? Is 300,000 positions searched (nodes) enough?

## Why it matters to someone learning

Every weakness Rematch finds starts from these labels. If the engine stops too early, you get told off for moves that are fine, or let off for ones that aren't.

## What we found

On 160 rated rapid Lichess games, we compared both settings with Stockfish 19 searching 30 million nodes. 1 million disagreed with it on 2.5 moves in 100 fewer than 300,000 did, past our tolerance of 1 in 100; against 100 million nodes on 40 of the games, 2.2 fewer. For a player, about 7 in 100 of your move labels change, roughly two or three a game, mostly fewer inaccuracies with the same blunders, and game accuracy drops 0.4 of a point on average. The engine costs 39 CPU-seconds a game instead of 12: a 300-game import goes from about 15 to 49 minutes on 4 engines.

## What changed in Rematch

algo-4.0.0 searches every position at 1 million nodes. Games analysed before it keep their 300,000-node labels.

## How this helps us improve Rematch

It tunes the engine depth, the first step of the pipeline. It is live and re-runs on every release; algo-4.0.0 came out better on it and every version since has shown no clear difference.

## What we still don't know

The deep search isn't the truth: on the moves where the two settings disagree, 12% of its labels changed again between 30 and 100 million nodes. Whether your top weaknesses move with the depth (EXP-0027), and whether searching deeper only where the cheap searches disagree is nearly as good for less (EXP-0028). The maths audit found that if a follow-up had been needed, the chance of a wrong switch could have reached 0.10, not 0.05; this run decided at its first look, so it is unaffected.
