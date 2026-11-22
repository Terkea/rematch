"use client";

import { Board, type Square } from "@ci/ui/board";
import { EvalGraph, type EvalGraphPoint } from "@ci/ui/eval-graph";
import { games } from "@ci/ui/fixtures";
import { Kbd } from "@ci/ui/primitives";
import {
  ClockPair,
  type ClockSide,
  moverRow,
  ReviewControls,
  type ReviewKeyAction,
  reviewKeyAction,
  Scoresheet,
  type ScoresheetMove,
  useReviewKeys,
} from "@ci/ui/review";
import { Chess } from "chess.js";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

// The review components on a real fixture game (endgameGrind, 127 plies, bullet). The keys are
// live while the demo is active (click or focus inside it), so they never take Home and End from
// the rest of the styleguide. The graph's slider and the board keep their own arrows (the guard).

const game = games.endgameGrind;
const PLAY_MS = 1000;

const moves: ScoresheetMove[] = game.plies.map((p) => ({
  ply: p.ply,
  color: p.color,
  san: p.san,
  judgement: p.judgement,
  best: p.bestUci !== null && p.uci === p.bestUci,
  mine: p.color === game.playerColor,
}));

const points: EvalGraphPoint[] = game.plies.map((p) => ({
  ply: p.ply,
  winWhite: p.winWhite,
  san: p.san,
  judgement: p.judgement,
}));

/** The player's own judged moves, in game order. */
const moments = game.plies
  .filter((p) => p.judgement && p.color === game.playerColor)
  .map((p) => p.ply);

const lastPly = game.plies.length;

function fenAt(ply: number): string {
  return ply === 0 ? game.startFen : (game.plies[ply - 1]?.fenAfter ?? game.startFen);
}

function checkSquare(fen: string): Square | undefined {
  const chess = new Chess(fen);
  if (!chess.inCheck()) return undefined;
  for (const row of chess.board()) {
    for (const cell of row)
      if (cell?.type === "k" && cell.color === chess.turn()) return cell.square as Square;
  }
  return undefined;
}

/**
 * A side's clock at the shown ply: the clock before its next move, or before its last one. Demo
 * only: at the final position that is the clock before the last move, not the time remaining.
 */
function clockAt(color: "white" | "black", ply: number): number | null {
  const next = game.plies.find((p) => p.ply > ply && p.color === color);
  if (next) return next.clockSeconds;
  const last = game.plies.findLast((p) => p.ply <= ply && p.color === color);
  return last?.clockSeconds ?? null;
}

/** Seconds the mover used on the shown ply, from its clock before this move and its next one. */
function spentAt(ply: number): { seconds: number } | null {
  const move = game.plies[ply - 1];
  const next = game.plies[ply + 1];
  if (!move || !next || move.clockSeconds === null || next.clockSeconds === null) return null;
  const seconds = move.clockSeconds - next.clockSeconds;
  return seconds >= 0 ? { seconds } : null;
}

const KEY_LEGEND: { action: ReviewKeyAction; cap: string; text: string }[] = [
  { action: "prev", cap: "Left", text: "Previous move" },
  { action: "next", cap: "Right", text: "Next move" },
  { action: "first", cap: "Home", text: "First move" },
  { action: "last", cap: "End", text: "Last move" },
  { action: "prevMoment", cap: "Up", text: "Previous mistake" },
  { action: "nextMoment", cap: "Down", text: "Next mistake" },
  { action: "play", cap: "Space", text: "Autoplay" },
  { action: "flip", cap: "F", text: "Flip board" },
];

