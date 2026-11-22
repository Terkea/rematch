"use client";

import { lazy, type RefObject, Suspense, useEffect, useRef, useState } from "react";
import type { Annotation } from "../annotations/index.js";
import type { Square } from "../board/theme.js";
import { cn } from "../lib/index.js";
import { ArrowSwatch } from "../profile/arrow-swatch.js";
import type { CitedPosition } from "./types.js";

// A position an answer cites (Fix D, N5): a small static board of the position before the move,
// from the player's side, with the move played as one arrow (pen for the player's own mistake,
// pencil otherwise) and the engine's better move as a `good` arrow, the two told apart by their
// heads as well as their colour. The board itself is loaded only when it nears the viewport, so a
// long thread costs nothing until it is read. A click on the board opens the game at that ply;
// the caption's link is the same way in for the keyboard and screen readers.

const Board = lazy(() => import("../board/board.js").then((m) => ({ default: m.Board })));

const SQUARE = /^[a-h][1-8]$/;

function squares(uci: string | null | undefined): { from: Square; to: Square } | null {
  if (!uci) return null;
  const from = uci.slice(0, 2);
  const to = uci.slice(2, 4);
  if (!SQUARE.test(from) || !SQUARE.test(to) || from === to) return null;
  return { from: from as Square, to: to as Square };
}

/** The board's marks: the played move and, when it differs, the engine's better move. */
export function citedAnnotations(p: CitedPosition): Annotation[] {
  const out: Annotation[] = [];
  const best = p.best && p.best.uci !== p.played.uci ? squares(p.best.uci) : null;
  const played = squares(p.played.uci);
  if (best) out.push({ kind: "arrow", ...best, tone: "good" });
  if (played)
    out.push(
      p.mistake
        ? { kind: "arrow", ...played, tone: "pen", toEdge: true }
        : { kind: "arrow", ...played, tone: "pencil" },
    );
  return out;
}

/** "Position before 30. Rxg7; engine preferred Qe3". */
export function citedLabel(p: CitedPosition): string {
  const best = p.best && p.best.uci !== p.played.uci ? p.best : null;
  return `Position before ${p.moveText}${best ? `; engine preferred ${best.san}` : ""}`;
}

/** True once `ref` comes within a screen of the viewport (at once without IntersectionObserver). */
function useNear(ref: RefObject<HTMLElement | null>): boolean {
  const [near, setNear] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (typeof IntersectionObserver === "undefined") {
      setNear(true);
      return;
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setNear(true);
          io.disconnect();
        }
      },
      { rootMargin: "100% 0px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [ref]);
  return near;
}

const linkClass = cn(
  "inline-flex min-h-11 w-fit items-center font-sans text-sm text-ink underline decoration-rule-strong underline-offset-4",
  "transition-colors duration-(--dur-quick) ease-out hover:decoration-ink",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ink",
);

export function CitedBoard({ position }: { position: CitedPosition }) {
  const ref = useRef<HTMLDivElement>(null);
  const near = useNear(ref);
  const best = position.best && position.best.uci !== position.played.uci ? position.best : null;
  const placeholder = (
    <div aria-hidden="true" className="size-(--board-size) rounded-board bg-well" />
  );
  return (
    <figure data-cited-position className="m-0 flex w-48 shrink-0 flex-col gap-2 md:w-52">
      <div
        ref={ref}
        // ban-lint-allow TOK2: a reference board fills its small column in whole-pixel squares
        className="@container relative w-full [--board-size:round(down,100cqi,8px)] md:[--board-size:round(down,100cqi_-_16px,8px)]"
      >
        {near ? (
          <Suspense fallback={placeholder}>
            <Board
              fen={position.fen}
              label={citedLabel(position)}
              orientation={position.orientation}
              annotations={citedAnnotations(position)}
            />
          </Suspense>
        ) : (
          placeholder
        )}
        {/* The board opens the game for a pointer; the caption's link is the one tab stop. */}
        {/* biome-ignore lint/a11y/useAnchorContent: a pointer-only twin of the caption's link, hidden from assistive tech so the link is announced once */}
        <a
          href={position.href}
          tabIndex={-1}
          aria-hidden="true"
          className="absolute inset-0 rounded-board"
        />
      </div>
      <figcaption className="flex flex-col gap-1">
        <ul
          aria-label="Arrows"
          className="m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 font-sans text-sm text-ink-soft"
        >
          <li className="flex items-center gap-1.5">
            <ArrowSwatch tone={position.mistake ? "pen" : "pencil"} />
            Played, {position.played.san}
          </li>
          {best ? (
            <li className="flex items-center gap-1.5">
              <ArrowSwatch tone="good" />
              Better, {best.san}
            </li>
          ) : null}
        </ul>
        <a href={position.href} className={linkClass}>
          Open the game at {position.moveText}
        </a>
      </figcaption>
    </figure>
  );
}
