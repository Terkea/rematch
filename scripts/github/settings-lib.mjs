// Shared logic for the repository settings kept as code in .github/settings (ADR 0095).
// Used by apply-settings.sh (through settings-plan.mjs), the drift workflow (check-drift.mjs)
// and test-settings.mjs. No dependencies: it runs on a bare Node 22.
import { readFileSync } from "node:fs";
import { join } from "node:path";

/** The GitHub Actions app. Pinning required checks to it means no other app can satisfy them. */
export const ACTIONS_APP_ID = 15368;

export function loadSettings(root) {
  const read = (p) => JSON.parse(readFileSync(join(root, ".github", "settings", p), "utf8"));
  return {
    ruleset: read("rulesets/main.json"),
    repo: read("repo.json"),
    labels: read("labels.json"),
    milestones: read("milestones.json"),
    roadmapIssues: read("roadmap-issues.json"),
  };
}

// ---- validation (the shape GitHub's REST API takes) ----

const RULE_PARAMS = {
  deletion: null,
  non_fast_forward: null,
  required_linear_history: null,
  creation: null,
  required_signatures: null,
  pull_request: [
    "allowed_merge_methods",
    "dismiss_stale_reviews_on_push",
    "require_code_owner_review",
    "require_last_push_approval",
    "required_approving_review_count",
    "required_review_thread_resolution",
  ],
  required_status_checks: [
    "do_not_enforce_on_create",
    "required_status_checks",
    "strict_required_status_checks_policy",
  ],
};

const REPO_KEYS = new Set([
  "description",
  "homepage",
  "has_issues",
  "has_projects",
  "has_wiki",
  "has_discussions",
  "allow_squash_merge",
  "allow_merge_commit",
  "allow_rebase_merge",
  "allow_auto_merge",
  "allow_update_branch",
  "delete_branch_on_merge",
  "squash_merge_commit_title",
  "squash_merge_commit_message",
  "web_commit_signoff_required",
]);

function fail(msg) {
  throw new Error(msg);
}

function onlyKeys(obj, allowed, where) {
  for (const k of Object.keys(obj)) {
    if (!allowed.includes(k)) fail(`${where}: unknown key "${k}"`);
  }
}

export function validateRuleset(r) {
  onlyKeys(r, ["name", "target", "enforcement", "conditions", "bypass_actors", "rules"], "ruleset");
  if (typeof r.name !== "string" || r.name === "") fail("ruleset.name must be a string");
  if (r.target !== "branch") fail('ruleset.target must be "branch"');
  if (!["active", "evaluate", "disabled"].includes(r.enforcement)) fail("ruleset.enforcement");
  const ref = r.conditions?.ref_name;
  if (!Array.isArray(ref?.include) || !Array.isArray(ref?.exclude)) {
    fail("ruleset.conditions.ref_name needs include and exclude arrays");
  }
  if (!Array.isArray(r.bypass_actors)) fail("ruleset.bypass_actors must be an array");
  for (const a of r.bypass_actors) {
    onlyKeys(a, ["actor_id", "actor_type", "bypass_mode"], "bypass_actors[]");
    if (!Number.isInteger(a.actor_id)) fail("bypass_actors[].actor_id must be an integer");
    if (!["RepositoryRole", "Team", "Integration", "OrganizationAdmin"].includes(a.actor_type)) {
      fail(`bypass_actors[].actor_type "${a.actor_type}"`);
    }
    if (!["always", "pull_request"].includes(a.bypass_mode)) fail("bypass_actors[].bypass_mode");
  }
  if (!Array.isArray(r.rules) || r.rules.length === 0) fail("ruleset.rules must be non-empty");
  const seen = new Set();
  for (const rule of r.rules) {
    if (!(rule.type in RULE_PARAMS)) fail(`unknown rule type "${rule.type}"`);
    if (seen.has(rule.type)) fail(`rule "${rule.type}" appears twice`);
    seen.add(rule.type);
    const params = RULE_PARAMS[rule.type];
    if (params === null) {
      onlyKeys(rule, ["type"], `rule ${rule.type}`);
    } else {
      onlyKeys(rule, ["type", "parameters"], `rule ${rule.type}`);
      onlyKeys(rule.parameters ?? fail(`rule ${rule.type} needs parameters`), params, rule.type);
    }
  }
  const checks = r.rules.find((x) => x.type === "required_status_checks");
  for (const c of checks?.parameters.required_status_checks ?? []) {
    onlyKeys(c, ["context", "integration_id"], "required_status_checks[]");
    if (typeof c.context !== "string" || c.context === "") fail("a check needs a context");
  }
}

