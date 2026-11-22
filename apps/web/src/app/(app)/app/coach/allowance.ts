import { dayOf } from "@ci/ui/coach";
import type { ChatAllowance } from "../../../../server/coach/allowance";
import type { CapReason, QuotaWindow } from "../../../../server/coach/copy";

// The chat allowance as the coach pages show it (R6, ADR 0070): what is left in the tightest
// window, when it resets, and whether it is used up. Built on the server from `chatAllowance`,
// so the client only ever gets these few plain values.

export type AllowanceView = {
  left: number;
  limit: number;
  window: QuotaWindow;
  /** The reset instant, ISO. */
  resetsAt: string;
  /** Set when a window is used up: the composer is disabled with its cap copy. */
  exhausted: CapReason | null;
};

/** The window with the fewest messages left (the first on a tie, Rust's order); null with none. */
export function allowanceView(a: ChatAllowance): AllowanceView | null {
  let best: AllowanceView | null = null;
  for (const q of a.quotas) {
    if (!Number.isFinite(q.limit) || q.limit <= 0) continue;
    const used = Number.isFinite(q.used) ? Math.max(0, Math.trunc(q.used)) : 0;
    const left = Math.max(0, Math.trunc(q.limit) - used);
    if (best === null || left < best.left) {
      best = {
        left,
        limit: Math.trunc(q.limit),
        window: q.window,
        resetsAt: q.resetsAt.toISOString(),
        exhausted: null,
      };
    }
  }
  if (best && a.exhausted) {
    best = { ...best, exhausted: a.exhausted.reason, resetsAt: a.exhausted.resetsAt.toISOString() };
  }
  return best;
}

const nf = new Intl.NumberFormat("en-GB");

const PERIOD: Record<QuotaWindow, string> = {
  day: "today",
  week: "this week",
  month: "this month",
};

/**
 * "2 of 3 coach messages left this week, resets Monday" and the reset day ("23 Nov 2026", UTC),
 * shown after it in a `<time>`. Day windows reset "tomorrow", month windows on the date.
 */
export function allowanceText(v: AllowanceView): {
  text: string;
  day: { iso: string; text: string } | null;
} {
  const day = dayOf(v.resetsAt);
  const unit = v.limit === 1 ? "coach message" : "coach messages";
  const when =
    v.window === "week"
      ? "Monday"
      : v.window === "day"
        ? "tomorrow"
        : day
          ? "on"
          : "at the start of next month";
  return {
    text: `${nf.format(v.left)} of ${nf.format(v.limit)} ${unit} left ${PERIOD[v.window]}, resets ${when}`,
    day,
  };
}
