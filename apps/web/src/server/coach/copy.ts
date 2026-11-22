// The chat's fixed server copy. Strings mirrored from Rust are byte for byte the ones in
// `backend/crates/ai/src/chat/limits.rs` (pinned by `test/coach/copy-parity.test.ts`); they are
// server text, never model output, and carry no upsell (spec §14).

export type CapReason =
  | "weekly_cap"
  | "daily_cap"
  | "monthly_cap"
  | "conversation_full"
  | "too_long";

export type QuotaWindow = "day" | "week" | "month";

const CAP_TEXT: Record<CapReason, string> = {
  weekly_cap: "You've used this week's coach messages. They reset on Monday.",
  daily_cap: "You've used today's coach messages. They reset tomorrow.",
  monthly_cap: "You've used this month's coach messages. They reset at the start of next month.",
  conversation_full: "This conversation is full. Start a new one to keep going.",
  too_long: "That question is too long. Please shorten it.",
};

const ENGINE_PERIOD: Record<QuotaWindow, [string, string]> = {
  day: ["day", "tomorrow"],
  week: ["week", "on Monday"],
  month: ["month", "at the start of next month"],
};

export const CHAT_COPY = {
  /** `CHAT_FALLBACK_TEXT` */
  fallback:
    "I couldn't check that answer against your games, so I'd rather not guess. Try asking about a specific game, move or position.",
  /** `CHAT_REFUSAL_TEXT` */
  refusal: "I can't help with that one. Ask me about your games, openings or weaknesses.",
  /** `CHAT_OFF_TOPIC_TEXT` */
  offTopic:
    "I can only help with chess and your own games. Ask me about a game, a position, an opening or one of your weaknesses.",
  /** `CHAT_NO_DATA_TEXT` */
  noData: "Your games are still being analysed. Ask me again once your first report is ready.",
  /** `CHAT_UNAVAILABLE_TEXT` */
  unavailable: "Your coach is unavailable right now. Please try again later.",
  /** `CHAT_ENGINE_PENDING_TEXT` */
  enginePending: "Checking that position with the engine, one moment.",
  /** `dropped_note(n)` */
  droppedNote: (n: number) =>
    "I removed {n} statement(s) I couldn't check against your games.".replace("{n}", String(n)),
  /** `engine_limit_note(window)` */
  engineLimitNote: (window: QuotaWindow) => {
    const [period, reset] = ENGINE_PERIOD[window];
    return "You've used all your engine checks for this {period}. They reset {reset}."
      .replace("{period}", period)
      .replace("{reset}", reset);
  },
  /** `chat_cap_text(reason)` */
  capText: (reason: CapReason) => CAP_TEXT[reason],
} as const;

const isCapReason = (r: string): r is CapReason => Object.hasOwn(CAP_TEXT, r);

/**
 * What the player reads for a failed answer (`coach_messages.failure_reason` or a
 * `chat.message.failed` reason). Web-only copy for the reasons Rust has no player text for.
 */
export function failureCopy(reason: string): string {
  if (isCapReason(reason)) return CAP_TEXT[reason];
  switch (reason) {
    case "budget":
      return "Your coach has reached this month's spending limit. It will be back next month.";
    case "not_linked":
      return "This account is no longer linked to you, so the coach can't answer here.";
    case "lost":
    case "unavailable":
      return CHAT_COPY.unavailable;
    default:
      return "Your coach couldn't answer that one. Please try again.";
  }
}
