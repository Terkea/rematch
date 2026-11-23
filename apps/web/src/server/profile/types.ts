import type { Area } from "@ci/ui/area-score";
import type { TimelineSeries, WeaknessState } from "@ci/ui/timeline";
import type { TimeClass } from "../games/types";

export type { Area, TimelineSeries, WeaknessState };

/**
 * A forced mate a move gave up (`lost`: the mover had mate in `n` and after it had none) or
 * handed over (`allowed`: the opponent then had mate in `n`). Lichess judges these by the mate
 * rule (chess-core `judge`), so a `??` can come with a small win% drop.
 */
export type MateChange = { kind: "lost" | "allowed"; n: number };

/** One (account, time class) the coach reads (ruling R1, ADR 0068). */
/** Which games a stored profile row covers (ADR 0082): every game, or one colour's. */
export type Side = "both" | "white" | "black";

export type CoachScope = {
  accountId: string;
  timeClass: TimeClass;
  /**
   * The colour split's side the stored estimates and snapshots are read for (default "both").
   * Timeline states (`weakness_state_history`) have no side: they are always every game's.
   */
  side?: Side;
  platform: "lichess" | "chesscom";
  username: string;
  options: {
    accountId: string;
    platform: "lichess" | "chesscom";
    username: string;
    timeClass: TimeClass;
    analysedGames: number;
  }[];
};

/** An interval exactly as stored (80%); the web never widens or recomputes it. */
export type StoredInterval = { low: number; high: number; level: 0.8 };

export type WeaknessSummary = {
  seriesKey: string;
  contractId: string | null;
  label: string;
  area: Area;
  state: WeaknessState;
  /** The stored posterior `mean`, "estimated rate", with its stored `ci_low`–`ci_high` (R3). */
  rate: { mean: number } & StoredInterval;
  evidence: {
    /** `k_unweighted`. */
    events: number;
    /** `n_unweighted`. */
    chances: number;
    /** Rust `min_evidence`; null for a series without one (never sufficient). */
    minEvidence: number | null;
    sufficient: boolean;
    /** Chances still needed; null when sufficient or when there is no minimum. */
    needMore: number | null;
  };
  band: { median: number | null; p40: number | null };
  /**
   * Whether the estimate was compared with a rating-band benchmark (stored `excess` and band
   * median). False means its state is no claim: show "Not compared yet", never "not an issue".
   */
  compared: boolean;
  priority: number;
  confidence: number;
  pImp: number | null;
  updatedAt: Date;
  onset: { at: Date; low: Date | null; high: Date | null; gameId: string | null } | null;
};

/** A position from the newest report input (ruling R2). */
export type Exemplar = {
  gameId: string;
  ply: number;
  playedAt: string;
  fen: string;
  played: { san: string; uci: string };
  best: { san: string; uci: string } | null;
  bestPv: string[];
  refutationPv: string[];
  tags: string[];
  winBefore: number;
  winAfter: number;
  /** A forced mate the move gave up or allowed, from the report's stored scores (critic 1). */
  mate?: MateChange;
};

export type WeaknessDetail = WeaknessSummary & {
  /** The estimate's evidence window: `days` before its build (`updatedAt`); weekly covers it. */
  window: { days: number; from: Date; to: Date };
  history: {
    kind: "transition" | "onset";
    fromState: WeaknessState | null;
    toState: WeaknessState;
    at: Date | null;
    onsetAt: Date | null;
    onsetLow: Date | null;
    onsetHigh: Date | null;
    pChange: number | null;
    rateBefore: number | null;
    rateAfter: number | null;
    causeHints: unknown[];
    /**
     * The row's stored `evidence` counts (Rust `Evidence { k, n }`): the current segment, which on
     * a confirmed change (rates present) starts at the onset, so they are the after side's counts.
     * Null when the stored JSON lacks them.
     */
    counts: { events: number; chances: number } | null;
  }[];
  timeline: TimelineSeries;
  weekly: {
    week: string;
    events: number;
    chances: number;
    rate: ({ value: number } & StoredInterval) | null;
  }[];
  exemplars: { fromReportAt: Date; items: Exemplar[] } | null;
  /**
   * At most `MAX_EXAMPLES` positions for the page: the report's exemplars first, then the scope's
   * stored occurrences (`game_metrics` exemplar plies, the player's own moves within the window),
   * newest game first, one per game.
   */
  examples: Example[];
};

/** An example position, from the report (`report`) or from the stored occurrences (`games`). */
export type Example = Omit<Exemplar, "mate"> & {
  source: "report" | "games";
  /** The stored judgement of the played move, from `moves` when the game is imported. */
  judgement: "inaccuracy" | "mistake" | "blunder" | null;
  /** A forced mate the move gave up or allowed (the Lichess mate rule), or null. */
  mate: MateChange | null;
};

export type KpiPoint = {
  week: string;
  value: number | null;
  low: number | null;
  high: number | null;
  n: number;
};

export type KpiCurrent = {
  metric: string;
  label: string;
  unit: string | null;
  higherIsBetter: boolean | null;
  value: number | null;
  low: number | null;
  high: number | null;
  n: number;
  minN: number;
  sufficient: boolean;
  needMore: number | null;
  computedAt: Date;
};

export type KpiTrend = {
  metric: string;
  label: string;
  unit: string | null;
  current: KpiCurrent | null;
  points: KpiPoint[];
};

export type AreaView = {
  area: Area;
  score: { value: number | null; n: number; computedAt: Date } | null;
  kpis: KpiCurrent[];
  weaknesses: WeaknessSummary[];
  timeline: TimelineSeries[];
};

export type DifficultyStatus = {
  positions: number;
  pendingMaia: number;
  ok: number;
  notApplicable: number;
};

export type LeadFinding =
  | { kind: "weakness"; scope: CoachScope; weakness: WeaknessSummary }
  | {
      kind: "none";
      scope: CoachScope;
      analysedGames: number;
      /** False when no weakness of the scope had a rating-band benchmark (nothing was compared). */
      compared: boolean;
    }
  | { kind: "building" };
