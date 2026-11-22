"use client";

import { type Annotation, seededRandom, strokeOutline } from "@ci/ui/annotations";
import { Board, EvalBar, formatEval, judgementGlyph, type Score, type Square } from "@ci/ui/board";
import { EvalGraph, type EvalGraphPoint } from "@ci/ui/eval-graph";
import { games } from "@ci/ui/fixtures";
import { ArrowCounterClockwiseIcon, Icon } from "@ci/ui/icons";
import { animate, dur, ease, useReducedMotion } from "@ci/ui/motion";
import { Button } from "@ci/ui/primitives";
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

// The styleguide hero (DESIGN.md "Styleguide hero"; design/decisions/styleguide-hero.md, variant A),
// built from the shipped board, eval bar and eval graph. It tells one moment of a real game: the
// player's worst blunder, played on the board through the climax rows of choreography 1, then the
// scoresheet ring and the coach's margin comment. Everything below it is computed from the fixture.

const game = games.middlegameBlunder;
const plies = game.plies;
type Ply = (typeof plies)[number];

function must<T>(value: T | undefined, what: string): T {
  if (value === undefined) throw new Error(`hero: no ${what}`);
  return value;
}

/** The player's blunder that lost the most win chance. */
const keyPly = plies
  .filter((p) => p.color === game.playerColor && p.judgement === "blunder")
  .reduce((worst, p) => (drop(p) > drop(worst) ? p : worst));
function drop(p: Ply): number {
  const before = plies[p.ply - 2]?.winWhite ?? 50;
  return game.playerColor === "white" ? before - p.winWhite : p.winWhite - before;
}
const prevPly = must(plies[keyPly.ply - 2], "ply before the key move");
const nextPly = must(plies[keyPly.ply], "reply to the key move");

const moveNo = (ply: number) => Math.ceil(ply / 2);
const scoreOf = (p: Ply): Score =>
  p.mateWhite !== null ? { mate: p.mateWhite } : { cp: p.cpWhite ?? 0 };
const sq = (uci: string, at: 0 | 2) => uci.slice(at, at + 2) as Square;
const from = sq(keyPly.uci, 0);
const to = sq(keyPly.uci, 2);
const keyLabel = `${moveNo(keyPly.ply)}${keyPly.color === "white" ? "." : "…"} ${keyPly.san}${judgementGlyph.blunder}`;

const title = `${game.white.name} v ${game.black.name}`;
const you = game.playerColor === "white" ? "White" : "Black";
const context = `Lichess ${game.timeClass}, ${game.openingName}. You had ${you}, and after ${keyLabel} it is ${you === "White" ? "Black" : "White"} to move.`;
const note = `Your knight on ${from} was holding back ${game.playerColor === "white" ? "Black" : "White"}’s e-pawn. The moment it went to ${to}, the pawn came forward with ${moveNo(nextPly.ply)}…${nextPly.san}, and ${formatEval(scoreOf(prevPly))} became ${formatEval(scoreOf(keyPly))}.`;

/** The excerpt opens on the key move's row and stops before the next blunder (DESIGN.md). */
const rows: { n: number; white?: Ply; black?: Ply }[] = [];
for (let n = moveNo(keyPly.ply); rows.length < 3; n++) {
  const white = plies[2 * n - 2];
  const black = plies[2 * n - 1];
  if (!white || (rows.length > 0 && [white, black].some((p) => p?.judgement === "blunder"))) break;
  rows.push({ n, white, black });
}

/**
 * One mark in the hero graph: the key move, the only label in its lane (DESIGN.md "Styleguide
 * hero"). EvalGraph has no tick-only mark yet, so the other mistakes are left off, not pencilled.
 */
const points: EvalGraphPoint[] = plies.map((p) => ({
  ply: p.ply,
  winWhite: p.winWhite,
  san: p.san,
  judgement: p.ply === keyPly.ply ? p.judgement : null,
}));

const marks: Annotation[] = [
  { kind: "circle", square: to, tone: "pen" },
  { kind: "arrow", from, to, tone: "pen" },
  { kind: "mark", square: to, glyph: "??", tone: "pen" },
];

const ms = (seconds: number) => Math.round(seconds * 1000);
/** Choreography 1's climax rows (DESIGN.md), shifted by the graph's prologue when it plays. */
function beat(prologue: boolean) {
  const t0 = prologue ? ms(dur.reveal) : 0;
  return { lift: t0 + 400, ring: t0 + 860, comment: t0 + 1820 };
}

type Step = "before" | "played" | "marked" | "noted";
const order: Step[] = ["before", "played", "marked", "noted"];
const atLeast = (step: Step, min: Step) => order.indexOf(step) >= order.indexOf(min);

