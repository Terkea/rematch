// The contract Milestone 2's CoachingInput must satisfy; the integration step (Task I1) reconciles names.
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

const UCI_RE = /^[a-h][1-8][a-h][1-8][qrbn]?$/;

export const AreaId = z.enum(AREA_IDS);
export type AreaId = z.infer<typeof AreaId>;

export const BandId = z.enum(BAND_IDS);
export type BandId = z.infer<typeof BandId>;

export const WeaknessState = z.enum(WEAKNESS_STATES);
export type WeaknessState = z.infer<typeof WeaknessState>;

export const WeaknessId = z.string().regex(/^W(1[0-2]|[1-9])[a-d]?$/);
export type WeaknessId = z.infer<typeof WeaknessId>;

const Uci = z.string().regex(UCI_RE);
const IsoDatetime = z.iso.datetime();
const IsoDate = z.iso.date();
const Pct = z.number().min(0).max(100);
const Unit = z.number().min(0).max(1);

/** White point of view. */
export const Score = z.union([
  z.strictObject({ cp: z.number().int() }),
  z.strictObject({ mate: z.number().int() }),
]);
export type Score = z.infer<typeof Score>;

export const MoveRef = z.strictObject({ san: z.string().min(1), uci: Uci });
export type MoveRef = z.infer<typeof MoveRef>;

export const ExemplarPosition = z.strictObject({
  gameId: z.string().min(1),
  ply: z.number().int().min(1),
  playedAt: IsoDatetime,
  fen: z.string().refine((f) => validateFen(f).ok, { message: "invalid FEN" }),
  played: MoveRef,
  best: MoveRef.nullable(),
  bestPv: z.array(Uci),
  /** Starts from the position after `played`. */
  refutationPv: z.array(Uci),
  tags: z.array(z.string()),
  evalBefore: Score,
  evalAfter: Score,
  winBefore: Pct,
  winAfter: Pct,
});
export type ExemplarPosition = z.infer<typeof ExemplarPosition>;

export const Metric = z.strictObject({
  key: z.string().min(1),
  area: AreaId,
  label: z.string().min(1),
  value: z.number(),
  unit: z.enum(["rate", "percent", "per_game", "seconds", "cp", "score"]),
  n: z.number().int().min(0),
  low: z.number().nullable(),
  high: z.number().nullable(),
  bandMedian: z.number().nullable(),
  bandPercentile: Pct.nullable(),
  sufficient: z.boolean(),
  needMore: z.number().int().min(0).nullable(),
});
export type Metric = z.infer<typeof Metric>;

export const RankedWeakness = z.strictObject({
  id: WeaknessId,
  variant: z.string().nullable(),
  label: z.string().min(1),
  area: AreaId,
  rank: z.number().int().min(1),
  posteriorRate: Unit,
  intervalLow: z.number(),
  intervalHigh: z.number(),
  bandMedian: z.number(),
  bandPercentile: Pct,
  excessEplPerGame: z.number(),
  confidence: Unit,
  priority: z.number(),
  state: WeaknessState,
  pImp: Unit.nullable(),
  events: z.number().int().min(0),
  opportunities: z.number().int().min(0),
  exemplars: z.array(ExemplarPosition).max(5),
});
export type RankedWeakness = z.infer<typeof RankedWeakness>;

/** Added per controller ruling: strengths carry cited exemplar positions too. */
export const Strength = z.strictObject({
  area: AreaId,
  label: z.string().min(1),
  metricKey: z.string().nullable(),
  exemplars: z.array(ExemplarPosition).max(5),
});
export type Strength = z.infer<typeof Strength>;

export const OpeningVerdict = z.strictObject({
  lineKey: z.string().min(1),
  name: z.string().min(1),
  eco: z.string().nullable(),
  color: z.enum(["white", "black"]),
  side: z.enum(["chosen", "faced"]),
  movesSan: z.array(z.string()),
  games: z.number().int().min(0),
  score: Unit,
  performanceVsExpected: z.number().nullable(),
  avgEvalAtOpeningEnd: Score.nullable(),
  verdict: z.enum(["keep", "fix", "learn", "replace"]),
  deviationPly: z.number().int().nullable(),
  deviationMoveSan: z.string().nullable(),
  alternatives: z.array(
    z.strictObject({
      name: z.string().min(1),
      movesSan: z.array(z.string()),
      bandScore: Unit,
      games: z.number().int().min(0),
    }),
  ),
  examples: z.array(ExemplarPosition).max(3),
});
export type OpeningVerdict = z.infer<typeof OpeningVerdict>;

export const AreaScore = z.strictObject({
  area: AreaId,
  score: Pct.nullable(),
  n: z.number().int().min(0),
  sufficient: z.boolean(),
  needMore: z.number().int().min(0).nullable(),
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
  description: z.string().min(1),
  before: z.union([z.string(), z.number()]),
  after: z.union([z.string(), z.number()]),
});
export type CandidateCause = z.infer<typeof CandidateCause>;

export const TimelineChange = z.strictObject({
  seriesKey: z.string().min(1),
  weaknessId: WeaknessId.nullable(),
  label: z.string().min(1),
  fromState: WeaknessState,
  toState: WeaknessState,
  onsetGameId: z.string().min(1),
  onsetDate: IsoDate,
  onsetFrom: IsoDate,
  onsetTo: IsoDate,
  rateBefore: z.number(),
  rateAfter: z.number(),
  posteriorProb: Unit,
  candidateCauses: z.array(CandidateCause),
  examples: z
    .array(
      z.strictObject({
        ...ExemplarPosition.shape,
        side: z.enum(["before", "after"]),
      }),
    )
    .max(6),
});
export type TimelineChange = z.infer<typeof TimelineChange>;

export const Basis = z.strictObject({
  analysedGames: z.number().int().min(0),
  importedGames: z.number().int().min(0),
  windowStart: IsoDate,
  windowEnd: IsoDate,
  backfillComplete: z.boolean(),
});
export type Basis = z.infer<typeof Basis>;

export const CoachingInput = z.strictObject({
  inputVersion: z.literal("coaching-input-v1"),
  accountId: z.uuid(),
  platform: z.enum(["lichess", "chesscom"]),
  timeClass: z.enum(["bullet", "blitz", "rapid", "classical", "daily"]),
  timeControl: z
    .strictObject({
      baseSeconds: z.number().int().min(0),
      incrementSeconds: z.number().int().min(0),
    })
    .nullable(),
  band: BandId,
  rating: z.number().int().nullable(),
  generatedAt: IsoDatetime,
  basis: Basis,
  areaScores: z.array(AreaScore).superRefine((scores, ctx) => {
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
  metrics: z.array(Metric),
  weaknesses: z.array(RankedWeakness),
  strengths: z.array(Strength),
  openings: z.array(OpeningVerdict),
  changes: z.array(TimelineChange),
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
