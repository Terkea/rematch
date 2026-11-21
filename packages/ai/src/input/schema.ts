// The contract Milestone 2's CoachingInput must satisfy; the integration step (Task I1) reconciles names.
//
// Conventions used throughout:
// - Every object is strict: unknown keys are rejected, so name drift surfaces at integration.
// - Every array and string is bounded (see LIMITS). This is the trust boundary before anything is
//   put into a prompt, so oversize input is rejected rather than truncated.
// - `Score` (evaluations) is always from White's point of view. `winBefore`/`winAfter` are the
//   MOVER's win percentage (0..100), so a drop means the mover's position got worse.
// - `ply` is 1-based half-moves: ply 1 is White's first move, ply 2 Black's first reply.
import { validateFen } from "chess.js";
import { z } from "zod";

export const AREA_IDS = [
  "openings",
  "middlegame",
  "endgame",
  "tactics",
  "calculation",
  "time",
  "mindset",
] as const;
export const BAND_IDS = ["B0", "B1", "B2", "B3", "B4", "B5"] as const;
export const WEAKNESS_STATES = [
  "not_an_issue",
  "emerging",
  "active",
  "improving",
  "resolved",
  "relapsed",
] as const;

/** Size bounds applied at the trust boundary. */
export const LIMITS = {
  pvPlies: 16,
  tags: 12,
  tagChars: 40,
  metrics: 80,
  weaknesses: 24,
  strengths: 12,
  openings: 30,
  changes: 20,
  alternatives: 5,
  candidateCauses: 6,
  movesSan: 40,
  sanChars: 12,
  labelChars: 120,
  descriptionChars: 600,
  idChars: 64,
  exemplarsPerItem: 5,
  examplesPerOpening: 3,
  examplesPerChange: 6,
} as const;

const UCI_RE = /^[a-h][1-8][a-h][1-8][qrbn]?$/;

export const AreaId = z.enum(AREA_IDS);
export type AreaId = z.infer<typeof AreaId>;

export const BandId = z.enum(BAND_IDS);
export type BandId = z.infer<typeof BandId>;

export const WeaknessState = z.enum(WEAKNESS_STATES);
export type WeaknessState = z.infer<typeof WeaknessState>;

/** Weakness catalogue id: W1..W12, optionally with a variant letter a-d (e.g. "W9b"). */
export const WeaknessId = z
  .string()
  .max(5)
  .regex(/^W(1[0-2]|[1-9])[a-d]?$/);
export type WeaknessId = z.infer<typeof WeaknessId>;

const Label = z.string().min(1).max(LIMITS.labelChars);
const Description = z.string().min(1).max(LIMITS.descriptionChars);
const Id = z.string().min(1).max(LIMITS.idChars);
const San = z.string().min(1).max(LIMITS.sanChars);
const SanLine = z.array(San).max(LIMITS.movesSan);
const Uci = z.string().regex(UCI_RE);
const UciLine = z.array(Uci).max(LIMITS.pvPlies);
const IsoDatetime = z.iso.datetime().max(40);
const IsoDate = z.iso.date();
/** 0..100 percentage. */
const Pct = z.number().min(0).max(100);
/** 0..1 probability or rate. */
const Unit = z.number().min(0).max(1);
const Count = z.number().int().min(0).max(100_000_000);

/**
 * An engine evaluation from WHITE's point of view: `cp` is centipawns (positive = White better),
 * `mate` is moves to mate (positive = White mates, negative = Black mates).
 */
export const Score = z.union([
  z.strictObject({ cp: z.number().int().min(-100_000).max(100_000) }),
  z.strictObject({ mate: z.number().int().min(-500).max(500) }),
]);
export type Score = z.infer<typeof Score>;

/** A move as SAN (display) and UCI (machine, e.g. "e2e4" or "e7e8q"). */
export const MoveRef = z.strictObject({ san: San, uci: Uci });
export type MoveRef = z.infer<typeof MoveRef>;

const exemplarFields = {
  /** Platform game id. */
  gameId: Id,
  /** 1-based half-move number of the position's side to move: odd = White to move, even = Black. */
  ply: z.number().int().min(1).max(1000),
  /** When the game was played (ISO datetime). */
  playedAt: IsoDatetime,
  /** Position BEFORE `played`, standard FEN. */
  fen: z
    .string()
    .max(100)
    .refine((f) => validateFen(f).ok, { message: "invalid FEN" }),
  /** The move the player actually played. */
  played: MoveRef,
  /** The engine's preferred move, or null when `played` was already best. */
  best: MoveRef.nullable(),
  /** Engine line starting with the best move from `fen` (UCI, up to 16 plies). */
  bestPv: UciLine,
  /** Engine line starting from the position AFTER `played` (UCI, up to 16 plies). */
  refutationPv: UciLine,
  /** Short classifier tags such as "hung_piece". */
  tags: z.array(z.string().min(1).max(LIMITS.tagChars)).max(LIMITS.tags),
  /** Evaluation before the move, White POV. */
  evalBefore: Score,
  /** Evaluation after the move, White POV. */
  evalAfter: Score,
  /** The mover's win percentage before the move, 0..100. */
  winBefore: Pct,
  /** The mover's win percentage after the move, 0..100. */
  winAfter: Pct,
};

