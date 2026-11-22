#!/usr/bin/env node
// Regenerates the TS_CORPUS block in backend/crates/ai/tests/extract_gaps.rs and the
// TS_LEAK_CORPUS block in backend/crates/ai/tests/leaks.rs (tokens.ts and leaks.ts, Task 6).
//
// Provenance: the expected values come from the TS reference validator
// (.worktrees/m3/packages/ai/src/validator/{normalize,extract}.ts). For each
// input below, this script records normalizeText, extractNumbers, extractMoves
// and extractDates. The Rust test `ts_parity::rust_matches_the_recorded_ts_output_on_the_edge_corpus`
// asserts equality, except for the inputs it lists as deliberate hardening
// deviations (ADR 0058). The Node, ICU and Unicode versions and the m3 commit
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
const leakTarget = join(backend, "crates/ai/tests/leaks.rs");

function writeBlock(file, block, n) {
  const rs = readFileSync(file, "utf8");
  const re = /    \/\/ BEGIN GENERATED[\s\S]*?\/\/ END GENERATED/;
  if (!re.test(rs)) throw new Error(`no generated block in ${file}`);
  writeFileSync(file, rs.replace(re, () => block));
  console.log(`wrote ${n} rows to ${file}`);
}

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
  // Hardening deviations (ADR 0058): TS output recorded, Rust deliberately differs.
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
  // Fix round 2: a folded neighbour must not hide what TS finds (the union keeps the TS result).
  "Nf3\u{e9}",
  "\u{e9}Nf3",
  "Nf3\u{430}",
  "Nf3\u{663}",
  "Nf3\u{3bd}",
  "Qxe5\u{e9} wins",
  "e2e4\u{e9}",
  "e2-e4\u{430}",
  "O-O\u{e9}",
  "\u{e8}Rd1 and Rd1\u{e8}",
  "14 Oct\u{e9}",
  "5\u{e9} games and 7\u{663} games",
];

