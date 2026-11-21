import endgameGrindJson from "./games/endgame-grind.json";
import middlegameBlunderJson from "./games/middlegame-blunder.json";
import shortAttackJson from "./games/short-attack.json";
import { gameFixtureSchema } from "./schema";
import type { GameFixture } from "./types";

export { areaScoreFixtures } from "./area-scores";
export { kpiFixtures } from "./kpis";
export { fixturePlySchema, gameFixtureSchema } from "./schema";
export { timelineFixtures } from "./timeline";
export type * from "./types";

/**
 * Real analysed games from a public Lichess account (DrNykterstein), exported by
 * `scripts/export-fixtures.ts`. Parsed here so a bad file fails loudly.
 */
export const games: {
  middlegameBlunder: GameFixture;
  endgameGrind: GameFixture;
  shortAttack: GameFixture;
} = {
  middlegameBlunder: gameFixtureSchema.parse(middlegameBlunderJson),
  endgameGrind: gameFixtureSchema.parse(endgameGrindJson),
  shortAttack: gameFixtureSchema.parse(shortAttackJson),
};
