"use client";

import { contrast, over, parseColour, type Rgba } from "@ci/ui/lib";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { MotionTokens } from "./foundations-motion";

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
        <h2 id="foundations-title" className="font-serif text-2xl font-semibold">
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
      <Panel title="Space, shape and elevation">
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
    const tint: Rgba | undefined = check.under ? colour(check.under) : undefined;
    for (const bgToken of check.bg) {
      const surface = colour(bgToken);
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
            <ul className="mt-2 flex flex-col gap-1">
              {group.tokens.map(([token, check]) => {
                const result = typeof check === "string" ? undefined : measure(check);
                return (
                  <li
                    key={token}
                    className="flex items-center gap-3 rounded-field px-2 py-2 odd:bg-sheet"
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
  opponent = false,
}: {
  move: string;
  glyph?: string;
  evaluation?: string;
  /** The opponent's glyph is graphite pencil: red is only ever the player's mistake. */
  opponent?: boolean;
}) {
  const mistake = glyph?.startsWith("?") || glyph === "!?";
  return (
    <span className="font-serif font-semibold tabular-nums lining-nums">
      {move}
      {glyph && (
        <span
          className={`tracking-glyph ${opponent ? "text-pencil" : mistake ? "text-pen-text" : "text-good-text"}`}
        >
          {glyph}
        </span>
      )}
      {evaluation && <span className="font-normal text-ink-faint"> {evaluation}</span>}
    </span>
  );
}

const typeSteps: { step: string; token: string; use: string; sample: ReactNode }[] = [
  {
    step: "2xs",
    token: "2xs",
    use: "glyph-badge text and the eval bar label only",
    sample: (
      <span className="flex items-center gap-3">
        {(["??", "?", "?!"] as const).map((glyph) => (
          <GlyphBadge key={glyph} glyph={glyph} tone="pen" />
        ))}
        <GlyphBadge glyph="!" tone="good" />
        <span className="rounded-board border border-rule-strong bg-ink px-1 font-serif text-2xs font-semibold text-on-ink">
          +5.7
        </span>
      </span>
    ),
  },
  {
    step: "xs",
    token: "xs",
    use: "board coordinates, chart ticks",
    sample: <span className="text-xs">a b c d e f g h</span>,
  },
  {
    step: "sm",
    token: "sm",
    use: "secondary UI text, captions, graph labels",
    sample: (
      <span className="text-sm">
        <Notation move="27. Rbb1" glyph="?!" /> <Notation move="28. Nd2" glyph="??" />
      </span>
    ),
  },
  {
    step: "base",
    token: "base",
    use: "UI body, buttons, inputs, the move list",
    sample: (
      <span className="text-base">
        <Notation move="27…Rf8" /> <Notation move="28. Nd2" glyph="??" evaluation="−1.1" />
      </span>
    ),
  },
  {
    step: "prose",
    token: "prose",
    use: "serif body: annotations and explanations",
    sample: (
      <span className="font-serif text-prose">
        28. Nd2?? lets go of e4. After 28…e4 29. Nxe4 Bxf3+, Black's bishop takes the pawn in front
        of the king, with check.
      </span>
    ),
  },
  {
    step: "prose, italic",
    token: "prose",
    use: "an annotation comment: Literata italic, the whole comment, never one word",
    sample: (
      <span className="font-serif-italic text-prose italic">
        Your knight on e4 was holding back Black's e-pawn; one quiet retreat, and +2.7 became −1.1.
      </span>
    ),
  },
  {
    step: "lg",
    token: "lg",
    use: "sans lead text",
    sample: <span className="text-lg">Three moments decided this game.</span>,
  },
  {
    step: "xl",
    token: "xl",
    use: "panel headings, Literata 600",
    sample: <span className="font-serif text-xl font-semibold">Critical moments</span>,
  },
  {
    step: "2xl",
    token: "2xl",
    use: "page section headings",
    sample: <span className="font-serif text-2xl font-semibold">Opening development</span>,
  },
  {
    step: "3xl",
    token: "3xl",
    use: "page titles",
    sample: <span className="font-serif text-3xl font-semibold">DrNykterstein v Night-King96</span>,
  },
  {
    step: "4xl",
    token: "4xl",
    use: "rare: the result line of a reviewed game",
    sample: (
      <span className="font-serif text-4xl font-semibold tabular-nums lining-nums">
        1–0, 46. Qxd6
      </span>
    ),
  },
  {
    step: "5xl",
    token: "5xl",
    use: "landing only (M5)",
    sample: <span className="font-serif text-5xl font-semibold">Queen’s Indian</span>,
  },
];

/** A glyph badge as on the board: a filled circle on a 2px sheet halo, the glyph at its floor. */
function GlyphBadge({ glyph, tone }: { glyph: string; tone: "pen" | "good" }) {
  return (
    <span
      className={`grid size-6 place-items-center rounded-pill font-serif text-2xs font-semibold text-on-ink ring-2 ring-sheet ${tone === "pen" ? "bg-pen" : "bg-good"}`}
    >
      {glyph}
    </span>
  );
}

/** A length token in px, from its computed value (rem at the root's 16px). */
function px(value: string | undefined): string {
  if (!value) return " ";
  const n = Number.parseFloat(value);
  if (Number.isNaN(n)) return value;
  return `${Math.round(value.endsWith("rem") ? n * 16 : n)}`;
}

function TypeScale() {
  const tokens = [...new Set(typeSteps.map((t) => t.token))];
  const [ref, values] = useCustomProperties(
    tokens.flatMap((t) => [`--text-${t}`, `--text-${t}--line-height`]),
  );
  return (
    <div ref={ref} className="flex flex-col">
      <p className="max-w-prose text-sm text-ink-soft">
        Literata sets anything a coach would write, with its optical size following the rendered
        size. Schibsted Grotesk sets anything the app says.
      </p>
      <ul className="mt-4 flex flex-col gap-1">
        {typeSteps.map(({ step, token, use, sample }) => (
          <li
            key={step}
            className="flex flex-col gap-2 rounded-field px-3 py-4 odd:bg-paper md:flex-row md:items-baseline md:gap-6"
          >
            <span className="shrink-0 md:w-56">
              <span className="block text-sm font-medium">
                {step}{" "}
                <span className="font-normal text-ink-faint">
                  {px(values[`--text-${token}`])}/{px(values[`--text-${token}--line-height`])}
                </span>
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

// Rows 28 to 30 of the middlegame blunder fixture: the player had White.
const moves: { n: string; white: string; whiteGlyph?: string; black: string; glyph?: string }[] = [
  { n: "28.", white: "Nd2", whiteGlyph: "??", black: "e4", glyph: "?!" },
  { n: "29.", white: "Nxe4", black: "Bxf3+" },
  { n: "30.", white: "Kd2", black: "Ra8", glyph: "?!" },
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
            <Notation move={move.white} glyph={move.whiteGlyph} />
          </span>
          <Notation move={move.black} glyph={move.glyph} opponent />
        </li>
      ))}
    </ol>
  );
}

function SpaceAndShape() {
  const [ref, values] = useCustomProperties([
    "--space-panel-pad",
    "--space-gutter",
    "--space-move-row",
    "--space-scoresheet-row",
    "--radius-board",
    "--radius-field",
    "--radius-sheet",
    "--radius-pill",
    "--shadow-lift",
  ]);
  return (
    <div ref={ref} className="grid grid-cols-1 gap-8 md:grid-cols-2">
      <div className="flex flex-col gap-5">
        <div>
          <p className="text-sm font-medium">
            --space-move-row{" "}
            <span className="font-normal text-ink-faint">{values["--space-move-row"] || " "}</span>
          </p>
          <p className="mb-2 text-xs text-ink-faint">the move list beside the board</p>
          <MoveRows rowClass="h-(--space-move-row) text-sm" />
        </div>
        <div>
          <p className="text-sm font-medium">
            --space-scoresheet-row{" "}
            <span className="font-normal text-ink-faint">
              {values["--space-scoresheet-row"] || " "}
            </span>
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
              ["board", "rounded-board"],
              ["field", "rounded-field"],
              ["sheet", "rounded-sheet"],
              ["pill", "rounded-pill"],
            ] as const
          ).map(([name, cls]) => (
            <li key={name} className="flex flex-col gap-2">
              <span aria-hidden className={`h-14 border-2 border-ink-soft bg-well ${cls}`} />
              <span className="text-sm font-medium">
                --radius-{name}{" "}
                <span className="block font-normal text-ink-faint">
                  {values[`--radius-${name}`] || " "}
                </span>
              </span>
            </li>
          ))}
        </ul>
        <div className="rounded-field bg-well p-6">
          <div className="rounded-field bg-sheet p-4 shadow-lift">
            <p className="text-sm font-medium">--shadow-lift</p>
            <p className="text-xs text-ink-faint break-words">{values["--shadow-lift"] || " "}</p>
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
