import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { runBanLint } from "../ban-lint.js";
import { banRules, lintFile } from "../ban-rules.js";

function run(rule: string, text: string, path = "packages/ui/src/primitives/button.tsx") {
  const r = banRules.find((x) => x.id === rule);
  if (!r) throw new Error(`no rule ${rule}`);
  return r.test({ path, text });
}

const cases: { rule: string; bad: string; good: string; path?: string }[] = [
  {
    rule: "B12",
    bad: `import { Chessground } from "chessground";`,
    good: `import { Board } from "react-chessboard";`,
  },
  {
    rule: "B12",
    bad: `import x from "@lichess-org/chessground/dist/api";`,
    good: `import x from "chess.js";`,
  },
  {
    rule: "B12",
    bad: `import { Chess } from "chessops/chess";`,
    good: `import { threeUp } from "./three-up";`,
  },
  {
    rule: "B12",
    bad: `import { Canvas } from "@react-three/fiber";`,
    good: `import { a } from "@react/three";`,
  },
  {
    rule: "B12",
    bad: `import * as THREE from "three";`,
    good: `import { x } from "threejs-helper";`,
  },
  { rule: "B12", bad: `const g = await import("gsap");`, good: `const g = "gsap is banned";` },
  { rule: "B12", bad: `import Lenis from "lenis";`, good: `import { x } from "motion/react";` },
  {
    rule: "B13",
    bad: `import { Check } from "lucide-react";`,
    good: `import { Check } from "@ci/ui/icons";`,
  },
  {
    rule: "B5a",
    bad: `<p className="uppercase tracking-wide">x</p>`,
    good: `<p className="uppercase">x</p>`,
  },
  {
    rule: "B5a",
    bad: `cn("text-xs", "tracking-widest uppercase")`,
    good: `<p className="tracking-tight">x</p>`,
  },
  { rule: "B5b", bad: `<span>Next →</span>`, good: `<span>Next</span>` },
  {
    rule: "B5b",
    bad: `const s = "a → b";`,
    good: `// a → b is only a comment`,
    path: "packages/ui/src/x.tsx",
  },
  {
    rule: "B5c",
    bad: `color: #0B0B0B;`,
    good: `color: var(--ink);`,
    path: "packages/ui/src/x.css",
  },
  { rule: "B5c", bad: `const c = "#111111";`, good: `const c = "#1112";` },
  { rule: "B5c", bad: `const c = "#111";`, good: `const c = "#1a1a1a";` },
  {
    rule: "B6",
    bad: `import { Inter } from "next/font/google";`,
    good: `import { Literata } from "next/font/google";`,
  },
  {
    rule: "B6",
    bad: `import {\n  Literata,\n  Space_Grotesk,\n} from "next/font/google";`,
    good: `import { Inter } from "./inter";`,
  },
  {
    rule: "B6",
    bad: `import { Geist, Instrument_Serif } from "next/font/google";`,
    good: `import localFont from "next/font/local";`,
  },
  { rule: "B7", bad: `<div className="bg-gradient-to-r">`, good: `<div className="bg-paper">` },
  { rule: "B7", bad: `<div className="md:bg-linear-to-b">`, good: `<div className="bg-linear">` },
  { rule: "B7", bad: `<div className="bg-radial-[at_top]">`, good: `<div className="blur-none">` },
  { rule: "B7", bad: `<h1 className="bg-clip-text">`, good: `<h1 className="bg-clip-border">` },
  { rule: "B7", bad: `<div className="backdrop-blur-md">`, good: `<div className="blur-0">` },
  { rule: "B15", bad: `<p>Lorem ipsum dolor</p>`, good: `<p>Real copy</p>` },
  {
    rule: "B18",
    bad: `<Spinner className="animate-spin" />`,
    good: `<Spinner className="animate-pulse-ink" />`,
  },
  { rule: "TOK1", bad: `<div className="text-[#D3312B]">`, good: `<div className="text-pen">` },
  {
    rule: "TOK1",
    bad: `fill: #D3312B;`,
    good: `fill: #D3312B;`,
    path: "packages/ui/src/styles/tokens.css",
  },
  {
    rule: "TOK1",
    bad: `const c = "#fff";`,
    good: `const c = "#fff";`,
    path: "packages/ui/src/board/square.tsx",
  },
  {
    rule: "TOK1",
    bad: `const c = "#fff";`,
    good: `const c = "#fff";`,
    path: "packages/ui/src/annotations/ring.tsx",
  },
  { rule: "TOK2", bad: `<div className="w-[320px]">`, good: `<div className="w-80">` },
  {
    rule: "TOK2",
    bad: `<div className="w-[320px]">`,
    good: `<div className="w-[320px]">`,
    path: "packages/ui/src/board/b.tsx",
  },
  {
    rule: "TOK2",
    bad: `<div className="gap-[7px]">`,
    good: `<div className="w-[var(--board-size)]">`,
  },
];

