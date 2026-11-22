"use client";

import {
  AiDisclosure,
  ChatComposer,
  ChatMessage,
  type ChatMessageProps,
  CoachState,
  type CoachStateProps,
  type Segment,
  SegmentText,
  type StreamedChunk,
} from "@ci/ui/coach";
import { TooltipProvider } from "@ci/ui/primitives";
import { useEffect, useState } from "react";

// The coach's text, its disclosure, its states and the chat pieces (M5b Task 3, ADR 0071). The
// chess is real: 28. Nd2 (+2.7 to −1.1) from the fixture blitz game DrNykterstein v Night-King96,
// and 9… b5? 10. Nxb5 from Morphy v Duke Karl and Count Isouard, Paris 1858 (the references' game).
// The demo answers are sample copy, labelled as such, never presented as a player's own report.

const disclosure = {
  label: "Written by AI",
  note: "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.",
  model: "claude-sonnet-5-5",
  promptVersion: "chat@1",
  generatedAt: "2026-11-12T08:30:00.000Z",
};

const G = "middlegame-blunder";
const OPERA = "opera-1858";

const nd2: Segment[] = [
  { kind: "text", text: "Against Night-King96 you were winning until " },
  { kind: "move", gameId: G, ply: 55, san: "Nd2" },
  { kind: "text", text: " on move 28. The knight left " },
  { kind: "square", square: "e4" },
  { kind: "text", text: " and the evaluation fell from " },
  { kind: "eng", evalId: "ev-54", line: 0, n: null, text: "+2.7" },
  { kind: "text", text: " to " },
  { kind: "eng", evalId: "ev-55", line: 0, n: null, text: "−1.1" },
  { kind: "text", text: ". It was the first of " },
  { kind: "num", path: "game.blunders", value: 3, display: "3" },
  { kind: "text", text: " blunders you made in that game. Your next report is due on " },
  { kind: "date", path: "report.nextAt", date: "2026-12-12" },
  { kind: "text", text: "." },
];

const opera: Segment[] = [
  { kind: "text", text: "In the Opera game, " },
  { kind: "move", gameId: OPERA, ply: 18, san: "b5" },
  { kind: "text", text: " was met at once by " },
  { kind: "best", gameId: OPERA, ply: 19, san: "Nxb5" },
  { kind: "text", text: ", and after " },
  { kind: "pv", gameId: OPERA, ply: 19, side: "best", n: 0, san: "Nxb5 cxb5 Bxb5+ Nbd7" },
  { kind: "text", text: " White's pieces poured in. The game had opened with " },
  { kind: "line", lineKey: "philidor", n: null, san: "1. e4 e5 2. Nf3 d6" },
  { kind: "text", text: ", the Philidor Defence." },
];

const hrefFor = (s: Segment) =>
  s.kind === "move" || s.kind === "best" || s.kind === "pv" ? `#coach-ply-${s.ply}` : null;

const states: { caption: string; props: CoachStateProps }[] = [
  {
    caption: "Building, with a count",
    props: {
      kind: "building",
      title: "Building your report",
      body: "Sections appear here as they are written.",
      progress: { done: 2, total: 5, unit: "sections" },
    },
  },
  {
    caption: "Writing, nothing counted yet",
    props: {
      kind: "writing",
      title: "Your coach is reading your games",
      body: "The first section is usually ready within a minute.",
    },
  },
  {
    caption: "Failed",
    props: {
      kind: "failed",
      title: "The report could not be written",
      body: "The AI service did not answer. Your computed numbers are below; try again in a few minutes.",
      action: { label: "Try again", onClick: () => {} },
    },
  },
  {
    caption: "Allowance used",
    props: {
      kind: "budget",
      title: "This week's questions are used",
      body: "You have asked 3 of 3 questions this week. They reset on 16 Nov 2026.",
    },
  },
  {
    caption: "Unavailable",
    props: {
      kind: "unavailable",
      title: "Coaching is switched off here",
      body: "This server has no AI key, so reports show computed numbers only.",
    },
  },
  {
    caption: "Computed only",
    props: {
      kind: "computed_only",
      title: "Numbers without commentary",
      body: "These figures come from your analysed games. No AI text was written for this report.",
    },
  },
  {
    caption: "Empty",
    props: {
      kind: "empty",
      title: "No questions yet",
      body: "Ask about a game, a move or a weakness, and the answer stays here.",
      action: { label: "Open your games", href: "#coach" },
    },
  },
];

const base: ChatMessageProps = {
  role: "assistant",
  status: "done",
  segments: null,
  streamed: [],
  notes: [],
  engineChecks: [],
  failure: null,
  disclosure: null,
};

const STREAM: StreamedChunk[] = [
  { seq: 0, interim: false, segments: [{ kind: "text", text: "Before " }] },
  {
    seq: 1,
    interim: false,
    segments: [
      { kind: "move", gameId: G, ply: 55, san: "Nd2" },
      { kind: "text", text: " the knight on " },
      { kind: "square", square: "e4" },
      { kind: "text", text: " held your position together. " },
    ],
  },
  {
    seq: 2,
    interim: true,
    segments: [{ kind: "text", text: "Checking 28. Nd2 e4 29. Nxe4 with the engine" }],
  },
  {
    seq: 3,
    interim: false,
    segments: [{ kind: "text", text: "Once it left, Black pushed e4 and" }],
  },
];

