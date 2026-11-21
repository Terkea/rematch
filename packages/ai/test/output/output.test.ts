import { zodOutputFormat } from "@anthropic-ai/sdk/helpers/zod";
import { describe, expect, it } from "vitest";
import {
  AI_DISCLOSURE,
  DRILL_TYPES,
  drillsFor,
  makeDisclosure,
  ReportSectionEnvelope,
} from "../../src/index.js";

const item = { text: "x", citations: [{ gameId: "g1", ply: 3 }] };
const empty = { overview: null, weaknesses: null, openings: null, climbPlan: null, areas: null };
const focus = {
  weaknessId: "W1",
  why: item,
  drills: [{ drillType: "own_mistake_puzzles", rationale: "r" }],
  habit: item,
};
const plan = (n: number) => ({
  intro: item,
  focuses: Array.from({ length: n }, () => focus),
  weeklyRhythm: item,
});

describe("ReportSectionEnvelope", () => {
  it("parses a weaknesses envelope with other parts null", () => {
    const r = ReportSectionEnvelope.safeParse({
      ...empty,
      section: "weaknesses",
      weaknesses: { items: [{ ...item, weaknessId: "W1", title: "t" }] },
    });
    expect(r.success).toBe(true);
  });
  it("accepts 3 focuses, rejects 4", () => {
    const mk = (n: number) =>
      ReportSectionEnvelope.safeParse({ ...empty, section: "climb_plan", climbPlan: plan(n) });
    expect(mk(3).success).toBe(true);
    expect(mk(4).success).toBe(false);
  });
  it("rejects unknown keys and 5 strengths", () => {
    expect(ReportSectionEnvelope.safeParse({ ...empty, section: "areas", extra: 1 }).success).toBe(
      false,
    );
    const s = { ...item, area: "tactics" };
    const ov = { headline: item, summary: item, strengths: [s, s, s, s, s] };
    expect(
      ReportSectionEnvelope.safeParse({ ...empty, section: "overview", overview: ov }).success,
    ).toBe(false);
  });
  it("JSON schema: additionalProperties false on every object, no $ref", () => {
    const js = zodOutputFormat(ReportSectionEnvelope).schema as unknown;
    const text = JSON.stringify(js);
    expect(text).not.toContain("$ref");
    let objects = 0;
    const walk = (n: unknown) => {
      if (Array.isArray(n)) return n.forEach(walk);
      if (n && typeof n === "object") {
        const o = n as Record<string, unknown>;
        if (o.type === "object") {
          objects++;
          expect(o.additionalProperties).toBe(false);
        }
        Object.values(o).forEach(walk);
      }
    };
    walk(js);
    expect(objects).toBeGreaterThan(5);
  });
});

describe("drillsFor", () => {
  it("W12 is session_rules only", () => {
    expect(drillsFor("W12", null)).toEqual(["session_rules"]);
  });
  it("W3 deep", () => {
    expect(drillsFor("W3", "deep")).toContain("long_calculation");
    expect(drillsFor("W3", "deep")).not.toContain("themed_puzzles");
    expect(drillsFor("W3", null)).toContain("themed_puzzles");
  });
  it("W1 includes own_game_analysis", () => {
    expect(drillsFor("W1", null)).toContain("own_game_analysis");
  });
  it("every id returns a non-empty list of known drill types", () => {
    const ids = [
      "W1",
      "W2",
      "W3",
      "W4",
      "W5",
      "W6",
      "W7",
      "W8",
      "W9a",
      "W9b",
      "W9c",
      "W9d",
      "W10",
      "W11",
      "W12",
    ];
    for (const id of ids) {
      const d = drillsFor(id, null);
      expect(d.length).toBeGreaterThan(0);
      for (const x of d) expect(DRILL_TYPES).toContain(x);
    }
    expect(drillsFor("W9b", null)).toEqual(["blitz_safety", "clock_pressure", "own_game_analysis"]);
  });
  it("unknown id yields empty list", () => {
    expect(drillsFor("W99", null)).toEqual([]);
  });
});

describe("disclosure", () => {
  it("makeDisclosure", () => {
    const d = new Date("2026-11-21T10:00:00.000Z");
    expect(makeDisclosure("claude-sonnet-5-5", "report@1", d)).toEqual({
      label: "Written by AI",
      note: AI_DISCLOSURE.note,
      model: "claude-sonnet-5-5",
      promptVersion: "report@1",
      generatedAt: d.toISOString(),
    });
    expect(AI_DISCLOSURE.note).toBe(
      "This coaching was written by an AI model (Claude) from your analysed games. It can contain mistakes. The engine data shown alongside it is the source of truth.",
    );
  });
});
