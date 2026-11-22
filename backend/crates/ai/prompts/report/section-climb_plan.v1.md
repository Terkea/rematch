Write the `climbPlan` section: `"section": "climb_plan"`, fill `climbPlan`, and set `overview`, `weaknesses`, `openings` and `areas` to null.

- `intro`: a short paragraph on what the plan works on and why these focuses come first.
- `focuses`: exactly the input's `focuses`, in that order, with `weaknessId` set to the focus id (`general` for the general focus). For each focus:
  - `why`: why this focus matters now, shown on a cited position of that weakness, including the `evidenceWindow` text as written and using only that weakness's `likelihoodPhrase` for likelihood words;
  - `drills`: one to three drills, each `drillType` taken from that focus's `allowedDrills`, each with a short plain-words `rationale` (no tokens, digits or notation);
  - `habit`: one concrete habit to use in every game, in a sentence or two.
- `weeklyRhythm`: how to spread the drills and games over a week, in plain words.
