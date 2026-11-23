#!/usr/bin/env node
// Checks the repository settings kept as code in .github/settings (ADR 0095):
//   1. every JSON file there parses and has the shape GitHub's REST API takes;
//   2. the main ruleset requires exactly the checks CI reports, so renaming or adding a job in
//      .github/workflows/ci.yml cannot silently drop (or block on) a required check;
//   3. the diff and plan logic that apply-settings.sh and the drift check share.
// Run: node scripts/github/test-settings.mjs (no dependencies, no network).
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ACTIONS_APP_ID,
  ciCheckNames,
  diffLabels,
  diffSubset,
  driftReport,
  findRuleset,
  loadSettings,
  planChanges,
  validateLabels,
  validateRepo,
  validateRuleset,
} from "./settings-lib.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
let failed = 0;
let passed = 0;

function check(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`ok   ${name}`);
  } catch (err) {
    failed += 1;
    console.log(`FAIL ${name}\n     ${String(err.message).split("\n").join("\n     ")}`);
  }
}

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

// ---- 1. the files ----

const settingsDir = join(root, ".github", "settings");
for (const file of walk(settingsDir).filter((f) => f.endsWith(".json"))) {
  check(`${relative(root, file).replaceAll("\\", "/")} is valid JSON`, () => {
    JSON.parse(readFileSync(file, "utf8"));
  });
}

const settings = loadSettings(root);

check("rulesets/main.json has the REST ruleset shape and protects the default branch", () => {
  validateRuleset(settings.ruleset);
  assert.deepEqual(settings.ruleset.conditions.ref_name.include, ["~DEFAULT_BRANCH"]);
  const types = settings.ruleset.rules.map((r) => r.type).sort();
  assert.deepEqual(types, [
    "deletion",
    "non_fast_forward",
    "pull_request",
    "required_linear_history",
    "required_status_checks",
  ]);
  const pr = settings.ruleset.rules.find((r) => r.type === "pull_request");
  assert.equal(pr.parameters.required_approving_review_count, 0);
  assert.equal(pr.parameters.required_review_thread_resolution, true);
  const checks = settings.ruleset.rules.find((r) => r.type === "required_status_checks");
  assert.equal(checks.parameters.strict_required_status_checks_policy, true);
});

check("only a repository admin bypasses the ruleset, always", () => {
  assert.deepEqual(settings.ruleset.bypass_actors, [
    { actor_id: 5, actor_type: "RepositoryRole", bypass_mode: "always" },
  ]);
});

check("repo.json: squash merges only, titled by the PR, and the public fields", () => {
  validateRepo(settings.repo);
  const r = settings.repo.repo;
  assert.equal(r.allow_squash_merge, true);
  assert.equal(r.allow_merge_commit, false);
  assert.equal(r.allow_rebase_merge, false);
  assert.equal(r.squash_merge_commit_title, "PR_TITLE");
  assert.equal(r.delete_branch_on_merge, true);
  assert.equal(r.has_discussions, true);
  assert.equal(r.has_wiki, false);
  assert.equal(r.homepage, "https://terkea.github.io/rematch/");
  assert.equal(settings.repo.private_vulnerability_reporting, true);
});

check("labels.json: unique names, 6-digit colours, the triage set", () => {
  validateLabels(settings.labels);
  const names = settings.labels.map((l) => l.name);
  for (const want of [
    "needs-triage",
    "good first issue",
    "help wanted",
    "area/analysis",
    "area/coach",
    "area/trainer",
    "area/web-ui",
    "area/rust-backend",
    "area/docs",
  ]) {
    assert.ok(names.includes(want), `missing label ${want}`);
  }
  assert.ok(
    names.some((n) => n.startsWith("type/")),
    "no type/* label",
  );
});

// ---- 2. required checks == CI's job names ----

const ciPath = join(root, ".github", "workflows", "ci.yml");
// A Windows checkout may have CRLF line ends; the checks read LF.
const ciYaml = readFileSync(ciPath, "utf8").replaceAll("\r\n", "\n");

check("the ruleset requires exactly the check names ci.yml reports", () => {
  const want = ciCheckNames(ciYaml).sort();
  const got = settings.ruleset.rules
    .find((r) => r.type === "required_status_checks")
    .parameters.required_status_checks.map((c) => c.context)
    .sort();
  assert.deepEqual(
    got,
    want,
    `rulesets/main.json requires [${got}] but ci.yml reports [${want}].\n` +
      "Update required_status_checks in .github/settings/rulesets/main.json to match.",
  );
});