/** The beat: one run per mount or replay; the end state at once under reduced motion. */
function useBeat(run: number, graph: React.RefObject<HTMLElement | null>) {
  const reduced = useReducedMotion();
  const [state, setState] = useState<{ step: Step; prologue: boolean }>({
    step: "before",
    prologue: false,
  });
  // biome-ignore lint/correctness/useExhaustiveDependencies: `run` restarts the beat
  useEffect(() => {
    if (reduced) {
      setState({ step: "noted", prologue: false });
      return;
    }
    // The prologue plays only when the whole graph is in view at the start (a phone has it below
    // the fold, and gets no stillness instead).
    const rect = graph.current?.getBoundingClientRect();
    const prologue = !!rect && rect.top >= 0 && rect.bottom <= window.innerHeight;
    const t = beat(prologue);
    setState({ step: "before", prologue });
    const timers = [
      setTimeout(() => setState((s) => ({ ...s, step: "played" })), t.lift),
      setTimeout(() => setState((s) => ({ ...s, step: "marked" })), t.ring),
      setTimeout(() => setState((s) => ({ ...s, step: "noted" })), t.comment),
    ];
    return () => {
      for (const timer of timers) clearTimeout(timer);
    };
  }, [reduced, run, graph]);
  return { ...state, reduced };
}

/**
 * The scoresheet ring (DESIGN.md "Pen stroke geometry"): the board pen at 0.8 width, fitted to the
 * word as an ellipse with rx = width / 2 + 10px and ry = 14px, tilted 2–4° (seeded), spiralling to
 * 380°. Drawn on over `dur.drawSmall` with `ease.pen`, through a mask, on a sheet halo.
 */
function SheetRing({ word, draw, instant }: { word: string; draw: boolean; instant: boolean }) {
  const host = useRef<HTMLSpanElement>(null);
  const reveal = useRef<SVGPathElement>(null);
  const [width, setWidth] = useState(0);
  const id = useId();
  useLayoutEffect(() => {
    const el = host.current;
    if (!el) return;
    const measure = () => setWidth(el.getBoundingClientRect().width);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const path = reveal.current;
    if (!path || !draw) return;
    if (instant) {
      path.style.strokeDashoffset = "0";
      return;
    }
    const controls = animate(
      path,
      { strokeDashoffset: [1, 0] },
      { duration: dur.drawSmall, ease: [...ease.pen] },
    );
    return () => controls.stop();
  }, [draw, instant]);

  const rx = width / 2 + 10;
  const ry = 14;
  const rand = seededRandom(`sheet-ring:${word}`);
  const tilt = ((2 + 2 * rand()) * (rand() < 0.5 ? -1 : 1) * Math.PI) / 180;
  const phi1 = rand() * 2 * Math.PI;
  const phi2 = rand() * 2 * Math.PI;
  const pad = 8;
  const cx = rx + pad;
  const cy = ry + pad;
  let d = "";
  for (let deg = 0; deg <= 380; deg += 5) {
    const th = ((-60 - deg) * Math.PI) / 180;
    const drift = 1 + 0.04 * Math.sin(2 * th + phi1) + 0.02 * Math.sin(3 * th + phi2);
    const grow = (2.5 * deg) / 360;
    const ex = (rx * drift + grow) * Math.cos(th);
    const ey = (ry * drift + grow) * Math.sin(th);
    const x = cx + ex * Math.cos(tilt) - ey * Math.sin(tilt);
    const y = cy + ex * Math.sin(tilt) + ey * Math.cos(tilt);
    d += `${deg === 0 ? "M" : " L"}${x.toFixed(2)} ${y.toFixed(2)}`;
  }
  // DESIGN.md widths are at a 56px square, in px here: 0.8 of the board pen.
  const outline = width > 0 ? strokeOutline(d, 56 * 0.8, { minWidth: 1.1 }) : "";
  return (
    <span ref={host} className="relative inline-block">
      {word}
      {outline ? (
        <svg
          aria-hidden
          data-sheet-ring
          className="pointer-events-none absolute overflow-visible"
          style={{ left: width / 2 - cx, top: `calc(50% - ${cy}px)` }}
          width={2 * cx}
          height={2 * cy}
        >
          <mask id={`${id}-m`} maskUnits="userSpaceOnUse">
            <path
              ref={reveal}
              d={d}
              pathLength={1}
              fill="none"
              stroke="white"
              strokeWidth={8}
              strokeLinecap="round"
              strokeDasharray="1 1"
              style={{ strokeDashoffset: draw && instant ? 0 : 1 }}
            />
          </mask>
          <g mask={`url(#${id}-m)`}>
            <path
              d={outline}
              className="fill-sheet stroke-sheet"
              strokeWidth={4}
              strokeLinejoin="round"
            />
            <path d={outline} className="fill-pen" />
          </g>
        </svg>
      ) : null}
    </span>
  );
}

function Cell({ ply, ring }: { ply?: Ply; ring?: { draw: boolean; instant: boolean } }) {
  if (!ply) return <td className="border-l border-rule pl-5" />;
  const mine = ply.color === game.playerColor;
  const glyph = ply.judgement ? judgementGlyph[ply.judgement] : "";
  const word = (
    <>
      {ply.san}
      {glyph ? (
        <span className={`tracking-(--tracking-glyph) ${mine ? "text-pen-text" : "text-pencil"}`}>
          {glyph}
        </span>
      ) : null}
    </>
  );
  return (
    <td className="border-l border-rule pl-5 whitespace-nowrap">
      {ring ? <SheetRing word={`${ply.san}${glyph}`} {...ring} /> : word}
      {ply.judgement ? (
        <span className="ml-5 font-normal text-ink-faint">{formatEval(scoreOf(ply))}</span>
      ) : null}
    </td>
  );
}

