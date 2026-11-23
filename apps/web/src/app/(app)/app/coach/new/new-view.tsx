"use client";

import { ChatComposer, ChatMessage, CoachState } from "@ci/ui/coach";
import { Icon, WarningIcon } from "@ci/ui/icons";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { type ReactNode, useEffect, useState } from "react";
import { VERIFY_ACTION, VERIFY_BODY, VERIFY_COACH_TITLE } from "../../../../../lib/verify-copy";
import { type ChatAnchor, colourQuestion } from "../../../../../server/coach/anchor";
import { CHAT_COPY } from "../../../../../server/coach/copy";
import { COACH_UNAVAILABLE_TITLE } from "../../../../../server/coach/status-copy";
import type { TimeClass } from "../../../../../server/games/types";
import { BackLink } from "../[threadId]/thread-view";
import type { AllowanceView } from "../allowance";
import { AllowanceLine } from "../allowance-line";
import {
  COACH_UNAVAILABLE_BODY,
  COMPACT_COMPOSER,
  COMPOSER_PLACEHOLDER,
  postErrorCopy,
  SENDING,
} from "../copy";

// A new conversation (Task 9): the anchor it is opened from, then the composer. Sending creates
// the thread with its first question in one request (T5 `startConversation`) and opens it; until
// the server answers, the question is shown as sending and the composer waits.

/**
 * How long the client navigation to a created thread may take before the page loads the thread
 * itself. The thread and its answer are saved either way; a stalled transition must never leave
 * the question on "Sending" (bug hunt B4).
 */
export const OPEN_FALLBACK_MS = 4000;

export function NewConversation({
  scope,
  anchor,
  allowance,
  listHref,
  available = true,
  verified = true,
  scopeSwitch,
  color = null,
  children,
}: {
  scope: { accountId: string; timeClass: TimeClass };
  anchor: ChatAnchor | null;
  /** Opened from one colour's page: the question is about that colour's games (audit A3). */
  color?: "white" | "black" | null;
  allowance: AllowanceView | null;
  listHref: string;
  /** False while no coach could answer (bug hunt I5): the composer is off and says why. */
  available?: boolean;
  /** False while the user's link is not verified (holistic F10): the composer is off. */
  verified?: boolean;
  scopeSwitch?: ReactNode;
  children?: ReactNode;
}) {
  const router = useRouter();
  const [sending, setSending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** The created thread's page, once the server answered: the question is no longer sending. */
  const [opening, setOpening] = useState<string | null>(null);

  // Still here after the client navigation should have left: open the thread with a full load.
  useEffect(() => {
    if (opening === null) return;
    const timer = setTimeout(() => window.location.assign(opening), OPEN_FALLBACK_MS);
    return () => clearTimeout(timer);
  }, [opening]);

  async function start(question: string): Promise<boolean> {
    setError(null);
    setSending(question);
    try {
      const res = await fetch("/api/coach/threads", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ...scope, anchor, question, ...(color ? { color } : {}) }),
      });
      const data = (await res.json().catch(() => null)) as {
        id?: string;
        error?: string;
        retryAt?: string;
      } | null;
      if (!res.ok || typeof data?.id !== "string") {
        setError(postErrorCopy(data?.error ?? "unknown", data?.retryAt));
        setSending(null);
        return false;
      }
      // The question stays shown, with a way to the thread, while it opens.
      const href = `/app/coach/${encodeURIComponent(data.id)}`;
      setOpening(href);
      router.push(href);
      return true;
    } catch {
      setError(postErrorCopy("network"));
      setSending(null);
      return false;
    }
  }

  const exhausted = allowance?.exhausted ?? null;
  const reason = !verified
    ? VERIFY_COACH_TITLE
    : !available
      ? COACH_UNAVAILABLE_TITLE
      : exhausted
        ? CHAT_COPY.capText(exhausted)
        : sending !== null
          ? SENDING
          : null;
  // `role` through a variable: Biome reads a literal role="user" as an ARIA role (T3).
  const role = "user" as const;

  return (
    <div className="flex min-w-0 flex-col gap-6">
      <header className="flex flex-col gap-2">
        <BackLink href={listHref} />
        <h1 className="m-0 font-serif text-2xl font-semibold text-ink md:text-3xl">
          New conversation
        </h1>
        <p className="m-0 max-w-prose text-lg text-ink-soft">
          Your coach answers from your analysed games and checks moves with the engine.
        </p>
      </header>
      {scopeSwitch}
      {color ? (
        <p data-slot="ask-colour" className="m-0 max-w-prose text-base text-ink-soft">
          About your games with {color === "white" ? "White" : "Black"}: your question is sent with
          that colour, so your coach answers from those games.
        </p>
      ) : null}
      {!verified ? (
        <CoachState
          kind="computed_only"
          titleAs="h2"
          title={VERIFY_COACH_TITLE}
          body={VERIFY_BODY}
          action={VERIFY_ACTION}
        />
      ) : available ? null : (
        <CoachState
          kind="unavailable"
          titleAs="h2"
          title={COACH_UNAVAILABLE_TITLE}
          body={COACH_UNAVAILABLE_BODY}
        />
      )}
      {children}
      {sending !== null ? (
        <div className="flex flex-col gap-1.5" aria-busy="true">
          <ChatMessage
            {...{ role }}
            status="done"
            segments={[{ kind: "text", text: colourQuestion(sending, color) }]}
            streamed={[]}
            notes={[]}
            engineChecks={[]}
            failure={null}
            disclosure={null}
            className="[&_p]:whitespace-pre-line"
          />
          {opening === null ? (
            <p className="m-0 font-sans text-sm text-ink-faint">Sending</p>
          ) : (
            <p className="m-0 font-sans text-sm text-ink-soft">
              Sent.{" "}
              <Link
                href={opening}
                className="rounded-field text-ink underline decoration-rule-strong underline-offset-4 hover:decoration-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
              >
                Open your conversation
              </Link>
            </p>
          )}
        </div>
      ) : null}
      <div className="flex flex-col gap-2">
        {error ? (
          <p
            role="alert"
            className="m-0 flex max-w-prose items-start gap-2 font-sans text-sm text-ink"
          >
            <Icon icon={WarningIcon} size={16} className="mt-0.5 shrink-0" />
            <span>{error}</span>
          </p>
        ) : null}
        <ChatComposer
          onSubmit={start}
          placeholder={COMPOSER_PLACEHOLDER}
          disabled={reason !== null}
          disabledReason={exhausted || !available || !verified ? reason : null}
          className={COMPACT_COMPOSER}
        />
        {/* No allowance while nothing could answer; no AI disclosure before any AI text (M3). */}
        {available && verified ? <AllowanceLine allowance={allowance} /> : null}
      </div>
    </div>
  );
}
