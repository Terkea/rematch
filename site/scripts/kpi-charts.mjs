// The Engine scorecard (plan §4, closes #35): the KPI tree of ADR 0119 as rows, and per-KPI charts,
// all drawn at build time as inline SVG with no client script. Sources:
// - lab/leaderboard.json (schema leaderboard-v1, the arena's records, ADR 0109);
// - a bench history (schema bench-history-v1, defined below), when one exists. The weekly full
//   suite will append to it (#58); until then the bench rows say "unmeasured";
// - algorithm/CHANGELOG.md, for which experiment's decision shipped in which version.
// Every colour, size and font is a token: the SVGs carry classes only (`kc-*`), which docs.css styles
// from tokens.css, so day and night follow the page. sync-content.mjs calls `writeScorecard`;
// test/kpi-charts.test.ts covers it.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { BASE, GITHUB_BLOB, REPO } from "./content-lib.mjs";

/**
 * A bench history: what `cli bench compare` (#58) appends to the `bench-history` branch. Each run
 * uses the leaderboard's `{ value, interval, level }` shape, so one renderer serves both.
 * @typedef {{ value: number | null, interval: [number, number] | null, level: number | string,
 *   n?: number, unit?: string }} Metric
 * @typedef {{ schema: "run-record-v1", suite: "smoke" | "full", date: string, algorithm_id: string,
 *   commit?: string, seeds?: string, tier: "synthetic" | "real", metrics: Record<string, Metric> }} RunRecord
 * @typedef {{ schema: "bench-history-v1", fixture?: boolean, runs: RunRecord[] }} BenchHistory
 */
export const HISTORY_SCHEMA = "bench-history-v1";
export const RUN_SCHEMA = "run-record-v1";
/** Every interval the scorecard computes itself is an 80% one (ADR-WEB-19). */
export const LEVEL = 0.8;
const Z80 = 1.2815515655446004;

// ---- numbers -------------------------------------------------------------------------------

/** Wilson score interval for k of n at `level` (80% by default). Null for n = 0. */
export function wilson(k, n, level = LEVEL) {
  if (!n) return null;
  const z = level === LEVEL ? Z80 : normalQuantile(1 - (1 - level) / 2);
  const p = k / n;
  const z2 = z * z;
  const centre = (p + z2 / (2 * n)) / (1 + z2 / n);
  const half = (z * Math.sqrt((p * (1 - p)) / n + z2 / (4 * n * n))) / (1 + z2 / n);
  return [Math.max(0, centre - half), Math.min(1, centre + half)];
}

