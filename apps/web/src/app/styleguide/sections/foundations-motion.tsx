"use client";

import { games } from "@ci/ui/fixtures";
import {
  animate,
  arrivalTime,
  dampingRatio,
  dur,
  ease,
  overshoot,
  type PieceType,
  pieceWeight,
  type Spring,
  settleFor,
  spring,
  stagger,
  useReducedMotion,
} from "@ci/ui/motion";
import {
  type ReactNode,
  type Ref,
  type RefObject,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { type PlaceholderPiece, PlaceholderPieceSvg } from "./placeholder-pieces";

// The motion tokens, each shown doing its job: pieces lift, travel on their own settle and land
// with their squash; strokes draw like the pen; the eval bar swings. Under reduced motion every
// demo jumps to its end state with one opacity cue (M10).

type Controls = { stop: () => void };
type Player = (reduced: boolean) => Controls[] | Promise<Controls[]>;

/** Replays `run` on its own Play button and whenever "Play all" ticks. */
function usePlayer(all: number, run: Player) {
  const reduced = useReducedMotion();
  const running = useRef<Controls[]>([]);
  const runRef = useRef(run);
  runRef.current = run;
  const play = useCallback(async () => {
    for (const c of running.current) c.stop();
    running.current = await runRef.current(reduced);
  }, [reduced]);
  useEffect(() => {
    if (all > 0) void play();
  }, [all, play]);
  useEffect(
    () => () => {
      for (const c of running.current) c.stop();
    },
    [],
  );
  return play;
}

const wait = (seconds: number) => new Promise((resolve) => setTimeout(resolve, seconds * 1000));

const zetaLabel = (s: Spring) => {
  const shoot = overshoot(s) * 100;
  const pct = shoot < 0.01 ? "0" : shoot < 1 ? shoot.toFixed(2) : shoot.toFixed(0);
  return `ζ ${dampingRatio(s).toFixed(2)}, overshoot ${pct}%`;
};

const springValues = (s: Spring) => `stiffness ${s.stiffness}, damping ${s.damping}`;

function Row({
  title,
  detail,
  use,
  onPlay,
  children,
}: {
  title: string;
  detail: string;
  use: string;
  onPlay: () => void;
  children: ReactNode;
}) {
  return (
    <li className="flex flex-col gap-3 rounded-field px-3 py-3 odd:bg-well lg:flex-row lg:items-center lg:gap-6">
      <span className="shrink-0 lg:w-96">
        <span className="block text-sm font-medium">{title}</span>
        <span className="block text-xs text-ink-soft">{detail}</span>
        <span className="block text-xs text-ink-faint">{use}</span>
      </span>
      <span className="flex min-w-0 items-center gap-4 lg:flex-1">
        <span className="min-w-0 flex-1">{children}</span>
        <PlayButton onClick={onPlay} label={`Play ${title}`} />
      </span>
    </li>
  );
}

function PlayButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="shrink-0 rounded-field border border-rule-strong bg-sheet px-3 py-1 text-sm font-medium text-ink hover:bg-paper active:scale-97"
    >
      Play
    </button>
  );
}

/* Pieces ------------------------------------------------------------------------------------- */

const placeholderFor: Record<PieceType, PlaceholderPiece> = {
  pawn: "pawn",
  knight: "bishop",
  bishop: "bishop",
  rook: "rook",
  queen: "queen",
  king: "queen",
};

/** Lift, travel on the piece's settle, land with its squash (DESIGN.md "Physical feel"). */
async function playMove(
  el: HTMLElement,
  distance: number,
  piece: PieceType,
  reduced: boolean,
): Promise<Controls[]> {
  const weight = pieceWeight[piece];
  if (reduced) {
    return [
      animate(
        el,
        { x: distance, y: 0, scale: 1, opacity: [0, 1] },
        { duration: 0, opacity: { duration: dur.quick } },
      ),
    ];
  }
  animate(el, { x: 0, y: 0, scale: 1, scaleX: 1, scaleY: 1 }, { duration: 0 });
  const lift = animate(
    el,
    { y: -4, scale: weight.lift },
    { duration: dur.instant, ease: ease.out },
  );
  await lift;
  const travel = animate(el, { x: distance }, settleFor(piece));
  // Contact: when the piece is within 1% of the square, it is set down at its squash.
  await wait(arrivalTime(settleFor(piece), 0.99));
  animate(el, { y: 0, scale: 1 }, { duration: 0 });
  const land = animate(
    el,
    { scaleX: [weight.squash.scaleX, 1], scaleY: [weight.squash.scaleY, 1] },
    spring.land,
  );
  return [travel, land];
}

