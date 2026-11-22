import type { Area } from "@ci/ui/area-score";

// The profile catalogue, mirrored from Rust (ADR 0068). `backend/fixtures/profile/catalogue.json`
// is written by `profile-core`'s `catalogue_snapshot` test; `test/profile/catalogue.test.ts` fails
// when these tables differ from it. Edit both together, never one alone.

/**
 * The seven areas in `@ci/ui`'s `areaLabels` order. Kept here because `area-score` is a client
 * module, whose values a server component cannot read; `catalogue.test.ts` pins the order.
 */
export const AREAS: readonly Area[] = [
  "openings",
  "middlegame",
  "endgame",
  "tactics",
  "calculation",
  "time",
  "mindset",
];

export type WeaknessDef = {
  /** The series key, e.g. `W3.shallow`. */
  key: string;
  /** The contract `RankedWeakness.id`, e.g. `W3`. */
  contractId: string;
  variant: string | null;
  label: string;
  area: Area;
  /** Rust `min_evidence`: opportunities (games for W9a) before the weakness is sufficient. */
  minEvidence: number;
  rankable: boolean;
};

export type MotifDef = { tag: string; contractTag: string };

export type KpiDef = {
  metric: string;
  /** Null for the overall rows (`rating`, `perf_vs_expected`, `accuracy`). */
  area: Area | null;
  kind: "mean" | "rate" | "last";
  unit: "rate" | "per_game" | "pct" | "seconds" | "elo" | null;
  /** Null when neither direction is better. */
  higherIsBetter: boolean | null;
  minN: number;
  label: string;
};

export const WEAKNESSES: readonly WeaknessDef[] = [
  {
    key: "W1",
    contractId: "W1",
    variant: null,
    label: "Hung material",
    area: "calculation",
    minEvidence: 1000,
    rankable: true,
  },
  {
    key: "W2",
    contractId: "W2",
    variant: null,
    label: "Missed opponent threat",
    area: "calculation",
    minEvidence: 1000,
    rankable: true,
  },
  {
    key: "W3.shallow",
    contractId: "W3",
    variant: "shallow",
    label: "Missed own tactic (shallow)",
    area: "tactics",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W3.deep",
    contractId: "W3",
    variant: "deep",
    label: "Missed own tactic (deep)",
    area: "tactics",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W4.missed",
    contractId: "W4",
    variant: "missed",
    label: "Missed mate",
    area: "tactics",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W4.allowed",
    contractId: "W4",
    variant: "allowed",
    label: "Allowed mate",
    area: "calculation",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W5",
    contractId: "W5",
    variant: null,
    label: "Conversion failure",
    area: "middlegame",
    minEvidence: 15,
    rankable: true,
  },
  {
    key: "W6",
    contractId: "W6",
    variant: null,
    label: "Defence / resourcefulness",
    area: "middlegame",
    minEvidence: 15,
    rankable: true,
  },
  {
    key: "W7.traps",
    contractId: "W7",
    variant: "traps",
    label: "Opening traps",
    area: "openings",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W7.prep",
    contractId: "W7",
    variant: "prep",
    label: "Opening preparation",
    area: "openings",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W8.basics",
    contractId: "W8",
    variant: "basics",
    label: "Endgame technique (basics)",
    area: "endgame",
    minEvidence: 50,
    rankable: true,
  },
  {
    key: "W8.advanced",
    contractId: "W8",
    variant: "advanced",
    label: "Endgame technique (advanced)",
    area: "endgame",
    minEvidence: 50,
    rankable: true,
  },
  {
    key: "W9a",
    contractId: "W9a",
    variant: null,
    label: "Time trouble",
    area: "time",
    minEvidence: 20,
    rankable: true,
  },
  {
    key: "W9b",
    contractId: "W9b",
    variant: null,
    label: "Time-pressure collapse",
    area: "time",
    minEvidence: 40,
    rankable: true,
  },
  {
    key: "W9c",
    contractId: "W9c",
    variant: null,
    label: "Rushing critical moves",
    area: "time",
    minEvidence: 60,
    rankable: true,
  },
  {
    key: "W9d",
    contractId: "W9d",
    variant: null,
    label: "Time misallocation",
    area: "time",
    minEvidence: 20,
    rankable: false,
  },
  {
    key: "W10",
    contractId: "W10",
    variant: null,
    label: "Positional drift",
    area: "middlegame",
    minEvidence: 1500,
    rankable: true,
  },
  {
    key: "W11",
    contractId: "W11",
    variant: null,
    label: "Easy-move errors (vision)",
    area: "calculation",
    minEvidence: 1000,
    rankable: true,
  },
  {
    key: "W12",
    contractId: "W12",
    variant: null,
    label: "Tilt / fatigue",
    area: "mindset",
    minEvidence: 60,
    rankable: false,
  },
];

