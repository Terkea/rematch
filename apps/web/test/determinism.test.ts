import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

// ADR 0104 (issue #53): every random draw in the maths is mulberry32 seeded from the data
// (`server/stats/bootstrap.ts`), and time enters as a parameter (the trainer's `today`, the
// focus's `now`). A planted `Math.random`, `Date.now()` or argument-less `new Date()` in one of
// these modules fails here.

const SRC = join(__dirname, "../src/server");
const MATHS = ["stats", "train/model.ts", "focus/goal.ts", "overview", "profile"];
const BANNED: [string, RegExp][] = [
  ["Math.random", /Math\.random\s*\(/],
  ["Date.now()", /Date\.now\s*\(\s*\)/],
  ["new Date()", /new Date\s*\(\s*\)/],
];
/** Legitimate uses: [file relative to src/server, banned call, reason]. None today. */
const ALLOWED: [string, string, string][] = [];

const files = (p: string): string[] =>
  statSync(p).isDirectory()
    ? readdirSync(p)
        .sort()
        .flatMap((n) => files(join(p, n)))
    : p.endsWith(".ts")
      ? [p]
      : [];

/** The code of a source text without `//` and `/* *\/` comments. */
const code = (text: string): string =>
  text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'`])\/\/.*$/gm, "$1");

function violations(text: string): string[] {
  const c = code(text);
  return BANNED.filter(([, re]) => re.test(c)).map(([name]) => name);
}

describe("the randomness and clock ban in the maths modules", () => {
  it("finds no banned call outside the allow-list", () => {
    const found: string[] = [];
    for (const m of MATHS) {
      for (const f of files(join(SRC, m))) {
        const rel = relative(SRC, f).split("\\").join("/");
        for (const v of violations(readFileSync(f, "utf8"))) {
          if (!ALLOWED.some(([af, ac]) => af === rel && ac === v)) found.push(`${rel}: ${v}`);
        }
      }
    }
    expect(found).toEqual([]);
  });

  it("covers every listed module", () => {
    for (const m of MATHS) expect(files(join(SRC, m)).length, m).toBeGreaterThan(0);
  });

  it("catches a planted call and ignores comments and dated constructors", () => {
    expect(violations("const x = Math.random();")).toEqual(["Math.random"]);
    expect(violations("const t = Date.now();\nconst d = new Date();")).toEqual([
      "Date.now()",
      "new Date()",
    ]);
    expect(violations("const d = new Date(iso); // Date.now() is banned")).toEqual([]);
    expect(violations("/* Math.random() */ const y = 1;")).toEqual([]);
  });
});
