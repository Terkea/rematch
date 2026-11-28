# EXP-0009 in plain words

**Bottom line:** we found no weakness whose improvement came before a rating rise, and our own sanity check failed too, so we have no data-driven teaching order yet and the order stays as the research notes set it.

## What we asked

Among real players, does getting better at one thing in a month, say fewer early blunders, come before a higher rating the month after? We followed 16,204 anonymised Lichess rapid players below 2000 from July to October 2026.

## Why it matters to someone learning

Rematch puts some weaknesses ahead of others partly by how much fixing them should help. That order should come from players who actually climbed, not from our hunch.

## What we found

None of the eight weaknesses we could measure came before a rise in October's rating. The one clear link ran the wrong way: players who converted more won games in September than in August gained slightly less in October, most likely a rating drifting back after a lucky month. The sanity check failed too: better accuracy didn't come before climbing either. The likeliest reason is our design: September's rating came from the same games as September's rates, so October's drift back from September's luck pulled every estimate down. A month of 10 to 40 games is also a very noisy measure of change. A better August level of accuracy, early blunders and leaving the opening worse did predict a higher rating two months on, but that is a rating catching up with play, not proof that teaching helps.

## What changed in Rematch

Nothing. The teaching order stays, and we say plainly it isn't data-driven.

## How this helps us improve Rematch

It checks whether real players' climbing backs the trainability rows. It re-runs on every release, with no clear difference on any version so far. It led to EXP-0029, which starts the outcome a month after the measured games. Measuring the other nine weaknesses with our own engine would have cost about 4,560 CPU-hours, so it didn't run.

## What we still don't know

Whether any weakness's improvement leads to climbing. This is observational and makes no causal claims either way. The maths audit confirmed the luck pulls the estimates down but showed the evidence we cited doesn't measure how much.