check("every required check comes from the GitHub Actions app", () => {
  const checks = settings.ruleset.rules.find((r) => r.type === "required_status_checks").parameters
    .required_status_checks;
  for (const c of checks) assert.equal(c.integration_id, ACTIONS_APP_ID, c.context);
});

check("ci.yml: push runs only on main, pull_request on every branch, with concurrency", () => {
  const has = (re, what) => assert.ok(re.test(ciYaml), `ci.yml must have ${what}`);
  has(
    /^on:\n {2}push:\n {4}branches: \[main\]\n {2}pull_request:\n/m,
    "on: push [main], pull_request",
  );
  has(/^concurrency:\n {2}group: .+\n {2}cancel-in-progress: true$/m, "a cancelling concurrency");
  has(/^permissions:\n {2}contents: read$/m, "permissions: contents: read");
});

// ---- 3. the shared logic ----

check("ciCheckNames: a job's name wins over its id, and matrix jobs are refused", () => {
  const yaml = [
    "name: CI",
    "on: push",
    "jobs:",
    "  alpha:",
    "    runs-on: ubuntu-latest",
    "  beta:",
    "    name: Beta gate",
    "    runs-on: ubuntu-latest",
    "    steps:",
    "      - name: not a job name",
    "        run: echo",
    "",
  ].join("\n");
  assert.deepEqual(ciCheckNames(yaml), ["alpha", "Beta gate"]);
  const matrix = `${yaml}  gamma:\n    strategy:\n      matrix:\n        os: [a, b]\n`;
  assert.throws(() => ciCheckNames(matrix), /matrix/);
});

check("diffSubset: compares only the keys we manage, arrays as sets", () => {
  const desired = { a: 1, b: { c: [2, 1] }, d: [{ x: 1 }, { x: 2 }] };
  const live = { a: 1, b: { c: [1, 2], extra: true }, d: [{ x: 2, id: 9 }, { x: 1 }], z: 0 };
  assert.deepEqual(diffSubset(desired, live), []);
  assert.deepEqual(diffSubset({ a: 1, b: { c: true } }, { a: 2, b: {} }), [
    "a: 2 -> 1",
    "b.c: (missing) -> true",
  ]);
  // A read-only token cannot see some fields; drift mode skips what is not there.
  assert.deepEqual(diffSubset({ a: 1, hidden: true }, { a: 1 }, { skipMissing: true }), []);
});

check("diffSubset: rules are matched by type, so order does not matter", () => {
  const desired = { rules: [{ type: "deletion" }, { type: "pull_request", parameters: { n: 0 } }] };
  const live = {
    rules: [{ type: "pull_request", parameters: { n: 0, more: 1 } }, { type: "deletion" }],
  };
  assert.deepEqual(diffSubset(desired, live), []);
  const drifted = { rules: [{ type: "deletion" }, { type: "creation" }] };
  assert.deepEqual(diffSubset(desired, drifted), [
    "rules[creation]: present -> (absent)",
    "rules[pull_request]: (missing) -> present",
  ]);
});

check("findRuleset: by name, repository-owned only", () => {
  const list = [
    { id: 1, name: "main", source_type: "Organization" },
    { id: 2, name: "other", source_type: "Repository" },
    { id: 3, name: "main", source_type: "Repository" },
  ];
  assert.equal(findRuleset(list, "main")?.id, 3);
  assert.equal(findRuleset(list, "none"), undefined);
});

check("diffLabels: creates, updates, and keeps extras unless pruning", () => {
  const desired = [
    { name: "good first issue", color: "7057ff", description: "A friendly start" },
    { name: "needs-triage", color: "ededed", description: "New" },
  ];
  const live = [
    { name: "good first issue", color: "7057FF", description: "old" },
    { name: "bug", color: "d73a4a", description: "" },
  ];
  const d = diffLabels(desired, live);
  assert.deepEqual(
    d.create.map((l) => l.name),
    ["needs-triage"],
  );
  assert.deepEqual(
    d.update.map((l) => l.name),
    ["good first issue"],
  );
  assert.deepEqual(d.extra, ["bug"]);
});

