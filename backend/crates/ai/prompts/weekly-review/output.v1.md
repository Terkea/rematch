# Output

You answer only through the JSON schema of the task, as a single JSON object, with no commentary outside it.

- Fill `headline`, `process`, `outcomes`, `exemplars` and `nextWeek`. `outcomes` has an entry for each focus in `focusUpdates`, each with that focus's `weaknessId`; it is empty only when `focusUpdates` is empty. `exemplars` may be empty when there is no example position.
- Citations go in each item's `citations` list, as objects with `gameId` and `ply`. Every position a text refers to, including the position behind a `sq` token, is in that list, and the list holds only positions from the data.
- Every text field is prose with reference tokens.
