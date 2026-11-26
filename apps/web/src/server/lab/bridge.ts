import { z } from "zod";
import { ALGORITHM_ID } from "../../lib/algorithm";
import { chooseTarget, FOCUS_VERSION, focusProgress } from "../focus/goal";
import {
  isHeldOut,
  keepCandidate,
  primarySource,
  SOURCES,
  TRAINER_VERSION,
} from "../train/model";

// The lab's bridge to the web's rules (#125, ADR 0115): `node lab-bridge.mjs <step>` reads one
// JSON request on stdin and writes one JSON response on stdout, as `cli lab` does for the Rust
// rules (ADR 0106). Each step calls the exported functions of `focus/goal.ts` or `train/model.ts`
// and nothing else, so the lab measures the rule the app runs (ADR 0104: one implementation).
// No database, no Next.js, no clock: the same request gives the same bytes. The messages are in
// `lab/schemas/lab-v1.schema.json`; the web's own objects (`Target`, `Progress`) keep their
// camelCase, as the Rust steps keep the CoachingInput's.

export const LAB_SCHEMA_VERSION = "lab-v1";
export const LAB_STEPS = ["focus-target", "focus-progress", "trainer-sources"] as const;
export type LabStep = (typeof LAB_STEPS)[number];

const version = z.literal(LAB_SCHEMA_VERSION, {
  error: (i) => `schema_version ${JSON.stringify(i.input)} is not supported (this bridge speaks ${LAB_SCHEMA_VERSION})`,
});
const rate = z.number().min(0).max(1);
const count = z.number().int().min(0);

const FocusTargetRequest = z.strictObject({
  schema_version: version,
  items: z.array(
    z.strictObject({
      id: z.string(),
      /** The weakness's current rate per chance (the estimate's mean). */
      baseline: z.number(),
      /** The peer median the app would offer, or null (`focus/index.ts` `candidate`). */
      peer: z.number().nullable(),
    }),
  ),
});

const FocusProgressRequest = z.strictObject({
  schema_version: version,
  items: z.array(
    z.strictObject({
      id: z.string(),
      k: count,
      n: count,
      games: count,
      games_n: z.number().int().min(1),
      target: rate,
    }),
  ),
});

const TrainerSourcesRequest = z.strictObject({
  schema_version: version,
  candidates: z.array(
    z.strictObject({
      game_id: z.string(),
      ply: count,
      fen: z.string(),
      uci: z.string(),
      best_uci: z.string().nullable(),
      win_before: z.number(),
      win_after: z.number(),
      prev_san: z.string().nullable(),
      sources: z.array(z.enum(SOURCES)),
    }),
  ),
});

const envelope = () => ({ schema_version: LAB_SCHEMA_VERSION, algorithm_id: ALGORITHM_ID });

function parse<T>(schema: z.ZodType<T>, input: string): T {
  let doc: unknown;
  try {
    doc = JSON.parse(input);
  } catch (e) {
    throw new Error(`the request is not JSON: ${(e as Error).message}`);
  }
  const r = schema.safeParse(doc);
  if (!r.success) {
    const lines = r.error.issues.map((i) => `${i.path.join("/") || "<root>"}: ${i.message}`);
    throw new Error(`bad request:\n  ${lines.slice(0, 20).join("\n  ")}`);
  }
  return r.data;
}

/** `focus-target`: `chooseTarget` per item. */
function focusTarget(input: string) {
  const req = parse(FocusTargetRequest, input);
  return {
    ...envelope(),
    focus_version: FOCUS_VERSION,
    items: req.items.map((i) => ({
      id: i.id,
      target: chooseTarget({ baseline: i.baseline, peer: i.peer }),
    })),
  };
}

/** `focus-progress`: `focusProgress` per item. */
function focusProgressStep(input: string) {
  const req = parse(FocusProgressRequest, input);
  return {
    ...envelope(),
    focus_version: FOCUS_VERSION,
    items: req.items.map((i) => ({
      id: i.id,
      progress: focusProgress({ k: i.k, n: i.n, games: i.games, gamesN: i.games_n, target: i.target }),
    })),
  };
}

/** `trainer-sources`: `keepCandidate`, `primarySource` and `isHeldOut` per candidate. */
function trainerSources(input: string) {
  const req = parse(TrainerSourcesRequest, input);
  return {
    ...envelope(),
    trainer_version: TRAINER_VERSION,
    sources: [...SOURCES],
    candidates: req.candidates.map((c) => ({
      game_id: c.game_id,
      ply: c.ply,
      kept: keepCandidate({
        gameId: c.game_id,
        ply: c.ply,
        fen: c.fen,
        uci: c.uci,
        bestUci: c.best_uci,
        winBefore: c.win_before,
        winAfter: c.win_after,
        prevSan: c.prev_san,
        sources: c.sources,
      }),
      primary_source: primarySource(c.sources),
      held_out: isHeldOut(c.game_id, c.ply),
    })),
  };
}

export const isLabStep = (v: unknown): v is LabStep =>
  typeof v === "string" && (LAB_STEPS as readonly string[]).includes(v);

/** One step on a request string: pretty JSON with a final newline. Throws on a bad request. */
export function runLabStep(step: string, input: string): string {
  if (!isLabStep(step)) throw new Error(`usage: lab-bridge ${LAB_STEPS.join("|")} < request.json`);
  const out =
    step === "focus-target"
      ? focusTarget(input)
      : step === "focus-progress"
        ? focusProgressStep(input)
        : trainerSources(input);
  return `${JSON.stringify(out, null, 2)}\n`;
}
