import type { Color } from "../../../../server/filters/params";
import type { GameStats, SideStats } from "../../../../server/stats/game-stats";
import { gamesText, type StatUnit, statRange, statValue } from "../_filters/stat-text";

// The overview's quiet White | Black row (T3): accuracy with its 80% range, blunders per game,
// score with W-D-L, and n each, from `gameStats` with the filters applied. Plain strings.

export type SplitCell = { value: string; detail: string };
export type SplitColumn = {
  key: "white" | "black";
  label: string;
  games: string;
  accuracy: SplitCell;
  blunders: SplitCell;
  score: SplitCell;
};
export type ColourSplitView = { columns: SplitColumn[] };

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function column(key: "white" | "black", s: SideStats, basis: string): SplitColumn {
  const cell = (e: SideStats["accuracy"], unit: StatUnit, kind: string): SplitCell => ({
    value: statValue(e, unit),
    detail: `${statRange(e, unit)}, ${gamesText(e.n, kind)}`,
  });
  const sc = s.score;
  return {
    key,
    label: key === "white" ? "White" : "Black",
    games: gamesText(s.n, basis),
    accuracy: cell(s.accuracy, "accuracy", "analysed"),
    blunders: cell(s.perGame.blunders, "perGame", "analysed"),
    score: {
      value: statValue(sc.score, "score"),
      detail: `${plural(sc.wins, "win", "wins")}, ${plural(sc.draws, "draw", "draws")}, ${plural(sc.losses, "loss", "losses")}`,
    },
  };
}

/** The columns the colour filter allows: both colours, or only the one it names. */
export function colourSplitView(stats: GameStats, color: Color, basis: string): ColourSplitView {
  const keys = color === "both" ? (["white", "black"] as const) : [color];
  return { columns: keys.map((k) => column(k, stats.byColor[k], basis)) };
}
