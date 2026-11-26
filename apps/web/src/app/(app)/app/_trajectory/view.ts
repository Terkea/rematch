import { ALGORITHM_MANIFEST } from "../../../../lib/algorithm";
import type { TimeClass } from "../../../../server/games/types";
import { type GlossaryEntry, WEAKNESS_GLOSSARY } from "../../../../server/profile/glossary";
import type { Estimate } from "../../../../server/stats/estimate";
import type {
  RateEstimate,
  ThenNowRow,
  Trajectory,
  TrajectoryKind,
  TrajectoryWindow,
} from "../../../../server/trajectory/model";
import type {
  CompareCell,
  CompareChange,
  CompareView,
  CompareViewRow,
} from "../_filters/compare-view";
import { STATS_GLOSSARY } from "../_filters/glossary";
import { rangeWords } from "../_filters/model";
import { DASH, diffRange, diffValue, gamesText, statRange, statValue } from "../_filters/stat-text";
import { TRAJECTORY_GLOSSARY } from "./glossary";

// The trajectory's words (F6), built on the server from the read model: the headline, the
// "since then" sentence, the then | now table (the period comparison's block) and the chart's
// points. A change is "clearly better" or "clearly worse" only when its verdict says so (ADR 0083
// §6); a window below the minimum says what it still needs. Never NaN, never an arrow.
//
// The rating's own verdict is computed and not stated while the manifest's
// `profile.trajectory_rating_verdict.shown` is false (EXP-0015, ADR 0116): it told most players
// whose strength had not changed that their rating was falling. The headline then states the
// peak, the current rating and the change as numbers, and the table's rating row shows both
// windows' averages and the difference, with no range and no "Worse" or "Better". Accuracy,
// blunders, score and the weaknesses keep their verdicts.

/** Whether the rating's verdict is stated (Rust `coaching_input::trajectory::RATING_VERDICT_SHOWN`). */
export const RATING_VERDICT_SHOWN: boolean =
  ALGORITHM_MANIFEST.components.profile.trajectory_rating_verdict.shown;

export type TrajectoryViewOptions = {
  /** Defaults to `RATING_VERDICT_SHOWN`; a test passes `true` for the copy a cleared rule gets. */
  ratingVerdict?: boolean;
};

export type TrajectoryChartPoint = { id: string; date: string; rating: number };

export type TrajectoryChart = {
  points: TrajectoryChartPoint[];
  peak: { date: string; rating: number; label: string };
  current: { date: string; rating: number; label: string };
  earlier: { from: string; to: string; label: string; short: string };
  recent: { from: string; to: string; label: string; short: string };
};

export type TrajectoryView =
  | { state: "not_enough_games"; text: string; games: number }
  | {
      state: "ready";
      /** The rating's verdict, or `null` while it is withheld. */
      kind: TrajectoryKind | null;
      timeClass: TimeClass;
      /** The section's "What is this?": it says the rating is not called while it is withheld. */
      explain: GlossaryEntry;
      headline: string;
      since: string;
      /** "Based on 523 rated rapid games." */
      basis: string;
      chart: TrajectoryChart;
      compare: CompareView;
    };

const count = new Intl.NumberFormat("en-GB");
const MINUS = "−";
const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const rating = (r: number) => count.format(Math.round(r));
const signed = (v: number) =>
  v > 0 ? `+${count.format(v)}` : v < 0 ? `${MINUS}${count.format(-v)}` : "±0";