function ReviewDemo() {
  const [ply, setPly] = useState(() => moments[0] ?? lastPly);
  const [playing, setPlaying] = useState(false);
  const [orientation, setOrientation] = useState<"white" | "black">(game.playerColor);
  const [active, setActive] = useState(false);
  const [lastKey, setLastKey] = useState<ReviewKeyAction | null>(null);
  const root = useRef<HTMLDivElement>(null);

  const go = useCallback((next: number) => {
    setPlaying(false);
    setPly(Math.min(lastPly, Math.max(0, next)));
  }, []);
  const moment = useCallback(
    (dir: 1 | -1) => {
      const target = dir === 1 ? moments.find((m) => m > ply) : moments.findLast((m) => m < ply);
      if (target !== undefined) go(target);
    },
    [ply, go],
  );
  const togglePlay = useCallback(() => {
    setPlaying((p) => {
      if (!p && ply >= lastPly) setPly(0);
      return !p;
    });
  }, [ply]);
  const flip = useCallback(() => setOrientation((o) => (o === "white" ? "black" : "white")), []);

  useEffect(() => {
    if (!playing) return;
    const id = setInterval(() => {
      setPly((p) => {
        if (p >= lastPly - 1) setPlaying(false);
        return Math.min(lastPly, p + 1);
      });
    }, PLAY_MS);
    return () => clearInterval(id);
  }, [playing]);

  // Active while the pointer or focus was last inside the demo.
  useEffect(() => {
    const inside = (t: EventTarget | null) => t instanceof Node && !!root.current?.contains(t);
    const onPointer = (e: PointerEvent) => setActive(inside(e.target));
    const onFocus = (e: FocusEvent) => setActive(inside(e.target));
    window.addEventListener("pointerdown", onPointer);
    window.addEventListener("focusin", onFocus);
    return () => {
      window.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("focusin", onFocus);
    };
  }, []);

  useReviewKeys(
    {
      prev: () => go(ply - 1),
      next: () => go(ply + 1),
      first: () => go(0),
      last: () => go(lastPly),
      prevMoment: () => moment(-1),
      nextMoment: () => moment(1),
      play: togglePlay,
      flip,
    },
    { enabled: active },
  );

  // The legend lights the key that just acted, with the same guard as the hook.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      const action = reviewKeyAction(e);
      if (action && action !== "engine") setLastKey(action);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [active]);

  const fen = fenAt(ply);
  const move = game.plies[ply - 1];
  const lastMove = move
    ? { from: move.uci.slice(0, 2) as Square, to: move.uci.slice(2, 4) as Square }
    : undefined;
  const check = useMemo(() => checkSquare(fen), [fen]);
  const whiteToMove = ply % 2 === 0;
  const side = (color: "white" | "black"): ClockSide => ({
    name: game[color].name,
    rating: game[color].rating,
    seconds: clockAt(color, ply),
    toMove: (color === "white") === whiteToMove && ply < lastPly,
  });
  const top = orientation === "white" ? side("black") : side("white");
  const bottom = orientation === "white" ? side("white") : side("black");
  const title = `${game.white.name} v ${game.black.name}`;

  return (
    <div
      ref={root}
      data-review-demo
      data-keys={active ? "on" : "off"}
      className="grid gap-6 lg:grid-cols-[auto_minmax(0,1fr)] max-md:-mx-(--space-panel-pad)"
    >
      <div className="flex min-w-0 flex-col gap-4">
        <Board
          fen={fen}
          label={`${title}, after ${move ? move.san : "the start"}, seen from ${orientation === "white" ? "White" : "Black"}`}
          orientation={orientation}
          {...(lastMove ? { lastMove } : {})}
          {...(check ? { check } : {})}
        />
        <ReviewControls
          className="max-md:px-(--space-panel-pad)"
          ply={ply}
          lastPly={lastPly}
          playing={playing}
          hasMoments={moments.length > 0}
          onFirst={() => go(0)}
          onPrev={() => go(ply - 1)}
          onPlayToggle={togglePlay}
          onNext={() => go(ply + 1)}
          onLast={() => go(lastPly)}
          onFlip={flip}
          onPrevMoment={() => moment(-1)}
          onNextMoment={() => moment(1)}
        />
      </div>
      <div className="flex min-w-0 flex-col gap-4 max-md:px-(--space-panel-pad)">
        {/* The fixture's %clk is whole seconds: no tenths it doesn't have. */}
        <ClockPair
          top={top}
          bottom={bottom}
          spent={(() => {
            const s = spentAt(ply);
            return s
              ? { ...s, by: moverRow(ply, orientation === "white" ? "black" : "white") }
              : null;
          })()}
          tenths={false}
        />
        <Scoresheet
          moves={moves}
          currentPly={ply}
          onSelectPly={go}
          label={`Moves, ${title}`}
          announce={!playing}
          className="h-72 bg-paper lg:h-96"
        />
        <EvalGraph
          points={points}
          label={`Win chance, ${title}`}
          currentPly={ply}
          onSelectPly={go}
          playerColor={game.playerColor}
        />
        <div className="flex flex-col gap-2">
          <p className="text-sm text-ink-soft">
            {active
              ? "Keys are on. Focus the graph or the board and the arrows are theirs."
              : "Click or tab into the demo to turn the keys on."}
          </p>
          <ul className="flex flex-wrap gap-x-4 gap-y-2" aria-label="Review keys">
            {KEY_LEGEND.map((k) => (
              <li
                key={k.action}
                data-lit={lastKey === k.action || undefined}
                className="flex items-center gap-1.5 text-sm text-ink-soft data-lit:text-ink"
              >
                <Kbd className="in-data-lit:border-b-ink in-data-lit:text-ink">{k.cap}</Kbd>
                {k.text}
              </li>
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}

const noClockSide = (name: string, toMove: boolean): ClockSide => ({
  name,
  rating: null,
  seconds: null,
  toMove,
});

export function ReviewSection() {
  return (
    <section
      id="review"
      data-section="review"
      aria-labelledby="review-title"
      className="flex flex-col gap-8 rounded-sheet bg-sheet p-(--space-panel-pad)"
    >
      <div className="flex max-w-prose flex-col gap-1">
        <h2 id="review-title" className="font-serif text-2xl font-semibold">
          Review
        </h2>
        <p className="text-sm text-ink-soft">
          The scoresheet, clocks and controls of the game review, on {game.white.name} v{" "}
          {game.black.name} ({game.timeClass}, {lastPly} half-moves). The current move carries a
          well that travels down the sheet. Your mistakes are glyphed in pen, the opponent's in
          graphite.
        </p>
      </div>
      <ReviewDemo />
      <div className="grid gap-8 md:grid-cols-2">
        <figure className="flex flex-col gap-3">
          <figcaption className="text-sm text-ink-soft">
            A game without clock data shows no clocks, never 0:00.
          </figcaption>
          <ClockPair
            top={noClockSide(game.black.name, false)}
            bottom={noClockSide(game.white.name, true)}
            spent={null}
          />
        </figure>
        <figure className="flex flex-col gap-3">
          <figcaption className="text-sm text-ink-soft">
            Synthetic example: a daily game, clocks in days and hours.
          </figcaption>
          <ClockPair
            top={{ name: "Correspondent", rating: 1840, seconds: 183_600, toMove: false }}
            bottom={{ name: "You", rating: 1712, seconds: 40_210, toMove: true }}
            spent={{ seconds: 3_723, by: "bottom" }}
          />
        </figure>
        <figure className="flex flex-col gap-3 md:col-span-2">
          <figcaption className="text-sm text-ink-soft">An empty game.</figcaption>
          <Scoresheet
            moves={[]}
            currentPly={0}
            onSelectPly={() => {}}
            label="Moves, empty game"
            className="bg-paper"
          />
        </figure>
      </div>
    </section>
  );
}
