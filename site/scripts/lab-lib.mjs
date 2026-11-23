// Pure helpers for the Experiments and Progress pages, the landing strip and the lag guard.
// sync-content.mjs calls them; test/lab.test.ts covers them. Sources: lab/experiments/EXP-*/summary.md
// and docs/progress.md.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { BASE, GITHUB_BLOB, REPO, rewriteMarkdown } from "./content-lib.mjs";

export const STATUSES = ["pre-registered", "running", "decided"];
export const STATUS_LABEL = {
  "pre-registered": "Pre-registered",
  running: "Running",
  decided: "Decided",
};

const FIELDS = ["id", "title", "status", "algorithm", "issue", "updated"];

/** Parses a summary.md: flat `key: value` front matter and the hyphen bullets of the body. */
export function parseSummary(text, folder = "") {
  const src = text.replace(/\r\n/g, "\n");
  const m = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(src);
  if (!m) throw new Error(`${folder}/summary.md: front matter is missing`);
  const meta = {};
  for (const line of (m[1] ?? "").split("\n")) {
    const kv = /^([a-z_]+):\s*(.*)$/.exec(line.trim());
    if (!kv) continue;
    let v = (kv[2] ?? "").trim();
    if (/^(".*"|'.*')$/.test(v)) v = v.slice(1, -1);
    meta[kv[1] ?? ""] = v;
  }
  for (const f of FIELDS) {
    if (!meta[f]) throw new Error(`${folder}/summary.md: front matter has no "${f}"`);
  }
  if (!STATUSES.includes(meta.status)) {
    throw new Error(
      `${folder}/summary.md: status "${meta.status}" is not one of ${STATUSES.join(", ")}`,
    );
  }
  const bullets = [];
  for (const line of (m[2] ?? "").split("\n")) {
    if (/^- /.test(line)) bullets.push(line.slice(2).trim());
    else if (/^\s+\S/.test(line) && bullets.length)
      bullets[bullets.length - 1] += ` ${line.trim()}`;
  }
  return { ...meta, issue: String(meta.issue).replace(/^#/, ""), bullets };
}

/** Every experiment under `<labDir>/experiments`, newest `updated` first (then id, descending). */
export function loadExperiments(labDir) {
  const dir = join(labDir, "experiments");
  if (!existsSync(dir)) return [];
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    const sum = join(dir, name, "summary.md");
    if (!/^EXP-\d+/.test(name) || !existsSync(sum)) continue;
    const exp = parseSummary(readFileSync(sum, "utf8"), name);
    const has = (f) => existsSync(join(dir, name, f));
    out.push({
      ...exp,
      folder: name,
      slug: name.toLowerCase(),
      files: {
        preregistration: has("preregistration.md"),
        results: has("results.json"),
        review: has("review.md"),
        decision: has("decision.md"),
      },
    });
  }
  return out.sort((a, b) => b.updated.localeCompare(a.updated) || b.id.localeCompare(a.id));
}

/** Experiments whose folder holds a decision.md but whose summary does not say `decided`. */
export function lagging(exps) {
  return exps.filter((e) => e.files.decision && e.status !== "decided").map((e) => e.folder);
}

/** Folders with a decision.md and no summary.md at all. */
export function missingSummaries(labDir) {
  const dir = join(labDir, "experiments");
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter(
    (n) =>
      /^EXP-\d+/.test(n) &&
      existsSync(join(dir, n, "decision.md")) &&
      !existsSync(join(dir, n, "summary.md")),
  );
}

const badge = (status) =>
  `<span class="lab-badge lab-badge-${status}">${STATUS_LABEL[status]}</span>`;
const esc = (s) => s.replace(/</g, "&lt;");
const issueUrl = (n) => `https://github.com/${REPO}/issues/${n}`;
const fileUrl = (e, f) => `${GITHUB_BLOB}/lab/experiments/${e.folder}/${f}`;

export function experimentPage(e) {
  const links = [];
  if (e.files.preregistration)
    links.push(`[Pre-registration](${fileUrl(e, "preregistration.md")})`);
  if (e.files.results) links.push(`[Results](${fileUrl(e, "results.json")})`);
  if (e.files.review) links.push(`[Review](${fileUrl(e, "review.md")})`);
  if (e.files.decision) links.push(`[Decision](${fileUrl(e, "decision.md")})`);
  links.push(`[Issue #${e.issue}](${issueUrl(e.issue)})`);
  return [
    `${badge(e.status)} Run on ${esc(e.algorithm)} · updated ${e.updated}`,
    "",
    ...e.bullets.map((b) => `- ${b}`),
    "",
    links.map((l) => `- ${l}`).join("\n"),
    "",
    `<p class="generated-source">Generated at build time from <a href="${GITHUB_BLOB}/lab/experiments/${e.folder}/summary.md">${e.folder}/summary.md</a>; edit it there.</p>`,
    "",
  ].join("\n");
}

export function experimentsIndex(exps) {
  const intro =
    "An experiment asks one question about the recommendation algorithm, with the hypothesis, the metric and the decision rule committed before anything runs. Whatever the result, it is decided by that rule and published. The method is in [How Rematch is evaluated](" +
    `${GITHUB_BLOB}/docs/evaluation.md#how-we-do-science-on-the-algorithm` +
    ").";
  if (!exps.length) {
    return `${intro}\n\nNo experiment has a summary yet. The first, EXP-0001 and EXP-0002, are being pre-registered; they appear here when their \`summary.md\` is merged.\n`;
  }
  const rows = exps.map(
    (e) =>
      `<li class="lab-item"><a class="lab-question" href="${BASE}/docs/experiments/${e.slug}/">${esc(e.id)}: ${esc(e.title)}</a>${badge(e.status)}<span class="lab-meta">Run on ${esc(e.algorithm)} · ${e.updated}</span></li>`,
  );
  return `${intro}\n\n<ul class="lab-list">\n${rows.join("\n")}\n</ul>\n`;
}

const ENTRY = /^## (\d{4}-\d{2}-\d{2})(?::| -|\s)\s*(.+)$/;

/** Milestones from docs/progress.md: `## YYYY-MM-DD: title` then bullets, newest first. */
export function parseProgress(text) {
  const out = [];
  let cur = null;
  for (const line of text
    .replace(/\r\n/g, "\n")
    .replace(/<!--[\s\S]*?-->/g, "")
    .split("\n")) {
    const h = ENTRY.exec(line);
    if (h) {
      cur = { date: h[1], title: (h[2] ?? "").trim(), bullets: [] };
      out.push(cur);
    } else if (cur && /^- /.test(line)) cur.bullets.push(line.slice(2).trim());
    else if (cur && /^\s+\S/.test(line) && cur.bullets.length)
      cur.bullets[cur.bullets.length - 1] += ` ${line.trim()}`;
  }
  return out
    .map((m, i) => ({ m, i }))
    .sort((a, b) => b.m.date.localeCompare(a.m.date) || a.i - b.i)
    .map((x) => x.m);
}

export function progressPage(text) {
  const body = rewriteMarkdown(text.replace(/<!--[\s\S]*?-->\s*/g, ""), "docs/progress.md").replace(
    /^# [^\n]*\n+/,
    "",
  );
  return `${body.trimEnd()}\n\n<p class="generated-source">Generated at build time from <a href="${GITHUB_BLOB}/docs/progress.md">docs/progress.md</a>; edit it there.</p>\n`;
}

/** The landing strip: the three newest experiments and the newest milestone. Empty-safe. */
export function labStrip(exps, milestones) {
  const strip = (s) =>
    s
      .replace(/\s*\((?:\[[^\]]+\]\([^)]*\)[,\s]*)+\)/g, "")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/[`*]/g, "");
  const m = milestones[0];
  return {
    experiments: exps.slice(0, 3).map((e) => ({
      id: e.id,
      title: e.title,
      status: e.status,
      statusLabel: STATUS_LABEL[e.status],
      updated: e.updated,
      href: `${BASE}/docs/experiments/${e.slug}/`,
    })),
    milestone: m
      ? {
          date: m.date,
          title: strip(m.title),
          first: strip(m.bullets[0] ?? ""),
          href: `${BASE}/docs/progress/`,
        }
      : null,
  };
}
