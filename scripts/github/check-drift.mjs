#!/usr/bin/env node
// The settings drift check (ADR 0095): compares .github/settings with what the workflow's
// read-only GITHUB_TOKEN can see of the live repository. On main and on the weekly schedule a
// difference fails the run; on a pull request (whose settings are applied only after merge) it
// is printed as a warning. Env: GITHUB_TOKEN, GITHUB_REPOSITORY, DRIFT_FAILS=1|0.
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { driftReport, findRuleset, loadSettings } from "./settings-lib.mjs";

const repo = process.env.GITHUB_REPOSITORY;
const token = process.env.GITHUB_TOKEN;
const fails = process.env.DRIFT_FAILS !== "0";
if (!repo || !token) {
  console.error("GITHUB_REPOSITORY and GITHUB_TOKEN are required");
  process.exit(2);
}

async function get(path) {
  const res = await fetch(`https://api.github.com/${path}`, {
    headers: {
      accept: "application/vnd.github+json",
      authorization: `Bearer ${token}`,
      "x-github-api-version": "2022-11-28",
      "user-agent": "rematch-settings-drift",
    },
  });
  const body = await res.json().catch(() => ({}));
  return res.ok ? body : { error: `HTTP ${res.status}: ${body.message ?? res.statusText}` };
}

async function getAll(path) {
  const all = [];
  for (let page = 1; page <= 10; page += 1) {
    const sep = path.includes("?") ? "&" : "?";
    const batch = await get(`${path}${sep}per_page=100&page=${page}`);
    if (!Array.isArray(batch)) return batch;
    all.push(...batch);
    if (batch.length < 100) break;
  }
  return all;
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const settings = loadSettings(root);

const repoInfo = await get(`repos/${repo}`);
if (repoInfo.error) {
  console.error(`::error::could not read ${repo}: ${repoInfo.error}`);
  process.exit(1);
}
const labels = await getAll(`repos/${repo}/labels`);
if (!Array.isArray(labels)) {
  console.error(`::error::could not read the labels: ${labels.error}`);
  process.exit(1);
}
let rulesets = await getAll(`repos/${repo}/rulesets`);
if (Array.isArray(rulesets)) {
  const found = findRuleset(rulesets, settings.ruleset.name);
  rulesets = {
    list: rulesets,
    detail: found ? await get(`repos/${repo}/rulesets/${found.id}`) : undefined,
  };
}

const { drift, notices } = driftReport(settings, { repo: repoInfo, labels, rulesets });
for (const n of notices) console.log(`::notice::${n}`);
if (drift.length === 0) {
  console.log("No drift: the live settings match .github/settings.");
  process.exit(0);
}
const level = fails ? "error" : "warning";
for (const d of drift) console.log(`::${level}::settings drift: ${d}`);
console.log(
  `${drift.length} difference(s). The maintainer runs: sh scripts/github/apply-settings.sh --dry-run, then without --dry-run.`,
);
process.exit(fails ? 1 : 0);