// Acklam's rational approximation, enough for an interval's z.
function normalQuantile(p) {
  const a = [
    -39.6968302866538, 220.946098424521, -275.928510446969, 138.357751867269, -30.6647980661472,
    2.50662827745924,
  ];
  const b = [
    -54.4760987982241, 161.585836858041, -155.698979859887, 66.8013118877197, -13.2806815528857,
  ];
  const c = [
    -0.00778489400243029, -0.322396458041136, -2.40075827716184, -2.54973253934373,
    4.37466414146497, 2.93816398269878,
  ];
  const d = [0.00778469570904146, 0.32246712907004, 2.445134137143, 3.75440866190742];
  const q = p < 0.5 ? p : 1 - p;
  let x;
  if (q < 0.02425) {
    const t = Math.sqrt(-2 * Math.log(q));
    x =
      (((((c[0] * t + c[1]) * t + c[2]) * t + c[3]) * t + c[4]) * t + c[5]) /
      ((((d[0] * t + d[1]) * t + d[2]) * t + d[3]) * t + 1);
  } else {
    const t = q - 0.5;
    const r = t * t;
    x =
      ((((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * t) /
      (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1);
  }
  return p < 0.5 ? x : -x;
}

const finite = (x) => typeof x === "number" && Number.isFinite(x);
/** A number at `d` places, with a real minus (U+2212). A dash for anything not finite. */
export function fmt(x, d = 3) {
  if (!finite(x)) return "—";
  const s = Math.abs(x).toFixed(d);
  return x < 0 && Number(s) !== 0 ? `−${s}` : s;
}
/** An interval rounded outward at `d` places (DESIGN.md "KPI stats"): low floored, high ceiled. */
export function fmtInterval(iv, d = 2) {
  if (!Array.isArray(iv) || !finite(iv[0]) || !finite(iv[1])) return null;
  const m = 10 ** d;
  const lo = Math.floor(iv[0] * m + 1e-9) / m;
  const hi = Math.ceil(iv[1] * m - 1e-9) / m;
  return `[${fmt(lo, d)}, ${fmt(hi, d)}]`;
}
export const levelText = (l) =>
  typeof l === "number" ? `${Math.round(l * 1000) / 10}%` : String(l ?? "");
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;");
const bits = (s) =>
  typeof s === "string" ? { k: [...s].filter((c) => c === "1").length, n: s.length } : null;

/**
 * The status of an interval against a target: "meets" when the whole interval is on the right
 * side, "misses" when the whole interval is on the wrong side, else "no clear difference".
 * @param {[number, number] | null} iv
 * @param {{ kind: "min" | "max" | "band", value?: number, band?: [number, number] }} target
 */
export function statusFor(iv, target) {
  if (!Array.isArray(iv) || !finite(iv[0]) || !finite(iv[1])) return "unmeasured";
  const [lo, hi] = iv;
  if (target.kind === "min")
    return lo >= target.value ? "meets" : hi < target.value ? "misses" : "unclear";
  if (target.kind === "max")
    return hi <= target.value ? "meets" : lo > target.value ? "misses" : "unclear";
  const [a, b] = target.band;
  return lo >= a && hi <= b ? "meets" : hi < a || lo > b ? "misses" : "unclear";
}
export const STATUS_TEXT = {
  meets: "Meets",
  misses: "Misses",
  unclear: "No clear difference from the target",
  unjudged: "Not judged",
  unmeasured: "Unmeasured",
};

// ---- versions ------------------------------------------------------------------------------

/** `algo-6.0.0+f40a2b27677a` → `6.0.0`. */
export const shortVersion = (id) =>
  String(id)
    .replace(/^algo-/, "")
    .replace(/\+.*$/, "");
const semver = (id) => shortVersion(id).split(".").map(Number);
const bySemver = (a, b) => {
  const x = semver(a);
  const y = semver(b);
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  return 0;
};

/**
 * The versions the arena re-ran: those with a record in more than one experiment, oldest first,
 * the last `max` of them. A lab-only version (one experiment's candidate) is left out.
 */
export function arenaVersions(board, max = 7) {
  const per = new Map();
  for (const r of board?.records ?? []) {
    if (!per.has(r.algorithm_id)) per.set(r.algorithm_id, new Set());
    per.get(r.algorithm_id).add(r.experiment);
  }
  return [...per.entries()]
    .filter(([, s]) => s.size > 1)
    .map(([v]) => v)
    .sort(bySemver)
    .slice(-max);
}

/** The newest record of `exp` on `version`, or null. */
export function recordOf(board, exp, version) {
  return (
    (board?.records ?? [])
      .filter((r) => r.experiment === exp && r.algorithm_id === version)
      .sort((a, b) => a.date.localeCompare(b.date))
      .at(-1) ?? null
  );
}

/**
 * The experiment whose decision each version shipped, from algorithm/CHANGELOG.md: a major or
 * minor version (x.y.0) whose dated summary line names an experiment (the last one it names). Patch versions change no
 * output, so they ship no decision.
 * @returns {Record<string, string>} short version → experiment id
 */
export function decisionsFromChangelog(text) {
  const out = {};
  const parts = String(text ?? "")
    .replace(/\r\n/g, "\n")
    .split(/^## /m)
    .slice(1);
  for (const part of parts) {
    const id = /^(algo-[^\s]+)/.exec(part)?.[1];
    if (!id) continue;
    const v = shortVersion(id);
    if (!/\.0$/.test(v)) continue;
    const summary = part.split("\n").find((l) => /^\d{4}-\d{2}-\d{2}\./.test(l)) ?? "";
    // The last experiment named is the one decided ("drawn from EXP-0002's model ... (EXP-0006)").
    const exp = summary.match(/EXP-\d{4}/g)?.at(-1);
    if (exp) out[v] = exp;
  }
  return out;
}

// ---- the bench history -----------------------------------------------------------------------

/** Problems with a bench history, as messages; empty when it is valid. */
export function validateHistory(h) {
  const errs = [];
  if (!h || typeof h !== "object") return ["not an object"];
  if (h.schema !== HISTORY_SCHEMA) errs.push(`schema is "${h.schema}", not "${HISTORY_SCHEMA}"`);
  if (!Array.isArray(h.runs)) return [...errs, "runs is not a list"];
  h.runs.forEach((r, i) => {
    const at = `runs[${i}]`;
    if (r?.schema !== RUN_SCHEMA) errs.push(`${at}: schema is not "${RUN_SCHEMA}"`);
    if (!/^\d{4}-\d{2}-\d{2}$/.test(r?.date ?? "")) errs.push(`${at}: date is not YYYY-MM-DD`);
    if (!/^algo-/.test(r?.algorithm_id ?? "")) errs.push(`${at}: algorithm_id is not an algo id`);
    if (!["synthetic", "real"].includes(r?.tier)) errs.push(`${at}: tier is not synthetic or real`);
    for (const [name, m] of Object.entries(r?.metrics ?? {})) {
      if (m.value !== null && !finite(m.value)) errs.push(`${at}.${name}: value is not a number`);
      if (
        m.interval !== null &&
        !(
          Array.isArray(m.interval) &&
          m.interval.length === 2 &&
          m.interval.every(finite) &&
          m.interval[0] <= m.interval[1]
        )
      )
        errs.push(`${at}.${name}: interval is not [low, high]`);
      if (m.level === undefined) errs.push(`${at}.${name}: no level`);
    }
  });
  return errs;
}

/** A bench history from `path`, or null when there is none. Throws on an invalid one. */
export function loadBenchHistory(path) {
  if (!path || !existsSync(path)) return null;
  const h = JSON.parse(readFileSync(path, "utf8"));
  const errs = validateHistory(h);
  if (errs.length) throw new Error(`${path}: ${errs.join("; ")}`);
  return h;
}

/** The newest run of each version in the history (full suite before smoke on the same date). */
function historyByVersion(history) {
  const out = new Map();
  const runs = [...(history?.runs ?? [])].sort(
    (a, b) =>
      a.date.localeCompare(b.date) || Number(a.suite === "full") - Number(b.suite === "full"),
  );
  for (const r of runs) out.set(shortVersion(r.algorithm_id), r);
  return out;
}

// ---- what each experiment measured, per version ------------------------------------------------

const FLAWS = [
  ["hang", "Hangs pieces"],
  ["low_clock_blunder", "Blunders on a low clock"],
  ["no_convert", "Fails to convert"],
  ["opening_blunder", "Blunders in the opening"],
  ["miss_mate", "Misses mates"],
];

/** EXP-0001: recall at 2, pooled and per flaw type, from the players' hit2 flags. */
function recallOf(rec) {
  const players = Object.values(rec?.measures?.flawed ?? {});
  if (!players.length) return null;
  const k = players.filter((p) => p.hit2).length;
  const flaws = {};
  for (const [f] of FLAWS) {
    const ps = players.filter((p) => p.flaw === f);
    if (!ps.length) continue;
    const kf = ps.filter((p) => p.hit2).length;
    flaws[f] = { k: kf, n: ps.length, value: kf / ps.length, interval: wilson(kf, ps.length) };
  }
  return {
    k,
    n: players.length,
    value: k / players.length,
    interval: wilson(k, players.length),
    flaws,
  };
}

/** EXP-0015: false alarms at 120 games and detection of 100- and 200-point falls, from the bits. */
function trajectoryOf(rec) {
  const m = rec?.measures;
  if (!m) return null;
  const rate = (s) => {
    const b = bits(s);
    return b?.n ? { ...b, value: b.k / b.n, interval: wilson(b.k, b.n) } : null;
  };
  const fa = rate(m.false_alarm?.["120"]);
  const d100 = rate(m.detection?.["100"]);
  const d200 = rate(m.detection?.["200"]);
  return fa && d100 ? { fa, d100, d200 } : null;
}

const metric = (rec, name) => {
  const m = rec?.metrics?.[name];
  return m && finite(m.value) ? m : null;
};

/** Everything the scorecard reads, per arena version. */
export function perVersion(board, versions) {
  return versions.map((v) => {
    const e7 = recordOf(board, "EXP-0007", v);
    const trusted = e7?.metrics?.trusted_tags;
    const precisions = (trusted?.tags ?? [])
      .map((t) => ({ tag: t, m: metric(e7, `precision_${t}`) }))
      .filter((x) => x.m && Array.isArray(x.m.interval));
    const weakest = precisions.sort((a, b) => a.m.interval[0] - b.m.interval[0])[0] ?? null;
    return {
      version: v,
      short: shortVersion(v),
      recall: recallOf(recordOf(board, "EXP-0001", v)),
      worst: metric(recordOf(board, "EXP-0002", v), "worst_coverage"),
      rho1: metric(recordOf(board, "EXP-0002", v), "rho1_coverage"),
      trajectory: trajectoryOf(recordOf(board, "EXP-0015", v)),
      trusted:
        trusted && Array.isArray(trusted.tags)
          ? { count: trusted.tags.length, of: 15, tags: trusted.tags, at: trusted.at }
          : null,
      weakest,
      stepDown: metric(recordOf(board, "EXP-0008", v), "step_down"),
      stepUp: metric(recordOf(board, "EXP-0008", v), "step_up"),
      depthAt: recordOf(board, "EXP-0008", v)?.metrics?.ships_the_chosen_count?.at ?? null,
    };
  });
}

// ---- links ---------------------------------------------------------------------------------

const expHref = (id) => `${BASE}/docs/experiments/${id.toLowerCase()}/`;
const issueHref = (n) => `https://github.com/${REPO}/issues/${n}`;
const link = (href, text) => `<a href="${esc(href)}">${esc(text)}</a>`;
const expLink = (id) => link(expHref(id), id);
const issueLink = (n, text = `#${n}`) => link(issueHref(n), text);

// ---- the KPI rows --------------------------------------------------------------------------

/**
 * The scorecard's model: the arena versions, the champion, one row per KPI of ADR 0119 (North Star
 * and its proxy, drivers D1a to D7, guards) and the data each chart draws.
 * @param {any} board lab/leaderboard.json
 * @param {BenchHistory | null} history
 * @param {{ champion?: string | null, changelog?: string, baseline?: any }} [opts]
 */
export function buildScorecard(board, history = null, opts = {}) {
  const versions = arenaVersions(board);
  const champion =
    opts.champion && versions.includes(opts.champion) ? opts.champion : (versions.at(-1) ?? null);
  const data = perVersion(board, versions);
  const now = data.find((d) => d.version === champion) ?? null;
  const decisions = decisionsFromChangelog(opts.changelog ?? "");
  const bench = historyByVersion(history);
  for (const d of data) d.bench = bench.get(d.short)?.metrics ?? null;
  const benchNow = champion ? (bench.get(shortVersion(champion)) ?? null) : null;
  const benchMetric = (name) => benchNow?.metrics?.[name] ?? null;
  const benchTrend = (name) => data.map((d) => toPoint(bench.get(d.short)?.metrics?.[name]));
  const fixture = Boolean(history?.fixture);
  // bench/baseline.json: the smoke suite's last accepted scorecard, the older metrics shown until
  // the history has the new ones.
  const smoke = opts.baseline?.metrics ?? null;
  const benchTier = fixture ? "fixture" : "synthetic";

  /** @type {any[]} */
  const rows = [];
  const unmeasured = (row) =>
    rows.push({
      tier: "unmeasured",
      status: "unmeasured",
      value: null,
      interval: null,
      trend: [],
      ...row,
    });

  // North Star and its proxy.
  unmeasured({
    id: "north-star",
    group: "north",
    name: "A correct focus on real players",
    question: "Does the weakness Rematch puts first lead real players to fewer lost points?",
    target: "Not set until it can be measured",
    links: `Tier 2 ${issueLink(64)}, and ${expLink("EXP-0004")} (${issueLink(94)}). Tier 4, whether learners improve on their focus, needs a consented design and an ADR before anything is built.`,
  });
  unmeasured({
    id: "ns-2",
    group: "north",
    name: "NS-2: focus lift on anonymised Lichess players",
    question:
      "Does the rank-1 weakness hit a player's top two sources of lost points in their next 40 games more often than a peer-frequency ranker?",
    target: "Paired 80% interval of the lift clears 0 (about 400 players)",
    links: `Pre-registered by ${expLink("EXP-0004")} (${issueLink(94)}); the run is EXP-0035, still to be registered.`,
  });
  unmeasured({
    id: "transfer",
    group: "north",
    name: "Transfer from synthetic to human-like players",
    question: "Does the synthetic recall hold when the simulated players move like humans?",
    target: "Not set until it can be measured",
    links: `EXP-0036, still to be registered. ${expLink("EXP-0012")} (${issueLink(107)}) is a partial proxy.`,
  });

  // D1a: flaw types detectable.
  if (now?.recall) {
    const types = Object.entries(now.recall.flaws);
    const at = types.filter(([, f]) => f.value >= 0.8).length;
    const statuses = types.map(([, f]) => statusFor(f.interval, { kind: "min", value: 0.8 }));
    const status = statuses.every((s) => s === "meets")
      ? "meets"
      : statuses.includes("misses")
        ? "misses"
        : "unclear";
    const below = types
      .filter((_, i) => statuses[i] === "misses")
      .map(([f]) => flawLabel(f).toLowerCase());
    // Lead with what the intervals show, as the status is judged; the point count comes second.
    const clear = statuses.filter((s) => s === "meets").length;
    const unclear = statuses.filter((s) => s === "unclear").length;
    rows.push({
      id: "d1a",
      group: "drivers",
      name: "D1a: flaw types the engine detects",
      question: "How many of the five injected flaw types reach recall at 2 of 0.8?",
      tier: "synthetic",
      value: clear,
      display: `${clear} of ${types.length}`,
      context: "types clearly at or above 0.80",
      interval: null,
      intervalText: `${unclear} no clear difference from 0.80, ${below.length} wholly below; ${at} of ${types.length} at or above 0.80 by point estimate alone`,
      basis: `${now.recall.n / types.length} simulated players per type, 80 games each (${expLink("EXP-0001")})`,
      target: "Every type at or above 0.80",
      status,
      reason: below.length
        ? `The 80% interval${below.length > 1 ? "s" : ""} of ${below.join(" and ")} ${below.length > 1 ? "lie" : "lies"} wholly below 0.80.`
        : "",
      trend: data.map((d) =>
        d.recall
          ? {
              value: Object.values(d.recall.flaws).filter(
                (f) => statusFor(f.interval, { kind: "min", value: 0.8 }) === "meets",
              ).length,
            }
          : null,
      ),
      trendLabel: "types clearly at 0.80",
      chart: "chart-flaws",
    });
  }

  // D1b: macro recall at 2 (the bench), or pooled recall at 2 (EXP-0001) until it exists.
  const macro = benchMetric("recall_at_2_macro");
  if (macro) {
    rows.push(
      benchRow(
        "d1b",
        "D1b: macro recall at 2",
        "The mean over flaw types of the share of flawed players whose flaw ranks in the top two.",
        macro,
        { kind: "min", value: 0.8 },
        "At or above 0.80",
        benchTrend("recall_at_2_macro"),
        benchTier,
        "players",
        "chart-recall",
      ),
    );
  } else if (now?.recall) {
    rows.push({
      id: "d1b",
      group: "drivers",
      name: "D1b: recall at 2",
      older:
        "Shown as pooled recall at 2, the metric today's data supports. Macro recall arrives with the bench changes of ADR 0119; with six players per type the two have the same point estimate here.",
      question: "Of the flawed players, how many have their flaw in the top two?",
      tier: "synthetic",
      value: now.recall.value,
      display: fmt(now.recall.value),
      interval: now.recall.interval,
      level: LEVEL,
      basis: `${now.recall.k} of ${now.recall.n} simulated players, 80 games each (${expLink("EXP-0001")})`,
      target: "At or above 0.80",
      status: statusFor(now.recall.interval, { kind: "min", value: 0.8 }),
      trend: data.map((d) => toPoint(d.recall)),
      chart: "chart-recall",
    });
  }

  // D2: worst-cell calibration.
  if (now?.worst) {
    rows.push({
      id: "d2",
      group: "drivers",
      name: "D2: worst-cell calibration",
      question:
        "Of the 20 weakness × sample-size cells, how close is the worst coverage of the 80% range to 0.80?",
      tier: "synthetic",
      value: now.worst.value,
      display: fmt(now.worst.value),
      interval: now.worst.interval,
      level: now.worst.level,
      basis: `3,600 simulated players per cell, the worst at ${esc(now.worst.at ?? "")} (${expLink("EXP-0002")})`,
      target: "Every cell in [0.75, 0.85]",
      status: statusFor(now.worst.interval, { kind: "band", band: [0.75, 0.85] }),
      trend: data.map((d) => toPoint(d.worst)),
      chart: "chart-calibration",
    });
  }

  // D3: false weakness on clean players.
  const clean = benchMetric("clean_false_weakness");
  if (clean) {
    rows.push(
      benchRow(
        "d3",
        "D3: false weakness on clean players",
        "Of players with no injected flaw, how many are shown a rank-1 weakness?",
        clean,
        { kind: "max", value: 0.05 },
        "At or below 0.05",
        benchTrend("clean_false_weakness"),
        benchTier,
        "clean players",
      ),
    );
  } else {
    const e1rec = champion ? recordOf(board, "EXP-0001", champion) : null;
    const e1 = metric(e1rec, "clean_false_alarms");
    const controls = Object.keys(e1rec?.measures?.clean ?? {}).length;
    const smokeClean = finite(smoke?.clean_players)
      ? ` where the smoke bench reads ${smoke.clean_top1_false_alarms} of ${smoke.clean_players}, so one of the two measures the wrong thing; that is being checked`
      : "";
    unmeasured({
      id: "d3",
      group: "drivers",
      name: "D3: false weakness on clean players",
      question: "Of players with no injected flaw, how many are shown a rank-1 weakness?",
      target: "At or below 0.05, which needs 32 clean players",
      older: e1
        ? `Today ${expLink("EXP-0001")} has ${controls} clean controls and reads ${fmt(e1.value)} ${fmtInterval(e1.interval) ?? ""} (${levelText(e1.level)})${smokeClean}.`
        : "",
      links: `${expLink("EXP-0013")} (${issueLink(108)}) at 32 clean players.`,
    });
  }

  // D4: the trajectory pair.
  if (now?.trajectory) {
    const t = now.trajectory;
    const prev = previousWith(data, champion, (d) => d.trajectory);
    const cmp = champion ? recordOf(board, "EXP-0015", champion)?.comparison?.detection_100 : null;
    const fa = statusFor(t.fa.interval, { kind: "max", value: 0.1 });
    const lower = changeReading(cmp) === "lower";
    rows.push({
      id: "d4",
      group: "drivers",
      name: "D4: the trajectory's false alarms and detection",
      question:
        "How often is a player of constant strength told “falling” at 120 games, and how often is a 100-point fall caught?",
      tier: "synthetic",
      value: t.fa.value,
      display: `${fmt(t.fa.value)} and ${fmt(t.d100.value)}`,
      context: "told “falling”, and falls caught",
      interval: t.fa.interval,
      intervalText: `${fmtInterval(t.fa.interval)} and ${fmtInterval(t.d100.interval)}, 80%`,
      level: LEVEL,
      basis: `${t.fa.n} constant-strength and ${t.d100.n} falling simulated players (${expLink("EXP-0015")})`,
      target: "False alarms at or below 0.10, detection not lower",
      status: lower || fa === "misses" ? "misses" : fa === "meets" ? "meets" : "unclear",
      reason:
        lower && prev
          ? `Detection of a 100-point fall is lower than on ${esc(prev.short)}: ${fmt(prev.trajectory.d100.value)} to ${fmt(t.d100.value)}, and the paired interval of the change, ${fmtInterval(cmp.interval)}, excludes 0. The false alarms fell from ${fmt(prev.trajectory.fa.value)} to ${fmt(t.fa.value)}.`
          : "",
      trend: data.map((d) => toPoint(d.trajectory?.fa)),
      trendLabel: "false alarms",
      trend2: data.map((d) => toPoint(d.trajectory?.d100)),
      trend2Label: "100-point falls caught",
      chart: "chart-trajectory",
    });
  }

  // D5: Kaplan–Meier median games to detection.
  const km = benchMetric("km_median_games");
  if (km) {
    rows.push(
      benchRow(
        "d5",
        "D5: median games to detection",
        "Kaplan–Meier median of the games a flawed player plays before the flaw ranks in the top two.",
        km,
        { kind: "max", value: 45 },
        "At or below 45 games",
        benchTrend("km_median_games"),
        benchTier,
        "flawed players",
        null,
        0,
      ),
    );
  } else {
    unmeasured({
      id: "d5",
      group: "drivers",
      name: "D5: median games to detection",
      question:
        "Kaplan–Meier median of the games a flawed player plays before the flaw ranks in the top two.",
      target: "At or below 45 games",
      older: !finite(smoke?.mean_games_to_detection)
        ? ""
        : `The smoke bench's mean games to a detection is ${smoke.mean_games_to_detection} (${link(`${GITHUB_BLOB}/bench/baseline.json`, "bench/baseline.json")}), with no interval, and a mean leaves out the players never detected.`,
      links: `The bench history, ${issueLink(58)}.`,
    });
  }

  // D6: stability.
  const jac = benchMetric("top2_jaccard");
  if (jac) {
    rows.push(
      benchRow(
        "d6",
        "D6: stability of the top two",
        "Jaccard agreement of a player's top two weaknesses between adjacent checkpoints.",
        jac,
        { kind: "min", value: 0.85 },
        "At or above 0.85",
        benchTrend("top2_jaccard"),
        benchTier,
        "players",
      ),
    );
  } else if (finite(smoke?.top2_agreement)) {
    rows.push({
      id: "d6",
      group: "drivers",
      name: "D6: stability of the top two",
      older: `Shown as the smoke bench's top-2 agreement (${link(`${GITHUB_BLOB}/bench/baseline.json`, "bench/baseline.json")}), the metric today's data supports; top-2 Jaccard replaces it with the bench changes of ADR 0119.`,
      question: "Do a player's top two weaknesses stay put between adjacent checkpoints?",
      tier: "synthetic",
      value: smoke.top2_agreement,
      display: fmt(smoke.top2_agreement),
      interval: null,
      intervalText: "No interval recorded",
      basis: "The smoke suite's flawed players; the baseline records no count, so n is not known",
      target: "At or above 0.85",
      status: "unjudged",
      trend: [],
    });
  } else {
    unmeasured({
      id: "d6",
      group: "drivers",
      name: "D6: stability of the top two",
      question: "Do a player's top two weaknesses stay put between adjacent checkpoints?",
      target: "At or above 0.85",
      links: `The bench history, ${issueLink(58)}.`,
    });
  }

  // D7: cost.
  const cpu = benchMetric("cpu_seconds_per_game");
  if (cpu) {
    rows.push(
      benchRow(
        "d7",
        "D7: engine cost per game",
        "Engine CPU-seconds per analysed game on the bench runs.",
        cpu,
        { kind: "max", value: 40 },
        "At or below 40 CPU-seconds",
        benchTrend("cpu_seconds_per_game"),
        benchTier,
        "games",
        "chart-cost",
        1,
      ),
    );
  } else {
    unmeasured({
      id: "d7",
      group: "drivers",
      name: "D7: engine cost per game, and p95 time to analysed",
      question:
        "Engine CPU-seconds per game (and the 95th percentile of the time to a game's analysis), on bench runs only, never app users.",
      target: "At or below 40 CPU-seconds a game",
      older: `${expLink("EXP-0008")} measured 39.1 CPU-seconds a game at 1 million nodes when algo-4.0.0 shipped, once and with no interval.`,
      links: `${issueLink(67, "cli kpi, #67")}.`,
    });
  }

  // Guards.
  const det = benchMetric("deterministic");
  const smokeDet = smoke?.deterministic;
  if (det || typeof smokeDet === "boolean")
    rows.push({
      id: "determinism",
      group: "guards",
      name: "Determinism",
      question: "Do two runs on the same seeds give byte-identical results?",
      tier: det ? benchTier : "synthetic",
      value: det ? det.value : Number(smokeDet),
      display: (det ? det.value === 1 : smokeDet) ? "Identical" : "Different",
      interval: null,
      intervalText: "Exact",
      basis: det
        ? "The bench runs"
        : `The smoke suite (${link(`${GITHUB_BLOB}/bench/baseline.json`, "bench/baseline.json")}) and every experiment's rerun`,
      target: "Exact",
      status: (det ? det.value === 1 : smokeDet) ? "meets" : "misses",
      trend: [],
    });
  if (now?.weakest) {
    const w = now.weakest;
    rows.push({
      id: "motif-precision",
      group: "guards",
      name: "Trusted motif precision",
      question:
        "Is every motif tag a player is told about right at least 80% of the time against Lichess's puzzle themes?",
      tier: "real",
      value: w.m.value,
      display: `at least ${fmt(Math.floor(w.m.interval[0] * 100) / 100, 2)}`,
      context: `the lowest lower end of the ${now.trusted?.count ?? ""} trusted tags, for ${esc(w.tag)} (estimate ${fmt(w.m.value)})`,
      interval: w.m.interval,
      level: w.m.level,
      basis: `Lichess puzzles (${expLink("EXP-0007")})`,
      target: "Every trusted tag at or above 0.80",
      status: statusFor(w.m.interval, { kind: "min", value: 0.8 }),
      trend: data.map((d) => toPoint(d.weakest?.m)),
      chart: "chart-motifs",
    });
  }
  if (now?.rho1) {
    rows.push({
      id: "peer-coverage",
      group: "guards",
      name: "Coverage at the peers' rate",
      question:
        "For players at their peers' rate, how often does the 80% range hold the true rate?",
      tier: "synthetic",
      value: now.rho1.value,
      display: fmt(now.rho1.value),
      interval: now.rho1.interval,
      level: now.rho1.level,
      basis: `Simulated players at the peers' rate (${expLink("EXP-0002")})`,
      target: "In [0.75, 0.85]",
      status: statusFor(now.rho1.interval, { kind: "band", band: [0.75, 0.85] }),
      trend: data.map((d) => toPoint(d.rho1)),
      chart: "chart-calibration",
    });
  }
  return { versions, champion, data, decisions, rows, board, fixture, history: Boolean(history) };
}

function benchRow(
  id,
  name,
  question,
  m,
  target,
  targetText,
  trend,
  tier,
  unit,
  chart = null,
  d = 3,
) {
  return {
    id,
    group: id.startsWith("d") ? "drivers" : "guards",
    name,
    question,
    tier,
    value: m.value,
    display: fmt(m.value, d),
    interval: m.interval,
    level: m.level,
    digits: d === 3 ? 2 : d,
    basis: finite(m.n) ? `${m.n} ${unit} (the bench history)` : "The bench history",
    target: targetText,
    status: statusFor(m.interval, target),
    trend,
    chart,
  };
}

const toPoint = (m) =>
  m && finite(m.value)
    ? { value: m.value, interval: Array.isArray(m.interval) ? m.interval : null }
    : null;
const flawLabel = (f) => FLAWS.find(([k]) => k === f)?.[1] ?? f;
function previousWith(data, champion, has) {
  const i = data.findIndex((d) => d.version === champion);
  for (let j = i - 1; j >= 0; j--) {
    const d = data[j];
    if (d && has(d) && JSON.stringify(has(d)) !== JSON.stringify(has(data[i]))) return d;
  }
  return null;
}

// ---- SVG -----------------------------------------------------------------------------------

const r1 = (x) => Math.round(x * 10) / 10;
/** A whole SVG, with a title and description for assistive tech. */
function svg(id, w, h, title, desc, body) {
  return `<svg class="kc-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img" aria-labelledby="${id}-t ${id}-d"><title id="${id}-t">${esc(title)}</title><desc id="${id}-d">${esc(desc)}</desc>${body}</svg>`;
}
const text = (x, y, s, cls = "kc-tick", anchor = "middle") =>
  `<text class="${cls}" x="${r1(x)}" y="${r1(y)}" text-anchor="${anchor}">${esc(s)}</text>`;
const line = (x1, y1, x2, y2, cls) =>
  `<line class="${cls}" x1="${r1(x1)}" y1="${r1(y1)}" x2="${r1(x2)}" y2="${r1(y2)}"/>`;

/** A marker: a circle for the first series, a square for the second (shape, not colour, tells them apart). */
function mark(shape, x, y, cls) {
  if (shape === "square")
    return `<rect class="${cls}" x="${r1(x - 4.5)}" y="${r1(y - 4.5)}" width="9" height="9"/>`;
  return `<circle class="${cls}" cx="${r1(x)}" cy="${r1(y)}" r="5"/>`;
}

/**
 * A dot chart over the versions: each point a dot and its interval bar, the version on a
 * categorical axis, a target line or band, and a rule at every version that shipped a decision.
 * @param {{ id: string, width: number, versions: string[], champion: string | null,
 *   series: { label: string, shape?: string, points: ({ value: number, interval?: [number, number] | null, title: string } | null)[] }[],
 *   domain: [number, number], ticks: number[], tickDigits?: number, target?: { value: number, label: string },
 *   band?: { lo: number, hi: number, label: string }, decisions: Record<string, string>, title: string, desc: string }} c
 */
export function dotChart(c) {
  const narrow = c.width < 500;
  const left = 44;
  const labelLane = c.series.length > 1 ? (narrow ? 0 : 128) : 0;
  const right = 12 + labelLane;
  const top = narrow ? 14 : 30;
  const bottom = 40;
  const h = narrow ? 230 : 260;
  const pw = c.width - left - right;
  const ph = h - top - bottom;
  const step = pw / Math.max(1, c.versions.length);
  const cx = (i) => left + step * (i + 0.5);
  const [d0, d1] = c.domain;
  const y = (v) => top + ph - ((Math.min(Math.max(v, d0), d1) - d0) / (d1 - d0)) * ph;
  const parts = [];
  if (c.band) {
    parts.push(
      `<rect class="kc-band" x="${left}" y="${r1(y(c.band.hi))}" width="${r1(pw)}" height="${r1(y(c.band.lo) - y(c.band.hi))}"/>`,
    );
    parts.push(line(left, y(c.band.hi), left + pw, y(c.band.hi), "kc-band-edge"));
    parts.push(line(left, y(c.band.lo), left + pw, y(c.band.lo), "kc-band-edge"));
  }
  for (const t of c.ticks) {
    parts.push(line(left, y(t), left + pw, y(t), "kc-grid"));
    parts.push(text(left - 8, y(t) + 4, fmt(t, c.tickDigits ?? 2), "kc-tick", "end"));
  }
  parts.push(line(left, top + ph, left + pw, top + ph, "kc-axis"));
  c.versions.forEach((v, i) => {
    const s = shortVersion(v);
    parts.push(
      text(cx(i), top + ph + 18, s, v === c.champion ? "kc-tick kc-tick-strong" : "kc-tick"),
    );
    const exp = c.decisions[s];
    if (exp) {
      const x = cx(i) - step / 2;
      parts.push(
        `<g class="kc-decision"><title>${esc(`${s} shipped ${exp}'s decision`)}</title>${line(x, top - (narrow ? 4 : 14), x, top + ph, "kc-rule")}${narrow ? "" : text(x + 4, top - 4, exp, "kc-rule-label", "start")}</g>`,
      );
    }
  });
  if (c.target) {
    parts.push(line(left, y(c.target.value), left + pw, y(c.target.value), "kc-target"));
    parts.push(text(left + 4, y(c.target.value) - 6, c.target.label, "kc-label", "start"));
  }
  if (c.band) parts.push(text(left + pw - 4, y(c.band.lo) + 14, c.band.label, "kc-label", "end"));
  const off = c.series.length > 1 ? Math.min(8, step / 6) : 0;
  // "not run" sits halfway between two gridlines, never on one.
  // The band just above the middle, clear of a target line and its label low on the left.
  const mt = Math.max(0, Math.floor((c.ticks.length - 1) / 2));
  const missingY = (y(c.ticks[mt] ?? d0) + y(c.ticks[mt + 1] ?? d1)) / 2 + 4;
  c.series.forEach((s, si) => {
    const dx = (si - (c.series.length - 1) / 2) * off * 2;
    const shape = s.shape ?? (si ? "square" : "circle");
    s.points.forEach((p, i) => {
      const x = cx(i) + dx;
      if (!p || !finite(p.value)) {
        if (si === 0)
          parts.push(
            `<g><title>${esc(`${shortVersion(c.versions[i])}: not run on this version`)}</title>${text(cx(i), missingY, "not run", "kc-missing")}</g>`,
          );
        return;
      }
      const iv = Array.isArray(p.interval) ? p.interval : null;
      const bar = iv ? line(x, y(iv[1]), x, y(iv[0]), `kc-bar kc-s${si}`) : "";
      parts.push(
        `<g class="kc-point"><title>${esc(p.title)}</title><rect class="kc-hit" x="${r1(x - 10)}" y="${top}" width="20" height="${ph}"/>${bar}${mark(shape, x, y(p.value), `kc-dot kc-s${si}`)}${c.valueLabels ? text(x, y(p.value) - 12, fmt(p.value, c.tickDigits ?? 2), "kc-label") : ""}</g>`,
      );
    });
    if (labelLane) {
      const last = [...s.points].reverse().find((p) => p && finite(p.value));
      if (last) {
        const ly = y(last.value) + 4 + (si ? 14 : -4);
        parts.push(
          `${mark(shape, left + pw + 18, ly - 4, `kc-dot kc-s${si}`)}${text(left + pw + 28, ly, s.label, "kc-label", "start")}`,
        );
      }
    }
  });
  return svg(c.id, c.width, h, c.title, c.desc, parts.join(""));
}

/** The flaw × version matrix: per cell the k of n players found, and a shape for the verdict. */
export function flawMatrix(id, width, fullModel) {
  const narrow = width < 500;
  // At phone width the last three versions only, so every label and count has room.
  const keep = narrow ? 3 : fullModel.versions.length;
  const model = {
    ...fullModel,
    versions: fullModel.versions.slice(-keep),
    data: fullModel.data.slice(-keep),
  };
  const left = narrow ? 168 : 190;
  const top = 8;
  const rowH = 34;
  const bottom = 30;
  const step = (width - left - 8) / Math.max(1, model.versions.length);
  const h = top + rowH * FLAWS.length + bottom;
  const parts = [];
  FLAWS.forEach(([f, label], ri) => {
    const yy = top + rowH * ri + rowH / 2;
    parts.push(line(0, top + rowH * (ri + 1), width, top + rowH * (ri + 1), "kc-grid"));
    parts.push(text(0, yy + 4, label, "kc-label", "start"));
    model.data.forEach((d, i) => {
      const x = left + step * (i + 0.5);
      const cell = d.recall?.flaws?.[f];
      if (!cell) {
        parts.push(
          `<g><title>${esc(`${d.short}: not run`)}</title>${text(x, yy + 4, "not run", "kc-missing")}</g>`,
        );
        return;
      }
      const st = statusFor(cell.interval, { kind: "min", value: 0.8 });
      const shape =
        st === "misses"
          ? `<circle class="kc-cell-miss" cx="${r1(x - 13)}" cy="${r1(yy)}" r="5"/>`
          : st === "meets"
            ? `<circle class="kc-dot kc-s0" cx="${r1(x - 13)}" cy="${r1(yy)}" r="5"/>`
            : `<circle class="kc-cell-half" cx="${r1(x - 13)}" cy="${r1(yy)}" r="5"/><path class="kc-cell-half-fill" d="M${r1(x - 13)} ${r1(yy - 5)} a5 5 0 0 1 0 10 z"/>`;
      parts.push(
        `<g class="kc-point"><title>${esc(`${label}, ${d.short}: ${cell.k} of ${cell.n} found in the top two, ${fmt(cell.value)} ${fmtInterval(cell.interval)} (80% Wilson). ${STATUS_TEXT[st]} 0.80.`)}</title>${shape}${text(x + 2, yy + 4, `${cell.k}/${cell.n}`, "kc-label", "start")}</g>`,
      );
    });
  });
  model.versions.forEach((v, i) => {
    parts.push(
      text(
        left + step * (i + 0.5),
        h - 10,
        shortVersion(v),
        v === model.champion ? "kc-tick kc-tick-strong" : "kc-tick",
      ),
    );
  });
  return svg(
    id,
    width,
    h,
    "Recall at 2 by flaw type and version",
    "For each injected flaw type and algorithm version, how many of the six flawed players had the flaw in their top two. A filled dot: the 80% interval lies wholly at or above 0.80; a half-filled dot: no clear difference from 0.80; a hollow dot: wholly below.",
    parts.join(""),
  );
}

/**
 * The trajectory trade-off: false alarms (x) against detection (y), each version a point with its
 * 80% interval as a cross, an arrow from each version to the next where the point moved.
 */
export function tradeoffChart(id, width, model) {
  const narrow = width < 500;
  const left = 64;
  const right = narrow ? 14 : 150;
  const top = 16;
  const bottom = 46;
  const h = narrow ? 340 : 380;
  const pw = width - left - right;
  const ph = h - top - bottom;
  const x = (v) => left + v * pw;
  const y = (v) => top + ph - v * ph;
  const parts = [
    `<defs><marker id="${id}-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path class="kc-arrowhead" d="M0 0 L10 5 L0 10 z"/></marker></defs>`,
  ];
  for (const t of [0, 0.25, 0.5, 0.75, 1]) {
    parts.push(
      line(left, y(t), left + pw, y(t), "kc-grid"),
      text(left - 8, y(t) + 4, fmt(t, 2), "kc-tick", "end"),
    );
    parts.push(
      line(x(t), top, x(t), top + ph, "kc-grid"),
      text(x(t), top + ph + 18, fmt(t, 2), "kc-tick"),
    );
  }
  parts.push(
    line(left, top + ph, left + pw, top + ph, "kc-axis"),
    line(left, top, left, top + ph, "kc-axis"),
  );
  parts.push(line(x(0.1), top, x(0.1), top + ph, "kc-target"));
  parts.push(text(x(0.1) + 6, top + ph - 8, "False-alarm target 0.10", "kc-label", "start"));
  parts.push(
    text(
      left + pw / 2,
      h - 6,
      "Constant-strength players told “falling” at 120 games",
      "kc-axis-title",
    ),
  );
  parts.push(
    `<text class="kc-axis-title" transform="translate(12 ${r1(top + ph / 2)}) rotate(-90)" text-anchor="middle">Falls caught</text>`,
  );
  const pts = model.data.filter((d) => d.trajectory);
  const series = [
    { key: "d100", label: "100-point fall", shape: "circle", si: 0 },
    { key: "d200", label: "200-point fall", shape: "square", si: 1 },
  ];
  for (const s of series) {
    // Versions with the same numbers share one point (a rerun on the same seeds).
    const groups = [];
    for (const d of pts) {
      const t = d.trajectory;
      const det = t[s.key];
      if (!det) continue;
      const g = groups.at(-1);
      if (g && g.fa === t.fa.value && g.det === det.value) g.versions.push(d.short);
      else
        groups.push({
          fa: t.fa.value,
          det: det.value,
          faIv: t.fa.interval,
          detIv: det.interval,
          n: det.n,
          faN: t.fa.n,
          versions: [d.short],
        });
    }
    groups.forEach((g, i) => {
      const next = groups[i + 1];
      if (next) {
        const [x0, y0, x1, y1] = [x(g.fa), y(g.det), x(next.fa), y(next.det)];
        const len = Math.hypot(x1 - x0, y1 - y0);
        if (len > 20) {
          const k0 = 10 / len;
          const k1 = 12 / len;
          parts.push(
            `<line class="kc-arrow" x1="${r1(x0 + (x1 - x0) * k0)}" y1="${r1(y0 + (y1 - y0) * k0)}" x2="${r1(x1 - (x1 - x0) * k1)}" y2="${r1(y1 - (y1 - y0) * k1)}" marker-end="url(#${id}-arrow)"/>`,
          );
        }
      }
      const cross = `${line(x(g.faIv[0]), y(g.det), x(g.faIv[1]), y(g.det), `kc-bar kc-s${s.si}`)}${line(x(g.fa), y(g.detIv[1]), x(g.fa), y(g.detIv[0]), `kc-bar kc-s${s.si}`)}`;
      const name = g.versions.join(", ");
      parts.push(
        `<g class="kc-point"><title>${esc(`${name}, ${s.label}: told “falling” ${fmt(g.fa)} ${fmtInterval(g.faIv)} of ${g.faN}; caught ${fmt(g.det)} ${fmtInterval(g.detIv)} of ${g.n} (80% Wilson)`)}</title>${cross}${mark(s.shape, x(g.fa), y(g.det), `kc-dot kc-s${s.si}`)}</g>`,
      );
      // A point on the right is where an arrow leaves: its label goes right of the cross. A point
      // on the left is where an arrow lands from the upper right: its label goes below.
      if (g.fa > 0.5) parts.push(text(x(g.faIv[1]) + 8, y(g.det) + 4, name, "kc-label", "start"));
      else parts.push(text(x(g.fa) + 8, y(g.det) + 20, name, "kc-label", "start"));
    });
    if (!narrow && groups.length) {
      const ly = s.si ? top + 56 : top + 36;
      parts.push(
        `${mark(s.shape, left + pw + 20, ly - 4, `kc-dot kc-s${s.si}`)}${text(left + pw + 30, ly, s.label, "kc-label", "start")}`,
      );
    }
  }
  return svg(
    id,
    width,
    h,
    "The trajectory's trade-off between false alarms and detection",
    "Each point is an algorithm version: how often players of constant strength were told their rating was falling at 120 games, against how often a real 100-point or 200-point fall was caught, with 80% intervals as crosses. Arrows run from one version to the next.",
    parts.join(""),
  );
}

/** A sparkline over the arena versions (DESIGN.md "KPI stats"): pencil line, a null breaks it, the latest an ink dot. */
export function sparkline(name, points) {
  const w = 112;
  const h = 28;
  const vals = points.filter((p) => p && finite(p.value));
  if (vals.length < 2) return `<span class="kc-spark-none">Fewer than two versions measured</span>`;
  const lo = Math.min(...vals.map((p) => p.value));
  const hi = Math.max(...vals.map((p) => p.value));
  const last = vals.at(-1);
  const widths = vals.filter((p) => p.interval).map((p) => p.interval[1] - p.interval[0]);
  const minSpan = widths.length ? 2 * widths.at(-1) : 0;
  const span = Math.max(hi - lo, minSpan, 1e-9);
  const mid = (hi + lo) / 2;
  const y = (v) => r1(h - 5 - ((v - (mid - span / 2)) / span) * (h - 10));
  const step = (w - 10) / Math.max(1, points.length - 1);
  const segs = [];
  let cur = [];
  points.forEach((p, i) => {
    if (p && finite(p.value)) cur.push(`${r1(5 + i * step)},${y(p.value)}`);
    else if (cur.length) {
      segs.push(cur);
      cur = [];
    }
  });
  if (cur.length) segs.push(cur);
  const body = segs
    .map((s) =>
      s.length > 1
        ? `<polyline class="kc-spark" points="${s.join(" ")}"/>`
        : `<circle class="kc-spark-lone" cx="${s[0].split(",")[0]}" cy="${s[0].split(",")[1]}" r="1.5"/>`,
    )
    .join("");
  const li = points.lastIndexOf(last);
  const first = vals[0];
  const label = `${name}: ${vals.length} of ${points.length} versions measured, from ${fmt(first.value)} to ${fmt(last.value)}`;
  return `<svg class="kc-spark-svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img" aria-label="${esc(label)}">${body}<circle class="kc-spark-last" cx="${r1(5 + li * step)}" cy="${y(last.value)}" r="3"/></svg>`;
}

// ---- the charts, from the model ---------------------------------------------------------------

const title = (d, value, iv, level, n) =>
  `${d.short}: ${fmt(value)}${iv ? ` ${fmtInterval(iv)}` : ""}${level ? ` (${levelText(level)})` : ""}${n ? `, ${n}` : ""}`;

/** Every chart, wide and narrow, keyed by its id. */
export function charts(model) {
  const base = { versions: model.versions, champion: model.champion, decisions: model.decisions };
  const out = {};
  const both = (id, make) => {
    out[id] = { wide: make(`${id}-w`, 680), narrow: make(`${id}-n`, 340) };
  };
  both("chart-recall", (id, width) =>
    dotChart({
      ...base,
      id,
      width,
      domain: [0, 1],
      ticks: [0, 0.25, 0.5, 0.75, 1],
      target: { value: 0.8, label: "Target 0.80" },
      series: [
        {
          label: "Recall at 2",
          points: model.data.map((d) =>
            d.recall
              ? {
                  value: d.recall.value,
                  interval: d.recall.interval,
                  title: title(
                    d,
                    d.recall.value,
                    d.recall.interval,
                    LEVEL,
                    `${d.recall.k} of ${d.recall.n} flawed players`,
                  ),
                }
              : null,
          ),
        },
      ],
      title: "Recall at 2 per version",
      desc: "Pooled recall at 2 at 80 games for each algorithm version, with its 80% Wilson interval and the target of 0.80.",
    }),
  );
  both("chart-flaws", (id, width) => flawMatrix(id, width, model));
  both("chart-calibration", (id, width) =>
    dotChart({
      ...base,
      id,
      width,
      domain: [0, 1],
      ticks: [0, 0.25, 0.5, 0.75, 1],
      band: { lo: 0.75, hi: 0.85, label: "Band 0.75 to 0.85" },
      series: [
        {
          label: "Worst cell",
          points: model.data.map((d) =>
            d.worst
              ? {
                  value: d.worst.value,
                  interval: d.worst.interval,
                  title: title(
                    d,
                    d.worst.value,
                    d.worst.interval,
                    d.worst.level,
                    `worst at ${d.worst.at}`,
                  ),
                }
              : null,
          ),
        },
        {
          label: "At the peers' rate",
          points: model.data.map((d) =>
            d.rho1
              ? {
                  value: d.rho1.value,
                  interval: d.rho1.interval,
                  title: title(
                    d,
                    d.rho1.value,
                    d.rho1.interval,
                    d.rho1.level,
                    "at the peers' rate",
                  ),
                }
              : null,
          ),
        },
      ],
      title: "Calibration of the 80% ranges per version",
      desc: "The worst of the 20 coverages of the 80% range, and the coverage at the peers' rate, for each algorithm version, with Bonferroni 95% intervals and the band from 0.75 to 0.85.",
    }),
  );
  both("chart-trajectory", (id, width) => tradeoffChart(id, width, model));
  both("chart-motifs", (id, width) =>
    dotChart({
      ...base,
      id,
      width,
      domain: [0, 15],
      ticks: [0, 5, 10, 15],
      tickDigits: 0,
      valueLabels: true,
      series: [
        {
          label: "Trusted tags",
          points: model.data.map((d) =>
            d.trusted
              ? {
                  value: d.trusted.count,
                  interval: null,
                  title: `${d.short}: ${d.trusted.count} of ${d.trusted.of} tags trusted (${d.trusted.tags.join(", ")})`,
                }
              : null,
          ),
        },
      ],
      title: "Motif tags trusted per version",
      desc: "How many of the 15 motif tags EXP-0007's registered rule trusts, for each algorithm version.",
    }),
  );
  if (model.data.some((d) => d.bench?.cpu_seconds_per_game))
    both("chart-cost", (id, width) =>
      dotChart({
        ...base,
        id,
        width,
        domain: [0, 60],
        ticks: [0, 20, 40, 60],
        tickDigits: 0,
        target: { value: 40, label: "Target 40 s" },
        series: [
          {
            label: "CPU-seconds a game",
            points: model.data.map((d) => {
              const m = d.bench?.cpu_seconds_per_game;
              return m && finite(m.value)
                ? {
                    value: m.value,
                    interval: m.interval,
                    title: `${d.short}: ${fmt(m.value, 1)} CPU-seconds a game ${fmtInterval(m.interval, 1) ?? ""} (${levelText(m.level)})`,
                  }
                : null;
            }),
          },
        ],
        title: "Engine cost per game, per version",
        desc: "Engine CPU-seconds per analysed game on the bench runs, with 80% intervals and the target of 40.",
      }),
    );
  both("chart-depth", (id, width) =>
    dotChart({
      ...base,
      id,
      width,
      domain: [0, 0.04],
      ticks: [0, 0.01, 0.02, 0.03, 0.04],
      target: { value: 0.01, label: "Tolerance 0.01" },
      series: [
        {
          label: "100k to 300k",
          points: model.data.map((d) =>
            d.stepDown
              ? {
                  value: d.stepDown.value,
                  interval: d.stepDown.interval,
                  title: title(
                    d,
                    d.stepDown.value,
                    d.stepDown.interval,
                    d.stepDown.level,
                    "D(100,000) − D(300,000)",
                  ),
                }
              : null,
          ),
        },
        {
          label: "300k to 1M",
          points: model.data.map((d) =>
            d.stepUp
              ? {
                  value: d.stepUp.value,
                  interval: d.stepUp.interval,
                  title: title(
                    d,
                    d.stepUp.value,
                    d.stepUp.interval,
                    d.stepUp.level,
                    "D(300,000) − D(1 million)",
                  ),
                }
              : null,
          ),
        },
      ],
      title: "What a deeper search changes, per version",
      desc: "The share of moves whose label differs from the reference, removed by each step up in nodes (EXP-0008), with 95% intervals and the tolerance of 0.01.",
    }),
  );
  return out;
}

// ---- the page ------------------------------------------------------------------------------

const TIER_TEXT = {
  synthetic: "Synthetic",
  real: "Real",
  unmeasured: "Unmeasured",
  fixture: "Fixture",
};
const GROUPS = [
  ["north", "North Star"],
  ["drivers", "Drivers"],
  ["guards", "Guards"],
];

function rowHtml(row) {
  const ivText = row.intervalText ?? fmtInterval(row.interval, row.digits ?? 2) ?? "";
  const lvl =
    row.level !== undefined && row.interval && !row.intervalText
      ? `, ${esc(levelText(row.level))}`
      : "";
  const measured = row.tier !== "unmeasured";
  const value = measured
    ? `<p class="sc-value"><span class="sc-num">${row.display}</span>${row.context ? ` <span class="sc-context">${row.context}</span>` : ""}<span class="sc-sub">${esc(ivText)}${lvl}</span></p>`
    : `<p class="sc-value"><span class="sc-dash">—</span><span class="sc-sub">No measurement yet</span></p>`;
  // A trade-off row draws both halves, so the scan never shows the good half alone.
  const spark = (pts, label) =>
    `<div class="sc-trend">${sparkline(`${row.name} trend${label ? ` (${label})` : ""}`, pts)}<span class="sc-trend-label">Last ${pts.length} versions${label ? `, ${esc(label)}` : ""}</span></div>`;
  const trend =
    measured && (row.trend ?? []).length
      ? `<div class="sc-trends">${spark(row.trend, row.trendLabel)}${row.trend2 ? spark(row.trend2, row.trend2Label) : ""}</div>`
      : "";
  const notes = [
    row.reason ? `<p class="sc-note">${row.reason}</p>` : "",
    row.older ? `<p class="sc-note">${row.older}</p>` : "",
    row.links ? `<p class="sc-note">${measured ? "" : "To measure it: "}${row.links}</p>` : "",
    row.chart ? `<p class="sc-note"><a href="#${row.chart}">The chart</a></p>` : "",
  ].join("");
  return `<li class="sc-row sc-${row.tier}" id="kpi-${row.id}"><div class="sc-head"><h3 class="sc-name">${esc(row.name)}</h3><span class="sc-tier sc-tier-${row.tier}">${TIER_TEXT[row.tier]}</span>${measured ? `<span class="sc-status sc-status-${row.status}">${STATUS_TEXT[row.status]}</span>` : ""}</div><p class="sc-question">${esc(row.question)}</p><div class="sc-measure">${value}${trend}</div><dl class="sc-facts"><div><dt>Based on</dt><dd>${row.basis ?? "—"}</dd></div><div><dt>Target</dt><dd>${esc(row.target)}</dd></div></dl>${notes}</li>`;
}

/**
 * How a paired change reads, from its interval alone: "higher" or "lower" when the interval
 * excludes 0, "no clear difference" only when it holds 0. Null without an interval.
 * @returns {"higher" | "lower" | "no clear difference" | null}
 */
export function changeReading(c) {
  if (!c || !Array.isArray(c.interval) || !c.interval.every(finite)) return null;
  const [lo, hi] = c.interval;
  if (lo > 0) return "higher";
  if (hi < 0) return "lower";
  return "no clear difference";
}

/**
 * The arena's comparison of the champion with the version before it, in words derived from the
 * interval. A guard that passes on a floor while the change excludes 0 says so in its own clause.
 */
export function verdictText(model, exp, metricName, label) {
  if (!model.champion) return "";
  const r = recordOf(model.board, exp, model.champion);
  const c = r?.comparison?.[metricName];
  if (!r || !c || !r.against || r.against === r.algorithm_id) return "";
  const [v, a] = [esc(shortVersion(r.algorithm_id)), esc(shortVersion(r.against))];
  const reading = changeReading(c);
  if (!reading) {
    // No interval (a rule's verdict, such as a count): name the rule's verdict as the rule's.
    return `${esc(label)}: ${v} against ${a} has no interval to read; its registered rule finds ${c.verdict === "better" ? "it better" : c.verdict === "worse" ? "it worse" : "no change"}.`;
  }
  const change = `the paired change ${fmt(c.effect)}, ${levelText(c.level)} interval ${fmtInterval(c.interval)}`;
  if (reading === "no clear difference") {
    return `${esc(label)}: ${v} is no clear difference from ${a} (${change}, which holds 0).`;
  }
  let text = `${esc(label)}: ${v} is ${reading} than ${a} (${change}, which excludes 0)`;
  if (finite(c.floor)) {
    const now = r.metrics?.[metricName]?.value;
    const before = recordOf(model.board, exp, r.against)?.metrics?.[metricName]?.value;
    const moved = finite(now) && finite(before) ? ` (${fmt(before)} to ${fmt(now)})` : "";
    text += `. The arena's guard passed only because it checks a floor of ${fmt(c.floor, 1)}, which ${v} clears${moved}`;
  }
  return `${text}.`;
}

function tableHtml(caption, head, rows) {
  return `<details class="sc-numbers"><summary>${esc(caption)}</summary><div class="lab-table-wrap"><table class="lab-leaderboard"><thead><tr>${head.map((h) => `<th scope="col">${esc(h)}</th>`).join("")}</tr></thead><tbody>${rows.map((r) => `<tr>${r.map((c, i) => (i ? `<td>${c}</td>` : `<th scope="row">${c}</th>`)).join("")}</tr>`).join("")}</tbody></table></div></details>`;
}

function chartBlock(id, c, caption, verdicts, table) {
  return `<figure class="kc-figure" id="${id}"><div class="kc-wide">${c.wide}</div><div class="kc-narrow">${c.narrow}</div><figcaption>${caption}</figcaption></figure>${verdicts.filter(Boolean).length ? `<p class="kc-verdict">${verdicts.filter(Boolean).join(" ")}</p>` : ""}${table}`;
}

const ivCell = (m) =>
  m
    ? `${fmt(m.value)} ${fmtInterval(m.interval) ?? ""}`
    : '<span class="lab-missing">Not run</span>';

/** What the recall chart shows, from the champion's per-type counts. */
function recallNarrative(model) {
  const now = model.data.find((d) => d.version === model.champion)?.recall;
  if (!now) return "";
  const versions = model.data.filter((d) => d.recall);
  const same = versions.every((d) => d.recall.k === now.k && d.recall.n === now.n);
  const types = FLAWS.filter(([f]) => now.flaws[f]).map(
    ([f, label]) => `${label.toLowerCase()} ${now.flaws[f].k} of ${now.flaws[f].n}`,
  );
  return `${same ? `The recall is ${now.k} of ${now.n} on all ${versions.length} versions.` : ""} By type, on ${esc(shortVersion(model.champion))}: ${types.join(", ")}. The pooled number is set by which types the bench injects, so no tuning of the ranking moves it until the missing types are detected. The matrix shows it.`;
}

/** What the trade-off chart shows: both halves of the change, from the arena's paired comparison. */
function trajectoryNarrative(model, t) {
  const now = t.at(-1);
  const before = [...t].reverse().find((d) => d.trajectory.fa.value !== now.trajectory.fa.value);
  if (!before) return "";
  const r = recordOf(model.board, "EXP-0015", now.version);
  const det = r?.comparison?.detection_100;
  const fa = r?.comparison?.false_alarm_120;
  const exp = model.decisions[now.short];
  const reading = changeReading(det);
  const lost =
    reading === "lower"
      ? ` The paired change, ${fmt(det.effect)} with ${levelText(det.level)} interval ${fmtInterval(det.interval)}, excludes 0: this is a real loss of detection, the price of the fewer false alarms.`
      : reading
        ? ` The paired change, ${fmt(det.effect)} with ${levelText(det.level)} interval ${fmtInterval(det.interval)}, ${reading === "higher" ? "excludes 0, upwards" : "holds 0: no clear difference"}.`
        : "";
  return `The rule that shipped in ${esc(now.short)}${exp ? `, decided by ${expLink(exp)},` : ""} cut the share of constant-strength players told "falling" at 120 games from ${fmt(before.trajectory.fa.value)} to ${fmt(now.trajectory.fa.value)}${fa ? ` (paired change ${fmt(fa.effect)}, ${levelText(fa.level)} interval ${fmtInterval(fa.interval)})` : ""}. It also catches a 100-point fall ${fmt(now.trajectory.d100.value)} of the time, down from ${fmt(before.trajectory.d100.value)}.${lost} Both halves belong in any report of this change.`;
}

/** The scorecard page's body (markdown with HTML blocks; no blank line inside a block). */
export function scorecardPage(model) {
  const c = charts(model);
  const decided = Object.entries(model.decisions)
    .filter(([v]) => model.versions.some((x) => shortVersion(x) === v))
    .map(([v, e]) => `${v} shipped ${expLink(e)}`)
    .join(", ");
  const rules = decided
    ? ` The vertical rules mark the versions that shipped an experiment's decision: ${decided}.`
    : "";
  const counts = {
    measured: model.rows.filter((r) => r.tier !== "unmeasured").length,
    unmeasured: model.rows.filter((r) => r.tier === "unmeasured").length,
  };
  const out = [];
  out.push(
    `Is Rematch a good engine? This page answers with the numbers it has, and shows the ones it does not have yet. ${counts.measured} measures have a value and ${counts.unmeasured} are unmeasured; the unmeasured ones stay on the page as grey rows, each with the experiment or issue that will measure it. The champion is <code>${esc(model.champion ?? "none")}</code>, the version on main.`,
    "",
    `Each row says where its number comes from. **Synthetic** means simulated players with flaws we injected, so we know the truth; **real** means real games or puzzles. A synthetic number shows the engine finds what it was built to find; only a real one shows it helps real players. Intervals are 80% unless the row says otherwise, rounded outward. A row meets its target only when its whole interval does, misses it only when its whole interval misses, and otherwise reads "no clear difference from the target". The sparkline runs over the last ${model.versions.length} versions the arena re-ran (${esc(model.versions.map(shortVersion).join(", "))}).`,
    "",
  );
  if (model.fixture) {
    out.push(
      `<p class="sc-fixture">The bench numbers on this page come from a test fixture. They are not measurements.</p>`,
      "",
    );
  }
  for (const [g, label] of GROUPS) {
    const rows = model.rows.filter((r) => r.group === g);
    if (!rows.length) continue;
    out.push(
      `## ${label}`,
      "",
      `<ol class="sc-rows" role="list">${rows.map(rowHtml).join("")}</ol>`,
      "",
    );
  }
  out.push(
    "## The charts",
    "",
    `Each chart puts the version on its own column, oldest on the left, the champion in bold.${rules} Hover a mark for its numbers, or open the table under the chart. Intervals are 80% where the scorecard computes them; where it shows an experiment's own record it keeps that record's level (95%, or higher when the experiment corrects for several comparisons), and says so.`,
    "",
  );

  out.push("### Recall at 2", "");
  out.push(
    chartBlock(
      "chart-recall",
      c["chart-recall"],
      `Pooled recall at 2 at 80 games, from ${expLink("EXP-0001")}: the share of 30 simulated flawed players whose flaw ranks in their top two, with its 80% Wilson interval. The dashed line is the target of 0.80.`,
      [verdictText(model, "EXP-0001", "recall_at_2", "The arena")],
      tableHtml(
        "The numbers",
        ["Version", "Recall at 2 [80%]", "Players found"],
        model.data.map((d) => [
          esc(d.short),
          d.recall
            ? `${fmt(d.recall.value)} ${fmtInterval(d.recall.interval)}`
            : '<span class="lab-missing">Not run</span>',
          d.recall ? `${d.recall.k} of ${d.recall.n}` : "—",
        ]),
      ),
    ),
    "",
  );
  out.push(recallNarrative(model), "");
  out.push(
    chartBlock(
      "chart-flaws",
      c["chart-flaws"],
      `Recall at 2 by flaw type: k of n players found per version, six players per type. A filled dot means the type's 80% interval lies wholly at or above 0.80, a half-filled one no clear difference from 0.80, a hollow one wholly below. With six players a type found six times out of six still reads "no clear difference" (its interval starts at 0.78). ${expLink("EXP-0026")} (${issueLink(127)}) is the lever on missed mates. At phone width the matrix shows the last three versions; the table has them all.`,
      [],
      tableHtml(
        "The numbers",
        ["Flaw type", ...model.data.map((d) => d.short)],
        FLAWS.map(([f, label]) => [
          esc(label),
          ...model.data.map((d) => {
            const x = d.recall?.flaws?.[f];
            return x ? `${x.k}/${x.n} ${fmtInterval(x.interval)}` : "—";
          }),
        ]),
      ),
    ),
    "",
  );

  out.push("### Calibration", "");
  out.push(
    chartBlock(
      "chart-calibration",
      c["chart-calibration"],
      `How often the 80% range holds the true rate, from ${expLink("EXP-0002")}: the worst of the 20 weakness × sample-size cells (circles) and the coverage at the peers' rate (squares), with Bonferroni 95% intervals, as the experiment records them; with 3,600 players a cell most intervals are narrower than their dot. The shaded band is 0.75 to 0.85.`,
      [
        verdictText(model, "EXP-0002", "worst_coverage", "Worst cell"),
        verdictText(model, "EXP-0002", "rho1_coverage", "At the peers' rate"),
      ],
      tableHtml(
        "The numbers",
        ["Version", "Worst cell", "Where", "At the peers' rate"],
        model.data.map((d) => [
          esc(d.short),
          ivCell(d.worst),
          d.worst ? esc(d.worst.at ?? "") : "—",
          ivCell(d.rho1),
        ]),
      ),
    ),
    "",
  );

  out.push("### The trajectory's trade-off", "");
  const t = model.data.filter((d) => d.trajectory);
  const first = t[0]?.trajectory;
  const lastT = t.at(-1)?.trajectory;
  out.push(
    chartBlock(
      "chart-trajectory",
      c["chart-trajectory"],
      `Each point is a version: how often a player of constant strength was told "falling" at 120 games (across) against how often a real fall was caught (up), with 80% Wilson intervals as crosses, from ${expLink("EXP-0015")}'s registered seeds. Up and to the left is better. Versions with identical numbers share a point. EXP-0015 first ran on 4.0.1, so older versions have no point.`,
      [
        verdictText(model, "EXP-0015", "false_alarm_120", "False alarms at 120 games"),
        verdictText(model, "EXP-0015", "detection_100", "Detection of a 100-point fall"),
        verdictText(model, "EXP-0015", "detection_200", "Detection of a 200-point fall"),
      ],
      tableHtml(
        "The numbers",
        ["Version", "Told “falling”, 120 games", "100-point fall caught", "200-point fall caught"],
        t.map((d) => [
          esc(d.short),
          `${fmt(d.trajectory.fa.value)} ${fmtInterval(d.trajectory.fa.interval)} of ${d.trajectory.fa.n}`,
          `${fmt(d.trajectory.d100.value)} ${fmtInterval(d.trajectory.d100.interval)} of ${d.trajectory.d100.n}`,
          d.trajectory.d200
            ? `${fmt(d.trajectory.d200.value)} ${fmtInterval(d.trajectory.d200.interval)} of ${d.trajectory.d200.n}`
            : "—",
        ]),
      ),
    ),
    "",
  );
  if (first && lastT && t.length > 1) {
    out.push(trajectoryNarrative(model, t), "");
  }

  out.push("### Motif tags", "");
  out.push(
    chartBlock(
      "chart-motifs",
      c["chart-motifs"],
      `How many of the 15 motif tags ${expLink("EXP-0007")}'s registered rule trusts against Lichess's puzzle themes (real data). No interval: the count follows from each tag's own precision interval. A trusted tag must stay at or above 0.80 precision; the guard row above gives the weakest.`,
      [verdictText(model, "EXP-0007", "trusted_tags", "The arena")],
      tableHtml(
        "The numbers",
        ["Version", "Trusted", "Tags"],
        model.data.map((d) => [
          esc(d.short),
          d.trusted
            ? `${d.trusted.count} of ${d.trusted.of}`
            : '<span class="lab-missing">Not run</span>',
          d.trusted ? esc(d.trusted.tags.join(", ")) : "—",
        ]),
      ),
    ),
    "",
  );

  out.push("### Search depth", "");
  out.push(
    chartBlock(
      "chart-depth",
      c["chart-depth"],
      `What each step up in search depth changes, from ${expLink("EXP-0008")} on 160 real rated games: the share of moves whose label moves closer to the reference, from 100,000 to 300,000 nodes (circles) and from 300,000 to 1 million (squares), with 95% intervals. The rule: a step is worth its cost when its interval lies wholly above the tolerance of 0.01.`,
      [verdictText(model, "EXP-0008", "step_up", "300,000 to 1 million")],
      tableHtml(
        "The numbers",
        ["Version", "100k to 300k", "300k to 1M", "Nodes"],
        model.data.map((d) => [
          esc(d.short),
          ivCell(d.stepDown),
          ivCell(d.stepUp),
          d.depthAt ? esc(d.depthAt) : "—",
        ]),
      ),
    ),
    "",
  );

  if (c["chart-cost"]) {
    out.push("### Engine cost", "");
    out.push(
      chartBlock(
        "chart-cost",
        c["chart-cost"],
        `Engine CPU-seconds per analysed game on the bench runs${model.fixture ? " (test fixture, not a measurement)" : ""}, with 80% intervals. The dashed line is the target of 40.`,
        [],
        tableHtml(
          "The numbers",
          ["Version", "CPU-seconds a game"],
          model.data.map((d) => {
            const m = d.bench?.cpu_seconds_per_game;
            return [
              esc(d.short),
              m
                ? `${fmt(m.value, 1)} ${fmtInterval(m.interval, 1) ?? ""}`
                : '<span class="lab-missing">Not run</span>',
            ];
          }),
        ),
      ),
      "",
    );
  }
  out.push(
    "### Not drawn yet",
    "",
    c["chart-cost"]
      ? `Kaplan–Meier curves of the games to a detection, per flaw (D5), need the bench history to record the curves, not only the median (${issueLink(58)}).`
      : `Kaplan–Meier curves of the games to a detection, per flaw (D5), and the engine's cost per game with its interval (D7) need the bench history (${issueLink(58)}) and ${issueLink(67, "cli kpi")}. They appear here once a run records them.`,
    "",
    `<p class="generated-source">Generated at build time by <a href="${GITHUB_BLOB}/site/scripts/kpi-charts.mjs">site/scripts/kpi-charts.mjs</a> from <a href="${GITHUB_BLOB}/lab/leaderboard.json">lab/leaderboard.json</a>${model.history ? " and the bench history" : ""}, with no script in the page.</p>`,
    "",
  );
  return out.join("\n");
}

/** A compact copy of the model for the landing page and the tests (src/data/generated/scorecard.json). */
export function scorecardData(model) {
  return {
    champion: model.champion,
    versions: model.versions.map(shortVersion),
    fixture: model.fixture,
    rows: model.rows.map((r) => ({
      id: r.id,
      group: r.group,
      name: r.name,
      tier: r.tier,
      value: finite(r.value) ? r.value : null,
      interval: Array.isArray(r.interval) ? r.interval : null,
      level: r.interval ? (r.level ?? null) : null,
      status: r.status,
    })),
  };
}

/** Writes the SVGs and scorecard.json to `dir` and returns the page body. */
export function writeScorecard(dir, model) {
  mkdirSync(join(dir, "scorecard"), { recursive: true });
  for (const [id, c] of Object.entries(charts(model))) {
    const xml = (s) => s.replace("<svg ", '<svg xmlns="http://www.w3.org/2000/svg" ');
    writeFileSync(join(dir, "scorecard", `${id}.svg`), `${xml(c.wide)}\n`);
    writeFileSync(join(dir, "scorecard", `${id}-narrow.svg`), `${xml(c.narrow)}\n`);
  }
  writeFileSync(join(dir, "scorecard.json"), `${JSON.stringify(scorecardData(model), null, 2)}\n`);
  return scorecardPage(model);
}
