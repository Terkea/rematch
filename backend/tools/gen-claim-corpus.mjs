#!/usr/bin/env node
// Regenerates the TS_CLAIM_CORPUS block in backend/crates/ai/tests/validator_adversarial.rs
// (R3 Task 8, the claim validator).
//
// Provenance. TS M3 has no token-era claim validator (its A4/A5 validator predates facts by
// reference), so this is a leaf-module parity check: `tsValidateText` below composes the R3
// check order (ADR-R3-4 "Claim validator", before fix round 1) from m3's own modules:
// parseTokens (tokens.ts), findLeaks (leaks.ts), normalizeText + extractMoves/Numbers/Dates
// (normalize.ts, extract.ts), buildEvidence + resolveToken (evidence.ts, chess.js from m3's
// node_modules). The language rules are the TS plan A5 regexes verbatim, as JS RegExps, in their
// token forms (the evidence window and the needMore count are num tokens). The rows record the
// deduplicated codes TS gives. The Rust test `ts_parity::rust_rejects_everything_ts_rejects`
// asserts superset safety on every row (TS rejects => Rust rejects), and that the rows only Rust
// rejects are exactly its STRICTER list. The fix round 1 rules TS never had are pinned by the
// Rust-only `must_reject` vectors instead.
//
// Usage: node backend/tools/gen-claim-corpus.mjs (Node >= 22.18 strips TS types by default).
// Override the m3 package folder with M3_AI=<dir>. Output is deterministic.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
const target = join(backend, "crates/ai/tests/validator_adversarial.rs");

