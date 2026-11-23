import { describe, expect, it } from "vitest";
import {
  formatCondition,
  formatKey,
  formatLabel,
  OTHER_FORMAT,
  parseFormatKey,
  topFormats,
} from "../../src/server/filters/formats";
import {
  type FilterContext,
  gamesPageParams,
  parseFilters,
  type StatsFilters,
  serializeFilters,
  setRange,
} from "../../src/server/filters/params";
import {
  applyPreset,
  comparePeriods,
  DEFAULT_PRESET,
  PRESETS,
  presetOf,
  presetRange,
} from "../../src/server/filters/presets";
import { parseGameQuery } from "../../src/server/games/query";

// The last 90 days ending 2026-03-31 are 1 Jan to 31 Mar (ADR 0083 Amendment 2).
const NOW = new Date("2026-03-31T15:00:00Z");
const ctx: FilterContext = { available: ["blitz", "rapid", "daily"], now: NOW };
const parse = (q: string, c: FilterContext = ctx) => parseFilters(new URLSearchParams(q), c);
const DEFAULTS: StatsFilters = {
  tc: "rapid",
  fmt: null,
  from: "2026-01-01",
  to: "2026-03-31",
  color: "both",
  cmp: null,
  a: null,
  b: null,
};

describe("formats", () => {
  it.each([
    [600, 0, "600+0", "10+0"],
    [180, 2, "180+2", "3+2"],
    [90, 1, "90+1", "1.5+1"],
    [30, 0, "30+0", "30s+0"],
    [900, 10, "900+10", "15+10"],
    [60, null, "60+0", "1+0"],
  ] as const)("%s+%s is %s, shown %s", (base, inc, key, label) => {
    expect(formatKey(base, inc)).toBe(key);
    expect(formatLabel(key)).toBe(label);
    expect(parseFormatKey(key)).toEqual({ base, increment: inc ?? 0 });
  });

  it("has no key without a base, and labels Other", () => {
    expect(formatKey(null, 2)).toBeNull();
    expect(formatLabel(OTHER_FORMAT)).toBe("Other");
  });

  it("lists the top 5 by games, then Other with the rest and the unknown", () => {
    const counts = [
      { base: 600, increment: 0, games: 50 },
      { base: 600, increment: 5, games: 20 },
      { base: 900, increment: 10, games: 20 },
      { base: 1800, increment: 0, games: 5 },
      { base: 420, increment: 0, games: 4 },
      { base: 300, increment: 5, games: 3 },
      { base: 1200, increment: 0, games: 2 },
      { base: null, increment: null, games: 1 },
    ];
    expect(topFormats(counts, "rapid")).toEqual([
      { key: "600+0", label: "10+0", games: 50 },
      { key: "600+5", label: "10+5", games: 20 },
      { key: "900+10", label: "15+10", games: 20 },
      { key: "1800+0", label: "30+0", games: 5 },
      { key: "420+0", label: "7+0", games: 4 },
      { key: OTHER_FORMAT, label: "Other", games: 6 },
    ]);
  });

  it("has no Other when every game is named, and no formats for daily", () => {
    const counts = [{ base: 600, increment: 0, games: 3 }];
    expect(topFormats(counts, "rapid").map((o) => o.key)).toEqual(["600+0"]);
    expect(topFormats(counts, "daily")).toEqual([]);
    expect(topFormats([], "blitz")).toEqual([]);
  });

  it("builds no condition without fmt", () => {
    expect(formatCondition(null, [])).toBeNull();
    expect(formatCondition("600+0", [])).not.toBeNull();
  });
});