function BoardStrip({
  squares,
  pieceRef,
  piece,
  start = 0,
  badgeRef,
}: {
  squares: number;
  pieceRef: Ref<HTMLDivElement>;
  piece: PlaceholderPiece;
  start?: number;
  badgeRef?: Ref<HTMLSpanElement>;
}) {
  return (
    <div
      className={`relative overflow-visible ${squares > 4 ? "max-w-xl" : "max-w-60"}`}
      aria-hidden
    >
      <div className="flex overflow-hidden rounded-board">
        {Array.from({ length: squares }, (_, i) => (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed row of squares
            key={i}
            className={`aspect-square flex-1 ${i % 2 === 0 ? "bg-board-dark" : "bg-board-light"}`}
          />
        ))}
      </div>
      <div
        ref={pieceRef}
        className="absolute top-0 left-0 h-full p-0.5"
        // The landing squash pivots on the piece's base (DESIGN.md "Physical feel").
        style={{
          width: `${100 / squares}%`,
          left: `${(100 / squares) * start}%`,
          transformOrigin: "50% 90%",
        }}
      >
        <PlaceholderPieceSvg piece={piece} />
        {badgeRef && (
          <span
            ref={badgeRef}
            className="absolute -top-1 -right-1 grid size-5 place-items-center rounded-pill bg-pen font-serif text-2xs font-semibold text-on-ink ring-2 ring-sheet"
          >
            ?
          </span>
        )}
      </div>
    </div>
  );
}

const ladder: { piece: PieceType; label: string }[] = [
  { piece: "pawn", label: "Pawn" },
  { piece: "knight", label: "Knight, bishop" },
  { piece: "rook", label: "Rook" },
  { piece: "queen", label: "Queen, king" },
];