const monthOf = (d: Date) => `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
const day = (d: Date) => d.toISOString().slice(0, 10);
const windowRange = (w: TrajectoryWindow) => rangeWords({ from: day(w.from), to: day(w.to) });

/** "clearly worse", "no clear change", "not enough games to tell". */
function verdictWords(v: string): string {
  switch (v) {
    case "better":
      return "clearly better";
    case "worse":
      return "clearly worse";
    case "no_clear_change":
      return "no clear change";
    default:
      return "not enough games to tell";
  }
}

type Ready = Extract<Trajectory, { kind: TrajectoryKind }>;

/** The peak, the current rating and the change, with no word for a direction (ADR 0116). */
function factHeadline(t: Ready, tc: TimeClass): string {
  const now = rating(t.current.rating);
  const delta = signed(t.change);
  if (t.then.basis === "peak") {
    return `Your ${tc} rating peaked at ${rating(t.peak.rating)} in ${monthOf(t.peak.at)} and is ${now} now (${delta}).`;
  }
  const peak =
    t.peak.gameId !== null && t.peak.rating > t.current.rating
      ? ` Your peak was ${rating(t.peak.rating)} in ${monthOf(t.peak.at)}.`
      : "";
  return `Your ${tc} rating is ${now} now; it averaged ${rating(t.then.meanRating)} over your first ${t.then.games} rated games (${delta}).${peak}`;
}

function headline(t: Ready, tc: TimeClass, shown: boolean): string {
  if (!shown) return factHeadline(t, tc);
  const now = rating(t.current.rating);
  const delta = signed(t.change);
  if (t.then.basis === "peak") {
    const peak = `${rating(t.peak.rating)} in ${monthOf(t.peak.at)}`;
    if (t.kind === "falling") {
      return `Your ${tc} rating peaked at ${peak} and is ${now} now (${delta}).`;
    }
    return `Your ${tc} rating is ${now} now, ${delta} from its peak of ${peak}: ${verdictWords(t.rating.verdict)} since the games around it.`;
  }
  const first = `your first ${t.then.games} rated games`;
  const mean = rating(t.then.meanRating);
  if (t.kind === "rising") {
    const peak =
      t.peak.gameId !== null && t.peak.rating > t.current.rating
        ? ` Your peak was ${rating(t.peak.rating)} in ${monthOf(t.peak.at)}.`
        : "";
    return `Your ${tc} rating is ${now} now, up from an average of ${mean} over ${first} (${delta}).${peak}`;
  }
  if (t.kind === "falling") {
    return `Your ${tc} rating is ${now} now, down from an average of ${mean} over ${first} (${delta}).`;
  }
  return `Your ${tc} rating is ${now} now, about where it was over ${first} (an average of ${mean}): no clear change.`;
}

const pct0 = (v: number) => `${Math.round(v * 100)}%`;

function rateValue(e: RateEstimate): string {
  return e.kind === "estimate" && Number.isFinite(e.value) ? pct0(e.value) : DASH;
}

function rateRange(e: RateEstimate): string {
  if (e.kind !== "estimate") {
    return `Need ${count.format(e.needed)} more ${e.needed === 1 ? "game" : "games"} with the chance`;
  }
  const low = Math.floor(e.interval.low * 100 + 1e-9);
  const high = Math.ceil(e.interval.high * 100 - 1e-9);
  return `80% range ${low}% to ${high}%`;
}

/** One measure's "accuracy 69.1% then, 66.0% now (no clear change)". */
function phrase(r: ThenNowRow): string {
  const name = r.label.charAt(0).toLowerCase() + r.label.slice(1);
  const verdict = verdictWords(r.result.verdict);
  if (r.unit === "rate") {
    if (r.a.kind !== "estimate" || r.b.kind !== "estimate") return `${name} (${verdict})`;
    return `${name} ${rateValue(r.a)} then, ${rateValue(r.b)} now of chances (${verdict})`;
  }
  const a = statValue(r.a, r.unit);
  const b = statValue(r.b, r.unit);
  if (a === DASH || b === DASH) return `${name} (${verdict})`;
  return `${name} ${a} then, ${b} now (${verdict})`;
}

function ratingCell(e: Estimate, games: number, shown: boolean): CompareCell {
  if (e.kind !== "estimate") return { value: DASH, detail: gamesText(games, "rated") };
  // The window's range is the rule's evidence, as narrow as its verdict is wrong (EXP-0015).
  if (!shown) return { value: `${rating(e.value)} on average`, detail: gamesText(games, "rated") };
  const range = e.interval
    ? `80% range ${rating(Math.floor(e.interval.low))} to ${rating(Math.ceil(e.interval.high))}`
    : "Every game had the same rating, so there is no range";
  return {
    value: `${rating(e.value)} on average`,
    detail: `${range}, ${gamesText(games, "rated")}`,
  };
}

const CHANGE_TEXT = {
  better: "Better",
  worse: "Worse",
  no_clear_change: "No clear change",
  not_enough_games: "Not enough games to tell",
} as const;

function change(r: { result: ThenNowRow["result"] }, detail: string | null): CompareChange {
  return {
    verdict: r.result.verdict,
    text: CHANGE_TEXT[r.result.verdict],
    detail,
  };
}

/** The rating row's change: its verdict, or "Not called" with the difference alone. */
function ratingChange(t: Ready, shown: boolean): CompareChange {
  if (t.rating.verdict === "not_enough_games") return change({ result: t.rating }, null);
  const diff = `Difference ${signed(Math.round(t.rating.diff.value))} points`;
  const why = "A rating moves after every game, so this isn't judged on its own.";
  if (!shown) return { verdict: "withheld", text: "Not called", detail: `${diff}. ${why}` };
  return change(
    { result: t.rating },
    `${diff}, 80% range ${signed(Math.floor(t.rating.diff.interval.low))} to ${signed(Math.ceil(t.rating.diff.interval.high))}`,
  );
}

function compareRows(t: Ready, shown: boolean): CompareViewRow[] {
  const rows: CompareViewRow[] = [
    {
      key: "rating",
      label: "Rating",
      explain: shown ? TRAJECTORY_GLOSSARY.rating : TRAJECTORY_GLOSSARY.ratingNotCalled,
      a: ratingCell(t.ratingSides.a, t.then.games, shown),
      b: ratingCell(t.ratingSides.b, t.now.games, shown),
      change: ratingChange(t, shown),
    },
  ];
  for (const r of t.rows) {
    if (r.unit === "rate") {
      const cell = (
        e: RateEstimate,
        c: { events: number; opportunities: number; games: number },
      ) => ({
        value: rateValue(e),
        detail: `${rateRange(e)}, ${count.format(c.events)} of ${count.format(c.opportunities)} chances in ${gamesText(c.games)}`,
      });
      const res = r.result;
      const detail =
        res.verdict === "not_enough_games" || r.a.kind !== "estimate" || r.b.kind !== "estimate"
          ? null
          : `Difference ${signed(Math.round((r.b.value - r.a.value) * 100))} points of chances`;
      rows.push({
        key: r.key,
        label: r.label,
        explain: WEAKNESS_GLOSSARY[r.seriesKey] ?? TRAJECTORY_GLOSSARY.weakness,
        a: cell(r.a, r.counts.a),
        b: cell(r.b, r.counts.b),
        change: change(r, detail),
      });
      continue;
    }
    const basis = r.key === "score" ? "finished" : "analysed";
    const cell = (e: Estimate): CompareCell => ({
      value: statValue(e, r.unit),
      detail: `${statRange(e, r.unit)}, ${gamesText(e.n, basis)}`,
    });
    const res = r.result;
    rows.push({
      key: r.key,
      label: r.label,
      explain: STATS_GLOSSARY[r.key],
      a: cell(r.a),
      b: cell(r.b),
      change: change(
        r,
        res.verdict === "not_enough_games"
          ? null
          : `Difference ${diffValue(res.diff.value, r.unit)}, ${diffRange(res.diff.interval, r.unit)}`,
      ),
    });
  }
  return rows;
}

/** The trajectory's view for one time class (module docs). */
export function trajectoryView(
  t: Trajectory,
  tc: TimeClass,
  { ratingVerdict = RATING_VERDICT_SHOWN }: TrajectoryViewOptions = {},
): TrajectoryView {
  if (t.kind === "not_enough_games") {
    return {
      state: "not_enough_games",
      text: `Your rating trajectory appears after ${t.games + t.needed} rated ${tc} games: ${count.format(t.needed)} more to go.`,
      games: t.games,
    };
  }
  const thenLabel =
    t.then.basis === "peak" ? "Around your peak" : `Your first ${t.then.games} games`;
  const nowLabel = `Your last ${t.now.games} games`;
  const phrases = t.rows.map(phrase);
  const lead = t.then.basis === "peak" ? "Since then" : "Since your first games";
  return {
    state: "ready",
    kind: ratingVerdict ? t.kind : null,
    timeClass: tc,
    explain: ratingVerdict
      ? TRAJECTORY_GLOSSARY.trajectory
      : TRAJECTORY_GLOSSARY.trajectoryNotCalled,
    headline: headline(t, tc, ratingVerdict),
    since: phrases.length > 0 ? `${lead}: ${phrases.join(", ")}.` : "",
    basis: `Based on ${count.format(t.games)} rated ${tc} ${t.games === 1 ? "game" : "games"}.`,
    chart: {
      points: t.series.map((p) => ({ id: p.id, date: p.at.toISOString(), rating: p.rating })),
      peak: {
        date: t.peak.at.toISOString(),
        rating: t.peak.rating,
        label: `Peak ${rating(t.peak.rating)}, ${monthOf(t.peak.at)}`,
      },
      current: {
        date: t.current.at.toISOString(),
        rating: t.current.rating,
        label: `Now ${rating(t.current.rating)}`,
      },
      earlier: {
        from: t.then.from.toISOString(),
        to: t.then.to.toISOString(),
        label: thenLabel,
        short: "Then",
      },
      recent: {
        from: t.now.from.toISOString(),
        to: t.now.to.toISOString(),
        label: nowLabel,
        short: "Now",
      },
    },
    compare: {
      a: {
        label: thenLabel,
        detail: `${gamesText(t.then.games, "rated")}, ${windowRange(t.then)}`,
      },
      b: { label: nowLabel, detail: `${gamesText(t.now.games, "rated")}, ${windowRange(t.now)}` },
      rows: compareRows(t, ratingVerdict),
    },
  };
}
