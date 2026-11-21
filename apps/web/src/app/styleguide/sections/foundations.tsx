"use client";

import {
  animate,
  dampingRatio,
  dur,
  ease,
  overshoot,
  type Spring,
  settleFor,
  spring,
  stagger,
  useReducedMotion,
} from "@ci/ui/motion";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

// Every value on this section is read from tokens.css at run time and every ratio is computed in
// the browser, so the page shows what ships, not a copy of DESIGN.md.

export function FoundationsSection() {
  return (
    <section
      id="foundations"
      data-section="foundations"
      aria-labelledby="foundations-title"
      className="flex flex-col gap-6"
    >
      <div className="max-w-prose">
        <h2 id="foundations-title" className="font-serif text-3xl font-semibold">
          Foundations
        </h2>
        <p className="mt-2 text-ink-soft">
          Colour, type, space and motion, read live from the tokens. Contrast is computed in your
          browser against the surface named beside each ratio.
        </p>
      </div>
      <Panel title="Colour">
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <ColourPanel theme="light" label="Day" />
          <ColourPanel theme="dark" label="Night study" />
        </div>
      </Panel>
      <Panel title="Type">
        <TypeScale />
      </Panel>
      <Panel title="Space and shape">
        <SpaceAndShape />
      </Panel>
      <Panel title="Motion">
        <MotionTokens />
      </Panel>
    </section>
  );
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="rounded-sheet bg-sheet p-(--space-panel-pad)">
      <h3 className="mb-4 font-serif text-xl font-semibold">{title}</h3>
      {children}
    </div>
  );
}

/* Colour ------------------------------------------------------------------------------------- */

type Rgb = [number, number, number, number];

function parseColour(value: string): Rgb | undefined {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex?.[1]) {
    const n = Number.parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)\s*(?:[/,]\s*([\d.]+))?\s*\)$/.exec(value);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), Number(rgb[4] ?? 1)];
  return undefined;
}

function over(top: Rgb, bottom: Rgb): Rgb {
  const mix = (i: 0 | 1 | 2) => Math.round(top[i] * top[3] + bottom[i] * (1 - top[3]));
  return [mix(0), mix(1), mix(2), 1];
}

function luminance([r, g, b]: Rgb): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(fg: Rgb, bg: Rgb): number {
  const a = luminance(over(fg, bg));
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

type Theme = "light" | "dark";

/** What a token is checked against. `fg` and `bg` name tokens, or pick one per theme. */
interface Check {
  fg: string | Record<Theme, string>;
  bg: string[];
  min: number;
  /** composite this alpha token over `bg` and check `fg` on the result */
  under?: string;
}

const text = (token: string): Check => ({
  fg: token,
  bg: ["paper", "sheet", "well"],
  min: 4.5,
});
const stroke = (token: string): Check => ({ fg: token, bg: ["sheet"], min: 3 });

const colourGroups: { title: string; tokens: [string, Check | string][] }[] = [
  {
    title: "Surfaces and lines",
    tokens: [
      ["paper", { fg: "ink", bg: ["paper"], min: 4.5 }],
      ["sheet", { fg: "ink", bg: ["sheet"], min: 4.5 }],
      ["well", { fg: "ink", bg: ["well"], min: 4.5 }],
      ["rule", "decorative, no minimum"],
      ["rule-strong", { fg: "rule-strong", bg: ["paper", "sheet", "well"], min: 3 }],
    ],
  },
  {
    title: "Ink",
    tokens: [
      ["ink", text("ink")],
      ["ink-soft", text("ink-soft")],
      ["ink-faint", text("ink-faint")],
      ["on-ink", { fg: "on-ink", bg: ["ink"], min: 4.5 }],
    ],
  },
  {
    title: "Marks",
    tokens: [
      ["pen", stroke("pen")],
      ["pen-text", text("pen-text")],
      ["good", stroke("good")],
      ["good-text", text("good-text")],
      ["pencil", stroke("pencil")],
    ],
  },
  {
    title: "Board and pieces",
    tokens: [
      ["board-light", { fg: { light: "ink", dark: "on-ink" }, bg: ["board-light"], min: 4.5 }],
      ["board-dark", { fg: "ink", bg: ["board-dark"], min: 4.5 }],
      ["piece-dark", { fg: "piece-dark", bg: ["board-light", "board-dark"], min: 3 }],
      ["piece-light", { fg: "piece-light", bg: ["piece-dark"], min: 3 }],
      ["board-lastmove", "supplementary tint"],
      ["board-select", "supplementary tint"],
      ["board-premove", "supplementary tint"],
      ["selection", { fg: "ink", bg: ["sheet", "paper"], min: 4.5, under: "selection" }],
    ],
  },
];

const allColourTokens = colourGroups.flatMap((group) => group.tokens.map(([token]) => token));

/** Reads custom properties from an element after mount. */
function useCustomProperties(names: string[]) {
  const ref = useRef<HTMLDivElement>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const key = names.join(",");
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const read = () => {
      const style = getComputedStyle(el);
      setValues(
        Object.fromEntries(key.split(",").map((n) => [n, style.getPropertyValue(n).trim()])),
      );
    };
    read();
    window.addEventListener("resize", read);
    return () => window.removeEventListener("resize", read);
  }, [key]);
  return [ref, values] as const;
}

