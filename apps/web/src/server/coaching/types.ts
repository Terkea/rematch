// The coaching read model's wire types: TS mirrors of the R3 JSON the worker stores (camelCase).
// Sources: `backend/crates/ai/src/segments.rs`, `output/{report,weekly,disclosure,codes}.rs`,
// `generate/{computed,payload}.rs` and `backend/crates/coaching-input/src/types.rs`. The
// contract snapshots are `backend/fixtures/ai/payload/report-payload*.example.json`.

import type { reportRuns, reportSections, reports } from "@ci/db/generated";
import type { TimeClass } from "../games/types";

/** One coach scope: a linked account and a time class. Task 1's `CoachScope` is a superset. */
export type CoachTarget = { accountId: string; timeClass: TimeClass };

export type SectionKey = "overview" | "weaknesses" | "openings" | "climb_plan" | "areas";
/** Rust `SECTION_KEYS`, in this order. */
export const SECTION_KEYS: readonly SectionKey[] = [
  "overview",
  "weaknesses",
  "openings",
  "climb_plan",
  "areas",
];

export type AreaId =
  | "openings"
  | "middlegame"
  | "endgame"
  | "tactics"
  | "calculation"
  | "time"
  | "mindset";
export type BandId = "B0" | "B1" | "B2" | "B3" | "B4" | "B5";
export type WeaknessStateId =
  | "not_an_issue"
  | "emerging"
  | "active"
  | "improving"
  | "resolved"
  | "relapsed";
export type Verdict = "keep" | "fix" | "learn" | "replace";
export type DrillType =
  | "own_mistake_puzzles"
  | "safe_unsafe"
  | "themed_puzzles"
  | "long_calculation"
  | "mate_patterns"
  | "convert_vs_engine"
  | "defend_vs_engine"
  | "opening_trainer"
  | "opening_trap_puzzles"
  | "tablebase_drills"
  | "clock_pressure"
  | "critical_or_not"
  | "blitz_safety"
  | "model_games"
  | "guess_the_move"
  | "session_rules"
  | "slow_games"
  | "own_game_analysis";

/** Rust `segments.rs` `Segment` (tag `kind`). Model text is rendered only from these (ADR 0060). */
export type Segment =
  | { kind: "text"; text: string }
  | { kind: "move"; gameId: string; ply: number; san: string }
  | { kind: "best"; gameId: string; ply: number; san: string }
  | { kind: "pv"; gameId: string; ply: number; side: "best" | "refutation"; n: number; san: string }
  | { kind: "line"; lineKey: string; n: number | null; san: string }
  | { kind: "num"; path: string; value: number; display: string }
  | { kind: "date"; path: string; date: string }
  | { kind: "square"; square: string }
  | { kind: "eng"; evalId: string; line: number | null; n: number | null; text: string };

export type Citation = { gameId: string; ply: number };

/** A validated model statement. `text` keeps the `{{…}}` tokens and is never rendered raw. */
export type NarrativeItem = { text: string; segments: Segment[]; citations: Citation[] };

/** Rust `AiDisclosure` (spec §14). */
export type AiDisclosure = {
  label: string;
  note: string;
  model: string;
  promptVersion: string;
  generatedAt: string;
};

// --- Computed report (`reports.computed`) ---

export type Score = { cp: number } | { mate: number };
export type MoveRef = { san: string; uci: string };

export type ExemplarPosition = {
  gameId: string;
  ply: number;
  playedAt: string;
  fen: string;
  played: MoveRef;
  best: MoveRef | null;
  bestPv: string[];
  refutationPv: string[];
  tags: string[];
  evalBefore: Score;
  evalAfter: Score;
  winBefore: number;
  winAfter: number;
};

export type Basis = {
  analysedGames: number;
  importedGames: number;
  windowStart: string;
  windowEnd: string;
  backfillComplete: boolean;
};

export type AreaScoreRow = {
  area: AreaId;
  score: number | null;
  n: number;
  sufficient: boolean;
  needMore: number | null;
};

export type MetricRow = {
  key: string;
  area: AreaId;
  label: string;
  value: number;
  unit: "rate" | "per_game" | "pct" | "seconds" | "elo";
  n: number;
  low: number | null;
  high: number | null;
  bandMedian: number | null;
  bandPercentile: number | null;
  sufficient: boolean;
  needMore: number | null;
};

export type RankedWeakness = {
  id: string;
  variant: string | null;
  label: string;
  area: AreaId;
  rank: number;
  posteriorRate: number;
  intervalLow: number;
  intervalHigh: number;
  bandMedian: number;
  bandPercentile: number;
  excessEplPerGame: number;
  confidence: number;
  priority: number;
  state: WeaknessStateId;
  pImp: number | null;
  events: number;
  opportunities: number;
  exemplars: ExemplarPosition[];
};

export type Strength = {
  area: AreaId;
  label: string;
  metricKey: string | null;
  exemplars: ExemplarPosition[];
};

export type OpeningVerdict = {
  lineKey: string;
  name: string;
  eco: string | null;
  color: "white" | "black";
  side: "chosen" | "faced";
  movesSan: string[];
  games: number;
  score: number;
  performanceVsExpected: number | null;
  avgEvalAtOpeningEnd: Score | null;
  verdict: Verdict;
  deviationPly: number | null;
  deviationMoveSan: string | null;
  alternatives: { name: string; movesSan: string[]; bandScore: number; games: number }[];
  examples: ExemplarPosition[];
};

