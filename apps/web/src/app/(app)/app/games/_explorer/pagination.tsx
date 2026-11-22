import { CaretLeftIcon, CaretRightIcon, Icon } from "@ci/ui/icons";
import Link from "next/link";
import type { GameListQuery } from "../../../../../server/games/types";
import { explorerHref } from "./href";

const step =
  "relative inline-flex h-10 items-center gap-1.5 rounded-field px-2 font-sans text-base font-medium whitespace-nowrap sm:px-3";
const live = `${step} text-ink hover:bg-(--surface-hover) focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink pointer-coarse:after:absolute pointer-coarse:after:inset-x-0 pointer-coarse:after:-inset-y-0.5`;
const dead = `${step} text-ink-faint`;

/**
 * "Previous page", "Page 2 of 17" and "Next page", as links that keep the filters and the sort.
 * At either end the missing step is plain text (not a link); a single page renders nothing.
 */
export function Pagination({
  query,
  page,
  pageCount,
}: {
  query: GameListQuery;
  page: number;
  pageCount: number;
}) {
  if (pageCount <= 1) return null;
  const href = (p: number) => explorerHref({ ...query, page: p });
  return (
    <nav
      aria-label="Pages"
      className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 md:justify-end"
    >
      {page > 1 ? (
        <Link href={href(page - 1)} className={live}>
          <Icon icon={CaretLeftIcon} size={16} />
          Previous page
        </Link>
      ) : (
        <span aria-hidden="true" className={dead}>
          <Icon icon={CaretLeftIcon} size={16} />
          Previous page
        </span>
      )}
      <span className="order-first w-full text-center text-sm whitespace-nowrap text-ink-soft tabular-nums sm:order-none sm:w-auto sm:px-2">
        {`Page ${page} of ${pageCount}`}
      </span>
      {page < pageCount ? (
        <Link href={href(page + 1)} className={live}>
          Next page
          <Icon icon={CaretRightIcon} size={16} />
        </Link>
      ) : (
        <span aria-hidden="true" className={dead}>
          Next page
          <Icon icon={CaretRightIcon} size={16} />
        </span>
      )}
    </nav>
  );
}