export function validateRepo(s) {
  onlyKeys(s, ["repo", "topics", "private_vulnerability_reporting", "pages"], "repo.json");
  for (const k of Object.keys(s.repo)) {
    if (!REPO_KEYS.has(k)) fail(`repo.json repo: unknown key "${k}"`);
  }
  if (typeof s.repo.description !== "string" || s.repo.description.length > 350) {
    fail("repo.description must be a string of at most 350 characters");
  }
  if (!Array.isArray(s.topics) || s.topics.length > 20) fail("topics: at most 20");
  for (const t of s.topics) {
    if (!/^[a-z0-9][a-z0-9-]{0,49}$/.test(t)) fail(`topic "${t}" is not a valid GitHub topic`);
  }
  if (typeof s.private_vulnerability_reporting !== "boolean") {
    fail("private_vulnerability_reporting must be a boolean");
  }
  if (!["workflow", "legacy"].includes(s.pages?.build_type)) fail("pages.build_type");
}

export function validateLabels(labels) {
  if (!Array.isArray(labels)) fail("labels.json must be an array");
  const names = new Set();
  for (const l of labels) {
    onlyKeys(l, ["name", "color", "description"], `label ${l.name}`);
    if (typeof l.name !== "string" || l.name === "" || l.name.length > 50) fail("label name");
    if (names.has(l.name.toLowerCase())) fail(`label "${l.name}" appears twice`);
    names.add(l.name.toLowerCase());
    if (!/^[0-9a-f]{6}$/.test(l.color)) fail(`label "${l.name}": color must be 6 lowercase hex`);
    if (typeof l.description !== "string" || l.description.length > 100) {
      fail(`label "${l.name}": description of at most 100 characters`);
    }
  }
}

// ---- the check names CI reports ----

/**
 * The status-check names a workflow's jobs report: a job's `name:` if it has one, else its id.
 * A tiny line parser for this repo's two-space YAML, so it needs no YAML package. Matrix jobs
 * report one check per combination; we have none, and refuse them rather than guess.
 */
