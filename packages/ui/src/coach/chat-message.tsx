"use client";

import type { ReactNode } from "react";
import { CheckCircleIcon, Icon, InfoIcon, WarningCircleIcon, WarningIcon } from "../icons/index.js";
import { cn } from "../lib/index.js";
import { motion, spring, useReducedMotion } from "../motion/index.js";
import { AiDisclosure, AiLabel } from "./ai-disclosure.js";
import { CoachState } from "./coach-state.js";
import { notationClass, SegmentText } from "./segment-text.js";
import type { AiDisclosureData, EngineCheck, Segment, StreamedChunk } from "./types.js";

// One message in a coach thread (ADR 0071). It reads like a coach's notebook, not a chat app: the
// question is set in the UI face on a `well`, the answer in Literata prose on the page, left
// aligned, with no bubbles or avatars. Model text is always labelled: the stored disclosure once
// done, the fixed label while it streams.

export interface ChatMessageProps {
  role: "user" | "assistant";
  status: "pending" | "streaming" | "done" | "failed";
  segments: Segment[] | null;
  streamed: StreamedChunk[];
  notes: string[];
  engineChecks: EngineCheck[];
  failure: string | null;
  disclosure: AiDisclosureData | null;
  hrefFor?: ((s: Segment) => string | null) | undefined;
  className?: string;
}

/** Placeholder copy while nothing has streamed yet (ADR 0071). */
export const READING_COPY = "Your coach is reading your games";

const FAILED_FALLBACK = "This answer could not be written. Ask again in a minute.";

/**
 * The tried moves in notation, numbered from the game ply of the first one: `startPly` 55 gives
 * `28. Nd2 e4 29. Nxe4`, 56 gives `28… e4 29. Nxe4`. Without a valid ply the moves are left
 * unnumbered rather than guessed from move 1.
 */
export function engineLine(moves: readonly string[], startPly?: number | null): string {
  const list = moves.filter((m) => typeof m === "string" && m !== "");
  if (typeof startPly !== "number" || !Number.isInteger(startPly) || startPly < 1)
    return list.join(" ");
  const out: string[] = [];
  list.forEach((san, i) => {
    const ply = startPly + i;
    const move = Math.ceil(ply / 2);
    if (ply % 2 === 1) out.push(`${move}.`);
    else if (i === 0) out.push(`${move}…`);
    out.push(san);
  });
  return out.join(" ");
}

const STATUS_WORD: Record<EngineCheck["status"], string> = {
  pending: "checking",
  done: "done",
  failed: "failed",
};

type Run = { interim: false; chunks: StreamedChunk[] } | { interim: true; chunk: StreamedChunk };

/** Chunks in `seq` order, with consecutive answer chunks joined into one run of prose. */
export function streamRuns(streamed: readonly StreamedChunk[]): Run[] {
  const sorted = [...streamed]
    .filter((c) => c && Number.isFinite(c.seq))
    .sort((a, b) => a.seq - b.seq);
  const runs: Run[] = [];
  for (const chunk of sorted) {
    const last = runs.at(-1);
    if (chunk.interim) runs.push({ interim: true, chunk });
    else if (last && !last.interim) last.chunks.push(chunk);
    else runs.push({ interim: false, chunks: [chunk] });
  }
  return runs;
}

/** A small dashed pencil ring: the mark for "the engine was asked", beside interim lines. */
function PencilRing() {
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden="true"
      focusable="false"
      className="mt-0.5 size-4 shrink-0 fill-none stroke-pencil"
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
  );
}