/** All four weights race the same seven squares, so heavier visibly means slower. */
function WeightRace({ all }: { all: number }) {
  const lanes = useRef<(HTMLDivElement | null)[]>([]);
  const play = usePlayer(all, (reduced) =>
    Promise.all(
      ladder.map(({ piece }, i) => {
        const el = lanes.current[i];
        if (!el?.parentElement) return [];
        return playMove(el, (el.parentElement.clientWidth / 8) * 7, piece, reduced);
      }),
    ).then((sets) => sets.flat()),
  );
  return (
    <div>
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <h4 className="font-serif text-lg font-semibold">Weight</h4>
        <PlayButton onClick={play} label="Play the weight race" />
      </div>
      <p className="mt-1 max-w-prose text-sm text-ink-soft">
        Each piece lifts, travels seven squares on its own settle and lands with its squash. All
        share ζ 0.92, so heavier only means slower: none overtakes a lighter one.
      </p>
      <ul className="mt-4 flex flex-col gap-3">
        {ladder.map(({ piece, label }, i) => {
          const s = settleFor(piece);
          const w = pieceWeight[piece];
          return (
            <li key={piece} className="flex flex-col gap-2 md:flex-row md:items-center md:gap-6">
              <span className="shrink-0 md:w-72">
                <span className="block text-sm font-medium">
                  {label}{" "}
                  <span className="font-normal text-ink-faint">mass factor {w.massFactor}</span>
                </span>
                <span className="block text-xs text-ink-soft">
                  {springValues(s)}; 90% at {Math.round(arrivalTime(s, 0.9) * 1000)}ms
                </span>
                <span className="block text-xs text-ink-faint">
                  lift {w.lift}, squash {w.squash.scaleX} / {w.squash.scaleY}
                </span>
              </span>
              <span className="min-w-0 flex-1">
                <BoardStrip
                  squares={8}
                  piece={placeholderFor[piece]}
                  pieceRef={(el) => {
                    lanes.current[i] = el;
                  }}
                />
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function SettleRow({ all }: { all: number }) {
  const piece = useRef<HTMLDivElement>(null);
  const play = usePlayer(all, async (reduced) => {
    const el = piece.current;
    if (!el?.parentElement) return [];
    return playMove(el, (el.parentElement.clientWidth / 4) * 3, "knight", reduced);
  });
  return (
    <Row
      title="settle"
      detail={`${springValues(spring.settle)}; ${zetaLabel(spring.settle)}`}
      use="piece travel, here a bishop: lift, travel three squares, land"
      onPlay={play}
    >
      <BoardStrip squares={4} piece="bishop" pieceRef={piece} />
    </Row>
  );
}

function LandRow({ all }: { all: number }) {
  const piece = useRef<HTMLDivElement>(null);
  const badge = useRef<HTMLSpanElement>(null);
  const play = usePlayer(all, async (reduced) => {
    const el = piece.current;
    const b = badge.current;
    if (!el || !b) return [];
    if (reduced) return [animate(b, { scale: 1, opacity: [0, 1] }, { duration: dur.quick })];
    const w = pieceWeight.rook;
    animate(b, { scale: 0 }, { duration: 0 });
    await animate(el, { y: -4, scale: w.lift }, { duration: dur.instant, ease: ease.out });
    await wait(0.12);
    animate(el, { y: 0, scale: 1 }, { duration: 0 });
    const land = animate(
      el,
      { scaleX: [w.squash.scaleX, 1], scaleY: [w.squash.scaleY, 1] },
      spring.land,
    );
    await wait(0.18);
    return [land, animate(b, { scale: [0, 1] }, spring.land)];
  });
  return (
    <Row
      title="land"
      detail={`${springValues(spring.land)}; ${zetaLabel(spring.land)}`}
      use="a rook set down with its squash, then a glyph badge landing"
      onPlay={play}
    >
      <BoardStrip squares={4} start={1} piece="rook" pieceRef={piece} badgeRef={badge} />
    </Row>
  );
}

function TiltRow({ all }: { all: number }) {
  const piece = useRef<HTMLDivElement>(null);
  const play = usePlayer(all, (reduced) => {
    const el = piece.current;
    if (!el) return [];
    if (reduced) return [animate(el, { rotate: 0, opacity: [0, 1] }, { duration: dur.quick })];
    return [animate(el, { rotate: [8, 0] }, spring.tilt)];
  });
  return (
    <Row
      title="tilt"
      detail={`${springValues(spring.tilt)}; ${zetaLabel(spring.tilt)}`}
      use="a dragged piece at full tilt (8°) returning upright as the pointer slows"
      onPlay={play}
    >
      <BoardStrip squares={4} start={1} piece="queen" pieceRef={piece} />
    </Row>
  );
}

/* Eval bar, page, pen ------------------------------------------------------------------------ */

function SwingRow({
  name,
  s,
  from,
  to,
  use,
  all,
}: {
  name: string;
  s: Spring;
  from: number;
  to: number;
  use: string;
  all: number;
}) {
  const fill = useRef<HTMLSpanElement>(null);
  const play = usePlayer(all, (reduced) => {
    const el = fill.current;
    if (!el) return [];
    if (reduced)
      return [
        animate(
          el,
          { scaleX: to, opacity: [0, 1] },
          { duration: 0, opacity: { duration: dur.quick } },
        ),
      ];
    return [animate(el, { scaleX: [from, to] }, s)];
  });
  return (
    <Row title={name} detail={`${springValues(s)}; ${zetaLabel(s)}`} use={use} onPlay={play}>
      <span
        className="relative block h-4 max-w-xl overflow-hidden rounded-board border border-rule-strong bg-sheet"
        aria-hidden
      >
        <span
          ref={fill}
          className="absolute inset-0 origin-left bg-ink"
          style={{ transform: `scaleX(${to})` }}
        />
      </span>
      <span className="relative block h-2 max-w-xl" aria-hidden>
        <span className="absolute top-0 h-2 w-0.5 bg-ink-soft" style={{ left: `${to * 100}%` }} />
      </span>
    </Row>
  );
}

function PageRow({ all }: { all: number }) {
  const sheet = useRef<HTMLSpanElement>(null);
  const play = usePlayer(all, (reduced) => {
    const el = sheet.current;
    if (!el) return [];
    if (reduced) return [animate(el, { opacity: [0, 1] }, { duration: dur.quick })];
    return [
      animate(
        el,
        { rotateY: [14, 0], x: [24, 0], opacity: [0, 1], transformPerspective: 1600 },
        { ...spring.page, opacity: { duration: dur.quick, ease: ease.out } },
      ),
    ];
  });
  return (
    <Row
      title="page"
      detail={`${springValues(spring.page)}; ${zetaLabel(spring.page)}`}
      use="the incoming page of a page turn, from its right edge"
      onPlay={play}
    >
      <span className="block max-w-xl rounded-field bg-paper p-2" aria-hidden>
        <span ref={sheet} className="block origin-right rounded-field bg-sheet p-3 shadow-lift">
          <span className="block h-2 w-2/3 rounded-pill bg-ink-soft" />
          <span className="mt-2 block h-2 w-1/2 rounded-pill bg-rule" />
        </span>
      </span>
    </Row>
  );
}

function PenRow({ all }: { all: number }) {
  const line = useRef<SVGPathElement>(null);
  const play = usePlayer(all, (reduced) => {
    const el = line.current;
    if (!el) return [];
    if (reduced)
      return [
        animate(
          el,
          { strokeDashoffset: 0, opacity: [0, 1] },
          { duration: 0, opacity: { duration: dur.quick } },
        ),
      ];
    return [animate(el, { strokeDashoffset: [1, 0] }, spring.pen)];
  });
  return (
    <Row
      title="pen"
      detail={`${springValues(spring.pen)}; ${zetaLabel(spring.pen)}`}
      use="stroke progress that follows input mid-draw"
      onPlay={play}
    >
      <svg viewBox="0 0 300 24" className="block h-6 w-full max-w-xl" aria-hidden>
        <path
          d="M6 16 Q150 4 294 12"
          className="stroke-sheet"
          strokeWidth={7}
          fill="none"
          strokeLinecap="round"
        />
        <path
          ref={line}
          d="M6 16 Q150 4 294 12"
          pathLength={1}
          strokeDasharray="1 1"
          className="stroke-pen"
          strokeWidth={3}
          fill="none"
          strokeLinecap="round"
        />
      </svg>
    </Row>
  );
}

/* Strokes ------------------------------------------------------------------------------------ */

/**
 * A pen ring per DESIGN.md "Pen stroke geometry" (tilted ellipse, ±6% drift, 380° spiral), at a
 * constant width. PLACEHOLDER for the annotations lane's variable-width outline.
 */
function ringPath(size: number): string {
  const c = size / 2;
  const ry = 0.38 * size;
  const rx = 1.12 * ry;
  const tilt = (10 * Math.PI) / 180;
  const grow = (2.5 * size) / 56;
  const points: string[] = [];
  for (let d = 0; d <= 380; d += 4) {
    const t = ((-60 - d) * Math.PI) / 180;
    const drift = 1 + 0.04 * Math.sin(2 * t + 0.9) + 0.02 * Math.sin(3 * t + 2.3);
    const g = (d / 360) * grow;
    const x = (rx * drift + g) * Math.cos(t);
    const y = (ry * drift + g) * Math.sin(t);
    const px = c + x * Math.cos(tilt) - y * Math.sin(tilt);
    const py = c + x * Math.sin(tilt) + y * Math.cos(tilt);
    points.push(`${px.toFixed(2)} ${py.toFixed(2)}`);
  }
  return `M${points.join(" L")}`;
}

const RING = ringPath(56);

function PenRing({ strokes }: { strokes: RefObject<(SVGPathElement | null)[]> }) {
  return (
    <span className="relative block size-14 shrink-0 bg-board-light" aria-hidden>
      <span className="absolute inset-1.5 block">
        <PlaceholderPieceSvg piece="pawn" />
      </span>
      <svg viewBox="0 0 56 56" className="absolute inset-0 size-full overflow-visible" aria-hidden>
        {(["stroke-sheet", "stroke-pen"] as const).map((cls, i) => (
          <path
            key={cls}
            ref={(el) => {
              strokes.current[i] = el;
            }}
            d={RING}
            pathLength={1}
            strokeDasharray="1 1"
            className={cls}
            strokeWidth={i === 0 ? 6.6 : 2.6}
            fill="none"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ))}
      </svg>
    </span>
  );
}

function MarkerTrack({ marker }: { marker: RefObject<HTMLSpanElement | null> }) {
  return (
    <span className="relative block h-6 max-w-xl" aria-hidden>
      <span className="absolute inset-x-0 top-1/2 h-0.5 rounded-pill bg-rule-strong" />
      <span className="absolute top-0 left-4/5 h-6 w-0.5 bg-ink-soft" />
      <span
        ref={marker}
        className="absolute top-1/2 left-0 -mt-2 block size-4 rounded-pill bg-ink"
      />
    </span>
  );
}

const durationRows: {
  token: keyof typeof dur;
  name: string;
  curve: keyof typeof ease;
  use: string;
  demo: "marker" | "ring" | "graph" | "soak";
}[] = [
  {
    token: "instant",
    name: "instant",
    curve: "out",
    use: "press, arrowhead stroke, lift",
    demo: "marker",
  },
  {
    token: "quick",
    name: "quick",
    curve: "out",
    use: "menus, toggles, retracting marks",
    demo: "marker",
  },
  {
    token: "base",
    name: "base",
    curve: "out",
    use: "panels, drawers, a captured piece",
    demo: "marker",
  },
  { token: "drawSmall", name: "draw-small", curve: "pen", use: "a small stroke", demo: "ring" },
  { token: "draw", name: "draw", curve: "pen", use: "one board stroke: the ring", demo: "ring" },
  {
    token: "reveal",
    name: "reveal",
    curve: "pen",
    use: "the eval graph drawing across",
    demo: "graph",
  },
  { token: "turn", name: "turn", curve: "turn", use: "a page turn", demo: "marker" },
  { token: "stream", name: "stream", curve: "out", use: "one replayed move", demo: "marker" },
  { token: "soak", name: "soak", curve: "soak", use: "an ink-soak progress segment", demo: "soak" },
];

// A real analysed game (fixture middlegame-blunder): White's win chance after every ply.
const graphGame = games.middlegameBlunder;
const GRAPH = (() => {
  const plies = graphGame.plies;
  return plies
    .map((ply, i) => {
      const x = (i / Math.max(1, plies.length - 1)) * 192;
      const y = (1 - ply.winWhite / 100) * 34;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`;
    })
    .join(" ");
})();

function DurationRow({ row, all }: { row: (typeof durationRows)[number]; all: number }) {
  const seconds = dur[row.token];
  const marker = useRef<HTMLSpanElement>(null);
  const strokes = useRef<(SVGPathElement | null)[]>([]);
  const graph = useRef<SVGPathElement>(null);
  const soak = useRef<HTMLSpanElement>(null);
  const options = useMemo(
    () => ({ duration: seconds, ease: ease[row.curve] }),
    [seconds, row.curve],
  );
  const play = usePlayer(all, (reduced) => {
    const cue = { duration: 0, opacity: { duration: dur.quick } };
    if (row.demo === "ring") {
      const els = strokes.current.filter((el): el is SVGPathElement => el !== null);
      return els.map((el) =>
        animate(
          el,
          { strokeDashoffset: reduced ? 0 : [1, 0], opacity: reduced ? [0, 1] : 1 },
          reduced ? cue : options,
        ),
      );
    }
    if (row.demo === "graph" && graph.current)
      return [
        animate(
          graph.current,
          { strokeDashoffset: reduced ? 0 : [1, 0], opacity: reduced ? [0, 1] : 1 },
          reduced ? cue : options,
        ),
      ];
    if (row.demo === "soak" && soak.current)
      return [
        animate(
          soak.current,
          { scaleX: reduced ? 0.6 : [0.25, 0.6], opacity: reduced ? [0, 1] : 1 },
          reduced ? cue : options,
        ),
      ];
    const el = marker.current;
    if (!el?.parentElement) return [];
    const distance = el.parentElement.clientWidth * 0.8 - el.offsetWidth / 2;
    return [
      animate(
        el,
        { x: reduced ? distance : [0, distance], opacity: reduced ? [0, 1] : 1 },
        reduced ? cue : options,
      ),
    ];
  });
  return (
    <Row
      title={`--dur-${row.name}`}
      detail={`${Math.round(seconds * 1000)}ms on --ease-${row.curve}`}
      use={row.use}
      onPlay={play}
    >
      {row.demo === "ring" ? (
        <PenRing strokes={strokes} />
      ) : row.demo === "graph" ? (
        <svg
          viewBox="0 0 192 34"
          className="block h-12 w-full max-w-xl rounded-board bg-well"
          aria-hidden
        >
          <path d="M0 17 H192" className="stroke-rule-strong" strokeWidth={0.75} />
          <path
            ref={graph}
            d={GRAPH}
            pathLength={1}
            strokeDasharray="1 1"
            className="stroke-ink"
            strokeWidth={1.5}
            fill="none"
            strokeLinejoin="round"
          />
        </svg>
      ) : row.demo === "soak" ? (
        <span className="block h-2 max-w-xl overflow-hidden rounded-pill bg-well" aria-hidden>
          <span
            ref={soak}
            className="block h-full origin-left bg-ink"
            style={{ transform: "scaleX(0.6)" }}
          />
        </span>
      ) : (
        <MarkerTrack marker={marker} />
      )}
    </Row>
  );
}

function EaseCurve({ name }: { name: keyof typeof ease }) {
  const [x1, y1, x2, y2] = ease[name];
  const d = `M0 1 C${x1} ${1 - y1} ${x2} ${1 - y2} 1 0`;
  return (
    <li className="flex items-center gap-3">
      <svg
        viewBox="-0.05 -0.05 1.1 1.1"
        className="size-16 shrink-0 rounded-field bg-paper"
        aria-hidden
      >
        <path d="M0 1 L1 0" className="stroke-rule" strokeWidth={0.01} fill="none" />
        <path d={d} className="stroke-ink" strokeWidth={0.03} fill="none" strokeLinecap="round" />
      </svg>
      <span>
        <span className="block text-sm font-medium">--ease-{name}</span>
        <span className="block text-xs text-ink-faint">cubic-bezier({ease[name].join(", ")})</span>
      </span>
    </li>
  );
}

function StaggerRow({ name, all }: { name: keyof typeof stagger; all: number }) {
  const seconds = stagger[name];
  const ticks = useRef<(HTMLSpanElement | null)[]>([]);
  const play = usePlayer(all, (reduced) =>
    ticks.current
      .filter((t): t is HTMLSpanElement => t !== null)
      .map((tick, i) =>
        animate(
          tick,
          { opacity: [0, 1] },
          { duration: dur.quick, delay: reduced ? 0 : i * seconds },
        ),
      ),
  );
  return (
    <li className="flex items-center gap-4 rounded-field px-3 py-3 odd:bg-well">
      <span className="w-40 shrink-0 text-sm font-medium">
        --stagger-{name}{" "}
        <span className="font-normal text-ink-faint">{Math.round(seconds * 1000)}ms</span>
      </span>
      <span className="flex min-w-0 flex-1 gap-1.5" aria-hidden>
        {Array.from({ length: 8 }, (_, i) => (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed row of identical ticks
            key={i}
            ref={(el) => {
              ticks.current[i] = el;
            }}
            className="h-5 w-1.5 rounded-board bg-ink-soft"
          />
        ))}
      </span>
      <PlayButton onClick={play} label={`Play --stagger-${name}`} />
    </li>
  );
}

export function MotionTokens() {
  const [all, setAll] = useState(0);
  const reduced = useReducedMotion();
  return (
    <div className="flex flex-col gap-10">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="max-w-prose text-sm text-ink-soft">
          Every value here is the token itself, playing the job it was made for.
          {reduced &&
            " Your system asks for reduced motion, so each demo shows its end state with a fade instead."}
        </p>
        <button
          type="button"
          onClick={() => setAll((n) => n + 1)}
          className="rounded-field bg-ink px-4 py-2 text-sm font-medium text-on-ink hover:bg-ink-soft active:scale-97"
        >
          Play all
        </button>
      </div>
      <WeightRace all={all} />
      <div>
        <h4 className="font-serif text-lg font-semibold">Springs</h4>
        <ul className="mt-3 flex flex-col">
          <SettleRow all={all} />
          <LandRow all={all} />
          <TiltRow all={all} />
          <SwingRow
            name="swing"
            s={spring.swing}
            from={0.48}
            to={0.6}
            use="eval bar and graph cursor, changes below 0.3 win chance"
            all={all}
          />
          <SwingRow
            name="swing-big"
            s={spring.swingBig}
            from={0.25}
            to={0.8}
            use="eval bar, changes of 0.3 or more: it swings past and back"
            all={all}
          />
          <PageRow all={all} />
          <PenRow all={all} />
        </ul>
      </div>
      <div>
        <h4 className="font-serif text-lg font-semibold">Durations</h4>
        <ul className="mt-3 flex flex-col">
          {durationRows.map((row) => (
            <DurationRow key={row.token} row={row} all={all} />
          ))}
        </ul>
      </div>
      <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
        <div>
          <h4 className="font-serif text-lg font-semibold">Easings</h4>
          <ul className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
            {(Object.keys(ease) as (keyof typeof ease)[]).map((name) => (
              <EaseCurve key={name} name={name} />
            ))}
          </ul>
        </div>
        <div>
          <h4 className="font-serif text-lg font-semibold">Staggers</h4>
          <ul className="mt-3 flex flex-col">
            {(Object.keys(stagger) as (keyof typeof stagger)[]).map((name) => (
              <StaggerRow key={name} name={name} all={all} />
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}
