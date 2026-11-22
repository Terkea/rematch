Write the `areas` section: `"section": "areas"`, fill `areas`, and set `overview`, `weaknesses`, `openings` and `climbPlan` to null.

- An item for each entry of the input's `areaScores`, with `area` set to that area; no area is left out and none is repeated.
- `text`: what the score says about this part of the player's game, briefly, with number tokens for the score and sample size. Scores are on their own scale, not fractions: show them with `int`, not in a rate format.
- When the area's `sufficient` is false, say that there is not enough data yet and how many more games it needs, with the token `{{num:areaScores.<area>.needMore}}`; do not judge the area.
- `citations`: positions the text refers to, if any; the list may be empty.
