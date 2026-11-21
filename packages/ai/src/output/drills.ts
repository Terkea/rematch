export const DRILL_TYPES = [
  "own_mistake_puzzles",
  "safe_unsafe",
  "themed_puzzles",
  "long_calculation",
  "mate_patterns",
  "convert_vs_engine",
  "defend_vs_engine",
  "opening_trainer",
  "opening_trap_puzzles",
  "tablebase_drills",
  "clock_pressure",
  "critical_or_not",
  "blitz_safety",
  "model_games",
  "guess_the_move",
  "session_rules",
  "slow_games",
  "own_game_analysis",
] as const;
export type DrillType = (typeof DRILL_TYPES)[number];

const BASE: Record<string, readonly DrillType[]> = {
  W1: ["own_mistake_puzzles", "safe_unsafe", "themed_puzzles", "slow_games"],
  W2: ["own_mistake_puzzles", "safe_unsafe", "themed_puzzles", "slow_games"],
  W11: ["own_mistake_puzzles", "safe_unsafe", "themed_puzzles", "slow_games"],
  W3: ["themed_puzzles"],
  W4: ["mate_patterns"],
  W5: ["convert_vs_engine"],
  W6: ["defend_vs_engine", "themed_puzzles"],
  W7: ["opening_trainer", "opening_trap_puzzles"],
  W8: ["tablebase_drills"],
  W9a: ["clock_pressure", "critical_or_not", "slow_games"],
  W9c: ["clock_pressure", "critical_or_not", "slow_games"],
  W9d: ["clock_pressure", "critical_or_not", "slow_games"],
  W9b: ["blitz_safety", "clock_pressure"],
  W10: ["model_games", "guess_the_move"],
  W12: ["session_rules"],
};

/** Allowed drill types for a weakness (improvement-engine §6.4). Doses belong to Milestone 6. */
export function drillsFor(weaknessId: string, variant: string | null): readonly DrillType[] {
  const base = BASE[weaknessId];
  if (!base) return [];
  const list: DrillType[] =
    weaknessId === "W3" && variant === "deep" ? ["long_calculation"] : [...base];
  if (weaknessId !== "W12") list.push("own_game_analysis");
  return list;
}