export function ciCheckNames(yaml) {
  const lines = yaml.split(/\r?\n/);
  const start = lines.findIndex((l) => /^jobs:\s*$/.test(l));
  if (start < 0) fail("ci.yml has no jobs: block");
  const names = [];
  let current = null;
  const flush = () => {
    if (current) names.push(current.name ?? current.id);
  };
  for (const line of lines.slice(start + 1)) {
    if (/^\S/.test(line) && !line.startsWith("#")) break;
    const job = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line);
    if (job) {
      flush();
      current = { id: job[1], name: undefined };
      continue;
    }
    if (!current) continue;
    const name = /^ {4}name:\s*(.+?)\s*$/.exec(line);
    if (name) {
      const value = name[1].replace(/^(["'])(.*)\1$/, "$2");
      if (value.includes("${{")) fail(`job ${current.id}: a computed name is not supported`);
      current.name = value;
    }
    if (/^ {6}matrix:/.test(line)) {
      fail(`job ${current.id} uses a matrix; list each combination's check name by hand`);
    }
  }
  flush();
  return names;
}

// ---- diffing ----

const show = (v) => (v === undefined ? "(missing)" : JSON.stringify(v));
const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);

/**
 * Lines describing where `live` differs from `desired`, as "path: live -> desired". Only keys
 * present in `desired` are compared, so the fields GitHub adds (ids, links, defaults) are ignored.
 * Arrays are compared as sets; arrays of `{type}` objects (ruleset rules) are matched by type.
 * With `skipMissing`, a key the live side does not show (a read-only token) is not drift.
 */
export function diffSubset(desired, live, opts = {}, path = "") {
  const at = (k) => (path ? `${path}.${k}` : k);
  if (isObj(desired)) {
    if (!isObj(live)) return [`${path || "(root)"}: ${show(live)} -> ${show(desired)}`];
    return Object.keys(desired).flatMap((k) => {
      if (live[k] === undefined && opts.skipMissing) return [];
      if (live[k] === undefined && desired[k] !== undefined && !isObj(desired[k])) {
        return [`${at(k)}: (missing) -> ${show(desired[k])}`];
      }
      return diffSubset(desired[k], live[k], opts, at(k));
    });
  }
  if (Array.isArray(desired)) {
    if (!Array.isArray(live)) return [`${path}: ${show(live)} -> ${show(desired)}`];
    if (desired.every((d) => isObj(d) && typeof d.type === "string")) {
      return diffTyped(desired, live, opts, path);
    }
    if (desired.every((d) => !isObj(d) && !Array.isArray(d))) {
      const a = [...desired].map(String).sort();
      const b = [...live].map(String).sort();
      return JSON.stringify(a) === JSON.stringify(b)
        ? []
        : [`${path}: ${show(live)} -> ${show(desired)}`];
    }
    const unmatched = [...live];
    let ok = desired.length === live.length;
    for (const d of desired) {
      const i = unmatched.findIndex((l) => diffSubset(d, l, opts).length === 0);
      if (i < 0) ok = false;
      else unmatched.splice(i, 1);
    }
    return ok ? [] : [`${path}: ${show(live)} -> ${show(desired)}`];
  }
  return desired === live ? [] : [`${path}: ${show(live)} -> ${show(desired)}`];
}

function diffTyped(desired, live, opts, path) {
  const types = [...new Set([...desired, ...live].map((r) => r.type))].sort();
  return types.flatMap((t) => {
    const d = desired.find((r) => r.type === t);
    const l = live.find((r) => r.type === t);
    if (!d) return [`${path}[${t}]: present -> (absent)`];
    if (!l) return [`${path}[${t}]: (missing) -> present`];
    return diffSubset(d, l, opts, `${path}[${t}]`);
  });
}

export function findRuleset(list, name) {
  return list.find((r) => r.name === name && r.source_type === "Repository");
}

export function diffLabels(desired, live) {
  const byName = new Map(live.map((l) => [l.name.toLowerCase(), l]));
  const create = [];
  const update = [];
  for (const d of desired) {
    const l = byName.get(d.name.toLowerCase());
    if (!l) create.push(d);
    else if (
      l.name !== d.name ||
      l.color.toLowerCase() !== d.color ||
      (l.description ?? "") !== d.description
    ) {
      update.push({ ...d, from: l });
    }
  }
  const wanted = new Set(desired.map((d) => d.name.toLowerCase()));
  const extra = live.filter((l) => !wanted.has(l.name.toLowerCase())).map((l) => l.name);
  return { create, update, extra };
}

// ---- the plan apply-settings.sh carries out ----

/**
 * What to change so the live repository matches the settings. `live` holds the GET responses
 * (or `{error}` when a GET failed): repo, labels, rulesets {list, detail}, vulnerabilityReporting
 * and pages. Returns readable `lines` (the diff) and the `calls` that would apply it.
 */
export function planChanges(settings, live, repo, opts = {}) {
  const lines = [];
  const calls = [];
  const call = (method, path, body) => calls.push({ method, path: `repos/${repo}${path}`, body });

  // Repository settings.
  const repoDiff = live.repo?.error
    ? [`(could not read the repository: ${live.repo.error})`]
    : diffSubset(settings.repo.repo, live.repo);
  if (repoDiff.length > 0) {
    lines.push("~ repository settings", ...repoDiff.map((l) => `    ${l}`));
    call("PATCH", "", settings.repo.repo);
  }
  const liveTopics = live.repo?.topics ?? live.topics?.names ?? [];
  const topicsDiff = diffSubset({ topics: settings.repo.topics }, { topics: liveTopics });
  if (topicsDiff.length > 0) {
    lines.push("~ topics", ...topicsDiff.map((l) => `    ${l}`));
    call("PUT", "/topics", { names: settings.repo.topics });
  }

  // The ruleset, found by name.
  const name = settings.ruleset.name;
  const existing = live.rulesets?.list ? findRuleset(live.rulesets.list, name) : undefined;
  if (!existing) {
    const why = live.rulesets?.error ? ` (listing rulesets failed: ${live.rulesets.error})` : "";
    lines.push(`+ ruleset "${name}" (create)${why}`);
    call("POST", "/rulesets", settings.ruleset);
  } else {
    const d = diffSubset(settings.ruleset, live.rulesets.detail ?? {});
    if (d.length > 0) {
      lines.push(`~ ruleset "${name}" (id ${existing.id})`, ...d.map((l) => `    ${l}`));
      call("PUT", `/rulesets/${existing.id}`, settings.ruleset);
    }
  }

  // Private vulnerability reporting.
  if (
    settings.repo.private_vulnerability_reporting &&
    live.vulnerabilityReporting?.enabled !== true
  ) {
    const why = live.vulnerabilityReporting?.error
      ? ` (now: ${live.vulnerabilityReporting.error})`
      : "";
    lines.push(`~ private vulnerability reporting: off -> on${why}`);
    call("PUT", "/private-vulnerability-reporting");
  }

  // Pages, built by Actions.
  const want = settings.repo.pages.build_type;
  if (live.pages?.error) {
    lines.push(`+ pages: build_type ${want} (create; now: ${live.pages.error})`);
    call("POST", "/pages", { build_type: want });
  } else if (live.pages?.build_type !== want) {
    lines.push(`~ pages: build_type ${show(live.pages?.build_type)} -> ${show(want)}`);
    call("PUT", "/pages", { build_type: want });
  }

  // Labels.
  const labels = diffLabels(settings.labels, Array.isArray(live.labels) ? live.labels : []);
  for (const l of labels.create) {
    lines.push(`+ label "${l.name}" #${l.color} ${JSON.stringify(l.description)}`);
    call("POST", "/labels", { name: l.name, color: l.color, description: l.description });
  }
  for (const l of labels.update) {
    lines.push(
      `~ label "${l.name}": #${l.from.color} ${JSON.stringify(l.from.description ?? "")} -> #${l.color} ${JSON.stringify(l.description)}`,
    );
    call("PATCH", `/labels/${encodeURIComponent(l.from.name)}`, {
      new_name: l.name,
      color: l.color,
      description: l.description,
    });
  }
  for (const name of labels.extra) {
    if (opts.prune) {
      lines.push(`- label "${name}" (delete)`);
      call("DELETE", `/labels/${encodeURIComponent(name)}`);
    } else {
      lines.push(`  label "${name}" is not in labels.json (kept; --prune deletes it)`);
    }
  }

  // Milestones, matched by title (only when the caller read them).
  if (live.milestones !== undefined && settings.milestones) {
    if (live.milestones.error) {
      lines.push(`(could not read the milestones: ${live.milestones.error})`);
    } else {
      const m = planMilestones(settings.milestones, live.milestones, opts);
      lines.push(...m.lines);
      for (const c of m.calls) call(c.method, c.path, c.body);
    }
  }
  return { lines, calls };
}

// ---- the drift check (a read-only token) ----

const PUBLIC_REPO_KEYS = [
  "description",
  "homepage",
  "has_issues",
  "has_projects",
  "has_wiki",
  "has_discussions",
];

/**
 * Drift between the settings and what a read-only GITHUB_TOKEN can see. `live` is
 * {repo, labels, rulesets: {list, detail} | {error}}. Returns {drift: [...], notices: [...]}.
 * While the repository is private (rulesets need a public repo on the free plan), it only notices.
 */
export function driftReport(settings, live) {
  const drift = [];
  const notices = [];
  if (live.repo.private) {
    notices.push(
      "The repository is private: rulesets are not enforced yet, so drift is not checked.",
    );
    return { drift, notices };
  }
  const visible = Object.fromEntries(
    Object.entries(settings.repo.repo).filter(
      ([k]) => PUBLIC_REPO_KEYS.includes(k) || k in live.repo,
    ),
  );
  drift.push(...diffSubset(visible, live.repo).map((l) => `repo ${l}`));
  drift.push(
    ...diffSubset({ topics: settings.repo.topics }, { topics: live.repo.topics ?? [] }).map(
      (l) => `repo ${l}`,
    ),
  );

  const labels = diffLabels(settings.labels, live.labels);
  for (const l of labels.create) drift.push(`label "${l.name}" is missing`);
  for (const l of labels.update)
    drift.push(
      `label "${l.name}" differs (#${l.from.color} ${JSON.stringify(l.from.description ?? "")})`,
    );
  if (labels.extra.length) notices.push(`Labels not in labels.json: ${labels.extra.join(", ")}`);

  if (live.rulesets.error) {
    notices.push(`Rulesets are not readable with this token (${live.rulesets.error}).`);
    return { drift, notices };
  }
  const found = findRuleset(live.rulesets.list, settings.ruleset.name);
  if (!found) {
    drift.push(`ruleset "${settings.ruleset.name}" does not exist`);
    return { drift, notices };
  }
  const detail = { ...live.rulesets.detail };
  const desired = { ...settings.ruleset };
  if (!Array.isArray(detail.bypass_actors) || detail.bypass_actors.length === 0) {
    // GitHub shows the bypass list only to those who can edit the ruleset.
    delete desired.bypass_actors;
    notices.push(
      "The bypass list is not visible to this token; apply-settings.sh --dry-run compares it.",
    );
  }
  drift.push(...diffSubset(desired, detail).map((l) => `ruleset ${l}`));
  return { drift, notices };
}

// ---- the roadmap: milestones and seed issues (ADR 0099) ----

export const HORIZONS = ["now", "next", "later"];

/** What each size means, written into the seeded issue so a newcomer can judge it. */
export const SIZES = {
  S: "S: a few hours to a day for someone new to the code",
  M: "M: two to four days",
  L: "L: one to two weeks",
  XL: "XL: several weeks, across more than one area",
};

/** The sections every epic's spec has (the `spec.yml` issue form, in the same order). */
export const SPEC_SECTIONS = [
  "## Problem",
  "## Who it's for",
  "## Goals",
  "## Non-goals",
  "## Evidence",
  "## Proposed design",
  "## Acceptance criteria",
  "## Test plan",
  "## ADR",
  "## Risks and open questions",
  "## Pointers",
];

export function validateMilestones(milestones) {
  if (!Array.isArray(milestones) || milestones.length === 0) {
    fail("milestones.json must be a non-empty array");
  }
  const titles = new Set();
  for (const m of milestones) {
    onlyKeys(m, ["title", "description", "state", "due_on", "horizon"], `milestone ${m.title}`);
    if (typeof m.title !== "string" || m.title === "" || m.title.length > 100) {
      fail("a milestone needs a title of at most 100 characters");
    }
    if (titles.has(m.title.toLowerCase())) fail(`milestone "${m.title}" appears twice`);
    titles.add(m.title.toLowerCase());
    if (typeof m.description !== "string" || m.description === "") {
      fail(`milestone "${m.title}" needs a description`);
    }
    if (!["open", "closed"].includes(m.state)) fail(`milestone "${m.title}": state open|closed`);
    if (!HORIZONS.includes(m.horizon)) {
      fail(`milestone "${m.title}": horizon must be one of ${HORIZONS.join(", ")}`);
    }
    if (m.due_on !== undefined && !/^\d{4}-\d{2}-\d{2}T00:00:00Z$/.test(m.due_on)) {
      fail(`milestone "${m.title}": due_on must look like 2027-01-31T00:00:00Z`);
    }
  }
}

/**
 * The seed issues: unique titles, a known milestone and known labels, a size, and parent links
 * that point at an epic in the same milestone. Epics carry the full spec; good first issues are
 * small. Throws on the first problem.
 */
export function validateRoadmapIssues(issues, { milestones, labels }) {
  if (!Array.isArray(issues) || issues.length === 0) fail("roadmap-issues.json must be an array");
  const milestoneTitles = new Set(milestones.map((m) => m.title));
  const labelNames = new Set(labels.map((l) => l.name));
  const byTitle = new Map();
  for (const i of issues) {
    onlyKeys(i, ["title", "milestone", "epic", "parent", "size", "labels", "body"], `${i.title}`);
    if (typeof i.title !== "string" || i.title === "" || i.title.length > 256) {
      fail("an issue needs a title of at most 256 characters");
    }
    const key = i.title.trim().toLowerCase();
    if (byTitle.has(key)) fail(`issue "${i.title}" appears twice`);
    byTitle.set(key, i);
  }
  for (const i of issues) {
    const where = `issue "${i.title}"`;
    if (!milestoneTitles.has(i.milestone)) {
      fail(`${where}: milestone "${i.milestone}" is not in milestones.json`);
    }
    if (!(i.size in SIZES)) fail(`${where}: size must be one of ${Object.keys(SIZES).join(", ")}`);
    if (!Array.isArray(i.labels) || i.labels.length === 0) fail(`${where}: needs labels`);
    for (const l of i.labels) {
      if (!labelNames.has(l)) fail(`${where}: label "${l}" is not in labels.json`);
    }
    if (new Set(i.labels).size !== i.labels.length) fail(`${where}: a label appears twice`);
    if (!i.labels.includes("roadmap")) fail(`${where}: seed issues carry the roadmap label`);
    if (!i.labels.some((l) => l.startsWith("type/"))) fail(`${where}: needs a type/* label`);
    if (!Array.isArray(i.body) || !i.body.every((l) => typeof l === "string")) {
      fail(`${where}: body must be an array of lines`);
    }
    const body = i.body.join("\n");
    if (body.length > 60000) fail(`${where}: body is longer than GitHub allows`);
    if (!body.startsWith("## Problem")) fail(`${where}: the body starts with "## Problem"`);
    for (const s of ["## Acceptance criteria", "## Pointers"]) {
      if (!body.includes(s)) fail(`${where}: missing "${s}"`);
    }
    if (!/^- \[ \] /m.test(body)) fail(`${where}: acceptance criteria are a checkbox list`);
    if (i.epic === true) {
      if (i.parent !== undefined) fail(`${where}: an epic has no parent`);
      if (!i.labels.includes("epic")) fail(`${where}: an epic carries the epic label`);
      if (!i.title.startsWith("Epic: ")) fail(`${where}: an epic's title starts with "Epic: "`);
      let at = -1;
      for (const s of SPEC_SECTIONS) {
        const next = body.indexOf(`${s}\n`, at + 1);
        if (next < 0) fail(`${where}: the spec is missing "${s}" (or it is out of order)`);
        at = next;
      }
    } else {
      if (i.epic !== undefined) fail(`${where}: "epic" is true or absent`);
      if (i.labels.includes("epic")) fail(`${where}: only epics carry the epic label`);
    }
    if (i.labels.includes("good first issue") && i.size !== "S") {
      fail(`${where}: a good first issue is size S`);
    }
    if (i.parent !== undefined) {
      const p = byTitle.get(String(i.parent).trim().toLowerCase());
      if (!p) fail(`${where}: parent "${i.parent}" is not in roadmap-issues.json`);
      if (p.epic !== true) fail(`${where}: parent "${i.parent}" is not an epic`);
      if (p.milestone !== i.milestone) fail(`${where}: not in its epic's milestone`);
    }
  }
  for (const e of issues.filter((i) => i.epic)) {
    if (!issues.some((i) => i.parent === e.title)) fail(`epic "${e.title}" has no sub-issues`);
  }
}

/** The body GitHub gets: the size first, then the spec, then where it came from. */
export function composeIssueBody(issue) {
  return [
    `Size: ${SIZES[issue.size]}.`,
    "",
    ...issue.body,
    "",
    "---",
    "Seeded from `.github/settings/roadmap-issues.json` (ADR 0099). From now on the issue is the",
    "source of truth: edit it here, not in the file.",
  ].join("\n");
}

const milestoneBody = (m) => {
  const b = { title: m.title, state: m.state, description: m.description };
  if (m.due_on) b.due_on = m.due_on;
  return b;
};

/** Create or update milestones by title; delete unknown ones only with `prune`. */
export function planMilestones(desired, live, opts = {}) {
  const lines = [];
  const calls = [];
  const byTitle = new Map(live.map((m) => [m.title, m]));
  for (const d of desired) {
    const l = byTitle.get(d.title);
    if (!l) {
      lines.push(`+ milestone "${d.title}" (${d.state})`);
      calls.push({ method: "POST", path: "/milestones", body: milestoneBody(d) });
      continue;
    }
    const diffs = [];
    if ((l.description ?? "") !== d.description) diffs.push("description");
    if (l.state !== d.state) diffs.push(`state: ${l.state} -> ${d.state}`);
    const liveDue = l.due_on ? String(l.due_on).slice(0, 10) : null;
    const wantDue = d.due_on ? d.due_on.slice(0, 10) : null;
    if (liveDue !== wantDue) diffs.push(`due_on: ${liveDue} -> ${wantDue}`);
    if (diffs.length > 0) {
      lines.push(`~ milestone "${d.title}" (#${l.number}): ${diffs.join(", ")}`);
      const body = milestoneBody(d);
      if (!d.due_on && l.due_on) body.due_on = null;
      calls.push({ method: "PATCH", path: `/milestones/${l.number}`, body });
    }
  }
  const wanted = new Set(desired.map((d) => d.title));
  for (const l of live.filter((m) => !wanted.has(m.title))) {
    if (opts.prune) {
      lines.push(`- milestone "${l.title}" (#${l.number}, delete)`);
      calls.push({ method: "DELETE", path: `/milestones/${l.number}` });
    } else {
      lines.push(`  milestone "${l.title}" is not in milestones.json (kept; --prune deletes it)`);
    }
  }
  return { lines, calls };
}

/** The live issue with this title (any state, pull requests excluded), or undefined. */
function liveIssue(liveIssues, title) {
  return liveIssues.find((i) => !i.pull_request && i.title.trim() === title.trim());
}

/**
 * Seed issues that do not exist yet (matched by exact title, open or closed), epics first. An
 * issue that exists is never edited: after seeding, GitHub is the source of truth.
 */
export function planSeedIssues(issues, liveIssues, liveMilestones, repo) {
  const lines = [];
  const calls = [];
  const number = new Map(liveMilestones.map((m) => [m.title, m.number]));
  const ordered = [...issues.filter((i) => i.epic), ...issues.filter((i) => !i.epic)];
  let existing = 0;
  for (const i of ordered) {
    if (liveIssue(liveIssues, i.title)) {
      existing += 1;
      continue;
    }
    const m = number.get(i.milestone);
    const kind = i.epic ? "epic" : i.parent ? "sub-issue" : "issue";
    const pending = m === undefined ? " (milestone created first)" : "";
    lines.push(`+ ${kind} "${i.title}" [${i.labels.join(", ")}] in "${i.milestone}"${pending}`);
    const body = { title: i.title, body: composeIssueBody(i), labels: i.labels };
    if (m !== undefined) body.milestone = m;
    calls.push({ method: "POST", path: `repos/${repo}/issues`, body });
  }
  if (existing > 0) lines.push(`  ${existing} seed issue(s) already exist (left as they are)`);
  return { lines, calls };
}

/** The live numbers of the epics, to read their sub-issues. */
export function epicNumbers(issues, liveIssues) {
  return issues
    .filter((i) => i.epic)
    .map((i) => liveIssue(liveIssues, i.title)?.number)
    .filter((n) => n !== undefined);
}

/**
 * Attach each seed issue to its epic as a sub-issue, unless it already is. `subIssues` maps an
 * epic's number to the sub-issues GitHub lists for it.
 */
export function planSubIssues(issues, liveIssues, subIssues, repo) {
  const lines = [];
  const calls = [];
  let linked = 0;
  for (const i of issues.filter((x) => x.parent)) {
    const parent = liveIssue(liveIssues, i.parent);
    const child = liveIssue(liveIssues, i.title);
    if (!parent || !child) {
      lines.push(`+ link "${i.title}" under "${i.parent}" (once both exist)`);
      continue;
    }
    const current = subIssues.get(parent.number) ?? [];
    if (current.some((s) => s.id === child.id)) {
      linked += 1;
      continue;
    }
    lines.push(`+ link #${child.number} "${i.title}" under #${parent.number}`);
    calls.push({
      method: "POST",
      path: `repos/${repo}/issues/${parent.number}/sub_issues`,
      body: { sub_issue_id: child.id },
    });
  }
  if (linked > 0) lines.push(`  ${linked} sub-issue link(s) already in place`);
  return { lines, calls };
}
