import { type ExplainEntry, ExplainPopover } from "@ci/ui/profile";
import type { CSSProperties, ReactNode } from "react";

/**
 * A section's ruled head: a `rule-strong` line, then the heading. On a first view the line draws
 * on `scaleX` from its left end, and the heading appears when it reaches 60% (globals.css,
 * DESIGN.md choreography 2). `order` is the section's place in the first viewport; a section
 * below the fold passes none and is simply there.
 */
export function SectionHead({
  id,
  title,
  order,
  explain,
  level = 2,
  children,
}: {
  id: string;
  title: string;
  order?: number;
  /** 3 for a sub-section of the overview's Details: a quieter `rule` line and an `xl` heading. */
  level?: 2 | 3;
  /** A glossary entry: an "i" beside the title and its one-sentence meaning under it. */
  explain?: ExplainEntry | null;
  children?: ReactNode;
}) {
  const style = order === undefined ? undefined : ({ "--reveal-i": order } as CSSProperties);
  const reveal = order !== undefined;
  return (
    <div className="flex flex-col gap-3" style={style}>
      <span
        aria-hidden="true"
        {...(reveal ? { "data-reveal-rule": "" } : {})}
        className={
          level === 2
            ? "block h-0.5 w-full origin-left bg-rule-strong"
            : "block h-px w-full origin-left bg-rule"
        }
      />
      <div {...(reveal ? { "data-reveal-fig": "" } : {})} className="flex flex-col gap-1">
        <div className="flex items-center gap-1.5">
          {level === 2 ? (
            <h2 id={id} className="font-serif text-2xl font-semibold text-ink">
              {title}
            </h2>
          ) : (
            <h3 id={id} className="m-0 font-serif text-xl font-semibold text-ink">
              {title}
            </h3>
          )}
          {explain ? <ExplainPopover entry={explain} /> : null}
        </div>
        {explain ? (
          <p className="m-0 max-w-prose text-sm text-ink-soft">{explain.meaning}</p>
        ) : null}
        {children}
      </div>
    </div>
  );
}
