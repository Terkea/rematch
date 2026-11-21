#!/usr/bin/env node
// Regenerates the TS_CORPUS block in backend/crates/ai/tests/extract_gaps.rs.
//
// Provenance: the expected values come from the TS reference validator
// (.worktrees/m3/packages/ai/src/validator/{normalize,extract}.ts). For each
// input below, this script records normalizeText, extractNumbers, extractMoves
// and extractDates. The Rust test `ts_parity::rust_matches_the_recorded_ts_output_on_the_edge_corpus`
// asserts equality, except for the inputs it lists as deliberate hardening
// deviations (ADR-R3-2). The Node, ICU and Unicode versions and the m3 commit
// are written into the generated block, because NFKC and \p{Nd} depend on them.
//
// Usage: node backend/tools/gen-validator-corpus.mjs (Node >= 22.18 strips TS types by default;
// older 22.x need --experimental-strip-types). Last run: Node 22.22.0, ICU 77.1, Unicode 16.0.
// Override the TS source folder with M3_VALIDATOR=<dir>. Output is deterministic.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const backend = resolve(here, "..");
const wt = here.indexOf(join("", ".worktrees"));
const repo = wt > 0 ? here.slice(0, wt - 1) : resolve(backend, "..");
const src = process.env.M3_VALIDATOR ?? join(repo, ".worktrees/m3/packages/ai/src/validator");
const target = join(backend, "crates/ai/tests/extract_gaps.rs");

const sp = (n) => " ".repeat(n);
const E = "\u{1F600}"; // an astral char: two UTF-16 units
const inputs = [
  "0-0-0x",
  "O-O-Ox",
  "O-O-O-O",
  "e7-e8=Qx",
  "e8=Qx",
  "e8(Q)x and e8(Qa",
  "a7a8qz",
  "g1f3e7e5",
  "Ng1\u2013f3 then Kxe1",
  "O\u2013O and 0\u20140",
  "o-o and oo",
  "1,234,5678 games",
  "1,2345 games",
  "12,345.5 points",
  "1,5.25",
  "Oct 14th: game",
  "Oct 14:30",
  "June 2026",
  "June 5 2026",
  "14 Oct, 2026x",
  "14 Oct 2026",
  "21st May 2026",
  "May 5",
  "MAY 5",
  "may 5",
  "Sept 3",
  "Sep. 3, 2026",
  "2026-10-14T",
  "12-2026-10-14",
  "3 Septembers",
  "Decx 5",
  "5 Mayor",
  "a half-open file",
  "double check",
  "double-check",
  "double checkmate",
  "twice as many",
  "half the games",
  "the second half",
  "half again as much",
  "half your moves",
  "one hundred thousand games",
  "thousand thousand games",
  "twenty-one games",
  "a dozen games",
  "two dozen moves",
  "three-quarters of games",
  "an eighth of the games",
  "two hundred twelve games",
  "ninety nine games",
  "nineteen ninety games",
  "one two three games",
  "a sixteenth",
  "fourty games",
  "three times as many",
  "eleven times",
  "2x",
  "2X faster",
  "2xy",
  "5 \u00d7 3",
  "\u22123",
  "+-5",
  "--5",
  "5%-6%",
  "1.2.3",
  ".5.5",
  "50 % \u2013 60 %",
  "about\u20285 games",
  "\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00\ud83d\ude00 about 5 games",
  "roughly                    5 games",
  "at   least 5",
  "approx. 7 games",
  "\u223c9 games",
  "per cent",
  "10 per  cent of games",
  "W12a W1e W123",
  "B6 B5",
  "1/2-1/2 and \u00bd-\u00bd",
  "about \u00bd of games",
  "3rd 4TH 21st",
  "Nxe5?? 12...Qh4+ 13. g3",
  "1. e4 e5 2. Nf3",
  "the e-pawn",
  "h8=Q",
  "exd8=Q+",
  "Rxe6 then d5 (e4) ...",
  "12 . e4",
  "12.\u2028e4",
  "play\u2028e4",
  "you PLAYED to e4",
  "pushed  to  d4",
  "Q\u00e9e5",
  "\u00e9e4",
  "e4\u00e9",
  "1\u2009000\u2009000 games",
  "1\u00a0000 0",
  "\u0661\u00a0000",
  "\u0663\u0661\u0664",
  "\uff11\uff12\uff13 games",
  "12\u200b34",
  "[[g:abc:2026-10-14]] 5 games",
  "since 2026-10-14 vs 2026-10-15",
  "2026-02-31",
  "on 31 Feb",
  "0 games",
  "00 games",
  "007 moves",
  "1e5 games",
  "3.0 games",
  "-0 games",
  // Fix round 1: JS \s members Rust's \s treats differently, and other separators.
  "12.\u{85}e4",
  "about\u{85}5 games",
  "three\u{85}games",
  "you played\u{1680}h4",
  "12.\te4 and 13.\u{b}d4",
  "about\u{2029}5 games",
  "five\u{2029}games",
  "Oct 14th, 2026",
  "Oct\u{b}14th,\u{85}2026",
  // Astral chars split by the 16-unit (bare move), 12-unit (half) and 20-unit (hedge) windows.
  ...[11, 12, 13, 14, 15].flatMap((k) => [`x${E}1.${sp(k)}e4`, `${E}${E}1.${sp(k)}e4`, `${E}...${sp(k)}d5`]),
  ...[4, 5, 6, 7, 8].flatMap((k) => [`${E}first${sp(k)}half of games`, `${E}last${sp(k)}half the games`]),
  ...[13, 14, 15, 16].flatMap((k) => [`${E}about${sp(k)}5 games`, `${E}${E}more than${sp(k - 4)}7 games`]),
  // The gap shape that made the bare-square scan quadratic, at a small size.
  `Nf3${sp(40)}${"x e4 ".repeat(8)}`,
  // Hardening deviations (ADR-R3-2): TS output recorded, Rust deliberately differs.
  "\u{663} games",
  "\u{966}\u{96f} games",
  "\u{435}4 and \u{41d}\u{435}4",
  "\u{39d}f3 \u{41a}\u{430}4",
  "N\u{301}f3",
  "e\u{308}4",
  "N\u{180f}f3 and N\u{1d173}f3 and N\u{fff0}f3",
  "O\u{2d7}O and 0\u{2e3a}0\u{2e3b}0 and O\u{58a}O\u{30fc}O",
  "e2\u{2d7}e4 and Ng1\u{2e3a}f3",
  "1\u{2006}000 and 1\u{2008}000 and 1\u{200a}000 games",
];

