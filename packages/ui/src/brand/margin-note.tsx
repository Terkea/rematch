import { cn } from "../lib/cn.js";
import { CrayonStroke } from "./crayon-stroke.js";

export interface MarginNoteProps {
  /** The note itself: one short line, eight words at most. */
  children: string;
  /**
   * The words a screen reader hears, when the note says something the page doesn't. Without it the
   * note repeats visible text and is hidden from assistive tech.
   */
  label?: string;
  /** Underlines the note in crayon: a drill solved or a milestone only. */
  celebrate?: boolean;
  /** `landing` sets it one step larger, for the landing page. */
  size?: "note" | "landing";
  className?: string;
}

/**
 * Dad's margin note ("same knight again", "there you go"), in his handwriting. Only in empty states,
 * the landing page, a drill solved and milestones (DESIGN.md, "Hand-made touches"); never red.
 */
export function MarginNote({
  children,
  label,
  celebrate = false,
  size = "note",
  className,
}: MarginNoteProps) {
  const named = label !== undefined && label !== "";
  const box = cn("m-0 inline-flex w-fit origin-left -rotate-1 flex-col", className);
  const body = (
    <>
      <span
        aria-hidden={named ? true : undefined}
        className={cn(
          "font-hand font-semibold text-ink-soft",
          size === "landing" ? "text-3xl sm:text-4xl" : "text-2xl sm:text-3xl",
        )}
      >
        {children}
      </span>
      {celebrate ? <CrayonStroke seed={children} className="-mt-1" /> : null}
    </>
  );
  // A named note is read by its label; an unnamed one repeats visible text and stays silent.
  return named ? (
    <p data-margin-note role="note" aria-label={label} className={box}>
      {body}
    </p>
  ) : (
    <p data-margin-note aria-hidden="true" className={box}>
      {body}
    </p>
  );
}
