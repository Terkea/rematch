// The words of the report's White vs Black block (ADR 0085), from the stored computed report's
// `byColor`. A plain module, so it is unit tested on its own. Honesty rules: n beside every
// number, a dash and the shortfall for anything below its minimum (never 0), and a difference
// called only when the two 80% ranges separate.

import type {
  ColorMetric,
  ColorSummary,
  ColorWeakness,
  InputBenchmark,
} from "../../../../../server/coaching/types";

export type Colour = "white" | "black";
export type ColourMetricKind = "score" | "accuracy" | "blunders";

export const COLOUR_NAME: Record<Colour, string> = { white: "White", black: "Black" };

const count = new Intl.NumberFormat("en-GB");
const fin = (n: number | null | undefined): n is number =>
  typeof n === "number" && Number.isFinite(n);
const int = (n: number) => count.format(Math.max(0, Math.trunc(n)));
const plural = (n: number, one: string, many: string) => `${int(n)} ${n === 1 ? one : many}`;

/** "42 games: 20 won, 4 drawn, 18 lost", or "No analysed games yet". */
export function colourRecordText(s: ColorSummary): string {
  if (!fin(s.games) || s.games <= 0) return "No analysed games yet";
  return `${plural(s.games, "game", "games")}: ${int(s.wins)} won, ${int(s.draws)} drawn, ${int(s.losses)} lost`;
}

/** A number in the block's own format: a share as a whole percent, an accuracy with one decimal. */
function formatValue(v: number, kind: ColourMetricKind): string {
  switch (kind) {
    case "score":
      return `${Math.round(v * 100)}%`;
    case "accuracy":
      return `${v.toFixed(1)}%`;
    case "blunders":
      return v.toFixed(1);
  }
}

function formatRange(low: number, high: number, kind: ColourMetricKind): string {
  const strip = (s: string) => s.replace(/%$/, "");
  const lo = formatValue(low, kind);
  const hi = formatValue(high, kind);
  return kind === "blunders" ? `${lo}–${hi}` : `${strip(lo)}–${hi}`;
}

/**
 * The value and its detail line: "52%" with "80% range 44–60%", or "—" with "Need 3 more games"
 * when the number is not sufficient (it has no value then, never 0).
 */
export function colourMetricText(
  m: ColorMetric,
  kind: ColourMetricKind,
): { value: string; detail: string } {
  if (!m.sufficient || !fin(m.value)) {
    const need = fin(m.needMore) && m.needMore > 0 ? m.needMore : null;
    return {
      value: "—",
      detail: need ? `Need ${plural(need, "more game", "more games")}` : "Not measured yet",
    };
  }
  const range =
    fin(m.low) && fin(m.high) ? `80% range ${formatRange(m.low, m.high, kind)}` : "No range yet";
  return { value: formatValue(m.value, kind), detail: range };
}

const VERB: Record<ColourMetricKind, [more: string, less: string]> = {
  score: ["You score more", "You score less"],
  accuracy: ["You play more accurately", "You play less accurately"],
  blunders: ["You blunder more", "You blunder less"],
};

/**
 * One sentence on how the colours compare on a number, or null when either colour has too few
 * games: a direction only when White's and Black's 80% ranges do not overlap, else "no clear
 * difference". `higherIsBetter` only picks the wording's subject (the better colour).
 */
export function colourCompareText(
  kind: ColourMetricKind,
  white: ColorMetric,
  black: ColorMetric,
  higherIsBetter = true,
): string | null {
  const usable = (m: ColorMetric) => m.sufficient && fin(m.value) && fin(m.low) && fin(m.high);
  if (!usable(white) || !usable(black)) return null;
  const [wl, wh, bl, bh] = [white.low, white.high, black.low, black.high] as number[];
  const whiteHigher = (wl as number) > (bh as number);
  const blackHigher = (bl as number) > (wh as number);
  if (!whiteHigher && !blackHigher) {
    return "No clear difference between your White and Black games yet.";
  }
  const [more, less] = VERB[kind];
  // Name the colour that is better: the higher one, or the lower one when lower is better.
  const better: Colour = whiteHigher === higherIsBetter ? "white" : "black";
  const verb = (better === "white") === whiteHigher ? more : less;
  return `${verb} as ${COLOUR_NAME[better]}.`;
}

/**
 * Who a colour's weakness rate is compared with: that colour's own peers (the opponents who had
 * that colour, ADR 0082), with the band from the benchmark; never "players at your level".
 */
export function colourWeaknessPeerText(
  w: ColorWeakness,
  colour: Colour,
  benchmark: InputBenchmark | null | undefined,
): string {
  if (!fin(w.peerRate)) {
    return `Not compared: no players you've faced with ${COLOUR_NAME[colour]} to compare with yet`;
  }
  const band =
    benchmark?.source === "peers" && fin(benchmark.ratingLow) && fin(benchmark.ratingHigh)
      ? `, rated about ${int(benchmark.ratingLow)}–${int(benchmark.ratingHigh)}`
      : "";
  const players = fin(w.peerPlayers) ? ` (${plural(w.peerPlayers, "player", "players")})` : "";
  return `Players you've faced who had ${COLOUR_NAME[colour]}${band}: ${(w.peerRate * 100).toFixed(1)}%${players}`;
}

/** "14.0% of chances (80% range 9.0–20.0%), 9 of 61 chances". */
export function colourWeaknessRateText(w: ColorWeakness): string {
  if (!fin(w.rate) || !fin(w.low) || !fin(w.high)) return "Not measured";
  const p = (x: number) => (x * 100).toFixed(1);
  return `${p(w.rate)}% of chances (80% range ${p(w.low)}–${p(w.high)}%), ${int(w.events)} of ${plural(w.opportunities, "chance", "chances")}`;
}
