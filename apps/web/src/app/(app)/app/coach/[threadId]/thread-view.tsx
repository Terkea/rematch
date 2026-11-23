"use client";

import {
  ChatComposer,
  ChatMessage,
  type CitedPosition,
  CoachState,
  type Segment,
} from "@ci/ui/coach";
import { CaretLeftIcon, Icon, TrashIcon, WarningIcon } from "@ci/ui/icons";
import { motion, spring, useReducedMotion } from "@ci/ui/motion";
import { toast } from "@ci/ui/overlays";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@ci/ui/primitives";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { fetchPositionLine } from "../../../../../lib/position-line";
import { reviewHref } from "../../../../../lib/segment-href";
import { VERIFY_ACTION, VERIFY_BODY, VERIFY_COACH_TITLE } from "../../../../../lib/verify-copy";
import { COACH_UNAVAILABLE_TITLE } from "../../../../../server/coach/status-copy";
import { PendingButton } from "../../../../ui/pending-button";
import type { AllowanceView } from "../allowance";
import { AllowanceLine } from "../allowance-line";
import { COMPACT_COMPOSER, COMPOSER_PLACEHOLDER, READING, WRITING } from "../copy";
import { type MessageView, type ThreadData, useThread } from "./use-thread";

// One conversation (Task 9): the messages in order, then the composer pinned to the foot of the
// viewport, so a streaming answer grows above it and never moves it. Answers are only ever
// rendered from stored or streamed segments (never HTML), and each carries its AI label.

const CAPS = new Set(["weekly_cap", "daily_cap", "monthly_cap", "conversation_full", "too_long"]);

const backLink =
  "inline-flex min-h-11 items-center gap-1 self-start font-sans text-sm text-ink-soft hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink rounded-field";

export function BackLink({ href }: { href: string }) {
  return (
    <Link href={href} className={backLink}>
      <Icon icon={CaretLeftIcon} size={16} />
      All conversations
    </Link>
  );
}

/**
 * An answer nothing has streamed for yet: one calm line at answer scale, with the dashed pencil
 * ring the kit uses for work in progress (review V1). No skeleton, no second wording.
 */
function PendingAnswer() {
  return (
    <article aria-label="Coach's answer" data-role="assistant" data-status="pending">
      <p className="m-0 flex items-center gap-2 font-serif text-prose text-ink-soft">
        <svg
          viewBox="0 0 16 16"
          aria-hidden="true"
          focusable="false"
          className="size-4 shrink-0 fill-none stroke-pencil"
        >
          <ellipse
            cx="8"
            cy="8"
            rx="6.2"
            ry="5.4"
            transform="rotate(-10 8 8)"
            strokeWidth="1.5"
            strokeDasharray="3 2.2"
            strokeLinecap="round"
          />
        </svg>
        {READING}
      </p>
    </article>
  );
}

/** A failed answer: the allowance and the spending limit wait, an outage can be asked again. */
function FailedAnswer({
  m,
  onRetry,
  retryDisabled,
}: {
  m: MessageView;
  onRetry: (q: string) => void;
  retryDisabled: boolean;
}) {
  const reason = m.failureReason ?? "";
  const failure = m.failure ?? "";
  const retry =
    m.retry !== null ? { label: "Ask again", onClick: () => m.retry && onRetry(m.retry) } : null;
  let body: ReactNode;
  if (reason === "unloaded") {
    // The answer finished on the server but could not be read here (review I2).
    body = (
      <CoachState
        kind="failed"
        title={failure}
        titleAs="p"
        action={{ label: "Reload", onClick: () => window.location.reload() }}
      />
    );
  } else if (CAPS.has(reason) || reason === "budget") {
    body = <CoachState kind="budget" title={failure} titleAs="p" />;
  } else if (reason === "unavailable" || reason === "lost" || reason === "not_linked") {
    body = (
      <CoachState
        kind="unavailable"
        title={failure}
        titleAs="p"
        action={retry && !retryDisabled ? retry : null}
      />
    );
  } else {
    body = (
      <div className="flex flex-col items-start gap-3">
        <ChatMessage
          {...{ role: "assistant" as const }}
          status="failed"
          segments={null}
          streamed={[]}
          notes={[]}
          engineChecks={[]}
          failure={failure}
          disclosure={null}
        />
        {retry ? (
          <Button variant="outline" disabled={retryDisabled} onClick={retry.onClick}>
            Ask again
          </Button>
        ) : null}
      </div>
    );
  }
  return (
    <article aria-label="Coach's answer" data-role="assistant" data-status="failed">
      {body}
    </article>
  );
}