describe("banRules", () => {
  it("covers every rule in the brief", () => {
    expect(banRules.map((r) => r.id).sort()).toEqual(
      ["B12", "B13", "B15", "B18", "B5a", "B5b", "B5c", "B6", "B7", "TOK1", "TOK2"].sort(),
    );
  });

  for (const c of cases) {
    const path = c.path ?? "packages/ui/src/primitives/button.tsx";
    // Exempt-path cases have bad === good: the sample is flagged elsewhere, not here.
    const exempt = c.bad === c.good;
    it(`${c.rule} ${exempt ? "exempts" : "flags"} ${JSON.stringify(exempt ? c.good : c.bad)} (${path})`, () => {
      if (exempt) {
        expect(run(c.rule, c.bad, path)).toEqual([]);
        expect(run(c.rule, c.bad).length).toBeGreaterThan(0);
      } else {
        expect(run(c.rule, c.bad, path).length).toBeGreaterThan(0);
        expect(run(c.rule, c.good, path)).toEqual([]);
      }
    });
  }

  it("TOK2 allows container-query breakpoints (@min-[Npx]) but not lengths", () => {
    expect(run("TOK2", `<div className="@min-[560px]:flex @max-[640px]:hidden">`)).toEqual([]);
    expect(run("TOK2", `<div className="@min-[560px]:mt-[9px]">`)).toHaveLength(1);
  });

  it("reports 1-based lines", () => {
    const hits = run("B15", `a\nb\nlorem`);
    expect(hits).toEqual([{ line: 3, match: "lorem" }]);
  });

  it("reports the line of a multi-line font import name", () => {
    const hits = run("B6", `import {\n  Literata,\n  Space_Grotesk,\n} from "next/font/google";`);
    expect(hits).toEqual([{ line: 3, match: "Space_Grotesk" }]);
  });
});

describe("tightened rules", () => {
  it("B5a joins adjacent literals in cn/clsx/className={}, across lines", () => {
    expect(run("B5a", `cn("uppercase", "tracking-wide")`)).toHaveLength(1);
    expect(
      run("B5a", `clsx(\n  "text-xs uppercase",\n  active && "md:tracking-widest",\n)`),
    ).toHaveLength(1);
    expect(
      run("B5a", `<p className={cn("a", cond ? "uppercase" : "", "tracking-tight")} />`),
    ).toHaveLength(1);
    expect(run("B5a", `cn("uppercase", "font-bold")`)).toEqual([]);
    expect(run("B5a", `cn("uppercase")\ncn("tracking-wide")`)).toEqual([]);
  });

  it("HEX ignores anchors and SVG references but flags colours in more contexts", () => {
    expect(run("TOK1", `<a href="#fade">x</a>`)).toEqual([]);
    expect(run("TOK1", `<use href="#abc" />`)).toEqual([]);
    expect(run("TOK1", `fill="url(#def)"`)).toEqual([]);
    expect(run("TOK1", `<div id="#add" />`)).toEqual([]);
    expect(run("TOK1", `const c = ["#fff"];`)).toHaveLength(1);
    expect(run("TOK1", `style={{ color: '#D3312B' }}`)).toHaveLength(1);
    expect(run("TOK1", `box-shadow: 0 0 0 1px #abc;`, "packages/ui/src/x.css")).toHaveLength(1);
    expect(run("TOK1", `a#fff`)).toEqual([]);
    expect(run("B5c", `<a href="#111">x</a>`)).toEqual([]);
    expect(run("B5c", `x = "#111"`)).toHaveLength(1);
  });

  it("allow comments need a reason", () => {
    const path = "packages/ui/src/primitives/button.tsx";
    const flagged = (text: string) => lintFile({ path, text }).map((f) => f.rule);
    expect(flagged(`const c = "#D3312B";`)).toEqual(["TOK1"]);
    expect(
      flagged(`const c = "#D3312B"; // ban-lint-allow TOK1: brand mark from the logo file`),
    ).toEqual([]);
    expect(flagged(`// ban-lint-allow TOK1: brand mark\nconst c = "#D3312B";`)).toEqual([]);
    expect(flagged(`const c = "#D3312B"; // ban-lint-allow TOK1`)).toEqual(["TOK1"]);
    expect(flagged(`const c = "#D3312B"; // ban-lint-allow TOK1:   `)).toEqual(["TOK1"]);
    expect(flagged(`const c = "#D3312B"; // ban-lint-allow TOK2: wrong rule`)).toEqual(["TOK1"]);
    expect(flagged(`const c = "#D3312B";\n\n// ban-lint-allow TOK1: too far\n`)).toEqual(["TOK1"]);
  });

  it("TOK2 widens to rem/em, spaces, case, arbitrary properties and calc", () => {
    for (const c of [
      "w-[2rem]",
      "p-[1.5em]",
      "w-[ 7PX ]",
      "[width:7px]",
      "[--gap:0.5rem]",
      "w-[calc(100%-2px)]",
      "h-[calc(var(--x)+1rem)]",
      "mt-[-3px]",
      "grid-cols-[7.5rem_minmax(0,1fr)]",
      "shadow-[inset_0_0_0_1px_red]",
    ])
      expect(run("TOK2", `<div className="${c}">`), c).toHaveLength(1);
    for (const c of [
      "w-[var(--board-size)]",
      "grid-cols-[1fr_auto]",
      "@min-[560px]:flex",
      "@max-[40rem]:hidden",
      "h-[100svh]",
      "[&>svg]:size-4",
    ])
      expect(run("TOK2", `<div className="${c}">`), c).toEqual([]);
  });

  it("B5b skips JSX comments and flags &rarr; and the glyph", () => {
    expect(run("B5b", `<p>{/* a → b */}</p>`)).toEqual([]);
    expect(run("B5b", `{/*\n a → b\n*/}\n<p>x</p>`)).toEqual([]);
    expect(run("B5b", `<p>Go &rarr;</p>`)).toHaveLength(1);
    expect(run("B5b", `<p>Go →</p>`)).toHaveLength(1);
  });

  it("B6 catches default and namespace imports", () => {
    expect(run("B6", `import Inter from "next/font/google";`)).toHaveLength(1);
    expect(run("B6", `import * as Geist from "next/font/google";`)).toHaveLength(1);
    expect(
      run("B6", `import * as fonts from "next/font/google";\nconst f = fonts.Inter({});`),
    ).toHaveLength(1);
    expect(
      run("B6", `import * as fonts from "next/font/google";\nconst f = fonts.Literata({});`),
    ).toEqual([]);
    expect(run("B6", `import localFont from "next/font/local";`)).toEqual([]);
    expect(run("B6", `import Space_Grotesk, { Literata } from "next/font/google";`)).toHaveLength(
      1,
    );
  });
});

