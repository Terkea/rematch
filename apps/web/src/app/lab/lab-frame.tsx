import type { ReactNode } from "react";

const variants = ["a", "b", "c"] as const;

export interface LabFrameProps {
  surface: string;
  variant: "a" | "b" | "c";
  title: string;
  rationale: string;
  children: ReactNode;
}

// Wraps one design variant. Everything marked data-lab-chrome is hidden by scripts/shots.ts, so
// screenshots show the variant alone.
export function LabFrame({ surface, variant, title, rationale, children }: LabFrameProps) {
  return (
    <section
      className="min-h-dvh bg-paper text-ink"
      data-lab-surface={surface}
      data-lab-variant={variant}
    >
      <nav
        data-lab-chrome
        aria-label={`${surface} variants`}
        className="flex items-center gap-3 px-(--space-gutter) py-3 text-sm"
      >
        <a href="/lab" className="underline">
          Lab
        </a>
        <span>{surface}</span>
        <ul className="flex gap-2">
          {variants.map((v) => (
            <li key={v}>
              <a
                href={`/lab/${surface}/${v}`}
                aria-current={v === variant ? "page" : undefined}
                className="inline-flex min-h-11 min-w-11 items-center justify-center underline aria-[current=page]:font-semibold"
              >
                {v.toUpperCase()}
              </a>
            </li>
          ))}
        </ul>
      </nav>
      {children}
      <footer data-lab-chrome className="px-(--space-gutter) py-4 text-sm">
        <h2 className="font-semibold">{title}</h2>
        <p>{rationale}</p>
      </footer>
    </section>
  );
}