export function HeroSection() {
  const [run, setRun] = useState(0);
  const graphRef = useRef<HTMLDivElement>(null);
  const { step, prologue, reduced } = useBeat(run, graphRef);
  const [plot, setPlot] = useState<number | undefined>(undefined);

  // From 1024px the graph fills the right column down to the board's bottom (--hero-graph-height),
  // never below its 120px plot; elsewhere it keeps its real sizes.
  useLayoutEffect(() => {
    const el = graphRef.current;
    if (!el) return;
    const wide = window.matchMedia("(width >= 1024px)");
    const measure = () => {
      if (!wide.matches) return setPlot(undefined);
      const lanes = 22 + 18;
      setPlot(Math.max(120, Math.floor(el.getBoundingClientRect().height) - lanes));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    wide.addEventListener("change", measure);
    return () => {
      observer.disconnect();
      wide.removeEventListener("change", measure);
    };
  }, []);

  const played = atLeast(step, "played");
  const shown = played ? keyPly : prevPly;
  const ring = { draw: atLeast(step, "marked"), instant: reduced };

  return (
    <section
      id="hero"
      data-section="hero"
      aria-labelledby="hero-title"
      className="grid grid-cols-1 gap-x-(--space-gutter) gap-y-6 lg:grid-cols-[auto_minmax(0,1fr)] lg:grid-rows-[auto_minmax(0,1fr)_auto]"
    >
      <header className="flex flex-col gap-3 lg:col-start-2 lg:row-start-1">
        <h1 id="hero-title" className="font-serif text-3xl font-semibold text-balance">
          {title}
        </h1>
        <p className="max-w-prose text-ink-soft">{context}</p>
        <div data-lab-chrome>
          <Button variant="quiet" size="sm" className="-ml-3" onClick={() => setRun((n) => n + 1)}>
            <Icon icon={ArrowCounterClockwiseIcon} />
            Play the moment again
          </Button>
        </div>
      </header>

      <div className="flex w-fit flex-col gap-2 md:flex-row lg:col-start-1 lg:row-span-2 lg:row-start-1">
        <EvalBar
          score={scoreOf(shown)}
          fen={shown.fenAfter}
          label={`Evaluation after ${played ? keyLabel : "27… Rf8"}`}
          className="md:my-2"
        />
        <Board
          key={run}
          fen={shown.fenAfter}
          label={`${title}, after ${played ? keyLabel : `${moveNo(prevPly.ply)}… ${prevPly.san}`}`}
          lastMove={{ from: sq(shown.uci, 0), to: sq(shown.uci, 2) }}
          annotations={atLeast(step, "marked") ? marks : []}
          animateAnnotations={!reduced}
        />
      </div>

      <div
        data-surface="sheet"
        className="grid grid-cols-1 gap-y-5 rounded-sheet bg-sheet p-(--space-panel-pad) lg:col-span-2 lg:row-start-3 lg:grid-cols-subgrid"
      >
        <table className="w-full max-w-(--board-size) font-serif text-base font-semibold tabular-nums lining-nums lg:max-w-none">
          <caption className="sr-only">
            Moves {rows[0]?.n} to {rows.at(-1)?.n}
          </caption>
          <tbody>
            {rows.map((row) => (
              <tr
                key={row.n}
                className="h-(--space-scoresheet-row) border-b border-rule last:border-b-0"
              >
                <th scope="row" className="w-10 pr-3 text-left font-normal text-ink-faint sm:w-14">
                  {row.n}
                </th>
                <Cell ply={row.white} ring={row.white?.ply === keyPly.ply ? ring : undefined} />
                <Cell ply={row.black} ring={row.black?.ply === keyPly.ply ? ring : undefined} />
              </tr>
            ))}
          </tbody>
        </table>
        <p
          data-comment
          className={`max-w-[52ch] self-start font-serif-italic text-prose text-pretty text-ink italic transition-opacity duration-(--dur-base) ease-out motion-reduce:transition-none lg:pt-0.5 ${atLeast(step, "noted") ? "opacity-100" : "opacity-0 motion-reduce:opacity-100"}`}
        >
          {note}
        </p>
      </div>

      <div
        ref={graphRef}
        className="flex min-h-0 min-w-0 flex-col justify-end lg:col-start-2 lg:row-start-2"
        style={{ "--eval-graph-surface": "var(--color-paper)" } as React.CSSProperties}
      >
        <EvalGraph
          key={`${run}-${prologue}`}
          points={points}
          label={`Win chance, ${title}`}
          currentPly={shown.ply}
          playerColor={game.playerColor}
          drawIn={prologue && !reduced}
          {...(plot ? { height: plot } : {})}
        />
      </div>
    </section>
  );
}
