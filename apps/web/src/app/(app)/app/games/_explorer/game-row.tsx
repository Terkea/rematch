import { dayText } from "@ci/ui/charts";
import Link from "next/link";
import type {
  GameRow as GameRowData,
  PlayerResult,
  TimeClass,
} from "../../../../../server/games/types";
import { CLASS_NAME } from "./href";

/**
 * One row as rendered: strings and the few numbers the marks need, nothing else (no account id,
 * no player name, no termination), so the row never carries data it does not show.
 */
export type RowView = {
  id: string;
  href: string;
  /** The whole game in one sentence: the link's accessible name, on phones and in the table. */
  label: string;
  date: string;
  colour: "white" | "black";
  /** The player's score, as a crosstable writes it: 1, ½, 0, or * when unfinished. */
  mark: "1" | "½" | "0" | "*";
  opponent: string;
  opponentRating: string | null;
  result: string;
  timeControl: string;
  opening: string | null;
  analysis:
    | { kind: "accuracy"; accuracy: string; counts: string; blunders: number; mistakes: number }
    | { kind: "status"; text: string };
};

const RESULT: Record<PlayerResult, string> = {
  win: "Win",
  draw: "Draw",
  loss: "Loss",
  unfinished: "Unfinished",
};

const MARK: Record<PlayerResult, RowView["mark"]> = {
  win: "1",
  draw: "½",
  loss: "0",
  unfinished: "*",
};

const COLOUR_WORD = { white: "White", black: "Black" } as const;

const STATUS = {
  pending: "Waiting for analysis",
  analysing: "Waiting for analysis",
  failed: "Couldn't analyse",
  skipped: "Skipped",
} as const;

const FRACTION: Record<number, string> = { 0.25: "¼", 0.5: "½", 0.75: "¾" };

/** Minutes as a player reads them: 10, ½, 1½, or 2.4 for anything odder. */
function minutes(seconds: number): string {
  const m = seconds / 60;
  const whole = Math.floor(m);
  const frac = FRACTION[m - whole];
  if (m === whole) return String(m);
  if (frac) return whole === 0 ? frac : `${whole}${frac}`;
  return String(Math.round(m * 10) / 10);
}