type ExemplarShape = {
  ply: number;
  fen: string;
};

/** The FEN's side to move must agree with the ply parity (odd ply = White to move). */
function checkSideToMove(e: ExemplarShape, ctx: z.RefinementCtx): void {
  const turn = e.fen.split(" ")[1];
  if (turn !== "w" && turn !== "b") return; // invalid FEN is reported by the fen field itself
  const expected = e.ply % 2 === 1 ? "w" : "b";
  if (turn !== expected) {
    ctx.addIssue({
      code: "custom",
      path: ["fen"],
      message: `FEN side to move "${turn}" disagrees with ply ${e.ply} (expected "${expected}")`,
    });
  }
}

/** A concrete position from one of the player's games, cited as evidence. */
export const ExemplarPosition = z.strictObject(exemplarFields).superRefine(checkSideToMove);
export type ExemplarPosition = z.infer<typeof ExemplarPosition>;

/**
 * What a metric's `value`, `low`, `high` and `bandMedian` measure:
 * - `rate`: events per opportunity, 0..1
 * - `per_game`: a count per game, >= 0
 * - `pct`: a percentage, 0..100
 * - `seconds`: a duration in seconds, >= 0
 * - `elo`: a rating-point quantity (e.g. a performance rating), 0..4000
 */
export const METRIC_UNITS = ["rate", "per_game", "pct", "seconds", "elo"] as const;
export type MetricUnit = (typeof METRIC_UNITS)[number];

const UNIT_MAX: Record<MetricUnit, number> = {
  rate: 1,
  per_game: 1000,
  pct: 100,
  seconds: 100_000,
  elo: 4000,
};

export const Metric = z
  .strictObject({
    /** Stable metric key. */
    key: z.string().min(1).max(LIMITS.idChars),
    area: AreaId,
    label: Label,
    /** Point estimate in `unit`. */
    value: z.number(),
    unit: z.enum(METRIC_UNITS),
    /** Number of observations behind the estimate. */
    n: Count,
    /** Lower end of the credible interval, in `unit`; null when not computed. */
    low: z.number().nullable(),
    /** Upper end of the credible interval, in `unit`; null when not computed. */
    high: z.number().nullable(),
    /** Median of the player's rating band for this metric, in `unit`. */
    bandMedian: z.number().nullable(),
    /** Percentile within the band, 0..100. */
    bandPercentile: Pct.nullable(),
    /** True when `n` meets the minimum sample for this metric. */
    sufficient: z.boolean(),
    /** How many more observations are needed to become sufficient; null when sufficient. */
    needMore: Count.nullable(),
  })
  .superRefine((m, ctx) => {
    const max = UNIT_MAX[m.unit];
    for (const field of ["value", "low", "high", "bandMedian"] as const) {
      const v = m[field];
      if (v !== null && (v < 0 || v > max)) {
        ctx.addIssue({
          code: "custom",
          path: [field],
          message: `${field} ${v} is outside 0..${max} for unit "${m.unit}"`,
        });
      }
    }
    if (m.low !== null && m.high !== null && m.low > m.high) {
      ctx.addIssue({ code: "custom", path: ["low"], message: "low must be <= high" });
    }
  });
export type Metric = z.infer<typeof Metric>;

export const RankedWeakness = z
  .strictObject({
    id: WeaknessId,
    /** Variant name within the weakness (e.g. "fork"); null when none. */
    variant: z.string().min(1).max(LIMITS.tagChars).nullable(),
    label: Label,
    area: AreaId,
    /** 1 = highest priority. Unique across `weaknesses`. */
    rank: z.number().int().min(1).max(LIMITS.weaknesses),
    /** Posterior event rate: events per opportunity, 0..1. */
    posteriorRate: Unit,
    /** Credible interval bounds for `posteriorRate` (events per opportunity, 0..1). */
    intervalLow: Unit,
    intervalHigh: Unit,
    /** Median event rate of the player's band (0..1). */
    bandMedian: Unit,
    /** Percentile within the band, 0..100. */
    bandPercentile: Pct,
    /** Expected points lost per game beyond the band median, >= 0. */
    excessEplPerGame: z.number().min(0).max(100),
    /** Confidence that this is a real weakness, 0..1. */
    confidence: Unit,
    /** Ranking score, 0..1000; larger = more important. */
    priority: z.number().min(0).max(1000),
    state: WeaknessState,
    /** Probability the player is improving; null when not applicable. */
    pImp: Unit.nullable(),
    /** Observed events (e.g. blunders of this type). */
    events: Count,
    /** Opportunities in which the event could have happened; events <= opportunities. */
    opportunities: Count,
    /** Cited positions, up to 5. */
    exemplars: z.array(ExemplarPosition).max(LIMITS.exemplarsPerItem),
  })
  .superRefine((w, ctx) => {
    if (w.intervalLow > w.intervalHigh) {
      ctx.addIssue({
        code: "custom",
        path: ["intervalLow"],
        message: "intervalLow must be <= intervalHigh",
      });
    }
    if (w.posteriorRate < w.intervalLow || w.posteriorRate > w.intervalHigh) {
      ctx.addIssue({
        code: "custom",
        path: ["posteriorRate"],
        message: "posteriorRate must lie within [intervalLow, intervalHigh]",
      });
    }
    if (w.events > w.opportunities) {
      ctx.addIssue({
        code: "custom",
        path: ["events"],
        message: "events must be <= opportunities",
      });
    }
  });
