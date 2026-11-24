#!/usr/bin/env node
// The planning half of apply-settings.sh (ADR 0095). It reads the GET responses the script saved
// in --live, prints the diff against .github/settings, and writes the writes to make as
// <out>/calls.tsv ("METHOD<TAB>PATH<TAB>BODY-FILE or -") with one JSON body file per call.
// Usage: node scripts/github/settings-plan.mjs --live DIR --repo OWNER/REPO --out DIR [--prune]
//          [--phase settings|issues|sub-issues]
// The phases run in that order (ADR 0099): the settings and milestones (with renames, ADR 0104),
// then the seed issues (created, moved or updated, epics first), then the sub-issue links, each
// against a fresh read.
//        node scripts/github/settings-plan.mjs --live DIR --epic-numbers
// prints the live numbers of the seeded epics, one per line, so the script can read their
// sub-issues.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  epicNumbers,
  loadSettings,
  planChanges,
  planSeedIssues,
  planSubIssues,
} from "./settings-lib.mjs";

const args = process.argv.slice(2);
const opt = (name) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const liveDir = opt("--live");
const repo = opt("--repo");
const out = opt("--out");
const phase = opt("--phase") ?? "settings";
const wantEpics = args.includes("--epic-numbers");
if (!liveDir || (!wantEpics && (!repo || !out))) {
  console.error(
    "usage: settings-plan.mjs --live DIR --repo OWNER/REPO --out DIR [--prune] [--phase P]\n" +
      "       settings-plan.mjs --live DIR --epic-numbers",
  );
  process.exit(2);
}
if (!["settings", "issues", "sub-issues"].includes(phase)) {
  console.error(`unknown phase: ${phase}`);
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

/** A list GET that must have worked. */
function readList(name) {
  const v = read(name, { lines: true });
  if (!Array.isArray(v)) {
    console.error(`could not read ${name}: ${v.error}`);
    process.exit(1);
  }
  return v;
}

if (wantEpics) {
  for (const n of epicNumbers(settings.roadmapIssues, readList("issues.jsonl"))) console.log(n);
  process.exit(0);
}

let plan;
if (phase === "settings") {
  const list = read("rulesets.jsonl", { lines: true });
  const live = {
    repo: read("repo.json"),
    labels: read("labels.jsonl", { lines: true }),
    rulesets: Array.isArray(list)
      ? {
          list,
          detail: existsSync(join(liveDir, "ruleset.json")) ? read("ruleset.json") : undefined,
        }
      : list,
    vulnerabilityReporting: read("vulnerability-reporting.json"),
    pages: read("pages.json"),
    forkApproval: read("actions-fork-approval.json"),
    actionsWorkflow: read("actions-workflow.json"),
    milestones: read("milestones.jsonl", { lines: true }),
  };
  if (Array.isArray(live.labels) === false && live.labels.error) {
    console.error(`could not read labels: ${live.labels.error}`);
    process.exit(1);
  }
  plan = planChanges(settings, live, repo, { prune: args.includes("--prune") });
} else if (phase === "issues") {
  plan = planSeedIssues(
    settings.roadmapIssues,
    readList("issues.jsonl"),
    readList("milestones.jsonl"),
    repo,
    settings.milestones,
  );
} else {
  const issues = readList("issues.jsonl");
  const subIssues = new Map();
  for (const n of epicNumbers(settings.roadmapIssues, issues)) {
    const file = `sub-issues-${n}.jsonl`;
    subIssues.set(n, existsSync(join(liveDir, file)) ? readList(file) : []);
  }
  plan = planSubIssues(settings.roadmapIssues, issues, subIssues, repo);
}

const none = {
  settings: "No changes: the repository matches .github/settings.",
  issues: "No issue changes: every seed issue exists and matches roadmap-issues.json.",
  "sub-issues": "No new sub-issue links.",
}[phase];
console.log(plan.lines.length === 0 ? none : plan.lines.join("\n"));

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
