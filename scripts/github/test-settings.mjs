#!/usr/bin/env node
// Checks the repository settings kept as code in .github/settings (ADR 0095):
//   1. every JSON file there parses and has the shape GitHub's REST API takes;
//   2. the main ruleset requires exactly the checks CI reports, so renaming or adding a job in
//      .github/workflows/ci.yml cannot silently drop (or block on) a required check;
//   3. the diff and plan logic that apply-settings.sh and the drift check share;
//   4. the roadmap kept as code (ADR 0099): milestones.json and roadmap-issues.json agree with
//      each other and with labels.json, and the milestone, seed-issue and sub-issue plans;
//   5. milestone renames, moved and updated issues, and adopted issues (ADR 0104);
//   6. the Actions settings: fork pull request approval and the default token (ADR 0108).
// Run: node scripts/github/test-settings.mjs (no dependencies, no network).
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ACTIONS_APP_ID,
  ciCheckNames,
  composeIssueBody,
  diffLabels,
  diffSubset,
  driftReport,
  e2eSecretOrderProblems,
  epicNumbers,
  findRuleset,
  keepTicks,
  loadSettings,
  planChanges,
  planMilestones,
  planSeedIssues,
  planSubIssues,
  resolveMilestones,
  validateLabels,
  validateMilestones,
  validateActions,
  validateRepo,
  validateRoadmapIssues,
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

// The workflows whose every job gates a merge to main.
const GATING_WORKFLOWS = ["ci.yml", "pr-title.yml"];
// A Windows checkout may have CRLF line ends; the checks read LF.
const workflow = (name) =>
  readFileSync(join(root, ".github", "workflows", name), "utf8").replaceAll("\r\n", "\n");
const ciYaml = workflow("ci.yml");
const prTitleYaml = workflow("pr-title.yml");

check(
  `the ruleset requires exactly the check names ${GATING_WORKFLOWS.join(" and ")} report`,
  () => {
    const want = GATING_WORKFLOWS.flatMap((w) => ciCheckNames(workflow(w))).sort();
    const got = settings.ruleset.rules
      .find((r) => r.type === "required_status_checks")
      .parameters.required_status_checks.map((c) => c.context)
      .sort();
    assert.deepEqual(
      got,
      want,
      `rulesets/main.json requires [${got}] but the workflows report [${want}].\n` +
        "Update required_status_checks in .github/settings/rulesets/main.json to match.",
    );
    assert.ok(want.includes("pr-title"), "the PR-title lint is a required check");
  },
);

check(
  "no workflow uses pull_request_target or workflow_run (fork code never gets a write token)",
  () => {
    const dir = join(root, ".github", "workflows");
    const bad = readdirSync(dir)
      .filter((f) => /\.ya?ml$/.test(f))
      .filter((f) => /^\s*(pull_request_target|workflow_run)\s*:/m.test(workflow(f)));
    assert.deepEqual(bad, []);
  },
);

check("labels.json has the sensitive-reviewed override label", () => {
  assert.ok(settings.labels.some((l) => l.name === "sensitive-reviewed"));
});