function Message({
  m,
  onRetry,
  retryDisabled,
  hrefFor,
  positions,
}: {
  m: MessageView;
  onRetry: (q: string) => void;
  retryDisabled: boolean;
  hrefFor: (s: Segment) => string | null;
  /** The thread's cited positions (Fix D, N5): a done answer shows its boards. */
  positions: Readonly<Record<string, CitedPosition>>;
}) {
  const reduced = useReducedMotion();
  if (m.role === "assistant" && m.status === "failed") {
    return <FailedAnswer m={m} onRetry={onRetry} retryDisabled={retryDisabled} />;
  }
  if (m.role === "assistant" && m.status === "pending" && m.streamed.length === 0) {
    return <PendingAnswer />;
  }
  // `role` goes through a variable: Biome reads a literal role="user" as an ARIA role (T3).
  const role = m.role;
  const message = (
    <ChatMessage
      {...{ role }}
      status={m.status}
      segments={m.segments}
      streamed={m.streamed}
      notes={[]}
      engineChecks={m.engineChecks}
      failure={m.failure}
      disclosure={m.disclosure}
      hrefFor={hrefFor}
      positions={m.role === "assistant" && m.status === "done" ? positions : undefined}
      loadLine={fetchPositionLine}
      // Stored answers keep their paragraph breaks ("\n\n" text segments), and stored prose is
      // set at the streamed size, so a finished answer never reflows (review V2).
      className="[&_p]:whitespace-pre-line [&_p.font-serif]:text-prose"
    />
  );
  if (m.sending) {
    return (
      <div className="flex flex-col gap-1.5" aria-busy="true">
        {message}
        <p className="m-0 font-sans text-sm text-ink-faint">Sending</p>
      </div>
    );
  }
  if (m.settled && m.status === "done" && !reduced) {
    // The finished answer settles on the pen spring (transform and opacity only).
    return (
      <motion.div
        initial={{ opacity: 0.6, y: 4 }}
        animate={{ opacity: 1, y: 0 }}
        transition={spring.pen}
      >
        {message}
      </motion.div>
    );
  }
  return message;
}

