import { cn } from "@ci/ui/lib";
import Link from "next/link";
import type { SwitchOption } from "./view";

/**
 * All and each time control with analysed games, as plain links (`?tc=`): the filter is in the
 * URL, so it survives a reload and needs no client code. The current one has `aria-current`.
 */
export function TimeClassSwitch({ options }: { options: SwitchOption[] }) {
  if (options.length === 0) return null;
  return (
    <nav aria-labelledby="tc-label" className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <span id="tc-label" className="text-sm text-ink-soft">
        Time control
      </span>
      <ul className="m-0 flex list-none flex-wrap gap-1 p-0">
        {options.map((o) => (
          <li key={o.href}>
            <Link
              href={o.href}
              scroll={false}
              {...(o.current ? { "aria-current": "page" as const } : {})}
              className={cn(
                "inline-flex min-h-9 items-center gap-1.5 rounded-field px-3 text-sm",
                "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
                "pointer-coarse:min-h-11",
                o.current
                  ? "bg-ink font-medium text-on-ink"
                  : "text-ink-soft hover:bg-well hover:text-ink",
              )}
            >
              {o.label} <span className="tabular-nums">{o.count}</span>
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
