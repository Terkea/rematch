import type { ReactNode } from "react";

/**
 * The scope switch's row (review V4, T6's treatment on the report page): its labels can be wider
 * than a phone, so it scrolls inside its own row, and below 640px the row fades out at its right
 * edge so a cut label reads as "more this way".
 */
export function ScopeRow({ children }: { children: ReactNode }) {
  return (
    // ban-lint-allow TOK2: a mask gradient is alpha only, not a colour
    <div className="-mx-1 max-w-full min-w-0 overflow-x-auto px-1 py-1 max-sm:pr-8 max-sm:[mask-image:linear-gradient(to_right,black_calc(100%_-_2.5rem),transparent)]">
      {children}
    </div>
  );
}
