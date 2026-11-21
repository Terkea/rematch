// Deterministic post-step for `drizzle-kit pull` output (ADR 0033). Usage: node normalise-generated.mjs <dir>
// Rewrites <dir>/schema.ts and <dir>/relations.ts in place. Every rule is documented in ADR 0033 and
// FAILS LOUDLY when its input pattern is missing, so a drizzle-kit upgrade cannot silently change the output.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
if (!dir) throw new Error("usage: normalise-generated.mjs <dir>");

function fail(msg) {
  console.error(`normalise-generated: ${msg}`);
  process.exit(1);
}

/** Splits source into [header, ...top-level `export const` blocks]; a block runs to the next one. */
function splitBlocks(src, file) {
  const starts = [...src.matchAll(/^export const (\w+) = /gm)];
  if (starts.length === 0) fail(`${file}: no top-level exports found`);
  const header = src.slice(0, starts[0].index);
  const blocks = starts.map((m, i) => ({
    name: m[1],
    text: src.slice(m.index, i + 1 < starts.length ? starts[i + 1].index : src.length).trimEnd(),
  }));
  return { header, blocks };
}

const byName = (a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);

/** Replace exactly `expected` occurrences (default 1) or fail. */
function replaceExact(text, pattern, replacement, what, expected = 1) {
  const count = [...text.matchAll(pattern)].length;
  if (count !== expected) fail(`${what}: expected ${expected} match(es), found ${count}`);
  return text.replace(pattern, replacement);
}

// ---------------------------------------------------------------- schema.ts
const schemaPath = join(dir, "schema.ts");
const schema = splitBlocks(readFileSync(schemaPath, "utf8"), "schema.ts");
const isEnum = (b) => b.text.includes("= pgEnum(");

for (const b of schema.blocks) {
  if (isEnum(b)) continue;
  // timestamps use Date objects, like the hand-written schema
  b.text = b.text.replace(/(timestamp\([^)]*?)mode: (["'])string\2/g, '$1mode: "date"');
  // drizzle-kit labels opclasses from the wrong column; Postgres picks the right default anyway
  b.text = b.text.replace(/\.op\((["'])\w+\1\)/g, "");
  // an empty text[] default is introspected as [""]; position_evals.pv has one
  if (b.name === "positionEvals") {
    b.text = replaceExact(
      b.text,
      /(pv: text\(\)\.array\(\)\.default\()\[(["'])\2\]\)/g,
      "$1sql`'{}'::text[]`)",
      "position_evals.pv",
    );
  }
  // the same for interactive_evals.moves (R3, ADR-R3-3)
  if (b.name === "interactiveEvals") {
    b.text = replaceExact(
      b.text,
      /(moves: text\(\)\.array\(\)\.default\()\[(["'])\2\]\)/g,
      "$1sql`'{}'::text[]`)",
      "interactive_evals.moves",
    );
  }
  // drizzle-kit emits an empty-string default as the broken literal `default(')`
  if (b.name === "aiSdkSessionEntries") {
    b.text = replaceExact(
      b.text,
      /(subpath: text\(\)\.default\()'\)/g,
      '$1"")',
      "ai_sdk_session_entries.subpath",
    );
  }
  if (b.name === "userEvents") {
    b.text = replaceExact(
      b.text,
      /bigserial\(\{ mode: (["'])bigint\1 \}\)/g,
      'bigserial({ mode: "number" })',
      "user_events.id",
    );
  }
  if (b.name === "linkedAccounts") {
    b.text = replaceExact(
      b.text,
      /jsonb\((["'])import_cursor\1\)/g,
      'jsonb("import_cursor").$type<ImportCursor>()',
      "linked_accounts.import_cursor",
    );
  }
  if (b.name === "importRuns") {
    b.text = replaceExact(
      b.text,
      /text\((["'])error_code\1\)/g,
      'text("error_code").$type<ImportErrorCode>()',
      "import_runs.error_code",
    );
  }
}
const positionEvals = schema.blocks.find((b) => b.name === "positionEvals");
if (!positionEvals?.text.includes("sql`'{}'::text[]`"))
  fail("position_evals.pv default was not normalised");
for (const b of schema.blocks) {
  if (
    /timestamp\([^)]*mode: (["'])string\1/.test(b.text) ||
    /\.op\(/.test(b.text) ||
    /default\(\[(["'])\1\]\)/.test(b.text)
  ) {
    fail(`${b.name}: unnormalised output left`);
  }
}

if (!schema.blocks.some((b) => b.text.includes('mode: "date"')))
  fail("no timestamp was normalised to mode: date");
const typeDefs = `import { sql } from "drizzle-orm";

/** Why an import run failed, from the typed platform errors. */
export type ImportErrorCode = "not_found" | "rate_limited" | "timeout" | "circuit_open" | "unknown";

/**
 * Import checkpoint on the account. Games stream newest first. \`oldestReached\` is the oldest
 * playedAt committed by a pass that has not finished yet (null when no pass is open), and
 * \`newestSeenAt\` is the playedAt watermark below which every game has been imported. ISO strings.
 */
export interface ImportCursor {
  oldestReached: string | null;
  newestSeenAt: string | null;
}
`;
// Enums first (tables evaluate them eagerly), then tables; each group sorted by name.
const sorted = [
  ...schema.blocks.filter(isEnum).sort(byName),
  ...schema.blocks.filter((b) => !isEnum(b)).sort(byName),
];
writeFileSync(
  schemaPath,
  `${schema.header.trimEnd()}\n${typeDefs}\n${sorted.map((b) => b.text).join("\n\n")}\n`,
);

// ------------------------------------------------------------- relations.ts
const relPath = join(dir, "relations.ts");
let relSrc = readFileSync(relPath, "utf8");
// ESM (nodenext): relative imports carry the .js extension
relSrc = replaceExact(
  relSrc,
  /from "\.\/schema"/g,
  'from "./schema.js"',
  "relations.ts schema import",
);
const rel = splitBlocks(relSrc, "relations.ts");
for (const b of rel.blocks) {
  // sort the `key: one(...)|many(...)` entries inside each block
  const m = b.text.match(/^([^\n]*=> \(\{\n)([\s\S]*)(\n\}\)\);?)$/);
  if (!m) fail(`relations.ts: unexpected shape of ${b.name}`);
  const entries = m[2].split(/\n(?=[ \t]*\w+: (?:one|many)\()/);
  if (entries.length !== [...m[2].matchAll(/^\s*\w+: (?:one|many)\(/gm)].length)
    fail(`relations.ts: could not split the entries of ${b.name}`);
  const key = (e) => e.match(/^\s*(\w+):/)[1];
  entries.sort((x, y) => (key(x) < key(y) ? -1 : key(x) > key(y) ? 1 : 0));
  b.text = `${m[1]}${entries.join("\n")}${m[3]}`;
}
rel.blocks.sort(byName);
writeFileSync(
  relPath,
  `${rel.header.trimEnd()}\n\n${rel.blocks.map((b) => b.text).join("\n\n")}\n`,
);
