export type Color = "white" | "black";
export type Judgement = "inaccuracy" | "mistake" | "blunder";
export type Phase = "opening" | "middlegame" | "endgame";

/**
 * One half-move of an analysed game. Evals are in White's point of view: `winWhite` is White's
 * win percentage (0 to 100), `cpWhite` and `mateWhite` describe the position after the move, and
 * `clockSeconds` is the mover's clock before the move.
 */
export interface FixturePly {
  ply: number;
  color: Color;
  san: string;
  uci: string;
  fenAfter: string;
  winWhite: number;
  cpWhite: number | null;
  mateWhite: number | null;
  judgement: Judgement | null;
  bestUci: string | null;
  clockSeconds: number | null;
  phase: Phase;
}

export interface FixturePlayer {
  name: string;
  rating: number | null;
}

export interface GameFixture {
  id: string;
  /** Game URL on the source platform. */
  source: string;
  white: FixturePlayer;
  black: FixturePlayer;
  playerColor: Color;
  result: "1-0" | "0-1" | "1/2-1/2";
  timeClass: string;
  eco: string | null;
  openingName: string | null;
  startFen: string;
  plies: FixturePly[];
}

/** Synthetic fixtures are shaped like the spec but carry made-up numbers until M2 lands. */
export interface Synthetic {
  synthetic: true;
}

export type KpiFormat = "percent" | "decimal" | "integer";

/** Props of `KpiStat` (Task 14). */
export interface KpiStatProps {
  label: string;
  value: number;
  format: KpiFormat;
  interval: { low: number; high: number; level: 0.8 };
  basis: { games: number; timeClass: string };
  provisional?: boolean;
  trend?: (number | null)[];
  target?: { value: number; label: string };
}

export type Area =
  | "openings"
  | "middlegame"
  | "endgame"
  | "tactics"
  | "calculation"
  | "time"
  | "mindset";

/** Props of `AreaScore` (Task 15). */
export interface AreaScoreProps {
  area: Area;
  /** 0 to 100. */
  score: number;
  /** `next` is optional: absent, the next band is not measured yet and no tick is drawn (R8). */
  band: { median: number; next?: number };
  delta?: number;
  /**
   * The sample behind the score. Without `unit` it reads "22 rapid games"; with one, "148 chances"
   * (an area's n can count opportunities rather than games).
   */
  basis: { games: number; timeClass: string; unit?: { one: string; many: string } };
}

export type WeaknessState =
  | "not_an_issue"
  | "emerging"
  | "active"
  | "improving"
  | "resolved"
  | "relapsed";

export interface TimelineSeries {
  id: string;
  label: string;
  segments: { state: WeaknessState; from: string; to: string | null }[];
  onset?: { date: string; low: string; high: string };
  /**
   * False when the weakness has no rating-band benchmark: its stored `not_an_issue` is then no
   * claim, and reads "Not compared yet" (bug hunt I1). Omitted means compared.
   */
  compared?: boolean;
}

/** Props of `WeaknessTimeline` (Task 15). */
export interface WeaknessTimelineProps {
  series: TimelineSeries[];
  events?: { date: string; kind: "plan" | "drill"; label: string }[];
  range: { from: string; to: string };
}
