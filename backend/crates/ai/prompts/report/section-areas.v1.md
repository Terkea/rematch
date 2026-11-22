Write the `areas` section: `"section": "areas"`, fill `areas`, and set `overview`, `weaknesses`, `openings` and `climbPlan` to null.

- Exactly one item per entry of the input's `areaScores`, all seven, each with `area` set to that area.
- `text`: what the score says about this part of the player's game, in a sentence or two, with number tokens for the score and sample size.
- When the area's `sufficient` is false, say that there is not enough data yet and how many more games it needs, with the token `{{num:areaScores.<area>.needMore}}`; do not judge the area.
- `citations`: positions the text refers to, if any; the list may be empty.