check("planChanges: a fresh repo gets every write; a matching repo gets none", () => {
  const repo = "o/r";
  const fresh = planChanges(
    settings,
    {
      repo: { description: null, has_wiki: true, topics: [] },
      topics: { names: [] },
      labels: [],
      rulesets: { error: "HTTP 403: Upgrade to GitHub Pro" },
      vulnerabilityReporting: { error: "HTTP 404: Not Found" },
      pages: { error: "HTTP 404: Not Found" },
    },
    repo,
  );
  const calls = fresh.calls.map((c) => `${c.method} ${c.path}`);
  assert.ok(calls.includes("PATCH repos/o/r"));
  assert.ok(calls.includes("PUT repos/o/r/topics"));
  assert.ok(calls.includes("POST repos/o/r/rulesets"));
  assert.ok(calls.includes("PUT repos/o/r/private-vulnerability-reporting"));
  assert.ok(calls.includes("POST repos/o/r/pages"));
  assert.ok(calls.includes("POST repos/o/r/labels"));
  // Label names with spaces are escaped in the path.
  const withLabel = planChanges(
    settings,
    {
      ...matching(settings),
      labels: settings.labels.map((l) => ({ ...l, description: "changed" })),
    },
    repo,
  );
  assert.ok(
    withLabel.calls.some((c) => c.path === "repos/o/r/labels/good%20first%20issue"),
    withLabel.calls.map((c) => c.path).join(", "),
  );

  const same = planChanges(settings, matching(settings), repo);
  assert.deepEqual(same.calls, [], same.lines.join("\n"));
});

check("planChanges: an existing ruleset is updated in place by id", () => {
  const live = matching(settings);
  live.rulesets = {
    list: [{ id: 42, name: settings.ruleset.name, source_type: "Repository" }],
    detail: { ...settings.ruleset, id: 42, enforcement: "evaluate" },
  };
  const plan = planChanges(settings, live, "o/r");
  assert.deepEqual(
    plan.calls.map((c) => `${c.method} ${c.path}`),
    ["PUT repos/o/r/rulesets/42"],
  );
  assert.ok(plan.lines.some((l) => l.includes('enforcement: "evaluate" -> "active"')));
});

check("driftReport: a private repo only notices; a public one compares what it can see", () => {
  const live = matching(settings);
  const priv = driftReport(settings, { ...live, repo: { ...live.repo, private: true } });
  assert.deepEqual(priv.drift, []);
  assert.match(priv.notices.join(" "), /private/);

  // A read-only token sees no merge settings and no bypass list: neither is drift.
  const publicFields = {
    private: false,
    description: settings.repo.repo.description,
    homepage: settings.repo.repo.homepage,
    has_issues: true,
    has_projects: false,
    has_wiki: false,
    has_discussions: true,
    topics: settings.repo.topics,
  };
  const readOnly = {
    ...live,
    repo: publicFields,
    rulesets: { ...live.rulesets, detail: { ...live.rulesets.detail, bypass_actors: [] } },
  };
  const clean = driftReport(settings, readOnly);
  assert.deepEqual(clean.drift, []);
  assert.match(clean.notices.join(" "), /bypass/);

  const loosened = structuredClone(readOnly);
  loosened.repo.has_wiki = true;
  loosened.rulesets.detail.rules = loosened.rulesets.detail.rules.filter(
    (r) => r.type !== "deletion",
  );
  loosened.labels = loosened.labels.filter((l) => l.name !== "needs-triage");
  assert.deepEqual(driftReport(settings, loosened).drift, [
    "repo has_wiki: true -> false",
    'label "needs-triage" is missing',
    "ruleset rules[deletion]: (missing) -> present",
  ]);

  const unreadable = driftReport(settings, { ...readOnly, rulesets: { error: "HTTP 403" } });
  assert.deepEqual(unreadable.drift, []);
  assert.match(unreadable.notices.join(" "), /not readable/);
});

/** A live state that already matches the settings. */
function matching(s) {
  return {
    repo: { ...s.repo.repo, topics: s.repo.topics },
    topics: { names: s.repo.topics },
    labels: s.labels.map((l) => ({ ...l, id: 1 })),
    rulesets: {
      list: [{ id: 7, name: s.ruleset.name, source_type: "Repository" }],
      detail: { ...s.ruleset, id: 7, source: "o/r" },
    },
    vulnerabilityReporting: { enabled: true },
    pages: { build_type: s.repo.pages.build_type },
  };
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
