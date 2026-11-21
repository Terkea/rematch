#!/usr/bin/env node
// Regenerates backend/fixtures/ai/evidence/ts-evidence.json (R3 Task 7).
//
// Provenance: the expected values come from the TS reference evidence index
// (.worktrees/m3/packages/ai/src/validator/{evidence,pv,extract,normalize}.ts) and chess.js from
// m3's node_modules. For each AI input fixture this script records what `buildEvidence` indexes
// (number and date paths, numbers, ratios, dates, game ids, positions, lines), and it records
// `formatNumber` over a grid of values and formats. Numbers are written as JS `String(n)` (the
// shortest round-trip form), so Rust's correctly rounded `str::parse` reads the exact double. The Rust test `tests/evidence.rs`
// (`ts_parity`) compares the Rust port against it. Sets are written sorted.
//
// Usage: node backend/tools/gen-evidence-golden.mjs (Node >= 22.18 strips TS types by default).
// Override the m3 package folder with M3_AI=<dir>. Output is deterministic.
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const backend = resolve(here, "..");
const wt = here.indexOf(join("", ".worktrees"));
const repo = wt > 0 ? here.slice(0, wt - 1) : resolve(backend, "..");
const pkg = process.env.M3_AI ?? join(repo, ".worktrees/m3/packages/ai");
const src = join(pkg, "src/validator");
const chessJs = pathToFileURL(join(pkg, "node_modules/chess.js/dist/esm/chess.js")).href;
const outDir = join(backend, "fixtures/ai/evidence");
const target = join(outDir, "ts-evidence.json");

const tmp = mkdtempSync(join(tmpdir(), "evidence-golden-"));
let mod;
try {
  for (const f of ["normalize.ts", "extract.ts", "pv.ts", "evidence.ts", "tokens.ts"]) {
    const text = readFileSync(join(src, f), "utf8")
      .replace(/from "\.\/(\w+)\.js"/g, 'from "./$1.ts"')
      .replace('from "chess.js"', `from "${chessJs}"`)
      .replace(/^import type .*input\/schema\.js";$/m, "");
    writeFileSync(join(tmp, f), text);
  }
  mod = await import(pathToFileURL(join(tmp, "evidence.ts")).href);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
const { buildEvidence, formatNumber } = mod;

const sorted = (xs) => [...xs].sort();
const num = (a, b) => a - b;
const obj = (m, f = (x) => x) =>
  Object.fromEntries([...m.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)).map(([k, v]) => [k, f(v)]));

const fixtures = {};
for (const name of ["b2-rapid", "b0-blitz", "early-read"]) {
  const input = JSON.parse(
    readFileSync(join(backend, `fixtures/ai/input/coaching-input.${name}.json`), "utf8"),
  );
  const ev = buildEvidence(input);
  fixtures[name] = {
    numberPaths: obj(ev.numberPaths, String),
    datePaths: obj(ev.datePaths),
    numbers: [...ev.numbers].sort(num).map(String),
    ratios: [...ev.ratios].sort(num).map(String),
    dates: sorted(ev.dates),
    gameIds: sorted(ev.gameIds),
    positions: obj(ev.positions, (p) => ({
      gameId: p.gameId,
      ply: p.ply,
      fen: p.fen,
      played: p.played,
      best: p.best,
      allowedSan: sorted(p.allowedSan),
      allowedUci: sorted(p.allowedUci),
      bestPvSan: p.bestPvSan,
      refutationPvSan: p.refutationPvSan,
    })),
    lines: obj(ev.lines, sorted),
    lineMoves: obj(ev.lineMoves),
  };
}

const values = [
  0, -0.04, 0.04, 0.05, -0.05, 0.3149, 0.005, 0.015, 0.125, 1, 1.005, 1.234, 1.235, 1.25, -1.25,
  1.5, -1.5, 2.5, -2.5, 3.5, 0.5, -0.5, 12.345, 99.995, 214.4, 3912, -3912, 1234567, 1234567.891,
  -0.001, 0.1 + 0.2, 1 / 3, 2 / 3, -2 / 3, 1e6, 123456.789, 0.999, -0.999, 4.2, -4.2, 0.3333333333333,
];
const formats = [null, "int", "pct0", "pct1", "dec1", "dec2", "signed1"];
const format = [];
for (const v of values) for (const f of formats) format.push([String(v), f, formatNumber(v, f)]);

let commit = "unknown";
try {
  commit = execFileSync("git", ["-C", pkg, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
} catch {}

mkdirSync(outDir, { recursive: true });
const out = {
  provenance: `node ${process.versions.node}, chess.js 1.4.0, m3 ${commit}`,
  fixtures,
  format,
};
writeFileSync(target, `${JSON.stringify(out, null, 1)}\n`);
console.log(`wrote ${target}`);
