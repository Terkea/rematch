"use client";

import type { Annotation } from "@ci/ui/annotations";
import { Board, type BoardMove, NotationText } from "@ci/ui/board";
import { MarginNote } from "@ci/ui/brand";
import { CheckCircleIcon, Icon } from "@ci/ui/icons";
import { type ExplorerMove, explorerAnnotations, moveFrom } from "@ci/ui/position-explorer";
import { Button } from "@ci/ui/primitives";
import { dayText } from "@ci/ui/profile";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useMemo, useRef, useState } from "react";
import type { AttemptResult, TrainItem } from "../../../../server/train/store";

// Today's session (ADR 0090): one position at a time, from the player's side, with the prompt
// "Find the best move" and no motif named. The player plays a move, says how sure they are, and
// only then sees the verdict, which is about the move ("Right: Rd8# mates." or "Not this time:
// ..."), with the engine's line as numbered arrows and a link to the game at that ply. Grading is
// the server's (`POST /api/train/attempts`), so the answer never reaches the page before it.

type Reveal = AttemptResult & { game: AttemptResult["game"] & { href: string } };

type Phase =
  | { kind: "find" }
  | { kind: "confirm"; uci: string; san: string; fenAfter: string }
  | { kind: "sending"; uci: string; san: string; fenAfter: string }
  | { kind: "revealed"; result: Reveal };

export type SourceNames = Record<string, string>;

const SQUARE = /^[a-h][1-8]$/;
const sq = (s: string) => (SQUARE.test(s) ? (s as BoardMove["from"]) : null);

/** The engine line as explorer moves from the item's position (for the numbered arrows). */
function engineMoves(fen: string, line: readonly { uci: string }[]): ExplorerMove[] {
  const out: ExplorerMove[] = [];
  let at = fen;
  for (const [i, m] of line.entries()) {
    const next = moveFrom(at, m.uci, i + 1);
    if (!next) break;
    out.push(next);
    at = next.fenAfter;
  }
  return out;
}

/** The reveal's marks: the engine's line numbered (or its move), and a wrong move in pen. */
export function revealAnnotations(
  fen: string,
  r: Pick<AttemptResult, "correct" | "move" | "solution" | "line">,
): Annotation[] {
  const engine = engineMoves(fen, r.line);
  const marks = explorerAnnotations(
    { played: r.solution, best: r.solution, found: true },
    "best",
    engine,
  );
  // A one-move line: the engine's move as a good arrow with its `!`.
  const base =
    engine.length >= 2
      ? marks
      : explorerAnnotations({ played: r.solution, best: null, found: true }, "both");
  if (r.correct || r.move.uci === r.solution.uci) return base;
  const from = sq(r.move.uci.slice(0, 2));
  const to = sq(r.move.uci.slice(2, 4));
  if (!from || !to) return base;
  return [
    ...base,
    { kind: "arrow", from, to, tone: "pen", toEdge: true },
    { kind: "mark", square: to, glyph: "?", tone: "pen" },
  ];
}

const DAY = 86_400_000;

/** "today", "yesterday", "3 days ago", else the date. */
export function whenText(playedAt: string, now: Date): string {
  const days = Math.floor((now.getTime() - Date.parse(playedAt)) / DAY);
  if (!Number.isFinite(days)) return "";
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  return `on ${dayText(playedAt.slice(0, 10))}`;
}