function Streamed({
  streamed,
  hrefFor,
}: {
  streamed: StreamedChunk[];
  hrefFor: ChatMessageProps["hrefFor"];
}) {
  const reduced = useReducedMotion();
  const runs = streamRuns(streamed);
  return (
    <motion.div
      data-part="streamed"
      className="flex flex-col gap-3"
      initial={reduced ? false : { opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      transition={spring.pen}
    >
      {runs.map((run) =>
        run.interim ? (
          <aside
            key={`i${run.chunk.seq}`}
            data-tone="pencil"
            aria-label="Engine working"
            className="flex max-w-prose gap-2 font-sans text-sm text-ink-soft"
          >
            <PencilRing />
            <SegmentText as="span" segments={run.chunk.segments} hrefFor={hrefFor} />
          </aside>
        ) : (
          <p
            key={`p${run.chunks[0]?.seq ?? 0}`}
            className="m-0 max-w-prose font-serif text-prose whitespace-pre-line text-ink"
          >
            {run.chunks.map((c) => (
              <SegmentText key={c.seq} as="span" segments={c.segments} hrefFor={hrefFor} />
            ))}
          </p>
        ),
      )}
    </motion.div>
  );
}

/** Done is a settled check, pending the dashed pencil ring, failed a warning: shape, not colour. */
function CheckMark({ status }: { status: EngineCheck["status"] }) {
  if (status === "done")
    return (
      <span data-mark="done" className="mt-0.5 inline-flex shrink-0 text-ink-soft">
        <Icon icon={CheckCircleIcon} size={16} />
      </span>
    );
  if (status === "failed")
    return (
      <span data-mark="failed" className="mt-0.5 inline-flex shrink-0 text-ink-soft">
        <Icon icon={WarningCircleIcon} size={16} />
      </span>
    );
  return (
    <span data-mark="pending" className="inline-flex shrink-0">
      <PencilRing />
    </span>
  );
}

function EngineChecks({ checks }: { checks: EngineCheck[] }) {
  const valid = checks.filter((c) => c && Array.isArray(c.moves) && c.moves.length > 0);
  if (valid.length === 0) return null;
  return (
    <ul aria-label="Engine checks" className="m-0 flex list-none flex-col gap-1 p-0">
      {valid.map((c, i) => (
        <li
          // biome-ignore lint/suspicious/noArrayIndexKey: the server's checks are a fixed list.
          key={i}
          data-engine-check={c.status}
          className="flex items-start gap-2 font-sans text-sm text-ink-soft"
        >
          <CheckMark status={c.status} />
          <span>
            Engine check: <span className={notationClass}>{engineLine(c.moves, c.startPly)}</span> (
            {STATUS_WORD[c.status] ?? "checking"})
          </span>
        </li>
      ))}
    </ul>
  );
}

function Notes({ notes }: { notes: string[] }) {
  const list = notes.filter((n) => typeof n === "string" && n.trim() !== "");
  if (list.length === 0) return null;
  return (
    <ul aria-label="Notes" className="m-0 flex list-none flex-col gap-1 p-0">
      {list.map((n, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: notes are fixed server copy and may repeat.
        <li key={`${i}-${n}`} className="flex items-start gap-2 font-sans text-sm text-ink-soft">
          <Icon icon={InfoIcon} size={16} className="mt-0.5 shrink-0" />
          <span className="max-w-prose">{n}</span>
        </li>
      ))}
    </ul>
  );
}

export function ChatMessage({
  role,
  status,
  segments,
  streamed,
  notes,
  engineChecks,
  failure,
  disclosure,
  hrefFor,
  className,
}: ChatMessageProps) {
  if (role === "user") {
    return (
      <article
        data-role="user"
        aria-label="Your question"
        className={cn("flex flex-col items-start gap-1.5", className)}
      >
        <p className="m-0 font-sans text-sm text-ink-faint">You asked</p>
        <div className="max-w-prose rounded-field bg-well px-4 py-3 font-sans text-base whitespace-pre-line text-ink">
          <SegmentText as="span" segments={segments ?? []} hrefFor={hrefFor} />
        </div>
      </article>
    );
  }

  const live = status === "pending" || status === "streaming";
  const chunks = Array.isArray(streamed) ? streamed : [];
  const hasStream = live && chunks.length > 0;

  let body: ReactNode;
  if (status === "failed") {
    body = (
      <div
        role="status"
        data-coach-state="failed"
        className="flex max-w-prose gap-3 rounded-field border border-rule-strong bg-well p-4"
      >
        <Icon icon={WarningIcon} size={20} className="mt-0.5 shrink-0 text-ink" />
        <p className="m-0 font-sans text-base text-ink">{failure?.trim() || FAILED_FALLBACK}</p>
      </div>
    );
  } else if (hasStream) {
    body = (
      <>
        <Streamed streamed={chunks} hrefFor={hrefFor} />
        <p className="m-0 font-sans text-sm text-ink-faint">Your coach is still writing</p>
      </>
    );
  } else if (live) {
    body = <CoachState kind="writing" title={READING_COPY} titleAs="p" />;
  } else {
    body = (
      <SegmentText
        segments={segments ?? []}
        hrefFor={hrefFor}
        className="max-w-prose font-serif text-prose text-ink"
      />
    );
  }

  const header =
    status === "done" && disclosure ? (
      <AiDisclosure disclosure={disclosure} compact />
    ) : hasStream || (status === "done" && (segments?.length ?? 0) > 0) ? (
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <AiLabel />
      </div>
    ) : null;

  return (
    <article
      data-role="assistant"
      data-status={status}
      aria-label="Coach's answer"
      className={cn("flex flex-col gap-3", className)}
    >
      {header}
      {/* Polite only while the answer is arriving: chunks are read as they land, and the final
          text that replaces them at done is not read a second time. */}
      <div aria-live={live ? "polite" : "off"} className="flex flex-col gap-3">
        {body}
      </div>
      {status !== "failed" ? (
        <>
          <EngineChecks checks={Array.isArray(engineChecks) ? engineChecks : []} />
          <Notes notes={Array.isArray(notes) ? notes : []} />
        </>
      ) : null}
    </article>
  );
}
