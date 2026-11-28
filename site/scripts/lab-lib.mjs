// Pure helpers for the Experiments and Progress pages, the landing strip and the lag guard.
// sync-content.mjs calls them; test/lab.test.ts covers them. Sources: lab/experiments/EXP-*/summary.md,
// lab/leaderboard.json (the arena's records, ADR 0109) and docs/progress.md.
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
/**
 * @typedef {{ id: string, title: string, status: string, algorithm: string, issue: string,
 *   updated: string, rerun?: string, live: boolean, bullets: string[] }} Summary
 * @returns {Summary}
 */
export function parseSummary(text, folder = "") {
  const src = text.replace(/\r\n/g, "\n");
  const m = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(src);
  if (!m) throw new Error(`${folder}/summary.md: front matter is missing`);
  /** @type {Record<string, string>} */
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
  return {
    ...meta,
    issue: String(meta.issue).replace(/^#/, ""),
    live: meta.live === "true",
    bullets,
  };
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
        math: has("math.md"),
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

const RERUN_TEXT = {
  "per-pr": "on every pull request that changes the algorithm version",
  weekly: "every week",
  release: "on every release",
};

export function experimentPage(e) {
  const links = [];
  if (e.files.math) links.push(`[The maths](${BASE}/docs/experiments/${e.slug}/math/)`);
  if (e.files.preregistration)
    links.push(`[Pre-registration](${fileUrl(e, "preregistration.md")})`);
  if (e.files.results) links.push(`[Results](${fileUrl(e, "results.json")})`);
  if (e.files.review) links.push(`[Review](${fileUrl(e, "review.md")})`);
  if (e.files.decision) links.push(`[Decision](${fileUrl(e, "decision.md")})`);
  links.push(`[Issue #${e.issue}](${issueUrl(e.issue)})`);
  return [
    `${badge(e.status)} Run on ${esc(e.algorithm)} · updated ${e.updated}`,
    "",
    ...(e.live && RERUN_TEXT[e.rerun]
      ? [
          `Live: re-run against the champion ${RERUN_TEXT[e.rerun]} ([the leaderboard](${BASE}/docs/experiments/#leaderboard)).`,
          "",
        ]
      : []),
    ...e.bullets.map((b) => `- ${b}`),
    "",
    links.map((l) => `- ${l}`).join("\n"),
    "",
    `<p class="generated-source">Generated at build time from <a href="${GITHUB_BLOB}/lab/experiments/${e.folder}/summary.md">${e.folder}/summary.md</a>; edit it there.</p>`,
    "",
  ].join("\n");
}

/**
 * An experiment's maths page, from its math.md (ADR 0118): comments and the title dropped (the
 * page has its own), links rewritten, the maths left for KaTeX.
 */
export function mathPage(e, text) {
  return generatedPage(text, `lab/experiments/${e.folder}/math.md`);
}

/** algorithm/MATH.md as the site's page on the algorithm's maths. */
export function algorithmMathPage(text) {
  return generatedPage(text, "algorithm/MATH.md");
}

function generatedPage(text, repoPath) {
  const source = text.replace(/\r\n/g, "\n").replace(/<!--[\s\S]*?-->\s*/g, "");
  const body = rewriteMarkdown(source, repoPath).replace(/^# [^\n]*\n+/, "");
  return `${body.trimEnd()}\n\n<p class="generated-source">Generated at build time from <a href="${GITHUB_BLOB}/${repoPath}">${repoPath}</a>; edit it there.</p>\n`;
}

/**
 * lab/leaderboard.json, or null when the lab has none.
 * @returns {{ schema: string, specs?: Record<string, any[]>, records: any[] } | null}
 */
export function loadLeaderboard(labDir) {
  const path = join(labDir, "leaderboard.json");
  if (!existsSync(path)) return null;
  return JSON.parse(readFileSync(path, "utf8"));
}

const num = (x) => (typeof x === "number" && Number.isFinite(x) ? x.toFixed(3) : null);
const levelText = (l) =>
  typeof l === "number" ? `${Math.round(l * 1000) / 10}%` : String(l ?? "");
const VERDICT_TEXT = {
  better: "better than",
  worse: "worse than",
  "no clear difference": "no clear difference from",
  overridden: "worse, overridden by an ADR, against",
};

/**
 * The Leaderboard section: a row per algorithm version (the champion first and marked), a
 * column per live experiment's primary metric, each cell the value and its interval. A version
 * an experiment has not measured shows a dash, never a number. Empty without a board or a live
 * experiment.
 */
export function leaderboardSection(exps, board, champion) {
  const live = exps.filter((e) => e.live);
  if (!board || !live.length) return "";
  const columns = live.flatMap((e) =>
    (board.specs?.[e.id] ?? [])
      .filter((m) => m.role === "primary")
      .map((m) => ({ exp: e, metric: m })),
  );
  if (!columns.length) return "";
  const records = board.records.filter((r) => live.some((e) => e.id === r.experiment));
  const latest = (version, exp) =>
    records
      .filter((r) => r.algorithm_id === version && r.experiment === exp)
      .sort((a, b) => a.date.localeCompare(b.date))
      .at(-1);
  const newest = (v) =>
    records
      .filter((r) => r.algorithm_id === v)
      .map((r) => r.date)
      .sort()
      .at(-1) ?? "";
  // The champion has a row even before it has a record.
  const ids = records.map((r) => r.algorithm_id);
  const versions = [...new Set(champion ? [champion, ...ids] : ids)].sort(
    (a, b) =>
      Number(b === champion) - Number(a === champion) ||
      newest(b).localeCompare(newest(a)) ||
      a.localeCompare(b),
  );
  if (!versions.length) return "";
  const level = (c) => {
    const r = records.find((x) => x.experiment === c.exp.id && x.metrics?.[c.metric.name]);
    return r ? levelText(r.metrics[c.metric.name].level) : "";
  };
  const head = columns.map(
    (c) =>
      `<th scope="col">${esc(c.metric.label)}<span class="lab-meta"><a href="${BASE}/docs/experiments/${c.exp.slug}/">${esc(c.exp.id)}</a> · ${esc(level(c))}</span></th>`,
  );
  const cell = (version, c) => {
    const r = latest(version, c.exp.id);
    const m = r?.metrics?.[c.metric.name];
    const v = num(m?.value);
    if (!r || v === null) {
      return '<td><span class="lab-missing">—</span><span class="lab-meta">Not run on this version</span></td>';
    }
    const iv = Array.isArray(m.interval) ? ` [${num(m.interval[0])}, ${num(m.interval[1])}]` : "";
    const notes = [];
    if (m.at) notes.push(esc(m.at));
    if (r.against && r.against !== r.algorithm_id && VERDICT_TEXT[r.verdict]) {
      const who = r.against === champion ? "the champion" : `<code>${esc(r.against)}</code>`;
      notes.push(`${VERDICT_TEXT[r.verdict]} ${who}`);
    }
    const meta = notes.length ? `<span class="lab-meta">${notes.join(" · ")}</span>` : "";
    return `<td><span class="lab-value">${v}${iv}</span>${meta}</td>`;
  };
  const rows = versions.map((version) => {
    const isChampion = version === champion;
    const mark = isChampion ? ' <span class="lab-badge lab-badge-champion">Champion</span>' : "";
    return `<tr${isChampion ? ' class="lab-champion"' : ""}><th scope="row"><code>${esc(version)}</code>${mark}</th>${columns.map((c) => cell(version, c)).join("")}</tr>`;
  });
  const intro =
    "Every algorithm version against the primary metric of every live experiment, on the same registered seeds and inputs ([ADR 0109](" +
    `${GITHUB_BLOB}/docs/decisions/0109-champion-and-challenger.md` +
    ")). The champion is the version on main, the one in production. A challenger replaces it only when it beats it on the primaries with no clear regression on a guarded metric. The guarded metrics, each comparison's effect and its interval are in [lab/leaderboard.json](" +
    `${GITHUB_BLOB}/lab/leaderboard.json` +
    ").";
  return [
    "## Leaderboard",
    "",
    intro,
    "",
    '<div class="lab-table-wrap">',
    '<table class="lab-leaderboard">',
    `<thead><tr><th scope="col">Algorithm version</th>${head.join("")}</tr></thead>`,
    "<tbody>",
    ...rows,
    "</tbody>",
    "</table>",
    "</div>",
    "",
  ].join("\n");
}

/**
 * @param {any[]} exps
 * @param {any} [board]
 * @param {string | null} [champion]
 */
export function experimentsIndex(exps, board = null, champion = null) {
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
  const table = leaderboardSection(exps, board, champion);
  return `${intro}\n\n<ul class="lab-list">\n${rows.join("\n")}\n</ul>\n${table ? `\n${table}` : ""}`;
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
