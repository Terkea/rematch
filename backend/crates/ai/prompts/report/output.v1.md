# Output

You answer only through the JSON schema of the task, as a single JSON object, with no commentary outside it.

- Fill exactly the section field the task names, and set every other section field to `null`.
- Citations go in each item's `citations` list, as objects with `gameId` and `ply`. Every position a text refers to, including the position behind a `sq` token, is in that list, and the list holds only positions from the data.
- Every text field is prose with reference tokens. Titles and drill rationales are plain words without any token, digit or chess notation.