export const MOTIFS: readonly MotifDef[] = [
  { tag: "hangingPiece", contractTag: "hanging_piece" },
  { tag: "fork", contractTag: "fork" },
  { tag: "pin", contractTag: "pin" },
  { tag: "skewer", contractTag: "skewer" },
  { tag: "discoveredAttack", contractTag: "discovered_attack" },
  { tag: "deflection", contractTag: "deflection" },
  { tag: "sacrifice", contractTag: "sacrifice" },
  { tag: "promotion", contractTag: "promotion" },
  { tag: "mate", contractTag: "mate" },
  { tag: "mateIn1", contractTag: "mate_in_1" },
  { tag: "mateIn2", contractTag: "mate_in_2" },
  { tag: "mateIn3", contractTag: "mate_in_3" },
  { tag: "mateIn4Plus", contractTag: "mate_in_4_plus" },
  { tag: "matingNet", contractTag: "mating_net" },
  { tag: "backRankMate", contractTag: "back_rank_mate" },
  { tag: "smotheredMate", contractTag: "smothered_mate" },
];

export const KPIS: readonly KpiDef[] = [
  {
    metric: "rating",
    area: null,
    kind: "last",
    unit: "elo",
    higherIsBetter: true,
    minN: 1,
    label: "Rating",
  },
  {
    metric: "perf_vs_expected",
    area: null,
    kind: "mean",
    unit: null,
    higherIsBetter: true,
    minN: 20,
    label: "Score vs expected",
  },
  {
    metric: "accuracy",
    area: null,
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "Accuracy",
  },
  {
    metric: "accuracy.opening",
    area: "openings",
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "Opening accuracy",
  },
  {
    metric: "accuracy.middlegame",
    area: "middlegame",
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "Middlegame accuracy",
  },
  {
    metric: "accuracy.endgame",
    area: "endgame",
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "Endgame accuracy",
  },
  {
    metric: "blunders_per_game",
    area: "calculation",
    kind: "mean",
    unit: "per_game",
    higherIsBetter: false,
    minN: 20,
    label: "Blunders per game",
  },
  {
    metric: "one_move_blunders_per_game",
    area: "calculation",
    kind: "mean",
    unit: "per_game",
    higherIsBetter: false,
    minN: 20,
    label: "One-move blunders per game",
  },
  {
    metric: "blunder_rate_low_clock",
    area: "time",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 40,
    label: "Blunder rate on a low clock",
  },
  {
    metric: "conversion_rate",
    area: "middlegame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: true,
    minN: 15,
    label: "Winning positions converted",
  },
  {
    metric: "resilience_rate",
    area: "middlegame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: true,
    minN: 15,
    label: "Lost positions saved",
  },
  {
    metric: "missed_punish_rate",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 30,
    label: "Mistakes left unpunished",
  },
  {
    metric: "tactics_missed_per_game",
    area: "tactics",
    kind: "mean",
    unit: "per_game",
    higherIsBetter: false,
    minN: 20,
    label: "Tactics missed per game",
  },
  {
    metric: "tactics_allowed_per_game",
    area: "tactics",
    kind: "mean",
    unit: "per_game",
    higherIsBetter: false,
    minN: 20,
    label: "Tactics allowed per game",
  },
  {
    metric: "sharp_error_rate",
    area: "calculation",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 30,
    label: "Errors in sharp positions",
  },
  {
    metric: "quiet_loss_per_move",
    area: "middlegame",
    kind: "mean",
    unit: "pct",
    higherIsBetter: false,
    minN: 20,
    label: "Win% lost per quiet move",
  },
  {
    metric: "eval_trend_15_35",
    area: "middlegame",
    kind: "mean",
    unit: null,
    higherIsBetter: true,
    minN: 20,
    label: "Eval trend, moves 15-35",
  },
  {
    metric: "time_trouble_rate",
    area: "time",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Games in time trouble",
  },
  {
    metric: "losses_on_time_rate",
    area: "time",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Games lost on time",
  },
  {
    metric: "time_per_move.opening",
    area: "time",
    kind: "mean",
    unit: "seconds",
    higherIsBetter: null,
    minN: 20,
    label: "Time per opening move",
  },
  {
    metric: "time_per_move.middlegame",
    area: "time",
    kind: "mean",
    unit: "seconds",
    higherIsBetter: null,
    minN: 20,
    label: "Time per middlegame move",
  },
  {
    metric: "time_per_move.endgame",
    area: "time",
    kind: "mean",
    unit: "seconds",
    higherIsBetter: null,
    minN: 20,
    label: "Time per endgame move",
  },
  {
    metric: "critical_time_ratio",
    area: "time",
    kind: "mean",
    unit: null,
    higherIsBetter: true,
    minN: 20,
    label: "Time on critical moves vs others",
  },
  {
    metric: "eval_after_opening",
    area: "openings",
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "Win% after the opening",
  },
  {
    metric: "opening_out_worse_rate",
    area: "openings",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Openings left worse",
  },
  {
    metric: "book_exit_ply",
    area: "openings",
    kind: "mean",
    unit: null,
    higherIsBetter: true,
    minN: 20,
    label: "Book exit ply",
  },
  {
    metric: "time_in_opening",
    area: "openings",
    kind: "mean",
    unit: "seconds",
    higherIsBetter: null,
    minN: 20,
    label: "Time spent in the opening",
  },
  {
    metric: "endgame_conversion_rate",
    area: "endgame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: true,
    minN: 15,
    label: "Winning endgames converted",
  },
  {
    metric: "endgame_hold_rate",
    area: "endgame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: true,
    minN: 15,
    label: "Drawn endgames held",
  },
  {
    metric: "tb_errors_per_endgame",
    area: "endgame",
    kind: "mean",
    unit: "per_game",
    higherIsBetter: false,
    minN: 20,
    label: "Tablebase errors per endgame",
  },
  {
    metric: "score_after_loss",
    area: "mindset",
    kind: "mean",
    unit: "rate",
    higherIsBetter: true,
    minN: 20,
    label: "Score after a loss",
  },
  {
    metric: "score_after_non_loss",
    area: "mindset",
    kind: "mean",
    unit: "rate",
    higherIsBetter: true,
    minN: 20,
    label: "Score after a win or draw",
  },
  {
    metric: "session_accuracy_decline",
    area: "mindset",
    kind: "mean",
    unit: null,
    higherIsBetter: true,
    minN: 20,
    label: "Accuracy change late in a session",
  },
  {
    metric: "first_game_accuracy",
    area: "mindset",
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "First-game accuracy",
  },
  {
    metric: "later_games_accuracy",
    area: "mindset",
    kind: "mean",
    unit: "pct",
    higherIsBetter: true,
    minN: 20,
    label: "Later-games accuracy",
  },
  {
    metric: "saveable_resignation_rate",
    area: "mindset",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 10,
    label: "Resignations in saveable positions",
  },
  {
    metric: "motif_miss_rate:hangingPiece",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed hanging pieces",
  },
  {
    metric: "motif_miss_rate:fork",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed forks",
  },
  {
    metric: "motif_miss_rate:pin",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed pins",
  },
  {
    metric: "motif_miss_rate:skewer",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed skewers",
  },
  {
    metric: "motif_miss_rate:discoveredAttack",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed discovered attacks",
  },
  {
    metric: "motif_miss_rate:deflection",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed deflections",
  },
  {
    metric: "motif_miss_rate:sacrifice",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed sacrifices",
  },
  {
    metric: "motif_miss_rate:promotion",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed promotions",
  },
  {
    metric: "motif_miss_rate:mate",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed mates",
  },
  {
    metric: "motif_miss_rate:mateIn1",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed mates in 1",
  },
  {
    metric: "motif_miss_rate:mateIn2",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed mates in 2",
  },
  {
    metric: "motif_miss_rate:mateIn3",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed mates in 3",
  },
  {
    metric: "motif_miss_rate:mateIn4Plus",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed mates in 4+",
  },
  {
    metric: "motif_miss_rate:matingNet",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed mating nets",
  },
  {
    metric: "motif_miss_rate:backRankMate",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed back-rank mates",
  },
  {
    metric: "motif_miss_rate:smotheredMate",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 20,
    label: "Missed smothered mates",
  },
  {
    metric: "weakness:W1",
    area: "calculation",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 300,
    label: "Hung material",
  },
  {
    metric: "weakness:W2",
    area: "calculation",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 100,
    label: "Missed opponent threat",
  },
  {
    metric: "weakness:W3.shallow",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 60,
    label: "Missed own tactic (shallow)",
  },
  {
    metric: "weakness:W3.deep",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 60,
    label: "Missed own tactic (deep)",
  },
  {
    metric: "weakness:W4.missed",
    area: "tactics",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 40,
    label: "Missed mate",
  },
  {
    metric: "weakness:W4.allowed",
    area: "calculation",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 40,
    label: "Allowed mate",
  },
  {
    metric: "weakness:W5",
    area: "middlegame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 30,
    label: "Conversion failure",
  },
  {
    metric: "weakness:W6",
    area: "middlegame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 30,
    label: "Defence / resourcefulness",
  },
  {
    metric: "weakness:W7.traps",
    area: "openings",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 40,
    label: "Opening traps",
  },
  {
    metric: "weakness:W7.prep",
    area: "openings",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 40,
    label: "Opening preparation",
  },
  {
    metric: "weakness:W8.basics",
    area: "endgame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 80,
    label: "Endgame technique (basics)",
  },
  {
    metric: "weakness:W8.advanced",
    area: "endgame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 80,
    label: "Endgame technique (advanced)",
  },
  {
    metric: "weakness:W9a",
    area: "time",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 30,
    label: "Time trouble",
  },
  {
    metric: "weakness:W9b",
    area: "time",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 40,
    label: "Time-pressure collapse",
  },
  {
    metric: "weakness:W9c",
    area: "time",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 60,
    label: "Rushing critical moves",
  },
  {
    metric: "weakness:W10",
    area: "middlegame",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 600,
    label: "Positional drift",
  },
  {
    metric: "weakness:W11",
    area: "calculation",
    kind: "rate",
    unit: "rate",
    higherIsBetter: false,
    minN: 300,
    label: "Easy-move errors (vision)",
  },
];