export function TrainSession({
  items,
  sources,
}: {
  items: TrainItem[];
  /** Source names, shown only after an attempt. */
  sources: SourceNames;
}) {
  const router = useRouter();
  const [index, setIndex] = useState(0);
  const [phase, setPhase] = useState<Phase>({ kind: "find" });
  const [results, setResults] = useState<boolean[]>([]);
  const [error, setError] = useState<string | null>(null);
  const started = useRef(Date.now());
  const heading = useRef<HTMLHeadingElement>(null);
  const next = useRef<HTMLButtonElement>(null);
  const sure = useRef<HTMLButtonElement>(null);
  const item = items[index];
  const total = items.length;

  // A new position: the clock restarts and the heading takes focus, so it is announced.
  useEffect(() => {
    started.current = Date.now();
    if (index > 0) heading.current?.focus();
  }, [index]);

  useEffect(() => {
    if (phase.kind === "revealed") next.current?.focus();
    if (phase.kind === "confirm") sure.current?.focus();
  }, [phase.kind]);

  const annotations = useMemo(
    () =>
      item && phase.kind === "revealed" ? revealAnnotations(item.fen, phase.result) : undefined,
    [item, phase],
  );

  if (!item) return null;

  const onMove = (m: BoardMove): boolean => {
    if (phase.kind !== "find") return false;
    const uci = `${m.from}${m.to}${m.promotion ?? ""}`;
    const played = moveFrom(item.fen, uci, 1);
    if (!played) return false;
    setError(null);
    setPhase({ kind: "confirm", uci: played.uci, san: played.san, fenAfter: played.fenAfter });
    return true;
  };

  const send = async (confidence: "sure" | "guess") => {
    if (phase.kind !== "confirm") return;
    const { uci, san, fenAfter } = phase;
    setPhase({ kind: "sending", uci, san, fenAfter });
    try {
      const res = await fetch("/api/train/attempts", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          itemId: item.id,
          uci,
          confidence,
          ms: Math.max(0, Math.round(Date.now() - started.current)),
        }),
      });
      if (!res.ok) throw new Error(String(res.status));
      const result = (await res.json()) as Reveal;
      setResults((r) => [...r, result.correct]);
      setPhase({ kind: "revealed", result });
    } catch {
      setError("Your answer was not saved. Check your connection and choose again.");
      setPhase({ kind: "confirm", uci, san, fenAfter });
    }
  };

  const advance = () => {
    if (index + 1 >= total) {
      // The server's summary takes over (today's counts and tomorrow's due items).
      router.refresh();
      return;
    }
    setIndex(index + 1);
    setPhase({ kind: "find" });
  };

  const shownFen = phase.kind === "confirm" || phase.kind === "sending" ? phase.fenAfter : item.fen;
  const shownLast =
    phase.kind === "confirm" || phase.kind === "sending"
      ? { from: phase.uci.slice(0, 2), to: phase.uci.slice(2, 4) }
      : item.lastMove;
  const last =
    shownLast && sq(shownLast.from) && sq(shownLast.to)
      ? { from: shownLast.from as BoardMove["from"], to: shownLast.to as BoardMove["to"] }
      : undefined;
  const moveNumber = Math.max(1, Number(item.fen.split(" ")[5] ?? 1) || 1);
  const colour = item.orientation;
  const right = results.filter(Boolean).length;

  return (
    <section aria-labelledby="train-position" data-item={item.id} className="flex flex-col gap-6">
      <div className="flex flex-col gap-1">
        <p className="m-0 text-sm text-ink-soft [font-variant-numeric:lining-nums]">
          Position {index + 1} of {total}
          {results.length > 0 ? `, ${right} of ${results.length} right so far` : ""}
        </p>
        <h2
          id="train-position"
          ref={heading}
          tabIndex={-1}
          className="font-serif text-2xl font-semibold text-ink outline-none"
        >
          Find the best move
        </h2>
      </div>
      <div className="flex flex-col gap-8 lg:flex-row lg:items-start">
        <Board
          key={item.id}
          fen={shownFen}
          label={`${item.label}. ${phase.kind === "find" ? "Find the best move." : ""}`.trim()}
          orientation={item.orientation}
          {...(last ? { lastMove: last } : {})}
          {...(annotations ? { annotations } : {})}
          interactive={phase.kind === "find"}
          onMove={onMove}
        />
        <div className="flex min-w-0 max-w-md flex-1 flex-col gap-5">
          <p className="m-0 text-base text-ink">
            {colour === "white" ? "White" : "Black"} to move.
          </p>
          {phase.kind === "find" ? (
            <p className="m-0 text-base text-ink-soft">
              Play the move you would choose. Drag a piece, or pick it with the arrow keys and
              Enter.
            </p>
          ) : null}
          {phase.kind === "confirm" || phase.kind === "sending" ? (
            <fieldset className="m-0 flex flex-col gap-3 border-0 p-0">
              <legend className="mb-3 p-0 text-base text-ink">
                Your move: <NotationText moveNumber={moveNumber} color={colour} san={phase.san} />.
                How sure are you?
              </legend>
              <div className="flex flex-wrap gap-3">
                <Button
                  ref={sure}
                  disabled={phase.kind === "sending"}
                  onClick={() => void send("sure")}
                >
                  Sure
                </Button>
                <Button
                  variant="outline"
                  disabled={phase.kind === "sending"}
                  onClick={() => void send("guess")}
                >
                  A guess
                </Button>
                <Button
                  variant="quiet"
                  disabled={phase.kind === "sending"}
                  onClick={() => {
                    setError(null);
                    setPhase({ kind: "find" });
                  }}
                >
                  Take back
                </Button>
              </div>
              {error ? (
                <p role="alert" className="m-0 text-sm text-pen-text">
                  {error}
                </p>
              ) : null}
            </fieldset>
          ) : null}
          {phase.kind === "revealed" ? (
            <RevealPanel
              result={phase.result}
              moveNumber={moveNumber}
              colour={colour}
              source={sources[phase.result.sourceKey] ?? null}
              last={index + 1 >= total}
              nextRef={next}
              onNext={advance}
            />
          ) : null}
        </div>
      </div>
    </section>
  );
}

