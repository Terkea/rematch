// The benchmark's words (ADR 0080), with no database import so client components and the
// report can name it too.

export type Benchmark =
  | {
      source: "peers";
      platform: "chesscom" | "lichess";
      /** The rating window, on the platform's own scale. */
      low: number;
      high: number;
      /** Distinct opponents in the window, and their games. */
      players: number;
      games: number;
    }
  | { source: "lichess_bands" };

const PLATFORM = { chesscom: "chess.com", lichess: "lichess" } as const;
const count = new Intl.NumberFormat("en-GB");

/** The bias every peer benchmark carries, stated wherever it is named. */
export const BENCHMARK_BIAS_TEXT =
  "Only their games against you count, so it is a rough guide, not a rating-band average.";

const fin = (n: number) => Number.isFinite(n);

/**
 * "Compared with players you've faced on chess.com, rated about 650–950 (12 players)", or null
 * when the benchmark is not a peer band (the legacy band reference, or none).
 */
export function benchmarkLabel(b: Benchmark | null): string | null {
  if (b?.source !== "peers" || !fin(b.low) || !fin(b.high) || !fin(b.players)) return null;
  const n = Math.max(0, Math.trunc(b.players));
  return `Compared with players you've faced on ${PLATFORM[b.platform]}, rated about ${count.format(b.low)}–${count.format(b.high)} (${count.format(n)} ${n === 1 ? "player" : "players"})`;
}

/** The short group name a weakness row compares with ("Players you've faced, rated about …"). */
export function benchmarkGroupLabel(b: Benchmark | null): string | undefined {
  if (b?.source !== "peers" || !fin(b.low) || !fin(b.high)) return undefined;
  return `Players you've faced, rated about ${count.format(b.low)}–${count.format(b.high)}`;
}
