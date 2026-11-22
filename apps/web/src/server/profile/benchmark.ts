import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";
import { readOnly } from "../games";
import type { Benchmark } from "./benchmark-text";
import { ownsScope } from "./scope";
import type { CoachScope } from "./types";

export {
  BENCHMARK_BIAS_TEXT,
  type Benchmark,
  benchmarkGroupLabel,
  benchmarkLabel,
} from "./benchmark-text";

// The benchmark a scope's last build-profile compared with (ADR 0080). By default it is the
// players the account has faced: the opponent side of its own analysed games, on the account's
// own rating scale. Rust stores the band in `account_benchmark` and its rows in
// `account_benchmark_stat`; the web only reads and names them.

type Row = {
  source: string;
  low: number | null;
  high: number | null;
  players: number | null;
  games: number | null;
};

/**
 * The scope's stored benchmark, or null without one, or when none of its rows is served (every
 * row low-sample: nothing was compared).
 */
export async function getBenchmark(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<Benchmark | null> {
  return readOnly(db, async (tx) => {
    if (!(await ownsScope(tx, userId, scope))) return null;
    const [row] = await tx.execute<Row>(sql`
      select b.source, b.rating_low as low, b.rating_high as high, b.n_players as players,
        b.n_games as games
      from account_benchmark b
      where b.account_id = ${scope.accountId} and b.time_class = ${scope.timeClass}
        and exists (select 1 from account_benchmark_stat s
          where s.account_id = b.account_id and s.time_class = b.time_class
            and not s.low_sample)`);
    if (!row) return null;
    if (row.source === "lichess_bands") return { source: "lichess_bands" };
    if (
      row.source !== "peers" ||
      row.low == null ||
      row.high == null ||
      row.players == null ||
      row.games == null
    ) {
      return null;
    }
    return {
      source: "peers",
      platform: scope.platform,
      low: row.low,
      high: row.high,
      players: row.players,
      games: row.games,
    };
  });
}
