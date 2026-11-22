# Strengths (`/app/strengths`): critique log

Screenshots are not committed. They are taken through the `qa` container of the throwaway compose
project `ci-t6` (no host ports), from the e2e stack's `web` dev server, signed in as the adult
e2e user and seeded with `e2e/helpers/seed-strengths.ts` only: the coach seed's synthetic rows,
plus strength plies on real moves of two public fixture games (DrNykterstein's games from
lichess, the player as Black), with synthetic `move_features` (critical, Maia pending). States
shot, at 390 and 1440, light and dark, reduced motion:

- `/app/strengths` (rapid, the default): no band reference (own-baseline wording), too few
  games for the own comparison, one confirmed improving weakness, no judged opening line, the
  report's strengths, no hard spot.
- `/app/strengths?tc=blitz`: the critical-position fallback with three boards.

## Pass 1 (implementer self-critique; self-critique only)

The implementer was told not to dispatch subagents, so the fresh critic pass
(`critique-prompt.md`, PNGs only) is still owed: **the controller runs it and appends its verdict
here.**
