import { z } from "zod";
import { AREA_IDS } from "../input/schema.js";
import { DRILL_TYPES } from "./drills.js";

export const VALIDATION_CODES = [
  "schema_invalid",
  "refusal",
  "truncated",
  "wrong_section",
  "unknown_citation",
  "missing_citation",
  "unknown_move",
  "unknown_number",
  "unknown_date",
  "unknown_weakness",
  "unknown_line",
  "verdict_mismatch",
  "focus_not_selected",
  "too_many_focuses",
  "drill_not_allowed",
  "area_missing",
  "banned_phrase",
  "likelihood_mismatch",
  "missing_evidence_window",
  "causal_claim",
  "missing_hedge",
  "marketing",
  "insufficient_data_not_stated",
] as const;
export type ValidationCode = (typeof VALIDATION_CODES)[number];

/** Declared here so A4 (validator) imports it. */
export type ValidationError = { code: ValidationCode; path: string; detail: string };

export const SECTION_KEYS = ["overview", "weaknesses", "openings", "climb_plan", "areas"] as const;
export type SectionKey = (typeof SECTION_KEYS)[number];

// Built from factories: zod's JSON schema emits $defs/$ref for a schema instance used
// more than once, and the output format must be ref-free.
const citation = () => z.strictObject({ gameId: z.string(), ply: z.number().int() });
const itemShape = () => ({ text: z.string().min(1), citations: z.array(citation()) });
const item = () => z.strictObject(itemShape());
const area = () => z.enum(AREA_IDS);
// Array length bounds are set at construction: .min()/.max() clone the schema, and zod
// then emits a $ref to the parent, which the output format must not contain.
const bounded = <T extends z.ZodType>(element: T, min: number | null, max: number | null) =>
  new z.ZodArray({
    type: "array",
    element,
    checks: [
      ...(min === null ? [] : [z.minLength(min)]),
      ...(max === null ? [] : [z.maxLength(max)]),
    ],
  }) as unknown as z.ZodArray<T>;

export const Citation = citation();
export type Citation = z.infer<typeof Citation>;
export const Item = item();
export type Item = z.infer<typeof Item>;

export const OverviewSection = z.strictObject({
  headline: item(),
  summary: item(),
  strengths: bounded(z.strictObject({ ...itemShape(), area: area() }), null, 4),
});
export type OverviewSection = z.infer<typeof OverviewSection>;

export const WeaknessesSection = z.strictObject({
  items: z.array(z.strictObject({ ...itemShape(), weaknessId: z.string(), title: z.string() })),
});
export type WeaknessesSection = z.infer<typeof WeaknessesSection>;

export const OPENING_VERDICTS = ["keep", "fix", "learn", "replace"] as const;
export const OpeningsSection = z.strictObject({
  items: z.array(
    z.strictObject({ ...itemShape(), lineKey: z.string(), verdict: z.enum(OPENING_VERDICTS) }),
  ),
});
export type OpeningsSection = z.infer<typeof OpeningsSection>;

export const ClimbPlanSection = z.strictObject({
  intro: item(),
  focuses: bounded(
    z.strictObject({
      weaknessId: z.string(),
      why: item(),
      drills: bounded(
        z.strictObject({ drillType: z.enum(DRILL_TYPES), rationale: z.string() }),
        1,
        3,
      ),
      habit: item(),
    }),
    null,
    3,
  ),
  weeklyRhythm: item(),
});
export type ClimbPlanSection = z.infer<typeof ClimbPlanSection>;

export const AreasSection = z.strictObject({
  items: z.array(z.strictObject({ ...itemShape(), area: area() })),
});
export type AreasSection = z.infer<typeof AreasSection>;

/** One schema for every section call, so the output format (and cached prefix) is identical. */
export const ReportSectionEnvelope = z.strictObject({
  section: z.enum(SECTION_KEYS),
  overview: OverviewSection.nullable(),
  weaknesses: WeaknessesSection.nullable(),
  openings: OpeningsSection.nullable(),
  climbPlan: ClimbPlanSection.nullable(),
  areas: AreasSection.nullable(),
});
export type ReportSectionEnvelope = z.infer<typeof ReportSectionEnvelope>;

/** Post-validation shapes: dropped singleton Items become null, dropped array entries are removed. */
export type ValidatedSection = {
  overview:
    | (Omit<OverviewSection, "headline" | "summary"> & {
        headline: Item | null;
        summary: Item | null;
      })
    | null;
  weaknesses: WeaknessesSection | null;
  openings: OpeningsSection | null;
  climbPlan:
    | (Omit<ClimbPlanSection, "intro" | "weeklyRhythm" | "focuses"> & {
        intro: Item | null;
        weeklyRhythm: Item | null;
        focuses: (Omit<ClimbPlanSection["focuses"][number], "why" | "habit"> & {
          why: Item | null;
          habit: Item | null;
        })[];
      })
    | null;
  areas: AreasSection | null;
};

export type DroppedItem = { section: SectionKey; path: string; errors: ValidationError[] };