function DeleteConversation({ threadId, listHref }: { threadId: string; listHref: string }) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  async function remove() {
    if (pending) return;
    setPending(true);
    setFailed(false);
    try {
      const res = await fetch(`/api/coach/threads/${encodeURIComponent(threadId)}`, {
        method: "DELETE",
        credentials: "same-origin",
        headers: { "content-type": "application/json" },
      });
      // Not found means it is already gone: the outcome the player asked for.
      if (res.status !== 204 && res.status !== 404) throw new Error(String(res.status));
      setOpen(false);
      toast("Conversation deleted");
      router.push(listHref);
      router.refresh();
    } catch {
      setFailed(true);
    } finally {
      setPending(false);
    }
  }
  return (
    <>
      <Button variant="outline" size="sm" onClick={() => setOpen(true)}>
        <Icon icon={TrashIcon} size={16} />
        Delete conversation
      </Button>
      <Dialog open={open} onOpenChange={(next) => (pending ? null : setOpen(next))}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete this conversation?</DialogTitle>
            <DialogDescription>
              Your questions and your coach's answers are deleted for good. This can't be undone.
            </DialogDescription>
          </DialogHeader>
          {failed ? (
            <p role="alert" className="m-0 font-sans text-sm text-ink">
              {"The conversation wasn't deleted. Check your connection and try again."}
            </p>
          ) : null}
          <DialogFooter>
            <DialogClose render={<Button variant="outline" />}>Cancel</DialogClose>
            <PendingButton
              variant="danger"
              pending={pending}
              pendingLabel="Deleting"
              onClick={() => void remove()}
            >
              Delete conversation
            </PendingButton>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

export function ThreadView({
  thread: initial,
  allowance,
  listHref,
  anchorCard,
  aside,
  gameIds = {},
  available = true,
  verified = true,
}: {
  thread: ThreadData;
  allowance: AllowanceView | null;
  listHref: string;
  /** False while no coach could answer (bug hunt I5): answers stay, the composer is off. */
  available?: boolean;
  /** False while the user's link is not verified (holistic F10): answers stay, no new question. */
  verified?: boolean;
  /** Platform game id to `games.id`, for the answers' move links (`reviewGameIds`). */
  gameIds?: Record<string, string>;
  anchorCard?: ReactNode;
  aside?: ReactNode;
}) {
  const router = useRouter();
  const reduced = useReducedMotion();
  // A move names its game by platform id; unknown ones (a fresh answer) link once refreshed.
  const hrefFor = reviewHref(gameIds);
  const { thread, messages, composer, error, post, pace, answering } = useThread(
    initial.id,
    initial,
    {
      exhausted: allowance?.exhausted ?? null,
      // A finished turn changes the allowance and the list's order: re-read the server parts.
      onSettled: () => router.refresh(),
    },
  );

  // I4: the box is disabled while a question is sent and answered, which would drop keyboard
  // focus on <body>. A send from the box moves focus to the polite "writing" status, and it
  // returns to the box when the composer opens again.
  const composerBox = useRef<HTMLDivElement>(null);
  const statusLine = useRef<HTMLParagraphElement>(null);
  const restoreFocus = useRef(false);
  const send = (question: string) => {
    restoreFocus.current = composerBox.current?.contains(document.activeElement) ?? false;
    return post(question);
  };
  useEffect(() => {
    if (!restoreFocus.current) return;
    if (answering) statusLine.current?.focus();
    else if (!composer.disabled) {
      composerBox.current?.querySelector("textarea")?.focus();
      restoreFocus.current = false;
    }
  }, [answering, composer.disabled]);

  // A new message scrolls into view (instantly under reduced motion); a reload does not.
  const end = useRef<HTMLDivElement>(null);
  const count = useRef(messages.length);
  useEffect(() => {
    if (messages.length > count.current) {
      end.current?.scrollIntoView?.({ block: "end", behavior: reduced ? "auto" : "smooth" });
    }
    count.current = messages.length;
  }, [messages.length, reduced]);

  const main = (
    <div className="flex min-w-0 flex-col gap-6">
      <header className="flex flex-col gap-2">
        <BackLink href={listHref} />
        <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2">
          <h1 className="m-0 min-w-0 font-serif text-2xl font-semibold text-ink [overflow-wrap:anywhere] md:text-3xl">
            {thread.title}
          </h1>
          <DeleteConversation threadId={thread.id} listHref={listHref} />
        </div>
      </header>
      {anchorCard}
      <ol aria-label="Conversation" className="m-0 flex list-none flex-col gap-8 p-0">
        {messages.map((m) => (
          <li key={m.id}>
            <Message
              m={m}
              onRetry={(q) => void post(q)}
              retryDisabled={composer.disabled}
              hrefFor={hrefFor}
              positions={thread.positions ?? {}}
            />
          </li>
        ))}
      </ol>
      <div ref={end} />
      <div
        ref={composerBox}
        data-part="composer"
        className="sticky bottom-0 z-10 flex flex-col gap-1.5 border-t border-rule bg-paper pt-2 pb-3 sm:gap-2 sm:pt-3 sm:pb-4"
      >
        {error ? (
          <p
            role="alert"
            className="m-0 flex max-w-prose items-start gap-2 font-sans text-sm text-ink"
          >
            <Icon icon={WarningIcon} size={16} className="mt-0.5 shrink-0" />
            <span>{error}</span>
          </p>
        ) : null}
        {verified ? null : (
          <CoachState
            kind="computed_only"
            titleAs="h2"
            title={VERIFY_COACH_TITLE}
            body={VERIFY_BODY}
            action={VERIFY_ACTION}
          />
        )}
        <ChatComposer
          onSubmit={send}
          placeholder={COMPOSER_PLACEHOLDER}
          disabled={composer.disabled || !available || !verified}
          // Only a used-up allowance or an unavailable coach pauses questions; an answer in
          // progress is a status line, so the box never says "paused" while the coach is writing.
          disabledReason={
            !verified
              ? VERIFY_COACH_TITLE
              : !available
                ? COACH_UNAVAILABLE_TITLE
                : allowance?.exhausted
                  ? composer.reason
                  : null
          }
          className={COMPACT_COMPOSER}
        />
        <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
          {available && verified ? (
            <AllowanceLine
              allowance={allowance}
              className={answering && !allowance?.exhausted ? "max-sm:hidden" : undefined}
            />
          ) : null}
          {/* Always mounted, so the change is announced when an answer starts. */}
          <p
            ref={statusLine}
            role="status"
            tabIndex={-1}
            className="m-0 font-sans text-sm text-ink-soft empty:hidden focus:outline-none"
          >
            {answering && !allowance?.exhausted ? (
              <>
                {WRITING}
                {pace ? <span className="text-ink-faint">. {pace}</span> : null}
              </>
            ) : null}
          </p>
        </div>
      </div>
    </div>
  );

  if (!aside) return main;
  return (
    <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_minmax(0,3fr)] lg:gap-12">
      <nav aria-label="Other conversations" className="max-lg:hidden">
        <div className="sticky top-6 flex flex-col gap-3">{aside}</div>
      </nav>
      {main}
    </div>
  );
}
