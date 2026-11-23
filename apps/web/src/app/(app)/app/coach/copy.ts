import type { ChatAnchor } from "../../../../server/coach/anchor";
import { type CapReason, failureCopy } from "../../../../server/coach/copy";

// The chat pages' own copy (Task 9). The disclosure note is Rust's `AI_DISCLOSURE_NOTE` byte for
// byte (pinned by test/pages/coach.test.tsx); everything else is web copy.

/** Rust `ai::output::disclosure::AI_DISCLOSURE_NOTE`. */
export const AI_DISCLOSURE_NOTE =
  "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.";

export const EMPTY_THREADS =
  "Ask about any of your games: a move, an opening or a weakness. We'll go through it together.";

/** Under "Coach not available right now" (bug hunt I5): what still works, and what to expect. */
export const COACH_UNAVAILABLE_BODY =
  "Questions can't be answered at the moment. Your past conversations stay readable, and you can ask again once your coach is back.";

/** One line even on a phone, so the empty box stays one line high (review V3). */
export const COMPOSER_PLACEHOLDER = "Ask about your games";

export const ANSWERING = "Your coach is answering";
export const SENDING = "Sending your question";
/** The one line for an answer nothing has streamed for yet (review V1). */
export const READING = "Reading your games";
/** The status under the box while an answer runs (a polite live region; focus waits there). */
export const WRITING = "Your coach is writing";

/**
 * The composer on a phone (review V3): the box and Send on one row, the box one line high and
 * growing as you type; the reason (allowance used) spans the row. Class names only, applied to
 * the kit's `ChatComposer` form.
 */
export const COMPACT_COMPOSER =
  "max-sm:grid max-sm:grid-cols-[minmax(0,1fr)_auto] max-sm:items-end max-sm:gap-x-2 max-sm:gap-y-1.5 max-sm:[&>p]:col-span-2 max-sm:[&_textarea]:min-h-11 max-sm:[&_textarea]:py-2";

/** While the event stream is not open (M4 `useRealtime`): never "live". */
export const NOT_LIVE = "Updates every few seconds";

/** The kind of context a conversation was opened from, as the thread list names it. */
export function anchorKindText(anchor: ChatAnchor | null): string {
  switch (anchor?.kind) {
    case "report_section":
      return "From your report";
    case "weakness":
      return "About a weakness";
    case "position":
      return "About a position";
    case "move":
      return "About a move";
    case "opening_line":
      return "About an opening";
    case "game":
      return "About a game";
    default:
      return "General question";
  }
}

const CAPS: readonly string[] = [
  "weekly_cap",
  "daily_cap",
  "monthly_cap",
  "conversation_full",
  "too_long",
] satisfies CapReason[];

/**
 * Whether a failed answer is worth asking again: an outage or an unknown error is; a used-up
 * allowance, a full conversation, the spending limit or an unlinked account would only fail the
 * same way.
 */
export function canRetry(reason: string | null): boolean {
  if (reason === null) return true;
  return !CAPS.includes(reason) && reason !== "budget" && reason !== "not_linked";
}

const minutesFrom = (iso: string | undefined, now: number): number | null => {
  const at = iso ? Date.parse(iso) : Number.NaN;
  return Number.isFinite(at) ? Math.max(1, Math.ceil((at - now) / 60_000)) : null;
};

/** What a refused post says, for the chat routes' error codes. */
export function postErrorCopy(code: string, retryAt?: string, now = Date.now()): string {
  switch (code) {
    case "busy":
      return "Your coach is still answering your last question. Ask again once it's done.";
    case "rate_limited": {
      const m = minutesFrom(retryAt, now);
      return m === null
        ? "You've asked a lot in the last few minutes. Wait a little, then ask again."
        : `You've asked a lot in the last few minutes. You can ask again in ${m} ${m === 1 ? "minute" : "minutes"}.`;
    }
    case "too_long":
    case "too_large":
      return failureCopy("too_long");
    case "empty":
      return "Write a question first.";
    case "not_linked":
      return failureCopy("not_linked");
    case "not_verified":
      return "Verify this account to ask your coach. You can do it on the Accounts page.";
    case "not_found":
      return "This conversation no longer exists. Start a new one from the coach page.";
    default:
      return "Your question didn't send. Check your connection and try again.";
  }
}