export type RankedWeakness = z.infer<typeof RankedWeakness>;

/** A strength, cited with exemplar positions (controller ruling). */
export const Strength = z.strictObject({
  area: AreaId,
  label: Label,
  /** Key of the supporting Metric, when there is one. */
  metricKey: z.string().min(1).max(LIMITS.idChars).nullable(),
  /** Cited positions (0..5). */
  exemplars: z.array(ExemplarPosition).max(LIMITS.exemplarsPerItem),
});
export type Strength = z.infer<typeof Strength>;

export const OpeningVerdict = z.strictObject({
  /** Stable key for the line. */
  lineKey: z.string().min(1).max(LIMITS.idChars),
  name: Label,
  /** ECO code such as "B90"; null when unknown. */
  eco: z.string().min(1).max(8).nullable(),
  /** The player's colour in these games. */
  color: z.enum(["white", "black"]),
  /** "chosen": the player plays this line; "faced": the opponent plays it against the player. */
  side: z.enum(["chosen", "faced"]),
  /** Moves of the line in SAN from the starting position. */
  movesSan: SanLine,
  games: Count,
  /** Average result for the player, 0..1 (win = 1, draw = 0.5). */
  score: Unit,
  /** score minus the expected score at this rating (-1..1); null when unavailable. */
  performanceVsExpected: z.number().min(-1).max(1).nullable(),
  /** Evaluation at the end of the opening phase, White POV; null when unavailable. */
  avgEvalAtOpeningEnd: Score.nullable(),
  verdict: z.enum(["keep", "fix", "learn", "replace"]),
  /** Ply (1-based half-move) where the player left theory; null when never. */
  deviationPly: z.number().int().min(1).max(1000).nullable(),
  /** The move played at `deviationPly`, SAN. */
  deviationMoveSan: San.nullable(),
  alternatives: z
    .array(
      z.strictObject({
        name: Label,
        movesSan: SanLine,
        /** Band-average score with this alternative, 0..1. */
        bandScore: Unit,
        games: Count,
      }),
    )
    .max(LIMITS.alternatives),
  examples: z.array(ExemplarPosition).max(LIMITS.examplesPerOpening),
});
export type OpeningVerdict = z.infer<typeof OpeningVerdict>;

export const AreaScore = z.strictObject({
  area: AreaId,
  /** 0..100 (higher = stronger); null when there is not enough data. */
  score: Pct.nullable(),
  /** Observations behind the score. */
  n: Count,
  sufficient: z.boolean(),
  /** More observations needed to reach sufficiency; null when sufficient. */
  needMore: Count.nullable(),
});
export type AreaScore = z.infer<typeof AreaScore>;

export const CandidateCause = z.strictObject({
  kind: z.enum([
    "time_control_mix",
    "new_opening",
    "session_length",
    "rating_level",
    "protocol_version",
  ]),
  /** A correlated possible cause, never proven. */
  description: Description,
  /** Value before the change point. */
  before: z.union([z.string().max(LIMITS.labelChars), z.number()]),
  /** Value after the change point. */
  after: z.union([z.string().max(LIMITS.labelChars), z.number()]),
});
export type CandidateCause = z.infer<typeof CandidateCause>;

const ChangeExample = z
  .strictObject({
    ...exemplarFields,
    /** Whether the example is from before or after the change point. */
    side: z.enum(["before", "after"]),
  })
  .superRefine(checkSideToMove);