describe("runBanLint", () => {
  let root = "";
  afterEach(async () => {
    if (root) await rm(root, { recursive: true, force: true });
    root = "";
  });

  async function tree(files: Record<string, string>) {
    root = await mkdtemp(join(tmpdir(), "ban-lint-"));
    for (const [p, text] of Object.entries(files)) {
      const abs = join(root, p);
      await mkdir(dirname(abs), { recursive: true });
      await writeFile(abs, text);
    }
    return root;
  }

  it("returns no findings for a clean tree", async () => {
    const r = await tree({ "apps/web/src/app/page.tsx": `export const P = () => <p>hi</p>;` });
    expect((await runBanLint(r)).findings).toEqual([]);
  });

  it("flags a raw hex in a component", async () => {
    const r = await tree({ "packages/ui/src/primitives/button.tsx": `const c = "#D3312B";\n` });
    expect((await runBanLint(r)).findings).toEqual([
      { rule: "TOK1", path: "packages/ui/src/primitives/button.tsx", line: 1, match: "#D3312B" },
    ]);
  });

  it("exempts lab pages from every rule, but not the same text elsewhere", async () => {
    const text = `const c = "#D3312B"; // Lorem\n`;
    const r = await tree({
      "apps/web/src/app/lab/board/page.tsx": text,
      "apps/web/src/app/real/page.tsx": text,
    });
    const paths = (await runBanLint(r)).findings.map((f) => f.path);
    expect(paths.length).toBeGreaterThan(0);
    expect(paths.every((p) => p === "apps/web/src/app/real/page.tsx")).toBe(true);
  });

  it("scans the site's Astro, TS and CSS sources too (ADR 0096)", async () => {
    const r = await tree({
      "site/src/pages/index.astro": `<a href="/docs">Docs →</a>
<p style="color: #D3312B">x</p>
`,
      "site/src/styles/landing.css": `.x { color: #16233b; }
`,
      "site/src/styles/generated/tokens.css": `:root { --color-ink: #16233b; }
`,
    });
    expect((await runBanLint(r)).findings).toEqual([
      { rule: "B5b", path: "site/src/pages/index.astro", line: 1, match: "→" },
      { rule: "TOK1", path: "site/src/pages/index.astro", line: 2, match: "#D3312B" },
      { rule: "TOK1", path: "site/src/styles/landing.css", line: 1, match: "#16233b" },
    ]);
  });

  it("scans only apps/web/src, packages/ui/src and site/src and tolerates missing dirs", async () => {
    const r = await tree({
      "packages/other/src/x.tsx": `lorem`,
      "packages/ui/src/node_modules/x.ts": `lorem`,
    });
    expect((await runBanLint(r)).findings).toEqual([]);
  });
});
