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