export const TimelineChange = z
  .strictObject({
    /** Key of the time series the change was detected on. */
    seriesKey: z.string().min(1).max(LIMITS.idChars),
    weaknessId: WeaknessId.nullable(),
    label: Label,
    fromState: WeaknessState,
    toState: WeaknessState,
    /** Game at which the change is placed. */
    onsetGameId: Id,
    onsetDate: IsoDate,
    /** Uncertainty window for the onset: onsetFrom <= onsetDate <= onsetTo. */
    onsetFrom: IsoDate,
    onsetTo: IsoDate,
    /** Event rate (0..1) before the change. */
    rateBefore: Unit,
    /** Event rate (0..1) after the change. */
    rateAfter: Unit,
    /** Posterior probability that a change happened, 0..1. */
    posteriorProb: Unit,
    candidateCauses: z.array(CandidateCause).max(LIMITS.candidateCauses),
    examples: z.array(ChangeExample).max(LIMITS.examplesPerChange),
  })
  .superRefine((c, ctx) => {
    if (c.onsetFrom > c.onsetDate || c.onsetDate > c.onsetTo) {
      ctx.addIssue({
        code: "custom",
        path: ["onsetDate"],
        message: "must satisfy onsetFrom <= onsetDate <= onsetTo",
      });
    }
  });
export type TimelineChange = z.infer<typeof TimelineChange>;

export const Basis = z
  .strictObject({
    /** Games with engine analysis, the evidence window size. */
    analysedGames: Count,
    /** Games imported from the platform (>= analysedGames). */
    importedGames: Count,
    windowStart: IsoDate,
    windowEnd: IsoDate,
    /** Whether the full history import finished. */
    backfillComplete: z.boolean(),
  })
  .superRefine((b, ctx) => {
    if (b.analysedGames > b.importedGames) {
      ctx.addIssue({
        code: "custom",
        path: ["analysedGames"],
        message: "analysedGames must be <= importedGames",
      });
    }
    if (b.windowStart > b.windowEnd) {
      ctx.addIssue({
        code: "custom",
        path: ["windowStart"],
        message: "windowStart must be <= windowEnd",
      });
    }
  });
export type Basis = z.infer<typeof Basis>;

export const CoachingInput = z.strictObject({
  inputVersion: z.literal("coaching-input-v1"),
  accountId: z.uuid(),
  platform: z.enum(["lichess", "chesscom"]),
  timeClass: z.enum(["bullet", "blitz", "rapid", "classical", "daily"]),
  /** Typical time control in seconds; null when mixed or unknown. */
  timeControl: z
    .strictObject({
      baseSeconds: z.number().int().min(0).max(10_000_000),
      incrementSeconds: z.number().int().min(0).max(100_000),
    })
    .nullable(),
  band: BandId,
  /** Current rating; null when unknown. */
  rating: z.number().int().min(0).max(4000).nullable(),
  generatedAt: IsoDatetime,
  basis: Basis,
  /** Each AREA_IDS value exactly once. */
  areaScores: z
    .array(AreaScore)
    .max(AREA_IDS.length)
    .superRefine((scores, ctx) => {
      const seen = new Map<string, number>();
      for (const s of scores) seen.set(s.area, (seen.get(s.area) ?? 0) + 1);
      const ok = AREA_IDS.every((a) => seen.get(a) === 1) && scores.length === AREA_IDS.length;
      if (!ok) {
        ctx.addIssue({
          code: "custom",
          message: `areaScores must contain each of ${AREA_IDS.join(", ")} exactly once`,
        });
      }
    }),
  metrics: z.array(Metric).max(LIMITS.metrics),
  /** Ranked weaknesses; `rank` values are unique. */
  weaknesses: z
    .array(RankedWeakness)
    .max(LIMITS.weaknesses)
    .superRefine((ws, ctx) => {
      const seen = new Set<number>();
      ws.forEach((w, i) => {
        if (seen.has(w.rank)) {
          ctx.addIssue({
            code: "custom",
            path: [i, "rank"],
            message: `duplicate rank ${w.rank}`,
          });
        }
        seen.add(w.rank);
      });
    }),
  strengths: z.array(Strength).max(LIMITS.strengths),
  openings: z.array(OpeningVerdict).max(LIMITS.openings),
  changes: z.array(TimelineChange).max(LIMITS.changes),
});
export type CoachingInput = z.infer<typeof CoachingInput>;

export type CoachingInputIssue = { path: string; message: string };
export type ParseCoachingInputResult =
  | { ok: true; input: CoachingInput }
  | { ok: false; issues: CoachingInputIssue[] };

export function parseCoachingInput(raw: unknown): ParseCoachingInputResult {
  const r = CoachingInput.safeParse(raw);
  if (r.success) return { ok: true, input: r.data };
  return {
    ok: false,
    issues: r.error.issues.map((i) => ({
      path: i.path.map(String).join("."),
      // Unrecognized-key issues carry the key names in `keys`; surface them.
      message: i.code === "unrecognized_keys" ? `${i.message} (${i.keys.join(", ")})` : i.message,
    })),
  };
}