check("pr-title.yml: plain pull_request with a read token, the action pinned by commit", () => {
  assert.ok(!prTitleYaml.includes("pull_request_target"), "never pull_request_target");
  assert.match(prTitleYaml, /^permissions:\n {2}pull-requests: read$/m);
  assert.match(prTitleYaml, /uses: amannn\/action-semantic-pull-request@[0-9a-f]{40}$/m);
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

check(
  "every workflow sets E2E_BETTER_AUTH_SECRET before compose reads the e2e-prod overlay",
  () => {
    const dir = join(root, ".github", "workflows");
    const problems = readdirSync(dir)
      .filter((f) => /\.ya?ml$/.test(f))
      .flatMap((f) => e2eSecretOrderProblems(workflow(f)).map((p) => `${f}: ${p}`));
    assert.deepEqual(problems, []);
  },
);

// ---- 3. the shared logic ----

check("e2eSecretOrderProblems: compose before the secret, no secret, and the good orders", () => {
  const job = (env, steps) =>
    ["jobs:", "  e2e:", "    runs-on: ubuntu-latest", ...env, "    steps:", ...steps, ""].join(
      "\n",
    );
  const prodEnv = [
    "    env:",
    "      COMPOSE_FILE: docker-compose.yml:docker-compose.e2e-prod.yml",
  ];
  const build = ["      - name: Build", "        run: $COMPOSE build web"];
  const secret = [
    "      - name: Secret",
    "        run: |",
    "          s=$(openssl rand -hex 32)",
    '          echo "E2E_BETTER_AUTH_SECRET=$s" >> "$GITHUB_ENV"',
  ];
  assert.deepEqual(e2eSecretOrderProblems(job(prodEnv, [...secret, ...build])), []);
  assert.match(
    e2eSecretOrderProblems(job(prodEnv, [...build, ...secret]))[0],
    /step 1 runs compose before step 2/,
  );
  assert.match(e2eSecretOrderProblems(job(prodEnv, build))[0], /never sets/);
  assert.deepEqual(e2eSecretOrderProblems(job([], build)), [], "no overlay, no secret needed");
  const inStep = ["      - run: docker compose -f docker-compose.e2e-prod.yml up -d"];
  assert.match(e2eSecretOrderProblems(job([], inStep))[0], /never sets/);
  const jobSecret = [...prodEnv, "      E2E_BETTER_AUTH_SECRET: x"];
  assert.deepEqual(e2eSecretOrderProblems(job(jobSecret, build)), []);
});

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
    has_projects: settings.repo.repo.has_projects,
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

// ---- 4. the roadmap ----

check("milestones.json: unique titles, a description and a horizon each, v1.0 MVP last", () => {
  validateMilestones(settings.milestones);
  assert.equal(settings.milestones.at(-1).title, "v1.0 MVP");
  assert.ok(
    settings.milestones.some((m) => m.horizon === "now"),
    "something is Now",
  );
});

check("labels.json has the spec flow and the sweep labels", () => {
  const names = settings.labels.map((l) => l.name);
  for (const want of ["needs-spec", "spec", "accepted", "epic", "roadmap", "stale", "sweep"]) {
    assert.ok(names.includes(want), `missing label ${want}`);
  }
});

check("roadmap-issues.json: milestones and labels exist, titles unique, epics carry a spec", () => {
  validateRoadmapIssues(settings.roadmapIssues, settings);
  const epics = settings.roadmapIssues.filter((i) => i.epic);
  for (const m of settings.milestones) {
    assert.ok(
      epics.some((e) => e.milestone === m.title),
      `milestone "${m.title}" has no epic`,
    );
  }
  const gfi = settings.roadmapIssues.filter((i) => i.labels.includes("good first issue"));
  assert.ok(gfi.length >= 3, "at least three good first issues for newcomers");
});

check("validateRoadmapIssues refuses the mistakes that would break seeding", () => {
  const firstChild = (x) => x.find((i) => i.parent);
  const cases = [
    [(x) => x.push({ ...x[0] }), /appears twice/],
    [
      (x) => {
        firstChild(x).milestone = "v9 Nowhere";
      },
      /not in milestones.json/,
    ],
    [(x) => firstChild(x).labels.push("no-such-label"), /not in labels.json/],
    [
      (x) => {
        firstChild(x).parent = "Epic: missing";
      },
      /is not in roadmap-issues/,
    ],
    [
      (x) => {
        const children = x.filter((i) => i.parent);
        children[0].parent = children[1].title;
      },
      /is not an epic/,
    ],
    [
      (x) => {
        x.find((i) => i.epic).body = [
          "## Problem",
          "- [ ] x",
          "## Acceptance criteria",
          "## Pointers",
        ];
      },
      /spec is missing/,
    ],
    [
      (x) => x.find((i) => !i.epic && i.size !== "S").labels.push("good first issue"),
      /good first issue is size S/,
    ],
    [
      (x) => {
        const e = x.find((i) => i.labels.includes("experiment"));
        e.body = e.body.slice(e.body.indexOf("## Problem"));
      },
      /starts with "## Value case"/,
    ],
    [
      (x) => {
        const e = x.find((i) => i.labels.includes("experiment"));
        e.body = e.body.filter((l) => !l.startsWith("- Cheaper alternative? "));
      },
      /missing "- Cheaper alternative\?"/,
    ],
    [
      (x) => {
        const e = x.find((i) => i.labels.includes("experiment"));
        e.body = e.body.map((l) => l.replace(/= [\d.]+/, "= 27"));
      },
      /the value score is/,
    ],
    [
      (x) => {
        const e = x.find((i) => i.labels.includes("experiment"));
        e.body = e.body.map((l) => l.replace("Pre-register before running", "Register"));
      },
      /Pre-register before running/,
    ],
  ];
  for (const [mutate, err] of cases) {
    const copy = structuredClone(settings.roadmapIssues);
    mutate(copy);
    assert.throws(() => validateRoadmapIssues(copy, settings), err);
  }
});

check("every issue form's labels are in labels.json, and the spec form exists", () => {
  const dir = join(root, ".github", "ISSUE_TEMPLATE");
  const names = new Set(settings.labels.map((l) => l.name));
  const forms = readdirSync(dir).filter((f) => f.endsWith(".yml") && f !== "config.yml");
  assert.ok(forms.includes("spec.yml"), "the spec form exists");
  for (const f of forms) {
    const line = /^labels:\s*\[([^\]]*)\]/m.exec(readFileSync(join(dir, f), "utf8"));
    for (const l of (line?.[1] ?? "").split(",").map((x) => x.trim().replace(/^"|"$/g, ""))) {
      if (l) assert.ok(names.has(l), `${f} uses label "${l}", which is not in labels.json`);
    }
  }
});

check("composeIssueBody: the size first, the spec, then where it is kept", () => {
  const body = composeIssueBody(settings.roadmapIssues[0]);
  assert.match(body, /^Size: [SMLX]+: /);
  assert.ok(body.includes("## Problem"));
  assert.match(body, /roadmap-issues\.json/);
  assert.match(body, /replaced on the next/);
});

check("planMilestones: create by title, update what differs, delete only with prune", () => {
  const desired = [
    { title: "v0.2 A", description: "a", state: "open", horizon: "now" },
    { title: "v0.3 B", description: "b", state: "open", horizon: "next" },
  ];
  const live = [
    { number: 4, title: "v0.2 A", description: "old", state: "open", due_on: null },
    { number: 9, title: "Someday", description: "", state: "open", due_on: null },
  ];
  const plan = planMilestones(desired, live);
  assert.deepEqual(
    plan.calls.map((c) => `${c.method} ${c.path}`),
    ["PATCH /milestones/4", "POST /milestones"],
  );
  assert.equal(plan.calls[1].body.horizon, undefined, "horizon is ours, not GitHub's");
  assert.ok(plan.lines.some((l) => l.includes('"Someday" is not in milestones.json (kept')));
  const pruned = planMilestones(desired, live, { prune: true });
  assert.ok(pruned.calls.some((c) => c.method === "DELETE" && c.path === "/milestones/9"));
  const same = planMilestones(desired, [
    { number: 4, title: "v0.2 A", description: "a", state: "open", due_on: null },
    { number: 5, title: "v0.3 B", description: "b", state: "open", due_on: null },
  ]);
  assert.deepEqual(same.calls, []);
});

check("planChanges plans the milestones only when they were read", () => {
  const live = matching(settings);
  assert.deepEqual(planChanges(settings, live, "o/r").calls, []);
  const fresh = planChanges(settings, { ...live, milestones: [] }, "o/r");
  assert.equal(
    fresh.calls.filter((c) => c.path === "repos/o/r/milestones").length,
    settings.milestones.length,
  );
});

check("planSeedIssues: epics first, existing titles (open or closed) skipped, PRs ignored", () => {
  const issues = settings.roadmapIssues;
  const child = issues.find((i) => i.parent);
  const live = [
    { number: 1, id: 101, title: child.title, state: "closed" },
    { number: 2, id: 102, title: issues.find((i) => i.epic).title, pull_request: {} },
  ];
  const milestones = settings.milestones.map((m, n) => ({ number: n + 1, title: m.title }));
  const plan = planSeedIssues(issues, live, milestones, "o/r");
  assert.equal(plan.calls.length, issues.length - 1, "a PR with an epic's title does not count");
  const titles = plan.calls.map((c) => c.body.title);
  const lastEpic = Math.max(...titles.map((t, n) => (t.startsWith("Epic: ") ? n : -1)));
  const firstOther = titles.findIndex((t) => !t.startsWith("Epic: "));
  assert.ok(lastEpic < firstOther, "every epic is created before any other issue");
  assert.ok(!titles.includes(child.title));
  assert.ok(plan.calls.every((c) => Number.isInteger(c.body.milestone)));
  // Before the milestones exist (a dry run), the issue is planned without a number.
  const early = planSeedIssues(issues, [], [], "o/r");
  assert.ok(early.calls.every((c) => c.body.milestone === undefined));
  const planned = early.lines.filter((l) => l.startsWith("+ "));
  assert.equal(planned.length, issues.length);
  assert.ok(planned.every((l) => l.includes("milestone created first")));
});

check("planSubIssues: links each child under its epic once, by the child's id", () => {
  const issues = settings.roadmapIssues;
  const live = issues.map((i, n) => ({ number: n + 1, id: 1000 + n, title: i.title }));
  const numbers = epicNumbers(issues, live);
  assert.equal(numbers.length, issues.filter((i) => i.epic).length);
  const children = issues.filter((i) => i.parent);
  const plan = planSubIssues(issues, live, new Map(), "o/r");
  assert.equal(plan.calls.length, children.length);
  const child = live.find((l) => l.title === children[0].title);
  const parent = live.find((l) => l.title === children[0].parent);
  assert.equal(plan.calls[0].path, `repos/o/r/issues/${parent.number}/sub_issues`);
  assert.deepEqual(plan.calls[0].body, { sub_issue_id: child.id, replace_parent: true });
  // Already linked: nothing to do.
  const linked = new Map(numbers.map((n) => [n, live]));
  assert.deepEqual(planSubIssues(issues, live, linked, "o/r").calls, []);
  // Not on GitHub yet: planned for after creation, with no call.
  const none = planSubIssues(issues, [], new Map(), "o/r");
  assert.deepEqual(none.calls, []);
  assert.equal(none.lines.length, children.length);
});

// ---- 5. renames, moves and updates (ADR 0104) ----

check(
  "milestones.json: the evaluation-first roadmap, in order, with the old titles renamed",
  () => {
    assert.deepEqual(
      settings.milestones.map((m) => m.title),
      [
        "v0.2 Factual and deterministic foundation",
        "v0.3 The evaluation lab",
        "v0.4 Patterns and progression",
        "v0.5 Board, sound and clarity",
        "v0.6 Practice habit and trainer depth",
        "v0.7 More players",
        "v1.0 MVP",
      ],
    );
    const from = settings.milestones.flatMap((m) => m.renamed_from ?? []);
    for (const t of [
      "v0.2 Practice habit",
      "v0.3 Trainer depth",
      "v0.4 Self-host and benchmark",
      "v0.5 More players",
    ]) {
      assert.ok(from.includes(t), `"${t}" is renamed or retired, not left behind`);
    }
    const lab = settings.milestones.find((m) => m.title === "v0.3 The evaluation lab");
    assert.deepEqual(lab.adopt_issues, [11, 13, 14]);
    const epic = settings.roadmapIssues.find(
      (i) => i.epic && i.milestone === "v0.3 The evaluation lab" && /evaluation lab/i.test(i.title),
    );
    assert.ok(epic, "v0.3 has the evaluation lab epic");
    const body = epic.body.join("\n");
    for (const n of [11, 13, 14]) assert.match(body, new RegExp(`#${n}\\b`));
  },
);

check("validateMilestones refuses a rename that clashes", () => {
  const base = [
    { title: "v0.2 A", description: "a", state: "open", horizon: "now" },
    { title: "v0.3 B", description: "b", state: "open", horizon: "next" },
  ];
  const cases = [
    [
      (x) => {
        x[0].renamed_from = ["v0.3 B"];
      },
      /is a current milestone/,
    ],
    [
      (x) => {
        x[0].renamed_from = ["old"];
        x[1].renamed_from = ["old"];
      },
      /more than one renamed_from/,
    ],
    [
      (x) => {
        x[0].renamed_from = "old";
      },
      /must be an array/,
    ],
    [
      (x) => {
        x[0].adopt_issues = [3];
        x[1].adopt_issues = [3];
      },
      /more than one milestone/,
    ],
    [
      (x) => {
        x[0].adopt_issues = [0];
      },
      /adopt_issues are numbers/,
    ],
  ];
  for (const [mutate, err] of cases) {
    const copy = structuredClone(base);
    mutate(copy);
    assert.throws(() => validateMilestones(copy), err);
  }
  validateMilestones(base);
});

const renameDesired = [
  { title: "v0.2 Fresh", description: "f", state: "open", horizon: "now" },
  {
    title: "v0.6 Habit",
    description: "h",
    state: "open",
    horizon: "later",
    renamed_from: ["v0.2 Practice", "v0.3 Trainer"],
  },
  {
    title: "v0.7 More",
    description: "m",
    state: "open",
    horizon: "later",
    renamed_from: ["v0.5 More"],
    adopt_issues: [70],
  },
];
const renameLive = [
  { number: 1, title: "v0.2 Practice", description: "old", state: "open", due_on: null },
  { number: 2, title: "v0.3 Trainer", description: "old", state: "open", due_on: null },
  { number: 4, title: "v0.5 More", description: "m", state: "open", due_on: null },
];

check("planMilestones: renames through renamed_from, retires the rest, then does nothing", () => {
  const plan = planMilestones(renameDesired, renameLive);
  assert.deepEqual(
    plan.calls.map((c) => `${c.method} ${c.path} ${JSON.stringify(c.body)}`),
    [
      'POST /milestones {"title":"v0.2 Fresh","state":"open","description":"f"}',
      'PATCH /milestones/1 {"title":"v0.6 Habit","state":"open","description":"h"}',
      'PATCH /milestones/4 {"title":"v0.7 More","state":"open","description":"m"}',
      'PATCH /milestones/2 {"state":"closed"}',
    ],
  );
  assert.ok(plan.lines.some((l) => l.includes('"v0.3 Trainer" (#2), retired into "v0.6 Habit"')));
  assert.ok(!plan.lines.some((l) => l.includes("is not in milestones.json")));
  // After the apply the renamed ones match by title and the retired one is closed.
  const after = [
    { number: 1, title: "v0.6 Habit", description: "h", state: "open", due_on: null },
    { number: 2, title: "v0.3 Trainer", description: "old", state: "closed", due_on: null },
    { number: 4, title: "v0.7 More", description: "m", state: "open", due_on: null },
    { number: 6, title: "v0.2 Fresh", description: "f", state: "open", due_on: null },
  ];
  const again = planMilestones(renameDesired, after);
  assert.deepEqual(again.calls, [], again.lines.join("\n"));
  assert.deepEqual(again.lines, []);
  // With --prune the retired milestone is deleted instead of closed; a renamed one never is.
  const pruned = planMilestones(renameDesired, renameLive, { prune: true });
  assert.ok(pruned.calls.some((c) => c.method === "DELETE" && c.path === "/milestones/2"));
  assert.ok(!pruned.calls.some((c) => c.method === "DELETE" && c.path === "/milestones/1"));
});

check(
  "resolveMilestones: an exact title wins over a rename, and each live one is used once",
  () => {
    const live = [...renameLive, { number: 9, title: "v0.6 Habit", state: "open" }];
    const r = resolveMilestones(renameDesired, live);
    assert.equal(r.byDesired.get("v0.6 Habit").number, 9);
    assert.equal(r.byDesired.get("v0.7 More").number, 4);
    assert.deepEqual(
      r.retired.map((x) => x.live.number),
      [1, 2],
    );
  },
);

check("planSeedIssues: moves, updates and adopts open issues, leaves closed ones alone", () => {
  const spec = (title, milestone) => ({
    title,
    milestone,
    size: "S",
    labels: ["type/chore", "roadmap"],
    body: ["## Problem", "", "x", "", "## Acceptance criteria", "", "- [ ] one", "- [ ] two"],
  });
  const file = [
    spec("Stays", "v0.6 Habit"),
    spec("Moves", "v0.6 Habit"),
    spec("Text changed", "v0.7 More"),
    spec("Closed", "v0.7 More"),
    spec("Brand new", "v0.2 Fresh"),
  ];
  const ms = (n) => ({ number: n, title: renameLive.find((m) => m.number === n).title });
  const labels = [{ name: "type/chore" }, { name: "roadmap" }];
  const ticked = composeIssueBody(file[2]).replace("- [ ] one", "- [x] one");
  const issue = (number, title, milestone, extra) => ({
    number,
    id: 100 + number,
    title,
    state: "open",
    milestone,
    labels,
    ...extra,
  });
  const live = [
    issue(10, "Stays", ms(1), { body: composeIssueBody(file[0]) }),
    issue(11, "Moves", ms(2), {
      labels: [{ name: "roadmap" }],
      body: composeIssueBody(file[1]).replace("- [ ] two", "- [X] two"),
    }),
    issue(12, "Text changed", ms(4), { body: ticked.replace("\nx\n", "\nold text\n") }),
    issue(13, "Closed", ms(2), { state: "closed", labels: [], body: "" }),
    issue(70, "Filed by hand", null, { labels: [], body: "mine" }),
  ];
  const plan = planSeedIssues(file, live, renameLive, "o/r", renameDesired);
  const calls = Object.fromEntries(plan.calls.map((c) => [c.path, c]));
  // "Stays" is in v0.2 Practice, which becomes v0.6 Habit: nothing to do.
  assert.equal(calls["repos/o/r/issues/10"], undefined);
  // "Moves" is in the retired v0.3 Trainer: moved to #1 with its missing label; the text matches.
  assert.deepEqual(calls["repos/o/r/issues/11"].body, {
    milestone: 1,
    labels: ["roadmap", "type/chore"],
  });
  // "Text changed" gets the file's text with its ticked box kept.
  const patched = calls["repos/o/r/issues/12"].body;
  assert.deepEqual(Object.keys(patched), ["body"]);
  assert.match(patched.body, /- \[x\] one/);
  assert.match(patched.body, /\nx\n/);
  assert.equal(calls["repos/o/r/issues/13"], undefined, "a closed issue is never edited");
  assert.deepEqual(calls["repos/o/r/issues/70"].body, { milestone: 4 }, "adopted: milestone only");
  const created = plan.calls.filter((c) => c.method === "POST");
  assert.deepEqual(
    created.map((c) => c.body.title),
    ["Brand new"],
  );
  assert.equal(created[0].body.milestone, undefined, "v0.2 Fresh does not exist yet");
  assert.ok(plan.lines.some((l) => l.includes('"v0.3 Trainer" -> "v0.6 Habit"')));
  assert.ok(plan.lines.some((l) => /1 to create, 2 to move, 1 to update in place/.test(l)));

  // After the apply nothing differs, so the plan is empty.
  const after = [
    { number: 1, title: "v0.6 Habit", state: "open" },
    { number: 2, title: "v0.3 Trainer", state: "closed" },
    { number: 4, title: "v0.7 More", state: "open" },
    { number: 6, title: "v0.2 Fresh", state: "open" },
  ];
  const byNumber = (n) => after.find((m) => m.number === n);
  const done = live.map((l) => {
    const c = calls[`repos/o/r/issues/${l.number}`];
    const milestone = l.milestone ? byNumber(l.milestone.number) : null;
    if (!c) return { ...l, milestone };
    return {
      ...l,
      milestone: c.body.milestone ? byNumber(c.body.milestone) : milestone,
      labels: c.body.labels ? c.body.labels.map((name) => ({ name })) : l.labels,
      body: c.body.body ?? l.body,
    };
  });
  done.push(issue(14, "Brand new", byNumber(6), { body: composeIssueBody(file[4]) }));
  const again = planSeedIssues(file, done, after, "o/r", renameDesired);
  assert.deepEqual(again.calls, [], again.lines.join("\n"));
  assert.deepEqual(again.lines, []);
});

check("keepTicks: only boxes with the same text stay ticked", () => {
  const body = "- [ ] a\n- [ ] b\n  - [ ] c";
  assert.equal(
    keepTicks(body, "- [x] a\r\n- [ ] b\r\n  - [X] c\n- [x] gone"),
    "- [x] a\n- [ ] b\n  - [x] c",
  );
  assert.equal(keepTicks(body, undefined), body);
});

// ---- 6. the Actions settings (ADR 0108) ----

check("repo.json actions: every outside contributor is approved first, a read-only token", () => {
  assert.deepEqual(settings.repo.actions, {
    fork_pr_contributor_approval: "all_external_contributors",
    default_workflow_permissions: "read",
    can_approve_pull_request_reviews: false,
  });
});

check("validateActions rejects unknown keys and values", () => {
  const ok = settings.repo.actions;
  validateActions(ok);
  assert.throws(() => validateActions({ ...ok, extra: 1 }), /unknown key "extra"/);
  assert.throws(
    () => validateActions({ ...ok, fork_pr_contributor_approval: "everyone" }),
    /fork_pr_contributor_approval/,
  );
  assert.throws(
    () => validateActions({ ...ok, default_workflow_permissions: "admin" }),
    /default_workflow_permissions/,
  );
  assert.throws(
    () => validateActions({ ...ok, can_approve_pull_request_reviews: "no" }),
    /can_approve_pull_request_reviews/,
  );
});

check("planChanges: the Actions settings are written only where the live repo differs", () => {
  const live = matching(settings);
  live.forkApproval = { approval_policy: "first_time_contributors" };
  live.actionsWorkflow = {
    default_workflow_permissions: "write",
    can_approve_pull_request_reviews: true,
  };
  const plan = planChanges(settings, live, "o/r");
  assert.deepEqual(
    plan.calls.map((c) => `${c.method} ${c.path}`),
    [
      "PUT repos/o/r/actions/permissions/fork-pr-contributor-approval",
      "PUT repos/o/r/actions/permissions/workflow",
    ],
  );
  assert.deepEqual(plan.calls[0].body, { approval_policy: "all_external_contributors" });
  assert.deepEqual(plan.calls[1].body, {
    default_workflow_permissions: "read",
    can_approve_pull_request_reviews: false,
  });
  assert.ok(
    plan.lines.some((l) => l.includes('"first_time_contributors" -> "all_external_contributors"')),
    plan.lines.join("\n"),
  );

  // A failed read is planned as a write; matching state and unread state plan none.
  const failedRead = planChanges(
    settings,
    { ...matching(settings), forkApproval: { error: "HTTP 403: Forbidden" } },
    "o/r",
  );
  assert.deepEqual(
    failedRead.calls.map((c) => c.path),
    ["repos/o/r/actions/permissions/fork-pr-contributor-approval"],
  );
  assert.deepEqual(planChanges(settings, matching(settings), "o/r").calls, []);
  const unread = matching(settings);
  delete unread.forkApproval;
  delete unread.actionsWorkflow;
  assert.deepEqual(planChanges(settings, unread, "o/r").calls, []);
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
    forkApproval: { approval_policy: s.repo.actions.fork_pr_contributor_approval },
    actionsWorkflow: {
      default_workflow_permissions: s.repo.actions.default_workflow_permissions,
      can_approve_pull_request_reviews: s.repo.actions.can_approve_pull_request_reviews,
    },
  };
}

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
