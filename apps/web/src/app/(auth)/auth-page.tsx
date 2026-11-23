import type { ReactNode } from "react";
import { type DiagramKind, OperaDiagram } from "./opera-diagram";

/**
 * The signed-out grid and its form column, shared with the layout's wordmark so "Rematch" and the
 * page title start on one left edge at every width.
 */
export const AUTH_GRID = "lg:grid lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]";
export const AUTH_COLUMN = "mx-auto w-full max-w-sm";

export interface AuthPageProps {
  /** The page title (h1). Omit it when the children carry their own `#auth-title` heading. */
  title?: string;
  /** One sentence under the title. */
  lede?: ReactNode;
  /** The board beside the form from `lg`, and under it on smaller screens. */
  aside?: DiagramKind;
  children?: ReactNode;
}

/**
 * The signed-out page: a quiet form column on `paper`, and the board doing its job in a `sheet`
 * (DESIGN.md: boldness lives on the board, the forms stay quiet). From `lg` the sheet sits beside
 * the form; below it, it follows the form, so a phone shows the product too and not blank paper.
 */
export function AuthPage({ title, lede, aside, children }: AuthPageProps) {
  return (
    <div
      {...(aside ? { "data-bleed": "" } : {})}
      className={aside ? `flex flex-1 flex-col ${AUTH_GRID}` : "flex flex-1 flex-col"}
    >
      <section
        aria-labelledby="auth-title"
        className="flex flex-col px-(--space-gutter) pt-14 pb-12 sm:pt-24 lg:flex-1 lg:justify-center lg:py-12"
      >
        <div className={`${AUTH_COLUMN} flex flex-col gap-8`}>
          {title ? (
            <header className="flex flex-col gap-3">
              <h1 id="auth-title" className="font-serif text-3xl font-semibold text-ink">
                {title}
              </h1>
              {lede ? <div className="text-lg text-ink-soft">{lede}</div> : null}
            </header>
          ) : null}
          {children}
        </div>
      </section>
      {aside ? (
        <aside
          aria-label={aside === "mistake" ? "An annotated position" : "A position"}
          className="flex flex-1 items-center justify-center bg-sheet px-(--space-gutter) py-10 lg:py-12"
          data-surface="sheet"
        >
          <OperaDiagram kind={aside} />
        </aside>
      ) : null}
    </div>
  );
}
