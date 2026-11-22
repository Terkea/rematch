import { cn } from "@ci/ui/lib";
import { type AllowanceView, allowanceText } from "./allowance";

/**
 * "2 of 3 coach messages left this week, resets Monday (23 Nov 2026)"; a month window reads
 * "…, resets on 1 Dec 2026". On a phone the bracketed day is left out while the allowance is
 * open, so the line stays one line; once it is used up the day is always shown. Nothing without
 * a plan.
 */
export function AllowanceLine({
  allowance,
  className,
}: {
  allowance: AllowanceView | null;
  className?: string;
}) {
  if (!allowance) return null;
  const { text, day, bracketed } = allowanceText(allowance);
  const time = day ? <time dateTime={day.iso}>{day.text}</time> : null;
  return (
    <p
      data-allowance={allowance.exhausted ? "exhausted" : "open"}
      className={cn("m-0 font-sans text-sm text-ink-soft", className)}
    >
      {text}
      {time && bracketed ? (
        <span className={allowance.exhausted ? undefined : "max-sm:hidden"}>
          {" ("}
          {time}
          {")"}
        </span>
      ) : null}
      {time && !bracketed ? <> {time}</> : null}
    </p>
  );
}
