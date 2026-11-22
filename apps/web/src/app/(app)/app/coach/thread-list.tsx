import { dayText } from "@ci/ui/charts";
import { cn } from "@ci/ui/lib";
import Link from "next/link";
import type { ConversationSummary } from "../../../../server/coach/threads";
import { anchorKindText } from "./copy";

// The user's conversations on one account, most recently active first (Task 9). A ruled list on
// a sheet, like the games scoresheet: the title in Literata, then what it was opened from and the
// day it was last active. Each row is one link with a 44px minimum height.

const isoDay = (d: Date) => d.toISOString().slice(0, 10);

function lastActive(value: Date | string): { iso: string; text: string } | null {
  const d = value instanceof Date ? value : new Date(value);
  return Number.isFinite(d.getTime()) ? { iso: isoDay(d), text: dayText(d) } : null;
}

export function ThreadList({
  threads,
  currentId,
  compact = false,
}: {
  threads: ConversationSummary[];
  currentId?: string;
  /** The thread page's side rail: smaller titles, no sheet padding. */
  compact?: boolean;
}) {
  return (
    <ol aria-label="Your conversations" className="m-0 flex list-none flex-col p-0">
      {threads.map((t) => {
        const day = lastActive(t.lastMessageAt);
        const current = t.id === currentId;
        return (
          <li key={t.id} className="border-b border-rule last:border-b-0">
            <Link
              href={`/app/coach/${t.id}`}
              aria-current={current ? "page" : undefined}
              className={cn(
                "flex min-h-11 flex-col gap-1 py-3 text-left",
                compact ? "px-3" : "px-(--space-panel-pad) md:flex-row md:items-baseline md:gap-6",
                "transition-colors duration-(--dur-quick) ease-out hover:bg-(--surface-hover)",
                "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ink",
                current && "bg-well",
              )}
            >
              <span
                className={cn(
                  "min-w-0 font-serif text-ink [overflow-wrap:anywhere]",
                  compact ? "text-base" : "text-lg md:flex-1",
                  current && "font-semibold",
                )}
              >
                {t.title}
              </span>
              <span className="flex shrink-0 flex-wrap gap-x-3 font-sans text-sm text-ink-soft">
                <span>{anchorKindText(t.anchor)}</span>
                {day ? (
                  <time dateTime={day.iso} className="text-ink-faint">
                    {day.text}
                  </time>
                ) : null}
              </span>
            </Link>
          </li>
        );
      })}
    </ol>
  );
}