describe("parseFilters", () => {
  it("defaults to rapid, both colours, the last 90 days, no comparison", () => {
    expect(parse("")).toEqual(DEFAULTS);
  });

  it("falls back to the first available class by rapid-first order", () => {
    expect(parse("", { available: ["bullet", "blitz"] }).tc).toBe("blitz");
    expect(parse("", { available: [] }).tc).toBeNull();
    expect(parse("tc=bullet").tc).toBe("rapid");
    expect(parse("tc=nonsense").tc).toBe("rapid");
  });

  it("reads every key", () => {
    expect(parse("tc=blitz&fmt=180+2&from=2026-01-01&to=2026-02-01&color=black&cmp=50g")).toEqual({
      ...DEFAULTS,
      tc: "blitz",
      fmt: "180+2",
      from: "2026-01-01",
      to: "2026-02-01",
      color: "black",
      cmp: "50g",
    });
  });

  it("normalises fmt and drops it for daily, for junk and outside the known formats", () => {
    expect(parse("fmt=0600%2B00").fmt).toBe("600+0");
    expect(parse("fmt=other").fmt).toBe("other");
    expect(parse("tc=daily&fmt=600+0").fmt).toBeNull();
    expect(parse("fmt=10+0min").fmt).toBeNull();
    expect(parse("fmt=900+0", { ...ctx, formats: ["600+0"] }).fmt).toBeNull();
    expect(parse("fmt=600+0", { ...ctx, formats: ["600+0"] }).fmt).toBe("600+0");
  });

  it("defaults to the last 90 days, and reads from=all as all time", () => {
    expect(parse("")).toMatchObject({ from: "2026-01-01", to: "2026-03-31" });
    expect(parse("from=all")).toMatchObject({ from: null, to: null });
    expect(parse("from=all&to=2026-02-01")).toMatchObject({ from: null, to: "2026-02-01" });
    // One bound keeps the other end open, as before.
    expect(parse("to=2026-02-01")).toMatchObject({ from: null, to: "2026-02-01" });
    expect(parse("from=2026-02-01")).toMatchObject({ from: "2026-02-01", to: null });
    // A page whose default is every game (the games list) reads no dates as all time.
    expect(parse("", { ...ctx, defaultPeriod: "all" })).toMatchObject({ from: null, to: null });
  });

  it("writes all time as from=all and omits the 90-day default", () => {
    expect(serializeFilters({ ...DEFAULTS, from: null, to: null }, ctx).toString()).toBe(
      "from=all",
    );
    expect(serializeFilters({ ...DEFAULTS, from: null, to: "2026-02-01" }, ctx).toString()).toBe(
      "to=2026-02-01",
    );
    expect(serializeFilters(DEFAULTS, ctx).toString()).toBe("");
    const games = { ...ctx, defaultPeriod: "all" as const };
    expect(serializeFilters({ ...DEFAULTS, from: null, to: null }, games).toString()).toBe("");
    expect(serializeFilters(DEFAULTS, games).toString()).toBe("from=2026-01-01&to=2026-03-31");
  });

  it("writes a range into a link: days, nothing for the default, from=all for all time", () => {
    const at = (r: { from: string | null; to: string | null }) => {
      const q = new URLSearchParams("tc=blitz");
      setRange(q, r, { now: NOW });
      return q.toString();
    };
    expect(at({ from: "2026-02-01", to: null })).toBe("tc=blitz&from=2026-02-01");
    expect(at({ from: "2026-01-01", to: "2026-03-31" })).toBe("tc=blitz");
    expect(at({ from: null, to: null })).toBe("tc=blitz&from=all");
  });

  it("rejects impossible days and swaps a reversed range", () => {
    expect(parse("from=2026-02-30&to=1999-01-01")).toMatchObject({
      from: "2026-01-01",
      to: "2026-03-31",
    });
    expect(parse("from=2026-03-01&to=2026-01-01")).toMatchObject({
      from: "2026-01-01",
      to: "2026-03-01",
    });
  });

  it("reads the games page's colour as an alias; color wins", () => {
    expect(parse("colour=white").color).toBe("white");
    expect(parse("colour=black&color=white").color).toBe("white");
    expect(parse("colour=both").color).toBe("both");
    expect(parse("color=purple").color).toBe("both");
  });

  it("treats a repeated key as invalid", () => {
    expect(parse("color=white&color=black").color).toBe("both");
    expect(parseFilters({ tc: ["blitz", "rapid"] }, ctx).tc).toBe("rapid");
  });

  it("needs both custom periods, and ignores them without cmp=custom", () => {
    expect(parse("cmp=custom&a_from=2026-01-01&b_from=2026-02-01")).toMatchObject({
      cmp: "custom",
      a: { from: "2026-01-01", to: null },
      b: { from: "2026-02-01", to: null },
    });
    expect(parse("cmp=custom&a_from=2026-01-01")).toMatchObject({ cmp: null, a: null, b: null });
    expect(parse("cmp=month&a_from=2026-01-01")).toMatchObject({ cmp: "month", a: null });
  });
});

