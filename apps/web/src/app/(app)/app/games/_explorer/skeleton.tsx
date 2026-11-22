const WIDTHS = [64, 48, 72, 56, 40, 68, 52, 60] as const;

/**
 * The list's ruled shape in pencil while the first page loads: static wells on the scoresheet's
 * rules, never a spinner (B18). The line above it says what is happening.
 */
export function GamesSkeleton() {
  return (
    <div role="status" className="flex flex-col gap-4">
      <p className="font-serif text-base text-ink-soft">Fetching your games</p>
      <ul aria-hidden="true" className="rounded-sheet bg-sheet py-1">
        {WIDTHS.map((w) => (
          <li
            key={w}
            className="flex h-14 items-center gap-4 border-b border-rule px-(--space-panel-pad) last:border-b-0"
          >
            <span className="size-2.5 shrink-0 rounded-board bg-well" />
            <span className="h-2.5 rounded-pill bg-well" style={{ width: `${w}%` }} />
            <span className="ml-auto h-3 w-12 shrink-0 rounded-pill bg-well" />
          </li>
        ))}
      </ul>
    </div>
  );
}
