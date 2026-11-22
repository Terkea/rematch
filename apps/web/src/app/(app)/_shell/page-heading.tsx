import type { ReactNode } from "react";

/** A page's title and one-line lede, the same on every app page. */
export function PageHeading({ title, lede }: { title: string; lede?: ReactNode }) {
  return (
    <header className="flex max-w-2xl flex-col gap-2">
      <h1 className="font-serif text-3xl font-semibold text-ink">{title}</h1>
      {lede ? <p className="text-lg text-ink-soft">{lede}</p> : null}
    </header>
  );
}

/** The quiet state of a page that has nothing to show yet: one centred line and one way on. */
export function EmptySheet({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div
      data-surface="sheet"
      className="flex flex-col items-center gap-5 rounded-sheet bg-sheet px-(--space-panel-pad) py-12 text-center"
    >
      <p className="max-w-md text-base text-ink-soft">{children}</p>
      {action}
    </div>
  );
}
