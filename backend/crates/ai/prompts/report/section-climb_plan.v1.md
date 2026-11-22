Write the `climbPlan` section: `"section": "climb_plan"`, fill `climbPlan`, and set `overview`, `weaknesses`, `openings` and `areas` to null.

- `intro`: a short paragraph on what the plan works on and what these focuses have in common.
- `focuses`: exactly the input's `focuses`, in that order, with `weaknessId` set to the focus id (`general` for the general focus). For each focus:
  - `why`: what this focus is about, shown on a cited position. For a weakness, cite an exemplar of that weakness and use only its `likelihoodPhrase` for likelihood words; for `general`, cite any position from the data and use no likelihood word. It includes the `evidenceWindow` text as written;
  - `drills`: drills within the count the schema allows, each `drillType` taken from that focus's `allowedDrills`, each with a short plain-words `rationale` (no tokens, digits or notation);
  - `habit`: a concrete habit for the player's games, in a short sentence or a few.
- `weeklyRhythm`: how to spread the drills and games over a week, in plain words.