function ColourPanel({ theme, label }: { theme: Theme; label: string }) {
  const [ref, values] = useCustomProperties(allColourTokens.map((t) => `--color-${t}`));
  const colour = (token: string) => parseColour(values[`--color-${token}`] ?? "");

  function measure(check: Check) {
    const fgToken = typeof check.fg === "string" ? check.fg : check.fg[theme];
    const fg = colour(fgToken);
    let lowest: { ratio: number; bg: string } | undefined;
    for (const bgToken of check.bg) {
      const surface = colour(bgToken);
      const tint = check.under ? colour(check.under) : undefined;
      if (!fg || !surface || (check.under && !tint)) return undefined;
      const bg = tint ? over(tint, surface) : surface;
      const ratio = contrast(fg, bg);
      if (!lowest || ratio < lowest.ratio) lowest = { ratio, bg: bgToken };
    }
    return lowest && { ...lowest, fg: fgToken };
  }

  return (
    <div
      ref={ref}
      data-theme={theme}
      className="rounded-sheet bg-paper p-(--space-panel-pad) text-ink"
    >
      <h4 className="font-serif text-lg font-semibold">{label}</h4>
      <div className="mt-3 flex flex-col gap-5">
        {colourGroups.map((group) => (
          <div key={group.title}>
            <p className="text-sm font-medium text-ink-soft">{group.title}</p>
            <ul className="mt-2 flex flex-col">
              {group.tokens.map(([token, check]) => {
                const result = typeof check === "string" ? undefined : measure(check);
                return (
                  <li
                    key={token}
                    className="flex items-center gap-3 border-b border-rule py-2 last:border-b-0"
                  >
                    <span
                      aria-hidden
                      className="size-8 shrink-0 rounded-field border border-rule"
                      style={{ backgroundColor: `var(--color-${token})` }}
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block text-sm font-medium">{token}</span>
                      <span className="block text-xs text-ink-faint">
                        {values[`--color-${token}`] || " "}
                      </span>
                    </span>
                    <span className="text-right text-xs">
                      {typeof check === "string" ? (
                        <span className="text-ink-faint">{check}</span>
                      ) : result && typeof check !== "string" ? (
                        <ContrastLabel {...result} check={check} token={token} />
                      ) : (
                        " "
                      )}
                    </span>
                  </li>
                );
              })}
            </ul>
          </div>
        ))}
      </div>
    </div>
  );
}

function ContrastLabel({
  ratio,
  bg,
  fg,
  check,
  token,
}: {
  ratio: number;
  bg: string;
  fg: string;
  check: Check;
  token: string;
}) {
  const pass = ratio >= check.min;
  const where =
    check.under !== undefined
      ? `ink on selection over ${bg}`
      : fg === token
        ? check.bg.length > 1
          ? `lowest, on ${bg}`
          : `on ${bg}`
        : `${fg} on it`;
  return (
    <>
      <span className="block font-medium">
        {ratio.toFixed(2)}:1 <span className="font-normal text-ink-soft">{where}</span>
      </span>
      <span className={pass ? "block text-good-text" : "block text-pen-text"}>
        {pass ? `passes ${check.min}:1` : `below ${check.min}:1`}
      </span>
    </>
  );
}

