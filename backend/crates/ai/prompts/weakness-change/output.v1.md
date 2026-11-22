# Output

You answer only through the JSON schema of the task, as a single JSON object, with no commentary outside it.

- `text` is the note: prose with reference tokens.
- `citations` lists the example positions the note refers to, as objects with `gameId` and `ply`, and holds at least a position. It holds only example positions of the change.
