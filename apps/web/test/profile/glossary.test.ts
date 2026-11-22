import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { AREAS, KPIS, WEAKNESSES } from "../../src/server/profile/catalogue";
import {
  AREA_GLOSSARY,
  DETECTOR_CONSTANTS,
  GLOSSARY_STATES,
  GLOSSARY_TERMS,
  GLOSSARY_VERDICTS,
  type GlossaryEntry,
  glossaryEntry,
  glossaryForPrompt,
  KPI_GLOSSARY,
  STATE_GLOSSARY,
  seriesGlossary,
  TERM_GLOSSARY,
  VERDICT_CONSTANTS,
  VERDICT_GLOSSARY,
  WEAKNESS_GLOSSARY,
} from "../../src/server/profile/glossary";

// The Rust side writes this file (`profile-core`'s `detector_constants` test); a constant that
// changes there fails here until the copy is reviewed.
const fixture = JSON.parse(
  readFileSync(
    new URL(
      "../../../../backend/crates/profile-core/tests/fixtures/detector_constants.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as Record<string, number | number[]>;

const fixtureNumbers = new Set(Object.values(fixture).flat());

const FIELDS = ["name", "meaning", "measured", "whyItMatters", "tip"] as const;

function allEntries(): [string, GlossaryEntry][] {
  return [
    ...Object.entries(WEAKNESS_GLOSSARY),
    ...Object.entries(KPI_GLOSSARY),
    ...Object.entries(AREA_GLOSSARY),
    ...Object.entries(TERM_GLOSSARY),
    ...Object.entries(STATE_GLOSSARY),
  ];
}

function expectComplete(key: string, entry: GlossaryEntry | null) {
  expect(entry, key).not.toBeNull();
  for (const field of FIELDS) {
    expect(entry?.[field]?.trim() ?? "", `${key}.${field}`).not.toBe("");
  }
}

describe("glossary", () => {
  it("covers every weakness key with a complete entry", () => {
    for (const w of WEAKNESSES) expectComplete(w.key, glossaryEntry(w.key));
    expect(Object.keys(WEAKNESS_GLOSSARY).sort()).toEqual(WEAKNESSES.map((w) => w.key).sort());
  });

  it("covers every KPI with a complete entry", () => {
    for (const k of KPIS) expectComplete(k.metric, glossaryEntry(k.metric));
    expect(Object.keys(KPI_GLOSSARY).sort()).toEqual(KPIS.map((k) => k.metric).sort());
  });

  it("covers every area with a complete entry", () => {
    for (const a of AREAS) expectComplete(a, glossaryEntry(a));
  });

  it("covers the profile's words and every weakness state", () => {
    expect([...GLOSSARY_TERMS]).toEqual(
      expect.arrayContaining([
        "chances",
        "range_80",
        "rating_band",
        "early_read",
        "estimated_rate",
        "not_compared_yet",
      ]),
    );
    expect([...GLOSSARY_STATES].sort()).toEqual(
      ["active", "emerging", "improving", "not_an_issue", "relapsed", "resolved"].sort(),
    );
    for (const t of GLOSSARY_TERMS) expectComplete(t, glossaryEntry(t));
    for (const s of GLOSSARY_STATES) expectComplete(s, glossaryEntry(s));
  });

  it("gives an unknown key no entry", () => {
    expect(glossaryEntry("W99")).toBeNull();
  });

  it("writes meaning and why-it-matters as one sentence each", () => {
    for (const [key, e] of allEntries()) {
      for (const text of [e.meaning, e.whyItMatters]) {
        expect(text, key).toMatch(/[.?]$/);
        expect(text.slice(0, -1), key).not.toMatch(/[.?!] [A-Z]/);
      }
    }
  });

  it("mirrors the Rust detector constants", () => {
    expect(DETECTOR_CONSTANTS).toEqual(fixture);
  });

  it("quotes only numbers that are detector constants", () => {
    for (const [key, e] of allEntries()) {
      for (const n of e.measured.match(/\d+(?:\.\d+)?/g) ?? []) {
        expect(fixtureNumbers.has(Number(n)), `${key} quotes ${n}: "${e.measured}"`).toBe(true);
      }
    }
  });

  it("exports every entry as JSON for the coach prompts", () => {
    const parsed = JSON.parse(glossaryForPrompt()) as Record<string, Record<string, GlossaryEntry>>;
    expect(Object.keys(parsed.weaknesses ?? {})).toHaveLength(WEAKNESSES.length);
    expect(Object.keys(parsed.kpis ?? {})).toHaveLength(KPIS.length);
    expect(Object.keys(parsed.areas ?? {})).toHaveLength(AREAS.length);
    expect(parsed.weaknesses?.W5).toEqual(WEAKNESS_GLOSSARY.W5);
    expect(parsed.terms?.chances).toEqual(TERM_GLOSSARY.chances);
    expect(parsed.states?.active).toEqual(STATE_GLOSSARY.active);
  });
});

describe("seriesGlossary", () => {
  it.each([
    ["W1", "W1"],
    ["W3.shallow", "W3.shallow"],
    ["motif.missed:fork", "motif_miss_rate:fork"],
  ])("%s takes the entry of %s", (series, key) => {
    expect(seriesGlossary(series)).toEqual(glossaryEntry(key));
  });

  it.each(["motif.allowed:fork", "opening:white:italian", "endgame:rook", "W99"])(
    "%s has no entry, never an invented one",
    (series) => {
      expect(seriesGlossary(series)).toBeNull();
    },
  );
});

describe("verdict glossary", () => {
  const rust = readFileSync(
    new URL("../../../../backend/crates/profile-core/src/openings/verdicts.rs", import.meta.url),
    "utf8",
  );
  const num = (re: RegExp) => {
    const m = re.exec(rust);
    if (!m?.[1]) throw new Error(`not found in verdicts.rs: ${re}`);
    return Number(m[1]);
  };

  it("mirrors the Rust verdict rule's thresholds", () => {
    expect(VERDICT_CONSTANTS).toEqual({
      minGames: num(/min_games: (\d+),/),
      perfDropPp: -Math.round(num(/perf_drop: (-?[\d.]+),/) * 100),
      worseWinPct: num(/worse_win: ([\d.]+),/),
      minAnalysed: num(/min_analysed: (\d+),/),
      deviationMinGames: num(/deviation_min_games: (\d+),/),
      deviationSharePct: Math.round(num(/deviation_share: ([\d.]+),/) * 100),
      deviationDeltaWinPct: num(/deviation_delta_w: ([\d.]+),/),
      replaceMinGames: num(/REPLACE_MIN_GAMES: u32 = (\d+);/),
      replaceMaxPerfPp: -Math.round(num(/REPLACE_MAX_PERF: f64 = (-?[\d.]+);/) * 100),
      learnMaxDeviationPly: num(/LEARN_MAX_DEVIATION_PLY: f64 = ([\d.]+);/),
      learnMinSharePct: Math.round(num(/LEARN_MIN_SHARE: f64 = ([\d.]+);/) * 100),
    });
  });

  it("explains every verdict with a complete entry", () => {
    expect([...GLOSSARY_VERDICTS].sort()).toEqual(["fix", "keep", "learn", "replace"]);
    for (const v of GLOSSARY_VERDICTS) expectComplete(v, VERDICT_GLOSSARY[v]);
  });

  it("quotes only the rule's numbers (and the 80% range)", () => {
    const allowed = new Set([...Object.values(VERDICT_CONSTANTS), 80]);
    for (const [key, e] of Object.entries(VERDICT_GLOSSARY)) {
      for (const n of e.measured.match(/\d+(?:\.\d+)?/g) ?? []) {
        expect(allowed.has(Number(n)), `${key} quotes ${n}`).toBe(true);
      }
      for (const text of [e.meaning, e.whyItMatters]) {
        expect(text, key).toMatch(/[.?]$/);
      }
    }
  });
});
