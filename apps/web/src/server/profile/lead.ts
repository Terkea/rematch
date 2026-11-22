import type { Db } from "@ci/db";
import { readOnly } from "../games";
import { resolveIn } from "./scope";
import type { LeadFinding, WeaknessState } from "./types";
import { summariesIn } from "./weaknesses";

/** States a lead may be in: a problem that is current, not one that is fading or gone. */
const LEAD_STATES: ReadonlySet<WeaknessState> = new Set(["active", "emerging", "relapsed"]);

/**
 * The overview's "What to work on" (ADR 0068): over the default scope, the top weakness with
 * `priority > 0`, a current state, sufficient evidence (R3) and a benchmark. `none` when no
 * weakness qualifies, saying whether anything was compared at all (bug hunt I1); `building` when
 * the user has no analysed game yet.
 */
export async function leadFinding(db: Db, userId: string): Promise<LeadFinding> {
  return readOnly(db, async (tx) => {
    const scope = await resolveIn(tx, userId, {});
    if (!scope) return { kind: "building" };
    const all = await summariesIn(tx, scope);
    const weakness = all.find(
      (w) => w.compared && w.priority > 0 && LEAD_STATES.has(w.state) && w.evidence.sufficient,
    );
    if (weakness) return { kind: "weakness", scope, weakness };
    const analysedGames =
      scope.options.find((o) => o.accountId === scope.accountId && o.timeClass === scope.timeClass)
        ?.analysedGames ?? 0;
    return { kind: "none", scope, analysedGames, compared: all.some((w) => w.compared) };
  });
}
