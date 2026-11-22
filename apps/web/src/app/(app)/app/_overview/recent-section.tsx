import { CaretRightIcon, Icon } from "@ci/ui/icons";
import Link from "next/link";
import type { RecentRow } from "./view";

/**
 * The latest games as a ruled scoresheet table: date, result, opponent with rating, accuracy.
 * Each row opens the game's review: the date is an underlined link whose hit area covers the
 * row, the row tints on hover and rings on focus, and it ends in a chevron. On phones the date
 * drops its year and the opponent wraps, with the rating on its own line, so nothing is cut off.
 */
export function RecentSection({ rows, scope }: { rows: RecentRow[]; scope?: string | null }) {
  if (rows.length === 0) return null;
  return (
    <div className="flex flex-col gap-2">
      {scope && <p className="text-sm text-ink-soft">{scope} games only.</p>}
      <table className="w-full border-collapse text-sm">
        <thead>
          <tr className="border-b border-rule-strong text-left text-ink-soft">
            <th scope="col" className="py-2 pr-3 font-normal">
              Date
            </th>
            <th scope="col" className="py-2 pr-3 font-normal">
              Result
            </th>
            <th scope="col" className="py-2 pr-3 font-normal">
              Opponent
            </th>
            <th scope="col" className="py-2 pr-1 text-right font-normal">
              Accuracy
            </th>
            <th scope="col" className="w-6 py-2">
              <span className="sr-only">Open</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr
              key={r.id}
              className="group relative border-b border-rule align-top hover:bg-well has-[a:focus-visible]:bg-well"
            >
              <td className="py-3 pr-3 whitespace-nowrap">
                <Link
                  href={r.href}
                  aria-label={r.caption}
                  className="font-medium text-ink underline decoration-rule-strong underline-offset-4 group-hover:decoration-ink after:absolute after:inset-0 focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ink"
                >
                  <span className="sm:hidden">{r.dateShort}</span>
                  <span className="hidden sm:inline">{r.date}</span>
                </Link>
              </td>
              <td className="py-3 pr-3 text-ink">{r.result}</td>
              <td className="py-3 pr-3 break-words text-ink-soft">
                {r.opponent}
                {r.rating !== null && (
                  <>
                    {" "}
                    <span className="block text-ink-faint tabular-nums sm:ml-1 sm:inline">
                      {r.rating}
                    </span>
                  </>
                )}
              </td>
              <td className="py-3 pr-1 text-right text-ink">{r.accuracy ?? "No accuracy"}</td>
              <td className="py-3 text-right text-ink-faint group-hover:text-ink">
                <span data-part="row-chevron" aria-hidden="true" className="inline-flex">
                  <Icon icon={CaretRightIcon} size={16} />
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
