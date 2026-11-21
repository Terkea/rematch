import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { runBanLint } from "../ban-lint.js";
import { banRules } from "../ban-rules.js";

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

  it("scans only apps/web/src and packages/ui/src and tolerates missing dirs", async () => {
    const r = await tree({
      "packages/other/src/x.tsx": `lorem`,
      "packages/ui/src/node_modules/x.ts": `lorem`,
    });
    expect((await runBanLint(r)).findings).toEqual([]);
  });
});
