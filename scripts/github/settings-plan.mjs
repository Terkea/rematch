#!/usr/bin/env node
// The planning half of apply-settings.sh (ADR 0095). It reads the GET responses the script saved
// in --live, prints the diff against .github/settings, and writes the writes to make as
// <out>/calls.tsv ("METHOD<TAB>PATH<TAB>BODY-FILE or -") with one JSON body file per call.
// Usage: node scripts/github/settings-plan.mjs --live DIR --repo OWNER/REPO --out DIR [--prune]
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { loadSettings, planChanges } from "./settings-lib.mjs";

const args = process.argv.slice(2);
const opt = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const liveDir = opt("--live");
const repo = opt("--repo");
const out = opt("--out");
if (!liveDir || !repo || !out) {
  console.error("usage: settings-plan.mjs --live DIR --repo OWNER/REPO --out DIR [--prune]");
  process.exit(2);
}

/** A saved GET response, or {error} with gh's message when the GET failed. */
function read(name, { lines = false } = {}) {
  const err = join(liveDir, `${name}.err`);
  if (existsSync(err)) {
    const msg = readFileSync(err, "utf8").trim().split(/\r?\n/).filter(Boolean).pop() ?? "failed";
    return { error: msg.replace(/^gh: /, "") };
  }
  const text = readFileSync(join(liveDir, name), "utf8");
  if (lines) {
    return text
      .split(/\r?\n/)
      .filter((l) => l.trim() !== "")
      .map((l) => JSON.parse(l));
  }
  return JSON.parse(text);
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const settings = loadSettings(root);
const list = read("rulesets.jsonl", { lines: true });
const live = {
  repo: read("repo.json"),
  labels: read("labels.jsonl", { lines: true }),
  rulesets: Array.isArray(list)
    ? { list, detail: existsSync(join(liveDir, "ruleset.json")) ? read("ruleset.json") : undefined }
    : list,
  vulnerabilityReporting: read("vulnerability-reporting.json"),
  pages: read("pages.json"),
};
if (Array.isArray(live.labels) === false && live.labels.error) {
  console.error(`could not read labels: ${live.labels.error}`);
  process.exit(1);
}

const plan = planChanges(settings, live, repo, { prune: args.includes("--prune") });
console.log(
  plan.lines.length === 0
    ? "No changes: the repository matches .github/settings."
    : plan.lines.join("\n"),
);

const tsv = plan.calls.map((c, i) => {
  let body = "-";
  if (c.body !== undefined) {
    body = join(out, `body-${i}.json`).replaceAll("\\", "/");
    writeFileSync(body, JSON.stringify(c.body));
  }
  return `${c.method}\t${c.path}\t${body}`;
});
writeFileSync(join(out, "calls.tsv"), tsv.length ? `${tsv.join("\n")}\n` : "");
console.log(`\n${plan.calls.length} write(s) planned.`);