/* Type --------------------------------------------------------------------------------------- */

function Notation({
  move,
  glyph,
  evaluation,
}: {
  move: string;
  glyph?: string;
  evaluation?: string;
}) {
  const mistake = glyph?.startsWith("?") || glyph === "!?";
  return (
    <span className="font-serif font-semibold tabular-nums lining-nums">
      {move}
      {glyph && <span className={mistake ? "text-pen-text" : "text-good-text"}>{glyph}</span>}
      {evaluation && <span className="font-normal text-ink-faint"> {evaluation}</span>}
    </span>
  );
}

const typeSteps: { step: string; size: string; use: string; sample: ReactNode }[] = [
  {
    step: "2xs",
    size: "11/14",
    use: "glyph-badge text and the eval bar label only",
    sample: <span className="font-serif text-2xs font-semibold">+5.7 ?? ?!</span>,
  },
  {
    step: "xs",
    size: "12/16",
    use: "board coordinates, chart ticks",
    sample: <span className="text-xs">a b c d e f g h</span>,
  },
  {
    step: "sm",
    size: "14/20",
    use: "secondary UI text, captions, graph labels",
    sample: (
      <span className="text-sm">
        <Notation move="9…b5" glyph="?" /> <Notation move="10…cxb5" glyph="?!" />
      </span>
    ),
  },
  {
    step: "base",
    size: "16/24",
    use: "UI body, buttons, inputs, the move list",
    sample: (
      <span className="text-base">
        <Notation move="9. Bg5" /> <Notation move="b5" glyph="?" evaluation="+5.7" />
      </span>
    ),
  },
  {
    step: "prose",
    size: "18/30",
    use: "serif body: annotations and explanations",
    sample: (
      <span className="font-serif text-prose">
        9…b5? opens the diagonal to the king on e8. After 10. Nxb5 cxb5 11. Bxb5+, White's pieces
        arrive with check.
      </span>
    ),
  },
  {
    step: "lg",
    size: "18/26",
    use: "sans lead text",
    sample: <span className="text-lg">Three moments decided this game.</span>,
  },
  {
    step: "xl",
    size: "21/28",
    use: "panel headings, Literata 600",
    sample: <span className="font-serif text-xl font-semibold">Critical moments</span>,
  },
  {
    step: "2xl",
    size: "24/30",
    use: "page section headings",
    sample: <span className="font-serif text-2xl font-semibold">Opening development</span>,
  },
  {
    step: "3xl",
    size: "36/42",
    use: "page titles",
    sample: (
      <span className="font-serif text-3xl font-semibold">
        Morphy v Duke Karl of Brunswick and Count Isouard, Paris 1858
      </span>
    ),
  },
  {
    step: "4xl",
    size: "48/54",
    use: "rare: the result line of a reviewed game",
    sample: (
      <span className="font-serif text-4xl font-semibold tabular-nums lining-nums">
        1–0, 17. Rd8#
      </span>
    ),
  },
  {
    step: "5xl",
    size: "60/64",
    use: "landing only (M5)",
    sample: <span className="font-serif text-5xl font-semibold">Paris 1858</span>,
  },
];