/** The streaming answer: rests mid-stream for screenshots; Play streams it in from the start. */
function StreamingDemo() {
  const [shown, setShown] = useState(STREAM.length);
  const [run, setRun] = useState(0);
  useEffect(() => {
    if (run === 0 || shown >= STREAM.length) return;
    const id = setTimeout(() => setShown((n) => n + 1), 700);
    return () => clearTimeout(id);
  }, [run, shown]);
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-4">
        <h4 className="m-0 font-sans text-sm font-medium text-ink-faint">Streaming</h4>
        <button
          type="button"
          onClick={() => {
            setShown(0);
            setRun((n) => n + 1);
          }}
          className="relative rounded-field px-3 py-1.5 text-sm font-medium text-ink-soft hover:bg-well focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink"
        >
          Play stream
        </button>
      </div>
      <ChatMessage
        key={run}
        {...base}
        status={shown === 0 ? "pending" : "streaming"}
        streamed={STREAM.slice(0, shown)}
        engineChecks={[{ status: "pending", moves: ["Nd2", "e4", "Nxe4"] }]}
        hrefFor={hrefFor}
      />
    </div>
  );
}

function Caption({ children }: { children: string }) {
  return <h4 className="m-0 font-sans text-sm font-medium text-ink-faint">{children}</h4>;
}

export function CoachSection() {
  return (
    <TooltipProvider>
      <section
        id="coach"
        data-section="coach"
        aria-labelledby="coach-title"
        className="flex flex-col gap-10 rounded-sheet bg-sheet p-(--space-panel-pad)"
      >
        <div className="flex max-w-prose flex-col gap-1">
          <h2 id="coach-title" className="font-serif text-2xl font-semibold">
            Coach
          </h2>
          <p className="text-sm text-ink-soft">
            The coach writes in stored segments, never raw tokens. Moves are notation and link to
            the position, the better move carries the solid good head, and anything the engine
            checked while answering wears a dashed pencil line. Every piece of model text says it
            was written by AI. The answers here are sample copy over real games.
          </p>
        </div>

        <div className="flex flex-col gap-4">
          <h3 className="m-0 font-serif text-xl font-semibold">Segments</h3>
          <div className="flex max-w-prose flex-col gap-4 font-serif text-prose text-ink">
            <SegmentText segments={nd2} hrefFor={hrefFor} />
            <SegmentText segments={opera} hrefFor={hrefFor} />
          </div>
        </div>

        <div className="flex flex-col gap-4">
          <h3 className="m-0 font-serif text-xl font-semibold">AI disclosure</h3>
          <div className="flex flex-col gap-6">
            <div className="flex flex-col gap-2">
              <Caption>Under a report section</Caption>
              <AiDisclosure disclosure={disclosure} />
            </div>
            <div className="flex flex-col gap-2">
              <Caption>Compact, with the note behind the info button</Caption>
              <AiDisclosure disclosure={disclosure} compact />
            </div>
          </div>
        </div>

        <div className="flex flex-col gap-4">
          <h3 className="m-0 font-serif text-xl font-semibold">States</h3>
          <ul className="m-0 grid list-none gap-x-8 gap-y-8 p-0 lg:grid-cols-2">
            {states.map((s) => (
              <li key={s.props.kind} className="flex flex-col gap-2">
                <Caption>{s.caption}</Caption>
                <CoachState {...s.props} titleAs="p" />
              </li>
            ))}
          </ul>
        </div>

        <div className="flex flex-col gap-4">
          <h3 className="m-0 font-serif text-xl font-semibold">Conversation</h3>
          <div className="flex max-w-3xl flex-col gap-8">
            <ChatMessage
              {...{ ...base, role: "user" as const }}
              segments={[{ kind: "text", text: "Why did I lose the blitz game after move 28?" }]}
            />
            <ChatMessage
              {...base}
              segments={nd2}
              hrefFor={hrefFor}
              engineChecks={[{ status: "done", moves: ["Nd2", "e4", "Nxe4"] }]}
              notes={["One statement was left out because the engine could not confirm it."]}
              disclosure={disclosure}
            />
            <StreamingDemo />
            <div className="flex flex-col gap-3">
              <Caption>Waiting for the first words</Caption>
              <ChatMessage {...base} status="pending" />
            </div>
            <div className="flex flex-col gap-3">
              <Caption>Failed</Caption>
              <ChatMessage
                {...base}
                status="failed"
                failure="Your coach could not answer this time. Ask again in a minute; this question was not counted."
              />
            </div>
          </div>
        </div>

        <div className="flex flex-col gap-4">
          <h3 className="m-0 font-serif text-xl font-semibold">Composer</h3>
          <div className="flex max-w-3xl flex-col gap-8">
            <div className="flex flex-col gap-2">
              <Caption>Ready</Caption>
              <ChatComposer placeholder="Ask about your games" onSubmit={() => {}} />
            </div>
            <div className="flex flex-col gap-2">
              <Caption>Allowance used</Caption>
              <ChatComposer
                placeholder="Ask about your games"
                onSubmit={() => {}}
                disabled
                disabledReason="You have asked 3 of 3 questions this week. They reset on 16 Nov 2026."
              />
            </div>
          </div>
        </div>
      </section>
    </TooltipProvider>
  );
}