const tmp = mkdtempSync(join(tmpdir(), "claim-corpus-"));
let ev0, tk, lk, ex, nm;
try {
  for (const f of ["normalize.ts", "extract.ts", "pv.ts", "evidence.ts", "tokens.ts", "leaks.ts"]) {
    const text = readFileSync(join(src, f), "utf8")
      .replace(/from "\.\/(\w+)\.js"/g, 'from "./$1.ts"')
      .replace('from "chess.js"', `from "${chessJs}"`)
      .replace(/^import type .*input\/schema\.js";$/m, "");
    writeFileSync(join(tmp, f), text);
  }
  const imp = (f) => import(pathToFileURL(join(tmp, f)).href);
  [ev0, tk, lk, ex, nm] = await Promise.all(
    ["evidence.ts", "tokens.ts", "leaks.ts", "extract.ts", "normalize.ts"].map(imp),
  );
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
const { buildEvidence, resolveToken } = ev0;
const { parseTokens } = tk;
const { findLeaks } = lk;
const { extractMoves, extractNumbers, extractDates } = ex;
const { normalizeText } = nm;

// ---- The TS reference validator ----

const BANNED = [
  /\bguarantee[ds]?\b/i,
  /\bdefinitely\b/i,
  /\bcertainly\b/i,
  /\bproven?\b/i,
  /\bwill (gain|reach|get you to|raise your rating|improve your rating)\b/i,
];
const MARKETING =
  /\b(upgrade|subscribe|subscription|premium|pro plan|discount|free trial|refer a friend|share your (report|card))\b/i;
const CAUSAL = /\b(caused|because of|due to|the reason (is|was)|led to|proves?)\b/i;
const HEDGE = /\b(may|might|possibly|could|coincid\w*|around the time|at about the same time)\b/i;
const LIKELIHOOD = /\b(no clear change yet|very likely|unclear yet|likely)\b/gi;

const likelihoodPhrase = (p) =>
  p === null || p === undefined
    ? null
    : p >= 0.95
      ? "very likely"
      : p >= 0.85
        ? "likely"
        : p >= 0.5
          ? "unclear yet"
          : "no clear change yet";
const hasNum = (tokens, path) => tokens.some((t) => t.kind === "num" && t.path === path);

const banned = () => (s) => (BANNED.some((r) => r.test(s)) ? ["banned_phrase"] : []);
const marketing = () => (s) => (MARKETING.test(s) ? ["marketing"] : []);
const likelihood = (req) => (s) =>
  [...s.matchAll(LIKELIHOOD)].some((m) => m[0].toLowerCase() !== req)
    ? ["likelihood_mismatch"]
    : [];
const evidenceWindow = (tc) => (s, tokens) =>
  hasNum(tokens, "basis.analysedGames") && new RegExp(`\\b${tc}\\b`, "i").test(s)
    ? []
    : ["missing_evidence_window"];
const causal = () => (s) => [
  ...(CAUSAL.test(s) ? ["causal_claim"] : []),
  ...(HEDGE.test(s) ? [] : ["missing_hedge"]),
];
const insufficient = (area) => (_s, tokens) =>
  hasNum(tokens, `areaScores.${area}.needMore`) ? [] : ["insufficient_data_not_stated"];

/** The rule set of a row kind: `report_text_rules` kinds, plus `causal` (change notes) and `chat`. */
function rulesFor(input, row) {
  const r = { req: false, lineKeys: null, implicit: false, extra: [banned(), marketing()] };
  switch (row.k) {
    case "weakness":
    case "focus": {
      r.req = true;
      const w = input.weaknesses.find((x) => x.id === row.w);
      r.extra.push(
        likelihood(likelihoodPhrase(w ? w.pImp : null)),
        evidenceWindow(input.timeClass),
      );
      break;
    }
    case "summary":
      r.extra.push(evidenceWindow(input.timeClass));
      break;
    case "area":
      if (input.areaScores.some((a) => a.area === row.a && !a.sufficient))
        r.extra.push(insufficient(row.a));
      break;
    case "opening":
      r.req = true;
      r.lineKeys = row.l ? [row.l] : [];
      break;
    case "causal":
      r.extra = [causal()];
      break;
    case "chat":
      r.implicit = true;
      break;
    case "item":
      break;
    default:
      throw new Error(`kind ${row.k}`);
  }
  return r;
}

const POS = new Set(["move", "best", "pv"]);

/** Codes of the first failing stage, deduplicated, or []. */
function tsValidateText(ev, input, row) {
  const rules = rulesFor(input, row);
  const out = (codes) => [...new Set(codes)];
  let codes = [];
  for (const [g, p] of row.c) if (!ev.positions.has(`${g}#${p}`)) codes.push("unknown_citation");
  if (rules.req && row.c.length === 0) codes.push("missing_citation");
  if (codes.length) return out(codes);
  const parsed = parseTokens(row.s);
  if (parsed.errors.length) return ["token_syntax"];
  const s = parsed.stripped;
  if (findLeaks(s).length) return ["free_text_fact"];
  const t = normalizeText(s);
  const m = extractMoves(t);
  codes = [
    ...(m.san.length || m.uci.length ? ["unknown_move"] : []),
    ...(extractNumbers(t).length ? ["unknown_number"] : []),
    ...(extractDates(t).length ? ["unknown_date"] : []),
  ];
  if (codes.length) return codes;
  if (parsed.tokens.some((k) => resolveToken(ev, k) === undefined)) return ["unresolved_token"];
  if (!rules.implicit) {
    const cited = new Set(row.c.map(([g, p]) => `${g}#${p}`));
    if (parsed.tokens.some((k) => POS.has(k.kind) && !cited.has(`${k.gameId}#${k.ply}`)))
      return ["uncited_reference"];
  }
  if (
    rules.lineKeys &&
    parsed.tokens.some((k) => k.kind === "line" && !rules.lineKeys.includes(k.lineKey))
  )
    return ["unknown_line"];
  return out(rules.extra.flatMap((rule) => rule(s, parsed.tokens)));
}

// ---- Inputs ----

// The 56 TS leak escapes, read from the leak corpus tool so the two stay in sync.
const leakTool = readFileSync(join(here, "gen-validator-corpus.mjs"), "utf8");
const escSrc = leakTool.match(/const ESCAPES = (\[[\s\S]*?\]);/);
if (!escSrc) throw new Error("ESCAPES not found");
const ESCAPES = new Function(`return ${escSrc[1]}`)();
if (ESCAPES.length !== 56) throw new Error(`${ESCAPES.length} escapes`);

const W1 = ["li-w1-0", 5];
const W3 = ["li-w3-0", 16];
const OP = ["li-op1", 12];
const WIN = "Over your last {{num:basis.analysedGames|int}} analysed rapid games";
const row = (s, c = [W1], k = "item", extra = {}) => ({ s, c, k, f: "b2-rapid", ...extra });
const wk = (s, c = [W1], w = "W1") => row(s, c, "weakness", { w });

const rows = [
  // Each escape inside a cited, otherwise valid paragraph.
  ...ESCAPES.map((e) => row(`You played {{move:li-w1-0:5}} and then ${e} later.`)),
  // The adversarial cases.
  row("you left a piece loose", [["li-w1-0", 6]]),
  row("you left a piece loose", [["li-zz-9", 5]]),
  row("{{move:li-w1-1:9}} was the slip"),
  row("Qxf7# was there"),
  row("{{move:li-w1-0:5}} left a piece loose"),
  row("you blunder 4.7 times per game"),
  row("{{num:weaknesses.W1.posteriorRate|pct0}} of games"),
  row("{{num:weaknesses.W1.nope}} of games"),
  row("{{pv:li-w1-0:5:best:99}} was best"),
  row("five blunders in a row"),
  wk(`${WIN} you left pieces loose.`, []),
  row("N​xf7 lost"),
  row("４.７ blunders"),
  row("{{line:b-sicilian-najdorf:2}} starts it, as in {{move:li-op1:12}}", [OP], "opening", {
    l: "b-sicilian-najdorf",
  }),
  row("{{line:b-sicilian-najdorf:2}} starts it, as in {{move:li-op1:12}}", [OP], "opening", {
    l: "w-italian",
  }),
  row("see {{move:li-w1-0:5"),
  // Tokens: every kind, scoping, resolution and syntax.
  row("{{best:li-w1-0:5}} then {{pv:li-w1-0:5:refutation:1}} on {{sq:e4}}"),
  row("{{best:li-w1-1:9}} was better"),
  row("{{pv:li-w1-1:9:best:1}} was better"),
  row("{{move:li-w1-1:9}} was the slip", [W1, ["li-w1-1", 9]]),
  row("{{move:li-w1-1:9}} was the slip", [], "chat"),
  row("{{line:w-italian}} is your line"),
  row("{{line:w-nope}} is your line"),
  row("{{line:w-italian:99}} is your line"),
  row("{{date:basis.windowStart}} was the start"),
  row("{{date:basis.nope}} was the start"),
  row("{{num:basis.importedGames|int}} imported and {{num:basis.analysedGames}} analysed"),
  row("{{num:weaknesses.W1.posteriorRate|bogus}} of games"),
  row("this is {{offtopic}} here"),
  row("{{offtopic}}"),
  row("{{eng:0123456789abcdef0123456789abcdef:1}} was the engine view"),
  row("a {{move:li-w1-0:5}} } b"),
  row("{{{sq:e4}}} here"),
  row("{{mo​ve:li-w1-0:5}} here"),
  row("{{move:li-w1-0:５}} here"),
  row("{{sq:ｅ４}} here"),
  row("{{move:li-w1-0:5\n}} here"),
  // Defence in depth.
  row("you lose half the games"),
  row("twice as often as before"),
  row("a double attack"),
  row("you played h4 too early"),
  // Citations.
  row("steady play", []),
  row("steady play", [W1, W1]),
  row("steady play", [
    ["li-w1-0", 6],
    ["nope", 1],
  ]),
  wk(`${WIN} you left pieces loose, as with {{move:li-w1-0:5}}.`),
  wk(`${WIN} forks slipped past you, as with {{move:li-w3-0:16}}.`, [W3], "W3"),
  wk("In rapid games you leave pieces loose, as in {{move:li-w1-0:5}}."),
  wk("Over your last {{num:basis.importedGames|int}} rapid games you leave pieces loose."),
  wk("Over your last {{num:basis.analysedGames|int}} games you leave pieces loose."),
  wk(`${WIN} this is likely to stick.`),
  wk(`${WIN} this is unlikely to matter.`),
  wk(`${WIN} this is very likely to stick.`, [W1], "W9b"),
  wk(`${WIN} change is no clear change yet.`, [W1], "W9b"),
  wk(`${WIN} this is unclear yet.`, [W1], "W11"),
  row(`${WIN} you lose on time.`, [W1], "focus", { w: "general" }),
  row(`${WIN} your play was steady.`, [], "summary"),
  row("Your play was steady.", [], "summary"),
  // Banned and marketing phrases.
  row("You are guaranteed to gain rating points."),
  row("This will improve your rating."),
  row("This will raise your rating soon."),
  row("You will reach the next level."),
  row("DEFINITELY a pattern."),
  row("It is certainly so."),
  row("A proven plan."),
  row("It was proved."),
  row("Guarantor duties aside, play on."),
  row("Provenance matters."),
  row("Upgrade to Pro to see more."),
  row("Look for upgrades of your worst piece."),
  row("Share your report with a friend."),
  row("Share your card."),
  row("Try a free trial."),
  row("Subscribe for tips."),
  row("A premium move."),
  row("Refer a friend."),
  // Causal claims and hedges (change notes).
  row("This may have been caused by switching to blitz.", [W1], "causal"),
  row("This was caused by switching to blitz.", [W1], "causal"),
  row("This started around the time you switched to blitz.", [W1], "causal"),
  row("You switched to blitz.", [W1], "causal"),
  row("It could be due to the clock.", [W1], "causal"),
  row("The reason is the clock, it might be.", [W1], "causal"),
  row("This coincides with your switch.", [W1], "causal"),
  row("At about the same time you changed openings.", [W1], "causal"),
  row("That led to losses, possibly.", [W1], "causal"),
  row("This proves nothing, but it may matter.", [W1], "causal"),
  row("Mayhem at the board.", [W1], "causal"),
  // Areas (early read: endgame, calculation and mindset are below their sample).
  row("Your endgame sample is still small.", [], "area", { a: "endgame", f: "early-read" }),
  row("We need {{num:areaScores.endgame.needMore|int}} more endgame games.", [], "area", {
    a: "endgame",
    f: "early-read",
  }),
  row("We need {{num:areaScores.calculation.needMore|int}} more games.", [], "area", {
    a: "endgame",
    f: "early-read",
  }),
  row("Your openings look steady.", [], "area", { a: "openings", f: "early-read" }),
  row("You found {{move:li-s1:11}}.", [], "area", { a: "openings" }),
  // Deliberate hardening: TS accepts, Rust rejects (union of the hardened forms).
  row("You are guаranteed a win."),
  row("Up᠏grade now."),
  row("Discоunt codes inside."),
  row("It may be caúsed by time trouble.", [W1], "causal"),
  row("Prеmium tips."),
  // Disguised hedges stay missing in both.
  row("It mаy be the clock.", [W1], "causal"),
];

const inputs = {};
for (const f of new Set(rows.map((r) => r.f))) {
  inputs[f] = JSON.parse(
    readFileSync(join(backend, `fixtures/ai/input/coaching-input.${f}.json`), "utf8"),
  );
}
const evs = Object.fromEntries(Object.entries(inputs).map(([f, i]) => [f, buildEvidence(i)]));

const esc = (s) =>
  Array.from({ length: s.length }, (_, k) => s.charCodeAt(k))
    .map((c) =>
      c < 32 || c >= 127
        ? String.fromCharCode(92) + "u" + c.toString(16).padStart(4, "0")
        : String.fromCharCode(c),
    )
    .join("");
const out = rows.map((r) =>
  esc(JSON.stringify({ ...r, ts: tsValidateText(evs[r.f], inputs[r.f], r) })),
);
if (out.some((r) => r.includes('"#'))) throw new Error("a row would end the raw string");

let commit = "unknown";
try {
  commit = execFileSync("git", ["-C", pkg, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
} catch {}
const v = process.versions;
const block = [
  "    // BEGIN GENERATED by backend/tools/gen-claim-corpus.mjs; do not edit by hand.",
  `    // node ${v.node}, icu ${v.icu}, unicode ${v.unicode}, m3 ${commit}, ${out.length} rows`,
  `    const TS_CLAIM_CORPUS: &str = r#"[`,
  out.join(",\n"),
  `]"#;`,
  "    // END GENERATED",
].join("\n");
const rs = readFileSync(target, "utf8");
const re = /    \/\/ BEGIN GENERATED[\s\S]*?\/\/ END GENERATED/;
if (!re.test(rs)) throw new Error(`no generated block in ${target}`);
writeFileSync(
  target,
  rs.replace(re, () => block),
);
console.log(`wrote ${out.length} rows to ${target}`);
