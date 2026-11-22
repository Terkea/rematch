// The benchmark's words (ADRs 0080, 0081), with no database import so client components and
// the report can name it too.

import { shareText } from "@ci/ui/profile";

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
      /**
       * Raw chances in the window each weakness row's pooled rate is pooled over, by series key
       * ("W4.missed"). A row without a count is absent.
       */
      chances: Record<string, number>;
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

/** "pooled over 65 of their chances": what a peer row's rate is (ADR 0081). */
export function peerPooledBasis(b: Benchmark | null, seriesKey: string): string | undefined {
  if (b?.source !== "peers") return undefined;
  const n = b.chances[seriesKey];
  if (n == null || !fin(n) || n < 0) return undefined;
  return `pooled over ${count.format(Math.trunc(n))} of their chances`;
}

/**
 * "Players you've faced, rated about 650–950: 9.6% (pooled over 65 of their chances)", or null
 * when the benchmark is not a peer band or the rate is not a number.
 */
export function peerRateText(b: Benchmark | null, seriesKey: string, rate: number): string | null {
  const group = benchmarkGroupLabel(b);
  if (group === undefined || !fin(rate)) return null;
  const basis = peerPooledBasis(b, seriesKey);
  return `${group}: ${shareText(rate)}${basis ? ` (${basis})` : ""}`;
}
