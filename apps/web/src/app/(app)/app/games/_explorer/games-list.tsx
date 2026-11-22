import { PAGE_SIZE } from "../../../../../server/games/types";
import { GameRow, GameTableRow, type RowView } from "./game-row";

const NUM = new Intl.NumberFormat("en-GB");

/** "Showing 26–50 of 412 games"; one page reads "Showing 12 games" (or "Showing 1 game"). */
export function showingText({
  total,
  page,
  pageCount,
}: {
  total: number;
  page: number;
  pageCount: number;
}): string {
  if (pageCount <= 1) return `Showing ${NUM.format(total)} ${total === 1 ? "game" : "games"}`;
  const first = (page - 1) * PAGE_SIZE + 1;
  const last = Math.min(total, page * PAGE_SIZE);
  return `Showing ${NUM.format(first)}–${NUM.format(last)} of ${NUM.format(total)} games`;
}

/** Consecutive rows that share a day, in order. */
function byDay(rows: RowView[]): { date: string; rows: RowView[] }[] {
  const out: { date: string; rows: RowView[] }[] = [];
  for (const r of rows) {
    const last = out.at(-1);
    if (last && last.date === r.date) last.rows.push(r);
    else out.push({ date: r.date, rows: [r] });
  }
  return out;
}

const HEAD = "pt-3 pr-4 pb-2 text-sm font-medium text-ink-faint";
const LEGEND = "The square is the colour you played; the figure is your score.";
const PAD_FIRST = "pl-(--space-panel-pad)";

/**
 * The games as a ruled scoresheet on a `sheet`. Sorted by date, each day is named once, in a date
 * cell spanning that day's games (a heading row per day put one over nearly every game, web
 * polish critic I8); under any other sort each row carries its date. Phone rows always carry
 * their date inline. From `md` it is a table whose last two columns form the coach's ruled
 * margin (accuracy, then the red-pen counts); below `md` it is two-line rows. Only one of the two is ever displayed, so
 * assistive tech meets one.
 */
export function GamesList({ rows, grouped }: { rows: RowView[]; grouped: boolean }) {
  const groups = grouped ? byDay(rows) : [{ date: "", rows }];
  return (
    <div data-surface="sheet" className="rounded-sheet bg-sheet py-1">
      <table className="hidden w-full border-collapse text-left md:table">
        <caption className="sr-only">Your games</caption>
        <thead>
          <tr className="border-b border-rule-strong">
            <th scope="col" className={`${HEAD} ${PAD_FIRST}`}>
              Date
            </th>
            <th scope="col" className={HEAD}>
              Result
              <span className="block text-xs font-normal">Square: your colour</span>
            </th>
            <th scope="col" className={HEAD}>
              Opponent
            </th>
            <th scope="col" className={HEAD}>
              Time control
            </th>
            <th scope="col" className={HEAD}>
              Opening
            </th>
            <th scope="col" className={`${HEAD} border-l border-rule-strong pl-4 text-right`}>
              Accuracy
            </th>
            <th
              scope="col"
              className="pt-3 pr-(--space-panel-pad) pb-2 text-right text-sm font-medium text-ink-faint"
            >
              Marks
              <span className="block text-xs font-normal">
                <span className="font-serif text-pen-text">??</span> blunder,{" "}
                <span className="font-serif text-pen-text">?</span> mistake
              </span>
            </th>
          </tr>
        </thead>
        {groups.map((g) => (
          <tbody key={g.date || "all"} className="border-b border-rule last:border-b-0">
            {g.rows.map((r, i) =>
              grouped ? (
                <GameTableRow key={r.id} row={r} withDate daySpan={i === 0 ? g.rows.length : 0} />
              ) : (
                <GameTableRow key={r.id} row={r} withDate />
              ),
            )}
          </tbody>
        ))}
      </table>
      <div className="md:hidden">
        <p className={`pt-2 pr-4 pb-1 ${PAD_FIRST} text-xs text-ink-faint`}>
          {LEGEND}{" "}
          <span data-part="marks-key" className="whitespace-nowrap">
            <span className="font-serif text-pen-text">??</span> blunder,{" "}
            <span className="font-serif text-pen-text">?</span> mistake
          </span>
        </p>
        {/* Each phone row carries its date inline: a heading per day doubled the list (I8). */}
        <ul aria-label="Your games">
          {rows.map((r) => (
            <GameRow key={r.id} row={r} />
          ))}
        </ul>
      </div>
    </div>
  );
}
