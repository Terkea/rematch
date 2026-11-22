import { describe, expect, it } from "vitest";
import { emptyState, toViewB, toViewC } from "../src/app/lab/overview/view.js";
import { classes, empty, overview } from "./helpers/overview-fixture.js";

// Variants b and c are client components. Whatever they receive is serialised into the RSC
// payload and the HTML, so they get a narrow view model holding only what they render.

/** Every key the view models may carry. Anything else (raw rows, opponents, ids) is a leak. */
const ALLOWED = new Set([
  // header and empty states
  "header",
  "accounts",
  "name",
  "notes",
  "status",
  "empty",
  "kind",
  "total",
  // estimates (EstimateView)
  "value",
  "interval",
  "low",
  "high",
  "level",
  "basis",
  "games",
  "timeClass",
  "note",
  "need",
  // KPI rows
  "key",
  "label",
  "format",
  "view",
  "trend",
  // the accuracy trend
  "points",
  "id",
  "date",
  "accuracy",
  "result",
  "caption",
  "href",
  "rolling",
  "mean",
  // judgement columns and result rows
  "columns",
  "columnsLabel",
  "blunders",
  "mistakes",
  "inaccuracies",
  "wins",
  "draws",
  "losses",
  // variant b
  "lead",
  "kpis",
  "results",
  "scores",
  // variant c
  "summary",
  "open",
  "rows",
  "split",
  "score",
  "detail",
]);

function keysOf(value: unknown, into = new Set<string>()): Set<string> {
  if (Array.isArray(value)) for (const v of value) keysOf(v, into);
  else if (value !== null && typeof value === "object") {
    for (const [k, v] of Object.entries(value)) {
      into.add(k);
      keysOf(v, into);
    }
  }
  return into;
}

const models = {
  b: () => toViewB(overview()),
  c: () => toViewC(overview(), classes()),
};

describe.each(["b", "c"] as const)("variant %s client props", (v) => {
  it("carry only the allowed keys", () => {
    const extra = [...keysOf(models[v]())].filter((k) => !ALLOWED.has(k));
    expect(extra).toEqual([]);
  });

  it("are plain JSON, with dates as strings", () => {
    const model = models[v]();
    expect(JSON.parse(JSON.stringify(model))).toStrictEqual(model);
  });

  it("never carry the recent games, opponents or the account's identity", () => {
    const json = JSON.stringify(models[v]());
    for (const secret of ["SecretOpponent", "SomePlayer", '"acc"', "King's Pawn"]) {
      expect(json).not.toContain(secret);
    }
  });
});

describe("emptyState", () => {
  it("names the state with nothing to chart, and is null when there is something", () => {
    expect(emptyState(empty())).toEqual({ kind: "no-links" });
    const base = overview();
    expect(
      emptyState({ ...base, status: { ...base.status, total: 0, analysed: 0, waiting: 0 } }),
    ).toEqual({ kind: "no-games" });
    expect(emptyState({ ...base, status: { ...base.status, total: 4, analysed: 0 } })).toEqual({
      kind: "none-analysed",
      total: 4,
    });
    expect(emptyState(base)).toBeNull();
  });
});