const tmp = mkdtempSync(join(tmpdir(), "validator-corpus-"));
try {
  writeFileSync(join(tmp, "normalize.ts"), readFileSync(join(src, "normalize.ts")));
  writeFileSync(
    join(tmp, "extract.ts"),
    readFileSync(join(src, "extract.ts"), "utf8").replace('"./normalize.js"', '"./normalize.ts"'),
  );
  const { extractNumbers, extractMoves, extractDates } = await import(pathToFileURL(join(tmp, "extract.ts")).href);
  const { normalizeText } = await import(pathToFileURL(join(tmp, "normalize.ts")).href);
  const esc = (s) => Array.from({ length: s.length }, (_, k) => s.charCodeAt(k)).map((c) => (c < 32 || c >= 127 ? String.fromCharCode(92) + "u" + c.toString(16).padStart(4, "0") : String.fromCharCode(c))).join("");
  const rows = inputs.map((s) =>
    esc(JSON.stringify({ s, t: normalizeText(s), n: extractNumbers(s), m: extractMoves(s), d: extractDates(s) })),
  );
  if (rows.some((r) => r.includes('"#'))) throw new Error("a row would end the raw string");
  let commit = "unknown";
  try {
    commit = execFileSync("git", ["-C", src, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  } catch {}
  const v = process.versions;
  const block = [
    "    // BEGIN GENERATED by backend/tools/gen-validator-corpus.mjs; do not edit by hand.",
    `    // node ${v.node}, icu ${v.icu}, unicode ${v.unicode}, m3 ${commit}, ${rows.length} inputs`,
    `    const TS_CORPUS: &str = r#"[`,
    rows.join(",\n"),
    `]"#;`,
    "    // END GENERATED",
  ].join("\n");
  const rs = readFileSync(target, "utf8");
  const re = /    \/\/ BEGIN GENERATED[\s\S]*?\/\/ END GENERATED/;
  if (!re.test(rs)) throw new Error(`no generated block in ${target}`);
  writeFileSync(target, rs.replace(re, () => block));
  console.log(`wrote ${rows.length} rows to ${target}`);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
