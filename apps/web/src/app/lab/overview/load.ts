import { requireUser } from "../../../server/auth/session";
import { getDb } from "../../../server/db";
import type { TimeClass } from "../../../server/games/types";
import { getOverview, type Overview } from "../../../server/overview";

/** The signed-in user's real overview (all time controls by default). Lab only. */
export async function loadLabOverview(timeClass: TimeClass | null = null): Promise<Overview> {
  const user = await requireUser();
  return getOverview(getDb(), user.id, { timeClass });
}

export type ClassOverview = { timeClass: TimeClass; overview: Overview };

/** The overview for all games, plus one per time control that has analysed games (variant c). */
export async function loadLabOverviewByClass(): Promise<{
  all: Overview;
  classes: ClassOverview[];
}> {
  const user = await requireUser();
  const db = getDb();
  const all = await getOverview(db, user.id, { timeClass: null });
  const classes = await Promise.all(
    all.timeClasses.map(async ({ timeClass }) => ({
      timeClass,
      overview: await getOverview(db, user.id, { timeClass }),
    })),
  );
  return { all, classes };
}
