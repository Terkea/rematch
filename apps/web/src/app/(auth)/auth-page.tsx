import type { ReactNode } from "react";
import { OperaDiagram } from "./opera-diagram";

export interface AuthPageProps {
  /** The page title (h1). Omit it when the children carry their own `#auth-title` heading. */
  title?: string;
  /** One sentence under the title. */
  lede?: ReactNode;
  /** Show the annotated diagram beside the form from `lg` (sign-in and the age screen). */
  diagram?: boolean;
  children?: ReactNode;
}

/**
 * The signed-out page: a quiet form column on `paper`, and from `lg` the board doing its job on a
 * real game in a `sheet` beside it (DESIGN.md: boldness lives on the board, the forms stay quiet).
 */
export function AuthPage({ title, lede, diagram = false, children }: AuthPageProps) {
  return (
    <div
      className={diagram ? "grid flex-1 lg:grid-cols-[minmax(0,5fr)_minmax(0,7fr)]" : "flex flex-1"}
    >
      <section
        aria-labelledby="auth-title"
        className="flex flex-1 flex-col px-(--space-gutter) pt-14 pb-12 sm:pt-24 lg:justify-center lg:py-12"
      >
        <div className="mx-auto flex w-full max-w-sm flex-col gap-8">
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
      {diagram ? (
        <aside
          aria-label="An annotated position"
          className="hidden items-center justify-center bg-sheet px-(--space-gutter) py-12 lg:flex"
          data-surface="sheet"
        >
          <OperaDiagram />
        </aside>
      ) : null}
    </div>
  );
}
