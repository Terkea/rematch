Write the `openings` section: `"section": "openings"`, fill `openings`, and set `overview`, `weaknesses`, `climbPlan` and `areas` to null.

- An item for each opening in the input that has an example position, with `lineKey` set to its `lineKey` and `verdict` set to its `verdict`, unchanged. Skip openings without an example; the list may be empty.
- `text`: what the results and the example show about this line and what to do next (keep, repair or replace it, as the verdict says). Opening line tokens in this text use only this item's own `lineKey`.
- `citations`: the opening's example positions the text refers to; at least a single position.
