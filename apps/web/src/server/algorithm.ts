import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";
import { ALGORITHM_ID, ALGORITHM_MANIFEST } from "../lib/algorithm";

// The algorithm the web stamps on the rows it writes (`focuses`, `drill_items`; ADR 0105).
// `test/algorithm.test.ts` fails when the trainer or focus rules no longer match the manifest.

export {
  ALGORITHM_ID,
  ALGORITHM_VERSION,
  type AlgorithmLink,
  algorithmLink,
  changelogAnchor,
} from "../lib/algorithm";

/**
 * Inserts this build's manifest once, in the writer's transaction, so the row's foreign key
 * holds. Returns the id to stamp.
 */
export async function registerAlgorithm(tx: Db): Promise<string> {
  await tx.execute(sql`
    insert into algorithm_versions (id, manifest)
    values (${ALGORITHM_ID}, ${JSON.stringify(ALGORITHM_MANIFEST)}::jsonb)
    on conflict (id) do nothing`);
  return ALGORITHM_ID;
}
