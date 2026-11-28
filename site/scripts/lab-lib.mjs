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
        plain: has("plain.md"),
      },
      plain: has("plain.md") ? readFileSync(join(dir, name, "plain.md"), "utf8") : null,
      figures: loadFigures(join(dir, name), name),
    });
  }
  return out.sort((a, b) => b.updated.localeCompare(a.updated) || b.id.localeCompare(a.id));
}

// ---- figures ---------------------------------------------------------------------------------

const unquote = (lit) =>
  lit
    .replace(/^f?["']|["']$/g, "")
    .replace(/\{[^}]*\}/g, "")
    .replace(/\s+/g, " ")
    .trim();

/**
 * The axis labels and titles the analysis gave a figure: the block that saves it (from the
 * previous `savefig` or `def`), or the body of the function the file name is passed to.
 * @returns {{ kind: string, text: string }[]}
 */
export function figureLabels(analysis, file) {
  const src = String(analysis ?? "").replace(/\r\n/g, "\n");
  const at = src.indexOf(`"${file}"`);
  if (at < 0) return [];
  const lineStart = src.lastIndexOf("\n", at) + 1;
  const lineEnd = src.indexOf("\n", at);
  const theLine = src.slice(lineStart, lineEnd < 0 ? undefined : lineEnd);
  let block;
  if (/savefig/.test(theLine)) {
    const before = src.slice(0, lineStart);
    const from = Math.max(before.lastIndexOf("savefig"), before.lastIndexOf("\ndef "), 0);
    block = src.slice(from, lineEnd);
  } else {
    const fn = /^\s*(\w+)\(/.exec(theLine)?.[1];
    const def = fn ? src.indexOf(`\ndef ${fn}(`) : -1;
    if (def < 0) return [];
    const next = src.indexOf("\ndef ", def + 1);
    block = src.slice(def, next < 0 ? undefined : next);
  }
  // A loop over a literal tuple (`for i, metric in enumerate(("precision", "recall"))`) names the
  // values an f-string's placeholder takes; any other placeholder drops the label.
  const loops = {};
  for (const m of block.matchAll(/for\s+([\w\s,]+?)\s+in\s+[^\n]*?\(((?:\s*"[^"]*"\s*,?)+)\)/g)) {
    const v = (m[1] ?? "").split(",").at(-1)?.trim();
    if (v) loops[v] = [...(m[2] ?? "").matchAll(/"([^"]*)"/g)].map((x) => x[1]);
  }
  const out = [];
  for (const m of block.matchAll(/set_(title|xlabel|ylabel)\(\s*(f?"[^"]*"|f?'[^']*')/g)) {
    const kind = m[1] ?? "";
    let texts = [m[2] ?? ""];
    for (;;) {
      const ph = /\{(\w+)\}/.exec(texts[0] ?? "");
      if (!ph || !loops[ph[1] ?? ""]) break;
      texts = texts.flatMap((t) => loops[ph[1] ?? ""].map((v) => t.replace(`{${ph[1]}}`, v)));
    }
    for (const raw of texts) {
      // A placeholder left, or a title that runs on into another literal ("when true = ").
      if (/\{/.test(raw) || /[=:]\s*["']$/.test(raw)) continue;
      const text = unquote(raw);
      if (text && !out.some((o) => o.kind === kind && o.text === text)) out.push({ kind, text });
    }
  }
  return out;
}

/** A sentence about a figure, without its path or a leading "Figure:" or "Results:". */
export function cleanContext(context, file) {
  const path = `figures/${file.replace(/[.-]/g, "\\$&")}`;
  const s = String(context ?? "")
    .replace(/^(?:Figures?|Results|Algorithm)\b[^:]*:\s*/, "")
    .replace(new RegExp(`^${path}\\s+(?:is|are|shows)\\s+`), "")
    .replace(new RegExp(`^${path}`), "")
    .replace(new RegExp(path, "g"), "the figure")
    .replace(/^[,;:\s]+|[,;\s]+$/g, "")
    .replace(/^\((.*)\)$/, "$1")
    .trim();
  // A bookkeeping line ("results.json (commit …), the figure in") says nothing about the figure.
  if (!s || /commit|sha256|the figure in$/.test(s)) return "";
  const short = s.length > 240 ? `${s.slice(0, s.lastIndexOf(", ", 240))}…` : s;
  return cap(short.replace(/[.;]$/, ""));
}

/** The first sentence of the experiment's own files that names the figure, as plain text. */
export function figureContext(texts, file) {
  for (const t of texts) {
    const plain = String(t ?? "")
      .replace(/\r\n/g, "\n")
      .replace(/<!--[\s\S]*?-->/g, "");
    for (const para of plain.split(/\n\s*\n|\n(?=[-*] |\d+\. )/)) {
      if (!para.includes(`figures/${file}`)) continue;
      const sentences = para.replace(/\s+/g, " ").split(/(?<=[.;])\s+(?=[A-Z`(])/);
      const hit = sentences.find((x) => x.includes(`figures/${file}`));
      if (hit)
        return hit
          .replace(/^[-*]\s+|^\d+\.\s+/, "")
          .replace(/\*\*/g, "")
          .replace(/`/g, "")
          .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
          .trim();
    }
  }
  return "";
}

const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);

/** A figure's alt text: what it plots, from the analysis's labels, else from its context. */
export function figureAlt(expId, file, labels, context) {
  const get = (k) => labels.filter((l) => l.kind === k).map((l) => l.text);
  const [titles, xs, ys] = [get("title"), get("xlabel"), get("ylabel")];
  const parts = [];
  if (ys.length && xs.length) parts.push(`${cap(ys[0])} against ${xs[0]}`);
  else if (xs.length) parts.push(cap(xs[0]));
  else if (ys.length) parts.push(cap(ys[0]));
  if (titles.length)
    parts.push(
      `${parts.length ? "p" : "P"}${titles.length > 1 ? "anels" : "anel"}: ${titles.join("; ")}`,
    );
  const what = parts.join(", ");
  if (what) return `${expId} figure: ${what}.`;
  if (context) return `${expId} figure: ${context}.`;
  return `${expId} figure ${file.replace(/\.png$/, "").replace(/-/g, " ")}, from its analysis.`;
}

/** The PNGs in an experiment's figures/ folder, each with its alt text and the sentence naming it. */
export function loadFigures(expDir, expId) {
  const figDir = join(expDir, "figures");
  if (!existsSync(figDir)) return [];
  const read = (f) => (existsSync(join(expDir, f)) ? readFileSync(join(expDir, f), "utf8") : "");
  const analysis = read("analysis.py");
  const texts = ["decision.md", "review.md", "preregistration.md", "summary.md"].map(read);
  return readdirSync(figDir)
    .filter((f) => /\.png$/i.test(f))
    .sort()
    .map((file) => {
      const context = cleanContext(figureContext(texts, file), file);
      return { file, alt: figureAlt(expId, file, figureLabels(analysis, file), context), context };
    });
}

/** Where a copied figure is served (sync-content.mjs copies it to public/lab-figures/). */
export const figureHref = (e, f) => `${BASE}/lab-figures/${e.slug}/${f.file}`;

const attr = (s) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");

/** The experiment's own matplotlib figures, as published in its folder. */
function figuresSection(e) {
  const figs = e.figures ?? [];
  if (!figs.length) return [];
  return [
    "## Figures",
    "",
    "Drawn by the experiment's frozen `analysis.py` from its `results.json`, and copied here unchanged: the colours, the reference lines and the interval levels are the analysis's own (the pre-registration states each level), not the site's. The analysis is frozen once registered, so they are not redrawn.",
    "",
    ...figs.map(
      (f) =>
        `<figure class="lab-figure"><img src="${figureHref(e, f)}" alt="${attr(f.alt)}" loading="lazy" decoding="async"><figcaption>${esc(f.alt.replace(/^EXP-\d+ figure: /, ""))}${f.context && !f.alt.includes(f.context) ? ` ${esc(f.context)}.` : ""} <a href="${fileUrl(e, `figures/${f.file}`)}">figures/${esc(f.file)}</a></figcaption></figure>`,
    ),
    "",
  ];
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

/**
 * The experiment in plain words (plain.md), as the page's first section: its title dropped (the
 * section has its own), its headings one level down, links rewritten, `<` escaped.
 */
function plainSection(e) {
  if (!e.plain) return [];
  const src = e.plain.replace(/\r\n/g, "\n").replace(/<!--[\s\S]*?-->\s*/g, "");
  const body = rewriteMarkdown(esc(src), `lab/experiments/${e.folder}/plain.md`)
    .replace(/^# [^\n]*\n+/, "")
    .replace(/^(#{2,5}) /gm, "#$1 ");
  return ["## In plain words", "", body.trim(), "", "## The record", ""];
}

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
    ...plainSection(e),
    ...e.bullets.map((b) => `- ${b}`),
    "",
    links.map((l) => `- ${l}`).join("\n"),
    "",
    ...figuresSection(e),
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
