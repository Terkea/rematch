#!/usr/bin/env node
// Regenerates backend/fixtures/ai/prompts/ts-canonical.json (R3 Task 13).
//
// The expected values are what the TS reference `canonicalJson` / `inputHash` (TS plan B2:
// "sorted keys, no whitespace"; `inputHash` is its sha256) give in JavaScript: object keys
// sorted with the default `Array.prototype.sort` (UTF-16 code units), every primitive written by
// `JSON.stringify`. The Rust test `tests/prompts.rs` compares `canonical_json` and `input_hash`
// against it byte for byte.
//
// Usage: node backend/tools/gen-canonical-golden.mjs. Output is deterministic.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const backend = resolve(here, "..");

function canonicalJson(v) {
  if (Array.isArray(v)) return `[${v.map(canonicalJson).join(",")}]`;
  if (v !== null && typeof v === "object") {
    const keys = Object.keys(v).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(v[k])}`).join(",")}}`;
  }
  return JSON.stringify(v);
}

const sha256 = (s) => createHash("sha256").update(s, "utf8").digest("hex");

const fixtures = {};
for (const name of ["b2-rapid", "b0-blitz", "early-read"]) {
  const text = readFileSync(join(backend, `fixtures/ai/input/coaching-input.${name}.json`), "utf8");
  const c = canonicalJson(JSON.parse(text));
  fixtures[name] = { canonicalLength: c.length, inputHash: sha256(c) };
}

// Number and key edge cases, written as JSON source so both sides parse the same text.
const casesSource = [
  '{"b":1,"a":{"d":1,"c":2}}',
  "[48.0,-0.0,0.1,1e-7,1.5e-7,0.000001,1e21,1.2e21,123456789012345680000,0.13999999999999999,-2.5,100]",
  '{"Z":1,"a":2,"_":3,"é":4,"10":5,"9":6}',
  '{"s":"a<b\\u2028\\u0001\\"\\\\/","n":null,"t":true,"f":false,"e":[],"o":{}}',
];
const cases = casesSource.map((source) => ({ source, canonical: canonicalJson(JSON.parse(source)) }));

const out = {
  provenance: {
    generator: "backend/tools/gen-canonical-golden.mjs",
    node: process.version,
    rule: "keys sorted with Array.prototype.sort, primitives via JSON.stringify, no whitespace",
  },
  fixtures,
  cases,
};
mkdirSync(join(backend, "fixtures/ai/prompts"), { recursive: true });
writeFileSync(join(backend, "fixtures/ai/prompts/ts-canonical.json"), `${JSON.stringify(out, null, 2)}\n`);
