"use client";

import { AiLabel, ChatComposer, ChatMessage } from "@ci/ui/coach";
import { Icon, WarningIcon } from "@ci/ui/icons";
import { useRouter } from "next/navigation";
import { type ReactNode, useState } from "react";
import type { ChatAnchor } from "../../../../../server/coach/anchor";
import { CHAT_COPY } from "../../../../../server/coach/copy";
import type { TimeClass } from "../../../../../server/games/types";
import { BackLink } from "../[threadId]/thread-view";
import type { AllowanceView } from "../allowance";
import { AllowanceLine } from "../allowance-line";
import {
  AI_DISCLOSURE_NOTE,
  COMPACT_COMPOSER,
  COMPOSER_PLACEHOLDER,
  postErrorCopy,
  SENDING,
} from "../copy";

// A new conversation (Task 9): the anchor it is opened from, then the composer. Sending creates
// the thread with its first question in one request (T5 `startConversation`) and opens it; until
// the server answers, the question is shown as sending and the composer waits.

export function NewConversation({
  scope,
  anchor,
  allowance,
  listHref,
  scopeSwitch,
  children,
}: {
  scope: { accountId: string; timeClass: TimeClass };
  anchor: ChatAnchor | null;
  allowance: AllowanceView | null;
  listHref: string;
  scopeSwitch?: ReactNode;
  children?: ReactNode;
}) {
  const router = useRouter();
  const [sending, setSending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function start(question: string): Promise<boolean> {
    setError(null);
    setSending(question);
    try {
      const res = await fetch("/api/coach/threads", {
        method: "POST",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ...scope, anchor, question }),
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
      // The question stays shown as sending while the thread opens.
      router.push(`/app/coach/${data.id}`);
      return true;
    } catch {
      setError(postErrorCopy("network"));
      setSending(null);
      return false;
    }
  }

  const exhausted = allowance?.exhausted ?? null;
  const reason = exhausted ? CHAT_COPY.capText(exhausted) : sending !== null ? SENDING : null;
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
      {children}
      {sending !== null ? (
        <div className="flex flex-col gap-1.5" aria-busy="true">
          <ChatMessage
            {...{ role }}
            status="done"
            segments={[{ kind: "text", text: sending }]}
            streamed={[]}
            notes={[]}
            engineChecks={[]}
            failure={null}
            disclosure={null}
            className="[&_p]:whitespace-pre-line"
          />
          <p className="m-0 font-sans text-sm text-ink-faint">Sending</p>
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
          disabledReason={exhausted ? reason : null}
          className={COMPACT_COMPOSER}
        />
        <AllowanceLine allowance={allowance} />
      </div>
      <div className="flex max-w-prose items-start gap-3">
        <span className="shrink-0 whitespace-nowrap">
          <AiLabel />
        </span>
        <p className="m-0 font-sans text-sm text-ink-faint">{AI_DISCLOSURE_NOTE}</p>
      </div>
    </div>
  );
}