describe("serializeFilters round-trips", () => {
  const cases: StatsFilters[] = [
    DEFAULTS,
    { ...DEFAULTS, tc: "blitz", fmt: "180+2", color: "white" },
    { ...DEFAULTS, fmt: OTHER_FORMAT, from: "2026-01-01", to: "2026-03-31", cmp: "30d" },
    { ...DEFAULTS, tc: "daily", color: "black", cmp: "month" },
    { ...DEFAULTS, from: null, to: null },
    { ...DEFAULTS, from: null, to: "2026-02-01" },
    { ...DEFAULTS, from: "2025-06-01", to: "2025-07-01" },
    {
      ...DEFAULTS,
      cmp: "custom",
      a: { from: "2026-01-01", to: "2026-01-31" },
      b: { from: "2026-02-01", to: null },
    },
  ];

  it.each(cases)("parse(serialize(f)) is f: %j", (f) => {
    const url = serializeFilters(f, ctx);
    expect(parse(url.toString())).toEqual(f);
    // The canonical URL is a fixed point.
    expect(serializeFilters(parse(url.toString()), ctx).toString()).toBe(url.toString());
  });

  it("omits defaults and writes keys in the contract's order", () => {
    expect(serializeFilters(DEFAULTS, ctx).toString()).toBe("");
    const f: StatsFilters = {
      tc: "blitz",
      fmt: "180+2",
      from: "2026-01-01",
      to: "2026-02-01",
      color: "black",
      cmp: "custom",
      a: { from: "2025-01-01", to: "2025-01-31" },
      b: { from: "2025-02-01", to: "2025-02-28" },
    };
    expect([...serializeFilters(f, ctx).keys()]).toEqual([
      "tc",
      "fmt",
      "from",
      "to",
      "color",
      "cmp",
      "a_from",
      "a_to",
      "b_from",
      "b_to",
    ]);
  });

  it("canonicalises messy URLs", () => {
    const messy = "colour=white&fmt=0180%2B2&tc=blitz&to=2026-01-01&from=2026-02-01&junk=1";
    expect(serializeFilters(parse(messy), ctx).toString()).toBe(
      "tc=blitz&fmt=180%2B2&from=2026-01-01&to=2026-02-01&color=white",
    );
  });

  it("maps to the games page's own params, colour and all", () => {
    const f = parse("tc=blitz&fmt=180+2&color=black&from=2026-01-01&cmp=50g");
    const g = gamesPageParams(f, ctx);
    expect(g.toString()).toBe("tc=blitz&colour=black&from=2026-01-01");
    const q = parseGameQuery(Object.fromEntries(g), "rapid");
    expect(q.filters).toMatchObject({ timeClass: "blitz", colour: "black", from: "2026-01-01" });
    // The games list opens on every game, so the stats pages' 90 days travel as days and all
    // time as no dates.
    expect(gamesPageParams(DEFAULTS, ctx).toString()).toBe("from=2026-01-01&to=2026-03-31");
    expect(gamesPageParams({ ...DEFAULTS, from: null, to: null }, ctx).toString()).toBe("");
  });
});

describe("presets", () => {
  const now = new Date("2026-03-31T15:00:00Z");

  it.each([
    ["30d", { from: "2026-03-02", to: "2026-03-31" }],
    ["90d", { from: "2026-01-01", to: "2026-03-31" }],
    ["6m", { from: "2025-10-01", to: "2026-03-31" }],
    ["12m", { from: "2025-04-01", to: "2026-03-31" }],
    ["all", { from: null, to: null }],
  ] as const)("%s ending 2026-03-31", (preset, want) => {
    expect(presetRange(preset, now)).toEqual(want);
    expect(presetOf(want, now)).toBe(preset);
  });

  it("clamps month ends (Aug 31 minus 6 months is Feb 28)", () => {
    expect(presetRange("6m", new Date("2026-08-31T00:00:00Z")).from).toBe("2026-03-01");
  });

  it("names any other range custom, and applies presets", () => {
    expect(PRESETS).toEqual(["30d", "90d", "6m", "12m", "all", "custom"]);
    expect(DEFAULT_PRESET).toBe("90d");
    expect(presetOf({ from: "2026-01-05", to: null }, now)).toBe("custom");
    expect(applyPreset(DEFAULTS, "30d", now)).toMatchObject({
      from: "2026-03-02",
      to: "2026-03-31",
    });
    const custom = { ...DEFAULTS, from: "2026-01-05" };
    expect(applyPreset(custom, "custom", now)).toBe(custom);
  });
});

describe("comparePeriods", () => {
  const now = new Date("2026-03-10T08:00:00Z");

  it("has none without cmp", () => {
    expect(comparePeriods(DEFAULTS, now)).toBeNull();
  });

  it.each([
    [
      "month",
      { kind: "days", from: "2026-02-01", to: "2026-02-28" },
      { kind: "days", from: "2026-03-01", to: "2026-03-10" },
    ],
    [
      "30d",
      { kind: "days", from: "2026-01-10", to: "2026-02-08" },
      { kind: "days", from: "2026-02-09", to: "2026-03-10" },
    ],
    ["50g", { kind: "games", skip: 50, take: 50 }, { kind: "games", skip: 0, take: 50 }],
  ] as const)("%s: A earlier, B later", (cmp, a, b) => {
    expect(comparePeriods({ ...DEFAULTS, cmp }, now)).toEqual({ a, b });
  });

  it("uses the custom ranges", () => {
    const f = parse("cmp=custom&a_from=2026-01-01&a_to=2026-01-31&b_from=2026-02-01");
    expect(comparePeriods(f, now)).toEqual({
      a: { kind: "days", from: "2026-01-01", to: "2026-01-31" },
      b: { kind: "days", from: "2026-02-01", to: null },
    });
  });
});