export type TimelineChange = {
  seriesKey: string;
  weaknessId: string | null;
  label: string;
  fromState: WeaknessStateId;
  toState: WeaknessStateId;
  onsetGameId: string;
  onsetDate: string;
  onsetFrom: string;
  onsetTo: string;
  rateBefore: number;
  rateAfter: number;
  posteriorProb: number;
  candidateCauses: {
    kind:
      | "time_control_mix"
      | "new_opening"
      | "session_length"
      | "rating_level"
      | "protocol_version";
    description: string;
    before: number | string;
    after: number | string;
  }[];
  examples: (ExemplarPosition & { side: "before" | "after" })[];
};

/** Rust `ColorMetric` (coaching-input-v2, ADR 0084): null numbers when not sufficient. */
export type ColorMetric = {
  value: number | null;
  unit: "rate" | "pct" | "per_game";
  n: number;
  low: number | null;
  high: number | null;
  sufficient: boolean;
  needMore: number | null;
};

/** Rust `ColorWeakness`: a rate over one colour's games, with that colour's own peers. */
export type ColorWeakness = {
  seriesKey: string;
  id: string;
  variant: string | null;
  label: string;
  area: AreaId;
  rate: number;
  low: number;
  high: number;
  events: number;
  opportunities: number;
  peerRate: number | null;
  peerPercentile: number | null;
  peerPlayers: number | null;
};

/** Rust `ColorSummary`: the player's games with one colour (the last 50 at most). */
export type ColorSummary = {
  games: number;
  sufficient: boolean;
  needMore: number | null;
  wins: number;
  draws: number;
  losses: number;
  score: ColorMetric;
  accuracy: ColorMetric;
  blundersPerGame: ColorMetric;
  weaknesses: ColorWeakness[];
};

export type ByColor = { white: ColorSummary; black: ColorSummary };

/** Rust `Benchmark`: who the comparisons are with (ADR 0080). */
export type InputBenchmark = {
  source: "peers" | "lichess_bands";
  ratingLow: number | null;
  ratingHigh: number | null;
  players: number | null;
  games: number | null;
};

/** Rust `ComputedReport`: the engine-derived part, shown with or without the AI narrative. */
export type ComputedReport = {
  player: {
    platform: "lichess" | "chesscom";
    timeClass: TimeClass;
    band: BandId;
    rating: number | null;
  };
  basis: Basis;
  evidenceWindow: string;
  areaScores: AreaScoreRow[];
  metrics: MetricRow[];
  weaknesses: RankedWeakness[];
  openings: OpeningVerdict[];
  strengths: Strength[];
  focuses: string[];
  newThisPeriod: TimelineChange[];
  fixedThisPeriod: TimelineChange[];
  /** coaching-input-v2's colour split; absent in reports from a v1 input. */
  byColor?: ByColor;
  /** v2: the benchmark band; absent without one (and in v1 reports). */
  benchmark?: InputBenchmark;
};

// --- Validated sections (`report_sections.content`) ---

export type OverviewSection = {
  headline: NarrativeItem | null;
  summary: NarrativeItem | null;
  strengths: (NarrativeItem & { area: AreaId })[];
};
export type WeaknessesSection = {
  items: (NarrativeItem & { weaknessId: string; title: string })[];
};
export type OpeningsSection = {
  items: (NarrativeItem & { lineKey: string; verdict: Verdict })[];
};
export type ClimbPlanSection = {
  intro: NarrativeItem | null;
  focuses: {
    weaknessId: string;
    why: NarrativeItem | null;
    drills: { drillType: DrillType; rationale: string }[];
    habit: NarrativeItem | null;
  }[];
  weeklyRhythm: NarrativeItem | null;
};
export type AreasSection = { items: (NarrativeItem & { area: AreaId })[] };

/** Rust `ValidatedSection`: every key present, the section's own part set, the others null. */
export type SectionContent = {
  overview: OverviewSection | null;
  weaknesses: WeaknessesSection | null;
  openings: OpeningsSection | null;
  climbPlan: ClimbPlanSection | null;
  areas: AreasSection | null;
};

export type PayloadDroppedItem = {
  section: string;
  path: string;
  errors: { code: string; path: string }[];
};

/**
 * Rust `ReportPayload`. In `sections` a dropped section is null and a section not generated
 * (yet) is absent, exactly as the Rust payload, so the type is a `Partial` record.
 */
export type ReportPayload = {
  schemaVersion: string;
  reportId: string;
  version: number;
  status: "computed_only" | "partial" | "complete";
  computed: ComputedReport;
  sections: Partial<Record<SectionKey, SectionContent | null>>;
  dropped: PayloadDroppedItem[];
  aiDisclosure: AiDisclosure | null;
  generatedAt: string;
};

// --- Weekly review (`weekly_reviews.content`, Rust `ValidatedWeeklyReview`) ---

export type WeeklyContent = {
  headline: NarrativeItem | null;
  process: NarrativeItem | null;
  outcomes: (NarrativeItem & { weaknessId: string; label: string })[];
  exemplars: NarrativeItem[];
  nextWeek: NarrativeItem | null;
};

/** The `ai_item_status` enum (weekly reviews, change notes). */
export type AiItemStatus = "pending" | "ready" | "dropped" | "failed";

// --- Stored rows (the Drizzle schema generated from db/migrations) ---

export type ReportRunRow = typeof reportRuns.$inferSelect;
export type ReportRow = typeof reports.$inferSelect;
export type ReportSectionRow = typeof reportSections.$inferSelect;
export type ReportRunStatus = ReportRunRow["status"];
