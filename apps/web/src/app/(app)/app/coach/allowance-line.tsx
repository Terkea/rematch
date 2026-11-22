import { cn } from "@ci/ui/lib";
import { type AllowanceView, allowanceText } from "./allowance";

/** "2 of 3 coach messages left this week, resets Monday (23 Nov 2026)". Nothing without a plan. */
export function AllowanceLine({
  allowance,
  className,
}: {
  allowance: AllowanceView | null;
  className?: string;
}) {
  if (!allowance) return null;
  const { text, day } = allowanceText(allowance);
  return (
    <p
      data-allowance={allowance.exhausted ? "exhausted" : "open"}
      className={cn("m-0 font-sans text-sm text-ink-soft", className)}
    >
      {text}
      {day ? (
        <>
          {" ("}
          <time dateTime={day.iso}>{day.text}</time>
          {")"}
        </>
      ) : null}
    </p>
  );
}