/** "Rapid 10+0", "Bullet ½+1", "Daily, 3 days a move", or just the class when the clock is unknown. */
export function timeControlText(
  timeClass: TimeClass,
  base: number | null,
  increment: number | null,
): string {
  const name = CLASS_NAME[timeClass];
  if (base === null) return name;
  if (timeClass === "daily") {
    const days = Math.max(1, Math.round(base / 86_400));
    return `${name}, ${days} ${days === 1 ? "day" : "days"} a move`;
  }
  return `${name} ${minutes(base)}+${increment ?? 0}`;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

export function toRowView(row: GameRowData): RowView {
  const analysis: RowView["analysis"] =
    row.analysisStatus === "done" && row.accuracy !== null && row.counts !== null
      ? {
          kind: "accuracy",
          accuracy: `${row.accuracy.toFixed(1)}%`,
          counts: `${plural(row.counts.blunders, "blunder", "blunders")}, ${plural(row.counts.mistakes, "mistake", "mistakes")}`,
          blunders: row.counts.blunders,
          mistakes: row.counts.mistakes,
        }
      : {
          kind: "status",
          text: row.analysisStatus === "done" ? "No accuracy recorded" : STATUS[row.analysisStatus],
        };
  const result = RESULT[row.result];
  const rating = row.opponent.rating === null ? null : String(row.opponent.rating);
  const date = dayText(row.playedAt);
  const timeControl = timeControlText(row.timeClass, row.baseSeconds, row.incrementSeconds);
  const opening = row.openingName ?? row.eco ?? null;
  const parts = [
    `${result} v ${row.opponent.name}${rating ? ` (${rating})` : ""}`,
    `you played ${COLOUR_WORD[row.playerColor]}`,
    timeControl,
    date,
    ...(opening ? [opening] : []),
    analysis.kind === "accuracy"
      ? `accuracy ${analysis.accuracy}, ${analysis.counts}`
      : analysis.text,
  ];
  return {
    id: row.id,
    href: `/app/games/${row.id}`,
    label: parts.join(", "),
    date,
    colour: row.playerColor,
    mark: MARK[row.result],
    opponent: row.opponent.name,
    opponentRating: rating,
    result,
    timeControl,
    opening,
    analysis,
  };
}

/**
 * The result as a scoresheet mark: the player's score (1, ½, 0) on a square in the colour they
 * played, as a crosstable notes it. Its words are in the row's link name.
 */
function ResultMark({ row }: { row: RowView }) {
  const white = row.colour === "white";
  return (
    <span
      aria-hidden="true"
      title={white ? "You played White" : "You played Black"}
      className={
        white
          ? "inline-flex size-6 shrink-0 items-center justify-center rounded-board border border-rule-strong bg-piece-light font-serif text-sm font-semibold text-piece-dark tabular-nums"
          : "inline-flex size-6 shrink-0 items-center justify-center rounded-board border border-rule-strong bg-piece-dark font-serif text-sm font-semibold text-piece-light tabular-nums"
      }
    >
      {row.mark}
    </span>
  );
}

function Opponent({ row }: { row: RowView }) {
  return (
    <span className="flex min-w-0 items-baseline gap-2">
      <span className="truncate font-medium text-ink">{row.opponent}</span>
      {row.opponentRating ? (
        <span className="shrink-0 text-ink-faint tabular-nums">{row.opponentRating}</span>
      ) : null}
    </span>
  );
}

/**
 * The judgement counts in the coach's notation: `2??` blunders and `1?` mistakes, the glyphs in
 * the red pen (they mark mistakes, red's one meaning). A zero is faint. The words are in the link
 * name and in `title`.
 */
function Marks({
  blunders,
  mistakes,
  counts,
}: {
  blunders: number;
  mistakes: number;
  counts: string;
}) {
  const mark = (n: number, glyph: string) => (
    <span className={n > 0 ? "text-ink" : "text-ink-faint"}>
      {n}
      <span className={n > 0 ? "tracking-glyph text-pen-text" : "tracking-glyph"}>{glyph}</span>
    </span>
  );
  return (
    <span title={counts} className="inline-flex gap-2.5 font-serif font-semibold tabular-nums">
      {mark(blunders, "??")}
      {mark(mistakes, "?")}
    </span>
  );
}

const rowLink =
  "outline-none after:absolute after:inset-0 after:rounded-field focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ink";

/**
 * A phone's two-line row: the result mark, who you played and how accurately, then the result word
 * and the kind of game, with the red-pen counts in the margin. The date shows here only when the
 * list is not grouped by day. The link's name is the whole game in words.
 */
export function GameRow({ row, withDate = true }: { row: RowView; withDate?: boolean }) {
  const a = row.analysis;
  return (
    <li className="relative border-b border-rule last:border-b-0 hover:bg-(--surface-hover)">
      <Link
        href={row.href}
        aria-label={row.label}
        className={`grid min-h-14 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-3 gap-y-0.5 px-(--space-panel-pad) py-2.5 ${rowLink}`}
      >
        <span className="row-span-2 self-center">
          <ResultMark row={row} />
        </span>
        <span className="min-w-0 text-base">
          <Opponent row={row} />
        </span>
        <span className="text-right">
          {a.kind === "accuracy" ? (
            <span className="font-serif text-lg font-semibold text-ink tabular-nums">
              {a.accuracy}
            </span>
          ) : (
            <span className="text-sm text-ink-faint">{a.text}</span>
          )}
        </span>
        <span className="flex min-w-0 text-sm text-ink-soft">
          <span data-part="facts" className="shrink-0 whitespace-nowrap">
            <span className="font-serif font-semibold text-ink">{row.result}</span>,{" "}
            {row.timeControl}
            {withDate ? `, ${row.date}` : null}
          </span>
          {row.opening ? (
            <span data-part="opening" className="min-w-0 truncate text-ink-faint">
              , {row.opening}
            </span>
          ) : null}
        </span>
        <span className="text-right text-sm">
          {a.kind === "accuracy" ? (
            <Marks blunders={a.blunders} mistakes={a.mistakes} counts={a.counts} />
          ) : null}
        </span>
      </Link>
    </li>
  );
}

/**
 * A table row from `md` up: the result mark and word, the opponent (whose link stretches over the
 * row), the clock, the opening (the quietest cell), then the ruled margin: accuracy and the red-pen
 * counts. With `daySpan` (sorted by date) the day is named once: the day's first row carries a
 * date cell spanning `daySpan` rows, and the others (`daySpan` 0) leave the column to it.
 */
export function GameTableRow({
  row,
  withDate,
  daySpan,
}: {
  row: RowView;
  withDate: boolean;
  daySpan?: number;
}) {
  const a = row.analysis;
  const first = "py-2.5 pr-4 pl-(--space-panel-pad)";
  return (
    <tr className="relative border-b border-rule align-middle last:border-b-0 hover:bg-(--surface-hover)">
      {withDate ? (
        daySpan === undefined ? (
          <td className={`${first} text-sm whitespace-nowrap text-ink-soft tabular-nums`}>
            {row.date}
          </td>
        ) : daySpan > 0 ? (
          // The day, once, spanning its games (sorted by date): no heading row per day.
          <th
            scope="rowgroup"
            rowSpan={daySpan}
            className="pt-3.5 pr-4 pb-2.5 pl-(--space-panel-pad) align-top text-left text-sm font-medium whitespace-nowrap text-ink tabular-nums"
          >
            {row.date}
          </th>
        ) : null
      ) : null}
      <td className={`${withDate ? "py-2.5 pr-4" : first} whitespace-nowrap`}>
        <span className="inline-flex items-center gap-2.5 font-serif text-base font-semibold text-ink">
          <ResultMark row={row} />
          {row.result}
        </span>
      </td>
      <td className="max-w-56 py-2.5 pr-4 text-base">
        <Link href={row.href} aria-label={row.label} className={`block min-w-0 ${rowLink}`}>
          <Opponent row={row} />
        </Link>
      </td>
      <td className="py-2.5 pr-4 text-sm whitespace-nowrap text-ink-soft">{row.timeControl}</td>
      <td className="max-w-60 py-2.5 pr-4 text-sm text-ink-faint">
        <span className="line-clamp-1">{row.opening ?? "Not recorded"}</span>
      </td>
      <td className="border-l border-rule-strong py-2.5 pr-4 pl-4 text-right whitespace-nowrap">
        {a.kind === "accuracy" ? (
          <span className="font-serif text-lg font-semibold text-ink tabular-nums">
            {a.accuracy}
          </span>
        ) : (
          <span className="text-sm text-ink-faint">{a.text}</span>
        )}
      </td>
      <td className="py-2.5 pr-(--space-panel-pad) text-right whitespace-nowrap">
        {a.kind === "accuracy" ? (
          <Marks blunders={a.blunders} mistakes={a.mistakes} counts={a.counts} />
        ) : null}
      </td>
    </tr>
  );
}