const weaknessByKey = new Map(WEAKNESSES.map((w) => [w.key, w]));
const kpiByMetric = new Map(KPIS.map((k) => [k.metric, k]));

export function weaknessDef(key: string): WeaknessDef | null {
  return weaknessByKey.get(key) ?? null;
}

export function kpiDef(metric: string): KpiDef | null {
  return kpiByMetric.get(metric) ?? null;
}

const ENDGAME_LABELS: Record<string, string> = {
  pawn: "Pawn endgames",
  rook: "Rook endgames",
  minor: "Minor-piece endgames",
  queen: "Queen endgames",
  mixed: "Mixed endgames",
};

const COLOURS: Record<string, string> = { white: "White", black: "Black" };

/** A weakness-style key (`W1`, `W3.shallow`, `W9a`), known or not. */
const W_KEY = /^W\d+[a-z]?(\.[a-z]+)?$/;

function words(slug: string): string {
  const text = slug.replace(/[-_]+/g, " ").trim();
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * The coaching area of a series. `W*` keys take their Rust area (an unknown variant takes its
 * family's, e.g. `W3.x` that of `W3.shallow`); `motif.*` series are tactics, `opening:*` openings
 * and `endgame:*` endgame. Anything else is tactics, the area of the motif detectors.
 */
export function seriesArea(seriesKey: string): Area {
  const def = weaknessDef(seriesKey);
  if (def) return def.area;
  if (W_KEY.test(seriesKey)) {
    const family = seriesKey.split(".")[0];
    const sibling = WEAKNESSES.find((w) => w.contractId === family);
    if (sibling) return sibling.area;
  }
  if (seriesKey.startsWith("opening:")) return "openings";
  if (seriesKey.startsWith("endgame:")) return "endgame";
  return "tactics";
}

/**
 * The English label of a series (ADR 0068): the Rust weakness label, the KPI label of a missed
 * motif ("Missed forks") and its allowed twin ("Allowed forks"), an opening's name (else its slug
 * in words) with the player's colour, or the endgame type. An unknown key is shown as itself.
 */
export function seriesLabel(seriesKey: string, openingName?: string | null): string {
  const def = weaknessDef(seriesKey);
  if (def) return def.label;
  const missed = seriesKey.match(/^motif\.(missed|allowed):(.+)$/);
  if (missed) {
    const label = kpiDef(`motif_miss_rate:${missed[2]}`)?.label;
    if (label) return missed[1] === "missed" ? label : label.replace(/^Missed /, "Allowed ");
    return seriesKey;
  }
  const opening = seriesKey.match(/^opening:(white|black):(.+)$/);
  if (opening) {
    const name = openingName?.trim() ? openingName.trim() : words(opening[2] ?? "");
    return `${name} (as ${COLOURS[opening[1] ?? ""]})`;
  }
  const endgame = seriesKey.match(/^endgame:(.+)$/);
  if (endgame) return ENDGAME_LABELS[endgame[1] ?? ""] ?? seriesKey;
  return seriesKey;
}

/** The contract `RankedWeakness.id` of a weakness series (`W3.shallow` gives `W3`); null otherwise. */
export function contractId(seriesKey: string): string | null {
  const def = weaknessDef(seriesKey);
  if (def) return def.contractId;
  return W_KEY.test(seriesKey) ? (seriesKey.split(".")[0] ?? null) : null;
}