export function RevealPanel({
  result,
  moveNumber,
  colour,
  source,
  last,
  nextRef,
  onNext,
}: {
  result: Reveal;
  moveNumber: number;
  colour: "white" | "black";
  source: string | null;
  last: boolean;
  nextRef: React.RefObject<HTMLButtonElement | null>;
  onNext: () => void;
}) {
  const when = whenText(result.game.playedAt, new Date());
  return (
    <div className="flex flex-col gap-4">
      <p
        role="status"
        data-slot="verdict"
        className={`m-0 flex items-start gap-2 font-serif text-xl font-semibold ${result.correct ? "text-good-text" : "text-ink"}`}
      >
        {result.correct ? <Icon icon={CheckCircleIcon} className="mt-1 shrink-0" /> : null}
        <span>{result.feedback}</span>
      </p>
      {result.correct ? <MarginNote celebrate>there you go</MarginNote> : null}
      {result.correct ? null : (
        <p className="m-0 text-base text-ink">
          Your move, <NotationText moveNumber={moveNumber} color={colour} san={result.move.san} />,
          is not one of the engine's good moves here.
        </p>
      )}
      {result.line.length > 0 ? (
        <div className="flex flex-col gap-1">
          <p className="m-0 text-sm text-ink-soft">
            {result.line.length > 1
              ? "The engine's line, numbered on the board:"
              : "The engine's move:"}
          </p>
          <p className="m-0 flex flex-wrap gap-x-2 gap-y-1 text-base">
            {result.line.map((m, i) => {
              // Moves after the first are numbered on from the position's move number.
              const n = moveNumber + Math.floor((i + (colour === "black" ? 1 : 0)) / 2);
              const shownNumber = i === 0 || m.color === "white";
              return (
                <span key={`${n}-${m.color}`}>
                  {shownNumber ? (
                    <NotationText moveNumber={n} color={m.color} san={m.san} />
                  ) : (
                    <span className="font-serif font-semibold text-ink">{m.san}</span>
                  )}
                </span>
              );
            })}
          </p>
        </div>
      ) : null}
      <p className="m-0 max-w-prose text-base text-ink-soft">
        In the game you played{" "}
        <NotationText moveNumber={moveNumber} color={colour} san={result.played.san} />
        {result.game.opponent ? ` against ${result.game.opponent}` : ""}
        {when ? `, ${when}` : ""}.{source ? ` It is filed under ${source}.` : ""}
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <Button ref={nextRef} onClick={onNext}>
          {last ? "Finish session" : "Next position"}
        </Button>
        <Button asChild variant="quiet">
          <Link href={result.game.href}>Open the game at this move</Link>
        </Button>
      </div>
    </div>
  );
}
