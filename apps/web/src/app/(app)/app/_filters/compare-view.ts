import type { Period } from "../../../../server/filters/presets";
import type { Called, Pair } from "../../../../server/stats/compare";
import type {
  CompareKey,
  DaySpan,
  PeriodComparison,
} from "../../../../server/stats/period-compare";
import { verdictText } from "../../../../server/stats/period-compare";
import { type GlossaryEntry, STATS_GLOSSARY } from "./glossary";
import { rangeWords } from "./model";
import { diffRange, diffValue, gamesText, type StatUnit, statRange, statValue } from "./stat-text";

// The A | B | change block's view (T3): plain strings, built on the server. A change is named only
// by its verdict (ADR 0083 §6); its difference is shown beside a called or "no clear change"
// verdict, never below the minimum.

export type CompareCell = { value: string; detail: string };
export type CompareChange = {
  verdict: Called | "not_enough_games";
  text: string;
  detail: string | null;
};
export type CompareViewRow = {
  key: CompareKey | (string & {});
  label: string;
  explain: GlossaryEntry;
  a: CompareCell;
  b: CompareCell;
  change: CompareChange;
};
export type CompareView = {
  a: { label: string; detail: string };
  b: { label: string; detail: string };
  rows: CompareViewRow[];
};

const UNIT: Record<CompareKey, StatUnit> = {
  accuracy: "accuracy",
  blunders: "perGame",
  score: "score",
};
const LABEL: Record<CompareKey, string> = {
  accuracy: "Accuracy",
  blunders: "Blunders per game",
  score: "Score",
};
const BASIS: Record<CompareKey, string> = {
  accuracy: "analysed",
  blunders: "analysed",
  score: "finished",
};

function periodDetail(p: Period, span: DaySpan | null, side: "a" | "b", n: number): string {
  const games = gamesText(n);
  if (p.kind === "days") return `${rangeWords(p)}, ${games}`;
  const head = side === "b" ? `The last ${p.take} games` : `The ${p.take} games before`;
  return span ? `${head}, ${rangeWords(span)}, ${games}` : `${head}, ${games}`;
}

export function compareView(input: {
  periods: Pair<Period>;
  spans: Pair<DaySpan | null>;
  comparison: PeriodComparison;
}): CompareView {
  const { periods, spans, comparison: c } = input;
  return {
    a: { label: "Period A", detail: periodDetail(periods.a, spans.a, "a", c.n.a) },
    b: { label: "Period B", detail: periodDetail(periods.b, spans.b, "b", c.n.b) },
    rows: c.rows.map((r) => {
      const unit = UNIT[r.key];
      const cell = (e: typeof r.a): CompareCell => ({
        value: statValue(e, unit),
        detail: `${statRange(e, unit)}, ${gamesText(e.n, BASIS[r.key])}`,
      });
      const res = r.result;
      const detail =
        res.verdict === "not_enough_games"
          ? null
          : `Difference ${diffValue(res.diff.value, unit)}, ${diffRange(res.diff.interval, unit)}`;
      return {
        key: r.key,
        label: LABEL[r.key],
        explain: STATS_GLOSSARY[r.key],
        a: cell(r.a),
        b: cell(r.b),
        change: { verdict: res.verdict, text: verdictText(res), detail },
      };
    }),
  };
}
