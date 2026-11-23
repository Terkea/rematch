import type { Db } from "@ci/db";
import { gameMetrics, weaknessSeriesPoint } from "@ci/db/generated";
import { seedSyntheticGame } from "./games";

// Test-only writers for the weekly focus (ADR 0090). Every row is synthetic.

export const DAY = 86_400_000;
/** A Thursday: the focus tests start here. */
export const START = new Date(Date.UTC(2026, 9, 1, 12));

/** An analysed rapid game `days` after START with `[events, chances]` for each series given. */
export async function play(
  d: Db,
  account: string,
  days: number,
  series: Record<string, [number, number]>,
  colour: "white" | "black" = "white",
) {
  const playedAt = new Date(START.getTime() + days * DAY);
  const { gameId } = await seedSyntheticGame(d, account, {
    playedAt: playedAt.toISOString(),
    timeClass: "rapid",
    result: "1-0",
    playerColor: colour,
  });
  await d.insert(gameMetrics).values({
    gameId,
    accountId: account,
    timeClass: "rapid",
    playedAt,
    protocol: "test-1",
    metrics: { version: 1 },
  });
  for (const [seriesKey, [events, opportunities]] of Object.entries(series)) {
    await d.insert(weaknessSeriesPoint).values({
      accountId: account,
      timeClass: "rapid",
      seriesKey,
      gameId,
      playedAt,
      events,
      opportunities,
      epl: 0,
      protocol: "test-1",
    });
  }
}