function TypeScale() {
  return (
    <div className="flex flex-col">
      <p className="max-w-prose text-sm text-ink-soft">
        Literata sets anything a coach would write, with its optical size following the rendered
        size. Schibsted Grotesk sets anything the app says.
      </p>
      <ul className="mt-4 flex flex-col">
        {typeSteps.map(({ step, size, use, sample }) => (
          <li
            key={step}
            className="flex flex-col gap-2 border-t border-rule py-4 md:flex-row md:items-baseline md:gap-6"
          >
            <span className="shrink-0 md:w-56">
              <span className="block text-sm font-medium">
                {step} <span className="font-normal text-ink-faint">{size}</span>
              </span>
              <span className="block text-xs text-ink-faint">{use}</span>
            </span>
            <span className="min-w-0 break-words">{sample}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/* Space and shape ---------------------------------------------------------------------------- */

const moves: { n: string; white: string; black: string; glyph?: string }[] = [
  { n: "8.", white: "Nc3", black: "c6" },
  { n: "9.", white: "Bg5", black: "b5", glyph: "?" },
  { n: "10.", white: "Nxb5", black: "cxb5", glyph: "?!" },
];

function MoveRows({ rowClass }: { rowClass: string }) {
  return (
    <ol className="rounded-field bg-paper px-3">
      {moves.map((move) => (
        <li
          key={move.n}
          className={`flex items-center gap-3 border-b border-rule last:border-b-0 ${rowClass}`}
        >
          <span className="w-7 font-serif text-ink-faint tabular-nums lining-nums">{move.n}</span>
          <span className="w-16">
            <Notation move={move.white} />
          </span>
          <Notation move={move.black} glyph={move.glyph} />
        </li>
      ))}
    </ol>
  );
}

function SpaceAndShape() {
  const [ref, values] = useCustomProperties(["--space-panel-pad", "--space-gutter"]);
  return (
    <div ref={ref} className="grid grid-cols-1 gap-8 md:grid-cols-2">
      <div className="flex flex-col gap-5">
        <div>
          <p className="text-sm font-medium">
            --space-move-row <span className="font-normal text-ink-faint">26px</span>
          </p>
          <p className="mb-2 text-xs text-ink-faint">the move list beside the board</p>
          <MoveRows rowClass="h-(--space-move-row) text-sm" />
        </div>
        <div>
          <p className="text-sm font-medium">
            --space-scoresheet-row <span className="font-normal text-ink-faint">34px</span>
          </p>
          <p className="mb-2 text-xs text-ink-faint">the full scoresheet, room for its ring</p>
          <MoveRows rowClass="h-(--space-scoresheet-row) text-base" />
        </div>
        <dl className="grid grid-cols-2 gap-3 text-sm">
          <div className="rounded-field bg-well p-3">
            <dt className="font-medium">--space-panel-pad</dt>
            <dd className="text-ink-faint">{values["--space-panel-pad"] || " "} here</dd>
          </div>
          <div className="rounded-field bg-well p-3">
            <dt className="font-medium">--space-gutter</dt>
            <dd className="text-ink-faint">{values["--space-gutter"] || " "} here</dd>
          </div>
        </dl>
      </div>
      <div className="flex flex-col gap-5">
        <ul className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {(
            [
              ["board", "rounded-board", "2px"],
              ["field", "rounded-field", "6px"],
              ["sheet", "rounded-sheet", "10px"],
              ["pill", "rounded-pill", "9999px"],
            ] as const
          ).map(([name, cls, value]) => (
            <li key={name} className="flex flex-col gap-2">
              <span aria-hidden className={`h-14 border-2 border-ink-soft bg-well ${cls}`} />
              <span className="text-sm font-medium">
                --radius-{name} <span className="block font-normal text-ink-faint">{value}</span>
              </span>
            </li>
          ))}
        </ul>
        <div className="rounded-field bg-well p-6">
          <div className="rounded-field bg-sheet p-4 shadow-lift">
            <p className="text-sm font-medium">--shadow-lift</p>
            <p className="text-xs text-ink-faint">
              The one shadow, for what floats: popovers, the drawer, the command menu and a dragged
              piece. Everything else separates by tone.
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}

/* Motion ------------------------------------------------------------------------------------- */

type Transition =
  | Spring
  | { type: "tween"; duration: number; ease: readonly [number, number, number, number] };

/** A marker that travels to a target line at 80% of its track, so overshoot shows past it. */
function Track({ transition, play }: { transition: Transition; play: number }) {
  const track = useRef<HTMLDivElement>(null);
  const marker = useRef<HTMLSpanElement>(null);
  const reduced = useReducedMotion();

  useEffect(() => {
    if (play === 0 || !track.current || !marker.current) return;
    const distance = track.current.clientWidth * 0.8 - marker.current.offsetWidth / 2;
    // M10: no movement, the end state with one opacity cue.
    const controls = reduced
      ? animate(
          marker.current,
          { x: distance, opacity: [0, 1] },
          { x: { duration: 0 }, opacity: { duration: dur.quick } },
        )
      : animate(
          marker.current,
          { x: [0, distance] },
          transition.type === "spring"
            ? { ...transition }
            : { duration: transition.duration, ease: [...transition.ease] },
        );
    return () => controls.stop();
  }, [play, reduced, transition]);

  return (
    <div ref={track} className="relative h-6 min-w-40 flex-1" aria-hidden>
      <span className="absolute inset-x-0 top-1/2 h-px bg-rule" />
      <span className="absolute top-0 left-4/5 h-6 w-0.5 bg-rule-strong" />
      <span
        ref={marker}
        className="absolute top-1/2 left-0 -mt-2 block size-4 rounded-pill bg-ink"
      />
    </div>
  );
}

function PlayButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      className="shrink-0 rounded-field border border-rule-strong px-3 py-1 text-sm font-medium text-ink hover:bg-well active:scale-97"
    >
      Play
    </button>
  );
}

function usePlay(all: number) {
  const [play, setPlay] = useState(0);
  useEffect(() => {
    if (all > 0) setPlay((n) => n + 1);
  }, [all]);
  return [play, useCallback(() => setPlay((n) => n + 1), [])] as const;
}

const ms = (seconds: number) => `${Math.round(seconds * 1000)}ms`;

const springRows: { name: string; spring: Spring; use: string }[] = [
  { name: "settle", spring: spring.settle, use: "piece travel between squares" },
  { name: "settle, pawn", spring: settleFor("pawn"), use: "lighter: travels quicker, no bounce" },
  { name: "settle, rook", spring: settleFor("rook"), use: "heavier: looser settle" },
  { name: "settle, queen and king", spring: settleFor("queen"), use: "heaviest" },
  { name: "land", spring: spring.land, use: "squash on arrival, badges, streak ticks" },
  { name: "pen", spring: spring.pen, use: "a stroke that follows input" },
  { name: "swing", spring: spring.swing, use: "eval bar and graph cursor, small changes" },
  { name: "swing-big", spring: spring.swingBig, use: "eval bar, changes of 0.3 or more" },
  { name: "page", spring: spring.page, use: "incoming page, layout morphs" },
  { name: "tilt", spring: spring.tilt, use: "a dragged piece returning upright" },
];

function SpringRow({ name, spring: s, use, all }: (typeof springRows)[number] & { all: number }) {
  const [play, replay] = usePlay(all);
  const shoot = overshoot(s) * 100;
  return (
    <li className="flex flex-col gap-3 border-t border-rule py-3 md:flex-row md:items-center">
      <span className="shrink-0 md:w-80">
        <span className="block text-sm font-medium">{name}</span>
        <span className="block text-xs text-ink-faint">
          stiffness {s.stiffness}, damping {s.damping}
          {s.mass !== undefined && `, mass ${s.mass}`}; ζ {dampingRatio(s).toFixed(2)}, overshoot{" "}
          {shoot < 0.01 ? "0" : shoot < 1 ? shoot.toFixed(2) : shoot.toFixed(0)}%
        </span>
        <span className="block text-xs text-ink-faint">{use}</span>
      </span>
      <span className="flex items-center gap-4 md:flex-1">
        <Track transition={s} play={play} />
        <PlayButton onClick={replay} label={`Play ${name}`} />
      </span>
    </li>
  );
}

const durationRows: { token: string; seconds: number; curve: keyof typeof ease; use: string }[] = [
  { token: "instant", seconds: dur.instant, curve: "out", use: "press, arrowhead stroke, lift" },
  { token: "quick", seconds: dur.quick, curve: "out", use: "menus, toggles, retracting marks" },
  { token: "base", seconds: dur.base, curve: "out", use: "panels, drawers, a captured piece" },
  { token: "draw-small", seconds: dur.drawSmall, curve: "pen", use: "a small stroke" },
  { token: "draw", seconds: dur.draw, curve: "pen", use: "one board stroke" },
  { token: "reveal", seconds: dur.reveal, curve: "pen", use: "the eval graph drawing across" },
  { token: "turn", seconds: dur.turn, curve: "turn", use: "a page turn" },
  { token: "stream", seconds: dur.stream, curve: "out", use: "one replayed move" },
  { token: "soak", seconds: dur.soak, curve: "soak", use: "an ink-soak progress segment" },
];

function DurationRow({
  token,
  seconds,
  curve,
  use,
  all,
}: (typeof durationRows)[number] & { all: number }) {
  const [play, replay] = usePlay(all);
  const transition = useMemo(
    () => ({ type: "tween", duration: seconds, ease: ease[curve] }) as const,
    [seconds, curve],
  );
  return (
    <li className="flex flex-col gap-3 border-t border-rule py-3 md:flex-row md:items-center">
      <span className="shrink-0 md:w-80">
        <span className="block text-sm font-medium">
          --dur-{token} <span className="font-normal text-ink-faint">{ms(seconds)}</span>
        </span>
        <span className="block text-xs text-ink-faint">
          {use}, shown on --ease-{curve}
        </span>
      </span>
      <span className="flex items-center gap-4 md:flex-1">
        <Track transition={transition} play={play} />
        <PlayButton onClick={replay} label={`Play --dur-${token}`} />
      </span>
    </li>
  );
}

function EaseCurve({ name }: { name: keyof typeof ease }) {
  const [x1, y1, x2, y2] = ease[name];
  // A 0–1 box drawn upside down, so progress rises to the right.
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

function StaggerRow({
  name,
  seconds,
  all,
}: {
  name: keyof typeof stagger;
  seconds: number;
  all: number;
}) {
  const [play, replay] = usePlay(all);
  const reduced = useReducedMotion();
  const ticks = useRef<HTMLSpanElement[]>([]);
  useEffect(() => {
    if (play === 0) return;
    const controls = ticks.current.map((tick, i) =>
      animate(tick, { opacity: [0, 1] }, { duration: dur.quick, delay: reduced ? 0 : i * seconds }),
    );
    return () => {
      for (const c of controls) c.stop();
    };
  }, [play, reduced, seconds]);
  return (
    <li className="flex items-center gap-4 border-t border-rule py-3">
      <span className="w-36 shrink-0 text-sm font-medium">
        --stagger-{name} <span className="font-normal text-ink-faint">{ms(seconds)}</span>
      </span>
      <span className="flex min-w-0 flex-1 gap-1.5" aria-hidden>
        {Array.from({ length: 8 }, (_, i) => (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed row of identical ticks
            key={i}
            ref={(el) => {
              if (el) ticks.current[i] = el;
            }}
            className="h-5 w-1.5 rounded-board bg-ink-soft"
          />
        ))}
      </span>
      <PlayButton onClick={replay} label={`Play --stagger-${name}`} />
    </li>
  );
}

function MotionTokens() {
  const [all, setAll] = useState(0);
  const reduced = useReducedMotion();
  return (
    <div className="flex flex-col gap-8">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="max-w-prose text-sm text-ink-soft">
          Each marker travels to the line. A spring with ζ below 1 runs past it and swings back.
          {reduced &&
            " Your system asks for reduced motion, so markers appear at the end with a fade instead."}
        </p>
        <button
          type="button"
          onClick={() => setAll((n) => n + 1)}
          className="rounded-field bg-ink px-4 py-2 text-sm font-medium text-on-ink hover:bg-ink-soft active:scale-97"
        >
          Play all
        </button>
      </div>
      <div>
        <h4 className="font-serif text-lg font-semibold">Springs</h4>
        <ul className="mt-2">
          {springRows.map((row) => (
            <SpringRow key={row.name} {...row} all={all} />
          ))}
        </ul>
      </div>
      <div>
        <h4 className="font-serif text-lg font-semibold">Durations</h4>
        <ul className="mt-2">
          {durationRows.map((row) => (
            <DurationRow key={row.token} {...row} all={all} />
          ))}
        </ul>
      </div>
      <div className="grid grid-cols-1 gap-8 md:grid-cols-2">
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
          <ul className="mt-2">
            {(Object.keys(stagger) as (keyof typeof stagger)[]).map((name) => (
              <StaggerRow key={name} name={name} seconds={stagger[name]} all={all} />
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}