// Leak corpus (tests/leaks.rs): canonicalForTokens, parseTokens and findLeaks on the raw and the
// token-stripped text. The 56 TS ESCAPES come first (keep them in sync with leaks.test.ts).
const ESCAPES = [
  "12. Nf3 d5", "1. e4 e5 2. Nf3 Nc6", "Instead d5 was best", "...d6", "Nxe5 d6 (e4)", "e2-e4",
  "Ng1-f3", "e2xd5", "e8Q", "e8(Q)", "O–O", "O‑O‑O", "O—O", "O−O", "O-O",
  "0-0-0", "N⁦f3", "N͏f3", "♞️f3", "forty percent", "thirty games",
  "ten per cent", "one game", "three openings", "a third of games", "twenty-one games", "2.5x",
  "3 times", "nf3", "NF3", "♙e4", "Pe4", "Nf3Nc6", "exd6e.p.", "exd6 e.p.",
  "the 14th of October", "14 Oct", "14/10", "2026-10-14", ".5%", "1,5", "1 000", "1 000",
  "１２ games", "١٢ games", "half of your games", "a quarter", "a dozen",
  "twice as often", "the second game", "ten", "е4", "N​f‌3", "Nㅤf3", "♟",
  "♔",
];
const leakInputs = [
  ...ESCAPES,
  // Rust extras of the escape loop.
  "Ｎｆ３", "é4", "Nf3️", "twentieth move", "O‑O‑O",
  // Clean prose and token strings from the TS tests.
  "you tend to rush in sharp positions",
  "the {{sq:e4}} square",
  "you lost the {{move:li-w1a:6}} knight too early; {{best:li-w1a:6}} was safer",
  "your rate was {{num:weaknesses.W1.posteriorRate|pct0}} over {{num:basis.analysedGames|int}} games",
  "Pay attention to king safety and loose pieces. Castling early helps.",
  "a {{move:g1:6}} b {{best:g1:6}} c {{pv:g1:6:best:2}} d {{pv:g1:6:refutation:1}} e {{line:w-italian}} f {{line:w-italian:3}} g {{num:basis.analysedGames}} h {{num:weaknesses.W1.posteriorRate|pct1}} i {{date:basis.windowStart}} j {{sq:f7}}",
  "x {{move:g1:6}} y {{sq:e4}}",
  ...["{{move:g1}}", "{{move:g1:x}}", "{{move:g1:0}}", "{{pv:g1:6:middle:1}}", "{{pv:g1:6:best:0}}",
    "{{num:a.b|bogus}}", "{{num:}}", "{{sq:i9}}", "{{sq:e44}}", "{{nope:x}}", "{{move:g1:6",
    "move:g1:6}}", "{{}}"].map((b) => `see ${b} here`),
  "{{num:a.b|int}} {{num:a.b|pct0}} {{num:a.b|dec2}} {{num:a.b|signed1}}",
  "{{mo​ve:g1:６}}",
  "play O-O", "play ♞", "about 40", "about forty", "a 14 Oct date",
  // Token edges.
  "a {{move:g1}} {{num:a.b|}} {{nope:x}} } {{sq:e4",
  "{{line:w:italian:2}} {{line:w:italian:x}} {{num:a#1.b|int}}",
  "{{line:}} {{line}} {{num}} {{date:}} {{date:a..b}} {{sq:e4:x}} {{best:g 1:3}} {{move:g1:10000}}",
  "{{pv:g1:6:best:1000}} {{pv:g1:6:best}} {{move:g1:0012}} {{num:a.b|int|pct0}} {{num:a b}}",
  "{{{sq:e4}}} {{sq:e4}}} {{{{sq:e4}}}}",
  "été {{sq:e4}} and​ {{move:g1:6}} 😀 {{sq:a1}} {",
  "{{move:g1:6\n}} {{sq:ｅ４}} {{​sq:h8}}",
  // Number words: plurals, backtracking, boundaries.
  "tens tenses tenth tenths sevenths twenties forties nineties fifties",
  "often attention none alone bones someone",
  "per  cent percent percents percentage per cent per\u0085cent",
  "a couple of pairs, thrice, quadruple, halves",
  "TEN Twenty Million",
  "fourteenths and hundredths",
  "xten tenx 1ten ten1",
  "zeroes dozens",
  // SAN, promotions, e.p.
  "e8 = q) and a1 (n)x and h8qa and b1r",
  "g8 ( Q ) or c1=b!",
  "ep. e.p e p. e. p. xe.p. e.p.x",
  "a1-b2 a1xb2 a1—b2 a1－b2 a1˗b2",
  // Castling.
  "o-o-o-o 0-0x o - o ao-o o-o1 o--o 0– 0",
  "O-O-O! and O-O+",
  // Dates.
  "jan 5 janx 5 jan. the 3rd", "the 3rd of may", "3 of mayo", "end of march", "march the fifth",
  "14 junes", "1/2 12 . 3 2026 - 10 - 14", "JAN 5", "jan\u00855", "sept 3 sep 3", "x12 of oct",
  "12 of oct", "5th of june, 2026",
  // Confusables, case and other scripts.
  "Неро", "ΑΝΟ", "πάτησε",
  "İstanbul", "ΣΟΣ", "Α-Ο", "О-О",
  // Digits, percent and figurines.
  "‰ and ٪", "𝟙𝟚", "① ² Ⅻ", "🨀 🩯 🩰",
  "٣ games",
  // Hardening deviations (ADR 0058, Task 6): TS output recorded, Rust deliberately adds leaks.
  "fιveé games", "Օ-Օé", "t᠏en games", "tẃenty games",
  "O˗O", "օnce", "база",
  // The union keeps what TS finds next to a folded neighbour.
  "tené", "ten᠏x", "Nf3é", "O-Oé", "e.p.é", "éone game", "threeι",
  "Café talk: often your pieces are loose.", "Été and naïve plans",
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
  for (const f of ["tokens.ts", "leaks.ts"]) {
    writeFileSync(join(tmp, f), readFileSync(join(src, f), "utf8").replace('"./normalize.js"', '"./normalize.ts"'));
  }
  const { canonicalForTokens, parseTokens } = await import(pathToFileURL(join(tmp, "tokens.ts")).href);
  const { findLeaks } = await import(pathToFileURL(join(tmp, "leaks.ts")).href);
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
  writeBlock(target, block, rows.length);
  if (ESCAPES.length !== 56) throw new Error(`${ESCAPES.length} escapes`);
  const leakRows = leakInputs.map((s) => {
    const p = parseTokens(s);
    return esc(JSON.stringify({ s, c: canonicalForTokens(s), st: p.stripped, tk: p.tokens, er: p.errors, l: findLeaks(p.stripped), r: findLeaks(s) }));
  });
  if (leakRows.some((r) => r.includes('"#'))) throw new Error("a row would end the raw string");
  const leakBlock = [
    "    // BEGIN GENERATED by backend/tools/gen-validator-corpus.mjs; do not edit by hand.",
    `    // node ${v.node}, icu ${v.icu}, unicode ${v.unicode}, m3 ${commit}, ${leakRows.length} inputs`,
    `    const TS_LEAK_CORPUS: &str = r#"[`,
    leakRows.join(",\n"),
    `]"#;`,
    "    // END GENERATED",
  ].join("\n");
  writeBlock(leakTarget, leakBlock, leakRows.length);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
