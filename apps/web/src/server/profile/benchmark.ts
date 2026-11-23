import type { Db } from "@ci/db";
import { sql } from "drizzle-orm";
import type { HeadlinePeer } from "../../lib/explain";
import { readOnly } from "../games";
import type { Benchmark } from "./benchmark-text";
import { ownsScope } from "./scope";
import type { CoachScope } from "./types";

export {
  BENCHMARK_BIAS_TEXT,
  type Benchmark,
  baselineGroupLabel,
  benchmarkGroupLabel,
  benchmarkLabel,
  MODEL_BASELINE_LABEL,
  notComparedText,
  peerPooledBasis,
  peerRateText,
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
            and s.side = 'both' and not s.low_sample)`);
    if (!row) return null;
    const stats = await tx.execute<{ key: string; chances: number }>(sql`
      select substr(metric, length('weakness:') + 1) as key, n_chances as chances
      from account_benchmark_stat
      where account_id = ${scope.accountId} and time_class = ${scope.timeClass}
        and side = ${scope.side ?? "both"} and metric like 'weakness:%' and n_chances is not null`);
    const chances: Record<string, number> = {};
    for (const s of stats) chances[s.key] = Number(s.chances);
    // One colour's opponents (A11): the header counts every game; a colour's rows count only
    // the opponents who had that colour, and the widest row (accuracy, any analysed game) says
    // how many there were.
    if (scope.side === "white" || scope.side === "black") {
      const [own] = await tx.execute<{ players: number | null; games: number | null }>(sql`
        select max(n_players) as players, max(n_games) as games
        from account_benchmark_stat
        where account_id = ${scope.accountId} and time_class = ${scope.timeClass}
          and side = ${scope.side}`);
      row.players = own?.players == null ? 0 : Number(own.players);
      row.games = own?.games == null ? 0 : Number(own.games);
    }
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
      chances,
    };
  });
}

/**
 * The overview headline's comparison (Q1): the scope's peer band and the opponents' median
 * blunders a game (`blunders_per_game`, both colours, served rows only). Null without a peer
 * benchmark; `blunders` is null when that row is low-sample or missing.
 */
export async function peerHeadline(
  db: Db,
  userId: string,
  scope: CoachScope,
): Promise<HeadlinePeer | null> {
  const benchmark = await getBenchmark(db, userId, { ...scope, side: "both" });
  if (benchmark?.source !== "peers") return null;
  return readOnly(db, async (tx) => {
    const [row] = await tx.execute<{ median: number | null }>(sql`
      select median from account_benchmark_stat
      where account_id = ${scope.accountId} and time_class = ${scope.timeClass}
        and side = 'both' and metric = 'blunders_per_game' and not low_sample`);
    const median = row?.median == null ? null : Number(row.median);
    return {
      platform: benchmark.platform,
      low: benchmark.low,
      high: benchmark.high,
      players: benchmark.players,
      blunders: median !== null && Number.isFinite(median) ? median : null,
    };
  });
}
